# eminus (Rust)

La vista tui de BetterThanEminus como programa nativo de terminal: agenda, cursos, miyu, tareas calificadas y contenido, leídos directo de la API de Eminus. Sin navegador.

```bash
cargo run --release
```

El binario queda en `target/release/eminus` (≈2.4 MB). Para tenerlo en el PATH: `cargo install --path .`

## Con ventana

```bash
cargo run --release --bin eminus-gui
```

La misma app en una ventana (egui): pestañas, lista a la izquierda y detalle a la derecha, con botones para entregar, descargar, abrir en Eminus, actualizar, cambiar de tema y cerrar sesión. Comparte todo con la de terminal: sesión, perfiles, caché y tema. Para entregar se pueden elegir los archivos con el diálogo del sistema o soltarlos sobre la ventana. Las flechas (o `j` `k`) mueven la selección.

Usa bastante más memoria que la de terminal (≈115 MB frente a ≈8.5 MB), porque dibuja con la GPU; en reposo no consume CPU.

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
| `enter` / `o` | abrir en Eminus (con la extensión instalada llega directo al detalle) |
| `O` | abrir el curso |
| `/` | filtrar · `esc` quita el filtro |
| `a` | mostrar entregadas o cursos anteriores |
| `D` | descargar a `~/Downloads` los archivos de la actividad o del elemento de contenido |
| `E` | entregar la actividad seleccionada |
| `r` | actualizar · `c` tema · `?` ayuda · `X` cerrar sesión · `q` salir |

El ratón también sirve: clic selecciona, la rueda desplaza. Para copiar texto, arrastra con shift (u option en macOS).

## Entregar actividades

`E` sobre una actividad abre la ventana de entrega:

1. Agrega archivos arrastrándolos a la terminal (o escribe la ruta y `enter`). `⌫` con el campo vacío quita el último.
2. `tab` y escribe el comentario. Eminus no acepta entregas sin texto.
3. `enter` pide confirmación; `s` entrega.

La app hace lo mismo que la página de Eminus: crea el borrador, sube los archivos y envía. Si algo falla antes del envío, la actividad **no** queda entregada (queda el borrador en Eminus con lo que sí subió) y la ventana lo dice. Si ya había una entrega, se actualiza el comentario y se agregan los archivos nuevos; para quitar archivos ya subidos hay que hacerlo en Eminus. Las actividades por equipos, las ya calificadas y las que Eminus tiene cerradas no se pueden entregar desde aquí.

Después de entregar por primera vez, compruébalo en Eminus: esta función está probada contra un servidor de pruebas que imita a Eminus, no contra entregas reales.

## Por qué es rápida y ligera

- Un hilo dibuja; seis hilos fijos hablan con la red. La interfaz nunca espera una respuesta.
- Sólo se pinta cuando algo cambió, y a la terminal sólo van las celdas distintas. En reposo no escribe nada ni usa CPU.
- Cursos, actividades y contenido se guardan en caché (`~/Library/Caches/eminus` o `~/.cache/eminus`): abre al instante y se pone al día por detrás (5 min actividades, 30 min contenido).
- Las descripciones se convierten de HTML a texto al llegar (las imágenes incrustadas se descartan) y sólo se conservan las 60 más recientes. Las descargas van directo al disco.
- Medido con datos de prueba: ≈8.5 MB de memoria residente y primer cuadro en ≈60 ms.

## Pruebas

```bash
cargo test
```

Las pruebas marcadas `#[ignore]` pintan las vistas y recorren los perfiles contra un servidor que imite a Eminus (`EMINUS_ORIGIN=http://127.0.0.1:8765`); la de perfiles usa el llavero de verdad con un usuario de prueba que quita al terminar.
