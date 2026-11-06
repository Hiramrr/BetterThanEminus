// Lógica HTTP de solo lectura: expone las tareas de Eminus a Muse con una API key propia.
// Usa Request/Response estándar, así que corre igual en Node (node-server.mjs) y en
// Cloudflare Workers (worker.mjs).
//
//   GET /health             sin autenticación, sin datos
//   GET /docs               sin autenticación: instrucciones para Muse con la URL de este conector
//   GET /tasks              lista paginada de tareas
//   GET /updates            resumen de cambios, ?since=<version> evita repetir datos
//   GET /tasks/{id}         una tarea, con su descripción
//   GET /tasks/{id}/submission  tu entrega: texto, archivos subidos y revisión
//   GET /courses            cursos vigentes (?scope=all incluye los pasados)
//   GET /courses/{id}/content   unidades, recursos, texto y archivos del curso
//   GET /files/{fileId}         TEXTO del archivo en JSON (PDF por páginas, notebooks, texto); alias /text
//   GET /files/{fileId}/download  archivo original (binario)
//
// Cualquier otro método responde 405. Nunca se registran cabeceras, query ni cuerpos.

import { createHash, timingSafeEqual } from "node:crypto";
import { EminusError, extOf, mimeOf } from "./eminus.mjs";
import { canExtract, extractText } from "./files.mjs";

const STATUSES = new Set(["open", "pending", "overdue", "upcoming", "submitted", "reviewed", "all"]);
const OPEN = new Set(["pending", "overdue", "upcoming"]);

const ERROR_STATUS = {
  signed_out: 502,
  upstream_error: 502,
  forbidden: 502,
  upstream_timeout: 504,
  too_many_requests: 503,
  credentials_rejected: 503,
  password_change_required: 503,
  credentials_missing: 503,
  not_found: 404,
  file_too_large: 413,
  unsupported_type: 415,
  unreadable_file: 422
};

// Vercel limita la respuesta de una función a ~4.5 MB.
const MAX_BINARY = 4 * 1024 * 1024;

/** Nombre/extensión de un archivo descargado: cabecera de Eminus o, si no, por su contenido. */
function guessName(bytes, contentType, disposition) {
  const m = /filename\*?=(?:UTF-8'')?"?([^";]+)"?/i.exec(disposition || "");
  if (m) { try { return decodeURIComponent(m[1]); } catch { return m[1]; } }
  const head = Buffer.from(bytes.subarray(0, 64)).toString("latin1");
  if (head.startsWith("%PDF-")) return "archivo.pdf";
  for (const ext of ["docx", "pptx", "xlsx"]) if (String(contentType).split(";")[0] === mimeOf("archivo." + ext)) return "archivo." + ext;
  if (head.startsWith("PK")) return "archivo.zip";
  const t = String(contentType);
  if (/json/.test(t) || /^\s*\{/.test(head)) {
    try { if (Array.isArray(JSON.parse(Buffer.from(bytes).toString("utf8")).cells)) return "archivo.ipynb"; } catch {}
    return "archivo.json";
  }
  if (/csv/.test(t)) return "archivo.csv";
  if (/^text\//.test(t)) return "archivo.txt";
  return "archivo";
}

export const sha256 = (s) => createHash("sha256").update(String(s), "utf8").digest("hex");

function send(status, body, headers = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
      ...headers
    }
  });
}

const fail = (status, code, message, headers) => send(status, { error: { code, message } }, headers);

function presentedKey(request) {
  const auth = request.headers.get("authorization") || "";
  const m = auth.match(/^Bearer\s+(\S+)$/i);
  if (m) return m[1];
  return (request.headers.get("x-api-key") || "").trim();
}

function keyMatches(presented, expectedHash) {
  if (!presented || !expectedHash) return false;
  const a = Buffer.from(sha256(presented), "hex");
  const b = Buffer.from(expectedHash, "hex");
  return a.length === b.length && timingSafeEqual(a, b);
}

const encodeCursor = (offset) => Buffer.from(JSON.stringify({ o: offset })).toString("base64url");
function decodeCursor(cursor) {
  try {
    const o = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")).o;
    return Number.isInteger(o) && o >= 0 ? o : null;
  } catch {
    return null;
  }
}

/** Caché en memoria (Node). En Workers se usa la Cache API (ver worker.mjs). */
export function memoryStore() {
  let value = null;
  return { get: async () => value, put: async (v) => { value = v; } };
}

/**
 * Manejador de solo lectura con Request/Response estándar (Node 18+ y Cloudflare Workers).
 * @param {{ client: import("./eminus.mjs").EminusClient, apiKeyHash: string, cacheTtlSeconds?: number,
 *           rateLimitPerMinute?: number, store?: { get(): Promise<any>, put(v: any): Promise<void> } }} opts
 */
export function createHandler({ client, apiKeyHash, cacheTtlSeconds = 300, rateLimitPerMinute = 60, store = memoryStore(), docs = {} }) {
  if (!/^[0-9a-f]{64}$/.test(apiKeyHash || "")) throw new Error("falta el hash de la API key");
  let inflight = null;
  const hits = new Map();
  const contentCache = new Map(); // courseId → { at, data }
  const fileCache = new Map(); // fileId → { at, bytes, name, contentType }
  let allCourses = null; // { at, list }
  const fileNames = new Map(); // fileId → nombre (de tareas)
  const remember = (files) => { for (const f of files || []) { fileNames.set(f.id, f.name); if (fileNames.size > 2000) fileNames.delete(fileNames.keys().next().value); } };

  async function knownCourses() {
    if (allCourses && Date.now() - allCourses.at < 600_000) return allCourses.list;
    allCourses = { at: Date.now(), list: await client.courses() };
    return allCourses.list;
  }

  async function ownCourse(courseId) {
    return (await knownCourses()).some((c) => c.id === courseId);
  }

  async function courseContent(courseId) {
    const hit = contentCache.get(courseId);
    if (hit && Date.now() - hit.at < 600_000) return hit;
    const v = { at: Date.now(), data: await client.content(courseId) };
    contentCache.set(courseId, v);
    if (contentCache.size > 20) contentCache.delete(contentCache.keys().next().value);
    return v;
  }

  /** Nombre real del archivo si ya pasó por /content o por una tarea. */
  function knownName(fileId) {
    if (fileNames.has(fileId)) return fileNames.get(fileId);
    for (const { data } of contentCache.values())
      for (const u of data) for (const e of u.elements) for (const f of e.files) if (f.id === fileId) return f.name;
    return null;
  }

  async function fetchFile(fileId) {
    const hit = fileCache.get(fileId);
    if (hit && Date.now() - hit.at < 600_000) return hit;
    const course = /^[caer](\d+)-/.exec(fileId)?.[1];
    if (!course || !(await ownCourse(course))) throw new EminusError("not_found", "archivo no encontrado", 404);
    const { bytes, contentType, contentDisposition } = await client.download(fileId);
    const name = knownName(fileId) || guessName(bytes, contentType, contentDisposition);
    const v = { at: Date.now(), bytes, name, contentType };
    fileCache.set(fileId, v);
    let total = 0;
    for (const [k, f] of [...fileCache.entries()].reverse()) { total += f.bytes.length; if (total > 80 * 1024 * 1024) fileCache.delete(k); }
    return v;
  }

  async function snapshot(force = false) {
    const cache = await store.get().catch(() => null); // { at, data }
    if (!force && cache && Date.now() - cache.at < cacheTtlSeconds * 1000) return { ...cache.data, stale: false, fetchedAt: cache.at };
    if (!inflight) {
      inflight = client.tasks()
        .then(async (data) => { const v = { at: Date.now(), data }; await store.put(v).catch(() => {}); return v; })
        .finally(() => { inflight = null; });
    }
    try {
      const v = await inflight;
      return { ...v.data, stale: false, fetchedAt: v.at };
    } catch (err) {
      // Si Eminus falla, se sirve la última copia (hasta 6 h) marcada como `stale`.
      if (cache && Date.now() - cache.at < 6 * 3600 * 1000 && err.code !== "credentials_rejected") {
        return { ...cache.data, stale: true, fetchedAt: cache.at };
      }
      throw err;
    }
  }

  function limited(ip) {
    const now = Date.now();
    const list = (hits.get(ip) || []).filter((t) => now - t < 60000);
    list.push(now);
    hits.set(ip, list);
    if (hits.size > 1000) hits.clear();
    return list.length > rateLimitPerMinute;
  }

  /** Agrega text_url / download_url absolutos a cada archivo. */
  const linkFiles = (files, origin) => (files || []).map((f) => ({
    ...f,
    readable_as_text: canExtract(f.name),
    text_url: origin + "/files/" + f.id,
    download_url: origin + "/files/" + f.id + "/download"
  }));

  /** Texto de los archivos (primeras páginas), para entregarlo junto con la tarea. */
  async function inlineText(files, { pages = 30, maxChars = 60_000 } = {}) {
    return Promise.all(files.map(async (f) => {
      if (!f.readable_as_text) return { ...f, text: null, text_status: "unsupported_type" };
      try {
        const file = await fetchFile(f.id);
        const r = await extractText(file.bytes, f.name || file.name, { from: 1, to: pages });
        let text = r.pages.map((p) => (r.format === "pdf" ? `[página ${p.page}]\n` : "") + p.text).join("\n\n");
        const cut = text.length > maxChars;
        if (cut) text = text.slice(0, maxChars);
        return {
          ...f,
          text: r.has_text ? text : null,
          text_status: !r.has_text ? "no_text_layer" : cut || r.next_from ? "partial" : "complete",
          total_pages: r.total_pages,
          next_from: cut ? null : r.next_from
        };
      } catch {
        return { ...f, text: null, text_status: "unreadable" };
      }
    }));
  }

  async function listTasks(url) {
    const q = url.searchParams;
    const status = q.get("status") || "open";
    if (!STATUSES.has(status)) return fail(400, "invalid_parameter", "status debe ser uno de: " + [...STATUSES].join(", "));
    const limit = q.has("limit") ? Number(q.get("limit")) : 50;
    if (!Number.isInteger(limit) || limit < 1 || limit > 200) return fail(400, "invalid_parameter", "limit debe ser un entero entre 1 y 200");
    let offset = 0;
    if (q.get("cursor")) {
      offset = decodeCursor(q.get("cursor"));
      if (offset === null) return fail(400, "invalid_cursor", "cursor inválido");
    }
    const parseDate = (name) => {
      if (!q.has(name)) return undefined;
      const t = Date.parse(q.get(name));
      return Number.isFinite(t) ? t : null;
    };
    const dueAfter = parseDate("due_after");
    const dueBefore = parseDate("due_before");
    if (dueAfter === null || dueBefore === null) return fail(400, "invalid_parameter", "due_after/due_before deben ser fechas ISO 8601");
    const courseId = q.get("course_id");
    if (courseId && !/^\d{1,19}$/.test(courseId)) return fail(400, "invalid_parameter", "course_id debe ser numérico");

    const snap = await snapshot();
    const items = snap.tasks.filter((t) => {
      if (status === "open" ? !OPEN.has(t.status) : status !== "all" && t.status !== status) return false;
      if (courseId && t.course_id !== courseId) return false;
      const due = t.due_at ? Date.parse(t.due_at) : null;
      if (dueAfter !== undefined && (due === null || due < dueAfter)) return false;
      if (dueBefore !== undefined && (due === null || due > dueBefore)) return false;
      return true;
    });
    const page = items.slice(offset, offset + limit);
    if (q.get("include") === "description") {
      if (limit > 20) return fail(400, "invalid_parameter", "con include=description, limit debe ser ≤ 20");
      await Promise.all(page.map(async (t, i) => {
        try {
          const d = await client.description(t.course_id, t.activity_id);
          remember(d.files);
          page[i] = { ...t, description: d.text || null, attachments: linkFiles(d.files, url.origin) };
        } catch {}
      }));
    }
    return send(200, {
      data: page,
      total: items.length,
      next_cursor: offset + limit < items.length ? encodeCursor(offset + limit) : null,
      fetched_at: new Date(snap.fetchedAt).toISOString(),
      stale: snap.stale,
      partial: snap.failedCourses.length > 0
    });
  }

  async function oneTask(taskId, url) {
    if (!/^\d{1,19}-\d{1,19}$/.test(taskId)) return fail(404, "not_found", "tarea no encontrada");
    const snap = await snapshot();
    const task = snap.tasks.find((t) => t.id === taskId);
    if (!task) return fail(404, "not_found", "tarea no encontrada");
    let detail = { text: null, files: [] };
    try { detail = await client.description(task.course_id, task.activity_id); remember(detail.files); } catch {}
    const attachments = url.searchParams.get("include") === "none" ? linkFiles(detail.files, url.origin) : await inlineText(linkFiles(detail.files, url.origin));
    return send(200, { data: { ...task, description: detail.text || null, attachments }, fetched_at: new Date(snap.fetchedAt).toISOString(), stale: snap.stale });
  }

  async function updates(url) {
    const since = url.searchParams.get("since");
    if (since !== null && !/^[0-9a-f]{64}$/.test(since)) return fail(400, "invalid_parameter", "since debe ser una versión devuelta por /updates");
    const snap = await snapshot();
    const items = snap.tasks.map((t) => ({
      kind: "task", id: t.id, title: t.title, course_id: t.course_id, course: t.course,
      due_at: t.due_at, status: t.status, submitted: t.submitted,
      version: sha256(JSON.stringify(t)), url: url.origin + "/tasks/" + t.id
    }));
    const failed = [...snap.failedCourses];
    // Limita la concurrencia para no multiplicar las lecturas de unidades de Eminus.
    const queue = [...snap.courses];
    await Promise.all(Array.from({ length: Math.min(3, queue.length) }, async () => {
      while (queue.length) {
        const c = queue.shift();
        try {
          const content = await courseContent(c.id);
          for (const u of content.data) {
            items.push({ kind: "unit", id: c.id + ":" + u.id, title: u.title, course_id: c.id, course: c.name,
              version: sha256(JSON.stringify([u.id, u.title, u.published_at])), url: url.origin + "/courses/" + c.id + "/content" });
            for (const e of u.elements) items.push({
              kind: "resource", id: c.id + ":" + u.id + ":" + e.id, title: e.title, course_id: c.id, course: c.name,
              published_at: e.published_at, files: e.files.length,
              attachments: linkFiles(e.files, url.origin), links: e.links || [],
              version: sha256(JSON.stringify(e)), url: url.origin + "/courses/" + c.id + "/content"
            });
          }
        } catch (err) {
          if (["signed_out", "credentials_rejected", "password_change_required"].includes(err.code)) throw err;
          failed.push(c.id);
        }
      }
    }));
    items.sort((a, b) => a.kind.localeCompare(b.kind) || a.id.localeCompare(b.id));
    const partial = failed.length > 0;
    // No publicar una versión incompleta que Muse pueda confundir con eliminaciones.
    const version = partial || snap.stale ? null : sha256(JSON.stringify(items.map(({ kind, id, version }) => [kind, id, version])));
    const changed = version === null || version !== since;
    return send(200, {
      changed, version, baseline: since === null, partial, stale: snap.stale,
      failed_courses: [...new Set(failed)], checked_at: new Date().toISOString(),
      poll_after_seconds: 1800, ...(changed ? { items } : {})
    });
  }

  async function handle(request, ip = "?") {
    const url = new URL(request.url);
    const req = request;
    const path = url.pathname.replace(/\/+$/, "") || "/";
    if (path === "/health") return send(200, { status: "ok" });
    if (path === "/docs" || path.startsWith("/docs/")) {
      const docHeaders = { "Content-Type": "text/markdown; charset=utf-8", "Cache-Control": "public, max-age=300", "X-Content-Type-Options": "nosniff" };
      if (path === "/docs") {
        const list = Object.keys(docs).map((n) => `- [${n}.md](${url.origin}/docs/${n}.md)`).join("\n");
        return new Response(`# Conector Eminus para Muse\n\nBase URL: ${url.origin}\n\nPásale a Muse estos archivos:\n\n${list}\n`, { headers: docHeaders });
      }
      const name = /^\/docs\/([A-Z]+)\.md$/.exec(path)?.[1];
      if (!name || !docs[name]) return fail(404, "not_found", "documento no encontrado");
      return new Response(docs[name].replaceAll("{{BASE_URL}}", url.origin), { headers: docHeaders });
    }
    if (req.method !== "GET" && req.method !== "HEAD") return fail(405, "method_not_allowed", "este conector es de solo lectura", { Allow: "GET" });
    if (!keyMatches(presentedKey(request), apiKeyHash)) {
      return fail(401, "unauthorized", "API key ausente o inválida", { "WWW-Authenticate": 'Bearer realm="eminus"' });
    }
    if (limited(ip)) return fail(429, "rate_limited", "demasiadas solicitudes; espera un minuto", { "Retry-After": "60" });
    try {
      if (path === "/updates") return await updates(url);
      if (path === "/tasks") return await listTasks(url);
      const sm = path.match(/^\/tasks\/(\d{1,19}-\d{1,19})\/submission$/);
      if (sm) {
        const snap = await snapshot();
        const task = snap.tasks.find((t) => t.id === sm[1]);
        if (!task) return fail(404, "not_found", "tarea no encontrada");
        const sub = await client.submission(task.course_id, task.activity_id);
        if (sub) {
          remember([...sub.files, ...sub.review.files]);
          const inline = url.searchParams.get("include") !== "none";
          sub.files = inline ? await inlineText(linkFiles(sub.files, url.origin)) : linkFiles(sub.files, url.origin);
          sub.review.files = inline ? await inlineText(linkFiles(sub.review.files, url.origin)) : linkFiles(sub.review.files, url.origin);
        }
        return send(200, { task_id: task.id, title: task.title, course: task.course, data: sub });
      }
      const m = path.match(/^\/tasks\/([^/]+)$/);
      if (m) return await oneTask(decodeURIComponent(m[1]), url);
      if (path === "/courses") {
        if (url.searchParams.get("scope") === "all") return send(200, { data: await knownCourses(), fetched_at: new Date(allCourses.at).toISOString(), stale: false });
        const snap = await snapshot();
        return send(200, { data: snap.courses, fetched_at: new Date(snap.fetchedAt).toISOString(), stale: snap.stale });
      }
      const cm = path.match(/^\/courses\/(\d{1,19})\/content$/);
      if (cm) {
        if (!(await ownCourse(cm[1]))) return fail(404, "not_found", "curso no encontrado");
        const v = await courseContent(cm[1]);
        const files = v.data.reduce((n, u) => n + u.elements.reduce((m, e) => m + e.files.length, 0), 0);
        const data = v.data.map((u) => ({ ...u, elements: u.elements.map((e) => ({ ...e, files: linkFiles(e.files, url.origin) })) }));
        return send(200, { data, course_id: cm[1], units: v.data.length, files, fetched_at: new Date(v.at).toISOString() });
      }
      const fm = path.match(/^\/files\/([caer]\d{1,19}-\d{1,19}-\d{1,19})(\/text|\/download)?$/);
      if (fm) {
        const file = await fetchFile(fm[1]);
        if (fm[2] === "/download") {
          if (file.bytes.length > MAX_BINARY) return fail(413, "file_too_large", "el archivo pesa más de 4 MB; usa GET /files/{id} para leer su texto");
          const safe = file.name.replace(/[^\w.\- ()áéíóúñÁÉÍÓÚÑ]/g, "_");
          return new Response(file.bytes, { status: 200, headers: {
            "Content-Type": mimeOf(file.name) !== "application/octet-stream" ? mimeOf(file.name) : file.contentType,
            "Content-Disposition": `attachment; filename*=UTF-8''${encodeURIComponent(safe)}`,
            "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff"
          } });
        }
        const q = url.searchParams;
        const from = q.has("from") ? Number(q.get("from")) : 1;
        const to = q.has("to") ? Number(q.get("to")) : undefined;
        if (!Number.isInteger(from) || from < 1 || (to !== undefined && (!Number.isInteger(to) || to < from || to - from > 99))) {
          return fail(400, "invalid_parameter", "from/to deben ser enteros, from ≥ 1, to ≥ from y como máximo 100 páginas");
        }
        try {
          const result = await extractText(file.bytes, file.name, { from, to });
          if (from > result.total_pages && result.total_pages > 0) return fail(400, "invalid_parameter", `el archivo tiene ${result.total_pages} páginas`);
          const linked = linkFiles([{ id: fm[1], name: file.name }], url.origin)[0];
          return send(200, { ...linked, type: extOf(file.name) || null, size_bytes: file.bytes.length, ...result });
        } catch (err) {
          if (err.code === "unsupported_type") return fail(415, "unsupported_type", err.message + ": " + url.origin + "/files/" + fm[1] + "/download");
          return fail(422, "unreadable_file", "no se pudo leer el texto del archivo; descárgalo");
        }
      }
      return fail(404, "not_found", "ruta no encontrada");
    } catch (err) {
      const code = err instanceof EminusError ? err.code : /credenciales/.test(err?.message) ? "credentials_missing" : "internal_error";
      const status = ERROR_STATUS[code] || 500;
      const message = err instanceof EminusError || code === "credentials_missing" ? err.message : "error interno";
      return fail(status, code, message, status === 503 ? { "Retry-After": "1800" } : {});
    }
  }

  return {
    async fetch(request, ip) {
      try { return await handle(request, ip); } catch { return fail(500, "internal_error", "error interno"); }
    },
    /** Precarga la caché para que la primera consulta de Muse sea rápida. */
    warm: (force = false) => snapshot(force).then(() => true, () => false)
  };
}
