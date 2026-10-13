// Cliente de la API de Eminus y el grupo de hilos que la consulta. La
// interfaz nunca espera a la red: encola trabajos y recibe mensajes.

use crate::model::{self, Activity, CFile, ContentDetail, ContentItem, Course, Detail};
use crate::text;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

const WORKERS: usize = 6;

pub enum ApiError {
    Auth(String),
    Other(String),
}

impl ApiError {
    pub fn message(&self) -> &str {
        match self {
            ApiError::Auth(m) | ApiError::Other(m) => m,
        }
    }
}

type Res<T> = Result<T, ApiError>;

pub struct Client {
    agent: ureq::Agent,
    origin: String,
    token: RwLock<String>,
}

fn enc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn rows_of(json: Value) -> Vec<Value> {
    match json {
        Value::Array(rows) => rows,
        Value::Object(mut map) => match map.remove("contenido") {
            Some(Value::Array(rows)) => rows,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

// La API anterior a veces responde con un solo objeto en vez de una lista.
fn rows_of_old(json: Value) -> Vec<Value> {
    let data = match json {
        Value::Object(mut map) if map.get("contenido").is_some_and(|c| !c.is_null()) => map.remove("contenido").unwrap_or(Value::Null),
        other => other,
    };
    match data {
        Value::Array(rows) => rows,
        Value::Object(_) => vec![data],
        _ => Vec::new(),
    }
}

impl Client {
    pub fn new(token: &str) -> Self {
        Client {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(30))
                .max_idle_connections_per_host(WORKERS)
                .user_agent(concat!("BetterThanEminus/", env!("CARGO_PKG_VERSION")))
                .build(),
            // Sólo para pruebas contra un servidor local.
            origin: std::env::var("EMINUS_ORIGIN").unwrap_or_else(|_| "https://eminus.uv.mx".into()),
            token: RwLock::new(token.to_string()),
        }
    }

    pub fn set_token(&self, token: &str) {
        *self.token.write().unwrap() = token.to_string();
    }

    fn api8(&self, path: &str) -> String {
        format!("{}/eminusapi8/api{path}", self.origin)
    }

    // El contenido de los cursos y el login sólo están en la API anterior.
    fn api_old(&self, path: &str) -> String {
        format!("{}/eminusapi/api{path}", self.origin)
    }

    fn request(&self, url: &str) -> Res<ureq::Response> {
        let token = self.token.read().unwrap().clone();
        if token.is_empty() {
            return Err(ApiError::Auth("sin sesión".into()));
        }
        match self.agent.get(url).set("Authorization", &format!("Bearer {token}")).set("Accept", "application/json").call() {
            Ok(res) => Ok(res),
            Err(ureq::Error::Status(401 | 403, _)) => Err(ApiError::Auth("sesión expirada".into())),
            Err(ureq::Error::Status(code, _)) => Err(ApiError::Other(format!("HTTP {code}"))),
            Err(ureq::Error::Transport(t)) => Err(ApiError::Other(match t.kind() {
                ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => "sin conexión con eminus".into(),
                ureq::ErrorKind::Io => "la conexión se cortó o tardó demasiado".into(),
                kind => kind.to_string(),
            })),
        }
    }

    fn get(&self, url: &str) -> Res<Value> {
        self.request(url)?.into_json().map_err(|_| ApiError::Other("respuesta ilegible de eminus".into()))
    }

    pub fn courses(&self) -> Res<Vec<Course>> {
        Ok(model::courses_from(&rows_of(self.get(&self.api8("/Course/getAllCourses"))?), text::now()))
    }

    pub fn activities(&self, course: &Course) -> Res<Vec<Arc<Activity>>> {
        let rows = rows_of(self.get(&self.api8(&format!("/Activity/getActividadesEstudiante/{}", enc(&course.id))))?);
        Ok(model::activities_from(&rows, course))
    }

    pub fn activity_detail(&self, course_id: &str, id: &str) -> Res<Detail> {
        let json = self.get(&self.api8(&format!("/Activity/getActividadEstudiante/{}/{}", enc(course_id), enc(id))))?;
        let row = match json {
            Value::Array(mut rows) if !rows.is_empty() => rows.swap_remove(0),
            Value::Object(mut map) => match map.remove("contenido") {
                Some(Value::Array(mut rows)) if !rows.is_empty() => rows.swap_remove(0),
                Some(row @ Value::Object(_)) => row,
                _ => Value::Null,
            },
            _ => Value::Null,
        };
        Ok(model::detail_from(&row))
    }

    // Módulos publicados de un curso, cada uno seguido de sus elementos. Los
    // elementos se piden de tres módulos a la vez.
    pub fn content(&self, course: &Course) -> Res<Vec<Arc<ContentItem>>> {
        let now = text::now();
        let units: Vec<ContentItem> = rows_of_old(self.get(&self.api_old(&format!("/Contenido/getUnidades/{}/0", enc(&course.id))))?)
            .iter()
            .filter(|u| model::is_published(u, now))
            .filter_map(|u| model::unit_from(u, course))
            .collect();
        let next = AtomicUsize::new(0);
        let elements: Vec<Mutex<Vec<ContentItem>>> = units.iter().map(|_| Mutex::new(Vec::new())).collect();
        let auth: Mutex<Option<ApiError>> = Mutex::new(None);
        std::thread::scope(|scope| {
            for _ in 0..units.len().min(3) {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(unit) = units.get(i) else { break };
                    match self.get(&self.api_old(&format!("/Contenido/getElementos/{}/{}", enc(&course.id), enc(&unit.unit_id)))) {
                        Ok(json) => *elements[i].lock().unwrap() = rows_of_old(json).iter().filter(|e| model::is_published(e, now)).filter_map(|e| model::element_from(e, unit)).collect(),
                        Err(err @ ApiError::Auth(_)) => {
                            *auth.lock().unwrap() = Some(err);
                            break;
                        }
                        // Un módulo ilegible no tira el curso entero.
                        Err(ApiError::Other(_)) => {}
                    }
                });
            }
        });
        if let Some(err) = auth.into_inner().unwrap() {
            return Err(err);
        }
        let mut items = Vec::new();
        for (unit, els) in units.into_iter().zip(elements) {
            items.push(Arc::new(unit));
            items.extend(els.into_inner().unwrap().into_iter().map(Arc::new));
        }
        Ok(items)
    }

    // Archivos de un curso según su ubicación (módulo o elemento).
    pub fn course_files(&self, course_id: &str) -> Res<Vec<CFile>> {
        let rows = rows_of_old(self.get(&self.api_old(&format!("/Contenido/UbicacionArchivos/{}", enc(course_id))))?);
        Ok(rows
            .iter()
            .flat_map(|row| {
                let unit_id = model::id_of(model::pick(row, &model::UNIT_ID_FIELDS), true);
                match row.get("archivos").and_then(Value::as_array) {
                    Some(files) => files.iter().filter_map(|f| model::content_file(f, &unit_id)).collect::<Vec<_>>(),
                    None => model::content_file(row, &unit_id).into_iter().collect(),
                }
            })
            .collect())
    }

    // Descripción completa y archivos de un módulo o elemento.
    pub fn content_detail(&self, item: &ContentItem, located: &[CFile]) -> Res<ContentDetail> {
        let mut files: Vec<CFile> = located.iter().filter(|f| if item.unit { f.unit_id == item.unit_id } else { f.element_id == item.element_id }).cloned().collect();
        let mut text = item.text.clone();
        if !item.unit {
            match self.get(&self.api_old(&format!("/Contenido/getElemento/{}/{}", enc(&item.course_id), enc(&item.element_id)))) {
                Ok(json) => {
                    if let Some(row) = rows_of_old(json).first() {
                        let full = model::desc_of(row);
                        if !full.is_empty() {
                            text = full;
                        }
                        if files.is_empty() {
                            files = model::content_files_of(row);
                        }
                    }
                }
                Err(err @ ApiError::Auth(_)) => return Err(err),
                Err(ApiError::Other(_)) => {}
            }
        }
        let mut seen = HashSet::new();
        let files = item.files.iter().cloned().chain(files).filter(|f| seen.insert(f.id.clone())).collect();
        Ok(ContentDetail { text, files })
    }

    // Va directo al disco, sin pasar el archivo por memoria.
    pub fn download(&self, course_id: &str, element_id: &str, file: &CFile) -> Res<PathBuf> {
        let url = self.api_old(&format!("/contenido/descargaElementoArchivo/{}/{}/{}", enc(course_id), enc(element_id), enc(&file.id)));
        let res = self.request(&url)?;
        let fail = |e: io::Error| ApiError::Other(e.to_string());
        let dir = downloads_dir();
        std::fs::create_dir_all(&dir).map_err(fail)?;
        let safe: String = file.name.chars().map(|c| if c == '/' || c == '\\' || c == ':' || c.is_control() { '_' } else { c }).collect();
        let safe = safe.trim_matches(['.', ' ']);
        let safe = if safe.is_empty() { "archivo" } else { safe };
        let (stem, ext) = match safe.rfind('.') {
            Some(dot) if dot > 0 => safe.split_at(dot),
            _ => (safe, ""),
        };
        // create_new: nunca pisa un archivo que ya esté en Descargas.
        let (path, mut out) = (0..1000)
            .find_map(|n| {
                let path = dir.join(if n == 0 { safe.to_string() } else { format!("{stem} ({n}){ext}") });
                std::fs::OpenOptions::new().write(true).create_new(true).open(&path).ok().map(|f| (path, f))
            })
            .ok_or_else(|| ApiError::Other("no se pudo crear el archivo en Descargas".into()))?;
        if let Err(err) = io::copy(&mut res.into_reader(), &mut out) {
            drop(out);
            let _ = std::fs::remove_file(&path);
            return Err(fail(err));
        }
        Ok(path)
    }

    // `password` ya viene cifrada como la manda el formulario de Eminus.
    pub fn login(&self, user: &str, password: &str) -> Res<String> {
        let sent = self.agent.post(&self.api_old("/auth")).send_json(json!({ "username": user, "password": password }));
        let json: Value = match sent {
            Ok(res) => res.into_json().map_err(|_| ApiError::Other("respuesta ilegible de eminus".into()))?,
            Err(ureq::Error::Status(400..=499, _)) => return Err(ApiError::Other("usuario o contraseña incorrectos".into())),
            Err(ureq::Error::Status(code, _)) => return Err(ApiError::Other(format!("HTTP {code}"))),
            Err(_) => return Err(ApiError::Other("sin conexión con eminus".into())),
        };
        let data = match json {
            Value::Array(mut rows) => rows.pop().unwrap_or(Value::Null),
            other => other,
        };
        let data = data.get("contenido").filter(|c| c.is_object()).unwrap_or(&data);
        match data.get("accessToken").and_then(Value::as_str) {
            Some(token) if !token.is_empty() => Ok(token.to_string()),
            _ => Err(ApiError::Other("usuario o contraseña incorrectos".into())),
        }
    }
}

pub fn downloads_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default()).join("Downloads")
}

// ───────────── trabajos ─────────────

pub enum Msg {
    Courses(Res<Vec<Course>>),
    Acts(String, Res<Vec<Arc<Activity>>>),
    Detail(String, Res<Detail>),
    Content(String, Res<Vec<Arc<ContentItem>>>),
    // id del elemento, id del curso, archivos del curso recién leídos, detalle
    ContentDetail(String, String, Option<Arc<Vec<CFile>>>, Res<ContentDetail>),
    Downloaded(String, Res<PathBuf>),
    Login(Res<String>),
}

type Job = Box<dyn FnOnce(&Client) -> Msg + Send>;

// Cada trabajo lleva la «época» de la sesión en que se pidió: lo que llegue
// de una sesión anterior (otra cuenta) se descarta.
pub struct Pool {
    pub client: Arc<Client>,
    jobs: Sender<(u64, Job)>,
    pub inbox: Receiver<(u64, Msg)>,
    pub pending: usize,
}

impl Pool {
    pub fn new(client: Client) -> Self {
        let client = Arc::new(client);
        let (jobs, queue) = mpsc::channel::<(u64, Job)>();
        let (outbox, inbox) = mpsc::channel();
        let queue = Arc::new(Mutex::new(queue));
        for _ in 0..WORKERS {
            let (queue, outbox, client) = (queue.clone(), outbox.clone(), client.clone());
            std::thread::spawn(move || loop {
                let next = queue.lock().unwrap().recv();
                let Ok((epoch, job)) = next else { break };
                if outbox.send((epoch, job(&client))).is_err() {
                    break;
                }
            });
        }
        Pool { client, jobs, inbox, pending: 0 }
    }

    pub fn spawn(&mut self, epoch: u64, job: impl FnOnce(&Client) -> Msg + Send + 'static) {
        if self.jobs.send((epoch, Box::new(job))).is_ok() {
            self.pending += 1;
        }
    }
}
