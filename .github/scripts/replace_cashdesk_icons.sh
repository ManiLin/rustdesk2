#!/usr/bin/env bash
# Branding: custom icons + Windows version info (all builds).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP_NAME="${RUSTDESK_APP_NAME:-TnursRemoteDesk}"
EXE_BASE="$(echo "$APP_NAME" | tr '[:upper:]' '[:lower:]')"

echo "Applying branding: app=$APP_NAME exe=${EXE_BASE}.exe"

# App icon (graphic only, no "Татнефть-УРС" text)
cp -v "$ROOT/res/cashdesk_icon.png"      "$ROOT/res/icon.png"
cp -v "$ROOT/res/cashdesk_icon.ico"      "$ROOT/res/icon.ico"
cp -v "$ROOT/res/cashdesk_mac-icon.png"  "$ROOT/res/mac-icon.png"
cp -v "$ROOT/res/cashdesk_128x128.png"   "$ROOT/res/128x128.png"
cp -v "$ROOT/res/cashdesk_64x64.png"     "$ROOT/res/64x64.png"
cp -v "$ROOT/res/cashdesk_32x32.png"     "$ROOT/res/32x32.png"
cp -v "$ROOT/res/cashdesk_icon.png"      "$ROOT/res/128x128@2x.png"
cp -v "$ROOT/res/cashdesk_tray-icon.ico" "$ROOT/res/tray-icon.ico"

# Windows Flutter exe icon (Runner.rc → embedded in .exe) + taskbar at runtime (win32_window.cpp)
mkdir -p "$ROOT/flutter/windows/runner/resources"
cp -v "$ROOT/res/cashdesk_icon.ico" "$ROOT/flutter/windows/runner/resources/app_icon.ico"
cp -v "$ROOT/res/cashdesk_icon.ico" "$ROOT/flutter/assets/icon.ico"
cp -v "$ROOT/res/cashdesk_icon.png" "$ROOT/flutter/assets/icon.png" 2>/dev/null || true

# Windows Flutter runner version metadata
RUNNER_RC="$ROOT/flutter/windows/runner/Runner.rc"
if [ -f "$RUNNER_RC" ]; then
  sed -i \
    -e "s/RustDesk Remote Desktop/${APP_NAME} Remote Desktop/g" \
    -e "s/VALUE \"InternalName\", \"rustdesk\"/VALUE \"InternalName\", \"${EXE_BASE}\"/g" \
    -e "s/VALUE \"OriginalFilename\", \"rustdesk.exe\"/VALUE \"OriginalFilename\", \"${EXE_BASE}.exe\"/g" \
    -e "s/VALUE \"ProductName\", \"RustDesk\"/VALUE \"ProductName\", \"${APP_NAME}\"/g" \
    "$RUNNER_RC"
  echo "Patched $RUNNER_RC"
fi

echo "Branding applied."
