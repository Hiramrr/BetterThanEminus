# API de Eminus

## Bases y autenticación

- API principal, `https://eminus.uv.mx/eminusapi/api`.
- API de módulos nuevos, `https://eminus.uv.mx/eminusapi8/api`.
- Imágenes, `https://eminus.uv.mx/eminusapi/`, sin el segmento `api` final.

Las consultas usan `Authorization: Bearer <token>` y `Accept: application/json`. El perfil y los integrantes devolvieron 401 sin token. Las imágenes de perfil y las miniaturas probadas devolvieron 200 sin él.

En las tablas, "Principal" y "Nueva" indican la base. La ruta se escribe a continuación de la base, por ejemplo `https://eminus.uv.mx/eminusapi8/api/Course/getAllCourses`.

## Acceso

```
POST principal /auth
Content-Type: application/json

{"username": "<usuario>", "password": "<contraseña cifrada>"}
```

La contraseña se cifra como en el formulario oficial, que usa CryptoJS:

1. Recortar espacios y saltos de línea al inicio y al final.
2. Generar 16 bytes aleatorios de sal y 16 de IV en cada intento.
3. Derivar una llave de 32 bytes con PBKDF2-HMAC-SHA1, 100 iteraciones, la sal y la clave pública del protocolo `ow7dxys8glfor9tnc2ansdfo1etkgjcv`.
4. Cifrar la contraseña en UTF-8 con AES-256-CBC y relleno PKCS7.
5. Enviar `base64(sal ‖ iv ‖ cifrado)`.

`Tests/AuthChecks.swift` incluye un vector conocido. Con la llave `llave-app`, la contraseña `Secreto-123`, la sal 0…15 y el IV 16…31, el resultado es `AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh9oZfiMRxMgnOyLHb3eoy2D`. Se comprobó también con PBKDF2 de Python y AES de OpenSSL.

La respuesta puede ser un objeto, un arreglo o un objeto con `contenido`. En un arreglo se toma el último registro, igual que el `pop()` del frontend. El registro trae `accessToken`, un JWT. Si `estadoUsuarioChecar` vale 2, Eminus exige cambiar la contraseña y la app no abre la sesión.

El estado 429 indica demasiados intentos. Cualquier otro 4xx indica usuario o contraseña incorrectos. Un 5xx es un fallo del servidor.

La app no verifica la firma del JWT. Lee `exp` para conocer el vencimiento y toma la cuenta del primer campo no vacío entre `nameid`, `unique_name`, `sub`, `email`, `idPersona` y `matricula`. Esa cuenta se usa en `/Seguimiento/SegDetalle` y para saber si un mensaje es propio.

## Rutas de lectura

| Base | Ruta GET | Respuesta y uso |
| --- | --- | --- |
| Nueva | `/Course/getAllCourses` | `contenido[]`, con un objeto `curso`. Incluye `idCurso`, `nombre`, `idUsuario`, `codigoHexa`, `ruta`, `archivado`, `fechaTerminoEpoch` y conteos |
| Nueva | `/Activity/getActividadesEstudiante/{curso}` | `contenido[]`. Actividades con `idActividad`, `titulo`, `visible`, fechas `fecha…Epoch`, `estadoEntrega`, `idEstado`, `calificacion` y `valor` |
| Nueva | `/Activity/getActividadEstudiante/{curso}/{actividad}` | `contenido[]`. Enunciado en `descripcion`, `archivosActsConTamano`, `idEquipo` e `idRubricaUtilizada` |
| Nueva | `/Activity/getActUsuarioMiRevision/{curso}/{actividad}/{equipo}/{rubrica}` | `contenido` es un objeto. Entrega, retroalimentación, nota y archivos de revisión. Usar 0 cuando equipo o rúbrica sean null |
| Principal | `/Usuario/getUsuario` | Objeto con `nombre`, `paterno`, `materno`, `correo` y `ruta` de la foto |
| Principal | `/Usuario/getIntegrantesRes/{curso}/1` | Arreglo de docentes. Perfil 1 significa Facilitador. Verificado en los cinco cursos vigentes |
| Principal | `/Usuario/getIntegrantes/{curso}` | Integrantes con nombre, `idTipoPerfil`, `tipoPerfil`, `correo`, `ruta` de la foto y `ultimoAcceso` |
| Principal | `/Cursos/obtieneCurso/{curso}` | Objeto. `propietario` es un identificador, no el nombre del docente |
| Principal | `/Contenido/getUnidades/{curso}/0` | Unidades del contenido del curso |
| Principal | `/Contenido/getElementos/{curso}/{unidad}` | Elementos de una unidad. Unidad 0 corresponde a Inicio |
| Principal | `/Contenido/getElemento/{curso}/{elemento}` | Detalle de un elemento, con `descripcion` en HTML, enlaces y `archivosElementosConTamano` |
| Principal | `/Seguimiento/SegPrincipal/{curso}` | Resumen del curso y sus estudiantes |
| Principal | `/Seguimiento/SegDetalle/{curso}/{usuario}` | Arreglo con `totalRecursos`, `totalPuntos`, fechas y `detalle` con `actividades`, `foros` y `examenes`. El usuario sale del JWT |
| Principal | `/Seguimiento/SegTablaDetalleGeneral/{curso}` | Objeto con arreglos `actividades`, `foros` y `examenes` |
| Principal | `/Examen/getExamenesEst/{curso}` | Arreglo o 204 vacío. `estado` y `estadoAvance` son campos distintos. Incluye `fechaInicio`, `fechaTermino`, `tiempo`, `calificacion` y `valor`. No se inició ningún examen |
| Principal | `/Foro/getForos/{curso}/0` | Arreglo o 204 vacío. Incluye `idForo`, `totalComentarios` y `nuevosComentarios` |
| Principal | `/Foro/getComentariosForo/{curso}/{foro}` | Comentarios del foro, con `idComentarioForo`, autor, fecha, `descripcion` y adjuntos en `archivoForoConTamano` |
| Nueva | `/MessageCenter/GetListadoMensajes/1/0` | Bandeja global. `leido` es entero 0/1, `confirmacionLectura` es un identificador o null |
| Nueva | `/MessageCenter/GetListadoMensajes/1/{curso}` | Bandeja filtrada por curso |
| Nueva | `/MessageCenter/folders/{curso}` | Carpetas en `data[]`. Omitir el curso devuelve 400 |
| Principal | `/centromensajes/GetMensajeDetalle/{mensaje}` | El primer registro es el mensaje y los siguientes son respuestas. Incluye `remitente`, `destinatarios` y adjuntos con `idArchivosMensajes` |
| Principal | `/Global/NotificacionesUsuario` | Conteos `msjNuevos`, `actNuevas`, `actActivas`, `frNuevos`, `frActivos`, `exNuevos`, `exActivos` |
| Principal | `/Global/getModulosResumen/{curso}/0/0` | Resumen por módulo, no una lista de notificaciones |
| Principal | `/SalonInteractivo/getSalonesInteractivos/{curso}/0` | Salas generales. 204 en el curso probado |
| Principal | `/SalonInteractivo/salonInteractivoTeams/{curso}` | Sesiones de Teams. 204 en el curso probado |
| Principal | `/SalonInteractivo/getSalonZoomEst/{curso}` | Sesiones de Zoom. 204 en el curso probado |
| Principal | `/SalonInteractivo/getSalonOtros/{curso}` | Otras sesiones. 204 en el curso probado |

Las listas pueden llegar como arreglo directo, dentro de `contenido` o dentro de `data`. La app acepta las tres formas.

El concentrado `/Seguimiento/SeguimientoConcentradoCal/{curso}` devolvió 403 con rol alumno. La app no debe usarlo para las notas del estudiante. Biblioteca abre `https://www.uv.mx/bvirtual/` y no tiene una API identificada bajo estas bases.

## Perfil e imágenes

`ruta` puede llegar como `\profiles\archivo.png` o `/thumbnail/usuario/archivo.png`. Sustituir barras invertidas, quitar la barra inicial y añadir la base de imágenes. La app rechaza rutas con `..` y dominios ajenos. Los cinco cursos actuales tenían `ruta = null` y `codigoHexa = "#1697A6"`.

Los nombres vienen separados. Usar `nombre`, seguido por `paterno` y `materno`, para conservar los apellidos compuestos. El identificador `curso.idUsuario` no sirve como nombre del docente. La app consulta Integrantes cuando no tiene el nombre y lo conserva en la caché.

La lista nativa de integrantes usa esos campos de una sola consulta. `ultimoAcceso` llega como `2026-10-23T01:13:46.35`, con fracciones variables o sin fracciones y sin zona horaria. Se interpreta como hora local, igual que el cliente oficial. Un null indica que no hay fecha disponible. Las fotos se descargan por separado y no retrasan la lista. Si no hay foto o Eminus devuelve `usuario_generico.svg`, se muestran las iniciales. El correo abre un enlace `mailto:` en la aplicación del sistema.

## Estados y errores

`estadoAct` indica vigencia, no entrega. En el cliente antiguo, 1 corresponde a vencida, 2 vigente, 3 por iniciar y 4 prórroga.

`idEstado` y `estadoEntrega` describen la entrega. Se verificaron 0 o null sin entregar, 1 entregada sin revisión y 3 revisada. Un borrador puede tener `fechaEntrega` con estado 0. El filtro del módulo nuevo usa `fechaEntrega != null && idEstado > 0`. Hay datos históricos inconsistentes, incluido un estado 3 sin fecha de entrega. La app prioriza el estado explícito sobre la fecha y mantiene el estado 0 como pendiente.

La fecha límite es `fechaProrrogaEpoch` cuando existe y, si no, `fechaTerminoEpoch`. Una actividad con `visible` igual a 0 o false no se muestra.

`calificacion = 0` es una nota válida. `valor` son los puntos máximos y puede ser 0. `fechaRevision` puede existir sin nota numérica, por lo que no basta para deducir una calificación.

Una respuesta 204 es una lista vacía. La lista de actividades vacía responde 404 con `{"codigo":404,"salida":"No se encontraron actividades"}`. Ese caso se convierte en arreglo vacío. Un 404 distinto conserva su error. Un 401 indica sesión vencida y activa la recuperación de sesión. Un 403 indica que el rol no tiene permiso.

Los conteos de mensajes no coinciden siempre. La bandeja tenía 15 registros con `leido = 0`, mientras el contador global decía cero. No se abrieron mensajes individuales ni se enviaron confirmaciones de lectura.

## Renovación

El frontend principal declara `/refresh`, pero su función de renovación devuelve un observable vacío. El módulo nuevo de Actividades sí implementa `POST /refresh`, con cuerpo `{}` y Bearer del access token. Se encontró en el código y no se ejecutó durante la exploración. No se ha confirmado el contrato del servidor.

Eminus usa el acceso nativo para renovar cuando la persona activa "Mantener sesión iniciada". Guarda las credenciales en el llavero y repite `POST /auth` cinco minutos antes del vencimiento del JWT. Comprueba que la cuenta no cambie y cancela la renovación al cerrar sesión. La clave pública de cifrado se incluye en la app. El acceso solo solicita `/auth`, sin descargar el sitio ni sus scripts.

## Entregar una tarea

Estos contratos se obtuvieron del código publicado por Eminus. Las pruebas usan respuestas controladas. No se ejecutaron entregas reales durante la investigación.

### 1. Consultar la actividad y la entrega actual

1. GET nueva `/Activity/getActividadEstudiante/{curso}/{actividad}`. Del primer registro se toman `idEquipo` e `idRubricaUtilizada`. Si son null se usa 0.
2. GET nueva `/Activity/getActUsuarioMiRevision/{curso}/{actividad}/{equipo}/{rubrica}`. De `contenido` se toman:
   - `idActividadEntrega` o `idActividadEntregas`, el id de la entrega. Se usa 0 si no existe.
   - `descEntrega` o `texto`, el texto entregado.
   - `descRevision`, la retroalimentación del docente.
   - `calificacion`, la nota.
   - `archivosEntregaConTamano` y `archivosRevisionConTamano`, los archivos.

Un 404 en el segundo paso indica que todavía no hay entrega. En ese caso la app empieza con id 0 y sin texto.

### 2. Guardar el borrador o entregar

```
POST nueva /Activity/InsertaActividadEntrega
Content-Type: application/json

{
  "idActividad": 123,
  "idCurso": 456,
  "idEquipo": 0,
  "idActividadEntrega": 0,
  "texto": "<p>Texto de la entrega</p>",
  "esBorrador": 1,
  "visible": true
}
```

- Los identificadores van como números.
- `idEquipo` vale 0 en las actividades individuales.
- `idActividadEntrega` vale 0 para crear la entrega o lleva el id existente para actualizarla.
- `esBorrador` vale 1 para guardar un borrador y 0 para entregar.
- `texto` es HTML. La app escapa `&`, `<` y `>`, cambia los saltos de línea por `<br>` y envuelve el resultado en `<p>`.

La respuesta trae `respuestaFechaEntrega.estado` con 200 o 201 y `respuestaFechaEntrega.salida` con el id de la entrega. Ese id se usa para subir archivos y para las llamadas siguientes.

### 3. Adjuntar archivos

```
POST nueva /Activity/adjuntarEntrega?idCurso={curso}&idActividad={actividad}&idActividadEntregas={entrega}
Content-Type: multipart/form-data; boundary=<boundary>

--<boundary>
Content-Disposition: form-data; name="files"; filename="tarea.pdf"
Content-Type: application/pdf

<bytes del archivo>
--<boundary>--
```

Se envía una petición por archivo, con el campo `files` y el nombre original. La app sustituye comillas y saltos de línea del nombre por `_`. El tipo MIME se deduce de la extensión y, si no se conoce, se usa `application/octet-stream`. La app limita cada archivo a 25 MB.

### Orden de las llamadas

- Solo texto: un `InsertaActividadEntrega` con `esBorrador` 1 o 0.
- Con archivos nuevos:
  1. `InsertaActividadEntrega` con `esBorrador: 1` para obtener el id.
  2. Un `adjuntarEntrega` por archivo.
  3. Si la persona eligió entregar, un `InsertaActividadEntrega` con el mismo id y `esBorrador: 0`.

Si una subida falla, la entrega queda como borrador y la app conserva los archivos pendientes para reintentar. Al terminar vuelve a consultar las actividades y confirma la entrega cuando `idEstado` es mayor que 0. La edición de adjuntos ya enviados no está implementada.

## Mensajes y foros

| Método y ruta | Cuerpo y resultado |
| --- | --- |
| POST principal `/centromensajes/insertaActualizaMensaje` | `{asunto,destinatarios,destinatarioscc,idMensaje,mensaje,idMensajeCarpeta,idMensajePadre}`. Una respuesta usa el asunto `RE: …`, el remitente como único destinatario, tal como llegó en `GetMensajeDetalle`, `destinatarioscc` vacío, id 0, carpeta 1 y el mensaje original como padre. Si el remitente es la propia cuenta, el destinatario es el primero de `destinatarios` que no sea ella |
| POST principal `/Foro/InsertaComentarioForo` | `{listaArchivos:[],IdComentarioForo:0,Descripcion,IdPadre:0,IdCurso,IdForo}`. Respetar las mayúsculas. `IdCurso` e `IdForo` son números |

`mensaje` y `Descripcion` usan el mismo HTML que el texto de una entrega.

En todas las escrituras, cualquier estado fuera de 2xx es un error y un 401 indica sesión vencida. Una respuesta 200 cuyo JSON tenga `codigo` o `estado` mayor o igual que 400 también se trata como error.

## Descargas

Todas las descargas son GET con `Authorization: Bearer <token>`. El nombre visible viene de `nombreOriginal`. Si falta, se usan `nombreArchivo`, `nombre` o `fileName`.

| Archivo | Ruta GET | De dónde salen los ids |
| --- | --- | --- |
| Adjunto de contenido | Principal `/contenido/descargaElementoArchivo/{curso}/{elemento}/{idArchivosElementos}` | `archivosElementosConTamano` de `getElemento`. `idElemento` viene en el archivo o en el elemento |
| Adjunto de mensaje | Principal `/centromensajes/descargaCentroMensajesArchivo/{mensaje}/{idArchivosMensajes}` | Archivos del registro de `GetMensajeDetalle` |
| Enunciado de actividad | Nueva `/Activity/descargaArchivo/{curso}/{actividad}/{idArchivosActividades}/2` | `archivosActsConTamano` de `getActividadEstudiante` |
| Archivo entregado | Nueva `/Activity/descargaArchivo/{curso}/{actividad}/{idArchivosEntregas}/1` | `archivosEntregaConTamano` de `getActUsuarioMiRevision` |
| Archivo de revisión | Nueva `/Activity/descargaArchivo/{curso}/{actividad}/{idArchivoActividadRevision}/3` | `archivosRevisionConTamano` de `getActUsuarioMiRevision` |

El último segmento de `descargaArchivo` es el tipo. 1 corresponde a entrega, 2 al enunciado y 3 a revisión.

Los adjuntos de foros llegan en `archivoForoConTamano`, `listaArchivos` o `archivos` y no tienen una ruta de descarga identificada. Para ellos, y para cualquier archivo sin los ids anteriores, la app usa `ruta`, `rutaArchivo` o `url`. Sustituye barras invertidas, rechaza rutas con `..` y solo acepta HTTPS en `eminus.uv.mx`. Una ruta relativa se añade a `https://eminus.uv.mx/eminusapi/`. Los documentos no se resuelven suponiendo que cualquier `ruta` sea pública.

La descarga es válida solo con estado 200. Una respuesta `text/html` indica un error o una página de acceso, no el archivo. La app rechaza archivos de 50 MB o más, los guarda en una carpeta temporal con protección de datos y los abre con Quick Look. Las descripciones se convierten a texto y enlaces nativos. No usa WebKit ni reproduce los iframes que pueda contener el HTML de la API.

## Índice de rutas

| Método | Base | Ruta | Sección |
| --- | --- | --- | --- |
| POST | Principal | `/auth` | Acceso |
| GET | Nueva | `/Course/getAllCourses` | Lectura |
| GET | Nueva | `/Activity/getActividadesEstudiante/{curso}` | Lectura |
| GET | Nueva | `/Activity/getActividadEstudiante/{curso}/{actividad}` | Lectura y entrega |
| GET | Nueva | `/Activity/getActUsuarioMiRevision/{curso}/{actividad}/{equipo}/{rubrica}` | Lectura y entrega |
| POST | Nueva | `/Activity/InsertaActividadEntrega` | Entrega |
| POST | Nueva | `/Activity/adjuntarEntrega?idCurso=…&idActividad=…&idActividadEntregas=…` | Entrega |
| GET | Nueva | `/Activity/descargaArchivo/{curso}/{actividad}/{archivo}/{tipo}` | Descargas |
| POST | Nueva | `/refresh` | Renovación, sin confirmar |
| GET | Nueva | `/MessageCenter/GetListadoMensajes/1/{curso o 0}` | Lectura |
| GET | Nueva | `/MessageCenter/folders/{curso}` | Lectura |
| GET | Principal | `/Usuario/getUsuario` | Lectura |
| GET | Principal | `/Usuario/getIntegrantesRes/{curso}/1` | Lectura |
| GET | Principal | `/Usuario/getIntegrantes/{curso}` | Lectura |
| GET | Principal | `/Cursos/obtieneCurso/{curso}` | Lectura |
| GET | Principal | `/Contenido/getUnidades/{curso}/0` | Lectura |
| GET | Principal | `/Contenido/getElementos/{curso}/{unidad}` | Lectura |
| GET | Principal | `/Contenido/getElemento/{curso}/{elemento}` | Lectura |
| GET | Principal | `/contenido/descargaElementoArchivo/{curso}/{elemento}/{archivo}` | Descargas |
| GET | Principal | `/Seguimiento/SegPrincipal/{curso}` | Lectura |
| GET | Principal | `/Seguimiento/SegDetalle/{curso}/{usuario}` | Lectura |
| GET | Principal | `/Seguimiento/SegTablaDetalleGeneral/{curso}` | Lectura |
| GET | Principal | `/Seguimiento/SeguimientoConcentradoCal/{curso}` | 403 con rol alumno |
| GET | Principal | `/Examen/getExamenesEst/{curso}` | Lectura |
| GET | Principal | `/Foro/getForos/{curso}/0` | Lectura |
| GET | Principal | `/Foro/getComentariosForo/{curso}/{foro}` | Lectura |
| POST | Principal | `/Foro/InsertaComentarioForo` | Mensajes y foros |
| GET | Principal | `/centromensajes/GetMensajeDetalle/{mensaje}` | Lectura |
| POST | Principal | `/centromensajes/insertaActualizaMensaje` | Mensajes y foros |
| GET | Principal | `/centromensajes/descargaCentroMensajesArchivo/{mensaje}/{archivo}` | Descargas |
| GET | Principal | `/Global/NotificacionesUsuario` | Lectura |
| GET | Principal | `/Global/getModulosResumen/{curso}/0/0` | Lectura |
| GET | Principal | `/SalonInteractivo/getSalonesInteractivos/{curso}/0` | Lectura |
| GET | Principal | `/SalonInteractivo/salonInteractivoTeams/{curso}` | Lectura |
| GET | Principal | `/SalonInteractivo/getSalonZoomEst/{curso}` | Lectura |
| GET | Principal | `/SalonInteractivo/getSalonOtros/{curso}` | Lectura |
