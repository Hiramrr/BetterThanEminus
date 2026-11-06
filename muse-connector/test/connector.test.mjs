import assert from "node:assert/strict";
import { createDecipheriv, pbkdf2Sync, X509Certificate } from "node:crypto";
import tls from "node:tls";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { computeAlerts } from "../src/alerts.mjs";
import { EminusClient, authRecord, htmlToText, contentLinks, normalizeTask, seal } from "../src/eminus.mjs";
import { zipSync, strToU8 } from "fflate";
import { GODADDY_R1 } from "../src/http.mjs";
import { createApp, sha256 } from "../src/node-server.mjs";

const FAKE_PASSWORD = "contraseña-de-prueba";

test("seal cifra como el formulario de Eminus y se puede abrir", () => {
  const salt = Buffer.alloc(16, 1);
  const iv = Buffer.alloc(16, 2);
  const sealed = seal(FAKE_PASSWORD, "llave-app", salt, iv);
  assert.equal(sealed, seal(FAKE_PASSWORD, "llave-app", salt, iv));
  assert.ok(!sealed.includes(FAKE_PASSWORD));
  const raw = Buffer.from(sealed, "base64");
  assert.deepEqual(raw.subarray(0, 16), salt);
  assert.deepEqual(raw.subarray(16, 32), iv);
  const key = pbkdf2Sync("llave-app", salt, 100, 32, "sha1");
  const d = createDecipheriv("aes-256-cbc", key, iv);
  assert.equal(Buffer.concat([d.update(raw.subarray(32)), d.final()]).toString(), FAKE_PASSWORD);
  assert.notEqual(seal(FAKE_PASSWORD), seal(FAKE_PASSWORD), "sal al azar");
});

test("la raíz GoDaddy R1 incluida tiene la huella esperada", () => {
  const r1 = new X509Certificate(GODADDY_R1);
  assert.match(r1.subject, /GoDaddy TLS Root CA - R1/);
  assert.equal(r1.fingerprint256, "25:CF:3D:A8:E9:B9:7A:DD:BF:92:54:3C:2B:82:52:7C:8A:4E:2C:FF:20:62:A6:48:30:40:D4:B6:4A:CE:71:9F");
  assert.ok(tls.rootCertificates.some((p) => new X509Certificate(p).subject.includes("Go Daddy Root Certificate Authority - G2")));
});

test("authRecord toma el último del arreglo y la envoltura contenido", () => {
  assert.equal(authRecord([{ accessToken: "a" }, { accessToken: "b" }]).accessToken, "b");
  assert.equal(authRecord({ contenido: { accessToken: "c" } }).accessToken, "c");
});

const course = { id: "123", name: "MATERIA A" };
const now = Date.parse("2026-11-05T12:00:00Z") / 1000;

test("normalizeTask: estados de entrega y prórroga", () => {
  const base = { idActividad: 9, titulo: " Tarea ", fechaInicioEpoch: now - 86400, fechaTerminoEpoch: now + 3600, visible: 1 };
  assert.equal(normalizeTask({ ...base, idEstado: 0 }, course, now).status, "pending");
  assert.equal(normalizeTask({ ...base, idEstado: 1 }, course, now).status, "submitted");
  assert.equal(normalizeTask({ ...base, idEstado: 3 }, course, now).status, "reviewed");
  assert.equal(normalizeTask({ ...base, idEstado: 0, fechaTerminoEpoch: now - 10 }, course, now).status, "overdue");
  assert.equal(normalizeTask({ ...base, fechaInicioEpoch: now + 10 }, course, now).status, "upcoming");
  const ext = normalizeTask({ ...base, fechaTerminoEpoch: now - 10, fechaProrrogaEpoch: now + 99 }, course, now);
  assert.equal(ext.status, "pending");
  assert.equal(ext.has_extension, true);
  assert.equal(ext.id, "123-9");
  assert.equal(ext.title, "Tarea");
  assert.equal(normalizeTask({ ...base, visible: 0 }, course, now), null);
});

test("htmlToText", () => {
  assert.equal(htmlToText("<p>Hola&nbsp;<b>mundo</b></p><ul><li>uno</li></ul>&aacute;"), "Hola mundo\n• uno\ná");
});

function fakeClient(tasks) {
  return {
    calls: 0,
    async tasks() { this.calls++; return { courses: [course], tasks, failedCourses: [] }; },
    async description() { return { text: "instrucciones", files: [] }; }
  };
}

async function withServer(client, fn) {
  const key = "mec_test_key";
  const app = createApp({ client, apiKeyHash: sha256(key), log: () => {} });
  await new Promise((r) => app.listen(0, "127.0.0.1", r));
  const base = `http://127.0.0.1:${app.address().port}`;
  try { await fn(base, key); } finally { app.close(); }
}

test("servidor: API key, solo lectura, paginación y errores", async () => {
  const tasks = Array.from({ length: 5 }, (_, i) => normalizeTask({ idActividad: i + 1, titulo: "T" + i, fechaTerminoEpoch: now + i * 3600 + 1e8, idEstado: i === 4 ? 1 : 0 }, course, now));
  const client = fakeClient(tasks);
  await withServer(client, async (base, key) => {
    assert.equal((await fetch(base + "/health")).status, 200);
    const noKey = await fetch(base + "/tasks");
    assert.equal(noKey.status, 401);
    assert.equal((await noKey.json()).error.code, "unauthorized");
    assert.equal((await fetch(base + "/tasks", { headers: { Authorization: "Bearer otra" } })).status, 401);
    assert.equal((await fetch(base + "/tasks", { method: "POST", headers: { Authorization: "Bearer " + key } })).status, 405);
    const h = { Authorization: "Bearer " + key };
    const p1 = await (await fetch(base + "/tasks?limit=2", { headers: h })).json();
    assert.equal(p1.total, 4);
    assert.equal(p1.data.length, 2);
    const p2 = await (await fetch(base + "/tasks?limit=2&cursor=" + p1.next_cursor, { headers: h })).json();
    assert.deepEqual(p2.data.map((t) => t.id), ["123-3", "123-4"]);
    assert.equal(p2.next_cursor, null);
    const all = await (await fetch(base + "/tasks?status=all", { headers: { "X-API-Key": key } })).json();
    assert.equal(all.total, 5);
    assert.equal((await fetch(base + "/tasks?limit=0", { headers: h })).status, 400);
    assert.equal((await fetch(base + "/tasks?cursor=@@", { headers: h })).status, 400);
    const one = await (await fetch(base + "/tasks/123-1", { headers: h })).json();
    assert.equal(one.data.description, "instrucciones");
    assert.equal((await fetch(base + "/tasks/999-1", { headers: h })).status, 404);
    assert.equal(client.calls, 1, "caché de 5 minutos");
  });
});

test("servidor: credenciales rechazadas → 503 sin filtrar detalles", async () => {
  const client = new EminusClient({
    getCredentials: async () => ({ user: "u", password: FAKE_PASSWORD }),
    fetch: async () => ({ status: 400, ok: false, json: async () => ({}), text: async () => "" })
  });
  await withServer(client, async (base, key) => {
    const res = await fetch(base + "/tasks", { headers: { Authorization: "Bearer " + key } });
    assert.equal(res.status, 503);
    const body = await res.text();
    assert.match(body, /credentials_rejected/);
    assert.ok(!body.includes(FAKE_PASSWORD));
  });
});

test("avisos: nueva, 1 día, vencida y sin duplicados", () => {
  const t0 = Date.parse("2026-11-05T12:00:00Z");
  const task = (id, dueOffsetH, extra = {}) => ({ id, title: id, course: "C", status: dueOffsetH < 0 ? "overdue" : "pending", submitted: false, due_at: new Date(t0 + dueOffsetH * 3600e3).toISOString(), ...extra });

  // Primera corrida: línea base, sin «nueva»; sí avisa lo urgente.
  let r = computeAlerts([task("a", 100), task("b", 10), task("c", -5)], {}, t0);
  assert.deepEqual(r.alerts.map((a) => a.task_id + ":" + a.kinds), ["c:overdue", "b:due_soon"]);

  // Misma información: nada.
  r = computeAlerts([task("a", 100), task("b", 10), task("c", -5)], r.state, t0 + 60e3);
  assert.equal(r.alerts.length, 0);

  // Aparece una tarea nueva que además vence pronto: un solo aviso combinado.
  r = computeAlerts([task("a", 100), task("b", 10), task("c", -5), task("d", 20)], r.state, t0 + 120e3);
  assert.deepEqual(r.alerts.map((a) => a.task_id + ":" + a.kinds.join("+")), ["d:new+due_soon"]);

  // «b» vence: aviso de vencida, una vez.
  const later = t0 + 11 * 3600e3;
  r = computeAlerts([task("a", 100), task("b", 10, { status: "overdue" }), task("c", -5), task("d", 20)], r.state, later);
  assert.deepEqual(r.alerts.map((a) => a.task_id + ":" + a.kinds), ["b:overdue"]);
  r = computeAlerts([task("a", 100), task("b", 10, { status: "overdue" }), task("c", -5), task("d", 20)], r.state, later + 60e3);
  assert.equal(r.alerts.length, 0);

  // Prórroga: cambia la fecha → puede volver a avisar «falta 1 día».
  r = computeAlerts([task("a", 100), task("b", 30), task("c", -5), task("d", 20)], r.state, later + 120e3);
  assert.deepEqual(r.alerts.map((a) => a.task_id + ":" + a.kinds), ["b:due_soon"]);

  // Entregada: no avisa.
  r = computeAlerts([task("a", 10, { status: "submitted", submitted: true })], r.state, t0 + 95 * 3600e3);
  assert.equal(r.alerts.length, 0);
});

import { parseFileRef } from "../src/eminus.mjs";
import { extractText } from "../src/files.mjs";

test("parseFileRef sólo acepta referencias numéricas propias", () => {
  assert.deepEqual(parseFileRef("c10-20-30"), { kind: "content", course: "10", parent: "20", file: "30" });
  assert.equal(parseFileRef("a10-20-30").kind, "activity");
  for (const bad of ["x1-2-3", "c1-2", "c1-2-3/../4", "c0-0-0", "c1-2-3?x"]) assert.equal(parseFileRef(bad), null, bad);
});

test("extractText: notebook por celdas y texto por bloques", async () => {
  const nb = Buffer.from(JSON.stringify({ cells: [{ cell_type: "markdown", source: ["# Tema\n", "intro"] }, { cell_type: "code", source: "print(1)" }] }));
  const r = await extractText(nb, "clase.ipynb");
  assert.equal(r.total_pages, 2);
  assert.match(r.pages[1].text, /```python\nprint\(1\)\n```/);
  const t = await extractText(Buffer.from("a".repeat(45000)), "datos.csv", { from: 2, to: 2 });
  assert.equal(t.total_pages, 3);
  assert.equal(t.pages[0].text.length, 20000);
  assert.equal(t.next_from, 3);
  await assert.rejects(extractText(Buffer.from("x"), "tarea.docx"), (e) => e.code === "unreadable_file");
  await assert.rejects(extractText(Buffer.from("x"), "tarea.doc"), (e) => e.code === "unsupported_type");
});

test("contentLinks conserva videos y enlaces sin ejecutar HTML", () => {
  const html = '<iframe src="https://www.youtube.com/embed/abcdefghijk?si=tracking"></iframe>' +
    '<a href="https://youtu.be/abcdefghijk">Video repetido</a>' +
    '<a href="https://example.org/?a=1&amp;b=2">Libro</a>' +
    '<a href="javascript:alert(1)">No</a><script><iframe src="https://evil.example"></iframe></script>';
  assert.deepEqual(contentLinks(html), [
    { type: "video", url: "https://www.youtube.com/watch?v=abcdefghijk", provider: "youtube" },
    { type: "link", url: "https://example.org/?a=1&b=2" }
  ]);
});

test("Office: texto de Word, orden de diapositivas y celdas de Excel", async () => {
  const zip = (files) => zipSync(Object.fromEntries(Object.entries(files).map(([name, xml]) => [name, strToU8(xml)])));
  const word = zip({ "word/document.xml": '<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Hola &amp; mundo</w:t><w:tab/><w:t>dato</w:t></w:r></w:p></w:body></w:document>' });
  assert.equal((await extractText(word, "tarea.docx")).pages[0].text, "Hola & mundo\tdato");
  await withServer({
    ...fakeClient([]), async courses() { return [course]; },
    async download() { return { bytes: word, contentType: "application/octet-stream", contentDisposition: 'attachment; filename="tarea.docx"' }; }
  }, async (base, key) => {
    const response = await fetch(base + "/files/c123-5-7", { headers: { Authorization: "Bearer " + key } });
    assert.equal(response.status, 200);
    const body = await response.json();
    assert.equal(body.format, "docx");
    assert.equal(body.pages[0].text, "Hola & mundo\tdato");
    assert.equal(body.text_url, base + "/files/c123-5-7");
  });
  const ppt = zip({
    "ppt/presentation.xml": '<p:presentation xmlns:p="urn:p" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="r2"/><p:sldId id="257" r:id="r1"/></p:sldIdLst></p:presentation>',
    "ppt/_rels/presentation.xml.rels": '<Relationships><Relationship Id="r1" Target="slides/slide1.xml"/><Relationship Id="r2" Target="slides/slide2.xml"/></Relationships>',
    "ppt/slides/slide1.xml": '<slide xmlns:a="urn:a"><a:p><a:r><a:t>Segundo</a:t></a:r></a:p></slide>',
    "ppt/slides/slide2.xml": '<slide xmlns:a="urn:a"><a:p><a:r><a:t>Primero</a:t></a:r></a:p></slide>'
  });
  const slides = await extractText(ppt, "clase.pptx", { from: 1, to: 1 });
  assert.match(slides.pages[0].text, /Primero/);
  assert.equal(slides.total_pages, 2);
  assert.equal(slides.next_from, 2);
  const xlsx = zip({
    "xl/workbook.xml": '<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Datos" r:id="r1"/></sheets></workbook>',
    "xl/_rels/workbook.xml.rels": '<Relationships><Relationship Id="r1" Target="/xl/worksheets/sheet1.xml"/></Relationships>',
    "xl/sharedStrings.xml": '<sst><si><t>Etiqueta</t></si></sst>',
    "xl/worksheets/sheet1.xml": '<worksheet><sheetData><row><c r="A1" t="s"><v>0</v></c><c r="C1"><v>42</v></c><c r="D1"><f>C1*2</f><v>84</v></c></row><row><c r="B2" t="inlineStr"><is><t>Texto</t></is></c></row></sheetData></worksheet>'
  });
  assert.equal((await extractText(xlsx, "datos.xlsx")).pages[0].text, "Hoja: Datos\nA1=Etiqueta\tC1=42\tD1=84\nB2=Texto");
  const dtd = zip({ "word/document.xml": '<!DOCTYPE x [<!ENTITY a "boom">]><x>&a;</x>' });
  await assert.rejects(extractText(dtd, "tarea.docx"), (e) => e.code === "unreadable_file");
  const oversized = zip({ "word/document.xml": "x".repeat(8 * 1024 * 1024 + 1) });
  await assert.rejects(extractText(oversized, "tarea.docx"), (e) => e.code === "unreadable_file");
});

test("servidor: archivos sólo de cursos propios y con API key", async () => {
  const client = {
    ...fakeClient([]),
    async courses() { return [course]; },
    async content() { return [{ id: "1", title: "U1", elements: [{ id: "5", title: "R", text: null, files: [{ id: "c123-5-7", name: "notas.txt", type: "txt" }] }] }]; },
    async download(ref) { assert.equal(ref, "c123-5-7"); return { bytes: Buffer.from("hola mundo"), contentType: "text/plain" }; }
  };
  await withServer(client, async (base, key) => {
    const h = { Authorization: "Bearer " + key };
    assert.equal((await fetch(base + "/files/c123-5-7/text")).status, 401);
    assert.equal((await fetch(base + "/files/c999-5-7/text", { headers: h })).status, 404);
    assert.equal((await fetch(base + "/courses/999/content", { headers: h })).status, 404);
    const content = await (await fetch(base + "/courses/123/content", { headers: h })).json();
    assert.equal(content.files, 1);
    const text = await (await fetch(base + "/files/c123-5-7/text", { headers: h })).json();
    assert.equal(text.name, "notas.txt");
    assert.equal(text.pages[0].text, "hola mundo");
    const bin = await fetch(base + "/files/c123-5-7/download", { headers: h });
    const asText = await (await fetch(base + "/files/c123-5-7", { headers: h })).json();
    assert.equal(asText.pages[0].text, "hola mundo", "GET /files/{id} devuelve texto");
    assert.match(content.data[0].elements[0].files[0].text_url, /\/files\/c123-5-7$/);
    assert.equal(await bin.text(), "hola mundo");
    assert.match(bin.headers.get("content-disposition"), /notas\.txt/);
  });
});

import { serverConfig } from "../src/config.mjs";

test("serverConfig: variables del formulario de Vercel", async () => {
  const a = serverConfig({ EMINUS_USER: " zs1 ", EMINUS_PASSWORD: FAKE_PASSWORD, MUSE_API_KEY: "k".repeat(10) + "abcdefghijklmnopqrstuvwxyz" });
  assert.equal(a.apiKeyHash, sha256("k".repeat(10) + "abcdefghijklmnopqrstuvwxyz"));
  assert.deepEqual(await a.getCredentials(), { user: "zs1", password: FAKE_PASSWORD });
  assert.equal(serverConfig({ MUSE_API_KEY: "corta" }).apiKeyHash, "");
  assert.match(serverConfig({ MUSE_API_KEY: "corta" }).problems[0], /32/);
  assert.equal(serverConfig({ MUSE_API_KEY: "a".repeat(40) }).apiKeyHash, "", "rechaza llaves triviales");
  const b = serverConfig({ MUSE_API_KEY_SHA256: sha256("x"), EMINUS_CREDENTIALS: JSON.stringify({ user: "u", password: "p" }) });
  assert.equal(b.apiKeyHash, sha256("x"));
  assert.equal((await b.getCredentials()).user, "u");
  await assert.rejects(serverConfig({ MUSE_API_KEY_SHA256: sha256("x") }).getCredentials(), /EMINUS_USER/);
});

test("servidor: /docs es público y lleva la URL del conector", async () => {
  const app = createApp({ client: fakeClient([]), apiKeyHash: sha256("k"), log: () => {}, docs: { CONNECTOR: "Base: {{BASE_URL}}" } });
  await new Promise((r) => app.listen(0, "127.0.0.1", r));
  const base = `http://127.0.0.1:${app.address().port}`;
  try {
    const doc = await fetch(base + "/docs/CONNECTOR.md");
    assert.equal(doc.status, 200);
    assert.equal(await doc.text(), "Base: " + base);
    assert.match(await (await fetch(base + "/docs")).text(), /CONNECTOR\.md/);
    assert.equal((await fetch(base + "/docs/../tasks")).status, 401);
    assert.equal((await fetch(base + "/docs/NADA.md")).status, 404);
  } finally { app.close(); }
});

test("Vercel: los documentos pasan por la función antes de servir archivos estáticos", async () => {
  const config = JSON.parse(readFileSync(new URL("../vercel.json", import.meta.url), "utf8"));
  assert.equal(config.routes[0].handle, undefined);
  const route = config.routes[0];
  const originalKey = process.env.MUSE_API_KEY_SHA256;
  process.env.MUSE_API_KEY_SHA256 = sha256("test-key");
  try {
    const { default: handler } = await import("../api/handler.mjs");
    for (const name of ["CONNECTOR", "AVISOS", "ESTUDIO"]) {
      const path = `/docs/${name}.md`;
      const destination = path.replace(new RegExp(`^${route.src}$`), route.dest);
      const response = await handler.fetch(new Request("https://otro-conector.example" + destination));
      assert.equal(response.status, 200);
      const text = await response.text();
      assert.ok(!text.includes("{{BASE_URL}}"));
      if (name === "CONNECTOR") assert.ok(text.includes("https://otro-conector.example"));
    }
    assert.equal((await handler.fetch(new Request("https://otro-conector.example/api/handler?__path=tasks"))).status, 401);
  } finally {
    if (originalKey === undefined) delete process.env.MUSE_API_KEY_SHA256;
    else process.env.MUSE_API_KEY_SHA256 = originalKey;
  }
});

test("updates: versiones estables entre instancias, tareas nuevas y contenido modificado", async () => {
  const tasks = [normalizeTask({ idActividad: 1, titulo: "Tarea", idEstado: 0 }, course, now)];
  const units = [{ id: "10", title: "Unidad", elements: [{ id: "20", title: "Lectura", text: "original", links: [{ type: "video", url: "https://example.org/video" }], files: [{ id: "c123-20-30", name: "apuntes.docx" }] }] }];
  const client = { ...fakeClient(tasks), async content() { return units; } };
  const read = async (since) => {
    let result;
    await withServer(client, async (base, key) => {
      assert.equal((await fetch(base + "/updates")).status, 401);
      const headers = { Authorization: "Bearer " + key };
      assert.equal((await fetch(base + "/updates?since=mal", { headers })).status, 400);
      const r = await fetch(base + "/updates" + (since ? "?since=" + since : ""), { headers });
      assert.equal(r.status, 200);
      result = await r.json();
    });
    return result;
  };
  const first = await read();
  assert.equal(first.baseline, true);
  assert.equal(first.items.length, 3);
  const resource = first.items.find((i) => i.kind === "resource");
  assert.match(resource.attachments[0].text_url, /\/files\/c123-20-30$/);
  assert.equal(resource.attachments[0].readable_as_text, true);
  assert.equal(resource.links[0].type, "video");
  assert.ok(first.version);
  const same = await read(first.version);
  assert.equal(same.changed, false);
  assert.equal(same.items, undefined);
  tasks.push(normalizeTask({ idActividad: 2, titulo: "Nueva", idEstado: 0 }, course, now));
  const added = await read(first.version);
  assert.equal(added.changed, true);
  assert.equal(added.items.filter((i) => i.kind === "task").length, 2);
  units[0].elements[0].text = "modificado";
  const edited = await read(added.version);
  assert.notEqual(edited.version, added.version);
  assert.notEqual(edited.items.find((i) => i.kind === "resource").version, added.items.find((i) => i.kind === "resource").version);
  client.content = async () => { throw new Error("no disponible"); };
  const partial = await read(edited.version);
  assert.equal(partial.partial, true);
  assert.equal(partial.version, null);
  assert.ok(partial.failed_courses.includes(course.id));
});

test("content: una unidad fallida no parece una unidad vacía", async () => {
  const client = new EminusClient({ getCredentials: async () => ({ user: "test", password: FAKE_PASSWORD }) });
  client.get = async (path) => {
    if (path.includes("getUnidades")) return { contenido: [{ idUnidad: 10 }] };
    throw new Error("falló la unidad");
  };
  await assert.rejects(client.content("123"), /falló la unidad/);
});
