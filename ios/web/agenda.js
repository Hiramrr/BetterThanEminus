// Sincroniza la sesión y la agenda con iOS. Los secretos permanecen en Swift.
(() => {
  if (window.top !== window || !globalThis.__bteNative || !globalThis.chrome) return;
  const native = globalThis.__bteNative;
  const sessionKey = "bte:m:session";
  let captured = "";
  let published = "";
  let renewing = null;
  let running = false;
  let lastToken = "";
  let sessionVersion = 0;

  const read = (storage, key) => {
    try { return JSON.parse(storage.getItem(key) || "null"); } catch (_) { return null; }
  };
  const claims = (token) => {
    try {
      const part = token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
      return JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(part.padEnd(Math.ceil(part.length / 4) * 4, "=")), c => c.charCodeAt(0))));
    } catch (_) { return {}; }
  };
  const identity = (token) => {
    const data = claims(token);
    return ["nameid", "unique_name", "sub", "email", "idPersona", "matricula"].map(k => String(data[k] ?? "").trim()).find(Boolean) || "";
  };
  const tokenNow = () => localStorage.getItem("accessToken") || sessionStorage.getItem("accessToken") || "";
  native.accountOf = identity;

  native.renewSession = (force = false, rejected) => {
    const before = tokenNow();
    if (!before) return Promise.resolve(null);
    if (force && rejected && rejected !== before) return Promise.resolve(before);
    if (renewing) return renewing;
    const version = sessionVersion;
    renewing = (async () => {
      const result = await native.post({ type: "session.refresh", force, rejected: rejected || before });
      // Una respuesta tardía nunca revive una sesión cerrada o cambia de cuenta.
      if (version !== sessionVersion || !result || typeof result.token !== "string" || tokenNow() !== before || identity(result.token) !== identity(before)) return null;
      localStorage.setItem("accessToken", result.token);
      if (sessionStorage.getItem("accessToken")) sessionStorage.setItem("accessToken", result.token);
      const session = read(localStorage, sessionKey);
      if (session) localStorage.setItem(sessionKey, JSON.stringify({ ...session, exp: claims(result.token).exp || 0 }));
      await chrome.runtime.sendMessage({ type: "bte:token", token: result.token });
      return result.token;
    })().finally(() => { renewing = null; });
    return renewing;
  };

  native.restoreSession = async (account) => {
    if (!account) return false;
    const before = tokenNow(), version = sessionVersion;
    const result = await native.post({ type: "session.refresh" });
    if (version !== sessionVersion || tokenNow() !== before || !result || typeof result.token !== "string" || identity(result.token) !== account) return false;
    localStorage.setItem("accessToken", result.token);
    const session = read(localStorage, sessionKey) || {};
    localStorage.setItem(sessionKey, JSON.stringify({ ...session, account, exp: claims(result.token).exp || 0 }));
    return true;
  };

  const sync = async () => {
    if (running) return;
    running = true;
    try {
      const token = tokenNow();
      const account = identity(token);
      if (!token || !account) {
        if (lastToken && (!claims(lastToken).exp || claims(lastToken).exp * 1000 > Date.now())) await native.clearSession();
        lastToken = "";
        return;
      }
      lastToken = token;
      const user = (read(localStorage, sessionKey) || {}).user || "";
      const captureKey = token + "\n" + user;
      if (captureKey !== captured) {
        const response = await native.post({ type: "session.capture", token, user });
        if (response !== true) {
          if (claims(token).exp * 1000 < Date.now() + 5 * 60000) await native.renewSession();
          return;
        }
        captured = captureKey;
      }
      if (claims(token).exp * 1000 < Date.now() + 5 * 60000) await native.renewSession();
      const cache = read(sessionStorage, "bte:store:session:bteAppCache");
      if (cache && cache.account === account && Array.isArray(cache.courses) && Array.isArray(cache.acts)) {
        const signature = account + ":" + cache.ts;
        if (signature !== published) {
          const courses = new Set(cache.courses.filter(c => c.current).map(c => c.id));
          const tasks = cache.acts.flatMap(([courseID, acts]) => courses.has(courseID) && Array.isArray(acts) ? acts : [])
            .filter(a => !a.delivered && Number.isFinite(a.deadline) && a.deadline > 0)
            .map(a => ({ id: a.courseId + "-" + a.id, title: a.title, course: a.courseName, deadline: a.deadline,
              target: JSON.stringify({ course: a.courseName, section: "activities", activity: { id: a.id, courseId: a.courseId } }) }));
          if (await native.post({ type: "agenda.sync", account, updatedAt: cache.ts / 1000, tasks }) === true) published = signature;
        }
      }
      await native.post({ type: "page.ready" });
    } finally { running = false; }
  };
  native.clearSession = async () => {
    sessionVersion++;
    captured = "";
    published = "";
    await native.post({ type: "session.clear" });
  };
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "session" && (changes.bteAppCache || changes.bteToken)) sync();
  });
  window.addEventListener("pageshow", sync);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) sync(); });
  window.addEventListener("online", sync);
  setInterval(() => { if (!document.hidden) sync(); }, 30000);
  sync();
})();
