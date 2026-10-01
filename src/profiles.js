// Perfiles: entrar a Eminus sin escribir usuario y contraseña, volver a
// entrar cuando la sesión vence y cambiar de cuenta.
(() => {
  const bte = globalThis.bte;
  if (!bte.isTop || bte.profilesStarted) return;
  bte.profilesStarted = true;

  const root = document.documentElement;
  const esc = (text) =>
    String(text).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  // Usuario y contraseña viven en el service worker (background.js), cifrados.
  // Aquí sólo se manejan nombres: para entrar se le pide que llene el
  // formulario; este script nunca lee una contraseña guardada.
  const post = async (type, extra) => {
    try {
      const res = await chrome.runtime.sendMessage({ type: "bte:profile." + type, ...extra });
      return res ? res.value : null;
    } catch (_) {
      return null; // la extensión se actualizó: hace falta recargar
    }
  };

  // Eminus vacía localStorage al cerrar o vencer la sesión, así que lo que
  // debe sobrevivir a eso se guarda en chrome.storage.local.
  const STATE_KEY = "bteProfileState"; // { session: { user, exp }, switchTo, nosave: [] }
  const AVATAR_KEY = "bteProfileAvatars"; // { [user]: miniatura }
  const PROFILES_KEY = "bteProfiles";
  const TRY_KEY = "bte:profileTry"; // intento en curso, por si la página recarga
  let state = { session: null, switchTo: "", nosave: [] };
  let avatars = {};

  const loadState = async () => {
    try {
      const data = await chrome.storage.local.get([STATE_KEY, AVATAR_KEY]);
      if (data[STATE_KEY] && typeof data[STATE_KEY] === "object") state = { ...state, ...data[STATE_KEY] };
      if (!Array.isArray(state.nosave)) state.nosave = [];
      if (data[AVATAR_KEY] && typeof data[AVATAR_KEY] === "object") avatars = data[AVATAR_KEY];
    } catch (_) {
    }
  };
  const saveState = (patch) => {
    state = { ...state, ...patch };
    try {
      return chrome.storage.local.set({ [STATE_KEY]: state }).catch(() => {});
    } catch (_) {
      return Promise.resolve();
    }
  };
  const saveAvatars = () => {
    try {
      chrome.storage.local.set({ [AVATAR_KEY]: avatars }).catch(() => {});
    } catch (_) {
    }
  };

  const onLogin = () => !!document.querySelector("m-auth");
  const tokenExpiry = () => {
    try {
      const token = localStorage.getItem("accessToken") || "";
      return JSON.parse(atob(token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/"))).exp || 0;
    } catch (_) {
      return 0;
    }
  };

  let profiles = [];
  let profilesEl = null;
  let backEl = null;
  let loginState = null; // "", "login" o "in": para actuar sólo al cambiar
  let lastAttempt = 0;
  let tryingUser = "";

  const setTrying = (user) => {
    tryingUser = user;
    try {
      if (user) sessionStorage.setItem(TRY_KEY, JSON.stringify({ user, ts: lastAttempt }));
      else sessionStorage.removeItem(TRY_KEY);
    } catch (_) {
    }
  };

  const loadProfiles = async () => {
    profiles = (await post("list")) || [];
    return profiles;
  };

  const loginWith = async (user, why) => {
    const profile = profiles.find((p) => p.user === user);
    if (!profile) return;
    lastAttempt = Date.now();
    setTrying(user);
    post("discard"); // lo escrito a mano ya no cuenta
    bte.toast((why ? why + ": " : "") + "entrando como " + profile.name.toLowerCase() + "…", 4000);
    renderProfiles();
    const result = await post("login", { user });
    if (result !== "ok") {
      setTrying("");
      bte.toast(result ? "no se pudo llenar el formulario (" + result + ")" : "recarga la página: la extensión se actualizó");
      renderProfiles();
      return;
    }
    // Si a los 10 s sigue el login, la contraseña ya no es válida.
    setTimeout(() => {
      if (tryingUser !== user) return;
      setTrying("");
      if (onLogin()) bte.toast("no se pudo entrar como " + profile.name.toLowerCase() + ": revisa la contraseña e inicia sesión a mano", 6000);
      renderProfiles();
    }, 10000);
  };

  // Fotos de perfil: una miniatura del avatar de Eminus por usuario, para
  // reconocer cada perfil en el selector (no es un dato secreto).
  const captureAvatar = async (user) => {
    if (!user) return;
    const img = await bte.waitFor(() => {
      const el = document.querySelector("m-header m-user-profile img");
      return el && el.complete && el.naturalWidth ? el : null;
    }, 8000, 400);
    if (!img) return;
    let data = "";
    try {
      const size = 112;
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = size;
      const side = Math.min(img.naturalWidth, img.naturalHeight);
      canvas.getContext("2d").drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, size, size);
      data = canvas.toDataURL("image/jpeg", 0.82);
    } catch (_) {
      data = /^https:/.test(img.src) ? img.src : ""; // imagen de otro origen: se guarda su dirección
    }
    if (!data || avatars[user] === data) return;
    avatars[user] = data;
    saveAvatars();
  };

  // Con perfiles guardados, la entrada es un selector (foto, nombre y
  // usuario) en lugar del formulario; «iniciar con otro perfil» lo muestra.
  let pickMode = true;
  const profileButtons = () => [...profilesEl.querySelectorAll("button")];
  const renderProfiles = () => {
    const available = onLogin() && profiles.length > 0;
    const show = available && pickMode;
    root.classList.toggle("bte-pick", show);
    if (!profilesEl) {
      if (!available) return;
      profilesEl = document.createElement("div");
      profilesEl.id = "bte-profiles";
      profilesEl.hidden = true;
      profilesEl.addEventListener("click", (e) => {
        if (tryingUser) return;
        const btn = e.target.closest("button");
        if (!btn) return;
        if (btn.dataset.user) loginWith(btn.dataset.user);
        else if (btn.dataset.a === "new") {
          pickMode = false;
          renderProfiles();
          const field = document.querySelector('m-auth input[name="username"], m-auth input[type="text"]');
          if (field) field.focus();
        }
      });
      // ↑ ↓ recorren los perfiles y un número entra directo.
      profilesEl.addEventListener("keydown", (e) => {
        if (e.ctrlKey || e.metaKey || e.altKey) return;
        const buttons = profileButtons();
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          const index = buttons.indexOf(document.activeElement);
          const next = buttons[(index + (e.key === "ArrowDown" ? 1 : buttons.length - 1) + buttons.length) % buttons.length];
          if (next) next.focus();
        } else if (/^[1-9]$/.test(e.key) && !tryingUser) {
          const btn = buttons[Number(e.key) - 1];
          if (btn && btn.dataset.user) {
            e.preventDefault();
            loginWith(btn.dataset.user);
          }
        }
      });
      document.body.appendChild(profilesEl);
      backEl = document.createElement("button");
      backEl.type = "button";
      backEl.id = "bte-profiles-back";
      backEl.textContent = "‹ perfiles guardados";
      backEl.hidden = true;
      backEl.addEventListener("click", () => {
        pickMode = true;
        renderProfiles();
      });
      document.body.appendChild(backEl);
    }
    backEl.hidden = !(available && !pickMode);
    if (!show) {
      profilesEl.hidden = true;
      return;
    }
    const html =
      '<div class="bte-profiles-brand">better than eminus</div>' +
      '<div class="bte-profiles-title">' + (profiles.length === 1 ? "tu perfil" : "elige un perfil") + "</div>" +
      profiles.map((p, index) => {
        const photo = avatars[p.user] || "";
        const busy = tryingUser === p.user;
        const initial = esc((p.name || p.user).trim().charAt(0).toUpperCase());
        return `<button type="button" class="bte-profile" data-user="${esc(p.user)}"${busy ? ' aria-busy="true"' : ""}>` +
          `<span class="bte-profile-avatar">${photo ? `<img src="${esc(photo)}" alt="" />` : initial}</span>` +
          `<span class="bte-profile-who"><b>${esc(p.name.toLowerCase())}</b><i>${busy ? "entrando…" : esc(p.user)}</i></span>` +
          `<span class="bte-profile-go">${index < 9 ? "<kbd>" + (index + 1) + "</kbd> " : ""}❯</span></button>`;
      }).join("") +
      '<button type="button" class="bte-profile-new" data-a="new">+ iniciar con otro perfil</button>';
    const wasHidden = profilesEl.hidden;
    if (profilesEl.innerHTML !== html) {
      const focused = profileButtons().indexOf(document.activeElement);
      profilesEl.innerHTML = html;
      if (focused !== -1 && profileButtons()[focused]) profileButtons()[focused].focus();
    }
    profilesEl.hidden = false;
    if (wasHidden) {
      // El formulario oculto no debe quedarse con el foco.
      const first = profilesEl.querySelector("button");
      if (first) first.focus({ preventScroll: true });
    }
  };

  // Lo que se escribe al entrar a mano se queda en la memoria del service
  // worker (no aquí ni en disco) hasta saber si entró y si se quiere guardar.
  // Se toma de los campos al escribir, al pulsar el botón y con enter: Eminus
  // no siempre dispara el evento submit del formulario.
  let stashTimer = null;
  const stashFrom = (auth) => {
    clearTimeout(stashTimer);
    if (!auth || tryingUser) return;
    const user = auth.querySelector('input[name="username"], input[type="text"]');
    const pass = auth.querySelector('input[name="password"], input[type="password"]');
    if (user && pass && user.value.trim() && pass.value) post("stash", { user: user.value.trim(), pass: pass.value });
  };
  const authOf = (e) => (e.target instanceof Element ? e.target.closest("m-auth") : null);
  for (const type of ["input", "change"]) {
    document.addEventListener(type, (e) => {
      const auth = authOf(e);
      if (!auth) return;
      clearTimeout(stashTimer);
      stashTimer = setTimeout(() => stashFrom(auth), 250);
    }, true);
  }
  document.addEventListener("submit", (e) => stashFrom(authOf(e)), true);
  document.addEventListener("click", (e) => e.target instanceof Element && e.target.closest("button") && stashFrom(authOf(e)), true);
  document.addEventListener("keydown", (e) => e.key === "Enter" && stashFrom(authOf(e)), true);

  const displayName = () => {
    const el = document.querySelector("m-header m-user-profile .h5");
    return el ? el.textContent.trim().replace(/\s+/g, " ") : "";
  };

  const offerSave = async () => {
    const info = await post("stashInfo");
    if (!info) return;
    saveState({ session: { user: info.user, exp: tokenExpiry() } });
    if (info.saved) {
      // Ya estaba: si la contraseña cambió, se actualiza sin preguntar.
      if (info.changed) await post("commit", { name: "" });
      else post("discard");
      return loadProfiles();
    }
    if (state.nosave.includes(info.user)) return post("discard");
    // El nombre tarda un momento en aparecer en el encabezado.
    const name = (await bte.waitFor(displayName, 6000, 300)) || info.user;
    const card = document.createElement("div");
    card.id = "bte-profile-save";
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-label", "guardar perfil");
    card.innerHTML = `<div><b>¿guardar este perfil?</b><span>${esc(name.toLowerCase())} · entrarás sin escribir la contraseña. se guarda cifrada, sólo en este navegador.</span></div>
      <div class="bte-profile-save-btns"><button type="button" data-a="no">ahora no</button><button type="button" data-a="yes">guardar</button></div>`;
    card.addEventListener("click", async (e) => {
      const btn = e.target.closest("button[data-a]");
      if (!btn) return;
      card.remove();
      if (btn.dataset.a === "yes") {
        const ok = await post("commit", { name });
        await loadProfiles();
        if (ok) captureAvatar(info.user);
        bte.toast(ok ? "perfil guardado · : → perfiles para cambiar o quitar" : "no se pudo guardar el perfil", 4500);
      } else {
        post("discard");
        saveState({ nosave: [...new Set([...state.nosave, info.user])] });
      }
    });
    document.body.appendChild(card);
  };

  // Salir sin pasar por el menú de Eminus. Lo «bte:» de localStorage
  // (preferencias, borradores) se conserva.
  const logoutLocal = async (switchTo) => {
    await saveState({ switchTo: switchTo || "", session: null });
    try {
      const kept = [];
      for (let i = 0; i < localStorage.length; i += 1) {
        const key = localStorage.key(i);
        if (key && key.startsWith("bte:")) kept.push([key, localStorage.getItem(key)]);
      }
      localStorage.clear();
      for (const [key, value] of kept) localStorage.setItem(key, value);
      sessionStorage.clear();
    } catch (_) {
    }
    location.assign(location.origin + "/eminus4/");
  };

  const syncProfiles = async () => {
    const now = onLogin() ? "login" : localStorage.getItem("accessToken") ? "in" : "";
    if (now === loginState) return;
    loginState = now;
    const stale = document.getElementById("bte-profile-save");
    if (stale && now !== "in") stale.remove();
    if (now === "login") {
      pickMode = true;
      await loadProfiles();
      renderProfiles();
      const session = state.session;
      const switchTo = state.switchTo;
      if (switchTo) saveState({ switchTo: "" });
      const recent = Date.now() - lastAttempt < 60000; // sin bucles si la contraseña falla
      const expired = !!session && !!session.exp && session.exp * 1000 < Date.now();
      // Si no se sabe de quién era la sesión y sólo hay un perfil, es ése.
      const owner = session ? session.user || (profiles.length === 1 ? profiles[0].user : "") : "";
      if (tryingUser) {
        // La página recargó a medio intento: no se repite.
      } else if (switchTo && profiles.some((p) => p.user === switchTo)) {
        loginWith(switchTo);
      } else if (expired && !recent && profiles.some((p) => p.user === owner)) {
        // La sesión venció sola (no se cerró a mano): se vuelve a entrar.
        loginWith(owner, "sesión vencida");
      } else if (session && !expired) {
        saveState({ session: null }); // cerró sesión a mano: elige perfil
      }
    } else if (now === "in") {
      renderProfiles();
      if (tryingUser) {
        saveState({ session: { user: tryingUser, exp: tokenExpiry() } });
        setTrying("");
      } else {
        await offerSave();
      }
      // Se anota cuándo vence para distinguir después «venció» de «cerró».
      saveState({ session: { user: (state.session && state.session.user) || "", exp: tokenExpiry() } });
      if (!profiles.length) await loadProfiles();
      // La foto del perfil en uso se mantiene al día.
      const who = state.session.user;
      if (who && profiles.some((p) => p.user === who)) captureAvatar(who);
    } else {
      renderProfiles();
    }
  };

  // Comandos para la paleta (grupo «perfiles»).
  bte.profileItems = () => {
    const current = state.session ? state.session.user : "";
    const items = profiles.filter((p) => p.user !== current).map((p) => ({
      group: "perfiles",
      label: "cambiar a " + p.name.toLowerCase(),
      meta: p.user,
      keywords: "perfil cuenta usuario sesion cambiar entrar",
      run: () => logoutLocal(p.user)
    }));
    if (!onLogin()) {
      items.push({ group: "perfiles", label: "cerrar sesión", meta: profiles.length ? "elegir otro perfil" : "", keywords: "salir logout perfil cuenta", run: () => logoutLocal() });
    }
    for (const p of profiles) {
      items.push({
        group: "perfiles",
        label: "quitar perfil: " + p.name.toLowerCase(),
        meta: p.user === current ? "● en uso" : p.user,
        keywords: "perfil borrar eliminar olvidar contraseña",
        run: async () => {
          if (!confirm("¿Quitar el perfil «" + p.name + "» de este navegador? Se borra su contraseña guardada.")) return;
          await post("remove", { user: p.user });
          delete avatars[p.user];
          saveAvatars();
          await loadProfiles();
          renderProfiles();
          bte.toast("perfil quitado");
        }
      });
    }
    if (!profiles.length) {
      items.push({ group: "perfiles", label: "perfiles: cómo se guardan", keywords: "perfil cuenta contraseña", run: () => bte.toast("inicia sesión a mano y la extensión te ofrece guardar el perfil (cifrado, sólo en este navegador)", 6000) });
    }
    return items;
  };

  // ───────────── ciclo de vida ─────────────

  // Otra pestaña guardó o quitó un perfil, o cambió de cuenta.
  try {
    chrome.storage.onChanged.addListener((changes, area) => {
      if (area !== "local") return;
      if (changes[STATE_KEY] && changes[STATE_KEY].newValue) state = { ...state, ...changes[STATE_KEY].newValue };
      if (changes[AVATAR_KEY]) avatars = changes[AVATAR_KEY].newValue || {};
      if (changes[PROFILES_KEY]) loadProfiles().then(renderProfiles);
    });
  } catch (_) {
  }

  try {
    const attempt = JSON.parse(sessionStorage.getItem(TRY_KEY) || "null");
    if (attempt && Date.now() - attempt.ts < 60000) {
      tryingUser = attempt.user;
      lastAttempt = attempt.ts;
      setTimeout(() => tryingUser === attempt.user && onLogin() && (setTrying(""), renderProfiles()), 10000);
    }
  } catch (_) {
  }

  loadState().then(() => {
    syncProfiles();
    setInterval(syncProfiles, 1500);
    new MutationObserver(() => {
      if (onLogin() !== (loginState === "login")) syncProfiles();
    }).observe(document.body, { childList: true, subtree: true });
  });
})();
