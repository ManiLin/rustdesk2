#!/usr/bin/env bash
set -euo pipefail

missing=()
for cmd in clang cmake curl gcc git g++ pkg-config wget nasm ninja; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    missing+=("$cmd")
  fi
done

for pkg in gtk+-3.0; do
  if ! pkg-config --exists "$pkg" 2>/dev/null; then
    missing+=("pkg-config:$pkg")
  fi
done

# libclang-dev does not always ship a pkg-config file; clang + headers are enough.
if ! test -f /usr/include/clang-c/Index.h 2>/dev/null; then
  found_header=""
  for h in /usr/lib/llvm-*/include/clang-c/Index.h /usr/lib/clang/*/include/clang-c/Index.h; do
    if test -f "$h"; then
      found_header=1
      break
    fi
  done
  if test -z "$found_header"; then
    missing+=("libclang-dev (clang-c/Index.h)")
  fi
fi

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
