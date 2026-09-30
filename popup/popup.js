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
  const inTab = location.hash === "#fondo";

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
    renderBg();
  };

  const renderBg = () => {
    bte.bg.sync(prefs);
    const meta = bte.bg.info();
    $("bg-pick").textContent = meta ? "[ cambiar… ]" : "[ elegir… ]";
    $("bg-clear").hidden = !meta;
    $("bg-opts").hidden = !meta;
    $("bg-strength").value = prefs.bgStrength;
    $("bg-strength-value").textContent = prefs.bgStrength + "%";
    $("bg-panel").value = prefs.bgPanel;
    $("bg-panel-value").textContent = prefs.bgPanel + "%";
    $("bg-zoom").value = prefs.bgZoom;
    $("bg-zoom-value").textContent = Math.round(prefs.bgZoom) + "%";
    $("bg-rotate").value = prefs.bgRotate;
    $("bg-rotate-value").textContent = prefs.bgRotate + "°";
    $("bg-flipx").setAttribute("aria-pressed", String(prefs.bgFlipX));
    $("bg-flipy").setAttribute("aria-pressed", String(prefs.bgFlipY));
    $("bg-card").value = prefs.bgCard;
    $("bg-card-value").textContent = prefs.bgCard + "%";
    $("bg-palette").checked = prefs.bgPalette;
    for (const b of $("bg-style").querySelectorAll("button")) {
      b.setAttribute("aria-pressed", String(b.dataset.v === prefs.bgStyle));
    }
    $("bg-note").textContent = !meta
      ? ""
      : !prefs.bgPalette
        ? ""
        : meta.tone
          ? "(colores tomados de la foto)"
          : "(la foto casi no tiene color: se queda el tema)";
  };

  const renderThumb = async () => {
    const data = bte.bg.info() ? await bte.bg.image() : null;
    const thumb = $("bg-thumb");
    thumb.style.backgroundImage = data ? `url("${data}")` : "";
    thumb.textContent = data ? "" : "ninguno";
  };

  const setBgFile = async (file) => {
    if (!file || !file.type.startsWith("image/")) return;
    $("bg-thumb").textContent = "…";
    try {
      await bte.bg.setFile(file);
    } catch (_) {
      $("bg-note").textContent = "(no se pudo leer la imagen)";
    }
    renderBg();
    renderThumb();
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
    await bte.bg.ready.catch(() => {});
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

    // El diálogo de archivos cierra el popup en muchos sistemas: desde el
    // popup se abre esta misma página en una pestaña y ahí se elige.
    if (inTab) {
      root.classList.add("bte-tab");
      $("fondo").scrollIntoView({ block: "center" });
    }
    $("bg-pick").addEventListener("click", () => {
      if (inTab) $("bg-file").click();
      else chrome.tabs.create({ url: chrome.runtime.getURL("popup/popup.html#fondo") }).then(() => window.close());
    });
    $("bg-file").addEventListener("change", (e) => setBgFile(e.target.files[0]));
    $("bg-clear").addEventListener("click", async () => {
      await bte.bg.clear();
      renderBg();
      renderThumb();
    });
    $("bg-style").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-v]");
      if (b) save({ bgStyle: b.dataset.v });
    });
    $("bg-strength").addEventListener("input", (e) => {
      $("bg-strength-value").textContent = e.target.value + "%";
    });
    $("bg-strength").addEventListener("change", (e) => save({ bgStrength: Number(e.target.value) }));
    $("bg-panel").addEventListener("input", (e) => {
      $("bg-panel-value").textContent = e.target.value + "%";
    });
    $("bg-panel").addEventListener("change", (e) => save({ bgPanel: Number(e.target.value) }));
    $("bg-zoom").addEventListener("input", (e) => {
      $("bg-zoom-value").textContent = e.target.value + "%";
    });
    $("bg-zoom").addEventListener("change", (e) => save({ bgZoom: Number(e.target.value) }));
    $("bg-rotate").addEventListener("input", (e) => {
      $("bg-rotate-value").textContent = e.target.value + "°";
    });
    $("bg-rotate").addEventListener("change", (e) => save({ bgRotate: Number(e.target.value) }));
    $("bg-flipx").addEventListener("click", () => save({ bgFlipX: !prefs.bgFlipX }));
    $("bg-flipy").addEventListener("click", () => save({ bgFlipY: !prefs.bgFlipY }));
    $("bg-reset").addEventListener("click", () => save({ ...bte.BG_FRAME_DEFAULTS }));
    $("bg-edit").addEventListener("click", async () => {
      // Se acomoda sobre la página de Eminus abierta; si no hay, se avisa.
      try {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        const res = await chrome.tabs.sendMessage(tab.id, { type: "bte:bgEdit" });
        if (res && res.ok) window.close();
        else $("bg-note").textContent = "(activa el reskin y elige una imagen)";
      } catch (_) {
        $("bg-note").textContent = "(abre eminus en esta pestaña para acomodar)";
      }
    });
    $("bg-card").addEventListener("input", (e) => {
      $("bg-card-value").textContent = e.target.value + "%";
    });
    $("bg-card").addEventListener("change", (e) => save({ bgCard: Number(e.target.value) }));
    $("bg-palette").addEventListener("change", (e) => save({ bgPalette: e.target.checked }));
    if (inTab) {
      document.addEventListener("dragover", (e) => {
        e.preventDefault();
        root.classList.add("drag");
      });
      document.addEventListener("dragleave", () => root.classList.remove("drag"));
      document.addEventListener("drop", (e) => {
        e.preventDefault();
        root.classList.remove("drag");
        setBgFile(e.dataTransfer.files[0]);
      });
    }
    // La imagen se guarda desde otra pestaña: aquí sólo se refresca.
    chrome.storage.onChanged.addListener((changes, area) => {
      if (area === "local" && "bteBgMeta" in changes) {
        renderBg();
        renderThumb();
      }
    });
    renderThumb();

    bte.onPrefsChanged((next) => {
      prefs = next;
      render();
    });
  };

  init();
})();
