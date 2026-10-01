// App de iOS: sustituto de las API de extensión (chrome.*) sobre las que está
// escrito BetterThanEminus. Todo vive en el origen de Eminus:
//
// - storage.local / sync → localStorage; storage.session → sessionStorage.
//   Los cambios llegan a los demás marcos (vista tui, ajustes, iframe de
//   actividades) por BroadcastChannel, como chrome.storage.onChanged.
// - runtime.sendMessage / tabs.sendMessage → BroadcastChannel entre marcos.
//   El relevo del token («bte:token») lo guarda aquí mismo, sin service worker.
// - runtime.getURL / getManifest y tabs.create, lo mínimo que se usa.
(() => {
  if (globalThis.chrome && globalThis.chrome.runtime && globalThis.chrome.runtime.id) return;

  const native = (globalThis.__bteNative = globalThis.__bteNative || {});
  const RUNTIME_ID = "bte-ios";

  // Puente con Swift (WKScriptMessageHandlerWithReply): devuelve una promesa.
  native.post = (msg) => {
    try {
      return Promise.resolve(window.webkit.messageHandlers.bte.postMessage(msg)).catch(() => null);
    } catch (_) {
      return Promise.resolve(null);
    }
  };

  // Eminus vacía localStorage al cerrar sesión, cuando vence y al cargar una
  // versión nueva. Ahí viven las preferencias, el fondo y los borradores
  // (que hacen falta justo al volver a entrar): lo de «bte:» se conserva.
  // sessionStorage (token y datos en caché de la sesión) sí se vacía.
  try {
    const proto = Storage.prototype;
    if (!proto.__bteKeep) {
      const clear = proto.clear;
      proto.clear = function () {
        const kept = [];
        if (this === window.localStorage) {
          for (let i = 0; i < this.length; i += 1) {
            const k = this.key(i);
            if (k && k.startsWith("bte:")) kept.push([k, this.getItem(k)]);
          }
        }
        clear.call(this);
        for (const [k, v] of kept) this.setItem(k, v);
      };
      Object.defineProperty(proto, "__bteKeep", { value: true });
    }
  } catch (_) {
  }

  // ───────────── storage ─────────────

  const PREFIX = "bte:store:";
  const storeChannel = new BroadcastChannel("bte-storage");
  const changeListeners = new Set();
  const backend = (area) => (area === "session" ? sessionStorage : localStorage);
  const fullKey = (area, key) => PREFIX + area + ":" + key;

  const read = (area, key) => {
    try {
      const raw = backend(area).getItem(fullKey(area, key));
      return raw == null ? undefined : JSON.parse(raw);
    } catch (_) {
      return undefined;
    }
  };

  const keysOf = (area) => {
    const store = backend(area);
    const prefix = PREFIX + area + ":";
    const out = [];
    for (let i = 0; i < store.length; i += 1) {
      const k = store.key(i);
      if (k && k.startsWith(prefix)) out.push(k.slice(prefix.length));
    }
    return out;
  };

  const emit = (changes, area, broadcast) => {
    for (const fn of [...changeListeners]) {
      try {
        fn(changes, area);
      } catch (err) {
        console.error("[bte] storage.onChanged", err);
      }
    }
    if (broadcast) storeChannel.postMessage({ changes, area });
  };
  storeChannel.onmessage = (e) => e.data && emit(e.data.changes, e.data.area, false);

  const makeArea = (name) => ({
    async get(keys) {
      let list;
      let defaults = {};
      if (keys == null) list = keysOf(name);
      else if (typeof keys === "string") list = [keys];
      else if (Array.isArray(keys)) list = keys;
      else {
        defaults = keys;
        list = Object.keys(keys);
      }
      const out = {};
      for (const k of list) {
        const v = read(name, k);
        if (v !== undefined) out[k] = v;
        else if (k in defaults) out[k] = defaults[k];
      }
      return out;
    },
    async set(items) {
      const changes = {};
      for (const [k, v] of Object.entries(items || {})) {
        const oldValue = read(name, k);
        try {
          backend(name).setItem(fullKey(name, k), JSON.stringify(v));
        } catch (_) {
          throw new Error("no hay espacio para guardar " + k);
        }
        changes[k] = { oldValue, newValue: v };
      }
      if (Object.keys(changes).length) emit(changes, name, true);
    },
    async remove(keys) {
      const changes = {};
      for (const k of typeof keys === "string" ? [keys] : keys || []) {
        const oldValue = read(name, k);
        if (oldValue === undefined) continue;
        backend(name).removeItem(fullKey(name, k));
        changes[k] = { oldValue };
      }
      if (Object.keys(changes).length) emit(changes, name, true);
    },
    async clear() {
      await this.remove(keysOf(name));
    }
  });

  // ───────────── mensajes entre marcos ─────────────

  const runtimeChannel = new BroadcastChannel("bte-runtime");
  const messageListeners = new Set();
  const waiting = new Map();
  const sender = { id: RUNTIME_ID, origin: location.origin, url: location.href };

  // Igual que chrome.runtime.onMessage: responde el primero que llame a
  // sendResponse; true como retorno deja la respuesta para después.
  const deliver = (message, reply) => {
    let answered = false;
    let later = false;
    const sendResponse = (response) => {
      if (answered) return;
      answered = true;
      reply(response);
    };
    for (const fn of [...messageListeners]) {
      try {
        if (fn(message, sender, sendResponse) === true) later = true;
      } catch (err) {
        console.error("[bte] runtime.onMessage", err);
      }
    }
    return answered || later;
  };

  runtimeChannel.onmessage = (e) => {
    const d = e.data;
    if (!d) return;
    if (d.kind === "req") {
      deliver(d.message, (response) => runtimeChannel.postMessage({ kind: "res", id: d.id, response }));
    } else if (d.kind === "res" && waiting.has(d.id)) {
      waiting.get(d.id).resolve(d.response);
      waiting.delete(d.id);
    }
  };

  // Sin respuesta en un rato: nadie escucha (como «Receiving end does not exist»).
  const broadcast = (message, timeoutMs, rejectOnSilence) =>
    new Promise((resolve, reject) => {
      const id = Math.random().toString(36).slice(2);
      waiting.set(id, { resolve });
      runtimeChannel.postMessage({ kind: "req", id, message });
      setTimeout(() => {
        if (!waiting.has(id)) return;
        waiting.delete(id);
        if (rejectOnSilence) reject(new Error("Could not establish connection. Receiving end does not exist."));
        else resolve(undefined);
      }, timeoutMs);
    });

  const session = makeArea("session");

  const eventOf = (set) => ({
    addListener: (fn) => set.add(fn),
    removeListener: (fn) => set.delete(fn),
    hasListener: (fn) => set.has(fn)
  });

  globalThis.chrome = {
    storage: {
      local: makeArea("local"),
      sync: makeArea("sync"),
      session,
      onChanged: eventOf(changeListeners)
    },
    runtime: {
      id: RUNTIME_ID,
      lastError: undefined,
      getURL: (path) => location.origin + "/__bte/" + String(path || "").replace(/^\/+/, ""),
      getManifest: () => ({ version: native.version || "?", name: "BetterThanEminus" }),
      async sendMessage(message) {
        // En la extensión, el service worker guarda el token para la vista
        // tui. Aquí todo es el mismo origen: va directo a storage.session.
        if (message && message.type === "bte:token") {
          const token = typeof message.token === "string" ? message.token.slice(0, 8192) : "";
          if ((await session.get("bteToken")).bteToken !== token) await session.set({ bteToken: token, bteTokenTs: Date.now() });
          return { ok: true };
        }
        return broadcast(message, 1500, false);
      },
      onMessage: eventOf(messageListeners)
    },
    tabs: {
      async query() {
        return [{ id: 1, active: true, url: location.href }];
      },
      sendMessage: (_tabId, message) => broadcast(message, 800, true),
      async create({ url }) {
        const target = new URL(url, location.href);
        if (target.hostname === "eminus.uv.mx") window.top.location.assign(target.href);
        else native.post({ type: "open", url: target.href });
        return { id: 1 };
      }
    }
  };
})();
