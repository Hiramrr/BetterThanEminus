// Contexto de página, secciones del curso, cursor j/k e integración con Miyu.
(() => {
  const bte = globalThis.bte;

  bte.isTop = window.top === window;
  bte.API = "https://eminus.uv.mx/eminusapi8/api";

  // Las secciones se abren pulsando el menú lateral de Eminus (así su router
  // carga el contexto del curso como siempre). El icono de Material es estable
  // aunque Eminus cambie de idioma.
  bte.SECTIONS = [
    { id: "courses", key: "c", label: "cursos", icon: "home", words: "inicio lista home courses" },
    { id: "content", key: "x", label: "contenido", icon: "dashboard", words: "content modulos unidades" },
    { id: "activities", key: "a", label: "actividades", icon: "assignment", words: "tareas activities entregas" },
    { id: "exams", key: "e", label: "exámenes", icon: "book", words: "examenes exams" },
    { id: "members", key: "i", label: "integrantes", icon: "people", words: "members alumnos companeros" },
    { id: "forums", key: "f", label: "foros", icon: "forum", words: "forums discusion" },
    { id: "messages", key: "m", label: "mensajes", icon: "email", words: "messages correo inbox" },
    { id: "lounge", key: "l", label: "sala interactiva", icon: "cast_connected", words: "interactive lounge videollamada" },
    { id: "tracking", key: "s", label: "seguimiento", icon: "timeline", words: "tracking calificaciones progreso" },
    { id: "library", key: "b", label: "biblioteca virtual", icon: "import_contacts", words: "library libros" }
  ];

  bte.sectionById = (id) => bte.SECTIONS.find((s) => s.id === id);

  const norm = (bte.norm = (text) =>
    String(text || "")
      .normalize("NFD")
      .replace(/[̀-ͯ]/g, "")
      .toLowerCase()
      .replace(/\s+/g, " ")
      .trim());

  bte.slug = (text) =>
    norm(text)
      .replace(/\(\d+\)/g, "")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40);

  bte.isVisible = (el) => {
    if (!(el instanceof Element) || !el.isConnected) return false;
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) return false;
    const style = getComputedStyle(el);
    return style.visibility !== "hidden" && style.display !== "none";
  };

  bte.sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  bte.waitFor = async (finder, timeoutMs = 8000, stepMs = 150) => {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      const found = finder();
      if (found) return found;
      await bte.sleep(stepMs);
    }
    return null;
  };

  // ───────────── contexto actual ─────────────

  const asideLinkByIcon = (icon) => {
    const icons = document.querySelectorAll("#m_ver_menu .m-menu__link-icon");
    for (const i of icons) {
      if (i.textContent.trim() === icon) return i.closest("a");
    }
    return null;
  };

  bte.currentSection = () => {
    const active = document.querySelector("#m_ver_menu .m-menu__item--active .m-menu__link-icon");
    if (active) {
      const icon = active.textContent.trim();
      const section = bte.SECTIONS.find((s) => s.icon === icon);
      if (section) return section;
    }
    const path = location.pathname;
    if (path.includes("/page/course/list")) return bte.sectionById("courses");
    if (path.includes("/page/course/content")) return bte.sectionById("content");
    if (path.includes("/page/course/activity") || path.includes("/aplicativoEminus/actividad")) return bte.sectionById("activities");
    if (path.includes("/page/messages")) return bte.sectionById("messages");
    return null;
  };

  bte.currentCourseName = () => {
    const el = document.querySelector(".EMINUSPleca .g-font-size-18");
    const name = el ? el.textContent.trim() : "";
    if (name) {
      try {
        sessionStorage.setItem("bte:courseName", name);
      } catch (_) {
      }
      return name;
    }
    if (location.pathname.includes("/page/course/list")) return "";
    try {
      return sessionStorage.getItem("bte:courseName") || "";
    } catch (_) {
      return "";
    }
  };

  bte.pageContext = () => {
    if (document.querySelector("m-auth")) return { parts: ["login"], title: "login" };
    const section = bte.currentSection();
    const course = bte.currentCourseName();
    const parts = ["~"];
    if (!section || section.id === "courses") {
      parts.push("cursos");
    } else {
      if (course) parts.push(bte.slug(course));
      parts.push(bte.slug(section.label));
    }
    if (location.pathname.includes("/actividad-detalle")) parts.push("detalle");
    const human = [course && section && section.id !== "courses" ? course.toLowerCase() : "", section ? section.label : "eminus"]
      .filter(Boolean)
      .join(" / ");
    return { parts, title: human, section, course };
  };

  // ───────────── navegación ─────────────

  bte.goSection = async (id) => {
    const section = bte.sectionById(id);
    if (!section) return false;
    const link = asideLinkByIcon(section.icon);
    if (link) {
      link.click();
      return true;
    }
    if (id === "courses") {
      location.assign(location.origin + "/eminus4/page/course/list");
      return true;
    }
    const inCourse = !!bte.currentCourseName();
    if (!inCourse) {
      bte.toast("abre un curso primero (" + "g c" + " para ver tus cursos)");
      return false;
    }
    // Fuera del layout de eminus4 (p. ej. detalle de actividad a pantalla
    // completa): vuelve al curso y pulsa la sección al cargar.
    try {
      sessionStorage.setItem("bte:pendingSection", JSON.stringify({ id, ts: Date.now() }));
    } catch (_) {
    }
    location.assign(location.origin + "/eminus4/page/course/content");
    return true;
  };

  bte.resumePendingSection = async () => {
    let pending = null;
    try {
      pending = JSON.parse(sessionStorage.getItem("bte:pendingSection") || "null");
      sessionStorage.removeItem("bte:pendingSection");
    } catch (_) {
    }
    if (!pending || Date.now() - pending.ts > 60000) return;
    const section = bte.sectionById(pending.id);
    if (!section) return;
    const link = await bte.waitFor(() => asideLinkByIcon(section.icon));
    if (link) link.click();
  };

  // Cursos desde la API (misma sesión de Eminus; mismo origen).
  bte.fetchCourses = async (force) => {
    if (!force) {
      try {
        const cached = JSON.parse(sessionStorage.getItem("bte:courses") || "null");
        if (cached && Date.now() - cached.ts < 10 * 60 * 1000) return cached.list;
      } catch (_) {
      }
    }
    const token = localStorage.getItem("accessToken") || sessionStorage.getItem("accessToken");
    if (!token) return [];
    const res = await fetch(bte.API + "/Course/getAllCourses", {
      headers: { Authorization: "Bearer " + token }
    });
    if (!res.ok) throw new Error("HTTP " + res.status);
    const json = await res.json();
    const rows = Array.isArray(json) ? json : json.contenido || json.data || [];
    const now = Date.now() / 1000;
    const list = rows
      .map((row) => row.curso || row)
      .filter((c) => c && c.idCurso && !c.archivado)
      .map((c) => ({
        id: c.idCurso,
        name: String(c.nombre || "").trim(),
        program: c.idPrograma || "",
        teacher: c.idUsuario || "",
        start: c.fechaInicioEpoch || 0,
        end: c.fechaTerminoEpoch || 0,
        favorite: !!c.esFavorito,
        activities: c.actividades || 0,
        current: !c.fechaTerminoEpoch || c.fechaTerminoEpoch >= now
      }))
      .sort((a, b) => Number(b.current) - Number(a.current) || Number(b.favorite) - Number(a.favorite) || b.end - a.end);
    try {
      sessionStorage.setItem("bte:courses", JSON.stringify({ ts: Date.now(), list }));
    } catch (_) {
    }
    return list;
  };

  // Actividades de todos los cursos actuales, para la búsqueda global.
  bte.fetchAllActivities = async (force) => {
    if (!force) {
      try {
        const cached = JSON.parse(sessionStorage.getItem("bte:acts") || "null");
        if (cached && Date.now() - cached.ts < 10 * 60 * 1000) return cached.list;
      } catch (_) {
      }
    }
    const token = localStorage.getItem("accessToken") || sessionStorage.getItem("accessToken");
    if (!token) return [];
    const courses = (await bte.fetchCourses()).filter((c) => c.current);
    const out = [];
    const queue = [...courses];
    const worker = async () => {
      while (queue.length) {
        const course = queue.shift();
        try {
          const res = await fetch(bte.API + "/Activity/getActividadesEstudiante/" + encodeURIComponent(course.id), {
            headers: { Authorization: "Bearer " + token }
          });
          if (!res.ok) continue;
          const json = await res.json();
          const rows = Array.isArray(json) ? json : json.contenido || [];
          for (const a of rows) {
            if (!a || !a.idActividad || a.visible === 0) continue;
            const entrega = String(a.fechaEntrega || "").trim();
            out.push({
              id: a.idActividad,
              courseId: course.id,
              courseName: course.name,
              title: String(a.titulo || "").trim(),
              deadline: a.fechaProrrogaEpoch || a.fechaTerminoEpoch || 0,
              delivered: !!entrega && !entrega.startsWith("0001") && entrega !== String(a.fechaTermino || "").trim()
            });
          }
        } catch (_) {
        }
      }
    };
    await Promise.all([worker(), worker(), worker(), worker()]);
    try {
      sessionStorage.setItem("bte:acts", JSON.stringify({ ts: Date.now(), list: out }));
    } catch (_) {
    }
    return out;
  };

  const findCourseCard = (name) => {
    const wanted = norm(name);
    for (const b of document.querySelectorAll("mat-card.CourseCard .CardHeaderContainer b")) {
      if (norm(b.textContent) === wanted) return b;
    }
    for (const b of document.querySelectorAll("mat-card.CourseCard .CardHeaderContainer b")) {
      const text = norm(b.textContent);
      if (text && (wanted.startsWith(text) || text.startsWith(wanted))) return b;
    }
    return null;
  };

  // Abre un curso pulsando su tarjeta en la lista (Eminus guarda el contexto
  // del curso al hacer clic; replicarlo a mano es frágil).
  bte.openCourse = async (course) => {
    try {
      sessionStorage.setItem("bte:pendingCourse", JSON.stringify({ name: course.name, ts: Date.now() }));
    } catch (_) {
    }
    if (!location.pathname.includes("/page/course/list")) {
      const home = asideLinkByIcon("home");
      if (home) home.click();
      else location.assign(location.origin + "/eminus4/page/course/list");
    }
    await bte.resumePendingCourse();
  };

  bte.resumePendingCourse = async () => {
    let pending = null;
    try {
      pending = JSON.parse(sessionStorage.getItem("bte:pendingCourse") || "null");
    } catch (_) {
    }
    if (!pending || Date.now() - pending.ts > 30000) return;
    const card = await bte.waitFor(
      () => (location.pathname.includes("/page/course/list") ? findCourseCard(pending.name) : null),
      10000
    );
    try {
      sessionStorage.removeItem("bte:pendingCourse");
    } catch (_) {
    }
    if (card) {
      bte.toast("abriendo " + pending.name.toLowerCase());
      // Si el router de Eminus estaba a media navegación (p. ej. al cerrar
      // la vista tui con atrás), descarta el clic: se reintenta.
      for (let attempt = 0; attempt < 4; attempt += 1) {
        const target = attempt === 0 ? card : findCourseCard(pending.name);
        if (!target) break;
        target.click();
        const left = await bte.waitFor(() => !location.pathname.includes("/page/course/list"), 1500, 100);
        if (left) break;
      }
    } else if (location.pathname.includes("/page/course/list")) {
      bte.toast("no encontré «" + pending.name + "» en la lista (¿filtro de cursos?)");
    }
  };

  // Abre una actividad dentro del iframe de actividades una vez cargado su
  // listado (así Eminus ya fijó el contexto del curso).
  bte.resumePendingActivity = async () => {
    let pending = null;
    try {
      pending = JSON.parse(sessionStorage.getItem("bte:pendingActivity") || "null");
    } catch (_) {
    }
    if (!pending || Date.now() - pending.ts > 60000) return;
    const frame = await bte.waitFor(() => {
      if (!location.pathname.includes("/page/course/activity")) return null;
      const f = document.querySelector("iframe.app-b-frame");
      try {
        return f && f.contentDocument && f.contentDocument.querySelector("app-activity-card, app-list-activities") ? f : null;
      } catch (_) {
        return null;
      }
    }, 20000, 250);
    try {
      sessionStorage.removeItem("bte:pendingActivity");
    } catch (_) {
    }
    if (!frame) return;
    const url = new URL(location.origin + "/aplicativoEminus/actividad-detalle/" + encodeURIComponent(pending.id));
    if (pending.courseId) url.searchParams.set("courseId", pending.courseId);
    frame.setAttribute("src", url.toString());
    bte.toast("abriendo actividad…");
  };

  // Guarda un destino {course, section, activity} para que la navegación lo
  // retome paso a paso (lista → curso → sección → actividad).
  bte.storeTarget = (target) => {
    if (!target || typeof target !== "object") return false;
    const now = Date.now();
    try {
      if (typeof target.course === "string" && target.course) {
        sessionStorage.setItem("bte:pendingCourse", JSON.stringify({ name: target.course.slice(0, 200), ts: now }));
      }
      if (bte.sectionById(target.section)) {
        sessionStorage.setItem("bte:pendingSection", JSON.stringify({ id: target.section, ts: now }));
      }
      const activity = target.activity;
      if (activity && /^\d+$/.test(String(activity.id))) {
        sessionStorage.setItem("bte:pendingActivity", JSON.stringify({
          id: String(activity.id),
          courseId: /^\d+$/.test(String(activity.courseId)) ? String(activity.courseId) : "",
          ts: now
        }));
      }
    } catch (_) {
      return false;
    }
    return true;
  };

  // Lleva a un destino desde la vista tui embebida, sin recargar si se puede.
  bte.goToTarget = async (target) => {
    if (!bte.storeTarget(target)) return;
    if (!location.pathname.includes("/page/course/list")) {
      const home = asideLinkByIcon("home");
      if (home) home.click();
      else {
        location.assign(location.origin + "/eminus4/page/course/list");
        return;
      }
    }
    await bte.resumePending();
  };

  // Destinos por URL: #bte={...} (abrir algo) o #bte-app=agenda|courses|miyu
  // (abrir la vista tui). Devuelve la vista a abrir, si la hay.
  bte.consumeHashTarget = () => {
    const app = location.hash.match(/^#bte-app=(agenda|courses|miyu)$/);
    if (app) {
      history.replaceState(history.state, "", location.pathname + location.search);
      return app[1];
    }
    const match = location.hash.match(/^#bte=(.+)$/);
    if (!match) return null;
    history.replaceState(history.state, "", location.pathname + location.search);
    try {
      bte.storeTarget(JSON.parse(decodeURIComponent(match[1])));
    } catch (_) {
    }
    return null;
  };

  bte.resumePending = async () => {
    await bte.resumePendingCourse();
    await bte.resumePendingSection();
    await bte.resumePendingActivity();
  };

  // ───────────── cursor j/k ─────────────

  // Elementos navegables por orden de preferencia. `open` es lo que se pulsa
  // con Enter; si no existe, se pulsa el propio elemento.
  bte.NAV_TARGETS = [
    { sel: "mat-card.CourseCard", open: ".CardHeaderContainer b" },
    { sel: ".m-kt-portlet-unit", open: ".m-alert__text" },
    { sel: ".cardInContent", open: ".m-alert__text" },
    { sel: ".m-kt-portlet-resource", open: "a.kt-widget4__title, a" },
    { sel: "app-activity-card mat-card, app-activity-card .activity-card", open: "a, button:not([disabled]), .g-pointer, .cursor-pointer" },
    { sel: ".mat-row, .mat-mdc-row, table.table tbody tr", open: "a, button:not([disabled])" },
    { sel: ".kt-widget4__item", open: "a" }
  ];

  const inForeignUi = (el) =>
    !!el.closest("#eminus-pending-panel, [id^='ep-'], #bte-statusline, #bte-overlay, .cdk-overlay-container");

  bte.navItems = () => {
    const selector = bte.NAV_TARGETS.map((t) => t.sel).join(", ");
    const all = [...document.querySelectorAll(selector)].filter((el) => !inForeignUi(el) && bte.isVisible(el));
    // Si un navegable contiene a otro, gana el de fuera.
    return all.filter((el) => !all.some((other) => other !== el && other.contains(el)));
  };

  bte.openTarget = (el) => {
    const target = bte.NAV_TARGETS.find((t) => el.matches(t.sel));
    const inner = target && target.open ? el.querySelector(target.open) : null;
    (inner || el).click();
  };

  // ───────────── Miyu --pendientes ─────────────

  // Miyu escucha atajos en document; los eventos DOM cruzan los mundos
  // aislados, así que basta con despacharlos. Nuestro manejador ignora los
  // eventos sintéticos (isTrusted = false) para no reaccionar a ellos.
  bte.miyu = {
    present: () => !!document.getElementById("eminus-pending-panel"),
    collapsed: () => !!document.querySelector("#eminus-pending-panel.ep-collapsed"),
    key: (init) => document.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init })),
    toggle: () => bte.miyu.key({ key: "e", code: "KeyE", altKey: true }),
    today: () => bte.miyu.key({ key: "t", code: "KeyT" }),
    refresh: () => bte.miyu.key({ key: "r", code: "KeyR" }),
    search: () => bte.miyu.key({ key: "/", code: "Slash" }),
    tab: (digit) => bte.miyu.key({ key: String(digit), code: "Digit" + digit })
  };
})();
