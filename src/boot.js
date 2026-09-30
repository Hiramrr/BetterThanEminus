// Corre en document_start: aplica el tema antes del primer pintado.
(() => {
  const bte = globalThis.bte;
  const root = document.documentElement;
  const FONT_LINK_ID = "bte-font-link";

  const MONO_STACK = '"JetBrains Mono", "SF Mono", ui-monospace, Menlo, Consolas, "Liberation Mono", monospace';

  // Un solo <link> con todas las familias que necesitan la fuente mono y el
  // estilo activos.
  const ensureFontLink = (families) => {
    const href = families.length
      ? "https://fonts.googleapis.com/css2?" + families.map((f) => "family=" + f).join("&") + "&display=swap"
      : "";
    let link = document.getElementById(FONT_LINK_ID);
    if (!href) {
      if (link) link.remove();
      return;
    }
    if (link && link.href === href) return;
    if (!link) {
      link = document.createElement("link");
      link.id = FONT_LINK_ID;
      link.rel = "stylesheet";
      (document.head || root).appendChild(link);
    }
    link.href = href;
  };

  bte.applyPrefs = (prefs) => {
    bte.prefs = prefs;
    const font = bte.fontById(prefs.font);
    const style = bte.styleById(prefs.style);
    const isTui = style.id === "tui";
    const mono = (font.css ? font.css + ", " : "") + MONO_STACK;
    root.classList.toggle("bte", prefs.enabled);
    // Corchetes y scanlines son adornos de terminal: sólo en el estilo tui.
    root.classList.toggle("bte-scanlines", prefs.enabled && isTui && prefs.scanlines && window.top === window);
    root.classList.toggle("bte-brackets", isTui && prefs.brackets);
    root.classList.toggle("bte-top", window.top === window);
    root.dataset.bteTheme = prefs.theme;
    root.dataset.bteStyle = style.id;
    root.style.setProperty("--bte-zoom", String(Math.round((prefs.fontSize / 14) * 1000) / 1000));
    root.style.setProperty("--bte-mono", mono);
    root.style.setProperty("--bte-font", style.font || mono);
    if (prefs.enabled) ensureFontLink([font.google, ...style.google].filter(Boolean));
    bte.writeMirror(prefs);
    document.dispatchEvent(new CustomEvent("bte:prefs"));
  };

  bte.applyPrefs(bte.readMirror());
  bte.loadPrefs().then(bte.applyPrefs);
  bte.onPrefsChanged(bte.applyPrefs);
})();
