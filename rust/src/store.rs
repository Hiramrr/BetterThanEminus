// Lo que vive en disco: la sesión, las preferencias y la caché que deja
// abrir la app al instante. Todo queda sólo para el usuario (0600).

use crate::model::{Activity, Acts, Content, ContentItem, Course};
use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use base64::Engine;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default())
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("eminus")
}

fn cache_dir() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) => PathBuf::from(dir).join("eminus"),
        None if cfg!(target_os = "macos") => home().join("Library/Caches/eminus"),
        None => home().join(".cache/eminus"),
    }
}

// Escribe a un temporal y renombra: nunca queda un archivo a medias.
fn write_private(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    write_private_with(path, |writer| writer.write_all(bytes))
}

fn write_private_with(path: &Path, write: impl FnOnce(&mut BufWriter<std::fs::File>) -> std::io::Result<()>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut writer = BufWriter::with_capacity(64 * 1024, options.open(&tmp)?);
    write(&mut writer)?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(&tmp, path)
}

fn read_json<T: DeserializeOwned>(path: PathBuf) -> Option<T> {
    serde_json::from_reader(BufReader::new(std::fs::File::open(path).ok()?)).ok()
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) {
    let _ = write_json_at(&path, value);
}

fn write_json_at<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    write_private_with(path, |writer| serde_json::to_writer(writer, value).map_err(std::io::Error::other))
}

// ───────────── sesión ─────────────

// Acepta lo que se suele copiar: con comillas o con «Bearer » delante.
pub fn clean_token(raw: &str) -> Option<String> {
    let token = raw.trim().trim_matches(['"', '\'']).trim();
    let token = token.strip_prefix("Bearer ").unwrap_or(token).trim();
    (token.split('.').count() == 3 && !token.contains(char::is_whitespace) && claims(token).is_some()).then(|| token.to_string())
}

fn claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?.trim_end_matches('=');
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// La caché es por cuenta: el id sale del token de Eminus.
pub fn account_of(token: &str) -> String {
    let Some(claims) = claims(token) else { return String::new() };
    ["nameid", "unique_name", "sub", "email", "idPersona", "matricula"]
        .iter()
        .filter_map(|k| claims.get(k))
        .map(|v| crate::model::string(Some(v)))
        .find(|v| !v.is_empty())
        .unwrap_or_default()
}

pub fn expiry_of(token: &str) -> i64 {
    claims(token).map_or(0, |c| crate::model::int(c.get("exp")))
}

pub fn load_token() -> String {
    std::env::var("EMINUS_TOKEN")
        .ok()
        .or_else(|| std::fs::read_to_string(config_dir().join("sesion")).ok())
        .and_then(|raw| clean_token(&raw))
        .unwrap_or_default()
}

pub fn save_token(token: &str) {
    let path = config_dir().join("sesion");
    if token.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        let _ = write_private(&path, token.as_bytes());
    }
}

// ───────────── login con contraseña ─────────────

// Eminus no recibe la contraseña tal cual: su formulario la cifra con una
// llave de la propia aplicación web. La app la lee de la página de Eminus
// al entrar; aquí sólo se puede fijar a mano (EMINUS_LOGIN_KEY o
// `llave_login` en preferencias) por si esa lectura deja de funcionar.
pub fn login_key() -> Option<String> {
    std::env::var("EMINUS_LOGIN_KEY").ok().or_else(|| load_prefs().llave_login).map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

// Mismo formato que el formulario: base64(sal ‖ iv ‖ AES-256-CBC), con la
// llave derivada por PBKDF2-HMAC-SHA1 (100 vueltas) de la llave de la app.
pub fn encrypt_password(password: &str, app_key: &str) -> Option<String> {
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random).ok()?;
    let (salt, iv) = random.split_at(16);
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(app_key.as_bytes(), salt, 100, &mut key);
    let cipher = cbc::Encryptor::<aes::Aes256>::new_from_slices(&key, iv).ok()?;
    let mut out = random.to_vec();
    out.extend(cipher.encrypt_padded_vec_mut::<Pkcs7>(password.as_bytes()));
    Some(base64::engine::general_purpose::STANDARD.encode(out))
}

// ───────────── perfiles ─────────────

// Hace aquí lo que el almacén cifrado de la extensión. En disco sólo va
// quién tiene perfil; la contraseña vive en el Llavero del sistema.
#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub user: String,
    pub name: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Profiles {
    // Con quién está abierta la sesión; vacío si la cerró el usuario.
    #[serde(default)]
    pub ultimo: String,
    #[serde(default)]
    pub perfiles: Vec<Profile>,
}

impl Profiles {
    pub fn has(&self, user: &str) -> bool {
        !user.is_empty() && self.perfiles.iter().any(|p| p.user == user)
    }

    pub fn save(&self) {
        write_json(config_dir().join("perfiles.json"), self);
    }
}

pub fn load_profiles() -> Profiles {
    read_json(config_dir().join("perfiles.json")).unwrap_or_default()
}

fn secret(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("eminus", user).map_err(|_| "no se pudo abrir el llavero".to_string())
}

pub fn secret_get(user: &str) -> Result<String, String> {
    secret(user)?.get_password().map_err(|err| match err {
        keyring::Error::NoEntry => "este perfil ya no tiene contraseña en el llavero".to_string(),
        _ => "el llavero no dio acceso a la contraseña".to_string(),
    })
}

pub fn secret_set(user: &str, password: &str) -> Result<(), String> {
    secret(user)?.set_password(password).map_err(|_| "no se pudo guardar en el llavero".to_string())
}

pub fn secret_delete(user: &str) {
    if let Ok(entry) = secret(user) {
        let _ = entry.delete_credential();
    }
}

// Nombre para mostrar, si el token lo trae.
pub fn name_of(token: &str) -> String {
    let Some(claims) = claims(token) else { return String::new() };
    ["name", "given_name", "nombre", "nombreCompleto"].iter().filter_map(|k| claims.get(k).and_then(Value::as_str)).map(str::trim).find(|v| !v.is_empty()).unwrap_or_default().to_string()
}

// ───────────── preferencias ─────────────

#[derive(Default, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub tema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llave_login: Option<String>,
    #[serde(default)]
    pub appearance: crate::personalization::Appearance,
    #[serde(default)]
    pub background: crate::background::BackgroundPrefs,
}

pub fn load_prefs() -> Prefs {
    read_json(config_dir().join("preferencias.json")).unwrap_or_default()
}

pub fn save_theme(name: &str) {
    let _ = update_prefs_at(&config_dir().join("preferencias.json"), |prefs| prefs.tema = name.to_string());
}

pub fn save_appearance(appearance: &crate::personalization::Appearance) -> std::io::Result<()> {
    update_prefs_at(&config_dir().join("preferencias.json"), |prefs| prefs.appearance = appearance.clone())
}

pub fn save_background(background: &crate::background::BackgroundPrefs) -> std::io::Result<()> {
    update_prefs_at(&config_dir().join("preferencias.json"), |prefs| prefs.background = background.clone())
}

fn update_prefs_at(path: &Path, update: impl FnOnce(&mut Prefs)) -> std::io::Result<()> {
    let mut prefs: Prefs = match std::fs::File::open(path) {
        Ok(file) => serde_json::from_reader(BufReader::new(file)).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Prefs::default(),
        Err(e) => return Err(e),
    };
    update(&mut prefs);
    prefs.appearance.sanitize();
    prefs.background.sanitize();
    write_json_at(path, &prefs)
}

// Borradores locales por cuenta y actividad, fuera de la caché descartable.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliveryDraft {
    pub text: String,
    pub files: Vec<PathBuf>,
    pub uploaded_before: Vec<String>,
    pub recovering: bool,
}

type Drafts = std::collections::HashMap<String, std::collections::HashMap<String, DeliveryDraft>>;

pub fn load_draft(account: &str, key: &str) -> Option<DeliveryDraft> {
    load_draft_at(&config_dir().join("borradores.json"), account, key)
}

fn load_draft_at(path: &Path, account: &str, key: &str) -> Option<DeliveryDraft> {
    if account.is_empty() {
        return None;
    }
    let mut drafts: Drafts = read_json(path.to_path_buf())?;
    drafts.get_mut(account)?.remove(key)
}

pub fn save_draft(account: &str, key: &str, draft: &DeliveryDraft) -> std::io::Result<()> {
    save_draft_at(&config_dir().join("borradores.json"), account, key, draft)
}

fn save_draft_at(path: &Path, account: &str, key: &str, draft: &DeliveryDraft) -> std::io::Result<()> {
    if account.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "la sesión no identifica una cuenta para guardar el borrador"));
    }
    let mut drafts: Drafts = match std::fs::File::open(path) {
        Ok(file) => serde_json::from_reader(BufReader::new(file)).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Drafts::new(),
        Err(e) => return Err(e),
    };
    if draft.text.is_empty() && draft.files.is_empty() {
        if let Some(account_drafts) = drafts.get_mut(account) {
            account_drafts.remove(key);
            if account_drafts.is_empty() {
                drafts.remove(account);
            }
        }
    } else {
        drafts.entry(account.to_string()).or_default().insert(key.to_string(), draft.clone());
    }
    write_json_at(path, &drafts)
}

// ───────────── caché ─────────────

#[derive(Serialize)]
struct CoursesOut<'a> {
    account: &'a str,
    ts: i64,
    courses: &'a [Course],
    acts: &'a Acts,
}

#[derive(Deserialize)]
struct CoursesIn {
    account: String,
    ts: i64,
    courses: Vec<Course>,
    acts: std::collections::HashMap<String, Vec<Arc<Activity>>>,
}

pub fn save_courses(account: &str, ts: i64, courses: &[Course], acts: &Acts) {
    if !account.is_empty() {
        write_json(cache_dir().join("cursos.json"), &CoursesOut { account, ts, courses, acts });
    }
}

pub fn load_courses(account: &str, now: i64) -> Option<(i64, Vec<Course>, Acts)> {
    let mut saved: CoursesIn = read_json(cache_dir().join("cursos.json"))?;
    if account.is_empty() || saved.account != account {
        return None;
    }
    saved.courses.iter_mut().for_each(|c| c.current = c.end == 0 || c.end >= now);
    // Serde crea un Arc por actividad; se vuelve a compartir el nombre del curso.
    for course in &saved.courses {
        if let Some(list) = saved.acts.get_mut(&course.id) {
            for act in list {
                if let Some(act) = Arc::get_mut(act) {
                    act.course_name = course.name.clone();
                }
            }
        }
    }
    crate::model::sort_courses(&mut saved.courses);
    Some((saved.ts, saved.courses, saved.acts))
}

#[derive(Serialize)]
struct ContentOut<'a> {
    account: &'a str,
    ts: i64,
    content: &'a Content,
}

#[derive(Deserialize)]
struct ContentIn {
    account: String,
    ts: i64,
    content: std::collections::HashMap<String, Vec<Arc<ContentItem>>>,
}

pub fn save_content(account: &str, ts: i64, content: &Content) {
    if !account.is_empty() {
        write_json(cache_dir().join("contenido.json"), &ContentOut { account, ts, content });
    }
}

pub fn load_content(account: &str) -> Option<(i64, Content)> {
    let saved: ContentIn = read_json(cache_dir().join("contenido.json"))?;
    (!account.is_empty() && saved.account == account).then_some((saved.ts, saved.content))
}

pub fn clear_cache() {
    let _ = std::fs::remove_file(cache_dir().join("cursos.json"));
    let _ = std::fs::remove_file(cache_dir().join("contenido.json"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_cache_keeps_the_existing_format_and_atomic_writes() {
        let dir = std::env::temp_dir().join(format!("eminus-cache-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cursos.json");
        let courses = crate::model::courses_from(&[serde_json::json!({"idCurso":7,"nombre":"Redes","fechaTerminoEpoch":100})], 50);
        let acts = Acts::from([("7".into(), crate::model::activities_from(&[serde_json::json!({"idActividad":3,"titulo":"Práctica"})], &courses[0]))]);
        let data = CoursesOut { account: "fixture", ts: 50, courses: &courses, acts: &acts };
        // El lector acepta cachés escritos por la versión anterior.
        let old = serde_json::to_vec(&data).unwrap();
        std::fs::write(&path, &old).unwrap();
        let loaded: CoursesIn = read_json(path.clone()).unwrap();
        assert_eq!(loaded.acts["7"][0].title.as_ref(), "Práctica");
        write_json_at(&path, &data).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), old);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        struct Fails;
        impl Serialize for Fails {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("error de prueba"))
            }
        }
        assert!(write_json_at(&path, &Fails).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), old);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tokens() {
        let b64 = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        let token = format!("{}.{}.firma", b64("{}"), b64(r#"{"nameid":"zs1","exp":1900000000}"#));
        assert_eq!(clean_token(&format!("  \"Bearer {token}\"\n")).as_deref(), Some(token.as_str()));
        assert_eq!(account_of(&token), "zs1");
        assert_eq!(expiry_of(&token), 1_900_000_000);
        assert!(clean_token("hola").is_none());
        assert!(clean_token("a.b.c").is_none());
    }

    #[test]
    fn password_shape() {
        let out = base64::engine::general_purpose::STANDARD.decode(encrypt_password("secreto", "llave").unwrap()).unwrap();
        assert_eq!(out.len(), 16 + 16 + 16);
        assert_ne!(encrypt_password("secreto", "llave"), encrypt_password("secreto", "llave"));
    }

    #[test]
    fn drafts_survive_restart_and_stay_with_their_account() {
        let dir = std::env::temp_dir().join(format!("eminus-drafts-{}", std::process::id()));
        let path = dir.join("borradores.json");
        let a = DeliveryDraft { text: "Primera línea\nSegunda línea".into(), files: vec![PathBuf::from("/tmp/tarea.pdf")], uploaded_before: vec!["1".into()], recovering: true };
        let b = DeliveryDraft { text: "Otra cuenta".into(), ..Default::default() };
        save_draft_at(&path, "cuenta-a", "1:2", &a).unwrap();
        save_draft_at(&path, "cuenta-b", "1:2", &b).unwrap();
        assert!(load_draft_at(&path, "", "1:2").is_none());
        assert!(load_draft_at(&path, "cuenta-c", "1:2").is_none());
        assert_eq!(load_draft_at(&path, "cuenta-a", "1:2").unwrap().text, a.text);
        assert_eq!(load_draft_at(&path, "cuenta-a", "1:2").unwrap().files, a.files);
        assert!(load_draft_at(&path, "cuenta-a", "1:2").unwrap().recovering);
        assert_eq!(load_draft_at(&path, "cuenta-a", "1:2").unwrap().uploaded_before, ["1"]);
        save_draft_at(&path, "cuenta-a", "1:2", &DeliveryDraft::default()).unwrap();
        assert!(load_draft_at(&path, "cuenta-a", "1:2").is_none());
        assert_eq!(load_draft_at(&path, "cuenta-b", "1:2").unwrap().text, b.text);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::write(&path, b"broken JSON").unwrap();
        assert!(save_draft_at(&path, "cuenta-a", "1:2", &a).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken JSON");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn appearance_and_background_keep_theme_and_login_preferences() {
        let dir = std::env::temp_dir().join(format!("eminus-appearance-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preferencias.json");
        std::fs::write(&path, br#"{"tema":"paper","llave_login":"fixture"}"#).unwrap();
        let old: Prefs = read_json(path.clone()).unwrap();
        assert_eq!(old.appearance.font_size, 16.0);
        assert!(old.background.image.is_none());
        update_prefs_at(&path, |p| p.appearance.font_size = 18.0).unwrap();
        update_prefs_at(&path, |p| p.background.image = Some(dir.join("fondo.jpg"))).unwrap();
        update_prefs_at(&path, |p| p.tema = "sakura".into()).unwrap();
        let saved: Prefs = read_json(path.clone()).unwrap();
        assert_eq!(saved.tema, "sakura");
        assert_eq!(saved.llave_login.as_deref(), Some("fixture"));
        assert_eq!(saved.appearance.font_size, 18.0);
        assert_eq!(saved.background.image, Some(dir.join("fondo.jpg")));
        std::fs::write(&path, b"broken JSON").unwrap();
        assert!(update_prefs_at(&path, |p| p.tema = "paper".into()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken JSON");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

// Comprobaciones a mano (cargo test -- --ignored --nocapture): la llave se
// puede leer de Eminus y el cifrado se puede abrir con otra herramienta.
#[cfg(test)]
#[test]
#[ignore]
fn llave_y_cifrado() {
    let key = crate::api::Client::new("").login_key().map_err(|e| e.message().to_string());
    println!("llave leída de eminus: {:?}", key.map(|k| k.len()));
    println!("cifrado de prueba: {}", encrypt_password("secreto ñ", "llave").unwrap());
}
