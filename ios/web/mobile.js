// App de iOS: capa táctil sobre BetterThanEminus.
//
// - Dock inferior (inicio, pendientes, buscar, ajustes, más) en lugar de los
//   atajos de teclado y la barra de estado.
// - Paleta, ayuda y ajustes como hojas que se cierran tocando fuera, con ✕
//   o deslizando hacia abajo.
// - Avisos como globo sobre el dock y la sesión por vencer como píldora.
// - Recordatorios locales antes de cada entrega (opcional).
(() => {
  const bte = globalThis.bte;
  const native = globalThis.__bteNative;
  if (!bte || !native || bte.mobileStarted) return;
  bte.mobileStarted = true;
  bte.native = native;

  const root = document.documentElement;
  root.classList.add("bte-app");

  const esc = (text) =>
    String(text == null ? "" : text).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  // ───────────── documentos propios (vista tui y ajustes) ─────────────

  // app.html y popup.html con sus hojas y scripts en línea, como srcdoc:
  // quedan en el origen de Eminus y la API responde sin CORS.
  const safeJs = (js) => String(js || "").replace(/<\/script/gi, "<\\/script");
  native.doc = (path) => {
    const files = native.files || {};
    const dir = path.replace(/[^/]*$/, "");
    const resolve = (href) => {
      const out = [];
      for (const part of (dir + href).split("/")) {
        if (part === "..") out.pop();
        else if (part && part !== ".") out.push(part);
      }
      return out.join("/");
    };
    const kind = path.startsWith("app/") ? "app" : "popup";
    let html = files[path] || "";
    html = html.replace(/<link rel="icon"[^>]*>/, "");
    html = html.replace(/<link rel="stylesheet" href="([^"]+)"\s*\/?>/g, (tag, href) =>
      /^https?:/.test(href) ? tag : "<style>" + (files[resolve(href)] || "") + "</style>");
    html = html.replace(/<script src="([^"]+)"><\/script>/g, (_, src) => {
      const file = resolve(src);
      // Tras prefs.js, los ajustes propios de la app (sólo estilo tui).
      const extra = file === "src/prefs.js" ? "\n" + (files["web/app-prefs.js"] || "") : "";
      return "<script>" + safeJs(files[file] + extra) + "</script>";
    });
    html = html.replace(/<meta name="viewport"[^>]*>/, "");
    const head =
      '<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, viewport-fit=cover" />' +
      "<script>window.__bteNative = " + JSON.stringify({ version: native.version, dev: native.dev }) + ";" +
      "document.documentElement.dataset.bteDoc = " + JSON.stringify(kind) + ";" +
      "document.documentElement.classList.add('bte-app');" +
      // Los ajustes se cierran con window.close() (popup.js).
      "window.close = function () { parent.postMessage({ bte: true, type: 'closeDoc' }, '*'); };</script>" +
      "<script>" + safeJs(files["web/shim.js"]) + "</script>";
    html = html.replace("<head>", "<head>" + head);
    const extraCss = files[kind === "app" ? "web/app-mobile.css" : "web/popup-mobile.css"] || "";
    html = html.replace("</head>", "<style>" + extraCss + "</style></head>");
    if (kind === "app" && files["web/app-mobile.js"]) {
      html = html.replace("</body>", "<script>" + safeJs(files["web/app-mobile.js"]) + "</script></body>");
    }
    return html;
  };

  if (!bte.isTop) return;

  const haptic = (style) => native.post({ type: "haptic", style: style || "selection" });
  bte.haptic = haptic;

  // ───────────── pantalla ─────────────

  // De borde a borde (el CSS usa env(safe-area-inset-*)) y sin el zoom
  // automático de iOS al enfocar un campo; pellizcar sigue funcionando.
  const fixViewport = () => {
    let meta = document.querySelector('meta[name="viewport"]');
    if (!meta) {
      meta = document.createElement("meta");
      meta.name = "viewport";
      (document.head || root).appendChild(meta);
    }
    const content = "width=device-width, initial-scale=1, maximum-scale=1, viewport-fit=cover";
    if (meta.content !== content) meta.content = content;
  };
  fixViewport();

  // Alto visible (se encoge con el teclado): las hojas se ajustan a él.
  const vv = window.visualViewport;
  const onViewport = () => {
    if (!vv) return;
    root.style.setProperty("--m-vvh", Math.round(vv.height) + "px");
    root.classList.toggle("m-keyboard", window.innerHeight - vv.height > 120);
    for (const el of document.querySelectorAll("#bte-overlay, #bte-settings")) el.style.top = Math.round(vv.offsetTop) + "px";
  };
  if (vv) {
    vv.addEventListener("resize", onViewport);
    vv.addEventListener("scroll", onViewport);
    onViewport();
  }

  // La barra de estado de iOS toma el color del encabezado del tema.
  let lastTheme = "";
  const probe = document.createElement("canvas").getContext("2d");
  const toHex = (c) => {
    probe.fillStyle = "#000";
    probe.fillStyle = c || "#000";
    return probe.fillStyle;
  };
  const reportTheme = () => {
    const css = getComputedStyle(root);
    const enabled = root.classList.contains("bte");
    const header = document.querySelector("m-header header");
    let bg = css.getPropertyValue("--bte-bg0").trim() || css.getPropertyValue("--bte-bg").trim();
    if (header) {
      const own = getComputedStyle(header).backgroundColor;
      if (own && !/rgba\(.*,\s*0\)$/.test(own) && own !== "transparent") bg = own;
    }
    const scheme = css.getPropertyValue("--bte-scheme").trim();
    const msg = {
      type: "theme",
      bg: enabled ? toHex(bg) : "#18794e",
      fg: enabled ? toHex(css.getPropertyValue("--bte-fg").trim()) : "#ffffff",
      dark: enabled ? scheme !== "light" : true
    };
    const key = JSON.stringify(msg);
    if (key === lastTheme) return;
    lastTheme = key;
    native.post(msg);
  };

  // ───────────── avisos ─────────────

  let toastEl = null;
  let toastTimer = null;
  const origToast = bte.toast;
  bte.toast = (text, ms = 2600) => {
    if (origToast) origToast(text, ms);
    if (!text) return;
    if (!toastEl) {
      toastEl = document.createElement("div");
      toastEl.id = "bte-toast-m";
      toastEl.setAttribute("role", "status");
      toastEl.hidden = true;
      document.body.appendChild(toastEl);
    }
    toastEl.textContent = text;
    toastEl.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toastEl.hidden = true), Math.max(1800, ms));
  };

  // La sesión por vencer (app.js la calcula en la barra oculta).
  let sessionEl = null;
  const syncSession = () => {
    const src = document.querySelector("#bte-statusline .bte-session");
    if (!sessionEl) {
      sessionEl = document.createElement("button");
      sessionEl.type = "button";
      sessionEl.id = "bte-session-m";
      sessionEl.hidden = true;
      sessionEl.addEventListener("click", () => bte.toast(sessionEl.title || "", 4000));
      document.body.appendChild(sessionEl);
    }
    const show = !!src && !src.hidden && !document.querySelector("m-auth");
    sessionEl.hidden = !show;
    if (show) {
      sessionEl.textContent = src.textContent;
      sessionEl.dataset.level = src.dataset.level || "";
      sessionEl.title = src.title;
    }
  };

  // ───────────── dock ─────────────

  const ICONS = {
    home: '<path d="M3 11.5 12 4l9 7.5"/><path d="M5.5 10v9.5h13V10"/><path d="M10 19.5v-5h4v5"/>',
    agenda: '<rect x="3.5" y="5" width="17" height="15.5" rx="2"/><path d="M3.5 10h17M8 3v4M16 3v4"/><path d="m8.5 15 2.2 2.2 4.8-4.7"/>',
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',
    settings: '<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2.2"/><circle cx="10" cy="17" r="2.2"/>',
    more: '<path d="M5 7.5h14M5 12h14M5 16.5h9"/>'
  };

  const dock = document.createElement("nav");
  dock.id = "bte-dock";
  dock.setAttribute("aria-label", "Navegación");
  dock.innerHTML = [
    ["home", "inicio"],
    ["agenda", "pendientes"],
    ["search", "buscar"],
    ["settings", "ajustes"],
    ["more", "más"]
  ].map(([id, label]) =>
    `<button type="button" data-dock="${id}"><svg viewBox="0 0 24 24" aria-hidden="true">${ICONS[id]}</svg><span>${label}</span>` +
    (id === "agenda" ? '<b class="m-badge" hidden></b>' : "") + "</button>").join("");
  document.body.appendChild(dock);

  const closeAll = () => {
    if (bte.overlayOpen && bte.overlayOpen()) bte.closeOverlay();
    closeSettings();
  };

  dock.addEventListener("click", (e) => {
    const btn = e.target.closest("button[data-dock]");
    if (!btn) return;
    haptic();
    const action = btn.dataset.dock;
    if (!root.classList.contains("bte") && action !== "settings") {
      bte.toast("el reskin está apagado: actívalo en ajustes");
      return;
    }
    if (action === "home") {
      closeAll();
      const go = () => bte.goSection("courses");
      if (bte.appOpen && bte.appOpen()) bte.hideApp("keep").then(go);
      else if (bte.currentSection() && bte.currentSection().id === "courses") window.scrollTo({ top: 0, behavior: "smooth" });
      else go();
    } else if (action === "agenda") {
      closeAll();
      if (bte.appOpen && bte.appOpen()) bte.hideApp();
      else bte.showApp("agenda");
    } else if (action === "search") {
      const open = bte.overlayOpen && bte.overlayOpen();
      closeAll();
      if (!open) bte.openActivityPalette();
    } else if (action === "settings") {
      const open = settingsOpen();
      closeAll();
      if (!open) openSettings();
    } else if (action === "more") {
      const open = bte.overlayOpen && bte.overlayOpen();
      closeAll();
      if (!open) bte.openCommandPalette();
    }
    setTimeout(updateDock, 0);
  });

  const updateDock = () => {
    const appOpen = !!(bte.appOpen && bte.appOpen());
    const onHome = !appOpen && location.pathname.includes("/page/course/list");
    for (const btn of dock.querySelectorAll("button")) {
      const id = btn.dataset.dock;
      const on = id === "agenda" ? appOpen : id === "home" ? onHome : id === "settings" ? settingsOpen() : false;
      if (btn.getAttribute("aria-current") !== String(on)) btn.setAttribute("aria-current", String(on));
    }
    // Sin cosas que jalar mientras hay una hoja o la vista tui encima.
    const refreshable = !appOpen && !settingsOpen() && !(bte.overlayOpen && bte.overlayOpen()) && !root.classList.contains("bte-bg-editing");
    if (refreshable !== lastRefreshable) {
      lastRefreshable = refreshable;
      native.post({ type: "refreshable", on: refreshable });
    }
  };
  let lastRefreshable = null;

  // ───────────── pendientes (insignia y recordatorios) ─────────────

  // La vista tui guarda cursos y actividades en storage.session; de ahí se
  // cuentan las vencidas y las de hoy sin volver a consultar la API.
  const readCache = () => {
    try {
      return JSON.parse(sessionStorage.getItem("bte:store:session:bteAppCache") || "null");
    } catch (_) {
      return null;
    }
  };

  const pendingActs = () => {
    const cache = readCache();
    if (!cache || !Array.isArray(cache.courses)) return [];
    const current = new Set(cache.courses.filter((c) => c.current).map((c) => c.id));
    const out = [];
    for (const [courseId, acts] of cache.acts || []) {
      if (!current.has(courseId)) continue;
      for (const a of acts || []) if (!a.delivered && a.deadline) out.push(a);
    }
    return out;
  };

  const updateBadge = () => {
    const now = Date.now() / 1000;
    const end = new Date();
    end.setHours(24, 0, 0, 0);
    let n = 0;
    for (const a of pendingActs()) {
      if ((a.deadline < now && now - a.deadline < 21 * 86400) || (a.deadline >= now && a.deadline < end.getTime() / 1000)) n += 1;
    }
    const badge = dock.querySelector(".m-badge");
    badge.hidden = !n;
    badge.textContent = n > 9 ? "9+" : String(n);
    badge.title = n + " vencidas o para hoy";
  };

  const NOTIFY_KEY = "bte:m:notify";
  const notifyOn = () => localStorage.getItem(NOTIFY_KEY) === "on";
  let lastPlan = "";

  // Un aviso un día antes y otro dos horas antes de cada entrega.
  const scheduleReminders = async (ask) => {
    if (!notifyOn() && !ask) return;
    const now = Date.now() / 1000;
    const items = [];
    for (const a of pendingActs()) {
      if (a.deadline <= now || (a.start && a.start > now + 30 * 86400)) continue;
      const course = String(a.courseName || "").replace(/\s*\(\d+\)\s*$/, "").toLowerCase();
      const target = JSON.stringify({ course: a.courseName, section: "activities", activity: { id: a.id, courseId: a.courseId } });
      const when = new Date(a.deadline * 1000).toLocaleString("es-MX", { weekday: "short", hour: "2-digit", minute: "2-digit" });
      for (const [before, label] of [[86400, "vence mañana"], [7200, "vence en 2 horas"]]) {
        const at = a.deadline - before;
        if (at > now + 60) items.push({ id: "bte-" + a.courseId + "-" + a.id + "-" + before, at, title: a.title, body: label + " (" + when + ") · " + course, target });
      }
    }
    items.sort((x, y) => x.at - y.at);
    const plan = JSON.stringify(items);
    if (plan === lastPlan && !ask) return;
    lastPlan = plan;
    const status = await native.post({ type: "notify", items, ask: !!ask });
    if (ask) {
      if (String(status).startsWith("on")) {
        localStorage.setItem(NOTIFY_KEY, "on");
        const count = Number(String(status).split(":")[1] || 0);
        bte.toast(count ? "avisos activados · " + count + " programados" : "avisos activados · se programan al cargar tus pendientes", 4000);
        if (!count && !(bte.appOpen && bte.appOpen())) bte.showApp("agenda");
      } else {
        localStorage.removeItem(NOTIFY_KEY);
        bte.toast(status === "denied" ? "avisos bloqueados: actívalos en Ajustes de iOS › Eminus" : "no se activaron los avisos", 5000);
      }
    }
  };

  const disableReminders = async () => {
    localStorage.removeItem(NOTIFY_KEY);
    lastPlan = "";
    await native.post({ type: "notify", items: [] });
    bte.toast("avisos de entrega desactivados");
  };

  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "session" && changes.bteAppCache) {
      updateBadge();
      scheduleReminders(false);
    }
  });

  // Con cuenta gratuita de Apple la app dura 7 días firmada; la Mac la
  // renueva sola (ios/dev/renew.sh). Aquí se avisa si ya falta poco.
  let expiry = 0;
  const expiryLabel = () => {
    if (!expiry) return "";
    const hours = (expiry - Date.now() / 1000) / 3600;
    if (hours <= 0) return "vencida";
    return hours < 48 ? "vence en " + Math.max(1, Math.round(hours)) + " h" : "vence en " + Math.round(hours / 24) + " días";
  };
  native.post({ type: "appExpiry" }).then((value) => {
    expiry = Number(value) || 0;
    const hours = (expiry - Date.now() / 1000) / 3600;
    if (expiry && hours < 36) {
      setTimeout(() => bte.toast("la app " + expiryLabel() + ": desbloquea el iPhone cerca de tu Mac para renovarla", 7000), 2500);
    }
  });

  // Comandos propios de la app en la paleta.
  const origCommands = bte.openCommandPalette;
  const origOpenPalette = bte.openPalette;
  bte.openPalette = (opts) => {
    if (opts && opts.sigil === ":" && Array.isArray(opts.items)) {
      const items = opts.items.filter((item) => !/atajos de teclado|scanlines|estilo: elegir/.test(item.label));
      const at = items.findIndex((item) => item.group === "herramientas");
      const extra = [
        notifyOn()
          ? { group: "herramientas", label: "avisos de entrega: desactivar", keywords: "notificaciones recordatorios", run: disableReminders }
          : { group: "herramientas", label: "avisos de entrega: activar", meta: "1 día y 2 h antes", keywords: "notificaciones recordatorios alarma", run: () => scheduleReminders(true) },
        { group: "ayuda", label: "cómo se usa en el teléfono", keywords: "help ayuda gestos", run: () => bte.showHelp() }
      ];
      if (expiry) {
        extra.push({
          group: "ayuda",
          label: "firma de la app",
          meta: expiryLabel(),
          keywords: "vence renovar 7 dias instalar expira",
          run: () => bte.toast("la app " + expiryLabel() + " · se renueva sola cuando el iPhone está desbloqueado cerca de tu Mac", 6000)
        });
      }
      items.splice(at === -1 ? items.length : at, 0, extra[0]);
      const tools = items.findIndex((item) => item.group === "vistas");
      items.splice(tools === -1 ? 0 : tools, 0, ...profileItems());
      items.push(...extra.slice(1));
      opts = { ...opts, items };
    }
    origOpenPalette(opts);
    decorateSheet();
    // Sólo las búsquedas abren el teclado; los menús se leen primero.
    if (!opts || !["cd", "find"].includes(opts.sigil)) {
      const input = document.querySelector("#bte-overlay .bte-prompt input");
      if (input) input.blur();
    }
    updateDock();
  };
  bte.openCommandPalette = () => origCommands();

  // ───────────── hojas ─────────────

  // Botón ✕, deslizar hacia abajo para cerrar y marca al tocar una fila.
  const decorateSheet = () => {
    const overlayEl = document.getElementById("bte-overlay");
    const win = overlayEl && overlayEl.querySelector(".bte-window");
    if (!win || win.dataset.mDecorated) return;
    win.dataset.mDecorated = "1";
    onViewport();
    const bar = win.querySelector(".bte-titlebar");
    if (bar && !bar.querySelector(".m-close")) {
      const dim = bar.querySelector(".bte-title-dim");
      if (dim) dim.remove();
      const close = document.createElement("button");
      close.type = "button";
      close.className = "m-close";
      close.setAttribute("aria-label", "Cerrar");
      close.textContent = "✕";
      close.addEventListener("click", () => bte.closeOverlay());
      bar.appendChild(close);
    }
    swipeToClose(win, bar, () => bte.closeOverlay());
    const list = win.querySelector(".bte-list");
    if (list) {
      let pressed = null;
      const release = () => {
        if (pressed) pressed.classList.remove("m-press");
        pressed = null;
      };
      list.addEventListener("touchstart", (e) => {
        const li = e.target.closest(".bte-item");
        if (!li || li.classList.contains("bte-item-has-range")) return;
        pressed = li;
        li.classList.add("m-press");
      }, { passive: true });
      list.addEventListener("touchmove", release, { passive: true });
      list.addEventListener("touchcancel", release, { passive: true });
      list.addEventListener("touchend", () => setTimeout(release, 180), { passive: true });
    }
  };

  const swipeToClose = (sheet, handle, close) => {
    if (!handle) return;
    let start = null;
    handle.addEventListener("touchstart", (e) => {
      if (e.target.closest("button, input")) return;
      start = { y: e.touches[0].clientY, t: Date.now() };
      sheet.style.transition = "none";
    }, { passive: true });
    handle.addEventListener("touchmove", (e) => {
      if (!start) return;
      const dy = Math.max(0, e.touches[0].clientY - start.y);
      sheet.style.transform = "translateY(" + dy + "px)";
    }, { passive: true });
    const end = (e) => {
      if (!start) return;
      const dy = (e.changedTouches[0] || {}).clientY - start.y;
      const fast = dy > 40 && Date.now() - start.t < 250;
      start = null;
      sheet.style.transition = "transform 0.2s ease-out";
      if (dy > 110 || fast) {
        sheet.style.transform = "translateY(100%)";
        setTimeout(close, 160);
      } else sheet.style.transform = "";
    };
    handle.addEventListener("touchend", end, { passive: true });
    handle.addEventListener("touchcancel", end, { passive: true });
  };

  // Tocar fuera de la hoja la cierra (palette.js escucha mousedown, que iOS
  // no siempre manda sobre un fondo sin enlaces).
  document.addEventListener("click", (e) => {
    const overlayEl = document.getElementById("bte-overlay");
    if (overlayEl && e.target === overlayEl) bte.closeOverlay();
  }, true);

  // Al cerrar una hoja, el dock y el jalar para recargar vuelven.
  const origClose = bte.closeOverlay;
  bte.closeOverlay = () => {
    origClose();
    updateDock();
  };

  // Ayuda para el teléfono: qué hace cada botón, en vez de los atajos.
  bte.showHelp = () => {
    closeAll();
    let overlayEl = document.getElementById("bte-overlay");
    if (!overlayEl) {
      bte.openPalette({ title: "ayuda", items: [] });
      overlayEl = document.getElementById("bte-overlay");
    }
    overlayEl.innerHTML = `<div class="bte-window" role="dialog" aria-label="ayuda">
      <div class="bte-titlebar"><span>cómo se usa</span></div>
      <div class="bte-help">
        <h4>abajo</h4>
        <dl>
          <dt>inicio</dt><dd>tus cursos; si ya estás ahí, sube al principio</dd>
          <dt>pendientes</dt><dd>agenda, cursos, miyu y calificaciones sin salir de eminus. el número rojo son las vencidas y las de hoy</dd>
          <dt>buscar</dt><dd>cualquier actividad de tus cursos actuales</dd>
          <dt>ajustes</dt><dd>estilo, tema, fondo con foto y zoom</dd>
          <dt>más</dt><dd>todos los comandos: recientes, secciones, modo lectura, borradores, avisos</dd>
        </dl>
        <h4>gestos</h4>
        <dl>
          <dt>desliza →</dt><dd>desde el borde izquierdo: página anterior</dd>
          <dt>jala ↓</dt><dd>arriba de la página: recargar</dd>
          <dt>hojas</dt><dd>se cierran con ✕, tocando fuera o deslizando hacia abajo</dd>
          <dt>secciones</dt><dd>dentro de un curso, la tira bajo el encabezado se desliza de lado</dd>
        </dl>
        <h4>también</h4>
        <p>lo que escribes en foros y entregas se guarda como borrador mientras escribes. si la sesión vence (dura 2 horas), vuelve a entrar y aparece «✎ borrador · recuperar».</p>
        <p>los archivos de eminus se abren en vista previa, desde donde puedes guardarlos o compartirlos.</p>
      </div>
    </div>`;
    overlayEl.hidden = false;
    bte.setMode && bte.setMode("cmd");
    decorateSheet();
    updateDock();
  };

  // Ajustes: el popup de la extensión, como hoja con su propio documento.
  let settings = null;
  const settingsOpen = () => !!settings && !settings.hidden;
  const openSettings = () => {
    if (!settings) {
      settings = document.createElement("div");
      settings.id = "bte-settings";
      settings.hidden = true;
      settings.innerHTML = `<div class="m-sheet" role="dialog" aria-label="ajustes">
        <div class="m-sheet-head"><span>ajustes</span><button type="button" class="m-close" aria-label="Cerrar">✕</button></div>
        <iframe title="ajustes"></iframe>
      </div>`;
      settings.addEventListener("click", (e) => {
        if (e.target === settings || e.target.closest(".m-close")) closeSettings();
      });
      document.body.appendChild(settings);
      const sheet = settings.querySelector(".m-sheet");
      swipeToClose(sheet, settings.querySelector(".m-sheet-head"), closeSettings);
    }
    const frame = settings.querySelector("iframe");
    // Se arma de nuevo cada vez: así refleja lo que cambió desde la paleta.
    frame.srcdoc = native.doc("popup/popup.html");
    const sheet = settings.querySelector(".m-sheet");
    sheet.style.transform = "";
    sheet.style.transition = "";
    settings.hidden = false;
    onViewport();
    updateDock();
  };
  const closeSettings = () => {
    if (!settingsOpen()) return;
    settings.hidden = true;
    settings.querySelector("iframe").srcdoc = "";
    updateDock();
  };

  window.addEventListener("message", (e) => {
    const frame = settings && settings.querySelector("iframe");
    if (!frame || e.source !== frame.contentWindow || !e.data || e.data.bte !== true) return;
    if (e.data.type === "closeDoc") closeSettings();
  });

  // ───────────── tira de secciones ─────────────

  // La sección activa siempre a la vista, y sin desvanecido al llegar al final.
  let stripFor = "";
  const syncStrip = () => {
    const menu = document.getElementById("m_ver_menu");
    if (!menu) return;
    if (!menu.dataset.mStrip) {
      menu.dataset.mStrip = "1";
      menu.addEventListener("scroll", () => {
        menu.classList.toggle("m-at-end", menu.scrollLeft + menu.clientWidth >= menu.scrollWidth - 12);
      }, { passive: true });
    }
    const active = menu.querySelector(".m-menu__item--active");
    const key = location.pathname + "|" + (active ? active.textContent.trim() : "");
    if (!active || key === stripFor) return;
    stripFor = key;
    const r = active.getBoundingClientRect();
    const target = menu.scrollLeft + r.left - (menu.clientWidth - r.width) / 2;
    menu.scrollTo({ left: Math.max(0, target), behavior: "smooth" });
  };

  // ───────────── modo lectura y fondo ─────────────

  // Sin esc en el teléfono: el modo lectura se deja con un botón flotante.
  const readingExit = document.createElement("button");
  readingExit.type = "button";
  readingExit.id = "bte-reading-exit";
  readingExit.textContent = "salir de lectura";
  readingExit.hidden = true;
  readingExit.addEventListener("click", () => {
    haptic();
    root.classList.remove("bte-reading");
    bte.setMode && bte.setMode("normal");
  });
  document.body.appendChild(readingExit);

  const syncOverlays = () => {
    readingExit.hidden = !root.classList.contains("bte-reading");
    // El editor del fondo (bg.js) explica ratón y rueda: aquí, dedos.
    const hint = document.querySelector("#bte-bg-edit .bte-bg-edit-hint");
    const text = "arrastra para mover · pellizca para el zoom · gira con dos dedos";
    if (hint && hint.textContent !== text) hint.textContent = text;
    updateDock();
  };
  new MutationObserver(syncOverlays).observe(root, { attributes: true, attributeFilter: ["class"] });

  // ───────────── iframe de actividades ─────────────

  // En escritorio app.js lo ajusta al hueco visible y se desplaza por dentro.
  // En el teléfono crece con su contenido: un solo scroll, y la pleca del
  // curso se va al bajar en vez de ocupar un tercio de la pantalla.
  const watched = new WeakSet();
  bte.sizeActivityFrame = (frame) => {
    let doc = null;
    try {
      doc = frame.contentDocument;
    } catch (_) {
    }
    if (!doc || !doc.body) return;
    const fit = () => {
      // Lo que ocupa el contenido, sin contar el alto actual del iframe
      // (scrollHeight nunca bajaría de él).
      const range = doc.createRange();
      range.selectNodeContents(doc.body);
      if (doc.defaultView.scrollY) doc.defaultView.scrollTo(0, 0);
      const content = range.getBoundingClientRect().bottom;
      const h = Math.max(320, Math.ceil(content) + 24) + "px";
      if (frame.style.getPropertyValue("height") !== h) frame.style.setProperty("height", h, "important");
    };
    // Red de seguridad: cualquier contenedor con unos píxeles de scroll
    // vertical propio atrapa el gesto; deja de ser desplazable.
    let tameTimer = null;
    const tame = () => {
      clearTimeout(tameTimer);
      tameTimer = setTimeout(() => {
        for (const el of doc.querySelectorAll("app-root *")) {
          const extra = el.scrollHeight - el.clientHeight;
          if (extra < 1 || extra > 48 || el.classList.contains("m-no-scroll")) continue;
          if (/(auto|scroll)/.test(doc.defaultView.getComputedStyle(el).overflowY)) el.classList.add("m-no-scroll");
        }
      }, 400);
    };
    if (!watched.has(doc)) {
      watched.add(doc);
      new MutationObserver(tame).observe(doc.body, { childList: true, subtree: true });
      tame();
      const observer = new ResizeObserver(fit);
      observer.observe(doc.body);
      for (const el of doc.querySelectorAll("app-root > *")) observer.observe(el);
      new MutationObserver(fit).observe(doc.body, { childList: true, subtree: true });
      // Los diálogos de Eminus se centran en el iframe, que ahora es tan
      // alto como su contenido: se desplaza la página hasta ellos.
      new MutationObserver(() => revealDialog(frame, doc)).observe(doc.body, { childList: true, subtree: true });
    }
    fit();
  };

  let revealed = null;
  const revealDialog = (frame, doc) => {
    const pane = doc.querySelector(".cdk-global-overlay-wrapper .cdk-overlay-pane, .swal2-popup, .modal.show .modal-dialog");
    if (!pane || pane === revealed) {
      if (!pane) revealed = null;
      return;
    }
    revealed = pane;
    requestAnimationFrame(() => {
      const r = pane.getBoundingClientRect();
      const top = frame.getBoundingClientRect().top + window.scrollY + r.top + r.height / 2 - window.innerHeight / 2;
      window.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
    });
  };

  // ───────────── perfiles ─────────────

  // Usuario y contraseña viven en el Llavero de iOS (ProfileStore.swift).
  // Aquí sólo se manejan nombres: para entrar se le pide a la app que llene
  // el formulario; esta página nunca lee una contraseña guardada.
  const SESSION_KEY = "bte:m:session"; // { user, exp } de la sesión en curso
  const SWITCH_KEY = "bte:m:switchTo"; // perfil con el que entrar tras salir
  const NOSAVE_KEY = "bte:m:nosave"; // usuarios que no quisieron guardarse
  const readJson = (key, fallback) => {
    try {
      const value = JSON.parse(localStorage.getItem(key) || "null");
      return value == null ? fallback : value;
    } catch (_) {
      return fallback;
    }
  };
  const onLogin = () => !!document.querySelector("m-auth");
  const tokenExpiry = () => {
    try {
      const token = localStorage.getItem("accessToken") || "";
      return JSON.parse(atob(token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/"))).exp || 0;
    } catch (_) {
      return 0;
    }
  };

  let profiles = [];
  let profilesEl = null;
  let loginState = ""; // "", "login" o "in": para actuar sólo al cambiar
  let lastAttempt = 0;
  let tryingUser = "";

  const loadProfiles = async () => {
    profiles = (await native.post({ type: "profile.list" })) || [];
    return profiles;
  };

  const loginWith = async (user, why) => {
    const profile = profiles.find((p) => p.user === user);
    if (!profile) return;
    lastAttempt = Date.now();
    tryingUser = user;
    native.post({ type: "profile.discard" }); // lo escrito a mano ya no cuenta
    bte.toast((why ? why + ": " : "") + "entrando como " + profile.name.toLowerCase() + "…", 4000);
    renderProfiles();
    const result = await native.post({ type: "profile.login", user });
    if (result !== "ok") {
      tryingUser = "";
      bte.toast("no se pudo llenar el formulario (" + result + ")");
      renderProfiles();
      return;
    }
    // Si a los 10 s sigue el login, la contraseña ya no es válida.
    setTimeout(() => {
      if (tryingUser !== user) return;
      tryingUser = "";
      if (onLogin()) bte.toast("no se pudo entrar como " + profile.name.toLowerCase() + ": revisa la contraseña e inicia sesión a mano", 6000);
      renderProfiles();
    }, 10000);
  };

  // Fotos de perfil: una miniatura del avatar de Eminus por usuario, para
  // reconocer cada perfil en el selector (no es un dato secreto).
  const AVATAR_KEY = "bte:m:avatars";
  const avatarOf = (user) => readJson(AVATAR_KEY, {})[user] || "";
  const captureAvatar = async (user) => {
    if (!user) return;
    const img = await bte.waitFor(() => {
      const el = document.querySelector("m-header m-user-profile img");
      return el && el.complete && el.naturalWidth ? el : null;
    }, 8000, 400);
    if (!img) return;
    let data = "";
    try {
      const size = 112;
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = size;
      const side = Math.min(img.naturalWidth, img.naturalHeight);
      canvas.getContext("2d").drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, size, size);
      data = canvas.toDataURL("image/jpeg", 0.82);
    } catch (_) {
      data = /^https:/.test(img.src) ? img.src : ""; // imagen de otro origen: se guarda su dirección
    }
    if (!data) return;
    const all = readJson(AVATAR_KEY, {});
    if (all[user] === data) return;
    all[user] = data;
    try {
      localStorage.setItem(AVATAR_KEY, JSON.stringify(all));
    } catch (_) {
    }
  };
  const forgetAvatar = (user) => {
    const all = readJson(AVATAR_KEY, {});
    delete all[user];
    localStorage.setItem(AVATAR_KEY, JSON.stringify(all));
  };

  // Con perfiles guardados, la entrada es un selector (foto, nombre y
  // usuario) en lugar del formulario; «iniciar con otro perfil» lo muestra.
  let pickMode = true;
  const renderProfiles = () => {
    const available = onLogin() && profiles.length > 0;
    const show = available && pickMode;
    root.classList.toggle("m-pick", show);
    root.classList.toggle("m-has-profiles", available && !pickMode);
    if (!profilesEl) {
      profilesEl = document.createElement("div");
      profilesEl.id = "bte-profiles";
      profilesEl.hidden = true;
      profilesEl.addEventListener("click", (e) => {
        if (tryingUser) return;
        const btn = e.target.closest("button");
        if (!btn) return;
        haptic();
        if (btn.dataset.user) loginWith(btn.dataset.user);
        else if (btn.dataset.a === "new") {
          pickMode = false;
          renderProfiles();
          const field = document.querySelector('m-auth input[name="username"]');
          if (field) field.focus();
        }
      });
      document.body.appendChild(profilesEl);
      backEl = document.createElement("button");
      backEl.type = "button";
      backEl.id = "bte-profiles-back";
      backEl.textContent = "‹ perfiles guardados";
      backEl.hidden = true;
      backEl.addEventListener("click", () => {
        haptic();
        pickMode = true;
        if (document.activeElement) document.activeElement.blur();
        renderProfiles();
      });
      document.body.appendChild(backEl);
    }
    backEl.hidden = !(available && !pickMode);
    if (!show) {
      profilesEl.hidden = true;
      return;
    }
    const html =
      '<div class="m-profiles-brand">better than eminus</div>' +
      '<div class="m-profiles-title">' + (profiles.length === 1 ? "tu perfil" : "elige un perfil") + "</div>" +
      profiles.map((p) => {
        const photo = avatarOf(p.user);
        const busy = tryingUser === p.user;
        const initial = esc((p.name || p.user).trim().charAt(0).toUpperCase());
        return `<button type="button" class="m-profile" data-user="${esc(p.user)}"${busy ? ' aria-busy="true"' : ""}>` +
          `<span class="m-avatar">${photo ? `<img src="${esc(photo)}" alt="" />` : initial}</span>` +
          `<span class="m-who"><b>${esc(p.name.toLowerCase())}</b><i>${busy ? "entrando…" : esc(p.user)}</i></span>` +
          '<span class="m-go">❯</span></button>';
      }).join("") +
      '<button type="button" class="m-profile-new" data-a="new">+ iniciar con otro perfil</button>';
    if (profilesEl.innerHTML !== html) profilesEl.innerHTML = html;
    profilesEl.hidden = false;
  };
  let backEl = null;

  // Lo que se escribe al entrar a mano se queda en la memoria de la app
  // (no aquí ni en disco) hasta saber si entró y si se quiere guardar. Se
  // toma de los campos al escribir, al pulsar el botón y con enter: Eminus
  // no siempre dispara el evento submit del formulario.
  let stashTimer = null;
  const stashFrom = (auth) => {
    clearTimeout(stashTimer);
    if (!auth || tryingUser) return;
    const user = auth.querySelector('input[name="username"], input[type="text"]');
    const pass = auth.querySelector('input[name="password"], input[type="password"]');
    if (user && pass && user.value.trim() && pass.value) native.post({ type: "profile.stash", user: user.value.trim(), pass: pass.value });
  };
  const authOf = (e) => (e.target instanceof Element ? e.target.closest("m-auth") : null);
  for (const type of ["input", "change"]) {
    document.addEventListener(type, (e) => {
      const auth = authOf(e);
      if (!auth) return;
      clearTimeout(stashTimer);
      stashTimer = setTimeout(() => stashFrom(auth), 250);
    }, true);
  }
  document.addEventListener("submit", (e) => stashFrom(authOf(e)), true);
  document.addEventListener("click", (e) => e.target instanceof Element && e.target.closest("button") && stashFrom(authOf(e)), true);
  document.addEventListener("keydown", (e) => e.key === "Enter" && stashFrom(authOf(e)), true);

  const displayName = () => {
    const el = document.querySelector("m-header m-user-profile .h5");
    return el ? el.textContent.trim().replace(/\s+/g, " ") : "";
  };

  const offerSave = async () => {
    const info = await native.post({ type: "profile.stashInfo" });
    if (!info) return;
    localStorage.setItem(SESSION_KEY, JSON.stringify({ user: info.user, exp: tokenExpiry() }));
    if (info.saved) {
      // Ya estaba: si la contraseña cambió, se actualiza sin preguntar.
      if (info.changed) await native.post({ type: "profile.commit", name: "" });
      else native.post({ type: "profile.discard" });
      return loadProfiles();
    }
    if (readJson(NOSAVE_KEY, []).includes(info.user)) return native.post({ type: "profile.discard" });
    // El nombre tarda un momento en aparecer en el encabezado.
    const name = (await bte.waitFor(displayName, 6000, 300)) || info.user;
    const card = document.createElement("div");
    card.id = "bte-profile-save";
    card.innerHTML = `<div><b>¿guardar este perfil?</b><span>${esc(name.toLowerCase())} · entrarás sin escribir la contraseña. se guarda en el llavero del iphone.</span></div>
      <div class="m-profile-save-btns"><button type="button" data-a="no">ahora no</button><button type="button" data-a="yes">guardar</button></div>`;
    card.addEventListener("click", async (e) => {
      const btn = e.target.closest("button[data-a]");
      if (!btn) return;
      haptic();
      card.remove();
      if (btn.dataset.a === "yes") {
        const ok = await native.post({ type: "profile.commit", name });
        await loadProfiles();
        if (ok) captureAvatar(info.user);
        bte.toast(ok ? "perfil guardado · más → perfiles para cambiar o quitar" : "no se pudo guardar el perfil", 4500);
      } else {
        native.post({ type: "profile.discard" });
        localStorage.setItem(NOSAVE_KEY, JSON.stringify([...new Set([...readJson(NOSAVE_KEY, []), info.user])]));
      }
    });
    document.body.appendChild(card);
  };

  // Salir sin pasar por el menú de Eminus (lo «bte:» se conserva, ver shim).
  const logoutLocal = (switchTo) => {
    if (switchTo) localStorage.setItem(SWITCH_KEY, switchTo);
    else localStorage.removeItem(SWITCH_KEY);
    localStorage.removeItem(SESSION_KEY);
    localStorage.clear();
    sessionStorage.clear();
    location.assign(location.origin + "/eminus4/");
  };

  const syncProfiles = async () => {
    const state = onLogin() ? "login" : localStorage.getItem("accessToken") ? "in" : "";
    if (state === loginState) return;
    const previous = loginState;
    loginState = state;
    const stale = document.getElementById("bte-profile-save");
    if (stale && state !== "in") stale.remove();
    if (state === "login") {
      pickMode = true;
      await loadProfiles();
      renderProfiles();
      const session = readJson(SESSION_KEY, null);
      const switchTo = localStorage.getItem(SWITCH_KEY);
      localStorage.removeItem(SWITCH_KEY);
      const recent = Date.now() - lastAttempt < 60000; // sin bucles si la contraseña falla
      const expired = !!session && !!session.exp && session.exp * 1000 < Date.now();
      // Si no se sabe de quién era la sesión y sólo hay un perfil, es ése.
      const owner = session ? session.user || (profiles.length === 1 ? profiles[0].user : "") : "";
      if (switchTo && profiles.some((p) => p.user === switchTo)) {
        loginWith(switchTo);
      } else if (expired && !recent && profiles.some((p) => p.user === owner)) {
        // La sesión venció sola (no se cerró a mano): se vuelve a entrar.
        loginWith(owner, "sesión vencida");
      } else if (session && !expired) {
        localStorage.removeItem(SESSION_KEY); // cerró sesión a mano: elige perfil
      }
    } else if (state === "in") {
      renderProfiles();
      if (tryingUser) {
        localStorage.setItem(SESSION_KEY, JSON.stringify({ user: tryingUser, exp: tokenExpiry() }));
        tryingUser = "";
      } else {
        await offerSave();
      }
      // Se anota cuándo vence para distinguir después «venció» de «cerró».
      const session = readJson(SESSION_KEY, null) || { user: "" };
      localStorage.setItem(SESSION_KEY, JSON.stringify({ user: session.user || "", exp: tokenExpiry() }));
      if (!profiles.length) await loadProfiles();
      // La foto del perfil en uso se mantiene al día.
      const who = readJson(SESSION_KEY, {}).user;
      if (who && profiles.some((p) => p.user === who)) captureAvatar(who);
    } else {
      renderProfiles();
    }
  };

  const profileItems = () => {
    const session = readJson(SESSION_KEY, null);
    const current = session ? session.user : "";
    const items = profiles.filter((p) => p.user !== current).map((p) => ({
      group: "perfiles",
      label: "cambiar a " + p.name.toLowerCase(),
      meta: p.user,
      keywords: "perfil cuenta usuario sesion cambiar entrar",
      run: () => logoutLocal(p.user)
    }));
    if (!onLogin()) {
      items.push({ group: "perfiles", label: "cerrar sesión", meta: profiles.length ? "elegir otro perfil" : "", keywords: "salir logout perfil cuenta", run: () => logoutLocal() });
    }
    for (const p of profiles) {
      items.push({
        group: "perfiles",
        label: "quitar perfil: " + p.name.toLowerCase(),
        meta: p.user === current ? "● en uso" : p.user,
        keywords: "perfil borrar eliminar olvidar contraseña",
        run: async () => {
          if (!confirm("¿Quitar el perfil «" + p.name + "» de este iPhone? Se borra su contraseña guardada.")) return;
          await native.post({ type: "profile.remove", user: p.user });
          forgetAvatar(p.user);
          await loadProfiles();
          renderProfiles();
          bte.toast("perfil quitado");
        }
      });
    }
    if (!profiles.length) {
      items.push({ group: "perfiles", label: "perfiles: cómo se guardan", keywords: "perfil cuenta contraseña", run: () => bte.toast("inicia sesión a mano y la app te ofrece guardar el perfil en el llavero del iphone", 6000) });
    }
    return items;
  };

  // ───────────── ciclo de vida ─────────────

  const tick = () => {
    syncStrip();
    updateDock();
    syncSession();
    reportTheme();
  };

  document.addEventListener("bte:prefs", () => setTimeout(tick, 40));
  window.addEventListener("popstate", () => setTimeout(tick, 40));
  new MutationObserver(() => {
    if (tickQueued) return;
    tickQueued = true;
    setTimeout(() => {
      tickQueued = false;
      tick();
      decorateSheet();
    }, 200);
  }).observe(document.body, { childList: true, subtree: false, attributes: true, attributeFilter: ["hidden"] });
  let tickQueued = false;

  // app.js muestra/oculta la vista tui cambiando `hidden` del host.
  const watchHost = () => {
    const host = document.getElementById("bte-app-host");
    if (!host || host.dataset.mWatched) return;
    host.dataset.mWatched = "1";
    new MutationObserver(tick).observe(host, { attributes: true, attributeFilter: ["hidden"] });
  };
  setInterval(() => {
    watchHost();
    syncProfiles();
    syncStrip();
    updateDock();
    syncSession();
    reportTheme();
  }, 1500);

  tick();
  syncProfiles();
  updateBadge();
  scheduleReminders(false);
  setInterval(updateBadge, 60000);
  bte.escapeHtml = esc;
})();
