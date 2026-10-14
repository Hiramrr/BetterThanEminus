// Foto de fondo de la ventana: la imagen elegida, tramada con puntos (Bayer
// 8×8) y fundida hacia el color de fondo del tema, igual que src/bg.js en la
// extensión.
//
// La foto se copia reducida a la caché de la app (el original no se toca) y
// todo lo pesado —leer, reducir, tramar— corre en hilos aparte: el hilo de la
// interfaz sólo sube la textura. La textura se rehace cuando cambia la imagen,
// el color de fondo, el efecto o el tamaño de la ventana; en reposo no se
// procesa ni se repinta nada.

use eframe::egui::{self, Color32};
use image::{imageops, DynamicImage, ImageDecoder, ImageReader, RgbImage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

/// Lado mayor de la copia guardada (y de lo que queda en memoria).
pub const MAX_SIDE: u32 = 1920;
/// Archivos más pesados no se abren.
pub const MAX_FILE_BYTES: u64 = 64 << 20;
/// Tope de memoria para decodificar: 256 MiB son unos 89 MP en RGB o 67 MP en RGBA.
pub const MAX_DECODE_BYTES: u64 = 256 << 20;
/// Lado mayor que se acepta al decodificar.
pub const MAX_DIMENSION: u32 = 16_384;
/// Lado mayor de la textura, en celdas: 2048 × 2048 × 4 bytes = 16 MiB.
pub const MAX_CELLS: u32 = 2048;

const CELL: f32 = 2.0; // tamaño del punto, en puntos de egui
const SETTLE: f64 = 0.15; // segundos sin cambiar de tamaño antes de volver a tramar
const PREFIX: &str = "fondo-";

// Misma matriz que bg.js: cada nivel reparte los cuadrantes en orden 0, 2, 3, 1.
const BAYER: [u8; 64] = {
    let mut m = [0u8; 64];
    let mut i = 0;
    while i < 64 {
        let (x, y) = (i & 7, i >> 3);
        let mut bit = 0;
        while bit < 3 {
            let quadrant = [0u8, 2, 3, 1][((y >> bit) & 1) * 2 + ((x >> bit) & 1)];
            m[i] += quadrant << (2 * (2 - bit));
            bit += 1;
        }
        i += 1;
    }
    m
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackgroundPrefs {
    /// La copia en la caché de la app, no el archivo original.
    pub image: Option<PathBuf>,
    pub dither: bool,
    /// 0..1: cuánto se nota la foto sobre el color de fondo.
    pub intensity: f32,
    /// 0.65..1: opacidad de los paneles cuando hay foto.
    pub panel_opacity: f32,
}

impl Default for BackgroundPrefs {
    fn default() -> Self {
        Self { image: None, dither: true, intensity: 0.7, panel_opacity: 0.8 }
    }
}

impl BackgroundPrefs {
    /// Deja los valores dentro de su rango (lo leído del disco puede traer cualquier cosa).
    pub fn sanitize(&mut self) {
        let default = Self::default();
        self.intensity = bounded(self.intensity, 0.0, 1.0, default.intensity);
        self.panel_opacity = bounded(self.panel_opacity, 0.65, 1.0, default.panel_opacity);
        if self.image.as_ref().is_some_and(|p| p.as_os_str().is_empty()) {
            self.image = None;
        }
    }
}

fn bounded(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() { value.clamp(min, max) } else { default }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default())
}

// Misma raíz que la caché de store.rs, en su propia carpeta.
fn cache_dir() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) => PathBuf::from(dir).join("eminus"),
        None if cfg!(target_os = "macos") => home().join("Library/Caches/eminus"),
        None => home().join(".cache/eminus"),
    }
    .join("fondo")
}

// ---------- imagen ----------

/// Lee una imagen sin pasar de los topes de arriba y la devuelve ya reducida
/// a MAX_SIDE y con la orientación de la cámara aplicada.
pub fn decode_bounded(path: &Path) -> Result<RgbImage, String> {
    decode_with(path, MAX_DECODE_BYTES)
}

fn decode_with(path: &Path, max_bytes: u64) -> Result<RgbImage, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("no se pudo abrir el archivo ({e})"))?;
    if !meta.is_file() {
        return Err("no es un archivo".into());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("el archivo pesa más de {} MB", MAX_FILE_BYTES >> 20));
    }
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|e| format!("no se pudo abrir el archivo ({e})"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(max_bytes);
    reader.limits(limits);
    let unreadable = |_| "no es una imagen PNG, JPEG o WebP que se pueda leer".to_string();
    let mut decoder = reader.into_decoder().map_err(|e| match e {
        image::ImageError::Limits(_) => format!("la imagen mide más de {MAX_DIMENSION} px de lado"),
        other => unreadable(other),
    })?;
    // No todos los decodificadores respetan max_alloc: se comprueba antes de reservar.
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err("la imagen está vacía".into());
    }
    if decoder.total_bytes() > max_bytes {
        return Err(format!("la imagen es demasiado grande ({width} × {height} px)"));
    }
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(unreadable)?;
    if width.max(height) > MAX_SIDE {
        img = img.thumbnail(MAX_SIDE, MAX_SIDE);
    }
    img.apply_orientation(orientation);
    Ok(img.into_rgb8())
}

/// Copia la imagen, reducida y en JPEG, a `dir` con un nombre nuevo. El
/// original sólo se lee.
pub fn import_image(src: &Path, dir: &Path) -> Result<(PathBuf, RgbImage), String> {
    let img = decode_bounded(src)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let path = dir.join(format!("{PREFIX}{stamp}.jpg"));
    write_jpeg_private(&path, &img).map_err(|e| format!("no se pudo guardar la copia ({e})"))?;
    Ok((path, img))
}

// Escribe a un temporal y renombra, sólo para el usuario (como store.rs).
fn write_jpeg_private(path: &Path, img: &RgbImage) -> std::io::Result<()> {
    use std::io::{BufWriter, Write};
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut writer = BufWriter::new(options.open(&tmp)?);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 86)
        .encode_image(img).map_err(std::io::Error::other)?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(&tmp, path)
}

// Borra las copias que ya no se usan. Sólo toca los «fondo-*.jpg» de `dir`.
fn sweep(dir: &Path, keep: Option<&Path>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = entry.file_name().to_str().is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(".jpg"));
        if ours && Some(path.as_path()) != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ---------- tramado ----------

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lum(r: f32, g: f32, b: f32) -> f32 {
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
}

/// La foto recortada para cubrir `w` × `h` celdas, tramada (o lisa) y fundida
/// hacia `bg`. Se ve entera arriba y se va apagando hacia abajo. Las medidas
/// se acotan a 1..=MAX_CELLS.
pub fn render(src: &RgbImage, w: u32, h: u32, bg: [u8; 3], dither: bool) -> egui::ColorImage {
    let (w, h) = (w.clamp(1, MAX_CELLS), h.clamp(1, MAX_CELLS));
    let back = Color32::from_rgb(bg[0], bg[1], bg[2]);
    if src.width() == 0 || src.height() == 0 {
        return egui::ColorImage::new([w as usize, h as usize], back);
    }

    // Como en bg.js, la foto cubre el 95 % de alto; lo que no alcance queda del
    // color de fondo. Se recorta antes de escalar: una foto muy alargada no
    // obliga a reservar más que la textura.
    let (iw, ih) = (src.width() as f32, src.height() as f32);
    let area = h as f32 * 0.95;
    let scale = (w as f32 / iw).max(area / ih);
    let cw = ((w as f32 / scale).round() as u32).clamp(1, src.width());
    let x0 = (src.width() - cw) / 2;
    let y0 = (((ih - area / scale) / 2.0).max(0.0) as u32).min(src.height() - 1);
    let ch = ((h as f32 / scale).round() as u32).clamp(1, src.height() - y0);
    let rows = ((ch as f32 * scale).round() as u32).clamp(1, h);
    let scaled = imageops::resize(&*imageops::crop_imm(src, x0, y0, cw, ch), w, rows, imageops::FilterType::Triangle);

    let [br, bg_, bb] = bg.map(f32::from);
    let bg_lum = lum(br, bg_, bb);
    let mut pixels = Vec::with_capacity(w as usize * h as usize);
    for y in 0..h {
        if y >= rows {
            pixels.extend(std::iter::repeat_n(back, w as usize));
            continue;
        }
        let v = 1.0 - 0.55 * smoothstep(0.2, 1.0, y as f32 / h as f32);
        let (gain, dot, shade) = (v * 1.1, 0.35 + 0.65 * v, 0.25 * v * v * v);
        let row = ((y & 7) << 3) as usize;
        for x in 0..w {
            let [r, g, b] = scaled.get_pixel(x, y).0.map(f32::from);
            let k = if dither {
                // Más puntos donde la foto se aleja del fondo; donde se parece queda una sombra tenue.
                let contrast = ((lum(r, g, b) - bg_lum).abs() * 1.6 + 0.1).min(1.0);
                let threshold = (f32::from(BAYER[row | (x & 7) as usize]) + 0.5) / 64.0;
                if contrast * gain > threshold { dot } else { shade }
            } else {
                v
            };
            let mix = |from: f32, to: f32| (from + (to - from) * k).round() as u8;
            pixels.push(Color32::from_rgb(mix(br, r), mix(bg_, g), mix(bb, b)));
        }
    }
    egui::ColorImage { size: [w as usize, h as usize], pixels }
}

// ---------- estado ----------

// De qué depende la textura: si nada de esto cambia, no se vuelve a tramar.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    image: u64,
    w: u32,
    h: u32,
    bg: [u8; 3],
    dither: bool,
}

enum Msg {
    Loaded(PathBuf, Result<RgbImage, String>),
    Imported(Result<(PathBuf, RgbImage), String>),
    Rendered(Key, egui::ColorImage),
}

pub struct Background {
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    dir: PathBuf,
    // La foto ya reducida: como mucho MAX_SIDE² × 3 bytes (unos 11 MiB).
    source: Option<(PathBuf, Arc<RgbImage>)>,
    image: u64, // cambia con cada foto
    loading: Option<PathBuf>,
    failed: Option<PathBuf>, // no se reintenta en cada cuadro
    importing: bool,
    imported: Option<(PathBuf, RgbImage)>, // espera a show_controls, que es quien toca las prefs
    texture: Option<(Key, egui::TextureHandle)>,
    rendering: Option<Key>,
    size: (u32, u32),
    size_at: f64,
    error: Option<String>,
}

impl Default for Background {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            dir: cache_dir(),
            source: None,
            image: 0,
            loading: None,
            failed: None,
            importing: false,
            imported: None,
            texture: None,
            rendering: None,
            size: (0, 0),
            size_at: 0.0,
            error: None,
        }
    }
}

impl Background {
    /// Hay una foto pintada: los paneles pueden dejarla ver.
    pub fn has_image(&self) -> bool {
        self.texture.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    // El trabajo corre en su hilo y despierta a la interfaz al terminar.
    fn spawn(&mut self, ctx: &egui::Context, job: impl FnOnce() -> Msg + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("eminus-fondo".into()).spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            self.error = Some(format!("no se pudo preparar el fondo ({e})"));
        }
    }

    fn set_source(&mut self, path: PathBuf, img: RgbImage) {
        self.source = Some((path, Arc::new(img)));
        self.image += 1;
        self.loading = None;
        self.failed = None;
        self.error = None;
    }

    fn clear(&mut self) {
        self.source = None;
        self.texture = None;
        self.loading = None;
        self.failed = None;
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Loaded(path, result) => {
                    if self.loading.as_ref() != Some(&path) {
                        continue; // ya se pidió otra
                    }
                    match result {
                        Ok(img) => self.set_source(path, img),
                        Err(e) => {
                            self.loading = None;
                            self.error = Some(e);
                            self.failed = Some(path);
                        }
                    }
                }
                Msg::Imported(result) => {
                    self.importing = false;
                    match result {
                        Ok(imported) => {
                            self.error = None;
                            self.imported = Some(imported);
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Msg::Rendered(key, pixels) => {
                    self.rendering = None;
                    if key.image == self.image && self.source.is_some() {
                        // Los puntos van nítidos; la foto lisa, suavizada.
                        let options = if key.dither { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
                        self.texture = Some((key, ctx.load_texture("eminus-fondo", pixels, options)));
                    }
                }
            }
        }
    }

    /// Pinta la foto en la capa de fondo. Hay que llamarlo antes de los
    /// paneles, que comparten esa capa y quedan encima.
    pub fn paint(&mut self, ctx: &egui::Context, prefs: &BackgroundPrefs, bg: Color32, _accent: Color32) {
        self.poll(ctx);
        let Some(path) = prefs.image.as_ref().filter(|p| !p.as_os_str().is_empty()) else {
            self.clear();
            return;
        };

        let ready = self.source.as_ref().is_some_and(|(loaded, _)| loaded == path);
        if !ready {
            if self.failed.as_ref() == Some(path) {
                self.clear();
                self.failed = Some(path.clone());
                return;
            }
            if self.loading.as_ref() != Some(path) {
                self.loading = Some(path.clone());
                let path = path.clone();
                self.spawn(ctx, move || {
                    let result = decode_bounded(&path);
                    Msg::Loaded(path, result)
                });
            }
        }

        let screen = ctx.screen_rect();
        if !(screen.width() >= 1.0 && screen.height() >= 1.0) {
            return; // ventana minimizada
        }
        let cell = CELL.max(screen.width().max(screen.height()) / MAX_CELLS as f32);
        let (w, h) = ((screen.width() / cell).ceil() as u32, (screen.height() / cell).ceil() as u32);
        let now = ctx.input(|i| i.time);
        if (w, h) != self.size {
            self.size = (w, h);
            self.size_at = now;
        }

        let key = Key { image: self.image, w, h, bg: [bg.r(), bg.g(), bg.b()], dither: prefs.dither };
        let current = self.texture.as_ref().map(|(key, _)| *key);
        if ready && current != Some(key) && self.rendering.is_none() {
            // Tramar en cada paso de un cambio de tamaño lo traba: mientras
            // dura se estira la textura que hay y se trama una vez al terminar.
            let resizing = current.is_some_and(|c| c.image == key.image && (c.w, c.h) != (w, h));
            let wait = SETTLE - (now - self.size_at);
            if resizing && wait > 0.0 {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            } else if let Some((_, src)) = &self.source {
                let src = src.clone();
                self.rendering = Some(key);
                self.spawn(ctx, move || Msg::Rendered(key, render(&src, key.w, key.h, key.bg, key.dither)));
            }
        }

        if let Some((drawn, texture)) = &self.texture {
            let scale = if (drawn.w, drawn.h) == (w, h) {
                cell
            } else {
                (screen.width() / drawn.w as f32).max(screen.height() / drawn.h as f32)
            };
            let rect = egui::Rect::from_min_size(screen.min, egui::vec2(drawn.w as f32, drawn.h as f32) * scale);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            let alpha = (bounded(prefs.intensity, 0.0, 1.0, 0.7) * 255.0).round() as u8;
            let painter = ctx.layer_painter(egui::LayerId::background());
            painter.rect_filled(screen, 0.0, bg);
            // La intensidad es sólo el tinte: moverla no vuelve a tramar.
            painter.image(texture.id(), rect, uv, Color32::from_white_alpha(alpha));
        }
    }

    /// Controles del fondo para el panel de ajustes. Devuelve true si cambió
    /// algo en `prefs` (para guardarlas).
    pub fn update_prefs(&mut self, ctx: &egui::Context, prefs: &mut BackgroundPrefs) -> bool {
        self.poll(ctx);
        let before = prefs.clone();
        prefs.sanitize();
        if let Some((path, img)) = self.imported.take() {
            sweep(&self.dir, Some(&path));
            prefs.image = Some(path.clone());
            self.set_source(path, img);
        }
        *prefs != before
    }

    pub fn show_controls(&mut self, ui: &mut egui::Ui, prefs: &mut BackgroundPrefs) -> bool {
        let before = prefs.clone();
        self.update_prefs(ui.ctx(), prefs);

        ui.horizontal_wrapped(|ui| {
            let label = if prefs.image.is_some() { "cambiar imagen…" } else { "elegir imagen…" };
            if ui.add_enabled(!self.importing, egui::Button::new(label)).clicked() {
                let picked = rfd::FileDialog::new()
                    .set_title("Foto de fondo")
                    .add_filter("Imágenes", &["png", "jpg", "jpeg", "webp"])
                    .pick_file();
                if let Some(src) = picked {
                    self.importing = true;
                    self.error = None;
                    let dir = self.dir.clone();
                    self.spawn(ui.ctx(), move || Msg::Imported(import_image(&src, &dir)));
                }
            }
            if prefs.image.is_some() && ui.add_enabled(!self.importing, egui::Button::new("quitar")).clicked() {
                prefs.image = None;
                sweep(&self.dir, None);
                self.clear();
                self.error = None;
            }
            if self.importing {
                ui.label("preparando la imagen…");
            } else if self.loading.is_some() {
                ui.label("cargando…");
            }
        });

        ui.add_enabled_ui(prefs.image.is_some(), |ui| {
            let percent = |v: f64, _| format!("{:.0} %", v * 100.0);
            ui.checkbox(&mut prefs.dither, "tramado de puntos");
            ui.add_space(12.0);
            ui.label("Intensidad de la imagen");
            ui.add(egui::Slider::new(&mut prefs.intensity, 0.0..=1.0).custom_formatter(percent));
            ui.add_space(12.0);
            ui.label("Opacidad de los paneles");
            ui.add(egui::Slider::new(&mut prefs.panel_opacity, 0.65..=1.0).custom_formatter(percent));
        });

        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, format!("no se pudo usar la imagen: {error}"));
        }
        *prefs != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Carpeta propia en el temporal del sistema: las pruebas no tocan la caché del usuario.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eminus-fondo-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn flat(w: u32, h: u32, rgb: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, image::Rgb(rgb))
    }

    #[test]
    fn bayer_is_the_standard_8x8_matrix() {
        assert_eq!(BAYER[..8], [0, 32, 8, 40, 2, 34, 10, 42]);
        assert_eq!(BAYER[8], 48);
        let mut sorted = BAYER;
        sorted.sort_unstable();
        assert!(sorted.iter().copied().eq(0..64));
    }

    #[test]
    fn dither_density_follows_contrast() {
        // Arriba v = 1: se encienden los umbrales por debajo de contraste × 1.1.
        let dots = |gray: u8| {
            let out = render(&flat(64, 64, [gray; 3]), 64, 80, [0, 0, 0], true);
            (0..8).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|&(x, y)| out.pixels[y * 64 + x].r() > gray / 2).count()
        };
        assert_eq!(dots(64), 35);
        assert_eq!(dots(16), 14); // casi igual que el fondo: pocos puntos
        assert_eq!(dots(255), 64);
        // Encendidos llevan el color de la foto; apagados, una sombra al 25 %.
        let out = render(&flat(64, 64, [64; 3]), 64, 80, [0, 0, 0], true);
        assert_eq!(out.pixels[0], Color32::from_rgb(64, 64, 64));
        assert_eq!(out.pixels[7], Color32::from_rgb(16, 16, 16));
    }

    #[test]
    fn plain_fades_towards_the_background() {
        let out = render(&flat(50, 50, [200, 100, 0]), 100, 40, [10, 10, 10], false);
        assert_eq!(out.size, [100, 40]);
        assert_eq!(out.pixels[0], Color32::from_rgb(200, 100, 0));
        let last = out.pixels[39 * 100];
        assert!(last.r() < 200 && last.r() > 10, "abajo se apaga sin desaparecer: {last:?}");
    }

    #[test]
    fn uncovered_rows_take_the_background() {
        // Foto muy alargada: cubre el 95 % de alto y el resto queda de fondo.
        let out = render(&flat(1920, 4, [255; 3]), 300, 200, [10, 20, 30], false);
        assert_eq!(out.size, [300, 200]);
        assert_eq!(out.pixels[0], Color32::from_rgb(255, 255, 255));
        assert_eq!(out.pixels[199 * 300], Color32::from_rgb(10, 20, 30));
    }

    #[test]
    fn render_size_is_bounded() {
        let src = flat(8, 8, [1, 2, 3]);
        assert_eq!(render(&src, 100_000, 0, [0; 3], true).size, [MAX_CELLS as usize, 1]);
        assert_eq!(render(&RgbImage::new(0, 0), 4, 4, [9; 3], true).pixels, vec![Color32::from_rgb(9, 9, 9); 16]);
    }

    #[test]
    fn prefs_defaults_and_bounds() {
        let prefs: BackgroundPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(prefs, BackgroundPrefs::default());
        assert!(prefs.image.is_none() && prefs.dither);

        let mut prefs: BackgroundPrefs = serde_json::from_str(r#"{"image":"","intensity":7,"panel_opacity":0.1}"#).unwrap();
        prefs.sanitize();
        assert_eq!((prefs.image.clone(), prefs.intensity, prefs.panel_opacity), (None, 1.0, 0.65));
        prefs.intensity = f32::NAN;
        prefs.panel_opacity = f32::INFINITY;
        prefs.sanitize();
        assert_eq!((prefs.intensity, prefs.panel_opacity), (0.7, 0.8));

        let saved = BackgroundPrefs { image: Some("/tmp/fondo-1.jpg".into()), dither: false, intensity: 0.4, panel_opacity: 0.9 };
        assert_eq!(serde_json::from_str::<BackgroundPrefs>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }

    #[test]
    fn import_copies_a_reduced_image_and_leaves_the_original() {
        let dir = temp_dir("import");
        let original = dir.join("foto.png");
        flat(3000, 1000, [200, 40, 40]).save(&original).unwrap();
        let bytes = std::fs::read(&original).unwrap();

        let cache = dir.join("cache");
        let (copy, img) = import_image(&original, &cache).unwrap();
        assert_eq!((img.width(), img.height()), (1920, 640));
        assert_eq!(copy.parent(), Some(cache.as_path()));
        assert!(copy.file_name().unwrap().to_str().unwrap().starts_with(PREFIX));
        assert_eq!(std::fs::read(&original).unwrap(), bytes);

        // La copia se vuelve a abrir aunque el original ya no esté.
        std::fs::remove_file(&original).unwrap();
        let again = decode_bounded(&copy).unwrap();
        assert_eq!((again.width(), again.height()), (1920, 640));
        assert!(again.get_pixel(10, 10).0[0] > 180);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn decode_rejects_what_it_cannot_bound() {
        let dir = temp_dir("decode");
        let text = dir.join("notas.png");
        std::fs::write(&text, "no soy una imagen").unwrap();
        assert!(decode_bounded(&text).unwrap_err().contains("PNG, JPEG o WebP"));
        assert!(decode_bounded(&dir.join("falta.png")).unwrap_err().contains("no se pudo abrir"));
        assert!(decode_bounded(&dir).is_err());

        let big = dir.join("grande.png");
        flat(400, 300, [0; 3]).save(&big).unwrap();
        assert!(decode_with(&big, 400 * 300 * 3 - 1).unwrap_err().contains("400 × 300"));
        assert!(decode_with(&big, 400 * 300 * 3).is_ok());
        assert!(import_image(&text, &dir.join("cache")).is_err());
        assert!(!dir.join("cache").exists(), "un import fallido no deja archivos");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sweep_only_removes_old_copies() {
        let dir = temp_dir("sweep");
        for name in ["fondo-1.jpg", "fondo-2.jpg", "fondo-3.tmp", "cursos.json"] {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        sweep(&dir, Some(&dir.join("fondo-2.jpg")));
        let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["cursos.json", "fondo-2.jpg", "fondo-3.tmp"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    // Corre cuadros sin ventana hasta que el hilo de tramado entregue.
    fn frames(ctx: &egui::Context, bg: &mut Background, prefs: &BackgroundPrefs, color: Color32, done: impl Fn(&Background) -> bool) {
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 200.0))),
            ..Default::default()
        };
        for _ in 0..400 {
            let _ = ctx.run(input(), |ctx| bg.paint(ctx, prefs, color, Color32::WHITE));
            if done(bg) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("el fondo no terminó de prepararse");
    }

    #[test]
    fn texture_is_cached_until_something_changes() {
        let ctx = egui::Context::default();
        let path = PathBuf::from("memoria.jpg");
        let mut prefs = BackgroundPrefs { image: Some(path.clone()), ..Default::default() };
        let mut bg = Background::default();
        assert!(!bg.has_image());
        bg.set_source(path, flat(64, 64, [200; 3]));

        let key = |bg: &Background| bg.texture.as_ref().map(|(key, _)| *key);
        let idle = |bg: &Background| bg.texture.is_some() && bg.rendering.is_none();
        frames(&ctx, &mut bg, &prefs, Color32::BLACK, idle);
        let (first, id) = (key(&bg).unwrap(), bg.texture.as_ref().unwrap().1.id());
        assert_eq!((first.w, first.h, first.dither), (160, 100, true));
        assert!(bg.has_image() && bg.error().is_none());

        // Nada cambió (la intensidad es sólo tinte): misma textura, sin trabajo nuevo.
        prefs.intensity = 0.2;
        for _ in 0..5 {
            frames(&ctx, &mut bg, &prefs, Color32::BLACK, |_| true);
            assert!(bg.rendering.is_none());
        }
        assert_eq!(bg.texture.as_ref().unwrap().1.id(), id);

        // Otro color de fondo u otro efecto: se vuelve a tramar.
        frames(&ctx, &mut bg, &prefs, Color32::WHITE, |bg| idle(bg) && key(bg) != Some(first));
        assert_eq!(key(&bg).unwrap().bg, [255; 3]);
        prefs.dither = false;
        frames(&ctx, &mut bg, &prefs, Color32::WHITE, |bg| idle(bg) && !key(bg).unwrap().dither);

        // Sin imagen se suelta todo.
        prefs.image = None;
        frames(&ctx, &mut bg, &prefs, Color32::WHITE, |_| true);
        assert!(!bg.has_image() && bg.source.is_none());
    }

    #[test]
    fn imported_photo_applies_after_the_settings_close() {
        let ctx = egui::Context::default();
        let dir = temp_dir("ajustes-cerrados");
        let original = dir.join("original.png");
        flat(40, 40, [150, 90, 210]).save(&original).unwrap();
        let imported = import_image(&original, &dir).unwrap();
        let copied_path = imported.0.clone();
        let mut background = Background::default();
        background.dir = dir.clone();
        background.tx.send(Msg::Imported(Ok(imported))).unwrap();
        let mut prefs = BackgroundPrefs::default();
        assert!(background.update_prefs(&ctx, &mut prefs));
        assert_eq!(prefs.image, Some(copied_path.clone()));
        assert!(original.exists() && copied_path.exists());
        assert!(!background.update_prefs(&ctx, &mut prefs));
        frames(&ctx, &mut background, &prefs, Color32::BLACK, Background::has_image);
        assert!(background.has_image());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_image_reports_an_error_once() {
        let ctx = egui::Context::default();
        let prefs = BackgroundPrefs { image: Some(temp_dir("falta").join("fondo-0.jpg")), ..Default::default() };
        let mut bg = Background::default();
        frames(&ctx, &mut bg, &prefs, Color32::BLACK, |bg| bg.error().is_some());
        frames(&ctx, &mut bg, &prefs, Color32::BLACK, |_| true);
        assert!(!bg.has_image() && bg.loading.is_none(), "no se reintenta en cada cuadro");
    }
}
