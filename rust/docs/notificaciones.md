# Avisos de actividades con la app cerrada

Investigación del 14 de octubre de 2026 con Claude Haiku 5.5, contrastada
con el código del proyecto, documentación oficial y una prueba nativa en
macOS 27.0.1. Este documento propone la implementación. No instala un
agente ni activa notificaciones.

## Recomendación

Crear `eminus-avisos`, un binario Rust sin interfaz que el sistema ejecute
cada 30 minutos. Consulta cursos vigentes y actividades, detecta novedades,
actualiza recordatorios y termina. Entre consultas no queda un proceso
de BetterThanEminus residente. El intervalo de 30 minutos es una propuesta
para reducir consultas y consumo; no es un límite documentado de Eminus.

En macOS, usar un LaunchAgent de la sesión del usuario y
`UNUserNotificationCenter`. Para distribución, registrar el agente con
`SMAppService`, disponible desde macOS 13, dentro del paquete de la app.
Apple documenta el plist en `Contents/Library/LaunchAgents` y la ruta del
ejecutable mediante `BundleProgram`.
[SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice),
[estructura de los helpers](https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos).

Separar dos tipos de aviso. Una actividad nueva exige consultar Eminus.
Un recordatorio de una fecha conocida puede quedar programado en el
sistema, que lo entrega aunque la app haya terminado. Actualizar o cancelar
ese recordatorio al detectar una entrega o una prórroga.
[Notificaciones locales de Apple](https://developer.apple.com/documentation/usernotifications/scheduling-a-notification-locally-from-your-app).

## Qué se puede reutilizar

- [api.rs](../src/api.rs) tiene `Client::courses`, `Client::activities` y
  `Client::sign_in`. Bastan las listas. El monitor no necesita consultar
  contenido, descripciones, retroalimentación, fotos ni adjuntos.
- [store.rs](../src/store.rs) lee el token, su vencimiento `exp`, la cuenta
  y los perfiles. `secret_get` recupera la contraseña del llavero.
- [app.rs](../src/app.rs) ya vuelve a iniciar sesión al vencer el token de
  un perfil guardado. Ese flujo depende del ciclo de la interfaz y no
  sigue ejecutándose cuando termina el proceso.
- [model.rs](../src/model.rs) conserva identificadores, fechas, entrega y
  calificación. `deadline` ya aplica la prórroga cuando existe.

No usar `App::new()` en el monitor. Inicializa el pool de seis hilos y
carga estado para las vistas. Usar directamente un cliente HTTP y
procesar un curso a la vez. El coste normal de un ciclo será una consulta
de cursos más una consulta por curso vigente, sin contar login o reintentos.

## Separar las dependencias gráficas

Hoy [Cargo.toml](../Cargo.toml) declara `eframe`, `rfd`, `image`,
`ttf-parser` y `ratatui` como dependencias obligatorias.
[lib.rs](../src/lib.rs) incluye todos los módulos de interfaz.
`Prefs` también contiene tipos de apariencia y fondo. Además, el modelo
`UserInfo` y la consulta de fotos usan `image`.

Inspeccioné el binario release existente con `otool -L`. Incluso `eminus`,
la versión de terminal, enlaza AppKit, OpenGL y CoreVideo. No basta con
añadir un tercer binario y evitar abrir una ventana.

La primera implementación debe introducir funciones opcionales de Cargo
para excluir esos módulos y dependencias del monitor. Mantener las
funciones predeterminadas de GUI y TUI para no cambiar los comandos
existentes. Separar los tipos de preferencias de las funciones que dibujan
la interfaz. Los lectores del monitor pueden deserializar solo los campos
que necesitan, pero no deben reescribir preferencias omitiendo otros campos.

Comprobar el resultado con `cargo tree` y `otool -L`. El monitor necesita
Foundation, UserNotifications y Security en macOS, pero no debe cargar el
motor de ventanas ni las imágenes de la app. Un crate nuevo para todo el
código común no es necesario de entrada.

## Identidad y permisos de notificación en macOS

Usar [objc2-user-notifications](https://docs.rs/objc2-user-notifications/0.3.2/objc2_user_notifications/),
con `objc2-foundation` y `block2`, y activar solo los tipos necesarios.
Son enlaces Rust al framework nativo. No requieren un navegador ni un
runtime asíncrono para hacer las consultas HTTP existentes.

La prueba local compiló un ejecutable mínimo que solo obtiene
`UNUserNotificationCenter.currentNotificationCenter`. Fuera de un `.app`
terminó con `bundleProxyForCurrentProcess is nil`. Dentro de un `.app`
con `Info.plist` y `CFBundleIdentifier`, obtuvo el centro y terminó con
código cero. El paquete de prueba no tenía firma de distribución.

Esto confirma la diferencia entre CLI y paquete en esta Mac. No prueba
todavía el permiso, el envío desde launchd ni la entrega después de salir.
El diseño propuesto coloca el monitor en un helper `.app` con identidad
propia y `LSUIElement`, sin icono en el Dock. Comprobar esa identidad en
las pruebas antes de registrar el servicio.

Al activar los avisos desde la interfaz, pedir autorización para la
identidad que enviará las notificaciones. El permiso del proceso principal
no debe suponerse compartido con un helper de otro identificador. Apple
exige autorización y permite consultar si cambió.
[Permisos de notificaciones](https://developer.apple.com/documentation/usernotifications/asking-permission-to-use-notifications).

Para distribución, firmar la app y el helper con una identidad estable.
Verificar que el helper puede leer la contraseña del llavero durante la
configuración inicial. El README ya advierte que recompilar puede provocar
otra solicitud de acceso. La inicialización que probamos sin firma no
demuestra acceso automático al llavero.

`notify-rust` resulta útil para Linux, pero su backend macOS actual usa
`mac-notification-sys`. Ese backend utiliza `NSUserNotificationCenter`,
que Apple marca como obsoleto desde macOS 11. No elegirlo como base del
monitor nuevo en macOS.
[Código del backend](https://github.com/h4llow3En/mac-notification-sys/blob/master/objc/notify.m),
[dependencias de notify-rust](https://github.com/hoodie/notify-rust/blob/main/Cargo.toml),
[API obsoleta de Apple](https://developer.apple.com/documentation/foundation/nsusernotificationcenter).

## Renovación de sesión

El proyecto documenta tokens de dos horas. Usar el `exp` real del token,
sin fijar dos horas en el monitor. No encontramos un mecanismo de
`refresh token` en el código. Renovar significa repetir `sign_in` con
la contraseña del perfil guardado.

Antes de consultar, releer la sesión. Si el token todavía sirve, usarlo.
Renovarlo cuando ya venció o queda menos que el presupuesto del ciclo,
con un margen inicial de cinco minutos. No renovar durante el reposo,
cuando los avisos están desactivados o solo para entregar un recordatorio
que el sistema ya tiene programado.

Un token pegado sin contraseña guardada sirve hasta vencer, pero no permite
renovación automática. Mostrar una vez que hace falta abrir la app para
iniciar sesión. No consultar el llavero ni intentar login en cada curso.

Conservar el código HTTP al clasificar errores. Ahora `request` agrupa
401 y 403 como sesión vencida, y `login` convierte cualquier 4xx en
contraseña incorrecta. Un 403 de un curso puede ser falta de permiso y un
429 requiere esperar. No deben disparar logins repetidos.

Para una sesión rechazada, permitir una renovación por ciclo y reintentar
la consulta afectada una vez. Para fallos de red o servidor, guardar
`next_attempt_at` y aumentar la espera a 30, 60 y 120 minutos. Respetar
`Retry-After` cuando exista. Ante credenciales inválidas, detener la
renovación hasta que el usuario actualice el perfil. El estado de fallo
debe persistir porque el proceso termina entre ciclos.

`login_key()` descarga el índice y el JavaScript completo como cadenas.
Puede dominar el pico de RAM durante un login. Guardar la llave pública
extraída junto con la URL del script, que incluye su versión, e invalidarla
si cambia esa URL. Medir también la primera extracción; cachear la llave
no elimina el pico de la primera descarga.

## Coordinación con la app abierta

`write_private_with` usa el mismo temporal para cada destino. La GUI y el
monitor pueden pisar `sesion.tmp`; renombrar no evita esa colisión ni
resuelve dos operaciones de leer, modificar y escribir.

Todas las operaciones de sesión deben usar un bloqueo compartido y
temporales únicos. Releer el token al adquirir el bloqueo, antes de
renovar. La GUI también debe adoptar un token renovado por el monitor.
Si el bloqueo está ocupado, el ciclo del monitor puede terminar y esperar
la siguiente ejecución.

Cerrar la ventana permite que sigan los avisos. Cerrar sesión
explícitamente debe detener el monitor para esa cuenta y cancelar sus
recordatorios pendientes. Revalidar la cuenta y ese estado antes de
guardar un login o emitir un aviso, por si el usuario cerró sesión durante
una consulta. Una operación que empezó antes del logout no debe restaurar
la sesión después.

No escribir la caché grande de las vistas desde el monitor. Guardar un
estado propio y compacto, con permisos privados, por cuenta. Esto evita
pisar `cursos.json` y cargar contenido que no necesita.

## Novedades y recordatorios sin repetición

Guardar identificadores por cuenta, curso y actividad, el vencimiento
efectivo y los avisos ya programados o enviados. Usar identificadores
estables también para las solicitudes al sistema.

- La primera consulta exitosa de cada curso establece la referencia.
  No anuncia todas las actividades antiguas como nuevas.
- Un identificador que aparece después genera un aviso. Cambiar el título
  no convierte una actividad conocida en nueva.
- Los recordatorios iniciales propuestos son a 24 horas y a dos horas del
  vencimiento, dentro de un horizonte de siete días. Omitir entregadas,
  calificadas y fechas inválidas. Al volver de suspensión, agrupar lo
  pendiente en vez de emitir todos los umbrales atrasados.
- Si cambia la fecha o se entrega la actividad, reemplazar o cancelar sus
  solicitudes pendientes. Una entrega realizada en la web se conoce en
  la siguiente consulta exitosa, por lo que puede existir un aviso
  desactualizado durante ese intervalo o mientras no haya conexión.
- Un curso que falla conserva su estado anterior. Validar el formato de
  la respuesta; hoy `rows_of` puede convertir una respuesta inesperada
  en una lista vacía. No interpretar errores como eliminaciones.
- Conservar identificadores vistos durante la vigencia del curso.
  Limpiarlos 30 días después de confirmar que el curso terminó. No borrar
  la referencia solo porque una actividad desapareció temporalmente.

El estado debe sobrevivir a cerrar la app, reiniciar y terminar el helper.
Registrar el éxito después de que el sistema acepte la solicitud. Hay una
ventana de fallo entre esa aceptación y la escritura del estado. Usar IDs
estables y reconciliar solicitudes pendientes o entregadas al reiniciar;
no prometer entrega exactamente una vez bajo cualquier interrupción.

La lista actual no expone todo el estado de cierre de una actividad.
Comprobar si Eminus lo incluye en las respuestas antes de prometer que
jamás habrá un aviso de una actividad cerrada manualmente por el docente.

## Ejecución y límites del sistema

El manual local `man launchd.plist` distingue los temporizadores.
`StartInterval` pierde una ejecución que coincida con suspensión.
`StartCalendarInterval` recupera las ejecuciones al despertar y las reúne
en una sola. Para el intervalo inicial, usar minutos 0 y 30, sin
`KeepAlive`, con `ProcessType` en `Background`. Ejecutar también una
comprobación al activar los avisos o iniciar la sesión del usuario.

Salir de GUI o TUI no detiene el servicio registrado. La suspensión impide
consultar novedades hasta despertar. Apagar el equipo o cerrar la sesión
del sistema tampoco permite hacer esas consultas. Una pantalla bloqueada
es distinta de una sesión cerrada; la presentación y las vistas previas
dependen de los ajustes del sistema. No se probó ese caso.

No encontramos una suscripción push en el proyecto. La detección propuesta
es periódica, con una demora normal de hasta 30 minutos más la consulta.
No promete avisos instantáneos ni conocimiento de novedades mientras el
equipo está apagado.

En Windows, la ruta equivalente es una tarea por usuario del Programador
de tareas y notificaciones con identidad propia. La ruta Win32 clásica
usa un AUMID registrado y un acceso directo en Inicio.
[Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/send-local-toast-desktop-cpp-wrl).

En Linux, usar un timer `systemd --user` y `notify-rust` sobre el bus de
notificaciones de la sesión gráfica. `Persistent=true` recupera ejecuciones
perdidas para timers con `OnCalendar`, no para todos los temporizadores.
[Especificación de notificaciones](https://specifications.freedesktop.org/notification/latest/),
[manual fuente de systemd](https://github.com/systemd/systemd/blob/main/man/systemd.timer.xml).
Estas rutas no se probaron en Windows ni Linux.

## Alternativas

Un proceso Rust permanente permite conservar conexiones y estado, pero
retiene RAM entre consultas y necesita manejar suspensión y reloj. No
aporta detección instantánea si Eminus solo se consulta periódicamente.

Mantener la GUI oculta aprovecha el código de la app, pero conserva sus
dependencias de ventanas y memoria. Es la opción que peor encaja con el
requisito de consumo mínimo.

El proceso periódico añade empaquetado, registro y un arranque por ciclo.
Es el que recomiendo para este proyecto. El coste de TLS y arranque debe
medirse también como CPU y energía, no solo como memoria.

## Implementación y comprobación

1. Separar dependencias y tipos de interfaz mediante funciones de Cargo.
   Crear el binario y la lógica de novedades sin inicializar `App`.
2. Extraer la renovación común, conservar estados HTTP y resolver el
   bloqueo de sesión, incluidos logout y cambios de cuenta.
3. Implementar el estado compacto y las reglas de recordatorio. Usar el
   patrón de servidor local que ya tienen las pruebas de API.
4. Armar el paquete macOS y comprobar autorización, envío y cancelación
   con avisos de prueba. Esperar las respuestas del framework antes de
   terminar el proceso.
5. Registrar el LaunchAgent desde un control visible de activar y
   desactivar avisos. Mostrar si el sistema denegó el servicio o las
   notificaciones. Desactivar debe cancelar los recordatorios pendientes.
6. Probar la primera consulta, un ID nuevo, cursos fallidos, prórrogas,
   entregas, token vencido, errores 403 y 429, reinicio, denegación del
   llavero y logout durante una consulta.
7. Probar la entrega con ambos procesos de interfaz cerrados, después de
   suspensión y después de reiniciar. Verificar la identidad mostrada por
   macOS y la ausencia de solicitudes repetidas.

Medir release con el mismo servidor falso y datos de las pruebas actuales,
incluidos 30 cursos y 1.800 actividades. Separar un ciclo con token válido,
uno con login y el primer login sin llave de cifrado cacheada. Usar
`/usr/bin/time -l` para RSS máximo, tiempo real y CPU. Durante un ciclo
retenido por el servidor, inspeccionar `ps`, `vmmap` o `footprint`. Repetir
con respuestas grandes y con una conexión fallida.

Comprobar que el helper desaparece después del ciclo y no vuelve a
ejecutarse hasta la siguiente fecha. Medir actividad y energía durante
varios ciclos; la ausencia de proceso residente no significa que las
consultas sean gratis.

No hay una cifra medida de RAM para este monitor, porque aún no existe.
Los números de GUI, TUI o asignaciones del README no permiten predecirla.
Quedan por probar la entrega real desde launchd, el acceso del helper al
llavero y si Eminus invalida un token anterior al iniciar otra sesión.
