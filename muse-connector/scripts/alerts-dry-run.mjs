#!/usr/bin/env node
// Simula una corrida de avisos con datos reales, sin enviar nada.
//   node scripts/alerts-dry-run.mjs            usa state/alerts-state.json (se crea; está en .gitignore)
//   node scripts/alerts-dry-run.mjs --reset    empieza de cero (primera corrida = línea base)
import { eminusFetch } from "../src/http.mjs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { alertText, computeAlerts } from "../src/alerts.mjs";
import { loadEminusCredentials } from "../src/credentials.mjs";
import { EminusClient } from "../src/eminus.mjs";

const file = new URL("../state/alerts-state.json", import.meta.url);
let state = {};
if (!process.argv.includes("--reset")) {
  try { state = JSON.parse(await readFile(file, "utf8")); } catch {}
}
const client = new EminusClient({ fetch: eminusFetch, getCredentials: loadEminusCredentials });
const { tasks } = await client.tasks();
const result = computeAlerts(tasks, state, Date.now());
console.log(state.initialized ? "corrida normal" : "primera corrida (línea base: no avisa «nueva»)");
for (const a of result.alerts) console.log("•", alertText(a));
if (!result.alerts.length) console.log("sin avisos nuevos");
await mkdir(new URL("../state/", import.meta.url), { recursive: true });
await writeFile(file, JSON.stringify(result.state, null, 2), { mode: 0o600 });
