#!/usr/bin/env bash
# =============================================================================
# restart-a-synced.sh — STN-006: transition node A bootstrap -> validator.
#
#   usage:  ./restart-a-synced.sh
#
# SCOPE:
#   Node A is first brought up as the BOOTSTRAP node with --enable-unsynced-mining
#   (so it can produce the very first blocks before any peer has a chain). That
#   flag is a bootstrap-only crutch and MUST be dropped once the node has really
#   synced (STN-006). This stage:
#     1. confirms node A has genuinely synced (node_synced=true), THEN
#     2. stops it cleanly, and
#     3. restarts it via node-a.sh in VALIDATOR mode (NODE_A_MODE=validator),
#        which starts kaspad WITHOUT --enable-unsynced-mining and WITH the
#        in-process DNS/beacon validator (using $DNS_SEED as the validator key
#        and $DNS_BOND as the stake bond), then
#     4. re-verifies A/B converge on the same sink.
#
#   Once self-mining is gone, node A no longer produces its own blocks: the REAL
#   algo-3 misaminer from supporting-miner.sh (supervised name "supporting-miner")
#   is what keeps the chain advancing, so it MUST already be running. Node B must
#   also be running for the same-sink parity check. Nothing here invokes the
#   seeded test-only palw_demo path or mints any algo-4/PALW block.
#
# Preconditions (fail-closed, actionable if unmet):
#   * DNS_SEED + DNS_BOND present in artifacts/state.env (produced by the bond /
#     dns-validator.sh stage). This script never invents or overwrites them.
#   * node A running  (this is a *restart* of a live bootstrap node)
#   * node B running  (same-sink parity is A-vs-B)
#   * supporting miner running (chain advance after self-mining is dropped)
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
# (if node A is already a validator with no --enable-unsynced-mining, the restart
# is SKIPPED and only the readiness gates are re-verified — nothing is stopped or
# clobbered); FAIL-CLOSED with actionable messages; a register_cleanup trap warns
# the operator (never leaves a silent half-restart) if the transition is aborted
# mid-way. It sources common.sh and uses ONLY its helpers — nothing reimplemented.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# Nicer per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-restart-a-synced}"; export PALW_LOG_TAG

# Supervised-process names (as used by write_pid / is_running / stop_pid in the
# sibling stage scripts): node A = "node-a", supporting miner = "supporting-miner".
NODE_A_NAME="node-a"
NODE_B_NAME="node-b"
MINER_NAME="supporting-miner"

# node-a.sh is the ONLY thing that constructs node A's kaspad command line; this
# stage merely selects its mode. We never rebuild that command here.
NODE_A_SH="$SCRIPT_DIR/node-a.sh"

# GLOBAL trap-state flags. They MUST be global (not local): the EXIT trap runs
# after this script's body, so a scoped flag would be gone and the cleanup would
# misjudge the outcome. _DONE=1 only after all gates pass; _STOPPED_NODE_A=1 once
# we have actually stopped node A (i.e. we are inside the transition window).
_DONE=0
_STOPPED_NODE_A=0

# Cleanup trap. On a clean, complete run (_DONE=1) this is a no-op and node A
# SURVIVES this launcher's exit. If the transition is interrupted, it does NOT
# kill a node — it emits an actionable WARN describing the exact recovery step
# (re-running this script is idempotent). It never touches keys or state files.
stn006_on_exit() {
    [ "${_DONE:-0}" = "1" ] && return 0
    warn "STN-006 did not complete cleanly."
    if [ "${_STOPPED_NODE_A:-0}" = "1" ] && ! is_running "$NODE_A_NAME"; then
        warn "node A is currently STOPPED (bootstrap->validator transition was interrupted). Recover with:  ./restart-a-synced.sh  (idempotent), or start it directly:  NODE_A_MODE=validator ./node-a.sh"
    elif is_running "$NODE_A_NAME"; then
        warn "node A is running but was NOT confirmed as a converged validator; inspect $(node_log a) then re-run ./restart-a-synced.sh"
    fi
    return 0
}

load_env

# ---------------------------------------------------------------------------
# Preconditions — every one fail-closed with an actionable message. None of
# these mutates anything, so an early die leaves the running net untouched.
# ---------------------------------------------------------------------------

# DNS_SEED — path to the validator key seed file (keygen --out). Required to
# start node A in validator mode. We validate the FILE exists; we never read or
# print its contents (no secrets to log/argv).
DNS_SEED="$(state_get DNS_SEED)"
[ -n "$DNS_SEED" ] || die "DNS_SEED is not set in artifacts/state.env — run the bond / dns-validator.sh stage first (it keygen's the validator seed and state_set's DNS_SEED)."
[ -f "$DNS_SEED" ] || die "DNS_SEED points to a missing file: $DNS_SEED — re-run dns-validator.sh (validator seed keygen) to regenerate it; refusing to start a validator without its key."

# DNS_BOND — the validator stake-bond outpoint (txid:index) from `bond`.
DNS_BOND="$(state_get DNS_BOND)"
[ -n "$DNS_BOND" ] || die "DNS_BOND is not set in artifacts/state.env — run the bond / dns-validator.sh stage first (it prints bond_outpoint and state_set's DNS_BOND)."
case "$DNS_BOND" in
    *:*) : ;;
    *)   die "DNS_BOND is malformed: '$DNS_BOND' (expected <txid>:<index>) — re-run the bond stage." ;;
esac

# node-a.sh must be present next to us (it owns node A's launch command).
[ -f "$NODE_A_SH" ] || die "node-a.sh not found next to this script: $NODE_A_SH"

# node A must be running — this stage RESTARTS a live bootstrap node, it does not
# cold-start one. (If node A has crashed, bring it back with node-a.sh first.)
is_running "$NODE_A_NAME" || die "node A is not running — start it with ./node-a.sh (bootstrap) and let it sync before running STN-006."

# node B must be running — the same-sink parity check compares A vs B.
is_running "$NODE_B_NAME" || die "node B is not running — start it with ./node-b.sh; the same-sink parity check needs both nodes."

# The supporting miner MUST be running: once node A drops --enable-unsynced-mining
# it no longer self-mines, so a live supporting miner is what keeps the chain
# advancing (and what let the DNS bond mature in the first place). Fail-closed.
is_running "$MINER_NAME" || die "supporting miner ('$MINER_NAME') is not running — start it with ./supporting-miner.sh start before STN-006; node A stops self-mining after this transition and the chain would otherwise stall."

# ---------------------------------------------------------------------------
# Idempotency — inspect node A's live argv. If it is already a validator with no
# --enable-unsynced-mining, the transition is already done: skip the stop/restart
# and only re-verify the gates. (is_running above guarantees the pid record is
# live and matches, so read_pid/_proc_cmd are safe here.)
# ---------------------------------------------------------------------------
NA_PID="$(read_pid "$NODE_A_NAME")" || die "internal: node A pid record missing despite is_running"
NA_CMD="$(_proc_cmd "$NA_PID")"

ALREADY_VALIDATOR=0
case "$NA_CMD" in
    *--enable-unsynced-mining*)
        log "node A is running WITH --enable-unsynced-mining (bootstrap) — transitioning to validator mode."
        ;;
    *--enable-validator*)
        log "node A already running in validator mode without --enable-unsynced-mining — skipping restart (idempotent); re-verifying gates only."
        ALREADY_VALIDATOR=1
        ;;
    *)
        log "node A running without --enable-unsynced-mining but not as a validator — restarting into validator mode."
        ;;
esac

# ---------------------------------------------------------------------------
# The transition (skipped when already a validator).
# ---------------------------------------------------------------------------
if [ "$ALREADY_VALIDATOR" != "1" ]; then
    # Only drop the bootstrap self-mining crutch once node A has GENUINELY synced.
    wait_node_synced a \
        || die "node A did not report node_synced=true (timeout) — refusing to drop --enable-unsynced-mining before it is synced; check the supporting miner and the node B P2P peer are healthy, then retry."

    # Arm the cleanup trap for the transition window: from here until _DONE=1,
    # node A may be stopped or half-restarted, and an abort must warn (not leave
    # a silent broken state). On success this becomes a no-op.
    register_cleanup 'stn006_on_exit'

    _STOPPED_NODE_A=1
    stop_pid "$NODE_A_NAME" \
        || die "failed to stop node A cleanly (pid $NA_PID) — inspect it manually before retrying; not restarting over a process that would not stop."

    # Restart via node-a.sh in validator mode. node-a.sh reads NODE_A_MODE to
    # select the validator command line (no --enable-unsynced-mining; adds the
    # in-process DNS/beacon validator keyed by DNS_SEED / DNS_BOND). It records
    # the "node-a" pid and gates its own wRPC/peer readiness before returning.
    # We pass DNS_SEED/DNS_BOND explicitly as well; node-a.sh also reloads them
    # from state.env, so this is belt-and-suspenders, not a second source of truth.
    log "restarting node A into validator mode via node-a.sh (NODE_A_MODE=validator, no --enable-unsynced-mining) ..."
    NODE_A_MODE=validator DNS_SEED="$DNS_SEED" DNS_BOND="$DNS_BOND" bash "$NODE_A_SH" \
        || die "node-a.sh failed to start node A in validator mode — inspect $(node_log a); node A may be down (re-run ./restart-a-synced.sh to recover)."
fi

# ---------------------------------------------------------------------------
# Re-verify readiness (both the transition and the idempotent path land here).
# ---------------------------------------------------------------------------
wait_rpc_up a \
    || die "node A wRPC did not answer after the validator restart (timeout) — inspect $(node_log a)."

wait_same_sink \
    || die "nodes A and B did not converge on the same sink after the validator restart (timeout) — check P2P connectivity and that the supporting miner is advancing the chain."

_DONE=1
log "STN-006 complete: node A is a synced in-process validator (no --enable-unsynced-mining), and A/B share the same sink."
