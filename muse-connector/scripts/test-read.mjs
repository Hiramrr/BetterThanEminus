#!/usr/bin/env node
// Prueba de lectura real. Sólo hace GET (y el POST de inicio de sesión de Eminus);
// no escribe nada en Eminus ni en disco.
//
//   node scripts/test-read.mjs                       directo contra Eminus (credenciales del Llavero)
//   node scripts/test-read.mjs --sanitize            igual, pero imprime JSON anonimizado como GET /tasks
//   node scripts/test-read.mjs --server URL          contra el conector, como lo haría Muse
//        La API key se pide sin eco (o se toma de MUSE_API_KEY en el entorno).
import { eminusFetch } from "../src/http.mjs";
import readline from "node:readline";
import assert from "node:assert/strict";
import { EminusClient } from "../src/eminus.mjs";
import { loadEminusCredentials } from "../src/credentials.mjs";

const argv = process.argv.slice(2);
const serverIdx = argv.indexOf("--server");
const server = serverIdx >= 0 ? argv[serverIdx + 1] : null;

function hiddenPrompt(q) {
  return new Promise((resolve) => {
    const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: true });
    rl._writeToOutput = (s) => { if (!rl.muted) rl.output.write(s); };
    rl.question(q, (a) => { rl.close(); process.stdout.write("\n"); resolve(a.trim()); });
    rl.muted = true;
  });
}

function summary(tasks) {
  const by = {};
  for (const t of tasks) by[t.status] = (by[t.status] || 0) + 1;
  return by;
}

function sanitize(task, i) {
  const fakeCourse = 100000 + (Number(task.course_id) % 7);
  const fakeAct = 900000 + i;
  return { ...task, id: `${fakeCourse}-${fakeAct}`, activity_id: String(fakeAct), course_id: String(fakeCourse), course: "MATERIA " + String.fromCharCode(65 + (Number(task.course_id) % 7)), title: "Actividad de ejemplo " + (i + 1) };
}

async function viaServer(base) {
  const key = process.env.MUSE_API_KEY || (await hiddenPrompt("API key de Muse (no se muestra): "));
  const get = async (path, auth = true) => {
    const res = await fetch(new URL(path, base), { headers: auth ? { Authorization: "Bearer " + key } : {} });
    return { status: res.status, body: await res.json().catch(() => null) };
  };
  const health = await get("/health", false);
  console.log("GET /health →", health.status, JSON.stringify(health.body));
  const noKey = await get("/tasks", false);
  console.log("GET /tasks sin llave →", noKey.status, noKey.body?.error?.code);
  const page = await get("/tasks?status=all&limit=5");
  console.log("GET /tasks?status=all&limit=5 →", page.status);
  if (page.status !== 200) return console.log(JSON.stringify(page.body));
  console.log("  total:", page.body.total, "· en la página:", page.body.data.length, "· next_cursor:", page.body.next_cursor ? "sí" : "no", "· stale:", page.body.stale);
  const open = await get("/tasks");
  console.log("GET /tasks (abiertas) →", open.status, "total:", open.body?.total, summary(open.body?.data || []));
  const first = open.body?.data?.[0] || page.body.data[0];
  if (first) {
    const one = await get("/tasks/" + encodeURIComponent(first.id));
    console.log(`GET /tasks/{id} →`, one.status, "· descripción:", one.body?.data?.description ? one.body.data.description.length + " caracteres" : "vacía");
  }
  const courses = await get("/courses");
  console.log("GET /courses →", courses.status, "cursos:", courses.body?.data?.length);
  const updates = await get("/updates");
  assert.equal(updates.status, 200, "lectura de novedades");
  assert.equal(updates.body.partial, false, "novedades completas");
  assert.ok(updates.body.version, "versión de referencia");
  console.log("GET /updates →", updates.status, "elementos:", updates.body.items.length);
  const unchanged = await get("/updates?since=" + updates.body.version);
  assert.equal(unchanged.status, 200);
  assert.equal(unchanged.body.changed, false, "sin cambios entre lecturas consecutivas");
  console.log("GET /updates?since=… →", unchanged.status, "changed:", unchanged.body.changed);
}

async function direct() {
  const creds = await loadEminusCredentials();
  console.error(`credenciales: ${creds.source} (usuario ****${creds.user.slice(-3)})`);
  const client = new EminusClient({ fetch: eminusFetch, getCredentials: async () => creds });
  const { courses, tasks, failedCourses } = await client.tasks();
  if (argv.includes("--sanitize")) {
    const pick = ["pending", "overdue", "upcoming", "submitted", "reviewed"].map((st) => tasks.find((t) => t.status === st)).filter(Boolean);
    const sample = pick.map(sanitize);
    if (sample[0]) {
      const d = await client.description(pick[0].course_id, pick[0].activity_id).catch(() => null);
      sample[0].description = d?.text ? "Texto de las instrucciones (" + d.text.length + " caracteres en la tarea real)." : null;
    }
    console.log(JSON.stringify({ data: sample, total: tasks.length, next_cursor: null, fetched_at: new Date().toISOString(), stale: false, partial: failedCourses.length > 0 }, null, 2));
    return;
  }
  console.log("cursos vigentes:", courses.filter((c) => c.current).length, "· tareas:", tasks.length, "· cursos con error:", failedCourses.length);
  console.log("por estado:", summary(tasks));
  const next = tasks.filter((t) => t.status === "pending").slice(0, 5);
  for (const t of next) console.log(`  ${t.due_at}  ${t.course} · ${t.title}`);
}

try {
  if (server) await viaServer(server);
  else await direct();
} catch (err) {
  console.error("fallo:", err.code || "", err.message);
  process.exit(1);
}
