// Fondo con foto: la imagen elegida, tramada con puntos (Bayer 8×8) y fundida
// hacia el color de fondo del tema, como en Servicio social. Si el usuario
// quiere, los colores del tema se toman de la foto.
//
// La imagen (ya reducida, en JPEG) vive en chrome.storage.local porque no cabe
// en sync. Aparte se guarda su metadato ({ tone }) para que los iframes y el
// popup no tengan que leer la imagen entera, y un espejo en localStorage para
// aplicar la paleta en document_start sin parpadeo.
(() => {
  const bte = globalThis.bte;
  const root = document.documentElement;

  const IMG_KEY = "bteBg";
  const META_KEY = "bteBgMeta";
  const MIRROR_KEY = "bte:bg";
  const MAX_SIDE = 1920; // lado mayor de la imagen guardada
  const CELL = 2; // tamaño del punto en px CSS

  // Se pinta en el marco superior de Eminus y en la vista tui (agenda, cursos,
  // miyu…); los iframes de Eminus y el popup sólo toman la paleta.
  const isAppView = location.protocol === "chrome-extension:" && location.pathname.endsWith("/app/app.html");
  const isPageTop = () => /^https?:$/.test(location.protocol) && window.top === window;
  const drawHere = isPageTop() || isAppView;

  const BAYER = (() => {
    let m = [[0]];
    for (let n = 1; n < 8; n *= 2) {
      const next = [];
      for (let y = 0; y < n * 2; y++) {
        next[y] = [];
        for (let x = 0; x < n * 2; x++) next[y][x] = 4 * m[y % n][x % n] + [0, 2, 3, 1][(y >= n) * 2 + (x >= n)];
      }
      m = next;
    }
    return m.map((row) => row.map((v) => (v + 0.5) / 64));
  })();

  // ---------- color ----------

  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const hex = (r, g, b) => "#" + [r, g, b].map((v) => Math.round(clamp(v, 0, 255)).toString(16).padStart(2, "0")).join("");
  const rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  function hsl(h, s, l) {
    const f = (n) => {
      const k = (n + h / 30) % 12;
      return l - s * Math.min(l, 1 - l) * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    };
    return hex(f(0) * 255, f(8) * 255, f(4) * 255);
  }
  function inkOn(h) {
    const [r, g, b] = rgb(h).map((c) => c / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.4 ? "#141414" : "#ffffff";
  }
  const lum = (r, g, b) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  const smoothstep = (a, b, x) => {
    const t = clamp((x - a) / (b - a), 0, 1);
    return t * t * (3 - 2 * t);
  };

  // Tono dominante: histograma de 36 tonos pesado por saturación; los píxeles
  // casi negros, blancos o grises no votan.
  function dominantTone(image) {
    const N = 64;
    const c = document.createElement("canvas");
    c.width = c.height = N;
    const ctx = c.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(image, 0, 0, N, N);
    const px = ctx.getImageData(0, 0, N, N).data;
    const bins = Array.from({ length: 36 }, () => ({ w: 0, x: 0, y: 0, s: 0 }));
    let total = 0;
    for (let i = 0; i < px.length; i += 4) {
      const r = px[i] / 255, g = px[i + 1] / 255, b = px[i + 2] / 255;
      const max = Math.max(r, g, b), min = Math.min(r, g, b), l = (max + min) / 2, d = max - min;
      total++;
      if (d < 0.03 || l < 0.06 || l > 0.96) continue;
      const s = d / (1 - Math.abs(2 * l - 1));
      let h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
      h = (h * 60 + 360) % 360;
      const w = s ** 1.5; // los colores vivos pesan más que los apagados
      const bin = bins[Math.floor(h / 10) % 36];
      bin.w += w;
      bin.s += s * w;
      bin.x += Math.cos((h * Math.PI) / 180) * w;
      bin.y += Math.sin((h * Math.PI) / 180) * w;
    }
    // Se suman los vecinos para que un tono repartido entre dos cubetas no pierda.
    let best = null, bestW = 0;
    bins.forEach((_, i) => {
      const w = bins[(i + 35) % 36].w * 0.5 + bins[i].w + bins[(i + 1) % 36].w * 0.5;
      if (w > bestW) {
        bestW = w;
        best = bins[i];
      }
    });
    if (!best || best.w === 0 || bestW / total < 0.002) return null; // casi sin color
    return { h: ((Math.atan2(best.y, best.x) * 180) / Math.PI + 360) % 360, s: best.s / best.w };
  }

  // Paleta completa del tema a partir del tono: neutros teñidos y un acento
  // saturado. Los colores con significado (rojo, verde…) se quedan los del tema.
  function paletteFor(tone, dark) {
    const { h } = tone;
    const sat = clamp(tone.s, 0.42, 0.75);
    const p = dark
      ? {
          bg: hsl(h, 0.16, 0.1),
          bg0: hsl(h, 0.18, 0.075),
          bg1: hsl(h, 0.14, 0.145),
          bg2: hsl(h, 0.12, 0.22),
          border: hsl(h, 0.12, 0.32),
          fg: hsl(h, 0.18, 0.86),
          "fg-strong": hsl(h, 0.25, 0.95),
          dim: hsl(h, 0.1, 0.62),
          accent: hsl(h, sat, 0.66),
          link: hsl(h, sat * 0.7, 0.76),
          scheme: "dark"
        }
      : {
          bg: hsl(h, 0.3, 0.965),
          bg0: hsl(h, 0.35, 0.985),
          bg1: hsl(h, 0.22, 0.925),
          bg2: hsl(h, 0.16, 0.86),
          border: hsl(h, 0.14, 0.76),
          fg: hsl(h, 0.22, 0.17),
          "fg-strong": hsl(h, 0.3, 0.09),
          dim: hsl(h, 0.1, 0.38),
          accent: hsl(h, sat, 0.38),
          link: hsl(h, sat * 0.85, 0.32),
          scheme: "light"
        };
    p.surface = dark ? p.bg1 : p.bg0;
    p["accent-fg"] = inkOn(p.accent);
    p.glow = "none";
    return p;
  }
  const PALETTE_VARS = Object.keys(paletteFor({ h: 0, s: 0.5 }, true));

  // ---------- estado ----------

  let meta = null; // { tone } si hay imagen guardada
  let prefs = bte.prefs || bte.readMirror();
  let img = null;
  let imgLoading = false;
  let host = null, canvas = null, live = null, frame = null;

  try {
    meta = JSON.parse(localStorage.getItem(MIRROR_KEY) || "null");
  } catch (_) {
  }

  const writeMirror = () => {
    try {
      if (meta) localStorage.setItem(MIRROR_KEY, JSON.stringify(meta));
      else localStorage.removeItem(MIRROR_KEY);
    } catch (_) {
    }
  };

  const active = () => prefs.enabled && !!meta;

  // Encuadre en uso: el guardado, o el que se está acomodando sin guardar.
  let framePreview = null;
  const frame_ = () => ({ ...bte.BG_FRAME_DEFAULTS, ...prefs, ...(framePreview || {}) });

  // El modo claro/oscuro lo decide el tema, o el estilo si trae sus colores.
  const isDark = () => {
    const style = bte.styleById(prefs.style);
    return style.ownColors ? false : bte.themeById(prefs.theme).dark;
  };

  function applyPalette() {
    const on = active() && prefs.bgPalette && meta.tone;
    if (!on) {
      for (const v of PALETTE_VARS) root.style.removeProperty("--bte-" + v);
      return;
    }
    const p = paletteFor(meta.tone, isDark());
    for (const [k, v] of Object.entries(p)) root.style.setProperty("--bte-" + k, v);
  }

  // ---------- dibujo ----------

  function bgColor() {
    const probe = document.createElement("canvas").getContext("2d");
    probe.fillStyle = getComputedStyle(root).getPropertyValue("--bte-bg").trim() || "#000";
    const m = /^#([0-9a-f]{6})$/i.exec(probe.fillStyle);
    return m ? rgb(probe.fillStyle) : [0, 0, 0];
  }

  // Encuadre en px CSS de la ventana: centro, ángulo y escala de la imagen.
  // Lo usan igual el tramado y la capa en vivo, así que al soltar no salta.
  function geometry(f) {
    const W = window.innerWidth, H = window.innerHeight;
    const area = H * 0.95; // con el encuadre por defecto cubre casi toda la ventana
    const rad = (f.bgRotate * Math.PI) / 180;
    const cos = Math.abs(Math.cos(rad)), sin = Math.abs(Math.sin(rad));
    // La escala base cubre el área aun girada; luego va el zoom.
    const cover = Math.max((W * cos + area * sin) / img.width, (W * sin + area * cos) / img.height);
    return {
      cx: W / 2 + (f.bgX / 100) * (W / 2),
      cy: area / 2 + (f.bgY / 100) * (area / 2),
      rad,
      scale: cover * (f.bgZoom / 100)
    };
  }

  function draw() {
    frame = null;
    if (!canvas || !img || !active()) return;
    const width = window.innerWidth, height = window.innerHeight;
    if (!width || !height) return;
    const w = Math.ceil(width / CELL), h = Math.ceil(height / CELL);
    canvas.width = w;
    canvas.height = h;
    // Tamaño exacto en px CSS (sin estirar): cada celda mide CELL px.
    canvas.style.width = w * CELL + "px";
    canvas.style.height = h * CELL + "px";
    const ctx = canvas.getContext("2d", { willReadFrequently: true });

    const [br, bgG, bb] = bgColor();
    // Lo que la imagen no cubra (al moverla, girarla o alejarla) queda del
    // color de fondo.
    ctx.fillStyle = `rgb(${br}, ${bgG}, ${bb})`;
    ctx.fillRect(0, 0, w, h);

    const f = frame_();
    const g = geometry(f);
    ctx.save();
    ctx.scale(1 / CELL, 1 / CELL);
    ctx.translate(g.cx, g.cy);
    ctx.rotate(g.rad);
    ctx.scale(f.bgFlipX ? -1 : 1, f.bgFlipY ? -1 : 1);
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, (-img.width * g.scale) / 2, (-img.height * g.scale) / 2, img.width * g.scale, img.height * g.scale);
    ctx.restore();

    const data = ctx.getImageData(0, 0, w, h);
    const px = data.data;
    const bgL = lum(br, bgG, bb);
    const dither = prefs.bgStyle !== "plain";

    for (let y = 0; y < h; y++) {
      // Se ve entera arriba y se va tramando hacia abajo sin desaparecer.
      const v = 1 - 0.55 * smoothstep(0.2, 1, y / h);
      const row = BAYER[y & 7];
      for (let x = 0; x < w; x++) {
        const i = (y * w + x) * 4;
        const r = px[i], g = px[i + 1], b = px[i + 2];
        let k = v;
        if (dither) {
          // Más puntos donde la imagen se aleja del fondo; donde se parece queda una sombra tenue.
          const contrast = Math.min(1, Math.abs(lum(r, g, b) - bgL) * 1.6 + 0.1);
          k = contrast * v * 1.1 > row[x & 7] ? 0.35 + 0.65 * v : 0.25 * v * v * v;
        }
        px[i] = br + (r - br) * k;
        px[i + 1] = bgG + (g - bgG) * k;
        px[i + 2] = bb + (b - bb) * k;
        px[i + 3] = 255;
      }
    }
    ctx.putImageData(data, 0, 0);
  }

  const schedule = () => {
    if (drawHere) frame ??= requestAnimationFrame(draw);
  };

  // ---------- vista en vivo al acomodar ----------
  //
  // Tramar la ventana entera en cada movimiento hace que los puntos se
  // reacomoden (la imagen «tiembla»), avanza a saltos de CELL px y cuesta.
  // Mientras se mueve se muestra la foto como capa con transform CSS (fluida,
  // por GPU) y, en cuanto se deja de mover un momento, se trama una sola vez.

  const SETTLE_MS = 160;
  let liveFrame = null, settleTimer = null;

  function placeLive() {
    liveFrame = null;
    if (!live || !img) return;
    const f = frame_();
    const g = geometry(f);
    live.style.width = img.width + "px";
    live.style.height = img.height + "px";
    live.style.transform =
      `translate(${g.cx}px, ${g.cy}px) rotate(${g.rad}rad) ` +
      `scale(${g.scale * (f.bgFlipX ? -1 : 1)}, ${g.scale * (f.bgFlipY ? -1 : 1)}) translate(-50%, -50%)`;
  }

  function moveLive() {
    if (!drawHere || !host || !img) return schedule();
    if (live.getAttribute("src") !== img.src) live.src = img.src;
    liveFrame ??= requestAnimationFrame(placeLive);
    host.classList.add("bte-bg-live");
    clearTimeout(settleTimer);
    settleTimer = setTimeout(settle, SETTLE_MS);
  }

  function settle() {
    settleTimer = null;
    if (frame) {
      cancelAnimationFrame(frame);
      frame = null;
    }
    draw();
    host.classList.remove("bte-bg-live");
  }

  function ensureCanvas() {
    if (host) return;
    host = document.createElement("div");
    host.id = "bte-bg";
    host.setAttribute("aria-hidden", "true");
    canvas = document.createElement("canvas");
    live = document.createElement("img");
    live.className = "bte-bg-live-img";
    live.alt = "";
    live.decoding = "sync";
    host.append(canvas, live);
    // Hijo de <html>, fuera de <body>: Angular no lo toca.
    root.appendChild(host);
    window.addEventListener("resize", schedule);
  }

  async function loadImage() {
    if (img || imgLoading || !active()) return;
    imgLoading = true;
    try {
      const data = (await chrome.storage.local.get(IMG_KEY))[IMG_KEY];
      if (data) {
        const next = new Image();
        next.src = data;
        await next.decode();
        img = next;
      }
    } catch (_) {
    }
    imgLoading = false;
    schedule();
  }

  function render() {
    applyPalette();
    if (!drawHere) return;
    const on = active();
    root.classList.toggle("bte-has-bg", on);
    if (!on) return;
    ensureCanvas();
    root.style.setProperty("--bte-bg-strength", String(prefs.bgStrength / 100));
    root.style.setProperty("--bte-panel-alpha", prefs.bgPanel + "%");
    root.style.setProperty("--bte-card-alpha", prefs.bgCard + "%");
    loadImage();
    schedule();
  }

  // ---------- acomodar la imagen ----------

  const FRAME_KEYS = Object.keys(bte.BG_FRAME_DEFAULTS);
  const pickFrame = (p) => Object.fromEntries(FRAME_KEYS.map((k) => [k, p[k]]));
  let editor = null;

  const savePrefs = async (patch) => {
    const next = await bte.savePrefs(patch);
    if (bte.applyPrefs) bte.applyPrefs(next);
    else bte.bg.sync(next);
    return next;
  };

  // Capa sobre toda la página: arrastrar mueve, la rueda acerca (con shift
  // gira) y la barra de abajo tiene el resto. Nada se guarda hasta «guardar».
  function openEditor() {
    if (!drawHere || !isPageTop() || editor || !active()) return false;
    let f = pickFrame(frame_());
    const start = { ...f };
    const round = (v) => Math.round(v * 10) / 10;
    const set = (patch) => {
      f = bte.sanitize({ ...prefs, ...f, ...patch });
      f = pickFrame(f);
      framePreview = f;
      label.textContent = `x ${Math.round(f.bgX)} · y ${Math.round(f.bgY)} · zoom ${Math.round(f.bgZoom)}% · giro ${f.bgRotate}°`;
      moveLive();
    };

    const layer = document.createElement("div");
    layer.id = "bte-bg-edit";
    layer.innerHTML = `
      <div class="bte-bg-edit-bar" role="toolbar" aria-label="Acomodar fondo">
        <span class="bte-bg-edit-title">acomodar fondo</span>
        <span class="bte-bg-edit-label"></span>
        <span class="bte-bg-edit-btns">
          <button type="button" data-a="zoom-" title="Alejar (-)">−</button>
          <button type="button" data-a="zoom+" title="Acercar (+)">+</button>
          <button type="button" data-a="rot-" title="Girar a la izquierda ([ · shift 15°)">⟲</button>
          <button type="button" data-a="rot+" title="Girar a la derecha (] · shift 15°)">⟳</button>
          <button type="button" data-a="rot90" title="Girar 90° (r)">90°</button>
          <button type="button" data-a="flipx" title="Voltear horizontal (h)">⇋</button>
          <button type="button" data-a="flipy" title="Voltear vertical (v)">⇵</button>
          <button type="button" data-a="reset" title="Restablecer (0)">restablecer</button>
          <button type="button" data-a="cancel" title="Cancelar (esc)">cancelar</button>
          <button type="button" data-a="save" class="bte-bg-edit-save" title="Guardar (enter)">guardar</button>
        </span>
        <span class="bte-bg-edit-hint">arrastra para mover · rueda: zoom · shift + rueda: girar · flechas: mover</span>
      </div>`;
    const label = layer.querySelector(".bte-bg-edit-label");
    root.appendChild(layer);
    root.classList.add("bte-bg-editing");
    set({});

    const close = async (save) => {
      if (!editor) return;
      editor = null;
      window.removeEventListener("keydown", onKey, true);
      layer.remove();
      root.classList.remove("bte-bg-editing");
      // Primero el guardado (prefs = f) y luego se quita la vista previa: así
      // el tramado final sale ya en la posición nueva, sin brincar.
      if (save) await savePrefs({ ...f, bgX: round(f.bgX), bgY: round(f.bgY), bgZoom: round(f.bgZoom) });
      framePreview = null;
      clearTimeout(settleTimer);
      settle();
      if (bte.toast) bte.toast(save ? "fondo: encuadre guardado" : "fondo: sin cambios");
    };

    const actions = {
      "zoom-": () => set({ bgZoom: f.bgZoom / 1.1 }),
      "zoom+": () => set({ bgZoom: f.bgZoom * 1.1 }),
      "rot-": (big) => set({ bgRotate: f.bgRotate - (big ? 15 : 1) }),
      "rot+": (big) => set({ bgRotate: f.bgRotate + (big ? 15 : 1) }),
      rot90: () => set({ bgRotate: f.bgRotate + 90 }),
      flipx: () => set({ bgFlipX: !f.bgFlipX }),
      flipy: () => set({ bgFlipY: !f.bgFlipY }),
      reset: () => set({ ...bte.BG_FRAME_DEFAULTS }),
      cancel: () => {
        f = start;
        close(false);
      },
      save: () => close(true)
    };

    layer.querySelector(".bte-bg-edit-btns").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-a]");
      if (b) actions[b.dataset.a](e.shiftKey);
    });

    // Arrastrar: el desplazamiento se mide en % de media ventana, igual que
    // se guarda, así que se ve igual con cualquier tamaño de ventana.
    let drag = null;
    layer.addEventListener("pointerdown", (e) => {
      if (e.target.closest(".bte-bg-edit-bar")) return;
      drag = { x: e.clientX, y: e.clientY, bgX: f.bgX, bgY: f.bgY };
      layer.setPointerCapture(e.pointerId);
      layer.classList.add("dragging");
    });
    layer.addEventListener("pointermove", (e) => {
      if (!drag) return;
      set({
        bgX: drag.bgX + ((e.clientX - drag.x) / (window.innerWidth / 2)) * 100,
        bgY: drag.bgY + ((e.clientY - drag.y) / ((window.innerHeight * 0.95) / 2)) * 100
      });
    });
    const endDrag = () => {
      drag = null;
      layer.classList.remove("dragging");
    };
    layer.addEventListener("pointerup", endDrag);
    layer.addEventListener("pointercancel", endDrag);
    layer.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        if (e.shiftKey) set({ bgRotate: f.bgRotate + Math.sign(e.deltaY || e.deltaX) * 2 });
        else set({ bgZoom: f.bgZoom * Math.exp(-e.deltaY * 0.0015) });
      },
      { passive: false }
    );

    // Las teclas no llegan a Eminus ni a los atajos mientras se acomoda.
    const onKey = (e) => {
      e.stopImmediatePropagation();
      const step = e.shiftKey ? 10 : 2;
      const map = {
        ArrowLeft: () => set({ bgX: f.bgX - step }),
        ArrowRight: () => set({ bgX: f.bgX + step }),
        ArrowUp: () => set({ bgY: f.bgY - step }),
        ArrowDown: () => set({ bgY: f.bgY + step }),
        "+": actions["zoom+"],
        "=": actions["zoom+"],
        "-": actions["zoom-"],
        "[": () => actions["rot-"](e.shiftKey),
        "{": () => actions["rot-"](true),
        "]": () => actions["rot+"](e.shiftKey),
        "}": () => actions["rot+"](true),
        r: actions.rot90,
        h: actions.flipx,
        v: actions.flipy,
        0: actions.reset,
        Enter: actions.save,
        Escape: actions.cancel
      };
      const fn = map[e.key];
      if (fn && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        fn();
      }
    };
    window.addEventListener("keydown", onKey, true);
    editor = { close };
    return true;
  }

  // ---------- API ----------

  bte.bg = {
    // El popup no corre boot.js: pasa sus preferencias aquí.
    sync(next) {
      prefs = next;
      render();
    },

    // Se resuelve cuando ya se leyó de storage si hay imagen.
    ready: Promise.resolve(),

    info: () => meta,

    // Encuadre sin guardar (vista previa de los deslizadores); null lo quita.
    preview(patch) {
      framePreview = patch ? { ...(framePreview || {}), ...patch } : null;
      moveLive();
    },

    // Modo para acomodar la imagen con el ratón (sólo marco superior de Eminus).
    edit: () => openEditor(),

    async image() {
      try {
        return (await chrome.storage.local.get(IMG_KEY))[IMG_KEY] || null;
      } catch (_) {
        return null;
      }
    },

    // Reduce la imagen, calcula su tono y la guarda.
    async setFile(file) {
      const bitmap = await createImageBitmap(file);
      const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
      const c = document.createElement("canvas");
      c.width = Math.round(bitmap.width * scale);
      c.height = Math.round(bitmap.height * scale);
      const ctx = c.getContext("2d");
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(bitmap, 0, 0, c.width, c.height);
      bitmap.close();
      const next = { tone: dominantTone(c), at: Date.now() };
      // Imagen nueva, encuadre nuevo.
      await savePrefs({ ...bte.BG_FRAME_DEFAULTS }).catch(() => {});
      await chrome.storage.local.set({ [IMG_KEY]: c.toDataURL("image/jpeg", 0.86), [META_KEY]: next });
      meta = next;
      img = null;
      writeMirror();
      render();
      return meta;
    },

    async clear() {
      await chrome.storage.local.remove([IMG_KEY, META_KEY]);
      meta = null;
      img = null;
      writeMirror();
      render();
    }
  };

  // ---------- arranque ----------

  render();
  // Cambio de tema, estilo o efecto: nueva paleta y se vuelve a fundir hacia
  // el nuevo color de fondo.
  document.addEventListener("bte:prefs", () => {
    if (bte.prefs) bte.bg.sync(bte.prefs);
  });

  try {
    bte.bg.ready = chrome.storage.local.get(META_KEY).then((data) => {
      meta = data[META_KEY] || null;
      writeMirror();
      render();
    });
    chrome.storage.onChanged.addListener((changes, area) => {
      if (area !== "local" || !(META_KEY in changes || IMG_KEY in changes)) return;
      if (META_KEY in changes) meta = changes[META_KEY].newValue || null;
      img = null;
      writeMirror();
      render();
    });
  } catch (_) {
  }
})();
