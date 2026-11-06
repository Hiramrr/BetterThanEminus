// fetch mínimo sobre node:https con la cadena de confianza de Node + la raíz
// «GoDaddy TLS Root CA - R1», que firma el certificado actual de eminus.uv.mx.
// Esa raíz está firmada en cruzado por «Go Daddy Root Certificate Authority - G2»
// (ya incluida en Node); la prueba test/tls.test.mjs lo comprueba. No se
// desactiva ninguna verificación TLS.
import https from "node:https";
import tls from "node:tls";

import GODADDY_R1 from "./godaddy-r1.mjs";
export { GODADDY_R1 };
const agent = new https.Agent({ ca: [...tls.rootCertificates, GODADDY_R1], keepAlive: true, maxSockets: 8 });

export function eminusFetch(url, { method = "GET", headers = {}, body, signal } = {}) {
  const u = new URL(url);
  if (u.protocol !== "https:" || u.hostname !== "eminus.uv.mx") return Promise.reject(new Error("host no permitido"));
  return new Promise((resolve, reject) => {
    const req = https.request(u, { method, headers, agent, signal }, (res) => {
      const chunks = [];
      let size = 0;
      res.on("data", (c) => {
        size += c.length;
        if (size > 50 * 1024 * 1024) req.destroy(new Error("respuesta demasiado grande"));
        else chunks.push(c);
      });
      res.on("end", () => {
        const buf = Buffer.concat(chunks);
        resolve({
          status: res.statusCode,
          ok: res.statusCode >= 200 && res.statusCode < 300,
          headers: { get: (name) => res.headers[String(name).toLowerCase()] ?? null },
          bytes: async () => buf,
          text: async () => buf.toString("utf8"),
          json: async () => JSON.parse(buf.toString("utf8"))
        });
      });
      res.on("error", reject);
    });
    req.on("error", reject);
    if (body) req.write(body);
    req.end();
  });
}
