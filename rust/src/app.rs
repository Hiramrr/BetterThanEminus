// Estado de la app y lo que pasa con cada tecla y cada respuesta de la red.

use crate::api::{ApiError, Client, Msg, Pool};
use crate::model::{self, Acts, CFile, Clock, Content, ContentDetail, ContentItem, Counts, Course, Detail, Entry, Row};
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
        if self.map.insert(key.clone(), slot).is_none() {
            self.order.push_back(key);
            while self.order.len() > MAX_DETAILS {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
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

impl App {
    pub fn new() -> Self {
        let token = store::load_token();
        let prefs = store::load_prefs();
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
            expiry: 0,
            epoch: 0,
            msg: String::new(),
            msg_until: None,
            theme: theme::index_of(&prefs.tema),
            help: false,
            login: None,
            profiles: store::load_profiles(),
            offer: None,
            auto_tried: false,
            pool: Pool::new(Client::new("")),
            hit: Hit::default(),
            dirty: true,
            quit: false,
        };
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
        self.epoch += 1;
        self.loading = false;
        self.content_loading = false;
        self.acts_loading.clear();
        self.refresh_ids.clear();
        self.msg.clear();
        self.msg_until = None;
        self.error = false;
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
        self.model_dirty = true;
        self.dirty = true;
        if text::now() - self.loaded_at > CACHE_MAX_AGE {
            self.refresh(false);
        } else {
            self.ensure_content(false);
        }
    }

    fn forget(&mut self) {
        self.courses.clear();
        self.acts.clear();
        self.content.clear();
        self.details.clear();
        self.content_details.clear();
        self.content_files.clear();
        self.loaded_at = 0;
        self.content_loaded_at = 0;
        self.content_tried = false;
        self.lists = [ListState::default(); 5];
        self.mid = ListState::default();
    }

    fn logout(&mut self) {
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

    fn refresh(&mut self, manual: bool) {
        if self.loading || self.login.is_some() {
            return;
        }
        self.loading = true;
        self.error = false;
        self.refresh_manual = manual;
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
            Msg::ProfileSaved(profile, Ok(())) => {
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
        let filter = Filter::new(&self.filter);
        let none = Filter::default();
        self.mid_rows.clear();
        self.average = None;
        self.rows = match self.view {
            View::Agenda => model::agenda(&self.courses, &self.acts, &filter, self.show_all, &self.clock),
            View::Miyu => model::miyu(&self.courses, &self.acts, &filter, &self.clock),
            View::Grades => {
                let (rows, average) = model::grades(&self.courses, &self.acts, &filter);
                self.average = average;
                rows
            }
            View::Content => model::content(&self.courses, &self.content, &filter),
            View::Courses => model::course_rows(&self.courses, &self.acts, |id| self.acts_loading.contains(id), if self.focus_mid { &none } else { &filter }, self.show_all),
        };
        let state = &mut self.lists[self.view as usize];
        state.sel = state.sel.min(item_count(&self.rows).saturating_sub(1));
        if self.view == View::Courses {
            if let Some(Entry::Course(course)) = entry_at(&self.rows, state.sel).cloned() {
                match self.acts.get(&course.id) {
                    Some(list) => self.mid_rows = model::course_acts(list, if self.focus_mid { &filter } else { &none }, &self.clock),
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
                Entry::Act(act) => self.fetch_act_detail(&act.course_id, &act.id),
                Entry::Content(item) if i == 0 => self.fetch_content_detail(item),
                _ => {}
            }
        }
    }

    // ───────────── reloj ─────────────

    // Se llama en cada vuelta del ciclo, antes de pintar.
    pub fn tick(&mut self) {
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

    // ───────────── abrir en Eminus ─────────────

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

    fn activate(&mut self, course_only: bool) {
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

    // D: descarga a ~/Downloads los archivos del elemento seleccionado.
    fn download_selected(&mut self) {
        let Some(Entry::Content(item)) = self.target().cloned() else {
            return self.say("D descarga archivos en la vista de contenido (5)", 4);
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

    fn set_view(&mut self, view: View) {
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

    fn move_sel(&mut self, delta: isize) {
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
                let name = Some(store::name_of(&self.pool.client.token())).filter(|n| !n.is_empty()).unwrap_or_else(|| user.clone());
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
        if self.login.is_some() || self.help {
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
