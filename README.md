# BetterThanEminus

Extensión de Chrome (Manifest V3) que viste [Eminus](https://eminus.uv.mx) de terminal: tipografía monoespaciada, bordes de 1px, barra de estado estilo vim/tmux, paleta de comandos y navegación completa con teclado. Incluye además la **vista tui**: agenda, cursos en columnas y vista miyu, abierta dentro de la misma página de Eminus.

Es compatible con **Miyu --pendientes**: puedes tener las dos activas a la vez.

También hay una **app para iPhone** con todo esto adaptado al teléfono: ver [ios/README.md](ios/README.md).

## Instalación

1. Abre `chrome://extensions/`
2. Activa **Developer mode**
3. **Load unpacked** → elige esta carpeta
4. Recarga cualquier pestaña de Eminus

## Qué hace

### Herramientas que Eminus no tiene

- **Borradores automáticos**: lo que escribes en comentarios, foros o entregas se guarda mientras escribes (sólo en este navegador). Si la sesión vence o la página se recarga, al volver al campo aparece «✎ borrador · recuperar / descartar». Todos los borradores están en `:` → «borradores guardados» (enter copia el texto). Nunca se guardan contraseñas, búsquedas ni el login.
- **Aviso de sesión**: el token de Eminus dura 2 horas desde que entras y Eminus no avisa. La barra muestra «sesión 12 min» cuando quedan menos de 20 minutos (amarillo a los 10, rojo a los 3) y avisa a los 10, a los 3 y al vencer.
- **Búsqueda global de actividades** (`;`): todas las actividades de tus cursos actuales en una sola búsqueda; enter te lleva directo al detalle.
- **Recientes** (`g r`): vuelve al curso y la sección donde estabas.
- **Modo lectura** (`z`): oculta menú, pie, comentarios y «me gusta», y deja el contenido docente en una columna cómoda. `z` o `esc` para salir.
- **Tareas calificadas** (vista tui `4`, o `[ tareas calificadas ]` en el encabezado): calificaciones por materia con promedio por materia y general; lo entregado sin calificar aparece como «en revisión».

### Estilos

El estilo cambia la forma y el tema cambia los colores. Se combinan como quieras (`e` cambia el estilo, `c` el tema, o desde el popup):

| Estilo | Para qué |
|--------|----------|
| **tui** | Terminal: monoespaciada, bordes de 1px, corchetes, menú numerado y atajos a la vista |
| **nube** | Claro y suave, inspirado en cap.so: página gris claro, contenido en un panel grande con cielo pálido, títulos en Instrument Sans de peso normal, texto largo en Source Serif, botón principal azul "vidrio" y controles segmentados. Usa sus propios colores |
| **enfoque** | Para leer: sin cajas, cursos en lista de una columna, títulos serif y contenido docente a 70 caracteres por línea |
| **compacto** | Densidad: fuente del sistema a 13px, cursos en retícula apretada, filas de 30px con cebra y actividades en una línea. Ves el doble sin desplazarte |
| **editorial** | Periódico: mancheta en cursiva, doble filete, titulares en Newsreader, etiquetas en versalitas y módulos en números romanos |
| **neo** | Neobrutalismo: bordes negros de 2px, sombras duras, bloques de color y botones que se hunden al pulsarlos. Usa sus propios colores (contraste verificado) |
| **retro 95** | Ventanas, barras de título azules, botones con bisel y barra de tareas. Usa sus propios colores |

### Temas de color

17 temas, agrupados en el popup. Todos tienen contraste verificado (WCAG): texto ≥ 7:1 (≥ 10:1 en los propios), texto secundario y colores de estado ≥ 4.5:1, texto de botón ≥ 4.5:1 sobre el acento.

| Grupo | Temas |
|-------|-------|
| Oscuros | gruvbox, tokyo night, catppuccin mocha, nord, phosphor, amber, **medianoche** (azul tinta y ámbar), **terracota** (barro, arena y teja), **océano** (petróleo y turquesa), **grafito** (gris neutro con un solo acento lima) |
| Claros | gruvbox light, paper, **matcha** (crema y verde bosque), **lavanda** (lila y ciruela), **sakura** (rosa pétalo y cereza) |
| Accesibles | **alto contraste** (negro, blanco y amarillo, con foco de 3px y enlaces siempre subrayados) y **daltonismo** (estados con la paleta Okabe-Ito; además la barra de estado de cada actividad cambia de patrón, no sólo de color) |

### Fondo con foto

Desde el popup (sección **fondo**) eliges una imagen que queda detrás de Eminus, tramada con puntos (Bayer 8×8) y fundida hacia el color de fondo del tema, o lisa. El encabezado, el menú lateral y la tarjeta principal se vuelven translúcidos. Opcionalmente, **colores de la foto como tema**: se toma el tono dominante de la imagen y se arma con él la paleta (neutros teñidos y acento), en claro u oscuro según el tema o estilo elegido. Se ajustan la intensidad de la foto y la opacidad de los paneles de Eminus.

Puedes acomodar la imagen: *fondo: acomodar…* (en `:` o desde el popup) desvanece Eminus y te deja arrastrarla para moverla, hacer zoom con la rueda (o `+`/`-`), girarla con shift + rueda, `[` `]` o `r` (90°), voltearla con `h`/`v` y restablecer con `0`; `enter` guarda y `esc` cancela. Zoom, giro y volteos también están como deslizadores y comandos en `:` y en el popup. Al elegir otra imagen el encuadre vuelve a empezar.

La foto también se ve en la vista tui (agenda, cursos, miyu, tareas calificadas), con los paneles translúcidos.

Se controla desde la paleta de comandos (`:` → *fondo: elegir imagen…*, tramado/liso, deslizadores de *intensidad* de la foto y *opacidad de eminus* en la misma lista —se mueven con ← →—, colores de la foto, quitar) o desde el popup. Como el diálogo de archivos cierra el popup, ahí *elegir…* abre el popup en una pestaña, donde también puedes arrastrar la imagen.

### Reskin de Eminus

- Menú lateral numerado, cabecera con ruta (`internet de las cosas / actividades`), botones `[ así ]`, tarjetas de 1px, pestañas en video inverso.
- Los colores de estado de las actividades (anteriores, por iniciar, actuales, prórroga) se traducen a la paleta del tema.
- Barra de estado inferior: modo (`NORMAL`, `NAV`, `CMD`), avisos, sesión, indicador de Miyu, tema y reloj.
- Pantalla de inicio de sesión como ventana de terminal.

### Vista tui dentro de Eminus

Se abre sobre el contenido de Eminus, debajo de su encabezado, sin cambiar de página. Se abre desde los botones del encabezado (`[ agenda ] [ cursos ] [ miyu ] [ tareas calificadas ]`), con `A`, desde la paleta o desde el popup (`❯ ver pendientes`). Se precarga en segundo plano al entrar a Eminus, así que aparece al instante y con los datos listos; si estuvo oculta más de 5 minutos, se actualiza sola al volver a abrirla.

Para volver a Eminus: `esc` (o `q`), el botón `← eminus` de la barra inferior, o pulsar otra vez el mismo botón del encabezado. La página de detrás no se recarga, así que vuelves justo donde estabas.

- **1 agenda**: pendientes de todos tus cursos actuales agrupados en vencidas, hoy, mañana, próximos 7 días y después, con el detalle y la descripción de la actividad al lado.
- **2 cursos**: tres columnas estilo ranger (cursos → actividades → detalle) con barra de progreso de entregas.
- **3 miyu**: lo que hace Miyu --pendientes, en versión TUI. Siguiente entrega con cuenta regresiva en vivo, carga de la semana, avance por materia, movimiento reciente y pendientes agrupados por materia.
- **4 tareas calificadas**: calificaciones por materia con promedio.
- Los contadores (vencidas, hoy, 48 h, 7 días) están siempre en la barra inferior.
- `enter` cierra la vista y lleva a esa actividad en Eminus: entra al curso, va a Actividades y carga el detalle.

La vista siempre usa el estilo terminal con el tema elegido.

## Atajos

En Eminus (no se activan mientras escribes):

| Tecla | Acción |
|-------|--------|
| `:` o `ctrl k` | Paleta de comandos |
| `ñ` | Buscar y abrir un curso |
| `;` | Buscar una actividad en todos los cursos |
| `g r` | Recientes |
| `z` | Modo lectura |
| `A` | Abrir la vista tui (agenda) |
| `j` / `k` | Siguiente / anterior elemento (tarjetas, módulos, actividades) |
| `enter` / `o` | Abrir el elemento seleccionado |
| `g g` / `G` | Primero / último |
| `g c` `g x` `g a` `g e` `g i` `g f` `g m` `g l` `g s` `g b` | Cursos, contenido, actividades, exámenes, integrantes, foros, mensajes, sala, seguimiento, biblioteca |
| `d` / `u` | Media página abajo / arriba |
| `H` / `L` | Atrás / adelante |
| `c` | Siguiente tema |
| `e` | Siguiente estilo |
| `h` o `F1` | Ayuda |
| `alt ⇧ b` | Activar o desactivar el reskin |

En la vista tui: `1`/`2`/`3`/`4` vistas (agenda, cursos, miyu, tareas calificadas), `j`/`k` mover, `h`/`l` o `tab` cambiar de panel, `enter` abrir, `O` abrir el curso, `/` filtrar, `a` mostrar entregadas o cursos anteriores, `r` actualizar, `c` tema, `?` ayuda, `esc`/`q` volver a Eminus.

## Compatibilidad con Miyu --pendientes

- El CSS sólo aplica dentro de los contenedores de Eminus (`m-pages`, `m-auth`, `app-root`, overlays de Material). El panel `#eminus-pending-panel` y los elementos `ep-*` quedan fuera. Se verificó que sus 240 elementos conservan los mismos estilos calculados con cualquier tema y zoom.
- No se usan los atajos de Miyu (`alt e`, `/`, `r`, `t`, `?`, `1`–`7`). La paleta puede lanzarlos (mostrar panel, hoy, actualizar, buscar).
- `html`/`body` conservan tamaño de letra y `color-scheme`, que Miyu hereda. El zoom se aplica sólo a Eminus.
- No se reestructura el DOM de Eminus, así que la navegación de Miyu (iframe `app-b-frame`, `actividad-detalle`) sigue funcionando.
- La barra de estado deja hueco al botón "Volver a Eminus" de Miyu.

## Datos y privacidad

- Los borradores se guardan en el `localStorage` de eminus.uv.mx (sólo en este navegador, se borran solos a los 14 días). Los recientes, en `chrome.storage.local`.
- La imagen de fondo se reduce a 1920px y se guarda en `chrome.storage.local` (sólo en este navegador).
- Las preferencias (tema, fuente, zoom, detalles) se guardan en `chrome.storage.sync`, con un espejo en `localStorage` para pintar el tema sin parpadeo.
- La vista tui necesita tu sesión: el content script pasa el `accessToken` de Eminus al service worker, que lo guarda sólo en `chrome.storage.session` (en memoria, se borra al cerrar Chrome). También se cachean ahí los cursos y actividades de la sesión.
- Sólo se consulta `https://eminus.uv.mx`. Las fuentes se cargan de Google Fonts.

## Archivos

| Archivo | Descripción |
|---------|-------------|
| `manifest.json` | Configuración de la extensión |
| `background.js` | Relevo del token hacia la vista tui |
| `src/prefs.js` | Preferencias, temas y fuentes |
| `src/boot.js` | Aplica el tema en `document_start` |
| `src/bg.js` · `src/bg.css` | Fondo con foto: tramado y paleta tomada de la imagen |
| `src/themes.css` | Paletas |
| `src/tui.css` | Reskin de Eminus y UI propia |
| `src/styles.css` | Estilos nube, enfoque, compacto, editorial, neo y retro 95 |
| `src/nav.js` | Contexto, secciones, cursos, cursor j/k e integración con Miyu |
| `src/palette.js` | Paleta de comandos y ayuda |
| `src/app.js` | Barra de estado, atajos, sesión, recientes, modo lectura y comunicación entre marcos |
| `src/drafts.js` | Borradores automáticos |
| `app/` | Vista tui (se carga en un iframe dentro de Eminus) |
| `popup/` | Popup de ajustes |
| `icons/` | Iconos |
