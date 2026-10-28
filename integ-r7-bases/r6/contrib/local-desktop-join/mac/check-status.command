#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
SHARE_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"

cd "$SHARE_DIR"
chmod +x scripts/misaka-desktop-node.sh
scripts/misaka-desktop-node.sh status

printf '\nDone. Press Enter to close this window.'
read -r _
