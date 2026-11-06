// Adaptador Node: sirve el manejador de api.mjs con node:http.
import { createServer as httpServer } from "node:http";
import { createHandler } from "./api.mjs";

export { sha256 } from "./api.mjs";

export function createApp({ log = console.log, ...opts }) {
  const handler = createHandler(opts);
  const server = httpServer(async (req, res) => {
    const start = Date.now();
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const request = new Request(new URL(req.url, "http://" + (req.headers.host || "localhost")), {
      method: req.method,
      headers: Object.entries(req.headers).flatMap(([k, v]) => (Array.isArray(v) ? v.map((x) => [k, x]) : [[k, String(v)]])),
      body: ["GET", "HEAD"].includes(req.method) || !chunks.length ? undefined : Buffer.concat(chunks)
    });
    const response = await handler.fetch(request, req.socket.remoteAddress || "?");
    const body = req.method === "HEAD" ? "" : Buffer.from(await response.arrayBuffer());
    res.writeHead(response.status, Object.fromEntries(response.headers));
    res.end(body);
    log(`${new Date().toISOString()} ${req.method} ${new URL(req.url, "http://x").pathname} ${response.status} ${Date.now() - start}ms`);
  });
  server.warm = handler.warm;
  return server;
}
