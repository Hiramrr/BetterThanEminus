// App de iOS: ajustes de las preferencias antes de aplicar el tema.
(() => {
  const bte = globalThis.bte;
  const native = globalThis.__bteNative;
  if (!bte || !bte.STYLES) return;
  // En el teléfono sólo queda el estilo tui, que es el que mejor funciona;
  // cualquier otro guardado (p. ej. desde la extensión) cae en él.
  bte.STYLES = bte.STYLES.filter((style) => style.id === "tui");
  // La app trae ABC Areal empaquetada.
  bte.DEFAULTS.font = "areal-mono";

  // La fuente elegida se pide a la app (bytes en base64) y se registra en
  // este documento con FontFace; sólo la familia en uso, una vez.
  const FAMILIES = {
    "areal-mono": ["ABC Areal Mono", "ABCArealMono"],
    "areal-semi": ["ABC Areal Semi Mono", "ABCArealSemiMono"],
    areal: ["ABC Areal", "ABCAreal"]
  };
  const WEIGHTS = [[400, "Regular"], [500, "Medium"], [700, "Bold"]];
  const loaded = new Set();

  const decode = (b64) => {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
    return bytes.buffer;
  };

  const loadFamily = async (id) => {
    const entry = FAMILIES[id];
    if (!entry || loaded.has(id) || !native || !native.post) return;
    loaded.add(id);
    const [family, file] = entry;
    await Promise.all(WEIGHTS.map(async ([weight, name]) => {
      try {
        const b64 = await native.post({ type: "font", file: file + "-" + name + ".ttf" });
        if (!b64) return;
        const face = new FontFace(family, decode(b64), { weight: String(weight), display: "swap" });
        document.fonts.add(await face.load());
      } catch (err) {
        console.error("[bte] fuente", family, weight, err);
      }
    }));
  };

  document.addEventListener("bte:prefs", () => {
    if (bte.prefs) loadFamily(bte.prefs.font);
  });
})();
