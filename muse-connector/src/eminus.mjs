// Cliente de solo lectura para Eminus (Universidad Veracruzana).
//
// Sigue exactamente el protocolo de la app de Swift (EminusGlassMac/Sources/Core):
//   - EminusAuth.swift  → POST https://eminus.uv.mx/eminusapi/api/auth
//   - EminusAPI.swift   → GET  https://eminus.uv.mx/eminusapi8/api/...
// Nunca registra ni devuelve usuario, contraseña ni token.
// En Node se le pasa `eminusFetch` (http.mjs); en Workers usa el fetch nativo.

import { createCipheriv, pbkdf2Sync, randomBytes } from "node:crypto";

export const ORIGIN = "https://eminus.uv.mx";
export const AUTH_URL = ORIGIN + "/eminusapi/api/auth";
export const API8 = ORIGIN + "/eminusapi8/api";
export const API = ORIGIN + "/eminusapi/api"; // API principal (contenido, descargas de contenido)
export const WEB_URL = ORIGIN + "/eminus4/page/course/list";

// Clave pública del protocolo de Eminus: viene en el JavaScript público del
// formulario de entrada y es la misma que usa EminusAuth.publicAppKey.
// No es un secreto del usuario.
export const PUBLIC_APP_KEY = "ow7dxys8glfor9tnc2ansdfo1etkgjcv";

export class EminusError extends Error {
  constructor(code, message, status) {
    super(message);
    this.code = code; // signed_out | credentials_rejected | too_many_requests | password_change_required | forbidden | upstream_error | upstream_timeout
    this.status = status;
  }
}

// ───────────── cifrado de la contraseña (igual que EminusAuth.seal) ─────────────

/** base64(sal ‖ iv ‖ AES-256-CBC/PKCS7), llave PBKDF2-HMAC-SHA1 de 100 vueltas. */
export function seal(password, appKey = PUBLIC_APP_KEY, salt = randomBytes(16), iv = randomBytes(16)) {
  if (salt.length !== 16 || iv.length !== 16) throw new Error("sal e iv deben medir 16 bytes");
  const plain = Buffer.from(String(password).trim(), "utf8");
  if (!plain.length) throw new EminusError("credentials_rejected", "contraseña vacía");
  const key = pbkdf2Sync(Buffer.from(appKey, "utf8"), salt, 100, 32, "sha1");
  const cipher = createCipheriv("aes-256-cbc", key, iv);
  const out = Buffer.concat([cipher.update(plain), cipher.final()]);
  return Buffer.concat([salt, iv, out]).toString("base64");
}

// ───────────── JWT (sin verificar firma; Eminus la valida) ─────────────

export function jwtClaims(token) {
  const parts = String(token || "").split(".");
  if (parts.length !== 3 || token.length > 8192) return null;
  try {
    return JSON.parse(Buffer.from(parts[1], "base64url").toString("utf8"));
  } catch {
    return null;
  }
}

export function jwtExpiry(token) {
  const exp = Number(jwtClaims(token)?.exp);
  return Number.isFinite(exp) ? exp : 0;
}

/** Registro de /auth: de un arreglo toma el último y acepta la envoltura `contenido`. */
export function authRecord(json) {
  let value = Array.isArray(json) ? json[json.length - 1] : json;
  if (value && typeof value.contenido === "object" && !Array.isArray(value.contenido)) value = value.contenido;
  return value && typeof value === "object" ? value : null;
}

// ───────────── utilidades de datos ─────────────

const str = (v) => (typeof v === "string" ? v.trim() : typeof v === "number" ? String(v) : "");
const num = (v) => {
  const n = Number(str(v).replace(",", "."));
  return str(v) !== "" && Number.isFinite(n) ? n : null;
};
const id = (v) => {
  const s = str(v);
  return /^\d{1,19}$/.test(s) && /[1-9]/.test(s) ? s : null;
};
const epoch = (v) => {
  const n = num(v);
  return n && n > 0 ? n : null;
};
const iso = (sec) => (sec ? new Date(sec * 1000).toISOString() : null);

export function rowsOf(json) {
  if (Array.isArray(json)) return json;
  if (json && Array.isArray(json.contenido)) return json.contenido;
  if (json && Array.isArray(json.data)) return json.data;
  throw new EminusError("upstream_error", "respuesta de Eminus ilegible");
}

const ENTITIES = { nbsp: " ", amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", aacute: "á", eacute: "é", iacute: "í", oacute: "ó", uacute: "ú", ntilde: "ñ", Aacute: "Á", Eacute: "É", Iacute: "Í", Oacute: "Ó", Uacute: "Ú", Ntilde: "Ñ", uuml: "ü", iexcl: "¡", iquest: "¿", ldquo: "“", rdquo: "”", hellip: "…", ndash: "–", mdash: "—" };

/** Descripción HTML de Eminus → texto plano. */
export function htmlToText(html) {
  return String(html || "")
    .replace(/<(script|style)[\s\S]*?<\/\1>/gi, "")
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/(p|div|li|h[1-6]|tr)>/gi, "\n")
    .replace(/<li[^>]*>/gi, "• ")
    .replace(/<[^>]+>/g, "")
    .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(Number(d)))
    .replace(/&#x([0-9a-f]+);/gi, (_, h) => String.fromCodePoint(parseInt(h, 16)))
    .replace(/&([a-z]+);/gi, (m, n) => ENTITIES[n] ?? m)
    .replace(/[ \t ]+/g, " ")
    .replace(/ *\n */g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

// ───────────── normalización (igual criterio que EminusAPI.swift) ─────────────

export function normalizeCourse(row, nowSec = Date.now() / 1000) {
  const raw = row?.curso ?? row;
  const courseId = id(raw?.idCurso);
  if (!courseId || raw.archivado === true || raw.archivado === 1) return null;
  const end = epoch(raw.fechaTerminoEpoch);
  return {
    id: courseId,
    name: str(raw.nombre) || "Curso " + courseId,
    starts_at: iso(epoch(raw.fechaInicioEpoch)),
    ends_at: iso(end),
    current: !end || end >= nowSec,
    favorite: !!raw.esFavorito
  };
}

export function normalizeTask(raw, course, nowSec = Date.now() / 1000) {
  const activityId = id(raw?.idActividad);
  if (!activityId) return null;
  if (raw.visible !== undefined && ["0", "false"].includes(String(raw.visible).toLowerCase())) return null;
  const original = epoch(raw.fechaTerminoEpoch);
  const extension = epoch(raw.fechaProrrogaEpoch);
  const due = extension ?? original;
  const opens = epoch(raw.fechaInicioEpoch);
  const submittedAt = epoch(raw.fechaEntregaEpoch);
  const reviewedAt = epoch(raw.fechaRevisionEpoch);
  // idEstado / estadoEntrega: 0 o null sin entregar, 1 entregada, 3 revisada.
  const state = num(raw.idEstado) ?? num(raw.estadoEntrega);
  let submitted;
  if (state !== null) submitted = state > 0;
  else {
    const d = str(raw.fechaEntrega);
    submitted = !!submittedAt || (!!d && !d.startsWith("0001") && d !== str(raw.fechaTermino));
  }
  let status;
  if (submitted) status = state === 3 ? "reviewed" : "submitted";
  else if (opens && opens > nowSec) status = "upcoming";
  else if (due && due < nowSec) status = "overdue";
  else status = "pending";
  const value = num(raw.valor);
  return {
    id: course.id + "-" + activityId,
    activity_id: activityId,
    course_id: course.id,
    course: course.name,
    title: str(raw.titulo) || "Actividad " + activityId,
    status,
    submitted,
    opens_at: iso(opens),
    due_at: iso(due),
    original_due_at: iso(original),
    has_extension: !!extension && extension !== original,
    submitted_at: iso(submittedAt),
    reviewed_at: iso(reviewedAt),
    grade: num(raw.calificacion),
    max_grade: value && value > 0 ? value : null,
    description: null,
    url: WEB_URL
  };
}

// ───────────── cliente con sesión en memoria ─────────────

export class EminusClient {
  /** @param {{ getCredentials: () => Promise<{user: string, password: string}>, fetch?: typeof fetch, now?: () => number }} opts */
  constructor({ getCredentials, fetch: f = (...a) => globalThis.fetch(...a), now = () => Date.now() / 1000, tokenStore = null }) {
    this.getCredentials = getCredentials;
    this.tokenStore = tokenStore; // opcional {get(): token|null, put(token, exp)}: comparte el JWT entre instancias (Workers KV)
    this.fetch = f;
    this.now = now;
    this.token = "";
    this.expiry = 0;
    this.blockedUntil = 0; // tras credenciales rechazadas no se insiste (evita bloquear la cuenta)
    this.signingIn = null;
  }

  async request(url, init = {}, timeoutMs = 25000) {
    try {
      return await this.fetch(url, { ...init, signal: AbortSignal.timeout(timeoutMs) });
    } catch (err) {
      if (err?.name === "TimeoutError" || err?.name === "AbortError") throw new EminusError("upstream_timeout", "Eminus tardó demasiado en responder");
      throw new EminusError("upstream_error", "sin conexión con Eminus");
    }
  }

  async signIn() {
    if (this.now() < this.blockedUntil) {
      throw new EminusError("credentials_rejected", "Eminus rechazó las credenciales guardadas; actualízalas con `npm run setup`");
    }
    const { user, password } = await this.getCredentials();
    const res = await this.request(AUTH_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({ username: String(user).trim(), password: seal(password) })
    }, 20000);
    if (res.status === 429) throw new EminusError("too_many_requests", "Eminus recibió demasiados intentos de acceso", 429);
    if (res.status >= 400 && res.status < 500) {
      this.blockedUntil = this.now() + 30 * 60;
      throw new EminusError("credentials_rejected", "Eminus rechazó las credenciales guardadas", res.status);
    }
    if (!res.ok) throw new EminusError("upstream_error", "Eminus respondió HTTP " + res.status, res.status);
    const record = authRecord(await res.json().catch(() => null));
    const token = typeof record?.accessToken === "string" ? record.accessToken.trim() : "";
    if (str(record?.estadoUsuarioChecar) === "2") {
      this.blockedUntil = this.now() + 30 * 60;
      throw new EminusError("password_change_required", "Eminus pide cambiar la contraseña");
    }
    const exp = jwtExpiry(token);
    if (!token || exp - this.now() <= 30) throw new EminusError("upstream_error", "Eminus devolvió una sesión ilegible");
    this.token = token;
    this.expiry = exp;
    if (this.tokenStore) await this.tokenStore.put(token, exp).catch(() => {});
    return token;
  }

  async validToken(force = false) {
    if (!force && this.token && this.expiry - this.now() > 120) return this.token;
    if (!force && this.tokenStore) {
      const stored = await this.tokenStore.get().catch(() => null);
      const exp = jwtExpiry(stored);
      if (stored && exp - this.now() > 120) {
        this.token = stored;
        this.expiry = exp;
        return stored;
      }
    }
    if (!this.signingIn) this.signingIn = this.signIn().finally(() => { this.signingIn = null; });
    return this.signingIn;
  }

  /** GET a la API nueva. Reintenta una vez con sesión nueva si hay 401. */
  async get(path, { missingIsEmpty = false, base = API8 } = {}) {
    let token = await this.validToken();
    for (let attempt = 0; attempt < 2; attempt++) {
      const res = await this.request(base + path, {
        headers: { Authorization: "Bearer " + token, Accept: "application/json" }
      });
      if (res.status === 401 && attempt === 0) {
        token = await this.validToken(true);
        continue;
      }
      if (res.status === 401) throw new EminusError("signed_out", "Eminus no aceptó la sesión", 401);
      if (res.status === 403) throw new EminusError("forbidden", "Eminus no permite esta consulta", 403);
      if (res.status === 204) return [];
      if (res.status === 404 && missingIsEmpty) {
        const body = await res.json().catch(() => null);
        if (Number(body?.codigo) === 404 && body?.salida === "No se encontraron actividades") return { contenido: [] };
        throw new EminusError("upstream_error", "Eminus respondió HTTP 404", 404);
      }
      if (!res.ok) throw new EminusError("upstream_error", "Eminus respondió HTTP " + res.status, res.status);
      const text = await res.text();
      if (!text) return [];
      try {
        return JSON.parse(text);
      } catch {
        throw new EminusError("upstream_error", "respuesta de Eminus ilegible");
      }
    }
  }

  async courses() {
    const now = this.now();
    const seen = new Set();
    return rowsOf(await this.get("/Course/getAllCourses"))
      .map((r) => normalizeCourse(r, now))
      .filter((c) => c && !seen.has(c.id) && seen.add(c.id));
  }

  /** Tareas de los cursos vigentes. Un curso que falla no tumba los demás. */
  async tasks() {
    const now = this.now();
    const courses = (await this.courses()).filter((c) => c.current);
    const tasks = [];
    const failedCourses = [];
    const queue = [...courses];
    const worker = async () => {
      while (queue.length) {
        const course = queue.shift();
        try {
          const rows = rowsOf(await this.get("/Activity/getActividadesEstudiante/" + course.id, { missingIsEmpty: true }));
          const seen = new Set();
          for (const raw of rows) {
            const t = normalizeTask(raw, course, now);
            if (t && !seen.has(t.id)) { seen.add(t.id); tasks.push(t); }
          }
        } catch (err) {
          if (err.code === "signed_out" || err.code === "credentials_rejected") throw err;
          if (err.code !== "forbidden") failedCourses.push(course.id);
        }
      }
    };
    await Promise.all(Array.from({ length: Math.min(6, queue.length) }, worker));
    if (courses.length && failedCourses.length === courses.length) throw new EminusError("upstream_error", "no se pudo leer ningún curso");
    tasks.sort((a, b) => (a.due_at ?? "9999").localeCompare(b.due_at ?? "9999") || a.id.localeCompare(b.id));
    return { courses, tasks, failedCourses };
  }

  /** Enunciado de una actividad: GET /Activity/getActividadEstudiante/{curso}/{actividad}. */
  async description(courseId, activityId) {
    if (!id(courseId) || !id(activityId)) throw new EminusError("upstream_error", "identificador inválido");
    const json = await this.get("/Activity/getActividadEstudiante/" + courseId + "/" + activityId);
    let raw = null;
    try { raw = rowsOf(json)[0] ?? null; } catch { raw = json?.contenido ?? json; }
    const files = (raw?.archivosActsConTamano ?? [])
      .map((f) => {
        const fileId = id(f?.idArchivosActividades);
        return fileId ? normalizeFile(f, "a" + courseId + "-" + activityId + "-" + fileId) : null;
      })
      .filter(Boolean);
    return { text: htmlToText(raw?.descripcion), files };
  }

  /**
   * Tu entrega de una tarea: texto, archivos que subiste y revisión del docente.
   *   GET /eminusapi8/api/Activity/getActividadEstudiante/{curso}/{actividad}   (equipo y rúbrica)
   *   GET /eminusapi8/api/Activity/getActUsuarioMiRevision/{curso}/{actividad}/{equipo}/{rúbrica}
   * Devuelve null si no hay entrega.
   */
  async submission(courseId, activityId) {
    if (!id(courseId) || !id(activityId)) throw new EminusError("upstream_error", "identificador inválido");
    let detail = null;
    try { detail = rowsOf(await this.get("/Activity/getActividadEstudiante/" + courseId + "/" + activityId))[0] ?? null; } catch (err) { if (err.code !== "upstream_error") throw err; }
    const team = id(detail?.idEquipo) || "0";
    const rubric = id(detail?.idRubricaUtilizada) || "0";
    let json;
    try {
      json = await this.get(`/Activity/getActUsuarioMiRevision/${courseId}/${activityId}/${team}/${rubric}`);
    } catch (err) {
      if (err.status === 404) return null;
      throw err;
    }
    const c = json?.contenido;
    const rev = Array.isArray(c) ? c[0] : c && typeof c === "object" ? c : json;
    const submissionId = id(rev?.idActividadEntrega ?? rev?.idActividadEntregas);
    if (!rev || !submissionId) return null;
    const files = (key, prefix, idKey) => (rev[key] ?? [])
      .filter((f) => id(f?.[idKey]) && f.visible !== 0)
      .map((f) => normalizeFile(f, prefix + courseId + "-" + activityId + "-" + str(f[idKey])));
    const state = num(rev.idEstado);
    return {
      id: submissionId,
      status: state === 3 ? "reviewed" : state && state > 0 ? "submitted" : "draft",
      submitted_at: iso(epoch(rev.fechaEntregaEpoch)),
      text: htmlToText(rev.descEntrega) || null,
      files: files("archivosEntregaConTamano", "e", "idArchivosEntregas"),
      team_id: team === "0" ? null : team,
      review: {
        reviewed_at: iso(epoch(rev.fechaRevisionEpoch)),
        grade: num(rev.calificacion),
        feedback: htmlToText(rev.descRevision) || null,
        reviewer: str(rev.nombreFacilitador) || null,
        files: files("archivosRevisionConTamano", "r", "idArchivoActividadRevision")
      }
    };
  }

  /**
   * Contenido de un curso: unidades y sus elementos (texto + archivos).
   *   GET /eminusapi/api/Contenido/getUnidades/{curso}/0
   *   GET /eminusapi/api/Contenido/getElementos/{curso}/{unidad}
   * No usa getElemento (detalle) para no marcar recursos como leídos.
   */
  async content(courseId) {
    if (!id(courseId)) throw new EminusError("upstream_error", "identificador inválido");
    const units = rowsOf(await this.get("/Contenido/getUnidades/" + courseId + "/0", { base: API }))
      .filter((u) => id(u?.idUnidad) && u.visible !== 0)
      .sort((a, b) => (num(a.posicion) ?? 0) - (num(b.posicion) ?? 0));
    const out = [];
    const queue = units.map((u, i) => [u, i]);
    const worker = async () => {
      while (queue.length) {
        const [u, i] = queue.shift();
        const rows = rowsOf(await this.get("/Contenido/getElementos/" + courseId + "/" + u.idUnidad, { base: API, missingIsEmpty: true }));
        const elements = rows
          .filter((e) => id(e?.idElemento) && e.visible !== 0)
          .sort((a, b) => (num(a.posicion) ?? 0) - (num(b.posicion) ?? 0))
          .map((e) => ({
            id: str(e.idElemento),
            title: str(e.nombre) || "Recurso " + str(e.idElemento),
            text: htmlToText(e.texto) || null,
            published_at: iso(epoch(e.fechaEpoch)),
            required_reading: e.lecturaObligatoria === 1,
            files: (e.archivosElementosConTamano ?? [])
              .filter((f) => id(f?.idArchivosElementos) && f.visible !== 0)
              .map((f) => normalizeFile(f, "c" + courseId + "-" + str(e.idElemento) + "-" + str(f.idArchivosElementos)))
          }));
        out[i] = {
          id: str(u.idUnidad),
          title: str(u.nombre) || "Unidad " + (i + 1),
          published_at: iso(epoch(u.fechaPublicacionEpoch)),
          elements
        };
      }
    };
    await Promise.all(Array.from({ length: Math.min(4, queue.length) }, worker));
    return out.filter(Boolean);
  }

  /**
   * Descarga un archivo por su referencia (ver parseFileRef):
   *   c… → GET /eminusapi/api/contenido/descargaElementoArchivo/{curso}/{elemento}/{archivo}
   *   a… → GET /eminusapi8/api/Activity/descargaArchivo/{curso}/{actividad}/{archivo}/2  (enunciado)
   *   e… → …/descargaArchivo/{curso}/{actividad}/{archivo}/1  (tu entrega)
   *   r… → …/descargaArchivo/{curso}/{actividad}/{archivo}/3  (revisión del docente)
   */
  async download(ref) {
    const r = parseFileRef(ref);
    if (!r) throw new EminusError("not_found", "archivo no encontrado", 404);
    const url = r.kind === "content"
      ? API + "/contenido/descargaElementoArchivo/" + r.course + "/" + r.parent + "/" + r.file
      : API8 + "/Activity/descargaArchivo/" + r.course + "/" + r.parent + "/" + r.file + "/" + { activity: 2, submission: 1, review: 3 }[r.kind];
    let token = await this.validToken();
    for (let attempt = 0; attempt < 2; attempt++) {
      const res = await this.request(url, { headers: { Authorization: "Bearer " + token } }, 60000);
      if (res.status === 401 && attempt === 0) { token = await this.validToken(true); continue; }
      if (res.status === 401) throw new EminusError("signed_out", "Eminus no aceptó la sesión", 401);
      if (res.status === 403) throw new EminusError("forbidden", "Eminus no permite esta descarga", 403);
      if (res.status === 404) throw new EminusError("not_found", "archivo no encontrado", 404);
      if (!res.ok) throw new EminusError("upstream_error", "Eminus respondió HTTP " + res.status, res.status);
      const type = String(res.headers?.get?.("content-type") || "");
      if (type.includes("text/html")) throw new EminusError("upstream_error", "Eminus devolvió una página en lugar del archivo");
      const bytes = res.bytes ? await res.bytes() : Buffer.from(await res.arrayBuffer());
      return { bytes, contentType: type || "application/octet-stream" };
    }
  }
}

// ───────────── archivos ─────────────

const MIME = { pdf: "application/pdf", csv: "text/csv", txt: "text/plain", md: "text/markdown", ipynb: "application/x-ipynb+json", json: "application/json", py: "text/x-python", html: "text/html", htm: "text/html", doc: "application/msword", docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", ppt: "application/vnd.ms-powerpoint", pptx: "application/vnd.openxmlformats-officedocument.presentationml.presentation", xls: "application/vnd.ms-excel", xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", zip: "application/zip" };

export const extOf = (name) => (String(name).match(/\.([a-z0-9]{1,8})$/i)?.[1] || "").toLowerCase();
export const mimeOf = (name) => MIME[extOf(name)] || "application/octet-stream";

export function normalizeFile(f, ref) {
  const name = str(f?.nombreOriginal ?? f?.nombreArchivo ?? f?.nombre) || "archivo";
  return { id: ref, name, type: extOf(name) || null, mime: mimeOf(name), size_bytes: num(f?.tamano ?? f?.tamanio) };
}

/**
 * c<curso>-<elemento>-<archivo>   contenido
 * a<curso>-<actividad>-<archivo>  instrucciones de una tarea
 * e<curso>-<actividad>-<archivo>  archivo que subiste en tu entrega
 * r<curso>-<actividad>-<archivo>  archivo de revisión del docente
 */
export const FILE_REF = /^([caer])(\d{1,19})-(\d{1,19})-(\d{1,19})$/;
export function parseFileRef(ref) {
  const m = FILE_REF.exec(String(ref));
  if (!m || ![m[2], m[3], m[4]].every(id)) return null;
  return { kind: { c: "content", a: "activity", e: "submission", r: "review" }[m[1]], course: m[2], parent: m[3], file: m[4] };
}
