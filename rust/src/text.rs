// Texto y fechas: normalización para filtrar, HTML → texto plano y los
// formatos de fecha de la vista tui.

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const DAYS: [&str; 7] = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];
pub const MONTHS: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn local(epoch: i64) -> DateTime<Local> {
    Local.timestamp_opt(epoch, 0).single().unwrap_or_else(Local::now)
}

fn local_epoch(naive: NaiveDateTime) -> i64 {
    Local.from_local_datetime(&naive).earliest().map(|d| d.timestamp()).unwrap_or(0)
}

pub fn start_of_today() -> i64 {
    let n = Local::now();
    NaiveDate::from_ymd_opt(n.year(), n.month(), n.day())
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(local_epoch)
        .unwrap_or_else(|| now() - now() % 86400)
}

fn hhmm(d: &DateTime<Local>) -> String {
    format!("{:02}:{:02}", d.hour(), d.minute())
}

fn day_name(d: &DateTime<Local>) -> &'static str {
    DAYS[d.weekday().num_days_from_sunday() as usize]
}

pub fn clock() -> String {
    let d = Local::now();
    format!("{} {:02} {}", day_name(&d), d.day(), hhmm(&d))
}

pub fn year_of(epoch: i64) -> i32 {
    local(epoch).year()
}

// "hoy", "mañana" o "vie 16" para el día que empieza en `epoch`.
pub fn day_label(epoch: i64, offset: usize) -> String {
    match offset {
        0 => "hoy".into(),
        1 => "mañana".into(),
        _ => {
            let d = local(epoch);
            format!("{} {}", day_name(&d), d.day())
        }
    }
}

pub fn full_date(epoch: i64) -> String {
    if epoch == 0 {
        return "—".into();
    }
    let d = local(epoch);
    format!("{} {} {} {} · {}", day_name(&d), d.day(), MONTHS[d.month0() as usize], d.year(), hhmm(&d))
}

// "hoy 23:45", "mañana 09:00", "vie 23:45", "12 oct", "hace 2d"
pub fn rel_due(epoch: i64, now: i64, today: i64) -> String {
    if epoch == 0 {
        return "sin fecha".into();
    }
    let diff = epoch - now;
    if diff < 0 {
        let ago = -diff;
        return if ago < 3600 {
            format!("hace {}m", ((ago + 30) / 60).max(1))
        } else if ago < 86400 {
            format!("hace {}h", (ago + 1800) / 3600)
        } else {
            format!("hace {}d", (ago + 43200) / 86400)
        };
    }
    let d = local(epoch);
    if epoch < today + 86400 {
        if diff < 3600 {
            format!("en {}m", ((diff + 30) / 60).max(1))
        } else {
            format!("hoy {}", hhmm(&d))
        }
    } else if epoch < today + 2 * 86400 {
        format!("mañana {}", hhmm(&d))
    } else if epoch < today + 7 * 86400 {
        format!("{} {}", day_name(&d), hhmm(&d))
    } else if d.year() != local(now).year() {
        format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())
    } else {
        format!("{} {}", d.day(), MONTHS[d.month0() as usize])
    }
}

// Cuenta regresiva estilo Miyu: "1d 04h 22m", "3h 05m 12s", "vencida hace 2h".
pub fn countdown(epoch: i64, now: i64, seconds: bool) -> String {
    if epoch == 0 {
        return "sin fecha".into();
    }
    let past = epoch < now;
    let diff = (epoch - now).abs();
    let (d, h, m, s) = (diff / 86400, diff % 86400 / 3600, diff % 3600 / 60, diff % 60);
    let text = if d > 0 {
        format!("{d}d {h:02}h {m:02}m")
    } else if h > 0 {
        if seconds { format!("{h}h {m:02}m {s:02}s") } else { format!("{h}h {m:02}m") }
    } else if seconds {
        format!("{m}m {s:02}s")
    } else {
        format!("{m}m")
    };
    if past { format!("vencida hace {text}") } else { text }
}

// "12/oct/2026 - 23:59" o ISO → epoch en segundos (0 si no se entiende).
pub fn parse_eminus_date(text: &str) -> i64 {
    let lower = text.trim().to_lowercase();
    if lower.is_empty() {
        return 0;
    }
    let slashed = || -> Option<i64> {
        let (date, time) = lower.split_once('-')?;
        let mut parts = date.trim().split('/');
        let day: u32 = parts.next()?.trim().parse().ok()?;
        let name = parts.next()?.trim();
        let month = MONTHS.iter().position(|m| *m == name)? as u32 + 1;
        let year: i32 = parts.next()?.trim().parse().ok()?;
        let (h, m) = time.trim().split_once(':')?;
        let m: String = m.chars().take_while(|c| c.is_ascii_digit()).collect();
        let naive = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(h.trim().parse().ok()?, m.parse().ok()?, 0)?;
        Some(local_epoch(naive))
    };
    if let Some(epoch) = slashed() {
        return epoch;
    }
    let raw = text.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(raw) {
        return d.timestamp();
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, format) {
            return local_epoch(naive);
        }
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(local_epoch)
        .unwrap_or(0)
}

// ───────────── filtro ─────────────

// Minúscula sin acento, carácter por carácter: los índices del texto
// normalizado coinciden con los del original y así se puede resaltar.
pub fn fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        _ => c.to_lowercase().next().unwrap_or(c),
    }
}

pub fn norm(text: &str) -> String {
    text.chars().map(fold).collect()
}

#[derive(Default)]
pub struct Filter {
    words: Vec<Vec<char>>,
}

// ¿Está `word` (ya plegada) dentro de `text`, comparando carácter a carácter
// con fold? No reserva memoria: los textos que se filtran son títulos y
// nombres cortos, así que la búsqueda simple basta.
fn contains_folded(text: &str, word: &[char]) -> bool {
    if word.is_empty() {
        return true;
    }
    text.char_indices().any(|(at, _)| {
        let mut rest = text[at..].chars();
        word.iter().all(|&b| rest.next().is_some_and(|a| fold(a) == b))
    })
}

impl Filter {
    pub fn new(text: &str) -> Self {
        Filter { words: norm(text).split_whitespace().map(|w| w.chars().collect()).collect() }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn matches(&self, text: &str) -> bool {
        self.matches_fields(&[text])
    }

    // Como buscar en los campos unidos por un espacio, sin construir ese
    // texto: ninguna palabra del filtro trae espacios, así que ninguna
    // coincidencia puede cruzar de un campo a otro.
    pub fn matches_fields(&self, fields: &[&str]) -> bool {
        self.words.iter().all(|w| fields.iter().any(|f| contains_folded(f, w)))
    }

    // Qué caracteres de `text` coinciden con alguna palabra del filtro.
    pub fn marks(&self, text: &str) -> Vec<bool> {
        let hay: Vec<char> = text.chars().map(fold).collect();
        let mut marks = vec![false; hay.len()];
        for w in &self.words {
            if w.is_empty() || w.len() > hay.len() {
                continue;
            }
            for i in 0..=hay.len() - w.len() {
                if hay[i..i + w.len()] == w[..] {
                    marks[i..i + w.len()].iter_mut().for_each(|m| *m = true);
                }
            }
        }
        marks
    }
}

// ───────────── ancho en celdas ─────────────

pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

// Recorta a `max` celdas, con «…» si no cabe.
pub fn clip(text: &str, max: usize) -> String {
    if width(text) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w + 1 > max {
            break;
        }
        out.push(c);
        used += w;
    }
    if max > 0 {
        out.push('…');
    }
    out
}

// Recorta y rellena con espacios hasta `size` celdas exactas.
pub fn fit(text: &str, size: usize) -> String {
    let mut out = clip(text, size);
    let pad = size.saturating_sub(width(&out));
    out.extend(std::iter::repeat_n(' ', pad));
    out
}

// ───────────── HTML → texto ─────────────

const MAX_TEXT: usize = 24_000;

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "nbsp" => ' ',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "aacute" => 'á',
        "eacute" => 'é',
        "iacute" => 'í',
        "oacute" => 'ó',
        "uacute" => 'ú',
        "Aacute" => 'Á',
        "Eacute" => 'É',
        "Iacute" => 'Í',
        "Oacute" => 'Ó',
        "Uacute" => 'Ú',
        "ntilde" => 'ñ',
        "Ntilde" => 'Ñ',
        "uuml" => 'ü',
        "Uuml" => 'Ü',
        "iexcl" => '¡',
        "iquest" => '¿',
        "ordm" => 'º',
        "ordf" => 'ª',
        "deg" => '°',
        "laquo" => '«',
        "raquo" => '»',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "ldquo" => '“',
        "rdquo" => '”',
        "lsquo" => '‘',
        "rsquo" => '’',
        "bull" => '•',
        "middot" => '·',
        "copy" => '©',
        _ => return None,
    })
}

fn push_text(out: &mut String, chunk: &str) {
    let mut rest = chunk;
    while let Some(c) = rest.chars().next() {
        let mut len = c.len_utf8();
        let mut ch = c;
        if c == '&' {
            if let Some(end) = rest[1..].find(';').filter(|e| *e <= 10) {
                if let Some(decoded) = entity(&rest[1..=end]) {
                    ch = decoded;
                    len = end + 2;
                }
            }
        }
        rest = &rest[len..];
        if ch.is_whitespace() || ch == '\u{a0}' {
            if !out.is_empty() && !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
}

fn break_line(out: &mut String, blank: bool) {
    while out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        return;
    }
    let wanted = if blank { 2 } else { 1 };
    let have = out.len() - out.trim_end_matches('\n').len();
    for _ in have..wanted {
        out.push('\n');
    }
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))? + name.len() + 1;
    let rest = &tag[at..];
    let value = match rest.chars().next()? {
        q @ ('"' | '\'') => rest[1..].split(q).next()?,
        _ => rest.split_whitespace().next()?,
    };
    Some(value.to_string())
}

// Las descripciones de Eminus vienen en HTML (a veces con imágenes
// incrustadas de varios MB): aquí queda sólo el texto, con saltos de línea
// y viñetas, que es lo único que se guarda en memoria.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len().min(2048));
    let mut skip: Option<String> = None;
    let mut href: Option<String> = None;
    let mut rest = html;
    while !rest.is_empty() && out.len() < MAX_TEXT {
        let Some(lt) = rest.find('<') else {
            if skip.is_none() {
                push_text(&mut out, rest);
            }
            break;
        };
        if skip.is_none() {
            push_text(&mut out, &rest[..lt]);
        }
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if let Some(until) = &skip {
            if closing && name == *until {
                skip = None;
            }
            continue;
        }
        match name.as_str() {
            "script" | "style" | "svg" | "math" | "noscript" | "template" | "iframe" | "object" | "select" | "textarea" if !closing && !tag.ends_with('/') => skip = Some(name),
            "br" => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push('\n');
            }
            "img" => push_text(&mut out, "[imagen]"),
            "hr" => {
                break_line(&mut out, false);
                out.push_str("────────\n");
            }
            "li" if !closing => {
                break_line(&mut out, false);
                out.push_str("• ");
            }
            "td" | "th" if closing => out.push_str("  "),
            "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "pre" | "ul" | "ol" | "table" => break_line(&mut out, true),
            "div" | "tr" | "li" => break_line(&mut out, false),
            "a" if !closing => href = attr(tag, "href").filter(|h| h.starts_with("http://") || h.starts_with("https://")),
            "a" => {
                if let Some(link) = href.take() {
                    if !out.contains(&link) {
                        out.push_str(" <");
                        out.push_str(&link);
                        out.push('>');
                    }
                }
            }
            _ => {}
        }
    }
    let trimmed = out.trim();
    if trimmed.len() == out.len() { out } else { trimmed.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html() {
        assert_eq!(html_to_text("<p>Hola&nbsp;<b>mundo</b></p><p>Adi&oacute;s</p>"), "Hola mundo\n\nAdiós");
        assert_eq!(html_to_text("<ul><li>uno</li><li>dos</li></ul>fin"), "• uno\n• dos\n\nfin");
        assert_eq!(html_to_text("a<script>alert(1)</script>b<!-- x -->c<img src='data:...'>"), "abc[imagen]");
        assert_eq!(html_to_text("ver <a href=\"https://uv.mx/x\">aquí</a>"), "ver aquí <https://uv.mx/x>");
        assert_eq!(html_to_text("l1<br>l2 &lt;tag&gt; &#241; &bogus;"), "l1\nl2 <tag> ñ &bogus;");
    }

    #[test]
    fn filter() {
        let f = Filter::new("PROGRA  tarea");
        assert!(f.matches("Tarea 1 de Programación"));
        assert!(!f.matches("Tarea de redes"));
        assert_eq!(Filter::new("ción").marks("Acción"), [false, false, true, true, true, true]);
        assert!(Filter::new("").matches("x"));
        // Buscar por campos separados equivale a buscar en el texto unido.
        let f = Filter::new("redes tarea");
        assert!(f.matches_fields(&["Tarea 1", "Redes de datos"]));
        assert!(!f.matches_fields(&["Tarea 1", "Cálculo"]));
        assert!(!Filter::new("destarea").matches_fields(&["Redes", "Tarea"]));
        for query in ["á", "acción", "ñ 中文", "tarea", "acción acción", "acciónlarga", ""] {
            let f = Filter::new(query);
            for fields in [["Acción", "niño 中文"], ["Tarea", "de redes"], ["ac", "ción"], ["", ""]] {
                assert_eq!(f.matches_fields(&fields), f.matches(&fields.join(" ")), "{query:?}, {fields:?}");
            }
        }
        assert!(!Filter::new("tareas").matches("tarea"));
        assert_eq!(Filter::new("aa").marks("aaaa"), [true; 4]);
        assert_eq!(Filter::new("ñ 中文").marks("niño 中文"), [true, false, true, false, false, true, true]);
    }

    #[test]
    fn dates() {
        assert_eq!(countdown(100 + 86400 + 4 * 3600 + 22 * 60, 100, false), "1d 04h 22m");
        assert_eq!(countdown(100, 100 + 7200, false), "vencida hace 2h 00m");
        assert_eq!(countdown(100 + 65, 100, true), "1m 05s");
        assert_eq!(rel_due(0, 5, 0), "sin fecha");
        assert_eq!(rel_due(1000, 1000 + 2 * 86400, 0), "hace 2d");
        let epoch = parse_eminus_date("12/oct/2026 - 23:59");
        assert!(full_date(epoch).starts_with("lun 12 oct 2026 · 23:59"), "{}", full_date(epoch));
        assert_eq!(parse_eminus_date("2026-10-12T23:59:00"), epoch);
        assert_eq!(parse_eminus_date("nada"), 0);
    }

    #[test]
    fn cells() {
        assert_eq!(clip("programación", 6), "progr…");
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(width(&fit("programación", 6)), 6);
    }
}
