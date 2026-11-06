// Función de Vercel (runtime Node): el conector siempre encendido.
// Variables de entorno: ver src/config.mjs. vercel.json dirige todas las rutas hacia aquí.
import { readFileSync } from "node:fs";
import { createHandler } from "../src/api.mjs";
import { serverConfig } from "../src/config.mjs";
import { EminusClient } from "../src/eminus.mjs";
import { eminusFetch } from "../src/http.mjs";

const docs = Object.fromEntries(["CONNECTOR", "AVISOS", "ESTUDIO"].map((n) => {
  try { return [n, readFileSync(new URL(`../docs/${n}.md`, import.meta.url), "utf8")]; } catch { return [n, null]; }
}).filter(([, v]) => v));

let handler = null;
let problems = [];

function build() {
  if (handler) return handler;
  const config = serverConfig(process.env);
  problems = config.problems;
  if (!config.apiKeyHash) return null;
  handler = createHandler({
    client: new EminusClient({ fetch: eminusFetch, getCredentials: config.getCredentials }),
    apiKeyHash: config.apiKeyHash,
    cacheTtlSeconds: Number(process.env.CACHE_TTL_SECONDS || 300),
    docs
  });
  return handler;
}

async function serve(request) {
  // La ruta de Vercel pasa la dirección original en ?__path=…
  const url = new URL(request.url);
  const original = url.searchParams.get("__path");
  if (original !== null) {
    url.searchParams.delete("__path");
    url.searchParams.delete("path");
    url.pathname = "/" + original;
    request = new Request(url, request);
  }
  const h = build();
  if (!h) {
    return Response.json({ error: { code: "not_configured", message: "el conector no está configurado: " + problems.join("; ") } }, { status: 503, headers: { "Cache-Control": "no-store" } });
  }
  return h.fetch(request, request.headers.get("x-real-ip") || request.headers.get("x-forwarded-for")?.split(",")[0]?.trim() || "?");
}

export default { fetch: serve };
