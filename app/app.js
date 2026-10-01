// eminus tui: pestaña propia con agenda y cursos, leída de la API de Eminus.
(() => {
  const bte = globalThis.bte;
  const API8 = "https://eminus.uv.mx/eminusapi8/api";
  // El contenido de los cursos (módulos, elementos y archivos) sólo está en
  // la API anterior.
  const API_OLD = "https://eminus.uv.mx/eminusapi/api";
  const EMINUS = "https://eminus.uv.mx";
  const CACHE_KEY = "bteAppCache";
  const CONTENT_CACHE_KEY = "bteAppContent";
  // El contenido publicado cambia poco y leerlo cuesta varias consultas por
  // curso: se guarda en disco y sólo se vuelve a pedir pasado este tiempo.
  const CONTENT_MAX_AGE = 30 * 60 * 1000;
  const VIEWS = ["agenda", "courses", "miyu", "grades", "content"];
  const CACHE_MAX_AGE = 5 * 60 * 1000;
  const OVERDUE_WINDOW = 21 * 24 * 3600;

  const $ = (id) => document.getElementById(id);

  // Dentro de Eminus la app vive en un iframe; habla con la página por
  // postMessage (sólo con el origen de Eminus).
  const embedded = window.top !== window;
  document.documentElement.classList.toggle("embedded", embedded);
  const toParent = (msg) => {
    if (embedded) window.parent.postMessage({ bte: true, ...msg }, EMINUS);
  };
  const els = {
    main: $("main"),
    dash: $("dash"),
    paneDash: $("pane-dash"),
    list: $("list"),
    mid: $("mid"),
    detail: $("detail"),
    listTitle: $("list-title"),
    midTitle: $("mid-title"),
    paneList: $("pane-list"),
    paneMid: $("pane-mid"),
    paneDetail: $("pane-detail"),
    splash: $("splash"),
    mode: $("mode"),
    back: $("back"),
    hints: $("hints"),
    msg: $("msg"),
    filterWrap: $("filter-wrap"),
    filter: $("filter"),
    counters: $("counters"),
    clock: $("clock"),
    theme: $("theme"),
    help: $("help")
  };

  const state = {
    view: "agenda",
    focus: "list",
    token: "",
    courses: [],
    acts: new Map(),
    actsLoading: new Set(),
    details: new Map(),
    content: new Map(), // idCurso → módulos y elementos publicados, en orden
    contentDetails: new Map(),
    contentFiles: new Map(), // idCurso → archivos por ubicación (promesa)
    contentLoading: false,
    contentTried: false,
    contentLoadedAt: 0,
    contentAccount: "", // de quién es el contenido en memoria
    showAll: false,
    filter: "",
    sel: { agenda: 0, courses: 0, mid: 0, miyu: 0, grades: 0, content: 0 },
    loading: false,
    loadedAt: 0
  };

  // ───────────── utilidades ─────────────

  const esc = (text) =>
    String(text == null ? "" : text).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  const norm = (text) =>
    String(text || "").normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

  const nowSec = () => Date.now() / 1000;

  const DAYS = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];
  const MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
  const pad = (n) => String(n).padStart(2, "0");
  const hhmm = (d) => pad(d.getHours()) + ":" + pad(d.getMinutes());

  const fullDate = (epoch) => {
    if (!epoch) return "—";
    const d = new Date(epoch * 1000);
    return DAYS[d.getDay()] + " " + d.getDate() + " " + MONTHS[d.getMonth()] + " " + d.getFullYear() + " · " + hhmm(d);
  };

  const startOfDay = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / 1000;

  // "hoy 23:45", "mañana 09:00", "vie 23:45", "12 oct", "hace 2d"
  const relDue = (epoch) => {
    if (!epoch) return "sin fecha";
    const now = new Date();
    const d = new Date(epoch * 1000);
    const diff = epoch - nowSec();
    if (diff < 0) {
      const ago = -diff;
      if (ago < 3600) return "hace " + Math.max(1, Math.round(ago / 60)) + "m";
      if (ago < 86400) return "hace " + Math.round(ago / 3600) + "h";
      return "hace " + Math.round(ago / 86400) + "d";
    }
    const today = startOfDay(now);
    if (epoch < today + 86400) return diff < 3600 ? "en " + Math.max(1, Math.round(diff / 60)) + "m" : "hoy " + hhmm(d);
    if (epoch < today + 2 * 86400) return "mañana " + hhmm(d);
    if (epoch < today + 7 * 86400) return DAYS[d.getDay()] + " " + hhmm(d);
    return d.getDate() + " " + MONTHS[d.getMonth()] + (d.getFullYear() !== now.getFullYear() ? " " + d.getFullYear() : "");
  };

  // Cuenta regresiva estilo Miyu: "1d 04h 22m", "3h 05m 12s", "vencida hace 2h".
  const countdown = (epoch, withSeconds) => {
    if (!epoch) return "sin fecha";
    let diff = Math.round(epoch - nowSec());
    const past = diff < 0;
    diff = Math.abs(diff);
    const d = Math.floor(diff / 86400);
    const h = Math.floor((diff % 86400) / 3600);
    const m = Math.floor((diff % 3600) / 60);
    const sec = diff % 60;
    let text;
    if (d > 0) text = d + "d " + pad(h) + "h " + pad(m) + "m";
    else if (h > 0) text = h + "h " + pad(m) + "m" + (withSeconds ? " " + pad(sec) + "s" : "");
    else text = m + "m" + (withSeconds ? " " + pad(sec) + "s" : "");
    return past ? "vencida hace " + text : text;
  };

  const shortCourse = (name) => String(name || "").replace(/\s*\(\d+\)\s*$/, "").toLowerCase();

  // Mismo criterio que Miyu --pendientes: hay entrega si fechaEntrega existe y
  // no es la propia fecha límite.
  const isDelivered = (raw) => {
    const entrega = String(raw.fechaEntrega || "").trim();
    if (!entrega || entrega.startsWith("0001")) return false;
    return entrega !== String(raw.fechaTermino || "").trim();
  };

  const normalizeActivity = (raw, course) => ({
    id: raw.idActividad,
    courseId: course.id,
    courseName: course.name,
    title: String(raw.titulo || "sin título").trim(),
    start: raw.fechaInicioEpoch || 0,
    end: raw.fechaTerminoEpoch || 0,
    extension: raw.fechaProrrogaEpoch || 0,
    deadline: raw.fechaProrrogaEpoch || raw.fechaTerminoEpoch || 0,
    delivered: isDelivered(raw),
    deliveredAt: raw.fechaEntregaEpoch || 0,
    grade: raw.calificacion,
    value: raw.valor,
    visible: raw.visible !== 0
  });

  const urgency = (act) => {
    if (act.delivered) return "done";
    const now = nowSec();
    if (act.start && act.start > now) return "future";
    if (!act.deadline) return "normal";
    const diff = act.deadline - now;
    if (diff < 0) return "overdue";
    if (diff < 86400) return "imminent";
    if (diff < 2 * 86400) return "urgent";
    return "normal";
  };

  const GLYPH = { done: "✓", future: "○", overdue: "!", imminent: "◉", urgent: "●", normal: "●" };

  // Las palabras del filtro se separan una vez por filtro, no por fila, y el
  // texto normalizado de cada fila se recuerda entre teclas.
  let wordsFor = null;
  let words = [];
  const filterWords = () => {
    if (wordsFor !== state.filter) {
      wordsFor = state.filter;
      words = norm(state.filter).split(/\s+/).filter(Boolean);
    }
    return words;
  };

  const normCache = new Map();
  const normed = (text) => {
    let hay = normCache.get(text);
    if (hay === undefined) {
      if (normCache.size > 4000) normCache.clear();
      hay = norm(text);
      normCache.set(text, hay);
    }
    return hay;
  };

  const matches = (text) => {
    const list = filterWords();
    if (!list.length) return true;
    const hay = normed(text);
    return list.every((w) => hay.includes(w));
  };

  const highlight = (text) => {
    const list = filterWords();
    if (!list.length) return esc(text);
    const hay = normed(text);
    const marks = new Array(text.length).fill(false);
    for (const w of list) {
      let i = hay.indexOf(w);
      while (i !== -1) {
        for (let k = i; k < i + w.length; k += 1) marks[k] = true;
        i = hay.indexOf(w, i + w.length);
      }
    }
    // Un <mark> por tramo seguido, no por letra.
    let out = "";
    for (let i = 0; i < text.length; ) {
      let j = i + 1;
      while (j < text.length && marks[j] === marks[i]) j += 1;
      const chunk = esc(text.slice(i, j));
      out += marks[i] ? "<mark>" + chunk + "</mark>" : chunk;
      i = j;
    }
    return out;
  };

  // Los modelos de cada vista (agrupar, filtrar, ordenar) se piden varias
  // veces por tecla: se calculan una vez y valen mientras no cambien los
  // datos (dataRev), el filtro o el panel con foco. Lo que depende de la hora
  // se renueva solo cada 15 s.
  let dataRev = 0;
  let memoKey = "";
  let memo = new Map();
  const memoized = (name, build) => {
    const key = dataRev + "|" + state.showAll + "|" + state.focus + "|" + Math.floor(Date.now() / 15000) + "|" + state.filter;
    if (key !== memoKey) {
      memoKey = key;
      memo = new Map();
    }
    let value = memo.get(name);
    if (value === undefined) {
      value = build();
      memo.set(name, value);
    }
    return value;
  };

  // ───────────── estado de la barra ─────────────

  // Mensajes temporales vuelven solos al texto por defecto; con ms = 0 se
  // quedan hasta el siguiente mensaje.
  let msgTimer = null;
  let msgBusy = false;
  const say = (text, ms = 3000) => {
    els.msg.textContent = text;
    clearTimeout(msgTimer);
    msgBusy = true;
    if (ms) {
      msgTimer = setTimeout(() => {
        msgBusy = false;
        els.msg.textContent = defaultMsg();
      }, ms);
    }
  };

  const defaultMsg = () => {
    if (!state.loadedAt) return "";
    const mins = Math.round((Date.now() - state.loadedAt) / 60000);
    return "actualizado " + (mins < 1 ? "ahora" : "hace " + mins + " min");
  };

  const SPINNER = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
  let spinTimer = null;
  const setMode = (mode) => {
    clearInterval(spinTimer);
    els.mode.dataset.mode = mode;
    els.mode.hidden = mode === "normal";
    if (mode === "load") {
      let i = 0;
      spinTimer = setInterval(() => {
        els.mode.textContent = SPINNER[i++ % SPINNER.length] + " CARGANDO";
      }, 80);
      els.mode.textContent = "⠋ CARGANDO";
    } else {
      els.mode.textContent = { normal: "NORMAL", filter: "FILTRO", error: "ERROR" }[mode] || mode.toUpperCase();
    }
  };

  const tickClock = () => {
    const d = new Date();
    els.clock.textContent = DAYS[d.getDay()] + " " + pad(d.getDate()) + " " + hhmm(d);
  };

  // ───────────── API ─────────────

  class AuthError extends Error {}

  const api = async (path, base = API8) => {
    if (!state.token) throw new AuthError("sin sesión");
    const res = await fetch(base + path, { headers: { Authorization: "Bearer " + state.token } });
    if (res.status === 401 || res.status === 403) throw new AuthError("sesión expirada");
    if (!res.ok) throw new Error("HTTP " + res.status);
    return res.json();
  };

  const rowsOf = (json) => (Array.isArray(json) ? json : Array.isArray(json && json.contenido) ? json.contenido : []);

  // La API anterior a veces responde con un solo objeto en vez de una lista.
  const rowsOfOld = async (path) => {
    const json = await api(path, API_OLD);
    const data = json && typeof json === "object" && !Array.isArray(json) && json.contenido != null ? json.contenido : json;
    return Array.isArray(data) ? data : data && typeof data === "object" ? [data] : [];
  };

  const loadCourses = async () => {
    const rows = rowsOf(await api("/Course/getAllCourses"));
    const now = nowSec();
    state.courses = rows
      .map((r) => r.curso || r)
      .filter((c) => c && c.idCurso && !c.archivado)
      .map((c) => ({
        id: c.idCurso,
        name: String(c.nombre || "").trim(),
        end: c.fechaTerminoEpoch || 0,
        favorite: !!c.esFavorito,
        current: !c.fechaTerminoEpoch || c.fechaTerminoEpoch >= now
      }))
      .sort((a, b) => Number(b.current) - Number(a.current) || Number(b.favorite) - Number(a.favorite) || b.end - a.end || a.name.localeCompare(b.name));
    dataRev += 1;
  };

  const loadActivities = async (course) => {
    state.actsLoading.add(course.id);
    try {
      const rows = rowsOf(await api("/Activity/getActividadesEstudiante/" + encodeURIComponent(course.id)));
      state.acts.set(course.id, rows.map((r) => normalizeActivity(r, course)).filter((a) => a.id && a.visible));
      dataRev += 1;
    } finally {
      state.actsLoading.delete(course.id);
    }
  };

  // ── Contenido de los cursos (mismo origen de datos que Miyu --pendientes) ──

  // Los nombres de campo cambian de mayúsculas entre endpoints.
  const pick = (source, fields) => {
    if (!source || typeof source !== "object") return "";
    for (const field of fields) {
      const value = source[field];
      if (value !== null && value !== undefined && String(value).trim() !== "") return value;
    }
    return "";
  };
  const idOf = (value, allowZero) => {
    const raw = String(value == null ? "" : value).trim();
    return /^\d+$/.test(raw) && (allowZero || Number(raw) > 0) ? String(Number(raw)) : "";
  };

  // "12/oct/2026 - 23:59" o ISO → epoch en segundos (0 si no se entiende).
  const parseEminusDate = (text) => {
    const lower = String(text || "").trim().toLowerCase();
    if (!lower) return 0;
    const m = lower.match(/(\d{1,2})\/(\w{3})\/(\d{4})\s*-\s*(\d{1,2}):(\d{2})/);
    if (m && MONTHS.includes(m[2])) return new Date(Number(m[3]), MONTHS.indexOf(m[2]), Number(m[1]), Number(m[4]), Number(m[5])).getTime() / 1000;
    const t = new Date(String(text)).getTime();
    return Number.isNaN(t) ? 0 : t / 1000;
  };

  const PUBLISH_FIELDS = ["fechaPublicacionInicio", "FechaPublicacionInicio", "fechaPublicacion", "FechaPublicacion"];
  const isPublished = (entry) => {
    if (!entry || typeof entry !== "object") return false;
    const visible = entry.visible ?? entry.Visible ?? entry.estado ?? entry.estatus;
    if (["0", "false", "borrador", "draft", "oculto", "eliminado", "eliminada"].includes(String(visible ?? "").trim().toLowerCase())) return false;
    if (Number(visible) === 3) return false;
    return parseEminusDate(pick(entry, PUBLISH_FIELDS)) <= nowSec();
  };

  const FILE_ID_FIELDS = ["idArchivosElementos", "IdArchivosElementos", "idarchivoselementos", "idArchivoElemento", "IdArchivoElemento", "idarchivoelemento", "idArchivo", "IdArchivo", "idarchivo", "id"];
  const FILE_NAME_FIELDS = ["nombreOriginal", "NombreOriginal", "nombreoriginal", "nombreArchivo", "NombreArchivo", "nombrearchivo", "nombre", "Nombre", "name"];
  const ELEMENT_ID_FIELDS = ["idElemento", "IdElemento", "idelemento", "idContenido", "IdContenido", "idcontenido"];
  const UNIT_ID_FIELDS = ["idUnidad", "IdUnidad", "idunidad"];
  const DESC_FIELDS = ["descripcion", "Descripcion", "description", "contenido", "Contenido", "mensaje", "Mensaje", "texto", "Texto", "cuerpo", "Cuerpo", "detalle", "Detalle", "instrucciones", "Instrucciones", "observaciones", "Observaciones"];
  const TITLE_FIELDS = ["titulo", "Titulo", "nombre", "Nombre"];

  const contentFile = (row, unitId) => {
    if (!row || typeof row !== "object") return null;
    const id = idOf(pick(row, FILE_ID_FIELDS));
    if (!id) return null;
    const rawSize = pick(row, ["tamano", "Tamano", "size", "Size", "peso", "Peso"]);
    const bytes = typeof rawSize === "number" ? rawSize : Number(String(rawSize || "").replace(/[^\d.]/g, ""));
    return {
      id,
      elementId: idOf(pick(row, ELEMENT_ID_FIELDS)),
      unitId: idOf(pick(row, UNIT_ID_FIELDS), true) || unitId || "",
      name: String(pick(row, FILE_NAME_FIELDS) || "archivo").trim(),
      size: typeof rawSize === "string" && /[a-z]/i.test(rawSize) ? rawSize.trim() : fileSize({ size: bytes })
    };
  };

  const contentFilesOf = (entry) => {
    const raw = pick(entry, ["archivos", "Archivos", "adjuntos", "Adjuntos", "archivosElementos", "ArchivosElementos"]);
    return Array.isArray(raw) ? raw.map((f) => contentFile(f)).filter(Boolean) : [];
  };

  const descOf = (entry) => {
    const value = pick(entry, DESC_FIELDS);
    return typeof value === "string" ? value : "";
  };

  const loadContent = async (course) => {
    const courseId = encodeURIComponent(course.id);
    const units = (await rowsOfOld("/Contenido/getUnidades/" + courseId + "/0")).filter(isPublished);
    const perUnit = new Array(units.length).fill(null);
    await pool(units.map((unit, i) => [unit, i]), 3, async ([unit, i]) => {
      const unitId = idOf(pick(unit, ["idUnidad", "IdUnidad", "idContenido", "IdContenido", "id"]), true);
      const unitName = String(pick(unit, ["nombre", "Nombre", "unidad", "Unidad", "titulo", "Titulo"]) || "").trim();
      if (!unitId || !unitName) return;
      const base = { courseId: course.id, courseName: course.name, unitId, unitName };
      const items = [{
        ...base,
        kind: "unit",
        id: course.id + ":u:" + unitId,
        elementId: "",
        title: unitName,
        html: descOf(unit),
        files: [],
        published: parseEminusDate(pick(unit, [...PUBLISH_FIELDS, "fechaRegistro", "FechaRegistro", "fechaAlta", "FechaAlta"]))
      }];
      perUnit[i] = items;
      let elements = [];
      try {
        elements = (await rowsOfOld("/Contenido/getElementos/" + courseId + "/" + encodeURIComponent(unitId))).filter(isPublished);
      } catch (err) {
        if (err instanceof AuthError) throw err;
        items[0].partial = true;
      }
      for (const el of elements) {
        const elementId = idOf(pick(el, ["idElemento", "IdElemento", "idContenido", "IdContenido", "id"]));
        const title = String(pick(el, TITLE_FIELDS) || "").trim();
        if (!elementId || !title) continue;
        const files = contentFilesOf(el);
        const flagged = pick(el, ["tieneAdjuntos", "TieneAdjuntos"]);
        items.push({
          ...base,
          kind: "element",
          id: course.id + ":e:" + elementId,
          elementId,
          title,
          html: descOf(el),
          files,
          hasFiles: files.length > 0 || (!!flagged && !["0", "false", "no"].includes(String(flagged).trim().toLowerCase())),
          published: parseEminusDate(pick(el, [...PUBLISH_FIELDS, "fechaRegistro", "FechaRegistro", "fechaAlta", "FechaAlta"]))
        });
      }
    });
    state.content.set(course.id, perUnit.filter(Boolean).flat());
    dataRev += 1;
  };

  // La caché es por cuenta: el id sale del token de Eminus.
  const accountOf = (token) => {
    try {
      const b64 = String(token).split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
      const p = JSON.parse(decodeURIComponent(escape(atob(b64))));
      const id = p.nameid || p.unique_name || p.sub || p.email || p.idPersona || p.matricula;
      return id ? String(id) : "";
    } catch (_) {
      return "";
    }
  };

  // En chrome.storage.local: sobrevive a cerrar el navegador, así la vista
  // abre al instante y se pone al día por detrás.
  const saveContentCache = async () => {
    const account = state.contentAccount;
    if (!account) return;
    const entries = [...state.content.entries()];
    const put = (content) => chrome.storage.local.set({ [CONTENT_CACHE_KEY]: { account, ts: state.contentLoadedAt, content } });
    try {
      await put(entries);
    } catch (_) {
      // No cupo: sin las descripciones de los elementos, que se vuelven a
      // pedir al seleccionarlos.
      await put(entries.map(([id, items]) => [id, items.map((it) => (it.kind === "element" ? { ...it, html: "" } : it))])).catch(() => {});
    }
  };

  // Toma de la caché el contenido de la cuenta en sesión; si la cuenta
  // cambió, lo que había en memoria ya no vale.
  const adoptContentCache = async () => {
    const account = accountOf(state.token);
    if (account === state.contentAccount) return;
    state.contentAccount = account;
    state.content = new Map();
    dataRev += 1;
    state.contentDetails.clear();
    state.contentFiles.clear();
    state.contentLoadedAt = 0;
    state.contentTried = false;
    if (!account) return;
    try {
      const saved = (await chrome.storage.local.get(CONTENT_CACHE_KEY))[CONTENT_CACHE_KEY];
      if (saved && saved.account === account && Array.isArray(saved.content)) {
        state.content = new Map(saved.content);
        dataRev += 1;
        state.contentLoadedAt = saved.ts || 0;
        state.contentTried = true;
      }
    } catch (_) {
    }
  };

  // Pide el contenido si falta o ya es viejo (o siempre, con `force`). Se
  // llama al cargar la app, no al abrir la vista: cuando llegas ya está.
  const ensureContent = (force) => {
    if (!state.token || !state.courses.length) return;
    if (force || !state.contentLoadedAt || Date.now() - state.contentLoadedAt > CONTENT_MAX_AGE) loadAllContent();
  };

  const loadAllContent = async () => {
    if (state.contentLoading || !state.token) return;
    state.contentLoading = true;
    state.contentTried = true;
    const account = state.contentAccount;
    const current = state.courses.filter((c) => c.current);
    const live = () => state.view === "content";
    let done = 0;
    let failed = 0;
    if (live()) setMode("load");
    try {
      await pool(current, 4, async (course) => {
        try {
          await loadContent(course);
        } catch (err) {
          if (err instanceof AuthError) throw err;
          failed += 1;
        }
        done += 1;
        if (live()) say("contenido " + done + "/" + current.length + "…", 0);
        scheduleRender();
      });
      // Cambió la cuenta a media carga: esto ya no es de quien está en sesión.
      if (account !== state.contentAccount) return;
      state.contentFiles.clear();
      state.contentDetails.clear();
      // Con cursos sin leer no se da por fresco: se reintenta al volver.
      if (!failed) {
        state.contentLoadedAt = Date.now();
        saveContentCache();
      }
      if (!state.loading) setMode("normal");
      if (failed) say("contenido: no se pudo leer " + failed + (failed === 1 ? " curso" : " cursos"), 6000);
      else if (live()) {
        say(defaultMsg(), 0);
        msgBusy = false;
      }
    } catch (err) {
      setMode("error");
      if (err instanceof AuthError) showLogin(err.message);
      else say("error: " + (err && err.message ? err.message : err), 6000);
    } finally {
      state.contentLoading = false;
      render();
    }
  };

  const pool = async (items, limit, fn) => {
    const queue = [...items];
    const workers = Array.from({ length: Math.min(limit, queue.length) }, async () => {
      while (queue.length) {
        const item = queue.shift();
        try {
          await fn(item);
        } catch (err) {
          if (err instanceof AuthError) throw err;
        }
      }
    });
    await Promise.all(workers);
  };

  const saveCache = () => {
    chrome.storage.session.set({
      [CACHE_KEY]: { ts: state.loadedAt, courses: state.courses, acts: [...state.acts.entries()] }
    }).catch(() => {});
  };

  const restoreCache = async () => {
    try {
      const data = (await chrome.storage.session.get(CACHE_KEY))[CACHE_KEY];
      if (!data || !Array.isArray(data.courses)) return false;
      state.courses = data.courses;
      state.acts = new Map(data.acts || []);
      dataRev += 1;
      state.loadedAt = data.ts || 0;
      return true;
    } catch (_) {
      return false;
    }
  };

  // `manual` (tecla r): también vuelve a leer el contenido aunque sea reciente.
  const refresh = async (manual) => {
    if (state.loading) return;
    state.loading = true;
    setMode("load");
    say("consultando cursos…", 0);
    try {
      await loadCourses();
      const current = state.courses.filter((c) => c.current);
      let done = 0;
      await pool(current, 6, async (course) => {
        await loadActivities(course);
        done += 1;
        say("actividades " + done + "/" + current.length + "…", 0);
        scheduleRender();
      });
      state.loadedAt = Date.now();
      saveCache();
      hideSplash();
      setMode("normal");
      say(defaultMsg(), 0);
      msgBusy = false;
      ensureContent(manual === true);
    } catch (err) {
      if (err instanceof AuthError) {
        showLogin(err.message);
        setMode("error");
      } else {
        setMode("error");
        say("error: " + (err && err.message ? err.message : err), 6000);
      }
    } finally {
      state.loading = false;
      render();
    }
  };

  // ───────────── modelo de vistas ─────────────

  const currentActs = () => memoized("acts", () => {
    const out = [];
    for (const course of state.courses) {
      if (!course.current) continue;
      for (const act of state.acts.get(course.id) || []) out.push(act);
    }
    return out;
  });

  // Agenda: lista plana con cabeceras de grupo. Sólo las filas "item" son
  // seleccionables.
  const agendaModel = () => memoized("agenda", buildAgenda);
  const buildAgenda = () => {
    const now = nowSec();
    const today = startOfDay(new Date());
    const groups = [
      { key: "overdue", label: "vencidas", items: [] },
      { key: "today", label: "hoy", items: [] },
      { key: "tomorrow", label: "mañana", items: [] },
      { key: "week", label: "próximos 7 días", items: [] },
      { key: "later", label: "después", items: [] },
      { key: "nodate", label: "sin fecha", items: [] },
      { key: "done", label: "entregadas", items: [] }
    ];
    const by = Object.fromEntries(groups.map((g) => [g.key, g]));
    for (const act of currentActs()) {
      if (!matches(act.title + " " + act.courseName)) continue;
      if (act.delivered) {
        if (state.showAll) by.done.items.push(act);
        continue;
      }
      const dl = act.deadline;
      if (!dl) by.nodate.items.push(act);
      else if (dl < now) {
        if (state.showAll || now - dl < OVERDUE_WINDOW) by.overdue.items.push(act);
      } else if (dl < today + 86400) by.today.items.push(act);
      else if (dl < today + 2 * 86400) by.tomorrow.items.push(act);
      else if (dl < today + 8 * 86400) by.week.items.push(act);
      else by.later.items.push(act);
    }
    by.overdue.items.sort((a, b) => b.deadline - a.deadline);
    for (const g of groups) if (g.key !== "overdue") g.items.sort((a, b) => (a.deadline || Infinity) - (b.deadline || Infinity));
    by.done.items.sort((a, b) => b.deliveredAt - a.deliveredAt);
    return groups.filter((g) => g.items.length);
  };

  // Pendientes agrupados por materia, como el panel de Miyu.
  const miyuModel = () => memoized("miyu", buildMiyu);
  const buildMiyu = () => {
    const now = nowSec();
    const byCourse = new Map();
    for (const act of currentActs()) {
      if (act.delivered) continue;
      if (act.deadline && act.deadline < now && now - act.deadline > OVERDUE_WINDOW) continue;
      if (!matches(act.title + " " + act.courseName)) continue;
      if (!byCourse.has(act.courseName)) byCourse.set(act.courseName, []);
      byCourse.get(act.courseName).push(act);
    }
    const groups = [...byCourse.entries()].map(([name, items]) => ({
      label: shortCourse(name),
      items: items.sort((a, b) => (a.deadline || Infinity) - (b.deadline || Infinity))
    }));
    groups.sort((a, b) => (a.items[0].deadline || Infinity) - (b.items[0].deadline || Infinity));
    return groups;
  };

  const counts = () => {
    const now = nowSec();
    const today = startOfDay(new Date());
    const c = { overdue: 0, today: 0, soon: 0, week: 0, total: 0 };
    for (const act of currentActs()) {
      if (act.delivered || !act.deadline) continue;
      if (act.deadline < now) {
        if (now - act.deadline < OVERDUE_WINDOW) c.overdue += 1;
        continue;
      }
      c.total += 1;
      if (act.deadline < today + 86400) c.today += 1;
      if (act.deadline - now < 2 * 86400) c.soon += 1;
      if (act.deadline < today + 8 * 86400) c.week += 1;
    }
    return c;
  };

  const visibleCourses = () => memoized("courses", () =>
    state.courses.filter((c) => (state.showAll || c.current) && (state.focus !== "list" || matches(c.name))));

  const courseActs = (course) => memoized("acts:" + course.id, () => {
    const list = (state.acts.get(course.id) || []).slice();
    list.sort((a, b) => (b.start || 0) - (a.start || 0));
    return list.filter((a) => state.focus !== "mid" || matches(a.title));
  });

  // Contenido por materia: cada módulo seguido de sus elementos.
  const contentModel = () => memoized("content", () =>
    state.courses
      .filter((c) => c.current && state.content.has(c.id))
      .map((c) => {
        const items = state.content.get(c.id).filter((it) => matches(it.title + " " + it.unitName + " " + c.name));
        return { label: shortCourse(c.name), count: items.filter((it) => it.kind === "element").length, items };
      })
      .filter((g) => g.items.length));

  const MODELS = { content: () => contentModel(), agenda: () => agendaModel(), miyu: () => miyuModel(), grades: () => gradesModel() };

  // Elementos navegables del panel indicado.
  const itemsOf = (pane) => {
    const model = MODELS[state.view];
    if (model) return pane === "list" ? memoized("flat:" + state.view, () => model().flatMap((g) => g.items)) : [];
    if (pane === "list") return visibleCourses();
    if (pane === "mid") {
      const course = visibleCourses()[state.sel.courses];
      return course ? courseActs(course) : [];
    }
    return [];
  };

  const selKey = (pane) =>
    state.view === "agenda" ? "agenda" : state.view === "miyu" ? "miyu" : state.view === "grades" ? "grades" : state.view === "content" ? "content" : pane === "list" ? "courses" : "mid";

  const selected = (pane) => {
    const items = itemsOf(pane);
    const key = selKey(pane);
    if (state.sel[key] >= items.length) state.sel[key] = Math.max(0, items.length - 1);
    return items[state.sel[key]] || null;
  };

  // ───────────── render ─────────────

  const actRow = (act, index, isSel, withCourse) => {
    const u = urgency(act);
    const due = act.delivered ? "entregada" : relDue(act.deadline);
    return `<div class="row ${withCourse ? "" : "act-row"} u-${u}" role="option" data-index="${index}" aria-selected="${isSel}">
      <span class="st" title="${esc(u)}">${GLYPH[u]}</span>
      <span class="due">${esc(due)}</span>
      ${withCourse ? `<span class="course">${highlight(shortCourse(act.courseName))}</span>` : ""}
      <span class="title">${highlight(act.title)}</span>
    </div>`;
  };

  const progressBar = (done, total, width = 10) => {
    if (!total) return `<span class="bar"><span class="empty">${"░".repeat(width)}</span> 0/0</span>`;
    const filled = Math.round((done / total) * width);
    return `<span class="bar">${"█".repeat(filled)}<span class="empty">${"░".repeat(width - filled)}</span> ${done}/${total}</span>`;
  };

  const renderAgenda = () => {
    const groups = agendaModel();
    const flat = itemsOf("list");
    if (state.sel.agenda >= flat.length) state.sel.agenda = Math.max(0, flat.length - 1);
    els.listTitle.textContent = "agenda" + (state.showAll ? " · todo" : "") + (state.filter ? " · /" + state.filter : "");
    if (!flat.length) {
      els.list.innerHTML = `<div class="empty-note">${
        state.loading && !state.loadedAt
          ? "cargando actividades…"
          : state.filter
            ? "nada coincide con «" + esc(state.filter) + "»"
            : "✓ sin pendientes por ahora.\n\n  a  ver entregadas\n  2  explorar cursos\n  r  actualizar"
      }</div>`;
      return;
    }
    let html = "";
    let index = 0;
    for (const g of groups) {
      html += `<div class="group" data-count="${g.items.length}">${esc(g.label)}</div>`;
      for (const act of g.items) {
        html += actRow(act, index, index === state.sel.agenda, true);
        index += 1;
      }
    }
    els.list.innerHTML = html;
  };

  const renderMiyu = () => {
    const groups = miyuModel();
    const flat = itemsOf("list");
    if (state.sel.miyu >= flat.length) state.sel.miyu = Math.max(0, flat.length - 1);
    els.listTitle.textContent = "pendientes por materia" + (state.filter ? " · /" + state.filter : "");
    if (!flat.length) {
      els.list.innerHTML = `<div class="empty-note">${
        state.filter ? "nada coincide con «" + esc(state.filter) + "»" : "✓ todo entregado."
      }</div>`;
    } else {
      let html = "";
      let index = 0;
      for (const g of groups) {
        html += `<div class="group" data-count="${g.items.length}">${esc(g.label)}</div>`;
        for (const act of g.items) {
          const u = urgency(act);
          html += `<div class="row act-row u-${u}" role="option" data-index="${index}" aria-selected="${index === state.sel.miyu}">
            <span class="st">${GLYPH[u]}</span>
            <span class="due">${esc(u === "overdue" ? "vencida" : u === "future" ? "por iniciar" : countdown(act.deadline))}</span>
            <span class="title">${highlight(act.title)}</span>
          </div>`;
          index += 1;
        }
      }
      els.list.innerHTML = html;
    }
    renderDash();
  };

  const renderDash = () => {
    const now = nowSec();
    const acts = currentActs();
    const pending = acts.filter((a) => !a.delivered && a.deadline && a.deadline >= now && !(a.start && a.start > now));
    pending.sort((a, b) => a.deadline - b.deadline);
    const next = pending[0];
    let html = "";

    html += `<div class="dash-block"><div class="dash-label">siguiente entrega</div>`;
    if (next) {
      const u = urgency(next);
      const span = next.deadline - (next.start || next.deadline - 7 * 86400);
      const elapsed = Math.min(1, Math.max(0, (now - (next.start || now)) / (span || 1)));
      const width = 22;
      const filled = Math.round(elapsed * width);
      html += `<div class="next">
        <div class="next-title">${esc(next.title)}</div>
        <div class="next-course">${esc(shortCourse(next.courseName))}</div>
        <div class="countdown u-${u}" id="next-countdown" data-deadline="${next.deadline}">${esc(countdown(next.deadline, true))}</div>
        <div class="dim">${esc(fullDate(next.deadline))}</div>
        <div class="bar" title="tiempo transcurrido">${"█".repeat(filled)}<span class="empty">${"░".repeat(width - filled)}</span> ${Math.round(elapsed * 100)}%</div>
      </div>`;
    } else {
      html += `<div class="next dim">nada por entregar. disfruta.</div>`;
    }
    html += `</div>`;

    // Carga de la semana: una barra por día con las entregas que vencen.
    const today = startOfDay(new Date());
    const perDay = Array.from({ length: 7 }, (_, i) => ({ start: today + i * 86400, n: 0 }));
    for (const a of acts) {
      if (a.delivered || !a.deadline) continue;
      const i = Math.floor((a.deadline - today) / 86400);
      if (i >= 0 && i < 7) perDay[i].n += 1;
    }
    html += `<div class="dash-block"><div class="dash-label">carga de la semana</div>`;
    perDay.forEach((day, i) => {
      const d = new Date(day.start * 1000);
      const label = i === 0 ? "hoy" : i === 1 ? "mañana" : DAYS[d.getDay()] + " " + d.getDate();
      html += `<div class="load-row ${i === 0 ? "is-today" : ""}">
        <span class="day">${esc(label)}</span>
        <span class="blocks ${day.n >= 3 ? "heavy" : ""}">${day.n ? "■ ".repeat(day.n) : '<span class="dim">·</span>'}</span>
        <span class="n">${day.n || ""}</span>
      </div>`;
    });
    html += `</div>`;

    const byCourse = new Map();
    for (const a of acts) {
      if (!byCourse.has(a.courseName)) byCourse.set(a.courseName, { done: 0, total: 0 });
      const entry = byCourse.get(a.courseName);
      entry.total += 1;
      if (a.delivered) entry.done += 1;
    }
    if (byCourse.size) {
      html += `<div class="dash-block"><div class="dash-label">avance por materia</div>`;
      for (const [name, v] of byCourse) {
        html += `<div class="course-progress"><span>${esc(shortCourse(name))}</span>${progressBar(v.done, v.total, 8)}</div>`;
      }
      html += `</div>`;
    }

    const feed = [];
    for (const a of acts) {
      if (a.delivered && a.deliveredAt && now - a.deliveredAt < 7 * 86400) feed.push({ kind: "done", at: a.deliveredAt, act: a });
      if (a.start && a.start <= now && now - a.start < 3 * 86400) feed.push({ kind: "new", at: a.start, act: a });
    }
    feed.sort((a, b) => b.at - a.at);
    html += `<div class="dash-block"><div class="dash-label">reciente</div>`;
    html += feed.length
      ? feed.slice(0, 6).map((f) => `<div class="feed-row">
          <span class="k-${f.kind}">${f.kind === "done" ? "✓" : "+"}</span>
          <span class="when">${esc(relDue(f.at))}</span>
          <span>${esc(f.act.title)}</span>
        </div>`).join("")
      : `<div class="dash-note">sin movimiento en los últimos días</div>`;
    html += `</div>`;
    els.dash.innerHTML = html;
  };

  // ── Notas: calificaciones por materia con promedio ──
  const num = (v) => {
    const n = typeof v === "number" ? v : parseFloat(String(v == null ? "" : v).replace(",", "."));
    return Number.isFinite(n) ? n : null;
  };

  const gradesModel = () => memoized("grades", buildGrades);
  const buildGrades = () => {
    const byCourse = new Map();
    for (const act of currentActs()) {
      const grade = num(act.grade);
      if (grade === null && !act.delivered) continue;
      if (!matches(act.title + " " + act.courseName)) continue;
      if (!byCourse.has(act.courseName)) byCourse.set(act.courseName, []);
      byCourse.get(act.courseName).push(act);
    }
    return [...byCourse.entries()]
      .map(([name, items]) => {
        const graded = items.map((a) => num(a.grade)).filter((g) => g !== null);
        const avg = graded.length ? graded.reduce((x, y) => x + y, 0) / graded.length : null;
        items.sort((a, b) => Number(num(b.grade) !== null) - Number(num(a.grade) !== null) || (b.deliveredAt || 0) - (a.deliveredAt || 0));
        return { name, label: shortCourse(name), avg, graded: graded.length, items };
      })
      .sort((a, b) => a.label.localeCompare(b.label));
  };

  const gradeClass = (g) => (g === null ? "u-future" : g >= 9 ? "u-done" : g >= 7 ? "u-normal" : g >= 6 ? "u-urgent" : "u-overdue");

  const renderGrades = () => {
    const groups = gradesModel();
    const flat = itemsOf("list");
    if (state.sel.grades >= flat.length) state.sel.grades = Math.max(0, flat.length - 1);
    const all = flat.map((a) => num(a.grade)).filter((g) => g !== null);
    const overall = all.length ? (all.reduce((x, y) => x + y, 0) / all.length).toFixed(1) : "—";
    els.listTitle.textContent = "tareas calificadas · promedio " + overall + (state.filter ? " · /" + state.filter : "");
    if (!flat.length) {
      els.list.innerHTML = `<div class="empty-note">${
        state.filter ? "nada coincide con «" + esc(state.filter) + "»" : "aún no hay entregas calificadas.\n\n  las actividades entregadas aparecen aquí\n  como «en revisión» hasta que tengan nota."
      }</div>`;
      return;
    }
    let html = "";
    let index = 0;
    for (const g of groups) {
      html += `<div class="group grade-group" data-count="${g.graded}/${g.items.length} calificadas">${esc(g.label)} · promedio <b class="${gradeClass(g.avg)}">${g.avg === null ? "—" : g.avg.toFixed(1)}</b></div>`;
      for (const act of g.items) {
        const grade = num(act.grade);
        html += `<div class="row act-row ${gradeClass(grade)}" role="option" data-index="${index}" aria-selected="${index === state.sel.grades}">
          <span class="st">${grade === null ? "◌" : "✓"}</span>
          <span class="due grade">${grade === null ? "en revisión" : esc(String(grade)) + (num(act.value) ? " / " + esc(String(act.value)) : "")}</span>
          <span class="title">${highlight(act.title)}</span>
        </div>`;
        index += 1;
      }
    }
    els.list.innerHTML = html;
  };

  // ── Contenido: módulos y elementos publicados, por materia ──
  const contentLabel = (it) =>
    it.kind === "unit" ? "módulo" : it.files.length > 1 ? it.files.length + " archivos" : it.hasFiles ? "archivo" : "mensaje";

  const renderContent = () => {
    if (!state.contentTried && !state.contentLoading && !state.loading && state.token && state.courses.length) loadAllContent();
    const groups = contentModel();
    const flat = itemsOf("list");
    if (state.sel.content >= flat.length) state.sel.content = Math.max(0, flat.length - 1);
    els.listTitle.textContent = "contenido por materia" + (state.contentLoading && flat.length ? " · actualizando…" : "") + (state.filter ? " · /" + state.filter : "");
    if (!flat.length) {
      els.list.innerHTML = `<div class="empty-note">${
        state.contentLoading || state.loading || !state.contentTried
          ? "cargando contenido…"
          : state.filter
            ? "nada coincide con «" + esc(state.filter) + "»"
            : "tus cursos actuales no tienen contenido publicado.\n\n  r  actualizar"
      }</div>`;
      return;
    }
    let html = "";
    let index = 0;
    for (const g of groups) {
      html += `<div class="group" data-count="${g.count}">${esc(g.label)}</div>`;
      for (const it of g.items) {
        const unit = it.kind === "unit";
        html += `<div class="row act-row ${unit ? "c-unit" : "c-el" + (it.hasFiles ? " c-files" : "")}" role="option" data-index="${index}" aria-selected="${index === state.sel.content}">
          <span class="st">${unit ? "▸" : it.hasFiles ? "⬇" : "·"}</span>
          <span class="due">${esc(contentLabel(it))}</span>
          <span class="title">${highlight(it.title)}</span>
        </div>`;
        index += 1;
      }
    }
    els.list.innerHTML = html;
  };

  const renderCourses = () => {
    const courses = visibleCourses();
    if (state.sel.courses >= courses.length) state.sel.courses = Math.max(0, courses.length - 1);
    els.listTitle.textContent = "cursos" + (state.filter && state.focus === "list" ? " · /" + state.filter : "");
    let html = "";
    let lastCurrent = null;
    courses.forEach((c, i) => {
      if (c.current !== lastCurrent) {
        html += `<div class="group" data-count="${courses.filter((x) => x.current === c.current).length}">${c.current ? "actuales" : "anteriores"}</div>`;
        lastCurrent = c.current;
      }
      const acts = state.acts.get(c.id);
      const meta = acts
        ? progressBar(acts.filter((a) => a.delivered).length, acts.length, 8)
        : `<span class="meta">${state.actsLoading.has(c.id) ? "…" : c.current ? "" : new Date(c.end * 1000).getFullYear()}</span>`;
      html += `<div class="row course-row" role="option" data-index="${i}" aria-selected="${i === state.sel.courses}">
        <span class="title">${c.favorite ? "★ " : ""}${highlight(c.name.toLowerCase())}</span>${meta}
      </div>`;
    });
    els.list.innerHTML = html || `<div class="empty-note">${state.filter ? "nada coincide" : "sin cursos"}</div>`;

    const course = courses[state.sel.courses];
    els.midTitle.textContent = course ? shortCourse(course.name) : "actividades";
    if (!course) {
      els.mid.innerHTML = "";
      return;
    }
    if (!state.acts.has(course.id)) {
      els.mid.innerHTML = `<div class="empty-note">cargando actividades…</div>`;
      if (!state.actsLoading.has(course.id)) {
        loadActivities(course).then(render, (err) => {
          if (err instanceof AuthError) showLogin(err.message);
          else say("error: " + err.message);
        });
      }
      return;
    }
    const acts = courseActs(course);
    if (state.sel.mid >= acts.length) state.sel.mid = Math.max(0, acts.length - 1);
    els.mid.innerHTML = acts.length
      ? acts.map((a, i) => actRow(a, i, i === state.sel.mid, false)).join("")
      : `<div class="empty-note">sin actividades publicadas</div>`;
  };

  // ───────────── detalle ─────────────

  // Deja pasar sólo etiquetas de formato; quita atributos (salvo href http/https).
  const ALLOWED = new Set(["P", "BR", "B", "STRONG", "I", "EM", "U", "S", "UL", "OL", "LI", "A", "H1", "H2", "H3", "H4", "H5", "H6", "TABLE", "THEAD", "TBODY", "TR", "TD", "TH", "BLOCKQUOTE", "PRE", "CODE", "SPAN", "DIV", "SUB", "SUP", "HR"]);
  const DROP = new Set(["SCRIPT", "STYLE", "IFRAME", "OBJECT", "EMBED", "FORM", "INPUT", "BUTTON", "TEXTAREA", "SELECT", "LINK", "META", "SVG", "MATH", "NOSCRIPT", "TEMPLATE"]);

  const sanitizeInto = (html, target) => {
    const doc = new DOMParser().parseFromString(String(html || ""), "text/html");
    const clean = (node) => {
      for (const child of [...node.childNodes]) {
        if (child.nodeType === Node.COMMENT_NODE) {
          child.remove();
          continue;
        }
        if (child.nodeType !== Node.ELEMENT_NODE) continue;
        const tag = child.tagName;
        if (DROP.has(tag)) {
          child.remove();
          continue;
        }
        if (tag === "IMG") {
          child.replaceWith(doc.createTextNode("[imagen]"));
          continue;
        }
        clean(child);
        if (!ALLOWED.has(tag)) {
          child.replaceWith(...child.childNodes);
          continue;
        }
        const href = tag === "A" ? child.getAttribute("href") : null;
        for (const attr of [...child.attributes]) child.removeAttribute(attr.name);
        if (href && /^https?:\/\//i.test(href)) {
          child.setAttribute("href", href);
          child.setAttribute("target", "_blank");
          child.setAttribute("rel", "noopener noreferrer");
        }
      }
    };
    clean(doc.body);
    target.replaceChildren(...[...doc.body.childNodes].map((n) => document.importNode(n, true)));
  };

  const detailKey = (act) => act.courseId + ":" + act.id;

  // Las descripciones pueden pesar (traen imágenes incrustadas): se guardan
  // sólo las más recientes; las demás se vuelven a pedir si hacen falta.
  const MAX_DETAILS = 60;
  const trim = (map) => {
    while (map.size > MAX_DETAILS) map.delete(map.keys().next().value);
  };

  const fetchDetail = (act) => {
    const key = detailKey(act);
    if (!state.details.has(key)) {
      const p = api("/Activity/getActividadEstudiante/" + encodeURIComponent(act.courseId) + "/" + encodeURIComponent(act.id))
        .then((json) => {
          const row = rowsOf(json)[0] || (json && json.contenido) || null;
          state.details.set(key, { ok: true, data: row });
        })
        .catch((err) => {
          state.details.set(key, { ok: false, error: err });
          if (err instanceof AuthError) showLogin(err.message);
        });
      state.details.set(key, { pending: p });
      trim(state.details);
    }
    return state.details.get(key);
  };

  const fileName = (f) => f && (f.nombre || f.nombreArchivo || f.nombreOriginal || f.name || f.archivo || "");
  const fileSize = (f) => {
    const bytes = Number(f && (f.tamano || f.tamanio || f.size || f.peso));
    if (!bytes) return "";
    return bytes > 1048576 ? (bytes / 1048576).toFixed(1) + " MB" : Math.max(1, Math.round(bytes / 1024)) + " KB";
  };

  let fetchTimer = null;
  let prefetchTimer = null;
  let detailShown = "";

  // Descarga en segundo plano la descripción de las filas vecinas para que
  // al bajar con j ya esté lista.
  const prefetchAround = () => {
    clearTimeout(prefetchTimer);
    prefetchTimer = setTimeout(() => {
      const pane = state.view === "courses" ? "mid" : "list";
      if (state.view === "content" || (state.view === "courses" && state.focus === "list")) return;
      const items = itemsOf(pane);
      const i = state.sel[selKey(pane)];
      for (const act of [items[i + 1], items[i - 1], items[i + 2]]) {
        if (act && !state.details.has(detailKey(act))) fetchDetail(act);
      }
    }, 400);
  };

  const renderActivityDetail = (act) => {
    const key = detailKey(act);
    const u = urgency(act);
    const status = {
      done: "✓ entregada " + (act.deliveredAt ? fullDate(act.deliveredAt) : ""),
      future: "○ por iniciar",
      overdue: "! vencida, sin entregar",
      imminent: "◉ vence en menos de 24 h",
      urgent: "● vence en menos de 48 h",
      normal: "● pendiente"
    }[u];

    const cached = state.details.get(key);
    const body = document.createElement("div");
    body.innerHTML = `
      <h3>${esc(act.title)}</h3>
      <div class="sub">${esc(act.courseName.toLowerCase())}</div>
      <dl class="kv">
        <dt>estado</dt><dd class="u-${u}"><span class="st">${esc(status)}</span></dd>
        <dt>vence</dt><dd>${esc(fullDate(act.deadline))} <span class="dim">(${esc(relDue(act.deadline))})</span></dd>
        <dt>inicia</dt><dd>${esc(fullDate(act.start))}</dd>
        ${act.extension ? `<dt>prórroga</dt><dd>${esc(fullDate(act.extension))}</dd>` : ""}
        <dt>valor</dt><dd>${esc(act.value == null ? "—" : act.value)}</dd>
        <dt>calificación</dt><dd>${esc(act.grade == null ? "sin calificar" : act.grade)}</dd>
      </dl>
      <div class="section-label">descripción</div>
      <div class="rich" id="rich"></div>
      <div id="files"></div>
      <div class="actions">
        <button type="button" class="btn" data-action="open-activity">abrir en eminus</button>
        <button type="button" class="btn" data-action="open-course">ir al curso</button>
      </div>`;
    const rich = body.querySelector("#rich");
    if (!cached || cached.pending) {
      rich.innerHTML = `<span class="dim">cargando…</span>`;
      clearTimeout(fetchTimer);
      fetchTimer = setTimeout(() => {
        fetchDetail(act).pending?.then(() => {
          if (detailShown === key) renderDetail();
        });
      }, cached ? 0 : 120);
    } else if (!cached.ok) {
      rich.innerHTML = `<span class="dim">no se pudo cargar la descripción (${esc(cached.error && cached.error.message)})</span>`;
    } else {
      const data = cached.data || {};
      if (String(data.descripcion || "").trim()) sanitizeInto(data.descripcion, rich);
      else rich.innerHTML = `<span class="dim">sin descripción</span>`;
      const files = (data.archivosActsConTamano || []).filter(fileName);
      if (files.length) {
        body.querySelector("#files").innerHTML =
          `<div class="section-label">archivos</div><ul class="files">` +
          files.map((f) => `<li>${esc(fileName(f))} <span class="dim">${esc(fileSize(f))}</span></li>`).join("") +
          `</ul>`;
      }
    }
    els.detail.replaceChildren(...body.childNodes);
  };

  // Archivos de un curso según su ubicación (módulo o elemento). Una consulta
  // por curso, compartida por todos sus elementos.
  const courseFiles = (courseId) => {
    if (!state.contentFiles.has(courseId)) {
      state.contentFiles.set(courseId, rowsOfOld("/Contenido/UbicacionArchivos/" + encodeURIComponent(courseId)).then(
        (rows) => rows.flatMap((row) => {
          const unitId = idOf(pick(row, UNIT_ID_FIELDS), true);
          return (Array.isArray(row && row.archivos) ? row.archivos : [row]).map((f) => contentFile(f, unitId)).filter(Boolean);
        }),
        (err) => {
          state.contentFiles.delete(courseId);
          if (err instanceof AuthError) throw err;
          return [];
        }
      ));
    }
    return state.contentFiles.get(courseId);
  };

  // Descripción completa y archivos de un módulo o elemento, al seleccionarlo.
  const fetchContentDetail = (item) => {
    if (!state.contentDetails.has(item.id)) {
      const load = async () => {
        const located = await courseFiles(item.courseId);
        let files = item.kind === "element" ? located.filter((f) => f.elementId === item.elementId) : located.filter((f) => f.unitId === item.unitId);
        let html = item.html;
        if (item.kind === "element") {
          try {
            const row = (await rowsOfOld("/Contenido/getElemento/" + encodeURIComponent(item.courseId) + "/" + encodeURIComponent(item.elementId)))[0] || {};
            html = descOf(row) || html;
            if (!files.length) files = contentFilesOf(row);
          } catch (err) {
            if (err instanceof AuthError) throw err;
          }
        }
        const seen = new Set();
        files = [...item.files, ...files].filter((f) => !seen.has(f.id) && seen.add(f.id));
        return { ok: true, html, files };
      };
      const p = load().then(
        (data) => state.contentDetails.set(item.id, data),
        (err) => {
          state.contentDetails.set(item.id, { ok: false, error: err });
          if (err instanceof AuthError) showLogin(err.message);
        }
      );
      state.contentDetails.set(item.id, { pending: p });
      trim(state.contentDetails);
    }
    return state.contentDetails.get(item.id);
  };

  // Lo que hay que mostrar ahora mismo de un elemento: lo ya cargado o, si
  // aún no llega, lo que venía en la lista.
  const contentView = (item) => {
    const cached = state.contentDetails.get(item.id);
    const ready = cached && cached.ok;
    return { cached, html: ready ? cached.html : item.html, files: ready ? cached.files : item.files };
  };

  const renderContentDetail = (item) => {
    const { cached, html, files } = contentView(item);
    const loading = !cached || cached.pending;
    const body = document.createElement("div");
    body.innerHTML = `
      <h3>${esc(item.title)}</h3>
      <div class="sub">${esc(item.courseName.toLowerCase())}</div>
      <dl class="kv">
        <dt>tipo</dt><dd>${esc(contentLabel({ ...item, files, hasFiles: item.hasFiles || files.length > 0 }))}</dd>
        ${item.kind === "element" ? `<dt>módulo</dt><dd>${esc(item.unitName)}</dd>` : ""}
        <dt>publicado</dt><dd>${esc(fullDate(item.published))}</dd>
      </dl>
      <div class="section-label">descripción</div>
      <div class="rich" id="rich"></div>
      <div class="section-label">archivos</div>
      <div id="files"></div>
      <div class="actions">
        <button type="button" class="btn" data-action="open-content">abrir en eminus</button>
        <button type="button" class="btn" data-action="open-course">ir al curso</button>
      </div>`;
    const rich = body.querySelector("#rich");
    if (String(html || "").trim()) sanitizeInto(html, rich);
    else rich.innerHTML = `<span class="dim">${loading ? "cargando…" : "sin descripción"}</span>`;
    body.querySelector("#files").innerHTML = files.length
      ? `<ul class="files">` + files.map((f, i) => `<li><button type="button" class="file" data-file="${i}" title="Descargar">${esc(f.name)}</button> <span class="dim">${esc(f.size)}</span></li>`).join("") + `</ul>`
      : `<span class="dim">${loading ? "buscando archivos…" : cached && !cached.ok ? "no se pudieron cargar los archivos" : "sin archivos"}</span>`;
    if (loading) {
      clearTimeout(fetchTimer);
      fetchTimer = setTimeout(() => {
        fetchContentDetail(item).pending?.then(() => {
          if (detailShown === "content:" + item.id) renderDetail();
        });
      }, cached ? 0 : 120);
    }
    els.detail.replaceChildren(...body.childNodes);
  };

  const downloadFile = async (item, file) => {
    const elementId = file.elementId || item.elementId;
    if (!elementId) return say("este archivo sólo se puede descargar desde eminus");
    say("descargando " + file.name + "…", 0);
    try {
      const res = await fetch(API_OLD + "/contenido/descargaElementoArchivo/" + [item.courseId, elementId, file.id].map(encodeURIComponent).join("/"), {
        headers: { Authorization: "Bearer " + state.token }
      });
      if (!res.ok) throw new Error("HTTP " + res.status);
      const url = URL.createObjectURL(await res.blob());
      const link = document.createElement("a");
      link.href = url;
      link.download = file.name || "archivo";
      document.body.appendChild(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 30000);
      say("descarga iniciada: " + file.name);
    } catch (err) {
      say("no se pudo descargar " + file.name + " (" + (err && err.message ? err.message : err) + ")", 6000);
    }
  };

  const renderCourseDetail = (course) => {
    const acts = state.acts.get(course.id) || [];
    const pending = acts.filter((a) => !a.delivered && a.deadline && a.deadline > nowSec()).sort((a, b) => a.deadline - b.deadline);
    const overdue = acts.filter((a) => urgency(a) === "overdue");
    els.detail.innerHTML = `
      <h3>${esc(course.name.toLowerCase())}</h3>
      <div class="sub">${course.current ? "curso actual" : "curso anterior"}${course.favorite ? " · ★ favorito" : ""}</div>
      <dl class="kv">
        <dt>termina</dt><dd>${esc(fullDate(course.end))}</dd>
        <dt>entregadas</dt><dd>${progressBar(acts.filter((a) => a.delivered).length, acts.length, 16)}</dd>
        <dt>pendientes</dt><dd>${pending.length}</dd>
        <dt>vencidas</dt><dd class="${overdue.length ? "u-overdue" : ""}"><span class="due">${overdue.length}</span></dd>
      </dl>
      ${pending.length ? `<div class="section-label">lo próximo</div>` + pending.slice(0, 5).map((a) => `<div>${esc(relDue(a.deadline))} · ${esc(a.title)}</div>`).join("") : ""}
      <div class="actions">
        <button type="button" class="btn" data-action="open-course">abrir en eminus</button>
        <button type="button" class="btn" data-action="open-course-activities">ver actividades en eminus</button>
      </div>`;
  };

  const detailTarget = () => {
    if (state.view === "content") return { content: selected("list") };
    if (state.view !== "courses") return { act: selected("list") };
    if (state.focus === "list") return { course: selected("list") };
    return { act: selected("mid"), course: selected("list") };
  };

  const renderDetail = () => {
    const t = detailTarget();
    const key = t.content ? "content:" + t.content.id : t.act ? detailKey(t.act) : t.course ? "course:" + t.course.id : "";
    const cached = t.content ? state.contentDetails.get(t.content.id) : t.act && state.details.get(key);
    // Mismo elemento y nada nuevo que mostrar: no se toca (conserva el scroll).
    const stamp = key + "|" + (cached ? (cached.pending ? "p" : "d") : "-") + "|" + (t.act ? t.act.delivered : "") + "|" + (t.course ? (state.acts.get(t.course.id) || []).length : "") + "|" + Math.floor(Date.now() / 60000);
    if (stamp === detailStamp) return;
    detailStamp = stamp;
    detailShown = key;
    if (t.content) {
      renderContentDetail(t.content);
      els.detail.scrollTop = 0;
    } else if (t.act) {
      renderActivityDetail(t.act);
      els.detail.scrollTop = 0;
      prefetchAround();
    } else if (t.course) {
      renderCourseDetail(t.course);
      els.detail.scrollTop = 0;
    } else {
      els.detail.innerHTML = `<div class="empty-note">nada seleccionado</div>`;
    }
  };
  let detailStamp = "";

  const renderCounters = () => {
    const c = counts();
    const html =
      `<span class="c-overdue${c.overdue ? "" : " zero"}">vencidas <b>${c.overdue}</b></span>` +
      `<span class="c-today${c.today ? "" : " zero"}">hoy <b>${c.today}</b></span>` +
      `<span class="c-soon${c.soon ? "" : " zero"}">48 h <b>${c.soon}</b></span>` +
      `<span class="c-week${c.week ? "" : " zero"}">7 días <b>${c.week}</b></span>`;
    if (els.counters.innerHTML !== html) els.counters.innerHTML = html;
  };

  const scrollSelectedIntoView = () => {
    for (const body of [els.list, els.mid]) {
      const row = body.querySelector('.row[aria-selected="true"]');
      if (row) row.scrollIntoView({ block: "nearest" });
    }
  };

  const render = () => {
    els.main.classList.toggle("view-courses", state.view === "courses");
    els.main.classList.toggle("view-miyu", state.view === "miyu");
    els.paneMid.hidden = state.view !== "courses";
    els.paneDash.hidden = state.view !== "miyu";
    for (const btn of document.querySelectorAll(".views button")) {
      btn.setAttribute("aria-current", btn.dataset.view === state.view ? "page" : "false");
    }
    if (state.view === "agenda") renderAgenda();
    else if (state.view === "miyu") renderMiyu();
    else if (state.view === "grades") renderGrades();
    else if (state.view === "content") renderContent();
    else renderCourses();
    for (const [pane, el] of [["list", els.paneList], ["mid", els.paneMid], ["detail", els.paneDetail]]) {
      el.classList.toggle("focused", state.focus === pane);
    }
    renderCounters();
    renderHints();
    els.theme.textContent = bte.themeById((bte.prefs || bte.DEFAULTS).theme).name;
    scrollSelectedIntoView();
    renderDetail();
  };

  // Varias respuestas de la API seguidas se pintan en un solo cuadro.
  let renderQueued = false;
  const scheduleRender = () => {
    if (renderQueued) return;
    renderQueued = true;
    requestAnimationFrame(() => {
      renderQueued = false;
      render();
    });
  };

  const renderHints = () => {
    const pairs = [["j/k", "mover"], state.view === "courses" && ["h/l", "panel"], ["enter", "abrir"], state.view === "content" && ["D", "descargar"], ["/", "filtrar"], ["?", "atajos"]];
    const html = pairs.filter(Boolean).map(([k, v]) => `<span><kbd>${k}</kbd> ${v}</span>`).join("");
    if (els.hints.innerHTML !== html) els.hints.innerHTML = html;
  };

  // Mover la selección sin reconstruir la lista: sólo cambia qué fila está
  // marcada y el detalle.
  const updateSelection = (pane) => {
    const body = pane === "mid" ? els.mid : els.list;
    const index = state.sel[selKey(pane)];
    const prev = body.querySelector('.row[aria-selected="true"]');
    const next = body.querySelector('.row[data-index="' + index + '"]');
    if (!next) return render();
    if (prev && prev !== next) prev.setAttribute("aria-selected", "false");
    next.setAttribute("aria-selected", "true");
    next.scrollIntoView({ block: "nearest" });
    renderDetail();
  };

  // ───────────── abrir en Eminus ─────────────

  const openInEminus = (target) => {
    if (embedded) {
      toParent({ type: "open", target });
      return;
    }
    const url = EMINUS + "/eminus4/page/course/list#bte=" + encodeURIComponent(JSON.stringify(target));
    chrome.tabs.create({ url });
  };

  const openActivity = (act) => {
    if (!act) return;
    openInEminus({ course: act.courseName, section: "activities", activity: { id: act.id, courseId: act.courseId } });
    say("abriendo «" + act.title + "» en eminus…");
  };

  const openCourse = (course, section) => {
    if (!course) return;
    openInEminus(section ? { course: course.name, courseId: course.id, section } : { course: course.name, courseId: course.id });
    say("abriendo " + course.name.toLowerCase() + " en eminus…");
  };

  const courseOf = (t) => t.course || state.courses.find((c) => c.id === (t.act || t.content || {}).courseId);

  // Abre en Eminus el módulo al que pertenece el elemento.
  const openContent = (item) => {
    if (!item) return;
    openInEminus({ course: item.courseName, courseId: item.courseId, section: "content", unit: item.unitId });
    say("abriendo «" + item.unitName + "» en eminus…");
  };

  // D: descarga el archivo del elemento seleccionado, si es uno solo.
  const downloadSelected = async () => {
    const item = state.view === "content" && selected("list");
    if (!item) return;
    const entry = fetchContentDetail(item);
    if (entry.pending) await entry.pending;
    const { files } = contentView(item);
    if (!files.length) say("«" + item.title + "» no tiene archivos");
    else if (files.length === 1) downloadFile(item, files[0]);
    else say(files.length + " archivos: elige cuál en el detalle");
  };

  els.detail.addEventListener("click", (e) => {
    const file = e.target.closest("[data-file]");
    if (file) {
      const item = detailTarget().content;
      const chosen = item && contentView(item).files[Number(file.dataset.file)];
      if (chosen) downloadFile(item, chosen);
      return;
    }
    const btn = e.target.closest("[data-action]");
    if (!btn) return;
    const t = detailTarget();
    const course = courseOf(t);
    if (btn.dataset.action === "open-content") openContent(t.content);
    if (btn.dataset.action === "open-activity") openActivity(t.act);
    if (btn.dataset.action === "open-course") openCourse(course);
    if (btn.dataset.action === "open-course-activities") openCourse(course, "activities");
  });

  // ───────────── sesión ─────────────

  const LOGO = "┏━╸┏┳┓╻┏┓╻╻ ╻┏━┓   ╺┳╸╻ ╻╻\n┣╸ ┃┃┃┃┃┗┫┃ ┃┗━┓    ┃ ┃ ┃┃\n┗━╸╹ ╹╹╹ ╹┗━┛┗━┛    ╹ ┗━┛╹";

  const showLogin = (reason) => {
    els.splash.hidden = false;
    els.splash.innerHTML = `<div class="splash-box">
      <div class="titlebar"><span>sin sesión</span><span class="dim">${esc(reason || "")}</span></div>
      <pre>${LOGO}</pre>
      <p>Abre Eminus e inicia sesión. Esta pestaña se actualiza sola en cuanto detecte tu sesión.</p>
      <div class="actions"><a class="btn" href="${EMINUS}/eminus4/" target="_blank" rel="noopener">abrir eminus</a></div>
    </div>`;
  };

  const hideSplash = () => {
    els.splash.hidden = true;
    els.splash.innerHTML = "";
  };

  chrome.storage.onChanged.addListener((changes, area) => {
    if (area !== "session" || !changes.bteToken) return;
    const token = changes.bteToken.newValue || "";
    if (token && token !== state.token) {
      state.token = token;
      state.details.clear();
      state.contentDetails.clear();
      state.contentFiles.clear();
      adoptContentCache().then(() => refresh());
    }
  });

  // ───────────── teclado ─────────────

  const panes = () => (state.view === "courses" ? ["list", "mid", "detail"] : ["list", "detail"]);

  const focusPane = (pane) => {
    state.focus = pane;
    if (state.filter) {
      state.filter = "";
      els.filter.value = "";
    }
    render();
  };

  const move = (delta) => {
    if (state.focus === "detail") {
      els.detail.scrollBy({ top: delta * 40 });
      return;
    }
    const items = itemsOf(state.focus);
    if (!items.length) return;
    const key = selKey(state.focus);
    const next = Math.max(0, Math.min(items.length - 1, state.sel[key] + delta));
    if (next === state.sel[key]) return;
    state.sel[key] = next;
    if (key === "courses") {
      state.sel.mid = 0;
      render();
    } else updateSelection(state.focus);
  };

  const jump = (toEnd) => {
    if (state.focus === "detail") {
      els.detail.scrollTo({ top: toEnd ? els.detail.scrollHeight : 0 });
      return;
    }
    const items = itemsOf(state.focus);
    const key = selKey(state.focus);
    state.sel[key] = toEnd ? Math.max(0, items.length - 1) : 0;
    if (key === "courses") {
      state.sel.mid = 0;
      render();
    } else updateSelection(state.focus);
  };

  const activate = () => {
    if (state.view === "content") {
      openContent(selected("list"));
    } else if (state.view !== "courses") {
      openActivity(selected("list"));
    } else if (state.focus === "list") {
      if (selected("list")) focusPane("mid");
    } else if (state.focus === "mid") {
      openActivity(selected("mid"));
    } else {
      const t = detailTarget();
      if (t.act) openActivity(t.act);
      else openCourse(t.course);
    }
  };

  const setView = (view, fromParent) => {
    if (!fromParent) toParent({ type: "view", view });
    if (state.view === view) return;
    state.view = view;
    state.focus = "list";
    state.filter = "";
    els.filter.value = "";
    render();
  };

  const openFilter = () => {
    if (state.focus === "detail") state.focus = "list";
    els.filterWrap.hidden = false;
    els.filter.value = state.filter;
    els.filter.focus();
    setMode("filter");
  };

  const closeFilter = (clear) => {
    if (clear) {
      state.filter = "";
      els.filter.value = "";
    }
    els.filterWrap.hidden = !state.filter;
    els.filter.blur();
    setMode("normal");
    render();
  };

  els.filter.addEventListener("input", () => {
    state.filter = els.filter.value;
    state.sel[selKey(state.focus)] = 0;
    render();
  });

  els.filter.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      closeFilter(true);
    } else if (e.key === "Enter") {
      e.preventDefault();
      closeFilter(false);
    } else if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "j")) {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "k")) {
      e.preventDefault();
      move(-1);
    }
  });

  const cycleTheme = async () => {
    const current = (bte.prefs || bte.DEFAULTS).theme;
    const index = bte.THEMES.findIndex((t) => t.id === current);
    const next = bte.THEMES[(index + 1) % bte.THEMES.length];
    bte.applyPrefs(await bte.savePrefs({ theme: next.id }));
    say("tema: " + next.name);
    render();
  };

  let pendingG = false;
  document.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (!els.help.hidden) {
      if (e.key === "Escape" || e.key === "?" || e.key === "q") {
        e.preventDefault();
        els.help.hidden = true;
      }
      return;
    }
    if (e.target instanceof HTMLInputElement) return;

    if (pendingG) {
      pendingG = false;
      if (e.key === "g") {
        e.preventDefault();
        jump(false);
        return;
      }
    }

    const key = e.key;
    const handled = () => e.preventDefault();
    if (key === "1") (handled(), setView("agenda"));
    else if (key === "2") (handled(), setView("courses"));
    else if (key === "3") (handled(), setView("miyu"));
    else if (key === "4") (handled(), setView("grades"));
    else if (key === "5") (handled(), setView("content"));
    else if (key === "D") (handled(), downloadSelected());
    else if (key === "j" || key === "ArrowDown") (handled(), move(1));
    else if (key === "k" || key === "ArrowUp") (handled(), move(-1));
    else if (key === "d" || key === "PageDown") (handled(), move(10));
    else if (key === "u" || key === "PageUp") (handled(), move(-10));
    else if (key === "g") (handled(), (pendingG = true), setTimeout(() => (pendingG = false), 1000));
    else if (key === "G") (handled(), jump(true));
    else if (key === "l" || key === "ArrowRight" || (key === "Tab" && !e.shiftKey)) {
      handled();
      const list = panes();
      focusPane(list[Math.min(list.length - 1, list.indexOf(state.focus) + 1)] || list[0]);
    } else if (key === "h" || key === "ArrowLeft" || (key === "Tab" && e.shiftKey)) {
      handled();
      const list = panes();
      focusPane(list[Math.max(0, list.indexOf(state.focus) - 1)]);
    } else if (key === "Enter" || key === "o") (handled(), activate());
    else if (key === "O") {
      handled();
      openCourse(courseOf(detailTarget()));
    } else if (key === "/") (handled(), openFilter());
    else if (key === "a") {
      handled();
      state.showAll = !state.showAll;
      if (state.view === "content") say("el contenido sólo se lee de los cursos actuales");
      else say(state.showAll ? (state.view === "agenda" ? "mostrando entregadas y vencidas antiguas" : "mostrando cursos anteriores") : "sólo lo actual");
      render();
    } else if (key === "r") (handled(), refresh(true));
    else if (key === "c") (handled(), cycleTheme());
    else if (key === "?") (handled(), (els.help.hidden = false));
    else if (key === "Escape" && state.filter) (handled(), closeFilter(true));
    else if ((key === "Escape" || key === "q") && embedded) (handled(), toParent({ type: "close" }));
  });

  // Ratón
  const onRowClick = (pane) => (e) => {
    const row = e.target.closest(".row");
    if (!row) return;
    const key = selKey(pane);
    state.sel[key] = Number(row.dataset.index);
    if (key === "courses") state.sel.mid = 0;
    if (state.focus === pane && key !== "courses") return updateSelection(pane);
    state.focus = pane;
    render();
  };
  els.list.addEventListener("click", onRowClick("list"));
  els.mid.addEventListener("click", onRowClick("mid"));
  els.list.addEventListener("dblclick", () => activate());
  els.mid.addEventListener("dblclick", () => activate());
  els.paneDetail.addEventListener("mousedown", () => {
    if (state.focus !== "detail") {
      state.focus = "detail";
      for (const [pane, el] of [["list", els.paneList], ["mid", els.paneMid], ["detail", els.paneDetail]]) el.classList.toggle("focused", pane === "detail");
    }
  });
  for (const btn of document.querySelectorAll(".views button")) btn.addEventListener("click", () => setView(btn.dataset.view));
  els.theme.addEventListener("click", cycleTheme);

  // ───────────── arranque ─────────────

  // Los relojes sólo corren mientras la vista se ve: oculta dentro de Eminus
  // o con la pestaña en segundo plano no hay nada que repintar. Al volver se
  // pone al día de una vez.
  let shown = !embedded;
  let timers = [];
  const awake = () => shown && !document.hidden;

  const tickCountdown = () => {
    const el = state.view === "miyu" && document.getElementById("next-countdown");
    if (el) el.textContent = countdown(Number(el.dataset.deadline), true);
  };

  // `repaint`: al despertar, las fechas relativas que quedaron viejas.
  const syncTimers = (repaint) => {
    const on = awake();
    if (on === timers.length > 0) return;
    for (const id of timers) clearInterval(id);
    timers = [];
    if (!on) return;
    tickClock();
    if (!msgBusy) els.msg.textContent = defaultMsg();
    if (repaint && state.loadedAt && !state.loading) render();
    timers = [
      setInterval(tickClock, 15000),
      // La cuenta regresiva de la vista miyu avanza cada segundo.
      setInterval(tickCountdown, 1000),
      // Refresca fechas relativas ("en 5m") cada minuto.
      setInterval(() => {
        if (!state.loading) render();
        if (!msgBusy) els.msg.textContent = defaultMsg();
      }, 60000)
    ];
  };

  const init = async () => {
    syncTimers(false);
    document.addEventListener("visibilitychange", () => syncTimers(true));
    // La app siempre usa el tema, aunque el reskin de Eminus esté apagado.
    document.documentElement.classList.add("bte");
    document.addEventListener("bte:prefs", () => {
      document.documentElement.classList.add("bte");
      els.theme.textContent = bte.themeById(bte.prefs.theme).name;
    });

    // La vista puede venir en la URL (#agenda, #courses, #miyu…) desde los
    // accesos del encabezado de Eminus.
    const viewFromHash = () => {
      const view = location.hash.slice(1);
      if (VIEWS.includes(view)) setView(view);
    };
    viewFromHash();
    window.addEventListener("hashchange", viewFromHash);

    if (embedded) {
      window.addEventListener("message", (e) => {
        if (e.source !== window.parent || e.origin !== EMINUS) return;
        const data = e.data;
        if (!data || data.bte !== true) return;
        if (data.type === "hide") {
          shown = false;
          syncTimers(false);
          return;
        }
        if (data.type === "view" && VIEWS.includes(data.view)) {
          shown = true;
          // Si cambia de vista, setView ya repinta.
          syncTimers(state.view === data.view);
          // Oculta no se dibuja el fondo: se pide ahora que ya tiene tamaño.
          if (bte.prefs) bte.bg.sync(bte.prefs);
          setView(data.view, true);
          els.list.focus({ preventScroll: true });
          // Estuvo oculta: si los datos ya son viejos, se actualizan sin
          // quitar lo que ya se ve.
          if (state.token && state.loadedAt && Date.now() - state.loadedAt > CACHE_MAX_AGE) refresh();
          else {
            if (!msgBusy) els.msg.textContent = defaultMsg();
            ensureContent();
          }
        }
      });
      els.back.addEventListener("click", () => toParent({ type: "close" }));
      toParent({ type: "ready" });
    }

    // La caché y el token se leen a la vez.
    const [restored, { bteToken }] = await Promise.all([restoreCache(), chrome.storage.session.get("bteToken")]);
    if (restored) render();

    state.token = bteToken || "";
    if (!state.token) {
      if (!restored) showLogin("abre eminus para empezar");
      else say("sin sesión: mostrando lo último guardado", 0);
      render();
      return;
    }
    await adoptContentCache();
    if (!restored || Date.now() - state.loadedAt > CACHE_MAX_AGE) refresh();
    else {
      els.msg.textContent = defaultMsg();
      render();
      ensureContent();
    }
  };

  init();
})();
