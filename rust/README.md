# eminus (Rust)

BetterThanEminus como app nativa en Rust, con ventana y terminal. Consulta actividades, entregas y retroalimentación, descarga archivos y entrega trabajos directamente a la API de Eminus.

```bash
cargo run --release
```

El binario queda en `target/release/eminus` (≈2.4 MB). Para tenerlo en el PATH: `cargo install --path .`

## Con ventana

```bash
cargo run --release --bin eminus-gui
```

La ventana comparte sesión, perfiles, caché y temas con la terminal. La navegación lateral reúne Actividades, Mis cursos, Resumen, Calificaciones y Contenido. La búsqueda permanece en el encabezado y filtra la sección actual. Las listas separan el título y los datos secundarios. El detalle distingue las instrucciones, tu entrega y la retroalimentación. Las fechas adicionales y la calificación de la actividad se pueden desplegar.

`Mi cuenta` muestra tu nombre completo, usuario y foto de perfil con los mismos datos que consulta Eminus. La foto se carga en segundo plano y queda en memoria durante la sesión. Si no puede cargarla, puedes seguir usando la app. Actualizar también vuelve a consultar el perfil.

El tema `eminus` usa paneles blancos, una lista gris clara y acento azul. Es el predeterminado de la ventana si no has elegido un tema. Los temas guardados se conservan. Puedes elegirlo desde Personalizar, Colores. Por debajo de 1000 puntos de ancho, la navegación pasa a un selector en el encabezado. Las divisiones entre cursos, actividades y detalle usan el espacio que queda después de la navegación, también al cambiar el zoom.

`1` a `5` cambian de vista, `j` y `k` o las flechas mueven la selección, `h` y `l` cambian de panel en cursos. `/` enfoca el filtro, `enter` termina de escribir y `esc` lo borra. `J` y `K` desplazan el detalle. `E` abre la entrega, `D` descarga y `?` muestra los atajos. Los campos de texto reciben las teclas sin activar estas acciones.

En macOS, `⌘F` enfoca el filtro, `⌘R` actualiza, `⌘1` a `⌘5` cambian de vista, `⌘W` cierra el diálogo activo y `⌘Q` sale. `⌘+`, `⌘-` y `⌘0` cambian o restablecen el zoom. En el editor, `⌘S` guarda el borrador local y `⌘Enter` pide confirmación antes de enviar. Los campos admiten seleccionar, copiar, cortar, pegar, deshacer y rehacer con los atajos de macOS. Windows y Linux usan Ctrl.

También se puede usar el ratón. Para entregar, elige los archivos con el diálogo del sistema o suéltalos sobre la ventana. La app pide confirmación antes de enviar. `esc` vuelve o cancela el diálogo y `s` o `n` responden a la confirmación.

El detalle de cada actividad muestra las instrucciones, tu entrega y los comentarios del docente, con su calificación y fecha de revisión. Los archivos del docente, de tu entrega y de la revisión tienen botones de descarga individual. `V` consulta de nuevo tu entrega sin abrir el editor. `B` descarga los archivos de tu entrega y revisión.

`enter` y `o` muestran el detalle dentro de la app. En cursos, `enter` pasa a las actividades de la materia. `O` vuelve al curso y los botones del detalle llevan a sus actividades o contenido. Para abrir Eminus en el navegador usa `w`; `W` abre el curso en la web.

En mediciones anteriores, la versión con ventana usaba unos 115 MB de RAM frente a 8.5 MB de la terminal. Esas cifras no incluyen una foto de fondo.

Arrastra la división entre paneles para cambiar su ancho. Los ajustes, la ayuda y el editor de entrega también se pueden redimensionar. `Ampliar lectura` oculta las listas. Al estrechar la ventana, los cursos y sus actividades comparten un panel y, por debajo de 700 puntos, la lista y el detalle se muestran por separado.

Las listas sólo dibujan las filas visibles, incluso con miles de elementos. El detalle del curso conserva sólo las cinco próximas entregas para mostrarlas; no copia ni ordena todas las actividades en cada cuadro. El logo usa el GIF del gato incluido en `assets/hi-otag.gif`, sin texto de marca ni eslóganes. Aparece en la navegación, en el encabezado de las ventanas estrechas y al iniciar sesión. La app decodifica sus seis cuadros una sola vez y los carga juntos en una textura de unos 492 KiB. Conserva los intervalos de 40 ms del original.

La animación pausa al perder el foco, minimizarse o quedar fuera del área visible. Mientras se mueve, egui necesita repintar la ventana hasta 25 veces por segundo. No se vuelve a decodificar ni subir la imagen a la GPU en cada cuadro. Se activó el soporte GIF de la biblioteca `image` que ya usa la app.

## Personalizar la ventana

Pulsa `p`, `⌘,` o el botón `Personalizar`. También puedes abrir los ajustes desde el inicio de sesión. Los cambios se guardan al hacerlos y se recuperan al volver a abrir la app.

- Elige entre 19 temas. Se suman gruvbox light, amber, medianoche, terracota, océano, grafito, matcha, lavanda, sakura, alto contraste y daltonismo. La terminal comparte estos temas y los recorre con `c`.
- En Texto, elige una fuente instalada por su nombre, filtra la lista o importa un archivo TTF, OTF o TTC. La app guarda la ruta original. Si mueves o borras el archivo, usa una fuente de reserva y muestra el error. Las listas también usan la fuente elegida.
- En Lectura, ajusta el interlineado y el ancho del detalle. `Usar lectura cómoda` aplica texto de 18 puntos y espacio de 1.6 entre líneas. En Interfaz, cambia la densidad y el zoom, hasta el 200 %.
- Elige un acento propio, limita el ancho del detalle y muestra u oculta el reloj, los contadores y la guía de atajos. El acento mantiene contraste legible sobre el fondo y la selección.
- Elige una foto PNG, JPEG o WebP. Activa el tramado de puntos Bayer de 8 por 8, o desactívalo para ver la imagen lisa. Ajusta la intensidad y la opacidad de los paneles. Las ventanas de entrega y ajustes conservan el fondo opaco.

La app conserva una copia de la foto reducida a 1920 píxeles de lado en su caché, sin modificar el original. Decodifica y prepara el tramado en otro hilo. La textura se reutiliza mientras la foto, el tema, el efecto y el tamaño de la ventana sigan iguales; mover la intensidad no vuelve a tramar. La imagen se funde hacia el color del tema en la parte inferior.

`quitar` elimina la copia del fondo. `Restablecer apariencia` devuelve fuente, densidad, zoom y controles a sus valores iniciales. Conserva el tema y la foto elegidos. Los ajustes viven en `~/.config/eminus/preferencias.json` y no cambian la sesión ni los perfiles.

Los adjuntos tienen iconos según su extensión. PDF, documentos, hojas de cálculo, presentaciones, imágenes, archivos comprimidos, código, texto, audio y video se distinguen en contenido, entregas y retroalimentación. La terminal usa etiquetas como `[PDF]`, `[IMG]` y `[ZIP]` para funcionar con cualquier fuente.

## Sesión y perfiles

Entra con tu **usuario y contraseña** de Eminus. La contraseña se cifra igual que en el formulario de Eminus (la app lee la llave de cifrado de la propia página de Eminus al entrar) y se manda sólo a eminus.uv.mx.

Al entrar, la app ofrece **guardar el perfil**. Si aceptas:

- La contraseña va al llavero del sistema (Llavero de macOS), nunca a un archivo. En `~/.config/eminus/perfiles.json` sólo queda quién tiene perfil.
- La pantalla de login pasa a ser un selector: `enter`, `↑` `↓` o el número del perfil. «+ iniciar con otro perfil» muestra el formulario; `x` dos veces quita un perfil (y su contraseña del llavero).
- El token de Eminus dura 2 horas. Si la sesión vence sola, la app vuelve a entrar sin preguntar; si la cierras tú con `X`, se queda en el selector.
- No guardes perfiles en una computadora compartida. Al recompilar la app, macOS puede pedir permiso de nuevo para leer el llavero.

Sin perfil guardado no se conserva la contraseña: sólo el token de la sesión (`~/.config/eminus/sesion`, permisos 0600).

También puedes pegar un **token** en vez de la contraseña: en la consola del navegador, `copy(localStorage.accessToken)`. O pasarlo con la variable `EMINUS_TOKEN`.

Si Eminus cambia su página y la app ya no encuentra la llave, se puede fijar a mano con `EMINUS_LOGIN_KEY` o con `llave_login` en `~/.config/eminus/preferencias.json`.

## Atajos

| Tecla | Acción |
|-------|--------|
| `1`–`5` | agenda · cursos · miyu · calificadas · contenido |
| `j` `k` / flechas | mover · `g` `G` primero y último · `ctrl d` `ctrl u` media página |
| `h` `l` / `tab` | cambiar de panel (cursos) |
| `J` `K` | desplazar el detalle |
| `enter` / `o` | ver el detalle o explorar el curso dentro de la app |
| `O` | ir al curso dentro de la app |
| `w` / `W` | abrir lo seleccionado o el curso en la web |
| `/` | filtrar · `esc` quita el filtro |
| `a` | mostrar entregadas o cursos anteriores |
| `D` | descargar a `~/Downloads` los archivos de la actividad o del elemento de contenido |
| `B` | descargar los archivos de tu entrega y de la revisión |
| `V` | consultar de nuevo tu entrega y retroalimentación |
| `E` | entregar la actividad seleccionada |
| `r` | actualizar · `c` tema · `?` ayuda · `X` cerrar sesión · `q` salir |
| `p` | personalizar la ventana |

El ratón también sirve: clic selecciona, la rueda desplaza. Para copiar texto, arrastra con shift (u option en macOS).

## Entregar actividades

`E` sobre una actividad abre la ventana de entrega:

1. Agrega archivos arrastrándolos a la terminal (o escribe la ruta y `enter`). `⌫` con el campo vacío quita el último.
2. `tab` y escribe el comentario. Eminus no acepta entregas sin texto.
3. `enter` pide confirmación; `s` entrega.

El comentario y las rutas de archivos se guardan en `~/.config/eminus/borradores.json`, separados por cuenta y actividad, con permisos 0600 en Unix. Se recuperan al volver al editor, también después de cerrar la app. `esc` conserva el borrador. Los archivos siguen en su ubicación original; si moviste alguno, vuelve a elegirlo antes de entregar. Los borradores se borran después de confirmar el envío. No se borran al cerrar sesión.

La app crea el borrador en Eminus, sube los archivos y envía. Después consulta la entrega para comprobar el estado enviado, el comentario y la cantidad de adjuntos. Si no puede confirmar, conserva el borrador y consulta lo que quedó en Eminus. Los archivos cuya subida pudo comprobar dejan la lista de pendientes; los demás se conservan. Puedes reintentar la consulta con `ctrl r` dentro del editor.

Si ya había una entrega, se actualiza el comentario y se agregan los archivos nuevos. La app conserva los saltos de línea del comentario. Las actividades por equipos, las ya calificadas y las que Eminus tiene cerradas no se pueden entregar desde aquí. Quitar archivos ya enviados también requiere Eminus.

La consulta y las descargas usan los endpoints que aparecen en el código público de Eminus. Las pruebas de envío usan un servidor local; no se enviaron trabajos a una cuenta real. La app necesita los servicios de Eminus y conexión para consultar datos nuevos o entregar.

## Por qué es rápida y ligera

- Un hilo dibuja; seis hilos fijos hablan con la red. La interfaz nunca espera una respuesta.
- Sólo se pinta cuando algo cambió, y a la terminal sólo van las celdas distintas. En reposo no escribe nada ni usa CPU.
- Cursos, actividades y contenido se guardan en caché (`~/Library/Caches/eminus` o `~/.cache/eminus`): abre al instante y se pone al día por detrás (5 min actividades, 30 min contenido).
- Las descripciones se convierten de HTML a texto al llegar (las imágenes incrustadas se descartan) y sólo se conservan las 60 más recientes. Las descargas van directo al disco.
- Medido con datos de prueba: ≈8.5 MB de memoria residente y primer cuadro en ≈60 ms.

El filtro compara los campos sin unirlos ni copiar cada título. La app prepara el filtro cuando cambia el texto y reutiliza los caracteres al resaltar coincidencias. Las listas conservan su capacidad y comparten los títulos con las actividades y el contenido mediante `Arc<str>`.

La caché se lee desde un lector con búfer y se escribe con un búfer de 64 KiB. Mantiene el formato JSON anterior, los permisos 0600 en Unix y el reemplazo mediante archivo temporal. Los nombres de los cursos vuelven a compartirse al cargar las actividades. Las entregas también se analizan desde el lector, con errores para JSON cortado o conexiones interrumpidas. La copia JPEG del fondo se codifica directamente al archivo temporal.

Medición del 14 de octubre de 2026, en macOS. Se compara el estado incompleto que dejó Muse con esta versión. Son medianas de tres ejecuciones en release, con 30 cursos y 1.800 actividades. La prueba de caché usa títulos largos y un archivo de 15,2 MB.

| Operación | Estado de Muse | Versión terminada |
| --- | ---: | ---: |
| 100 reconstrucciones con filtro y reutilización de filas | 108,6 ms | 78,1 ms |
| Asignaciones en esas reconstrucciones | 1.315.314 | 955.314 |
| Pico adicional al guardar la caché | 23,34 MB | 0,066 MB |
| Pico adicional al leer la caché | 30,52 MB | 15,36 MB |

El pico cuenta la memoria que Rust reserva durante cada operación, además de los datos ya cargados. Leer la caché de prueba pasó de 4,5 ms a 35,5 ms a cambio de reducir ese pico a la mitad. Guardarla pasó de 8,0 ms a 7,3 ms. Falta medir la RAM residente de la app con una sesión real.

## Pruebas

```bash
cargo test
```

Las pruebas normales comprueban respuestas de API, descargas y confirmación de envío contra un servidor local, recuperación de borradores por cuenta, navegación nativa y teclado, edición de texto, fuentes y tamaños de la ventana. No necesitan credenciales.

Las pruebas manuales de `src/main.rs`, marcadas `#[ignore]`, recorren las vistas y perfiles contra un servidor que imite a Eminus (`EMINUS_ORIGIN=http://127.0.0.1:8765`); la de perfiles usa el llavero de verdad con un usuario de prueba que quita al terminar.

Para repetir las mediciones de rendimiento y memoria:

```bash
cargo test --release --locked --test perf_bench -- --ignored --nocapture --test-threads=1
```

Para medir el dibujo de la interfaz sin red, con 10.000 filas y una descripción de 16.800 caracteres:

```bash
cargo test --release --locked --bin eminus-gui bench_gui_frames -- --ignored --nocapture
```

La prueba registra mediana y percentil 95 de 300 cuadros después de 20 cuadros de calentamiento. Mide la generación de la interfaz en egui, sin incluir GPU, composición de la ventana ni latencia de la red. Antes de añadir el logo animado, en esta máquina el percentil 95 fue 0,048 ms.

Comprobación anterior al logo animado, en macOS del 14 de octubre de 2026, con cinco cursos y 30 actividades de un servidor local, sin foto. En tres arranques por versión, la mediana de RAM residente al cuarto segundo fue 114,5 MiB antes y 114,3 MiB después. La variación entre ejecuciones supera esa diferencia. No se aprecia un aumento de RAM en esta prueba; falta medir con una sesión real.

Estas mediciones usan datos sintéticos y una carpeta temporal. No consultan la sesión ni el llavero. El contador de memoria requiere ejecutar una prueba a la vez.
