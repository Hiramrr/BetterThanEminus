// Arranque: statusline, atajos, paleta y comunicación entre marcos.
(() => {
  const bte = globalThis.bte;
  if (bte.appStarted) return;
  bte.appStarted = true;

  const enabled = () => document.documentElement.classList.contains("bte");

  // ───────────── marcos ─────────────

  // El iframe de actividades es del mismo origen: sus comandos globales se
  // envían al marco superior como evento DOM (cruza los mundos aislados).
  let topDoc = null;
  try {
    topDoc = bte.isTop ? document : window.top.document;
  } catch (_) {
    topDoc = null;
  }

  const sendToTop = (cmd, arg) => {
    if (bte.isTop || !topDoc) {
      runCommand(cmd, arg);
      return;
    }
    topDoc.dispatchEvent(new CustomEvent("bte:cmd", { detail: JSON.stringify({ cmd, arg }) }));
  };

  // ───────────── aviso breve ─────────────

  let toastTimer = null;
  bte.toast = (text, ms = 2600) => {
    if (!bte.isTop && topDoc) {
      sendToTop("toast", text);
      return;
    }
    const msg = document.querySelector("#bte-statusline .bte-msg");
    if (msg) {
      msg.textContent = text;
      msg.classList.add("bte-msg-live");
      clearTimeout(toastTimer);
      toastTimer = setTimeout(() => {
        msg.classList.remove("bte-msg-live");
        msg.textContent = "";
      }, ms);
    }
  };

  // ───────────── statusline ─────────────

  let mode = "normal";
  bte.setMode = (next) => {
    mode = next || (bte.appOpen && bte.appOpen() ? "tui" : readingOn() ? "read" : cursorEl && cursorEl.isConnected ? "nav" : "normal");
    const el = document.querySelector("#bte-statusline .bte-mode");
    if (el) {
      el.dataset.mode = mode;
      el.textContent = { normal: "NORMAL", nav: "NAV", cmd: "CMD", g: "G-", tui: "TUI", read: "LECTURA" }[mode] || mode.toUpperCase();
    }
  };

  const buildStatusline = () => {
    if (!bte.isTop || document.getElementById("bte-statusline")) return;
    const bar = document.createElement("div");
    bar.id = "bte-statusline";
    bar.innerHTML = `
      <span class="bte-seg bte-mode" data-mode="normal">NORMAL</span>
      <span class="bte-seg bte-msg"></span>
      <span class="bte-seg bte-hint bte-clickable" title="Todos los atajos (h)"><span><kbd>:</kbd> comandos</span><span><kbd>h</kbd> ayuda</span></span>
      <span class="bte-seg bte-session" hidden></span>
      <span class="bte-seg bte-miyu bte-clickable" title="Mostrar u ocultar Miyu (alt+e)" hidden></span>
      <span class="bte-seg bte-theme bte-clickable" title="Cambiar tema (c)"></span>
      <span class="bte-seg bte-clock"></span>`;
    bar.querySelector(".bte-theme").addEventListener("click", () => cycleTheme(1));
    bar.querySelector(".bte-miyu").addEventListener("click", () => bte.miyu.toggle());
    bar.querySelector(".bte-hint").addEventListener("click", () => bte.showHelp());
    document.body.appendChild(bar);
    updateStatus();
    tickClock();
  };

  const tickClock = () => {
    const el = document.querySelector("#bte-statusline .bte-clock");
    if (!el) return;
    const now = new Date();
    const days = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];
    el.textContent = days[now.getDay()] + " " + String(now.getDate()).padStart(2, "0") + " " +
      String(now.getHours()).padStart(2, "0") + ":" + String(now.getMinutes()).padStart(2, "0");
  };

  // Accesos a la pestaña TUI junto a la ruta del encabezado. Si Angular vuelve
  // a pintar el encabezado, el MutationObserver los repone.
  const ensureHeaderLinks = (logo) => {
    if (!logo || !logo.parentElement || logo.parentElement.querySelector("#bte-header-links")) return;
    const nav = document.createElement("nav");
    nav.id = "bte-header-links";
    nav.setAttribute("aria-label", "Vistas");
    nav.innerHTML = `<button type="button" data-view="agenda" title="Pendientes por fecha (A)">agenda</button>
      <button type="button" data-view="courses" title="Cursos y actividades en columnas">cursos</button>
      <button type="button" data-view="miyu" title="Pendientes por materia">miyu</button>
      <button type="button" data-view="grades" title="Calificaciones y promedios">tareas calificadas</button>`;
    nav.addEventListener("click", (e) => {
      const btn = e.target.closest("button[data-view]");
      if (!btn) return;
      e.preventDefault();
      e.stopPropagation();
      bte.toggleApp(btn.dataset.view);
    });
    logo.after(nav);
  };

  // Escribir sólo si cambia: el MutationObserver también ve nuestros cambios.
  const setText = (el, text) => {
    if (el.textContent !== text) el.textContent = text;
  };

  let lastPathKey = "";
  const updateStatus = () => {
    if (!bte.isTop) return;
    const ctx = bte.pageContext();
    const pathKey = ctx.parts.join("/");

    const logo = document.querySelector("m-header a.m-brand__logo-wrapper");
    if (logo && logo.getAttribute("data-bte-path") !== ctx.title) logo.setAttribute("data-bte-path", ctx.title);
    ensureHeaderLinks(logo);

    if (pathKey !== lastPathKey) {
      lastPathKey = pathKey;
      if (ctx.course && ctx.section && ctx.section.id !== "courses") recordRecent(ctx.course, ctx.section.id);
      if (ctx.section) document.title = "eminus · " + ctx.title;
      clearCursor();
    }

    const themeEl = document.querySelector("#bte-statusline .bte-theme");
    if (themeEl && bte.prefs) {
      const style = bte.styleById(bte.prefs.style);
      setText(themeEl, style.ownColors ? style.name : style.id === "tui" ? bte.themeById(bte.prefs.theme).name : style.name + " · " + bte.themeById(bte.prefs.theme).name);
    }

    // Miyu pone "Volver a Eminus" fijo abajo a la derecha en el detalle de
    // actividad: se le deja hueco en vez de taparlo.
    const bar = document.getElementById("bte-statusline");
    const hasBackBtn = !!document.getElementById("ep-back-home-btn");
    if (bar && bar.classList.contains("bte-miyu-btn") !== hasBackBtn) bar.classList.toggle("bte-miyu-btn", hasBackBtn);

    const miyuEl = document.querySelector("#bte-statusline .bte-miyu");
    if (miyuEl) {
      const present = bte.miyu.present();
      if (miyuEl.hidden === present) miyuEl.hidden = !present;
      if (present) setText(miyuEl, (bte.miyu.collapsed() ? "○" : "●") + " miyu");
    }

    fitActivityFrame();

    // El cursor pudo vivir en el iframe y desaparecer al navegar dentro de él.
    if (mode === "nav" && !(cursorEl && cursorEl.isConnected)) {
      const doc = childFrameDoc();
      if (!doc || !doc.querySelector(".bte-cursor")) bte.setMode("normal");
    }
  };

  // Eminus da al iframe de actividades el alto de la ventana aunque empieza
  // debajo del banner: su final queda fuera de la pantalla y hay dos scrolls.
  // Se ajusta al hueco visible, así sólo se desplaza la lista.
  const fitActivityFrame = () => {
    const frame = document.querySelector("iframe.app-b-frame");
    if (!frame) return;
    if (!enabled()) {
      frame.style.removeProperty("height");
      return;
    }
    // La app de iOS lo hace crecer con su contenido (un solo scroll).
    if (bte.sizeActivityFrame) return bte.sizeActivityFrame(frame);
    const top = frame.getBoundingClientRect().top + window.scrollY;
    const bar = document.getElementById("bte-statusline");
    const height = Math.max(320, Math.floor(window.innerHeight - top - (bar ? bar.offsetHeight : 0))) + "px";
    if (frame.style.getPropertyValue("height") !== height) frame.style.setProperty("height", height, "important");
  };

  // ───────────── cursor j/k ─────────────

  let cursorEl = null;

  const clearCursor = () => {
    const had = !!cursorEl;
    if (cursorEl) cursorEl.classList.remove("bte-cursor");
    cursorEl = null;
    if (mode === "nav") bte.setMode();
    if (had && !bte.isTop) sendToTop("mode", "normal");
  };

  const setCursor = (el) => {
    if (cursorEl) cursorEl.classList.remove("bte-cursor");
    cursorEl = el;
    if (!el) return;
    el.classList.add("bte-cursor");
    el.scrollIntoView({ block: "nearest", behavior: "auto" });
    if (mode !== "cmd") bte.setMode("nav");
    const items = bte.navItems();
    const pos = items.indexOf(el) + 1;
    const label = (el.innerText || "").trim().split("\n")[0].slice(0, 70);
    bte.toast(pos + "/" + items.length + "  " + label, 4000);
    if (!bte.isTop) sendToTop("mode", "nav");
  };

  // Si este marco no tiene elementos pero contiene el iframe de actividades,
  // la navegación se delega a ese iframe (así j/k funcionan sin hacer clic
  // dentro de él primero).
  const childFrameDoc = () => {
    const frame = document.querySelector("iframe.app-b-frame");
    try {
      return frame && frame.contentDocument && frame.contentDocument.documentElement.classList.contains("bte")
        ? frame.contentDocument
        : null;
    } catch (_) {
      return null;
    }
  };

  const delegate = (action) => {
    if (bte.navItems().length) return false;
    const doc = childFrameDoc();
    if (!doc) return false;
    doc.dispatchEvent(new CustomEvent("bte:nav", { detail: action }));
    return true;
  };

  document.addEventListener("bte:nav", (e) => {
    const action = e.detail;
    if (action === "next") moveCursor(1);
    else if (action === "prev") moveCursor(-1);
    else if (action === "first") jumpCursor(false);
    else if (action === "last") jumpCursor(true);
    else if (action === "clear") clearCursor();
    else if (action === "open" && cursorEl && cursorEl.isConnected) bte.openTarget(cursorEl);
  });

  const moveCursor = (delta) => {
    const items = bte.navItems();
    if (!items.length) {
      if (delegate(delta > 0 ? "next" : "prev")) return;
      window.scrollBy({ top: delta * 80 });
      return;
    }
    let index = cursorEl && cursorEl.isConnected ? items.indexOf(cursorEl) : -1;
    if (index === -1) {
      // Sin cursor: empieza por el primer elemento visible en pantalla.
      const firstVisible = items.findIndex((el) => el.getBoundingClientRect().bottom > 40);
      index = firstVisible === -1 ? 0 : firstVisible;
      setCursor(items[index]);
      return;
    }
    index = Math.max(0, Math.min(items.length - 1, index + delta));
    setCursor(items[index]);
  };

  const jumpCursor = (toEnd) => {
    const items = bte.navItems();
    if (!items.length) {
      if (delegate(toEnd ? "last" : "first")) return;
      window.scrollTo({ top: toEnd ? document.body.scrollHeight : 0 });
      return;
    }
    setCursor(items[toEnd ? items.length - 1 : 0]);
  };

  // ───────────── temas ─────────────

  const cycleTheme = async (delta) => {
    const current = bte.prefs ? bte.prefs.theme : bte.DEFAULTS.theme;
    const index = bte.THEMES.findIndex((t) => t.id === current);
    const next = bte.THEMES[(index + delta + bte.THEMES.length) % bte.THEMES.length];
    await setPrefs({ theme: next.id });
    bte.toast("tema: " + next.name);
  };

  const cycleStyle = async () => {
    const current = bte.prefs ? bte.prefs.style : bte.DEFAULTS.style;
    const index = bte.STYLES.findIndex((s) => s.id === current);
    const next = bte.STYLES[(index + 1) % bte.STYLES.length];
    await setPrefs({ style: next.id });
    bte.toast("estilo: " + next.name + " · " + next.desc);
  };

  const styleItems = () =>
    bte.STYLES.map((st) => ({
      group: "estilos",
      label: st.name,
      meta: bte.prefs && bte.prefs.style === st.id ? "● actual" : st.desc,
      keywords: st.desc,
      run: () => setPrefs({ style: st.id }).then(() => bte.toast("estilo: " + st.name))
    }));

  const setPrefs = async (patch) => {
    const next = await bte.savePrefs(patch);
    bte.applyPrefs(next);
    updateStatus();
    return next;
  };

  // ───────────── fondo con foto ─────────────

  // Se llama desde la paleta al pulsar enter o clic: todavía hay activación
  // del usuario, así que el diálogo de archivos se puede abrir aquí mismo.
  const pickBackground = () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.addEventListener("change", async () => {
      const file = input.files[0];
      if (!file || !file.type.startsWith("image/")) return;
      bte.toast("fondo: procesando imagen…");
      try {
        const meta = await bte.bg.setFile(file);
        const colors = bte.prefs.bgPalette ? (meta.tone ? " · colores tomados de la foto" : " · la foto casi no tiene color") : "";
        bte.toast("fondo: listo" + colors);
      } catch (_) {
        bte.toast("fondo: no se pudo leer la imagen");
      }
    });
    input.click();
  };

  const backgroundItems = () => {
    const items = [{ group: "fondo", label: "fondo: " + (bte.bg.info() ? "cambiar imagen…" : "elegir imagen…"), keywords: "background foto imagen wallpaper", run: pickBackground }];
    if (!bte.bg.info()) return items;
    const p = bte.prefs;
    // Vista previa en vivo en el marco superior (donde está el lienzo); se
    // guarda al soltar y de ahí llega a la vista tui y a los demás marcos.
    const preview = (name, value) => (topDoc || document).documentElement.style.setProperty(name, value);
    items.push(
      { group: "fondo", label: "fondo: " + (p.bgStyle === "dither" ? "liso (sin tramado)" : "tramado con puntos"), keywords: "background dither puntos efecto", run: () => setPrefs({ bgStyle: p.bgStyle === "dither" ? "plain" : "dither" }) },
      { group: "fondo", label: (p.bgPalette ? "usar colores del tema" : "usar colores de la foto como tema"), keywords: "fondo background paleta colores foto", run: () => setPrefs({ bgPalette: !p.bgPalette }) },
      {
        group: "fondo",
        label: "fondo: intensidad",
        keywords: "background foto opacidad slider deslizador tenue intenso",
        range: {
          min: 10, max: 100, step: 5, value: p.bgStrength, format: (v) => v + "%",
          onInput: (v) => preview("--bte-bg-strength", String(v / 100)),
          onCommit: (v) => setPrefs({ bgStrength: v })
        }
      },
      {
        group: "fondo",
        label: "fondo: opacidad de paneles",
        keywords: "background encabezado menu lateral vista tui transparencia translucido slider deslizador eminus",
        range: {
          min: 0, max: 100, step: 5, value: p.bgPanel, format: (v) => v + "%",
          onInput: (v) => preview("--bte-panel-alpha", v + "%"),
          onCommit: (v) => setPrefs({ bgPanel: v })
        }
      },
      {
        group: "fondo",
        label: "fondo: opacidad de tarjetas",
        keywords: "background cursos cajas portlets transparencia translucido slider deslizador eminus",
        range: {
          min: 0, max: 100, step: 5, value: p.bgCard, format: (v) => v + "%",
          onInput: (v) => preview("--bte-card-alpha", v + "%"),
          onCommit: (v) => setPrefs({ bgCard: v })
        }
      },
      { group: "fondo", label: "fondo: acomodar (mover, zoom, girar)…", keywords: "background encuadre posicion mover arrastrar rotar girar voltear espejo zoom", run: () => sendToTop("bgEdit") },
      {
        group: "fondo",
        label: "fondo: zoom",
        keywords: "background acercar alejar escala tamaño slider deslizador",
        range: {
          min: 50, max: 400, step: 10, value: Math.round(p.bgZoom / 10) * 10, format: (v) => v + "%",
          onInput: (v) => sendToTop("bgPreview", { bgZoom: v }),
          onCommit: (v) => setPrefs({ bgZoom: v }).then(() => sendToTop("bgPreview", null))
        }
      },
      {
        group: "fondo",
        label: "fondo: girar",
        keywords: "background rotar rotacion angulo grados slider deslizador",
        range: {
          min: -180, max: 180, step: 5, value: Math.round(p.bgRotate / 5) * 5, format: (v) => v + "°",
          onInput: (v) => sendToTop("bgPreview", { bgRotate: v }),
          onCommit: (v) => setPrefs({ bgRotate: v }).then(() => sendToTop("bgPreview", null))
        }
      },
      { group: "fondo", label: "fondo: voltear horizontal", meta: p.bgFlipX ? "● volteada" : "", keywords: "background espejo reflejar invertir", run: () => setPrefs({ bgFlipX: !p.bgFlipX }) },
      { group: "fondo", label: "fondo: voltear vertical", meta: p.bgFlipY ? "● volteada" : "", keywords: "background espejo reflejar invertir de cabeza", run: () => setPrefs({ bgFlipY: !p.bgFlipY }) },
      { group: "fondo", label: "fondo: restablecer encuadre", keywords: "background centrar reiniciar posicion zoom giro", run: () => setPrefs({ ...bte.BG_FRAME_DEFAULTS }).then(() => bte.toast("fondo: encuadre restablecido")) },
      { group: "fondo", label: "fondo: quitar imagen", keywords: "background borrar eliminar", run: () => bte.bg.clear().then(() => bte.toast("fondo: quitado")) }
    );
    return items;
  };

  // ───────────── recientes ─────────────

  const RECENT_KEY = "bteRecent";

  const readRecent = async () => {
    try {
      const data = await chrome.storage.local.get(RECENT_KEY);
      return Array.isArray(data[RECENT_KEY]) ? data[RECENT_KEY] : [];
    } catch (_) {
      return [];
    }
  };

  const recordRecent = async (course, section) => {
    const list = (await readRecent()).filter((r) => !(r.course === course && r.section === section));
    list.unshift({ course, section, ts: Date.now() });
    try {
      await chrome.storage.local.set({ [RECENT_KEY]: list.slice(0, 10) });
    } catch (_) {
    }
  };

  const recentItems = async () =>
    (await readRecent()).map((r) => {
      const section = bte.sectionById(r.section);
      return {
        group: "recientes",
        label: r.course.toLowerCase() + " / " + (section ? section.label : r.section),
        meta: bte.draftAgo ? bte.draftAgo(r.ts) : "",
        keywords: "reciente continuar volver",
        run: () => {
          if (bte.norm(bte.currentCourseName()) === bte.norm(r.course)) bte.goSection(r.section);
          else bte.goToTarget({ course: r.course, section: r.section });
        }
      };
    });

  bte.openRecentPalette = () =>
    bte.openPalette({ title: "recientes", sigil: "cd -", placeholder: "curso o sección…", items: recentItems });

  // ───────────── búsqueda global de actividades ─────────────

  const activityItems = async () => {
    const acts = await bte.fetchAllActivities();
    const now = Date.now() / 1000;
    return acts
      .sort((a, b) => Number(a.delivered) - Number(b.delivered) || (a.deadline || Infinity) - (b.deadline || Infinity))
      .map((a) => {
        let meta = a.delivered ? "✓ entregada" : !a.deadline ? "sin fecha" : a.deadline < now ? "! vencida" : "vence " + new Date(a.deadline * 1000).toLocaleDateString("es-MX", { day: "numeric", month: "short" });
        return {
          group: a.delivered ? "entregadas" : "pendientes",
          label: a.title,
          meta: a.courseName.replace(/\s*\(\d+\)\s*$/, "").toLowerCase() + " · " + meta,
          keywords: a.courseName,
          run: () => bte.goToTarget({ course: a.courseName, section: "activities", activity: { id: a.id, courseId: a.courseId } })
        };
      });
  };

  bte.openActivityPalette = () =>
    bte.openPalette({ title: "actividades de todos tus cursos", sigil: "find", placeholder: "título o curso…", items: activityItems });

  // ───────────── borradores ─────────────

  const draftItems = () =>
    (bte.listDrafts ? bte.listDrafts() : []).map((d) => ({
      group: "borradores (enter copia el texto)",
      label: d.text.replace(/\s+/g, " ").slice(0, 80),
      meta: bte.draftAgo(d.ts) + " · " + (d.label || ""),
      run: async () => {
        try {
          await navigator.clipboard.writeText(d.text);
          bte.toast("borrador copiado: pégalo donde lo necesites");
        } catch (_) {
          bte.toast("no pude copiar al portapapeles");
        }
      }
    }));

  // ───────────── modo lectura ─────────────

  const readingOn = () => document.documentElement.classList.contains("bte-reading");

  const toggleReading = (force) => {
    const on = typeof force === "boolean" ? force : !readingOn();
    document.documentElement.classList.toggle("bte-reading", on);
    bte.setMode(on ? "read" : "normal");
    bte.toast(on ? "modo lectura · z o esc para salir" : "modo lectura desactivado", 1600);
  };

  // ───────────── sesión de Eminus ─────────────

  // El token de Eminus dura 2 horas desde que inicias sesión y Eminus no
  // avisa. Se muestra el tiempo restante en la barra cuando queda poco.
  const warned = new Set();
  const tokenExp = () => {
    const token = localStorage.getItem("accessToken") || "";
    try {
      const payload = JSON.parse(atob(token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/")));
      return typeof payload.exp === "number" ? payload.exp : 0;
    } catch (_) {
      return 0;
    }
  };

  const tickSession = () => {
    const el = document.querySelector("#bte-statusline .bte-session");
    if (!el) return;
    // En el login no hay sesión que vigilar (el token guardado es de la
    // sesión anterior).
    const exp = document.querySelector("m-auth") ? 0 : tokenExp();
    if (!exp) {
      el.hidden = true;
      return;
    }
    const left = Math.round(exp - Date.now() / 1000);
    const mins = Math.ceil(left / 60);
    el.hidden = left > 20 * 60;
    el.dataset.level = left <= 0 ? "expired" : left <= 3 * 60 ? "crit" : left <= 10 * 60 ? "warn" : "ok";
    setText(el, left <= 0 ? "sesión vencida" : "sesión " + mins + " min");
    el.title = "Tu sesión de Eminus vence a las " + new Date(exp * 1000).toLocaleTimeString("es-MX", { hour: "2-digit", minute: "2-digit" });
    for (const limit of [10, 3, 0]) {
      const id = exp + ":" + limit;
      if (left <= limit * 60 && !warned.has(id)) {
        warned.add(id);
        bte.toast(
          limit === 0
            ? "tu sesión de eminus venció: vuelve a entrar; lo que escribiste quedó en borradores"
            : "tu sesión de eminus vence en " + limit + " min · lo que escribes se guarda como borrador",
          8000
        );
        break;
      }
    }
  };

  // ───────────── comandos ─────────────

  const courseItems = async () => {
    const courses = await bte.fetchCourses();
    return courses.map((c) => ({
      group: c.current ? "cursos actuales" : "cursos anteriores",
      label: c.name.toLowerCase(),
      meta: (c.favorite ? "★ " : "") + (c.current ? "actual" : c.end ? String(new Date(c.end * 1000).getFullYear()) : ""),
      keywords: c.teacher + " " + c.program,
      run: () => bte.openCourse(c)
    }));
  };

  bte.openCoursePalette = () =>
    bte.openPalette({ title: "cursos", sigil: "cd", placeholder: "nombre del curso…", items: courseItems });

  const themeItems = () =>
    bte.THEMES.map((t) => ({
      group: "temas",
      label: t.name,
      meta: bte.prefs && bte.prefs.theme === t.id ? "● actual" : t.desc || (t.dark ? "oscuro" : "claro"),
      keywords: (t.a11y ? "accesible accesibilidad " : "") + (t.dark ? "oscuro" : "claro"),
      run: () => setPrefs({ theme: t.id }).then(() => bte.toast("tema: " + t.name))
    }));

  const fontItems = () =>
    bte.FONTS.map((f) => ({
      group: "fuente",
      label: f.name.toLowerCase(),
      meta: bte.prefs && bte.prefs.font === f.id ? "● actual" : "",
      run: () => setPrefs({ font: f.id }).then(() => bte.toast("fuente: " + f.name))
    }));

  bte.openCommandPalette = () => {
    const ctx = bte.pageContext();
    const inCourse = !!ctx.course || !!document.querySelector("#m_ver_menu");
    const items = [];

    items.push({
      group: "ir a",
      label: "actividades: buscar en todos los cursos…",
      hint: ";",
      keywords: "find buscar tarea actividad global",
      run: () => bte.openActivityPalette()
    });

    items.push({
      group: "ir a",
      label: "recientes: volver a donde estabas…",
      hint: "g r",
      keywords: "historial continuar volver reciente",
      run: () => bte.openRecentPalette()
    });

    items.push({
      group: "ir a",
      label: "cursos: buscar y abrir…",
      hint: "ñ",
      keywords: "cd abrir curso materia",
      run: () => bte.openCoursePalette()
    });

    for (const s of bte.SECTIONS) {
      if (s.id !== "courses" && !inCourse) continue;
      items.push({
        group: "ir a",
        label: s.id === "courses" ? "lista de cursos" : s.label,
        hint: "g " + s.key,
        keywords: s.words,
        run: () => bte.goSection(s.id)
      });
    }

    items.push(
      { group: "ir a", label: "atrás", hint: "H", keywords: "back volver", run: () => history.back() },
      { group: "ir a", label: "recargar página", keywords: "reload refresh", run: () => location.reload() },
      { group: "vistas", label: "agenda de pendientes", hint: "A", keywords: "app tui fechas vence", run: () => bte.showApp("agenda") },
      { group: "vistas", label: "cursos en columnas", keywords: "app tui actividades", run: () => bte.showApp("courses") },
      { group: "vistas", label: "pendientes por materia (miyu)", keywords: "app tui miyu", run: () => bte.showApp("miyu") },
      { group: "vistas", label: "tareas calificadas", keywords: "app tui notas calificaciones promedio grades", run: () => bte.showApp("grades") }
    );

    if (bte.miyu.present()) {
      items.push(
        { group: "miyu --pendientes", label: "mostrar / ocultar panel", hint: "alt e", keywords: "tareas pendientes", run: () => bte.miyu.toggle() },
        { group: "miyu --pendientes", label: "tareas de hoy", hint: "t", keywords: "hoy today", run: () => bte.miyu.today() },
        { group: "miyu --pendientes", label: "actualizar pendientes", hint: "r", keywords: "refresh escanear", run: () => bte.miyu.refresh() },
        { group: "miyu --pendientes", label: "buscar en pendientes", hint: "/", keywords: "filtrar", run: () => bte.miyu.search() }
      );
    }

    items.push(
      { group: "apariencia", label: "estilo: elegir…", hint: "e", keywords: "diseño look skin apariencia", run: () => bte.openPalette({ title: "estilos", sigil: "set style=", items: styleItems() }) },
      { group: "apariencia", label: "tema: elegir…", hint: "c", keywords: "color colores theme paleta", run: () => bte.openPalette({ title: "temas", sigil: "set theme=", items: themeItems() }) },
      { group: "apariencia", label: "fuente: elegir…", keywords: "font tipografia letra", run: () => bte.openPalette({ title: "fuente", sigil: "set font=", items: fontItems() }) },
      { group: "apariencia", label: "zoom: más grande", keywords: "texto letra font size aumentar", run: () => setPrefs({ fontSize: bte.prefs.fontSize + 1 }).then((p) => bte.toast("zoom: " + Math.round((p.fontSize / 14) * 100) + "%")) },
      { group: "apariencia", label: "zoom: más pequeño", keywords: "texto letra font size reducir", run: () => setPrefs({ fontSize: bte.prefs.fontSize - 1 }).then((p) => bte.toast("zoom: " + Math.round((p.fontSize / 14) * 100) + "%")) },
      { group: "apariencia", label: "zoom: normal", keywords: "texto letra font size restablecer 100", run: () => setPrefs({ fontSize: 14 }).then(() => bte.toast("zoom: 100%")) },
      { group: "apariencia", label: (bte.prefs.scanlines ? "desactivar" : "activar") + " scanlines crt", keywords: "retro crt lineas", run: () => setPrefs({ scanlines: !bte.prefs.scanlines }) },
      { group: "apariencia", label: (bte.prefs.brackets ? "quitar" : "poner") + " [ corchetes ] en botones", keywords: "botones", run: () => setPrefs({ brackets: !bte.prefs.brackets }) },
      { group: "apariencia", label: "desactivar reskin (eminus original)", hint: "alt ⇧ b", keywords: "apagar off original tema", run: () => toggleEnabled() }
    );

    items.push(...backgroundItems());

    items.push(
      { group: "herramientas", label: (readingOn() ? "salir del" : "entrar al") + " modo lectura", hint: "z", keywords: "leer enfoque concentracion", run: () => toggleReading() },
      { group: "herramientas", label: "borradores guardados…", keywords: "draft recuperar texto perdido", run: () => bte.openPalette({ title: "borradores", sigil: "✎", items: draftItems() }) }
    );

    items.push({ group: "ayuda", label: "atajos de teclado", hint: "h", keywords: "help ayuda keys", run: () => bte.showHelp() });

    bte.openPalette({ title: "comandos — " + ctx.parts.join("/"), items });
  };

  const toggleEnabled = async () => {
    const next = await setPrefs({ enabled: !enabled() });
    if (!next.enabled) {
      bte.closeOverlay();
      clearCursor();
    } else {
      bte.toast("reskin activado");
    }
  };

  const runCommand = (cmd, arg) => {
    if (!bte.isTop) return;
    switch (cmd) {
      case "palette": bte.openCommandPalette(); break;
      case "courses": bte.openCoursePalette(); break;
      case "activities": bte.openActivityPalette(); break;
      case "recent": bte.openRecentPalette(); break;
      case "reading": toggleReading(); break;
      case "help": bte.showHelp(); break;
      case "section": bte.goSection(arg); break;
      case "theme": cycleTheme(1); break;
      case "style": cycleStyle(); break;
      case "toggle": toggleEnabled(); break;
      case "app": bte.showApp(arg || "agenda"); break;
      case "toast": bte.toast(String(arg || "")); break;
      case "mode": if (!bte.overlayOpen()) bte.setMode(arg === "nav" ? "nav" : "normal"); break;
      case "bgEdit": if (!bte.bg.edit()) bte.toast("fondo: primero elige una imagen"); break;
      case "bgPreview": bte.bg.preview(arg && typeof arg === "object" ? arg : null); break;
      default: break;
    }
  };

  if (bte.isTop) {
    document.addEventListener("bte:cmd", (e) => {
      try {
        const { cmd, arg } = JSON.parse(e.detail);
        runCommand(cmd, arg);
      } catch (_) {
      }
    });
  }

  // ───────────── teclado ─────────────

  let pendingG = false;
  let gTimer = null;

  const isTyping = (el) =>
    el instanceof HTMLInputElement ||
    el instanceof HTMLTextAreaElement ||
    el instanceof HTMLSelectElement ||
    (el instanceof HTMLElement && (el.isContentEditable || el.closest("[contenteditable=''], [contenteditable='true'], .fr-element")));

  const cancelG = () => {
    pendingG = false;
    clearTimeout(gTimer);
    if (mode === "g") bte.setMode();
  };

  document.addEventListener(
    "keydown",
    (e) => {
      // Ignora eventos sintéticos: incluye los que despachamos hacia Miyu.
      if (!e.isTrusted) return;

      if (e.altKey && e.shiftKey && !e.ctrlKey && !e.metaKey && e.code === "KeyB") {
        e.preventDefault();
        sendToTop("toggle");
        return;
      }
      if (!enabled()) return;

      // Ctrl/Cmd+K abre la paleta incluso escribiendo.
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        e.stopPropagation();
        if (bte.isTop && bte.overlayOpen()) bte.closeOverlay();
        else sendToTop("palette");
        return;
      }

      if (bte.isTop && bte.appOpen && bte.appOpen() && !bte.overlayOpen()) {
        if (e.key === "Escape" && !isTyping(e.target)) {
          e.preventDefault();
          bte.hideApp();
          return;
        }
        if (e.key === ":" && !isTyping(e.target)) {
          e.preventDefault();
          bte.openCommandPalette();
        }
        return;
      }

      if (bte.isTop && bte.overlayOpen()) {
        if (e.key === "Escape" || (e.key === "h" && !isTyping(e.target))) {
          e.preventDefault();
          bte.closeOverlay();
        }
        return;
      }

      if (e.key === "F1") {
        e.preventDefault();
        sendToTop("help");
        return;
      }

      if (isTyping(e.target) || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.target instanceof Element && e.target.closest("#eminus-pending-panel")) return;

      if (pendingG) {
        cancelG();
        if (e.key === "g") {
          e.preventDefault();
          jumpCursor(false);
          return;
        }
        if (e.key === "r") {
          e.preventDefault();
          sendToTop("recent");
          return;
        }
        const section = bte.SECTIONS.find((s) => s.key === e.key);
        if (section) {
          e.preventDefault();
          sendToTop("section", section.id);
        }
        return;
      }

      switch (e.key) {
        case ":":
          e.preventDefault();
          sendToTop("palette");
          break;
        case "ñ":
        case "Ñ":
          e.preventDefault();
          sendToTop("courses");
          break;
        case ";":
          e.preventDefault();
          sendToTop("activities");
          break;
        case "z":
          e.preventDefault();
          sendToTop("reading");
          break;
        case "h":
          e.preventDefault();
          sendToTop("help");
          break;
        case "c":
          e.preventDefault();
          sendToTop("theme");
          break;
        case "e":
          e.preventDefault();
          sendToTop("style");
          break;
        case "g":
          e.preventDefault();
          pendingG = true;
          bte.setMode("g");
          gTimer = setTimeout(cancelG, 1200);
          break;
        case "G":
          e.preventDefault();
          jumpCursor(true);
          break;
        case "A":
          e.preventDefault();
          sendToTop("app", "agenda");
          break;
        case "j":
          e.preventDefault();
          moveCursor(1);
          break;
        case "k":
          e.preventDefault();
          moveCursor(-1);
          break;
        case "d":
          e.preventDefault();
          window.scrollBy({ top: window.innerHeight / 2 });
          break;
        case "u":
          e.preventDefault();
          window.scrollBy({ top: -window.innerHeight / 2 });
          break;
        case "H":
          e.preventDefault();
          history.back();
          break;
        case "L":
          e.preventDefault();
          history.forward();
          break;
        case "o":
        case "Enter":
          if (cursorEl && cursorEl.isConnected) {
            e.preventDefault();
            bte.openTarget(cursorEl);
          } else if (mode === "nav" && delegate("open")) {
            e.preventDefault();
          }
          break;
        case "Escape":
          if (bte.isTop && readingOn()) {
            toggleReading(false);
          } else if (cursorEl) {
            clearCursor();
            bte.toast("selección quitada", 1200);
          } else if (mode === "nav" && delegate("clear")) {
            bte.setMode("normal");
          }
          break;
        default:
          break;
      }
    },
    true
  );

  // ───────────── vista tui dentro de Eminus ─────────────

  const runtimeAlive = () => {
    try {
      return !!(chrome.runtime && chrome.runtime.id);
    } catch (_) {
      return false;
    }
  };

  // La vista tui consulta la API con la sesión de Eminus; el token viaja al
  // service worker, que lo guarda sólo en memoria (storage.session).
  let lastToken = null;
  const relayToken = () => {
    if (!runtimeAlive()) return;
    const token = localStorage.getItem("accessToken") || sessionStorage.getItem("accessToken") || "";
    if (token === lastToken) return;
    lastToken = token;
    chrome.runtime.sendMessage({ type: "bte:token", token }).catch(() => {
      lastToken = null;
    });
  };

  // La app (app/app.html) se carga en un iframe que cubre el contenido de
  // Eminus, bajo su encabezado y sobre la barra de estado. Se crea una vez y
  // luego sólo se oculta: volver a Eminus es instantáneo y la página de
  // detrás no se toca. Se habla con ella por postMessage, verificando origen.
  const VIEWS = ["agenda", "courses", "miyu", "grades"];
  // En la app de iOS la vista tui es un documento srcdoc del mismo origen.
  const native = globalThis.__bteNative;
  const EXT_ORIGIN = (() => {
    if (native) return location.origin;
    try {
      return new URL(chrome.runtime.getURL("/")).origin;
    } catch (_) {
      return "";
    }
  })();
  let appHost = null;
  let appFrame = null;
  let appReady = false;
  let appQueued = null;
  let appView = null;

  const postToApp = (msg) => {
    if (!appFrame) return;
    if (!appReady) {
      appQueued = msg;
      return;
    }
    appFrame.contentWindow.postMessage({ bte: true, ...msg }, EXT_ORIGIN);
  };

  const placeAppHost = () => {
    if (!appHost) return;
    // Debajo del encabezado de Eminus; cubre su barra de estado (la app
    // trae la suya).
    const header = document.querySelector("m-header header");
    appHost.style.top = Math.max(0, header ? header.getBoundingClientRect().bottom : 0) + "px";
  };

  const ensureAppHost = () => {
    if (appHost) return;
    appHost = document.createElement("div");
    appHost.id = "bte-app-host";
    appHost.hidden = true;
    appFrame = document.createElement("iframe");
    appFrame.title = "eminus tui";
    if (native) appFrame.srcdoc = native.doc("app/app.html");
    else appFrame.src = chrome.runtime.getURL("app/app.html") + "#embedded";
    appHost.appendChild(appFrame);
    document.body.appendChild(appHost);
    window.addEventListener("resize", placeAppHost);
  };

  const updateHeaderLinkState = () => {
    for (const btn of document.querySelectorAll("#bte-header-links button[data-view]")) {
      btn.setAttribute("aria-pressed", String(bte.appOpen() && btn.dataset.view === appView));
    }
  };

  bte.appOpen = () => !!appHost && !appHost.hidden;

  bte.showApp = (view) => {
    if (!bte.isTop) {
      sendToTop("app", view);
      return;
    }
    if (!runtimeAlive()) {
      bte.toast("recarga la página: la extensión se actualizó");
      return;
    }
    relayToken();
    ensureAppHost();
    appView = VIEWS.includes(view) ? view : "agenda";
    bte.closeOverlay();
    clearCursor();
    // Una entrada en el historial: el botón atrás del navegador cierra la
    // vista en vez de sacarte de la página de Eminus.
    if (appHost.hidden && !(history.state && history.state.bteApp)) {
      history.pushState({ ...(history.state || {}), bteApp: true }, "");
    }
    appHost.hidden = false;
    document.documentElement.classList.add("bte-app-open");
    placeAppHost();
    postToApp({ type: "view", view: appView });
    appFrame.focus();
    bte.setMode("tui");
    updateHeaderLinkState();
    updateStatus();
  };

  // Quita la entrada que puso showApp. Espera a que el navegador la saque
  // para que una navegación posterior de Eminus no se deshaga con ella.
  const dropHistoryEntry = () =>
    new Promise((resolve) => {
      if (!(history.state && history.state.bteApp)) return resolve();
      const done = () => {
        window.removeEventListener("popstate", done);
        clearTimeout(timer);
        resolve();
      };
      const timer = setTimeout(done, 400);
      window.addEventListener("popstate", done);
      history.back();
    });

  // how: "history" (ya se salió con atrás), "keep" (Eminus va a navegar:
  // sólo se quita la marca, sin retroceder) o nada (retrocede la entrada).
  bte.hideApp = async (how) => {
    if (!bte.appOpen()) return;
    appHost.hidden = true;
    document.documentElement.classList.remove("bte-app-open");
    appView = null;
    bte.setMode("normal");
    updateHeaderLinkState();
    updateStatus();
    window.focus();
    if (how === "keep") {
      if (history.state && history.state.bteApp) {
        const { bteApp, ...rest } = history.state;
        history.replaceState(rest, "");
      }
    } else if (how !== "history") await dropHistoryEntry();
  };

  window.addEventListener("popstate", () => {
    if (bte.isTop && bte.appOpen() && !(history.state && history.state.bteApp)) bte.hideApp("history");
  });

  // Cualquier clic en el encabezado o el menú de Eminus (logo, inicio,
  // perfil) vuelve a Eminus; sólo los botones de las vistas no.
  document.addEventListener(
    "click",
    (e) => {
      if (!bte.isTop || !bte.appOpen() || !(e.target instanceof Element)) return;
      if (e.target.closest("#bte-header-links")) return;
      if (e.target.closest("m-header, #m_ver_menu")) bte.hideApp("keep");
    },
    true
  );

  // Mismo botón: abre esa vista o, si ya está abierta, vuelve a Eminus.
  bte.toggleApp = (view) => {
    if (bte.appOpen() && appView === view) bte.hideApp();
    else bte.showApp(view);
  };

  window.addEventListener("message", (e) => {
    if (!appFrame || e.source !== appFrame.contentWindow || e.origin !== EXT_ORIGIN) return;
    const data = e.data;
    if (!data || data.bte !== true) return;
    if (data.type === "ready") {
      appReady = true;
      if (appQueued) {
        postToApp(appQueued);
        appQueued = null;
      }
    } else if (data.type === "close") {
      bte.hideApp();
    } else if (data.type === "view" && VIEWS.includes(data.view)) {
      appView = data.view;
      updateHeaderLinkState();
    } else if (data.type === "open" && data.target && typeof data.target === "object") {
      bte.toast("abriendo en eminus…");
      bte.hideApp().then(() => bte.goToTarget(data.target));
    }
  });

  // El popup pide abrir la vista en la pestaña actual.
  if (bte.isTop && runtimeAlive()) {
    chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
      if (sender.id !== chrome.runtime.id || !message) return false;
      if (message.type === "bte:bgEdit") {
        bte.hideApp && bte.appOpen() && bte.hideApp();
        sendResponse({ ok: bte.bg.edit() });
        return false;
      }
      if (message.type !== "bte:showApp") return false;
      bte.showApp(message.view);
      sendResponse({ ok: true });
      return false;
    });
  }

  // ───────────── ciclo de vida ─────────────

  let scheduled = false;
  const scheduleUpdate = () => {
    if (scheduled) return;
    scheduled = true;
    setTimeout(() => {
      scheduled = false;
      if (cursorEl && !cursorEl.isConnected) clearCursor();
      updateStatus();
    }, 250);
  };

  const start = () => {
    buildStatusline();
    if (bte.isTop) {
      new MutationObserver(scheduleUpdate).observe(document.body, { childList: true, subtree: true });
      // Las navegaciones dentro del iframe no mutan este DOM; su "load" sí
      // llega a document en fase de captura.
      document.addEventListener("load", (e) => {
        if (e.target instanceof HTMLIFrameElement) scheduleUpdate();
      }, true);
      setInterval(tickClock, 15000);
      window.addEventListener("resize", fitActivityFrame);
      tickSession();
      setInterval(tickSession, 15000);
      const hashView = bte.consumeHashTarget();
      bte.resumePending();
      if (hashView) bte.showApp(hashView);
      relayToken();
      setInterval(relayToken, 5000);
      // Precarga la vista tui (oculta) cuando Eminus termina de cargar: al
      // abrirla ya tiene los datos y aparece al instante.
      setTimeout(() => {
        const preload = () => {
          if (!enabled() || appHost || !runtimeAlive() || document.querySelector("m-auth")) return;
          if (!localStorage.getItem("accessToken")) return;
          ensureAppHost();
        };
        if (window.requestIdleCallback) requestIdleCallback(preload, { timeout: 4000 });
        else preload();
      }, 1500);
    }
    document.addEventListener("bte:prefs", () => {
      if (!enabled()) {
        clearCursor();
        if (bte.isTop) bte.hideApp();
      }
      updateStatus();
      // El alto del encabezado cambia con el estilo.
      setTimeout(placeAppHost, 50);
    });
  };

  if (document.body) start();
  else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
