// Paleta de comandos con búsqueda difusa y ventana de ayuda.
(() => {
  const bte = globalThis.bte;

  const esc = (text) =>
    String(text).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  // Coincidencia por subsecuencia: premia letras seguidas e inicios de palabra.
  // Devuelve null si no coincide, o { score, hits } con los índices resaltables.
  bte.fuzzy = (query, text) => {
    const q = bte.norm(query).replace(/\s+/g, "");
    if (!q) return { score: 0, hits: [] };
    const t = bte.norm(text);
    // bte.norm quita acentos pero conserva la longitud sólo si no hay
    // espacios dobles; el texto de entrada ya viene limpio.
    const hits = [];
    let score = 0;
    let ti = 0;
    let prev = -2;
    for (const ch of q) {
      const found = t.indexOf(ch, ti);
      if (found === -1) return null;
      const wordStart = found === 0 || /[\s\-_/(.]/.test(t[found - 1]);
      score += 1;
      if (found === prev + 1) score += 3;
      if (wordStart) score += 4;
      score -= Math.min(3, (found - ti) * 0.1);
      hits.push(found);
      prev = found;
      ti = found + 1;
    }
    if (t.startsWith(q)) score += 6;
    return { score, hits };
  };

  const highlight = (text, hits) => {
    if (!hits || !hits.length) return esc(text);
    const set = new Set(hits);
    let out = "";
    for (let i = 0; i < text.length; i += 1) {
      out += set.has(i) ? "<mark>" + esc(text[i]) + "</mark>" : esc(text[i]);
    }
    return out;
  };

  const overlay = () => {
    let el = document.getElementById("bte-overlay");
    if (!el) {
      el = document.createElement("div");
      el.id = "bte-overlay";
      el.hidden = true;
      el.addEventListener("mousedown", (e) => {
        if (e.target === el) bte.closeOverlay();
      });
      document.body.appendChild(el);
    }
    return el;
  };

  let restoreFocus = null;
  let onClose = null;

  bte.overlayOpen = () => {
    const el = document.getElementById("bte-overlay");
    return !!el && !el.hidden;
  };

  bte.closeOverlay = () => {
    const el = document.getElementById("bte-overlay");
    if (!el || el.hidden) return;
    el.hidden = true;
    el.innerHTML = "";
    const cb = onClose;
    onClose = null;
    if (cb) cb();
    if (restoreFocus && restoreFocus.isConnected) {
      try {
        restoreFocus.focus({ preventScroll: true });
      } catch (_) {
      }
    }
    restoreFocus = null;
  };

  const showWindow = (html, closeCb) => {
    const el = overlay();
    restoreFocus = document.activeElement;
    onClose = closeCb || null;
    el.innerHTML = html;
    el.hidden = false;
    return el;
  };

  // items: [{ group, label, meta, hint, keywords, run }]
  // Un item con `range: { min, max, step, value, format, onInput, onCommit }`
  // se dibuja con un deslizador en la misma fila: ← → lo mueven cuando está
  // seleccionado, onInput da la vista previa y onCommit guarda (con un respiro,
  // y siempre al cerrar la paleta).
  // `items` puede ser una función async; se muestra "cargando" mientras llega.
  bte.openPalette = ({ title = "comandos", placeholder = "escribe para filtrar…", sigil = ":", items, initialQuery = "" }) => {
    const el = showWindow(
      `<div class="bte-window" role="dialog" aria-label="${esc(title)}">
        <div class="bte-titlebar"><span>${esc(title)}</span></div>
        <label class="bte-prompt">
          <span class="bte-prompt-sigil">${esc(sigil)}</span>
          <input type="text" spellcheck="false" autocomplete="off" placeholder="${esc(placeholder)}" aria-label="${esc(title)}" />
          <span class="bte-count"></span>
        </label>
        <ul class="bte-list" role="listbox"></ul>
        <div class="bte-footer">
          <span><kbd>↑↓</kbd> mover</span>
          <span><kbd>enter</kbd> abrir</span>
          <span><kbd>tab</kbd> completar</span>
          <span><kbd>esc</kbd> cerrar</span>
        </div>
      </div>`,
      () => {
        flushRanges();
        bte.setMode && bte.setMode();
      }
    );
    bte.setMode && bte.setMode("cmd");

    const pending = new Map(); // item → timeout del guardado
    const commitRange = (item) => {
      clearTimeout(pending.get(item));
      pending.delete(item);
      if (item.range.onCommit) item.range.onCommit(item.range.value);
    };
    const flushRanges = () => [...pending.keys()].forEach(commitRange);
    const setRange = (item, value, row) => {
      const r = item.range;
      const v = Math.min(r.max, Math.max(r.min, Math.round(value / r.step) * r.step));
      if (v === r.value) return;
      r.value = v;
      if (row) {
        const slider = row.querySelector(".bte-item-range");
        if (slider) slider.value = v;
        const out = row.querySelector(".bte-item-value");
        if (out) out.textContent = (r.format || String)(v);
      }
      if (r.onInput) r.onInput(v);
      clearTimeout(pending.get(item));
      pending.set(item, setTimeout(() => commitRange(item), 350));
    };

    const input = el.querySelector("input");
    const list = el.querySelector(".bte-list");
    const count = el.querySelector(".bte-count");
    let all = [];
    let shown = [];
    let selected = 0;
    let loading = typeof items === "function";

    const render = () => {
      const query = input.value;
      if (!query.trim()) {
        shown = all.map((item) => ({ item, hits: [] }));
      } else {
        shown = all
          .map((item) => {
            const m = bte.fuzzy(query, item.label);
            if (m) return { item, hits: m.hits, score: m.score + 5 };
            // Respaldo: palabras clave, sólo si coinciden de forma compacta
            // (evita que «cursos anteriores» atrape cualquier consulta).
            const k = bte.fuzzy(query, (item.keywords || "") + " " + (item.meta || ""));
            const minScore = bte.norm(query).replace(/\s+/g, "").length * 3;
            return k && k.score >= minScore ? { item, hits: [], score: k.score - 10 } : null;
          })
          .filter(Boolean)
          .sort((a, b) => b.score - a.score);
      }
      selected = Math.min(selected, Math.max(0, shown.length - 1));
      count.textContent = loading ? "…" : shown.length + "/" + all.length;

      if (loading) {
        list.innerHTML = `<li class="bte-empty">cargando…</li>`;
        return;
      }
      if (!shown.length) {
        list.innerHTML = `<li class="bte-empty">sin resultados para «${esc(query)}»</li>`;
        return;
      }
      let html = "";
      let lastGroup = null;
      const grouped = !query.trim();
      shown.forEach(({ item, hits }, index) => {
        if (grouped && item.group !== lastGroup) {
          html += `<li class="bte-group" role="presentation">${esc(item.group || "")}</li>`;
          lastGroup = item.group;
        }
        const meta = grouped ? item.meta || "" : item.meta || item.group || "";
        const r = item.range;
        const right = r
          ? `<span class="bte-item-meta bte-item-slider"><input type="range" class="bte-item-range" tabindex="-1" min="${r.min}" max="${r.max}" step="${r.step}" value="${r.value}" aria-label="${esc(item.label)}" /><output class="bte-item-value">${esc((r.format || String)(r.value))}</output></span>`
          : `<span class="bte-item-meta">${item.hint ? `<kbd>${esc(item.hint)}</kbd> ` : ""}${esc(meta)}</span>`;
        html += `<li class="bte-item${r ? " bte-item-has-range" : ""}" role="option" data-index="${index}" aria-selected="${index === selected}">
          <span class="bte-item-mark">${index === selected ? "❯" : " "}</span>
          <span class="bte-item-label">${highlight(item.label, hits)}</span>
          ${right}
        </li>`;
      });
      list.innerHTML = html;
      const current = list.querySelector('[aria-selected="true"]');
      if (current) current.scrollIntoView({ block: "nearest" });
    };

    const move = (delta) => {
      if (!shown.length) return;
      selected = (selected + delta + shown.length) % shown.length;
      render();
    };

    const run = async (index) => {
      const entry = shown[index];
      if (!entry) return;
      bte.closeOverlay();
      try {
        await entry.item.run();
      } catch (err) {
        bte.toast("error: " + (err && err.message ? err.message : err));
      }
    };

    input.addEventListener("input", () => {
      selected = 0;
      render();
    });

    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Escape") {
        e.preventDefault();
        bte.closeOverlay();
      } else if (e.key === "ArrowDown" || (e.ctrlKey && (e.key === "j" || e.key === "n"))) {
        e.preventDefault();
        move(1);
      } else if (e.key === "ArrowUp" || (e.ctrlKey && (e.key === "k" || e.key === "p"))) {
        e.preventDefault();
        move(-1);
      } else if (e.key === "PageDown") {
        e.preventDefault();
        move(8);
      } else if (e.key === "PageUp") {
        e.preventDefault();
        move(-8);
      } else if (e.key === "Tab") {
        e.preventDefault();
        const entry = shown[selected];
        if (entry) {
          input.value = entry.item.label;
          render();
        }
      } else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && shown[selected] && shown[selected].item.range) {
        // Sobre un deslizador, las flechas lo mueven en vez del cursor de texto.
        e.preventDefault();
        const item = shown[selected].item;
        const step = item.range.step * (e.shiftKey ? 4 : 1);
        setRange(item, item.range.value + (e.key === "ArrowRight" ? step : -step), list.querySelector(`.bte-item[data-index="${selected}"]`));
      } else if (e.key === "Enter") {
        e.preventDefault();
        if (shown[selected] && shown[selected].item.range) bte.closeOverlay();
        else run(selected);
      } else if (e.key === "Backspace" && !input.value && sigil !== ":") {
        // En una subpaleta, borrar con el campo vacío vuelve a la principal.
        e.preventDefault();
        bte.closeOverlay();
        bte.openCommandPalette();
      }
    });
    // Las teclas no deben llegar a Eminus ni a Miyu mientras escribes aquí.
    input.addEventListener("keyup", (e) => e.stopPropagation());
    input.addEventListener("keypress", (e) => e.stopPropagation());

    list.addEventListener("mousemove", (e) => {
      // Mientras se arrastra un deslizador no se redibuja la lista.
      if (e.buttons) return;
      const li = e.target.closest(".bte-item");
      if (!li) return;
      const index = Number(li.dataset.index);
      if (index !== selected) {
        selected = index;
        render();
      }
    });
    list.addEventListener("click", (e) => {
      const li = e.target.closest(".bte-item");
      if (!li) return;
      const entry = shown[Number(li.dataset.index)];
      // En una fila con deslizador el clic no ejecuta nada: el foco vuelve al
      // campo de texto para seguir con ← →.
      if (entry && entry.item.range) input.focus();
      else run(Number(li.dataset.index));
    });
    list.addEventListener("input", (e) => {
      const slider = e.target.closest(".bte-item-range");
      if (!slider) return;
      const li = slider.closest(".bte-item");
      const entry = shown[Number(li.dataset.index)];
      if (entry) setRange(entry.item, Number(slider.value), li);
    });

    input.value = initialQuery;
    input.focus();

    const load = async () => {
      if (typeof items === "function") {
        render();
        try {
          all = await items();
        } catch (err) {
          all = [];
          loading = false;
          list.innerHTML = `<li class="bte-empty">error: ${esc(err && err.message ? err.message : err)}</li>`;
          return;
        }
        loading = false;
      } else {
        all = items;
      }
      if (document.getElementById("bte-overlay")?.contains(input)) render();
    };
    load();
  };

  // ───────────── ayuda ─────────────

  const LOGO = [
    "┏┓ ┏━╸╺┳╸╺┳╸┏━╸┏━┓   ╺┳╸╻ ╻┏━┓┏┓╻",
    "┣┻┓┣╸  ┃  ┃ ┣╸ ┣┳┛    ┃ ┣━┫┣━┫┃┗┫",
    "┗━┛┗━╸ ╹  ╹ ┗━╸╹┗╸    ╹ ╹ ╹╹ ╹╹ ╹",
    "            ┏━╸┏┳┓╻┏┓╻╻ ╻┏━┓",
    "            ┣╸ ┃┃┃┃┃┗┫┃ ┃┗━┓",
    "            ┗━╸╹ ╹╹╹ ╹┗━┛┗━┛"
  ].join("\n");

  bte.showHelp = () => {
    const sections = bte.SECTIONS.map((s) => `<dt>g ${s.key}</dt><dd>${esc(s.label)}</dd>`).join("");
    showWindow(
      `<div class="bte-window" role="dialog" aria-label="ayuda">
        <div class="bte-titlebar"><span>atajos</span><span class="bte-title-dim">esc cierra</span></div>
        <div class="bte-help" tabindex="-1">
          <pre aria-hidden="true">${LOGO}</pre>
          <div class="bte-help-grid">
            <div>
              <h4>general</h4>
              <dl>
                <dt>: · ctrl k</dt><dd>paleta de comandos</dd>
                <dt>ñ</dt><dd>buscar curso</dd>
                <dt>;</dt><dd>buscar actividad en todos los cursos</dd>
                <dt>g r</dt><dd>recientes: volver a donde estabas</dd>
                <dt>z</dt><dd>modo lectura</dd>
                <dt>A</dt><dd>agenda de pendientes</dd>
                <dt>c · e</dt><dd>siguiente tema · estilo</dd>
                <dt>H / L</dt><dd>atrás / adelante</dd>
                <dt>alt ⇧ b</dt><dd>activar o desactivar el reskin</dd>
              </dl>
              <h4>moverse en la página</h4>
              <dl>
                <dt>j / k</dt><dd>siguiente / anterior elemento</dd>
                <dt>enter · o</dt><dd>abrir el elemento</dd>
                <dt>g g / G</dt><dd>primero / último</dd>
                <dt>d / u</dt><dd>media página abajo / arriba</dd>
                <dt>esc</dt><dd>quitar selección</dd>
              </dl>
            </div>
            <div>
              <h4>ir a (dentro de un curso)</h4>
              <dl>${sections}</dl>
              <h4>miyu --pendientes</h4>
              <dl>
                <dt>alt e</dt><dd>mostrar / ocultar panel</dd>
                <dt>t · r · /</dt><dd>hoy · actualizar · buscar</dd>
                <dt>1 – 7</dt><dd>pestañas del panel</dd>
                <dt>?</dt><dd>atajos de miyu</dd>
              </dl>
            </div>
          </div>
          <p class="bte-help-note">No se activan mientras escribes.</p>
        </div>
      </div>`,
      () => bte.setMode && bte.setMode()
    );
    bte.setMode && bte.setMode("cmd");
    const help = document.querySelector("#bte-overlay .bte-help");
    if (help) help.focus();
  };
})();
