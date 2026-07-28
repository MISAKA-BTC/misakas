#!/usr/bin/env bash
# =============================================================================
# bootstrap-funds.sh — STN-010 (funding) stage of the PALW closed two-node
#                      testnet harness (Phase-0 wiring).
#
#   SOURCE-then-run model: this script sources common.sh and uses ONLY its
#   helpers (load_env, logging, gates, state/PID/cleanup). It reimplements
#   nothing that common.sh already provides.
#
# WHAT IT DOES (idempotent, fail-closed):
#   1. Keygen the five harness identities into keys/*.seed (skips any that
#      already exist — `keygen` itself refuses to overwrite --out) and captures
#      each real on-chain funding_address into artifacts/state.env (*_ADDR).
#   2. Ensures the continuous supporting miner (chain liveness) is running.
#   3. Mines real coinbase blocks to each *bonding* identity address until its
#      address balance covers a 10 MSK bond + fee headroom.
#   4. Mines a maturity buffer so every funding coinbase is >= coinbase_maturity
#      (1000 DAA) deep and therefore spendable (bond aggregation needs mature
#      UTXOs; `VAL balance` reports total UTXO value incl. immature coinbase, so
#      spendability is guaranteed by DAA depth, not by the balance figure alone).
#   5. Verifies each bonding identity's balance >= 10 MSK + fee.
#
# HONEST LIMITS (do not overstate):
#   * This funds identities with REAL on-chain coinbase on a closed, no-value
#     devnet. It NEVER invokes the seeded, test-only `palw_demo` path.
#   * Funding assumes instant devnet blocks (skip_proof_of_work=true). Under
#     real algo-3 PoW (testnet-110) the mining bursts here are not realistic;
#     the script warns and proceeds if asked.
#   * Funding addresses are PUBLIC and may be logged. Seeds are secret: this
#     script never reads, prints, or copies a *.seed (only `keygen` writes them,
#     chmod 0600).
#
# Run standalone or via run-all.sh (after supporting-miner.sh, before
# dns-validator.sh). Safe to re-run: already-keyed identities, already-funded
# addresses, and an already-matured run are each detected and left untouched.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Nice per-stage log tag; honoured by common.sh log/warn/die (before load_env).
export PALW_LOG_TAG="${PALW_LOG_TAG:-bootstrap-funds}"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"
load_env

# -----------------------------------------------------------------------------
# Tunable knobs (env-overridable; devnet-safe defaults). Sompi arithmetic:
#   1 MSK = 1e8 sompi;  provider/stake bond floor = 10 MSK = 1e9 sompi.
# -----------------------------------------------------------------------------
: "${COINBASE_MATURITY:=1000}"                 # consensus coinbase_maturity (DAA), both PALW presets
: "${MSK_SOMPI:=100000000}"                    # 1 MSK in sompi
: "${FUND_MIN_SOMPI:=1000000000}"              # 10 MSK bond floor
: "${FUND_FEE_SOMPI:=100000000}"               # +1 MSK headroom for tx fee / change
: "${FUND_TARGET_SOMPI:=$(( FUND_MIN_SOMPI + FUND_FEE_SOMPI ))}"   # per-identity target (11 MSK)
: "${FUND_BLOCKS_PER_ID:=20}"                  # coinbase blocks mined per funding round
: "${FUND_MAX_ROUNDS:=50}"                     # cap funding rounds -> fail-closed if still short
: "${FUND_INTERVAL_MS:=0}"                     # devnet blocks are instant; mine as fast as possible
: "${MATURITY_MARGIN_BLOCKS:=200}"             # slack over coinbase_maturity for DAG width
: "${MATURITY_BUFFER_BLOCKS:=$(( COINBASE_MATURITY + MATURITY_MARGIN_BLOCKS ))}"  # buffer size
: "${MATURITY_INTERVAL_MS:=0}"                 # buffer mining interval
: "${MINE_BURST_TIMEOUT_SECS:=600}"            # deadline for a finite --blocks burst to finish
# The persistent supporting miner's supervised name. MUST match the CANONICAL name
# used by supporting-miner.sh / stop.sh / restart-a-synced / register-providers /
# create-lifecycle / submit-lifecycle / start-palw-miner ("supporting-miner"), so
# every stage refers to the SAME process. A mismatch here spawns a duplicate miner
# that stop.sh cannot reap and that defeats create-lifecycle's DAA freeze.
: "${SUPPORTING_MINER_NAME:=supporting-miner}"

# Defensive defaults for env vars we reference directly that load_env does NOT
# force-require (env.example sets them, but a minimal custom env.local might not).
# Keeps this script from tripping `set -u` if any are absent.
: "${ADDR_PREFIX:=misakadev}"                  # address prefix used to parse keygen output
: "${MINER_WORKER:=rig0}"                      # misaminer --worker label
: "${GATE_POLL_SECS:=2}"                        # poll interval for our wait loops

# Identity table: "<seed-basename>:<state-key-for-its-funding-address>".
#   ALL five are keyed and have their funding address captured.
#   The FOUR bonding identities (provider A/B, auditor C, dns-validator) are
#   explicitly funded to the bond floor + fee and verified.
#   'supporting' is the continuous miner's payout wallet — it accrues coinbase
#   from that miner, so it is keyed but NOT separately burst-funded/verified.
ALL_IDENTITIES="provider-a:PROV_A_ADDR provider-b:PROV_B_ADDR auditor-c:AUD_C_ADDR supporting:SUPPORTING_ADDR dns-validator:DNS_ADDR"
FUND_IDENTITIES="provider-a:PROV_A_ADDR provider-b:PROV_B_ADDR auditor-c:AUD_C_ADDR dns-validator:DNS_ADDR"

# -----------------------------------------------------------------------------
# Small local wrappers (miner launch + balance read). These are NOT common.sh
# helpers; they only compose the verified MINER/VAL flags + common.sh helpers.
# -----------------------------------------------------------------------------
# miner_log <name>  — conventional per-miner log path under logs/.
miner_log() { printf '%s/logs/%s.log\n' "$PALW_DATA_ROOT" "${1:?name}"; }

# balance_sompi <addr>  — echo the address balance in sompi (integer) via
#   `VAL balance` against node A (node A must run with --utxoindex). Empty on
#   any failure so callers can treat it as 0 (fail-open READ, fail-closed spend
#   decisions happen in the >= comparisons).
balance_sompi() {
    local addr="${1:?address}" out
    out="$("$VAL" balance --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" --address "$addr" 2>/dev/null || true)"
    # First all-digit field on the line is the sompi value ("<addr>\t<sompi>\t<MSK> MSK").
    printf '%s\n' "$out" | awk '{ for (i=1;i<=NF;i++) if ($i ~ /^[0-9]+$/) { print $i; exit } }'
}

# mine_blocks <name> <wallet> <blocks> [interval_ms]  — run ONE finite misaminer
#   burst (--blocks <n>) paying <wallet>, supervised under <name>, and block
#   until it finishes or MINE_BURST_TIMEOUT_SECS elapses. Idempotent: if a burst
#   of <name> is already running it just waits on it. Returns non-zero on timeout.
mine_blocks() {
    local name="${1:?name}" wallet="${2:?wallet}" blocks="${3:?blocks}" interval="${4:-0}" deadline
    if is_running "$name"; then
        log "mine_blocks: $name already running; waiting for it to finish"
    else
        "$MINER" --pool "$(node_grpc a)" --network-id "$NETWORK" \
            --wallet "$wallet" --worker "$MINER_WORKER" \
            --blocks "$blocks" --min-block-interval-ms "$interval" \
            >> "$(miner_log "$name")" 2>&1 &
        write_pid "$name" "$!"
        log "mine_blocks: $name mining $blocks block(s) -> $wallet"
    fi
    deadline=$(( $(date +%s) + MINE_BURST_TIMEOUT_SECS ))
    while is_running "$name"; do
        if [ "$(date +%s)" -ge "$deadline" ]; then
            warn "mine_blocks: $name did not finish within ${MINE_BURST_TIMEOUT_SECS}s (see $(miner_log "$name"))"
            stop_pid "$name" >/dev/null 2>&1 || true
            return 1
        fi
        sleep "$GATE_POLL_SECS"
    done
    stop_pid "$name" >/dev/null 2>&1 || true   # reap the finished burst's pidfile
    return 0
}

# _register_burst_cleanup  — arm ONE cleanup trap that tears down the transient
#   funding/maturity burst miners on EXIT/INT/TERM. Deliberately excludes the
#   persistent supporting miner (later stages depend on it; stop.sh ends it).
_register_burst_cleanup() {
    local tok base snippet=""
    for tok in $FUND_IDENTITIES; do
        base="${tok%%:*}"
        snippet="$snippet stop_pid \"miner-fund-$base\" >/dev/null 2>&1 || true;"
    done
    snippet="$snippet stop_pid \"miner-maturity\" >/dev/null 2>&1 || true;"
    register_cleanup "$snippet"
}

# -----------------------------------------------------------------------------
# Stage steps
# -----------------------------------------------------------------------------
# keygen_identity <basename> <state-key>  — idempotently create keys/<basename>.seed
#   and capture its funding_address into <state-key>. keygen refuses to overwrite
#   an existing --out, so we skip when both the seed AND the captured address are
#   already present, and fail-closed (with recovery guidance) on a partial state
#   where the seed exists but its address was never recorded.
keygen_identity() {
    local base="${1:?basename}" key="${2:?state-key}" seed addr out
    seed="$PALW_DATA_ROOT/keys/$base.seed"

    if [ -f "$seed" ]; then
        addr="$(state_get "$key")"
        if [ -n "$addr" ]; then
            log "keygen: $base already present (seed + $key in state); skipping"
            return 0
        fi
        die "keygen: seed exists ($seed) but $key is missing from state — partial prior keygen. \
keygen refuses to overwrite an existing --out and the funding address cannot be re-derived here. \
Recover by restoring $key into $(state_file), or (closed no-value net ONLY) remove $seed to regenerate a fresh identity."
    fi

    log "keygen: generating $base identity -> $seed"
    out="$("$VAL" keygen --network "$NETWORK_BASE" --out "$seed" 2>&1)" \
        || die "keygen: '$VAL keygen --network $NETWORK_BASE' failed for $base: $out"
    chmod 0600 "$seed" 2>/dev/null || true

    # Prefer parsing by the configured address prefix (robust to label wording);
    # fall back to the funding_address key.
    addr="$(printf '%s\n' "$out" | grep -Eo "${ADDR_PREFIX}:[0-9a-z]+" | head -n1 || true)"
    [ -n "$addr" ] || addr="$(printf '%s\n' "$out" | _kv funding_address || true)"
    case "$addr" in
        ?*:?*) : ;;
        *) die "keygen: could not parse a funding address for $base. \
The seed was written ($seed) but its address is unknown — remove that seed to retry. Raw keygen output: $out" ;;
    esac

    state_set "$key" "$addr"          # state_set logs the key name only (never the seed)
    log "keygen: $base funding address captured into $key"
}

# ensure_supporting_miner  — make sure the persistent supporting miner is up,
#   paying the supporting identity's address. Starts it (supervised, NOT
#   cleanup-registered) if absent so this stage is self-sufficient.
ensure_supporting_miner() {
    local addr
    addr="$(state_get SUPPORTING_ADDR)"
    [ -n "$addr" ] || die "ensure_supporting_miner: SUPPORTING_ADDR unset — the keygen step must run first"

    if is_running "$SUPPORTING_MINER_NAME"; then
        log "supporting miner already running ($SUPPORTING_MINER_NAME)"
        return 0
    fi

    log "starting continuous supporting miner ($SUPPORTING_MINER_NAME) -> $addr"
    "$MINER" --pool "$(node_grpc a)" --network-id "$NETWORK" \
        --wallet "$addr" --worker "$MINER_WORKER" \
        --blocks 0 --min-block-interval-ms "$MINER_INTERVAL_MS" \
        >> "$(miner_log "$SUPPORTING_MINER_NAME")" 2>&1 &
    write_pid "$SUPPORTING_MINER_NAME" "$!"
    # NOT register_cleanup'd on purpose: this is a persistent service the later
    # stages (palw-submit inclusion) rely on; it must outlive this script.

    sleep "$GATE_POLL_SECS"
    is_running "$SUPPORTING_MINER_NAME" \
        || die "supporting miner ($SUPPORTING_MINER_NAME) exited immediately; see $(miner_log "$SUPPORTING_MINER_NAME")"
    log "supporting miner up ($SUPPORTING_MINER_NAME)"
}

# fund_identity <basename> <state-key>  — mine coinbase to the identity's address
#   until its balance >= FUND_TARGET_SOMPI. Idempotent: skips immediately if the
#   address is already at/above target (robust to partial prior runs).
fund_identity() {
    local base="${1:?basename}" key="${2:?state-key}" addr have rounds=0 name
    name="miner-fund-$base"
    addr="$(state_get "$key")"
    [ -n "$addr" ] || die "fund_identity: no address for $base ($key) — keygen step incomplete"

    while :; do
        have="$(balance_sompi "$addr")"
        case "$have" in ''|*[!0-9]*) have=0 ;; esac
        if [ "$have" -ge "$FUND_TARGET_SOMPI" ]; then
            log "fund: $base ($key) balance $have sompi >= target $FUND_TARGET_SOMPI; funded"
            return 0
        fi
        if [ "$rounds" -ge "$FUND_MAX_ROUNDS" ]; then
            die "fund: $base still $have sompi (< $FUND_TARGET_SOMPI) after $rounds rounds. \
Block subsidy too low, node A not producing, or node A missing --utxoindex. Check node-a.log and the supporting miner."
        fi
        log "fund: $base has $have sompi (< $FUND_TARGET_SOMPI); mining $FUND_BLOCKS_PER_ID block(s) -> $addr (round $(( rounds + 1 )))"
        mine_blocks "$name" "$addr" "$FUND_BLOCKS_PER_ID" "$FUND_INTERVAL_MS" \
            || die "fund: mining burst failed for $base (see $(miner_log "$name"))"
        rounds=$(( rounds + 1 ))
        sleep "$GATE_POLL_SECS"   # let node A's utxoindex settle before re-reading
    done
}

# mature_funding  — advance the chain so every funding coinbase is at least
#   coinbase_maturity (1000) DAA deep. Mines a finite buffer burst; additionally
#   asserts the sink DAA advanced by >= coinbase_maturity when that figure is
#   readable (pre-DNS it usually is not, so the script falls back to block count).
mature_funding() {
    local base_daa cur_daa depth
    base_daa="$(node_sink_daa a 2>/dev/null || true)"

    log "maturity: mining a $MATURITY_BUFFER_BLOCKS-block buffer -> supporting wallet (coinbase_maturity=$COINBASE_MATURITY DAA)"
    mine_blocks "miner-maturity" "$(state_get SUPPORTING_ADDR)" "$MATURITY_BUFFER_BLOCKS" "$MATURITY_INTERVAL_MS" \
        || die "maturity: buffer mining burst failed (see $(miner_log miner-maturity))"

    case "$base_daa" in
        ''|*[!0-9]*)
            warn "maturity: sink DAA not readable at this stage (expected pre-DNS); \
relying on the $MATURITY_BUFFER_BLOCKS-block buffer (>= coinbase_maturity $COINBASE_MATURITY + $MATURITY_MARGIN_BLOCKS margin)"
            return 0 ;;
    esac
    # The buffer burst is submitted near-instantly; node A ingests it into the sink
    # ASYNCHRONOUSLY and may be briefly unresponsive on wRPC while catching up. POLL for
    # the sink to advance >= coinbase_maturity rather than checking ONCE (a single
    # immediate read races the ingestion and reports "advanced 0").
    local deadline=$(( $(date +%s) + ${MATURITY_WAIT_SECS:-300} )) last=""
    while :; do
        cur_daa="$(node_sink_daa a 2>/dev/null || true)"
        case "$cur_daa" in
            ''|*[!0-9]*) : ;;                       # wRPC transiently unreadable (burst-busy) — keep polling
            *)
                depth=$(( cur_daa - base_daa )); last="$depth"
                if [ "$depth" -ge "$COINBASE_MATURITY" ]; then
                    log "maturity: sink DAA advanced $depth (>= coinbase_maturity $COINBASE_MATURITY); funding coinbases are spendable"
                    return 0
                fi
                ;;
        esac
        if [ "$(date +%s)" -ge "$deadline" ]; then
            case "$last" in
                '') warn "maturity: sink DAA stayed unreadable for ${MATURITY_WAIT_SECS:-300}s after the buffer; relying on the block-count buffer"; return 0 ;;
                *)  die "maturity: sink DAA advanced only $last (< coinbase_maturity $COINBASE_MATURITY) after ${MATURITY_WAIT_SECS:-300}s — node A is not ingesting the buffer fast enough (check $(node_log a); pace the burst with MATURITY_INTERVAL_MS)." ;;
            esac
        fi
        sleep 3
    done
}

# funds_all_at_target  — 0 iff every bonding identity address is already >= target.
funds_all_at_target() {
    local tok addr have
    for tok in $FUND_IDENTITIES; do
        addr="$(state_get "${tok##*:}")"
        [ -n "$addr" ] || return 1
        have="$(balance_sompi "$addr")"
        case "$have" in ''|*[!0-9]*) return 1 ;; esac
        [ "$have" -ge "$FUND_TARGET_SOMPI" ] || return 1
    done
    return 0
}

# verify_funding  — assert every bonding identity address covers the bond + fee.
verify_funding() {
    local tok base key addr have ok=1
    for tok in $FUND_IDENTITIES; do
        base="${tok%%:*}"; key="${tok##*:}"
        addr="$(state_get "$key")"
        have="$(balance_sompi "$addr")"
        case "$have" in ''|*[!0-9]*) have=0 ;; esac
        if [ "$have" -ge "$FUND_TARGET_SOMPI" ]; then
            log "verify: $base ($key) $have sompi >= target $FUND_TARGET_SOMPI (OK)"
        else
            warn "verify: $base ($key) $have sompi < target $FUND_TARGET_SOMPI (SHORT)"
            ok=0
        fi
    done
    [ "$ok" -eq 1 ] || die "verify: one or more identities under-funded (see WARN lines above)"
    log "verify: all $(printf '%s\n' "$FUND_IDENTITIES" | wc -w | tr -d ' ') bonding identities >= $FUND_TARGET_SOMPI sompi"
}

# -----------------------------------------------------------------------------
# main
# -----------------------------------------------------------------------------
main() {
    require_cmd awk grep date

    case "$NETWORK_BASE" in
        devnet) : ;;
        *) warn "NETWORK_BASE=$NETWORK_BASE: funding assumes instant (skip_proof_of_work) devnet blocks; \
under real PoW (e.g. testnet-110) mining $MATURITY_BUFFER_BLOCKS + funding blocks may be infeasible from this harness. Proceeding as requested." ;;
    esac

    log "STN-010(funding): bootstrapping funds for the closed two-node PALW testnet (NETWORK=$NETWORK, target=$FUND_TARGET_SOMPI sompi ~$(( FUND_TARGET_SOMPI / MSK_SOMPI )) MSK per identity)"

    # 1. Idempotent keygen for all five identities + capture funding addresses.
    local tok base key
    for tok in $ALL_IDENTITIES; do
        base="${tok%%:*}"; key="${tok##*:}"
        keygen_identity "$base" "$key"
    done

    # 2. Node A must answer RPC before we can mine / read balances.
    wait_rpc_up a || die "node-a wRPC is not up — start node-a.sh (and node-b.sh) first"

    # 3. Ensure the persistent supporting miner (chain liveness for later stages).
    ensure_supporting_miner

    # 4. Arm cleanup for the transient funding/maturity bursts (NOT the supporting miner).
    _register_burst_cleanup

    # 5. Fast path: a prior run already funded + matured everything -> re-verify only.
    if [ "$(state_get FUNDS_BOOTSTRAP_DONE)" = "1" ] && funds_all_at_target; then
        log "funds already bootstrapped (FUNDS_BOOTSTRAP_DONE=1 and every balance >= target); re-verifying only"
        verify_funding
        log "STN-010(funding): complete (idempotent no-op)"
        return 0
    fi

    # 6. Fund each bonding identity to >= target (idempotent per identity).
    for tok in $FUND_IDENTITIES; do
        base="${tok%%:*}"; key="${tok##*:}"
        fund_identity "$base" "$key"
    done

    # 7. Maturity buffer so every funding coinbase is >= coinbase_maturity deep.
    mature_funding

    # 8. Verify + record completion marker.
    verify_funding
    state_set FUNDS_BOOTSTRAP_DONE 1
    log "STN-010(funding): complete — bonding identities funded >= $FUND_TARGET_SOMPI sompi and matured >= $COINBASE_MATURITY DAA. \
Discovered addresses persisted to $(state_file)."
}

main "$@"
