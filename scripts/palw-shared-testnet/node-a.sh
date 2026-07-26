#!/usr/bin/env bash
# =============================================================================
# node-a.sh — start node A of the closed two-node PALW Phase-0 testnet (STN-004).
#
# A readiness-gated launcher for the `kaspad` process that plays node A. It does
# ONE job: bring node A up (or confirm it is already up) and prove it is ready.
# It is parameterised by a single env var:
#
#   NODE_A_MODE = bootstrap | validator      (default: bootstrap)
#
#     bootstrap : the initial genesis-follower / block source. Adds
#                 --enable-unsynced-mining so the supporting miner can extend
#                 the chain before any peer has synced. NO validator/beacon.
#                 (STN-006: once synced, drop --enable-unsynced-mining by
#                 re-launching in validator mode — restart-a-synced.sh stops
#                 this node first, then calls us with NODE_A_MODE=validator.)
#
#     validator : NO --enable-unsynced-mining. Adds the IN-PROCESS DNS
#                 validator + beacon (invariant 5: it runs inside kaspad):
#                   --enable-validator --enable-beacon --validator-mode=active
#                   --validator-key=$DNS_SEED --stake-bond=$DNS_BOND
#                 Requires DNS_SEED (the validator/DNS seed FILE) and DNS_BOND
#                 (the stake-bond outpoint <txid>:<index>) from the earlier bond
#                 stage (dns-validator.sh); fail-closed if either is missing.
#
# Flags passed in BOTH modes: the network family flag (--devnet/--testnet) +
# --netsuffix, --appdir, --archival, --utxoindex, loopback RPC (gRPC + wRPC-borsh
# on the A ports), P2P --listen on 0.0.0.0:A_P2P, --connect to node B, and
# --palw-enable-algo4 (see the identical-on-all-nodes note in step 3).
#
# Design rules obeyed (same as common.sh):
#   * IDEMPOTENT  — if node A is already running (PID+argv+start-time match) this
#                   is a no-op; a prior log is rotated (never silently truncated);
#                   DNS seed/bond are consumed read-only (never overwritten).
#   * FAIL-CLOSED — every readiness gate's return code is checked; on any failure
#                   the half-started node is stopped by the cleanup trap and the
#                   script exits non-zero with an actionable message.
#   * HONEST      — starts a REAL kaspad with only verified flags; it never
#                   invokes the seeded test-only palw_demo path, and it claims
#                   readiness only for conditions it actually gates on.
#
# All shared behaviour lives in common.sh and is CALLED, never reimplemented.
# =============================================================================

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="node-a"          # tag all log/warn/die lines from this stage.
load_env                       # source config + state.env, make dirs, bind bins.

NODE="a"                       # this script is node A (label understood by helpers)
NAME="node-a"                  # pid record / supervised-process name
LOGF="$(node_log "$NODE")"     # $PALW_DATA_ROOT/logs/node-a.log (grepped by gates)

# -----------------------------------------------------------------------------
# Local helpers (there is no common.sh gate for a log-marker match, so this one
# is defined here; it mirrors the common.sh gate contract: 0 ok, non-zero+WARN
# on timeout — the caller MUST check the return code).
# -----------------------------------------------------------------------------
# _wait_log_line <logfile> <extended-regex> [timeout] [interval]
_wait_log_line() {
    local logf="$1" re="$2"
    local timeout="${3:-$GATE_TIMEOUT_SECS}" interval="${4:-$GATE_POLL_SECS}"
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        if [ -f "$logf" ] && grep -Eiq "$re" "$logf"; then
            return 0
        fi
        [ "$(date +%s)" -ge "$deadline" ] && { warn "log marker /$re/ not seen in $logf after ${timeout}s"; return 1; }
        sleep "$interval"
    done
}
# _fail_node <msg> — dump the tail of the log for context, then die (fail-closed;
#   the cleanup trap stops the half-started node because _NODE_A_OK is unset).
_fail_node() {
    if [ -f "$LOGF" ]; then
        warn "last 20 log lines ($LOGF):"
        tail -n 20 "$LOGF" 1>&2 2>/dev/null || true
    fi
    die "$1"
}
# add_arg <flag> — append one verified flag to the kaspad argv (bash-3.2 safe
#   index assignment; matches common.sh's array idiom).
add_arg() { ARGS[${#ARGS[@]}]="$1"; }

# -----------------------------------------------------------------------------
# 0. validate NODE_A_MODE.
# -----------------------------------------------------------------------------
NODE_A_MODE="${NODE_A_MODE:-bootstrap}"
case "$NODE_A_MODE" in
    bootstrap|validator) : ;;
    *) die "NODE_A_MODE must be 'bootstrap' or 'validator', got '$NODE_A_MODE'" ;;
esac

# -----------------------------------------------------------------------------
# 1. idempotency: if node A is already running, do nothing.
# -----------------------------------------------------------------------------
if is_running "$NAME"; then
    log "node-a already running (pid $(read_pid "$NAME") mode=$NODE_A_MODE requested); no-op."
    log "  changing mode (bootstrap<->validator) needs a stop first: restart-a-synced.sh / stop.sh"
    exit 0
fi
# A leftover, non-live pid record is just a stale artifact. Note it; write_pid
# (step 7) records the fresh one — the documented idempotent replace, NOT the
# silent overwrite of a live process (is_running above already ruled that out).
if [ -f "$(pid_file "$NAME")" ]; then
    warn "stale pid record for $NAME (recorded process is not alive); will replace on relaunch"
fi

# -----------------------------------------------------------------------------
# 2. network family flag. Verified present: --devnet | --testnet. Anything else
#    is unsupported by this harness — fail-closed rather than invent a flag.
# -----------------------------------------------------------------------------
case "$NETWORK_BASE" in
    devnet)  NET_FLAG="--devnet"  ;;
    testnet) NET_FLAG="--testnet" ;;
    *) die "unsupported NETWORK_BASE='$NETWORK_BASE' (this harness verifies devnet|testnet only; see env.example)" ;;
esac

# -----------------------------------------------------------------------------
# 3. algo-4 switch. --palw-enable-algo4 MUST be passed IDENTICALLY on node-a AND
#    node-b (never a subset) — it is a runtime override of the shipped
#    palw_algo4_accept=false and a closed-net wiring switch only. Both node
#    scripts read this SAME env knob (PALW_ENABLE_ALGO4, default on), so they
#    stay identical; do NOT enable it on only one node.
# -----------------------------------------------------------------------------
case "$(printf '%s' "${PALW_ENABLE_ALGO4:-1}" | tr 'A-Z' 'a-z')" in
    1|true|yes|on)  WANT_ALGO4=1 ;;
    0|false|no|off) WANT_ALGO4=0 ;;
    *) die "PALW_ENABLE_ALGO4 must be 0/1 (true/false), got '${PALW_ENABLE_ALGO4:-}'" ;;
esac

# -----------------------------------------------------------------------------
# 4. validator-mode prerequisites (fail-closed, read-only). DNS_SEED / DNS_BOND
#    are produced by the bond stage and overlaid from artifacts/state.env by
#    load_env. DNS_SEED is a seed FILE PATH — never the seed value — so nothing
#    secret reaches argv or the log.
# -----------------------------------------------------------------------------
DNS_SEED_VAL="${DNS_SEED:-}"
DNS_BOND_VAL="${DNS_BOND:-}"
if [ "$NODE_A_MODE" = "validator" ]; then
    [ -n "$DNS_SEED_VAL" ] || die "NODE_A_MODE=validator needs DNS_SEED (validator/DNS seed FILE). Run the bond stage (dns-validator.sh) first."
    [ -f "$DNS_SEED_VAL" ] || die "DNS_SEED is not a readable file: $DNS_SEED_VAL (created by 'kaspa-pq-validator keygen --out')."
    [ -n "$DNS_BOND_VAL" ] || die "NODE_A_MODE=validator needs DNS_BOND (validator stake-bond outpoint <txid>:<index>). Run dns-validator.sh (bond) first."
    case "$DNS_BOND_VAL" in
        *:*) : ;;
        *) die "DNS_BOND must be <txid>:<index>, got '$DNS_BOND_VAL'" ;;
    esac
    _bidx="${DNS_BOND_VAL##*:}"; _btxid="${DNS_BOND_VAL%:*}"
    case "$_bidx" in ''|*[!0-9]*) die "DNS_BOND index must be numeric: '$DNS_BOND_VAL'" ;; esac
    [ -n "$_btxid" ] || die "DNS_BOND txid is empty: '$DNS_BOND_VAL'"
fi

# -----------------------------------------------------------------------------
# 5. assemble the kaspad argv (verified flags only — invents nothing).
#    RPC (gRPC + wRPC-borsh) binds loopback ($RPC_BIND). P2P listens on
#    0.0.0.0:A_P2P (the PALW preset still rejects any IP not in --connect
#    pre-handshake). --connect points at node B (NODE_B_HOST:B_P2P).
# -----------------------------------------------------------------------------
ARGS=()
add_arg "$NET_FLAG"
add_arg "--netsuffix=$NETSUFFIX"
add_arg "--appdir=$(node_appdir "$NODE")"
# Archival is now OPT-IN (PALW_ARCHIVAL=1): with it, NOTHING is ever pruned and the
# datadir grows without bound (~1.2 GB / 14 h observed at 1 BPS — mostly block/tx
# data incl. the validator's per-epoch ML-DSA attestation traffic). devnet-palw-111
# no longer mandates archival; per-block PALW validation never reads below the
# pruning point, and a pruned late-join still fails closed at IBD where it should.
# --yes: a datadir that RAN archival before asks an interactive "may delete
# archived data, confirm?" on the first non-archival start; this harness is
# headless and that deletion (pruning the backlog) is exactly the intent.
if [ "${PALW_ARCHIVAL:-0}" = 1 ]; then add_arg "--archival"; else add_arg "--yes"; fi
add_arg "--utxoindex"
add_arg "--listen=0.0.0.0:$A_P2P_PORT"
add_arg "--rpclisten=$RPC_BIND:$A_GRPC_PORT"
add_arg "--rpclisten-borsh=$RPC_BIND:$A_WRPC_PORT"
add_arg "--connect=$(node_p2p_addr b)"
# Extra P2P peers (STN-014): PALW_CONNECT_PEERS is a space-separated list of
# host:port addresses to additionally --connect. The kaspad --connect flag is
# multi-value (ArgAction::Append), and the PALW preset derives its pre-handshake
# inbound IP allowlist FROM --connect, so listing a joiner here both dials it and
# admits it. This is how a third node joins the closed set without editing the
# script: add its address on BOTH existing nodes + restart, and start the joiner
# with --connect back at A and B. Empty by default (the 2-node A<->B star).
for _peer in ${PALW_CONNECT_PEERS:-}; do
    add_arg "--connect=$_peer"
done
[ "$WANT_ALGO4" = "1" ] && add_arg "--palw-enable-algo4"

if [ "$NODE_A_MODE" = "bootstrap" ]; then
    # bootstrap block source: permit mining before any peer has synced.
    add_arg "--enable-unsynced-mining"
else
    # validator: in-process DNS validator + beacon. --validator-key is a FILE
    # PATH (never the seed value); --stake-bond is a public outpoint.
    add_arg "--enable-validator"
    add_arg "--enable-beacon"
    add_arg "--validator-mode=active"
    add_arg "--validator-key=$DNS_SEED_VAL"
    add_arg "--stake-bond=$DNS_BOND_VAL"
fi

# -----------------------------------------------------------------------------
# 6. preserve any prior log (rotate — never silently overwrite). We only get
#    here when node A is NOT running, so a fresh log also keeps the readiness
#    grep in step 8 from matching a stale run's endpoint line.
# -----------------------------------------------------------------------------
if [ -s "$LOGF" ]; then
    _bak="$LOGF.$(date '+%Y%m%d-%H%M%S')"
    if mv "$LOGF" "$_bak" 2>/dev/null; then
        log "rotated previous node-a log -> $_bak"
    else
        warn "could not rotate previous log $LOGF (continuing; the readiness grep may see stale lines)"
    fi
fi

# -----------------------------------------------------------------------------
# 7. launch the daemon in the background and record it.
# -----------------------------------------------------------------------------
log "starting kaspad node-a: mode=$NODE_A_MODE network=$NETWORK algo4=$WANT_ALGO4"
log "  argv: $KASPAD ${ARGS[*]}"
# nohup + </dev/null detaches from the controlling terminal (SIGHUP-safe), matching node-b.sh.
nohup "$KASPAD" "${ARGS[@]}" >> "$LOGF" 2>&1 </dev/null &
NODE_A_PID=$!
write_pid "$NAME" "$NODE_A_PID"

# Stop the half-started node on ANY early failure. Disarmed by setting
# _NODE_A_OK=1 once all gates pass, so a successful launch LEAVES node A running
# (this is a daemon launcher, not a run-to-completion job). Registered only on
# the launch path — the idempotent no-op above returns before this, so an
# already-running node is never touched by the trap.
register_cleanup 'if [ "${_NODE_A_OK:-0}" != "1" ]; then warn "node-a launch aborted before readiness; stopping half-started node-a"; stop_pid node-a >/dev/null 2>&1 || true; fi'

# -----------------------------------------------------------------------------
# 8. readiness gates (STN-004) — each fail-closed.
# -----------------------------------------------------------------------------
# 8a. survived the first second (catches bad flags / port-in-use fast crashes
#     with a clear message instead of a slow wRPC timeout).
sleep 1
is_running "$NAME" || _fail_node "node-a exited immediately after launch (bad flags? A ports already in use? a leftover kaspad on $A_P2P_PORT/$A_GRPC_PORT/$A_WRPC_PORT?)"

# 8b. the node's own wRPC answers a status query.
if ! wait_rpc_up "$NODE"; then
    _fail_node "node-a wRPC did not come up on $(node_wrpc "$NODE") (is A_WRPC_PORT=$A_WRPC_PORT free? RPC_BIND=$RPC_BIND)"
fi

# 8c. startup actually printed its endpoints (init completed, not just a socket).
if ! _wait_log_line "$LOGF" 'MISAKA node endpoints'; then
    _fail_node "node-a did not log the 'MISAKA node endpoints' line within the gate window (startup incomplete)"
fi

# 8d. validator mode: confirm the in-process validator service started. (The
#     stronger dns_confirmed:true + advancing-anchor gate belongs to
#     dns-validator.sh — it is intentionally NOT asserted here.)
if [ "$NODE_A_MODE" = "validator" ]; then
    if ! _wait_log_line "$LOGF" '\[validator-service\]'; then
        _fail_node "validator/beacon service did not start (its [validator-service] line never appeared); check DNS_SEED / DNS_BOND"
    fi
fi

# -----------------------------------------------------------------------------
# 9. success — disarm the failure cleanup and report.
# -----------------------------------------------------------------------------
_NODE_A_OK=1
log "node-a READY: mode=$NODE_A_MODE pid=$NODE_A_PID wrpc=$(node_wrpc "$NODE") listen=0.0.0.0:$A_P2P_PORT connect=$(node_p2p_addr b) log=$LOGF"
if [ "$NODE_A_MODE" = "validator" ]; then
    log "  next: dns-validator.sh gates dns_confirmed:true + an advancing dns_anchor (not gated here)."
fi
exit 0
