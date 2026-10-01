// Service worker: relevo del token de Eminus hacia la vista tui y almacén de
// perfiles (usuario y contraseña) para entrar sin escribirlos.
//
// El token sólo se guarda en chrome.storage.session: vive en memoria, no se
// sincroniza ni se escribe a disco y desaparece al cerrar el navegador. Los
// content scripts no pueden escribir ahí, por eso pasa por aquí.

const EMINUS_ORIGIN = "https://eminus.uv.mx";

// ───────────── perfiles ─────────────

// Hace aquí lo que el Llavero en la app de iOS. Las contraseñas se guardan en
// chrome.storage.local (sólo en este navegador, no se sincronizan) cifradas
// con AES-GCM; la llave no es extraíble y vive en el IndexedDB de la
// extensión, al que ni la página ni los content scripts llegan. La página
// nunca recibe una contraseña guardada: pide «entrar con el perfil X» y es
// este service worker el que llena el formulario.
const PROFILES_KEY = "bteProfiles"; // { [user]: { name, iv, data } }
const STASH_KEY = "bteProfileStash"; // lo escrito a mano, sólo en memoria
const STASH_TTL = 10 * 60 * 1000;

let keyPromise = null;
const cryptoKey = () => {
  if (!keyPromise) {
    keyPromise = new Promise((resolve, reject) => {
      const open = indexedDB.open("bte-keys", 1);
      open.onupgradeneeded = () => open.result.createObjectStore("keys");
      open.onerror = () => reject(open.error);
      open.onsuccess = () => {
        const db = open.result;
        const get = db.transaction("keys").objectStore("keys").get("profiles");
        get.onerror = () => reject(get.error);
        get.onsuccess = async () => {
          if (get.result) return resolve(get.result);
          try {
            const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
            const put = db.transaction("keys", "readwrite").objectStore("keys").put(key, "profiles");
            put.onerror = () => reject(put.error);
            put.onsuccess = () => resolve(key);
          } catch (err) {
            reject(err);
          }
        };
      };
    }).catch((err) => {
      keyPromise = null;
      throw err;
    });
  }
  return keyPromise;
};

const toB64 = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes)));
const fromB64 = (text) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0));

const encrypt = async (password) => {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const data = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, await cryptoKey(), new TextEncoder().encode(password));
  return { iv: toB64(iv), data: toB64(data) };
};

const decrypt = async (entry) => {
  try {
    const plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: fromB64(entry.iv) }, await cryptoKey(), fromB64(entry.data));
    return new TextDecoder().decode(plain);
  } catch (_) {
    return null;
  }
};

const readProfiles = async () => {
  const data = await chrome.storage.local.get(PROFILES_KEY);
  const all = data[PROFILES_KEY];
  return all && typeof all === "object" ? all : {};
};

const passwordFor = async (user) => {
  const entry = (await readProfiles())[user];
  return entry ? decrypt(entry) : null;
};

const readStash = async (tabId) => {
  const stash = (await chrome.storage.session.get(STASH_KEY))[STASH_KEY];
  return stash && stash.tabId === tabId && Date.now() - stash.ts < STASH_TTL ? stash : null;
};

// Llena y envía el formulario de Eminus. Corre en el mundo aislado de la
// pestaña: la contraseña va al campo del formulario, no a los scripts de la
// página.
const fillLogin = async (user, pass) => {
  const auth = document.querySelector("m-auth");
  if (!auth) return "sin-login";
  const u = auth.querySelector('input[name="username"], input[type="text"]');
  const p = auth.querySelector('input[type="password"]');
  const b = auth.querySelector('button[type="submit"]');
  if (!u || !p || !b) return "sin-formulario";
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  const fill = (el, value) => {
    setter.call(el, value);
    for (const type of ["input", "change", "blur"]) el.dispatchEvent(new Event(type, { bubbles: true }));
  };
  fill(u, user);
  fill(p, pass);
  await new Promise((r) => setTimeout(r, 120));
  b.click();
  return "ok";
};

const handleProfile = async (message, sender) => {
  const tabId = sender.tab.id;
  const user = typeof message.user === "string" ? message.user.trim().slice(0, 200) : "";
  switch (message.type) {
    case "bte:profile.list": {
      const all = await readProfiles();
      return Object.keys(all)
        .map((u) => ({ user: u, name: all[u].name || u }))
        .sort((a, b) => a.name.localeCompare(b.name, "es", { sensitivity: "base" }));
    }
    case "bte:profile.stash": {
      const pass = typeof message.pass === "string" ? message.pass.slice(0, 1024) : "";
      if (user && pass) await chrome.storage.session.set({ [STASH_KEY]: { user, pass, tabId, ts: Date.now() } });
      else await chrome.storage.session.remove(STASH_KEY);
      return null;
    }
    case "bte:profile.stashInfo": {
      // Nunca devuelve la contraseña: sólo de quién es y si ya está guardada.
      const stash = await readStash(tabId);
      if (!stash) return null;
      const saved = await passwordFor(stash.user);
      return { user: stash.user, saved: saved != null, changed: saved != null && saved !== stash.pass };
    }
    case "bte:profile.commit": {
      const stash = await readStash(tabId);
      if (!stash) return false;
      const all = await readProfiles();
      const name = typeof message.name === "string" ? message.name.trim().slice(0, 200) : "";
      const known = all[stash.user] ? all[stash.user].name : "";
      all[stash.user] = { name: name || known || stash.user, ...(await encrypt(stash.pass)) };
      await chrome.storage.local.set({ [PROFILES_KEY]: all });
      await chrome.storage.session.remove(STASH_KEY);
      return true;
    }
    case "bte:profile.discard":
      await chrome.storage.session.remove(STASH_KEY);
      return null;
    case "bte:profile.remove": {
      const all = await readProfiles();
      if (!(user in all)) return false;
      delete all[user];
      await chrome.storage.local.set({ [PROFILES_KEY]: all });
      return true;
    }
    case "bte:profile.login": {
      const password = await passwordFor(user);
      if (password == null) return "sin-perfil";
      const tab = await chrome.tabs.get(tabId);
      // Sin el permiso "tabs", la dirección sólo se ve en páginas de Eminus.
      if (!tab.url || new URL(tab.url).origin !== EMINUS_ORIGIN) return "fuera";
      const [result] = await chrome.scripting.executeScript({
        target: { tabId, frameIds: [0] },
        func: fillLogin,
        args: [user, password]
      });
      return (result && result.result) || "error";
    }
    default:
      return null;
  }
};

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id || !message || typeof message !== "object") return false;

  if (message.type === "bte:token") {
    // Sólo se acepta desde páginas de Eminus.
    if (sender.origin !== EMINUS_ORIGIN) return false;
    const token = typeof message.token === "string" ? message.token.slice(0, 8192) : "";
    chrome.storage.session.set({ bteToken: token, bteTokenTs: Date.now() }).then(() => sendResponse({ ok: true }));
    return true;
  }

  if (typeof message.type === "string" && message.type.startsWith("bte:profile.")) {
    // Los perfiles sólo se manejan desde la página principal de Eminus, no
    // desde sus iframes ni desde otras páginas.
    if (sender.origin !== EMINUS_ORIGIN || sender.frameId !== 0 || !sender.tab) return false;
    handleProfile(message, sender)
      .then((value) => sendResponse({ value }))
      .catch(() => sendResponse({ value: message.type === "bte:profile.login" ? "error" : null }));
    return true;
  }

  return false;
});
