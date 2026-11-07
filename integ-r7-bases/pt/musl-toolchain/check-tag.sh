#!/bin/bash
# Tag-consistency guard (audit H-5).
#
# The musl toolchain consumer (musl-toolchain/build.sh: TOOLCHAIN_TAG) and the
# publisher (.github/workflows/musl-toolchain.yaml: DEFAULT_RELEASE_TAG) must
# reference the SAME immutable release tag. A mismatch silently makes CI download
# from a tag that the publisher never pushed (fail-open / wrong-artifact risk),
# so this script extracts both and exits non-zero if they differ.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_SH="$SCRIPT_DIR/build.sh"
WORKFLOW="$SCRIPT_DIR/../.github/workflows/musl-toolchain.yaml"

if [ ! -f "$BUILD_SH" ]; then
  echo "ERROR: cannot find build.sh at $BUILD_SH" >&2
  exit 2
fi
if [ ! -f "$WORKFLOW" ]; then
  echo "ERROR: cannot find musl-toolchain.yaml at $WORKFLOW" >&2
  exit 2
fi

# Extract TOOLCHAIN_TAG="..." from build.sh (first match).
CONSUMER_TAG=$(grep -E '^TOOLCHAIN_TAG=' "$BUILD_SH" | head -n1 | sed -E 's/^TOOLCHAIN_TAG=["'\'']?([^"'\'' ]+).*/\1/')

# Extract DEFAULT_RELEASE_TAG: '...' from the workflow env (first match).
PUBLISHER_TAG=$(grep -E '^\s*DEFAULT_RELEASE_TAG:' "$WORKFLOW" | head -n1 | sed -E "s/.*DEFAULT_RELEASE_TAG:\s*['\"]?([^'\" ]+).*/\1/")

echo "build.sh TOOLCHAIN_TAG          = '$CONSUMER_TAG'"
echo "musl-toolchain.yaml DEFAULT_RELEASE_TAG = '$PUBLISHER_TAG'"

if [ -z "$CONSUMER_TAG" ] || [ -z "$PUBLISHER_TAG" ]; then
  echo "ERROR: failed to parse one or both tags" >&2
  exit 2
fi

if [ "$CONSUMER_TAG" != "$PUBLISHER_TAG" ]; then
  echo "ERROR (audit H-5): musl toolchain tag mismatch." >&2
  echo "  build.sh consumes:        $CONSUMER_TAG" >&2
  echo "  musl-toolchain.yaml pub.:  $PUBLISHER_TAG" >&2
  echo "  Bump both to the same immutable tag." >&2
  exit 1
fi

echo "OK: musl toolchain tags are consistent ($CONSUMER_TAG)"
