# Conector Eminus → Miyu

Conector de **solo lectura** para consultar las tareas de Eminus (Universidad Veracruzana) del usuario. Muse se autentica con una **API key propia del conector**. El conector está siempre en línea en `{{BASE_URL}}` y es el que se conecta a Eminus.

> **Para Miyu:** este conector te da acceso de solo lectura a las tareas de Eminus (la plataforma de la Universidad Veracruzana) del usuario. Úsalo para responder preguntas sobre sus tareas, para avisarle según las reglas de `AVISOS.md` y para ayudarle a estudiar con el material de sus cursos según `ESTUDIO.md`. La credencial ya está configurada en el conector de Muse: **nunca le pidas al usuario claves, tokens ni su contraseña de Eminus en el chat.**

## Cómo usarlo

- **Para responder** «¿qué tengo pendiente?»: `GET /tasks` (por defecto devuelve las que faltan por entregar, ordenadas por fecha límite).
- **Para revisar avisos** cada 30 minutos: `GET /tasks?status=all&limit=200`, siguiendo `next_cursor`, y aplica `AVISOS.md`.
- **Para ver las instrucciones** de una tarea: `GET /tasks/{id}`.
- Muestra las fechas en hora de Ciudad de México (`America/Mexico_City`). Eminus suele poner las fechas límite a las 23:45 hora local, que en la API aparecen como `05:45Z` del día siguiente.
- **Para leer un PDF o cualquier archivo**: haz GET a su `text_url` (devuelve el texto en JSON). En `GET /tasks/{id}` los adjuntos ya traen su texto en el campo `text`. **No te quedes con el nombre del archivo: lee su texto.**
- **Para ver lo que el usuario entregó** (su texto y los archivos que subió, con su contenido): `GET /tasks/{id}/submission`.
- **Para estudiar** con el material de clase: `GET /courses/{id}/content` y el `text_url` de cada archivo. Ver `ESTUDIO.md`.
- Para entregar una tarea, el usuario tiene que entrar a Eminus (`url`); el conector no puede entregar nada.

## Ficha para Muse

| Campo | Valor |
|---|---|
| **Slug del proveedor** | `eminus-uv` |
| **Nombre** | Eminus (Universidad Veracruzana): tareas |
| **Host de la API (para Muse)** | `{{BASE_URL}}` |
| **Host del proveedor (lo usa el conector, no Muse)** | `eminus.uv.mx`: `/eminusapi/api` (auth) y `/eminusapi8/api` (datos) |
| **Esquema de autenticación** | API key (bearer estático) |
| **Dónde va la clave** | Cabecera `Authorization: Bearer <API_KEY>`. Alternativa: cabecera `X-API-Key: <API_KEY>`. Nunca en la URL |
| **Formato de la clave** | El instalador genera `mec_` seguido de 43 caracteres base64url y guarda su SHA-256. Con el formulario de Vercel, usa al menos 32 caracteres aleatorios en `MUSE_API_KEY` |
| **Permisos** | Solo `GET`. Cualquier otro método responde `405` |
| **Formato** | JSON UTF-8. Fechas en ISO 8601 UTC (`Z`). Zona horaria del usuario: `America/Mexico_City` |

## Endpoints verificados

Estas rutas se verificaron en el despliegue de referencia el 2026-11-05. Cada copia devuelve los cursos y tareas de su propio usuario.

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

Ejemplo real de respuesta, anonimizado:

```json
{
  "data": [
    {
      "id": "100006-900000",
      "activity_id": "900000",
      "course_id": "100006",
      "course": "MATERIA G",
      "title": "Actividad de ejemplo 1",
      "status": "pending",
      "submitted": false,
      "opens_at": "2026-11-05T06:00:00.000Z",
      "due_at": "2026-11-07T05:45:00.000Z",
      "original_due_at": "2026-11-07T05:45:00.000Z",
      "has_extension": false,
      "submitted_at": null,
      "reviewed_at": null,
      "grade": null,
      "max_grade": null,
      "description": null,
      "url": "https://eminus.uv.mx/eminus4/page/course/list"
    },
    {
      "id": "100005-900001",
      "activity_id": "900001",
      "course_id": "100005",
      "course": "MATERIA F",
      "title": "Actividad de ejemplo 2",
      "status": "overdue",
      "submitted": false,
      "opens_at": "2026-10-05T06:00:00.000Z",
      "due_at": "2026-10-06T05:45:00.000Z",
      "original_due_at": "2026-10-06T05:45:00.000Z",
      "has_extension": false,
      "submitted_at": null,
      "reviewed_at": null,
      "grade": null,
      "max_grade": 1,
      "description": null,
      "url": "https://eminus.uv.mx/eminus4/page/course/list"
    },
    {
      "id": "100006-900002",
      "activity_id": "900002",
      "course_id": "100006",
      "course": "MATERIA G",
      "title": "Actividad de ejemplo 3",
      "status": "submitted",
      "submitted": true,
      "opens_at": "2026-08-18T06:00:00.000Z",
      "due_at": "2026-08-18T14:30:00.000Z",
      "original_due_at": "2026-08-18T14:30:00.000Z",
      "has_extension": false,
      "submitted_at": "2026-08-18T14:19:42.000Z",
      "reviewed_at": null,
      "grade": null,
      "max_grade": 1,
      "description": null,
      "url": "https://eminus.uv.mx/eminus4/page/course/list"
    }
  ],
  "total": 38,
  "next_cursor": "eyJvIjozfQ",
  "fetched_at": "2026-11-06T03:46:35.056Z",
  "stale": false,
  "partial": false
}
```


### `GET /tasks/{id}`
Una tarea, siempre con `description` (texto plano) y `attachments`: los archivos de las instrucciones **con su texto ya incluido** (campo `text`; ver «Objeto archivo»). Su entrega está en `GET /tasks/{id}/submission`. `{id}` tiene la forma `<course_id>-<activity_id>`. Ejemplo:

```json
{
  "data": {
    "id": "100006-900000",
    "activity_id": "900000",
    "course_id": "100006",
    "course": "MATERIA G",
    "title": "Actividad de ejemplo 1",
    "status": "pending",
    "submitted": false,
    "opens_at": "2026-11-05T06:00:00.000Z",
    "due_at": "2026-11-07T05:45:00.000Z",
    "original_due_at": "2026-11-07T05:45:00.000Z",
    "has_extension": false,
    "submitted_at": null,
    "reviewed_at": null,
    "grade": null,
    "max_grade": null,
    "description": "Texto plano de las instrucciones del docente (el HTML de Eminus se convierte a texto).",
    "attachments": ["archivo-de-ejemplo.pdf"],
    "url": "https://eminus.uv.mx/eminus4/page/course/list"
  },
  "fetched_at": "2026-11-06T03:46:35.056Z",
  "stale": false
}
```

### `GET /courses`
Cursos vigentes: `id`, `name`, `starts_at`, `ends_at`, `current`, `favorite`.

## Contenido y archivos (para estudiar)

Verificados en el despliegue de referencia el 2026-11-05: el texto de PDFs de contenido y de adjuntos de tareas, notebooks `.ipynb`, CSV y descargas binarias.

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

Solo se aceptan archivos de cursos del usuario; cualquier otro responde `404`. Descargar un archivo cuenta como una descarga en Eminus, igual que abrirlo desde la página.

### `GET /tasks/{id}/submission`
**La entrega del usuario** para esa tarea: lo que escribió, los archivos que subió (con su texto) y la revisión del docente. `data` es `null` si no entregó nada.

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
| 422 | `unreadable_file` | El archivo está dañado o protegido | Avisar al usuario |
| 405 | `method_not_allowed` | El conector es de solo lectura | |
| 429 | `rate_limited` | Más de 60 peticiones por minuto | Esperar lo que indique `Retry-After` |
| 502 | `upstream_error`, `signed_out`, `forbidden` | Eminus respondió mal | Reintentar más tarde |
| 503 | `credentials_rejected` | Eminus rechazó la contraseña guardada. El conector deja de intentar 30 min para no bloquear la cuenta | Avisar al usuario: «Eminus rechazó tu contraseña; actualízala en el conector» |
| 503 | `password_change_required` | Eminus pide cambiar la contraseña | Avisar al usuario |
| 503 | `too_many_requests` | Eminus limitó los intentos de acceso | Esperar |
| 503 | `credentials_missing` | El conector no tiene credenciales de Eminus | Avisar al usuario |
| 504 | `upstream_timeout` | Eminus tardó más de 25 s | Reintentar más tarde |

## Avisos

Las reglas para avisar están en `AVISOS.md`.
