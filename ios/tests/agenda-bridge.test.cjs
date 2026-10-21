const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const source = readFileSync(new URL('../web/agenda.js', 'file://' + __filename), 'utf8');
const jwt = (account, exp = Date.now() / 1000 + 3600) => 'header.' + Buffer.from(JSON.stringify({ nameid: account, exp })).toString('base64url') + '.signature';
const flush = async () => { for (let i = 0; i < 10; i++) await new Promise(setImmediate); };
const storage = () => {
  const data = new Map();
  return { getItem: k => data.get(k) ?? null, setItem: (k, v) => data.set(k, String(v)), removeItem: k => data.delete(k) };
};

async function load({ token = jwt('123'), cache, reply } = {}) {
  const localStorage = storage(), sessionStorage = storage();
  if (token) localStorage.setItem('accessToken', token);
  if (cache) sessionStorage.setItem('bte:store:session:bteAppCache', JSON.stringify(cache));
  const posts = [], timers = [], events = new Map();
  let change;
  const native = { post: async msg => {
    posts.push(msg);
    if (reply) { const result = await reply(msg); if (result !== undefined) return result; }
    return msg.type === 'session.capture' || msg.type === 'agenda.sync' ? true : null;
  } };
  const window = { addEventListener: (type, fn) => events.set(type, fn) };
  window.top = window;
  const context = { window, document: { hidden: false, addEventListener: (type, fn) => events.set(type, fn) },
    localStorage, sessionStorage, __bteNative: native, TextDecoder, Uint8Array, atob, Date,
    chrome: { storage: { onChanged: { addListener: fn => { change = fn; } } }, runtime: { sendMessage: async () => {} } },
    setInterval: fn => timers.push(fn) };
  vm.runInNewContext(source, context);
  await flush();
  return { localStorage, sessionStorage, posts, native, tick: async () => { timers[0](); await flush(); }, change };
}

const cache = {
  account: '123', ts: Date.now(), courses: [{ id: 1, current: true }, { id: 2, current: false }],
  acts: [[1, [{ id: 5, courseId: 1, title: 'Informe', courseName: 'Física', deadline: Date.now() / 1000 + 7200 },
    { id: 6, delivered: true, deadline: 1 }, { id: 7, deadline: 0 }]], [2, [{ id: 8, deadline: 1 }]]]
};

test('publica solo pendientes con fecha de cursos actuales y conserva el destino', async () => {
  const app = await load({ cache });
  const snapshot = app.posts.find(p => p.type === 'agenda.sync');
  assert.equal(snapshot.account, '123');
  assert.equal(snapshot.tasks.length, 1);
  assert.equal(snapshot.tasks[0].id, '1-5');
  assert.equal(JSON.parse(snapshot.tasks[0].target).activity.id, 5);
  await app.tick();
  assert.equal(app.posts.filter(p => p.type === 'agenda.sync').length, 1);
});

test('una caché de otra cuenta no se publica en widgets ni avisos', async () => {
  const app = await load({ cache: { ...cache, account: 'otra' } });
  assert.equal(app.posts.some(p => p.type === 'agenda.sync'), false);
});

test('reintenta la publicación si el puente todavía no está listo', async () => {
  let attempts = 0;
  const app = await load({ cache, reply: msg => msg.type === 'agenda.sync' ? ++attempts > 1 : undefined });
  await app.tick();
  assert.equal(attempts, 2);
});

test('renovar nunca acepta un token de otra cuenta', async () => {
  const token = jwt('123');
  const app = await load({ token, reply: msg => msg.type === 'session.refresh' ? { token: jwt('456') } : undefined });
  assert.equal(await app.native.renewSession(true), null);
  assert.equal(app.localStorage.getItem('accessToken'), token);
});

test('una renovación tardía no revive una sesión cerrada', async () => {
  let resolve;
  const app = await load({ reply: msg => msg.type === 'session.refresh' ? new Promise(r => { resolve = r; }) : undefined });
  const pending = app.native.renewSession(true);
  await flush();
  app.localStorage.removeItem('accessToken');
  resolve({ token: jwt('123') });
  assert.equal(await pending, null);
  assert.equal(app.localStorage.getItem('accessToken'), null);
});

test('al salir con token vigente se limpian la sesión nativa y su agenda', async () => {
  const app = await load();
  app.localStorage.removeItem('accessToken');
  await app.tick();
  await app.tick();
  assert.equal(app.posts.filter(p => p.type === 'session.clear').length, 1);
});

test('no borra la sesión nativa al expirar el token web', async () => {
  const app = await load({ token: jwt('123', Date.now() / 1000 - 10) });
  app.localStorage.removeItem('accessToken');
  await app.tick();
  assert.equal(app.posts.some(p => p.type === 'session.clear'), false);
});

test('reutiliza el token nuevo cuando llega tarde un rechazo del anterior', async () => {
  const app = await load();
  const current = jwt('123');
  app.localStorage.setItem('accessToken', current);
  assert.equal(await app.native.renewSession(true, jwt('123', 1)), current);
  assert.equal(app.posts.some(p => p.type === 'session.refresh'), false);
});

test('restaura la sesión web solo con un token de la cuenta esperada', async () => {
  const app = await load({ token: '', reply: msg => msg.type === 'session.refresh' ? { token: jwt('456') } : undefined });
  assert.equal(await app.native.restoreSession('123'), false);
  assert.equal(app.localStorage.getItem('accessToken'), null);
  assert.equal(await app.native.restoreSession('456'), true);
  assert.equal(JSON.parse(app.localStorage.getItem('bte:m:session')).account, '456');
});

test('cerrar la sesión invalida la renovación aunque localStorage aún no se haya vaciado', async () => {
  let resolve;
  const app = await load({ reply: msg => msg.type === 'session.refresh' ? new Promise(r => { resolve = r; }) : undefined });
  const before = app.localStorage.getItem('accessToken');
  const pending = app.native.renewSession(true);
  await flush();
  await app.native.clearSession();
  resolve({ token: jwt('123', Date.now() / 1000 + 7200) });
  assert.equal(await pending, null);
  assert.equal(app.localStorage.getItem('accessToken'), before);
});
