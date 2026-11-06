// Lógica de avisos de referencia para Miyu. Es una función pura: recibe las tareas
// de GET /tasks?status=all, el estado guardado de la corrida anterior y la hora actual,
// y devuelve los avisos que hay que enviar y el estado nuevo. Ver AVISOS.md.

export const DAY = 24 * 3600 * 1000;
export const DEFAULTS = {
  dueSoonWindowMs: DAY, // «falta 1 día»
  overdueGraceMs: 7 * DAY, // no avisar vencidas con más de 7 días
  forgetAfterMs: 60 * DAY // olvidar tareas que ya no aparecen
};

const OPEN = new Set(["pending", "overdue", "upcoming"]);

/**
 * @param {Array<object>} tasks  tareas tal como las devuelve GET /tasks?status=all
 * @param {{ version?: number, initialized?: boolean, tasks?: Record<string, any> }} state
 * @param {number} now  milisegundos epoch
 * @returns {{ alerts: Array<object>, state: object }}
 */
export function computeAlerts(tasks, state = {}, now = Date.now(), opts = {}) {
  const o = { ...DEFAULTS, ...opts };
  const prev = state.tasks || {};
  const next = {};
  const alerts = [];
  const firstRun = !state.initialized;

  for (const t of tasks) {
    const due = t.due_at ? Date.parse(t.due_at) : null;
    const dueKey = t.due_at || "none"; // si el docente da prórroga, due_at cambia y se puede volver a avisar
    const rec = { ...(prev[t.id] || { first_seen: new Date(now).toISOString(), sent: {} }), last_seen: new Date(now).toISOString() };
    rec.sent = { ...rec.sent };
    next[t.id] = rec;

    const open = OPEN.has(t.status) && !t.submitted;
    const kinds = [];

    // 1. Tarea nueva: no estaba en el estado anterior. En la primera corrida sólo se
    //    registra (línea base), para no mandar 40 avisos de golpe.
    if (!prev[t.id] && !firstRun && open) kinds.push("new");
    if (!prev[t.id]) rec.sent.new = true;

    // 2. Falta 1 día: vence dentro de las próximas 24 h y no se avisó para esta fecha.
    if (open && due !== null && due > now && due - now <= o.dueSoonWindowMs && rec.sent.due_soon !== dueKey) {
      kinds.push("due_soon");
      rec.sent.due_soon = dueKey;
    }

    // 3. Vencida sin entregar: una sola vez por fecha límite y sólo si venció hace poco.
    if (open && due !== null && due <= now && now - due <= o.overdueGraceMs && rec.sent.overdue !== dueKey) {
      kinds.push("overdue");
      rec.sent.overdue = dueKey;
    }

    // Cuando un aviso de vencida ya salió, no tiene sentido mandar «falta 1 día» después.
    if (kinds.length) {
      alerts.push({
        key: `${t.id}:${kinds.join("+")}:${dueKey}`,
        kinds,
        task_id: t.id,
        title: t.title,
        course: t.course,
        due_at: t.due_at,
        status: t.status,
        url: t.url
      });
    }
  }

  // Conservar tareas que no vinieron esta vez (un curso pudo fallar) hasta forgetAfterMs.
  for (const [taskId, rec] of Object.entries(prev)) {
    if (!next[taskId] && now - Date.parse(rec.last_seen || 0) < o.forgetAfterMs) next[taskId] = rec;
  }

  alerts.sort((a, b) => (a.due_at || "9999").localeCompare(b.due_at || "9999"));
  return { alerts, state: { version: 1, initialized: true, tasks: next } };
}

/** Texto en español para cada aviso (Miyu puede reescribirlo con su propio tono). */
export function alertText(a, timeZone = "America/Mexico_City") {
  const when = a.due_at
    ? new Date(a.due_at).toLocaleString("es-MX", { timeZone, weekday: "long", day: "numeric", month: "long", hour: "2-digit", minute: "2-digit" })
    : "sin fecha límite";
  const parts = [];
  if (a.kinds.includes("new")) parts.push("Nueva tarea");
  if (a.kinds.includes("due_soon")) parts.push("vence en menos de 24 h");
  if (a.kinds.includes("overdue")) parts.push("venció sin entregar");
  const head = parts.join(" · ");
  return `${head.charAt(0).toUpperCase() + head.slice(1)}: «${a.title}» (${a.course}). Fecha límite: ${when}${when.endsWith(".") ? "" : "."}`;
}
