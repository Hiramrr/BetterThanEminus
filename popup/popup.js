(() => {
  const bte = globalThis.bte;
  const root = document.documentElement;
  const $ = (id) => document.getElementById(id);

  // Muestras de color para la lista (fondo, texto, acento, rojo).
  const SWATCHES = {
    gruvbox: ["#282828", "#ebdbb2", "#fabd2f", "#fb4934"],
    "gruvbox-light": ["#fbf1c7", "#3c3836", "#af3a03", "#9d0006"],
    tokyo: ["#1a1b26", "#c0caf5", "#7aa2f7", "#f7768e"],
    catppuccin: ["#1e1e2e", "#cdd6f4", "#cba6f7", "#f38ba8"],
    nord: ["#2e3440", "#d8dee9", "#88c0d0", "#bf616a"],
    phosphor: ["#0a0f0a", "#33ff66", "#9dffb5", "#ff5f56"],
    amber: ["#120b02", "#ffb000", "#ffd479", "#ff6a3d"],
    paper: ["#fafaf7", "#1b1b1b", "#6b6b6b", "#c0392b"],
    matcha: ["#f4f1e8", "#2b3a2e", "#4a7336", "#a93a2a"],
    lavanda: ["#f3f0f8", "#332b45", "#6445ad", "#ad3249"],
    sakura: ["#fbf1f3", "#3d2a30", "#b33c61", "#396393"],
    medianoche: ["#0f1526", "#d8dcea", "#f2b04a", "#82aaff"],
    terracota: ["#231a16", "#eadccb", "#e8845a", "#adc680"],
    oceano: ["#0b1f24", "#cfe6e6", "#4fd1c5", "#ff7a85"],
    grafito: ["#16171a", "#d6d8dc", "#c6f432", "#7fb3ff"],
    contraste: ["#000000", "#ffffff", "#ffe600", "#66ccff"],
    daltonismo: ["#f7f7f5", "#1f1f1f", "#0068a3", "#b84f00"]
  };

  const themeGroup = (t) => (t.a11y ? "accesibles" : t.dark ? "oscuros" : "claros");

  let prefs = bte.sanitize(null);

  const render = () => {
    root.dataset.bteTheme = prefs.theme;
    $("enabled").checked = prefs.enabled;
    $("brackets").checked = prefs.brackets;
    $("scanlines").checked = prefs.scanlines;
    $("font").value = prefs.font;
    $("zoom-value").textContent = Math.round((prefs.fontSize / 14) * 100) + "%";
    for (const li of $("styles").children) {
      const selected = li.dataset.id === prefs.style;
      li.setAttribute("aria-selected", String(selected));
      li.tabIndex = selected ? 0 : -1;
      li.firstElementChild.textContent = selected ? "❯" : " ";
    }
    const style = bte.styleById(prefs.style);
    $("theme-note").textContent = style.ownColors ? "(" + style.name + " usa sus colores)" : "";
    $("themes").classList.toggle("muted", !!style.ownColors);
    $("brackets").disabled = prefs.style !== "tui";
    $("scanlines").disabled = prefs.style !== "tui";
    for (const li of $("themes").querySelectorAll("li[data-id]")) {
      const selected = li.dataset.id === prefs.theme;
      li.setAttribute("aria-selected", String(selected));
      li.tabIndex = selected ? 0 : -1;
      li.firstElementChild.textContent = selected ? "❯" : " ";
    }
  };

  // Se pinta antes de guardar: el cambio se ve al instante.
  const save = async (patch) => {
    prefs = bte.sanitize({ ...prefs, ...patch });
    render();
    prefs = await bte.savePrefs(patch);
  };

  const buildThemes = () => {
    const list = $("themes");
    const ordered = ["oscuros", "claros", "accesibles"].flatMap((g) => bte.THEMES.filter((t) => themeGroup(t) === g));
    let lastGroup = null;
    for (const theme of ordered) {
      if (themeGroup(theme) !== lastGroup) {
        lastGroup = themeGroup(theme);
        const header = document.createElement("li");
        header.className = "group";
        header.setAttribute("role", "presentation");
        header.textContent = lastGroup;
        list.appendChild(header);
      }
      const li = document.createElement("li");
      li.dataset.id = theme.id;
      li.setAttribute("role", "option");
      const mark = document.createElement("span");
      const name = document.createElement("span");
      name.textContent = theme.name;
      const swatch = document.createElement("span");
      swatch.className = "swatch";
      for (const color of SWATCHES[theme.id] || []) {
        const i = document.createElement("i");
        i.style.background = color;
        swatch.appendChild(i);
      }
      li.append(mark, name, swatch);
      if (theme.desc) li.title = theme.desc;
      li.addEventListener("click", () => save({ theme: theme.id }));
      list.appendChild(li);
    }
    list.addEventListener("keydown", (e) => {
      const index = ordered.findIndex((t) => t.id === prefs.theme);
      let next = null;
      if (e.key === "ArrowDown" || e.key === "j") next = (index + 1) % ordered.length;
      if (e.key === "ArrowUp" || e.key === "k") next = (index - 1 + ordered.length) % ordered.length;
      if (next === null) return;
      e.preventDefault();
      const theme = ordered[next];
      save({ theme: theme.id }).then(() => list.querySelector(`li[data-id="${theme.id}"]`).focus());
    });
  };

  const buildStyles = () => {
    const list = $("styles");
    for (const style of bte.STYLES) {
      const li = document.createElement("li");
      li.dataset.id = style.id;
      li.setAttribute("role", "option");
      li.title = style.desc;
      const mark = document.createElement("span");
      const name = document.createElement("span");
      name.textContent = style.name;
      li.append(mark, name);
      li.addEventListener("click", () => save({ style: style.id }));
      list.appendChild(li);
    }
    list.addEventListener("keydown", (e) => {
      const index = bte.STYLES.findIndex((s) => s.id === prefs.style);
      let next = null;
      if (e.key === "ArrowDown" || e.key === "j") next = (index + 1) % bte.STYLES.length;
      if (e.key === "ArrowUp" || e.key === "k") next = (index - 1 + bte.STYLES.length) % bte.STYLES.length;
      if (next === null) return;
      e.preventDefault();
      const style = bte.STYLES[next];
      save({ style: style.id }).then(() => list.children[next].focus());
    });
  };

  const buildFonts = () => {
    for (const font of bte.FONTS) {
      const option = document.createElement("option");
      option.value = font.id;
      option.textContent = font.name;
      $("font").appendChild(option);
    }
  };

  const init = async () => {
    try {
      $("version").textContent = "v" + chrome.runtime.getManifest().version;
    } catch (_) {
    }
    buildStyles();
    buildThemes();
    buildFonts();
    prefs = await bte.loadPrefs();
    render();

    $("open-app").addEventListener("click", async () => {
      // Si la pestaña actual es Eminus, se abre ahí mismo; si no, se abre
      // Eminus con la vista ya desplegada.
      try {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        await chrome.tabs.sendMessage(tab.id, { type: "bte:showApp", view: "agenda" });
      } catch (_) {
        await chrome.tabs.create({ url: "https://eminus.uv.mx/eminus4/page/course/list#bte-app=agenda" });
      }
      window.close();
    });
    $("enabled").addEventListener("change", (e) => save({ enabled: e.target.checked }));
    $("brackets").addEventListener("change", (e) => save({ brackets: e.target.checked }));
    $("scanlines").addEventListener("change", (e) => save({ scanlines: e.target.checked }));
    $("font").addEventListener("change", (e) => save({ font: e.target.value }));
    $("zoom-in").addEventListener("click", () => save({ fontSize: prefs.fontSize + 1 }));
    $("zoom-out").addEventListener("click", () => save({ fontSize: prefs.fontSize - 1 }));
    $("zoom-reset").addEventListener("click", () => save({ fontSize: 14 }));

    bte.onPrefsChanged((next) => {
      prefs = next;
      render();
    });
  };

  init();
})();
