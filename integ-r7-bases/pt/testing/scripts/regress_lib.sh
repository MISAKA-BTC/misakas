#!/usr/bin/env bash
# Shared helpers for the VPS regression fixture.
#
# These run on hosts that also run production testnet-10. Everything here is written on the
# assumption that a mistake costs the network, not the test.

# NOT /tmp. These fixtures are two independently mined chains that cost hours to produce, and /tmp
# is cleared on reboot and by tmpfiles cleanup — a fixture that can evaporate is one you will
# rebuild at the worst moment, or worse, silently re-mine into something that is no longer the same
# experiment.
BASE=${BASE:-/var/lib/misaka-regression}
BIN=$BASE/src/target/release/kaspad
RPC=$BASE/src/target/release/regress-rpc

# Stop a process ONLY after proving it is one of ours.
#
# `pkill -f kaspad` on these hosts matches the production node. That is not a hypothetical: it has
# already happened once in this work, and systemd restarting the miner within seconds is the only
# reason it was not worse. So the pid comes from a pidfile we wrote, and /proc/<pid>/exe must
# resolve to a binary inside the regression tree before anything is signalled.
stop_regress_pid() {
  local pidfile=$1
  [ -f "$pidfile" ] || return 0
  local pid exe
  pid=$(cat "$pidfile" 2>/dev/null) || return 0
  [ -n "$pid" ] || return 0
  exe=$(readlink "/proc/$pid/exe" 2>/dev/null || true)
  case "$exe" in
    "$BASE"/*)
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 30); do
        [ -e "/proc/$pid" ] || break
        sleep 1
      done
      [ -e "/proc/$pid" ] && kill -9 "$pid" 2>/dev/null || true
      echo "stopped regression pid=$pid ($exe)"
      ;;
    "")
      echo "pid=$pid already gone"
      ;;
    *)
      echo "REFUSING to signal pid=$pid: exe=$exe is outside $BASE" >&2
      return 1
      ;;
  esac
  rm -f "$pidfile"
}

# Bring a node up on an EXISTING data directory. The counterpart to regress_node.sh, which wipes.
#
# Kept separate rather than adding a flag: the mined branches cost hours, and a script whose default
# is `rm -rf` should not be one keystroke away from the script used to restart them.
start_regress_node_resume() {
  local name=$1 p2p=$2 grpc=$3; shift 3
  [ -d "$BASE/$name" ] || { echo "no data directory $BASE/$name to resume" >&2; return 1; }
  nohup "$BIN" --simnet \
    --override-params-file="$BASE/shallow_preset.json" \
    --appdir="$BASE/$name" \
    --listen=0.0.0.0:"$p2p" \
    --rpclisten=127.0.0.1:"$grpc" \
    --rpclisten-borsh=127.0.0.1:$((grpc+1000)) \
    --rpclisten-json=127.0.0.1:$((grpc+2000)) \
    --disable-upnp --unsaferpc --enable-unsynced-mining --enforce-chain-participation \
    --nologfiles "$@" >> "$BASE/$name.log" 2>&1 &
  echo $! > "$BASE/$name.pid"
  sleep 8
  echo "RESUMED $name p2p=$p2p grpc=$grpc pid=$(cat "$BASE/$name.pid")"
}

# Wait until the probe reports a DAA score at or above a target, or give up.
# Echoes the final probe line either way — a timeout still has to say where the node got to.
wait_for_score() {
  local grpc=$1 target=$2 timeout=$3
  local deadline=$((SECONDS + timeout)) line score
  while [ $SECONDS -lt $deadline ]; do
    line=$("$RPC" "127.0.0.1:$grpc" 2>/dev/null || true)
    score=$(sed -n 's/.*virtual_daa_score=\([0-9]*\).*/\1/p' <<<"$line")
    if [ -n "$score" ] && [ "$score" -ge "$target" ]; then
      echo "$line"
      return 0
    fi
    sleep 5
  done
  echo "${line:-<probe never answered>}"
  return 1
}
