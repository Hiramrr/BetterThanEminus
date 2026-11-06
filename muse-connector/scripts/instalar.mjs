#!/usr/bin/env node
// Instala (o actualiza) TU conector de Eminus en TU cuenta de Vercel. Funciona en
// Windows, macOS y Linux; solo necesita Node 22+.
//
//   npm run instalar                     primera vez, o para volver a desplegar
//   npm run instalar -- --credentials    cambiaste tu contraseña de Eminus
//   npm run instalar -- --rotate-key     generar una API key nueva para Muse
//
// Tu contraseña se escribe aquí, sin eco; se comprueba con Eminus y se envía por stdin
// a Vercel como variable cifrada «sensitive». No pasa por argumentos, archivos ni pantalla.
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import readline from "node:readline";
import { fileURLToPath } from "node:url";
import { sha256 } from "../src/api.mjs";
import { keychainRead, keychainWrite, loadEminusCredentials, SERVICE } from "../src/credentials.mjs";
import { EminusClient } from "../src/eminus.mjs";
import { eminusFetch } from "../src/http.mjs";

const args = new Set(process.argv.slice(2));
const cwd = fileURLToPath(new URL("..", import.meta.url));
const isMac = process.platform === "darwin";
const isWin = process.platform === "win32";
const KEY_ACCOUNT = "muse-api-key";
const VERCEL_VERSION = "53.1.1";

// ───────────── utilidades ─────────────

const hasGlobalVercel = spawnSync(isWin ? "where" : "which", ["vercel"], { stdio: "ignore", shell: isWin }).status === 0;
const vercelCmd = hasGlobalVercel ? ["vercel"] : ["npx", "-y", "vercel@" + VERCEL_VERSION];

function vercel(argv, { stdin, inherit = false } = {}) {
  return new Promise((resolve, reject) => {
    const [cmd, ...pre] = vercelCmd;
    const child = spawn(cmd, [...pre, ...argv], { cwd, shell: isWin, stdio: inherit ? "inherit" : ["pipe", "pipe", "pipe"] });
    let out = "";
    if (!inherit) {
      child.stdout.on("data", (d) => (out += d));
      child.stderr.on("data", (d) => (out += d));
      child.stdin.end(stdin ?? "");
    }
    child.on("error", reject);
    child.on("close", (code) => (code === 0 ? resolve(out) : reject(Object.assign(new Error(`vercel ${argv[0]} falló`), { out }))));
  });
}

function ask(question, { hidden = false, fallback = "" } = {}) {
  return new Promise((resolve, reject) => {
    if (!process.stdin.isTTY) return reject(new Error("Ejecuta el instalador en una terminal interactiva."));
    const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: true });
    if (hidden) rl._writeToOutput = (s) => { if (!rl.muted) rl.output.write(s); };
    rl.question(question, (a) => { rl.close(); if (hidden) process.stdout.write("\n"); resolve((hidden ? a : a.trim()) || fallback); });
    if (hidden) rl.muted = true;
  });
}

const yes = (a) => ["", "s", "si", "sí", "y", "yes"].includes(a.toLowerCase());
const step = (n, text) => console.log(`\n${n}. ${text}`);

async function setSecret(name, value) {
  await vercel(["env", "rm", name, "production", "--yes"]).catch(() => {});
  await vercel(["env", "add", name, "production", "--sensitive"], { stdin: value });
}

async function removeVar(name) {
  await vercel(["env", "rm", name, "production", "--yes"]).catch(() => {});
}

// ───────────── pasos ─────────────

console.log("Conector Eminus → Muse · instalador");

step(1, "Cuenta de Vercel");
try {
  const who = (await vercel(["whoami"])).trim().split("\n").pop();
  console.log("   sesión iniciada como " + who);
} catch {
  console.log("   Se abrirá el navegador para entrar a Vercel (crea una cuenta gratuita si no tienes).");
  await vercel(["login"], { inherit: true });
}

step(2, "Proyecto");
if (!existsSync(new URL("../.vercel/project.json", import.meta.url))) {
  const name = (await ask("   Nombre del proyecto [muse-eminus]: ", { fallback: "muse-eminus" })).toLowerCase().replace(/[^a-z0-9-]/g, "-");
  await vercel(["link", "--yes", "--project", name]);
  console.log("   proyecto enlazado: " + name);
} else {
  console.log("   ya enlazado (.vercel/project.json)");
}

const envList = await vercel(["env", "ls", "production"]).catch(() => "");
const has = (n) => new RegExp("(^|\\s)" + n + "(\\s|$)", "m").test(envList);

step(3, "Credenciales de Eminus");
if (args.has("--credentials") || !(has("EMINUS_USER") && has("EMINUS_PASSWORD")) && !has("EMINUS_CREDENTIALS")) {
  let creds = null;
  if (isMac) {
    const saved = await loadEminusCredentials().catch(() => null);
    if (saved && yes(await ask(`   Hay credenciales en tu Llavero (usuario ****${saved.user.slice(-3)}). ¿Usarlas? [S/n] `))) creds = saved;
  }
  if (!creds) {
    const user = await ask("   Usuario de Eminus (tu matrícula, p. ej. zs12345678): ");
    const password = await ask("   Contraseña de Eminus (no se muestra): ", { hidden: true });
    if (!user || !password) { console.error("   faltan datos"); process.exit(1); }
    creds = { user, password };
  }
  process.stdout.write("   comprobando con Eminus… ");
  try {
    await new EminusClient({ fetch: eminusFetch, getCredentials: async () => creds }).validToken();
  } catch (err) {
    console.error("no: " + err.message);
    process.exit(1);
  }
  console.log("aceptadas");
  await setSecret("EMINUS_USER", creds.user);
  await setSecret("EMINUS_PASSWORD", creds.password);
  await removeVar("EMINUS_CREDENTIALS");
  console.log("   guardadas en Vercel como variables cifradas (sensitive)");
} else {
  console.log("   ya configuradas (usa --credentials para cambiarlas)");
}

step(4, "API key para Muse");
let key = isMac ? await keychainRead(SERVICE, KEY_ACCOUNT) : null;
let newKey = false;
if (args.has("--rotate-key") || !(has("MUSE_API_KEY_SHA256") || has("MUSE_API_KEY"))) {
  key = "mec_" + randomBytes(32).toString("base64url");
  newKey = true;
  await setSecret("MUSE_API_KEY_SHA256", sha256(key));
  await removeVar("MUSE_API_KEY");
  if (isMac) await keychainWrite(SERVICE, KEY_ACCOUNT, key);
  console.log("   llave nueva generada; en Vercel solo queda su huella (SHA-256)");
} else {
  console.log("   ya configurada (usa --rotate-key para cambiarla)");
}

step(5, "Despliegue");
const out = await vercel(["deploy", "--prod", "--yes"]);
const urls = [...new Set([...out.matchAll(/https:\/\/[a-z0-9.-]+\.vercel\.app/g)].map((m) => m[0]))];
// El alias estable es el más corto (sin el sufijo de la versión ni del equipo).
const base = urls.sort((a, b) => a.length - b.length)[0];
if (!base) { console.error("   no encontré la URL del despliegue:\n" + out.slice(-800)); process.exit(1); }
console.log("   " + base);

step(6, "Prueba");
const health = await fetch(base + "/health").then((r) => r.status).catch(() => 0);
console.log("   GET /health → " + health);
let tasksOk = false;
if (key) {
  const r = await fetch(base + "/tasks?limit=1", { headers: { Authorization: "Bearer " + key } }).catch(() => null);
  const body = r ? await r.json().catch(() => null) : null;
  tasksOk = r?.status === 200 && Array.isArray(body?.data);
  console.log("   GET /tasks → " + (r?.status ?? "sin respuesta") + (body?.total !== undefined ? ` (${body.total} tareas pendientes)` : body?.error ? " " + body.error.code : ""));
}

const doc = await fetch(base + "/docs/CONNECTOR.md").catch(() => null);
const docText = doc?.ok ? await doc.text() : "";
const docsOk = docText.includes(base) && !docText.includes("{{BASE_URL}}");
console.log("   GET /docs/CONNECTOR.md → " + (docsOk ? "URL correcta" : "falló"));
if (health !== 200 || !docsOk || (key && !tasksOk)) {
  console.error("El despliegue no pasó la comprobación. Revisa la configuración de Vercel antes de conectarlo a Muse.");
  process.exitCode = 1;
}

console.log(`
──────────────────────────────────────────────
${process.exitCode ? "Revisa los errores anteriores." : "Listo."} Para conectarlo en Muse (en el formulario de conectores, NO en el chat):
  Tipo:      API key
  Base URL:  ${base}
  Cabecera:  Authorization: Bearer <tu API key>
Instrucciones para pasarle a Muse:  ${base}/docs`);
if (newKey && !isMac) {
  console.log(`
Tu API key (cópiala ahora; no se vuelve a mostrar ni queda guardada en este equipo):

  ${key}
`);
} else if (isMac) {
  console.log(`
Copiar tu API key al portapapeles sin mostrarla:
  security find-generic-password -s ${SERVICE} -a ${KEY_ACCOUNT} -w | pbcopy`);
} else {
  console.log("\nSi perdiste tu API key, genera otra con: npm run instalar -- --rotate-key");
}
