#!/usr/bin/env bash
# =============================================================================
# register-providers.sh — STN-010: register the three PALW provider bonds for
#                         the closed two-node testnet (Phase-0 wiring).
#
#   usage:  ./register-providers.sh            # (or: ./register-providers.sh register)
#
# WHAT THIS DOES (honest scope):
#   For each of three identities — provider-a, provider-b and an INDEPENDENT
#   auditor-c — it
#     1. builds a provider-bond payload with `VAL palw-payload provider-bond`
#        (DISTINCT operator-group-id per identity — the auditor group differs
#        from BOTH providers; the SHARED runtime-class; capacity
#        <SHAPE_ID>=<CAPACITY_COUNT>; a DISTINCT reward-key-root; amount 10 MSK;
#        unbond-delay 6 epochs), then
#     2. submits it as a REAL on-chain carrier with `VAL palw-submit
#        --kind provider-bond` (the supporting miner MUST be running — submit
#        blocks on inclusion), capturing locked_provider_bond_outpoint into
#        artifacts/state.env (PROV_A_BOND / PROV_B_BOND / AUD_C_BOND), then
#     3. verifies provider.in_registry=true + status=active on BOTH nodes.
#
#   These are REAL provider bonds registered through the real lifecycle carriers,
#   so BOTH nodes obtain them over P2P. This is deliberately NOT the seeded,
#   test-only `palw_demo` path, and it mints NO algo-4 block (no ticket is
#   involved at this stage).
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
#   (a bond outpoint already recorded, or a payload file already present, is
#   never rebuilt / re-submitted / silently overwritten); FAIL-CLOSED with
#   actionable messages; a register_cleanup trap removes any half-written
#   payload so a failed run never leaves a truncated .borsh in place. It sources
#   common.sh and uses ONLY its helpers — nothing is reimplemented.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# Nicer per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-register-providers}"; export PALW_LOG_TAG

# Supervised name of the continuous algo-3 supporting miner (set by
# supporting-miner.sh). palw-submit blocks on inclusion, so this must be alive.
MINER_PID_NAME="${MINER_PID_NAME:-supporting-miner}"

usage() {
    cat >&2 <<EOF
usage: ${0##*/} [register]

  Register the three PALW provider bonds for the closed two-node testnet
  (STN-010): provider-a, provider-b and an INDEPENDENT auditor-c, each with a
  DISTINCT operator-group-id (the auditor group differs from BOTH providers),
  the SHARED runtime-class, capacity <SHAPE_ID>=<CAPACITY_COUNT>, a DISTINCT
  reward-key-root, amount \$PROVIDER_*_AMOUNT/\$AUDITOR_AMOUNT (>=10 MSK) and
  unbond-delay \$UNBOND_DELAY_EPOCHS (>=6). For each identity it builds the
  provider-bond payload (VAL palw-payload provider-bond) and submits it as a
  REAL on-chain carrier (VAL palw-submit --kind provider-bond) with the
  supporting miner running, records locked_provider_bond_outpoint into
  artifacts/state.env (PROV_A_BOND / PROV_B_BOND / AUD_C_BOND), then verifies
  provider.in_registry=true + status=active on BOTH nodes.

  These are REAL provider bonds registered through the real lifecycle carriers
  so both nodes obtain them over P2P — NOT the seeded, test-only palw_demo path,
  and NOT a minted algo-4 block (no ticket is involved here).

  Idempotent: an identity whose bond outpoint is already recorded, or whose
  payload file already exists, is not rebuilt or re-submitted. Prerequisites
  (nodes up + synced, supporting miner running, funded provider seeds) are
  checked fail-closed with actionable messages.
EOF
}

# ---------------------------------------------------------------------------
# Tiny local validators (NOT reimplementations of common.sh helpers — common.sh
# ships no hex/distinctness validator).
# ---------------------------------------------------------------------------
# _is_hex128 <str>  — 0 iff <str> is exactly 128 hex chars (a 64-byte Hash64).
_is_hex128() {
    case "$1" in *[!0-9a-fA-F]*) return 1 ;; esac
    [ "${#1}" -eq 128 ]
}
# _require_distinct3 <a> <b> <c> <field>  — die unless all three differ.
_require_distinct3() {
    local a="$1" b="$2" c="$3" field="$4"
    [ "$a" != "$b" ] || die "$field: values for A and B must differ (both = '$a')."
    [ "$a" != "$c" ] || die "$field: values for A and C (auditor) must differ (both = '$a')."
    [ "$b" != "$c" ] || die "$field: values for B and C (auditor) must differ (both = '$b')."
}

# ---------------------------------------------------------------------------
# Preconditions — fail-closed before we touch any funds.
# ---------------------------------------------------------------------------
preflight() {
    wait_rpc_up a       || die "node A wRPC is not answering — start node-a.sh (and node-b.sh) first."
    wait_rpc_up b       || die "node B wRPC is not answering — start node-b.sh first (STN-010 asserts both-node parity)."
    wait_node_synced a  || die "node A is not synced — cannot submit provider-bond carriers reliably."
    wait_node_synced b  || die "node B is not synced — cannot assert both-node provider parity."
    is_running "$MINER_PID_NAME" \
        || die "supporting miner '$MINER_PID_NAME' is not running — palw-submit blocks on inclusion and needs a live miner. Start ./supporting-miner.sh start, then retry."
}

# ---------------------------------------------------------------------------
# register_one <name> <tag a|b|c> <seed> <group> <reward_root> <amount> <state_key>
#   Build the provider-bond payload (idempotent), submit the carrier, capture
#   locked_provider_bond_outpoint into <state_key>, and mine a child so the
#   past-relative palw-status view will reflect it.
# ---------------------------------------------------------------------------
register_one() {
    local name="$1" tag="$2" seed="$3" group="$4" reward="$5" amount="$6" statekey="$7"
    local existing out tmp submit_out bond_op op

    # Idempotency: a recorded bond outpoint means this identity is already
    # registered — never re-submit (a second bond would double-lock funds and
    # overwrite the outpoint). Verification below still covers it.
    existing="$(state_get "$statekey")"
    if [ -n "$existing" ]; then
        log "$name: bond outpoint already recorded ($statekey=$existing); skipping build+submit (idempotent)."
        return 0
    fi

    # The seed is produced and FUNDED to coinbase maturity by the earlier
    # funding stage; this stage does not create unfunded keys. Fail-closed.
    [ -f "$seed" ] || die "$name: validator/funding seed not found at '$seed'. Run the funding stage (keygen + fund to maturity) first, or set the seed path via env (PROV_A_KEY / PROV_B_KEY / AUD_C_KEY)."

    out="$PALW_DATA_ROOT/artifacts/provider-bond-$tag.borsh"
    if [ -s "$out" ]; then
        # Detected an existing payload — reuse it, never silently overwrite.
        # (provider-bond payloads are deterministic for fixed inputs; if you
        # changed a bond input, remove the stale .borsh before re-running.)
        log "$name: reusing existing provider-bond payload $out (not overwriting)."
    else
        # Build to a .partial and mv into place on success so a crash mid-write
        # never leaves a truncated payload (and never clobbers a prior-good one).
        tmp="$out.partial"
        register_cleanup "rm -f '$tmp'"
        log "$name: building provider-bond payload -> $out (group=$group capacity=${SHAPE_ID}=${CAPACITY_COUNT} unbond-delay=${UNBOND_DELAY_EPOCHS}e)"
        "$VAL" palw-payload provider-bond \
            --network "$NETWORK" \
            --validator-key "$seed" \
            --operator-group-id "$group" \
            --runtime-class "$RUNTIME_CLASS" \
            --capacity "${SHAPE_ID}=${CAPACITY_COUNT}" \
            --reward-key-root "$reward" \
            --amount "$amount" \
            --unbond-delay-epochs "$UNBOND_DELAY_EPOCHS" \
            --out "$tmp" \
            || die "$name: 'palw-payload provider-bond' failed (verify group/runtime/reward are 128-hex, the capacity shape, and amount >= 10 MSK)."
        [ -s "$tmp" ] || die "$name: payload build produced an empty file ($tmp)."
        mv "$tmp" "$out" || die "$name: could not finalize payload $out."
    fi

    # A live miner is required for the carrier to reach inclusion.
    is_running "$MINER_PID_NAME" \
        || die "$name: supporting miner '$MINER_PID_NAME' stopped — palw-submit blocks on inclusion. Start ./supporting-miner.sh start and retry."

    # Invariant 4: exclude every already-known bond outpoint from funding-input
    # selection. Harmless for an outpoint this seed does not control (it is never
    # a selection candidate anyway); REQUIRED when a single funding key backs
    # more than one bond. Values come from the live (state-overlaid) env.
    #
    # RETIRED bonds must be excluded too, and they are NOT in state.env: when an
    # identity is RE-bonded (its previous bond was slashed, or unbonded and left
    # to lapse), the operator clears PROV_*_BOND so register_one rebuilds — which
    # also drops that outpoint from this list. Its locked output-0 is plain P2PKH
    # at the SAME funding address, so `VAL balance` still counts it and funding
    # selection will happily pick it, but ProviderBondSpendFilter (ADR-0040
    # ECON-03 leg 4) locks a non-releasable bond's output-0 forever: the carrier
    # is then SKIPPED at acceptance (not block-rejected), so it silently never
    # confirms and palw-submit times out waiting for its change outpoint. List
    # every such outpoint in EXTRA_EXCLUDE_OUTPOINTS (space-separated txid:index).
    EXCLUDES=()
    for op in "${DNS_BOND:-}" "${PROV_A_BOND:-}" "${PROV_B_BOND:-}" "${AUD_C_BOND:-}" ${EXTRA_EXCLUDE_OUTPOINTS:-}; do
        [ -n "$op" ] || continue
        EXCLUDES[${#EXCLUDES[@]}]="--exclude-funding-outpoint"
        EXCLUDES[${#EXCLUDES[@]}]="$op"
    done

    log "$name: submitting provider-bond carrier to node A ($(node_wrpc a)) — waits for inclusion"
    if ! submit_out="$("$VAL" palw-submit \
            --node-wrpc-borsh "$(node_wrpc a)" \
            --network "$NETWORK" \
            --validator-key "$seed" \
            --kind provider-bond \
            --payload-file "$out" \
            ${EXCLUDES[@]+"${EXCLUDES[@]}"} 2>&1)"; then
        printf '%s\n' "$submit_out" >&2
        die "$name: 'palw-submit --kind provider-bond' failed (see output above — miner running? funding matured & sufficient? node A RPC up?)."
    fi

    # Capture the bond-locking outpoint (txid:index) — the id used by
    # palw-status --provider-bond. This is NOT the change outpoint.
    bond_op="$(printf '%s\n' "$submit_out" | _kv locked_provider_bond_outpoint)"
    case "$bond_op" in
        *:[0-9]*) : ;;
        *) printf '%s\n' "$submit_out" >&2
           die "$name: could not parse 'locked_provider_bond_outpoint' (txid:index) from palw-submit output (see above)." ;;
    esac

    state_set "$statekey" "$bond_op"
    log "$name: locked_provider_bond_outpoint=$bond_op recorded as $statekey"

    # Invariant 3: mine >=1 selected child so the past-relative palw-status view
    # reflects this carrier before provider.* is read (in verify_all).
    wait_inclusion a || die "$name: no selected child was mined after the provider-bond carrier (is the supporting miner still running?)."

    return 0
}

# ---------------------------------------------------------------------------
# record_reward_spk <name> <tag> <seed> <group> <reward_root> <amount> <state_key>
#   Record the ONE script a leaf may name as this provider's reward target.
#
#   WHY THIS EXISTS. `palw_work_reward_class` (utxo_validation.rs, CRITICAL-1)
#   requires `leaf.provider_{a,b}_reward_script == provider_bond_lock_spk(bond
#   owner_public_key)`. Name anything else and the algo-4 source classifies as
#   ReplicaPalwUnbackedCollateral: providers are paid NOTHING and, because that is
#   the merging block's whole mergeset, its coinbase comes out with ZERO outputs —
#   the block is accepted and the entire subsidy is paid to nobody. The old mock
#   named a synthetic 0x71/0x72 byte pattern (PROV_*_REWARD_PK_BYTE), so no live
#   algo-4 block ever paid a provider.
#
#   It is NOT `provider.owner_pubkey_hash` (what palw-status prints): that is the
#   unkeyed overlay credential; the lock uses blake2b_512_address_payload. Locking
#   to the wrong one would freeze the collateral permanently (palw.rs:3160).
#
#   The value is a pure function of the seed, so this recomputes it offline every
#   run (idempotent, no on-chain effect) and works for an ALREADY-registered bond.
#   The payload is built to a throwaway path purely to read the printed SPK.
# ---------------------------------------------------------------------------
record_reward_spk() {
    local name="$1" tag="$2" seed="$3" group="$4" reward="$5" amount="$6" statekey="$7"
    local tmp out spk prev

    [ -f "$seed" ] || die "$name: seed '$seed' missing — cannot derive the reward SPK consensus requires."
    tmp="$PALW_DATA_ROOT/artifacts/.reward-spk-$tag.$$.borsh"
    rm -f "$tmp"
    register_cleanup "rm -f '$tmp'"
    out="$("$VAL" palw-payload provider-bond \
            --network "$NETWORK" --validator-key "$seed" \
            --operator-group-id "$group" --runtime-class "$RUNTIME_CLASS" \
            --capacity "${SHAPE_ID}=${CAPACITY_COUNT}" --reward-key-root "$reward" \
            --amount "$amount" --unbond-delay-epochs "$UNBOND_DELAY_EPOCHS" \
            --out "$tmp" 2>&1)" \
        || { printf '%s\n' "$out" >&2; die "$name: could not derive the provider bond lock SPK (see above)."; }
    rm -f "$tmp"
    spk="$(printf '%s\n' "$out" | _kv provider_bond_lock_spk)"
    case "$spk" in
        0000*[!0-9a-fA-F]*|"") printf '%s\n' "$out" >&2
            die "$name: 'palw-payload provider-bond' did not print a usable provider_bond_lock_spk (needs a build that emits it)." ;;
    esac

    # Never silently change a recorded SPK: a leaf already committed on-chain names
    # the old one, and a mismatch is exactly the zero-payout bug this guards.
    prev="$(state_get "$statekey")"
    if [ -n "$prev" ] && [ "$prev" != "$spk" ]; then
        die "$name: recorded $statekey ($prev) differs from this seed's bond lock SPK ($spk) — the identity was re-keyed. Retire the old batch before continuing."
    fi
    [ -n "$prev" ] || state_set "$statekey" "$spk"
    log "$name: reward SPK (the only script consensus will pay) $statekey=$spk"
}

# ---------------------------------------------------------------------------
# verify_all — provider.in_registry=true + status=active on BOTH nodes (STN-010).
# ---------------------------------------------------------------------------
verify_all() {
    local pair name op n inreg status st bad=0
    log "verifying provider registry on BOTH nodes (STN-010)"

    # Ensure A and B share a sink so B's past-relative view has caught up.
    wait_same_sink || die "nodes A and B did not converge on a common sink — cannot assert both-node provider parity. Check P2P connectivity / sync."

    for pair in "provider-a=${PROV_A_BOND:-}" "provider-b=${PROV_B_BOND:-}" "auditor-c=${AUD_C_BOND:-}"; do
        name="${pair%%=*}"; op="${pair#*=}"
        [ -n "$op" ] || die "$name: no provider-bond outpoint recorded (state_set missing) — cannot verify."
        for n in a b; do
            st="$(palw_provider_status "$n" "$op" 2>/dev/null || true)"
            inreg="$(printf '%s\n' "$st" | _kv in_registry)"
            status="$(printf '%s\n' "$st" | _kv status)"
            if [ "$inreg" = "true" ] && [ "$status" = "active" ]; then
                log "OK: $name ($op) in_registry=true status=active on node-$n"
            else
                warn "FAIL: $name ($op) on node-$n -> in_registry='$inreg' status='$status'"
                bad=1
            fi
        done
    done

    [ "$bad" -eq 0 ] || die "provider registry verification FAILED on >=1 node (STN-010). Usual causes: a stale outpoint in artifacts/state.env, an un-mined carrier child, or node B not yet synced. Inspect node logs and $(state_file)."
    log "STN-010 OK: provider-a, provider-b and independent auditor-c all in_registry=true, status=active on BOTH node A and node B."
}

# ---------------------------------------------------------------------------
do_register() {
    local pair grpA grpB grpAUD rewA rewB rewAUD seedA seedB seedC

    # ---- shared runtime-class (providers' --runtime-class MUST equal every
    #      leaf's runtime_class_id). RUNTIME_CLASS is honored if set; otherwise
    #      RUNTIME_CLASS_ID from env.example is used. ---------------------------
    RUNTIME_CLASS="${RUNTIME_CLASS:-${RUNTIME_CLASS_ID:-}}"
    [ -n "$RUNTIME_CLASS" ] || die "runtime class unset: set RUNTIME_CLASS (or RUNTIME_CLASS_ID in env.local). Providers' --runtime-class MUST equal every leaf's runtime_class_id."
    _is_hex128 "$RUNTIME_CLASS" || die "RUNTIME_CLASS must be 128 hex chars (64-byte Hash64); got ${#RUNTIME_CLASS} chars."
    export RUNTIME_CLASS

    # ---- capacity <SHAPE_ID>=<CAPACITY_COUNT> (env.example wires both into the
    #      provider --capacity <SHAPE>=<COUNT> flag). Every leaf's shape_id MUST
    #      equal this SHAPE (consensus invariant). Live-verified run used 1:1. --
    case "${SHAPE_ID:-}"       in ''|*[!0-9]*) die "SHAPE_ID must be an integer (leaf shape_id must match this provider capacity SHAPE).";; esac
    case "${CAPACITY_COUNT:-}" in ''|*[!0-9]*) die "CAPACITY_COUNT must be a positive integer.";; esac
    [ "$CAPACITY_COUNT" -ge 1 ] || die "CAPACITY_COUNT must be >= 1; got $CAPACITY_COUNT."

    # ---- provider unbond-delay floor = 6 epochs (verified consensus constant) -
    case "${UNBOND_DELAY_EPOCHS:-}" in ''|*[!0-9]*) die "UNBOND_DELAY_EPOCHS must be an integer.";; esac
    [ "$UNBOND_DELAY_EPOCHS" -ge 6 ] || die "UNBOND_DELAY_EPOCHS must be >= 6 (provider unbond-delay floor); got $UNBOND_DELAY_EPOCHS."

    # ---- DISTINCT operator groups (auditor group MUST differ from A and B) ----
    grpA="${OPERATOR_GROUP_A:-}"; grpB="${OPERATOR_GROUP_B:-}"; grpAUD="${OPERATOR_GROUP_AUD:-}"
    for pair in "OPERATOR_GROUP_A:$grpA" "OPERATOR_GROUP_B:$grpB" "OPERATOR_GROUP_AUD:$grpAUD"; do
        _is_hex128 "${pair#*:}" || die "${pair%%:*} must be 128 hex chars (64-byte operator-group-id)."
    done
    _require_distinct3 "$grpA" "$grpB" "$grpAUD" "operator-group-id (auditor group MUST differ from providers A and B)"

    # ---- DISTINCT reward-key-roots. env.example ships A and B; the auditor's
    #      default is a deterministic devnet placeholder (fine for a closed,
    #      no-value wiring run) — override REWARD_KEY_ROOT_AUD to change it. ----
    rewA="${REWARD_KEY_ROOT_A:-}"; rewB="${REWARD_KEY_ROOT_B:-}"
    rewAUD="${REWARD_KEY_ROOT_AUD:-$(h64 c9)}"
    for pair in "REWARD_KEY_ROOT_A:$rewA" "REWARD_KEY_ROOT_B:$rewB" "REWARD_KEY_ROOT_AUD:$rewAUD"; do
        _is_hex128 "${pair#*:}" || die "${pair%%:*} must be 128 hex chars (64-byte reward-key-root)."
    done
    _require_distinct3 "$rewA" "$rewB" "$rewAUD" "reward-key-root"

    # ---- amounts (provider-bond floor 10 MSK; the binary enforces the floor) --
    [ -n "${PROVIDER_A_AMOUNT:-}" ] || die "PROVIDER_A_AMOUNT unset."
    [ -n "${PROVIDER_B_AMOUNT:-}" ] || die "PROVIDER_B_AMOUNT unset."
    [ -n "${AUDITOR_AMOUNT:-}" ]    || die "AUDITOR_AMOUNT unset."

    # ---- per-identity funding seeds (produced+funded by the funding stage) ----
    seedA="${PROV_A_KEY:-$PALW_DATA_ROOT/keys/provider-a.seed}"
    seedB="${PROV_B_KEY:-$PALW_DATA_ROOT/keys/provider-b.seed}"
    seedC="${AUD_C_KEY:-$PALW_DATA_ROOT/keys/auditor-c.seed}"

    preflight

    log "registering provider bonds via real on-chain carriers (NOT the seeded palw_demo path): provider-a, provider-b, auditor-c"
    register_one "provider-a" a "$seedA" "$grpA"   "$rewA"   "$PROVIDER_A_AMOUNT" PROV_A_BOND
    register_one "provider-b" b "$seedB" "$grpB"   "$rewB"   "$PROVIDER_B_AMOUNT" PROV_B_BOND
    register_one "auditor-c"  c "$seedC" "$grpAUD" "$rewAUD" "$AUDITOR_AMOUNT"    AUD_C_BOND

    # The ONLY reward script consensus will pay (CRITICAL-1). Recorded for BOTH
    # providers regardless of whether register_one built or skipped, because it is
    # a pure function of the seed — and because create-lifecycle fail-closes
    # without it (a placeholder reward script mints an algo-4 block that pays
    # nobody and leaves the merging coinbase empty).
    record_reward_spk "provider-a" a "$seedA" "$grpA" "$rewA" "$PROVIDER_A_AMOUNT" PROV_A_REWARD_SPK
    record_reward_spk "provider-b" b "$seedB" "$grpB" "$rewB" "$PROVIDER_B_AMOUNT" PROV_B_REWARD_SPK

    verify_all
}

# ---------------------------------------------------------------------------
# Dispatch. Validate the argument before load_env so --help works unconfigured.
ACTION="${1:-}"
case "$ACTION" in
    -h|--help|help) usage; exit 0 ;;
    ""|register)    : ;;
    *)              usage; die "unknown argument '$ACTION' (this stage takes no argument, or 'register')." ;;
esac

load_env
do_register
