#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
SHARE_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"

HOST="${MISAKA_DESKTOP_WEB_HOST:-127.0.0.1}"
PORT="${MISAKA_DESKTOP_WEB_PORT:-}"
TOKEN="${MISAKA_DESKTOP_WEB_TOKEN:-}"
DESKTOP_HOME="${MISAKA_DESKTOP_HOME:-$HOME/.misaka-desktop-node}"
STATE_DIR="$DESKTOP_HOME/state"
WEB_STATE="$STATE_DIR/web.env"
OPEN_BROWSER=1
FORCE_NEW=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --host)
      HOST="${2:?--host requires a value}"
      shift 2
      ;;
    --port)
      PORT="${2:?--port requires a value}"
      shift 2
      ;;
    --token)
      TOKEN="${2:?--token requires a value}"
      shift 2
      ;;
    --no-open)
      OPEN_BROWSER=0
      shift
      ;;
    --force-new)
      FORCE_NEW=1
      shift
      ;;
    -h|--help)
      cat <<'EOF'
Usage:
  scripts/misaka-desktop-web.sh [--host 127.0.0.1] [--port 8788] [--no-open] [--force-new]

Starts the local MISAKA desktop Web UI. It is intentionally bound to localhost
by default and controls scripts/misaka-desktop-node.sh on this machine.

If a previous local Web UI is still running, this command reopens that page
instead of starting another server. Use --force-new to start a new one.
EOF
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

open_url() {
  local url="$1"
  case "$(uname -s)" in
    Darwin)
      open "$url" >/dev/null 2>&1 || true
      ;;
    Linux)
      if grep -qi microsoft /proc/version 2>/dev/null && command -v cmd.exe >/dev/null 2>&1; then
        cmd.exe /c start "" "$url" >/dev/null 2>&1 || true
      elif command -v xdg-open >/dev/null 2>&1; then
        xdg-open "$url" >/dev/null 2>&1 || true
      fi
      ;;
  esac
}

is_wsl() {
  [ "$(uname -s)" = "Linux" ] && grep -qi microsoft /proc/version 2>/dev/null
}

linux_prepare_deps_ready() {
  command -v dpkg >/dev/null 2>&1 || return 1
  for cmd in curl git pkg-config protoc clang lld rsync unzip python3; do
    command -v "$cmd" >/dev/null 2>&1 || return 1
  done
  dpkg -s \
    ca-certificates \
    build-essential \
    libssl-dev \
    protobuf-compiler \
    clang \
    lld \
    rsync \
    unzip \
    python3 >/dev/null 2>&1
}

install_linux_prepare_deps() {
  if [ "$(uname -s)" != "Linux" ] || ! command -v apt-get >/dev/null 2>&1; then
    return
  fi
  if linux_prepare_deps_ready; then
    return
  fi

  if [ "$(id -u)" -ne 0 ]; then
    if ! command -v sudo >/dev/null 2>&1; then
      echo "ERROR: sudo is required on Ubuntu/Debian to install build packages." >&2
      echo "Install the required packages manually or run this launcher as a user with sudo permission." >&2
      exit 1
    fi
    if is_wsl; then
      echo "WSL Ubuntu may ask for your Ubuntu password now."
    else
      echo "Linux may ask for your account password now."
    fi
    echo "Installing required packages before starting the Web UI."
    sudo -v
    sudo apt-get update
    sudo apt-get install -y \
      curl git ca-certificates build-essential pkg-config libssl-dev \
      protobuf-compiler clang lld rsync unzip python3
  else
    echo "Installing required packages before starting the Web UI."
    apt-get update
    apt-get install -y \
      curl git ca-certificates build-essential pkg-config libssl-dev \
      protobuf-compiler clang lld rsync unzip python3
  fi
}

install_linux_prepare_deps

if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 is required for the local Web UI." >&2
  echo "macOS: install Xcode Command Line Tools or Python 3." >&2
  echo "Ubuntu/Debian: sudo apt install -y python3" >&2
  exit 1
fi

if [ "$FORCE_NEW" = "0" ] && [ -f "$WEB_STATE" ]; then
  # This file is written by this script under the user's local MISAKA home.
  # shellcheck disable=SC1090
  . "$WEB_STATE" || true
  if [ -n "${WEB_URL:-}" ] && [ -n "${WEB_PING_URL:-}" ]; then
    if python3 - "$WEB_PING_URL" >/dev/null 2>&1 <<'PY'
import sys
import urllib.request

try:
    with urllib.request.urlopen(sys.argv[1], timeout=2) as response:
        raise SystemExit(0 if response.status == 200 else 1)
except Exception:
    raise SystemExit(1)
PY
    then
      echo "MISAKA local desktop Web UI is already running"
      echo
      echo "Open:"
      echo "  $WEB_URL"
      echo
      echo "Use --force-new if you intentionally want another local server."
      if [ "$OPEN_BROWSER" = "1" ]; then
        open_url "$WEB_URL"
      fi
      exit 0
    fi
  fi
fi

if [ -z "$PORT" ]; then
  PORT="$(python3 - <<'PY'
import socket
for port in range(8788, 8809):
    s = socket.socket()
    try:
        s.bind(("127.0.0.1", port))
        print(port)
        raise SystemExit
    except OSError:
        pass
    finally:
        s.close()
raise SystemExit("no free port in 8788-8808")
PY
)"
fi

if [ -z "$TOKEN" ]; then
  TOKEN="$(python3 - <<'PY'
import secrets
print(secrets.token_hex(20))
PY
)"
fi

URL="http://127.0.0.1:${PORT}/?token=${TOKEN}"
PING_URL="http://127.0.0.1:${PORT}/api/session/ping?token=${TOKEN}"

cd "$SHARE_DIR"
chmod +x scripts/misaka-desktop-node.sh
mkdir -p "$STATE_DIR"
chmod 700 "$DESKTOP_HOME" "$STATE_DIR" 2>/dev/null || true
cat > "$WEB_STATE" <<EOF
WEB_URL='$URL'
WEB_PING_URL='$PING_URL'
WEB_HOST='$HOST'
WEB_PORT='$PORT'
WEB_TOKEN='$TOKEN'
WEB_STARTED_AT='$(date -u +"%Y-%m-%dT%H:%M:%SZ")'
EOF
chmod 600 "$WEB_STATE" 2>/dev/null || true

echo "MISAKA local desktop Web UI"
echo
echo "Open:"
echo "  $URL"
echo
echo "This page is local-only by default. Keep this terminal open while using it."
echo "If you close the browser tab, run this command again to reopen the same page."

if [ "$OPEN_BROWSER" = "1" ]; then
  ( sleep 1; open_url "$URL" ) >/dev/null 2>&1 &
fi

exec python3 "$SCRIPT_DIR/misaka-desktop-web.py" \
  --host "$HOST" \
  --port "$PORT" \
  --token "$TOKEN" \
  --share-dir "$SHARE_DIR"
