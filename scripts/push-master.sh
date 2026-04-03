#!/usr/bin/env bash
set -euo pipefail

unset GIT_ASKPASS SSH_ASKPASS VSCODE_GIT_ASKPASS_NODE 2>/dev/null || true
export GIT_TERMINAL_PROMPT=0

cd /opt/rustdesk2
git push origin refs/heads/master:refs/heads/master
