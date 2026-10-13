# eminus (Rust)

La vista tui de BetterThanEminus como programa nativo de terminal: agenda, cursos, miyu, tareas calificadas y contenido, leídos directo de la API de Eminus. Sin navegador.

```bash
cargo run --release
```

El binario queda en `target/release/eminus` (≈2.4 MB). Para tenerlo en el PATH: `cargo install --path .`

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
| `D` | descargar los archivos del elemento a `~/Downloads` |
| `r` | actualizar · `c` tema · `?` ayuda · `X` cerrar sesión · `q` salir |

El ratón también sirve: clic selecciona, la rueda desplaza. Para copiar texto, arrastra con shift (u option en macOS).

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
