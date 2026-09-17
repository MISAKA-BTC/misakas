#!/usr/bin/env bash
set -Eeuo pipefail

NETWORK="${MISAKA_NETWORK:-testnet-10}"
WEB_PORT="${MISAKA_SETUP_PORT:-8787}"
PUBLIC_IP="${MISAKA_PUBLIC_IP:-}"
CLIENT_IP="${MISAKA_ALLOW_CLIENT_IP:-}"
REPO_URL="${MISAKA_REPO_URL:-https://github.com/MISAKA-BTC/misakas.git}"
NODE_SERVICE="${MISAKA_KASPAD_SERVICE:-misaka-kaspad}"
SERVICE_USER="${MISAKA_SERVICE_USER:-}"
APPDIR="${MISAKA_APPDIR:-}"
SKIP_APT="${MISAKA_SKIP_APT:-0}"
SKIP_BUILD="${MISAKA_SKIP_BUILD:-0}"
RUN_TESTS="${MISAKA_RUN_TESTS:-0}"
CHECK_ONLY="${MISAKA_CHECK_ONLY:-0}"
ENABLE_UTXO_INDEX="${MISAKA_ENABLE_UTXO_INDEX:-0}"
TMUX_SESSION="misaka-setup-web"
SETUP_DIR="/var/log/misaka-setup"
LAUNCH_SCRIPT="$SETUP_DIR/start-vps-webui.sh"
WEB_LOG="$SETUP_DIR/vps-webui.log"

usage() {
  cat <<'EOF'
Start the MISAKA setup Web UI directly on an Ubuntu/Debian VPS.

Usage:
  sudo ./scripts/misaka-vps-webui-start.sh [options]

Options:
  --network <id>            Network id (default: testnet-10)
  --port <port>             Web UI port (default: 8787)
  --public-ip <IPv4>        VPS public IPv4; auto-detected when omitted
  --allow-client-ip <IPv4>  Browser/source IPv4; SSH client is auto-detected
  --service <name>          Existing kaspad systemd service (default: misaka-kaspad)
  --service-user <user>     Existing service user; auto-detected from systemd
  --appdir <path>           Existing kaspad appdir; auto-detected from the running process
  --skip-apt                Do not run apt-get update/install
  --skip-build              Reuse already-installed Web UI support binaries
  --run-tests               Run misaka-cli tests before installation
  --check-only              Show service, appdir, UTXO index, disk, and exit
  --enable-utxoindex        Back up and update an existing node, then exit
  -h, --help                Show this help

Environment equivalents:
  MISAKA_NETWORK
  MISAKA_SETUP_PORT
  MISAKA_PUBLIC_IP
  MISAKA_ALLOW_CLIENT_IP
  MISAKA_KASPAD_SERVICE
  MISAKA_SERVICE_USER
  MISAKA_APPDIR
  MISAKA_SKIP_APT=1
  MISAKA_SKIP_BUILD=1
  MISAKA_RUN_TESTS=1
  MISAKA_CHECK_ONLY=1
  MISAKA_ENABLE_UTXO_INDEX=1

When an existing systemd node is found, this script preserves its service user
and appdir. It installs only the Web UI support binaries; it does not replace or
restart kaspad and does not delete node data, validator keys, bonds, or setup state.
The explicit --enable-utxoindex operation is the only exception: it backs up the
existing unit, adds only --utxoindex, validates it, and restarts an active node once.
EOF
}

log() {
  printf '\n== %s ==\n' "$*"
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

need_value() {
  if [[ $# -lt 2 || -z "${2:-}" ]]; then
    die "$1 requires a value"
  fi
}

valid_ipv4() {
  local value="$1"
  local part
  local -a parts

  [[ "$value" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || return 1
  IFS='.' read -r -a parts <<<"$value"
  [[ ${#parts[@]} -eq 4 ]] || return 1
  for part in "${parts[@]}"; do
    [[ "$part" =~ ^[0-9]+$ ]] || return 1
    ((10#$part >= 0 && 10#$part <= 255)) || return 1
  done
}

detect_public_ipv4() {
  local endpoint
  local value
  for endpoint in \
    https://api.ipify.org \
    https://ifconfig.me/ip \
    https://icanhazip.com
  do
    value="$(curl -4fsS --max-time 8 "$endpoint" 2>/dev/null | tr -d '[:space:]' || true)"
    if valid_ipv4 "$value"; then
      printf '%s\n' "$value"
      return 0
    fi
  done
  return 1
}

detect_ssh_client_ipv4() {
  local value=""

  if [[ -n "${SSH_CLIENT:-}" ]]; then
    value="${SSH_CLIENT%% *}"
  elif [[ -n "${SSH_CONNECTION:-}" ]]; then
    value="${SSH_CONNECTION%% *}"
  else
    value="$(who -m 2>/dev/null | sed -n 's/.*(\([^()]\{1,64\}\)).*/\1/p' | head -n 1 || true)"
  fi

  if valid_ipv4 "$value"; then
    printf '%s\n' "$value"
    return 0
  fi
  return 1
}

service_loaded() {
  [[ "$(systemctl show "$NODE_SERVICE" -p LoadState --value 2>/dev/null || true)" == "loaded" ]]
}

detect_existing_service_user() {
  if [[ -n "$SERVICE_USER" ]]; then
    return
  fi
  if service_loaded; then
    SERVICE_USER="$(systemctl show "$NODE_SERVICE" -p User --value 2>/dev/null || true)"
    if [[ -z "$SERVICE_USER" ]]; then
      SERVICE_USER="root"
    fi
  else
    SERVICE_USER="misaka_user"
  fi
}

detect_existing_appdir() {
  local main_pid=""
  local arg=""
  local exec_start=""

  if [[ -n "$APPDIR" ]]; then
    return
  fi
  if service_loaded; then
    main_pid="$(systemctl show "$NODE_SERVICE" -p MainPID --value 2>/dev/null || true)"
    if [[ "$main_pid" =~ ^[1-9][0-9]*$ && -r "/proc/$main_pid/cmdline" ]]; then
      while IFS= read -r arg; do
        if [[ "$arg" == --appdir=* ]]; then
          APPDIR="${arg#--appdir=}"
          break
        fi
      done < <(tr '\0' '\n' < "/proc/$main_pid/cmdline")
    fi
    if [[ -z "$APPDIR" ]]; then
      exec_start="$(systemctl show "$NODE_SERVICE" -p ExecStart --value 2>/dev/null || true)"
      APPDIR="$(sed -n 's/.*--appdir=\([^ ;}]*\).*/\1/p' <<<"$exec_start" | head -n 1)"
    fi
  fi
  APPDIR="${APPDIR:-/var/lib/misaka}"
}

utxo_index_enabled() {
  local main_pid=""
  local arg=""
  local exec_start=""

  service_loaded || return 1
  main_pid="$(systemctl show "$NODE_SERVICE" -p MainPID --value 2>/dev/null || true)"
  if [[ "$main_pid" =~ ^[1-9][0-9]*$ && -r "/proc/$main_pid/cmdline" ]]; then
    while IFS= read -r arg; do
      if [[ "$arg" == "--utxoindex" ]]; then
        return 0
      fi
    done < <(tr '\0' '\n' < "/proc/$main_pid/cmdline")
    return 1
  fi

  exec_start="$(systemctl show "$NODE_SERVICE" -p ExecStart --value 2>/dev/null || true)"
  grep -Eq '(^|[[:space:];{])--utxoindex([[:space:];}]|$)' <<<"$exec_start"
}

available_disk_kib() {
  df -Pk "$APPDIR" 2>/dev/null | awk 'NR == 2 { print $4 }'
}

enable_existing_utxo_index() {
  local unit_file=""
  local backup_file=""
  local candidate_file=""
  local unit_mode=""
  local unit_uid=""
  local unit_gid=""
  local node_was_active=""
  local miner_was_active=""
  local after_pid=""
  local repaired=0

  service_loaded || die "--enable-utxoindex requires an existing $NODE_SERVICE systemd service"
  if utxo_index_enabled; then
    printf '\nUTXO index is already enabled. No service or data was changed.\n'
    return 0
  fi

  command -v systemd-analyze >/dev/null 2>&1 || die "systemd-analyze is required for safe unit validation"
  unit_file="$(systemctl show "$NODE_SERVICE" -p FragmentPath --value 2>/dev/null || true)"
  [[ "$unit_file" == /* && -f "$unit_file" ]] || \
    die "could not find the editable unit file for $NODE_SERVICE: ${unit_file:-missing}"
  grep -Eq '^[[:space:]]*ExecStart=.*kaspad' "$unit_file" || \
    die "refusing to edit a unit without a kaspad ExecStart: $unit_file"

  backup_file="${unit_file}.before-utxoindex-$(date -u +%Y%m%dT%H%M%SZ)-$$"
  candidate_file="$(mktemp "/tmp/${NODE_SERVICE}.utxoindex.XXXXXX")"
  cp -a "$unit_file" "$backup_file"

  if grep -Eq '(^|[[:space:]])--utxoindex([[:space:]\\]|$)' "$unit_file"; then
    cp "$unit_file" "$candidate_file"
  elif ! awk '
    BEGIN { updated = 0 }
    {
      if (!updated && $0 ~ /^[[:space:]]*ExecStart=/ && $0 ~ /kaspad/) {
        if ($0 ~ /\\[[:space:]]*$/) {
          sub(/[[:space:]]*\\[[:space:]]*$/, " --utxoindex " sprintf("%c", 92))
        } else {
          $0 = $0 " --utxoindex"
        }
        updated = 1
      }
      print
    }
    END { if (!updated) exit 42 }
  ' "$unit_file" >"$candidate_file"; then
    rm -f "$candidate_file"
    die "could not add --utxoindex to $unit_file; backup: $backup_file"
  fi

  grep -Eq '(^|[[:space:]])--utxoindex([[:space:]\\]|$)' "$candidate_file" || {
    rm -f "$candidate_file"
    die "candidate unit does not contain --utxoindex; backup: $backup_file"
  }

  unit_mode="$(stat -c '%a' "$unit_file")"
  unit_uid="$(stat -c '%u' "$unit_file")"
  unit_gid="$(stat -c '%g' "$unit_file")"
  if ! install -o "$unit_uid" -g "$unit_gid" -m "$unit_mode" "$candidate_file" "$unit_file"; then
    cp -a "$backup_file" "$unit_file"
    rm -f "$candidate_file"
    die "could not install the candidate unit; restored $backup_file"
  fi
  rm -f "$candidate_file"

  if ! systemd-analyze verify "$unit_file"; then
    cp -a "$backup_file" "$unit_file"
    systemctl daemon-reload || true
    die "unit validation failed; restored $backup_file"
  fi

  node_was_active="$(systemctl is-active "$NODE_SERVICE" 2>/dev/null || true)"
  miner_was_active="$(systemctl is-active misaka-miner.service 2>/dev/null || true)"
  if [[ "$miner_was_active" == "active" ]]; then
    if ! systemctl stop misaka-miner.service; then
      cp -a "$backup_file" "$unit_file"
      systemctl daemon-reload || true
      die "could not stop the active miner; restored $backup_file"
    fi
  fi

  if ! systemctl daemon-reload; then
    cp -a "$backup_file" "$unit_file"
    systemctl daemon-reload || true
    if [[ "$miner_was_active" == "active" ]]; then
      systemctl restart misaka-miner.service || true
    fi
    die "systemd daemon-reload failed; restored $backup_file"
  fi
  if [[ "$node_was_active" == "active" ]]; then
    if systemctl restart "$NODE_SERVICE"; then
      for _attempt in {1..30}; do
        if [[ "$(systemctl is-active "$NODE_SERVICE" 2>/dev/null || true)" == "active" ]] && utxo_index_enabled; then
          repaired=1
          break
        fi
        sleep 1
      done
    fi

    if [[ "$repaired" != "1" ]]; then
      cp -a "$backup_file" "$unit_file"
      systemctl daemon-reload || true
      systemctl restart "$NODE_SERVICE" || true
      if [[ "$miner_was_active" == "active" ]]; then
        systemctl restart misaka-miner.service || true
      fi
      die "node verification failed; restored $backup_file and restarted the previous unit"
    fi
  else
    repaired=1
  fi

  after_pid="$(systemctl show "$NODE_SERVICE" -p MainPID --value 2>/dev/null || true)"
  cat <<EOF

UTXO index update completed safely.
  Unit:          $unit_file
  Backup:        $backup_file
  Node was:      ${node_was_active:-unknown}
  Node now:      $(systemctl is-active "$NODE_SERVICE" 2>/dev/null || true)
  Node PID:      ${after_pid:-0}
  UTXO index:    enabled
EOF
  if [[ "$miner_was_active" == "active" ]]; then
    printf '  Miner:         stopped; restart it after the node is synced\n'
  fi
  if [[ "$node_was_active" != "active" ]]; then
    printf '  Note:          the node was not active, so the new flag applies on its next start\n'
  fi
  printf '\nRun --check-only, then start the Web UI without --enable-utxoindex.\n'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --network)
      need_value "$1" "${2:-}"
      NETWORK="$2"
      shift 2
      ;;
    --port)
      need_value "$1" "${2:-}"
      WEB_PORT="$2"
      shift 2
      ;;
    --public-ip)
      need_value "$1" "${2:-}"
      PUBLIC_IP="$2"
      shift 2
      ;;
    --allow-client-ip)
      need_value "$1" "${2:-}"
      CLIENT_IP="$2"
      shift 2
      ;;
    --service)
      need_value "$1" "${2:-}"
      NODE_SERVICE="$2"
      shift 2
      ;;
    --service-user)
      need_value "$1" "${2:-}"
      SERVICE_USER="$2"
      shift 2
      ;;
    --appdir)
      need_value "$1" "${2:-}"
      APPDIR="$2"
      shift 2
      ;;
    --skip-apt)
      SKIP_APT=1
      shift
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --run-tests)
      RUN_TESTS=1
      shift
      ;;
    --check-only)
      CHECK_ONLY=1
      shift
      ;;
    --enable-utxoindex)
      ENABLE_UTXO_INDEX=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

[[ $EUID -eq 0 ]] || die "run as root: sudo ./scripts/misaka-vps-webui-start.sh"
[[ "$NETWORK" =~ ^[A-Za-z0-9._-]+$ ]] || die "invalid network id: $NETWORK"
[[ "$WEB_PORT" =~ ^[0-9]+$ ]] || die "port must be a number"
((WEB_PORT >= 1 && WEB_PORT <= 65535)) || die "port must be between 1 and 65535"
[[ "$NODE_SERVICE" =~ ^[A-Za-z0-9_.@-]+$ ]] || die "invalid systemd service name: $NODE_SERVICE"
if [[ "$CHECK_ONLY" == "1" && "$ENABLE_UTXO_INDEX" == "1" ]]; then
  die "--check-only and --enable-utxoindex cannot be used together"
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"

[[ -f "$REPO_DIR/Cargo.toml" ]] || die "repository root is incomplete: $REPO_DIR/Cargo.toml is missing"
[[ -f "$REPO_DIR/misaka-cli/Cargo.toml" ]] || die "misaka-cli is missing; clone the complete official misakas repository"

detect_existing_service_user
detect_existing_appdir

if service_loaded && ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
  die "systemd service $NODE_SERVICE refers to missing user: $SERVICE_USER"
fi

NODE_ACTIVE_BEFORE="$(systemctl is-active "$NODE_SERVICE" 2>/dev/null || true)"
NODE_PID_BEFORE="$(systemctl show "$NODE_SERVICE" -p MainPID --value 2>/dev/null || true)"
UTXO_INDEX_STATUS="not-installed"
if service_loaded; then
  if utxo_index_enabled; then
    UTXO_INDEX_STATUS="enabled"
  else
    UTXO_INDEX_STATUS="missing"
  fi
fi
DISK_AVAILABLE_KIB="$(available_disk_kib || true)"

log "Existing node compatibility"
printf 'Service:      %s\n' "$NODE_SERVICE"
printf 'Service user: %s\n' "$SERVICE_USER"
printf 'Appdir:       %s\n' "$APPDIR"
printf 'Node state:   %s\n' "${NODE_ACTIVE_BEFORE:-not-installed}"
printf 'Node PID:     %s\n' "${NODE_PID_BEFORE:-0}"
printf 'UTXO index:   %s\n' "$UTXO_INDEX_STATUS"
if [[ -n "$DISK_AVAILABLE_KIB" ]]; then
  printf 'Disk free:    %s KiB\n' "$DISK_AVAILABLE_KIB"
else
  printf 'Disk free:    unknown\n'
fi

if [[ "$CHECK_ONLY" == "1" ]]; then
  printf '\nCheck only: no packages, binaries, services, firewall rules, or data were changed.\n'
  exit 0
fi

if [[ "$ENABLE_UTXO_INDEX" == "1" ]]; then
  enable_existing_utxo_index
  exit 0
fi

if service_loaded && [[ "$UTXO_INDEX_STATUS" == "missing" ]]; then
  die "existing node has no UTXO index; run this script with --enable-utxoindex, then rerun normally"
fi

if [[ "$SKIP_APT" != "1" ]]; then
  command -v apt-get >/dev/null 2>&1 || die "apt-get is missing; Ubuntu/Debian is required or use --skip-apt"
  log "Install VPS build requirements"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y \
    curl git ca-certificates \
    build-essential pkg-config libssl-dev \
    protobuf-compiler clang lld \
    tmux ufw openssl
fi

for command_name in curl git tmux install; do
  command -v "$command_name" >/dev/null 2>&1 || die "$command_name is required"
done

if [[ -z "$PUBLIC_IP" ]]; then
  PUBLIC_IP="$(detect_public_ipv4 || true)"
fi
valid_ipv4 "$PUBLIC_IP" || die "could not detect a public IPv4; use --public-ip <VPS_IPV4>"

if [[ -z "$CLIENT_IP" ]]; then
  CLIENT_IP="$(detect_ssh_client_ipv4 || true)"
fi
valid_ipv4 "$CLIENT_IP" || die "could not detect the SSH client IPv4; use --allow-client-ip <YOUR_PUBLIC_IPV4>"

if [[ "$SKIP_BUILD" != "1" ]]; then
  log "Prepare Rust"
  if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  fi
  if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
  command -v cargo >/dev/null 2>&1 || die "cargo is unavailable after Rust setup"
  cargo --version
  rustc --version

  log "Build Web UI support binaries without replacing kaspad"
  cd "$REPO_DIR"
  if [[ "$RUN_TESTS" == "1" ]]; then
    cargo test -p misaka-cli
  fi
  cargo build --release -p misaka-cli -p kaspa-pq-validator -p misaminer
  for binary_name in misaka kaspa-pq-validator misaminer; do
    [[ -x "$REPO_DIR/target/release/$binary_name" ]] || die "build completed without target/release/$binary_name"
  done
fi

log "Stop the previous setup Web UI when present"
if command -v misaka >/dev/null 2>&1; then
  misaka --network "$NETWORK" setup web-stop 2>/dev/null || true
fi
tmux kill-session -t "$TMUX_SESSION" 2>/dev/null || true
rm -f "$SETUP_DIR/web-session.json" "$SETUP_DIR/web-url.txt"

if [[ "$SKIP_BUILD" != "1" ]]; then
  log "Back up and install Web UI support binaries"
  BACKUP_SUFFIX="before-webui-$(date -u +%Y%m%dT%H%M%SZ)"
  for binary_name in misaka kaspa-pq-validator misaminer; do
    if [[ -f "/usr/local/bin/$binary_name" ]]; then
      cp -a "/usr/local/bin/$binary_name" "/usr/local/bin/$binary_name.$BACKUP_SUFFIX"
    fi
    install -o root -g root -m 0755 "$REPO_DIR/target/release/$binary_name" "/usr/local/bin/$binary_name"
  done
else
  /usr/local/bin/misaka --network "$NETWORK" setup --help >/dev/null 2>&1 || \
    die "--skip-build requires an installed misaka CLI with the setup subcommand"
  command -v kaspa-pq-validator >/dev/null 2>&1 || die "--skip-build requires kaspa-pq-validator"
  command -v misaminer >/dev/null 2>&1 || die "--skip-build requires misaminer"
fi
/usr/local/bin/misaka --version || true

log "Create private Web UI launcher"
install -d -o root -g root -m 0700 "$SETUP_DIR"
: >"$WEB_LOG"
chmod 0600 "$WEB_LOG"

{
  printf '#!/usr/bin/env bash\n'
  printf 'set -Eeuo pipefail\n'
  printf 'exec /usr/local/bin/misaka'
  printf ' --network %q' "$NETWORK"
  printf ' setup web --public'
  printf ' --public-ip %q' "$PUBLIC_IP"
  printf ' --port %q' "$WEB_PORT"
  printf ' --allow-client-ip %q' "$CLIENT_IP"
  printf ' --service %q' "$NODE_SERVICE"
  printf ' --service-user %q' "$SERVICE_USER"
  printf ' --appdir %q' "$APPDIR"
  printf ' --repo-dir %q' "$REPO_DIR"
  printf ' --repo-url %q' "$REPO_URL"
  printf ' >>%q 2>&1\n' "$WEB_LOG"
} >"$LAUNCH_SCRIPT"
chmod 0700 "$LAUNCH_SCRIPT"

log "Start MISAKA Setup Web UI"
tmux new-session -d -s "$TMUX_SESSION" "$LAUNCH_SCRIPT"

for _attempt in {1..30}; do
  if [[ -s "$SETUP_DIR/web-url.txt" ]]; then
    break
  fi
  if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    printf 'Web UI did not stay running. Recent log:\n' >&2
    tail -n 80 "$WEB_LOG" >&2 || true
    exit 1
  fi
  sleep 0.5
done

if [[ ! -s "$SETUP_DIR/web-url.txt" ]]; then
  printf 'Web UI URL was not created. Recent log:\n' >&2
  tail -n 80 "$WEB_LOG" >&2 || true
  exit 1
fi

URL="$(tr -d '\r\n' <"$SETUP_DIR/web-url.txt")"
NODE_ACTIVE_AFTER="$(systemctl is-active "$NODE_SERVICE" 2>/dev/null || true)"
NODE_PID_AFTER="$(systemctl show "$NODE_SERVICE" -p MainPID --value 2>/dev/null || true)"

if [[ "$NODE_ACTIVE_BEFORE" == "active" && "$NODE_ACTIVE_AFTER" != "active" ]]; then
  die "existing node was active before Web UI setup but is now $NODE_ACTIVE_AFTER"
fi

cat <<EOF

MISAKA Setup Web UI is ready.

Open this URL in your browser:
  $URL

Access restriction:
  VPS public IPv4: $PUBLIC_IP
  Allowed client:  $CLIENT_IP
  TCP port:        $WEB_PORT

Existing node preserved:
  Service:         $NODE_SERVICE
  Service user:    $SERVICE_USER
  Appdir:          $APPDIR
  PID before/after: ${NODE_PID_BEFORE:-0}/${NODE_PID_AFTER:-0}

Keep the token URL private.
If the page does not open, allow TCP $WEB_PORT from $CLIENT_IP in the VPS provider firewall.

Show the URL again:
  sudo misaka --network $NETWORK setup web-status

Reopen or restart the Web UI:
  sudo misaka --network $NETWORK setup web-resume --public-ip $PUBLIC_IP --service $NODE_SERVICE --service-user $SERVICE_USER --appdir $APPDIR --repo-dir $REPO_DIR

Stop only the Web UI:
  sudo misaka --network $NETWORK setup web-stop

Web UI log:
  $WEB_LOG
EOF
