#!/usr/bin/env node
// Arranca el conector. Variables (ninguna es secreta):
//   HOST (127.0.0.1)  PORT (8787)  CACHE_TTL_SECONDS (300)
// Los secretos se leen del Llavero o de archivos 600: ver src/credentials.mjs.
import { eminusFetch } from "./http.mjs";
import { EminusClient } from "./eminus.mjs";
import { loadApiKeyHash, loadEminusCredentials } from "./credentials.mjs";
import { createApp } from "./node-server.mjs";

const host = process.env.HOST || "127.0.0.1";
const port = Number(process.env.PORT || 8787);
const apiKeyHash = await loadApiKeyHash();
if (!apiKeyHash) {
  console.error("Falta la API key de Muse. Ejecuta `npm run setup` (o define MUSE_API_KEY_SHA256).");
  process.exit(1);
}
const client = new EminusClient({ fetch: eminusFetch,
  getCredentials: async () => {
    const { user, password } = await loadEminusCredentials();
    return { user, password };
  }
});
const app = createApp({ client, apiKeyHash, cacheTtlSeconds: Number(process.env.CACHE_TTL_SECONDS || 300) });
app.listen(port, host, async () => {
  console.log(`muse-connector escuchando en http://${host}:${port} (solo lectura)`);
  console.log((await app.warm()) ? "caché precargada" : "no se pudo precargar; se reintentará en la primera consulta");
});
