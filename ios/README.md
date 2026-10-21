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
- Widgets de pendientes para pantalla de inicio y bloqueo, con acceso a cada
  actividad. La app comparte la agenda con la extensión mediante App Groups.
- Live Activity con la próxima entrega y su cuenta regresiva en Dynamic
  Island y pantalla bloqueada, si vence en las siguientes ocho horas.
- Avisos locales un día, dos horas y diez minutos antes. iOS conserva los
  avisos programados aunque la app no esté abierta. En *más → widgets,
  avisos y sesión* puedes activarlos y consultar su estado.
- **Perfiles**: al iniciar sesión a mano la app ofrece guardar el perfil.
  Después se entra con un toque desde la pantalla de login, se vuelve a
  entrar sola cuando la sesión vence y se cambia de cuenta en *más →
  perfiles*. Usuario y contraseña van al Llavero de iOS (sólo en este
  dispositivo). La sesión se renueva cinco minutos antes de vencer, al usar
  la app o cuando iOS permite una actualización en segundo plano. El login
  nativo usa el mismo cifrado de Eminus que la versión de Rust. Las
  credenciales quedan disponibles tras el primer desbloqueo del iPhone para
  que la renovación funcione con la pantalla bloqueada.
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
| `BetterThanEminus/SessionService.swift` | Token en el Llavero y renovación de la sesión |
| `BetterThanEminus/AgendaSync.swift` | Actualización nativa de tareas y ejecución con BGAppRefreshTask |
| `BetterThanEminus/ReminderScheduler.swift` | Recordatorios locales, cancelación y límite de avisos |
| `Shared/` · `Widgets/` | Agenda compartida, widgets y Live Activity |
| `BetterThanEminus/AppExpiry.swift` | Fecha en que vence la firma, para avisar |
| `BetterThanEminus/DevBridge.swift` | Sólo desarrollo: evalúa JS en la página desde la terminal |
| `web/shim.js` | Sustituto de `chrome.storage`, `chrome.runtime` y `chrome.tabs` |
| `web/agenda.js` | Sincronización entre el navegador y la agenda nativa |
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

Los dos targets necesitan el App Group `group.mx.hiram.BetterThanEminus`
en su perfil de firma. Consulta [Widgets/README.md](Widgets/README.md) si
tu equipo de Apple no admite esa capacidad. La actividad en vivo y los
recordatorios no necesitan compartir un contenedor con la extensión.

## Activar widgets y avisos

1. Inicia sesión y guarda el perfil para permitir la renovación automática.
2. En *más → widgets, avisos y sesión*, activa los recordatorios y acepta el
   permiso de iOS. Ahí puedes consultar la última sincronización.
3. Mantén pulsada la pantalla de inicio o la pantalla bloqueada, añade un
   widget y busca Eminus. La agenda se carga al iniciar sesión.

La app guarda hasta 60 tareas en el contenedor compartido. Los widgets no
reciben tokens ni contraseñas. Cerrar sesión o cambiar de cuenta limpia la
agenda anterior, termina la Live Activity y cancela sus avisos.

iOS decide cuándo concede tiempo de ejecución en segundo plano. Las tareas
nuevas y los cambios de fecha se detectan al abrir la app o cuando permite
una actualización. Los avisos locales ya programados no dependen de esa
actualización. Esta versión no tiene un servidor de notificaciones push.

La Live Activity se inicia con la app abierta y puede seguir visible al
cerrarla. No se inicia automáticamente con la app cerrada. Su duración
máxima es de ocho horas. Al tocar un widget, un aviso o la actividad, la app
conserva el destino hasta que la página y la sesión estén listas.

## Comprobar los cambios

Desde la raíz del repositorio:

```bash
node --test ios/tests/*.test.cjs
swiftc -parse-as-library ios/Shared/Agenda.swift ios/Shared/Checks/AgendaChecks.swift -o /tmp/bte-agenda-checks
/tmp/bte-agenda-checks
swiftc -parse-as-library ios/Shared/Agenda.swift ios/BetterThanEminus/ReminderScheduler.swift ios/tests/ReminderChecks.swift -o /tmp/bte-reminder-checks
/tmp/bte-reminder-checks
swiftc -parse-as-library ios/BetterThanEminus/ProfileStore.swift ios/BetterThanEminus/SessionService.swift ios/tests/session_test.swift -o /tmp/bte-session-checks
/tmp/bte-session-checks
```

Las pruebas no usan credenciales reales ni acceden a Eminus. Cubren cuentas
distintas, respuestas tardías, renovación tras 401, cifrado, selección de
tareas y planificación de avisos. Los permisos, la entrega real de avisos y
la frecuencia de actualización necesitan una comprobación en un iPhone.

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
