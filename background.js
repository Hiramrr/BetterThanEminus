// Service worker: relevo del token de Eminus hacia la vista tui.
//
// El token sólo se guarda en chrome.storage.session: vive en memoria, no se
// sincroniza ni se escribe a disco y desaparece al cerrar el navegador. Los
// content scripts no pueden escribir ahí, por eso pasa por aquí.

const EMINUS_ORIGIN = "https://eminus.uv.mx";

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id || !message || typeof message !== "object") return false;

  if (message.type === "bte:token") {
    // Sólo se acepta desde páginas de Eminus.
    if (sender.origin !== EMINUS_ORIGIN) return false;
    const token = typeof message.token === "string" ? message.token.slice(0, 8192) : "";
    chrome.storage.session.set({ bteToken: token, bteTokenTs: Date.now() }).then(() => sendResponse({ ok: true }));
    return true;
  }

  return false;
});
