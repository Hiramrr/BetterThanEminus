// Cliente de la API de Eminus y el grupo de hilos que la consulta. La
// interfaz nunca espera a la red: encola trabajos y recibe mensajes.

use crate::model::{self, Activity, CFile, ContentDetail, ContentItem, Course, DeliveryInfo, Detail};
use std::io::Read;
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

    pub fn token(&self) -> String {
        self.token.read().unwrap().clone()
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

    pub fn download(&self, course_id: &str, element_id: &str, file: &CFile) -> Res<PathBuf> {
        self.save(&self.api_old(&format!("/contenido/descargaElementoArchivo/{}/{}/{}", enc(course_id), enc(element_id), enc(&file.id))), &file.name)
    }

    // Archivo adjunto a una actividad por el docente.
    pub fn download_activity_file(&self, course_id: &str, activity_id: &str, file_id: &str, name: &str) -> Res<PathBuf> {
        self.save(&self.api_old(&format!("/actividad/descargaActividadArchivo/{}/{}/{}", enc(course_id), enc(activity_id), enc(file_id))), name)
    }

    // Va directo al disco, sin pasar el archivo por memoria.
    fn save(&self, url: &str, name: &str) -> Res<PathBuf> {
        let res = self.request(url)?;
        let fail = |e: io::Error| ApiError::Other(e.to_string());
        let dir = downloads_dir();
        std::fs::create_dir_all(&dir).map_err(fail)?;
        let safe: String = name.chars().map(|c| if c == '/' || c == '\\' || c == ':' || c.is_control() { '_' } else { c }).collect();
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

    // ── entregas ──

    fn post(&self, url: &str, body: Value) -> Res<Value> {
        let token = self.token();
        match self.agent.post(url).set("Authorization", &format!("Bearer {token}")).send_json(body) {
            Ok(res) => Ok(res.into_json().unwrap_or(Value::Null)),
            Err(ureq::Error::Status(401 | 403, _)) => Err(ApiError::Auth("sesión expirada".into())),
            Err(ureq::Error::Status(code, _)) => Err(ApiError::Other(format!("eminus respondió HTTP {code}"))),
            Err(_) => Err(ApiError::Other("la conexión con eminus se cortó".into())),
        }
    }

    // La entrega que ya exista (borrador o enviada), o una vacía.
    pub fn delivery_info(&self, course_id: &str, activity_id: &str) -> Res<DeliveryInfo> {
        let url = self.api_old(&format!("/Actividad/getActUsuarioMiRevision/{}/{}/0", enc(course_id), enc(activity_id)));
        // Sin entrega previa Eminus responde vacío, que no es JSON.
        let json = self.request(&url)?.into_json::<Value>().unwrap_or(Value::Null);
        Ok(model::delivery_from(rows_of_old(json).first().unwrap_or(&Value::Null)))
    }

    // Los mismos tres pasos que la página de Eminus: borrador, adjuntos y
    // envío. Si algo falla antes del último, la actividad NO queda entregada
    // (queda el borrador con lo que sí se subió).
    pub fn deliver(&self, course_id: &str, activity_id: &str, delivery_id: i64, text: &str, files: &[PathBuf], attached: usize) -> Res<()> {
        let ids = |id: i64| json!({ "idCurso": model::id_json(course_id), "idActividad": model::id_json(activity_id), "idActividadEntrega": id, "idEquipo": 0 });
        let endpoint = self.api_old("/Actividad/InsertaActividadEntrega");
        let id = if delivery_id > 0 {
            delivery_id
        } else {
            let made = self.post(&endpoint, ids(0))?;
            let made = match made {
                Value::Array(mut rows) if !rows.is_empty() => rows.swap_remove(0),
                other => other,
            };
            match model::int(made.get("salida")) {
                id if id > 0 => id,
                _ => return Err(ApiError::Other("eminus no creó el borrador de la entrega".into())),
            }
        };
        for path in files {
            self.attach(course_id, activity_id, id, path)?;
        }
        let escaped = text.trim().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br>");
        let mut body = ids(id);
        body["texto"] = json!(format!("<p>{escaped}</p>"));
        body["fechaEntrega"] = Value::Null;
        body["tieneAdjuntos"] = json!(i32::from(attached + files.len() > 0));
        body["visible"] = json!(1);
        self.post(&endpoint, body).map(|_| ())
    }

    // Sube un archivo a la entrega, leyéndolo del disco por partes.
    fn attach(&self, course_id: &str, activity_id: &str, delivery_id: i64, path: &std::path::Path) -> Res<()> {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "archivo".into());
        let fail = |what: &str| ApiError::Other(format!("{what}: {name}"));
        let file = std::fs::File::open(path).map_err(|_| fail("no se pudo leer"))?;
        let size = file.metadata().map_err(|_| fail("no se pudo leer"))?.len();
        let mut random = [0u8; 12];
        let _ = getrandom::getrandom(&mut random);
        let boundary = format!("----eminus{}", random.iter().map(|b| format!("{b:02x}")).collect::<String>());
        let head = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
            name.replace(['"', '\r', '\n'], "_")
        );
        let tail = format!("\r\n--{boundary}--\r\n");
        let length = head.len() as u64 + size + tail.len() as u64;
        let body = io::Cursor::new(head.into_bytes()).chain(file).chain(io::Cursor::new(tail.into_bytes()));
        let sent = self
            .agent
            .post(&self.api_old("/actividad/adjuntarEntrega"))
            .query("idCurso", course_id)
            .query("idActividad", activity_id)
            .query("idActividadEntregas", &delivery_id.to_string())
            .set("Authorization", &format!("Bearer {}", self.token()))
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .set("Content-Length", &length.to_string())
            .send(body);
        match sent {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(401 | 403, _)) => Err(ApiError::Auth("sesión expirada".into())),
            Err(ureq::Error::Status(413, _)) => Err(fail("eminus dice que es demasiado grande")),
            Err(_) => Err(fail("no se pudo subir")),
        }
    }

    // El formulario de Eminus cifra la contraseña con una llave que viene en
    // el código de su propia página; se lee de ahí, igual que el navegador.
    pub fn login_key(&self) -> Res<String> {
        let unreadable = || ApiError::Other("no se pudo leer el formulario de eminus; entra con un token".into());
        let page = |url: &str| self.agent.get(url).call().ok().and_then(|res| res.into_string().ok());
        let base = format!("{}/eminus4/", self.origin);
        let index = page(&base).ok_or_else(|| ApiError::Other("sin conexión con eminus".into()))?;
        let script = index.split('"').find(|part| part.starts_with("main.") && part.contains(".js")).ok_or_else(unreadable)?;
        let code = page(&format!("{base}{script}")).ok_or_else(unreadable)?;
        let key = code.split("hash:{key:\"").nth(1).and_then(|rest| rest.split('"').next()).filter(|key| !key.is_empty()).ok_or_else(unreadable)?;
        Ok(key.to_string())
    }

    pub fn sign_in(&self, user: &str, password: &str) -> Res<String> {
        let key = match crate::store::login_key() {
            Some(key) => key,
            None => self.login_key()?,
        };
        let sealed = crate::store::encrypt_password(password.trim(), &key).ok_or_else(|| ApiError::Other("no se pudo cifrar la contraseña".into()))?;
        self.login(user, &sealed)
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
    // clave de la actividad y su entrega actual
    Delivery(String, Res<DeliveryInfo>),
    // id del curso, título de la actividad y resultado del envío
    Delivered(String, String, Res<()>),
    // usuario, contraseña (sólo si se escribió a mano, para ofrecer guardarla)
    // y el token o el error
    Login(String, Option<String>, Res<String>),
    ProfileSaved(crate::store::Profile, Result<(), String>),
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
