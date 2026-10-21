# Widgets y Live Activity

Extensión `EminusWidgets` (`mx.hiram.BetterThanEminus.Widgets`):

- **Pendientes**: widget pequeño (próxima entrega), mediano (tres
  próximas, cada una abre su actividad) y de pantalla bloqueada
  (rectangular, circular con las que vencen en 24 h y en línea).
- **Próxima entrega**: Live Activity en la pantalla bloqueada y la Dynamic
  Island cuando una entrega vence en 8 horas o menos.

Los widgets no se conectan a Eminus. Leen `agenda.json`, que la app escribe
con `AgendaStore.save` en el App Group `group.mx.hiram.BetterThanEminus`.
Ese archivo sólo trae título, curso, fecha, el JSON para abrir la actividad
y la cuenta. No guarda tokens, cookies ni contraseñas.

## Enlaces

| URL | Abre |
|-----|------|
| `eminus://agenda` | la lista de pendientes |
| `eminus://activity?target=<JSON>` | una actividad; el JSON va codificado con porcentajes |

`AgendaLink(url:)` interpreta las dos formas.

## Comprobar la selección de tareas

```bash
cd ios
swiftc -parse-as-library Shared/Agenda.swift Shared/Checks/AgendaChecks.swift -o /tmp/agenda-checks && /tmp/agenda-checks
```

## Cuenta gratuita de Apple

La Live Activity no necesita App Group. Sólo usa la clave
`NSSupportsLiveActivities` y funciona con cualquier firma.

Los widgets necesitan el App Group para leer lo que guarda la app. Si Xcode
no puede firmar con esa capacidad, selecciona un equipo que la admita. Para
probar la app sin compartir datos con los widgets puedes compilar sin ese
permiso:

```bash
xcodebuild -project BetterThanEminus.xcodeproj -scheme BetterThanEminus \
  -destination 'id=<iPhone>' -allowProvisioningUpdates \
  CODE_SIGN_ENTITLEMENTS= build
```

Sin App Group, `AgendaStore` guarda la agenda dentro de la app. La app, los
avisos y la Live Activity siguen funcionando, y los widgets muestran "Abre
Eminus para cargar tus pendientes". `AgendaStore.isShared` dice cuál de los
dos casos aplica.

La extensión cuenta como otro App ID. Con cuenta gratuita el límite es de 10
App IDs por semana, y la firma de la extensión vence a los 7 días, igual que
la de la app. `dev/renew.sh` renueva las dos porque compila el esquema
completo.
