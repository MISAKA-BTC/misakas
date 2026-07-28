#!/usr/bin/env bash
# =============================================================================
# partition-rejoin.sh — P0-G row "partition/rejoin": node B is isolated from the
# network for a window while the chain advances, then rejoins and must converge
# back onto node A's sink.
#
#   usage:  ./partition-rejoin.sh            (default 60 s partition window)
#           PARTITION_SECS=300 ./partition-rejoin.sh
#
# SCOPE:
#   1. Requires A + B running, mutually synced (same sink), miner advancing.
#   2. "Partitions" B by stopping its process for PARTITION_SECS while A keeps
#      producing (records the sink gap that opens).
#   3. Restarts B via node-b.sh (same appdir — this is a REJOIN, not a fresh
#      join) and requires: peer reconnect, node_synced=true, and A/B sink
#      convergence, with the chain having genuinely advanced past the
#      pre-partition sink.
#
# LIMITATION: a single-host, single-miner
# topology cannot produce a SPLIT-BRAIN — the isolated side has no block
# production, so this exercises dark-window rejoin/convergence, NOT competing
# forks. The dual-production partition (miners on both sides, heavier-chain
# resolution on rejoin) needs the two-host setup with independent miners and
# stays on the external live-test row until that window exists.
#
# Design rules: set -euo pipefail; FAIL-CLOSED; helpers from common.sh ONLY.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="${PALW_LOG_TAG:-partition-rejoin}"; export PALW_LOG_TAG
load_env

PARTITION_SECS="${PARTITION_SECS:-60}"
REJOIN_TIMEOUT="${REJOIN_TIMEOUT:-600}"

# -- 1. preconditions: healthy, converged, advancing -------------------------
_endpoint_open "$(node_wrpc a)" || die "node A RPC is not reachable"
_endpoint_open "$(node_wrpc b)" || die "node B RPC is not reachable (partition needs a running B to isolate)"
is_running node-b || die "node B is not under harness supervision (node-b.sh) — refusing to kill an unmanaged process"
is_running supporting-miner || die "supporting miner is not running — the chain must advance during the partition"
wait_node_synced a 30 || die "node A is not synced"
wait_node_synced b 30 || die "node B is not synced"
wait_same_sink 60 || die "A/B are not converged before the partition — fix that first"

sink_pre="$(node_sink a)"
daa_pre="$(node_sink_daa a)" || die "cannot read pre-partition DAA"
log "pre-partition: converged sink=$sink_pre daa=$daa_pre; isolating B for ${PARTITION_SECS}s"

# -- 2. the dark window ------------------------------------------------------
stop_pid node-b
sleep "$PARTITION_SECS"
daa_dark="$(node_sink_daa a)" || die "cannot read node A DAA during the partition"
[ "$daa_dark" -gt "$daa_pre" ] || die "chain did not advance while B was dark (miner stalled?) — inconclusive, not a pass"
log "partition window closed: A advanced DAA $daa_pre -> $daa_dark while B was dark"

# -- 3. rejoin and converge ---------------------------------------------------
log "restarting node B (same appdir — rejoin)"
"$SCRIPT_DIR/node-b.sh"
wait_rpc_up b 60 || die "node B RPC did not come back"
wait_peer_connected a 60 || die "A does not see the rejoined B as a peer"
wait_node_synced b "$REJOIN_TIMEOUT" || die "rejoined B did not reach node_synced=true within ${REJOIN_TIMEOUT}s"
wait_same_sink "$REJOIN_TIMEOUT" || die "rejoined B did not converge on node A's sink"

sink_post="$(node_sink a)"; sink_post_b="$(node_sink b)"
daa_post="$(node_sink_daa b)" || die "cannot read post-rejoin DAA from B"
[ "$daa_post" -ge "$daa_dark" ] || die "B's post-rejoin DAA is behind the dark-window tip — convergence claim would be false"
log "PASS: rejoin converged. sink=$sink_post (A) == $sink_post_b (B), DAA $daa_pre -> $daa_dark -> $daa_post"
log "reminder: this run proves dark-window rejoin only; dual-production split-brain needs the two-host window"
