#!/usr/bin/env bash
# =============================================================================
# verify-coinbase.sh — STN-013 / T15: assert the algo-4 PALW coinbase split.
#
#   usage:  ./verify-coinbase.sh
#
# WHAT THIS ASSERTS (only IF an algo-4 block was actually minted):
#   Given the minted block's subsidy S (sompi) and the leaf's replica premium π,
#   re-derive the four PALW payout shares from consensus and assert the observed
#   coinbase outputs match:
#     * provider A payout   (leaf provider-A one-time reward SPK)   — EXACT
#     * provider B payout   (leaf provider-B one-time reward SPK)   — EXACT
#     * §D inclusion pool   (worker-inclusion bounty to the includer)
#     * §E validator pool   (validator participation payouts)
#   and that a HALTED / DUPLICATE-WORK / UNBACKED-COLLATERAL source is
#   do-not-mint (paid NOTHING on every axis).
#
# IF NO block was minted (the normal TICKET_MODE=skip end state — skip reaches
# batch.status=active but can NEVER mint), this prints a clear N/A line and
# exits 0. It fabricates no block and claims no mint that did not happen.
#
# --------------------------------------------------------------------------- #
# HOW THE EXPECTED SPLIT DERIVES FROM WorkRewardClass::ReplicaPalw
# (consensus/src/processes/coinbase.rs — the blues loop's ReplicaPalw arm,
#  ADR-0039 §17.2/§17.3 + ADR-0040 §16′). Nothing here is invented; every
# number traces to a named consensus constant/function:
#
#   Let S = the algo-4 block subsidy (sompi) at the block's DAA score.
#   The PALW fee-split "lane" (FeeSplitParams::palw_lane, consensus/core/src/
#   dns_finality.rs) reweights the subsidy with these bps
#   (consensus/core/src/palw.rs):
#       PALW_INCLUSION_BPS      =  800   (§D worker-inclusion pool  =  8 %)
#       PALW_VALIDATOR_BPS      = 1500   (§E validator pool         = 15 %)
#       PALW_PROVIDER_BASE_BPS  = 7700   (provider pair base        = 77 %)
#   (7700 + 800 + 1500 = 10000; the PALW-lane Service share is 0.)
#
#   split_block_subsidy (dns_finality.rs) pays each NON-PRIMARY carve by
#   floor(S·bps/10000) and hands the FLOOR REMAINDER to the primary
#   (worker_base) — so, byte-for-byte as consensus computes it:
#       inclusion_pool = floor(S · 800  / 10000)
#       validator_pool = floor(S · 1500 / 10000)
#       service        = 0
#       worker_base    = S − inclusion_pool − validator_pool − service
#   (worker_base is the primary and absorbs the flooring residue; it is NOT
#    simply floor(S·7700/10000) for an S that does not divide evenly.)
#
#   The 77 % worker_base is then split between the two providers by the leaf's
#   replica premium π (premium_split, consensus/core/src/palw_premium.rs; the
#   replica_count m = 1 for a v1 leaf — A + B):
#       denom      = PALW_PREMIUM_BPS_ONE + π_bps          (10000 + π_bps)
#       provider_a = floor(worker_base · 10000 / denom)
#       provider_b = worker_base − provider_a
#   At the NEUTRAL premium (π_bps = 10000) this is provider_a = floor(base/2),
#   provider_b = base − provider_a — an equal A/B split, byte-identical to the
#   pre-premium fixed-half rule.
#
#   Coinbase TransactionOutputs the arm actually pushes (payout targets):
#     * provider_a  → provider_a_reward_script  (leaf provider-A SPK)   EXACT
#     * provider_b  → provider_b_reward_script  (leaf provider-B SPK)   EXACT
#     * inclusion_pool → §D bounty to the INCLUDER (this block's miner), paid
#         stake-proportionally against the epoch's expected stake, 1.0× urgency;
#         the actual output is ≤ pool and the unspent remainder is BURNED by
#         don't-mint. So we assert 0 ≤ observed ≤ pool, NOT equality.
#     * validator_pool → §E validator_reward_outputs; their sum is ≤ pool, and
#         with NO bonded validator the whole carve is burned (0 outputs) — a
#         deliberate bootstrap-period supply reduction, NOT an error.
#
#   DO-NOT-MINT source classes (empty consensus arms — paid nothing anywhere):
#     * ReplicaPalwHalted             (K5,  ADR-0039 §11.3   — halted beacon)
#     * ReplicaPalwDuplicateWork      (G16, ADR-0040 §5.15.13 — duplicate work)
#     * ReplicaPalwUnbackedCollateral (ECON-03 "THE WIRE"    — bonds unresolved)
#   For these, verify asserts all four axes == 0; the unminted base/validator
#   carve is burned, never rerouted to the includer.
# --------------------------------------------------------------------------- #
#
# HONEST SCOPE (read before trusting a PASS):
#   * The binaries expose NO "give me the last algo-4 block's coinbase" query,
#     and an algo-4 block has fork-choice weight 0 (PALW-014) so it never becomes
#     the sink — it cannot be read back off the tip either. The mint stage
#     (STN-012, start-palw-miner.sh, reachable ONLY with TICKET_MODE=mock) is the
#     component that KNOWS a block was minted and observed its coinbase; it is
#     responsible for recording the block's subsidy, premium and the four
#     observed payouts into artifacts/state.env via state_set (see the slot
#     names below). This stage READS those slots and re-derives the split
#     independently — it never invents an RPC and never mints.
#   * This asserts the split ARITHMETIC against a recorded coinbase. It is not a
#     substitute for the consensus unit tests; it is the harness-level evidence
#     that a real minted block paid exactly what ReplicaPalw prescribes.
#   * It never invokes any seeded/test-only path and mints nothing.
#
# state.env slots the mint stage state_set's once a block is minted (READ-only
# here; unset ⇒ "no block minted" ⇒ N/A):
#     PALW_ALGO4_BLOCK_HASH_A        algo-4 block hash node A observed (mint marker)
#     PALW_ALGO4_SUBSIDY_SOMPI       S — the block subsidy in sompi
#     PALW_ALGO4_PREMIUM_PI_BPS      π_bps (optional; default 10000 = neutral)
#     PALW_ALGO4_SOURCE_CLASS        replica_palw|halted|duplicate|unbacked
#                                    (optional; default replica_palw)
#     PALW_ALGO4_CB_PROVIDER_A_SOMPI observed coinbase output → provider A
#     PALW_ALGO4_CB_PROVIDER_B_SOMPI observed coinbase output → provider B
#     PALW_ALGO4_CB_INCLUSION_SOMPI  observed §D inclusion bounty output (≤ pool)
#     PALW_ALGO4_CB_VALIDATOR_SOMPI  observed §E validator outputs sum  (≤ pool)
#     PALW_ALGO4_CB_PROVIDER_A_SPK   observed provider-A output SPK (optional)
#     PALW_ALGO4_CB_PROVIDER_B_SPK   observed provider-B output SPK (optional)
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
# (reads discovered state; creates no keys/pids/outpoints; the one file it writes
# — artifacts/verify-coinbase.txt — is produced atomically via temp+mv and the
# replacement is LOGGED, never a silent clobber); FAIL-CLOSED with actionable
# messages; a register_cleanup trap removes a half-written temp report on any
# early exit. It SOURCES common.sh and uses ONLY its helpers — reimplements none.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# Per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-verify-coinbase}"; export PALW_LOG_TAG

# Accept only --help; this stage takes no positional action.
case "${1:-}" in
    -h|--help|help)
        cat >&2 <<EOF
usage: ${0##*/}

  Assert the algo-4 PALW coinbase split (Provider A / Provider B / §D inclusion
  pool / §E validator pool, in sompi) against the split re-derived from
  WorkRewardClass::ReplicaPalw. Reads the minted block's facts recorded in
  artifacts/state.env by the mint stage (start-palw-miner.sh, TICKET_MODE=mock).
  If no algo-4 block was minted (the TICKET_MODE=skip end state), prints
  "coinbase assertion N/A" and exits 0. Writes artifacts/verify-coinbase.txt.
EOF
        exit 0
        ;;
    "") : ;;
    *)  die "unexpected argument '$1' (this stage takes no arguments; see --help)." ;;
esac

# -----------------------------------------------------------------------------
# Cleanup trap, armed up front (LIFO). This stage starts no long-lived process;
# the only teardown is removing a half-written temp report if we die/INT/TERM
# before the atomic mv. $REPORT_TMP is expanded at trap time; it is blanked the
# instant the report is committed, so the committed file is never removed.
# -----------------------------------------------------------------------------
REPORT_TMP=""
register_cleanup 'if [ -n "$REPORT_TMP" ]; then rm -f "$REPORT_TMP"; fi'

# load_env: sources config, realpaths REPO_ROOT/PALW_DATA_ROOT, creates the 0700
# data dirs, overlays state.env, validates required vars, binds the binaries.
load_env

# External tools this script uses directly.
require_cmd awk mktemp install date

# =============================================================================
# Consensus constants used by the derivation (documentary — the code below
# recomputes the split exactly as split_block_subsidy + premium_split do). Kept
# as named locals so a future consensus change is a one-line, reviewable edit.
# =============================================================================
PALW_INCLUSION_BPS=800          # consensus/core/src/palw.rs
PALW_VALIDATOR_BPS=1500         # consensus/core/src/palw.rs
PALW_SERVICE_BPS=0              # PALW-lane Service share (7700+800+1500 = 10000)
PALW_PREMIUM_BPS_ONE=10000      # consensus/core/src/palw_premium.rs (π neutral)

# =============================================================================
# Report + assertion bookkeeping.
#   rpt         — append one line to the pending report (temp file).
#   fail        — record one FAIL as a STOP condition (fail-closed) and log it.
#   assert_eq   — <label> <observed> <expected>: PASS iff equal, else fail.
#   assert_rng  — <label> <observed> <max>: PASS iff 0 <= observed <= max.
# assert_* always return 0 so `set -e` never trips mid-chain; the verdict is
# driven by STOP_COUNT.
# =============================================================================
REPORT_FILE="$PALW_DATA_ROOT/artifacts/verify-coinbase.txt"
STOP_COUNT=0

rpt()  { printf '%s\n' "$*" >> "$REPORT_TMP"; }
fail() {
    STOP_COUNT=$(( STOP_COUNT + 1 ))
    warn "STOP: $*"
    rpt "    >>> FAIL (STOP): $*"
    return 0
}
assert_eq() {   # <label> <observed> <expected>
    if [ "$2" = "$3" ]; then
        rpt "    PASS  $1: $2 == $3 sompi"
    else
        rpt "    ----  $1: observed $2, expected $3 sompi"
        fail "$1 mismatch: observed $2 sompi, expected $3 sompi."
    fi
    return 0
}
assert_rng() {  # <label> <observed> <max>  (assert 0 <= observed <= max)
    if [ "$2" -ge 0 ] && [ "$2" -le "$3" ]; then
        rpt "    PASS  $1: 0 <= $2 <= $3 sompi (pool)"
    else
        rpt "    ----  $1: observed $2 not in [0, $3] sompi (pool)"
        fail "$1 out of range: observed $2 sompi, pool is $3 sompi (must be 0..pool; remainder is burned)."
    fi
    return 0
}

# _uint <value> <name> — die unless <value> is a non-negative decimal integer.
_uint() {
    case "$1" in
        ''|*[!0-9]*) die "$2 is missing or not a non-negative integer: '$1' — the mint stage must state_set a sompi value here." ;;
        *) : ;;
    esac
}

# =============================================================================
# Read the recorded minted-block facts (state.env, via state_get). All optional
# at this layer — absence simply means "no block minted" (handled below).
# =============================================================================
BLOCK_HASH="$(state_get PALW_ALGO4_BLOCK_HASH_A)"
SUBSIDY="$(state_get PALW_ALGO4_SUBSIDY_SOMPI)"
PI_BPS="$(state_get PALW_ALGO4_PREMIUM_PI_BPS)"
SRC_CLASS="$(state_get PALW_ALGO4_SOURCE_CLASS)"
CB_A="$(state_get PALW_ALGO4_CB_PROVIDER_A_SOMPI)"
CB_B="$(state_get PALW_ALGO4_CB_PROVIDER_B_SOMPI)"
CB_INCL="$(state_get PALW_ALGO4_CB_INCLUSION_SOMPI)"
CB_VAL="$(state_get PALW_ALGO4_CB_VALIDATOR_SOMPI)"
CB_A_SPK="$(state_get PALW_ALGO4_CB_PROVIDER_A_SPK)"
CB_B_SPK="$(state_get PALW_ALGO4_CB_PROVIDER_B_SPK)"

# "A block was minted" ⇔ the mint stage recorded a block hash OR any coinbase
# output value. (Either alone is enough to trigger the full, fail-closed check
# — a partial record then fails loudly rather than being silently skipped.)
MINT_RECORDED=0
if [ -n "$BLOCK_HASH" ] || [ -n "$CB_A" ] || [ -n "$CB_B" ] || [ -n "$CB_INCL" ] || [ -n "$CB_VAL" ]; then
    MINT_RECORDED=1
fi

PALW_BATCH_ID="$(state_get PALW_BATCH_ID)"

# =============================================================================
# Open the report (atomic: build in a temp under artifacts/, mv into place at the
# end). load_env already created artifacts/ 0700; re-assert idempotently.
# =============================================================================
install -d -m 0700 "$(dirname "$REPORT_FILE")" || die "cannot create artifacts dir for $REPORT_FILE"
REPORT_TMP="$(mktemp "${REPORT_FILE}.XXXXXX")" || die "mktemp failed near $REPORT_FILE"

rpt "PALW algo-4 coinbase split report (STN-013 / T15)"
rpt "generated:            $(date '+%Y-%m-%dT%H:%M:%S%z')"
rpt "configured network:   $NETWORK   (base=$NETWORK_BASE suffix=$NETSUFFIX)"
rpt "ticket mode:          $TICKET_MODE"
rpt "batch id:             ${PALW_BATCH_ID:-<unset>}"
rpt "algo-4 block hash:    ${BLOCK_HASH:-<none recorded>}"
rpt ""
rpt "Split derivation (ReplicaPalw; consensus/src/processes/coinbase.rs):"
rpt "  inclusion_pool = floor(S * ${PALW_INCLUSION_BPS} / 10000)      §D worker-inclusion (8%)"
rpt "  validator_pool = floor(S * ${PALW_VALIDATOR_BPS} / 10000)     §E validator pool   (15%)"
rpt "  worker_base    = S - inclusion_pool - validator_pool          provider pair base  (77%)"
rpt "  provider_a     = floor(worker_base * 10000 / (10000 + pi_bps))"
rpt "  provider_b     = worker_base - provider_a                     (m=1 v1 leaf)"
rpt "  A/B EXACT; inclusion/validator are pools (observed <= pool, remainder burned)."
rpt ""

# =============================================================================
# Case 1 — NO block minted. This is the EXPECTED TICKET_MODE=skip end state.
# =============================================================================
if [ "$MINT_RECORDED" -eq 0 ]; then
    if [ "$TICKET_MODE" = "skip" ]; then
        # Exact, required N/A line for the skip end state.
        MSG="coinbase assertion N/A: no algo-4 block (TICKET_MODE=skip)"
    else
        MSG="coinbase assertion N/A: no algo-4 block minted yet (TICKET_MODE=$TICKET_MODE; PALW_ALGO4_BLOCK_HASH_A/PALW_ALGO4_CB_* unset). The mint stage (start-palw-miner.sh) captures the block hash and coinbase outputs and state_set's these once a wiring-only block is minted, using the mock-ticket helper (a workspace member built by build-and-hash.sh)."
    fi
    rpt "RESULT: $MSG"
    rpt ""
    rpt "VERDICT: N/A — nothing to assert (0 stop conditions)."
    log "$MSG"

    chmod 0644 "$REPORT_TMP" 2>/dev/null || true
    mv "$REPORT_TMP" "$REPORT_FILE" || die "failed to write report to $REPORT_FILE"
    REPORT_TMP=""
    log "report written -> $REPORT_FILE (replaces any prior run's report)"
    exit 0
fi

# =============================================================================
# Case 2 — a block IS recorded. First fail-closed on contradictory/incomplete
# state, then derive-and-assert.
# =============================================================================

# skip mode can NEVER mint — a recorded block here is stale/inconsistent state.
if [ "$TICKET_MODE" = "skip" ]; then
    rpt "RESULT: recorded algo-4 block state while TICKET_MODE=skip (which cannot mint)."
    fail "algo-4 coinbase state is recorded while TICKET_MODE=skip, which cannot mint a block — inconsistent/stale state. Clear the PALW_ALGO4_* slots in artifacts/state.env, or re-run the mint under TICKET_MODE=mock."
fi

# A minted block must carry its identity and its subsidy (needed to derive S).
[ -n "$BLOCK_HASH" ] || die "coinbase outputs are recorded but PALW_ALGO4_BLOCK_HASH_A is unset — the mint stage must state_set the block hash alongside the observed coinbase (partial record refused)."
_uint "$SUBSIDY" PALW_ALGO4_SUBSIDY_SOMPI

# Premium: default to neutral if unrecorded (v1 leaf at π = 1.0 splits base equally).
if [ -z "$PI_BPS" ]; then
    PI_BPS="$PALW_PREMIUM_BPS_ONE"
    rpt "premium pi_bps:       $PI_BPS (default: neutral — PALW_ALGO4_PREMIUM_PI_BPS unset)"
else
    _uint "$PI_BPS" PALW_ALGO4_PREMIUM_PI_BPS
    rpt "premium pi_bps:       $PI_BPS"
fi

# Source class: default replica_palw (a paying source).
SRC_CLASS="${SRC_CLASS:-replica_palw}"
rpt "source class:         $SRC_CLASS"
rpt "block subsidy S:      $SUBSIDY sompi"
rpt ""

# --- Derive the four shares exactly as consensus does ------------------------
INCL_POOL=$(( SUBSIDY * PALW_INCLUSION_BPS / 10000 ))
VAL_POOL=$(( SUBSIDY * PALW_VALIDATOR_BPS / 10000 ))
SERVICE=$(( SUBSIDY * PALW_SERVICE_BPS / 10000 ))            # 0 for the PALW lane
WORKER_BASE=$(( SUBSIDY - INCL_POOL - VAL_POOL - SERVICE ))  # primary takes remainder
DENOM=$(( PALW_PREMIUM_BPS_ONE + PI_BPS ))
EXP_A=$(( WORKER_BASE * PALW_PREMIUM_BPS_ONE / DENOM ))
EXP_B=$(( WORKER_BASE - EXP_A ))

rpt "Derived (sompi):"
rpt "  inclusion_pool = $INCL_POOL"
rpt "  validator_pool = $VAL_POOL"
rpt "  service        = $SERVICE"
rpt "  worker_base    = $WORKER_BASE"
rpt "  provider_a     = $EXP_A"
rpt "  provider_b     = $EXP_B"
rpt ""

# --- Internal conservation of the derivation (guards a bad S / constants) -----
# worker_base + inclusion_pool + validator_pool + service MUST reconstitute S,
# and provider_a + provider_b MUST reconstitute worker_base (base fully split).
SUM_SHARES=$(( WORKER_BASE + INCL_POOL + VAL_POOL + SERVICE ))
if [ "$SUM_SHARES" != "$SUBSIDY" ]; then
    fail "derivation conservation broken: worker_base+inclusion_pool+validator_pool+service ($SUM_SHARES) != subsidy S ($SUBSIDY)."
else
    rpt "    PASS  conservation: worker_base+pools+service == S ($SUBSIDY sompi)"
fi
SUM_AB=$(( EXP_A + EXP_B ))
if [ "$SUM_AB" != "$WORKER_BASE" ]; then
    fail "derivation conservation broken: provider_a+provider_b ($SUM_AB) != worker_base ($WORKER_BASE)."
else
    rpt "    PASS  conservation: provider_a+provider_b == worker_base ($WORKER_BASE sompi)"
fi
rpt ""

# =============================================================================
# Deferred-payout path (honest). An algo-4 block's OWN coinbase pays its mergeset
# (the algo-3 base blocks it merges), NOT its providers: provider A/B are paid only
# in the coinbase of a LATER block that merges THIS block as a blue ReplicaPalw
# source, and on the weight-0 wiring fork (PALW-014) that merge may be red (providers
# paid 0) or never happen. So at mint time the observed provider/inclusion/validator
# payouts are not present in this block. When the mint stage captured the subsidy S but
# those observed axes are unset, assert what IS verifiable now (S + the derived split
# that WILL apply on a blue merge) and report the payouts as DEFERRED — an honest
# verdict, neither a fabricated exact-match nor a blanket N/A.
if [ "$SRC_CLASS" = "replica_palw" ] && [ -z "$CB_A" ] && [ -z "$CB_B" ] && [ -z "$CB_INCL" ] && [ -z "$CB_VAL" ]; then
    rpt "Observed coinbase: provider / §D inclusion / §E validator payouts DEFERRED."
    rpt "  An algo-4 block's own coinbase pays its mergeset (algo-3 base blocks), not its"
    rpt "  providers. Provider A/B (derived $EXP_A / $EXP_B sompi), §D inclusion (<= $INCL_POOL)"
    rpt "  and §E validator (<= $VAL_POOL) are paid in a LATER block that merges this block as a"
    rpt "  blue ReplicaPalw source (red -> providers 0, or absent, on the weight-0 fork)."
    rpt ""
    # Review §8 (P0-5): try the descendant settlement NOW with the shipped verifier.
    # A located blue merge with exact-matching values upgrades this run to a full
    # observed verification; not-yet-merged (exit 2) or partial (exit 3, no SPKs)
    # stays PARTIAL_DEFERRED — never PASS for unobserved payouts.
    SETTLE_RC=0
    SETTLE_OUT="$("$VAL" find-reward-settlement --source-block "$BLOCK_HASH" \
        --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" 2>&1)" || SETTLE_RC=$?
    printf '%s\n' "$SETTLE_OUT" | while IFS= read -r line; do rpt "  $line"; done
    rpt ""
    if [ "$SETTLE_RC" -eq 0 ]; then
        rpt "VERDICT: PASS — descendant settlement located and verified (see settlement.* above)."
        rpt "verdict.machine: PASS_SETTLED"
        state_set PALW_COINBASE_VERDICT "pass-settled"
        log "coinbase assertion PASS: S=$SUBSIDY verified + descendant settlement verified."
    else
        rpt "VERDICT: PARTIAL (deferred) — block minted; subsidy S=$SUBSIDY verified and split derived;"
        rpt "         provider payouts NOT OBSERVED (paid only on a descendant blue merge; weight-0 fork)."
        rpt "         A deferred payout is NOT a verified payout (review §8.4)."
        rpt "verdict.machine: PARTIAL_DEFERRED"
        rpt "payouts.observed: false"
        state_set PALW_COINBASE_VERDICT "partial-deferred"
        log "coinbase assertion PARTIAL (deferred): S=$SUBSIDY sompi verified + split derived; provider/inclusion/validator payouts remain unobserved (settlement verifier: rc=$SETTLE_RC). Re-run ./verify-coinbase.sh (or kaspa-pq-validator find-reward-settlement with --provider-{a,b}-spk) once the mint has been merged."
    fi
    chmod 0644 "$REPORT_TMP" 2>/dev/null || true
    mv "$REPORT_TMP" "$REPORT_FILE" || die "failed to write report to $REPORT_FILE"
    exit 0
fi

# =============================================================================
# Assert the OBSERVED coinbase against the derivation.
# =============================================================================
case "$SRC_CLASS" in
    replica_palw)
        # A paying source: every observed coinbase value must be present + numeric.
        _uint "$CB_A"    PALW_ALGO4_CB_PROVIDER_A_SOMPI
        _uint "$CB_B"    PALW_ALGO4_CB_PROVIDER_B_SOMPI
        _uint "$CB_INCL" PALW_ALGO4_CB_INCLUSION_SOMPI
        _uint "$CB_VAL"  PALW_ALGO4_CB_VALIDATOR_SOMPI

        rpt "Observed coinbase (sompi) vs derived:"
        # Providers: EXACT match required.
        assert_eq "provider A payout" "$CB_A" "$EXP_A"
        assert_eq "provider B payout" "$CB_B" "$EXP_B"
        # §D inclusion / §E validator: pools — observed is a share, 0..pool, rest burned.
        assert_rng "inclusion (§D) payout" "$CB_INCL" "$INCL_POOL"
        assert_rng "validator (§E) payout" "$CB_VAL"  "$VAL_POOL"

        # The observed provider SPKs must be the leaf's reward scripts — which are
        # the providers' BOND-LOCK scripts, recorded by register-providers.sh as
        # PROV_{A,B}_REWARD_SPK. CRITICAL-1 pays nothing unless the leaf named
        # exactly those, so anything else here means the payout could not have
        # happened. (The old check rebuilt a synthetic SPK from
        # PROV_{A,B}_REWARD_PK_BYTE, which no bond ever owns.)
        if [ -n "$CB_A_SPK" ] || [ -n "$CB_B_SPK" ]; then
            rpt ""
            rpt "Provider reward SPK checks (must equal provider_bond_lock_spk):"
            _spk_a="$(state_get PROV_A_REWARD_SPK)"
            _spk_b="$(state_get PROV_B_REWARD_SPK)"
            if [ -n "$_spk_a" ] && [ -n "$CB_A_SPK" ]; then
                if [ "$(printf '%s' "$CB_A_SPK" | tr 'A-F' 'a-f')" = "$_spk_a" ]; then
                    rpt "    PASS  provider A SPK matches PROV_A_REWARD_SPK (bond-lock script)"
                else
                    rpt "    ----  provider A SPK observed '$CB_A_SPK' != expected '$_spk_a'"
                    fail "provider A coinbase SPK is not the provider's bond-lock script (PROV_A_REWARD_SPK)."
                fi
            elif [ -n "$CB_A_SPK" ]; then
                rpt "    ----  PROV_A_REWARD_SPK not recorded; cannot check provider A's SPK (run ./register-providers.sh)"
            fi
            if [ -n "$_spk_b" ] && [ -n "$CB_B_SPK" ]; then
                if [ "$(printf '%s' "$CB_B_SPK" | tr 'A-F' 'a-f')" = "$_spk_b" ]; then
                    rpt "    PASS  provider B SPK matches PROV_B_REWARD_SPK (bond-lock script)"
                else
                    rpt "    ----  provider B SPK observed '$CB_B_SPK' != expected '$_spk_b'"
                    fail "provider B coinbase SPK is not the provider's bond-lock script (PROV_B_REWARD_SPK)."
                fi
            elif [ -n "$CB_B_SPK" ]; then
                rpt "    ----  PROV_B_REWARD_SPK not recorded; cannot check provider B's SPK (run ./register-providers.sh)"
            fi
        fi
        ;;

    halted|duplicate|unbacked)
        # DO-NOT-MINT: a halted-beacon (K5), duplicate-work (G16) or unbacked-
        # collateral (ECON-03) source is paid NOTHING on EVERY axis — the
        # consensus arm body is empty, the whole reward is burned by don't-mint
        # and is never rerouted to the includer. Assert all four axes are zero.
        # Missing values are treated as 0 (nothing paid); non-zero is a hard FAIL.
        _z_a="${CB_A:-0}"; _z_b="${CB_B:-0}"; _z_i="${CB_INCL:-0}"; _z_v="${CB_VAL:-0}"
        _uint "$_z_a" PALW_ALGO4_CB_PROVIDER_A_SOMPI
        _uint "$_z_b" PALW_ALGO4_CB_PROVIDER_B_SOMPI
        _uint "$_z_i" PALW_ALGO4_CB_INCLUSION_SOMPI
        _uint "$_z_v" PALW_ALGO4_CB_VALIDATOR_SOMPI
        rpt "Do-not-mint source class '$SRC_CLASS' — asserting zero on every axis:"
        assert_eq "provider A payout (do-not-mint)" "$_z_a" 0
        assert_eq "provider B payout (do-not-mint)" "$_z_b" 0
        assert_eq "inclusion (§D) payout (do-not-mint)" "$_z_i" 0
        assert_eq "validator (§E) payout (do-not-mint)" "$_z_v" 0
        ;;

    *)
        rpt "RESULT: unknown source class '$SRC_CLASS'."
        fail "PALW_ALGO4_SOURCE_CLASS='$SRC_CLASS' is not one of replica_palw|halted|duplicate|unbacked — refusing to assert against an unknown class."
        ;;
esac
rpt ""

# =============================================================================
# Verdict — commit the report atomically, then fail closed if any STOP fired.
# =============================================================================
if [ "$STOP_COUNT" -gt 0 ]; then
    rpt "VERDICT: FAIL — $STOP_COUNT stop condition(s). Coinbase split NOT verified. STOP."
else
    rpt "VERDICT: PASS — 0 stop conditions. Coinbase split matches ReplicaPalw."
fi

# Commit: chmod the temp, mv into place, and blank REPORT_TMP so the cleanup trap
# leaves the committed file alone. A prior run's report is replaced here — the
# intended, LOGGED behaviour of regenerating this evidence file, not a silent
# clobber of unrelated data.
chmod 0644 "$REPORT_TMP" 2>/dev/null || true
mv "$REPORT_TMP" "$REPORT_FILE" || die "failed to write report to $REPORT_FILE"
REPORT_TMP=""
log "report written -> $REPORT_FILE (replaces any prior run's report)"

if [ "$STOP_COUNT" -gt 0 ]; then
    die "coinbase verification FAILED: $STOP_COUNT stop condition(s). See $REPORT_FILE for the per-share evidence."
fi
log "STN-013/T15 coinbase verification PASS: Provider A/B exact + §D inclusion/§E validator pools for block ${BLOCK_HASH} match the ReplicaPalw split. Report: $REPORT_FILE"
