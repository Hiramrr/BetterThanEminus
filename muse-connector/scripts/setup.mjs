#!/usr/bin/env node
// Configuración local, interactiva. Nada de lo que escribes queda en disco en claro
// ni en el historial del shell.
//
//   npm run setup                    credenciales de Eminus + API key de Muse
//   npm run setup -- --credentials   sólo credenciales de Eminus
//   npm run setup -- --api-key       sólo genera/rota la API key de Muse
//   npm run setup -- --api-key --no-keychain
//        genera la llave y muestra su SHA-256 para ponerlo en MUSE_API_KEY_SHA256
//        (servidores sin Llavero). La llave sólo se muestra esta vez.
import { eminusFetch } from "../src/http.mjs";
import { randomBytes } from "node:crypto";
import readline from "node:readline";
import { EminusClient } from "../src/eminus.mjs";
import { API_KEY_ACCOUNT, CREDENTIALS_ACCOUNT, GLASS_ACCOUNT, GLASS_SERVICE, SERVICE, keychainRead, keychainWrite } from "../src/credentials.mjs";
import { sha256 } from "../src/node-server.mjs";

const args = new Set(process.argv.slice(2));
const onlyCreds = args.has("--credentials");
const onlyKey = args.has("--api-key");
const noKeychain = args.has("--no-keychain");

function ask(question, { hidden = false } = {}) {
  return new Promise((resolve) => {
    const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: true });
    if (hidden) {
      rl._writeToOutput = (s) => { if (!rl.muted) rl.output.write(s); };
    }
    rl.question(question, (answer) => { rl.close(); if (hidden) process.stdout.write("\n"); resolve(answer); });
    if (hidden) rl.muted = true;
  });
}

async function setupCredentials() {
  if (process.platform !== "darwin") {
    console.log("Fuera de macOS usa EMINUS_CREDENTIALS_FILE (JSON {user,password}, chmod 600). Ver CONNECTOR.md.");
    return;
  }
  const glass = await keychainRead(GLASS_SERVICE, GLASS_ACCOUNT);
  if (glass) {
    const a = (await ask("EminusGlass ya tiene tu sesión en el Llavero. ¿Usarla en vez de escribir la contraseña? [S/n] ")).trim().toLowerCase();
    if (a === "" || a === "s" || a === "si" || a === "y") {
      console.log("Listo: el conector leerá las credenciales de EminusGlass (no se copian).");
      return;
    }
  }
  const user = (await ask("Usuario de Eminus (p. ej. zs12345678): ")).trim();
  const password = await ask("Contraseña (no se muestra): ", { hidden: true });
  if (!user || !password.trim()) throw new Error("faltan datos");
  process.stdout.write("Comprobando con Eminus… ");
  const client = new EminusClient({ fetch: eminusFetch, getCredentials: async () => ({ user, password }) });
  await client.validToken();
  console.log("aceptadas.");
  await keychainWrite(SERVICE, CREDENTIALS_ACCOUNT, JSON.stringify({ user, password }));
  console.log(`Guardadas en el Llavero (servicio ${SERVICE}, cuenta ${CREDENTIALS_ACCOUNT}).`);
}

async function setupApiKey() {
  const key = "mec_" + randomBytes(32).toString("base64url");
  const hash = sha256(key);
  if (noKeychain || process.platform !== "darwin") {
    console.log("\nMUSE_API_KEY_SHA256=" + hash);
  } else {
    await keychainWrite(SERVICE, API_KEY_ACCOUNT, hash);
    console.log(`\nHash guardado en el Llavero (servicio ${SERVICE}, cuenta ${API_KEY_ACCOUNT}). Reinicia el servidor si ya corría.`);
  }
  console.log("\nAPI key para Muse (cópiala ahora; no se vuelve a mostrar ni se guarda en claro):\n");
  console.log("  " + key + "\n");
  console.log("Pégala sólo en el formulario de credenciales del conector en Muse, nunca en el chat.");
}

try {
  if (!onlyKey) await setupCredentials();
  if (!onlyCreds) await setupApiKey();
} catch (err) {
  console.error("\nNo se completó: " + err.message);
  process.exit(1);
}
