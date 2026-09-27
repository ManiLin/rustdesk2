#!/usr/bin/env bash
# Upload a built artifact to the inventory portal as a PENDING build.
# Distribution to clients happens only after an admin approves it in the UI.
#
# Usage:
#   upload-build-to-portal.sh <portal-base-url> <token> <file> <version> <flavor> <platform>
#
# <flavor>   normal | cashdesk
# <platform> windows | linux | macos | android
#
# The portal computes and stores the sha256 itself.
set -euo pipefail

PORTAL_URL="${1:?portal base url is required}"
TOKEN="${2:?ci token is required}"
FILE="${3:?file is required}"
VERSION="${4:?version is required}"
FLAVOR="${5:-normal}"
PLATFORM="${6:-windows}"

if [ ! -f "$FILE" ]; then
  echo "upload-build-to-portal: file not found: $FILE" >&2
  exit 1
fi

echo "upload-build-to-portal: uploading $FILE (version=$VERSION flavor=$FLAVOR platform=$PLATFORM)"
curl -fsS -X POST "${PORTAL_URL%/}/api/v1/ci/builds" \
  -H "Authorization: Bearer ${TOKEN}" \
  -F "version=${VERSION}" \
  -F "flavor=${FLAVOR}" \
  -F "platform=${PLATFORM}" \
  -F "file=@${FILE}"
echo
echo "upload-build-to-portal: done (status=pending, awaiting admin approval)"
