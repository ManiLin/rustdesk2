#!/usr/bin/env bash
set -euo pipefail

missing=()
for cmd in clang cmake curl gcc git g++ pkg-config wget nasm ninja; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    missing+=("$cmd")
  fi
done

for pkg in gtk+-3.0 libclang; do
  if ! pkg-config --exists "$pkg" 2>/dev/null; then
    missing+=("pkg-config:$pkg")
  fi
done

if ((${#missing[@]} > 0)); then
  echo "Missing self-hosted bridge build dependencies:"
  printf '  - %s\n' "${missing[@]}"
  cat <<'EOF'

Install once on the Linux runner host (as root), then restart actions-runner:

  apt-get update
  apt-get install -y \
    ca-certificates clang cmake curl gcc git g++ \
    libclang-dev libgtk-3-dev llvm-dev nasm ninja-build pkg-config wget

Optional passwordless sudo for the runner user is NOT required by this workflow.
EOF
  exit 1
fi

echo "Self-hosted Linux bridge dependencies OK."
