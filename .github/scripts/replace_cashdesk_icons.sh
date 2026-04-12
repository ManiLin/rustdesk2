#!/usr/bin/env bash
# Replace default RustDesk icons with cashdesk branding.
# Called from CI when RUSTDESK_DESKTOP_UI_FLAVOR=cashdesk.
set -euo pipefail

if [ "${RUSTDESK_DESKTOP_UI_FLAVOR:-}" != "cashdesk" ]; then
  echo "Not a cashdesk build, skipping icon replacement."
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

cp -v "$ROOT/res/cashdesk_icon.png"      "$ROOT/res/icon.png"
cp -v "$ROOT/res/cashdesk_icon.ico"      "$ROOT/res/icon.ico"
cp -v "$ROOT/res/cashdesk_mac-icon.png"  "$ROOT/res/mac-icon.png"
cp -v "$ROOT/res/cashdesk_128x128.png"   "$ROOT/res/128x128.png"
cp -v "$ROOT/res/cashdesk_64x64.png"     "$ROOT/res/64x64.png"
cp -v "$ROOT/res/cashdesk_32x32.png"     "$ROOT/res/32x32.png"

# 128x128@2x is 256x256
cp -v "$ROOT/res/cashdesk_icon.png"      "$ROOT/res/128x128@2x.png"
cp -v "$ROOT/res/cashdesk_icon.ico"      "$ROOT/res/tray-icon.ico"

# Flutter asset icon
cp -v "$ROOT/flutter/assets/cashdesk_logo.png" "$ROOT/flutter/assets/icon.png" 2>/dev/null || true

echo "Cashdesk icons replaced."
