#!/usr/bin/env bash
# misaka-node-liveness-probe.sh — the watchdog half of `misaka node liveness`.
#
# Run it from a systemd timer (or cron) on the host that runs the node. It asks the node, from
# OUTSIDE the process, whether the RPC answers and whether the chain has moved; on WEDGED (exit 11)
# it restarts the unit at once, on STALLED (exit 12) it restarts only if the stall has lasted a
# second full window (a stalled node is sometimes an idle chain), and it never restarts twice
# within COOLDOWN seconds. `systemctl Restart=` cannot do this: a hung process never exits.
#
#   NETWORK       misaka network id (default testnet-11)
#   RPC           node wRPC Borsh host:port (default: the network's local default)
#   UNIT          systemd unit to restart (default misaka-t11-node)
#   STATE_DIR     where the probe keeps state (default /var/lib/misaka)
#   STALL_SECS    no-progress window (default 900 = 7-8 blocks at 120 s)
#   TIMEOUT       RPC timeout seconds (default 15)
#   COOLDOWN      minimum seconds between restarts (default 600)
#   RESTART_ON_STALL=0  log STALLED but never restart on it (a fresh chain has designed silences:
#                 nobody produces during the first artifact map, and a floor at genesis bits is
#                 hours per block) — WEDGED still restarts
#   MISAKA_BIN    the misaka CLI (default: misaka on PATH)
#   DRY_RUN=1     decide, log, do not restart
set -uo pipefail
NETWORK="${NETWORK:-testnet-11}"
UNIT="${UNIT:-misaka-t11-node}"
STATE_DIR="${STATE_DIR:-/var/lib/misaka}"
STALL_SECS="${STALL_SECS:-900}"
TIMEOUT="${TIMEOUT:-15}"
COOLDOWN="${COOLDOWN:-600}"
RESTART_ON_STALL="${RESTART_ON_STALL:-1}"
MISAKA_BIN="${MISAKA_BIN:-misaka}"
mkdir -p "$STATE_DIR"
STAMP="$STATE_DIR/liveness.last-restart"

args=(--network "$NETWORK" --timeout "$TIMEOUT" --output json node liveness --state "$STATE_DIR/liveness.json" --stall-secs "$STALL_SECS")
[ -n "${RPC:-}" ] && args=(--rpc "$RPC" "${args[@]}")
out="$("$MISAKA_BIN" "${args[@]}" 2>&1)"; code=$?
logger -t misaka-liveness "$UNIT exit=$code $out" 2>/dev/null || echo "[misaka-liveness] $UNIT exit=$code $out" >&2

restart() {
  local why="$1"
  local now; now=$(date +%s)
  local last=0; [ -f "$STAMP" ] && last=$(cat "$STAMP" 2>/dev/null || echo 0)
  if [ $((now - last)) -lt "$COOLDOWN" ]; then
    logger -t misaka-liveness "$UNIT: $why, but a restart ran $((now - last))s ago (cooldown ${COOLDOWN}s) — not restarting" 2>/dev/null
    return 0
  fi
  if [ "${DRY_RUN:-0}" = "1" ]; then
    logger -t misaka-liveness "$UNIT: DRY_RUN — would restart ($why)" 2>/dev/null; echo "would restart $UNIT: $why" >&2; return 0
  fi
  echo "$now" > "$STAMP"
  logger -t misaka-liveness "$UNIT: restarting ($why)" 2>/dev/null
  systemctl restart "$UNIT"
}

case "$code" in
  0) exit 0 ;;
  11) restart "wedged: the RPC did not answer" ;;
  12)
    # A stalled verdict needs two consecutive windows before a restart: the chain itself may be
    # idle, and the probe cannot see peers' DAA from one node. `idle_for` is in the JSON detail.
    idle=$(printf '%s' "$out" | sed -n 's/.*no progress for \([0-9]*\)s.*/\1/p' | head -1)
    if [ "$RESTART_ON_STALL" != "1" ]; then
      logger -t misaka-liveness "$UNIT: stalled for ${idle:-?}s — RESTART_ON_STALL=0, warning only" 2>/dev/null; exit 0
    fi
    if [ -n "$idle" ] && [ "$idle" -ge $((STALL_SECS * 2)) ]; then restart "stalled for ${idle}s"; else exit 0; fi
    ;;
  *) exit 0 ;;   # a probe error (bad args, cannot write state) must not cause restarts
esac
