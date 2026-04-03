#!/usr/bin/env bash
# One-time GitHub HTTPS auth for server terminal (bypasses broken Cursor git socket).
set -euo pipefail

CRED="${HOME}/.git-credentials"
CFG="${HOME}/.gitconfig"

# Cursor injects this; it fails in plain SSH/bash.
unset GIT_ASKPASS SSH_ASKPASS VSCODE_GIT_ASKPASS_NODE 2>/dev/null || true

git config --global credential.helper store

if [[ -f "${CRED}" ]] && grep -q 'github.com' "${CRED}" 2>/dev/null; then
  echo "OK: ${CRED} already has github.com entry."
  exit 0
fi

echo "Paste GitHub PAT."
echo "  Classic: enable 'repo' AND 'workflow' (without workflow, push of .github/workflows fails)."
echo "  Fine-grained: Contents + Actions (Read and write) on ManiLin/rustdesk2."
echo "Input hidden."
read -rs TOKEN
echo
if [[ -z "${TOKEN}" ]]; then
  echo "Empty token, abort." >&2
  exit 1
fi

chmod 700 "$(dirname "${CRED}")" 2>/dev/null || true
printf 'https://x-access-token:%s@github.com\n' "${TOKEN}" > "${CRED}"
chmod 600 "${CRED}"
unset TOKEN

echo "Saved to ${CRED}. Test: cd /opt/rustdesk2 && git push origin refs/heads/master:refs/heads/master"
