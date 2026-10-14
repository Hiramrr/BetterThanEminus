// Tipo de letra de la ventana: las fuentes instaladas en el sistema, con sus
// nombres reales, y la que elige la persona en «personalizar».
//
// La fuente elegida no se copia: se guarda la ruta del archivo original (y el
// número de cara si es una colección .ttc), así que el archivo tiene que
// seguir en su sitio. Si desaparece o está dañado se avisa y el texto sigue
// con la fuente predeterminada.
//
// Buscar las fuentes y leer la elegida corre en hilos aparte. Al buscar no se
// lee ningún archivo entero: sólo el índice de tablas y las tablas con el
// nombre y el estilo (unos KiB por cara). Al cargar se leen las tablas de esa
// cara y nada más, de modo que una colección de 60 MiB no entra en memoria.
//
// La familia `USER_FONT` queda registrada en egui desde el primer `update`,
// con la fuente elegida delante y las de egui detrás (acentos, símbolos y
// emoji que la elegida no traiga). Las familias `Monospace` y `Proportional`
// no se tocan. Mientras la fuente carga, o si falla, `USER_FONT` son sólo las
// de egui: el estilo puede apuntar a ella siempre.
//
// Registra la familia antes del primer cuadro. egui aplica set_fonts al
// empezar el cuadro siguiente y falla si el estilo pide una familia ausente.

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Nombre de la familia de egui que lleva la fuente elegida.
pub const USER_FONT: &str = "fuente-personalizada";

/// Archivos de fuente que se miran al buscar, entre todas las carpetas.
pub const MAX_FILES: usize = 4000;
/// Niveles de subcarpetas que se recorren.
pub const MAX_DEPTH: usize = 4;
/// Caras que se aceptan en una colección .ttc.
pub const MAX_FACES: u32 = 64;
/// Tablas que se aceptan en una cara.
pub const MAX_TABLES: usize = 128;
/// Tope de la tabla de nombres que se lee al buscar.
pub const MAX_NAME_BYTES: u32 = 1 << 20;
/// Tope de una cara cargada (lo que queda en memoria).
pub const MAX_FACE_BYTES: u64 = 32 << 20;

const MAX_NAME_CHARS: usize = 80;
const EXTENSIONS: [&str; 3] = ["ttf", "otf", "ttc"];

/// La fuente elegida, tal como se guarda en las preferencias.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontChoice {
    pub path: PathBuf,
    // Cara dentro de una colección .ttc; 0 en un .ttf o .otf.
    pub index: u32,
    pub name: String,
    pub monospace: bool,
}

// Una familia instalada, representada por su estilo más normal.
#[derive(Clone, Debug, PartialEq)]
struct Family {
    choice: FontChoice,
    key: String, // el nombre en minúsculas, para filtrar
}

struct Table {
    tag: [u8; 4],
    checksum: u32,
    offset: u32,
    len: u32,
}

// Lo que se sabe de una cara sin leer sus glifos.
#[derive(Debug)]
struct FaceInfo {
    index: u32,
    family: String,
    monospace: bool,
    // Cuánto se aleja del estilo normal: cursiva, ancho y peso.
    oddness: (bool, u16, u16),
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_at(file: &mut File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = vec![0; len];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn is_sfnt(tag: &[u8]) -> bool {
    matches!(tag, [0, 1, 0, 0] | b"OTTO" | b"true")
}

const DAMAGED: &str = "el archivo está dañado o no es una fuente TrueType u OpenType";

// Dónde empieza cada cara: una sola en .ttf/.otf, varias en una colección.
fn face_offsets(file: &mut File) -> Result<Vec<u32>, String> {
    let head = read_at(file, 0, 12).map_err(|_| DAMAGED)?;
    if is_sfnt(&head[..4]) {
        return Ok(vec![0]);
    }
    if &head[..4] != b"ttcf" {
        return Err(DAMAGED.into());
    }
    let count = be32(&head, 8).unwrap_or(0);
    if count == 0 || count > MAX_FACES {
        return Err(format!("la colección declara {count} fuentes (se aceptan de 1 a {MAX_FACES})"));
    }
    let raw = read_at(file, 12, count as usize * 4).map_err(|_| DAMAGED)?;
    Ok((0..count as usize).filter_map(|face| be32(&raw, face * 4)).collect())
}

// El índice de tablas de una cara, con cada tabla comprobada contra el
// tamaño del archivo. Devuelve también la versión sfnt, para rehacer la cara.
fn tables(file: &mut File, offset: u32, file_len: u64) -> Result<([u8; 4], Vec<Table>), String> {
    let head = read_at(file, offset as u64, 12).map_err(|_| DAMAGED)?;
    let version = [head[0], head[1], head[2], head[3]];
    let count = be16(&head, 4).unwrap_or(0) as usize;
    if !is_sfnt(&version) || count == 0 || count > MAX_TABLES {
        return Err(DAMAGED.into());
    }
    let raw = read_at(file, offset as u64 + 12, count * 16).map_err(|_| DAMAGED)?;
    let mut list = Vec::with_capacity(count);
    for record in (0..count).map(|i| &raw[i * 16..][..16]) {
        let table = Table {
            tag: [record[0], record[1], record[2], record[3]],
            checksum: be32(record, 4).unwrap_or(0),
            offset: be32(record, 8).unwrap_or(0),
            len: be32(record, 12).unwrap_or(0),
        };
        if table.offset as u64 + table.len as u64 > file_len {
            return Err(DAMAGED.into());
        }
        list.push(table);
    }
    Ok((version, list))
}

fn find<'a>(tables: &'a [Table], tag: &[u8; 4]) -> Option<&'a Table> {
    tables.iter().find(|t| &t.tag == tag)
}

// ab_glyph sólo dibuja contornos: una fuente de mapas de bits (el emoji en
// color, por ejemplo) dejaría el texto en blanco.
fn has_outlines(tables: &[Table]) -> bool {
    [b"glyf", b"CFF ", b"CFF2"].iter().any(|tag| find(tables, tag).is_some())
}

// Los primeros `want` bytes de una tabla (o menos, si es más corta).
fn table_head(file: &mut File, tables: &[Table], tag: &[u8; 4], want: u32) -> Option<Vec<u8>> {
    let table = find(tables, tag)?;
    read_at(file, table.offset as u64, table.len.min(want) as usize).ok()
}

fn clean_name(raw: &str) -> Option<String> {
    let name: String = raw.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

// El nombre con ese id, de preferencia en inglés (así se llama igual que en
// el resto del sistema) y si no, en lo que haya.
fn name_of(names: ttf_parser::name::Names, id: u16) -> Option<String> {
    use ttf_parser::PlatformId;
    let mut best: Option<(u8, String)> = None;
    for name in names {
        if name.name_id != id {
            continue;
        }
        let found = if name.is_unicode() {
            let english = name.platform_id == PlatformId::Unicode || name.language_id == 0x0409;
            name.to_string().map(|text| (if english { 0 } else { 2 }, text))
        } else if name.platform_id == PlatformId::Macintosh && name.encoding_id == 0 && name.name.is_ascii() {
            // Mac Roman coincide con ASCII en los nombres que no llevan acentos.
            let rank = if name.language_id == 0 { 1 } else { 3 };
            Some((rank, String::from_utf8_lossy(name.name).into_owned()))
        } else {
            None
        };
        let Some((rank, text)) = found else { continue };
        let Some(text) = clean_name(&text) else { continue };
        if best.as_ref().is_none_or(|(current, _)| rank < *current) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, text)| text)
}

fn describe(file: &mut File, index: u32, tables: &[Table]) -> Option<FaceInfo> {
    use ttf_parser::name_id;
    // Lo que no se podría cargar no se ofrece.
    if !has_outlines(tables) || tables.iter().map(|t| t.len as u64).sum::<u64>() > MAX_FACE_BYTES {
        return None;
    }
    let name = find(tables, b"name").filter(|t| t.len <= MAX_NAME_BYTES)?;
    let raw = read_at(file, name.offset as u64, name.len as usize).ok()?;
    let names = ttf_parser::name::Table::parse(&raw)?.names;
    let family = name_of(names, name_id::TYPOGRAPHIC_FAMILY).or_else(|| name_of(names, name_id::FAMILY))?;
    let cmap = table_head(file, tables, b"cmap", MAX_NAME_BYTES)?;
    let cmap = ttf_parser::cmap::Table::parse(&cmap)?;
    if !cmap.subtables.into_iter().any(|t| t.is_unicode() && t.glyph_index('a' as u32).is_some()) {
        return None;
    }

    // OS/2: peso, ancho y fsSelection (bit 0, cursiva). Sin OS/2, macStyle.
    let os2 = table_head(file, tables, b"OS/2", 64).unwrap_or_default();
    let oddness = match (be16(&os2, 4), be16(&os2, 6), be16(&os2, 62)) {
        (Some(weight), Some(width), Some(selection)) => (selection & 1 != 0, width.abs_diff(5), weight.abs_diff(400)),
        _ => {
            let head = table_head(file, tables, b"head", 46).unwrap_or_default();
            let style = be16(&head, 44).unwrap_or(0);
            (style & 2 != 0, 0, if style & 1 != 0 { 300 } else { 0 })
        }
    };
    // Monoespaciada según `post` (isFixedPitch) o PANOSE (proporción 9):
    // hay fuentes que sólo marcan una de las dos.
    let post = table_head(file, tables, b"post", 16).unwrap_or_default();
    let monospace = be32(&post, 12).is_some_and(|fixed| fixed != 0) || os2.get(35) == Some(&9);
    Some(FaceInfo { index, family, monospace, oddness })
}

// Las caras que se pueden usar de un archivo. Lee poco: índices y nombres.
fn faces_in(path: &Path) -> Result<Vec<FaceInfo>, String> {
    let mut file = File::open(path).map_err(|e| open_error(path, &e))?;
    let file_len = file.metadata().map_err(|e| open_error(path, &e))?.len();
    let mut faces = Vec::new();
    for (index, offset) in face_offsets(&mut file)?.into_iter().enumerate() {
        let Ok((_, tables)) = tables(&mut file, offset, file_len) else { continue };
        faces.extend(describe(&mut file, index as u32, &tables));
    }
    Ok(faces)
}

fn open_error(path: &Path, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("ya no existe el archivo {}", path.display())
    } else {
        format!("no se pudo leer {} ({e})", path.display())
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default())
}

// Carpetas de fuentes del sistema. Las de la persona van primero: si una
// familia está repetida gana la suya.
fn font_dirs() -> Vec<PathBuf> {
    let home = home();
    if cfg!(target_os = "macos") {
        vec![
            home.join("Library/Fonts"),
            "/Library/Fonts".into(),
            "/System/Library/Fonts".into(),
            // Las que macOS descarga a petición (Catálogo Tipográfico).
            "/System/Library/AssetsV2/com_apple_MobileAsset_Font7".into(),
        ]
    } else if cfg!(target_os = "windows") {
        let env = |name: &str| PathBuf::from(std::env::var_os(name).unwrap_or_default());
        vec![env("LOCALAPPDATA").join("Microsoft/Windows/Fonts"), env("WINDIR").join("Fonts")]
    } else {
        vec![
            home.join(".local/share/fonts"),
            home.join(".fonts"),
            "/usr/local/share/fonts".into(),
            "/usr/share/fonts".into(),
        ]
    }
}

// Los archivos de fuente de esas carpetas y sus subcarpetas, en orden y con
// tope. Los enlaces a carpetas no se siguen.
fn font_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending: Vec<(PathBuf, usize)> = dirs.iter().rev().map(|dir| (dir.clone(), 0)).collect();
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        let mut subdirs = Vec::new();
        for entry in entries {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                if depth < MAX_DEPTH {
                    subdirs.push((path, depth + 1));
                }
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x))) {
                if files.len() == MAX_FILES {
                    return files;
                }
                files.push(path);
            }
        }
        pending.extend(subdirs.into_iter().rev());
    }
    files
}

// Una entrada por familia, con su cara más normal (ni cursiva ni negrita, si
// la hay). Las familias que empiezan con punto son internas del sistema.
fn scan(dirs: &[PathBuf]) -> Vec<Family> {
    let mut best: std::collections::BTreeMap<String, ((bool, u16, u16), FontChoice)> = Default::default();
    for path in font_files(dirs) {
        for face in faces_in(&path).unwrap_or_default() {
            if face.family.starts_with('.') {
                continue;
            }
            let key = face.family.to_lowercase();
            if best.get(&key).is_some_and(|(oddness, _)| *oddness <= face.oddness) {
                continue;
            }
            let choice = FontChoice { path: path.clone(), index: face.index, name: face.family, monospace: face.monospace };
            best.insert(key, (face.oddness, choice));
        }
    }
    best.into_iter().map(|(key, (_, choice))| Family { choice, key }).collect()
}

// Un archivo elegido a mano: su cara más normal.
fn choice_from_file(path: &Path) -> Result<FontChoice, String> {
    let face = faces_in(path)?
        .into_iter()
        .min_by_key(|face| face.oddness)
        .ok_or("el archivo no trae ninguna fuente con contornos que se pueda usar")?;
    Ok(FontChoice { path: path.to_owned(), index: face.index, name: face.family, monospace: face.monospace })
}

// Una cara suelta en un archivo sfnt propio: de una colección sólo salen las
// tablas de esa cara, y egui la carga siempre con índice 0.
fn build_sfnt(version: [u8; 4], tables: &[(&Table, Vec<u8>)]) -> Vec<u8> {
    let count = tables.len() as u16;
    let entry_selector = 15 - count.leading_zeros() as u16; // count ≥ 1
    let search_range = 16 << entry_selector;
    let mut out = Vec::new();
    out.extend_from_slice(&version);
    for field in [count, search_range, entry_selector, count * 16 - search_range] {
        out.extend_from_slice(&field.to_be_bytes());
    }
    let mut offset = 12 + tables.len() * 16;
    for (table, data) in tables {
        out.extend_from_slice(&table.tag);
        for field in [table.checksum, offset as u32, data.len() as u32] {
            out.extend_from_slice(&field.to_be_bytes());
        }
        offset += data.len().next_multiple_of(4);
    }
    for (_, data) in tables {
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    out
}

/// Lee una cara y la deja lista para egui. El segundo valor dice si trae
/// letras latinas. Todo lo que egui no sabría abrir se rechaza aquí: una
/// fuente inválida en `set_fonts` tumba la app.
pub fn load_face(path: &Path, index: u32) -> Result<(Vec<u8>, bool), String> {
    let mut file = File::open(path).map_err(|e| open_error(path, &e))?;
    let file_len = file.metadata().map_err(|e| open_error(path, &e))?.len();
    let offsets = face_offsets(&mut file)?;
    let offset = *offsets.get(index as usize).ok_or("el archivo ya no trae esa fuente")?;
    let (version, mut list) = tables(&mut file, offset, file_len)?;
    if !has_outlines(&list) {
        return Err("la fuente no trae contornos (es de mapas de bits) y no se puede dibujar".into());
    }
    let total: u64 = list.iter().map(|t| t.len as u64).sum();
    if total > MAX_FACE_BYTES {
        return Err(format!("la fuente pesa {} MiB (el tope es {} MiB)", total >> 20, MAX_FACE_BYTES >> 20));
    }
    list.sort_by_key(|t| t.tag);
    list.dedup_by_key(|t| t.tag);
    let mut data = Vec::with_capacity(list.len());
    for table in &list {
        data.push((table, read_at(&mut file, table.offset as u64, table.len as usize).map_err(|_| DAMAGED)?));
    }
    let bytes = build_sfnt(version, &data);
    // El mismo analizador que usa egui por dentro (ab_glyph → ttf-parser).
    let face = ttf_parser::Face::parse(&bytes, 0).map_err(|e| format!("la fuente está dañada o no es compatible ({e})"))?;
    let latin = face.glyph_index('a').is_some();
    Ok((bytes, latin))
}

type Source = (PathBuf, u32);

fn same(source: &Option<Source>, choice: &FontChoice) -> bool {
    source.as_ref().is_some_and(|(path, index)| *path == choice.path && *index == choice.index)
}

enum Msg {
    Scanned(Vec<Family>),
    Picked(Result<FontChoice, String>),
    Loaded(Source, Result<(Vec<u8>, bool), String>),
}

pub struct SystemFonts {
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    dirs: Vec<PathBuf>,
    // Las familias instaladas; `None` hasta que alguien abre los controles.
    families: Option<Vec<Family>>,
    scanning: bool,
    filter: String,
    only_monospace: bool,
    picking: bool,
    picked: Option<FontChoice>, // espera a controls, que es quien toca la elección
    loading: Option<Source>,
    failed: Option<Source>, // no se reintenta en cada cuadro
    ready: Option<(Source, Vec<u8>)>, // leída, pendiente de pasar a egui
    // Lo que egui tiene ahora en USER_FONT: la fuente (si hay) y con qué
    // familia de reserva. `None` en la reserva: todavía no se registró.
    shown: Option<Source>,
    fallback: Option<bool>,
    error: Option<String>,
}

impl Default for SystemFonts {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            dirs: font_dirs(),
            families: None,
            scanning: false,
            filter: String::new(),
            only_monospace: false,
            picking: false,
            picked: None,
            loading: None,
            failed: None,
            ready: None,
            shown: None,
            fallback: None,
            error: None,
        }
    }
}

impl SystemFonts {
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    // El trabajo corre en su hilo y despierta a la interfaz al terminar.
    fn spawn(&mut self, ctx: &egui::Context, job: impl FnOnce() -> Msg + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("eminus-fuentes".into()).spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            self.error = Some(format!("no se pudieron leer las fuentes ({e})"));
        }
    }

    fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Scanned(families) => {
                    self.scanning = false;
                    self.families = Some(families);
                }
                Msg::Picked(result) => {
                    self.picking = false;
                    match result {
                        Ok(choice) => {
                            self.error = None;
                            self.picked = Some(choice);
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Msg::Loaded(source, result) => {
                    if self.loading.as_ref() != Some(&source) {
                        continue; // ya se pidió otra
                    }
                    self.loading = None;
                    match result {
                        Ok((bytes, latin)) => {
                            self.error = (!latin).then(|| "la fuente no trae letras latinas: el texto sigue con la predeterminada".into());
                            self.ready = Some((source, bytes));
                        }
                        Err(e) => {
                            self.error = Some(e);
                            self.failed = Some(source);
                        }
                    }
                }
            }
        }
    }

    fn install(&mut self, ctx: &egui::Context, font: Option<(Source, Vec<u8>)>, monospace: bool) {
        let mut defs = egui::FontDefinitions::default();
        let base = if monospace { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
        let mut family = defs.families.get(&base).cloned().unwrap_or_default();
        self.shown = font.map(|(source, bytes)| {
            defs.font_data.insert(USER_FONT.into(), egui::FontData::from_owned(bytes));
            family.insert(0, USER_FONT.into());
            source
        });
        defs.families.insert(egui::FontFamily::Name(USER_FONT.into()), family);
        ctx.set_fonts(defs);
        self.fallback = Some(monospace);
    }

    /// Mantiene la familia `USER_FONT` de egui al día con la elección. Hay
    /// que llamarlo antes de `personalization::apply`; es barato cuando nada
    /// cambió. Devuelve `true` si cambió las fuentes de egui (valen desde el
    /// cuadro siguiente).
    pub fn update(&mut self, ctx: &egui::Context, choice: &Option<FontChoice>) -> bool {
        self.poll();
        // Sin elección la familia existe igual, con la reserva monoespaciada.
        let monospace = choice.as_ref().is_none_or(|c| c.monospace);
        let Some(choice) = choice else {
            let stale = self.shown.is_some() || self.fallback.is_none();
            if self.loading.take().is_some() || self.failed.take().is_some() || self.shown.is_some() {
                self.error = None; // era de la fuente que ya no se usa
            }
            self.ready = None;
            if stale {
                self.install(ctx, None, monospace);
            }
            return stale;
        };

        if self.ready.as_ref().is_some_and(|(source, _)| *source == (choice.path.clone(), choice.index)) {
            let font = self.ready.take();
            self.install(ctx, font, monospace);
            return true;
        }
        let current = same(&self.shown, choice) && self.fallback == Some(monospace);
        let loading = same(&self.loading, choice);
        if !current && !loading && !same(&self.failed, choice) {
            let source = (choice.path.clone(), choice.index);
            self.loading = Some(source.clone());
            self.failed = None;
            self.spawn(ctx, move || {
                let result = if source.1 < MAX_FACES { load_face(&source.0, source.1) } else { Err("el archivo ya no trae esa fuente".into()) };
                Msg::Loaded(source, result)
            });
            return self.register(ctx, monospace);
        }
        // Mientras la nueva carga se queda la anterior; si falló, la reserva.
        if !current && !loading && (self.shown.is_some() || self.fallback != Some(monospace)) {
            self.install(ctx, None, monospace);
            return true;
        }
        self.register(ctx, monospace)
    }

    // La familia tiene que existir aunque todavía no haya fuente que poner.
    fn register(&mut self, ctx: &egui::Context, monospace: bool) -> bool {
        let missing = self.fallback.is_none();
        if missing {
            self.install(ctx, None, monospace);
        }
        missing
    }

    /// Controles del tipo de letra para el panel «personalizar». Devuelve
    /// `true` si cambió la elección (para guardarla). La primera vez que se
    /// muestran empieza la búsqueda de fuentes instaladas.
    pub fn controls(&mut self, ui: &mut egui::Ui, choice: &mut Option<FontChoice>) -> bool {
        let before = choice.clone();
        self.poll();
        if self.families.is_none() && !self.scanning {
            self.scanning = true;
            let dirs = self.dirs.clone();
            self.spawn(ui.ctx(), move || Msg::Scanned(scan(&dirs)));
        }
        if let Some(picked) = self.picked.take() {
            *choice = Some(picked);
        }

        let total = self.families.as_ref().map_or(0, Vec::len);
        ui.horizontal_wrapped(|ui| {
            ui.label("Tipo de letra");
            let selected = choice.as_ref().map_or("Predeterminada".to_owned(), |c| clean_name(&c.name).unwrap_or_else(|| "Sin nombre".into()));
            egui::ComboBox::from_id_salt("fuente-instalada").selected_text(selected).truncate().width(220.0_f32.min(ui.available_width())).height(280.0).show_ui(ui, |ui| {
                if ui.selectable_label(choice.is_none(), "Predeterminada").clicked() {
                    *choice = None;
                }
                let needle = self.filter.trim().to_lowercase();
                let mut shown = 0;
                for family in self.families.iter().flatten() {
                    if (self.only_monospace && !family.choice.monospace) || !family.key.contains(&needle) {
                        continue;
                    }
                    shown += 1;
                    let picked = choice.as_ref().is_some_and(|c| c.path == family.choice.path && c.index == family.choice.index);
                    if ui.selectable_label(picked, &family.choice.name).clicked() {
                        *choice = Some(family.choice.clone());
                        self.failed = None; // elegirla otra vez la vuelve a leer
                    }
                }
                if self.scanning {
                    ui.weak("Buscando fuentes instaladas…");
                } else if shown == 0 {
                    ui.weak(if total == 0 { "No se encontraron fuentes instaladas." } else { "Ninguna fuente coincide con el filtro." });
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filtrar por nombre").desired_width(160.0));
            ui.checkbox(&mut self.only_monospace, "Sólo monoespaciadas");
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.picking, egui::Button::new("Elegir archivo…")).clicked() {
                let picked = rfd::FileDialog::new().set_title("Archivo de fuente").add_filter("Fuentes", &EXTENSIONS).pick_file();
                if let Some(path) = picked {
                    self.picking = true;
                    self.error = None;
                    self.spawn(ui.ctx(), move || Msg::Picked(choice_from_file(&path)));
                }
            }
            if ui.add_enabled(choice.is_some(), egui::Button::new("Usar la predeterminada")).clicked() {
                *choice = None;
            }
            if self.picking {
                ui.weak("Leyendo el archivo…");
            } else if self.loading.is_some() {
                ui.weak("Cargando la fuente…");
            } else if self.scanning {
                ui.weak("Buscando fuentes instaladas…");
            } else if total > 0 {
                ui.weak(format!("{total} familias instaladas"));
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, format!("No se pudo usar la fuente: {error}."));
        }
        *choice != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Carpeta propia en el temporal del sistema: las pruebas no tocan las fuentes del usuario.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eminus-fuentes-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // Las fuentes que egui ya trae dentro sirven de muestra: Hack es
    // monoespaciada y Ubuntu-Light no.
    fn builtin(name: &str) -> Vec<u8> {
        egui::FontDefinitions::default().font_data[name].font.to_vec()
    }

    // Una colección .ttc con esas fuentes, una detrás de otra.
    fn collection(fonts: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"ttcf\0\x01\0\0".to_vec();
        out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
        let mut base = 12 + fonts.len() * 4;
        for font in fonts {
            out.extend_from_slice(&(base as u32).to_be_bytes());
            base += font.len();
        }
        for font in fonts {
            let base = out.len() as u32;
            let mut font = font.clone();
            for record in 0..be16(&font, 4).unwrap() as usize {
                let at = 12 + record * 16 + 8;
                let moved = be32(&font, at).unwrap() + base;
                font[at..at + 4].copy_from_slice(&moved.to_be_bytes());
            }
            out.extend_from_slice(&font);
        }
        out
    }

    fn headless_input(width: f32) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 480.0))),
            ..Default::default()
        }
    }

    // Corre cuadros hasta que `done`, llamando a `update` antes de cada uno.
    // Devuelve cuántas veces cambió las fuentes de egui.
    fn frames(ctx: &egui::Context, fonts: &mut SystemFonts, choice: &Option<FontChoice>, done: impl Fn(&SystemFonts) -> bool) -> usize {
        let mut changes = 0;
        for _ in 0..2000 {
            changes += fonts.update(ctx, choice) as usize;
            let _ = ctx.run(headless_input(720.0), |ctx| {
                // El texto con la familia propia se puede medir en cualquier cuadro.
                let font = egui::FontId::new(14.0, egui::FontFamily::Name(USER_FONT.into()));
                ctx.fonts(|f| f.layout_no_wrap("año ✓".into(), font, egui::Color32::WHITE));
            });
            if done(fonts) {
                return changes;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("la fuente no terminó de cargar");
    }

    fn user_family(ctx: &egui::Context) -> Vec<String> {
        ctx.fonts(|f| f.lock().fonts.definitions().families[&egui::FontFamily::Name(USER_FONT.into())].clone())
    }

    #[test]
    fn scan_groups_real_names_by_family() {
        let dir = temp_dir("scan");
        std::fs::create_dir_all(dir.join("sub/mas")).unwrap();
        std::fs::write(dir.join("hack.TTF"), builtin("Hack")).unwrap();
        std::fs::write(dir.join("sub/mas/todo.ttc"), collection(&[builtin("Hack"), builtin("Ubuntu-Light")])).unwrap();
        std::fs::write(dir.join("rota.ttf"), b"esto no es una fuente").unwrap();
        std::fs::write(dir.join("corta.otf"), &builtin("Hack")[..300]).unwrap();
        std::fs::write(dir.join("notas.txt"), builtin("Hack")).unwrap();

        let found = scan(&[dir.join("no-existe"), dir.clone()]);
        let names: Vec<_> = found.iter().map(|f| (f.choice.name.as_str(), f.choice.monospace, f.choice.index)).collect();
        assert_eq!(names, [("Hack", true, 0), ("Ubuntu", false, 1)]);
        // La repetida se queda con el primer archivo; la otra sale de la colección.
        assert_eq!(found[0].choice.path, dir.join("hack.TTF"));
        assert_eq!(found[1].choice.path, dir.join("sub/mas/todo.ttc"));
        assert_eq!(found[1].key, "ubuntu");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_is_bounded() {
        let dir = temp_dir("limites");
        // Más honda que MAX_DEPTH: no se llega a ella.
        let deep = (0..=MAX_DEPTH).fold(dir.clone(), |path, level| path.join(format!("n{level}")));
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("hack.ttf"), builtin("Hack")).unwrap();
        assert!(font_files(std::slice::from_ref(&dir)).is_empty());
        std::fs::write(deep.parent().unwrap().join("hack.ttf"), builtin("Hack")).unwrap();
        assert_eq!(font_files(std::slice::from_ref(&dir)).len(), 1);

        // Una colección que dice traer millones de caras no reserva memoria para ellas.
        let mut huge = b"ttcf\0\x01\0\0".to_vec();
        huge.extend_from_slice(&u32::MAX.to_be_bytes());
        std::fs::write(dir.join("enorme.ttc"), &huge).unwrap();
        assert!(faces_in(&dir.join("enorme.ttc")).unwrap_err().contains("colección"));
        assert!(load_face(&dir.join("enorme.ttc"), 0).is_err());

        // Una tabla que apunta fuera del archivo: la cara se descarta.
        let mut font = builtin("Hack");
        font[12 + 8..12 + 12].copy_from_slice(&u32::MAX.to_be_bytes());
        std::fs::write(dir.join("fuera.ttf"), &font).unwrap();
        assert!(faces_in(&dir.join("fuera.ttf")).unwrap().is_empty());
        assert!(load_face(&dir.join("fuera.ttf"), 0).is_err());

        // Demasiadas tablas declaradas.
        let mut font = builtin("Hack");
        font[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
        std::fs::write(dir.join("tablas.ttf"), &font).unwrap();
        assert!(load_face(&dir.join("tablas.ttf"), 0).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_skips_fonts_without_letters() {
        let dir = temp_dir("sin-letras");
        let mut font = builtin("Ubuntu-Light");
        for record in 0..be16(&font, 4).unwrap() as usize {
            let at = 12 + record * 16;
            if &font[at..at + 4] == b"cmap" { font[at..at + 4].copy_from_slice(b"xxxx"); }
        }
        std::fs::write(dir.join("sin-letras.ttf"), font).unwrap();
        assert!(scan(std::slice::from_ref(&dir)).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_extracts_one_face_from_a_collection() {
        let dir = temp_dir("cara");
        let path = dir.join("todo.ttc");
        std::fs::write(&path, collection(&[builtin("Hack"), builtin("Ubuntu-Light")])).unwrap();

        let (bytes, latin) = load_face(&path, 1).unwrap();
        assert!(latin);
        // Sólo esa cara: pesa como el original suelto, no como la colección.
        assert!(bytes.len() <= builtin("Ubuntu-Light").len() + 3 * MAX_TABLES);
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        assert!(!face.is_monospaced() && face.glyph_index('ñ').is_some());
        assert!(ttf_parser::Face::parse(&load_face(&path, 0).unwrap().0, 0).unwrap().is_monospaced());

        assert!(load_face(&path, 2).unwrap_err().contains("ya no trae"));
        assert!(load_face(&dir.join("no-existe.ttf"), 0).unwrap_err().contains("ya no existe"));
        let choice = choice_from_file(&path).unwrap();
        assert_eq!((choice.name.as_str(), choice.index, choice.monospace), ("Hack", 0, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn choice_survives_the_prefs_file() {
        let choice = FontChoice { path: "/Library/Fonts/Una Fuente.ttc".into(), index: 3, name: "Una Fuente".into(), monospace: true };
        let json = serde_json::to_string(&Some(choice.clone())).unwrap();
        assert_eq!(serde_json::from_str::<Option<FontChoice>>(&json).unwrap(), Some(choice));
    }

    #[test]
    fn update_installs_the_font_once_and_keeps_the_fallbacks() {
        let dir = temp_dir("egui");
        let path = dir.join("todo.ttc");
        std::fs::write(&path, collection(&[builtin("Hack"), builtin("Ubuntu-Light")])).unwrap();
        let ctx = egui::Context::default();
        let mut fonts = SystemFonts::default();

        // Sin elección la familia ya existe, con las fuentes de egui.
        assert_eq!(frames(&ctx, &mut fonts, &None, |_| true), 1);
        assert_eq!(user_family(&ctx)[0], "Hack");

        let choice = Some(FontChoice { path: path.clone(), index: 1, name: "Ubuntu Light".into(), monospace: false });
        assert_eq!(frames(&ctx, &mut fonts, &choice, |f| f.shown.is_some()), 1);
        let _ = ctx.run(headless_input(720.0), |_| {});
        let family = user_family(&ctx);
        // La elegida delante y, detrás, la reserva proporcional completa.
        assert_eq!(family[0], USER_FONT);
        assert_eq!(family[1..], egui::FontDefinitions::default().families[&egui::FontFamily::Proportional][..]);
        // Las familias de egui no cambian.
        let defs = ctx.fonts(|f| f.lock().fonts.definitions().clone());
        assert_eq!(defs.families[&egui::FontFamily::Monospace], egui::FontDefinitions::default().families[&egui::FontFamily::Monospace]);
        assert_eq!(defs.families[&egui::FontFamily::Proportional], egui::FontDefinitions::default().families[&egui::FontFamily::Proportional]);
        assert_eq!(defs.font_data[USER_FONT].index, 0);
        assert!(fonts.error().is_none());

        // En reposo no se vuelve a tocar egui.
        assert_eq!(frames(&ctx, &mut fonts, &choice, |_| true), 0);
        assert!(!fonts.update(&ctx, &choice));

        // De vuelta a la predeterminada.
        assert_eq!(frames(&ctx, &mut fonts, &None, |_| true), 1);
        let _ = ctx.run(headless_input(720.0), |_| {});
        assert!(!user_family(&ctx).contains(&USER_FONT.to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_font_reports_an_error_and_keeps_the_family() {
        let dir = temp_dir("falta");
        let ctx = egui::Context::default();
        let mut fonts = SystemFonts::default();
        // Lo guardado apunta a un archivo que ya no está: el primer cuadro
        // ya puede usar la familia.
        let choice = Some(FontChoice { path: dir.join("no-existe.ttf"), index: 0, name: "Perdida".into(), monospace: true });
        assert_eq!(frames(&ctx, &mut fonts, &choice, |f| f.error().is_some()), 1);
        assert!(fonts.error().unwrap().contains("ya no existe"));
        assert_eq!(user_family(&ctx)[0], "Hack");
        // No se reintenta ni se reinstala en cada cuadro.
        assert_eq!(frames(&ctx, &mut fonts, &choice, |_| true), 0);
        assert!(fonts.loading.is_none());

        // Un archivo dañado tampoco llega a egui.
        std::fs::write(dir.join("rota.ttf"), &builtin("Hack")[..4000]).unwrap();
        let choice = Some(FontChoice { path: dir.join("rota.ttf"), index: 0, name: "Rota".into(), monospace: false });
        frames(&ctx, &mut fonts, &choice, |f| f.failed.is_some());
        assert!(fonts.error().is_some() && fonts.shown.is_none());
        // Un índice imposible guardado en disco se rechaza sin abrir nada.
        let choice = Some(FontChoice { path: dir.join("rota.ttf"), index: u32::MAX, name: "Rota".into(), monospace: false });
        frames(&ctx, &mut fonts, &choice, |f| f.failed.as_ref().is_some_and(|s| s.1 == u32::MAX));

        // Al quitar la elección se va el aviso.
        frames(&ctx, &mut fonts, &None, |_| true);
        assert!(fonts.error().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn controls_scan_lazily_and_change_nothing_alone() {
        let dir = temp_dir("controles");
        std::fs::write(dir.join("hack.ttf"), builtin("Hack")).unwrap();
        let ctx = egui::Context::default();
        let mut fonts = SystemFonts { dirs: vec![dir.clone()], ..Default::default() };
        let mut choice = None;
        fonts.update(&ctx, &choice);
        assert!(fonts.families.is_none() && !fonts.scanning);
        for _ in 0..2000 {
            let mut changed = false;
            let _ = ctx.run(headless_input(720.0), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| changed = fonts.controls(ui, &mut choice));
            });
            assert!(!changed);
            if fonts.families.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(fonts.families.as_ref().unwrap().len(), 1);
        assert!(choice.is_none() && !ctx.wants_keyboard_input());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Las fuentes públicas del sistema: sólo se leen, y sólo en macOS.
    #[cfg(target_os = "macos")]
    #[test]
    fn finds_and_loads_an_installed_macos_font() {
        if !Path::new("/System/Library/Fonts/Menlo.ttc").exists() {
            return;
        }
        let found = scan(&["/System/Library/Fonts".into()]);
        let menlo = found.iter().find(|f| f.choice.name == "Menlo").expect("Menlo instalada");
        assert!(menlo.choice.monospace);
        assert!(found.iter().all(|f| !f.choice.name.starts_with('.')));
        let (bytes, latin) = load_face(&menlo.choice.path, menlo.choice.index).unwrap();
        assert!(latin && (bytes.len() as u64) < MAX_FACE_BYTES);
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        assert!(face.is_regular() && !face.is_italic());

        let ctx = egui::Context::default();
        let mut fonts = SystemFonts::default();
        let choice = Some(menlo.choice.clone());
        frames(&ctx, &mut fonts, &choice, |f| f.shown.is_some());
        assert!(fonts.error().is_none());
    }
}
