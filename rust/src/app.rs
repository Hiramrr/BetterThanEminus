// Estado de la app y lo que pasa con cada tecla y cada respuesta de la red.

use crate::api::{ApiError, Client, Msg, Pool};
use crate::model::{self, Activity, Acts, CFile, Clock, Content, ContentDetail, ContentItem, Counts, Course, DeliveryInfo, Detail, Entry, Row, UserInfo};
use std::path::PathBuf;
use crate::text::{self, Filter};
use crate::{store, theme};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CACHE_MAX_AGE: i64 = 5 * 60;
// El contenido publicado cambia poco y leerlo cuesta varias consultas por
// curso: sólo se vuelve a pedir pasado este tiempo.
const CONTENT_MAX_AGE: i64 = 30 * 60;
// Las descripciones pueden pesar: se guardan sólo las más recientes.
const MAX_DETAILS: usize = 60;
const DETAIL_DELAY: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Agenda,
    Courses,
    Miyu,
    Grades,
    Content,
}

impl View {
    pub const ALL: [View; 5] = [View::Agenda, View::Courses, View::Miyu, View::Grades, View::Content];

    pub fn label(self) -> &'static str {
        match self {
            View::Agenda => "agenda",
            View::Courses => "cursos",
            View::Miyu => "miyu",
            View::Grades => "calificadas",
            View::Content => "contenido",
        }
    }
}

pub enum Slot<T> {
    Pending,
    Ready(T),
    Failed(String),
}

// Caché acotada: al pasar de MAX_DETAILS se va lo más viejo.
pub struct Slots<T> {
    map: HashMap<String, Slot<T>>,
    order: VecDeque<String>,
}

impl<T> Slots<T> {
    fn new() -> Self {
        Slots { map: HashMap::new(), order: VecDeque::new() }
    }

    pub fn get(&self, key: &str) -> Option<&Slot<T>> {
        self.map.get(key)
    }

    fn set(&mut self, key: String, slot: Slot<T>) {
        self.map.insert(key.clone(), slot);
        self.order.retain(|old| old != &key);
        self.order.push_back(key);
        while self.order.len() > MAX_DETAILS {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }

    fn remove(&mut self, key: &str) {
        if self.map.remove(key).is_some() {
            self.order.retain(|k| k != key);
        }
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

#[derive(Default, Clone, Copy)]
pub struct ListState {
    pub sel: usize,
    pub offset: usize,
}

pub struct Login {
    pub user: String,
    pub pass: String,
    pub token: String,
    pub field: usize, // 0 usuario · 1 contraseña · 2 token
    pub note: String,
    pub busy: bool,
    // Selector de perfiles guardados; la última fila es «otro perfil».
    pub picker: bool,
    pub pick: usize,
    confirm_remove: bool,
}

// Ventana para entregar una actividad.
pub struct Delivery {
    pub act: Arc<Activity>,
    pub info: Slot<DeliveryInfo>,
    pub files: Vec<(PathBuf, u64)>,
    pub path: String,
    pub text: String,
    pub field: usize, // 0 archivo · 1 comentario
    pub note: String,
    pub confirm: bool,
    pub busy: bool,
    uploaded_before: HashSet<String>,
    recovering: bool,
}

// Lo que deja una terminal al arrastrarle un archivo: entre comillas, con
// los espacios escapados o como file://.
fn dropped_path(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim();
    let raw = raw.strip_prefix("file://").unwrap_or(raw);
    let unquoted = raw.trim_matches(['"', '\'']);
    let mut unescaped = String::new();
    let mut chars = unquoted.chars();
    while let Some(c) = chars.next() {
        unescaped.push(if c == '\\' { chars.next().unwrap_or(c) } else { c });
    }
    [unquoted.to_string(), unescaped].into_iter().find_map(|candidate| {
        let path = match candidate.strip_prefix("~/") {
            Some(rest) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
            None => PathBuf::from(&candidate),
        };
        path.is_file().then_some(path)
    })
}

// Dónde quedó cada cosa en pantalla, para el ratón.
#[derive(Default)]
pub struct Hit {
    pub tabs: Vec<(u16, u16, View)>,
    pub list: Rect,
    pub mid: Rect,
    pub detail: Rect,
    pub detail_max: u16,
}

pub struct App {
    pub view: View,
    pub focus_mid: bool,
    pub courses: Vec<Course>,
    pub acts: Acts,
    acts_loading: HashSet<String>,
    refresh_ids: HashSet<String>,
    refresh_total: usize,
    refresh_manual: bool,
    pub details: Slots<Detail>,
    pub deliveries: Slots<DeliveryInfo>,
    pub content: Content,
    pub content_details: Slots<ContentDetail>,
    content_files: HashMap<String, Arc<Vec<CFile>>>,
    pub content_loading: bool,
    content_left: usize,
    content_total: usize,
    content_failed: usize,
    pub content_tried: bool,
    content_loaded_at: i64,
    pub show_all: bool,
    pub filter: String,
    filter_text: String,
    pub compiled_filter: Filter,
    pub filtering: bool,
    pub lists: [ListState; 5],
    pub mid: ListState,
    pub rows: Vec<Row>,
    pub mid_rows: Vec<Row>,
    pub average: Option<f64>,
    pub counts: Counts,
    pub clock: Clock,
    model_dirty: bool,
    pub detail_scroll: u16,
    detail_due: Option<Instant>,
    pub loading: bool,
    pub loaded_at: i64,
    pub error: bool,
    pub account: String,
    pub user_info: UserInfo,
    pub expiry: i64,
    epoch: u64,
    msg: String,
    msg_until: Option<Instant>,
    pub theme: usize,
    pub help: bool,
    pub login: Option<Login>,
    pub profiles: store::Profiles,
    // Usuario y contraseña recién usados, mientras se pregunta si guardarlos.
    pub offer: Option<(String, String)>,
    pub delivery: Option<Delivery>,
    saved_draft: Option<store::DeliveryDraft>,
    auto_tried: bool,
    pub pool: Pool,
    pub hit: Hit,
    pub dirty: bool,
    pub quit: bool,
}

fn item_count(rows: &[Row]) -> usize {
    rows.iter().filter(|r| matches!(r, Row::Item { .. })).count()
}

pub fn line_of(rows: &[Row], sel: usize) -> Option<usize> {
    rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Item { .. })).nth(sel).map(|(i, _)| i)
}

fn entry_at(rows: &[Row], sel: usize) -> Option<&Entry> {
    rows.iter().filter_map(|r| if let Row::Item { entry, .. } = r { Some(entry) } else { None }).nth(sel)
}

pub fn detail_key(course_id: &str, id: &str) -> String {
    format!("{course_id}:{id}")
}

fn open_url(url: &str) -> bool {
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    use std::process::Stdio;
    cmd.arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
}

// Los ids de Eminus son números; así los espera la extensión.
fn id_json(id: &str) -> serde_json::Value {
    id.parse::<u64>().map_or_else(|_| json!(id), |n| json!(n))
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.save_delivery_draft();
    }
}

impl App {
    pub fn new() -> Self {
        let token = store::load_token();
        let prefs = store::load_prefs();
        let mut app = Self::with_client(Client::new(""));
        app.theme = theme::index_of(&prefs.tema);
        app.profiles = store::load_profiles();
        let expiry = store::expiry_of(&token);
        if token.is_empty() {
            app.expire("");
        } else if expiry != 0 && expiry <= text::now() {
            app.expire("la sesión guardada ya venció");
        } else {
            app.adopt_token(&token, false);
        }
        app
    }

    // Estado inicial sin leer sesión, perfiles ni preferencias del disco.
    pub fn with_client(client: Client) -> Self {
        let mut app = App {
            view: View::Agenda,
            focus_mid: false,
            courses: Vec::new(),
            acts: Acts::new(),
            acts_loading: HashSet::new(),
            refresh_ids: HashSet::new(),
            refresh_total: 0,
            refresh_manual: false,
            details: Slots::new(),
            deliveries: Slots::new(),
            content: Content::new(),
            content_details: Slots::new(),
            content_files: HashMap::new(),
            content_loading: false,
            content_left: 0,
            content_total: 0,
            content_failed: 0,
            content_tried: false,
            content_loaded_at: 0,
            show_all: false,
            filter: String::new(),
            filter_text: String::new(),
            compiled_filter: Filter::default(),
            filtering: false,
            lists: [ListState::default(); 5],
            mid: ListState::default(),
            rows: Vec::new(),
            mid_rows: Vec::new(),
            average: None,
            counts: Counts::default(),
            clock: Clock { now: text::now(), today: text::start_of_today() },
            model_dirty: true,
            detail_scroll: 0,
            detail_due: None,
            loading: false,
            loaded_at: 0,
            error: false,
            account: String::new(),
            user_info: UserInfo::default(),
            expiry: 0,
            epoch: 0,
            msg: String::new(),
            msg_until: None,
            theme: 0,
            help: false,
            login: None,
            profiles: store::Profiles::default(),
            offer: None,
            delivery: None,
            saved_draft: None,
            auto_tried: false,
            pool: Pool::new(client),
            hit: Hit::default(),
            dirty: true,
            quit: false,
        };
        app.show_login("");
        app
    }

    // ───────────── sesión ─────────────

    fn show_login(&mut self, note: &str) {
        self.login = Some(Login {
            user: String::new(),
            pass: String::new(),
            token: String::new(),
            field: 0,
            note: note.to_string(),
            busy: false,
            picker: !self.profiles.perfiles.is_empty(),
            pick: self.profiles.perfiles.iter().position(|p| p.user == self.profiles.ultimo).unwrap_or(0),
            confirm_remove: false,
        });
        self.help = false;
        self.filtering = false;
        self.dirty = true;
    }

    // La sesión venció sola: si es de un perfil guardado se vuelve a entrar
    // sin preguntar (una vez; si eso falla, queda el selector).
    fn expire(&mut self, note: &str) {
        let user = self.profiles.ultimo.clone();
        if self.auto_tried || !self.profiles.has(&user) {
            return self.end_session(note);
        }
        self.auto_tried = true;
        self.end_session("");
        self.login_profile(user, "la sesión venció: entrando de nuevo…");
    }

    // Entra con la contraseña que el perfil tiene en el llavero.
    fn login_profile(&mut self, user: String, note: &str) {
        if let Some(login) = &mut self.login {
            login.busy = true;
            login.note = note.to_string();
        }
        self.pool.spawn(self.epoch, move |c| {
            let result = store::secret_get(&user).map_err(ApiError::Other).and_then(|pass| c.sign_in(&user, &pass));
            Msg::Login(user, None, result)
        });
    }

    // La sesión dejó de servir: lo que venga en camino ya no cuenta.
    fn end_session(&mut self, note: &str) {
        self.save_delivery_draft();
        self.delivery = None;
        self.saved_draft = None;
        self.epoch += 1;
        self.loading = false;
        self.content_loading = false;
        self.acts_loading.clear();
        self.refresh_ids.clear();
        self.msg.clear();
        self.msg_until = None;
        self.error = false;
        self.user_info = UserInfo::default();
        self.pool.client.set_token("");
        store::save_token("");
        self.show_login(note);
    }

    fn adopt_token(&mut self, token: &str, save: bool) {
        self.pool.client.set_token(token);
        if save {
            store::save_token(token);
        }
        self.epoch += 1;
        self.expiry = store::expiry_of(token);
        self.login = None;
        let account = store::account_of(token);
        if account != self.account || self.courses.is_empty() {
            self.account = account;
            self.forget();
            let now = text::now();
            if let Some((ts, courses, acts)) = store::load_courses(&self.account, now) {
                (self.loaded_at, self.courses, self.acts) = (ts, courses, acts);
            }
            if let Some((ts, content)) = store::load_content(&self.account) {
                (self.content_loaded_at, self.content, self.content_tried) = (ts, content, true);
            }
        }
        self.user_info = UserInfo { name: store::name_of(token), ..Default::default() };
        self.fetch_user_info();
        self.model_dirty = true;
        self.dirty = true;
        if text::now() - self.loaded_at > CACHE_MAX_AGE {
            self.refresh(false);
        } else {
            self.ensure_content(false);
        }
    }

    fn forget(&mut self) {
        self.user_info = UserInfo::default();
        self.courses.clear();
        self.acts.clear();
        self.content.clear();
        self.details.clear();
        self.deliveries.clear();
        self.content_details.clear();
        self.content_files.clear();
        self.loaded_at = 0;
        self.content_loaded_at = 0;
        self.content_tried = false;
        self.lists = [ListState::default(); 5];
        self.mid = ListState::default();
    }

    fn logout(&mut self) {
        self.save_delivery_draft();
        self.account.clear();
        self.forget();
        store::clear_cache();
        self.profiles.ultimo.clear();
        self.profiles.save();
        self.end_session("sesión cerrada en este equipo");
    }

    pub fn session_left(&self) -> Option<i64> {
        (self.expiry != 0 && self.login.is_none()).then(|| self.expiry - text::now())
    }

    pub fn display_name(&self) -> &str {
        if !self.user_info.name.is_empty() { &self.user_info.name }
        else if !self.account.is_empty() { &self.account }
        else { "Sesión de Eminus" }
    }

    // ───────────── mensajes de la barra ─────────────

    fn say(&mut self, text: impl Into<String>, secs: u64) {
        self.msg = text.into();
        self.msg_until = (secs > 0).then(|| Instant::now() + Duration::from_secs(secs));
        self.dirty = true;
    }

    pub fn message(&self) -> String {
        if !self.msg.is_empty() {
            return self.msg.clone();
        }
        if self.loaded_at == 0 || self.login.is_some() {
            return String::new();
        }
        let mins = (text::now() - self.loaded_at + 30) / 60;
        format!("actualizado {} · ? ayuda", if mins < 1 { "ahora".to_string() } else { format!("hace {mins} min") })
    }

    // ───────────── red ─────────────

    fn fetch_user_info(&mut self) {
        let account = self.account.clone();
        self.pool.spawn(self.epoch, move |client| Msg::UserInfo(account, client.user_info()));
    }

    fn refresh(&mut self, manual: bool) {
        if self.loading || self.login.is_some() {
            return;
        }
        self.loading = true;
        self.error = false;
        self.refresh_manual = manual;
        if manual {
            self.fetch_user_info();
            self.details.clear();
            self.deliveries.clear();
            self.detail_due = Some(Instant::now());
        }
        self.say("consultando cursos…", 0);
        self.pool.spawn(self.epoch, |c| Msg::Courses(c.courses()));
    }

    fn load_activities(&mut self, course: &Course) {
        if self.acts_loading.insert(course.id.clone()) {
            let course = course.clone();
            self.pool.spawn(self.epoch, move |c| Msg::Acts(course.id.clone(), c.activities(&course)));
        }
    }

    fn finish_refresh(&mut self) {
        self.loading = false;
        self.loaded_at = text::now();
        self.auto_tried = false;
        self.msg.clear();
        store::save_courses(&self.account, self.loaded_at, &self.courses, &self.acts);
        self.ensure_content(self.refresh_manual);
    }

    // Pide el contenido si falta o ya es viejo (o siempre, con `force`). Se
    // llama al cargar la app, no al abrir la vista: cuando llegas ya está.
    fn ensure_content(&mut self, force: bool) {
        if self.content_loading || self.login.is_some() {
            return;
        }
        if !force && self.content_loaded_at != 0 && text::now() - self.content_loaded_at <= CONTENT_MAX_AGE {
            return;
        }
        let current: Vec<Course> = self.courses.iter().filter(|c| c.current).cloned().collect();
        self.content_tried = true;
        if current.is_empty() {
            return;
        }
        self.content_loading = true;
        self.content_total = current.len();
        self.content_left = current.len();
        self.content_failed = 0;
        for course in current {
            self.pool.spawn(self.epoch, move |c| Msg::Content(course.id.clone(), c.content(&course)));
        }
    }

    fn fail(&mut self, err: &ApiError) {
        match err {
            ApiError::Auth(note) => self.expire(note),
            ApiError::Other(note) => {
                self.error = true;
                self.say(format!("error: {note}"), 6);
            }
        }
    }

    pub fn on_msg(&mut self, epoch: u64, msg: Msg) {
        self.pool.pending = self.pool.pending.saturating_sub(1);
        self.dirty = true;
        if epoch != self.epoch {
            return;
        }
        self.model_dirty = true;
        match msg {
            Msg::UserInfo(account, Ok(mut info)) if account == self.account => {
                if info.name.is_empty() { info.name = self.user_info.name.clone(); }
                if !info.name.is_empty() {
                    if let Some(profile) = self.profiles.perfiles.iter_mut().find(|p| p.user.eq_ignore_ascii_case(&info.username) || p.user.eq_ignore_ascii_case(&self.account)) {
                        if profile.name != info.name {
                            profile.name = info.name.clone();
                            self.profiles.perfiles.sort_by_key(|p| text::norm(&p.name));
                            self.profiles.save();
                        }
                    }
                }
                self.user_info = info;
            }
            // La consulta del perfil no debe interrumpir los cursos ni cerrar la sesión.
            Msg::UserInfo(_, _) => {},
            Msg::Courses(Ok(courses)) => {
                self.courses = courses;
                let current: Vec<Course> = self.courses.iter().filter(|c| c.current).cloned().collect();
                self.refresh_total = current.len();
                self.refresh_ids = current.iter().map(|c| c.id.clone()).collect();
                self.acts_loading.clear();
                current.iter().for_each(|c| self.load_activities(c));
                if current.is_empty() {
                    self.finish_refresh();
                }
            }
            Msg::Courses(Err(err)) => {
                self.loading = false;
                self.fail(&err);
            }
            Msg::Acts(id, res) => {
                self.acts_loading.remove(&id);
                let refreshing = self.refresh_ids.remove(&id);
                match res {
                    Ok(list) => {
                        self.acts.insert(id, list);
                    }
                    Err(err @ ApiError::Auth(_)) => return self.fail(&err),
                    // Un curso ilegible no detiene a los demás.
                    Err(err) if !refreshing => self.fail(&err),
                    Err(_) => {}
                }
                if refreshing {
                    if self.refresh_ids.is_empty() {
                        self.finish_refresh();
                    } else {
                        self.say(format!("actividades {}/{}…", self.refresh_total - self.refresh_ids.len(), self.refresh_total), 0);
                    }
                }
            }
            Msg::Detail(key, Ok(detail)) => self.details.set(key, Slot::Ready(detail)),
            Msg::Detail(key, Err(ApiError::Other(note))) => self.details.set(key, Slot::Failed(note)),
            Msg::Detail(key, Err(err)) => {
                self.details.remove(&key);
                self.fail(&err);
            }
            Msg::Content(id, res) => {
                match res {
                    Ok(items) => {
                        self.content.insert(id, items);
                    }
                    Err(err @ ApiError::Auth(_)) => return self.fail(&err),
                    Err(_) => self.content_failed += 1,
                }
                self.content_left = self.content_left.saturating_sub(1);
                if self.content_left > 0 {
                    if self.view == View::Content && !self.loading {
                        self.say(format!("contenido {}/{}…", self.content_total - self.content_left, self.content_total), 0);
                    }
                    return;
                }
                self.content_loading = false;
                self.content_files.clear();
                self.content_details.clear();
                self.detail_due = Some(Instant::now());
                // Con cursos sin leer no se da por fresco: se reintenta al volver.
                if self.content_failed == 0 {
                    self.content_loaded_at = text::now();
                    store::save_content(&self.account, self.content_loaded_at, &self.content);
                    if !self.loading {
                        self.msg.clear();
                    }
                } else {
                    let n = self.content_failed;
                    self.say(format!("contenido: no se pudo leer {n} {}", if n == 1 { "curso" } else { "cursos" }), 6);
                }
            }
            Msg::ContentDetail(id, course_id, located, res) => {
                if let Some(files) = located {
                    self.content_files.insert(course_id, files);
                }
                match res {
                    Ok(detail) => self.content_details.set(id, Slot::Ready(detail)),
                    Err(ApiError::Other(note)) => self.content_details.set(id, Slot::Failed(note)),
                    Err(err) => {
                        self.content_details.remove(&id);
                        self.fail(&err);
                    }
                }
            }
            Msg::Delivery(key, res) => {
                match res {
                    Ok(info) => {
                        if let Some(delivery) = self.delivery.as_mut().filter(|d| detail_key(&d.act.course_id, &d.act.id) == key) {
                            if delivery.recovering {
                                let mut uploaded: HashMap<(String, String), usize> = HashMap::new();
                                for file in info.files.iter().filter(|f| !f.id.is_empty() && !delivery.uploaded_before.contains(&f.id)) {
                                    *uploaded.entry((file.name.clone(), file.size.clone())).or_default() += 1;
                                }
                                delivery.files.retain(|(path, size)| {
                                    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                                    let name = name.replace(['"', '\r', '\n'], "_");
                                    let size = model::file_size(*size as f64);
                                    if let Some(count) = uploaded.get_mut(&(name, size)).filter(|n| **n > 0) {
                                        *count -= 1;
                                        false
                                    } else {
                                        true
                                    }
                                });
                                if uploaded.values().any(|n| *n > 0) {
                                    delivery.note.push_str(". Hay archivos nuevos en eminus que no pude asociar a la lista local; revisa los adjuntos antes de reintentar");
                                }
                                delivery.recovering = false;
                            }
                            if delivery.text.is_empty() && self.saved_draft.is_none() {
                                delivery.text = info.text.clone();
                            }
                            delivery.info = Slot::Ready(info.clone());
                        }
                        self.deliveries.set(key, Slot::Ready(info));
                    }
                    Err(ApiError::Other(note)) => {
                        if let Some(delivery) = self.delivery.as_mut().filter(|d| detail_key(&d.act.course_id, &d.act.id) == key) {
                            delivery.info = Slot::Failed(note.clone());
                        }
                        self.deliveries.set(key, Slot::Failed(note));
                    }
                    Err(err) => {
                        self.deliveries.remove(&key);
                        self.fail(&err);
                    }
                }
            }
            Msg::Delivered(course_id, title, Ok(())) => {
                let mut draft_error = None;
                if let Some(delivery) = &self.delivery {
                    let key = detail_key(&delivery.act.course_id, &delivery.act.id);
                    if let Err(err) = store::save_draft(&self.account, &key, &store::DeliveryDraft::default()) {
                        draft_error = Some(err);
                    }
                    self.details.remove(&key);
                    self.deliveries.remove(&key);
                }
                self.delivery = None;
                self.saved_draft = None;
                self.detail_due = Some(Instant::now());
                self.say(match draft_error {
                    Some(err) => format!("✓ entregada: {title}; no se pudo borrar el borrador local: {err}"),
                    None => format!("✓ entregada: {title}"),
                }, 8);
                // La lista se vuelve a leer para que salga como entregada.
                if let Some(course) = self.courses.iter().find(|c| c.id == course_id).cloned() {
                    self.load_activities(&course);
                }
            }
            Msg::Delivered(_, _, Err(ApiError::Other(note))) => {
                if let Some(delivery) = &mut self.delivery {
                    delivery.busy = false;
                    delivery.confirm = false;
                    delivery.recovering = true;
                    delivery.info = Slot::Pending;
                    delivery.note = format!("no se pudo confirmar la entrega: {note}. Consultando lo que quedó en eminus");
                    let act = delivery.act.clone();
                    self.deliveries.remove(&detail_key(&act.course_id, &act.id));
                    self.fetch_delivery(&act);
                }
            }
            Msg::Delivered(_, _, Err(err)) => {
                if let Some(delivery) = &mut self.delivery {
                    delivery.busy = false;
                    delivery.confirm = false;
                    delivery.note = "la sesión venció durante el envío; consulta tu entrega antes de reintentar".into();
                }
                self.fail(&err);
                self.login_note("la sesión venció durante el envío; vuelve a entrar y consulta tu entrega antes de reintentar. Tu borrador local se conserva");
            }
            Msg::Downloaded(name, Ok(path)) => {
                let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or(name);
                self.say(format!("✓ descargado en ~/Downloads/{file}"), 8);
            }
            Msg::Downloaded(name, Err(ApiError::Other(note))) => self.say(format!("no se pudo descargar {name} ({note})"), 6),
            Msg::Downloaded(_, Err(err)) => self.fail(&err),
            Msg::Login(user, pass, Ok(token)) => match store::clean_token(&token) {
                Some(token) => {
                    self.adopt_token(&token, true);
                    self.profiles.ultimo = user.clone();
                    self.profiles.save();
                    match pass {
                        // Perfil ya guardado y contraseña escrita a mano: se
                        // pone al día en el llavero sin preguntar.
                        Some(pass) if self.profiles.has(&user) => {
                            let profile = self.profiles.perfiles.iter().find(|p| p.user == user).cloned();
                            if let Some(profile) = profile {
                                self.pool.spawn(self.epoch, move |_| Msg::ProfileSaved(profile.clone(), store::secret_set(&profile.user, &pass)));
                            }
                        }
                        Some(pass) => self.offer = Some((user, pass)),
                        None => {}
                    }
                }
                None => self.login_note("eminus respondió algo inesperado"),
            },
            Msg::Login(user, pass, Err(err)) => {
                self.login_note(err.message());
                // Falló un perfil guardado: queda el formulario con su usuario.
                if let (None, Some(login)) = (pass, &mut self.login) {
                    login.picker = false;
                    login.user = user;
                    login.field = 1;
                }
            }
            Msg::ProfileSaved(mut profile, Ok(())) => {
                if !self.user_info.name.is_empty() && (profile.user.eq_ignore_ascii_case(&self.user_info.username) || profile.user.eq_ignore_ascii_case(&self.account)) {
                    profile.name = self.user_info.name.clone();
                }
                if !self.profiles.has(&profile.user) {
                    self.say(format!("perfil guardado: {}", profile.name), 4);
                    self.profiles.perfiles.push(profile);
                    self.profiles.perfiles.sort_by_key(|p| text::norm(&p.name));
                }
                self.profiles.save();
            }
            Msg::ProfileSaved(_, Err(note)) => self.say(note, 6),
        }
    }

    fn login_note(&mut self, note: &str) {
        if let Some(login) = &mut self.login {
            login.busy = false;
            login.note = note.to_string();
        }
    }

    // ───────────── modelo ─────────────

    pub fn list(&self) -> ListState {
        self.lists[self.view as usize]
    }

    fn rebuild(&mut self) {
        self.model_dirty = false;
        self.clock = Clock { now: text::now(), today: text::start_of_today() };
        if self.filter != self.filter_text {
            self.compiled_filter = Filter::new(&self.filter);
            self.filter_text.clone_from(&self.filter);
        }
        let filter = &self.compiled_filter;
        let none = Filter::default();
        self.mid_rows.clear();
        self.average = None;
        // Las vistas rellenan las listas que ya hay: su capacidad se reusa.
        match self.view {
            View::Agenda => model::agenda_into(&mut self.rows, &self.courses, &self.acts, filter, self.show_all, &self.clock),
            View::Miyu => model::miyu_into(&mut self.rows, &self.courses, &self.acts, filter, &self.clock),
            View::Grades => self.average = model::grades_into(&mut self.rows, &self.courses, &self.acts, filter),
            View::Content => model::content_into(&mut self.rows, &self.courses, &self.content, filter),
            View::Courses => {
                let loading = &self.acts_loading;
                model::course_rows_into(&mut self.rows, &self.courses, &self.acts, |id| loading.contains(id), if self.focus_mid { &none } else { filter }, self.show_all)
            }
        }
        let state = &mut self.lists[self.view as usize];
        state.sel = state.sel.min(item_count(&self.rows).saturating_sub(1));
        if self.view == View::Courses {
            if let Some(Entry::Course(course)) = entry_at(&self.rows, state.sel).cloned() {
                match self.acts.get(&course.id) {
                    Some(list) => model::course_acts_into(&mut self.mid_rows, list, if self.focus_mid { filter } else { &none }, &self.clock),
                    None => self.load_activities(&course),
                }
            }
            self.mid.sel = self.mid.sel.min(item_count(&self.mid_rows).saturating_sub(1));
        }
        self.counts = model::counts(&self.courses, &self.acts, &self.clock);
        // Lo seleccionado puede ser otro tras recalcular (o ser el primero al
        // arrancar): su descripción se pide igual que al mover el cursor.
        self.detail_due.get_or_insert_with(Instant::now);
    }

    // Lo que muestra el panel de detalle ahora mismo.
    pub fn target(&self) -> Option<&Entry> {
        if self.view == View::Courses && self.focus_mid {
            entry_at(&self.mid_rows, self.mid.sel)
        } else {
            entry_at(&self.rows, self.list().sel)
        }
    }

    pub fn selected_course(&self) -> Option<&Course> {
        match entry_at(&self.rows, self.list().sel) {
            Some(Entry::Course(c)) if self.view == View::Courses => Some(c),
            _ => None,
        }
    }

    pub fn acts_pending(&self, course_id: &str) -> bool {
        self.acts_loading.contains(course_id) || !self.acts.contains_key(course_id)
    }

    fn selection_changed(&mut self) {
        self.detail_scroll = 0;
        self.detail_due = Some(Instant::now() + DETAIL_DELAY);
        self.dirty = true;
    }

    fn fetch_act_detail(&mut self, course_id: &str, id: &str) {
        let key = detail_key(course_id, id);
        if self.details.get(&key).is_none() {
            self.details.set(key.clone(), Slot::Pending);
            let (course_id, id) = (course_id.to_string(), id.to_string());
            self.pool.spawn(self.epoch, move |c| Msg::Detail(key, c.activity_detail(&course_id, &id)));
        }
    }

    fn fetch_content_detail(&mut self, item: &Arc<ContentItem>) {
        if self.content_details.get(&item.id).is_some() {
            return;
        }
        self.content_details.set(item.id.clone(), Slot::Pending);
        let known = self.content_files.get(&item.course_id).cloned();
        let item = item.clone();
        self.pool.spawn(self.epoch, move |c| {
            // Una consulta por curso, compartida por todos sus elementos.
            let (located, fresh) = match known {
                Some(files) => (files, None),
                None => match c.course_files(&item.course_id) {
                    Ok(files) => {
                        let files = Arc::new(files);
                        (files.clone(), Some(files))
                    }
                    Err(err @ ApiError::Auth(_)) => return Msg::ContentDetail(item.id.clone(), item.course_id.clone(), None, Err(err)),
                    Err(ApiError::Other(_)) => (Arc::new(Vec::new()), None),
                },
            };
            Msg::ContentDetail(item.id.clone(), item.course_id.clone(), fresh, c.content_detail(&item, &located))
        });
    }

    // Pide la descripción de lo seleccionado y de la fila siguiente, para
    // que al bajar con j ya esté lista.
    fn fetch_details(&mut self) {
        if self.login.is_some() {
            return;
        }
        let (rows, sel) = if self.view == View::Courses && self.focus_mid { (&self.mid_rows, self.mid.sel) } else { (&self.rows, self.list().sel) };
        let wanted: Vec<Entry> = [sel, sel + 1].iter().filter_map(|i| entry_at(rows, *i).cloned()).collect();
        for (i, entry) in wanted.iter().enumerate() {
            match entry {
                Entry::Act(act) => {
                    self.fetch_act_detail(&act.course_id, &act.id);
                    if i == 0 {
                        self.fetch_delivery(act);
                    }
                }
                Entry::Content(item) if i == 0 => self.fetch_content_detail(item),
                _ => {}
            }
        }
    }

    // ───────────── reloj ─────────────

    // Se llama en cada vuelta del ciclo, antes de pintar.
    pub fn tick(&mut self) {
        self.save_delivery_draft();
        let instant = Instant::now();
        if self.msg_until.is_some_and(|t| instant >= t) {
            self.msg.clear();
            self.msg_until = None;
            self.dirty = true;
        }
        if self.session_left().is_some_and(|left| left <= 0) {
            self.expire("la sesión venció (duran 2 horas)");
        }
        // Lo que depende de la hora («en 5m», grupos de la agenda) se
        // renueva solo cada 15 s.
        if self.model_dirty || text::now() - self.clock.now >= 15 {
            self.rebuild();
            self.dirty = true;
        }
        if self.detail_due.is_some_and(|t| instant >= t) {
            self.detail_due = None;
            self.fetch_details();
            self.dirty = true;
        }
    }

    // Cuánto puede dormir el ciclo sin que se note.
    pub fn wait(&self) -> Duration {
        if self.pool.pending > 0 {
            return Duration::from_millis(80);
        }
        let mut wait = if self.view == View::Miyu && self.login.is_none() { Duration::from_secs(1) } else { Duration::from_secs(15) };
        let now = Instant::now();
        for due in [self.detail_due, self.msg_until].into_iter().flatten() {
            wait = wait.min(due.saturating_duration_since(now));
        }
        wait
    }

    // ───────────── navegación ─────────────

    pub fn course_view(&mut self, course_id: &str, view: View) {
        if !matches!(view, View::Courses | View::Content) {
            return;
        }
        if view == View::Content && self.courses.iter().any(|c| c.id == course_id && !c.current) {
            return self.say("el contenido de cursos anteriores aún no se carga en la app", 5);
        }
        self.set_view(view);
        self.filter.clear();
        self.filtering = false;
        self.focus_mid = false;
        self.mid = ListState::default();
        if self.courses.iter().any(|c| c.id == course_id && !c.current) {
            self.show_all = true;
        }
        self.rebuild();
        if let Some(index) = self.rows.iter().filter_map(|r| match r {
            Row::Item { entry, .. } => Some(entry),
            _ => None,
        }).position(|entry| match entry {
            Entry::Course(c) => c.id == course_id,
            Entry::Content(c) => c.course_id == course_id,
            _ => false,
        }) {
            self.lists[view as usize].sel = index;
            self.rebuild();
            if view == View::Courses {
                self.focus_mid = true;
            }
        }
        self.selection_changed();
    }

    fn activate(&mut self, course_only: bool) {
        match self.target().cloned() {
            Some(Entry::Course(c)) => self.course_view(&c.id, View::Courses),
            Some(Entry::Act(a)) if course_only => self.course_view(&a.course_id, View::Courses),
            Some(Entry::Content(c)) if course_only => self.course_view(&c.course_id, View::Content),
            Some(Entry::Act(a)) => {
                let key = detail_key(&a.course_id, &a.id);
                if matches!(self.details.get(&key), Some(Slot::Failed(_))) || matches!(self.deliveries.get(&key), Some(Slot::Failed(_))) {
                    self.retry_detail(&a);
                } else {
                    self.fetch_act_detail(&a.course_id, &a.id);
                    self.fetch_delivery(&a);
                }
                self.detail_scroll = 0;
            }
            Some(Entry::Content(c)) => {
                if matches!(self.content_details.get(&c.id), Some(Slot::Failed(_))) {
                    self.content_details.remove(&c.id);
                }
                self.fetch_content_detail(&c);
                self.detail_scroll = 0;
            }
            None => {}
        }
    }

    fn retry_detail(&mut self, act: &Arc<Activity>) {
        let key = detail_key(&act.course_id, &act.id);
        if matches!(self.details.get(&key), Some(Slot::Failed(_))) {
            self.details.remove(&key);
        }
        self.fetch_act_detail(&act.course_id, &act.id);
        if !matches!(self.deliveries.get(&key), Some(Slot::Pending)) {
            self.deliveries.remove(&key);
        }
        self.fetch_delivery(act);
    }

    fn open_in_eminus(&mut self, target: serde_json::Value, what: &str) {
        let raw = target.to_string();
        let mut encoded = String::with_capacity(raw.len() * 2);
        for b in raw.bytes() {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                encoded.push(b as char);
            } else {
                encoded.push_str(&format!("%{b:02X}"));
            }
        }
        // Con la extensión instalada, este destino abre directo el detalle.
        if open_url(&format!("https://eminus.uv.mx/eminus4/page/course/list#bte={encoded}")) {
            self.say(format!("abriendo {what} en eminus…"), 3);
        } else {
            self.say("no se pudo abrir el navegador", 5);
        }
    }

    fn open_course(&mut self, name: &str, id: &str, section: Option<&str>) {
        let mut target = json!({ "course": name, "courseId": id_json(id) });
        if let Some(section) = section {
            target["section"] = json!(section);
        }
        self.open_in_eminus(target, &name.to_lowercase());
    }

    fn open_web(&mut self, course_only: bool) {
        match self.target().cloned() {
            Some(Entry::Act(act)) if course_only => self.open_course(&act.course_name, &act.course_id, None),
            Some(Entry::Act(act)) => self.open_in_eminus(
                json!({ "course": &*act.course_name, "section": "activities", "activity": { "id": id_json(&act.id), "courseId": id_json(&act.course_id) } }),
                &format!("«{}»", act.title),
            ),
            Some(Entry::Course(c)) => self.open_course(&c.name, &c.id, None),
            Some(Entry::Content(it)) if course_only => self.open_course(&it.course_name, &it.course_id, None),
            Some(Entry::Content(it)) => self.open_in_eminus(
                json!({ "course": &*it.course_name, "courseId": id_json(&it.course_id), "section": "content", "unit": id_json(&it.unit_id) }),
                &format!("«{}»", it.unit_name),
            ),
            None => {}
        }
    }

    // ───────────── entregar ─────────────

    fn fetch_delivery(&mut self, act: &Arc<Activity>) {
        let (key, course_id, id) = (detail_key(&act.course_id, &act.id), act.course_id.clone(), act.id.clone());
        if self.deliveries.get(&key).is_some() {
            return;
        }
        self.deliveries.set(key.clone(), Slot::Pending);
        self.pool.spawn(self.epoch, move |c| Msg::Delivery(key, c.delivery_info(&course_id, &id)));
    }

    // Por qué no se puede entregar desde aquí (None: sí se puede).
    pub fn delivery_block(&self, act: &Activity) -> Option<&str> {
        match self.details.get(&detail_key(&act.course_id, &act.id)) {
            Some(Slot::Ready(detail)) => detail.blocked.as_deref(),
            _ => None,
        }
    }

    // E: abre la ventana de entrega de la actividad seleccionada.
    fn open_delivery(&mut self) {
        let Some(Entry::Act(act)) = self.target().cloned() else {
            return self.say("E entrega la actividad seleccionada (elige una en agenda, cursos o miyu)", 4);
        };
        if let Some(reason) = self.delivery_block(&act) {
            return self.say(reason.to_string(), 6);
        }
        self.fetch_act_detail(&act.course_id, &act.id);
        let key = detail_key(&act.course_id, &act.id);
        let draft = store::load_draft(&self.account, &key);
        let recovering = draft.as_ref().is_some_and(|d| d.recovering);
        let mut files = Vec::new();
        let mut note = String::new();
        if let Some(draft) = &draft {
            note = "borrador local recuperado".into();
            for path in &draft.files {
                if let Ok(meta) = path.metadata().and_then(|m| if m.is_file() { Ok(m) } else { Err(std::io::Error::other("no es un archivo")) }) {
                    files.push((path.clone(), meta.len()));
                } else {
                    files.push((path.clone(), 0));
                    note = format!("borrador recuperado; falta el archivo {}", path.display());
                }
            }
        }
        let info = match self.deliveries.get(&key) {
            _ if recovering => Slot::Pending,
            Some(Slot::Ready(info)) => Slot::Ready(info.clone()),
            _ => Slot::Pending,
        };
        let text = draft.as_ref().map(|d| d.text.clone()).unwrap_or_else(|| match &info {
            Slot::Ready(info) => info.text.clone(),
            _ => String::new(),
        });
        let uploaded_before = draft.as_ref().map(|d| d.uploaded_before.iter().cloned().collect()).unwrap_or_default();
        self.saved_draft = draft;
        self.delivery = Some(Delivery { act: act.clone(), info, files, path: String::new(), text, field: 0, note, confirm: false, busy: false, uploaded_before, recovering });
        if matches!(self.deliveries.get(&key), Some(Slot::Failed(_))) || recovering && !matches!(self.deliveries.get(&key), Some(Slot::Pending)) {
            self.deliveries.remove(&key);
        }
        self.fetch_delivery(&act);
    }

    pub fn save_delivery_draft(&mut self) -> bool {
        let Some(delivery) = &self.delivery else { return true };
        if delivery.busy || self.account.is_empty() || matches!(delivery.info, Slot::Pending) && self.saved_draft.is_none() && delivery.text.is_empty() && delivery.files.is_empty() {
            return true;
        }
        let mut uploaded_before: Vec<_> = delivery.uploaded_before.iter().cloned().collect();
        uploaded_before.sort();
        let draft = store::DeliveryDraft { text: delivery.text.clone(), files: delivery.files.iter().map(|(p, _)| p.clone()).collect(), uploaded_before, recovering: delivery.recovering };
        if self.saved_draft.as_ref() == Some(&draft) {
            return true;
        }
        let key = detail_key(&delivery.act.course_id, &delivery.act.id);
        match store::save_draft(&self.account, &key, &draft) {
            Ok(()) => {
                self.saved_draft = Some(draft);
                true
            }
            Err(err) => {
                self.say(format!("no se pudo guardar el borrador local: {err}"), 6);
                false
            }
        }
    }

    pub fn retry_delivery(&mut self) {
        let Some(delivery) = &mut self.delivery else { return };
        if delivery.busy {
            return;
        }
        delivery.info = Slot::Pending;
        delivery.confirm = false;
        let act = delivery.act.clone();
        self.retry_detail(&act);
        self.dirty = true;
    }

    pub fn add_delivery_file(&mut self, raw: &str) {
        let Some(delivery) = &mut self.delivery else { return };
        match dropped_path(raw) {
            Some(path) if delivery.files.iter().any(|(p, _)| *p == path) => delivery.note = "ese archivo ya está en la lista".into(),
            Some(path) => {
                let size = path.metadata().map_or(0, |m| m.len());
                delivery.files.push((path, size));
                delivery.path.clear();
                delivery.note.clear();
            }
            None => delivery.note = format!("no existe ese archivo: {}", raw.trim()),
        }
    }

    fn confirm_delivery(&mut self) {
        let Some(delivery) = &self.delivery else { return };
        let (course_id, id) = (&delivery.act.course_id, &delivery.act.id);
        let note = match (&delivery.info, self.details.get(&detail_key(course_id, id))) {
            (_, Some(Slot::Ready(Detail { blocked: Some(reason), .. }))) => Some(reason.as_str()),
            (Slot::Ready(_), Some(Slot::Ready(_))) if delivery.text.trim().is_empty() => Some("escribe un comentario: eminus no acepta entregas sin texto"),
            (Slot::Ready(_), Some(Slot::Ready(_))) if delivery.files.iter().any(|(p, _)| !p.is_file()) => Some("hay un archivo que ya no existe; vuelve a elegirlo o quítalo de la entrega"),
            (Slot::Ready(_), Some(Slot::Ready(_))) => None,
            (Slot::Failed(_), _) | (_, Some(Slot::Failed(_))) => Some("no se pudo consultar la actividad en eminus: cierra (esc) e intenta de nuevo"),
            _ => Some("espera un momento: consultando la actividad en eminus…"),
        };
        if let Some(delivery) = &mut self.delivery {
            delivery.confirm = note.is_none();
            delivery.note = note.unwrap_or_default().to_string();
        }
    }

    fn send_delivery(&mut self) {
        let Some(delivery) = &mut self.delivery else { return };
        let Slot::Ready(info) = &delivery.info else { return };
        delivery.uploaded_before = info.files.iter().filter(|f| !f.id.is_empty()).map(|f| f.id.clone()).collect();
        delivery.recovering = true;
        if !self.save_delivery_draft() {
            let note = self.message();
            if let Some(delivery) = &mut self.delivery {
                delivery.note = note;
                delivery.confirm = false;
            }
            return;
        }
        let Some(delivery) = &mut self.delivery else { return };
        let Slot::Ready(info) = &delivery.info else { return };
        delivery.busy = true;
        delivery.note = "subiendo y entregando…".into();
        let (act, text, id, attached) = (delivery.act.clone(), delivery.text.clone(), info.id, info.files.len());
        let files: Vec<PathBuf> = delivery.files.iter().map(|(path, _)| path.clone()).collect();
        self.pool.spawn(self.epoch, move |c| Msg::Delivered(act.course_id.clone(), act.title.to_string(), c.deliver(&act.course_id, &act.id, id, &text, &files, attached)));
    }

    fn delivery_key(&mut self, key: KeyEvent) {
        let Some(delivery) = &mut self.delivery else { return };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if delivery.busy {
            return;
        }
        if ctrl && key.code == KeyCode::Char('r') {
            return self.retry_delivery();
        }
        if delivery.confirm {
            match key.code {
                KeyCode::Char('s' | 'S') => self.send_delivery(),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => delivery.confirm = false,
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Esc => {
                if !self.save_delivery_draft() {
                    let note = self.message();
                    if let Some(delivery) = &mut self.delivery {
                        delivery.note = note;
                    }
                    return;
                }
                self.delivery = None;
                self.saved_draft = None;
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => delivery.field = 1 - delivery.field,
            KeyCode::Char('s') if ctrl => self.confirm_delivery(),
            KeyCode::Enter if delivery.field == 0 && !delivery.path.trim().is_empty() => {
                let raw = delivery.path.clone();
                self.add_delivery_file(&raw);
            }
            KeyCode::Enter if delivery.field == 0 => delivery.field = 1,
            KeyCode::Enter => self.confirm_delivery(),
            KeyCode::Backspace if delivery.field == 0 && delivery.path.is_empty() => {
                delivery.files.pop();
            }
            KeyCode::Backspace => {
                [&mut delivery.path, &mut delivery.text][delivery.field].pop();
            }
            KeyCode::Char('u') if ctrl => [&mut delivery.path, &mut delivery.text][delivery.field].clear(),
            KeyCode::Char(c) if !ctrl => [&mut delivery.path, &mut delivery.text][delivery.field].push(c),
            _ => {}
        }
    }

    // D: descarga a ~/Downloads los archivos de lo seleccionado.
    pub fn download_activity_file(&mut self, act: &Activity, file_id: &str, name: &str) {
        let (course_id, activity_id, file_id, name) = (act.course_id.clone(), act.id.clone(), file_id.to_string(), name.to_string());
        if file_id.is_empty() {
            return self.say("eminus no identificó este archivo", 4);
        }
        self.say("descargando…", 0);
        self.pool.spawn(self.epoch, move |c| Msg::Downloaded(name.clone(), c.download_activity_file(&course_id, &activity_id, &file_id, &name)));
    }

    pub fn download_delivery_file(&mut self, act: &Activity, file_id: &str, name: &str) {
        let id = match self.deliveries.get(&detail_key(&act.course_id, &act.id)) {
            Some(Slot::Ready(info)) if info.id > 0 => info.id,
            _ => return self.say("consulta tu entrega con V antes de descargar", 4),
        };
        if file_id.is_empty() {
            return self.say("eminus no identificó este archivo", 4);
        }
        let (course_id, activity_id, file_id, name) = (act.course_id.clone(), act.id.clone(), file_id.to_string(), name.to_string());
        self.say("descargando…", 0);
        self.pool.spawn(self.epoch, move |c| Msg::Downloaded(name.clone(), c.download_delivery_file(&course_id, &activity_id, id, &file_id, &name)));
    }

    pub fn download_review_file(&mut self, act: &Activity, file_id: &str, name: &str) {
        if file_id.is_empty() {
            return self.say("eminus no identificó este archivo", 4);
        }
        let (course_id, activity_id, file_id, name) = (act.course_id.clone(), act.id.clone(), file_id.to_string(), name.to_string());
        self.say("descargando…", 0);
        self.pool.spawn(self.epoch, move |c| Msg::Downloaded(name.clone(), c.download_review_file(&course_id, &activity_id, &file_id, &name)));
    }

    fn download_submission(&mut self) {
        let Some(Entry::Act(act)) = self.target().cloned() else {
            return self.say("B descarga los archivos de tu entrega", 4);
        };
        let (files, review): (Vec<_>, Vec<_>) = match self.deliveries.get(&detail_key(&act.course_id, &act.id)) {
            Some(Slot::Ready(info)) => (info.files.iter().map(|f| (f.id.clone(), f.name.clone())).collect(), info.review_files.iter().map(|f| (f.id.clone(), f.name.clone())).collect()),
            _ => {
                self.retry_detail(&act);
                return self.say("consultando tu entrega; pulsa B cuando termine", 4);
            }
        };
        if files.is_empty() && review.is_empty() {
            return self.say("tu entrega y revisión no tienen archivos adjuntos", 4);
        }
        for (id, name) in files {
            self.download_delivery_file(&act, &id, &name);
        }
        for (id, name) in review {
            self.download_review_file(&act, &id, &name);
        }
    }

    fn download_selected(&mut self) {
        if let Some(Entry::Act(act)) = self.target().cloned() {
            let files: Vec<(String, String)> = match self.details.get(&detail_key(&act.course_id, &act.id)) {
                Some(Slot::Ready(detail)) => detail.files.iter().filter(|f| !f.id.is_empty()).map(|f| (f.id.clone(), f.name.clone())).collect(),
                Some(Slot::Failed(note)) => return self.say(format!("no se pudo cargar la actividad: {note}; pulsa enter para reintentar"), 5),
                _ => return self.say("la actividad aún se está cargando: pulsa D otra vez en un momento", 4),
            };
            if files.is_empty() {
                return self.say(format!("«{}» no tiene archivos adjuntos", act.title), 4);
            }
            self.say(if files.len() == 1 { "descargando…".to_string() } else { format!("descargando {} archivos…", files.len()) }, 0);
            for (file_id, name) in files {
                let act = act.clone();
                self.pool.spawn(self.epoch, move |c| Msg::Downloaded(name.clone(), c.download_activity_file(&act.course_id, &act.id, &file_id, &name)));
            }
            return;
        }
        let Some(Entry::Content(item)) = self.target().cloned() else {
            return self.say("D descarga los archivos de una actividad o de un elemento de contenido", 4);
        };
        let files = match self.content_details.get(&item.id) {
            Some(Slot::Ready(detail)) => detail.files.clone(),
            Some(Slot::Failed(_)) => item.files.clone(),
            _ => {
                self.fetch_content_detail(&item);
                return self.say("buscando archivos… pulsa D otra vez en un momento", 4);
            }
        };
        if files.is_empty() {
            return self.say(format!("«{}» no tiene archivos", item.title), 4);
        }
        let mut started = 0;
        for file in files {
            let element_id = if file.element_id.is_empty() { item.element_id.clone() } else { file.element_id.clone() };
            if element_id.is_empty() {
                continue;
            }
            started += 1;
            let course_id = item.course_id.clone();
            self.pool.spawn(self.epoch, move |c| Msg::Downloaded(file.name.clone(), c.download(&course_id, &element_id, &file)));
        }
        match started {
            0 => self.say("estos archivos sólo se pueden descargar desde eminus", 5),
            1 => self.say("descargando…", 0),
            n => self.say(format!("descargando {n} archivos…"), 0),
        }
    }

    // ───────────── teclado ─────────────

    // Para la ventana: lo mismo que pulsar esa tecla en la terminal.
    pub fn press(&mut self, code: KeyCode) {
        self.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    // Clic en una fila: le da el foco a su panel y la selecciona.
    pub fn select(&mut self, mid: bool, index: usize) {
        self.set_focus(mid);
        let current = if mid { self.mid.sel } else { self.list().sel };
        self.move_in(mid, index as isize - current as isize);
        self.dirty = true;
    }

    // El texto del filtro cambió desde fuera.
    pub fn refilter(&mut self) {
        self.model_dirty = true;
        self.selection_changed();
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.filtering = false;
        self.refilter();
    }

    pub fn set_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.focus_mid = false;
            self.filter.clear();
            self.filtering = false;
            self.model_dirty = true;
            self.selection_changed();
        }
    }

    fn move_in(&mut self, mid: bool, delta: isize) {
        let (state, rows) = if mid { (&mut self.mid, &self.mid_rows) } else { (&mut self.lists[self.view as usize], &self.rows) };
        let last = item_count(rows).saturating_sub(1);
        let next = state.sel.saturating_add_signed(delta).min(last);
        if next != state.sel {
            state.sel = next;
            if self.view == View::Courses && !mid {
                self.mid = ListState::default();
                self.model_dirty = true;
            }
            self.selection_changed();
        }
    }

    pub fn move_sel(&mut self, delta: isize) {
        self.move_in(self.view == View::Courses && self.focus_mid, delta);
    }

    fn set_focus(&mut self, mid: bool) {
        if self.view == View::Courses && self.focus_mid != mid && (!mid || self.selected_course().is_some()) {
            self.focus_mid = mid;
            self.filter.clear();
            self.model_dirty = true;
            self.selection_changed();
        }
    }

    fn scroll_detail(&mut self, delta: i32) {
        let next = (i32::from(self.detail_scroll) + delta).clamp(0, i32::from(self.hit.detail_max)) as u16;
        if next != self.detail_scroll {
            self.detail_scroll = next;
            self.dirty = true;
        }
    }

    fn page(&self) -> isize {
        (self.hit.list.height as isize / 2).max(1)
    }

    pub fn on_paste(&mut self, pasted: &str) {
        let clean: String = pasted.chars().filter(|c| !c.is_control()).collect();
        if self.login.is_none() && self.delivery.as_ref().is_some_and(|d| !d.busy && !d.confirm) {
            // Arrastrar archivos a la terminal los agrega a la entrega.
            if self.delivery.as_ref().is_some_and(|d| d.field == 0) {
                pasted.lines().filter(|l| !l.trim().is_empty()).for_each(|line| self.add_delivery_file(line));
            } else if let Some(delivery) = &mut self.delivery {
                delivery.text.extend(pasted.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t'));
            }
            self.dirty = true;
            return;
        }
        if let Some(login) = &mut self.login {
            match login.field {
                0 => login.user.push_str(clean.trim()),
                1 => login.pass.push_str(&clean),
                _ => login.token.push_str(clean.trim()),
            }
        } else if self.filtering {
            self.filter.push_str(&clean);
            self.model_dirty = true;
        }
        self.dirty = true;
    }

    fn submit_login(&mut self) {
        let Some(login) = &mut self.login else { return };
        if login.busy {
            return;
        }
        if !login.token.trim().is_empty() {
            match store::clean_token(&login.token) {
                Some(token) if store::expiry_of(&token) != 0 && store::expiry_of(&token) <= text::now() => login.note = "ese token ya venció: copia uno nuevo".into(),
                Some(token) => {
                    self.profiles.ultimo.clear();
                    self.profiles.save();
                    self.adopt_token(&token, true);
                }
                None => login.note = "eso no parece un token de eminus".into(),
            }
            return;
        }
        let user = login.user.trim().to_string();
        if user.is_empty() || login.pass.is_empty() {
            return login.note = "escribe tu usuario y tu contraseña".into();
        }
        // La contraseña no se guarda: sale de aquí cifrada, directo a Eminus.
        let pass = std::mem::take(&mut login.pass);
        login.busy = true;
        login.note = "entrando…".into();
        self.pool.spawn(self.epoch, move |c| {
            let result = c.sign_in(&user, &pass);
            Msg::Login(user, Some(pass), result)
        });
    }

    fn login_key(&mut self, key: KeyEvent) {
        let Some(login) = &mut self.login else { return };
        if login.busy {
            self.quit = key.code == KeyCode::Esc;
            return;
        }
        if login.picker {
            return self.picker_key(key);
        }
        let fields = [0, 1, 2];
        let at = login.field;
        match key.code {
            KeyCode::Esc if !self.profiles.perfiles.is_empty() => {
                login.picker = true;
                login.note.clear();
            }
            KeyCode::Esc => self.quit = true,
            KeyCode::Enter => self.submit_login(),
            KeyCode::Tab | KeyCode::Down => login.field = fields[(at + 1) % fields.len()],
            KeyCode::BackTab | KeyCode::Up => login.field = fields[(at + fields.len() - 1) % fields.len()],
            KeyCode::Backspace | KeyCode::Char(_) if login.busy => {}
            KeyCode::Backspace => {
                [&mut login.user, &mut login.pass, &mut login.token][login.field].pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => [&mut login.user, &mut login.pass, &mut login.token][login.field].clear(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => [&mut login.user, &mut login.pass, &mut login.token][login.field].push(c),
            _ => {}
        }
    }

    fn picker_key(&mut self, key: KeyEvent) {
        let Some(login) = &mut self.login else { return };
        let count = self.profiles.perfiles.len();
        let removing = std::mem::take(&mut login.confirm_remove);
        let mut chosen = None;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => login.pick = (login.pick + 1) % (count + 1),
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => login.pick = (login.pick + count) % (count + 1),
            KeyCode::Char(c @ '1'..='9') if (c as usize - '1' as usize) < count => chosen = Some(c as usize - '1' as usize),
            KeyCode::Enter | KeyCode::Char('l') => chosen = Some(login.pick),
            KeyCode::Char('+') | KeyCode::Char('n') => chosen = Some(count),
            KeyCode::Char('x') | KeyCode::Delete if login.pick < count => {
                let profile = &self.profiles.perfiles[login.pick];
                if removing {
                    store::secret_delete(&profile.user);
                    login.note = format!("perfil quitado: {}", profile.name);
                    self.profiles.perfiles.remove(login.pick);
                    self.profiles.save();
                    login.pick = 0;
                    login.picker = !self.profiles.perfiles.is_empty();
                } else {
                    login.confirm_remove = true;
                    login.note = format!("pulsa x otra vez para quitar el perfil de {}", profile.name);
                }
            }
            _ => {}
        }
        match chosen {
            Some(i) if i < count => {
                login.pick = i;
                let user = self.profiles.perfiles[i].user.clone();
                self.login_profile(user, "entrando…");
            }
            Some(_) => {
                login.picker = false;
                login.field = 0;
                login.note.clear();
            }
            None => {}
        }
    }

    // ¿Guardar como perfil lo que se acaba de escribir en el login?
    fn offer_key(&mut self, key: KeyEvent) {
        let Some((user, pass)) = self.offer.take() else { return };
        match key.code {
            KeyCode::Char('s' | 'S' | 'y') | KeyCode::Enter => {
                let name = if self.user_info.name.is_empty() { user.clone() } else { self.user_info.name.clone() };
                let profile = store::Profile { user, name };
                // Aparte: el llavero puede abrir un diálogo del sistema.
                self.pool.spawn(self.epoch, move |_| Msg::ProfileSaved(profile.clone(), store::secret_set(&profile.user, &pass)));
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => self.say("perfil no guardado", 2),
            _ => self.offer = Some((user, pass)),
        }
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filtering = false;
            }
            KeyCode::Enter | KeyCode::Down | KeyCode::Up => self.filtering = false,
            KeyCode::Backspace => {
                if self.filter.pop().is_none() {
                    self.filtering = false;
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => self.filter.clear(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => self.filter.push(c),
            _ => return,
        }
        self.model_dirty = true;
        self.selection_changed();
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        self.dirty = true;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.login.is_some() {
            return self.login_key(key);
        }
        if self.offer.is_some() {
            return self.offer_key(key);
        }
        if self.delivery.is_some() {
            return self.delivery_key(key);
        }
        if self.help {
            self.help = false;
            return;
        }
        if self.filtering {
            return self.filter_key(key);
        }
        // Al volver a la app tras un rato, los datos se ponen al día solos.
        if !self.loading && self.loaded_at != 0 && text::now() - self.loaded_at > CACHE_MAX_AGE {
            self.refresh(false);
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char(c @ '1'..='5') => self.set_view(View::ALL[c as usize - '1' as usize]),
            KeyCode::Char('d') if ctrl => self.move_sel(self.page()),
            KeyCode::Char('u') if ctrl => self.move_sel(-self.page()),
            KeyCode::Char('j') | KeyCode::Down => self.move_sel(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_sel(-1),
            KeyCode::Char('g') | KeyCode::Home => self.move_sel(isize::MIN),
            KeyCode::Char('G') | KeyCode::End => self.move_sel(isize::MAX),
            KeyCode::Char('J') | KeyCode::PageDown => self.scroll_detail(self.page() as i32),
            KeyCode::Char('K') | KeyCode::PageUp => self.scroll_detail(-(self.page() as i32)),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => self.set_focus(true),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::BackTab => self.set_focus(false),
            KeyCode::Enter | KeyCode::Char('o') => self.activate(false),
            KeyCode::Char('O') => self.activate(true),
            KeyCode::Char('w') => self.open_web(false),
            KeyCode::Char('W') => self.open_web(true),
            KeyCode::Char('V') => {
                if let Some(Entry::Act(act)) = self.target().cloned() {
                    self.retry_detail(&act);
                }
            }
            KeyCode::Char('/') => {
                self.filtering = true;
                self.filter.clear();
                self.model_dirty = true;
            }
            KeyCode::Esc if !self.filter.is_empty() => {
                self.filter.clear();
                self.model_dirty = true;
                self.selection_changed();
            }
            KeyCode::Char('a') => {
                self.show_all = !self.show_all;
                self.model_dirty = true;
                self.selection_changed();
                let note = match (self.view, self.show_all) {
                    (View::Courses, true) => "cursos anteriores a la vista",
                    (_, true) => "mostrando también entregadas y vencidas antiguas",
                    (_, false) => "sólo lo actual",
                };
                self.say(note, 3);
            }
            KeyCode::Char('D') => self.download_selected(),
            KeyCode::Char('B') => self.download_submission(),
            KeyCode::Char('E') => self.open_delivery(),
            KeyCode::Char('r') => {
                if self.loading {
                    self.say("ya se está actualizando…", 2);
                } else {
                    self.refresh(true);
                }
            }
            KeyCode::Char('c') => {
                self.theme = (self.theme + 1) % theme::THEMES.len();
                store::save_theme(theme::THEMES[self.theme].name);
                self.say(format!("tema: {}", theme::THEMES[self.theme].name), 2);
            }
            KeyCode::Char('?') | KeyCode::F(1) => self.help = true,
            KeyCode::Char('X') => self.logout(),
            _ => {}
        }
    }

    // ───────────── ratón ─────────────

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        if self.login.is_some() || self.help || self.delivery.is_some() || self.offer.is_some() {
            return;
        }
        let at = Position::new(mouse.column, mouse.row);
        let over_mid = self.hit.mid.contains(at);
        let over_list = self.hit.list.contains(at);
        match mouse.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let down = mouse.kind == MouseEventKind::ScrollDown;
                if self.hit.detail.contains(at) {
                    self.scroll_detail(if down { 2 } else { -2 });
                } else if over_mid || over_list {
                    self.move_in(over_mid, if down { 1 } else { -1 });
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, _, view)) = self.hit.tabs.iter().find(|(from, to, _)| mouse.row == 0 && (*from..*to).contains(&mouse.column)) {
                    return self.set_view(*view);
                }
                if !over_mid && !over_list {
                    return;
                }
                self.filtering = false;
                self.set_focus(over_mid);
                let (area, state, rows) = if over_mid { (self.hit.mid, self.mid, &self.mid_rows) } else { (self.hit.list, self.list(), &self.rows) };
                let line = state.offset + usize::from(mouse.row - area.y);
                if matches!(rows.get(line), Some(Row::Item { .. })) {
                    let index = rows[..line].iter().filter(|r| matches!(r, Row::Item { .. })).count();
                    self.move_in(over_mid, index as isize - state.sel as isize);
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> App {
        let mut app = App::with_client(Client::new(""));
        app.login = None;
        app.loaded_at = text::now();
        let course = Course { id: "7".into(), name: "Redes".into(), end: 0, favorite: false, current: true };
        let act = Arc::new(Activity { id: "3".into(), course_id: course.id.clone(), course_name: course.name.clone(), title: "Práctica de redes".into(), start: 0, extension: 0, deadline: text::now() + 3600, delivered: false, delivered_at: 0, grade: None, value: Some(10.0) });
        app.acts.insert(course.id.clone(), vec![act]);
        app.courses.push(course);
        app.details.set("7:3".into(), Slot::Ready(Detail { text: "Instrucciones".into(), files: vec![], blocked: None }));
        app.deliveries.set("7:3".into(), Slot::Ready(DeliveryInfo { id: 41, text: "Primera línea\nSegunda línea".into(), ..Default::default() }));
        app.rebuild();
        app
    }

    #[test]
    fn user_info_is_scoped_to_the_account_and_cleared_with_session_data() {
        let mut app = fixture();
        app.account = "zs123".into();
        assert_eq!(app.display_name(), "zs123");
        let info = || UserInfo { name: "PÉREZ ANA".into(), username: "zs123".into(), photo: Some(Arc::new(image::RgbaImage::new(128, 128))) };
        app.on_msg(99, Msg::UserInfo("zs123".into(), Ok(info())));
        assert_eq!(app.display_name(), "zs123");
        app.on_msg(0, Msg::UserInfo("otra-cuenta".into(), Ok(info())));
        assert_eq!(app.display_name(), "zs123");
        app.on_msg(0, Msg::UserInfo("zs123".into(), Ok(info())));
        assert_eq!(app.display_name(), "PÉREZ ANA");
        assert!(app.user_info.photo.is_some());
        app.on_msg(0, Msg::UserInfo("zs123".into(), Err(ApiError::Auth("perfil no disponible".into()))));
        assert!(app.login.is_none());
        assert_eq!(app.display_name(), "PÉREZ ANA");
        app.forget();
        assert_eq!(app.display_name(), "zs123");
        assert!(app.user_info.username.is_empty());
        assert!(app.user_info.photo.is_none());
    }

    #[test]
    fn changing_and_clearing_the_filter_updates_rows_and_highlighting() {
        let mut app = fixture();
        app.filter = "otra".into();
        app.rebuild();
        assert_eq!(item_count(&app.rows), 0);
        assert!(!app.compiled_filter.matches("Práctica de redes"));
        app.filter = "practica redes".into();
        app.rebuild();
        assert_eq!(item_count(&app.rows), 1);
        assert!(app.compiled_filter.matches("Práctica de redes"));
        app.set_view(View::Miyu);
        app.rebuild();
        assert_eq!(item_count(&app.rows), 1);
        app.filter.clear();
        app.rebuild();
        assert!(app.compiled_filter.is_empty());
        assert_eq!(item_count(&app.rows), 1);
    }

    #[test]
    fn native_navigation_keeps_the_course_and_never_requires_the_browser() {
        let mut app = fixture();
        app.course_view("7", View::Courses);
        assert!(app.view == View::Courses && app.focus_mid);
        assert!(matches!(app.target(), Some(Entry::Act(a)) if a.id == "3"));
        app.press(KeyCode::Left);
        assert!(matches!(app.target(), Some(Entry::Course(c)) if c.id == "7"));
        app.press(KeyCode::Enter);
        assert!(matches!(app.target(), Some(Entry::Act(a)) if a.id == "3"));
        assert!(app.focus_mid);
        app.press(KeyCode::Enter);
        assert_eq!(app.pool.pending, 0);
        app.set_view(View::Grades);
        app.set_view(View::Agenda);
        app.rebuild();
        app.press(KeyCode::Char('O'));
        assert!(app.view == View::Courses && app.focus_mid);
    }

    #[test]
    fn submission_is_readable_without_opening_the_editor_and_keeps_multiline_text() {
        let mut app = fixture();
        let info = DeliveryInfo { id: 41, text: "Uno\nDos".into(), sent: true, feedback: "Corrige la dirección IP".into(), grade: Some(8.0), ..Default::default() };
        app.on_msg(0, Msg::Delivery("7:3".into(), Ok(info)));
        assert!(app.delivery.is_none());
        assert!(matches!(app.deliveries.get("7:3"), Some(Slot::Ready(d)) if d.feedback.contains("dirección IP") && d.grade == Some(8.0)));
        app.press(KeyCode::Char('E'));
        assert_eq!(app.delivery.as_ref().unwrap().text, "Uno\nDos");
        app.delivery.as_mut().unwrap().field = 1;
        app.on_paste("\nTres\nCuatro");
        assert_eq!(app.delivery.as_ref().unwrap().text, "Uno\nDos\nTres\nCuatro");
        app.press(KeyCode::Esc);
        assert!(app.delivery.is_none());
        app.on_msg(99, Msg::Delivery("7:3".into(), Ok(DeliveryInfo { feedback: "otra cuenta".into(), ..Default::default() })));
        assert!(matches!(app.deliveries.get("7:3"), Some(Slot::Ready(d)) if !d.feedback.contains("otra cuenta")));
    }

    #[test]
    fn failed_upload_keeps_unsent_files_and_rechecks_the_server() {
        let mut app = fixture();
        app.press(KeyCode::Char('E'));
        let d = app.delivery.as_mut().unwrap();
        d.files = vec![(PathBuf::from("/tmp/subido.pdf"), 10), (PathBuf::from("/tmp/faltante.pdf"), 20)];
        d.uploaded_before.insert("1".into());
        d.busy = true;
        app.on_msg(0, Msg::Delivered("7".into(), "Práctica".into(), Err(ApiError::Other("la red se cortó".into()))));
        assert_eq!(app.delivery.as_ref().unwrap().files.len(), 2);
        assert!(!app.delivery.as_ref().unwrap().busy);
        assert!(app.delivery.as_ref().unwrap().note.contains("no se pudo confirmar"));
        app.on_msg(0, Msg::Delivery("7:3".into(), Ok(DeliveryInfo { id: 41, files: vec![model::AFile { id: "1".into(), name: "anterior.pdf".into(), size: "1 KB".into() }, model::AFile { id: "2".into(), name: "subido.pdf".into(), size: "1 KB".into() }], ..Default::default() })));
        assert_eq!(app.delivery.as_ref().unwrap().files.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(), vec![PathBuf::from("/tmp/faltante.pdf")]);
        assert!(!app.delivery.as_ref().unwrap().recovering);
    }

    #[test]
    fn graded_activity_cannot_open_the_submission_editor() {
        let mut app = fixture();
        app.details.set("7:3".into(), Slot::Ready(Detail { text: String::new(), files: vec![], blocked: Some("ya está calificada".into()) }));
        app.press(KeyCode::Char('E'));
        assert!(app.delivery.is_none());
        assert!(app.message().contains("calificada"));
    }

    #[test]
    fn terminal_shows_submission_feedback_and_review_attachments() {
        let mut app = fixture();
        app.on_msg(0, Msg::Delivery("7:3".into(), Ok(DeliveryInfo { id: 41, text: "Informe del estudiante".into(), feedback: "Corrige la dirección IP".into(), grade: Some(8.0), review_files: vec![model::AFile { id: "4".into(), name: "correcciones.pdf".into(), size: "10 KB".into() }], ..Default::default() })));
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(150, 42)).unwrap();
        terminal.draw(|frame| crate::ui::draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        for expected in ["mi entrega", "Informe del estudiante", "retroalimentación", "Corrige la dirección IP", "correcciones.pdf"] {
            assert!(text.contains(expected), "falta {expected}");
        }
    }

    #[test]
    fn modal_retry_keeps_comment_and_failed_download_reports_the_error() {
        let mut app = fixture();
        app.press(KeyCode::Char('E'));
        app.on_msg(0, Msg::Delivery("7:3".into(), Err(ApiError::Other("sin conexión".into()))));
        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert!(matches!(app.delivery.as_ref().unwrap().info, Slot::Pending));
        assert_eq!(app.delivery.as_ref().unwrap().text, "Primera línea\nSegunda línea");
        app.press(KeyCode::Esc);
        app.details.set("7:3".into(), Slot::Failed("sin conexión".into()));
        app.press(KeyCode::Char('D'));
        assert!(app.message().contains("sin conexión") && app.message().contains("reintentar"));
    }
}
