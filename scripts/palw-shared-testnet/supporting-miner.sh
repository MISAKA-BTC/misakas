#!/usr/bin/env bash
# =============================================================================
# supporting-miner.sh — STN-008: the continuous algo-3 supporting miner.
#
#   usage:  ./supporting-miner.sh start
#           ./supporting-miner.sh stop
#
# PURPOSE:
#   This is a REAL algo-3 misaminer producing REAL supporting-chain blocks. It
#   is NOT the seeded test-only `palw_demo` path and it mints NO algo-4 / PALW
#   block. Its only job is liveness: a miner must run continuously so that
#     * palw-submit carriers reach INCLUSION (submit blocks on inclusion), and
#     * DAA / epoch / coinbase-maturity keep ADVANCING
#   for the DNS/provider/lifecycle stages that follow. On devnet-111 blocks are
#   instant (skip_proof_of_work=true); MINER_INTERVAL_MS just paces propagation.
#
# The miner's coinbase pays SUPPORTING_ADDR; those matured UTXOs are what the
# later bond stages spend. This script only launches/stops that one process.
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
# (never launches a second miner over a live one; appends to the log, never
# clobbers it); FAIL-CLOSED with actionable messages; a register_cleanup trap
# tears down a half-started miner so a failed `start` never leaks an orphan.
# It sources common.sh and uses ONLY its helpers — nothing is reimplemented.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# Nicer per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-supporting-miner}"; export PALW_LOG_TAG

# Supervised-process name (used by write_pid / is_running / stop_pid) and the
# success flag consulted by the cleanup trap. Both are GLOBAL on purpose: the
# EXIT trap runs after do_start() has returned, so a `local` flag would be gone
# by then and the trap would wrongly kill a successfully-started miner.
MINER_NAME="supporting-miner"
_SUPPORTING_STARTED_OK=0

usage() {
    cat >&2 <<EOF
usage: ${0##*/} {start|stop}

  start  Launch the continuous algo-3 supporting miner (misaminer) against
         node A's loopback gRPC pool, paying coinbase to \$SUPPORTING_ADDR.
         Continuous block production is what advances DAA / epoch / coinbase
         maturity and lets palw-submit carriers reach inclusion (STN-008).
         Idempotent: a no-op while a supporting miner is already running.
  stop   Stop the supporting miner (SIGTERM -> SIGKILL). Idempotent.
EOF
}

# ---------------------------------------------------------------------------
do_start() {
    # Idempotent: never launch a second miner over a live, matching one.
    if is_running "$MINER_NAME"; then
        log "$MINER_NAME already running (pid $(read_pid "$MINER_NAME")); nothing to do"
        return 0
    fi

    # --- Resolve the coinbase payout address (fail-closed) -----------------
    # SUPPORTING_ADDR is discovered by an earlier funding/keygen stage and lives
    # in the environment or artifacts/state.env. We do NOT invent one.
    local addr
    addr="$(state_get SUPPORTING_ADDR)"
    [ -n "$addr" ] || die "SUPPORTING_ADDR is empty — set the supporting miner's coinbase payout address first, e.g.  state_set SUPPORTING_ADDR ${ADDR_PREFIX:-misakadev}:<...>  (or export it / put it in env.local), then re-run."

    # Refuse to mine to a wrong-network address (unspendable coinbase here).
    local prefix="${ADDR_PREFIX:-}"
    if [ -n "$prefix" ]; then
        case "$addr" in
            "$prefix":?*) : ;;
            *) die "SUPPORTING_ADDR '$addr' does not match the network address prefix '${prefix}:' (NETWORK=$NETWORK); refusing to mine to a wrong-network address." ;;
        esac
    else
        warn "ADDR_PREFIX unset; skipping payout-address prefix check for SUPPORTING_ADDR"
    fi

    # --- Precondition: node A must be up ------------------------------------
    # The miner --pool is node A's gRPC, which binds loopback only ($RPC_BIND);
    # so the miner must run co-located with node A. Gate on node A's RPC being
    # answerable before we spawn (fail-closed, actionable).
    wait_rpc_up a || die "node A RPC did not come up — start node-a.sh (and node-b.sh) before the supporting miner."

    require_cmd nohup

    local pool log_file pid
    pool="$(node_grpc a)"                                  # 127.0.0.1:<A_GRPC_PORT> (loopback gRPC)
    log_file="$PALW_DATA_ROOT/logs/miner-supporting.log"

    # If start does not fully succeed, tear down the half-started miner so we
    # never leak an orphan. On success we set _SUPPORTING_STARTED_OK=1 and this
    # snippet becomes a no-op, so the miner SURVIVES this launcher's exit.
    register_cleanup 'if [ "${_SUPPORTING_STARTED_OK:-0}" != "1" ]; then stop_pid "$MINER_NAME"; fi'

    log "starting $MINER_NAME: pool=$pool wallet=$addr worker=supporting interval=${MINER_INTERVAL_MS}ms -> $log_file"

    # Continuous (--blocks 0) supporting miner. Append to the log (never clobber
    # a prior run). nohup + </dev/null detaches it so it keeps running after
    # this launcher returns. Only verified misaminer flags are used.
    nohup "$MINER" \
        --pool "$pool" \
        --network-id "$NETWORK" \
        --wallet "$addr" \
        --worker supporting \
        --blocks 0 \
        --min-block-interval-ms "$MINER_INTERVAL_MS" \
        >>"$log_file" 2>&1 </dev/null &
    pid=$!

    write_pid "$MINER_NAME" "$pid"

    # Fail-closed startup check: give it a moment to settle, then confirm the
    # recorded process is still alive (catches immediate exit on a bad pool /
    # wallet). is_running also re-verifies argv+start-time (PID-reuse safe).
    sleep 1
    if ! is_running "$MINER_NAME"; then
        die "$MINER_NAME (pid $pid) exited immediately — inspect $log_file (verify node A gRPC pool $pool and wallet $addr)."
    fi

    _SUPPORTING_STARTED_OK=1
    log "$MINER_NAME up (pid $pid) — continuous algo-3 blocks advancing DAA/epoch/maturity and enabling carrier inclusion"
    return 0
}

# ---------------------------------------------------------------------------
do_stop() {
    # Idempotent: stop_pid SIGTERM->wait->SIGKILL, and just removes a stale or
    # absent pidfile (returning 0) when nothing is running. No cleanup trap is
    # needed here — stop creates no persistent process to unwind.
    stop_pid "$MINER_NAME"
    log "$MINER_NAME stopped"
    return 0
}

# ---------------------------------------------------------------------------
# Dispatch. Validate the action before load_env so --help works unconfigured.
ACTION="${1:-}"
case "$ACTION" in
    -h|--help|help) usage; exit 0 ;;
    start|stop)     : ;;
    "")             usage; die "missing action (expected: start|stop)" ;;
    *)              usage; die "unknown action '$ACTION' (expected: start|stop)" ;;
esac

load_env

case "$ACTION" in
    start) do_start ;;
    stop)  do_stop  ;;
esac
