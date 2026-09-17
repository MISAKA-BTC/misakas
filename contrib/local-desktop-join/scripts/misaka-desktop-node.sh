#!/usr/bin/env bash
set -Eeuo pipefail

NETWORK="${MISAKA_NETWORK:-testnet-10}"
REPO_URL="${MISAKA_REPO_URL:-https://github.com/MISAKA-BTC/misakas.git}"
SCRIPT_PATH="${BASH_SOURCE[0]:-$0}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$SCRIPT_PATH")" && pwd -P)"
SHARE_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)"
ROOT_DIR="${MISAKA_DESKTOP_HOME:-$HOME/.misaka-desktop-node}"
REPO_DIR="${MISAKA_REPO_DIR:-$ROOT_DIR/misakas}"
BIN_DIR="$ROOT_DIR/bin"
TOOL_DIR="$ROOT_DIR/tools"
APPDIR="$ROOT_DIR/node-data"
LOG_DIR="$ROOT_DIR/logs"
RUN_DIR="$ROOT_DIR/run"
STATE_DIR="$ROOT_DIR/state"
HOME_DIR="$ROOT_DIR/home"
VALIDATOR_DIR="$ROOT_DIR/validator"
SUPPORT_DIR="$ROOT_DIR/support"
VALIDATOR_KEY="$VALIDATOR_DIR/validator.seed"
VALIDATOR_DB="$VALIDATOR_DIR/validator.state"
STATE_FILE="$STATE_DIR/state.env"
WEB_STATE_FILE="$STATE_DIR/web.env"

KASPAD_PID="$RUN_DIR/kaspad.pid"
MINER_PID="$RUN_DIR/misaminer.pid"
VALIDATOR_PID="$RUN_DIR/validator.pid"
CAFFEINATE_PID="$RUN_DIR/caffeinate.pid"

KASPAD_LOG="$LOG_DIR/kaspad.log"
MINER_LOG="$LOG_DIR/miner.log"
VALIDATOR_LOG="$LOG_DIR/validator.log"

P2P_PORT="${MISAKA_P2P_PORT:-26211}"
GRPC_PORT="${MISAKA_GRPC_PORT:-26210}"
WRPC_BORSH_PORT="${MISAKA_WRPC_BORSH_PORT:-27210}"
MINER_THREADS="${MISAKA_MINER_THREADS:-1}"

say() {
  if [ -n "${NO_COLOR:-}" ]; then
    printf '== %s ==\n' "$*"
  else
    printf '\033[1;34m== %s ==\033[0m\n' "$*"
  fi
}

warn() {
  if [ -n "${NO_COLOR:-}" ]; then
    printf 'WARN: %s\n' "$*" >&2
  else
    printf '\033[1;33mWARN:\033[0m %s\n' "$*" >&2
  fi
}

die() {
  if [ -n "${NO_COLOR:-}" ]; then
    printf 'ERROR: %s\n' "$*" >&2
  else
    printf '\033[1;31mERROR:\033[0m %s\n' "$*" >&2
  fi
  exit 1
}

usage() {
  cat <<'EOF'
MISAKA desktop local node MVP

Usage:
  scripts/misaka-desktop-node.sh <command>

Commands:
  prepare           Install/check local tools, clone source, build binaries
  auto-node         prepare + start-node + status
  auto-validator    Wait sync, create key, start miner, and show next bond steps
  start-node        Start local kaspad in the background
  stop-node         Stop local kaspad
  restart-node      Restart local kaspad
  status            Show process state and node doctor
  doctor            Run detailed local diagnostics
  collect-support-log
                    Create a redacted support bundle under MISAKA desktop home
  wait-sync         Poll node doctor until Synced true
  logs              Tail local logs
  node-logs         Tail node logs only
  miner-logs        Tail funding miner logs only
  validator-logs    Tail validator logs only
  keygen            Create validator key and funding address
  miner-start       Start funding miner to the validator funding address
  miner-stop        Stop funding miner
  balance           Check funding address balance
  bond [amount]     Create stake bond, default 10MSK
  validator-start   Start validator sidecar
  validator-stop    Stop validator sidecar
  stop-all          Stop validator, miner, and node
  clean             Remove local desktop runtime directory

Environment:
  MISAKA_NETWORK=testnet-10
  MISAKA_DESKTOP_HOME=$HOME/.misaka-desktop-node
  MISAKA_REPO_URL=https://github.com/MISAKA-BTC/misakas.git
  MISAKA_MINER_THREADS=1
  MISAKA_KEEP_AWAKE=1       # macOS only: run caffeinate while kaspad is running
EOF
}

mkdirs() {
  mkdir -p "$BIN_DIR" "$TOOL_DIR" "$APPDIR" "$LOG_DIR" "$RUN_DIR" "$STATE_DIR" "$HOME_DIR" "$VALIDATOR_DIR"
}

is_macos() {
  [ "$(uname -s)" = "Darwin" ]
}

is_linux() {
  [ "$(uname -s)" = "Linux" ]
}

is_wsl() {
  is_linux && grep -qi microsoft /proc/version 2>/dev/null
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

redact_stream() {
  sed -E \
    -e 's#([?&]token=)[^&[:space:]"'"'"'<>]+#\1<redacted>#g' \
    -e 's#([Tt][Oo][Kk][Ee][Nn][=:][[:space:]]*)[^[:space:]]+#\1<redacted>#g' \
    -e 's#([Ss][Ee][Tt][Uu][Pp][_-]?[Tt][Oo][Kk][Ee][Nn][=:][[:space:]]*)[^[:space:]]+#\1<redacted>#g' \
    -e 's#([Ss][Ee][Cc][Rr][Ee][Tt][_-]?[Kk][Ee][Yy][=:][[:space:]]*)[^[:space:]]+#\1<redacted>#g' \
    -e 's#([Pp][Rr][Ii][Vv][Aa][Tt][Ee][_-]?[Kk][Ee][Yy][=:][[:space:]]*)[^[:space:]]+#\1<redacted>#g' \
    -e 's#([Pp][Rr][Ii][Vv][Aa][Tt][Ee][_-]?[Ss][Ee][Ee][Dd][=:][[:space:]]*)[^[:space:]]+#\1<redacted>#g' \
    -e 's#([Vv][Aa][Ll][Ii][Dd][Aa][Tt][Oo][Rr][_-]?[Ss][Ee][Ee][Dd][=:][[:space:]]*)[^[:space:]]+#\1<redacted>#g' \
    -e 's#([Ss][Ee][Ee][Dd][_-]?[Pp][Hh][Rr][Aa][Ss][Ee][=:][[:space:]]*).*#\1<redacted>#g' \
    -e 's#([Mm][Nn][Ee][Mm][Oo][Nn][Ii][Cc][=:][[:space:]]*).*#\1<redacted>#g'
}

redact_file_to() {
  local src="$1"
  local dst="$2"
  if [ -f "$src" ]; then
    redact_stream < "$src" > "$dst"
  else
    printf 'missing: %s\n' "$src" > "$dst"
  fi
  chmod 600 "$dst" 2>/dev/null || true
}

redact_tail_to() {
  local src="$1"
  local dst="$2"
  local lines="${3:-300}"
  if [ -f "$src" ]; then
    tail -n "$lines" "$src" 2>&1 | redact_stream > "$dst"
  else
    printf 'missing: %s\n' "$src" > "$dst"
  fi
  chmod 600 "$dst" 2>/dev/null || true
}

print_key_value() {
  printf '%-24s %s\n' "$1:" "$2"
}

print_command_check() {
  local cmd="$1"
  local version_args="${2:---version}"
  if command -v "$cmd" >/dev/null 2>&1; then
    local path
    path="$(command -v "$cmd")"
    local version
    version="$("$cmd" $version_args 2>&1 | head -n 1 || true)"
    print_key_value "$cmd" "OK $path ${version:+($version)}"
  else
    print_key_value "$cmd" "MISSING"
  fi
}

print_binary_check() {
  local name="$1"
  local path="$BIN_DIR/$name"
  if [ -x "$path" ]; then
    local version
    version="$("$path" --version 2>&1 | head -n 1 || true)"
    print_key_value "$name" "OK $path ${version:+($version)}"
  elif [ -e "$path" ]; then
    print_key_value "$name" "FOUND but not executable: $path"
  else
    print_key_value "$name" "MISSING"
  fi
}

port_listening() {
  local port="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -ltn 2>/dev/null | awk -v p=":$port" '$4 ~ p "$" {found=1} END {exit found ? 0 : 1}'
  elif command -v lsof >/dev/null 2>&1; then
    lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
  elif command -v netstat >/dev/null 2>&1; then
    netstat -an 2>/dev/null | awk -v p=".$port" '$0 ~ p && $0 ~ /LISTEN/ {found=1} END {exit found ? 0 : 1}'
  else
    return 2
  fi
}

print_port_check() {
  local label="$1"
  local port="$2"
  if port_listening "$port"; then
    print_key_value "$label $port" "listening"
  else
    local code=$?
    if [ "$code" -eq 2 ]; then
      print_key_value "$label $port" "unknown (ss/lsof/netstat missing)"
    else
      print_key_value "$label $port" "not listening"
    fi
  fi
}

print_pid_check() {
  local label="$1"
  local file="$2"
  if pid_alive "$file"; then
    print_key_value "$label" "running pid=$(cat "$file")"
  elif [ -f "$file" ]; then
    print_key_value "$label" "stopped (stale pid=$(cat "$file" 2>/dev/null || true))"
  else
    print_key_value "$label" "stopped"
  fi
}

ensure_macos_protoc() {
  if command -v protoc >/dev/null 2>&1; then
    return
  fi

  local version="${MISAKA_PROTOC_VERSION:-21.12}"
  local arch
  local asset_arch
  arch="$(uname -m)"
  case "$arch" in
    arm64|aarch64) asset_arch="osx-aarch_64" ;;
    x86_64|amd64) asset_arch="osx-x86_64" ;;
    *) die "unsupported macOS arch for local protoc download: $arch" ;;
  esac

  local protoc_root="$TOOL_DIR/protoc-$version-$asset_arch"
  local protoc_bin="$protoc_root/bin/protoc"
  if [ ! -x "$protoc_bin" ]; then
    require_cmd curl
    require_cmd unzip
    local zip="$TOOL_DIR/protoc-$version-$asset_arch.zip"
    local url="https://github.com/protocolbuffers/protobuf/releases/download/v${version}/protoc-${version}-${asset_arch}.zip"
    say "install local protoc"
    printf 'download: %s\n' "$url"
    rm -rf "$protoc_root"
    mkdir -p "$protoc_root"
    curl -fL "$url" -o "$zip"
    unzip -q "$zip" -d "$protoc_root"
    chmod +x "$protoc_bin"
  fi

  export PROTOC="$protoc_bin"
  export PATH="$protoc_root/bin:$PATH"
  printf 'protoc: %s\n' "$("$PROTOC" --version)"
}

linux_apt_deps_ready() {
  command -v dpkg >/dev/null 2>&1 || return 1
  for cmd in curl git pkg-config protoc clang lld rsync unzip; do
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
    unzip >/dev/null 2>&1
}

ensure_sudo_for_apt() {
  if [ "$(id -u)" -eq 0 ]; then
    return
  fi
  command -v sudo >/dev/null 2>&1 || die "sudo is required to install Linux packages. Install dependencies manually or run as root."
  if [ "${MISAKA_WEB_JOB:-0}" = "1" ] && ! sudo -n true >/dev/null 2>&1; then
    if is_wsl; then
      die "sudo permission is required before Web UI can install Linux packages. Close this job, run windows/start-web-ui-wsl.cmd again, and enter the Ubuntu password in the PowerShell window when asked."
    fi
    die "sudo permission is required before Web UI can install Linux packages. Close this job, rerun scripts/misaka-desktop-web.sh in a terminal, and enter your Linux account password when asked."
  fi
}

ensure_local_deps() {
  say "check local tools"
  if is_macos; then
    if ! xcode-select -p >/dev/null 2>&1; then
      warn "Xcode Command Line Tools are required."
      warn "Run: xcode-select --install"
      die "install Xcode Command Line Tools, then rerun"
    fi
    if command -v brew >/dev/null 2>&1; then
      brew list protobuf >/dev/null 2>&1 || brew install protobuf
      brew list pkg-config >/dev/null 2>&1 || brew install pkg-config
      brew list openssl@3 >/dev/null 2>&1 || brew install openssl@3
      export PKG_CONFIG_PATH="$(brew --prefix openssl@3)/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    else
      warn "Homebrew not found. The script will install protoc locally."
      warn "If a later native-library build fails, install Homebrew and run:"
      warn "  brew install pkg-config openssl@3 protobuf"
    fi
    ensure_macos_protoc
  elif is_linux; then
    if command -v apt-get >/dev/null 2>&1; then
      if linux_apt_deps_ready; then
        printf 'Linux build packages: already installed\n'
        return
      fi
      ensure_sudo_for_apt
      sudo apt-get update
      sudo apt-get install -y curl git ca-certificates build-essential pkg-config libssl-dev protobuf-compiler clang lld rsync unzip
    else
      warn "Non-apt Linux detected. Please ensure curl git clang lld pkg-config openssl/protobuf dev packages are installed."
    fi
  fi

  require_cmd curl
  require_cmd git
}

ensure_rust() {
  say "check rust"
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
  fi
  cargo --version
  rustc --version
}

net_args() {
  case "$NETWORK" in
    testnet-[0-9]*)
      printf '%s\n' "--testnet" "--netsuffix=${NETWORK#testnet-}"
      ;;
    testnet)
      printf '%s\n' "--testnet"
      ;;
    *)
      die "this MVP currently supports testnet/testnet-N only, got: $NETWORK"
      ;;
  esac
}

load_state() {
  if [ -f "$STATE_FILE" ]; then
    # shellcheck disable=SC1090
    . "$STATE_FILE"
  fi
}

save_state_value() {
  mkdirs
  local key="$1"
  local value="$2"
  touch "$STATE_FILE"
  if grep -q "^${key}=" "$STATE_FILE"; then
    tmp="${STATE_FILE}.tmp"
    awk -v k="$key" -v v="$value" 'BEGIN{q=sprintf("%c", 39)} $0 ~ "^" k "=" {print k "=" q v q; next} {print}' "$STATE_FILE" > "$tmp"
    mv "$tmp" "$STATE_FILE"
  else
    printf "%s='%s'\n" "$key" "$value" >> "$STATE_FILE"
  fi
  chmod 600 "$STATE_FILE" 2>/dev/null || true
}

clone_source() {
  say "prepare source"
  if [ -f "$REPO_DIR/Cargo.toml" ]; then
    printf 'Source exists: %s\n' "$REPO_DIR"
  else
    rm -rf "$REPO_DIR"
    git clone "$REPO_URL" "$REPO_DIR"
  fi
}

build_bins() {
  say "build release binaries"
  mkdirs
  cd "$REPO_DIR"
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
  fi
  cargo build --release -p kaspad --features evm
  cargo build --release -p misaka-cli -p kaspa-pq-validator -p misaminer
  cp target/release/kaspad "$BIN_DIR/kaspad"
  cp target/release/misaka "$BIN_DIR/misaka"
  cp target/release/kaspa-pq-validator "$BIN_DIR/kaspa-pq-validator"
  cp target/release/misaminer "$BIN_DIR/misaminer"
  chmod +x "$BIN_DIR/kaspad" "$BIN_DIR/misaka" "$BIN_DIR/kaspa-pq-validator" "$BIN_DIR/misaminer"
  "$BIN_DIR/kaspad" --version || true
  "$BIN_DIR/misaka" --version || true
}

prepare() {
  mkdirs
  ensure_local_deps
  ensure_rust
  clone_source
  build_bins
}

pid_alive() {
  local file="$1"
  [ -f "$file" ] || return 1
  local pid
  pid="$(cat "$file" 2>/dev/null || true)"
  [ -n "$pid" ] || return 1
  kill -0 "$pid" >/dev/null 2>&1
}

stop_pid() {
  local name="$1"
  local file="$2"
  if pid_alive "$file"; then
    local pid
    pid="$(cat "$file")"
    say "stop $name pid=$pid"
    kill "$pid" >/dev/null 2>&1 || true
    sleep 2
    kill -9 "$pid" >/dev/null 2>&1 || true
  fi
  rm -f "$file"
}

start_caffeinate_for_node() {
  if ! is_macos; then
    return
  fi
  if [ "${MISAKA_KEEP_AWAKE:-1}" = "0" ]; then
    return
  fi
  command -v caffeinate >/dev/null 2>&1 || return
  pid_alive "$KASPAD_PID" || return
  if pid_alive "$CAFFEINATE_PID"; then
    return
  fi
  local node_pid
  node_pid="$(cat "$KASPAD_PID")"
  say "keep Mac awake while node is running"
  caffeinate -dimsu -w "$node_pid" >/dev/null 2>&1 &
  echo $! > "$CAFFEINATE_PID"
}

start_node() {
  mkdirs
  [ -x "$BIN_DIR/kaspad" ] || die "kaspad is missing. Run prepare first."
  if pid_alive "$KASPAD_PID"; then
    say "kaspad already running pid=$(cat "$KASPAD_PID")"
    start_caffeinate_for_node
    return
  fi

  say "start local kaspad"
  flags=()
  while IFS= read -r flag; do
    flags+=("$flag")
  done < <(net_args)
  args=(
    "${flags[@]}"
    "--yes"
    "--appdir=$APPDIR"
    "--listen=0.0.0.0:$P2P_PORT"
    "--profile=local-validator"
    "--rpclisten-borsh=127.0.0.1:$WRPC_BORSH_PORT"
    "--utxoindex"
    "--ram-scale=0.3"
    "--async-threads=2"
    "--outpeers=8"
    "--maxinpeers=64"
    "--rpcmaxclients=8"
    "--min-disk-free-percent=10"
    "--perf-metrics"
    "--perf-metrics-interval-sec=60"
  )
  if [ -n "${MISAKA_EXTERNAL_IP:-}" ]; then
    args+=("--externalip=${MISAKA_EXTERNAL_IP}:$P2P_PORT")
  fi

  (
    cd "$ROOT_DIR"
    env HOME="$HOME_DIR" "$BIN_DIR/kaspad" "${args[@]}" > "$KASPAD_LOG" 2>&1 &
    echo $! > "$KASPAD_PID"
  )
  sleep 2
  if pid_alive "$KASPAD_PID"; then
    save_state_value NODE_STARTED_ONCE 1
    printf 'kaspad started pid=%s\n' "$(cat "$KASPAD_PID")"
    printf 'log: %s\n' "$KASPAD_LOG"
    start_caffeinate_for_node
  else
    tail -n 80 "$KASPAD_LOG" || true
    die "kaspad failed to start"
  fi
}

node_doctor() {
  if [ -x "$BIN_DIR/misaka" ]; then
    env HOME="$HOME_DIR" "$BIN_DIR/misaka" --network "$NETWORK" --rpc "127.0.0.1:$WRPC_BORSH_PORT" node doctor || true
  else
    warn "misaka binary missing"
  fi
}

status() {
  say "desktop node status"
  printf 'home:    %s\n' "$ROOT_DIR"
  printf 'network: %s\n' "$NETWORK"
  printf 'kaspad:  %s\n' "$(pid_alive "$KASPAD_PID" && printf 'running pid=%s' "$(cat "$KASPAD_PID")" || printf 'stopped')"
  printf 'miner:   %s\n' "$(pid_alive "$MINER_PID" && printf 'running pid=%s' "$(cat "$MINER_PID")" || printf 'stopped')"
  printf 'valid.:  %s\n' "$(pid_alive "$VALIDATOR_PID" && printf 'running pid=%s' "$(cat "$VALIDATOR_PID")" || printf 'stopped')"
  if is_macos; then
    printf 'awake:   %s\n' "$(pid_alive "$CAFFEINATE_PID" && printf 'caffeinate pid=%s' "$(cat "$CAFFEINATE_PID")" || printf 'off')"
  fi
  printf '\n'
  node_doctor
}

validator_status() {
  load_state
  if [ ! -x "$BIN_DIR/kaspa-pq-validator" ]; then
    warn "kaspa-pq-validator binary missing"
    return 0
  fi
  if [ -z "${BOND_OUTPOINT:-}" ]; then
    printf 'BOND_OUTPOINT missing. Run bond first.\n'
    return 0
  fi
  ensure_valid_bond_outpoint
  "$BIN_DIR/kaspa-pq-validator" status \
    --node-wrpc-borsh "127.0.0.1:$WRPC_BORSH_PORT" \
    --network "$NETWORK" \
    --stake-bond "$BOND_OUTPOINT" || true
}

print_system_info() {
  print_key_value "generated_utc" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  print_key_value "local_time" "$(date '+%Y-%m-%d %H:%M:%S %Z')"
  print_key_value "uname" "$(uname -a)"
  print_key_value "arch" "$(uname -m)"
  print_key_value "shell" "${SHELL:-unknown}"
  print_key_value "user" "$(id -un 2>/dev/null || true)"
  if is_macos && command -v sw_vers >/dev/null 2>&1; then
    sw_vers 2>/dev/null | sed 's/^/  /'
  fi
  if is_linux && [ -f /etc/os-release ]; then
    awk -F= '/^(PRETTY_NAME|VERSION_ID|ID)=/ {gsub(/^"|"$/, "", $2); printf "  %s: %s\n", $1, $2}' /etc/os-release
  fi
  if is_linux && grep -qi microsoft /proc/version 2>/dev/null; then
    print_key_value "wsl" "yes"
    if [ -n "${WSL_DISTRO_NAME:-}" ]; then
      print_key_value "wsl_distro" "$WSL_DISTRO_NAME"
    fi
    if command -v wslpath >/dev/null 2>&1; then
      print_key_value "wslpath" "available"
    fi
  else
    print_key_value "wsl" "no"
  fi
}

print_share_layout() {
  print_key_value "share_dir" "$SHARE_DIR"
  print_key_value "script_dir" "$SCRIPT_DIR"
  for path in windows scripts ui mac docs; do
    if [ -d "$SHARE_DIR/$path" ]; then
      print_key_value "$path" "OK"
    else
      print_key_value "$path" "MISSING"
    fi
  done
  for path in scripts/misaka-desktop-node.sh scripts/misaka-desktop-web.sh scripts/misaka-desktop-web.py; do
    if [ -f "$SHARE_DIR/$path" ]; then
      print_key_value "$path" "OK"
    else
      print_key_value "$path" "MISSING"
    fi
  done
  if [ -n "${MISAKA_SHARE_DIR:-}" ]; then
    print_key_value "MISAKA_SHARE_DIR" "$MISAKA_SHARE_DIR"
  fi
}

print_resources() {
  if command -v df >/dev/null 2>&1; then
    df -h "$ROOT_DIR" "$APPDIR" 2>/dev/null | sed 's/^/  /' || true
  fi
  if command -v du >/dev/null 2>&1; then
    du -sh "$ROOT_DIR" "$APPDIR" "$LOG_DIR" "$VALIDATOR_DIR" 2>/dev/null | sed 's/^/  /' || true
  fi
  if command -v free >/dev/null 2>&1; then
    free -h 2>/dev/null | sed 's/^/  /' || true
  elif is_macos && command -v vm_stat >/dev/null 2>&1; then
    vm_stat 2>/dev/null | head -n 8 | sed 's/^/  /' || true
  fi
  if command -v swapon >/dev/null 2>&1; then
    swapon --show 2>/dev/null | sed 's/^/  /' || true
  fi
}

print_next_action() {
  local node_doctor_output="$1"
  say "suggested next action"
  if [ ! -x "$BIN_DIR/kaspad" ] || [ ! -x "$BIN_DIR/misaka" ]; then
    printf 'Run: scripts/misaka-desktop-node.sh prepare\n'
  elif ! pid_alive "$KASPAD_PID"; then
    printf 'Run: scripts/misaka-desktop-node.sh start-node\n'
  elif ! printf '%s\n' "$node_doctor_output" | grep -q 'Synced[[:space:]]*true'; then
    printf 'Run: scripts/misaka-desktop-node.sh wait-sync\n'
  elif [ ! -f "$VALIDATOR_KEY" ]; then
    printf 'Run: scripts/misaka-desktop-node.sh keygen\n'
  elif ! pid_alive "$MINER_PID" && [ -z "${BOND_OUTPOINT:-}" ]; then
    printf 'Run: scripts/misaka-desktop-node.sh miner-start\n'
  elif [ -z "${BOND_OUTPOINT:-}" ]; then
    printf 'Run: scripts/misaka-desktop-node.sh balance, then bond 10MSK after mature funding is available\n'
  elif ! pid_alive "$VALIDATOR_PID"; then
    printf 'Run: scripts/misaka-desktop-node.sh validator-start\n'
  else
    printf 'Validator appears configured. Check validator status/logs for attestation progress.\n'
  fi
}

doctor() {
  mkdirs
  load_state

  say "desktop node doctor"
  print_system_info

  say "paths"
  print_key_value "desktop_home" "$ROOT_DIR"
  print_key_value "repo_dir" "$REPO_DIR"
  print_key_value "bin_dir" "$BIN_DIR"
  print_key_value "appdir" "$APPDIR"
  print_key_value "log_dir" "$LOG_DIR"
  print_key_value "validator_dir" "$VALIDATOR_DIR"
  print_key_value "network" "$NETWORK"

  say "share layout"
  print_share_layout

  say "tools"
  print_command_check bash "--version"
  print_command_check curl "--version"
  print_command_check git "--version"
  print_command_check cargo "--version"
  print_command_check rustc "--version"
  print_command_check protoc "--version"
  print_command_check clang "--version"
  print_command_check lld "--version"
  print_command_check rsync "--version"
  print_command_check unzip "-v"

  say "binaries"
  print_binary_check kaspad
  print_binary_check misaka
  print_binary_check kaspa-pq-validator
  print_binary_check misaminer

  say "processes"
  print_pid_check kaspad "$KASPAD_PID"
  print_pid_check miner "$MINER_PID"
  print_pid_check validator "$VALIDATOR_PID"
  if is_macos; then
    print_pid_check caffeinate "$CAFFEINATE_PID"
  fi

  say "ports"
  print_port_check "P2P" "$P2P_PORT"
  print_port_check "gRPC" "$GRPC_PORT"
  print_port_check "wRPC Borsh" "$WRPC_BORSH_PORT"

  say "state"
  print_key_value "validator_key" "$([ -f "$VALIDATOR_KEY" ] && printf 'exists' || printf 'missing')"
  print_key_value "validator_db" "$([ -e "$VALIDATOR_DB" ] && printf 'exists' || printf 'missing')"
  print_key_value "web_state" "$([ -f "$WEB_STATE_FILE" ] && printf 'exists' || printf 'missing')"
  print_key_value "funding_address" "${FUNDING_ADDRESS:-missing}"
  print_key_value "bond_outpoint" "${BOND_OUTPOINT:-missing}"
  print_key_value "miner_threads" "${MINER_THREADS:-$MISAKA_MINER_THREADS}"

  say "resources"
  print_resources

  say "node doctor"
  local node_out=""
  if [ -x "$BIN_DIR/misaka" ]; then
    node_out="$(env HOME="$HOME_DIR" "$BIN_DIR/misaka" --network "$NETWORK" --rpc "127.0.0.1:$WRPC_BORSH_PORT" node doctor 2>&1 || true)"
    printf '%s\n' "$node_out"
  else
    warn "misaka binary missing"
  fi

  say "validator status"
  validator_status 2>&1 | redact_stream || true

  say "recent logs"
  printf '\n[kaspad]\n'
  tail -n 30 "$KASPAD_LOG" 2>/dev/null | redact_stream || true
  printf '\n[miner]\n'
  tail -n 20 "$MINER_LOG" 2>/dev/null | redact_stream || true
  printf '\n[validator]\n'
  tail -n 30 "$VALIDATOR_LOG" 2>/dev/null | redact_stream || true

  print_next_action "$node_out"
}

collect_support_log() {
  mkdirs
  local stamp
  stamp="$(date -u '+%Y%m%dT%H%M%SZ')"
  local bundle_dir="$SUPPORT_DIR/misaka-support-$stamp"
  local archive="$SUPPORT_DIR/misaka-support-$stamp.tar.gz"

  mkdir -p "$bundle_dir"
  chmod 700 "$SUPPORT_DIR" "$bundle_dir" 2>/dev/null || true

  say "collect support log"
  printf 'bundle_dir: %s\n' "$bundle_dir"

  NO_COLOR=1 doctor 2>&1 | redact_stream > "$bundle_dir/doctor.txt"
  redact_tail_to "$KASPAD_LOG" "$bundle_dir/kaspad.tail.log" 500
  redact_tail_to "$MINER_LOG" "$bundle_dir/miner.tail.log" 300
  redact_tail_to "$VALIDATOR_LOG" "$bundle_dir/validator.tail.log" 500
  redact_file_to "$STATE_FILE" "$bundle_dir/state.env.redacted"
  redact_file_to "$WEB_STATE_FILE" "$bundle_dir/web.env.redacted"

  {
    say "runtime tree"
    printf 'The validator seed/key file is intentionally not copied.\n'
    for dir in "$ROOT_DIR" "$BIN_DIR" "$RUN_DIR" "$STATE_DIR" "$LOG_DIR" "$VALIDATOR_DIR" "$SUPPORT_DIR" "$SHARE_DIR"; do
      printf '\n[%s]\n' "$dir"
      ls -la "$dir" 2>/dev/null || true
    done
    say "pid files"
    for f in "$KASPAD_PID" "$MINER_PID" "$VALIDATOR_PID" "$CAFFEINATE_PID"; do
      if [ -f "$f" ]; then
        printf '%s: %s\n' "$f" "$(cat "$f" 2>/dev/null || true)"
      else
        printf '%s: missing\n' "$f"
      fi
    done
    say "process snapshot"
    ps -ww -o pid,ppid,stat,etime,command -p "$(cat "$KASPAD_PID" 2>/dev/null || printf 0)" 2>/dev/null || true
    ps -ww -o pid,ppid,stat,etime,command -p "$(cat "$MINER_PID" 2>/dev/null || printf 0)" 2>/dev/null || true
    ps -ww -o pid,ppid,stat,etime,command -p "$(cat "$VALIDATOR_PID" 2>/dev/null || printf 0)" 2>/dev/null || true
  } 2>&1 | redact_stream > "$bundle_dir/runtime.txt"
  chmod 600 "$bundle_dir"/* 2>/dev/null || true

  local support_artifact="$bundle_dir"
  if command -v tar >/dev/null 2>&1; then
    (
      cd "$SUPPORT_DIR"
      tar -czf "$(basename "$archive")" "$(basename "$bundle_dir")"
    )
    chmod 600 "$archive" 2>/dev/null || true
    support_artifact="$archive"
    printf 'support_archive: %s\n' "$archive"
  else
    warn "tar is missing; support bundle directory was created but not archived"
    printf 'support_dir: %s\n' "$bundle_dir"
  fi

  if is_linux && grep -qi microsoft /proc/version 2>/dev/null && command -v wslpath >/dev/null 2>&1; then
    local windows_artifact
    windows_artifact="$(wslpath -w "$support_artifact" 2>/dev/null || true)"
    if [ -n "$windows_artifact" ]; then
      printf 'support_windows_path: %s\n' "$windows_artifact"
    fi
  fi

  cat <<EOF

Share this support artifact with the project maintainer:
  ${support_artifact}

It should not contain validator.seed or private key material. The command also
redacts common token/seed/key patterns from logs before writing the bundle.
EOF
}

wait_sync() {
  say "wait for node sync"
  while true; do
    out="$(env HOME="$HOME_DIR" "$BIN_DIR/misaka" --network "$NETWORK" --rpc "127.0.0.1:$WRPC_BORSH_PORT" node doctor 2>&1 || true)"
    printf '%s\n' "$out" | grep -E 'Synced|Virtual DAA|wRPC Borsh|P2P' || true
    if printf '%s\n' "$out" | grep -q 'Synced[[:space:]]*true'; then
      say "node synced"
      return
    fi
    sleep 30
  done
}

auto_validator() {
  say "auto validator preparation"
  [ -x "$BIN_DIR/misaka" ] || prepare
  if ! pid_alive "$KASPAD_PID"; then
    start_node
  fi
  wait_sync
  keygen
  miner_start
  printf '\n'
  say "funding status"
  balance || true
  cat <<EOF

Next:
  1. Keep the miner running until coinbase maturity passes.
  2. Re-check balance:
       scripts/misaka-desktop-node.sh balance
  3. When mature funding is available, create the bond:
       scripts/misaka-desktop-node.sh bond 10MSK
  4. Start validator:
       scripts/misaka-desktop-node.sh validator-start

Note:
  Visible balance is not always bondable. Mined rewards need coinbase maturity first.
EOF
}

keygen() {
  mkdirs
  [ -x "$BIN_DIR/misaka" ] || die "misaka is missing. Run prepare first."
  if [ ! -f "$VALIDATOR_KEY" ]; then
    say "create validator key"
    env HOME="$HOME_DIR" "$BIN_DIR/misaka" --network "$NETWORK" key gen --out "$VALIDATOR_KEY"
    chmod 600 "$VALIDATOR_KEY"
  else
    say "validator key already exists"
  fi
  addr="$(env HOME="$HOME_DIR" "$BIN_DIR/misaka" --network "$NETWORK" key address --key-file "$VALIDATOR_KEY")"
  save_state_value FUNDING_ADDRESS "$addr"
  printf 'funding_address: %s\n' "$addr"
}

funding_address() {
  load_state
  if [ -n "${FUNDING_ADDRESS:-}" ]; then
    printf '%s\n' "$FUNDING_ADDRESS"
    return
  fi
  [ -f "$VALIDATOR_KEY" ] || die "validator key missing. Run keygen first."
  env HOME="$HOME_DIR" "$BIN_DIR/misaka" --network "$NETWORK" key address --key-file "$VALIDATOR_KEY"
}

extract_virtual_daa() {
  awk '/Virtual DAA score/ {for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+$/) {print $i; exit}}'
}

miner_start() {
  mkdirs
  [ -x "$BIN_DIR/misaminer" ] || die "misaminer is missing. Run prepare first."
  if pid_alive "$MINER_PID"; then
    say "miner already running pid=$(cat "$MINER_PID")"
    return
  fi
  addr="$(funding_address)"
  save_state_value MINER_THREADS "$MINER_THREADS"
  miner_start_daa="$(node_doctor 2>/dev/null | extract_virtual_daa || true)"
  if printf '%s' "$miner_start_daa" | grep -Eq '^[0-9]+$'; then
    save_state_value MINER_START_DAA "$miner_start_daa"
  fi
  say "start funding miner"
  (
    cd "$ROOT_DIR"
    env HOME="$HOME_DIR" "$BIN_DIR/misaminer" \
      --pool "127.0.0.1:$GRPC_PORT" \
      --network-id "$NETWORK" \
      --wallet "$addr" \
      --worker desktop-funding \
      --threads "$MINER_THREADS" \
      --blocks 0 \
      --min-block-interval-ms 1000 > "$MINER_LOG" 2>&1 &
    echo $! > "$MINER_PID"
  )
  sleep 2
  if pid_alive "$MINER_PID"; then
    printf 'miner started pid=%s\n' "$(cat "$MINER_PID")"
    printf 'funding_address: %s\n' "$addr"
    printf 'log: %s\n' "$MINER_LOG"
  else
    tail -n 80 "$MINER_LOG" || true
    die "miner failed to start"
  fi
}

balance() {
  [ -x "$BIN_DIR/kaspa-pq-validator" ] || die "kaspa-pq-validator is missing. Run prepare first."
  addr="$(funding_address)"
  "$BIN_DIR/kaspa-pq-validator" balance \
    --node-wrpc-borsh "127.0.0.1:$WRPC_BORSH_PORT" \
    --network "$NETWORK" \
    --address "$addr"
}

normalize_bond_outpoint() {
  local value="${1:-}"
  value="$(printf '%s' "$value" | tr -d '[:space:]')"
  if printf '%s' "$value" | grep -Eq '^([0-9a-fA-F]{64}|[0-9a-fA-F]{128}):[0-9]+$'; then
    printf '%s\n' "$value"
    return
  fi
  if printf '%s' "$value" | grep -Eq '^([0-9a-fA-F]{64}|[0-9a-fA-F]{128})$'; then
    printf '%s:0\n' "$value"
    return
  fi
  return 1
}

extract_bond_outpoint() {
  local output="$1"
  local raw_outpoint
  raw_outpoint="$(printf '%s\n' "$output" | awk '/^[ \t]*bond_outpoint[ \t]*:/ {line=$0; sub(/^[ \t]*bond_outpoint[ \t]*:[ \t]*/, "", line); print line; exit}')"
  normalize_bond_outpoint "$raw_outpoint"
}

ensure_valid_bond_outpoint() {
  local normalized
  if ! normalized="$(normalize_bond_outpoint "${BOND_OUTPOINT:-}")"; then
    die "invalid BOND_OUTPOINT. Expected txid:index; create the bond again if the saved value is not a transaction ID."
  fi
  if [ "$normalized" != "$BOND_OUTPOINT" ]; then
    warn "repair saved BOND_OUTPOINT by adding the StakeBond output index :0"
    BOND_OUTPOINT="$normalized"
    save_state_value BOND_OUTPOINT "$BOND_OUTPOINT"
  fi
}

bond() {
  [ -x "$BIN_DIR/kaspa-pq-validator" ] || die "kaspa-pq-validator is missing. Run prepare first."
  amount="${1:-10MSK}"
  say "create stake bond amount=$amount"
  set +e
  output="$("$BIN_DIR/kaspa-pq-validator" bond \
    --node-wrpc-borsh "127.0.0.1:$WRPC_BORSH_PORT" \
    --validator-key "$VALIDATOR_KEY" \
    --amount "$amount" \
    --network "$NETWORK" 2>&1)"
  code=$?
  set -e
  printf '%s\n' "$output"
  if [ "$code" -ne 0 ]; then
    die "bond failed. If it says not enough MATURE funding, keep mining and wait for coinbase maturity."
  fi
  if ! outpoint="$(extract_bond_outpoint "$output")"; then
    die "valid bond_outpoint not found in output"
  fi
  save_state_value BOND_OUTPOINT "$outpoint"
  printf 'bond_outpoint: %s\n' "$outpoint"
}

validator_start() {
  mkdirs
  [ -x "$BIN_DIR/kaspa-pq-validator" ] || die "kaspa-pq-validator is missing. Run prepare first."
  load_state
  [ -n "${BOND_OUTPOINT:-}" ] || die "BOND_OUTPOINT missing. Run bond first."
  ensure_valid_bond_outpoint
  if pid_alive "$VALIDATOR_PID"; then
    say "validator already running pid=$(cat "$VALIDATOR_PID")"
    return
  fi
  say "start validator sidecar"
  (
    cd "$ROOT_DIR"
    env HOME="$HOME_DIR" "$BIN_DIR/kaspa-pq-validator" run \
      --node-wrpc-borsh "127.0.0.1:$WRPC_BORSH_PORT" \
      --validator-key "$VALIDATOR_KEY" \
      --stake-bond "$BOND_OUTPOINT" \
      --signed-epoch-db "$VALIDATOR_DB" \
      --network "$NETWORK" > "$VALIDATOR_LOG" 2>&1 &
    echo $! > "$VALIDATOR_PID"
  )
  sleep 2
  if pid_alive "$VALIDATOR_PID"; then
    printf 'validator started pid=%s\n' "$(cat "$VALIDATOR_PID")"
    printf 'log: %s\n' "$VALIDATOR_LOG"
  else
    tail -n 80 "$VALIDATOR_LOG" || true
    die "validator failed to start"
  fi
}

logs() {
  say "kaspad log"
  tail -n 80 "$KASPAD_LOG" 2>/dev/null || true
  say "miner log"
  tail -n 60 "$MINER_LOG" 2>/dev/null || true
  say "validator log"
  tail -n 80 "$VALIDATOR_LOG" 2>/dev/null || true
}

node_logs() {
  tail -n 120 "$KASPAD_LOG" 2>/dev/null || true
}

miner_logs() {
  tail -n 160 "$MINER_LOG" 2>/dev/null || true
}

validator_logs() {
  tail -n 160 "$VALIDATOR_LOG" 2>/dev/null || true
}

clean() {
  stop_all || true
  rm -rf "$ROOT_DIR"
  say "removed $ROOT_DIR"
}

stop_all() {
  stop_pid "validator" "$VALIDATOR_PID"
  stop_pid "miner" "$MINER_PID"
  stop_pid "caffeinate" "$CAFFEINATE_PID"
  stop_pid "kaspad" "$KASPAD_PID"
}

cmd="${1:-help}"
shift || true

case "$cmd" in
  help|-h|--help) usage ;;
  prepare) prepare ;;
  auto-node) prepare; start_node; status ;;
  auto-validator) auto_validator ;;
  start-node) start_node ;;
  stop-node) stop_pid "caffeinate" "$CAFFEINATE_PID"; stop_pid "kaspad" "$KASPAD_PID" ;;
  restart-node) stop_pid "caffeinate" "$CAFFEINATE_PID"; stop_pid "kaspad" "$KASPAD_PID"; start_node ;;
  status) status ;;
  doctor) doctor ;;
  collect-support-log) collect_support_log ;;
  wait-sync) wait_sync ;;
  logs) logs ;;
  node-logs) node_logs ;;
  miner-logs) miner_logs ;;
  validator-logs) validator_logs ;;
  keygen) keygen ;;
  miner-start) miner_start ;;
  miner-stop) stop_pid "miner" "$MINER_PID" ;;
  balance) balance ;;
  bond) bond "${1:-10MSK}" ;;
  validator-start) validator_start ;;
  validator-stop) stop_pid "validator" "$VALIDATOR_PID" ;;
  stop-all) stop_all ;;
  clean) clean ;;
  *) usage; die "unknown command: $cmd" ;;
esac
