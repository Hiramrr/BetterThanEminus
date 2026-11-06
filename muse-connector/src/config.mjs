// Configuración del servidor desde variables de entorno (Vercel u otro hosting).
//
// Credenciales de Eminus (una de dos formas):
//   EMINUS_USER + EMINUS_PASSWORD      (más fácil de llenar en el formulario de Vercel)
//   EMINUS_CREDENTIALS = {"user","password"}
// API key de Muse (una de dos formas):
//   MUSE_API_KEY_SHA256  hash de la llave (lo que sube `npm run instalar`)
//   MUSE_API_KEY         la llave misma, de al menos 32 caracteres aleatorios
import { createHash } from "node:crypto";

export const MIN_KEY_LENGTH = 32;

export function serverConfig(env) {
  const problems = [];
  let apiKeyHash = String(env.MUSE_API_KEY_SHA256 || "").trim().toLowerCase();
  if (apiKeyHash && !/^[0-9a-f]{64}$/.test(apiKeyHash)) {
    problems.push("MUSE_API_KEY_SHA256 no es un SHA-256 válido");
    apiKeyHash = "";
  }
  if (!apiKeyHash && env.MUSE_API_KEY) {
    const key = String(env.MUSE_API_KEY).trim();
    if (key.length < MIN_KEY_LENGTH || /^(.)\1+$/.test(key)) problems.push(`MUSE_API_KEY debe tener al menos ${MIN_KEY_LENGTH} caracteres aleatorios`);
    else apiKeyHash = createHash("sha256").update(key, "utf8").digest("hex");
  }
  if (!apiKeyHash && !problems.length) problems.push("falta MUSE_API_KEY");

  let credentials = null;
  if (env.EMINUS_USER && env.EMINUS_PASSWORD) {
    credentials = { user: String(env.EMINUS_USER).trim(), password: String(env.EMINUS_PASSWORD) };
  } else if (env.EMINUS_CREDENTIALS) {
    try {
      const c = JSON.parse(env.EMINUS_CREDENTIALS);
      if (c?.user && c?.password) credentials = { user: String(c.user).trim(), password: String(c.password) };
    } catch {}
  }
  return {
    apiKeyHash,
    problems,
    getCredentials: async () => {
      if (!credentials) throw new Error("no hay credenciales de Eminus configuradas en el servidor (EMINUS_USER y EMINUS_PASSWORD)");
      return credentials;
    }
  };
}
