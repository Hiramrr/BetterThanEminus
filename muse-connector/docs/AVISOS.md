# Lógica de avisos para Miyu

Reglas para avisarle al usuario de sus tareas de Eminus usando el conector `eminus-uv` (ver `CONNECTOR.md`). Hay una implementación de referencia probada (`computeAlerts` en `src/alerts.mjs` del repositorio del conector).

## Consultar novedades

Puedes usar `/updates?since=<version>` para detectar tareas y contenido nuevos sin volver a leer todos los detalles. Guarda la referencia inicial y compara `kind`, `id` y `version` cuando haya cambios. Ver `CONNECTOR.md`. Los avisos por fecha requieren evaluar el reloj aunque no haya cambios. Conserva la deduplicación de avisos descrita aquí.

## Ciclo

Cada **30 minutos**:

1. `GET /tasks?status=all&limit=200`, siguiendo `next_cursor` hasta que sea `null`. Se usa `all`, y no `open`, para saber también qué tareas ya se entregaron.
2. Si la respuesta trae `stale: true` o el servidor responde con un error 5xx, **no envíes avisos de «nueva»** en esa corrida; los de fecha sí se pueden enviar.
3. Calcula los avisos con las reglas de abajo, envíalos y **después** guarda el estado.

## Estado que debes recordar

```json
{
  "version": 1,
  "initialized": true,
  "tasks": {
    "<task.id>": {
      "first_seen": "2026-11-05T12:00:00Z",
      "last_seen":  "2026-11-06T03:00:00Z",
      "sent": { "new": true, "due_soon": "<due_at con el que se avisó>", "overdue": "<due_at con el que se avisó>" }
    }
  }
}
```

La clave es `task.id` (`<course_id>-<activity_id>`), que es estable en Eminus.

## Reglas

Una tarea está **abierta** cuando `submitted == false` y su `status` es `pending`, `overdue` o `upcoming`.

| Aviso | Condición | Marca para no repetir |
|---|---|---|
| **Nueva tarea** | `task.id` no está en el estado, la tarea está abierta y **no** es la primera corrida | `sent.new = true` |
| **Falta 1 día** | Abierta y `0 < due_at − ahora ≤ 24 h` | `sent.due_soon = due_at` |
| **Vencida sin entregar** | Abierta y `0 ≤ ahora − due_at ≤ 7 días` | `sent.overdue = due_at` |

### Evitar avisos duplicados

- Antes de avisar, compara la marca con el `due_at` **actual**. Si el docente da una prórroga, `due_at` cambia y la tarea puede volver a recibir «falta 1 día» o «vencida». Si la fecha no cambia, nunca se repite el aviso.
- **Primera corrida** (`initialized` ausente): registra todas las tareas como ya vistas (`sent.new = true`) sin avisar «nueva»; así no recibes 40 avisos de golpe. Los de «falta 1 día» y «vencida» sí se envían.
- Si una tarea es nueva **y** además vence en menos de 24 h, se manda **un solo** mensaje que combina los dos avisos (`kinds: ["new", "due_soon"]`).
- Las tareas entregadas (`submitted == true`) nunca generan avisos.
- No se avisan las vencidas con más de 7 días de atraso. Siguen apareciendo en `GET /tasks?status=overdue` por si se las preguntas.
- Si una tarea deja de aparecer (por ejemplo, porque un curso falló y `partial` es `true`), su registro se conserva 60 días antes de olvidarlo. Así, cuando reaparezca, no se avisará otra vez como «nueva».
- El identificador del aviso es `"<task.id>:<kinds>:<due_at>"`. Si Muse permite marcar mensajes como enviados, usa ese valor como clave.

## Texto sugerido

Mensajes de ejemplo (puedes reescribirlos con tu propio tono):

- «Nueva tarea: «Actividad de ejemplo 1» (MATERIA G). Fecha límite: viernes, 6 de noviembre, 11:45 p.m.»
- «Vence en menos de 24 h: …»
- «Venció sin entregar: …»
- «Nueva tarea · vence en menos de 24 h: …»

Muestra las fechas en `America/Mexico_City`. En Eminus, el «23:45 hora local» llega como `05:45Z` del día siguiente.
