// App de iOS: la vista tui con el dedo. app.js no cambia: los botones
// despachan las mismas teclas que usa en escritorio (r, a, /) y aquí sólo se
// lleva qué panel se ve (lista → actividades → detalle).
(() => {
  const $ = (id) => document.getElementById(id);
  const main = $("main");
  if (!main || main.dataset.mStack) return;

  const key = (k) => document.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
  const post = (msg) => {
    try {
      window.webkit.messageHandlers.bte.postMessage(msg);
    } catch (_) {
    }
  };
  const haptic = () => post({ type: "haptic", style: "selection" });
  const isCourses = () => main.classList.contains("view-courses");

  // ───────────── herramientas arriba ─────────────

  // Nombres cortos y claros en el teléfono; data-view y teclas intactos.
  const renameTab = (view, label) => {
    const btn = document.querySelector('.views button[data-view="' + view + '"]');
    const text = btn && btn.lastChild;
    if (text && text.nodeType === Node.TEXT_NODE) text.textContent = " " + label;
  };
  renameTab("miyu", "resumen");
  renameTab("grades", "notas");

  const ICONS = {
    filter: '<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',
    all: '<path d="M2.5 12s3.5-6.5 9.5-6.5 9.5 6.5 9.5 6.5-3.5 6.5-9.5 6.5S2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="2.8"/>',
    refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4.5V11h-6.5"/>'
  };
  const tools = document.createElement("div");
  tools.className = "m-tools";
  tools.innerHTML = [
    ["filter", "/", "Filtrar"],
    ["all", "a", "Mostrar entregadas y cursos anteriores"],
    ["refresh", "r", "Actualizar"]
  ].map(([id, k, label]) => `<button type="button" data-k="${k}" data-tool="${id}" aria-label="${label}" aria-pressed="false"><svg viewBox="0 0 24 24" aria-hidden="true">${ICONS[id]}</svg></button>`).join("");
  document.querySelector(".topbar").appendChild(tools);
  tools.querySelector('[data-tool="refresh"]').removeAttribute("aria-pressed");

  tools.addEventListener("click", (e) => {
    const btn = e.target.closest("button[data-k]");
    if (!btn) return;
    haptic();
    if (btn.dataset.tool === "filter") {
      const wrap = $("filter-wrap");
      if (!wrap.hidden) {
        $("filter").dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
        return;
      }
      setStack("list");
    }
    if (btn.dataset.tool === "all") btn.setAttribute("aria-pressed", String(btn.getAttribute("aria-pressed") !== "true"));
    key(btn.dataset.k);
  });

  // El filtro va bajo las pestañas: abajo lo taparía el teclado.
  const filterWrap = $("filter-wrap");
  document.body.insertBefore(filterWrap, main);
  const filterInput = $("filter");
  filterInput.setAttribute("enterkeyhint", "done");
  filterInput.setAttribute("placeholder", "filtrar…");

  // Mientras carga, el botón de actualizar gira.
  const mode = $("mode");
  const syncTools = () => {
    tools.querySelector('[data-tool="refresh"]').classList.toggle("m-spin", mode.dataset.mode === "load");
    tools.querySelector('[data-tool="refresh"]').setAttribute("aria-busy", String(mode.dataset.mode === "load"));
    tools.querySelector('[data-tool="filter"]').setAttribute("aria-pressed", String(!filterWrap.hidden));
  };
  new MutationObserver(syncTools).observe(document.body, { attributes: true, subtree: true, attributeFilter: ["data-mode", "hidden"] });
  syncTools();

  // ───────────── pila de paneles ─────────────

  const backBar = (label, onBack) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "m-back";
    btn.innerHTML = "‹ <b></b><span></span>";
    btn.querySelector("b").textContent = label;
    btn.setAttribute("aria-label", "Volver a " + label);
    btn.addEventListener("click", () => {
      haptic();
      onBack();
    });
    return btn;
  };

  const viewName = () => {
    const view = document.querySelector('.views button[aria-current="page"]');
    return view ? view.textContent.replace(/^\d\s*/, "").trim() || "lista" : "lista";
  };

  const detailBack = backBar("lista", () => setStack(isCourses() ? "mid" : "list"));
  const midBack = backBar("cursos", () => setStack("list"));
  $("pane-mid").insertBefore(midBack, $("pane-mid").firstChild);

  // app.js reemplaza el contenido del detalle en cada selección: la barra se
  // vuelve a poner arriba cada vez, con el destino como título.
  const ensureDetailBar = () => {
    const detail = $("detail");
    if (detail.firstChild !== detailBack) detail.insertBefore(detailBack, detail.firstChild);
    const dest = isCourses() ? ($("mid-title").textContent.trim() || "actividades") : viewName();
    detailBack.querySelector("b").textContent = dest;
    detailBack.querySelector("span").textContent = "";
    detailBack.setAttribute("aria-label", "Volver a " + dest);
    midBack.querySelector("span").textContent = $("mid-title").textContent;
  };
  new MutationObserver(ensureDetailBar).observe($("detail"), { childList: true });

  let stack = "list";
  let listScroll = 0;
  const setStack = (next) => {
    if (next === stack) return;
    if (stack === "list") listScroll = main.scrollTop;
    stack = next;
    main.dataset.mStack = next;
    for (const [id, visible] of [["pane-list", next === "list"], ["pane-dash", next === "list"], ["pane-mid", next === "mid"], ["pane-detail", next === "detail"]]) {
      if ($(id)) $(id).inert = !visible;
    }
    ensureDetailBar();
    if (next === "detail") $("detail").scrollTop = 0;
    else if (next === "list") requestAnimationFrame(() => (main.scrollTop = listScroll));
    else main.scrollTop = 0;
  };
  main.dataset.mStack = "list";

  // Tocar una fila la abre (app.js ya la seleccionó con su propio clic).
  $("list").addEventListener("click", (e) => {
    if (!e.target.closest(".row")) return;
    haptic();
    setStack(isCourses() ? "mid" : "detail");
  });
  $("mid").addEventListener("click", (e) => {
    if (!e.target.closest(".row")) return;
    haptic();
    setStack("detail");
  });
  for (const btn of document.querySelectorAll(".views button")) {
    btn.addEventListener("click", () => {
      haptic();
      setStack("list");
      main.scrollTop = 0;
    });
  }

  // Deslizar a la derecha sobre el detalle o las actividades: volver.
  let swipe = null;
  main.addEventListener("touchstart", (e) => {
    if (stack === "list" || e.touches.length !== 1) return;
    swipe = { x: e.touches[0].clientX, y: e.touches[0].clientY };
  }, { passive: true });
  main.addEventListener("touchend", (e) => {
    if (!swipe) return;
    const t = e.changedTouches[0];
    const dx = t.clientX - swipe.x;
    const dy = Math.abs(t.clientY - swipe.y);
    swipe = null;
    if (dx > 80 && dy < 50) {
      haptic();
      setStack(stack === "detail" && isCourses() ? "mid" : "list");
    }
  }, { passive: true });

  // Marca al tocar una fila.
  let pressed = null;
  const release = () => {
    if (pressed) pressed.classList.remove("m-press");
    pressed = null;
  };
  main.addEventListener("touchstart", (e) => {
    const row = e.target.closest(".row");
    if (!row) return;
    pressed = row;
    row.classList.add("m-press");
  }, { passive: true });
  main.addEventListener("touchmove", release, { passive: true });
  main.addEventListener("touchend", () => setTimeout(release, 150), { passive: true });
  main.addEventListener("touchcancel", release, { passive: true });

  // Los textos de ayuda de la lista vacía hablan de teclas: aquí, botones.
  // data-k es la tecla que app.js espera; "Escape" limpia el filtro.
  const emptyActions = (buttons) => {
    const actions = document.createElement("div");
    actions.className = "m-empty-actions";
    actions.innerHTML = buttons.map(([k, label]) => `<button type="button" class="btn" data-k="${k}">${label}</button>`).join("");
    actions.addEventListener("click", (e) => {
      const b = e.target.closest("[data-k]");
      if (!b) return;
      haptic();
      if (b.dataset.k === "a") tools.querySelector('[data-tool="all"]').setAttribute("aria-pressed", "true");
      if (b.dataset.k === "Escape") $("filter").dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      else key(b.dataset.k);
    });
    return actions;
  };
  const fixEmpty = () => {
    const note = $("list").querySelector(".empty-note");
    if (!note || (note.nextElementSibling && note.nextElementSibling.classList.contains("m-empty-actions"))) return;
    const text = note.textContent;
    if (/\n\s+a\s+ver entregadas/.test(text)) {
      note.textContent = "✓ sin pendientes por ahora.";
      note.after(emptyActions([["a", "ver entregadas"], ["2", "explorar cursos"], ["r", "actualizar"]]));
    } else if (/^nada coincide/.test(text.trim())) {
      note.after(emptyActions([["Escape", "limpiar filtro"]]));
    } else if (/^✓ todo entregado|^sin cursos/.test(text.trim())) {
      note.after(emptyActions([["r", "actualizar"]]));
    }
  };
  new MutationObserver(fixEmpty).observe($("list"), { childList: true });
  fixEmpty();

  // Resumen: fecha de hoy y pendientes. Los números salen de los contadores
  // que app.js calcula desde la caché, así que no se toca la red ni la
  // sesión. renderDash reescribe el panel: se vuelve a poner arriba.
  const dashHead = document.createElement("div");
  dashHead.className = "m-dash-head";
  dashHead.innerHTML = '<div class="m-dash-date"></div><div class="m-dash-counts"></div>';
  const dateFmt = new Intl.DateTimeFormat("es", { weekday: "long", day: "numeric", month: "long" });
  const ensureDashHead = () => {
    const dash = $("dash");
    if (!dash || !dash.childNodes.length) return;
    if (dash.firstChild !== dashHead) dash.insertBefore(dashHead, dash.firstChild);
    dashHead.children[0].textContent = dateFmt.format(new Date());
    const num = (sel) => {
      const el = document.querySelector("#counters " + sel + " b");
      return el ? el.textContent.trim() : "";
    };
    const parts = [];
    const overdue = num(".c-overdue");
    const today = num(".c-today");
    const week = num(".c-week");
    if (overdue !== "") parts.push(overdue + " vencidas");
    if (today !== "") parts.push(today + " hoy");
    if (week !== "") parts.push(week + " en 7 días");
    dashHead.children[1].textContent = parts.join(" · ");
    dashHead.children[1].hidden = !parts.length;
  };
  if ($("dash")) {
    new MutationObserver(ensureDashHead).observe($("dash"), { childList: true });
    ensureDashHead();
  }
  if ($("counters")) new MutationObserver(ensureDashHead).observe($("counters"), { childList: true, subtree: true, characterData: true });

  // La línea de estado lleva carga y errores: se anuncia a VoiceOver.
  if ($("msg")) $("msg").setAttribute("aria-live", "polite");

  ensureDetailBar();
})();
