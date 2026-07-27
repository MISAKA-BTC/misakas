#!/usr/bin/env bash
# negative-tests.sh — STN / G7 failure-and-recovery cases for the closed two-node
# PALW testnet, as a RELEASE GATE (review §9): every case reports PASS / FAIL /
# SKIP, the counts are machine-readable, and a SKIP is never a pass.
#
# PREREQUISITE: a running 2-node net (./run-all.sh has brought up node-a + node-b +
# supporting-miner). These cases do NOT need a mint:
#   restart-a            node A FORCE restart via its host agent (pid must change)
#                        -> re-sync -> same sink   (§9.3 fix: never a no-op)
#   restart-b            node B FORCE restart via its host agent -> re-peer -> same sink
#   partition-reconnect  drop node B (partition proxy) -> A survives -> B rejoins -> same sink
#
# The following cases REQUIRE the mint path (TICKET_MODE=mock + an actually-minted
# algo-4 block, recorded as PALW_ALGO4_BLOCK_HASH_A by start-palw-miner.sh):
#   wrong-authority      palw-submit of the REAL leaf-chunk with a MISMATCHED
#                        --ticket-authority-key must be rejected fail-closed
#                        (the leaf names one authority hash; the key derives another)
#   duplicate-submit     G16 duplicate-work: the FIRST blue-merged algo-4 block for
#                        the batch pays both providers; every LATER blue merge of the
#                        same leaf/job_nullifier must pay NEITHER (withheld, not paid twice)
#   reorg-parity         force a REAL fork (sever the link, mine divergent branches on
#                        A and B, reconverge): the losing tip must be is_chain_block=false
#                        on BOTH nodes and the provider settlement must survive the reorg
# SKIP vs FAIL (audit STN-04 — this is the WHOLE policy, there is no other rule):
#   * EVERY skip is CLASSIFIED justified|unjustified at the point it is recorded
#     (via _skip), printed on the neg.case line, and written to the JSON. A skip
#     can no longer be recorded unclassified.
#   * JUSTIFIED is exactly ONE situation: a mint case with no recorded mint
#     evidence while NEG_REQUIRE_MINT=0 — structurally unreachable for an evidence
#     reason the operator deliberately opted into for this run.
#   * EVERY other skip is UNJUSTIFIED and increments unjustified_skips: missing
#     artifact/tool, RPC failure, unexpected chain state, or duplicate-submit
#     finding fewer than 2 blue merges to compare. Something that should have run
#     did not, and the report has to say so.
#   * NEG_REQUIRE_MINT=1 makes mint evidence MANDATORY: a mint case with no
#     PALW_ALGO4_BLOCK_HASH_A is then a FAIL (not a skip), and every mint case
#     that ran must PASS or the run dies — with NEG_REQUIRE_MINT=0 a "green" run
#     proves NOTHING about the PALW mint path (that was the STN-04 hole).
#
# RESULT CONTRACT (review §9.5):
#   * per-case line:      `neg.case: <name> result=<PASS|FAIL|SKIP> [skip=<justified|unjustified>] [reason=...]`
#   * final summary line: `neg.result: pass=<n> fail=<n> skip=<n> unjustified_skips=<n> require_mint=<0|1>`
#   * JSON report:        $PALW_DATA_ROOT/artifacts/negative-tests.json
#   * exit code:          non-zero iff ANY of
#                           - fail>0
#                           - NEG_REQUIRE_MINT=1 and a mint case that ran did not PASS
#                           - NEG_RELEASE=1 and some case of the list did not run at
#                             all (a single-case invocation is never a release verdict)
#                           - NEG_RELEASE=1 and unjustified_skips>0
#                           - NEG_RELEASE=1 and NEG_REQUIRE_MINT=1 and NOT
#                             (pass>0 and fail=0 and skip=0)
#                         `all` therefore still stays exit-0 on an honest
#                         NEG_REQUIRE_MINT=0 skip-mode run, while the FULL release
#                         gate (NEG_RELEASE=1 NEG_REQUIRE_MINT=1) demands that every
#                         case — mint cases included — actually ran and passed.
#
# usage:  ./negative-tests.sh [ all | <case> | list ]
# env:    NEG_RELEASE=1       release gate (see the exit code above)
#         NEG_REQUIRE_MINT=1  mint evidence mandatory (default 0)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd -P)"
PALW_LOG_TAG="${PALW_LOG_TAG:-neg-tests}"; export PALW_LOG_TAG
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"
# shellcheck source=remote.sh
. "$SCRIPT_DIR/remote.sh"   # node_dispatch — restarts run via each node's HOST agent (§9.3)

CASES="restart-a restart-b partition-reconnect wrong-authority duplicate-submit reorg-parity"
NET_CASES="restart-a restart-b partition-reconnect"          # runnable without a mint
MINT_CASES="wrong-authority duplicate-submit reorg-parity"   # need a reproducible mint

# NEG_REQUIRE_MINT=1 — "this run must actually validate PALW minting": a mint case
# with no recorded algo-4 block becomes a FAIL instead of a skip, and every mint
# case that ran must PASS. Default 0 keeps the honest no-mint run reporting
# justified skips. Validated here so a typo ("true", "yes") can never silently
# disable the gate.
NEG_REQUIRE_MINT="${NEG_REQUIRE_MINT:-0}"
case "$NEG_REQUIRE_MINT" in
    0|1) : ;;
    *) die "NEG_REQUIRE_MINT must be 0 or 1, got '$NEG_REQUIRE_MINT'" ;;
esac
# NEG_RELEASE needs the SAME guard, for the same reason: it is only ever tested as
# `= "1"`, so an unvalidated "true"/"yes"/"1 " would silently DISABLE the release gate
# instead of failing loudly. Normalised once here; use the bare "$NEG_RELEASE" below.
NEG_RELEASE="${NEG_RELEASE:-0}"
case "$NEG_RELEASE" in
    0|1) : ;;
    *) die "NEG_RELEASE must be 0 or 1, got '$NEG_RELEASE' — a fail-closed release gate does not guess" ;;
esac

PASS_COUNT=0; FAIL_COUNT=0; SKIP_COUNT=0; UNJUSTIFIED_SKIPS=0
UNJUSTIFIED_LIST=""   # " <case>" per unjustified skip, for the NO-GO message
RESULTS=""            # space-list of "<case>=<RESULT>" for the JSON report
NEG_TAB="$(printf '\t')"
NEG_NL='
'                     # literal newline (bash 3.2: no $'\n' games needed here)
CASE_DETAILS=""       # one "<case><TAB><RESULT><TAB><skip_class><TAB><reason>" record
                      # per line — the source of the JSON case_details array.

# _record <case> <PASS|FAIL|SKIP> [reason] [skip_class]
#   skip_class is set ONLY by _skip below, so a SKIP can never be recorded without
#   a justified|unjustified classification (audit STN-04: the counter must be
#   wired at the point of the skip, not decorative).
_record() {
    local name="$1" result="$2" reason="${3:-}" cls="${4:-}"
    case "$result" in
        PASS) PASS_COUNT=$((PASS_COUNT + 1)) ;;
        FAIL) FAIL_COUNT=$((FAIL_COUNT + 1)) ;;
        SKIP) SKIP_COUNT=$((SKIP_COUNT + 1))
              [ -n "$cls" ] || die "internal: SKIP recorded for '$name' with no justified|unjustified class — use _skip, never _record, for a skip" ;;
    esac
    RESULTS="$RESULTS $name=$result"
    CASE_DETAILS="$CASE_DETAILS$name$NEG_TAB$result$NEG_TAB$cls$NEG_TAB$reason$NEG_NL"
    if [ -n "$cls" ]; then
        printf 'neg.case: %s result=%s skip=%s reason=%s\n' "$name" "$result" "$cls" "$reason"
    elif [ -n "$reason" ]; then
        printf 'neg.case: %s result=%s reason=%s\n' "$name" "$result" "$reason"
    else
        printf 'neg.case: %s result=%s\n' "$name" "$result"
    fi
}

# _skip <case> <justified|unjustified> <reason>  — record a CLASSIFIED skip.
#   JUSTIFIED = structurally unreachable for an evidence reason the operator
#   cannot control in this mode; today that is exactly one thing, a mint case with
#   no mint evidence while NEG_REQUIRE_MINT=0. ANY other skip is UNJUSTIFIED and
#   counts (missing artifact, RPC failure, unexpected state, "<2 blue merges to
#   compare"): it means something that should have run did not.
_skip() {
    local name="$1" cls="$2" reason="$3"
    case "$cls" in
        justified) : ;;
        unjustified)
            UNJUSTIFIED_SKIPS=$((UNJUSTIFIED_SKIPS + 1))
            UNJUSTIFIED_LIST="$UNJUSTIFIED_LIST $name" ;;
        *) die "internal: _skip class must be justified|unjustified, got '$cls'" ;;
    esac
    _record "$name" SKIP "$reason" "$cls"
}

# _no_mint_evidence <case>  — the one shared decision for "PALW_ALGO4_BLOCK_HASH_A
#   is empty" in a mint-dependent case:
#     NEG_REQUIRE_MINT=1 -> FAIL, return 1. A release that is supposed to validate
#                           PALW minting must not go green with zero mint evidence.
#     NEG_REQUIRE_MINT=0 -> JUSTIFIED skip, return 0. Honest, and the run stays green.
_no_mint_evidence() {
    local name="$1"
    if [ "$NEG_REQUIRE_MINT" = "1" ]; then
        _record "$name" FAIL "mint evidence required (NEG_REQUIRE_MINT=1) but no PALW_ALGO4_BLOCK_HASH_A recorded — mint an algo-4 block (./start-palw-miner.sh) before gating a release on this case"
        return 1
    fi
    _skip "$name" justified "no algo-4 mint evidence recorded (mint path not exercised; set NEG_REQUIRE_MINT=1 to make this a FAIL)"
    return 0
}

# assert_healthy <a|b> — the standard post-perturbation recovery gate for one node.
assert_healthy() {
    local n="$1"
    wait_rpc_up        "$n" || { warn "G7: node-$n RPC did not come back up"; return 1; }
    wait_peer_connected "$n" || { warn "G7: node-$n did not re-establish its P2P peer"; return 1; }
    wait_node_synced   "$n" || { warn "G7: node-$n did not re-sync after the perturbation"; return 1; }
}

# assert_converged — both nodes up, peered, synced, and on the SAME sink.
assert_converged() {
    assert_healthy a || return 1
    assert_healthy b || return 1
    wait_same_sink || { warn "G7: nodes A and B did not converge to the same sink after recovery"; return 1; }
    log "G7: recovered — A and B are up, peered, synced, and on the same sink."
}

# _node_a_agent_mode — the mode to restart node A into, matching its CURRENT role
#   (validator when it runs the in-process validator; bootstrap otherwise). A
#   restart test must restart the SAME role, not silently change the topology.
_node_a_agent_mode() {
    local pid cmd
    if pid="$(read_pid node-a 2>/dev/null)" && [ -n "$pid" ]; then
        cmd="$(_proc_cmd "$pid")"
        case "$cmd" in
            *--palw-mine*)        printf 'miner\n'; return ;;
            *--enable-validator*) printf 'validator\n'; return ;;
        esac
    fi
    printf 'bootstrap\n'
}

t_restart_a() {
    # §9.3 fix: the restart runs through node A's HOST agent with --force, which
    # ASSERTS the pid/start-time changed — an idempotent no-op "restart" is a FAIL
    # inside the agent itself, never a silent pass here.
    local mode bid st
    mode="$(_node_a_agent_mode)"
    if [ "$mode" = miner ]; then
        # A miner relaunch is valid only while the pinned batch is ACTIVE:
        # start-palw-miner.sh fail-closes on a non-active batch, and a batch is
        # single-use — post-expiry the honest continuing role is validator, so
        # restart into THAT rather than failing on a role that no longer exists.
        bid="$(state_get PALW_BATCH_ID || true)"
        st=""
        [ -n "$bid" ] && st="$(palw_batch_status a "$bid" 2>/dev/null | _kv status || true)"
        if [ "$st" != "active" ]; then
            log "G7 restart-a: node A mines batch ${bid:-<none>} whose status is '${st:-gone}' (not active) — restarting into the VALIDATOR role instead (a miner relaunch would fail-close on the expired single-use batch)."
            mode=validator
        fi
    fi
    log "G7 restart-a: FORCE-restarting node A via its host agent (mode=$mode; pid must change)."
    node_dispatch a restart a "$mode" --force || { _record restart-a FAIL "agent force-restart failed or pid unchanged"; return 1; }
    assert_converged || { _record restart-a FAIL "did not reconverge after restart"; return 1; }
    _record restart-a PASS
}

t_restart_b() {
    log "G7 restart-b: FORCE-restarting node B via its host agent (pid must change), assert re-peer + same sink."
    wait_rpc_up a || { _record restart-b FAIL "node A must be up before perturbing B"; return 1; }
    node_dispatch b restart b bootstrap --force || { _record restart-b FAIL "agent force-restart failed or pid unchanged"; return 1; }
    assert_converged || { _record restart-b FAIL "did not reconverge after restart"; return 1; }
    _record restart-b PASS
}

t_partition_reconnect() {
    # Single-host partition proxy: dropping node B severs the only A<->B link, so A
    # is isolated; restarting B forces a fresh handshake + catch-up. A TRUE network
    # partition (both nodes up, link cut) needs host-level firewalling on separate
    # hosts (iptables/pfctl on the P2P port) — documented, not simulated here.
    log "G7 partition-reconnect: sever A<->B (stop B), verify A survives, then rejoin B and re-converge."
    node_dispatch b stop b || { _record partition-reconnect FAIL "node-b stop failed"; return 1; }
    wait_rpc_up a || { _record partition-reconnect FAIL "node A did not survive the partition"; return 1; }
    log "G7 partition: node A survived isolation; reconnecting node B."
    node_dispatch b start b bootstrap || { _record partition-reconnect FAIL "node-b rejoin failed"; return 1; }
    assert_converged || { _record partition-reconnect FAIL "did not reconverge after rejoin"; return 1; }
    _record partition-reconnect PASS "single-host proxy; true link-cut partition needs two hosts + firewall"
}

# ---------------------------------------------------------------------------
# Mint-case helpers.
# ---------------------------------------------------------------------------

# _mint_hash — the recorded algo-4 block hash, or empty. Empty is decided by
#   _no_mint_evidence: justified SKIP when NEG_REQUIRE_MINT=0, FAIL when it is 1.
_mint_hash() { state_get PALW_ALGO4_BLOCK_HASH_A 2>/dev/null || true; }

# _sink_hash <a|b> — that node's current sink block hash (128-hex).
_sink_hash() {
    "$VAL" palw-status --node-wrpc-borsh "$(node_wrpc "${1:?node}")" --network "$NETWORK" 2>/dev/null \
        | awk '/^sink:/{print $2; exit}'
}

# _neg_mine <a|b> <blocks> — mine EXACTLY n blocks on ONE node via a finite
#   misaminer burst (devnet skip-pow ⇒ instant). Mirrors submit-lifecycle's
#   _da_mine but takes the target node, which is what lets reorg-parity grow
#   DIVERGENT branches while the link is severed.
_neg_mine() {
    local n="${1:?node}" blocks="${2:?blocks}" addr pid deadline rc=0
    addr="$(state_get SUPPORTING_ADDR)"
    [ -n "$addr" ] || { warn "G7: SUPPORTING_ADDR not recorded — cannot burst-mine"; return 1; }
    install -d -m 0755 "$PALW_DATA_ROOT/logs" 2>/dev/null || true
    "$MINER" --pool "$(node_grpc "$n")" --network-id "$NETWORK" \
        --wallet "$addr" --worker "${MINER_WORKER:-rig0}" \
        --blocks "$blocks" --min-block-interval-ms 0 \
        >> "$PALW_DATA_ROOT/logs/miner-neg-tests.log" 2>&1 &
    pid=$!
    deadline=$(( $(date +%s) + ${NEG_MINE_TIMEOUT_SECS:-60} ))
    while kill -0 "$pid" 2>/dev/null; do
        if [ "$(date +%s)" -ge "$deadline" ]; then
            kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true
            warn "G7: misaminer --blocks $blocks burst on node-$n timed out — see $PALW_DATA_ROOT/logs/miner-neg-tests.log"
            return 1
        fi
        sleep 1
    done
    wait "$pid" || rc=$?
    [ "$rc" -eq 0 ] || warn "G7: misaminer burst on node-$n exited $rc — see $PALW_DATA_ROOT/logs/miner-neg-tests.log"
    return "$rc"
}

# wrong-authority — the ticket-authority binding must be fail-closed: submitting
# the batch's REAL leaf-chunk with a freshly-generated (wrong) authority key must
# be REJECTED before broadcast, because every leaf names the hash of the one
# authority that may open its ticket commitment.
t_wrong_authority() {
    local hA chunk store wrongkey out rc=0
    hA="$(_mint_hash)"
    if [ -z "$hA" ]; then
        _no_mint_evidence wrong-authority || return 1
        return 0
    fi
    chunk="$PALW_DATA_ROOT/artifacts/lifecycle/chunk-0.borsh"
    store="${TICKET_SECRET_FILE:-$PALW_DATA_ROOT/keys/ticket-secret.json}"
    [ -s "$chunk" ] || { _record wrong-authority FAIL "no leaf-chunk payload at $chunk (lifecycle bundle retired without a successor?)"; return 1; }
    [ -s "$store" ] || { _record wrong-authority FAIL "no ticket-secret store at $store"; return 1; }
    wrongkey="$PALW_DATA_ROOT/artifacts/.neg-wrong-authority.seed"
    rm -f "$wrongkey"
    register_cleanup "rm -f '$wrongkey'"
    "$VAL" keygen --network "$NETWORK_BASE" --out "$wrongkey" >/dev/null 2>&1 \
        || { _record wrong-authority FAIL "could not keygen a throwaway WRONG ticket-authority key"; return 1; }
    log "G7 wrong-authority: palw-submit of the real leaf-chunk with a MISMATCHED --ticket-authority-key — must be rejected fail-closed."
    out="$("$VAL" palw-submit --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" \
            --validator-key "$(state_get DNS_SEED)" --kind leaf-chunk --payload-file "$chunk" \
            --ticket-authority-key "$wrongkey" --ticket-secret-file "$store" 2>&1)" && rc=0 || rc=$?
    rm -f "$wrongkey"
    if [ "$rc" -eq 0 ]; then
        _record wrong-authority FAIL "palw-submit ACCEPTED a leaf-chunk whose leaves name a different ticket authority"
        return 1
    fi
    # TWO layers can legitimately reject, both of them THE authority binding:
    # the secret-store ownership check ("belongs to a different ticket authority",
    # store is bound to the authority hash it was created for) fires first; the
    # per-leaf check ("names ticket authority X, but ... derives Y") fires when the
    # store itself matches the wrong key. Either one is the fail-closed behaviour
    # this case exists to prove.
    case "$out" in
        *"names ticket authority"*)
            _record wrong-authority PASS "rejected fail-closed before broadcast: leaf authority-hash != --ticket-authority-key derivation"
            ;;
        *"belongs to a different ticket authority"*)
            _record wrong-authority PASS "rejected fail-closed before broadcast: ticket-secret store ownership != --ticket-authority-key derivation"
            ;;
        *)
            _record wrong-authority FAIL "rejected, but NOT by the authority binding — unexpected error: $(printf '%s' "$out" | tail -n1)"
            return 1
            ;;
    esac
}

# duplicate-submit — G16 duplicate-work at the CONSENSUS coordinate: the miner
# produced many algo-4 blocks for the SAME single-use leaf (same job_nullifier);
# only the FIRST blue-merged one may pay the providers. Assert the recorded mint
# settles PASS (exact SPKs) and a LATER blue merge of the same batch paid neither
# provider script. This uses real chain data — no synthetic re-submission.
t_duplicate_submit() {
    local hA spk_a spk_b first bid cand out cls cbid dup="" dup_out="" scanned=0
    hA="$(_mint_hash)"
    if [ -z "$hA" ]; then
        _no_mint_evidence duplicate-submit || return 1
        return 0
    fi
    spk_a="$(state_get PROV_A_REWARD_SPK)"; spk_b="$(state_get PROV_B_REWARD_SPK)"
    if [ -z "$spk_a" ] || [ -z "$spk_b" ]; then
        _record duplicate-submit FAIL "PROV_{A,B}_REWARD_SPK not recorded — run ./register-providers.sh first"
        return 1
    fi
    first="$("$VAL" find-reward-settlement --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" \
            --source-block "$hA" --provider-a-spk "$spk_a" --provider-b-spk "$spk_b" 2>&1)" \
        || { _record duplicate-submit FAIL "recorded mint ${hA:0:16} no longer settles PASS: $(printf '%s' "$first" | tail -n1)"; return 1; }
    bid="$(printf '%s\n' "$first" | _kv settlement.source_batch_id)"
    # Scan the miner's own log lines (current + rotated — node A's log rotates on
    # every relaunch) newest-ish first for ANOTHER blue-merged block of the SAME
    # batch. Old-batch candidates are filtered by source_batch_id, so the cap only
    # bounds RPC walks, not correctness.
    for cand in $(grep -hoE 'mined \+ submitted algo-4 block [0-9a-f]{128}' "$(node_log a)"* 2>/dev/null \
                    | awk '{print $NF}' | awk '{a[NR]=$0} END{for(i=NR;i>=1;i--)print a[i]}'); do
        [ "$cand" = "$hA" ] && continue
        scanned=$((scanned + 1)); [ "$scanned" -gt "${NEG_DUP_SCAN_MAX:-30}" ] && break
        # NOTE: without --provider-*-spk the tool exits 3 (PARTIAL) BY DESIGN even on
        # a successful walk, so the exit code must not gate the scan — only an empty
        # output (RPC failure / unknown block) skips the candidate.
        out="$("$VAL" find-reward-settlement --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" \
                --source-block "$cand" 2>/dev/null)" || true
        [ -n "$out" ] || continue
        cls="$(printf '%s\n' "$out" | _kv settlement.classification)"
        cbid="$(printf '%s\n' "$out" | _kv settlement.source_batch_id)"
        if [ "$cls" = blue ] && [ "$cbid" = "$bid" ]; then dup="$cand"; dup_out="$out"; break; fi
    done
    if [ -z "$dup" ]; then
        # UNJUSTIFIED: the mint evidence IS here, so this case was supposed to run —
        # we simply found nothing to compare it against. The G16 duplicate-work
        # property is therefore UNPROVEN by this run, which is not a pass.
        _skip duplicate-submit unjustified "no SECOND blue-merged algo-4 block of batch ${bid:0:8} found within the newest ${NEG_DUP_SCAN_MAX:-30} minted blocks — the duplicate-work assertion needs >= 2, so it did NOT run (mine another algo-4 block for the batch, or raise NEG_DUP_SCAN_MAX)"
        return 0
    fi
    # POSITIVE EVIDENCE FIRST: "paid NEITHER" is only meaningful if the coinbase list
    # was actually enumerated. The tool prints the count immediately before the output
    # lines; without it, an empty/short walk would masquerade as a clean PASS.
    _nout="$(printf '%s\n' "$dup_out" | _kv settlement.merging_coinbase_outputs)"
    case "$_nout" in
        ''|*[!0-9]*)
            _skip duplicate-submit unjustified "the settlement walk for ${dup:0:16} printed no settlement.merging_coinbase_outputs count — the payout list was never enumerated, so 'paid NEITHER' is UNPROVEN"
            return 0 ;;
    esac
    # MATERIALISE, never gate on a pipeline: `... | grep -qE` returns 141 (SIGPIPE)
    # precisely WHEN it matches, and `set -o pipefail` propagates that, so the `if`
    # would be FALSE exactly when a duplicate payout WAS found — the G16 violation
    # would be reported as a PASS. Also match case-insensitively: the Rust side
    # compares SPKs with eq_ignore_ascii_case, this ERE would not.
    _paid="$(printf '%s\n' "$dup_out" | grep -E '^settlement\.output_[0-9]+: ' | grep -iE "spk=($spk_a|$spk_b)" || true)"
    if [ -n "$_paid" ]; then
        _record duplicate-submit FAIL "G16 VIOLATED: duplicate blue block ${dup:0:16} PAID a provider again for the same job_nullifier"
        return 1
    fi
    _record duplicate-submit PASS "first merge ${hA:0:16} paid both providers (exact SPKs); duplicate blue merge ${dup:0:16} of the same batch/leaf enumerated $_nout merging coinbase output(s) and paid NEITHER (G16 duplicate-work withheld)"
}

# reorg-parity — force a REAL fork and prove both nodes converge AND the paid
# settlement survives: freeze the continuous producer, sever the link (stop B),
# mine +2 on A, swap the survivor (stop A, start B isolated), mine +4 on B, bring
# A back, and assert (a) one sink on both nodes, (b) A's divergent tip is
# is_chain_block=FALSE on BOTH (the reorg displaced it) while B's tip is TRUE,
# (c) find-reward-settlement still PASSes with exact SPKs on BOTH nodes.
_reorg_restore() {
    # Best-effort steady-state restore (A validator, B up, miner running); each
    # step is idempotent and failure is WARNED, never silent.
    is_running node-a || NODE_A_MODE=validator bash "$SCRIPT_DIR/node-a.sh" >/dev/null 2>&1 \
        || warn "G7 reorg: could not restart node A — run: NODE_A_MODE=validator ./node-a.sh"
    is_running node-b || bash "$SCRIPT_DIR/node-b.sh" start >/dev/null 2>&1 \
        || warn "G7 reorg: could not restart node B — run: ./node-b.sh start"
    bash "$SCRIPT_DIR/supporting-miner.sh" start >/dev/null 2>&1 \
        || warn "G7 reorg: could not restart the supporting miner — run: ./supporting-miner.sh start"
}
_reorg_fail() { _reorg_restore; _record reorg-parity FAIL "$1"; }

t_reorg_parity() {
    local hA spk_a spk_b n s0 tA tB icb
    hA="$(_mint_hash)"
    if [ -z "$hA" ]; then
        _no_mint_evidence reorg-parity || return 1
        return 0
    fi
    spk_a="$(state_get PROV_A_REWARD_SPK)"; spk_b="$(state_get PROV_B_REWARD_SPK)"
    if [ -z "$spk_a" ] || [ -z "$spk_b" ]; then
        _record reorg-parity FAIL "PROV_{A,B}_REWARD_SPK not recorded — run ./register-providers.sh first"
        return 1
    fi
    for n in a b; do
        "$VAL" find-reward-settlement --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" \
            --source-block "$hA" --provider-a-spk "$spk_a" --provider-b-spk "$spk_b" >/dev/null 2>&1 \
            || { _record reorg-parity FAIL "baseline: settlement of ${hA:0:16} is not PASS on node-$n before the fork"; return 1; }
    done
    log "G7 reorg-parity: severing the net and mining DIVERGENT branches (A+2 vs B+4), then reconverging."
    bash "$SCRIPT_DIR/supporting-miner.sh" stop >/dev/null 2>&1 || true   # freeze: our bursts must be the ONLY producers
    node_dispatch b stop b || { _reorg_fail "could not stop node B to sever the link"; return 1; }
    s0="$(_sink_hash a)"
    _neg_mine a 2 || { _reorg_fail "burst-mine of A's divergent branch failed"; return 1; }
    tA="$(_sink_hash a)"
    { [ -n "$tA" ] && [ "$tA" != "$s0" ]; } || { _reorg_fail "node A's sink did not advance for its divergent branch"; return 1; }
    node_dispatch a stop a || { _reorg_fail "could not stop node A to swap the mining side"; return 1; }
    # Isolated B start: the peer gate is EXPECTED to fail (A is down) — kaspad
    # itself survives it and keeps re-dialing A's endpoint, which is what makes
    # the later reconvergence automatic.
    NODE_B_ALLOW_ISOLATED_START=1 bash "$SCRIPT_DIR/node-b.sh" start >/dev/null 2>&1 || true
    wait_rpc_up b || { _reorg_fail "node B did not come up isolated (see $(node_log b))"; return 1; }
    _neg_mine b 4 || { _reorg_fail "burst-mine of B's divergent branch failed"; return 1; }
    tB="$(_sink_hash b)"
    { [ -n "$tB" ] && [ "$tB" != "$tA" ]; } || { _reorg_fail "no divergent B branch was created (tB='$tB')"; return 1; }
    log "G7 reorg-parity: fork built — A-tip ${tA:0:16}(+2) vs B-tip ${tB:0:16}(+4); bringing node A back (validator role) and resuming production to force the resolution."
    NODE_A_MODE=validator bash "$SCRIPT_DIR/node-a.sh" || { _reorg_fail "node A validator restart failed"; return 1; }
    # Convergence in this topology is DRIVEN BY PRODUCTION: two already-"synced"
    # peers do not re-announce old blocks on connect, so with no producer the
    # branches sit unexchanged until a NEW block's parent chain pulls them across
    # (observed: >3 min stall). Restart the supporting miner BEFORE the wait.
    bash "$SCRIPT_DIR/supporting-miner.sh" start >/dev/null 2>&1 \
        || { _reorg_fail "could not resume the supporting miner to drive convergence"; return 1; }
    assert_converged || { _reorg_fail "nodes did not reconverge to one sink after the fork"; return 1; }
    # WINNER-AGNOSTIC reorg proof. In a GHOSTDAG world the losing branch is not
    # discarded, it is MERGED — its tip simply stops being a selected-chain block.
    # Which side wins depends on where production resumes (here the miner mines on
    # node A, so A's branch usually outgrows B's +4 and it is NODE B that reorgs
    # 4 blocks deep), so asserting a fixed winner would test the topology, not the
    # reorg. The parity property is: BOTH nodes give IDENTICAL verdicts for both
    # divergent tips, and EXACTLY ONE tip is on the selected chain — i.e. one
    # branch was really displaced, identically everywhere.
    local a_tA a_tB b_tA b_tB
    a_tA="$("$VAL" get-block --hash "$tA" --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" 2>/dev/null | _kv block_is_chain_block)"
    a_tB="$("$VAL" get-block --hash "$tB" --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" 2>/dev/null | _kv block_is_chain_block)"
    b_tA="$("$VAL" get-block --hash "$tA" --node-wrpc-borsh "$(node_wrpc b)" --network "$NETWORK" 2>/dev/null | _kv block_is_chain_block)"
    b_tB="$("$VAL" get-block --hash "$tB" --node-wrpc-borsh "$(node_wrpc b)" --network "$NETWORK" 2>/dev/null | _kv block_is_chain_block)"
    { [ "$a_tA" = "$b_tA" ] && [ "$a_tB" = "$b_tB" ]; } \
        || { _reorg_fail "chain-membership verdicts DIVERGE across nodes: A says tA=$a_tA/tB=$a_tB but B says tA=$b_tA/tB=$b_tB — no parity"; return 1; }
    case "$a_tA/$a_tB" in
        true/false|false/true) : ;;   # exactly one branch survived as chain — a real, agreed displacement
        *) { _reorg_fail "expected exactly one divergent tip on the selected chain, got tA=$a_tA tB=$a_tB on both nodes"; return 1; } ;;
    esac
    for n in a b; do
        "$VAL" find-reward-settlement --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" \
            --source-block "$hA" --provider-a-spk "$spk_a" --provider-b-spk "$spk_b" >/dev/null 2>&1 \
            || { _reorg_fail "provider settlement of ${hA:0:16} is no longer PASS on node-$n after the reorg"; return 1; }
    done
    _reorg_restore
    _record reorg-parity PASS "severed; A(+2) vs B(+4) diverged; reconverged to one sink — tips agree on BOTH nodes (tA=$a_tA, tB=$a_tB; exactly one on-chain, the other branch displaced); provider settlement intact on both. Node A restored as validator (its previous miner pin was for a retired batch)."
}

run_case() {
    local rc=0
    case "$1" in
        restart-a)           t_restart_a || rc=1 ;;
        restart-b)           t_restart_b || rc=1 ;;
        partition-reconnect) t_partition_reconnect || rc=1 ;;
        wrong-authority)     t_wrong_authority || rc=1 ;;
        duplicate-submit)    t_duplicate_submit || rc=1 ;;
        reorg-parity)        t_reorg_parity || rc=1 ;;
        *) die "unknown case '$1' — one of: $CASES (or 'all' / 'list')" ;;
    esac
    return "$rc"
}

# _result_of <case>  — the recorded result for a case that RAN, or empty if this
#   invocation never ran it (single-case runs record exactly one case).
_result_of() {
    local pair
    for pair in $RESULTS; do
        [ "${pair%%=*}" = "$1" ] && { printf '%s\n' "${pair#*=}"; return 0; }
    done
    return 0
}
# _nonpass_of <case>...  — echo " <case>(<RESULT>)" for every named case that ran
#   and did NOT pass. Empty output == all of them green. This is what makes a
#   NO-GO message name its blockers instead of just counting them.
_nonpass_of() {
    local c r out=""
    for c in "$@"; do
        r="$(_result_of "$c")"
        [ -n "$r" ] || continue
        [ "$r" = PASS ] || out="$out $c($r)"
    done
    printf '%s' "$out"
}
# _notrun_of <case>...  — echo " <case>" for every named case this invocation never
#   ran. A release verdict has to cover the FULL case list, otherwise
#   `NEG_RELEASE=1 ./negative-tests.sh restart-a` would report green having tested
#   one restart and nothing else.
_notrun_of() {
    local c out=""
    for c in "$@"; do
        [ -n "$(_result_of "$c")" ] || out="$out $c"
    done
    printf '%s' "$out"
}
# _json_str <text>  — minimal JSON string-body escaper (backslash, double quote,
#   control chars -> space). Reasons can embed a tool's stderr tail, so they are
#   never safe to splice raw. sed only — no new dependency for the report path.
_json_str() {
    printf '%s' "${1:-}" | LC_ALL=C sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/[[:cntrl:]]/ /g'
}

# _finish — emit the machine-readable summary + JSON report and pick the exit code.
_finish() {
    printf 'neg.result: pass=%s fail=%s skip=%s unjustified_skips=%s require_mint=%s\n' \
        "$PASS_COUNT" "$FAIL_COUNT" "$SKIP_COUNT" "$UNJUSTIFIED_SKIPS" "$NEG_REQUIRE_MINT"
    # JSON report (review §9.6) — written best-effort next to the other artifacts.
    # The schema string is UNCHANGED on purpose: every pre-existing field keeps its
    # meaning and "cases" is still the flat <case>:<RESULT> map. The STN-04 facts
    # are additive — require_mint, unjustified_skips, and a case_details array
    # carrying each case's reason + skip classification.
    local json="$PALW_DATA_ROOT/artifacts/negative-tests.json" first=1 pair name result cls reason
    {
        printf '{"schema":"palw-negative-tests-v1","pass":%s,"fail":%s,"skip":%s,"unjustified_skips":%s,"release_mode":%s,"require_mint":%s,"cases":{' \
            "$PASS_COUNT" "$FAIL_COUNT" "$SKIP_COUNT" "$UNJUSTIFIED_SKIPS" \
            "$( [ "$NEG_RELEASE" = "1" ] && printf true || printf false )" \
            "$( [ "$NEG_REQUIRE_MINT" = "1" ] && printf true || printf false )"
        for pair in $RESULTS; do
            name="${pair%%=*}"; result="${pair#*=}"
            [ "$first" = "1" ] || printf ','
            first=0
            printf '"%s":"%s"' "$name" "$result"
        done
        printf '},"case_details":['
        # skip_class is "justified"/"unjustified" on a SKIP row and "" otherwise.
        first=1
        printf '%s' "$CASE_DETAILS" | while IFS="$NEG_TAB" read -r name result cls reason; do
            [ "$first" = "1" ] || printf ','
            first=0
            printf '{"case":"%s","result":"%s","skip_class":"%s","reason":"%s"}' \
                "$(_json_str "$name")" "$(_json_str "$result")" "$(_json_str "$cls")" "$(_json_str "$reason")"
        done
        printf ']}\n'
    } > "$json" 2>/dev/null || warn "could not write $json"
    log "G7 report -> $json"

    # ---- verdict (audit STN-04) ------------------------------------------
    # Ordered most-specific first so the fatal message names the real blocker.
    local mint_blockers="" all_blockers not_run
    if [ "$NEG_REQUIRE_MINT" = "1" ]; then mint_blockers="$(_nonpass_of $MINT_CASES)"; fi
    all_blockers="$(_nonpass_of $CASES)"
    not_run="$(_notrun_of $CASES)"

    # NEG_REQUIRE_MINT=1 means the run must carry REAL mint evidence: a mint case
    # that skipped proves nothing, so it is fatal here even outside release mode.
    if [ -n "$mint_blockers" ]; then
        die "G7: NEG_REQUIRE_MINT=1 but mint case(s)$mint_blockers did not PASS — this run carries NO usable mint evidence. NO-GO. See the neg.case lines above."
    fi
    # Release gate (review §9.5, rewired by STN-04). With NEG_REQUIRE_MINT=1 the bar
    # is the full one — pass>0, fail=0, skip=0 — because a skip of ANY flavour is a
    # case that did not run, and a case that did not run is not evidence. Without it,
    # release mode still fail-closes on every UNJUSTIFIED skip (the only justified
    # skip being "mint case, no mint evidence, NEG_REQUIRE_MINT=0").
    if [ "$NEG_RELEASE" = "1" ]; then
        # A release verdict covers the FULL case list — a single-case invocation
        # cannot be one, however green its one case looks.
        [ -z "$not_run" ] \
            || die "G7 release gate (NEG_RELEASE=1): case(s)$not_run did not run in this invocation — a release verdict needs the whole list, run './negative-tests.sh all'. NO-GO."
        if [ "$NEG_REQUIRE_MINT" = "1" ]; then
            [ "$PASS_COUNT" -gt 0 ] \
                || die "G7 release gate (NEG_RELEASE=1 NEG_REQUIRE_MINT=1): pass=0 — no case verified anything. NO-GO."
            [ -z "$all_blockers" ] \
                || die "G7 release gate (NEG_RELEASE=1 NEG_REQUIRE_MINT=1) requires pass>0 fail=0 skip=0; got pass=$PASS_COUNT fail=$FAIL_COUNT skip=$SKIP_COUNT — NO-GO. Blocking case(s):$all_blockers"
        elif [ "$UNJUSTIFIED_SKIPS" -gt 0 ]; then
            die "G7 release gate: $UNJUSTIFIED_SKIPS unjustified skip(s)$UNJUSTIFIED_LIST — NO-GO. (Add NEG_REQUIRE_MINT=1 for the full gate: mint evidence mandatory, no skips at all.)"
        fi
    fi
    if [ "$FAIL_COUNT" -gt 0 ]; then
        die "G7: $FAIL_COUNT case(s) FAILED — see the neg.case lines above."
    fi
    if [ "$SKIP_COUNT" -gt 0 ]; then
        log "G7: complete — pass=$PASS_COUNT skip=$SKIP_COUNT (unjustified=$UNJUSTIFIED_SKIPS); every skip is classified on its neg.case line, and a skip is NOT a pass."
    else
        log "G7: complete — all $PASS_COUNT case(s) passed, no skips."
    fi
    if [ "$NEG_REQUIRE_MINT" != "1" ]; then
        warn "mint evidence was OPTIONAL this run (NEG_REQUIRE_MINT=0) — a green result here does NOT mean the PALW mint path was validated. Use NEG_RELEASE=1 NEG_REQUIRE_MINT=1 for a release verdict."
    fi
    exit 0
}

ACTION="${1:-all}"
case "$ACTION" in
    -h|--help|help) printf 'usage: ./negative-tests.sh [ all | <case> | list ]\ncases: %s\nenv:\n  NEG_RELEASE=1       release gate: unjustified skips are fatal\n  NEG_REQUIRE_MINT=1  mint evidence mandatory: no mint = FAIL, every mint case must PASS\n  both=1              FULL gate: pass>0, fail=0, skip=0 (a skip is not evidence)\n' "$CASES"; exit 0 ;;
    list|--list)    printf 'net-runnable (no mint): %s\nmint-required (no mint evidence: justified SKIP if NEG_REQUIRE_MINT=0, FAIL if 1): %s\n' "$NET_CASES" "$MINT_CASES"; exit 0 ;;
    all)
        load_env
        log "G7: running failure/recovery cases against the running 2-node net (release_mode=$NEG_RELEASE, require_mint=$NEG_REQUIRE_MINT)."
        RC_ANY=0
        for c in $NET_CASES $MINT_CASES; do run_case "$c" || RC_ANY=1; done
        _finish
        ;;
    *)
        load_env
        run_case "$ACTION" || true
        _finish
        ;;
esac
