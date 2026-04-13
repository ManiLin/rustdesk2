#!/usr/bin/env bash
# Replace default RustDesk icons with cashdesk branding (graphic only, no text).
# Called from CI when RUSTDESK_DESKTOP_UI_FLAVOR=cashdesk.
set -euo pipefail

if [ "${RUSTDESK_DESKTOP_UI_FLAVOR:-}" != "cashdesk" ]; then
  echo "Not a cashdesk build, skipping icon replacement."
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# App icon (graphic only, no "Татнефть-УРС" text)
cp -v "$ROOT/res/cashdesk_icon.png"      "$ROOT/res/icon.png"
cp -v "$ROOT/res/cashdesk_icon.ico"      "$ROOT/res/icon.ico"
cp -v "$ROOT/res/cashdesk_mac-icon.png"  "$ROOT/res/mac-icon.png"
cp -v "$ROOT/res/cashdesk_128x128.png"   "$ROOT/res/128x128.png"
cp -v "$ROOT/res/cashdesk_64x64.png"     "$ROOT/res/64x64.png"
cp -v "$ROOT/res/cashdesk_32x32.png"     "$ROOT/res/32x32.png"
cp -v "$ROOT/res/cashdesk_icon.png"      "$ROOT/res/128x128@2x.png"

# Tray icon (graphic only, optimized for small sizes)
cp -v "$ROOT/res/cashdesk_tray-icon.ico" "$ROOT/res/tray-icon.ico"

# Flutter asset icon (graphic only for taskbar/window title)
cp -v "$ROOT/res/cashdesk_icon.png"      "$ROOT/flutter/assets/icon.png" 2>/dev/null || true

echo "Cashdesk icons replaced."
