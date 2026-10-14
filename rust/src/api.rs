// Cliente de la API de Eminus y el grupo de hilos que la consulta. La
// interfaz nunca espera a la red: encola trabajos y recibe mensajes.

use crate::model::{self, Activity, CFile, ContentDetail, ContentItem, Course, DeliveryInfo, Detail, UserInfo};
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

#[derive(Debug)]
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
    // «Bearer …» ya armado: se construye al fijar el token y cada consulta
    // sólo clona el Arc, en vez de copiar el token y formatear la cabecera.
    auth: RwLock<Arc<str>>,
}

fn enc(text: &str) -> String {
    // Los ids de Eminus son dígitos: el caso común no necesita escapar nada.
    if text.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.~".contains(&b)) {
        return text.to_string();
    }
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

// El aviso que venga en una respuesta que no trae lo pedido.
fn notice_of(json: &Value) -> Option<String> {
    let found = |v: &Value| model::pick(v, &["mensaje", "Mensaje", "message", "Message", "error"]).and_then(Value::as_str).map(|m| m.trim().to_string());
    found(json).or_else(|| json.get("contenido").and_then(found)).filter(|m| !m.is_empty())
}

// La actividad dentro de la respuesta: llega como objeto, como lista de un
// elemento o envuelta en «contenido». Si no está, es un error y no un
// detalle vacío.
fn detail_row(json: Value) -> Res<Value> {
    let notice = notice_of(&json);
    match rows_of_old(json).into_iter().find(model::is_activity_row) {
        Some(row) => Ok(row),
        None => Err(ApiError::Other(match notice {
            Some(notice) => format!("eminus no devolvió la actividad: {notice}"),
            None => "eminus no devolvió el detalle de la actividad".into(),
        })),
    }
}

// La entrega dentro de la respuesta, o nada si todavía no existe.
fn delivery_row_reader<R: io::Read>(reader: R) -> Res<Option<Value>> {
    use io::BufRead;
    let mut reader = io::BufReader::new(reader);
    let connection = || ApiError::Other("la conexión se cortó o tardó demasiado".into());
    // Algunas respuestas llevan BOM. Se consume sin guardar el cuerpo completo.
    while reader.fill_buf().map_err(|_| connection())?.first() == Some(&0xef) {
        let mut bom = [0; 3];
        reader.read_exact(&mut bom).map_err(|_| connection())?;
        if bom != [0xef, 0xbb, 0xbf] {
            return Err(ApiError::Other("respuesta ilegible de eminus".into()));
        }
    }
    loop {
        let buf = reader.fill_buf().map_err(|_| connection())?;
        if buf.is_empty() {
            return Ok(None);
        }
        let spaces = buf.iter().take_while(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')).count();
        reader.consume(spaces);
        if spaces == 0 {
            break;
        }
    }
    let json: Value = serde_json::from_reader(reader).map_err(|e| if e.is_io() {
        connection()
    } else {
        ApiError::Other("respuesta ilegible de eminus".into())
    })?;
    delivery_row_json(json)
}

fn delivery_row_json(json: Value) -> Res<Option<Value>> {
    let notice = notice_of(&json);
    let data = json.get("contenido").unwrap_or(&json);
    if notice.is_none() && (data.is_null() || data.as_array().is_some_and(Vec::is_empty)) {
        return Ok(None);
    }
    match rows_of_old(json).into_iter().find(|row| row.is_object()) {
        Some(row) if row.as_object().is_some_and(|r| r.is_empty()) => Ok(None),
        Some(row) if !model::is_delivery_row(&row) => Err(ApiError::Other(format!("eminus no devolvió la entrega: {}", notice.unwrap_or_else(|| "respuesta sin datos de entrega".into())))),
        Some(row) => Ok(Some(row)),
        None => Err(ApiError::Other("respuesta sin datos de entrega".into())),
    }
}

impl Client {
    pub fn new(token: &str) -> Self {
        // Sólo para pruebas contra un servidor local.
        Self::with_origin(token, &std::env::var("EMINUS_ORIGIN").unwrap_or_else(|_| "https://eminus.uv.mx".into()))
    }

    fn with_origin(token: &str, origin: &str) -> Self {
        Client {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(30))
                .max_idle_connections_per_host(WORKERS)
                .user_agent(concat!("BetterThanEminus/", env!("CARGO_PKG_VERSION")))
                .build(),
            origin: origin.trim_end_matches('/').to_string(),
            auth: RwLock::new(Arc::from(format!("Bearer {token}"))),
        }
    }

    pub fn set_token(&self, token: &str) {
        *self.auth.write().unwrap() = Arc::from(format!("Bearer {token}"));
    }

    pub fn token(&self) -> String {
        self.auth.read().unwrap()["Bearer ".len()..].to_string()
    }

    fn auth_header(&self) -> Res<Arc<str>> {
        let auth = self.auth.read().unwrap().clone();
        if auth.len() == "Bearer ".len() {
            return Err(ApiError::Auth("sin sesión".into()));
        }
        Ok(auth)
    }

    fn api8(&self, path: &str) -> String {
        format!("{}/eminusapi8/api{path}", self.origin)
    }

    // El contenido de los cursos y el login sólo están en la API anterior.
    fn api_old(&self, path: &str) -> String {
        format!("{}/eminusapi/api{path}", self.origin)
    }

    fn request(&self, url: &str) -> Res<ureq::Response> {
        let auth = self.auth_header()?;
        match self.agent.get(url).set("Authorization", &auth).set("Accept", "application/json").call() {
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

    pub fn user_info(&self) -> Res<UserInfo> {
        let row = rows_of_old(self.get(&self.api_old("/Usuario/getUsuario"))?).pop()
            .filter(Value::is_object).ok_or_else(|| ApiError::Other("eminus no devolvió tu perfil".into()))?;
        let mut info = model::user_info_from(&row);
        if info.name.is_empty() && info.username.is_empty() {
            return Err(ApiError::Other("eminus no devolvió tu perfil".into()));
        }
        info.photo = row.get("ruta").and_then(Value::as_str).and_then(|path| self.profile_photo(path)).map(Arc::new);
        Ok(info)
    }

    fn profile_photo(&self, path: &str) -> Option<image::RgbaImage> {
        let path = path.trim();
        if !path.contains("profile") || path.contains(":") || path.starts_with("//") || path.split('/').any(|part| part == "..") {
            return None;
        }
        // Eminus carga las fotos del repositorio sin enviar el token.
        let response = self.agent.get(&format!("{}/eminusapi/{}", self.origin, path.trim_start_matches('/'))).call().ok()?;
        let mut bytes = Vec::new();
        const MAX_BYTES: u64 = 2 * 1024 * 1024;
        response.into_reader().take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > MAX_BYTES { return None; }
        let mut reader = image::ImageReader::new(io::Cursor::new(bytes)).with_guessed_format().ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let photo = reader.decode().ok()?;
        Some(photo.resize_to_fill(128, 128, image::imageops::FilterType::Triangle).to_rgba8())
    }

    pub fn activities(&self, course: &Course) -> Res<Vec<Arc<Activity>>> {
        let rows = rows_of(self.get(&self.api8(&format!("/Activity/getActividadesEstudiante/{}", enc(&course.id))))?);
        Ok(model::activities_from(&rows, course))
    }

    // Si la API nueva contesta sin la actividad se pregunta a la anterior,
    // que es la que usa la página de Eminus para este detalle.
    pub fn activity_detail(&self, course_id: &str, id: &str) -> Res<Detail> {
        let path = format!("/getActividadEstudiante/{}/{}", enc(course_id), enc(id));
        let json = match self.get(&self.api8(&format!("/Activity{path}"))) {
            Ok(json) => json,
            Err(ApiError::Other(note)) if note == "HTTP 404" || note == "HTTP 405" => Value::Null,
            Err(err) => return Err(err),
        };
        let row = match detail_row(json) {
            Ok(row) => row,
            Err(missing) => match self.get(&self.api_old(&format!("/Actividad{path}"))).and_then(detail_row) {
                Ok(row) => row,
                Err(err @ ApiError::Auth(_)) => return Err(err),
                Err(ApiError::Other(_)) => return Err(missing),
            },
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

    fn delivery_file_url(&self, course_id: &str, activity_id: &str, file_id: &str) -> String {
        self.api_old(&format!("/actividad/descargaEntregaArchivo/{}/{}/{}", enc(course_id), enc(activity_id), enc(file_id)))
    }

    fn review_file_url(&self, course_id: &str, activity_id: &str, file_id: &str) -> String {
        self.api_old(&format!("/actividad/descargaArchivoActRevision/{}/{}/{}", enc(course_id), enc(activity_id), enc(file_id)))
    }

    // Archivo que el estudiante adjuntó a su entrega. Eminus lo localiza por
    // curso, actividad y archivo; la entrega sólo sirve para no pedir el de
    // un borrador que todavía no existe.
    pub fn download_delivery_file(&self, course_id: &str, activity_id: &str, delivery_id: i64, file_id: &str, name: &str) -> Res<PathBuf> {
        if delivery_id <= 0 || file_id.is_empty() {
            return Err(ApiError::Other(format!("eminus no dio con qué descargar {name}")));
        }
        self.save(&self.delivery_file_url(course_id, activity_id, file_id), name)
    }

    // Archivo que el docente adjuntó a su revisión.
    pub fn download_review_file(&self, course_id: &str, activity_id: &str, file_id: &str, name: &str) -> Res<PathBuf> {
        if file_id.is_empty() {
            return Err(ApiError::Other(format!("eminus no dio con qué descargar {name}")));
        }
        self.save(&self.review_file_url(course_id, activity_id, file_id), name)
    }

    fn save(&self, url: &str, name: &str) -> Res<PathBuf> {
        self.save_to(&downloads_dir(), url, name)
    }

    // Va directo al disco, sin pasar el archivo por memoria.
    fn save_to(&self, dir: &std::path::Path, url: &str, name: &str) -> Res<PathBuf> {
        let res = self.request(url)?;
        let fail = |e: io::Error| ApiError::Other(e.to_string());
        std::fs::create_dir_all(dir).map_err(fail)?;
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
        let auth = self.auth_header()?;
        match self.agent.post(url).set("Authorization", &auth).send_json(body) {
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
        let res = self.request(&url)?;
        Ok(model::delivery_from(&delivery_row_reader(res.into_reader())?.unwrap_or(Value::Null)))
    }

    // Los mismos tres pasos que la página de Eminus: borrador, adjuntos y
    // envío. Si algo falla antes del último, la actividad NO queda entregada
    // (queda el borrador con lo que sí se subió). Eminus contesta 200 aunque
    // no acepte el envío, así que al final se vuelve a leer la entrega, como
    // hace la página, y sólo entonces se da por entregada.
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
        let expected = text::html_to_text(body["texto"].as_str().unwrap_or_default());
        self.post(&endpoint, body)?;
        let same_text = |actual: &str| actual.split_whitespace().eq(expected.split_whitespace());
        match self.delivery_info(course_id, activity_id) {
            Ok(now) if now.id == id && now.sent && same_text(&now.text) && now.files.len() >= attached + files.len() => Ok(()),
            Ok(now) if now.sent => Err(ApiError::Other("eminus muestra una entrega enviada, pero no confirmó el comentario o los archivos nuevos; consulta tu entrega antes de reintentar".into())),
            Ok(_) => Err(ApiError::Other("eminus no registró el envío: la entrega sigue como borrador".into())),
            Err(err @ ApiError::Auth(_)) => Err(err),
            Err(ApiError::Other(why)) => Err(ApiError::Other(format!("se envió, pero no se pudo confirmar ({why}); revísala antes de volver a entregar"))),
        }
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
            .set("Authorization", &self.auth_header()?)
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
    UserInfo(String, Res<UserInfo>),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    type Seen = Arc<Mutex<Vec<String>>>;

    // Un Eminus de mentira: responde según el método y la ruta, y apunta lo
    // que se le pidió.
    fn serve(route: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> (Client, Seen) {
        serve_bytes(move |method, path, _| {
            let (code, body) = route(method, path);
            (code, body.into_bytes())
        })
    }

    fn serve_bytes(route: impl Fn(&str, &str, &str) -> (u16, Vec<u8>) + Send + 'static) -> (Client, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let seen: Seen = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    continue;
                }
                let mut length = 0;
                let mut headers = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    headers.push_str(&line);
                }
                let mut sent = vec![0; length];
                let _ = reader.read_exact(&mut sent);
                let mut parts = first.split_whitespace();
                let (method, path) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
                log.lock().unwrap().push(format!("{method} {path} {}", String::from_utf8_lossy(&sent)).trim().to_string());
                let (code, body) = route(method, path, &headers);
                let _ = write!(stream, "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(&body);
            }
        });
        (Client::with_origin("token", &origin), seen)
    }

    fn other<T>(res: Res<T>) -> String {
        match res {
            Err(ApiError::Other(note)) => note,
            Err(ApiError::Auth(note)) => panic!("error de sesión inesperado: {note}"),
            Ok(_) => panic!("se esperaba un error"),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eminus-prueba-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn user_info_reads_eminus_data_and_loads_the_photo_without_the_token() {
        let photo = image::RgbaImage::from_pixel(320, 160, image::Rgba([30, 60, 90, 255]));
        let mut png = io::Cursor::new(Vec::new());
        photo.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let (client, seen) = serve_bytes(move |method, path, headers| {
            assert_eq!(method, "GET");
            if path == "/eminusapi/api/Usuario/getUsuario" {
                assert!(headers.to_ascii_lowercase().contains("authorization: bearer token"));
                (200, serde_json::to_vec(&json!({ "nombre": "Ana", "paterno": "Pérez", "materno": "López", "idUsuario": "zs123@estudiantes.uv.mx", "perteneceUV": 1, "ruta": "repository/profile/avatar.png" })).unwrap())
            } else {
                assert_eq!(path, "/eminusapi/repository/profile/avatar.png");
                assert!(!headers.to_ascii_lowercase().contains("authorization:"));
                (200, png.get_ref().clone())
            }
        });
        let info = client.user_info().unwrap();
        assert_eq!(info.name, "PÉREZ LÓPEZ ANA");
        assert_eq!(info.username, "zs123");
        let photo = info.photo.unwrap();
        assert_eq!(photo.dimensions(), (128, 128));
        assert_eq!(photo.get_pixel(64, 64).0, [30, 60, 90, 255]);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn user_info_handles_envelopes_and_missing_or_broken_photos() {
        let row = json!({ "nombre": "Ana", "idUsuario": "externa@example.org", "perteneceUV": 0, "ruta": "profile/broken.png" });
        for body in [row.clone(), json!([row.clone()]), json!({"contenido": row.clone()}), json!({"contenido": [row]})] {
            let (client, _) = serve(move |_, path| {
                if path.ends_with("/getUsuario") { (200, body.to_string()) }
                else { (200, "imagen corrupta".into()) }
            });
            let info = client.user_info().unwrap();
            assert_eq!(info.name, "ANA");
            assert_eq!(info.username, "externa@example.org");
            assert!(info.photo.is_none());
        }
        for path in ["", "assets/usuario_generico.svg", "https://example.org/profile/a.png", "//example.org/profile/a.png", "../profile/a.png"] {
            let (client, seen) = serve(move |_, _| (200, json!({"nombre": "Ana", "ruta": path}).to_string()));
            assert!(client.user_info().unwrap().photo.is_none());
            assert_eq!(seen.lock().unwrap().len(), 1);
        }
    }

    #[test]
    fn user_info_rejects_invalid_data_and_keeps_the_name_when_photo_fails() {
        for body in ["[]", "null", "{}", r#"{"mensaje":"No tiene acceso"}"#, "{cortado"] {
            let (client, _) = serve(move |_, _| (200, body.into()));
            assert!(client.user_info().is_err());
        }
        for (code, body) in [(404, Vec::new()), (200, vec![0; 2 * 1024 * 1024 + 1])] {
            let (client, _) = serve_bytes(move |_, path, _| {
                if path.ends_with("/getUsuario") { (200, br#"{"nombre":"Ana","ruta":"profile/a.png"}"#.to_vec()) }
                else { (code, body.clone()) }
            });
            let info = client.user_info().unwrap();
            assert_eq!(info.name, "ANA");
            assert!(info.photo.is_none());
        }
        let mut oversized = io::Cursor::new(Vec::new());
        image::RgbaImage::new(4097, 1).write_to(&mut oversized, image::ImageFormat::Png).unwrap();
        let (client, _) = serve_bytes(move |_, path, _| {
            if path.ends_with("/getUsuario") { (200, br#"{"nombre":"Ana","ruta":"profile/a.png"}"#.to_vec()) }
            else { (200, oversized.get_ref().clone()) }
        });
        assert!(client.user_info().unwrap().photo.is_none());
        let (client, _) = serve(|_, _| (401, String::new()));
        assert!(matches!(client.user_info(), Err(ApiError::Auth(_))));
    }

    #[test]
    fn bodies_and_envelopes() {
        // El detalle llega como objeto, lista o dentro de «contenido».
        for json in [json!({ "idActividad": 3 }), json!([{ "idActividad": 3 }]), json!({ "contenido": { "idActividad": 3 } }), json!({ "contenido": [{ "idActividad": 3 }] })] {
            assert_eq!(detail_row(json).ok().unwrap()["idActividad"], 3);
        }
        assert_eq!(other(detail_row(json!([]))), "eminus no devolvió el detalle de la actividad");
        assert_eq!(other(detail_row(json!({ "contenido": null }))), "eminus no devolvió el detalle de la actividad");
        assert_eq!(other(detail_row(json!({ "contenido": [], "mensaje": "No tiene acceso" }))), "eminus no devolvió la actividad: No tiene acceso");

        let delivery_row = |body: &str| delivery_row_reader(body.as_bytes());
        for body in ["", " \r\n", "null", "[]", "{}", "\u{feff} \r\n"] {
            assert!(delivery_row(body).unwrap().is_none(), "{body:?}");
        }
        assert_eq!(delivery_row("\u{feff}[{\"idActividadEntrega\":4}]").unwrap().unwrap()["idActividadEntrega"], 4);
        assert_eq!(delivery_row(r#"{"contenido":{"idEstado":1}}"#).unwrap().unwrap()["idEstado"], 1);
        assert_eq!(other(delivery_row(r#"{"Message":"An error has occurred."}"#)), "eminus no devolvió la entrega: An error has occurred.");
        for body in ["{", "[", "nu", "{\"idActividadEntrega\":4", "{} {}", "null {}", "<html>error</html>"] {
            assert_eq!(other(delivery_row(body)), "respuesta ilegible de eminus", "{body:?}");
        }
        assert!(delivery_row(r#"{"otraCosa":1}"#).is_err());
        assert!(delivery_row("[1]").is_err());
    }

    #[test]
    fn delivery_reader_handles_small_chunks_and_connection_errors() {
        struct OneByte<'a>(&'a [u8]);
        impl Read for OneByte<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = buf.len().min(1);
                self.0.read(&mut buf[..n])
            }
        }
        let body = "\u{feff} \n{\"idActividadEntrega\":4}";
        assert_eq!(delivery_row_reader(OneByte(body.as_bytes())).unwrap().unwrap()["idActividadEntrega"], 4);
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::ConnectionReset, "corte"))
            }
        }
        for prefix in ["", "{\"idActividadEntrega\":", "null"] {
            assert_eq!(other(delivery_row_reader(prefix.as_bytes().chain(Broken))), "la conexión se cortó o tardó demasiado");
        }
    }

    #[test]
    fn authorization_changes_with_the_session() {
        let client = Client::with_origin("first", "http://127.0.0.1");
        let header = client.auth_header().unwrap();
        assert_eq!(&*header, "Bearer first");
        client.set_token("second");
        assert_eq!(client.token(), "second");
        assert_eq!(&*client.auth_header().unwrap(), "Bearer second");
        assert_eq!(&*header, "Bearer first");
        client.set_token("");
        assert!(client.token().is_empty());
        assert!(matches!(client.auth_header(), Err(ApiError::Auth(_))));
    }

    #[test]
    fn detail_over_http() {
        // La API nueva contesta sin la actividad: se usa la anterior.
        let (client, seen) = serve(|_, path| match path {
            "/eminusapi8/api/Activity/getActividadEstudiante/7/3" => (200, r#"{"contenido":[]}"#.into()),
            "/eminusapi/api/Actividad/getActividadEstudiante/7/3" => (200, r#"{"idActividad":3,"descripcion":"<p>Lee</p>","porEquipos":1}"#.into()),
            _ => (404, String::new()),
        });
        let detail = client.activity_detail("7", "3").ok().unwrap();
        assert_eq!(detail.text, "Lee");
        assert!(detail.blocked.is_some());
        assert_eq!(seen.lock().unwrap().len(), 2);

        let (client, _) = serve(|_, path| if path.starts_with("/eminusapi8/") { (404, String::new()) } else { (200, r#"{"idActividad":3,"descripcion":"API anterior"}"#.into()) });
        assert_eq!(client.activity_detail("7", "3").unwrap().text, "API anterior");

        // Ninguna la trae: es un error, no un detalle vacío.
        let (client, _) = serve(|_, path| if path.starts_with("/eminusapi8/") { (200, r#"{"contenido":null,"mensaje":"Sin permiso"}"#.into()) } else { (500, String::new()) });
        assert_eq!(other(client.activity_detail("7", "3")), "eminus no devolvió la actividad: Sin permiso");

        let (client, _) = serve(|_, _| (200, "<html>mantenimiento</html>".into()));
        assert_eq!(other(client.activity_detail("7", "3")), "respuesta ilegible de eminus");
        let (client, _) = serve(|_, _| (401, String::new()));
        assert!(matches!(client.activity_detail("7", "3"), Err(ApiError::Auth(_))));
    }

    #[test]
    fn delivery_over_http() {
        let (client, seen) = serve(|_, _| (200, r#"{"idActividadEntrega":41,"idEstado":2,"descEntrega":"<p>Listo</p>","fechaEntrega":"2026-03-01T10:00:00","descRevision":"<p>Bien</p>","calificacion":9,"fechaRevision":"2026-03-02T12:30:00","archivosEntregaConTamano":[{"idArchivosEntregas":9,"nombreOriginal":"tarea.pdf","tamano":2048}]}"#.into()));
        let info = client.delivery_info("7", "3").ok().unwrap();
        assert_eq!((info.id, info.sent, info.feedback.as_str(), info.grade, info.files[0].id.as_str()), (41, true, "Bien", Some(9.0), "9"));
        assert!(info.reviewed_at > 0);
        assert_eq!(seen.lock().unwrap()[0], "GET /eminusapi/api/Actividad/getActUsuarioMiRevision/7/3/0");

        // Sin entrega previa: vacío, pero sin error.
        let (client, _) = serve(|_, _| (200, String::new()));
        let none = client.delivery_info("7", "3").ok().unwrap();
        assert_eq!((none.id, none.sent, none.grade), (0, false, None));
        // Una página en vez de JSON no es «sin entrega».
        let (client, _) = serve(|_, _| (200, "<html>mantenimiento</html>".into()));
        assert_eq!(other(client.delivery_info("7", "3")), "respuesta ilegible de eminus");
        let (client, _) = serve(|_, _| (500, String::new()));
        assert_eq!(other(client.delivery_info("7", "3")), "HTTP 500");
    }

    #[test]
    fn downloads_delivery_files() {
        let (client, seen) = serve(|_, path| match path {
            "/eminusapi/api/actividad/descargaEntregaArchivo/7/3/9" => (200, "mi entrega".into()),
            "/eminusapi/api/actividad/descargaArchivoActRevision/7/3/5" => (200, "notas".into()),
            _ => (404, String::new()),
        });
        let dir = temp_dir("descargas");
        let saved = client.save_to(&dir, &client.delivery_file_url("7", "3", "9"), "../tarea.pdf").ok().unwrap();
        assert_eq!((saved.parent(), std::fs::read_to_string(&saved).unwrap().as_str()), (Some(dir.as_path()), "mi entrega"));
        // No pisa lo que ya está descargado.
        let again = client.save_to(&dir, &client.delivery_file_url("7", "3", "9"), "../tarea.pdf").ok().unwrap();
        assert_ne!(saved, again);
        let notes = client.save_to(&dir, &client.review_file_url("7", "3", "5"), "notas.docx").ok().unwrap();
        assert_eq!(std::fs::read_to_string(notes).unwrap(), "notas");
        // Un archivo que Eminus no tiene no deja un archivo vacío.
        assert_eq!(other(client.save_to(&dir, &client.delivery_file_url("7", "3", "10"), "otro.pdf")), "HTTP 404");
        assert!(!dir.join("otro.pdf").exists());
        let _ = std::fs::remove_dir_all(&dir);

        // Sin entrega o sin archivo no se llega a pedir nada.
        let asked = seen.lock().unwrap().len();
        assert!(other(client.download_delivery_file("7", "3", 0, "9", "tarea.pdf")).contains("tarea.pdf"));
        assert!(other(client.download_delivery_file("7", "3", 41, "", "tarea.pdf")).contains("tarea.pdf"));
        assert!(other(client.download_review_file("7", "3", "", "notas.docx")).contains("notas.docx"));
        assert_eq!(seen.lock().unwrap().len(), asked);
    }

    #[test]
    fn deliver_checks_what_eminus_kept() {
        let submit = |after: &'static str| {
            serve(move |method, path| match (method, path) {
                ("POST", "/eminusapi/api/Actividad/InsertaActividadEntrega") => (200, r#"[{"salida":"41"}]"#.into()),
                ("GET", "/eminusapi/api/Actividad/getActUsuarioMiRevision/7/3/0") => (200, after.into()),
                _ => (404, String::new()),
            })
        };
        let (client, seen) = submit(r#"{"idActividadEntrega":41,"idEstado":1,"fechaEntrega":"2026-03-01T10:00:00","descEntrega":"<p>hola &lt;b&gt;<br>fin</p>"}"#);
        assert!(client.deliver("7", "3", 0, "hola <b>\nfin", &[], 0).is_ok());
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 3);
        assert!(seen[0].contains(r#""idActividadEntrega":0"#) && seen[0].contains(r#""idCurso":7"#));
        assert!(seen[1].contains(r#""idActividadEntrega":41"#) && seen[1].contains("hola &lt;b&gt;<br>fin") && seen[1].contains(r#""visible":1"#));

        // HTTP 200 en todo, pero Eminus la dejó como borrador.
        let (client, _) = submit(r#"{"idActividadEntrega":41,"idEstado":0,"fechaEntrega":null}"#);
        assert!(other(client.deliver("7", "3", 41, "hola", &[], 0)).contains("borrador"));
        // Una entrega anterior enviada no confirma una actualización rechazada.
        let (client, _) = submit(r#"{"idActividadEntrega":41,"idEstado":1,"descEntrega":"comentario anterior"}"#);
        assert!(other(client.deliver("7", "3", 41, "comentario nuevo", &[], 0)).contains("no confirmó"));
        let (client, _) = submit(r#"{"idActividadEntrega":99,"idEstado":1,"descEntrega":"hola"}"#);
        assert!(client.deliver("7", "3", 41, "hola", &[], 0).is_err());
        let (client, _) = submit(r#"{"idActividadEntrega":41,"idEstado":1,"descEntrega":"hola"}"#);
        assert!(client.deliver("7", "3", 41, "hola", &[], 1).is_err());
        // No se pudo comprobar: no se afirma que quedó entregada.
        let (client, _) = submit("<html>");
        assert!(other(client.deliver("7", "3", 41, "hola", &[], 0)).contains("no se pudo confirmar"));
        // Sin borrador no hay nada que enviar.
        let (client, seen) = serve(|_, _| (200, r#"[{"salida":0}]"#.into()));
        assert!(other(client.deliver("7", "3", 0, "hola", &[], 0)).contains("borrador"));
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
