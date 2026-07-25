#!/usr/bin/env bash
# =============================================================================
# late-join.sh — P0-G row "late join": a FRESH node B joins the running network
# from genesis and must converge on node A's sink.
#
#   usage:  LATE_JOIN_WIPE_B=1 ./late-join.sh
#
# WHAT THIS DOES (honest scope):
#   1. Requires node A running + synced and the supporting miner advancing the
#      chain (a late join against a stalled chain proves nothing).
#   2. Stops node B if it is running, and — ONLY because LATE_JOIN_WIPE_B=1 was
#      explicitly set — deletes node B's appdir so B genuinely starts from
#      genesis. Without that env var this script refuses to touch B's data
#      (fail-closed: wiping a node dir is destructive and must be a deliberate
#      operator decision every time).
#   3. Starts node B via node-b.sh and waits until B reports node_synced=true
#      AND A/B converge on the SAME sink (wait_same_sink), i.e. the late joiner
#      followed the existing chain instead of forking its own.
#
# WHAT THIS DOES NOT PROVE: nothing here exercises PALW lifecycle state on the
# joiner beyond what sink parity implies (provider bonds / manifests are
# re-checked by the main harness stages), and a same-host join says nothing
# about WAN behaviour — the two-host variant of this run is the external row.
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
# (a B that is already a fresh-genesis join in progress is just re-gated);
# FAIL-CLOSED with actionable messages; helpers from common.sh ONLY.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="${PALW_LOG_TAG:-late-join}"; export PALW_LOG_TAG
load_env

LATE_JOIN_TIMEOUT="${LATE_JOIN_TIMEOUT:-600}"

# -- 1. preconditions: A up + synced, miner advancing ------------------------
_endpoint_open "$(node_wrpc a)" || die "node A RPC is not reachable — start node-a.sh first"
wait_node_synced a 30 || die "node A is not synced; a late join needs a healthy source chain"
is_running supporting-miner || die "supporting miner is not running — the chain must be advancing for a meaningful late join"

daa_before="$(node_sink_daa a)" || die "cannot read node A sink DAA"
log "node A sink DAA at start: $daa_before"

# -- 2. stop B and (explicitly authorized) wipe its appdir -------------------
if is_running node-b; then
    log "stopping the running node B before the fresh join"
    stop_pid node-b
fi
b_dir="$(node_appdir b)"
if [ -d "$b_dir" ]; then
    if [ "${LATE_JOIN_WIPE_B:-0}" != "1" ]; then
        die "node B appdir exists ($b_dir). A late-join test requires a FRESH B; re-run with LATE_JOIN_WIPE_B=1 to authorize deleting it (destructive, deliberate)."
    fi
    log "LATE_JOIN_WIPE_B=1 — removing $b_dir for a genuine genesis join"
    rm -rf -- "$b_dir"
fi

# -- 3. start fresh B and require convergence --------------------------------
log "starting node B from genesis"
"$SCRIPT_DIR/node-b.sh"

wait_rpc_up b 60                       || die "node B RPC did not come up"
wait_peer_connected a 60               || die "A does not see the late joiner as a peer"
wait_node_synced b "$LATE_JOIN_TIMEOUT" || die "late joiner did not reach node_synced=true within ${LATE_JOIN_TIMEOUT}s"
wait_same_sink "$LATE_JOIN_TIMEOUT"     || die "late joiner did not converge on node A's sink"

daa_after="$(node_sink_daa a)" || die "cannot read node A sink DAA after the join"
[ "$daa_after" -gt "$daa_before" ] || die "chain did not advance during the join (miner stalled?) — the run is inconclusive, not a pass"

sink_a="$(node_sink a)"; sink_b="$(node_sink b)"
log "PASS: late joiner converged. sink=$sink_a (A) == $sink_b (B), DAA $daa_before -> $daa_after"
