// Preferencias compartidas por el content script y el popup.
// chrome.storage.sync es la fuente de verdad; en la página se guarda un espejo
// en localStorage para aplicar el tema en document_start sin parpadeo.
(() => {
  const bte = (globalThis.bte = globalThis.bte || {});

  bte.STORAGE_KEY = "bte";
  bte.MIRROR_KEY = "bte:prefs";

  bte.THEMES = [
    { id: "gruvbox", name: "gruvbox", dark: true },
    { id: "gruvbox-light", name: "gruvbox light", dark: false },
    { id: "tokyo", name: "tokyo night", dark: true },
    { id: "catppuccin", name: "catppuccin mocha", dark: true },
    { id: "nord", name: "nord", dark: true },
    { id: "phosphor", name: "phosphor", dark: true },
    { id: "amber", name: "amber", dark: true },
    { id: "paper", name: "paper", dark: false },
    { id: "matcha", name: "matcha", dark: false, desc: "papel crema y tinta verde, calma para leer" },
    { id: "lavanda", name: "lavanda", dark: false, desc: "lila suave con tinta ciruela" },
    { id: "sakura", name: "sakura", dark: false, desc: "rosa pétalo y acento cereza" },
    { id: "medianoche", name: "medianoche", dark: true, desc: "azul tinta y ámbar, para estudiar de noche" },
    { id: "terracota", name: "terracota", dark: true, desc: "barro cálido, arena y teja" },
    { id: "oceano", name: "océano", dark: true, desc: "petróleo profundo y espuma turquesa" },
    { id: "grafito", name: "grafito", dark: true, desc: "gris neutro y un solo acento lima" },
    { id: "contraste", name: "alto contraste", dark: true, a11y: true, desc: "negro, blanco y amarillo; foco y enlaces reforzados" },
    { id: "daltonismo", name: "daltonismo", dark: false, a11y: true, desc: "estados Okabe-Ito, distinguibles sin ver rojo/verde" }
  ];

  // Estilo = forma (tipografía, bordes, espaciado). Tema = colores. Se
  // combinan libremente, salvo los que traen su propia paleta (ownColors).
  bte.STYLES = [
    { id: "tui", name: "tui", desc: "terminal: monoespaciada, 1px, atajos a la vista", font: "", google: [] },
    { id: "nube", name: "nube", desc: "claro y suave: cielo pálido, serif para leer, botón azul vidrio", font: '"Instrument Sans", system-ui, -apple-system, "Segoe UI", sans-serif', google: ["Instrument+Sans:wght@400;500;600", "Source+Serif+4:opsz,wght@8..60,400;8..60,600"], ownColors: true },
    { id: "enfoque", name: "enfoque", desc: "lectura: sin cajas, listas limpias, títulos serif", font: '"Inter", system-ui, -apple-system, "Segoe UI", sans-serif', google: ["Inter:wght@400;500;600;700", "Source+Serif+4:opsz,wght@8..60,500;8..60,700"] },
    { id: "compacto", name: "compacto", desc: "densidad: ves el doble sin desplazarte", font: 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif', google: [] },
    { id: "editorial", name: "editorial", desc: "periódico: titulares serif, versalitas y números romanos", font: '"Libre Franklin", "Helvetica Neue", Arial, sans-serif', google: ["Libre+Franklin:wght@400;600;700", "Newsreader:ital,opsz,wght@0,6..72,400;0,6..72,600;0,6..72,700;1,6..72,600;1,6..72,700"] },
    { id: "neo", name: "neo", desc: "neobrutalismo: bordes gruesos, sombras duras, botones que se hunden", font: '"Archivo", "Helvetica Neue", Arial, sans-serif', google: ["Archivo:wght@500;600;700;800"], ownColors: true },
    { id: "retro", name: "retro 95", desc: "ventanas, biseles y barra de título azul", font: 'Tahoma, "MS Sans Serif", Verdana, "Segoe UI", sans-serif', google: [], ownColors: true },
    { id: "minecraft", name: "minecraft", desc: "bloques: tierra y pasto, paneles de inventario, botones de piedra y fuente de píxeles", font: '"Pixelify Sans", "Press Start 2P", ui-monospace, Menlo, Consolas, monospace', google: ["Pixelify+Sans:wght@400;500;600;700"], ownColors: true }
  ];

  bte.FONTS = [
    { id: "jetbrains", name: "JetBrains Mono", css: '"JetBrains Mono"', google: "JetBrains+Mono:wght@400;500;700" },
    { id: "plex", name: "IBM Plex Mono", css: '"IBM Plex Mono"', google: "IBM+Plex+Mono:wght@400;500;700" },
    { id: "fira", name: "Fira Code", css: '"Fira Code"', google: "Fira+Code:wght@400;500;700" },
    { id: "vt323", name: "VT323 (retro)", css: '"VT323"', google: "VT323" },
    { id: "system", name: "del sistema", css: "", google: "" }
  ];

  bte.DEFAULTS = {
    enabled: true,
    theme: "gruvbox",
    style: "tui",
    font: "jetbrains",
    fontSize: 14,
    scanlines: false,
    brackets: true,
    // Fondo con foto (la imagen vive en chrome.storage.local, ver bg.js).
    bgStyle: "dither",
    bgStrength: 70,
    bgPanel: 65, // opacidad de los paneles de Eminus y de la vista tui
    bgCard: 85, // opacidad de las tarjetas de curso y cajas de Eminus
    // Encuadre de la foto: desplazamiento (% de media ventana), zoom (%),
    // giro (grados) y volteos.
    bgX: 0,
    bgY: 0,
    bgZoom: 100,
    bgRotate: 0,
    bgFlipX: false,
    bgFlipY: false,
    bgPalette: true
  };

  bte.BG_FRAME_DEFAULTS = { bgX: 0, bgY: 0, bgZoom: 100, bgRotate: 0, bgFlipX: false, bgFlipY: false };

  bte.sanitize = (raw) => {
    const p = { ...bte.DEFAULTS, ...(raw && typeof raw === "object" ? raw : {}) };
    if (!bte.THEMES.some((t) => t.id === p.theme)) p.theme = bte.DEFAULTS.theme;
    if (!bte.FONTS.some((f) => f.id === p.font)) p.font = bte.DEFAULTS.font;
    if (p.style === "moderno" || p.style === "suizo") p.style = "nube";
    if (!bte.STYLES.some((s) => s.id === p.style)) p.style = bte.DEFAULTS.style;
    p.fontSize = Math.min(20, Math.max(11, Number(p.fontSize) || bte.DEFAULTS.fontSize));
    p.enabled = p.enabled !== false;
    p.scanlines = !!p.scanlines;
    p.brackets = p.brackets !== false;
    if (p.bgStyle !== "plain") p.bgStyle = "dither";
    p.bgStrength = Math.min(100, Math.max(10, Math.round(Number(p.bgStrength) || bte.DEFAULTS.bgStrength)));
    p.bgPanel = Math.min(100, Math.max(0, Math.round(Number(p.bgPanel ?? bte.DEFAULTS.bgPanel))));
    if (Number.isNaN(p.bgPanel)) p.bgPanel = bte.DEFAULTS.bgPanel;
    p.bgCard = Math.min(100, Math.max(0, Math.round(Number(p.bgCard ?? bte.DEFAULTS.bgCard))));
    if (Number.isNaN(p.bgCard)) p.bgCard = bte.DEFAULTS.bgCard;
    p.bgPalette = p.bgPalette !== false;
    const num = (v, def, min, max) => {
      const n = Number(v);
      return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : def;
    };
    p.bgX = num(p.bgX, 0, -100, 100);
    p.bgY = num(p.bgY, 0, -100, 100);
    p.bgZoom = num(p.bgZoom, 100, 50, 400);
    p.bgRotate = ((Math.round(num(p.bgRotate, 0, -3600, 3600)) % 360) + 540) % 360 - 180;
    p.bgFlipX = !!p.bgFlipX;
    p.bgFlipY = !!p.bgFlipY;
    return p;
  };

  bte.readMirror = () => {
    try {
      return bte.sanitize(JSON.parse(localStorage.getItem(bte.MIRROR_KEY) || "null"));
    } catch (_) {
      return bte.sanitize(null);
    }
  };

  bte.writeMirror = (prefs) => {
    try {
      localStorage.setItem(bte.MIRROR_KEY, JSON.stringify(prefs));
    } catch (_) {
    }
  };

  bte.loadPrefs = async () => {
    try {
      const data = await chrome.storage.sync.get(bte.STORAGE_KEY);
      return bte.sanitize(data[bte.STORAGE_KEY]);
    } catch (_) {
      return bte.readMirror();
    }
  };

  bte.savePrefs = async (patch) => {
    const next = bte.sanitize({ ...(await bte.loadPrefs()), ...patch });
    try {
      await chrome.storage.sync.set({ [bte.STORAGE_KEY]: next });
    } catch (_) {
    }
    return next;
  };

  bte.onPrefsChanged = (callback) => {
    try {
      chrome.storage.onChanged.addListener((changes, area) => {
        if (area === "sync" && changes[bte.STORAGE_KEY]) {
          callback(bte.sanitize(changes[bte.STORAGE_KEY].newValue));
        }
      });
    } catch (_) {
    }
  };

  bte.themeById = (id) => bte.THEMES.find((t) => t.id === id) || bte.THEMES[0];
  bte.fontById = (id) => bte.FONTS.find((f) => f.id === id) || bte.FONTS[0];
  bte.styleById = (id) => bte.STYLES.find((s) => s.id === id) || bte.STYLES[0];
})();
