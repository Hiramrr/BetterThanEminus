// Lo que vive en disco: la sesión, las preferencias y la caché que deja
// abrir la app al instante. Todo queda sólo para el usuario (0600).

use crate::model::{Activity, Acts, Content, ContentItem, Course};
use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use base64::Engine;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
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
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(&tmp)?.write_all(bytes)?;
    std::fs::rename(&tmp, path)
}

fn read_json<T: DeserializeOwned>(path: PathBuf) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) {
    if let Ok(bytes) = serde_json::to_vec(value) {
        let _ = write_private(&path, &bytes);
    }
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
}

pub fn load_prefs() -> Prefs {
    read_json(config_dir().join("preferencias.json")).unwrap_or_default()
}

pub fn save_theme(name: &str) {
    let mut prefs = load_prefs();
    prefs.tema = name.to_string();
    write_json(config_dir().join("preferencias.json"), &prefs);
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
