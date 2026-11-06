// Dónde viven los secretos. Ninguno está en el código, en .env ni en logs.
//
// Credenciales de Eminus (en este orden):
//   1. EMINUS_CREDENTIALS_FILE: ruta a un JSON {"user","password"} con permisos 600
//      (para servidores Linux / Docker secrets).
//   2. Llavero de macOS, servicio "mx.hiram.muse-connector", cuenta "eminus-credentials"
//      (lo crea `npm run setup`).
//   3. Llavero de la app de Swift EminusGlass para Mac, servicio
//      "mx.hiram.EminusGlass.Mac.session", cuenta "credentials" (existe si activaste
//      «Mantener sesión iniciada»). macOS pedirá permiso la primera vez.
//
// Llave de Muse: sólo se guarda su SHA-256 (MUSE_API_KEY_SHA256 o Llavero,
// cuenta "api-key-sha256"). El servidor nunca conoce la llave en claro.

import { execFile, spawn } from "node:child_process";
import { readFile, stat } from "node:fs/promises";
import { promisify } from "node:util";

const run = promisify(execFile);

export const SERVICE = "mx.hiram.muse-connector";
export const CREDENTIALS_ACCOUNT = "eminus-credentials";
export const API_KEY_ACCOUNT = "api-key-sha256";
export const GLASS_SERVICE = "mx.hiram.EminusGlass.Mac.session";
export const GLASS_ACCOUNT = "credentials";

export async function keychainRead(service, account) {
  if (process.platform !== "darwin") return null;
  try {
    const { stdout } = await run("security", ["find-generic-password", "-s", service, "-a", account, "-w"], { timeout: 120000 });
    let value = stdout.replace(/\n$/, "");
    // `security -w` imprime en hexadecimal los datos que no son texto ASCII.
    if (/^([0-9a-f]{2})+$/i.test(value) && value.length > 8) {
      const decoded = Buffer.from(value, "hex").toString("utf8");
      if (decoded.startsWith("{")) value = decoded;
    }
    return value;
  } catch {
    return null;
  }
}

/** Escribe por stdin de `security -i` con -X (hex): el secreto nunca va en argv. */
export function keychainWrite(service, account, value) {
  return new Promise((resolve, reject) => {
    const safe = (s) => /^[A-Za-z0-9._-]+$/.test(s);
    if (!safe(service) || !safe(account)) return reject(new Error("nombre de llavero inválido"));
    const hex = Buffer.from(value, "utf8").toString("hex");
    const child = spawn("security", ["-i"], { stdio: ["pipe", "ignore", "pipe"] });
    let err = "";
    child.stderr.on("data", (d) => (err += d));
    child.on("close", (code) => (code === 0 && !err.trim() ? resolve() : reject(new Error("el Llavero no guardó el dato"))));
    child.stdin.end(`add-generic-password -U -s ${service} -a ${account} -l "Muse connector (Eminus)" -X ${hex}\n`);
  });
}

function parseCredentials(text) {
  try {
    const c = JSON.parse(text);
    if (typeof c?.user === "string" && c.user.trim() && typeof c?.password === "string" && c.password.trim()) {
      return { user: c.user.trim(), password: c.password };
    }
  } catch {}
  return null;
}

/** Devuelve {user, password, source}. Lanza un error sin datos sensibles si no hay. */
export async function loadEminusCredentials() {
  const file = process.env.EMINUS_CREDENTIALS_FILE;
  if (file) {
    const info = await stat(file);
    if (process.platform !== "win32" && (info.mode & 0o077) !== 0) {
      throw new Error("EMINUS_CREDENTIALS_FILE debe tener permisos 600");
    }
    const c = parseCredentials(await readFile(file, "utf8"));
    if (c) return { ...c, source: "file" };
    throw new Error("EMINUS_CREDENTIALS_FILE no tiene {user, password}");
  }
  const own = parseCredentials(await keychainRead(SERVICE, CREDENTIALS_ACCOUNT));
  if (own) return { ...own, source: "keychain" };
  const glass = parseCredentials(await keychainRead(GLASS_SERVICE, GLASS_ACCOUNT));
  if (glass) return { ...glass, source: "eminus-glass" };
  throw new Error("no hay credenciales de Eminus: ejecuta `npm run setup`");
}

export async function loadApiKeyHash() {
  const env = process.env.MUSE_API_KEY_SHA256?.trim();
  const value = env || (await keychainRead(SERVICE, API_KEY_ACCOUNT))?.trim();
  return value && /^[0-9a-f]{64}$/i.test(value) ? value.toLowerCase() : null;
}
