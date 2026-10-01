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

  // «tareas calificadas» no cabe junto a las demás pestañas.
  const gradesTab = document.querySelector('.views button[data-view="grades"]');
  if (gradesTab) gradesTab.lastChild.textContent = " notas";

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
  new MutationObserver(() => {
    tools.querySelector('[data-tool="refresh"]').classList.toggle("m-spin", mode.dataset.mode === "load");
    tools.querySelector('[data-tool="filter"]').setAttribute("aria-pressed", String(!filterWrap.hidden));
  }).observe(document.body, { attributes: true, subtree: true, attributeFilter: ["data-mode", "hidden"] });

  // ───────────── pila de paneles ─────────────

  const backBar = (label, onBack) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "m-back";
    btn.innerHTML = "‹ <b></b><span></span>";
    btn.querySelector("b").textContent = label;
    btn.addEventListener("click", () => {
      haptic();
      onBack();
    });
    return btn;
  };

  const detailBack = backBar("volver", () => setStack(isCourses() ? "mid" : "list"));
  const midBack = backBar("cursos", () => setStack("list"));
  $("pane-mid").insertBefore(midBack, $("pane-mid").firstChild);

  // app.js reemplaza el contenido del detalle en cada selección: la barra se
  // vuelve a poner arriba cada vez.
  const ensureDetailBar = () => {
    const detail = $("detail");
    if (detail.firstChild !== detailBack) detail.insertBefore(detailBack, detail.firstChild);
    const view = document.querySelector('.views button[aria-current="page"]');
    detailBack.querySelector("span").textContent = isCourses() ? $("mid-title").textContent : view ? view.textContent.replace(/^\d\s*/, "").trim() : "";
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
  const fixEmpty = () => {
    const note = $("list").querySelector(".empty-note");
    if (!note || !/\n\s+a\s+ver entregadas/.test(note.textContent)) return;
    note.textContent = "✓ sin pendientes por ahora.";
    const actions = document.createElement("div");
    actions.className = "m-empty-actions";
    actions.innerHTML = '<button type="button" class="btn" data-k="a">ver entregadas</button><button type="button" class="btn" data-k="2">explorar cursos</button><button type="button" class="btn" data-k="r">actualizar</button>';
    actions.addEventListener("click", (e) => {
      const b = e.target.closest("[data-k]");
      if (!b) return;
      if (b.dataset.k === "a") tools.querySelector('[data-tool="all"]').setAttribute("aria-pressed", "true");
      key(b.dataset.k);
    });
    note.after(actions);
  };
  new MutationObserver(fixEmpty).observe($("list"), { childList: true });
  fixEmpty();
  ensureDetailBar();
})();
