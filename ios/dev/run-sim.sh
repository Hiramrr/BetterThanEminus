#!/bin/sh
# Compila, instala y abre la app en el simulador en modo desarrollo.
set -e
cd "$(dirname "$0")/.."
DD=${DD:-/tmp/bte-dd}
xcodegen generate -q
xcodebuild -project BetterThanEminus.xcodeproj -scheme BetterThanEminus -sdk iphonesimulator -configuration Debug -derivedDataPath "$DD" build -quiet
xcrun simctl install booted "$DD/Build/Products/Debug-iphonesimulator/BetterThanEminus.app"
SIMCTL_CHILD_BTE_DEV=http://127.0.0.1:8777 xcrun simctl launch --terminate-running-process booted mx.hiram.BetterThanEminus
