// Datos de Eminus ya normalizados y los modelos de cada vista (agrupar,
// filtrar, ordenar). Todo aquí es puro: recibe la hora, no la consulta.

use crate::text::{self, Filter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

pub const OVERDUE_WINDOW: i64 = 21 * 24 * 3600;

#[derive(Default)]
pub struct UserInfo {
    pub name: String,
    pub username: String,
    pub photo: Option<Arc<image::RgbaImage>>,
}

pub fn user_info_from(row: &Value) -> UserInfo {
    let name = ["paterno", "materno", "nombre"].iter()
        .filter_map(|key| row.get(key).and_then(Value::as_str))
        .flat_map(str::split_whitespace).collect::<Vec<_>>().join(" ").to_uppercase();
    let username = row.get("idUsuario").and_then(Value::as_str).unwrap_or_default().trim();
    let username = if row.get("perteneceUV").and_then(Value::as_i64) == Some(1) {
        username.split('@').next().unwrap_or_default()
    } else { username };
    UserInfo { name, username: username.to_string(), photo: None }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: String,
    pub name: Arc<str>,
    pub end: i64,
    pub favorite: bool,
    #[serde(skip)]
    pub current: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub course_id: String,
    pub course_name: Arc<str>,
    pub title: Arc<str>,
    pub start: i64,
    pub extension: i64,
    pub deadline: i64,
    pub delivered: bool,
    pub delivered_at: i64,
    pub grade: Option<f64>,
    pub value: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AFile {
    pub id: String,
    pub name: String,
    pub size: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Detail {
    pub text: String,
    pub files: Vec<AFile>,
    // Por qué Eminus no aceptaría una entrega desde aquí, si es el caso.
    pub blocked: Option<String>,
}

// La entrega del estudiante tal como está en Eminus (borrador o enviada),
// con lo que el docente haya respondido.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliveryInfo {
    pub id: i64,
    pub text: String,
    pub files: Vec<AFile>,
    pub sent: bool,
    // Retroalimentación del docente, ya sin HTML.
    pub feedback: String,
    pub grade: Option<f64>,
    // Cuándo la revisó el docente; 0 si no la ha revisado.
    pub reviewed_at: i64,
    // Archivos que el docente adjuntó a su revisión.
    pub review_files: Vec<AFile>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CFile {
    pub id: String,
    pub element_id: String,
    pub unit_id: String,
    pub name: String,
    pub size: String,
}

#[derive(Serialize, Deserialize)]
pub struct ContentItem {
    pub id: String,
    pub unit: bool,
    pub course_id: String,
    pub course_name: Arc<str>,
    pub unit_id: String,
    pub unit_name: String,
    pub element_id: String,
    pub title: Arc<str>,
    pub text: String,
    pub files: Vec<CFile>,
    pub has_files: bool,
    pub published: i64,
}

pub struct ContentDetail {
    pub text: String,
    pub files: Vec<CFile>,
}

pub type Acts = HashMap<String, Vec<Arc<Activity>>>;
pub type Content = HashMap<String, Vec<Arc<ContentItem>>>;

// ───────────── JSON de Eminus ─────────────

// Los nombres de campo cambian de mayúsculas entre endpoints.
pub fn pick<'a>(source: &'a Value, fields: &[&str]) -> Option<&'a Value> {
    fields.iter().filter_map(|f| source.get(f)).find(|v| match v {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        _ => true,
    })
}

pub fn string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

pub fn int(value: Option<&Value>) -> i64 {
    num(value).map_or(0, |n| n as i64)
}

pub fn num(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', ".").parse().ok().filter(|n: &f64| n.is_finite()),
        _ => None,
    }
}

pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

pub fn id_of(value: Option<&Value>, allow_zero: bool) -> String {
    // Sin pasar por un String intermedio: el id ya viene como texto o número.
    let valid = |n: u64| allow_zero || n > 0;
    match value {
        Some(Value::String(s)) => {
            let raw = s.trim();
            match raw.parse::<u64>() {
                Ok(n) if raw.bytes().all(|b| b.is_ascii_digit()) && valid(n) => n.to_string(),
                _ => String::new(),
            }
        }
        Some(Value::Number(n)) => n.as_u64().filter(|n| valid(*n)).map(|n| n.to_string()).unwrap_or_default(),
        _ => String::new(),
    }
}

// Los ids de Eminus son números; así los espera su API.
pub fn id_json(id: &str) -> Value {
    id.parse::<u64>().map_or_else(|_| Value::from(id), Value::from)
}

pub fn courses_from(rows: &[Value], now: i64) -> Vec<Course> {
    let mut courses: Vec<Course> = Vec::with_capacity(rows.len());
    for r in rows {
        let c = r.get("curso").filter(|c| c.is_object()).unwrap_or(r);
        if !truthy(c.get("idCurso")) || truthy(c.get("archivado")) {
            continue;
        }
        let end = int(c.get("fechaTerminoEpoch"));
        courses.push(Course {
            id: string(c.get("idCurso")),
            name: string(c.get("nombre")).into(),
            end,
            favorite: truthy(c.get("esFavorito")),
            current: end == 0 || end >= now,
        });
    }
    sort_courses(&mut courses);
    courses
}

pub fn sort_courses(courses: &mut [Course]) {
    courses.sort_by(|a, b| {
        b.current.cmp(&a.current).then(b.favorite.cmp(&a.favorite)).then(b.end.cmp(&a.end)).then_with(|| a.name.cmp(&b.name))
    });
}

// Mismo criterio que Miyu --pendientes: hay entrega si fechaEntrega existe y
// no es la propia fecha límite.
fn is_delivered(raw: &Value) -> bool {
    let text_of = |field: &str| match raw.get(field) {
        Some(Value::String(s)) => Cow::Borrowed(s.trim()),
        other => Cow::Owned(string(other)),
    };
    let entrega = text_of("fechaEntrega");
    !entrega.is_empty() && !entrega.starts_with("0001") && entrega != text_of("fechaTermino")
}

pub fn activities_from(rows: &[Value], course: &Course) -> Vec<Arc<Activity>> {
    let mut out: Vec<Arc<Activity>> = Vec::with_capacity(rows.len());
    for r in rows {
        if !truthy(r.get("idActividad")) || r.get("visible").and_then(Value::as_f64) == Some(0.0) {
            continue;
        }
        let end = int(r.get("fechaTerminoEpoch"));
        let extension = int(r.get("fechaProrrogaEpoch"));
        let title = string(r.get("titulo"));
        out.push(Arc::new(Activity {
            id: string(r.get("idActividad")),
            course_id: course.id.clone(),
            course_name: course.name.clone(),
            title: if title.is_empty() { "sin título".into() } else { title.into() },
            start: int(r.get("fechaInicioEpoch")),
            extension,
            deadline: if extension != 0 { extension } else { end },
            delivered: is_delivered(r),
            delivered_at: int(r.get("fechaEntregaEpoch")),
            grade: num(r.get("calificacion")),
            value: num(r.get("valor")),
        }));
    }
    out
}

pub fn file_size(bytes: f64) -> String {
    if bytes <= 0.0 || !bytes.is_finite() {
        String::new()
    } else if bytes > 1_048_576.0 {
        format!("{:.1} MB", bytes / 1_048_576.0)
    } else {
        format!("{} KB", (bytes / 1024.0).round().max(1.0))
    }
}

pub fn detail_from(row: &Value) -> Detail {
    let files = row
        .get("archivosActsConTamano")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|f| a_file(f, &["idArchivosActividades", "idArchivoActividad", "idArchivo"]))
                .collect()
        })
        .unwrap_or_default();
    // Mismas condiciones que el botón «Entregar» de Eminus.
    let state = |field: &str| row.get(field).filter(|v| !v.is_null()).map(|v| int(Some(v)));
    let blocked = if truthy(row.get("porEquipos")) {
        Some("es una actividad por equipos: entrégala desde eminus")
    } else if state("estado") == Some(3) {
        Some("ya está calificada: eminus no acepta otra entrega")
    } else if state("estadoAct").is_some_and(|s| s != 2 && s != 4) {
        Some("eminus no acepta entregas de esta actividad ahora (fuera de fecha)")
    } else {
        None
    };
    Detail { text: text::html_to_text(&string(row.get("descripcion"))), files, blocked: blocked.map(String::from) }
}

// Un detalle de actividad trae al menos uno de estos campos; lo demás (un
// aviso de error, una lista vacía) no lo es.
pub fn is_activity_row(row: &Value) -> bool {
    ["idActividad", "descripcion", "titulo", "archivosActsConTamano"].iter().any(|f| row.get(f).is_some())
}

// Lo mismo para la entrega del estudiante.
pub fn is_delivery_row(row: &Value) -> bool {
    ["idActividadEntrega", "idEstado", "descEntrega", "fechaEntrega", "archivosEntregaConTamano", "descRevision", "calificacion"].iter().any(|f| row.get(f).is_some())
}

fn a_file(f: &Value, id_fields: &[&str]) -> Option<AFile> {
    let name = string(pick(f, &["nombreOriginal", "nombre", "nombreArchivo", "name", "archivo"]));
    let size = num(pick(f, &["tamano", "tamanio", "size", "peso"])).unwrap_or(0.0);
    (!name.is_empty()).then(|| AFile { id: id_of(pick(f, id_fields), false), name, size: file_size(size) })
}

pub fn delivery_from(row: &Value) -> DeliveryInfo {
    let files_of = |field: &str, ids: &[&str]| -> Vec<AFile> { row.get(field).and_then(Value::as_array).map(|files| files.iter().filter_map(|f| a_file(f, ids)).collect()).unwrap_or_default() };
    let delivered = string(row.get("fechaEntrega"));
    let feedback = text::html_to_text(&string(row.get("descRevision")));
    let revised = string(row.get("fechaRevision"));
    let reviewed_at = if revised.starts_with("0001") { 0 } else { text::parse_eminus_date(&revised).max(0) };
    // Sin revisión Eminus manda 0 (o nada) en vez de dejarla sin calificar.
    let reviewed = reviewed_at != 0 || !feedback.is_empty() || int(row.get("idActividadRevision")) > 0;
    DeliveryInfo {
        id: int(row.get("idActividadEntrega")),
        text: text::html_to_text(&string(row.get("descEntrega"))),
        files: files_of("archivosEntregaConTamano", &["idArchivosEntregas", "idArchivoEntrega", "idArchivo"]),
        sent: int(row.get("idEstado")) == 1 || (!delivered.is_empty() && !delivered.starts_with("0001")),
        feedback,
        grade: num(row.get("calificacion")).filter(|g| reviewed || *g != 0.0),
        reviewed_at,
        review_files: files_of("archivosRevisionConTamano", &["idArchivoActividadRevision", "idArchivosActividadesRevision", "idArchivo"]),
    }
}

// ── contenido de los cursos ──

const PUBLISH_FIELDS: [&str; 4] = ["fechaPublicacionInicio", "FechaPublicacionInicio", "fechaPublicacion", "FechaPublicacion"];
const PUBLISHED_AT: [&str; 8] = ["fechaPublicacionInicio", "FechaPublicacionInicio", "fechaPublicacion", "FechaPublicacion", "fechaRegistro", "FechaRegistro", "fechaAlta", "FechaAlta"];
const FILE_ID_FIELDS: [&str; 10] = ["idArchivosElementos", "IdArchivosElementos", "idarchivoselementos", "idArchivoElemento", "IdArchivoElemento", "idarchivoelemento", "idArchivo", "IdArchivo", "idarchivo", "id"];
const FILE_NAME_FIELDS: [&str; 9] = ["nombreOriginal", "NombreOriginal", "nombreoriginal", "nombreArchivo", "NombreArchivo", "nombrearchivo", "nombre", "Nombre", "name"];
const ELEMENT_ID_FIELDS: [&str; 6] = ["idElemento", "IdElemento", "idelemento", "idContenido", "IdContenido", "idcontenido"];
pub const UNIT_ID_FIELDS: [&str; 3] = ["idUnidad", "IdUnidad", "idunidad"];
const DESC_FIELDS: [&str; 18] = ["descripcion", "Descripcion", "description", "contenido", "Contenido", "mensaje", "Mensaje", "texto", "Texto", "cuerpo", "Cuerpo", "detalle", "Detalle", "instrucciones", "Instrucciones", "observaciones", "Observaciones", "body"];

pub fn is_published(entry: &Value, now: i64) -> bool {
    if !entry.is_object() {
        return false;
    }
    let visible = ["visible", "Visible", "estado", "estatus"].iter().filter_map(|f| entry.get(f)).find(|v| !v.is_null());
    let flag = string(visible).to_lowercase();
    if ["0", "false", "borrador", "draft", "oculto", "eliminado", "eliminada"].contains(&flag.as_str()) || num(visible) == Some(3.0) {
        return false;
    }
    text::parse_eminus_date(&string(pick(entry, &PUBLISH_FIELDS))) <= now
}

pub fn content_file(row: &Value, unit_id: &str) -> Option<CFile> {
    let id = id_of(pick(row, &FILE_ID_FIELDS), false);
    if id.is_empty() {
        return None;
    }
    let raw = pick(row, &["tamano", "Tamano", "size", "Size", "peso", "Peso"]);
    let size = match raw {
        Some(Value::String(s)) if s.chars().any(|c| c.is_ascii_alphabetic()) => s.trim().to_string(),
        Some(Value::String(s)) => file_size(s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect::<String>().parse().unwrap_or(0.0)),
        other => file_size(num(other).unwrap_or(0.0)),
    };
    let own_unit = id_of(pick(row, &UNIT_ID_FIELDS), true);
    let name = string(pick(row, &FILE_NAME_FIELDS));
    Some(CFile {
        id,
        element_id: id_of(pick(row, &ELEMENT_ID_FIELDS), false),
        unit_id: if own_unit.is_empty() { unit_id.to_string() } else { own_unit },
        name: if name.is_empty() { "archivo".into() } else { name },
        size,
    })
}

pub fn content_files_of(entry: &Value) -> Vec<CFile> {
    pick(entry, &["archivos", "Archivos", "adjuntos", "Adjuntos", "archivosElementos", "ArchivosElementos"])
        .and_then(Value::as_array)
        .map(|files| files.iter().filter_map(|f| content_file(f, "")).collect())
        .unwrap_or_default()
}

pub fn desc_of(entry: &Value) -> String {
    pick(entry, &DESC_FIELDS).and_then(Value::as_str).map(text::html_to_text).unwrap_or_default()
}

pub fn unit_from(unit: &Value, course: &Course) -> Option<ContentItem> {
    let unit_id = id_of(pick(unit, &["idUnidad", "IdUnidad", "idContenido", "IdContenido", "id"]), true);
    let unit_name = string(pick(unit, &["nombre", "Nombre", "unidad", "Unidad", "titulo", "Titulo"]));
    if unit_id.is_empty() || unit_name.is_empty() {
        return None;
    }
    Some(ContentItem {
        id: format!("{}:u:{unit_id}", course.id),
        unit: true,
        course_id: course.id.clone(),
        course_name: course.name.clone(),
        unit_id,
        title: unit_name.clone().into(),
        unit_name,
        element_id: String::new(),
        text: desc_of(unit),
        files: Vec::new(),
        has_files: false,
        published: text::parse_eminus_date(&string(pick(unit, &PUBLISHED_AT))),
    })
}

pub fn element_from(el: &Value, unit: &ContentItem) -> Option<ContentItem> {
    let element_id = id_of(pick(el, &["idElemento", "IdElemento", "idContenido", "IdContenido", "id"]), false);
    let title = string(pick(el, &["titulo", "Titulo", "nombre", "Nombre"]));
    if element_id.is_empty() || title.is_empty() {
        return None;
    }
    let files = content_files_of(el);
    let flagged = string(pick(el, &["tieneAdjuntos", "TieneAdjuntos"])).to_lowercase();
    Some(ContentItem {
        id: format!("{}:e:{element_id}", unit.course_id),
        unit: false,
        course_id: unit.course_id.clone(),
        course_name: unit.course_name.clone(),
        unit_id: unit.unit_id.clone(),
        unit_name: unit.unit_name.clone(),
        element_id,
        title: title.into(),
        text: desc_of(el),
        has_files: !files.is_empty() || !["", "0", "false", "no"].contains(&flagged.as_str()),
        files,
        published: text::parse_eminus_date(&string(pick(el, &PUBLISHED_AT))),
    })
}

pub fn content_label(unit: bool, files: usize, has_files: bool) -> String {
    if unit {
        "módulo".into()
    } else if files > 1 {
        format!("{files} archivos")
    } else if has_files || files == 1 {
        "archivo".into()
    } else {
        "mensaje".into()
    }
}

// ───────────── vistas ─────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Done,
    Future,
    Overdue,
    Imminent,
    Urgent,
    Normal,
    Plain,
}

impl Tone {
    pub fn glyph(self) -> &'static str {
        match self {
            Tone::Done => "✓",
            Tone::Future => "○",
            Tone::Overdue => "!",
            Tone::Imminent => "◉",
            _ => "●",
        }
    }
}

pub fn urgency(act: &Activity, now: i64) -> Tone {
    if act.delivered {
        Tone::Done
    } else if act.start != 0 && act.start > now {
        Tone::Future
    } else if act.deadline == 0 {
        Tone::Normal
    } else {
        match act.deadline - now {
            d if d < 0 => Tone::Overdue,
            d if d < 86400 => Tone::Imminent,
            d if d < 2 * 86400 => Tone::Urgent,
            _ => Tone::Normal,
        }
    }
}

pub fn grade_tone(grade: Option<f64>) -> Tone {
    match grade {
        None => Tone::Future,
        Some(g) if g >= 9.0 => Tone::Done,
        Some(g) if g >= 7.0 => Tone::Normal,
        Some(g) if g >= 6.0 => Tone::Urgent,
        Some(_) => Tone::Overdue,
    }
}

pub fn short_course(name: &str) -> String {
    // Quita el «(12345)» final.
    let trimmed = name.trim_end();
    let base = match trimmed.rfind('(') {
        Some(open) if trimmed.ends_with(')') && open + 2 < trimmed.len() && trimmed[open + 1..trimmed.len() - 1].bytes().all(|b| b.is_ascii_digit()) => trimmed[..open].trim_end(),
        _ => trimmed,
    };
    base.to_lowercase()
}

pub fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{n:.0}") } else { format!("{n}") }
}

#[derive(Clone)]
pub enum Entry {
    Act(Arc<Activity>),
    Course(Course),
    Content(Arc<ContentItem>),
}

pub enum Row {
    Group { label: String, count: String, tone: Tone },
    Item { glyph: &'static str, tone: Tone, due: String, course: String, title: Arc<str>, meta: String, entry: Entry },
}

pub struct Clock {
    pub now: i64,
    pub today: i64,
}

fn group(label: impl Into<String>, count: impl ToString) -> Row {
    Row::Group { label: label.into(), count: count.to_string(), tone: Tone::Plain }
}

fn act_row(act: &Arc<Activity>, clock: &Clock, with_course: bool) -> Row {
    let tone = urgency(act, clock.now);
    Row::Item {
        glyph: tone.glyph(),
        tone,
        due: if act.delivered { "entregada".into() } else { text::rel_due(act.deadline, clock.now, clock.today) },
        course: if with_course { short_course(&act.course_name) } else { String::new() },
        title: act.title.clone(),
        meta: String::new(),
        entry: Entry::Act(act.clone()),
    }
}

pub fn current_acts<'a>(courses: &'a [Course], acts: &'a Acts) -> impl Iterator<Item = &'a Arc<Activity>> {
    courses.iter().filter(|c| c.current).filter_map(|c| acts.get(&c.id)).flatten()
}

fn by_deadline(a: &Arc<Activity>, b: &Arc<Activity>) -> std::cmp::Ordering {
    let key = |x: &Arc<Activity>| if x.deadline == 0 { i64::MAX } else { x.deadline };
    key(a).cmp(&key(b))
}

fn act_matches(filter: &Filter, act: &Activity) -> bool {
    filter.is_empty() || filter.matches_fields(&[&act.title, &act.course_name])
}

pub fn agenda(courses: &[Course], acts: &Acts, filter: &Filter, show_all: bool, clock: &Clock) -> Vec<Row> {
    let mut rows = Vec::new();
    agenda_into(&mut rows, courses, acts, filter, show_all, clock);
    rows
}

// Rellena `rows` reutilizando su capacidad: la app reconstruye las vistas
// seguido y no tiene que soltar y reservar la lista en cada reconstrucción.
pub fn agenda_into(rows: &mut Vec<Row>, courses: &[Course], acts: &Acts, filter: &Filter, show_all: bool, clock: &Clock) {
    let Clock { now, today } = *clock;
    let labels = ["vencidas", "hoy", "mañana", "próximos 7 días", "después", "sin fecha", "entregadas"];
    let mut groups: [Vec<&Arc<Activity>>; 7] = Default::default();
    for act in current_acts(courses, acts) {
        if !act_matches(filter, act) {
            continue;
        }
        let dl = act.deadline;
        let slot = if act.delivered {
            if !show_all {
                continue;
            }
            6
        } else if dl == 0 {
            5
        } else if dl < now {
            if !show_all && now - dl >= OVERDUE_WINDOW {
                continue;
            }
            0
        } else if dl < today + 86400 {
            1
        } else if dl < today + 2 * 86400 {
            2
        } else if dl < today + 8 * 86400 {
            3
        } else {
            4
        };
        groups[slot].push(act);
    }
    groups[0].sort_by_key(|a| std::cmp::Reverse(a.deadline));
    groups[1..6].iter_mut().for_each(|g| g.sort_by(|a, b| by_deadline(a, b)));
    groups[6].sort_by_key(|a| std::cmp::Reverse(a.delivered_at));
    rows.clear();
    for (i, items) in groups.iter().enumerate().filter(|(_, g)| !g.is_empty()) {
        rows.push(Row::Group { label: labels[i].into(), count: items.len().to_string(), tone: if i == 0 { Tone::Overdue } else { Tone::Plain } });
        rows.extend(items.iter().map(|a| act_row(a, clock, true)));
    }
}

// Agrupa por materia conservando el orden de aparición.
fn by_course<'a>(acts: impl Iterator<Item = &'a Arc<Activity>>) -> Vec<(Arc<str>, Vec<&'a Arc<Activity>>)> {
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut groups: Vec<(Arc<str>, Vec<&Arc<Activity>>)> = Vec::new();
    for act in acts {
        let at = *index.entry(&act.course_name).or_insert_with(|| {
            groups.push((act.course_name.clone(), Vec::new()));
            groups.len() - 1
        });
        groups[at].1.push(act);
    }
    groups
}

// Pendientes agrupados por materia, como el panel de Miyu.
pub fn miyu(courses: &[Course], acts: &Acts, filter: &Filter, clock: &Clock) -> Vec<Row> {
    let mut rows = Vec::new();
    miyu_into(&mut rows, courses, acts, filter, clock);
    rows
}

pub fn miyu_into(rows: &mut Vec<Row>, courses: &[Course], acts: &Acts, filter: &Filter, clock: &Clock) {
    let now = clock.now;
    let pending = current_acts(courses, acts)
        .filter(|a| !a.delivered && !(a.deadline != 0 && a.deadline < now && now - a.deadline > OVERDUE_WINDOW) && act_matches(filter, a));
    let mut groups = by_course(pending);
    groups.iter_mut().for_each(|(_, items)| items.sort_by(|a, b| by_deadline(a, b)));
    groups.sort_by(|a, b| by_deadline(a.1[0], b.1[0]));
    rows.clear();
    for (name, items) in groups {
        rows.push(group(short_course(&name), items.len()));
        for act in items {
            let tone = urgency(act, now);
            rows.push(Row::Item {
                glyph: tone.glyph(),
                tone,
                due: match tone {
                    Tone::Overdue => "vencida".into(),
                    Tone::Future => "por iniciar".into(),
                    _ => text::countdown(act.deadline, now, false),
                },
                course: String::new(),
                title: act.title.clone(),
                meta: String::new(),
                entry: Entry::Act(act.clone()),
            });
        }
    }
}

// Calificaciones por materia con promedio; devuelve también el general.
pub fn grades(courses: &[Course], acts: &Acts, filter: &Filter) -> (Vec<Row>, Option<f64>) {
    let mut rows = Vec::new();
    let average = grades_into(&mut rows, courses, acts, filter);
    (rows, average)
}

pub fn grades_into(rows: &mut Vec<Row>, courses: &[Course], acts: &Acts, filter: &Filter) -> Option<f64> {
    let listed = current_acts(courses, acts).filter(|a| (a.grade.is_some() || a.delivered) && act_matches(filter, a));
    let mut groups = by_course(listed);
    groups.sort_by_cached_key(|(name, _)| short_course(name));
    let average = |grades: &[f64]| (!grades.is_empty()).then(|| grades.iter().sum::<f64>() / grades.len() as f64);
    let mut all = Vec::new();
    rows.clear();
    for (name, mut items) in groups {
        let graded: Vec<f64> = items.iter().filter_map(|a| a.grade).collect();
        let avg = average(&graded);
        all.extend_from_slice(&graded);
        items.sort_by(|a, b| b.grade.is_some().cmp(&a.grade.is_some()).then(b.delivered_at.cmp(&a.delivered_at)));
        rows.push(Row::Group {
            label: format!("{} · promedio {}", short_course(&name), avg.map_or("—".into(), |a| format!("{a:.1}"))),
            count: format!("{}/{} calificadas", graded.len(), items.len()),
            tone: grade_tone(avg),
        });
        for act in items {
            rows.push(Row::Item {
                glyph: if act.grade.is_some() { "✓" } else { "◌" },
                tone: grade_tone(act.grade),
                due: match act.grade {
                    None => "en revisión".into(),
                    Some(g) => match act.value.filter(|v| *v != 0.0) {
                        Some(v) => format!("{} / {}", fmt_num(g), fmt_num(v)),
                        None => fmt_num(g),
                    },
                },
                course: String::new(),
                title: act.title.clone(),
                meta: String::new(),
                entry: Entry::Act(act.clone()),
            });
        }
    }
    average(&all)
}

// Contenido por materia: cada módulo seguido de sus elementos.
pub fn content(courses: &[Course], content: &Content, filter: &Filter) -> Vec<Row> {
    let mut rows = Vec::new();
    content_into(&mut rows, courses, content, filter);
    rows
}

pub fn content_into(rows: &mut Vec<Row>, courses: &[Course], content: &Content, filter: &Filter) {
    rows.clear();
    for course in courses.iter().filter(|c| c.current) {
        let Some(all) = content.get(&course.id) else { continue };
        let items: Vec<_> = all.iter().filter(|it| filter.is_empty() || filter.matches_fields(&[&it.title, &it.unit_name, &course.name])).collect();
        if items.is_empty() {
            continue;
        }
        rows.push(group(short_course(&course.name), items.iter().filter(|it| !it.unit).count()));
        for it in items {
            rows.push(Row::Item {
                glyph: if it.unit { "▸" } else if it.has_files { "⬇" } else { "·" },
                tone: if it.unit { Tone::Urgent } else { Tone::Plain },
                due: content_label(it.unit, it.files.len(), it.has_files),
                course: String::new(),
                title: it.title.clone(),
                meta: String::new(),
                entry: Entry::Content(it.clone()),
            });
        }
    }
}

pub fn progress_bar(done: usize, total: usize, width: usize) -> (String, String, String) {
    if total == 0 {
        return (String::new(), "░".repeat(width), " 0/0".into());
    }
    let filled = (done as f64 / total as f64 * width as f64).round() as usize;
    ("█".repeat(filled), "░".repeat(width - filled), format!(" {done}/{total}"))
}

pub fn course_rows(courses: &[Course], acts: &Acts, loading: impl Fn(&str) -> bool, filter: &Filter, show_all: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    course_rows_into(&mut rows, courses, acts, loading, filter, show_all);
    rows
}

pub fn course_rows_into(rows: &mut Vec<Row>, courses: &[Course], acts: &Acts, loading: impl Fn(&str) -> bool, filter: &Filter, show_all: bool) {
    let visible: Vec<&Course> = courses.iter().filter(|c| (show_all || c.current) && filter.matches(&c.name)).collect();
    rows.clear();
    let mut last = None;
    for c in &visible {
        if last != Some(c.current) {
            last = Some(c.current);
            rows.push(group(if c.current { "actuales" } else { "anteriores" }, visible.iter().filter(|x| x.current == c.current).count()));
        }
        let meta = match acts.get(&c.id) {
            Some(list) => {
                let (full, empty, label) = progress_bar(list.iter().filter(|a| a.delivered).count(), list.len(), 8);
                format!("{full}{empty}{label}")
            }
            None if loading(&c.id) => "…".into(),
            None if !c.current && c.end != 0 => text::year_of(c.end).to_string(),
            None => String::new(),
        };
        rows.push(Row::Item {
            glyph: if c.favorite { "★" } else { " " },
            tone: Tone::Plain,
            due: String::new(),
            course: String::new(),
            title: c.name.to_lowercase().into(),
            meta,
            entry: Entry::Course((*c).clone()),
        });
    }
}

pub fn course_acts(list: &[Arc<Activity>], filter: &Filter, clock: &Clock) -> Vec<Row> {
    let mut rows = Vec::new();
    course_acts_into(&mut rows, list, filter, clock);
    rows
}

pub fn course_acts_into(rows: &mut Vec<Row>, list: &[Arc<Activity>], filter: &Filter, clock: &Clock) {
    let mut sorted: Vec<&Arc<Activity>> = list.iter().filter(|a| filter.matches(&a.title)).collect();
    sorted.sort_by_key(|a| std::cmp::Reverse(a.start));
    rows.clear();
    rows.extend(sorted.into_iter().map(|a| act_row(a, clock, false)));
}

#[derive(Default)]
pub struct Counts {
    pub overdue: usize,
    pub today: usize,
    pub soon: usize,
    pub week: usize,
}

pub fn counts(courses: &[Course], acts: &Acts, clock: &Clock) -> Counts {
    let Clock { now, today } = *clock;
    let mut c = Counts::default();
    for act in current_acts(courses, acts).filter(|a| !a.delivered && a.deadline != 0) {
        if act.deadline < now {
            c.overdue += usize::from(now - act.deadline < OVERDUE_WINDOW);
            continue;
        }
        c.today += usize::from(act.deadline < today + 86400);
        c.soon += usize::from(act.deadline - now < 2 * 86400);
        c.week += usize::from(act.deadline < today + 8 * 86400);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn user_info_matches_eminus_name_order_and_identity() {
        let info = user_info_from(&json!({ "paterno": " Pérez ", "materno": null, "nombre": " Ana  María ", "idUsuario": " zs123@estudiantes.uv.mx ", "perteneceUV": 1 }));
        assert_eq!(info.name, "PÉREZ ANA MARÍA");
        assert_eq!(info.username, "zs123");
        assert!(info.photo.is_none());
        let external = user_info_from(&json!({ "nombre": "Ana", "idUsuario": "ana@example.org", "perteneceUV": 0 }));
        assert_eq!(external.username, "ana@example.org");
        assert!(user_info_from(&Value::Null).name.is_empty());
    }

    fn fixture(now: i64) -> (Vec<Course>, Acts) {
        let courses = courses_from(
            &[
                json!({ "curso": { "idCurso": 7, "nombre": "REDES (123)", "fechaTerminoEpoch": now + 9_000_000 } }),
                json!({ "curso": { "idCurso": 8, "nombre": "Viejo", "fechaTerminoEpoch": now - 10 } }),
                json!({ "curso": { "idCurso": 9, "nombre": "Archivado", "archivado": true } }),
            ],
            now,
        );
        let rows = [
            json!({ "idActividad": 1, "titulo": " Vencida ", "fechaTerminoEpoch": now - 3600, "fechaTermino": "a" }),
            json!({ "idActividad": 2, "titulo": "Pronto", "fechaTerminoEpoch": now + 3600, "fechaProrrogaEpoch": now + 7200 }),
            json!({ "idActividad": 3, "titulo": "Hecha", "fechaTerminoEpoch": now + 50, "fechaTermino": "a", "fechaEntrega": "b", "calificacion": "9,5", "valor": 10 }),
            json!({ "idActividad": 4, "titulo": "Sin entregar", "fechaTermino": "a", "fechaEntrega": "a", "fechaTerminoEpoch": now + 30 * 86400 }),
            json!({ "idActividad": 5, "titulo": "Oculta", "visible": 0 }),
        ];
        let mut acts = Acts::new();
        acts.insert("7".into(), activities_from(&rows, &courses[0]));
        (courses, acts)
    }

    fn titles(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Group { label, count, .. } => format!("# {label} {count}"),
                Row::Item { title, .. } => title.to_string(),
            })
            .collect()
    }

    #[test]
    fn delivery_dates_keep_the_previous_normalization() {
        for delivered in [Value::Null, json!(""), json!(" 0001-01-01 "), json!(" fecha "), json!(123), json!(true)] {
            for deadline in [Value::Null, json!(""), json!("fecha"), json!(123), json!("123")] {
                let row = json!({"fechaEntrega": delivered, "fechaTermino": deadline});
                let expected = string(row.get("fechaEntrega"));
                assert_eq!(is_delivered(&row), !expected.is_empty() && !expected.starts_with("0001") && expected != string(row.get("fechaTermino")));
            }
        }
    }

    #[test]
    fn rebuilt_rows_share_titles_and_reuse_the_row_buffer() {
        let clock = Clock { now: text::start_of_today() + 3 * 3600, today: text::start_of_today() };
        let (courses, acts) = fixture(clock.now);
        let filter = Filter::new("pronto");
        let mut rows = agenda(&courses, &acts, &filter, false, &clock);
        let buffer = rows.as_ptr();
        agenda_into(&mut rows, &courses, &acts, &filter, false, &clock);
        assert_eq!(rows.as_ptr(), buffer);
        let Some(Row::Item { title, entry: Entry::Act(act), .. }) = rows.get(1) else { panic!("falta la actividad") };
        assert!(Arc::ptr_eq(title, &act.title));
        assert_eq!(title.as_ref(), "Pronto");
    }

    #[test]
    fn parses_and_groups() {
        let now = text::start_of_today() + 3 * 3600;
        let clock = Clock { now, today: text::start_of_today() };
        let (courses, acts) = fixture(now);
        assert_eq!(courses.len(), 2);
        assert!(courses[0].current && !courses[1].current);
        let list = &acts["7"];
        assert_eq!(list.len(), 4);
        assert_eq!(list[1].deadline, now + 7200);
        assert!(list[2].delivered && !list[3].delivered);
        assert_eq!(list[2].grade, Some(9.5));
        assert_eq!(urgency(&list[0], now), Tone::Overdue);
        assert_eq!(urgency(&list[1], now), Tone::Imminent);

        let all = Filter::default();
        assert_eq!(titles(&agenda(&courses, &acts, &all, false, &clock)), ["# vencidas 1", "Vencida", "# hoy 1", "Pronto", "# después 1", "Sin entregar"]);
        assert_eq!(titles(&agenda(&courses, &acts, &Filter::new("redes pron"), true, &clock)), ["# hoy 1", "Pronto"]);
        assert_eq!(titles(&miyu(&courses, &acts, &all, &clock)), ["# redes 3", "Vencida", "Pronto", "Sin entregar"]);
        let (rows, avg) = grades(&courses, &acts, &all);
        assert_eq!(titles(&rows), ["# redes · promedio 9.5 1/1 calificadas", "Hecha"]);
        assert_eq!(avg, Some(9.5));
        let c = counts(&courses, &acts, &clock);
        assert_eq!((c.overdue, c.today, c.soon, c.week), (1, 1, 1, 1));
        assert_eq!(titles(&course_rows(&courses, &acts, |_| false, &all, true)), ["# actuales 1", "redes (123)", "# anteriores 1", "viejo"]);
    }

    #[test]
    fn content_parsing() {
        let now = 2_000_000_000;
        let course = Course { id: "7".into(), name: "Redes".into(), end: 0, favorite: false, current: true };
        assert!(is_published(&json!({ "nombre": "x" }), now));
        assert!(!is_published(&json!({ "visible": "0" }), now));
        assert!(!is_published(&json!({ "estado": 3 }), now));
        assert!(!is_published(&json!({ "FechaPublicacion": "2099-01-01T00:00:00" }), now));
        let unit = unit_from(&json!({ "IdUnidad": 0, "Nombre": "Intro", "Descripcion": "<p>hola</p>" }), &course).unwrap();
        assert_eq!((unit.id.as_str(), unit.text.as_str()), ("7:u:0", "hola"));
        let el = element_from(&json!({ "idElemento": "12", "titulo": "Lectura", "archivos": [{ "idArchivo": 5, "nombre": "a.pdf", "tamano": 2048 }] }), &unit).unwrap();
        assert_eq!((el.files[0].name.as_str(), el.files[0].size.as_str(), el.has_files), ("a.pdf", "2 KB", true));
        assert_eq!(content_label(false, 1, true), "archivo");
        assert_eq!(short_course("Redes de Datos (98765) "), "redes de datos");
        assert_eq!(short_course("Cálculo (a)"), "cálculo (a)");
    }

    #[test]
    fn delivery_parsing() {
        // Los nombres de campo son los que lee la página de Eminus.
        let reviewed = delivery_from(&json!({
            "idActividadEntrega": 41, "idEstado": 2, "descEntrega": "<p>Mi tarea</p>", "fechaEntrega": "2026-03-01T10:00:00",
            "archivosEntregaConTamano": [{ "idArchivosEntregas": 9, "nombreOriginal": "tarea.pdf", "tamano": 2048 }, { "tamano": 1 }],
            "idActividadRevision": 5, "descRevision": "<p>Bien &amp; claro</p>", "calificacion": "8,5", "fechaRevision": "2026-03-02T12:30:00",
            "archivosRevisionConTamano": [{ "idArchivoActividadRevision": 3, "nombreOriginal": "notas.docx", "tamano": 4096 }],
        }));
        assert_eq!((reviewed.id, reviewed.text.as_str(), reviewed.sent), (41, "Mi tarea", true));
        assert_eq!(reviewed.files, [AFile { id: "9".into(), name: "tarea.pdf".into(), size: "2 KB".into() }]);
        assert_eq!((reviewed.feedback.as_str(), reviewed.grade), ("Bien & claro", Some(8.5)));
        assert!(reviewed.reviewed_at > 0);
        assert_eq!((reviewed.review_files[0].id.as_str(), reviewed.review_files[0].name.as_str()), ("3", "notas.docx"));

        // Enviada y sin revisar: el 0 de relleno no es una calificación.
        let waiting = delivery_from(&json!({ "idActividadEntrega": 42, "idEstado": 1, "calificacion": 0, "descRevision": null, "fechaRevision": null }));
        assert!(waiting.sent);
        assert_eq!((waiting.grade, waiting.reviewed_at, waiting.feedback.as_str()), (None, 0, ""));
        // Un cero puesto por el docente sí cuenta.
        assert_eq!(delivery_from(&json!({ "calificacion": 0, "idActividadRevision": 7 })).grade, Some(0.0));
        assert_eq!(delivery_from(&json!({ "calificacion": 0, "fechaRevision": "0001-01-01T00:00:00" })).grade, None);

        let draft = delivery_from(&json!({ "idActividadEntrega": 43, "idEstado": 0, "fechaEntrega": null }));
        assert!(!draft.sent && draft.id == 43);
        assert_eq!(delivery_from(&Value::Null), DeliveryInfo::default());
    }

    #[test]
    fn detail_parsing_and_cache() {
        let detail = detail_from(&json!({
            "idActividad": 3, "descripcion": "<p>Lee</p>", "estado": 3,
            "archivosActsConTamano": [{ "idArchivosActividades": 4, "nombreOriginal": "guia.pdf", "tamano": 3_145_728 }],
        }));
        assert_eq!((detail.text.as_str(), detail.files[0].size.as_str()), ("Lee", "3.0 MB"));
        assert!(detail.blocked.as_deref().is_some_and(|b| b.contains("calificada")));
        assert_eq!(detail_from(&json!({ "descripcion": "x", "estadoAct": 2 })).blocked, None);
        assert!(is_activity_row(&json!({ "idActividad": 3 })) && !is_activity_row(&json!({ "mensaje": "no" })) && !is_activity_row(&Value::Null));

        // Lo que se guarda en disco se lee igual, y un caché viejo sin los
        // campos nuevos sigue sirviendo.
        let back: Detail = serde_json::from_str(&serde_json::to_string(&detail).unwrap()).unwrap();
        assert_eq!(back, detail);
        let old: DeliveryInfo = serde_json::from_value(json!({ "id": 1, "text": "t", "files": [], "sent": true })).unwrap();
        assert_eq!((old.id, old.sent, old.grade, old.reviewed_at), (1, true, None, 0));
    }
}
