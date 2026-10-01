# Eminus para iPhone

App nativa (Swift + WKWebView) que abre Eminus a pantalla completa con
BetterThanEminus dentro: el mismo reskin, temas, fondo con foto, borradores,
búsqueda y vista tui de la extensión de Chrome, con una capa táctil encima.
El código web es el mismo de la raíz del repositorio (`src/`, `app/`,
`popup/`); aquí sólo vive lo propio de la app.

## Qué cambia respecto a la extensión

- **Dock inferior**: inicio, pendientes (con contador de vencidas y de hoy),
  buscar, ajustes y más (todos los comandos).
- **Secciones del curso** en una tira deslizable bajo el encabezado, en lugar
  del menú lateral.
- **Paleta, ayuda y ajustes** como hojas: se cierran con ✕, tocando fuera o
  deslizando hacia abajo.
- **Vista tui** de un panel a la vez (lista → actividades → detalle), con
  pestañas arriba y deslizar a la derecha para volver.
- **Actividades** con un solo scroll: el iframe de Eminus crece con su
  contenido.
- **Fondo con foto**: se acomoda con los dedos (arrastrar, pellizcar, girar).
- **Avisos de entrega** (más → *avisos de entrega: activar*): notificación
  local un día y dos horas antes de cada pendiente.
- **Perfiles**: al iniciar sesión a mano la app ofrece guardar el perfil.
  Después se entra con un toque desde la pantalla de login, se vuelve a
  entrar sola cuando la sesión vence y se cambia de cuenta en *más →
  perfiles*. Usuario y contraseña van al Llavero de iOS (sólo en este
  dispositivo); la página nunca puede leer una contraseña guardada.
- **Sólo estilo tui** (los demás no se cargan) y **ABC Areal** empaquetada
  como fuente (carpeta `ios/fonts/`, con licencia: no se sube al repositorio).
- Archivos de Eminus en vista previa con compartir, enlaces externos en una
  hoja de Safari, deslizar desde el borde para volver y jalar para recargar.
- Preferencias, fondo y borradores sobreviven a que Eminus vacíe
  `localStorage` al cerrar o vencer la sesión.

## Cómo funciona

| Archivo | Qué hace |
|---------|----------|
| `BetterThanEminus/EminusViewController.swift` | WKWebView, navegación, descargas, diálogos y puente con la página |
| `BetterThanEminus/WebBundle.swift` | Lee el código web e inyecta lo que en la extensión son content scripts |
| `BetterThanEminus/ProfileStore.swift` | Perfiles de Eminus en el Llavero |
| `BetterThanEminus/AppExpiry.swift` | Fecha en que vence la firma, para avisar |
| `BetterThanEminus/DevBridge.swift` | Sólo desarrollo: evalúa JS en la página desde la terminal |
| `web/shim.js` | Sustituto de `chrome.storage`, `chrome.runtime` y `chrome.tabs` |
| `web/app-prefs.js` | Sólo estilo tui y carga de ABC Areal |
| `web/mobile.js` · `web/mobile.css` | Dock, hojas, avisos y diseño de Eminus en el teléfono |
| `web/app-mobile.js` · `web/app-mobile.css` | La vista tui con el dedo |
| `web/popup-mobile.css` | Los ajustes dentro de una hoja |
| `project.yml` | Proyecto de Xcode (se genera con XcodeGen) |

La vista tui y los ajustes se arman como documentos `srcdoc` dentro de la
página, así quedan en el origen de Eminus y la API responde sin CORS.

## Compilar

```bash
brew install xcodegen
cd ios && xcodegen generate
open BetterThanEminus.xcodeproj
```

En Xcode elige tu iPhone y pulsa ▶.

## Renovación automática (cuenta gratuita de Apple)

Con una cuenta gratuita la firma dura 7 días y no hay forma de alargarla
(sólo el programa de pago da un año). Lo que sí se puede es renovarla sin
hacer nada: `ios/dev/renew.sh` pide un perfil nuevo, recompila e instala.

```bash
ios/dev/renew.sh --install-agent   # launchd: revisa cada 3 horas
ios/dev/renew.sh --status          # cuánto queda
ios/dev/renew.sh --force           # renovar ahora
ios/dev/renew.sh --remove-agent    # quitar la tarea
```

Renueva cuando quedan menos de 3 días. Necesita la Mac encendida y el iPhone
desbloqueado a la vista (cable o la misma red wifi); si no, reintenta en la
siguiente vuelta. Reinstalar no borra la sesión, los ajustes ni los
borradores. La app avisa cuando le quedan menos de 36 horas y en *más →
firma de la app* se ve la fecha. Registro: `~/Library/Logs/BetterThanEminus-renew.log`.

## Desarrollo en el simulador

```bash
python3 ios/dev/devserver.py   # en una terminal
ios/dev/run-sim.sh             # compila, instala y abre la app
ios/dev/js 'return document.title'   # evalúa JS dentro de la app
ios/dev/js __reload__          # relee el CSS/JS de la Mac y recarga
```

Con `BTE_DEV` la app lee el código web del servidor en vez del paquete, así
que un cambio de CSS o JS no necesita recompilar.
