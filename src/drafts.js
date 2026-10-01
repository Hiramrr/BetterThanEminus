// Borradores automáticos: lo que escribes en comentarios, foros o entregas
// se guarda mientras escribes (sólo en este navegador). Si la sesión vence o
// la página se recarga, al volver al campo se ofrece recuperarlo.
(() => {
  const bte = globalThis.bte;
  if (bte.draftsStarted) return;
  bte.draftsStarted = true;

  const STORE = "bte:drafts";
  const MAX_AGE = 14 * 24 * 3600 * 1000;
  const MAX_ITEMS = 60;
  const MIN_LENGTH = 12;

  const read = () => {
    try {
      const data = JSON.parse(localStorage.getItem(STORE) || "{}");
      return data && typeof data === "object" ? data : {};
    } catch (_) {
      return {};
    }
  };

  const write = (data) => {
    const now = Date.now();
    const entries = Object.entries(data)
      .filter(([, d]) => d && now - d.ts < MAX_AGE)
      .sort((a, b) => b[1].ts - a[1].ts)
      .slice(0, MAX_ITEMS);
    try {
      localStorage.setItem(STORE, JSON.stringify(Object.fromEntries(entries)));
    } catch (_) {
    }
  };

  // Nunca se guardan contraseñas, búsquedas, el login ni campos de Miyu o
  // de esta extensión.
  const EXCLUDED = "m-auth, #eminus-pending-panel, [id^='ep-'], #bte-overlay, #bte-statusline, .EMINUSPleca, .SEARCH_BAR_BOX, .FILTER_BOX";

  const editableOf = (target) => {
    if (!(target instanceof Element)) return null;
    const el = target.closest("textarea, input, [contenteditable=''], [contenteditable='true']");
    if (!el || el.closest(EXCLUDED)) return null;
    if (el instanceof HTMLInputElement) {
      if (el.type !== "text" && el.type !== "") return null;
      if (/search|buscar|filtr/i.test(el.placeholder + " " + (el.getAttribute("aria-label") || ""))) return null;
      if (/user|usuario|mail|correo|pass/i.test(el.name + " " + el.id + " " + el.autocomplete)) return null;
    }
    if (el.isContentEditable) {
      // Froala y otros editores: el contenedor editable más externo.
      let top = el;
      while (top.parentElement && top.parentElement.isContentEditable) top = top.parentElement;
      return top;
    }
    return el;
  };

  const pageKey = () => {
    const params = new URLSearchParams(location.search);
    for (const k of [...params.keys()]) if (k.startsWith("_")) params.delete(k);
    const course = bte.isTop && bte.currentCourseName ? bte.currentCourseName() : "";
    return location.pathname + (params.toString() ? "?" + params : "") + (course ? "#" + course : "");
  };

  const fieldName = (el) =>
    el.id || el.getAttribute("name") || el.getAttribute("placeholder") || el.getAttribute("aria-label") || el.tagName.toLowerCase();

  const fieldKey = (el) => {
    const name = fieldName(el);
    const all = [...document.querySelectorAll("textarea, input, [contenteditable]")]
      .map(editableOf)
      .filter((x, i, arr) => x && arr.indexOf(x) === i && fieldName(x) === name);
    return pageKey() + "::" + name + "::" + Math.max(0, all.indexOf(el));
  };

  const valueOf = (el) => (el.isContentEditable ? el.innerText.trim() : el.value.trim());

  const setValue = (el, draft) => {
    if (el.isContentEditable) {
      el.innerHTML = draft.html || "";
      if (!draft.html) el.innerText = draft.text;
    } else {
      const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, "value").set.call(el, draft.text);
    }
    // Que Angular/Froala se enteren del cambio.
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    el.dispatchEvent(new KeyboardEvent("keyup", { bubbles: true }));
  };

  const labelOf = (el) => {
    const where = bte.isTop && bte.pageContext ? bte.pageContext().title : document.title;
    return (where || "eminus") + " · " + (el.getAttribute("placeholder") || (el.isContentEditable ? "editor" : "campo de texto"));
  };

  // Elementos que ya tuvieron texto en esta visita: si luego quedan vacíos
  // sin que el usuario borre (Eminus los limpia al enviar), el borrador se
  // descarta.
  const touched = new Map();
  let saveTimer = null;

  document.addEventListener(
    "input",
    (e) => {
      if (!e.isTrusted) return;
      const el = editableOf(e.target);
      if (!el) return;
      hideChip();
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {
        const key = fieldKey(el);
        const text = valueOf(el);
        const data = read();
        touch(el, key);
        if (text.length < MIN_LENGTH) {
          if (!text && data[key]) {
            delete data[key];
            write(data);
          }
          return;
        }
        data[key] = { text, html: el.isContentEditable ? el.innerHTML : "", ts: Date.now(), label: labelOf(el) };
        write(data);
        if (bte.toast && !el.dataset.bteDraftNoted) {
          el.dataset.bteDraftNoted = "1";
          bte.toast("✎ borrador guardado mientras escribes", 1800);
        }
      }, 500);
    },
    true
  );

  // Sólo corre mientras haya campos con texto que vigilar.
  let sweepTimer = null;
  const sweep = () => {
    const data = read();
    let changed = false;
    for (const [el, key] of touched) {
      if (!el.isConnected) {
        touched.delete(el);
        continue;
      }
      if (!valueOf(el) && data[key] && document.activeElement !== el) {
        delete data[key];
        touched.delete(el);
        changed = true;
      }
    }
    if (changed) write(data);
    if (!touched.size) {
      clearInterval(sweepTimer);
      sweepTimer = null;
    }
  };
  const touch = (el, key) => {
    touched.set(el, key);
    sweepTimer ??= setInterval(sweep, 2000);
  };

  // ───────────── ofrecer recuperar ─────────────

  let chip = null;
  let chipFor = null;

  const hideChip = () => {
    if (chip) chip.hidden = true;
    chipFor = null;
  };

  const ago = (ts) => {
    const min = Math.round((Date.now() - ts) / 60000);
    if (min < 1) return "hace un momento";
    if (min < 60) return "hace " + min + " min";
    const h = Math.round(min / 60);
    return h < 24 ? "hace " + h + " h" : "hace " + Math.round(h / 24) + " d";
  };

  const showChip = (el, key, draft) => {
    if (!chip) {
      chip = document.createElement("div");
      chip.id = "bte-draft-chip";
      chip.setAttribute("role", "status");
      chip.innerHTML = `<span class="bte-draft-text"></span>
        <button type="button" data-act="restore">recuperar</button>
        <button type="button" data-act="discard">descartar</button>`;
      chip.addEventListener("mousedown", (e) => e.preventDefault());
      chip.addEventListener("click", (e) => {
        const btn = e.target.closest("button");
        if (!btn || !chipFor) return;
        const { el: target, key: k, draft: d } = chipFor;
        if (btn.dataset.act === "restore") {
          setValue(target, d);
          touch(target, k);
          target.focus();
        } else {
          const data = read();
          delete data[k];
          write(data);
        }
        hideChip();
      });
      document.body.appendChild(chip);
    }
    const preview = draft.text.replace(/\s+/g, " ").slice(0, 60);
    chip.querySelector(".bte-draft-text").textContent = "✎ borrador " + ago(draft.ts) + ": «" + preview + (draft.text.length > 60 ? "…" : "") + "»";
    const r = el.getBoundingClientRect();
    chip.style.left = Math.max(8, Math.min(r.left, window.innerWidth - 360)) + "px";
    chip.style.top = Math.min(window.innerHeight - 60, r.bottom + 6) + "px";
    chip.hidden = false;
    chipFor = { el, key, draft };
  };

  document.addEventListener(
    "focusin",
    (e) => {
      if (!document.documentElement.classList.contains("bte")) return;
      const el = editableOf(e.target);
      if (!el || valueOf(el)) return;
      const key = fieldKey(el);
      const draft = read()[key];
      if (draft && draft.text) showChip(el, key, draft);
    },
    true
  );

  document.addEventListener(
    "focusout",
    () => setTimeout(() => {
      if (chipFor && document.activeElement !== chipFor.el) hideChip();
    }, 150),
    true
  );

  window.addEventListener("scroll", hideChip, { capture: true, passive: true });

  // Para la paleta: todos los borradores guardados (copiar al portapapeles).
  bte.listDrafts = () =>
    Object.entries(read())
      .map(([key, d]) => ({ key, ...d }))
      .sort((a, b) => b.ts - a.ts);

  bte.draftAgo = ago;

  bte.deleteDraft = (key) => {
    const data = read();
    delete data[key];
    write(data);
  };
})();
