#!/usr/bin/env sh
set -eu

VPS_IP="${1:-${VPS_IP:-}}"
NETWORK="${MISAKA_NETWORK:-testnet-10}"
WEB_PORT="${MISAKA_SETUP_PORT:-8787}"
REPO_URL="${MISAKA_REPO_URL:-https://github.com/MISAKA-BTC/misakas.git}"
REMOTE_REPO="${MISAKA_REPO_DIR:-/opt/misakas}"

if [ -z "$VPS_IP" ]; then
  echo "Usage: $0 <VPS_PUBLIC_IP>"
  echo
echo "Example:"
  echo "  $0 203.0.113.10"
  exit 2
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
LOCAL_REPO=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

make_token() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 24
  else
    date "+%s" | shasum | awk '{print $1}'
  fi
}

TOKEN="${MISAKA_SETUP_TOKEN:-$(make_token)}"
URL="http://${VPS_IP}:${WEB_PORT}/setup?token=${TOKEN}"

echo "== MISAKA local Web UI fresh start =="
echo "VPS:      ${VPS_IP}"
echo "Network:  ${NETWORK}"
echo "Port:     ${WEB_PORT}"
echo "Local:    ${LOCAL_REPO}"
echo "Remote:   ${REMOTE_REPO}"
echo

echo "== prepare VPS source and toolchain =="
ssh root@"$VPS_IP" "MISAKA_REPO_URL='$REPO_URL' MISAKA_REPO_DIR='$REMOTE_REPO' bash -s" <<'REMOTE'
set -eu

export DEBIAN_FRONTEND=noninteractive

apt update
apt -y install \
  curl git ca-certificates \
  build-essential pkg-config libssl-dev \
  protobuf-compiler clang lld tmux ufw rsync

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi

cargo --version
rustc --version

rm -rf "$MISAKA_REPO_DIR"
git clone "$MISAKA_REPO_URL" "$MISAKA_REPO_DIR"
REMOTE

echo
echo "== sync local working tree to VPS =="
rsync -av --delete \
  --exclude '.git' \
  --exclude 'target' \
  --exclude '.DS_Store' \
  "$LOCAL_REPO"/ \
  root@"$VPS_IP":"$REMOTE_REPO"/

echo
echo "== build misaka CLI and start remote Web UI =="
ssh root@"$VPS_IP" "MISAKA_PUBLIC_IP='$VPS_IP' MISAKA_NETWORK='$NETWORK' MISAKA_SETUP_PORT='$WEB_PORT' MISAKA_SETUP_TOKEN='$TOKEN' MISAKA_REPO_URL='$REPO_URL' MISAKA_REPO_DIR='$REMOTE_REPO' bash -s" <<'REMOTE'
set -eu

. "$HOME/.cargo/env"

cd "$MISAKA_REPO_DIR"

cargo build --release -p misaka-cli
install -o root -g root -m 0755 target/release/misaka /usr/local/bin/misaka

/usr/local/bin/misaka setup --help >/dev/null

ufw allow "${MISAKA_SETUP_PORT}/tcp" || true

cat >/tmp/misaka-setup-web.sh <<'EOS'
#!/usr/bin/env sh
set -eu
exec /usr/local/bin/misaka \
  --network "$MISAKA_NETWORK" \
  setup web \
  --public \
  --public-ip "$MISAKA_PUBLIC_IP" \
  --port "$MISAKA_SETUP_PORT" \
  --token "$MISAKA_SETUP_TOKEN" \
  --repo-dir "$MISAKA_REPO_DIR" \
  --repo-url "$MISAKA_REPO_URL"
EOS
chmod 0700 /tmp/misaka-setup-web.sh

tmux kill-session -t misaka-setup-web 2>/dev/null || true
tmux new-session -d -s misaka-setup-web \
  "env MISAKA_PUBLIC_IP='$MISAKA_PUBLIC_IP' MISAKA_NETWORK='$MISAKA_NETWORK' MISAKA_SETUP_PORT='$MISAKA_SETUP_PORT' MISAKA_SETUP_TOKEN='$MISAKA_SETUP_TOKEN' MISAKA_REPO_DIR='$MISAKA_REPO_DIR' MISAKA_REPO_URL='$MISAKA_REPO_URL' /tmp/misaka-setup-web.sh"

sleep 2
tmux has-session -t misaka-setup-web
REMOTE

echo
echo "MISAKA Setup Web UI:"
echo "  ${URL}"
echo

if command -v open >/dev/null 2>&1; then
  open "$URL"
elif command -v xdg-open >/dev/null 2>&1; then
  xdg-open "$URL" >/dev/null 2>&1 || true
else
  echo "Open the URL above in your browser."
fi

echo "Remote Web UI is running in tmux session: misaka-setup-web"
echo "To stop it:"
echo "  ssh root@${VPS_IP} 'tmux kill-session -t misaka-setup-web'"
