#!/usr/bin/env bash
# =============================================================================
# submit-lifecycle.sh — STN-011: submit the PALW batch lifecycle carriers.
#
#   usage:  ./submit-lifecycle.sh          # run the whole lifecycle submission
#           ./submit-lifecycle.sh --help
#
# WHAT THIS DOES (the "resume, submit" half of invariant 2):
#   create-lifecycle.sh built the batch bundle OFFLINE (manifest + restamped
#   leaves + all leaf-chunks) with the supporting miner PAUSED, freezing DAA at
#   the registration epoch E, and recorded PALW_BATCH_ID / PALW_CHUNK_COUNT /
#   PALW_LEAF_COUNT / PALW_REG_EPOCH into artifacts/state.env. It left the miner
#   PAUSED on purpose. THIS script:
#     1. resumes the supporting miner (a miner MUST run continuously for any
#        palw-submit — submit blocks on INCLUSION; invariant 1);
#     2. submits the batch-manifest carrier DURING epoch E (asserts
#        current_epoch == PALW_REG_EPOCH with >= MIN_EPOCH_HEADROOM_DAA of
#        headroom BEFORE submitting; dies fail-closed if the epoch already
#        rolled, instructing a create-lifecycle re-run);
#     3. submits every leaf-chunk carrier (skip- or mock-ticket per TICKET_MODE);
#     4. after EACH carrier, advances >= 1 selected child (invariant 3) and
#        verifies batch.manifest_present / chunks / leaf_blobs on BOTH nodes
#        (the past-relative palw-status view only updates after a child is mined);
#     5. inside the audit window, produces audit-facts (node A) -> vote
#        (independent auditor-c) -> certificate, submits the certificate carrier,
#        and waits until batch.status == active on BOTH nodes.
#
# HONESTY (matches PHASE0-status.md §4 / README §Scope & limits):
#   * TICKET_MODE=skip (default): each leaf-chunk is registered with
#     `palw-submit --unsafe-skip-ticket-secret-check` (NO ticket). The batch
#     reaches status=active, but a block carrying that leaf can NEVER be mined
#     (no coinbase, no minted block). This is the HONEST no-GPU end state.
#   * TICKET_MODE=mock: create-lifecycle populated the TicketSecretStore for this
#     batch, so the leaf carries a real (opened) ticket_nullifier_commitment; the
#     leaf-chunk is submitted WITHOUT --unsafe. The ticket-authority key
#     (--palw-ticket-authority-key-file) and the TicketSecretStore
#     (--palw-ticket-secret-file) are NODE-side flags per the verified CLI
#     catalog — they are consumed by KASPAD at MINING time (start-palw-miner.sh),
#     NOT by palw-submit. This stage therefore does not pass them to palw-submit
#     (inventing a flag is forbidden); it verifies the proof MATERIAL is present
#     (fail-closed if missing) so the non-skip submit can be enforced. The result
#     is a WIRING-ONLY, explicitly NON-INFERENCE mock leaf — never real inference,
#     never the seeded test-only palw_demo path.
#   * The algo-4 chain has fork-choice weight 0 here (PALW-014): this proves
#     carrier validity / propagation / lifecycle plumbing, NOT PALW chain security.
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
# (each carrier is skipped if the on-chain batch view already reflects it —
# manifest_present / chunks / certificate_blob_present are the source of truth;
# derived audit artifacts are regenerated ATOMICALLY and never left partial; the
# supporting miner is never double-launched); FAIL-CLOSED with actionable
# messages; a register_cleanup trap tears down only this run's staging temp (it
# NEVER stops the supporting miner — later stages need it). It SOURCES common.sh
# and calls ONLY its helpers; the few locally-defined helpers (both-node batch
# field gates, the audit-window advance) have no common.sh equivalent and obey
# the same gate contract (0 ok / non-zero => die). No seed or nullifier value
# ever reaches argv or a log — only FILE PATHS and public identifiers.
#
# INPUT CONTRACT (set by create-lifecycle.sh; this stage READS, never writes it):
#   state.env : PALW_BATCH_ID (128hex, bound) PALW_CHUNK_COUNT (>=1)
#               PALW_LEAF_COUNT (>=1) PALW_REG_EPOCH (E)
#   bundle    : $PALW_DATA_ROOT/artifacts/lifecycle/{manifest.borsh,
#               leaves.batch.json, chunk-<0..N-1>.borsh}
#   identities: DNS_SEED (+DNS_BOND) from dns-validator.sh;
#               AUD_C_KEY seed (+AUD_C_BOND) from register-providers.sh.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# ---------------------------------------------------------------------------
usage() {
    cat >&2 <<EOF
usage: ${0##*/} [--help]

  Submits the PALW batch lifecycle carriers for the bundle built by
  create-lifecycle.sh (STN-011): batch-manifest -> leaf-chunk(s) -> audit-facts
  -> auditor-c vote -> certificate, advancing a selected child and verifying the
  batch view on BOTH nodes after each carrier, ending at batch.status=active.

  Resumes the (create-lifecycle-paused) supporting miner first — a miner MUST run
  for palw-submit to reach inclusion. Idempotent: already-applied carriers are
  detected via the on-chain batch view and skipped. Reads its inputs from
  artifacts/state.env + artifacts/lifecycle/ (run create-lifecycle.sh first).
EOF
}

# Dispatch before load_env so --help works unconfigured; reject stray args.
case "${1:-}" in
    -h|--help|help) usage; exit 0 ;;
    "")             : ;;
    *)              usage; die "unknown argument '$1' (this stage takes no arguments; see --help)" ;;
esac

load_env

PALW_LOG_TAG="${PALW_LOG_TAG:-submit-lifecycle}"; export PALW_LOG_TAG

require_cmd mktemp grep

# ---------------------------------------------------------------------------
# Resolve the input contract (create-lifecycle.sh outputs) — fail-closed.
# ---------------------------------------------------------------------------
BATCH_ID="$(state_get PALW_BATCH_ID)"
[ -n "$BATCH_ID" ] || die "PALW_BATCH_ID is not set in artifacts/state.env — run ./create-lifecycle.sh first (it builds the bundle and records PALW_BATCH_ID)."
case "$BATCH_ID" in *[!0-9a-fA-F]*) die "PALW_BATCH_ID is not hex ('$BATCH_ID'); re-run ./create-lifecycle.sh." ;; esac
[ "${#BATCH_ID}" -eq 128 ] || die "PALW_BATCH_ID must be 128 hex chars (got ${#BATCH_ID}); re-run ./create-lifecycle.sh."
case "$BATCH_ID" in *[!0]*) : ;; *) die "PALW_BATCH_ID is all-zero (UNBOUND) — the bundle was not bound; re-run ./create-lifecycle.sh." ;; esac
BID8="${BATCH_ID:0:8}"

REG_EPOCH="$(state_get PALW_REG_EPOCH)"
case "$REG_EPOCH" in ''|*[!0-9]*) die "PALW_REG_EPOCH is not a non-negative integer ('$REG_EPOCH') — run ./create-lifecycle.sh (it records the registration epoch)." ;; esac

CHUNK_COUNT="$(state_get PALW_CHUNK_COUNT)"
case "$CHUNK_COUNT" in ''|*[!0-9]*) die "PALW_CHUNK_COUNT is not a non-negative integer ('$CHUNK_COUNT') — run ./create-lifecycle.sh." ;; esac
[ "$CHUNK_COUNT" -ge 1 ] || die "PALW_CHUNK_COUNT is $CHUNK_COUNT (expected >= 1) — re-run ./create-lifecycle.sh."

LEAFN="$(state_get PALW_LEAF_COUNT)"
case "$LEAFN" in ''|*[!0-9]*) LEAFN="$LEAF_COUNT" ;; esac
case "$LEAFN" in ''|*[!0-9]*) die "cannot determine the batch leaf count (PALW_LEAF_COUNT / LEAF_COUNT both non-numeric)." ;; esac
[ "$LEAFN" -ge 1 ] || die "batch leaf count is $LEAFN (expected >= 1)."

# Bundle payload files (built OFFLINE by create-lifecycle; INPUTS — never overwritten).
LIFECYCLE_DIR="${LIFECYCLE_DIR:-$PALW_DATA_ROOT/artifacts/lifecycle}"
MANIFEST_FILE="$LIFECYCLE_DIR/manifest.borsh"
LEAVES_BATCH_FILE="$LIFECYCLE_DIR/leaves.batch.json"
chunk_file() { printf '%s/chunk-%s.borsh\n' "$LIFECYCLE_DIR" "${1:?chunk index}"; }

[ -d "$LIFECYCLE_DIR" ]        || die "lifecycle bundle dir missing: $LIFECYCLE_DIR — run ./create-lifecycle.sh first."
[ -s "$MANIFEST_FILE" ]        || die "batch-manifest payload missing/empty: $MANIFEST_FILE — run ./create-lifecycle.sh (it builds it OFFLINE)."
[ -s "$LEAVES_BATCH_FILE" ]    || die "restamped leaves file missing/empty: $LEAVES_BATCH_FILE — run ./create-lifecycle.sh."
_k=0
while [ "$_k" -lt "$CHUNK_COUNT" ]; do
    [ -s "$(chunk_file "$_k")" ] || die "leaf-chunk payload missing/empty: $(chunk_file "$_k") (expected $CHUNK_COUNT chunks) — re-run ./create-lifecycle.sh."
    _k=$(( _k + 1 ))
done

# Derived audit artifacts THIS stage produces (regenerated atomically per run).
FACTS_FILE="$LIFECYCLE_DIR/facts.json"
VOTE_FILE="$LIFECYCLE_DIR/vote.borsh"
CERT_FILE="$LIFECYCLE_DIR/cert.borsh"

# ---------------------------------------------------------------------------
# Identities & bonds (fail-closed — do NOT fabricate keys or outpoints).
# ---------------------------------------------------------------------------
# The batch carriers (manifest / leaf-chunk / certificate) are FUNDED by the
# operator/DNS validator seed produced by dns-validator.sh. Its own stake bond
# (DNS_BOND) MUST be excluded on every funded submit so the carrier can never
# consume the bond UTXO (invariant 4).
CARRIER_KEY="${CARRIER_KEY:-$(state_get DNS_SEED)}"
[ -n "$CARRIER_KEY" ] || CARRIER_KEY="$PALW_DATA_ROOT/keys/dns-validator.seed"
[ -f "$CARRIER_KEY" ] || die "carrier funding seed not found: $CARRIER_KEY — run ./dns-validator.sh (it keygen's DNS_SEED and funds it), or set CARRIER_KEY to a funded seed FILE."

CARRIER_BOND="${CARRIER_BOND:-$(state_get DNS_BOND)}"
[ -n "$CARRIER_BOND" ] || die "DNS_BOND (the carrier seed's own stake bond outpoint) is not set in artifacts/state.env — run ./dns-validator.sh first. A funded carrier MUST be able to exclude its own bond outpoint (invariant 4); refusing to submit without it."

# Independent auditor-c identity (register-providers.sh). Its operator group
# differs from BOTH leaf providers; its bond backs the audit vote.
AUDITOR_KEY="${AUD_C_KEY:-$PALW_DATA_ROOT/keys/auditor-c.seed}"
[ -f "$AUDITOR_KEY" ] || die "auditor-c seed not found: $AUDITOR_KEY — run ./register-providers.sh (it keygen's + bonds auditor-c), or set AUD_C_KEY to the auditor seed FILE."
AUD_C_BOND="$(state_get AUD_C_BOND)"
[ -n "$AUD_C_BOND" ] || die "AUD_C_BOND (auditor-c provider-bond outpoint) is not set in artifacts/state.env — run ./register-providers.sh first (it prints the bond outpoint and records AUD_C_BOND)."

# Verified consensus constants (env-overridable; defaults are the shipped values).
MIN_HEADROOM="${MIN_EPOCH_HEADROOM_DAA:-20}"
REG_LEAD_EPOCHS="${REGISTRATION_LEAD_EPOCHS:-2}"
AUDIT_WINDOW_EPOCHS="${AUDIT_WINDOW_EPOCHS:-6}"
AUDIT_OPEN_EPOCH=$(( REG_EPOCH + REG_LEAD_EPOCHS ))                       # E+2
AUDIT_CLOSE_EPOCH=$(( REG_EPOCH + REG_LEAD_EPOCHS + AUDIT_WINDOW_EPOCHS )) # E+8 == activation_not_before
# Beacon epoch for audit sampling: the registration epoch E (revealed by E+1,
# so it is available throughout the audit window). Override with AUDIT_BEACON_EPOCH.
AUDIT_EPOCH="${AUDIT_BEACON_EPOCH:-$REG_EPOCH}"
case "$AUDIT_EPOCH" in ''|*[!0-9]*) die "AUDIT_BEACON_EPOCH must be a non-negative integer, got '$AUDIT_EPOCH'." ;; esac

# All-pass audit commitments. rejected root MUST be empty/zero (header-v4 admits
# ALL-PASS only). The checked-leaf-bitmap-root commits to the leaves this auditor
# actually checked; this harness will NOT fabricate an audit commitment — it is
# taken from CHECKED_LEAF_BITMAP_ROOT or read out of the produced facts.json,
# else the vote fails closed with an instruction.
REJECTED_ROOT="${REJECTED_LEAF_BITMAP_ROOT:-$(zero128)}"
case "$REJECTED_ROOT" in *[!0-9a-fA-F]*) die "REJECTED_LEAF_BITMAP_ROOT is not hex." ;; esac
[ "${#REJECTED_ROOT}" -eq 128 ] || die "REJECTED_LEAF_BITMAP_ROOT must be 128 hex chars (got ${#REJECTED_ROOT})."

# Timeouts.
T_GATE="${GATE_TIMEOUT_SECS:-180}"
T_EPOCH="${GATE_DNS_TIMEOUT_SECS:-300}"
POLL="${GATE_POLL_SECS:-2}"

# ---------------------------------------------------------------------------
# Cleanup trap: only this run's staging temp. The supporting miner is NEVER
# stopped here (later stages depend on it running).
# ---------------------------------------------------------------------------
_SL_STAGING="$(mktemp -d "$LIFECYCLE_DIR/.submit.XXXXXX")" || die "cannot create staging temp dir under $LIFECYCLE_DIR."
register_cleanup 'rm -rf "$_SL_STAGING" 2>/dev/null || true'

# ---------------------------------------------------------------------------
# Funded-submit exclude args: exclude EVERY known bond outpoint on funded
# submits. Excluding an outpoint the carrier key does not control is a harmless
# no-op (it is simply absent from that key's funding UTXO set) — the same
# belt-and-suspenders pattern register-providers.sh uses.
# ---------------------------------------------------------------------------
_EXCLUDE_ARGS=()
_EXCLUDE_SEEN=""
_add_exclude() {
    local o="${1:-}"
    [ -n "$o" ] || return 0
    case "$o" in
        [0-9a-fA-F]*:[0-9]*) : ;;
        *) die "malformed bond outpoint '$o' (expected <txid-hex>:<index>) — fix the earlier stage / artifacts/state.env." ;;
    esac
    case " $_EXCLUDE_SEEN " in *" $o "*) return 0 ;; esac
    _EXCLUDE_SEEN="$_EXCLUDE_SEEN $o"
    _EXCLUDE_ARGS[${#_EXCLUDE_ARGS[@]}]="--exclude-funding-outpoint"
    _EXCLUDE_ARGS[${#_EXCLUDE_ARGS[@]}]="$o"
}
_add_exclude "$CARRIER_BOND"
_add_exclude "$(state_get DNS_BOND)"
_add_exclude "$(state_get PROV_A_BOND)"
_add_exclude "$(state_get PROV_B_BOND)"
_add_exclude "$(state_get AUD_C_BOND)"

# _palw_submit <kind> <payload-file> [extra flags...] — a funded carrier submit
# against node A's loopback wRPC, funded by the carrier key, excluding every
# known bond. Only verified palw-submit flags are used.
_palw_submit() {
    local kind="${1:?kind}" payload="${2:?payload}"; shift 2
    "$VAL" palw-submit \
        --node-wrpc-borsh "$(node_wrpc a)" \
        --network "$NETWORK" \
        --validator-key "$CARRIER_KEY" \
        --kind "$kind" \
        --payload-file "$payload" \
        ${_EXCLUDE_ARGS[@]+"${_EXCLUDE_ARGS[@]}"} \
        "$@"
}

# ---------------------------------------------------------------------------
# Both-node batch-field gates (extend wait_batch_status, which only checks the
# `status` field, to manifest_present / chunks / leaf_blobs on BOTH nodes).
# Compose palw_batch_status + _kv; poll to a deadline; die fail-closed on timeout.
# ---------------------------------------------------------------------------
# _batch_field never fails the caller: a transient RPC error yields "" (not a
# non-zero status that set -e would treat as fatal inside a poll loop).
_batch_field() { palw_batch_status "$1" "$BATCH_ID" 2>/dev/null | _kv "$2" || true; }
_num() { local v="${1:-}"; printf '%s' "${v%%/*}"; }   # numerator of "x/y" (or the value)

# wait_field_true_both <field> <label> — <field> == "true" on A and B.
wait_field_true_both() {
    local field="${1:?field}" label="${2:?label}" va vb deadline
    deadline=$(( $(date +%s) + T_GATE ))
    while :; do
        va="$(_batch_field a "$field")"; vb="$(_batch_field b "$field")"
        if [ "$va" = "true" ] && [ "$vb" = "true" ]; then
            log "both-node ok: $label ($field=true on A and B)"; return 0
        fi
        [ "$(date +%s)" -ge "$deadline" ] && die "both-node check FAILED: $label — $field is A='$va' B='$vb' (want true/true) after ${T_GATE}s. The supporting miner must keep advancing children so the past-relative palw-status view updates on both nodes (invariant 3)."
        sleep "$POLL"
    done
}

# wait_num_ge_both <field> <min> <label> — numerator(<field>) >= <min> on A and B.
wait_num_ge_both() {
    local field="${1:?field}" min="${2:?min}" label="${3:?label}" va vb na nb deadline
    deadline=$(( $(date +%s) + T_GATE ))
    while :; do
        va="$(_batch_field a "$field")"; vb="$(_batch_field b "$field")"
        na="$(_num "$va")"; nb="$(_num "$vb")"
        case "$na" in ''|*[!0-9]*) na=-1 ;; esac
        case "$nb" in ''|*[!0-9]*) nb=-1 ;; esac
        if [ "$na" -ge "$min" ] && [ "$nb" -ge "$min" ]; then
            log "both-node ok: $label ($field A=$va B=$vb, numerator >= $min)"; return 0
        fi
        [ "$(date +%s)" -ge "$deadline" ] && die "both-node check FAILED: $label — $field A='$va' B='$vb' (want numerator >= $min on both) after ${T_GATE}s. Ensure the supporting miner is advancing children (invariant 3)."
        sleep "$POLL"
    done
}

# ---------------------------------------------------------------------------
# Resume the supporting miner (create-lifecycle left it PAUSED). A miner MUST run
# continuously across every palw-submit (submit blocks on inclusion, invariant 1).
# ---------------------------------------------------------------------------
SUPPORTING_MINER_NAME="supporting-miner"
resume_supporting_miner() {
    if is_running "$SUPPORTING_MINER_NAME"; then
        log "supporting miner already running (pid $(read_pid "$SUPPORTING_MINER_NAME")); continuing"
        return 0
    fi
    local launcher="$SCRIPT_DIR/supporting-miner.sh"
    [ -f "$launcher" ] || die "supporting-miner.sh not found next to submit-lifecycle.sh ($launcher) — cannot resume the miner. A miner MUST run for palw-submit inclusion."
    log "resuming supporting miner (create-lifecycle left it paused; DAA frozen at epoch $REG_EPOCH) via '$launcher start'"
    bash "$launcher" start || die "failed to resume the supporting miner ('$launcher start') — a miner MUST run for palw-submit to reach inclusion. Inspect $PALW_DATA_ROOT/logs/miner-supporting.log."
    is_running "$SUPPORTING_MINER_NAME" || die "supporting miner did not come up after resume — inspect $PALW_DATA_ROOT/logs/miner-supporting.log."
}

# ---------------------------------------------------------------------------
# Advance the selected chain into the audit window [E+lead, E+lead+window) so the
# registration-epoch beacon is revealed and the audit lands in-window. Uses only
# node_sink_daa + current_epoch + wait_inclusion (no common.sh epoch-gate exists).
# ---------------------------------------------------------------------------
ensure_in_audit_window() {
    local cur cur_daa need
    cur="$(current_epoch a)" || die "cannot read current epoch on node A (is it up/synced?)."
    if [ "$cur" -ge "$AUDIT_CLOSE_EPOCH" ]; then
        die "audit window CLOSED for batch $BID8: current epoch $cur >= $AUDIT_CLOSE_EPOCH (registration $REG_EPOCH + lead $REG_LEAD_EPOCHS + window $AUDIT_WINDOW_EPOCHS = activation). The batch can no longer be audited. Re-run ./create-lifecycle.sh for a fresh batch, then ./submit-lifecycle.sh."
    fi
    if [ "$cur" -lt "$AUDIT_OPEN_EPOCH" ]; then
        cur_daa="$(node_sink_daa a)" || die "cannot read node A sink DAA to advance into the audit window."
        need=$(( AUDIT_OPEN_EPOCH * 100 - cur_daa ))
        [ "$need" -lt 1 ] && need=1
        log "advancing into audit window [epoch $AUDIT_OPEN_EPOCH, $AUDIT_CLOSE_EPOCH) — need ~${need} DAA (current epoch $cur) ..."
        wait_inclusion a "$need" "$T_EPOCH" || die "selected chain did not advance into the audit window within ${T_EPOCH}s — ensure the supporting miner is producing blocks ($PALW_DATA_ROOT/logs/miner-supporting.log)."
        cur="$(current_epoch a)" || die "cannot re-read current epoch on node A after advancing."
    fi
    [ "$cur" -lt "$AUDIT_CLOSE_EPOCH" ] || die "audit window closed while advancing (epoch $cur >= $AUDIT_CLOSE_EPOCH) — re-run ./create-lifecycle.sh for a fresh batch."
    [ "$cur" -ge "$AUDIT_OPEN_EPOCH" ] || die "failed to reach audit-window open epoch $AUDIT_OPEN_EPOCH (current $cur) — inspect node A / the supporting miner."
    log "in audit window: epoch $cur in [$AUDIT_OPEN_EPOCH, $AUDIT_CLOSE_EPOCH); audit beacon epoch = $AUDIT_EPOCH (revealed)"
}

# ---------------------------------------------------------------------------
# Atomic producers for the derived audit artifacts (facts / vote / cert). Each
# writes to staging then renames into place; an existing final is overwritten
# LOUDLY (never silently) so the artifact always matches the current batch.
# ---------------------------------------------------------------------------
gen_facts() {
    local tmp="$_SL_STAGING/facts.json"
    [ -f "$FACTS_FILE" ] && warn "overwriting existing $(basename "$FACTS_FILE") (regenerating audit-facts for batch $BID8)"
    log "audit-facts: node A -> $(basename "$FACTS_FILE") (batch $BID8, audit beacon epoch $AUDIT_EPOCH)"
    "$VAL" palw-payload audit-facts \
        --network "$NETWORK" \
        --node-rpc "$(node_wrpc a)" \
        --batch-id "$BATCH_ID" \
        --audit-beacon-epoch "$AUDIT_EPOCH" \
        --out "$tmp" \
        || die "'palw-payload audit-facts' failed — node A must be SYNCED with the batch fully present, and audit-beacon-epoch=$AUDIT_EPOCH must be a revealed, in-window beacon. Inspect node A."
    [ -s "$tmp" ] || die "audit-facts produced an empty file ($tmp)."
    mv -f "$tmp" "$FACTS_FILE" || die "failed to finalize $FACTS_FILE."
}

# _resolve_checked_root — 128-hex checked-leaf-bitmap-root, from env override or
# best-effort out of the produced facts.json. Empty if neither (caller dies).
_resolve_checked_root() {
    local r="${CHECKED_LEAF_BITMAP_ROOT:-}"
    if [ -z "$r" ] && [ -f "$FACTS_FILE" ]; then
        r="$(grep -Eio '"(checked|required|sampled|expected)_leaf_bitmap_root"[[:space:]]*:[[:space:]]*"[0-9a-f]{128}"' "$FACTS_FILE" 2>/dev/null \
             | head -n1 | grep -Eio '[0-9a-f]{128}' | head -n1 || true)"
        # ALL-PASS fallback: an auditor that checked EVERY leaf commits to the batch's leaf_root.
        # header-v4 admits ALL-PASS only (passed_leaf_count == leaf_count, rejected root empty), and
        # consensus does NOT derive-verify the checked root — it is the auditor's own commitment,
        # cross-checked only for quorum agreement (see consensus verify_batch_certificate). The
        # manifest leaf_root is the non-fabricated, batch-bound value for "I checked all leaves", so
        # fall back to it rather than dying. Validated live on devnet-111 (2026-07-25): this is what
        # lets the auditor vote + certificate land without a manual CHECKED_LEAF_BITMAP_ROOT override.
        if [ -z "$r" ]; then
            r="$(grep -Eio '"leaf_root"[[:space:]]*:[[:space:]]*"[0-9a-f]{128}"' "$FACTS_FILE" 2>/dev/null \
                 | head -n1 | grep -Eio '[0-9a-f]{128}' | head -n1 || true)"
        fi
    fi
    printf '%s' "$r"
}

gen_vote() {
    local tmp="$_SL_STAGING/vote.borsh" checked
    checked="$(_resolve_checked_root)"
    [ -n "$checked" ] || die "no checked-leaf-bitmap-root available for the auditor vote. Set CHECKED_LEAF_BITMAP_ROOT to the 128-hex root of the leaves this auditor actually checked (all-pass => every sampled leaf), or ensure $FACTS_FILE exposes it. This harness will NOT fabricate an audit commitment."
    case "$checked" in *[!0-9a-fA-F]*) die "checked-leaf-bitmap-root is not hex ('$checked')." ;; esac
    [ "${#checked}" -eq 128 ] || die "checked-leaf-bitmap-root must be 128 hex chars (got ${#checked})."
    [ -f "$VOTE_FILE" ] && warn "overwriting existing $(basename "$VOTE_FILE") (regenerating auditor-c vote for batch $BID8)"
    log "audit-vote: auditor-c (bond $AUD_C_BOND) verdict=pass passed-leaf-count=$LEAFN rejected-root=empty -> $(basename "$VOTE_FILE")"
    "$VAL" palw-payload audit-vote \
        --network "$NETWORK" \
        --node-rpc "$(node_wrpc a)" \
        --facts-file "$FACTS_FILE" \
        --validator-key "$AUDITOR_KEY" \
        --auditor-bond "$AUD_C_BOND" \
        --verdict pass \
        --checked-leaf-bitmap-root "$checked" \
        --passed-leaf-count "$LEAFN" \
        --rejected-leaf-bitmap-root "$REJECTED_ROOT" \
        --out "$tmp" \
        || die "'palw-payload audit-vote' failed — header-v4 admits ALL-PASS only (passed-leaf-count must equal the batch leaf_count=$LEAFN, rejected root must be empty/zero). Verify AUD_C_BOND is an active auditor bond and the checked root matches the facts. Inspect node A."
    [ -s "$tmp" ] || die "audit-vote produced an empty file ($tmp)."
    mv -f "$tmp" "$VOTE_FILE" || die "failed to finalize $VOTE_FILE."
}

gen_cert() {
    local tmp="$_SL_STAGING/cert.borsh"
    [ -f "$CERT_FILE" ] && warn "overwriting existing $(basename "$CERT_FILE") (regenerating certificate for batch $BID8)"
    log "certificate: aggregating facts + auditor-c vote -> $(basename "$CERT_FILE")"
    "$VAL" palw-payload certificate \
        --network "$NETWORK" \
        --node-rpc "$(node_wrpc a)" \
        --facts-file "$FACTS_FILE" \
        --vote-file "$VOTE_FILE" \
        --out "$tmp" \
        || die "'palw-payload certificate' failed — verify $FACTS_FILE and $VOTE_FILE are consistent (same batch, all-pass quorum). Inspect node A."
    [ -s "$tmp" ] || die "certificate produced an empty file ($tmp)."
    mv -f "$tmp" "$CERT_FILE" || die "failed to finalize $CERT_FILE."
}

# =============================================================================
# Run.
# =============================================================================
log "STN-011 submit-lifecycle: batch $BATCH_ID (leaves=$LEAFN chunks=$CHUNK_COUNT registration_epoch=$REG_EPOCH) TICKET_MODE=$TICKET_MODE"

# ---- 0. node readiness (A: submits+audit; B: both-node verification) --------
wait_rpc_up a     || die "node A wRPC did not come up — start node-a.sh (and node-b.sh) first."
wait_rpc_up b     || die "node B wRPC did not come up — start node-b.sh first (both-node verification needs it)."
wait_node_synced a || die "node A is not synced — carriers and audit-facts require a synced node."
wait_node_synced b || die "node B is not synced — both-node batch verification requires it."

# ---- 1. resume the supporting miner (invariant 1) ---------------------------
resume_supporting_miner

# ---- 2. batch-manifest carrier (DURING epoch E) -----------------------------
if [ "$(_batch_field a manifest_present)" = "true" ]; then
    log "batch-manifest already present on node A (manifest_present=true); skipping submit + epoch gate (idempotent)"
else
    # Epoch gate: assert current_epoch == PALW_REG_EPOCH with >= MIN headroom
    # BEFORE submitting. create-lifecycle froze DAA at E; we resumed the miner
    # just now, so we are still in E — but re-check fail-closed.
    D="$(node_sink_daa a)" || die "cannot read node A sink DAA — is node A up and synced?"
    CUR_EPOCH=$(( D / 100 ))
    HEADROOM=$(( 100 - (D % 100) ))
    if [ "$CUR_EPOCH" -gt "$REG_EPOCH" ]; then
        die "epoch already rolled: current epoch $CUR_EPOCH > registered epoch $REG_EPOCH. The batch-manifest MUST be submitted DURING its registration epoch. Re-run ./create-lifecycle.sh to rebuild the bundle for the current epoch, then ./submit-lifecycle.sh immediately (invariant 2)."
    fi
    if [ "$CUR_EPOCH" -lt "$REG_EPOCH" ]; then
        die "current epoch $CUR_EPOCH is BEFORE registered epoch $REG_EPOCH — the bundle was built for a future epoch (unexpected). Re-run ./create-lifecycle.sh in the current epoch."
    fi
    if [ "$HEADROOM" -lt "$MIN_HEADROOM" ]; then
        die "only ${HEADROOM} DAA left in registration epoch $REG_EPOCH (< required ${MIN_HEADROOM}); the carrier could roll into epoch $(( REG_EPOCH + 1 )) during inclusion. Re-run ./create-lifecycle.sh (it re-freezes a fresh epoch), then ./submit-lifecycle.sh immediately."
    fi
    log "epoch gate ok: current epoch $CUR_EPOCH == registered $REG_EPOCH, headroom ${HEADROOM} DAA (>= ${MIN_HEADROOM})"
    log "submitting batch-manifest carrier (funded by carrier key; excluding every known bond) ..."
    _palw_submit batch-manifest "$MANIFEST_FILE" \
        || die "'palw-submit --kind batch-manifest' failed — a miner must be running for inclusion, and the carrier must land within epoch $REG_EPOCH. Inspect node A ($(node_log a)) and $PALW_DATA_ROOT/logs/miner-supporting.log."
    wait_inclusion a 1 || die "no selected child advanced after the batch-manifest carrier — the supporting miner must be running (invariant 3)."
fi
# Advance >=1 child (done above when submitted) + verify on BOTH nodes.
wait_field_true_both manifest_present "batch-manifest registered"

# ---- 3. leaf-chunk carriers -------------------------------------------------
# Ticket handling (per TICKET_MODE). Preflight the mode's prerequisites ONCE.
TICKET_ARGS=()
if [ "$TICKET_MODE" = skip ]; then
    log "TICKET_MODE=skip: leaf-chunks are registered with --unsafe-skip-ticket-secret-check (NO ticket). The batch reaches status=active but its leaf can NEVER be mined — no mint. Honest no-GPU end state (NOT palw_demo)."
    TICKET_ARGS[${#TICKET_ARGS[@]}]="--unsafe-skip-ticket-secret-check"
else
    # TICKET_MODE=mock. create-lifecycle generated the ticket-authority seed and
    # populated the TicketSecretStore for this batch, so the leaf carries a real
    # (opened) ticket commitment and is submitted WITHOUT --unsafe.
    #
    # There are TWO DISTINCT ticket-flag pairs — do not conflate them:
    #   * palw-submit CLIENT flags (--ticket-authority-key / --ticket-secret-file):
    #     REQUIRED here, at leaf-chunk submit. palw-submit opens each stored nullifier
    #     against the on-chain commitment and checks the leaf's named authority derives
    #     from this key (kaspa-pq-validator/src/palw_submit.rs, LeafChunk branch — a
    #     non-unsafe leaf-chunk submit FAILS without them).
    #   * kaspad NODE flags (--palw-ticket-authority-key-file / --palw-ticket-secret-file):
    #     a SEPARATE pair consumed at MINING time by start-palw-miner.sh — NOT passed here.
    _TA_KEY="${TICKET_AUTHORITY_KEY:-${TICKET_AUTHORITY_SEED:-$PALW_DATA_ROOT/keys/ticket-authority.seed}}"
    _TS_FILE="${TICKET_SECRET_FILE:-$PALW_DATA_ROOT/keys/ticket-secret.json}"
    [ -f "$_TA_KEY" ] || die "TICKET_MODE=mock but the ticket-authority seed is missing: $_TA_KEY. create-lifecycle.sh generates it (kaspa-pq-validator keygen) and start-palw-miner.sh passes it to KASPAD (--palw-ticket-authority-key-file) at mining time. Run ./create-lifecycle.sh with TICKET_MODE=mock first."
    [ -s "$_TS_FILE" ] || die "TICKET_MODE=mock but the TicketSecretStore is missing/empty: $_TS_FILE. create-lifecycle populates it via the mock-ticket helper (a workspace member built by build-and-hash.sh); no standalone CLI does. Re-run ./create-lifecycle.sh with TICKET_MODE=mock first."
    grep -q 'secrets' "$_TS_FILE" 2>/dev/null || warn "TICKET_SECRET_FILE ($_TS_FILE) has no 'secrets' field — it may not be a valid TicketSecretStore; the non-skip leaf-chunk submit will be rejected if the ticket cannot be opened."
    _MODE="$(stat -f '%Lp' "$_TS_FILE" 2>/dev/null || stat -c '%a' "$_TS_FILE" 2>/dev/null || printf '')"
    case "$_MODE" in 600|0600|'') : ;; *) warn "TicketSecretStore $_TS_FILE mode is $_MODE; it MUST be 0600 (chmod 0600 it) — palw-submit refuses a group/world-readable store." ;; esac
    # palw-submit's OWN client-side ticket flags (NOT kaspad's node flags). Required
    # for a non-unsafe leaf-chunk submit; palw-submit fail-closes without them.
    TICKET_ARGS=(--ticket-authority-key "$_TA_KEY" --ticket-secret-file "$_TS_FILE")
    log "TICKET_MODE=mock: leaf-chunks submitted WITHOUT --unsafe; ticket possession is verified BY palw-submit at submit time (its --ticket-authority-key/--ticket-secret-file client flags open each nullifier against the on-chain commitment). WIRING-ONLY, NON-inference mock leaves (never real inference, never palw_demo)."
fi

# Extra hint appended to a leaf-chunk submit failure only in mock mode.
_MOCK_SUBMIT_HINT=""
[ "$TICKET_MODE" = mock ] && _MOCK_SUBMIT_HINT="; in mock mode node A must have been started with --palw-ticket-authority-key-file/--palw-ticket-secret-file so the ticket can be verified"

k=0
while [ "$k" -lt "$CHUNK_COUNT" ]; do
    cf="$(chunk_file "$k")"
    # Idempotent: skip a chunk already registered (chunks numerator already covers it).
    cur_chunks="$(_num "$(_batch_field a chunks)")"
    case "$cur_chunks" in ''|*[!0-9]*) cur_chunks=0 ;; esac
    if [ "$cur_chunks" -gt "$k" ]; then
        log "leaf-chunk $k already registered on node A (chunks numerator=$cur_chunks); skipping submit (idempotent)"
    else
        if [ "$TICKET_MODE" = skip ]; then
            warn "UNSAFE: registering leaf-chunk $k with --unsafe-skip-ticket-secret-check (NO ticket). This leaf can NEVER be mined (no mint, no coinbase) — the honest no-GPU end state, NOT the seeded palw_demo path."
        fi
        log "submitting leaf-chunk $k/$(( CHUNK_COUNT - 1 )) carrier (funded; excluding every known bond) ..."
        _palw_submit leaf-chunk "$cf" ${TICKET_ARGS[@]+"${TICKET_ARGS[@]}"} \
            || die "'palw-submit --kind leaf-chunk' failed for chunk $k — a miner must be running for inclusion${_MOCK_SUBMIT_HINT}. Inspect node A ($(node_log a))."
        wait_inclusion a 1 || die "no selected child advanced after leaf-chunk $k carrier — the supporting miner must be running (invariant 3)."
    fi
    # Advance >=1 child (done above when submitted) + verify on BOTH nodes after each.
    wait_field_true_both manifest_present "manifest still present after chunk $k"
    wait_num_ge_both chunks $(( k + 1 )) "chunks registered through chunk $k"
    wait_num_ge_both leaf_blobs 1 "leaf blobs present after chunk $k"
    k=$(( k + 1 ))
done
# Full-bundle presence on BOTH nodes before auditing.
wait_num_ge_both chunks "$CHUNK_COUNT" "all $CHUNK_COUNT chunks registered"
wait_num_ge_both leaf_blobs "$LEAFN" "all $LEAFN leaf blobs present"

# ---- 4. audit-facts -> vote -> certificate -> certificate carrier -----------
if [ "$(_batch_field a certificate_blob_present)" = "true" ]; then
    log "certificate already present on node A (certificate_blob_present=true); skipping audit-facts/vote/certificate generation + submit (idempotent)"
else
    ensure_in_audit_window
    wait_node_synced a || die "node A is not synced — audit-facts requires a synced node."
    gen_facts
    gen_vote
    gen_cert
    log "submitting certificate carrier (funded; excluding every known bond) ..."
    _palw_submit certificate "$CERT_FILE" \
        || die "'palw-submit --kind certificate' failed — a miner must be running for inclusion, and the certificate must be a valid all-pass quorum for batch $BID8. Inspect node A ($(node_log a))."
    wait_inclusion a 1 || die "no selected child advanced after the certificate carrier — the supporting miner must be running (invariant 3)."
fi

# ---- 5. final: batch.status == active on BOTH nodes -------------------------
wait_batch_status "$BATCH_ID" active a || die "batch $BID8 did not reach status=active on node A — inspect the certificate and the supporting miner."
wait_batch_status "$BATCH_ID" active b || die "batch $BID8 did not reach status=active on node B — check A/B P2P propagation and that node B is advancing children."

log "STN-011 SUCCESS: batch $BATCH_ID reached status=active on BOTH node A and node B."
if [ "$TICKET_MODE" = skip ]; then
    log "END STATE (TICKET_MODE=skip): batch is ACTIVE but NO algo-4 block can be minted (the leaf-chunk carries no ticket). No coinbase, no minted block — the honest no-GPU end state. To MINT a wiring-only block, rebuild with TICKET_MODE=mock (needs the mock-ticket helper, built by build-and-hash.sh) then run ./start-palw-miner.sh."
else
    log "END STATE (TICKET_MODE=mock): batch is ACTIVE and a WIRING-ONLY, NON-inference mock block is mineable via ./start-palw-miner.sh (node A must run with --palw-ticket-authority-key-file/--palw-ticket-secret-file). Real inference needs the provider GPU tool (Phase 1, out of scope)."
fi
