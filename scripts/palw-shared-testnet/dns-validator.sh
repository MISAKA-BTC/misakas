#!/usr/bin/env bash
# =============================================================================
# dns-validator.sh — STN-009: stand up the closed testnet's DNS/beacon validator.
#
# Phase-0 wiring, closed no-value net. This stage, in order:
#
#   1. ensure the DNS validator seed FILE exists (kaspa-pq-validator keygen;
#      idempotent skip-if-present — keygen refuses to overwrite an --out that
#      already exists, so we never clobber a live key), and publish DNS_SEED +
#      the derived funding address DNS_ADDR into artifacts/state.env;
#   2. ensure that funding address is funded + matured (delegates the coinbase
#      maturity wait to bootstrap-funds.sh — devnet maturity is 1000 DAA — then
#      confirms the balance fail-closed);
#   3. post the DNS stake bond ONCE (10 MSK, activation-daa-score 0, unbonding
#      700 blocks), reusing an already-recorded DNS_BOND outpoint instead of
#      posting a second bond;
#   4. wait bond_status=active on BOTH nodes;
#   5. restart node A into IN-PROCESS validator/beacon mode via
#      restart-a-synced.sh, which drops --enable-unsynced-mining and adds
#      --enable-validator/--enable-beacon/--validator-mode=active with
#      --validator-key=$DNS_SEED --stake-bond=$DNS_BOND (invariant 5). Skipped
#      idempotently if node A is already running with --enable-validator;
#   6. wait dns_confirmed:true with an ADVANCING dns_anchor on BOTH nodes.
#
# Limitations:
#   * The bond is a REAL on-chain stake posted from the DNS funding key on a
#     closed, no-value devnet. This stage never touches the seeded test-only
#     palw_demo path.
#   * dns_health is liveness-only (it may read Degraded on a fresh net because
#     its trailing window averages empty pre-validator epochs). It is NOT a
#     consensus gate; wait_dns_confirmed deliberately ignores it and gates on
#     dns_confirmed:true + an advancing dns_anchor.
#   * DNS_SEED is only ever a FILE PATH on argv/logs — never the seed value.
#
# Design rules (same as common.sh): IDEMPOTENT, FAIL-CLOSED, PORTABLE (bash 3.2).
# All shared behaviour lives in common.sh and is CALLED, never reimplemented.
# =============================================================================

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="dns-validator"   # tag all log/warn/die lines from this stage.
load_env                       # source config + state.env, make dirs, bind bins.

require_cmd awk sed grep date

# DNS validator seed FILE path. Overridable via env/state (load_env overlays any
# recorded value from artifacts/state.env, keeping re-runs idempotent). Kept
# under keys/ at mode 0600; NEVER copied between hosts (invariant 5).
DNS_SEED="${DNS_SEED:-$PALW_DATA_ROOT/keys/dns-validator.seed}"

NODE_A_NAME="node-a"           # pid record name node-a.sh supervises (see node-a.sh).
RESTART_A="$SCRIPT_DIR/restart-a-synced.sh"
BOOTSTRAP="$SCRIPT_DIR/bootstrap-funds.sh"

# -----------------------------------------------------------------------------
# Local helpers. There is no common.sh gate for a stake-bond, nor a unit->sompi
# converter, so these two live here; both mirror the common.sh contracts (gates
# return 0 ok / non-zero+WARN on timeout; callers MUST check the return code).
# They reuse common.sh helpers (node_status, _kv) rather than reimplementing them.
# -----------------------------------------------------------------------------
# amount_to_sompi <amount>  — echo the sompi value of "<n>MSK|<n>KAS|<n>sompi|<n>".
#   1 MSK = 1 KAS = 1e8 sompi (verified: 10 MSK -> bond_amount 1000000000). Returns
#   non-zero (and echoes nothing) if <amount> is not an integer with a known unit.
amount_to_sompi() {
    local a="${1:-}" num unit
    num="$(printf '%s' "$a" | sed -E 's/[A-Za-z]+$//')"
    unit="$(printf '%s' "$a" | sed -E 's/^[0-9]+//')"
    case "$num" in ''|*[!0-9]*) return 1 ;; esac
    case "$unit" in
        MSK|msk|Msk|KAS|kas|Kas) printf '%s\n' "$(( num * 100000000 ))" ;;
        sompi|SOMPI|Sompi|'')    printf '%s\n' "$num" ;;
        *) return 1 ;;
    esac
}

# wait_bond_active <a|b> <txid:index> [timeout] [interval]  — poll VAL status
#   (with --stake-bond) until bond_status == active. _kv is token-boundary aware,
#   so "bond_status" is matched (never "status" inside it).
wait_bond_active() {
    local n="${1:?node}" outp="${2:?stake-bond outpoint}"
    local timeout="${3:-$GATE_TIMEOUT_SECS}" interval="${4:-$GATE_POLL_SECS}"
    local st deadline
    deadline=$(( $(date +%s) + timeout ))
    while :; do
        st="$(node_status "$n" "$outp" 2>/dev/null | _kv bond_status || true)"
        [ "$st" = "active" ] && { log "gate ok: node-$n bond_status active ($outp)"; return 0; }
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_bond_active node-$n timeout after ${timeout}s (last='$st')"; return 1; }
        sleep "$interval"
    done
}

# _validate_outpoint <txid:index>  — fail-closed shape check (mirrors node-a.sh).
_validate_outpoint() {
    local v="${1:-}" idx txid
    case "$v" in *:*) : ;; *) die "DNS_BOND must be <txid>:<index>, got '$v'" ;; esac
    idx="${v##*:}"; txid="${v%:*}"
    case "$idx" in ''|*[!0-9]*) die "DNS_BOND index must be numeric: '$v'" ;; esac
    [ -n "$txid" ] || die "DNS_BOND txid is empty: '$v'"
}

# -----------------------------------------------------------------------------
# Cleanup trap. This stage owns no long-lived process (node A's lifecycle belongs
# to node-a.sh / restart-a-synced.sh), so cleanup only removes a half-written
# temp seed if keygen is interrupted — otherwise a re-run would see a corrupt
# seed file and wrongly skip regeneration.
# -----------------------------------------------------------------------------
_DNS_SEED_TMP=""
register_cleanup 'if [ -n "${_DNS_SEED_TMP:-}" ] && [ -e "$_DNS_SEED_TMP" ]; then warn "removing partial DNS seed temp $_DNS_SEED_TMP"; rm -f "$_DNS_SEED_TMP"; fi'

log "STN-009: DNS/beacon validator bring-up starting (network=$NETWORK)"

# -----------------------------------------------------------------------------
# 0. preconditions. Both nodes must be up, synced, and share a sink before we
#    spend from the funding key — otherwise the bond could be built against a
#    stale UTXO set, or land on a view node B does not share.
# -----------------------------------------------------------------------------
wait_rpc_up a       || die "node-a wRPC not answering on $(node_wrpc a); start node-a.sh first"
wait_rpc_up b       || die "node-b wRPC not answering on $(node_wrpc b); start node-b.sh first"
wait_node_synced a  || die "node-a not synced; let the supporting chain (supporting-miner.sh) catch up"
wait_node_synced b  || die "node-b not synced; let the supporting chain catch up"
wait_same_sink      || die "nodes A and B do not share a sink; check P2P (--connect allowlist) and that supporting-miner.sh is producing blocks"

# -----------------------------------------------------------------------------
# 1. ensure the DNS validator seed exists (idempotent), publish DNS_SEED/DNS_ADDR.
# -----------------------------------------------------------------------------
install -d -m 0700 "$(dirname "$DNS_SEED")" || die "cannot create the DNS seed directory: $(dirname "$DNS_SEED")"
if [ -f "$DNS_SEED" ]; then
    log "DNS seed already present: $DNS_SEED (reusing; keygen skipped)"
    DNS_ADDR="$(state_get DNS_ADDR || true)"
    [ -n "$DNS_ADDR" ] || die "DNS seed exists at $DNS_SEED but DNS_ADDR is unknown in artifacts/state.env; a funding address cannot be re-derived from an existing seed (keygen refuses to overwrite). Recover by setting DNS_ADDR in env/state, or remove $DNS_SEED and re-run to regenerate (WARNING: a new seed means a new funding address and a new bond)."
else
    # keygen needs the BARE base name ($NETWORK_BASE, e.g. 'devnet'), NOT the full
    # $NETWORK id (see the keygen/full-id note in common.sh). It refuses to
    # overwrite an existing --out, so write to a temp path and mv it into place
    # atomically; the cleanup trap removes the temp on abort.
    _DNS_SEED_TMP="$DNS_SEED.tmp.$$"
    rm -f "$_DNS_SEED_TMP"
    log "generating DNS validator seed (keygen, network=$NETWORK_BASE)"
    ks_out="$("$VAL" keygen --network "$NETWORK_BASE" --out "$_DNS_SEED_TMP" 2>&1)" \
        || die "kaspa-pq-validator keygen failed: $ks_out"
    # keygen prints validator_id + funding_address (never the seed). The
    # label-anchored _kv parse is the primary, prefix-independent path; only if
    # that yields nothing do we fall back to grepping the configured address
    # prefix (guarded so an env file that omits ADDR_PREFIX can't trip set -u).
    DNS_ADDR="$(printf '%s\n' "$ks_out" | _kv funding_address || true)"
    if [ -z "$DNS_ADDR" ] && [ -n "${ADDR_PREFIX:-}" ]; then
        DNS_ADDR="$(printf '%s\n' "$ks_out" | grep -Eo "${ADDR_PREFIX}:[0-9a-z]+" | head -n1 || true)"
    fi
    [ -n "$DNS_ADDR" ] || die "could not parse funding_address from keygen output (expected a funding_address line, prefix '${ADDR_PREFIX:-<unset>}:')"
    chmod 0600 "$_DNS_SEED_TMP" 2>/dev/null || true
    [ -s "$_DNS_SEED_TMP" ] || die "keygen produced an empty seed file at $_DNS_SEED_TMP"
    mv "$_DNS_SEED_TMP" "$DNS_SEED" || die "could not move DNS seed into place: $DNS_SEED"
    _DNS_SEED_TMP=""
    log "DNS validator seed written: $DNS_SEED  funding_address=$DNS_ADDR"
fi
state_set DNS_SEED "$DNS_SEED"
state_set DNS_ADDR "$DNS_ADDR"

# -----------------------------------------------------------------------------
# 2. funded + matured. The stake bond aggregates MATURE UTXOs (coinbase maturity
#    = 1000 DAA) from the DNS funding address. The long maturity wait belongs to
#    bootstrap-funds.sh; run it here if present so this stage is self-contained,
#    then confirm the balance fail-closed. The funding address is already in
#    state.env (step 1) so the supporting miner can target it. This is a real
#    closed-net bond from the funding key — never the seeded palw_demo path.
# -----------------------------------------------------------------------------
# Invoked via `bash` (not `./`) so it works whether or not the execute bit is set.
if [ -f "$BOOTSTRAP" ]; then
    log "ensuring coinbase maturity for $DNS_ADDR via bootstrap-funds.sh"
    bash "$BOOTSTRAP" || die "bootstrap-funds.sh failed; the DNS funding address is not funded/matured yet"
else
    warn "bootstrap-funds.sh not found at $BOOTSTRAP; relying on prior funding and the balance check below"
fi

need_sompi="$(amount_to_sompi "$DNS_BOND_AMOUNT" || true)"
case "$need_sompi" in
    ''|*[!0-9]*) need_sompi=""; warn "could not parse DNS_BOND_AMOUNT='$DNS_BOND_AMOUNT' to sompi; requiring a non-zero balance only" ;;
esac

_fund_deadline=$(( $(date +%s) + GATE_TIMEOUT_SECS ))
while :; do
    # VAL balance prints "<addr>\t<sompi>\t<MSK> MSK"; node A must run --utxoindex.
    bal_out="$("$VAL" balance --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" --address "$DNS_ADDR" 2>/dev/null || true)"
    dns_sompi="$(printf '%s\n' "$bal_out" | awk -v a="$DNS_ADDR" '{for(i=1;i<=NF;i++) if($i==a){ if((i+1)<=NF) print $(i+1); exit }}')"
    case "$dns_sompi" in ''|*[!0-9]*) dns_sompi=0 ;; esac
    if [ -n "$need_sompi" ]; then
        [ "$dns_sompi" -ge "$need_sompi" ] && break
    else
        [ "$dns_sompi" -gt 0 ] && break
    fi
    [ "$(date +%s)" -ge "$_fund_deadline" ] && die "DNS funding address $DNS_ADDR has $dns_sompi sompi (need >= ${need_sompi:-1} for a $DNS_BOND_AMOUNT bond) after ${GATE_TIMEOUT_SECS}s. Point supporting-miner.sh at this address and run bootstrap-funds.sh to await coinbase maturity (1000 DAA)."
    sleep "$GATE_POLL_SECS"
done
log "DNS funding confirmed: $DNS_ADDR has $dns_sompi sompi (>= bond $DNS_BOND_AMOUNT)"

# -----------------------------------------------------------------------------
# 3. post the DNS stake bond (idempotent: reuse a recorded outpoint).
# -----------------------------------------------------------------------------
DNS_BOND="$(state_get DNS_BOND || true)"
if [ -n "$DNS_BOND" ]; then
    _validate_outpoint "$DNS_BOND"
    log "DNS_BOND already recorded: $DNS_BOND (reusing; no second bond will be posted)"
else
    # This is the FIRST spend from the DNS funding key, so there is no prior bond
    # outpoint to --exclude-funding-outpoint (invariant 4 applies only to LATER
    # funded commands). A supporting miner MUST be running for the bond tx to be
    # included before it can reach 'active'.
    log "posting DNS stake bond: amount=$DNS_BOND_AMOUNT activation-daa-score=0 unbonding-period-blocks=$UNBONDING_PERIOD_BLOCKS"
    bond_out="$("$VAL" bond \
        --node-wrpc-borsh "$(node_wrpc a)" \
        --network "$NETWORK" \
        --validator-key "$DNS_SEED" \
        --amount "$DNS_BOND_AMOUNT" \
        --activation-daa-score 0 \
        --unbonding-period-blocks "$UNBONDING_PERIOD_BLOCKS" 2>&1)" \
        || die "kaspa-pq-validator bond failed: $bond_out"
    DNS_BOND="$(printf '%s\n' "$bond_out" | _kv bond_outpoint || true)"
    _validate_outpoint "$DNS_BOND"
    state_set DNS_BOND "$DNS_BOND"
    log "DNS bond posted: $DNS_BOND"
fi

# -----------------------------------------------------------------------------
# 4. the bond must be active on BOTH nodes before node A enables the validator
#    (node A starts with --stake-bond=$DNS_BOND; it needs valid, propagated stake).
# -----------------------------------------------------------------------------
wait_bond_active a "$DNS_BOND" || die "node-a never saw bond $DNS_BOND reach active; ensure supporting-miner.sh is running (the bond tx needs a miner to be included). If the chain was reset, clear DNS_BOND from artifacts/state.env and re-run."
wait_bond_active b "$DNS_BOND" || die "node-b never saw bond $DNS_BOND reach active; check P2P propagation between the two nodes."
log "DNS bond active on BOTH nodes: $DNS_BOND"

# -----------------------------------------------------------------------------
# 5. restart node A into in-process validator/beacon mode (STN-006/-009).
#    restart-a-synced.sh stops node A, then relaunches it via node-a.sh with
#    NODE_A_MODE=validator, which reads DNS_SEED/DNS_BOND from state.env (both
#    persisted above) and starts --enable-validator/--enable-beacon. The
#    validator/beacon state (validator-state.json, beacon-secret.json) lives
#    under node A's appdir and must never be copied to a second live host.
#
#    Idempotent: if node A is already running WITH --enable-validator (recorded
#    argv, PID-reuse-safe via is_running), skip the disruptive restart.
# -----------------------------------------------------------------------------
if is_running "$NODE_A_NAME" && grep -qF -- '--enable-validator' "$(pid_file "$NODE_A_NAME")" 2>/dev/null; then
    log "node A already running in validator mode (recorded argv has --enable-validator); skipping restart"
else
    [ -f "$RESTART_A" ] || die "restart-a-synced.sh not found at $RESTART_A; cannot bring node A up in validator mode"
    log "restarting node A into validator/beacon mode via restart-a-synced.sh"
    # Invoked via `bash` (not `./`) so it works whether or not the execute bit is set.
    bash "$RESTART_A" || die "restart-a-synced.sh failed; node A did not come back up in validator mode"
fi
wait_rpc_up a || die "node-a wRPC did not return after the validator restart (check $(node_log a))"

# -----------------------------------------------------------------------------
# 6. DNS confirmed + anchor advancing on BOTH nodes. Gate on dns_confirmed:true
#    AND an advancing dns_anchor; dns_health is intentionally ignored (liveness
#    only — it may read Degraded on a fresh net; see wait_dns_confirmed).
# -----------------------------------------------------------------------------
wait_dns_confirmed a || die "node-a did not reach dns_confirmed:true with an advancing anchor within ${GATE_DNS_TIMEOUT_SECS}s; check node-a's log ($(node_log a)) for '[validator-service] ... beacon liveness ENABLED'"
wait_dns_confirmed b || die "node-b did not reach dns_confirmed:true with an advancing anchor within ${GATE_DNS_TIMEOUT_SECS}s; confirm B is synced to A and sharing the sink"

# 6b. TICKET_MODE=mock only: warm the PALW beacon to SUSTAINED Healthy BEFORE the
#     lifecycle registers its batch, so the batch's short active window [r+8,r+14)
#     overlaps a Healthy stretch and Certified->Active actually opens (PHASE0 G3/G4 —
#     the fix for the Certified->Expired stall). Skip mode never mints, so it does not
#     need this. Fail-soft (warn, not die) so the operator can still inspect / retry
#     with tuned pacing if Healthy is slow to form.
if [ "${TICKET_MODE:-skip}" = "mock" ]; then
    log "TICKET_MODE=mock: warming the PALW beacon to Healthy before the lifecycle (pace via KASPA_VALIDATOR_HEARTBEAT_SECS + MINER_INTERVAL_MS)."
    wait_palw_beacon_healthy a "${BEACON_HEALTHY_POLLS:-3}" "${BEACON_WARMUP_TIMEOUT:-${GATE_DNS_TIMEOUT_SECS:-600}}" \
        || warn "beacon did not reach sustained Healthy before the lifecycle — the batch may stall at Certified and Expire. Proceeding so you can inspect; if the mint never activates, slow MINER_INTERVAL_MS and/or shorten KASPA_VALIDATOR_HEARTBEAT_SECS and retry."
fi

log "STN-009 complete: DNS confirmed with an advancing anchor on BOTH nodes; DNS_SEED=$DNS_SEED DNS_BOND=$DNS_BOND"
exit 0
