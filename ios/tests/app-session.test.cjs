const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const jwt = account => 'h.' + Buffer.from(JSON.stringify({ nameid: account, exp: Date.now() / 1000 + 3600 })).toString('base64url') + '.s';

function load({ fetch, cache, renewSession = async () => null }) {
  const element = { addEventListener() {}, classList: { toggle() {} } };
  const window = { top: {}, parent: { __bteNative: { renewSession }, postMessage() {} } };
  const context = vm.createContext({
    bte: {}, __bteNative: {}, fetch, atob, TextDecoder, Uint8Array, window,
    document: { documentElement: element, getElementById: () => element, querySelectorAll: () => [], addEventListener() {} },
    chrome: { storage: { session: { get: async () => ({ bteAppCache: cache }) }, onChanged: { addListener() {} } } }
  });
  const source = fs.readFileSync(path.join(__dirname, '../../app/app.js'), 'utf8');
  vm.runInContext(source.replace('  init();', '  globalThis.app = { state, api, restoreCache };'), context);
  context.app.state.token = jwt('123');
  return context.app;
}

test('la agenda nunca restaura una caché de otra cuenta', async () => {
  const app = load({ cache: { account: '456', courses: [{ id: 99 }], acts: [], ts: Date.now() } });
  assert.equal(await app.restoreCache(), false);
  assert.equal(app.state.courses.length, 0);
});

test('una respuesta de la cuenta anterior no entra después del cambio', async () => {
  let resolve;
  const app = load({ fetch: () => new Promise(r => { resolve = r; }) });
  const request = app.api('/Course/getAllCourses');
  app.state.token = jwt('456');
  resolve({ ok: true, status: 200, json: async () => [{ id: 1 }] });
  await assert.rejects(request, /la cuenta cambió/);
});

test('una respuesta 401 renueva y reintenta una sola vez con el token nuevo', async () => {
  const headers = [];
  const token = jwt('123');
  const app = load({ renewSession: async () => token, fetch: async (_, options) => {
    headers.push(options.headers.Authorization);
    return headers.length === 1 ? { status: 401 } : { ok: true, status: 200, json: async () => ['ok'] };
  } });
  assert.deepEqual(await app.api('/test'), ['ok']);
  assert.equal(headers.length, 2);
  assert.equal(headers[1], 'Bearer ' + token);
});

test('si el reintento vuelve a fallar no hay un bucle de login', async () => {
  let requests = 0, renewals = 0;
  const app = load({ renewSession: async () => { renewals++; return jwt('123'); }, fetch: async () => { requests++; return { status: 401 }; } });
  await assert.rejects(app.api('/test'), /sesión expirada/);
  assert.equal(requests, 2);
  assert.equal(renewals, 1);
});

test('un permiso denegado de una sección no renueva la sesión', async () => {
  let renewals = 0;
  const app = load({ renewSession: async () => { renewals++; return jwt('123'); }, fetch: async () => ({ status: 403 }) });
  await assert.rejects(app.api('/test'), /No tienes permiso/);
  assert.equal(renewals, 0);
});
