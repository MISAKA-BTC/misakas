#!/usr/bin/env bash
# =============================================================================
# node-b.sh — STN-004: start PALW closed-testnet node B (Phase-0 wiring).
#
#   usage:  ./node-b.sh            # start (default action)
#           ./node-b.sh start
#           ./node-b.sh stop
#
# WHY THIS EXISTS (honest scope):
#   Node B is ALWAYS a NON-BOOTSTRAP peer. It never carries
#   --enable-unsynced-mining (that is node A's bootstrap-only lever, dropped
#   after sync per STN-006) and runs NO validator / beacon / algo-4-miner flags.
#   B simply joins the closed mesh as an archival, utxo-indexed kaspad that DIALS
#   node A (--connect=NODE_A_HOST:A_P2P) and ACCEPTS algo-4 blocks
#   (--palw-enable-algo4). That accept flag must be IDENTICAL on every node (never
#   a subset); it is a runtime override of the shipped palw_algo4_accept=false and
#   a closed-net wiring switch only — never a public / value-bearing network.
#
#   This node performs NO inference, mints NO algo-4 / PALW block, and never
#   touches the seeded test-only `palw_demo` path. Its whole job is to be the
#   second, independent P2P peer so the later stages can assert both-node parity.
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
#   (never launches a second node over a live, matching one; a stale pidfile is
#   cleaned; a foreign process already bound to B's wRPC is a fail-closed error,
#   not a silent collision; an existing log is ROTATED, never silently clobbered);
#   FAIL-CLOSED with actionable messages (every readiness-gate return code is
#   checked); a register_cleanup trap tears down a half-started node so a failed
#   start never leaks an orphan, while a SUCCESSFUL start leaves node B running.
#   It sources common.sh and uses ONLY its helpers — nothing is reimplemented.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# Nicer per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-node-b}"; export PALW_LOG_TAG

# Supervised-process name (used by write_pid / is_running / stop_pid) and the
# success flag consulted by the cleanup trap. Both are GLOBAL on purpose: the
# EXIT trap runs after do_start() has returned, so a `local` flag would be gone
# by then and the trap would wrongly kill a node that started successfully.
NODE_NAME="node-b"
_NODE_B_STARTED_OK=0

usage() {
    cat >&2 <<EOF
usage: ${0##*/} {start|stop}

  start  Launch node B (kaspad) as a NON-BOOTSTRAP archival + utxoindex peer that
         dials node A (--connect=\$NODE_A_HOST:\$A_P2P_PORT) and accepts algo-4
         blocks (--palw-enable-algo4, identical on every node). Logs to
         logs/node-b.log, records a PID, then gates on wait_rpc_up +
         wait_peer_connected (STN-004). Default action when none is given.
         Idempotent: a no-op (re-verified) while a matching node B is running.
  stop   Stop node B (SIGTERM -> SIGKILL). Idempotent.
EOF
}

# ---------------------------------------------------------------------------
do_start() {
    local net_flag appdir log_file connect grpc wrpc pid rotated

    # Map the bare network base to the verified kaspad network flag. Only the two
    # wired presets are accepted; anything else is a fail-closed error — this
    # harness invents no flags (--devnet / --testnet are the only verified ones).
    case "$NETWORK_BASE" in
        devnet)  net_flag='--devnet'  ;;
        testnet) net_flag='--testnet' ;;
        *) die "node-b: NETWORK_BASE='$NETWORK_BASE' is not wired (expected 'devnet' or 'testnet'); fix NETWORK_BASE in your env (see env.example)." ;;
    esac

    appdir="$(node_appdir b)"          # $PALW_DATA_ROOT/node-b (created by load_env)
    log_file="$(node_log b)"           # $PALW_DATA_ROOT/logs/node-b.log
    connect="$(node_p2p_addr a)"       # NODE_A_HOST:A_P2P — B ALWAYS dials A
    grpc="$(node_grpc b)"              # 127.0.0.1:B_GRPC — loopback RPC only
    wrpc="$(node_wrpc b)"              # 127.0.0.1:B_WRPC — loopback RPC only

    # --- Idempotency guard 1: a managed node B is already up -----------------
    # Re-verify the readiness gates (cheap when healthy) and return 0 without
    # starting a second process. NO cleanup trap is armed on this path, so the
    # already-running node is never touched.
    if is_running "$NODE_NAME"; then
        log "$NODE_NAME already running (pid $(read_pid "$NODE_NAME")); re-verifying readiness gates"
        wait_rpc_up b         || die "$NODE_NAME is recorded running but wRPC $wrpc did not answer — inspect $log_file (a wedged node? stop it with ./node-b.sh stop and retry)."
        wait_peer_connected b || die "$NODE_NAME is recorded running but no connected peer appears in $log_file — is node A up at $connect with matching --palw-enable-algo4?"
        log "$NODE_NAME already ready: wRPC $wrpc, peer connected, appdir $appdir"
        return 0
    fi

    # A stale / PID-reuse-mismatched pidfile (crash, reboot) is cleaned, never
    # trusted. stop_pid on a non-running name just removes the record (rc 0).
    stop_pid "$NODE_NAME" >/dev/null 2>&1 || true

    # --- Idempotency guard 2: refuse to collide with a FOREIGN node ----------
    # Something already answering on B's wRPC port, with no managed pidfile, is a
    # fail-closed error — we will not launch a colliding node B on top of it.
    if _endpoint_open "$wrpc" && "$VAL" status --node-wrpc-borsh "$wrpc" --network "$NETWORK" >/dev/null 2>&1; then
        die "$NODE_NAME: a process is already serving wRPC $wrpc but no managed pidfile exists — refusing to start a colliding node B (stop that process, or free B_WRPC_PORT)."
    fi
    # (_endpoint_open first: VAL status uses a RETRY connect and would hang against a
    #  down port — the normal case here, since we are about to LAUNCH node B.)

    # --- Precondition: node A must be reachable to dial ----------------------
    # B connects OUTBOUND to A. On a single host (loopback NODE_A_HOST) node A
    # MUST already be running locally. In two-host mode NODE_A_HOST is remote and
    # cannot be probed from here (A's RPC is loopback-only on its own host), so we
    # rely on the wait_peer_connected gate + its timeout instead.
    case "$NODE_A_HOST" in
        127.0.0.1|localhost|::1)
            # NODE_B_ALLOW_ISOLATED_START=1 (G7 reorg-parity ONLY): start node B
            # while node A is deliberately DOWN so a divergent branch can be mined
            # on B. The dial target is unchanged — kaspad keeps re-dialing
            # $connect, which is what reconnects the net automatically when node A
            # returns. The peer gate below is then EXPECTED to fail; the caller
            # must tolerate that and verify RPC-up itself. Default stays
            # fail-closed: without the env this is the same hard error as before.
            if [ "${NODE_B_ALLOW_ISOLATED_START:-0}" = 1 ]; then
                log "$NODE_NAME: NODE_B_ALLOW_ISOLATED_START=1 — starting WITHOUT node A running (fork drill); kaspad will retry the outbound dial to $connect until node A returns, and the peer gate below is expected to fail."
            else
                is_running node-a || die "$NODE_NAME: node A is not running on this host — start ./node-a.sh first (node B dials $connect). For a two-host run set NODE_A_HOST to node A's routable/Tailscale address."
            fi
            ;;
        *)
            log "$NODE_NAME: node A is remote ($NODE_A_HOST); relying on wait_peer_connected to confirm the P2P dial to $connect"
            ;;
    esac

    # --- Rotate the previous log (never silently clobber) --------------------
    # wait_peer_connected greps this file for the 'Connected to ... peer' line, so
    # a fresh, non-empty prior log would be a FALSE OK. Move it aside (preserving
    # history under a unique name) rather than truncating it.
    if [ -s "$log_file" ]; then
        rotated="$log_file.$(date +%Y%m%d-%H%M%S)"
        if [ -e "$rotated" ]; then rotated="$log_file.$(date +%Y%m%d-%H%M%S).$$"; fi
        mv "$log_file" "$rotated" || die "$NODE_NAME: cannot rotate previous log $log_file"
        log "rotated previous log -> $rotated"
    fi

    # --- Build the verified-flag-only argv -----------------------------------
    # NON-BOOTSTRAP: NO --enable-unsynced-mining and NO validator/beacon/palw-mine
    # flags here. RPC (gRPC + wRPC-borsh) binds loopback only; P2P binds 0.0.0.0
    # but the PALW preset rejects any IP not in --connect pre-handshake.
    local -a args
    args=(
        "$net_flag"
        --netsuffix="$NETSUFFIX"
        --appdir="$appdir"
        --utxoindex
        --listen="0.0.0.0:$B_P2P_PORT"
        --rpclisten="$grpc"
        --rpclisten-borsh="$wrpc"
        --connect="$connect"
    )
    # Archival is OPT-IN (PALW_ARCHIVAL=1); see node-a.sh for the rationale —
    # without pruning the datadir grows without bound. --yes answers the one-time
    # "previously archival, may delete archived data" prompt (headless harness).
    if [ "${PALW_ARCHIVAL:-0}" = 1 ]; then args[${#args[@]}]="--archival"; else args[${#args[@]}]="--yes"; fi
    # Extra P2P peers (STN-014): PALW_CONNECT_PEERS is a space-separated list of
    # host:port addresses to additionally --connect (multi-value flag). The PALW
    # preset derives its pre-handshake inbound IP allowlist FROM --connect, so a
    # joiner listed here is both dialed and admitted. Empty by default (the 2-node
    # A<->B star). Must be set IDENTICALLY on A and B. See node-a.sh / env.example.
    for _peer in ${PALW_CONNECT_PEERS:-}; do
        args+=(--connect="$_peer")
    done

    # --palw-enable-algo4 must be IDENTICAL on ALL nodes (never a subset). Honor
    # the env toggle; if it is off, say so loudly so the operator keeps A and B in
    # parity (a subset mismatch would reject algo-4 blocks on one side).
    case "$(printf '%s' "${PALW_ENABLE_ALGO4:-1}" | tr 'A-Z' 'a-z')" in
        1|true|yes|on)
            args+=( --palw-enable-algo4 )
            ;;
        *)
            warn "$NODE_NAME: PALW_ENABLE_ALGO4='${PALW_ENABLE_ALGO4:-}' -> starting WITHOUT --palw-enable-algo4; node A MUST also omit it (this flag must match on EVERY node)."
            ;;
    esac

    require_cmd nohup

    # If start does not fully succeed, tear down the half-started node so we never
    # leak an orphan. On success we set _NODE_B_STARTED_OK=1 and this snippet
    # becomes a no-op, so node B SURVIVES this launcher's exit.
    register_cleanup 'if [ "${_NODE_B_STARTED_OK:-0}" != "1" ]; then stop_pid "$NODE_NAME"; fi'

    log "starting $NODE_NAME: $KASPAD ${args[*]}"

    # Append to this (freshly-rotated) log; nohup + </dev/null detaches kaspad so
    # it keeps running after this launcher returns. Only verified kaspad flags.
    nohup "$KASPAD" "${args[@]}" >>"$log_file" 2>&1 </dev/null &
    pid=$!

    write_pid "$NODE_NAME" "$pid"

    # Fail-closed startup check: give kaspad a moment to bind, then confirm the
    # recorded process is still alive (catches an immediate exit on a bad flag or
    # a port already in use). is_running also re-verifies argv + start-time.
    sleep 1
    if ! is_running "$NODE_NAME"; then
        die "$NODE_NAME (pid $pid) exited immediately — inspect $log_file (check that B ports $B_P2P_PORT/$B_GRPC_PORT/$B_WRPC_PORT are free and the flags are valid)."
    fi

    # --- Readiness gates (STN-004) — ALWAYS check the return code ------------
    # wait_peer_connected greps this run's log for the 'Connected to ... peer'
    # line node B prints on the outbound P2P handshake to node A.
    wait_rpc_up b || die "$NODE_NAME: wRPC $wrpc did not come up in time — inspect $log_file."
    if [ "${NODE_B_ALLOW_ISOLATED_START:-0}" = 1 ]; then
        # Fork drill: node A is deliberately DOWN, so the peer gate can never pass —
        # and letting it fail would trip the half-start cleanup and KILL the node we
        # just launched on purpose. kaspad keeps re-dialing $connect; the peer link
        # (and the caller's convergence gates) come back when node A returns.
        log "$NODE_NAME started ISOLATED (NODE_B_ALLOW_ISOLATED_START=1): peer gate skipped; kaspad keeps re-dialing $connect until node A returns."
    else
        wait_peer_connected b || die "$NODE_NAME: no P2P peer connected in time — is node A up at $connect with a matching --palw-enable-algo4 and reachable P2P port? inspect $log_file."
    fi

    _NODE_B_STARTED_OK=1
    log "$NODE_NAME ready: pid $pid, wRPC $wrpc, gRPC $grpc, dialed $connect, appdir $appdir"
    return 0
}

# ---------------------------------------------------------------------------
do_stop() {
    # Idempotent: stop_pid SIGTERM->wait->SIGKILL, and just removes a stale or
    # absent pidfile (returning 0) when nothing is running. No cleanup trap is
    # needed here — stop creates no persistent process to unwind.
    stop_pid "$NODE_NAME"
    log "$NODE_NAME stopped"
    return 0
}

# ---------------------------------------------------------------------------
# Dispatch. Validate the action before load_env so --help works unconfigured.
# `start` is the default so `./node-b.sh` (no args) starts node B.
ACTION="${1:-start}"
case "$ACTION" in
    -h|--help|help) usage; exit 0 ;;
    start|stop)     : ;;
    *)              usage; die "unknown action '$ACTION' (expected: start|stop)" ;;
esac

load_env

case "$ACTION" in
    start) do_start ;;
    stop)  do_stop  ;;
esac
