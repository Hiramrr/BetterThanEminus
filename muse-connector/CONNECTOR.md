# Conector Eminus → Muse (Miyu)

Conector de **solo lectura** que deja a Miyu (Muse) ver tus tareas de Eminus (Universidad Veracruzana) y avisarte de ellas. Muse se autentica con una **API key propia del conector**, nunca con tu contraseña. Está desplegado en **Vercel**, así que funciona aunque tu Mac esté apagada.

```
Muse / Miyu ──HTTPS + API key──▶ {{BASE_URL}} ──JWT de 2 h──▶ eminus.uv.mx
                                  │
                                  └─ credenciales de Eminus: variable cifrada «sensitive» de Vercel
```

## Por qué hace falta un backend propio

Esto se extrajo de la app de Swift (`EminusGlassMac/Sources/Core/EminusAuth.swift` y `EminusAPI.swift`) y se confirmó con llamadas reales el 2026-11-05:

| | Eminus |
|---|---|
| Inicio de sesión | `POST https://eminus.uv.mx/eminusapi/api/auth` con `{"username": "<usuario>", "password": "<contraseña cifrada>"}`. La contraseña se cifra como lo hace el formulario oficial: `base64(sal16 ‖ iv16 ‖ AES-256-CBC/PKCS7)`, con la llave derivada por PBKDF2-HMAC-SHA1 (100 vueltas) a partir de la clave pública del protocolo |
| Sesión | La respuesta trae `accessToken`, un JWT que dura **2 horas**. No hay API key, OAuth ni refresh token confirmados |
| Consultas | `Authorization: Bearer <accessToken>` |

Eminus solo acepta usuario y contraseña, y Muse no acepta ni contraseña ni sesión. El conector resuelve esto: entra a Eminus del lado del servidor y le muestra a Muse una API key fija con permiso de solo lectura.

## Ficha para Muse

| Campo | Valor |
|---|---|
| **Slug del proveedor** | `eminus-uv` |
| **Nombre** | Eminus (Universidad Veracruzana): tareas |
| **Host de la API (para Muse)** | `{{BASE_URL}}` |
| **Host del proveedor (lo usa el conector, no Muse)** | `eminus.uv.mx`: `/eminusapi/api` (auth) y `/eminusapi8/api` (datos) |
| **Esquema de autenticación** | API key (bearer estático) |
| **Dónde va la clave** | Cabecera `Authorization: Bearer <API_KEY>`. Alternativa: cabecera `X-API-Key: <API_KEY>`. Nunca en la URL |
| **Formato de la clave** | `mec_` seguido de 43 caracteres base64url. El servidor guarda solo su SHA-256 |
| **Permisos** | Solo `GET`. Cualquier otro método responde `405` |
| **Formato** | JSON UTF-8. Fechas en ISO 8601 UTC (`Z`). Zona horaria del usuario: `America/Mexico_City` |

## Consultar novedades con menos lecturas

`GET /updates` usa la misma API key. Resume tareas de los cursos vigentes y unidades y recursos del módulo Contenido. No descarga PDFs ni incluye sus textos.

1. Primera consulta sin parámetros. Guarda `version` y los `items` como referencia inicial. No anuncies todo lo existente como nuevo.
2. Cada 30 minutos pide `GET /updates?since=<version guardada>`.
3. Si `changed` es `false`, no hay cambios en los datos observados. La respuesta omite `items`.
4. Si `changed` es `true`, compara cada par `kind` e `id` con la referencia. Un identificador ausente antes es nuevo. Un identificador conocido con otra `version` cambió. Cada elemento trae `title`, `course` y `url` para consultar el detalle. Una tarea también trae su fecha y estado. Un recurso trae su número de archivos.
5. Tras procesar los cambios, guarda la versión y la referencia nuevas. Usa `kind:id:version` para evitar notificar dos veces el mismo cambio.

Si `partial` o `stale` son `true`, `version` es `null`. Conserva la referencia anterior y reintenta después. `failed_courses` indica qué cursos no se pudieron leer. No anuncies eliminaciones basándote en elementos ausentes.

`baseline: true` identifica una petición sin `since`. `poll_after_seconds` recomienda el intervalo. No hay paginación en este resumen.

Los avisos de vencimiento siguen las reglas de `AVISOS.md`. Evalúalos con las fechas guardadas cada 30 minutos, aunque `changed` sea `false`. Si llega la fecha y no puedes confirmar si la tarea se entregó, consulta `/tasks` antes de avisar.

La caché de tareas dura 5 minutos y la de contenido, 10 minutos. Detecta tareas nuevas y cambios en los campos de su lista, además de cambios en títulos, texto y metadatos de recursos. No detecta un cambio exclusivo del enunciado o adjuntos de una tarea, ni una sustitución del binario de un archivo que conserve sus metadatos. No marca recursos como leídos. No envía avisos por sí mismo.

## Endpoints verificados

Todos se probaron con llamadas reales a `{{BASE_URL}}` el 2026-11-05 (`node scripts/test-read.mjs --server …`): 5 cursos vigentes y 38 tareas.

### `GET /health`
Sin autenticación. Responde `{"status":"ok"}` y no expone datos.

### `GET /tasks`
Lista de tareas de los cursos vigentes.

| Parámetro | Tipo | Por defecto | Descripción |
|---|---|---|---|
| `status` | `open` \| `pending` \| `overdue` \| `upcoming` \| `submitted` \| `reviewed` \| `all` | `open` | `open` = `pending` + `overdue` + `upcoming` (lo que falta entregar) |
| `course_id` | número | (todos) | Filtra por curso |
| `due_after`, `due_before` | ISO 8601 | (sin límite) | Filtra por fecha límite. Excluye las tareas sin fecha |
| `limit` | 1–200 | 50 | Tamaño de página |
| `cursor` | texto opaco | (ninguno) | El valor de `next_cursor` de la página anterior |
| `include` | `description` | (ninguno) | Agrega `description` y `attachments` a cada tarea. Hace una llamada extra a Eminus por tarea, así que exige `limit` ≤ 20 |

Respuesta: ver [`examples/tasks.sample.json`](examples/tasks.sample.json).

```json
{ "data": [ /* tareas */ ], "total": 38, "next_cursor": "eyJvIjo1MH0", "fetched_at": "…", "stale": false, "partial": false }
```

### `GET /tasks/{id}`
Una tarea, siempre con `description` (texto plano) y `attachments`: los archivos de las instrucciones **con su texto ya incluido** (campo `text`; ver «Objeto archivo»). Su entrega está en `GET /tasks/{id}/submission`. `{id}` tiene la forma `<course_id>-<activity_id>`. Ver [`examples/task.sample.json`](examples/task.sample.json).

### `GET /courses`
Cursos vigentes: `id`, `name`, `starts_at`, `ends_at`, `current`, `favorite`.

## Contenido y archivos (para estudiar)

Verificados con llamadas reales a `{{BASE_URL}}` el 2026-11-05: el texto de PDFs de contenido y de adjuntos de tareas, notebooks `.ipynb`, CSV y descargas binarias.

### `GET /courses?scope=all`
Todos los cursos no archivados, incluidos los ya terminados. Sin `scope` solo devuelve los vigentes.

### `GET /courses/{course_id}/content`
Material que el docente publicó en el módulo **Contenido** del curso: unidades → recursos → archivos. Algunos docentes no usan este módulo y suben el material en las tareas; en ese caso `units` es `0` y los archivos están en los `attachments` de `GET /tasks/{id}`. Cada archivo trae `text_url`: haz GET a esa dirección para leer su texto.

```json
{
  "course_id": "100006",
  "units": 1,
  "files": 2,
  "fetched_at": "2026-11-06T04:10:00.000Z",
  "data": [
    {
      "id": "5001",
      "title": "Unidad 1. Introducción",
      "published_at": "2026-08-27T15:15:10.000Z",
      "elements": [
        {
          "id": "70001",
          "title": "Lectura 1",
          "text": "Texto del recurso escrito por el docente (HTML convertido a texto).",
          "published_at": "2026-08-27T15:20:00.000Z",
          "required_reading": true,
          "files": [
            { "id": "c100006-70001-80001", "name": "lectura-1.pdf", "type": "pdf", "mime": "application/pdf", "size_bytes": 259787 },
            { "id": "c100006-70001-80002", "name": "practica.ipynb", "type": "ipynb", "mime": "application/x-ipynb+json", "size_bytes": 26202 }
          ]
        }
      ]
    }
  ]
}
```

### `GET /files/{file_id}` (texto)
**Devuelve el TEXTO del archivo en JSON, no el binario.** Cada archivo del conector trae esta dirección completa en `text_url`; basta con hacer GET a `text_url`. `GET /files/{file_id}/text` es un alias.

| Parámetro | Por defecto | Descripción |
|---|---|---|
| `from` | `1` | Primera página (PDF), celda (notebook) o bloque de 20 000 caracteres (texto) |
| `to` | `from + 19` (PDF), `+49` (notebook), `+4` (texto) | Última. Como máximo 100 por petición |

Cada respuesta trae como máximo unos 120 000 caracteres. Si `next_from` no es `null`, pide `?from=<next_from>` para seguir.

```json
{
  "id": "c100006-70001-80001",
  "name": "lectura-1.pdf",
  "type": "pdf",
  "size_bytes": 259787,
  "format": "pdf",
  "total_pages": 21,
  "from": 1,
  "to": 20,
  "pages": [ { "page": 1, "text": "Texto de la página 1…" } ],
  "has_text": true,
  "truncated": false,
  "next_from": 21
}
```

- **PDF**: texto por página. `has_text: false` indica un PDF escaneado (imágenes); en ese caso usa `download_url` si puedes leer imágenes o PDFs.
- **`.ipynb`**: celdas de markdown y de código (estas en bloques de código ```` ```python ````), sin las salidas.
- **Texto** (`csv`, `txt`, `md`, `py`, `json`, `html`, `tex`, `sql`…): tal cual, en bloques de 20 000 caracteres.
- **Otros** (`docx`, `pptx`, `xlsx`, imágenes, `zip`): `415 unsupported_type`; usa `download_url`.

### `GET /files/{file_id}/download`
El archivo original en binario (`download_url`), con su `Content-Type` y su nombre en `Content-Disposition`. Límite de **4 MB**: si pesa más, responde `413 file_too_large`; lee el texto con `text_url`.

### Objeto archivo
Todos los archivos (de contenido, de tareas y de entregas) tienen esta forma:

```json
{
  "id": "a100006-900000-425166",
  "name": "instrucciones.pdf",
  "type": "pdf",
  "mime": "application/pdf",
  "size_bytes": 603138,
  "readable_as_text": true,
  "text_url": "{{BASE_URL}}/files/a100006-900000-425166",
  "download_url": "{{BASE_URL}}/files/a100006-900000-425166/download"
}
```

En `GET /tasks/{id}` y en `GET /tasks/{id}/submission` los archivos además **ya traen su texto**:

| Campo | Descripción |
|---|---|
| `text` | Texto de las primeras 30 páginas (máximo 60 000 caracteres). En los PDF, cada página empieza con `[página N]` |
| `text_status` | `complete`, `partial` (hay más: pide `text_url?from=<next_from>`), `no_text_layer` (PDF escaneado), `unsupported_type` o `unreadable` |
| `total_pages`, `next_from` | Para seguir leyendo |

Usa `?include=none` si solo quieres la lista sin texto.

### Identificadores de archivo
- `c<curso>-<recurso>-<archivo>`: archivo del módulo Contenido.
- `a<curso>-<actividad>-<archivo>`: archivo adjunto a las instrucciones de una tarea.
- `e<curso>-<actividad>-<archivo>`: archivo que **el usuario subió** en su entrega.
- `r<curso>-<actividad>-<archivo>`: archivo de revisión que devolvió el docente.

Solo se aceptan archivos de cursos de el usuario; cualquier otro responde `404`. Descargar un archivo cuenta como una descarga en Eminus, igual que abrirlo desde la página.

### `GET /tasks/{id}/submission`
**La entrega de el usuario** para esa tarea: lo que escribió, los archivos que subió (con su texto) y la revisión del docente. `data` es `null` si no entregó nada.

```json
{
  "task_id": "100006-900002",
  "title": "Actividad de ejemplo 3",
  "course": "MATERIA G",
  "data": {
    "id": "5550001",
    "status": "submitted",
    "submitted_at": "2026-08-18T14:19:42.000Z",
    "text": "Texto que el usuario escribió al entregar.",
    "team_id": null,
    "files": [
      { "id": "e100006-900002-777001", "name": "mi-entrega.pdf", "type": "pdf", "size_bytes": 410000,
        "readable_as_text": true, "text_url": "{{BASE_URL}}/files/e100006-900002-777001",
        "download_url": "{{BASE_URL}}/files/e100006-900002-777001/download",
        "text": "[página 1]\nContenido de la entrega…", "text_status": "complete", "total_pages": 4, "next_from": null }
    ],
    "review": { "reviewed_at": null, "grade": null, "feedback": null, "reviewer": "Nombre del docente", "files": [] }
  }
}
```

`status`: `submitted` (entregada, sin revisar), `reviewed` (revisada) o `draft` (borrador guardado sin enviar). `review.feedback` es el comentario del docente y `review.files` los archivos que devolvió.

## Campos de cada tarea

| Campo | Tipo | Origen en Eminus | Notas |
|---|---|---|---|
| `id` | string | `idCurso` + `-` + `idActividad` | Estable. Úsalo para no repetir avisos |
| `activity_id`, `course_id` | string | `idActividad`, `idCurso` | |
| `title` | string | `titulo` | |
| `course` | string | `curso.nombre` | Nombre de la materia |
| `status` | enum | `idEstado` / `estadoEntrega` + fechas | `pending`, `overdue` (venció sin entregar), `upcoming` (todavía no abre), `submitted`, `reviewed` |
| `submitted` | bool | `idEstado > 0` | Igual que EminusGlass: 0/null = sin entregar, 1 = entregada, 3 = revisada |
| `opens_at` | ISO \| null | `fechaInicioEpoch` | |
| `due_at` | ISO \| null | `fechaProrrogaEpoch` ?? `fechaTerminoEpoch` | Fecha límite efectiva, ya con la prórroga |
| `original_due_at` | ISO \| null | `fechaTerminoEpoch` | |
| `has_extension` | bool | | `true` si hay prórroga distinta de la fecha original |
| `submitted_at`, `reviewed_at` | ISO \| null | `fechaEntregaEpoch`, `fechaRevisionEpoch` | |
| `grade`, `max_grade` | número \| null | `calificacion`, `valor` | `0` es una nota válida |
| `description` | string \| null | `descripcion` (HTML → texto) | Solo con `include=description` o en `/tasks/{id}` |
| `attachments` | arreglo | `archivosActsConTamano` | Solo con `include=description` o en `/tasks/{id}`. Archivos de las instrucciones; en `/tasks/{id}` ya traen su `text` |
| `url` | string | | `https://eminus.uv.mx/eminus4/page/course/list`. Eminus no tiene enlaces directos verificados a cada actividad |

Las tareas con `visible = 0` y los cursos archivados o ya terminados no aparecen.

## Paginación

Se pagina con cursor. Para recorrer todo, repite la petición con `cursor=<next_cursor>` hasta que `next_cursor` sea `null`. El orden es estable: por `due_at` ascendente (las tareas sin fecha van al final) y después por `id`. `total` es el número de tareas después de aplicar los filtros.

## Caché y frescura

- Cada instancia consulta Eminus como máximo **una vez cada 5 minutos** (`CACHE_TTL_SECONDS`) y reutiliza el JWT mientras sigue vigente. Con caché, una consulta tarda unos 0,3 s. Una instancia en frío inicia sesión y lee los cursos, lo que tarda unos 8 a 10 s; la función admite hasta 60 s, así que conviene configurar en Muse un timeout de 30 s o más.
- Si Eminus falla, la instancia sirve su última copia (de hasta 6 h) con `"stale": true`.
- `"partial": true` indica que algún curso no respondió. Sus tareas pueden faltar en esa lectura, pero no significa que se hayan borrado.
- Para avisos basta con consultar cada **30 minutos**.

## Errores

Todos los errores tienen la forma `{"error": {"code": "…", "message": "…"}}`. Nunca incluyen la contraseña, el token ni el cuerpo que responde Eminus.

| HTTP | `code` | Qué significa | Qué debe hacer Miyu |
|---|---|---|---|
| 400 | `invalid_parameter`, `invalid_cursor` | Parámetro mal formado | Corregir la petición |
| 401 | `unauthorized` | Falta la API key o es inválida | No reintentar. Pedir que se revise la credencial del conector |
| 404 | `not_found` | Ruta, tarea, curso o archivo inexistente, o de otra persona | |
| 413 | `file_too_large` | El binario pesa más de 4 MB | Leer el texto con `text_url` |
| 415 | `unsupported_type` | Ese tipo de archivo no se puede convertir a texto | Usar `download_url` |
| 422 | `unreadable_file` | El archivo está dañado o protegido | Avisar a el usuario |
| 405 | `method_not_allowed` | El conector es de solo lectura | |
| 429 | `rate_limited` | Más de 60 peticiones por minuto | Esperar lo que indique `Retry-After` |
| 502 | `upstream_error`, `signed_out`, `forbidden` | Eminus respondió mal | Reintentar más tarde |
| 503 | `credentials_rejected` | Eminus rechazó la contraseña guardada. El conector deja de intentar 30 min para no bloquear la cuenta | Avisar a el usuario: «actualiza tu contraseña con `npm run instalar -- --credentials`» |
| 503 | `password_change_required` | Eminus pide cambiar la contraseña | Avisar a el usuario |
| 503 | `too_many_requests` | Eminus limitó los intentos de acceso | Esperar |
| 503 | `credentials_missing` | El conector no tiene credenciales de Eminus | Avisar a el usuario |
| 504 | `upstream_timeout` | Eminus tardó más de 25 s | Reintentar más tarde |

## Avisos

La lógica que debe usar Miyu está en [`AVISOS.md`](AVISOS.md). La implementación de referencia está en `src/alerts.mjs` y tiene pruebas.

---

## Conexión segura (paso a paso)

La guía para Windows, macOS y Linux está en [README.md](README.md). Cada usuario necesita su propio proyecto de Vercel, sus credenciales de Eminus y su propia API key de Muse.

Ejecuta `npm run instalar` para configurar y desplegar. Usa `npm run instalar -- --credentials` si cambiaste tu contraseña y `npm run instalar -- --rotate-key` para cambiar la API key. Si instalaste desde el botón, cambia las variables en el panel de Vercel y vuelve a desplegar.

En Muse, configura tu URL y `Authorization: Bearer <API key>` en el formulario de conectores. Las instrucciones públicas están en `/docs`.

### Pruebas

```sh
npm test                                                        # pruebas unitarias, sin red
node scripts/test-read.mjs                                      # lectura real directa a Eminus
MUSE_API_KEY="$(security find-generic-password -s mx.hiram.muse-connector -a muse-api-key -w)" \
  node scripts/test-read.mjs --server {{BASE_URL}}   # como Muse
node scripts/alerts-dry-run.mjs                                 # simula avisos con datos reales, sin enviar nada
```

### Opcional: correrlo en tu Mac

Hace lo mismo sin la nube, pero solo funciona con la Mac encendida: `npm run setup` (te da una llave local), luego `npm start` (en `127.0.0.1:8787`) y publícalo con `tailscale funnel --bg 8787`. Para que arranque solo, usa `examples/mx.hiram.muse-connector.plist`.

### Variables (las secretas solo como variables cifradas o en el Llavero)

| Variable | Dónde | Uso |
|---|---|---|
| `EMINUS_CREDENTIALS` | Vercel, sensitive | JSON `{"user","password"}` de Eminus |
| `MUSE_API_KEY_SHA256` | Vercel, sensitive | Hash de la API key |
| `CACHE_TTL_SECONDS` | opcional | Cada cuánto se vuelve a consultar Eminus (300) |
| `HOST`, `PORT`, `EMINUS_CREDENTIALS_FILE` | solo modo local | Ver `src/local.mjs` y `src/credentials.mjs` |

### Archivos

| Archivo | Qué es |
|---|---|
| `api/handler.mjs`, `vercel.json` | Función de Vercel |
| `src/api.mjs` | Rutas, API key, paginación y errores (Request/Response estándar) |
| `src/eminus.mjs` | Cliente de Eminus: login cifrado, lectura y normalización |
| `src/http.mjs`, `src/godaddy-r1.mjs` | HTTPS hacia `eminus.uv.mx` con la raíz pública de GoDaddy R1 |
| `src/alerts.mjs` | Lógica de avisos de referencia |
| `src/local.mjs`, `src/node-server.mjs`, `src/credentials.mjs` | Modo local en la Mac |
| `scripts/` | `instalar`, `setup`, `test-read`, `alerts-dry-run` |

### Garantías de seguridad

- En el repositorio no hay ninguna contraseña, token ni API key. `.gitignore` excluye `.env`, `*.secret`, `credentials*.json`, `state/` y `.vercel`.
- El instalador envía la contraseña por stdin a `vercel env add --sensitive`. En Windows y Linux muestra la API key nueva una vez en la terminal para que el usuario la guarde.
- El código no escribe logs propios. Vercel registra la ruta y el estado de cada petición, pero no las cabeceras, así que la llave nunca queda en los logs.
- El token de Eminus solo vive en memoria y se renueva solo cuando vence.
- La API key se compara en tiempo constante contra su SHA-256.
- Solo se aceptan conexiones HTTPS con HSTS; por HTTP se redirige (308).
- El cliente HTTP solo se conecta a `https://eminus.uv.mx` y verifica TLS. Se agrega la raíz pública «GoDaddy TLS Root CA - R1» (SHA-256 `25:CF:3D:A8:…:71:9F`), que firma el certificado actual de Eminus y está firmada en cruzado por «Go Daddy Root G2». El Node estándar todavía no la incluye, y hay una prueba que comprueba su huella.
- Cloudflare Workers no sirve como alternativa: su `fetch` rechaza esa cadena (HTTP 526) y no permite agregar una CA.
- Si Eminus rechaza la contraseña, la instancia no vuelve a intentar en 30 minutos, para no bloquear tu cuenta.
