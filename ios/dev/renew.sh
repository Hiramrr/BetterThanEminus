#!/bin/sh
# Renueva la firma de la app en el iPhone antes de que venzan los 7 días de
# la cuenta gratuita de Apple: pide un perfil nuevo, recompila e instala.
#
# Uso:  ios/dev/renew.sh            renueva si quedan menos de 3 días
#       ios/dev/renew.sh --force    renueva ahora
#       ios/dev/renew.sh --status   sólo dice cuánto queda
#       ios/dev/renew.sh --install-agent / --remove-agent
#
# El agente (launchd) lo corre cada 3 horas. Necesita el iPhone desbloqueado
# y a la vista de la Mac (cable o la misma red wifi); si no, lo reintenta en
# la siguiente vuelta. Los datos de la app no se tocan al reinstalar.
set -u
export PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

IOS="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE_ID="mx.hiram.BetterThanEminus"
STATE="$HOME/Library/Application Support/BetterThanEminus"
DERIVED="$HOME/Library/Caches/BetterThanEminus/DerivedData"
LOG="$HOME/Library/Logs/BetterThanEminus-renew.log"
AGENT="$HOME/Library/LaunchAgents/$BUNDLE_ID.renew.plist"
RENEW_BEFORE_DAYS=3
mkdir -p "$STATE" "$(dirname "$LOG")"

say() { echo "$(date '+%Y-%m-%d %H:%M:%S') $*" | tee -a "$LOG"; }
notify() { osascript -e "display notification \"$1\" with title \"Eminus para iPhone\"" >/dev/null 2>&1 || true; }

expiry_of() { # fecha de vencimiento (epoch) de un .mobileprovision
  security cms -D -i "$1" 2>/dev/null | plutil -extract ExpirationDate raw - 2>/dev/null |
    xargs -I{} date -j -u -f "%Y-%m-%dT%H:%M:%SZ" {} +%s 2>/dev/null
}

days_left() {
  [ -f "$STATE/expiry" ] || { echo 0; return; }
  echo $(( ($(cat "$STATE/expiry") - $(date +%s)) / 86400 ))
}

case "${1:-}" in
  --status)
    if [ -f "$STATE/expiry" ]; then
      echo "vence: $(date -r "$(cat "$STATE/expiry")" '+%a %d %b %H:%M') (quedan $(days_left) días)"
    else
      echo "sin registro: corre ios/dev/renew.sh --force"
    fi
    exit 0 ;;
  --install-agent)
    mkdir -p "$(dirname "$AGENT")"
    cat > "$AGENT" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$BUNDLE_ID.renew</string>
  <key>ProgramArguments</key>
  <array><string>/bin/sh</string><string>$IOS/dev/renew.sh</string></array>
  <key>StartInterval</key><integer>10800</integer>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>/dev/null</string>
  <key>StandardErrorPath</key><string>/dev/null</string>
</dict>
</plist>
EOF
    launchctl bootout "gui/$(id -u)/$BUNDLE_ID.renew" 2>/dev/null
    launchctl bootstrap "gui/$(id -u)" "$AGENT" && echo "agente instalado: revisa cada 3 horas ($AGENT)"
    exit 0 ;;
  --remove-agent)
    launchctl bootout "gui/$(id -u)/$BUNDLE_ID.renew" 2>/dev/null
    rm -f "$AGENT" && echo "agente quitado"
    exit 0 ;;
esac

FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

if [ "$FORCE" = 0 ] && [ "$(days_left)" -ge "$RENEW_BEFORE_DAYS" ]; then
  exit 0 # todavía falta: nada que hacer
fi

# Un iPhone físico emparejado y a la vista.
DEVICE=$(xcrun devicectl list devices 2>/dev/null |
  awk '/physical/ && /iPhone/ && (/available/ || /connected/) && !/unavailable/ { for (i = 1; i <= NF; i++) if ($i ~ /^[0-9A-F]{8}-[0-9A-F]{16}$/) { print $i; exit } }')
if [ -z "$DEVICE" ]; then
  say "sin iPhone a la vista (quedan $(days_left) días); se reintenta luego"
  [ "$(days_left)" -le 1 ] && notify "La app vence pronto: conecta o acerca el iPhone desbloqueado."
  exit 1
fi

# Sin el perfil guardado, Xcode pide uno nuevo con 7 días desde hoy.
for f in "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"/*.mobileprovision \
         "$HOME/Library/MobileDevice/Provisioning Profiles"/*.mobileprovision; do
  [ -f "$f" ] || continue
  name=$(security cms -D -i "$f" 2>/dev/null | plutil -extract Name raw - 2>/dev/null)
  case "$name" in *"$BUNDLE_ID"*) rm -f "$f" ;; esac
done

cd "$IOS" || exit 1
command -v xcodegen >/dev/null 2>&1 && xcodegen generate -q >>"$LOG" 2>&1

say "renovando en $DEVICE…"
if ! xcodebuild -project BetterThanEminus.xcodeproj -scheme BetterThanEminus -configuration Debug \
    -destination "id=$DEVICE" -derivedDataPath "$DERIVED" -allowProvisioningUpdates build >"$STATE/build.log" 2>&1; then
  reason=$(grep -m1 -E "error:|needs to be unlocked" "$STATE/build.log" | cut -c1-160)
  say "no se pudo compilar: ${reason:-ver $STATE/build.log}"
  [ "$(days_left)" -le 1 ] && notify "No se pudo renovar: desbloquea el iPhone cerca de la Mac."
  exit 1
fi

APP="$DERIVED/Build/Products/Debug-iphoneos/BetterThanEminus.app"
if ! xcrun devicectl device install app --device "$DEVICE" "$APP" >>"$STATE/build.log" 2>&1; then
  say "compiló, pero no se pudo instalar (¿iPhone bloqueado?)"
  exit 1
fi

expiry_of "$APP/embedded.mobileprovision" > "$STATE/expiry"
say "renovada: vence $(date -r "$(cat "$STATE/expiry")" '+%a %d %b %H:%M')"
notify "App renovada hasta el $(date -r "$(cat "$STATE/expiry")" '+%d %b')."
