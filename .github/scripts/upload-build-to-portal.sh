#!/usr/bin/env bash
# Upload a built artifact to the inventory portal as a PENDING build.
# Distribution to clients happens only after an admin approves it in the UI.
#
# Usage:
#   upload-build-to-portal.sh <portal-base-url> <token> <file> <version> <flavor> <platform>
#
# <flavor>   normal | cashdesk
# <platform> windows | linux | macos | android
# <architecture> x86_64 | aarch64 | x86 (optional)
#
# The portal computes and stores the sha256 itself.
set -euo pipefail

PORTAL_URL="${1:?portal base url is required}"
TOKEN="${2:?ci token is required}"
FILE="${3:?file is required}"
VERSION="${4:?version is required}"
FLAVOR="${5:-normal}"
PLATFORM="${6:-windows}"
ARCHITECTURE="${7:-}"

if [ ! -f "$FILE" ]; then
  echo "upload-build-to-portal: file not found: $FILE" >&2
  exit 1
fi

echo "upload-build-to-portal: uploading $FILE (version=$VERSION flavor=$FLAVOR platform=$PLATFORM)"
response="$(curl -sS -X POST "${PORTAL_URL%/}/api/v1/ci/builds" \
  -H "Authorization: Bearer ${TOKEN}" \
  -F "version=${VERSION}" \
  -F "flavor=${FLAVOR}" \
  -F "platform=${PLATFORM}" \
  -F "architecture=${ARCHITECTURE}" \
  -F "file=@${FILE}" \
  --write-out $'\n%{http_code}')"
http_code="${response##*$'\n'}"
response_body="${response%$'\n'*}"
if [[ ! "$http_code" =~ ^2[0-9][0-9]$ ]]; then
  error_message="$response_body"
  if command -v jq >/dev/null 2>&1; then
    parsed_message="$(printf '%s' "$response_body" | jq -r '.message // .code // empty' 2>/dev/null || true)"
    if [[ -n "$parsed_message" ]]; then
      error_message="$parsed_message"
    fi
  fi
  echo "upload-build-to-portal: HTTP ${http_code}: ${error_message}" >&2
  exit 1
fi
printf '%s\n' "$response_body"
echo "upload-build-to-portal: done (status=pending, awaiting admin approval)"
