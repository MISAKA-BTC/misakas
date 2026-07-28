#!/usr/bin/env bash
# =============================================================================
# verify-consensus.sh — STN-013: assert that the two closed-testnet nodes A and
#                        B share ONE consensus view, and record the evidence.
#
#   usage:  ./verify-consensus.sh
#
# WHAT THIS ASSERTS (both nodes, over independent RPC — never from code):
#   [1] node_network parity      — A and B report the SAME node_network.
#   [2] selected sink/tip parity — A and B report the SAME non-empty sink.
#   [3] provider registry parity — for providers A, B and auditor C, the
#       palw-status --provider-bond view is in_registry=true, status=active and
#       byte-identical (group/runtime/capacity/reward-root/amount/unbond-delay)
#       on BOTH nodes.
#   [4] batch.status parity      — palw-status --batch-id for PALW_BATCH_ID is
#       status=active AND identical on BOTH nodes (certificate hash etc. too).
#   [5] minted algo-4 block      — IF (and only if) a block was minted, BOTH
#       nodes must agree on the SAME algo-4 block hash and the SAME accept
#       verdict. See the honest note in check_minted_block() for how "minted" is
#       detected without inventing an RPC that the binaries do not expose.
#
# DIVERGENCE = STOP. Any mismatch is fail-closed: this script collects every
# stop condition, writes the full structured report either way, and then exits
# non-zero so no caller can treat a divergent net as verified.
#
# SCOPE:
#   * This compares the two configured RPC endpoints' views. On a SINGLE host it
#     proves the two processes agree; it is NOT a proof of real network-partition
#     survival (that needs two hosts — audit STN-003, stated plainly, not faked).
#   * algo-4 blocks carry fork-choice weight 0 (audit PALW-014): a minted algo-4
#     block does NOT become the sink, so check [5] cannot be derived from the
#     sink parity of check [2]; it is asserted separately from recorded evidence.
#   * This script reads ONLY real on-chain state through the validator RPC. It
#     never invokes the seeded test-only palw_demo path and mints nothing.
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
# (reads discovered state, creates no keys/pids/outpoints; the one file it writes
# — artifacts/verify-consensus.txt — is produced atomically via temp+mv and the
# replacement is LOGGED, never silent); FAIL-CLOSED with actionable messages;
# register_cleanup trap removes a half-written temp report on any early exit. It
# SOURCES common.sh and uses ONLY its helpers — it reimplements none of them.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# Per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-verify-consensus}"; export PALW_LOG_TAG

# -----------------------------------------------------------------------------
# Cleanup trap, armed up front (LIFO). This stage starts no long-lived process;
# the only teardown is removing a half-written temp report if we die/INT/TERM
# before the atomic mv. $REPORT_TMP is expanded at trap time (see _run_cleanup's
# eval); it is set to "" the instant the report is committed, so the committed
# report file is never removed.
# -----------------------------------------------------------------------------
REPORT_TMP=""
register_cleanup 'if [ -n "$REPORT_TMP" ]; then rm -f "$REPORT_TMP"; fi'

# load_env: sources config, realpaths REPO_ROOT/PALW_DATA_ROOT, creates the 0700
# data dirs, overlays state.env, validates required vars, binds+verifies the
# three binaries. Fail-closed and re-runnable.
load_env

# External tools this script uses directly (helpers rely on more, all present).
require_cmd awk mktemp install date

# =============================================================================
# Report + stop-condition bookkeeping.
#   rpt      — append one line to the pending report (temp file).
#   diverge  — record a hard consensus DIVERGENCE (A != B) as a STOP condition.
#   notready — record a state that is not divergent but is not the required
#              consensus outcome either (e.g. batch not yet active on both) —
#              still a STOP (fail-closed): we refuse to claim "verified".
# All three always return 0 so `test || diverge ...` chains never trip set -e.
# =============================================================================
REPORT_FILE="$PALW_DATA_ROOT/artifacts/verify-consensus.txt"
STOP_COUNT=0

rpt()     { printf '%s\n' "$*" >> "$REPORT_TMP"; }
diverge() {
    STOP_COUNT=$(( STOP_COUNT + 1 ))
    warn "STOP (divergence): $*"
    rpt "    >>> DIVERGENCE (STOP): $*"
    return 0
}
notready() {
    STOP_COUNT=$(( STOP_COUNT + 1 ))
    warn "STOP (not-ready): $*"
    rpt "    >>> NOT-READY (STOP): $*"
    return 0
}

# Field extractors over a captured status blob (thin wrappers over common.sh's
# token-boundary-aware parsers; _kv = first single token, _line = whole trailing
# value). Never reimplement the parsers — just feed them the captured text.
_fkv()   { printf '%s\n' "$1" | _kv   "$2"; }
_fline() { printf '%s\n' "$1" | _line "$2"; }

# =============================================================================
# Preconditions — every one fail-closed with an actionable message. These read
# discovered state and probe RPC; they mutate nothing, so an early die leaves the
# running net untouched and writes no partial report.
# =============================================================================

# Both nodes must answer wRPC — without both endpoints there is nothing to
# compare. wait_rpc_up loops to a deadline and returns non-zero on timeout; we
# ALWAYS check the rc (a gate never silently proceeds).
wait_rpc_up a || die "node A wRPC ($(node_wrpc a)) is not answering — start it (./node-a.sh) and let it come up before verifying consensus."
wait_rpc_up b || die "node B wRPC ($(node_wrpc b)) is not answering — start it (./node-b.sh) and let it come up before verifying consensus."

# Provider/auditor bond outpoints (txid:index) — required for registry parity.
# state_get returns "" when unset; we refuse to proceed on a blank or malformed
# outpoint rather than silently skip a provider (that would be a false PASS).
# The `|| true` guards the call site: the provided state_get greps state.env
# inside a pipefail command substitution, which under set -e would abort on a key
# that is missing from an existing state.env. Guarding the CALL suppresses set -e
# throughout the helper body (bash treats the whole call as non-fatal on the left
# of ||), so a missing key returns "" here and our actionable die below fires —
# instead of an abrupt, message-less exit. We never reimplement state_get.
PROV_A_BOND="$(state_get PROV_A_BOND || true)"
PROV_B_BOND="$(state_get PROV_B_BOND || true)"
AUD_C_BOND="$(state_get AUD_C_BOND || true)"
_require_bond() {   # <value> <name>
    [ -n "$1" ] || die "$2 is not set in artifacts/state.env — run the provider registration stage (register-providers.sh) first; it prints the bond outpoint and state_set's $2."
    case "$1" in
        *:*) : ;;
        *)   die "$2 is malformed: '$1' (expected <txid>:<index>) — re-run the provider registration stage." ;;
    esac
}
_require_bond "$PROV_A_BOND" PROV_A_BOND
_require_bond "$PROV_B_BOND" PROV_B_BOND
_require_bond "$AUD_C_BOND"  AUD_C_BOND

# Batch id (128-hex) — required for batch.status parity. Must be present, hex,
# 128 chars, and NOT the all-zero unbound sentinel.
PALW_BATCH_ID="$(state_get PALW_BATCH_ID || true)"
[ -n "$PALW_BATCH_ID" ] || die "PALW_BATCH_ID is not set in artifacts/state.env — run the batch-manifest stage (create-lifecycle.sh) first; it prints batch_id and state_set's PALW_BATCH_ID."
case "$PALW_BATCH_ID" in *[!0-9a-fA-F]*) die "PALW_BATCH_ID is not hex: '$PALW_BATCH_ID'." ;; esac
[ "${#PALW_BATCH_ID}" -eq 128 ] || die "PALW_BATCH_ID must be 128 hex chars (64-byte Hash64), got ${#PALW_BATCH_ID}."
[ "$PALW_BATCH_ID" != "$(zero128)" ] || die "PALW_BATCH_ID is the all-zero sentinel (unbound) — re-run the batch-manifest stage to obtain the content-derived batch id."

# =============================================================================
# Open the report (atomic: build in a temp under artifacts/, mv into place at the
# end). load_env already created artifacts/ 0700; re-assert idempotently.
# =============================================================================
install -d -m 0700 "$(dirname "$REPORT_FILE")" || die "cannot create artifacts dir for $REPORT_FILE"
REPORT_TMP="$(mktemp "${REPORT_FILE}.XXXXXX")" || die "mktemp failed near $REPORT_FILE"

rpt "PALW closed-testnet consensus parity report (STN-013)"
rpt "generated:            $(date '+%Y-%m-%dT%H:%M:%S%z')"
rpt "configured network:   $NETWORK   (base=$NETWORK_BASE suffix=$NETSUFFIX)"
rpt "ticket mode:          $TICKET_MODE"
rpt "algo-4 accept switch: PALW_ENABLE_ALGO4=${PALW_ENABLE_ALGO4:-unset}  (start-time override; NOT introspectable over RPC)"
rpt "node A wRPC:          $(node_wrpc a)"
rpt "node B wRPC:          $(node_wrpc b)"
rpt "batch id:             $PALW_BATCH_ID"
rpt "provider-A bond:      $PROV_A_BOND"
rpt "provider-B bond:      $PROV_B_BOND"
rpt "auditor-C  bond:      $AUD_C_BOND"
rpt ""
rpt "NOTE: this asserts the two configured RPC endpoints share one view. On a"
rpt "single host it proves the two processes agree; it is NOT a network-partition"
rpt "proof (STN-003). algo-4 blocks have fork-choice weight 0 (PALW-014), so the"
rpt "minted-block check [5] is asserted from recorded evidence, not from the sink."
rpt ""

# =============================================================================
# [1] node_network parity.
# =============================================================================
rpt "[1] node_network parity"
_st_a="$(node_status a 2>/dev/null || true)"
_st_b="$(node_status b 2>/dev/null || true)"
_net_a="$(_fkv "$_st_a" node_network)"
_net_b="$(_fkv "$_st_b" node_network)"
rpt "    A: ${_net_a:-<none>}"
rpt "    B: ${_net_b:-<none>}"
if [ -z "$_net_a" ] || [ -z "$_net_b" ]; then
    diverge "node_network unreadable (A='${_net_a:-}' B='${_net_b:-}') — a node is up but not returning node_network."
elif [ "$_net_a" != "$_net_b" ]; then
    diverge "node_network differs between nodes (A='$_net_a' B='$_net_b') — the two nodes are on DIFFERENT networks."
else
    rpt "    result: PASS (identical: $_net_a)"
    # Informational only: node_network's exact string can vary across builds, so
    # a mismatch with the configured NETWORK is a loud WARN, not a STOP (matches
    # preflight.sh's stance).
    [ "$_net_a" = "$NETWORK" ] || warn "both nodes agree on node_network='$_net_a' but the harness is configured NETWORK='$NETWORK' (informational; not a divergence)."
fi
rpt ""

# =============================================================================
# [2] selected sink/tip parity. A single paired read is the snapshot recorded in
#     the report; if it differs we allow brief propagation lag via wait_same_sink
#     (the authoritative gate) before declaring a divergence.
# =============================================================================
rpt "[2] selected sink/tip parity"
_sink_a="$(node_sink a)"
_sink_b="$(node_sink b)"
rpt "    A: ${_sink_a:-<none>}"
rpt "    B: ${_sink_b:-<none>}"
if [ -n "$_sink_a" ] && [ "$_sink_a" = "$_sink_b" ]; then
    rpt "    result: PASS (identical sink)"
elif wait_same_sink; then
    # The initial snapshot differed only because the chain advanced between the
    # two reads; wait_same_sink confirmed A and B do converge on one sink.
    rpt "    result: PASS (initial snapshot differed due to advance; wait_same_sink confirmed convergence)"
else
    diverge "nodes A and B do not share a sink (A='${_sink_a:-}' B='${_sink_b:-}') — selected-chain views have diverged."
fi
rpt ""

# =============================================================================
# [3] provider registry parity (A / B / auditor C).
# =============================================================================
# check_provider <label> <bond> — compare the palw-status provider.* view on A
#   vs B for one bond. in_registry must be true and status active on BOTH, and
#   every identity field must be byte-identical across the two nodes. Any gap is
#   a divergence. Appends its own section to the report; always returns 0.
check_provider() {
    local label="$1" bond="$2" oa ob f va vb
    rpt "    provider-$label  bond=$bond"
    oa="$(palw_provider_status a "$bond" 2>/dev/null || true)"
    ob="$(palw_provider_status b "$bond" 2>/dev/null || true)"

    if [ -z "$oa" ]; then rpt "        A: <no response>"; fi
    if [ -z "$ob" ]; then rpt "        B: <no response>"; fi
    if [ -z "$oa" ] || [ -z "$ob" ]; then
        local where=""
        [ -z "$oa" ] && where="A"
        [ -z "$ob" ] && where="${where:+$where+}B"
        diverge "provider-$label not resolvable on node(s) $where (bond $bond) — palw-status returned nothing; is the provider registered and visible on both nodes?"
        rpt "        result: FAIL (unresolved on a node)"
        return 0
    fi

    # in_registry — must be true on both.
    va="$(_fkv "$oa" in_registry)"; vb="$(_fkv "$ob" in_registry)"
    rpt "        in_registry         A=${va:-<none>}  B=${vb:-<none>}"
    { [ "$va" = "true" ] && [ "$vb" = "true" ]; } || diverge "provider-$label in_registry not true on both nodes (A='${va:-}' B='${vb:-}')."

    # status — must be active on both AND equal.
    va="$(_fkv "$oa" status)"; vb="$(_fkv "$ob" status)"
    rpt "        status              A=${va:-<none>}  B=${vb:-<none>}"
    [ "$va" = "$vb" ] || diverge "provider-$label status differs between nodes (A='${va:-}' B='${vb:-}')."
    [ "$va" = "active" ] || diverge "provider-$label status is not active on node A (='${va:-}')."
    [ "$vb" = "active" ] || diverge "provider-$label status is not active on node B (='${vb:-}')."

    # Identity fields — must be byte-identical across the two nodes. _line keeps
    # multi-token values intact (runtime_classes / capacity_by_shape).
    for f in operator_group_id runtime_classes capacity_by_shape reward_key_root amount_sompi unbond_delay_epochs; do
        va="$(_fline "$oa" "$f")"; vb="$(_fline "$ob" "$f")"
        rpt "        $f  A='${va}'  B='${vb}'"
        [ "$va" = "$vb" ] || diverge "provider-$label $f differs between nodes (A='${va}' B='${vb}')."
    done
    return 0
}

rpt "[3] provider registry parity (A / B / auditor C)"
check_provider A "$PROV_A_BOND"
check_provider B "$PROV_B_BOND"
check_provider C "$AUD_C_BOND"
rpt ""

# =============================================================================
# [4] batch.status parity — must be active AND identical on both nodes.
#     Reminder (invariant 3): palw-status is a past-relative view that excludes
#     the sink's own body, so a child must have been mined after the last carrier
#     for these fields to be current. This stage only READS; it does not mine. If
#     the batch is not yet active on both nodes, that is a NOT-READY stop (run the
#     lifecycle to completion first), not a silent pass.
# =============================================================================
rpt "[4] batch.status parity  (batch_id=$PALW_BATCH_ID)"
_bat_a="$(palw_batch_status a "$PALW_BATCH_ID" 2>/dev/null || true)"
_bat_b="$(palw_batch_status b "$PALW_BATCH_ID" 2>/dev/null || true)"
if [ -z "$_bat_a" ]; then rpt "    A: <no response>"; fi
if [ -z "$_bat_b" ]; then rpt "    B: <no response>"; fi
if [ -z "$_bat_a" ] || [ -z "$_bat_b" ]; then
    diverge "batch $PALW_BATCH_ID not resolvable on a node (A response empty=$( [ -z "$_bat_a" ] && echo yes || echo no), B empty=$( [ -z "$_bat_b" ] && echo yes || echo no)) — is the manifest carrier included and a child mined on both nodes?"
else
    _bst_a="$(_fkv "$_bat_a" status)"; _bst_b="$(_fkv "$_bat_b" status)"
    rpt "    status: A=${_bst_a:-<none>}  B=${_bst_b:-<none>}"
    if [ -z "$_bst_a" ] || [ -z "$_bst_b" ]; then
        diverge "batch.status unreadable (A='${_bst_a:-}' B='${_bst_b:-}')."
    elif [ "$_bst_a" != "$_bst_b" ]; then
        diverge "batch.status differs between nodes (A='$_bst_a' B='$_bst_b')."
    elif [ "$_bst_a" != "active" ]; then
        notready "batch.status is '$_bst_a' on both nodes, not 'active' — the lifecycle is incomplete; run submit-lifecycle.sh to certificate/active before verifying."
    else
        rpt "    result: PASS (active on both nodes)"
    fi

    # Additional batch parity: a differing certificate hash / view for the SAME
    # batch id is a real consensus divergence. Compare and flag mismatches.
    for f in in_sink_view manifest_present chunks leaf_blobs certificate_blob_present certificate_hash; do
        va="$(_fline "$_bat_a" "$f")"; vb="$(_fline "$_bat_b" "$f")"
        rpt "    $f  A='${va}'  B='${vb}'"
        [ "$va" = "$vb" ] || diverge "batch $f differs between nodes (A='${va}' B='${vb}')."
    done
fi
rpt ""

# =============================================================================
# [5] minted algo-4 block parity (CONDITIONAL — "if a block was minted").
#
# HONEST DETECTION (invents no RPC): the binaries expose no "give me the last
# algo-4 block hash / accept verdict" query, and algo-4 has fork-choice weight 0
# (PALW-014) so the block never becomes the sink. The mint stage (STN-012,
# start-palw-miner.sh) is therefore the component that KNOWS a block was minted;
# it is responsible for recording, per node, what each node saw:
#     PALW_ALGO4_BLOCK_HASH_A / PALW_ALGO4_BLOCK_HASH_B   (algo-4 block hash)
#     PALW_ALGO4_ACCEPT_A     / PALW_ALGO4_ACCEPT_B       (accept verdict)
# via state_set into artifacts/state.env. This stage READS them:
#   * none recorded            -> N/A (no block minted). In TICKET_MODE=skip this
#                                 is the EXPECTED outcome (skip reaches batch
#                                 active but can never mint — verified).
#   * recorded asymmetrically  -> STOP (one node saw/accepted a block the other
#                                 did not).
#   * recorded on both         -> the hashes must match AND the verdicts must
#                                 match; any mismatch is a STOP.
# It never fabricates a block hash and never claims a mint that did not happen.
# =============================================================================
check_minted_block() {
    local hA hB vA vB
    # `|| true` for the same reason as the bond/batch reads above: these optional
    # keys are usually absent, and state_get greps an existing state.env under
    # pipefail; guarding the call keeps a missing key as "" instead of aborting.
    hA="$(state_get PALW_ALGO4_BLOCK_HASH_A || true)"
    hB="$(state_get PALW_ALGO4_BLOCK_HASH_B || true)"
    vA="$(state_get PALW_ALGO4_ACCEPT_A || true)"
    vB="$(state_get PALW_ALGO4_ACCEPT_B || true)"

    if [ -z "$hA" ] && [ -z "$hB" ] && [ -z "$vA" ] && [ -z "$vB" ]; then
        if [ "$TICKET_MODE" = "skip" ]; then
            rpt "    result: N/A — TICKET_MODE=skip cannot mint an algo-4 block (verified: reaches batch.status=active but no mintable block). Nothing to compare."
            return 0
        fi
        # Review §7 (P0-3): mock mode with NO mint evidence is a STOP, never a
        # silent N/A pass — mock mode's contract is a hash-pinned mint.
        notready "TICKET_MODE=mock but no algo-4 mint evidence is recorded (PALW_ALGO4_BLOCK_HASH_A/_B unset) — mock mode must produce a hash-pinned mint; re-run ./start-palw-miner.sh and fix its failure before verifying."
        return 0
    fi

    rpt "    block_hash  A=${hA:-<unset>}  B=${hB:-<unset>}"
    rpt "    accept      A=${vA:-<unset>}  B=${vB:-<unset>}"

    # Recorded evidence in skip mode is contradictory (skip cannot mint) — flag it
    # rather than silently trusting a value that should not exist.
    if [ "$TICKET_MODE" = "skip" ]; then
        diverge "algo-4 block state is recorded while TICKET_MODE=skip, which cannot mint — inconsistent/stale state; clear PALW_ALGO4_BLOCK_HASH_*/PALW_ALGO4_ACCEPT_* or re-run under TICKET_MODE=mock."
    fi

    # Block hash parity.
    if [ -z "$hA" ] || [ -z "$hB" ]; then
        diverge "algo-4 block hash recorded on only one node (A='${hA:-}' B='${hB:-}') — asymmetric mint/propagation."
    elif [ "$hA" != "$hB" ]; then
        diverge "algo-4 block hash differs between nodes (A='$hA' B='$hB')."
    else
        rpt "    block_hash: identical ($hA)"
        # Review §7 (P0-4): the recorded hash must be the full 128-hex Hash64 and the
        # block must be FETCHABLE from BOTH nodes' RPC with a matching stable field
        # (coinbase subsidy) — log-derived state alone is not both-node proof.
        if [ "${#hA}" -ne 128 ]; then
            notready "recorded algo-4 hash is ${#hA} hex chars (need the full 128-hex Hash64 for RPC verification) — re-mint with the current start-palw-miner.sh."
        else
            local blobA blobB subA subB
            blobA="$("$VAL" get-block --hash "$hA" --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" 2>/dev/null || true)"
            blobB="$("$VAL" get-block --hash "$hA" --node-wrpc-borsh "$(node_wrpc b)" --network "$NETWORK" 2>/dev/null || true)"
            subA="$(printf '%s\n' "$blobA" | awk -F': ' '/^coinbase_subsidy_sompi: [0-9][0-9]*$/{print $2; exit}')"
            subB="$(printf '%s\n' "$blobB" | awk -F': ' '/^coinbase_subsidy_sompi: [0-9][0-9]*$/{print $2; exit}')"
            if [ -z "$subA" ]; then
                diverge "node A could not serve full block $hA over RPC (get-block returned no parseable coinbase) — the minted block is not retrievable where it was mined."
            fi
            if [ -z "$subB" ]; then
                diverge "node B could not serve full block $hA over RPC (get-block returned no parseable coinbase) — the minted block did not propagate as a retrievable full block."
            fi
            if [ -n "$subA" ] && [ -n "$subB" ]; then
                if [ "$subA" = "$subB" ]; then
                    rpt "    rpc fetch: BOTH nodes serve full block $hA (coinbase subsidy $subA sompi, identical)"
                else
                    diverge "nodes serve DIFFERENT content for block $hA (coinbase subsidy A=$subA B=$subB)."
                fi
            fi
        fi
    fi

    # Accept-verdict parity (required once a block hash is present on both).
    if [ -z "$vA" ] || [ -z "$vB" ]; then
        diverge "algo-4 accept verdict not recorded on both nodes (A='${vA:-}' B='${vB:-}') while a block hash is present — cannot confirm accept parity; the mint stage must state_set PALW_ALGO4_ACCEPT_A/_B."
    elif [ "$vA" != "$vB" ]; then
        diverge "algo-4 accept verdict differs between nodes (A='$vA' B='$vB')."
    else
        rpt "    accept verdict: identical ($vA)"
    fi
    return 0
}

rpt "[5] minted algo-4 block parity (conditional)"
check_minted_block
rpt ""

# =============================================================================
# Verdict — commit the report atomically, then fail closed if any STOP fired.
# =============================================================================
if [ "$STOP_COUNT" -gt 0 ]; then
    rpt "VERDICT: FAIL — $STOP_COUNT stop condition(s). Consensus NOT verified. STOP."
else
    rpt "VERDICT: PASS — 0 stop conditions. Nodes A and B are consensus-consistent."
fi

# Commit: chmod the temp, mv into place, and blank REPORT_TMP so the cleanup trap
# leaves the committed file alone. A prior run's report is replaced here — that
# is the intended, LOGGED behaviour of regenerating this evidence file (not a
# silent clobber of unrelated data).
chmod 0644 "$REPORT_TMP" 2>/dev/null || true
mv "$REPORT_TMP" "$REPORT_FILE" || die "failed to write report to $REPORT_FILE"
REPORT_TMP=""
log "report written -> $REPORT_FILE (replaces any prior run's report)"

if [ "$STOP_COUNT" -gt 0 ]; then
    die "consensus verification FAILED: $STOP_COUNT stop condition(s) — divergence = STOP. See $REPORT_FILE for the per-check evidence."
fi
log "STN-013 consensus verification PASS: node_network + sink + provider registry (A/B/C) + batch.status(active)$( [ "$TICKET_MODE" = mock ] && printf ' + minted-block' ) parity confirmed on both nodes. Report: $REPORT_FILE"
