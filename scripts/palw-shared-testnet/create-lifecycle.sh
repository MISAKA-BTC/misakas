#!/usr/bin/env bash
# =============================================================================
# create-lifecycle.sh — STN-011: build the PALW batch lifecycle payloads
#                        OFFLINE for the closed two-node testnet (Phase-0).
#
#   usage:  ./create-lifecycle.sh            # (or: ./create-lifecycle.sh create)
#
# SCOPE:
#   Invariant (2) — the batch-manifest MUST be registered DURING its registration
#   epoch E with headroom before the epoch boundary. To build it without the DAA
#   score drifting mid-build, this stage:
#     1. PAUSES the continuous algo-3 supporting miner so DAA is FROZEN, then
#        verifies the sink DAA is actually stationary (two identical samples);
#     2. pins E = current_epoch (from the frozen sink) and computes the mandated
#        admission windows activation=E+PALW_ACTIVATION_DELAY_EPOCHS and
#        expiry=activation+PALW_ACTIVE_WINDOW_EPOCHS (testnet-200 defaults E+8/E+24);
#     3. authors the unbound leaf-set JSON (schema "misaka.palw.leaf-set.v1") for
#        LEAF_COUNT leaves — each carrying the two DISTINCT provider bonds, the
#        shared runtime_class_id / model_profile_id, shape_id=SHAPE_ID, the
#        reward SPKs (reward_spk_p2pkh_mldsa), and DISTINCT job_nullifier /
#        private_match_commitment / receipt_da_root per leaf (distinct DA roots
#        matter for audit sampling);
#     4. builds the batch-manifest OFFLINE (palw-payload batch-manifest — no node
#        RPC, no block), records PALW_BATCH_ID, and builds every leaf-chunk
#        OFFLINE from the restamped (batch-bound) leaves file.
#
#   It DOES NOT resume the miner: submit-lifecycle.sh resumes it and submits the
#   carriers immediately within epoch E (invariant 2's "resume, submit").
#
# TICKET MODES:
#   skip (default) — ticket_nullifier_commitment / ticket_authority_pk_hash are
#     FIXED placeholders that are NEVER opened: submit-lifecycle registers the
#     leaf-chunk with `palw-submit --unsafe-skip-ticket-secret-check`, reaching
#     batch.status=active but a block with that leaf can NEVER be mined (no
#     ticket, no mint). This is the honest no-ticket end state.
#   mock — requires the mock-ticket helper binary (a workspace member built by build-and-hash.sh; see
#     mock-ticket/README.md). For each leaf a random 64-byte nullifier is drawn
#     (kept in a 0600 file, NEVER on argv/log), the helper opens its
#     ticket_nullifier_commitment + authority pk_hash, and — after the manifest
#     fixes the batch_id — the helper populates the TicketSecretStore. This mints
#     a WIRING-ONLY, explicitly NON-INFERENCE block.
#   real — requires palw-real-provider plus two independently signed Qwen
#     Receipt-v3 JSON files and their byte-identical worker results. The helper
#     verifies both ML-DSA-87 signatures, the exact k=2 projection, and the token
#     commitment, then derives the private ticket from that verified proof.
#
# Neither mock nor real mode uses the seeded test-only `palw_demo` path.
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
#   (a complete bundle already recorded is a no-op; a PARTIAL bundle is never
#   silently overwritten — fail-closed unless LIFECYCLE_FORCE=1); FAIL-CLOSED
#   with actionable messages; a register_cleanup trap removes the staging dir and
#   shreds the nullifier tmpdir so a failed run leaks nothing and never leaves a
#   truncated payload in place. It sources common.sh and uses ONLY its helpers —
#   nothing is reimplemented.
#
# Env knobs (all optional; defaults from env.example unless noted):
#   LIFECYCLE_FORCE=1  — wipe a partial/inconsistent bundle and rebuild.
#   FREEZE_SETTLE_SECS — seconds to let a paused miner's last block settle before
#                        sampling the sink DAA (default 3).
#   QUANTUM_COUNT / PROOF_TYPE — per-leaf wiring placeholders (default 1 / 0; not
#                        in env.example — closed no-value run).
#   MOCK_TICKET_BIN    — path to the mock-ticket helper (default
#                        $REPO_ROOT/target/release/mock-ticket). TICKET_MODE=mock.
#   REAL_PROVIDER_BIN  — path to palw-real-provider.
#   REAL_RECEIPT_A/B, REAL_RESULT_A/B — real Qwen evidence inputs.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

# Nicer per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-create-lifecycle}"; export PALW_LOG_TAG

# Supervised name of the continuous algo-3 supporting miner (set by
# supporting-miner.sh). This stage PAUSES it to freeze DAA and does NOT resume it.
MINER_PID_NAME="${MINER_PID_NAME:-supporting-miner}"

# Success flag consulted by the cleanup trap. GLOBAL on purpose: the EXIT trap
# runs after do_create() returns, so a `local` flag would be gone by then.
_LIFECYCLE_OK=0

usage() {
    cat >&2 <<EOF
usage: ${0##*/} [create]

  Build the PALW batch lifecycle payloads OFFLINE (STN-011): pause the supporting
  miner to FREEZE DAA, pin E=current_epoch, author the unbound leaf-set JSON for
  \$LEAF_COUNT leaves (configurable activation/expiry window; two DISTINCT provider bonds;
  shared runtime_class_id/model_profile_id; reward SPKs; distinct per-leaf
  job_nullifier/private_match_commitment/receipt_da_root), build the batch-
  manifest OFFLINE (records PALW_BATCH_ID), then build every leaf-chunk OFFLINE.

  The miner is LEFT PAUSED — submit-lifecycle.sh resumes it and submits within
  epoch E (invariant 2). The leaf is a MOCK (no real inference); this is NOT the
  seeded, test-only palw_demo path.

  TICKET_MODE=skip (default): fixed placeholder ticket fields (never opened;
    submit uses --unsafe-skip-ticket-secret-check -> batch.status=active, no mint).
  TICKET_MODE=mock: requires the mock-ticket helper (built by build-and-hash.sh) to
    open each leaf's ticket_nullifier_commitment and populate the TicketSecretStore
    for a WIRING-ONLY, non-inference block.
  TICKET_MODE=real: verifies an ML-DSA-87 signed k=2 Qwen Receipt-v3 pair and
    byte-identical worker results, derives the ticket from that proof, and carries
    every exact projection field into the on-chain provider receipts.

  Idempotent: a complete bundle already recorded is a no-op; a partial bundle is
  never silently overwritten (LIFECYCLE_FORCE=1 to wipe and rebuild). Fail-closed
  with actionable messages.
EOF
}

# ---------------------------------------------------------------------------
# Tiny local validators (NOT reimplementations of common.sh helpers — common.sh
# ships no hex/int/bond validator).
# ---------------------------------------------------------------------------
# _is_hex128 <str> — 0 iff <str> is exactly 128 hex chars (a 64-byte Hash64).
_is_hex128() {
    case "$1" in *[!0-9a-fA-F]*) return 1 ;; esac
    [ "${#1}" -eq 128 ]
}
# _lc <str> — lowercase a hex string (leaf JSON hex fields are lowercase).
_lc() { printf '%s' "$1" | tr 'A-F' 'a-f'; }

# _parse_bond <label> <txid:index> — validate and split into globals _TXID/_IDX.
#   Accepts the locked_provider_bond_outpoint form recorded by register-providers
#   (txid is a Hash64 = 128 hex; a 64-hex kaspa txid is also tolerated — the
#   payload builder is the authority on the exact width).
_parse_bond() {
    local label="$1" v="$2" txid idx
    case "$v" in
        *:*) : ;;
        *)   die "$label ('$v') is not in txid:index form (from artifacts/state.env)." ;;
    esac
    idx="${v##*:}"; txid="${v%:*}"
    case "$idx" in ''|*[!0-9]*) die "$label index '$idx' is not a non-negative integer (from '$v')." ;; esac
    txid="$(_lc "$txid")"
    case "$txid" in ''|*[!0-9a-f]*) die "$label transaction id '$txid' is not hex (from '$v')." ;; esac
    case "${#txid}" in
        64|128) : ;;
        *) die "$label transaction id has length ${#txid}; expected a 128-hex Hash64 (or a 64-hex txid), from '$v'." ;;
    esac
    _TXID="$txid"; _IDX="$idx"
}

# ---------------------------------------------------------------------------
# mock-ticket helper wrappers (TICKET_MODE=mock only).
#   The mock-ticket binary is a workspace member built by build-and-hash.sh (mock-ticket/README.md). It
#   owns the ticket cryptography (ticket_nullifier_commitment =
#   blake2b_512_keyed("misaka-palw-ticket-nf-commit-v1", nullifier); authority
#   pk_hash = blake2b_512_keyed over the verification key under the PALW
#   authorization domain) and the TicketSecretStore key derivation
#   (ticket_secret_key(batch_id, leaf_index)). The RAW nullifier is a SECRET and
#   is passed ONLY via a 0600 file (never on argv, never logged).
#
#   Contract (documented here; the helper implements it):
#     mock-ticket commit    --network <net> --authority-key <seed>
#                           --nullifier-file <0600 file>
#         -> stdout: ticket_nullifier_commitment: <128hex>
#                    ticket_authority_pk_hash:    <128hex>
#     mock-ticket store-add --network <net> --authority-key <seed>
#                           --secret-file <store.json> --batch-id <128hex>
#                           --leaf-index <i> --nullifier-file <0600 file>
#         -> idempotently upserts the TicketSecretStore entry (mode 0600).
# ---------------------------------------------------------------------------
# _mock_commit <nullifier-file> — set globals _MC_COMMIT / _MC_AUTH.
_mock_commit() {
    local nf="$1" out
    out="$("$MOCK_TICKET_BIN" commit \
            --network "$NETWORK" \
            --authority-key "$TICKET_AUTHORITY_KEY" \
            --nullifier-file "$nf" 2>&1)" \
        || die "mock-ticket 'commit' failed (bin=$MOCK_TICKET_BIN). Ensure it implements the interface in mock-ticket/README.md and that the authority seed is valid."
    _MC_COMMIT="$(printf '%s\n' "$out" | _kv ticket_nullifier_commitment)"
    _MC_AUTH="$(printf '%s\n' "$out"   | _kv ticket_authority_pk_hash)"
    _is_hex128 "$_MC_COMMIT" || die "mock-ticket 'commit' did not return a 128-hex ticket_nullifier_commitment."
    _is_hex128 "$_MC_AUTH"   || die "mock-ticket 'commit' did not return a 128-hex ticket_authority_pk_hash."
    _MC_COMMIT="$(_lc "$_MC_COMMIT")"; _MC_AUTH="$(_lc "$_MC_AUTH")"
}
# _mock_store_add <batch_id> <leaf_index> <nullifier-file> — upsert store entry.
_mock_store_add() {
    local bid="$1" idx="$2" nf="$3"
    "$MOCK_TICKET_BIN" store-add \
        --network "$NETWORK" \
        --authority-key "$TICKET_AUTHORITY_KEY" \
        --secret-file "$TICKET_SECRET_FILE" \
        --batch-id "$bid" \
        --leaf-index "$idx" \
        --nullifier-file "$nf" >/dev/null 2>&1 \
        || die "mock-ticket 'store-add' failed for leaf $idx (secret-file $TICKET_SECRET_FILE)."
    chmod 0600 "$TICKET_SECRET_FILE" 2>/dev/null || true
}

# Real-provider bridge wrappers. Unlike mock-ticket, verify-and-derive owns the
# inference boundary: it refuses to create a ticket unless both external
# Receipt-v3 signatures, worker identities/slots, k=2 projection, result bytes,
# and output-token commitment verify.
_real_verify_and_derive() {
    local nf="$1" proof="$2" out
    out="$("$REAL_PROVIDER_BIN" verify-and-derive \
            --receipt-a "$REAL_RECEIPT_A" \
            --receipt-b "$REAL_RECEIPT_B" \
            --result-a "$REAL_RESULT_A" \
            --result-b "$REAL_RESULT_B" \
            --authority-key "$TICKET_AUTHORITY_KEY" \
            --nullifier-out "$nf" \
            --proof-out "$proof" 2>&1)" \
        || die "palw-real-provider rejected the Qwen inference evidence (bin=$REAL_PROVIDER_BIN). No ticket was created."
    _RP_VERIFY="$(printf '%s\n' "$out" | _kv verification)"
    _RP_PAIR="$(printf '%s\n' "$out" | _kv external_pair_id)"
    _RP_RECEIPT_A="$(printf '%s\n' "$out" | _kv external_receipt_a_id)"
    _RP_RECEIPT_B="$(printf '%s\n' "$out" | _kv external_receipt_b_id)"
    _RP_JOBSET="$(printf '%s\n' "$out" | _kv compute_set_id)"
    _RP_CHALLENGE="$(printf '%s\n' "$out" | _kv job_challenge)"
    _RP_OUT="$(printf '%s\n' "$out" | _kv output_commitment)"
    _RP_SCHEDULE="$(printf '%s\n' "$out" | _kv schedule_root)"
    _RP_EXECUTION="$(printf '%s\n' "$out" | _kv execution_root)"
    _RP_ROUTE="$(printf '%s\n' "$out" | _kv route_root)"
    _RP_STATE="$(printf '%s\n' "$out" | _kv state_root)"
    _RP_CU="$(printf '%s\n' "$out" | _kv canonical_compute_units)"
    _RP_TOKENS="$(printf '%s\n' "$out" | _kv token_count)"
    _RP_STOP="$(printf '%s\n' "$out" | _kv stop_reason)"
    _RP_MODEL="$(printf '%s\n' "$out" | _kv model_profile_id)"
    _RP_PROOF="$(printf '%s\n' "$out" | _kv proof_commitment)"
    _RP_COMMIT="$(printf '%s\n' "$out" | _kv ticket_nullifier_commitment)"
    _RP_AUTH="$(printf '%s\n' "$out" | _kv ticket_authority_pk_hash)"
    [ "$_RP_VERIFY" = "receipt-v3+mldsa87+k2+tokens" ] \
        || die "palw-real-provider did not report the complete verification verdict."
    for _rp_hex in _RP_PAIR _RP_RECEIPT_A _RP_RECEIPT_B _RP_JOBSET _RP_CHALLENGE \
                   _RP_OUT _RP_SCHEDULE _RP_EXECUTION _RP_ROUTE _RP_STATE \
                   _RP_MODEL _RP_PROOF _RP_COMMIT _RP_AUTH; do
        _is_hex128 "${!_rp_hex}" || die "palw-real-provider returned an invalid ${_rp_hex#_RP_} field."
    done
    for _rp_int in _RP_CU _RP_TOKENS _RP_STOP; do
        case "${!_rp_int}" in ''|*[!0-9]*) die "palw-real-provider returned a non-integer ${_rp_int#_RP_} field." ;; esac
    done
}

_real_store_add() {
    local bid="$1" idx="$2" nf="$3"
    "$REAL_PROVIDER_BIN" store-add \
        --authority-key "$TICKET_AUTHORITY_KEY" \
        --secret-file "$TICKET_SECRET_FILE" \
        --batch-id "$bid" \
        --leaf-index "$idx" \
        --nullifier-file "$nf" >/dev/null 2>&1 \
        || die "palw-real-provider could not add the verified ticket to $TICKET_SECRET_FILE."
    chmod 0600 "$TICKET_SECRET_FILE" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# ADR-0045 D3-b (PCPB) evidence.
#
# A LeafV2 no longer chooses its own providers or its own job challenge. Clause 11
# re-derives receipt_v3_job_challenge under R_issued from the witness's preimage
# triple, and clause 12 re-runs the dispatch draw against the snapshot the node
# resolved itself. Both refuse a leaf whose declared fields disagree, so the leaf
# has to be BUILT from the chain's answer instead of asserting one.
#
# `palw-payload pcpb-witness` is that answer: one call yields the witness file the
# chunk carries and the leaf fields it must be paired with. Everything below is
# plumbing around that single source.
# ---------------------------------------------------------------------------

# _pcpb_bond_table — print "outpoint<TAB>seedfile" for every provider bond whose
#   owner key this host holds. The draw is bond-weighted over the WHOLE snapshot,
#   so a seat can land on any registered bond; a bond we cannot name (no seed)
#   cannot sign its DA session authorization and would strand the build.
_pcpb_bond_table() {
    local f base seed
    [ -n "$PROV_A_BOND" ] && printf '%s\t%s\n' "$PROV_A_BOND" "${PROV_A_KEY:-$PALW_DATA_ROOT/keys/provider-a.seed}"
    [ -n "$PROV_B_BOND" ] && printf '%s\t%s\n' "$PROV_B_BOND" "${PROV_B_KEY:-$PALW_DATA_ROOT/keys/provider-b.seed}"
    local aud; aud="$(state_get AUD_C_BOND)"
    [ -n "$aud" ] && printf '%s\t%s\n' "$aud" "$PALW_DATA_ROOT/keys/auditor-c.seed"
    # Capacity-floor providers registered by a separate pass keep seed and outpoint
    # side by side as "<name>" and "<name>.bond.outpoint".
    if [ -n "${PALW_EXTRA_PROVIDER_KEYDIR:-}" ] && [ -d "$PALW_EXTRA_PROVIDER_KEYDIR" ]; then
        for f in "$PALW_EXTRA_PROVIDER_KEYDIR"/*.bond.outpoint; do
            [ -s "$f" ] || continue
            base="${f%.bond.outpoint}"
            seed="$base"
            [ -s "$seed" ] || continue
            printf '%s\t%s\n' "$(cat "$f")" "$seed"
        done
    fi
}

# _pcpb_seed_for <outpoint> — the seed file that owns a drawn bond, or die.
#
#   Reads the table SNAPSHOT taken before the draw's seats were adopted, never
#   `_pcpb_bond_table` afresh: adopting the seats overwrites PROV_A_BOND/PROV_B_BOND,
#   so a late rebuild would hand every drawn bond the configured providers' seeds —
#   a silent mispairing that signs the DA session authorization with the wrong owner
#   key and names a reward script consensus will not pay.
_pcpb_seed_for() {
    local want="$1" op seed
    while IFS="$(printf '\t')" read -r op seed; do
        [ "$op" = "$want" ] && { printf '%s' "$seed"; return 0; }
    done <<EOF
$PCPB_BOND_TABLE
EOF
    die "the PCPB draw seated bond $want, but this host holds no owner seed for it. Every bond in the epoch snapshot can be drawn, so the harness needs each one's seed (PALW_EXTRA_PROVIDER_KEYDIR should point at the capacity-provider key dir)."
}

# _pcpb_reward_spk <seedfile> — the ONLY script consensus will pay this provider.
#   utxo_validation checks leaf.provider_X_reward_script == provider_bond_lock_spk(
#   bond owner pk), so it is a pure function of the seed; the payload built here is
#   thrown away and only its printed SPK is kept.
_pcpb_reward_spk() {
    local seed="$1" tmp out spk
    tmp="$PALW_DATA_ROOT/artifacts/.pcpb-spk.$$.borsh"
    rm -f "$tmp"
    out="$("$VAL" palw-payload provider-bond \
            --network "$NETWORK" --validator-key "$seed" \
            --operator-group-id "$(zero128)" --runtime-class "$RUNTIME_CLASS" \
            --capacity "${SHAPE_ID}=${CAPACITY_COUNT:-1}" --reward-key-root "$(zero128)" \
            --amount "${PROVIDER_A_AMOUNT:-10MSK}" --unbond-delay-epochs "${UNBOND_DELAY_EPOCHS:-6}" \
            --out "$tmp" 2>&1)" || { rm -f "$tmp"; printf '%s\n' "$out" >&2; die "could not derive the reward SPK for $seed (see above)."; }
    rm -f "$tmp"
    spk="$(printf '%s\n' "$out" | _kv provider_bond_lock_spk)"
    case "$spk" in
        0000*[!0-9a-fA-F]*|"") printf '%s\n' "$out" >&2
            die "provider-bond did not print a usable provider_bond_lock_spk for $seed." ;;
    esac
    printf '%s' "$spk"
}

# _real_reemit_receipts <challenge> <issued> <expires> <dir> — re-attest the Qwen
#   execution under the chain's job challenge.
#
#   Receipt v3 signs `output_commitment_v3(output_ids, job_challenge)`, so a receipt
#   is bound to the challenge it was made for. D3-b makes the challenge a CHAIN
#   value — clause 11 re-derives it from R_issued — which the recorded receipts
#   predate. Re-emitting them from the same execution (identical token ids, identical
#   engine roots, same worker keys) is what keeps `verify-and-derive` checking the
#   job this batch actually registers; feeding it the old receipts would verify a
#   different job than the leaf commits to.
#
#   The INFERENCE is not re-run and this does not pretend otherwise: the roots and
#   token ids are the recorded ones. What is re-signed is the attestation over them.
_real_reemit_receipts() {
    local challenge="$1" issued="$2" expires="$3" dir="$4" slot src seed
    require_cmd python3
    [ -s "$REAL_QWEN_PROMPT_IDS" ] || die "receipt re-attestation needs the prompt token ids at $REAL_QWEN_PROMPT_IDS."
    [ -s "$REAL_QWEN_OUTPUT_IDS" ] || die "receipt re-attestation needs the output token ids at $REAL_QWEN_OUTPUT_IDS."
    [ -x "$QI35_RECEIPT_BIN" ] || die "receipt re-attestation needs $QI35_RECEIPT_BIN (cargo build --release -p misaka-palw-bridge --bin qi35-receipt-v3)."
    for slot in 0 1; do
        if [ "$slot" = 0 ]; then src="$REAL_RECEIPT_A"; seed="$REAL_QWEN_WORKER_A_KEY"; else src="$REAL_RECEIPT_B"; seed="$REAL_QWEN_WORKER_B_KEY"; fi
        [ -s "$seed" ] || die "receipt re-attestation needs the worker seed $seed (slot $slot)."
        # Every value below is lifted from the recorded receipt, so the re-attestation
        # can only differ in the fields D3-b forces to move.
        eval "$(python3 -c '
import json,sys,shlex
r=json.load(open(sys.argv[1]))
p,t,a=r["projection"],r["telemetry"],r["artifacts"]
for k,v in [("RE_SET",p["compute_set_id"]),("RE_NET",r["network_id"]),
            ("RE_ROUTE",p["route_root"]),("RE_KV",p["schedule_root"]),("RE_STATE",p["state_root"]),
            ("RE_CU",p["canonical_compute_units"]),("RE_STOP",p["stop_reason"]),
            ("RE_RCLASS",t["runtime_class_id"]),("RE_RMAN",t["runtime_manifest_hash"]),
            ("RE_ENGINE",a["engine_blake2b256"]),("RE_MODEL",a["model_blake2b256"]),
            ("RE_TABLES",a["tables_blake2b256"]),("RE_SECS",r["engine_seconds"]),
            ("RE_TS",r["timestamp_millis"]),("RE_LABEL",r["worker_label"])]:
    print("%s=%s" % (k, shlex.quote(str(v))))
' "$src")" || die "cannot read the recorded receipt $src for re-attestation."
        "$QI35_RECEIPT_BIN" \
            --prompt-ids "@$REAL_QWEN_PROMPT_IDS" --output-ids "@$REAL_QWEN_OUTPUT_IDS" \
            --route-root "$RE_ROUTE" --kv-root "$RE_KV" --state-root "$RE_STATE" \
            --job-challenge "$challenge" --compute-set-id "$RE_SET" --network-id-hash "$RE_NET" \
            --worker-key "$seed" --replica-slot "$slot" \
            --issued-epoch "$issued" --expires-epoch "$expires" \
            --canonical-compute-units "$RE_CU" --stop-reason "$RE_STOP" \
            --runtime-class-id "$RE_RCLASS" --runtime-manifest-hash "$RE_RMAN" \
            --engine-blake2b256 "$RE_ENGINE" --model-blake2b256 "$RE_MODEL" --tables-blake2b256 "$RE_TABLES" \
            --engine-seconds "$RE_SECS" --timestamp-millis "$RE_TS" --worker-label "$RE_LABEL" \
            --receipt-out "$dir/receipt-$slot.json" --result-out "$dir/result-$slot.json" >/dev/null 2>&1 \
            || die "qi35-receipt-v3 could not re-attest replica slot $slot under job challenge $challenge."
    done
    REAL_RECEIPT_A="$dir/receipt-0.json"; REAL_RESULT_A="$dir/result-0.json"
    REAL_RECEIPT_B="$dir/receipt-1.json"; REAL_RESULT_B="$dir/result-1.json"
    log "re-attested the recorded Qwen execution under chain job challenge $challenge (issued=$issued expires=$expires); the inference itself is unchanged."
}

# _pcpb_field <index> <name> — read one leaf binding out of the fields JSON.
_pcpb_field() {
    python3 -c '
import json,sys
d=json.load(open(sys.argv[1]))
print(d["leaves"][int(sys.argv[2])][sys.argv[3]])
' "$PCPB_FIELDS_FILE" "$1" "$2"
}

# _pcpb_derive <registration_epoch> <staging_dir> — resolve the PCPB evidence for
#   this batch, trying every anchor clause 11's window allows.
#
#   The window is `anchor + Δ ≤ registered ∧ registered − anchor ≤ w`, so with the
#   shipped w=6/Δ=2 the anchor may sit 2..6 epochs back. Preferring E−3 keeps both
#   the snapshot (anchor−k) and the draw beacon (anchor+Δ) strictly in the past, so
#   nothing waits. Later candidates matter because the draw is per-EPOCH: when an
#   epoch's two tickets land on the same provider that epoch cannot host any
#   external job, which with N equal bonds happens about 1/N of the time. That is
#   the protocol working, not an error, so walk to the next anchor.
_pcpb_derive() {
    local e="$1" staging="$2" bonds="" op seed a out ok=0
    # Snapshot the bond -> seed map ONCE, while PROV_A_BOND/PROV_B_BOND still hold
    # the configured bonds. Adopting the drawn seats overwrites them.
    PCPB_BOND_TABLE="$(_pcpb_bond_table)"
    while IFS="$(printf '\t')" read -r op seed; do
        [ -n "$op" ] && bonds="$bonds --provider-bond $op"
    done <<EOF
$PCPB_BOND_TABLE
EOF
    [ -n "$bonds" ] || die "no provider bonds with owner seeds are known — run register-providers.sh first."

    PCPB_WITNESS_FILE="$staging/pcpb-witness.borsh"
    PCPB_FIELDS_FILE="$staging/pcpb-fields.json"
    for a in $(( e - 3 )) $(( e - 4 )) $(( e - 5 )) $(( e - 2 )) $(( e - 6 )); do
        [ "$a" -gt 0 ] || continue
        rm -f "$PCPB_WITNESS_FILE" "$PCPB_FIELDS_FILE"
        if out="$("$VAL" palw-payload pcpb-witness \
                --network "$NETWORK" --node-wrpc-borsh "$(node_wrpc a)" \
                --anchor-epoch "$a" --shape-id "$SHAPE_ID" --leaf-count "$LEAF_COUNT" \
                --job-seed "${PALW_PCPB_JOB_SEED:-$NETWORK-batch-$e}" $bonds \
                --witness-out "$PCPB_WITNESS_FILE" --fields-out "$PCPB_FIELDS_FILE" 2>&1)"; then
            PCPB_ANCHOR="$a"
            ok=1
            log "PCPB anchor=$a resolved (snapshot=$(printf '%s\n' "$out" | _kv snapshot_epoch) draw=$(printf '%s\n' "$out" | _kv draw_epoch) providers=$(printf '%s\n' "$out" | _kv provider_count))"
            break
        fi
        log "PCPB anchor=$a unusable: $(printf '%s\n' "$out" | tail -1)"
    done
    [ "$ok" = 1 ] || die "no anchor in clause 11's window [$(( e - 6 )), $(( e - 2 ))] could seat a job for registration epoch $e. If every anchor reported a colliding draw, the snapshot has too few distinct bonded providers; register more before retrying."
    [ -s "$PCPB_WITNESS_FILE" ] || die "pcpb-witness reported success but wrote no witness file."
}

# ---------------------------------------------------------------------------
# Lifecycle-bundle presence checks (idempotency).
#   LIFECYCLE_DIR / PALW_BATCH_ID / PALW_CHUNK_COUNT are set in do_create.
# ---------------------------------------------------------------------------
# _lifecycle_built — 0 iff a COMPLETE, consistent bundle already exists
#   (manifest + restamped leaves + all chunk files + recorded batch id/chunk count).
_lifecycle_built() {
    local bid cc k
    [ -f "$LIFECYCLE_DIR/manifest.borsh" ]    || return 1
    [ -f "$LIFECYCLE_DIR/leaves.batch.json" ] || return 1
    bid="$(state_get PALW_BATCH_ID)";    _is_hex128 "$bid" || return 1
    cc="$(state_get PALW_CHUNK_COUNT)";  case "$cc" in ''|*[!0-9]*) return 1 ;; esac
    [ "$cc" -ge 1 ] || return 1
    k=0
    while [ "$k" -lt "$cc" ]; do
        [ -f "$LIFECYCLE_DIR/chunk-$k.borsh" ] || return 1
        k=$(( k + 1 ))
    done
    return 0
}
# _lifecycle_any — 0 iff ANY lifecycle payload file is present (partial detect).
_lifecycle_any() {
    local f
    [ -d "$LIFECYCLE_DIR" ] || return 1
    for f in "$LIFECYCLE_DIR"/leafset.json \
             "$LIFECYCLE_DIR"/manifest.borsh \
             "$LIFECYCLE_DIR"/leaves.batch.json \
             "$LIFECYCLE_DIR"/chunk-*.borsh; do
        [ -e "$f" ] && return 0
    done
    return 1
}

# ---------------------------------------------------------------------------
# emit_leaf <index> — print ONE leaf JSON object (serde field names/types exact).
#   Reads the per-leaf uniqueness arrays and the shared globals computed in
#   do_create. batch_id is all-zero (UNBOUND) — the manifest binds it; the
#   builder refuses a prebound (batch_id != 0) leaf.
# ---------------------------------------------------------------------------
emit_leaf() {
    local i="$1"
    cat <<EOF
    {
      "version": 1,
      "batch_id": "$BATCH_UNBOUND",
      "leaf_index": $i,
      "job_nullifier": "${JOB_NF[$i]}",
      "ticket_nullifier_commitment": "${TNC[$i]}",
      "model_profile_id": "$MODEL_PROFILE_ID",
      "runtime_class_id": "$RUNTIME_CLASS",
      "shape_id": $SHAPE_ID,
      "quantum_count": $QUANTUM_COUNT,
      "proof_type": $PROOF_TYPE,
      "provider_a_bond": { "transactionId": "$A_TXID", "index": $A_IDX },
      "provider_b_bond": { "transactionId": "$B_TXID", "index": $B_IDX },
      "provider_a_reward_script": "$RSPK_A",
      "provider_b_reward_script": "$RSPK_B",
      "ticket_authority_pk_hash": "${TAPKH[$i]}",
      "private_match_commitment": "${PMC[$i]}",
      "receipt_da_object_version": ${DAVERSION[$i]},
      "receipt_da_root": "${DAROOT[$i]}",
      "receipt_da_object_len": ${DALEN[$i]},
      "receipt_da_chunk_count": ${DACHUNK[$i]},
      "receipt_v3_compute_set_id": "${JOBSET[$i]}",
      "receipt_v3_job_challenge": "${JOB_NF[$i]}",
      "receipt_v3_issued_epoch": ${PCPB_ISSUED[$i]},
      "receipt_v3_expires_epoch": $EXP,
      "a_commit": "${PCPB_A_COMMIT[$i]}",
      "a_commit_epoch": ${PCPB_A_COMMIT_EPOCH[$i]},
      "provider_snapshot_root": "${PCPB_SNAP_ROOT[$i]}",
      "assignment_proof_root": "${PCPB_ASSIGN_ROOT[$i]}",
      "dispatch_kind": ${PCPB_DISPATCH_KIND[$i]},
      "registered_epoch": $E,
      "activation_epoch": $ACT,
      "expiry_epoch": $EXP,
      "leaf_bond_sompi": 0
    }
EOF
}

# ---------------------------------------------------------------------------
do_create() {
    require_cmd mktemp awk grep tr od seq

    # ---- 1. resolve + validate inputs (fail-closed, BEFORE touching anything) --

    # Provider bonds recorded by register-providers.sh (two DISTINCT outpoints).
    PROV_A_BOND="$(state_get PROV_A_BOND)"
    PROV_B_BOND="$(state_get PROV_B_BOND)"
    [ -n "$PROV_A_BOND" ] || die "PROV_A_BOND is empty — run register-providers.sh first (it records the provider A bond outpoint into artifacts/state.env)."
    [ -n "$PROV_B_BOND" ] || die "PROV_B_BOND is empty — run register-providers.sh first (provider B bond outpoint)."
    [ "$PROV_A_BOND" != "$PROV_B_BOND" ] || die "PROV_A_BOND and PROV_B_BOND are identical ($PROV_A_BOND); a leaf requires two DISTINCT provider bonds."
    _parse_bond PROV_A_BOND "$PROV_A_BOND"; A_TXID="$_TXID"; A_IDX="$_IDX"
    _parse_bond PROV_B_BOND "$PROV_B_BOND"; B_TXID="$_TXID"; B_IDX="$_IDX"

    # Shared runtime class (leaf runtime_class_id MUST equal providers'
    # --runtime-class). RUNTIME_CLASS is honored if set, else RUNTIME_CLASS_ID.
    RUNTIME_CLASS="$(_lc "${RUNTIME_CLASS:-${RUNTIME_CLASS_ID:-}}")"
    _is_hex128 "$RUNTIME_CLASS" || die "runtime class must be 128 hex chars (set RUNTIME_CLASS or RUNTIME_CLASS_ID); MUST equal every leaf's runtime_class_id and the providers' --runtime-class."

    # Shared model profile (manifest requires all leaves to share it).
    MODEL_PROFILE_ID="$(_lc "${MODEL_PROFILE_ID:-}")"
    _is_hex128 "$MODEL_PROFILE_ID" || die "MODEL_PROFILE_ID must be 128 hex chars (all leaves share one model_profile_id)."

    # Manifest inputs.
    DESCRIPTOR_ROOT="$(_lc "${DESCRIPTOR_ROOT:-}")"
    AUDIT_POLICY_ID="$(_lc "${AUDIT_POLICY_ID:-}")"
    _is_hex128 "$DESCRIPTOR_ROOT" || die "DESCRIPTOR_ROOT must be 128 hex chars (batch-manifest --descriptor-root)."
    _is_hex128 "$AUDIT_POLICY_ID" || die "AUDIT_POLICY_ID must be 128 hex chars (batch-manifest --audit-policy-id)."

    # Integer leaf fields. QUANTUM_COUNT/PROOF_TYPE are wiring placeholders and
    # are NOT in env.example (closed no-value run) — override via env if needed.
    QUANTUM_COUNT="${QUANTUM_COUNT:-1}"
    # proof_type must be a VALID non-zero enum for consensus validate_public_leaf
    # (0 is rejected: "leaf.proof_type is invalid"). 1 = the shipped mock/default.
    PROOF_TYPE="${PROOF_TYPE:-1}"
    local iv name val
    for iv in "SHAPE_ID:${SHAPE_ID:-}" "QUANTUM_COUNT:$QUANTUM_COUNT" "PROOF_TYPE:$PROOF_TYPE" "LEAF_COUNT:${LEAF_COUNT:-}"; do
        name="${iv%%:*}"; val="${iv#*:}"
        case "$val" in ''|*[!0-9]*) die "$name must be a non-negative integer (got '$val')." ;; esac
    done
    [ "$LEAF_COUNT" -ge 1 ] || die "LEAF_COUNT must be >= 1 (got $LEAF_COUNT)."

    # Per-leaf reward SPKs. These MUST be the providers' bond-lock scripts, which
    # register-providers.sh records as PROV_{A,B}_REWARD_SPK. `palw_work_reward_class`
    # (CRITICAL-1) pays a leaf's providers only when
    #   leaf.provider_x_reward_script == provider_bond_lock_spk(bond owner_public_key);
    # anything else classifies the algo-4 source as ReplicaPalwUnbackedCollateral,
    # which pays the providers NOTHING and leaves the merging block's coinbase with
    # ZERO outputs (block accepted, entire subsidy paid to nobody). The old mock
    # used a synthetic 0x71/0x72 byte pattern here, so no algo-4 block it produced
    # could ever pay a provider. Fail closed rather than mint another unpayable leaf.
    RSPK_A="$(state_get PROV_A_REWARD_SPK)"
    RSPK_B="$(state_get PROV_B_REWARD_SPK)"
    if [ -z "$RSPK_A" ] || [ -z "$RSPK_B" ]; then
        die "PROV_A_REWARD_SPK / PROV_B_REWARD_SPK are not recorded in artifacts/state.env — run ./register-providers.sh (it derives each provider's bond-lock SPK offline from its seed, for already-registered bonds too). Refusing to build a leaf whose providers consensus would never pay."
    fi
    [ "$RSPK_A" != "$RSPK_B" ] || die "PROV_A_REWARD_SPK == PROV_B_REWARD_SPK — the two providers must be distinct identities."

    # Sentinels.
    BATCH_UNBOUND="$(zero128)"            # unbound leaf batch_id
    # Skip-mode ticket placeholders — FIXED, obviously-placeholder 128-hex values
    # that are NEVER opened (submit uses --unsafe-skip-ticket-secret-check).
    TICKET_NF_PLACEHOLDER="$(_lc "${TICKET_NF_PLACEHOLDER:-$(h64 ee)}")"
    TICKET_AUTH_PLACEHOLDER="$(_lc "${TICKET_AUTH_PLACEHOLDER:-$(h64 dd)}")"
    _is_hex128 "$TICKET_NF_PLACEHOLDER"   || die "TICKET_NF_PLACEHOLDER must be 128 hex chars."
    _is_hex128 "$TICKET_AUTH_PLACEHOLDER" || die "TICKET_AUTH_PLACEHOLDER must be 128 hex chars."

    # Ticketed-mode preconditions — fail fast before pausing the miner.
    if [ "$TICKET_MODE" = mock ]; then
        MOCK_TICKET_BIN="${MOCK_TICKET_BIN:-$REPO_ROOT/target/release/mock-ticket}"
        [ -x "$MOCK_TICKET_BIN" ] || die "TICKET_MODE=mock requires the mock-ticket helper at $MOCK_TICKET_BIN, but it is missing/not executable. It is a workspace member built by build-and-hash.sh (cargo build --release -p mock-ticket) — run ./build-and-hash.sh (or set MOCK_TICKET_BIN to its path), or use TICKET_MODE=skip (reaches batch.status=active without minting). It opens each leaf's ticket_nullifier_commitment and populates the TicketSecretStore for a WIRING-ONLY, non-inference block."
    elif [ "$TICKET_MODE" = real ]; then
        [ "$LEAF_COUNT" -eq 1 ] || die "TICKET_MODE=real currently binds one verified Qwen k=2 job, so LEAF_COUNT must be 1."
        REAL_PROVIDER_BIN="${REAL_PROVIDER_BIN:-$REPO_ROOT/target/release/palw-real-provider}"
        [ -x "$REAL_PROVIDER_BIN" ] || die "TICKET_MODE=real requires $REAL_PROVIDER_BIN (cargo build --release -p palw-real-provider)."
        # D3-b re-attestation inputs. The token ids and worker seeds live beside the
        # recorded receipts; without them the recorded execution cannot be re-signed
        # under the chain's job challenge and TICKET_MODE=real cannot proceed.
        QI35_RECEIPT_BIN="${QI35_RECEIPT_BIN:-$REPO_ROOT/target/release/qi35-receipt-v3}"
        _rq_dir="$(dirname "$REAL_RECEIPT_A")"
        REAL_QWEN_PROMPT_IDS="${REAL_QWEN_PROMPT_IDS:-$_rq_dir/prompt-ids.txt}"
        REAL_QWEN_OUTPUT_IDS="${REAL_QWEN_OUTPUT_IDS:-$_rq_dir/output-ids.txt}"
        REAL_QWEN_WORKER_A_KEY="${REAL_QWEN_WORKER_A_KEY:-$_rq_dir/keys/worker-a.seed}"
        REAL_QWEN_WORKER_B_KEY="${REAL_QWEN_WORKER_B_KEY:-$_rq_dir/keys/worker-b.seed}"
        for _rp_input in QI35_RECEIPT_BIN REAL_QWEN_PROMPT_IDS REAL_QWEN_OUTPUT_IDS REAL_QWEN_WORKER_A_KEY REAL_QWEN_WORKER_B_KEY; do
            [ -s "${!_rp_input}" ] || die "TICKET_MODE=real needs $_rp_input at ${!_rp_input}: ADR-0045 D3-b makes the job challenge a chain value, so the recorded receipts must be re-attested under it."
        done
        for _rp_input in REAL_RECEIPT_A REAL_RECEIPT_B REAL_RESULT_A REAL_RESULT_B; do
            [ -n "${!_rp_input:-}" ] || die "TICKET_MODE=real requires $_rp_input."
            [ -s "${!_rp_input}" ] || die "$_rp_input does not name a nonempty evidence file: ${!_rp_input}"
        done
    fi
    if [ "$TICKET_MODE" = mock ] || [ "$TICKET_MODE" = real ]; then
        # Auto-init the ticket-authority seed (32-byte hex, 0600) if absent, via the
        # SAME loader kaspad's --palw-ticket-authority-key-file expects. keygen refuses
        # to clobber an existing file, so a re-run reuses the established authority; the
        # miner is later started with this exact seed, so both agree on the pk_hash.
        if [ ! -s "$TICKET_AUTHORITY_KEY" ]; then
            log "TICKET_MODE=$TICKET_MODE: no ticket-authority seed at $TICKET_AUTHORITY_KEY — generating one (kaspa-pq-validator keygen, 0600)."
            install -d -m 0700 "$(dirname "$TICKET_AUTHORITY_KEY")" || die "cannot create key dir for $TICKET_AUTHORITY_KEY"
            "$VAL" keygen --out "$TICKET_AUTHORITY_KEY" --network "$NETWORK_BASE" >/dev/null \
                || die "failed to generate the ticket-authority seed at $TICKET_AUTHORITY_KEY via '$VAL keygen'. Generate it manually (kaspa-pq-validator keygen --out $TICKET_AUTHORITY_KEY) or point TICKET_AUTHORITY_KEY at an existing 32-byte-hex 0600 seed."
        fi
        [ -s "$TICKET_AUTHORITY_KEY" ] || die "TICKET_MODE=$TICKET_MODE requires the ticket-authority seed at $TICKET_AUTHORITY_KEY but it is still missing after keygen."
    fi

    # ---- 2. idempotency / partial-state gate (no node, no miner needed) --------
    LIFECYCLE_DIR="$PALW_DATA_ROOT/artifacts/lifecycle"

    if _lifecycle_built && [ "${LIFECYCLE_FORCE:-}" != 1 ]; then
        log "lifecycle bundle already built (batch_id=$(state_get PALW_BATCH_ID), chunks=$(state_get PALW_CHUNK_COUNT), registration_epoch=$(state_get PALW_REG_EPOCH)); idempotent no-op. The miner is NOT touched — submit-lifecycle resumes it and submits within that epoch. A batch is single-use and EXPIRES (batch.epochs expiry); to run another one, re-run with LIFECYCLE_FORCE=1 to retire this bundle and build a fresh batch."
        _LIFECYCLE_OK=1
        return 0
    fi
    if _lifecycle_any; then
        if [ "${LIFECYCLE_FORCE:-}" = 1 ]; then
            # FORCE covers BOTH a partial bundle and a COMPLETE one. A complete
            # bundle is not sacred: a batch is single-use and expires, so after a
            # mint (or after its expiry epoch passes) the only way forward is a new
            # batch — previously there was no supported path to one and the stage
            # no-op'd forever. Wiping is local only: the retired batch stays
            # on-chain, and a fresh batch_id is derived on rebuild.
            warn "LIFECYCLE_FORCE=1: retiring the lifecycle bundle under $LIFECYCLE_DIR (batch_id=$(state_get PALW_BATCH_ID)) and rebuilding a FRESH batch. The retired batch remains on-chain; only local payload artifacts are removed."
            # Includes the submit-stage artifacts (facts/vote/cert): they are bound
            # to the OLD batch_id, and submit-lifecycle reuses an existing file
            # rather than overwriting it, so leaving them would attach a stale
            # certificate to the new batch.
            rm -rf "$LIFECYCLE_DIR"/leafset.json "$LIFECYCLE_DIR"/manifest.borsh \
                   "$LIFECYCLE_DIR"/leaves.batch.json "$LIFECYCLE_DIR"/chunk-*.borsh \
                   "$LIFECYCLE_DIR"/da \
                   "$LIFECYCLE_DIR"/pcpb-witness.borsh "$LIFECYCLE_DIR"/pcpb-fields.json \
                   "$LIFECYCLE_DIR"/facts.json "$LIFECYCLE_DIR"/vote.borsh "$LIFECYCLE_DIR"/cert.borsh
            # Forget stale discovered state so a fresh batch_id is recorded.
            state_set PALW_BATCH_ID ""
            state_set PALW_CHUNK_COUNT ""
            state_set PALW_LEAF_COUNT ""
            state_set PALW_REG_EPOCH ""
        else
            die "a PARTIAL or inconsistent lifecycle bundle exists under $LIFECYCLE_DIR (some payload files present, but not a complete manifest + restamped leaves + all chunk files with PALW_BATCH_ID/PALW_CHUNK_COUNT recorded). This harness will not silently overwrite it. Re-run with LIFECYCLE_FORCE=1 to wipe and rebuild, or remove $LIFECYCLE_DIR and re-run."
        fi
    fi

    # ---- 3. node readiness (needed only to read the epoch) ---------------------
    wait_rpc_up a      || die "node A wRPC is not answering — start node-a.sh (and node-b.sh) before create-lifecycle."
    wait_node_synced a || die "node A is not synced — its sink DAA would be stale; wait for sync (dns-validator.sh / earlier stages) before building the lifecycle."

    # Best-effort provider-active check (non-fatal): building a lifecycle for
    # unregistered providers is almost always a mistake, but the build itself only
    # needs the bond OUTPOINTS. submit-lifecycle enforces registry membership.
    local pair op inreg
    for pair in "provider-a=$PROV_A_BOND" "provider-b=$PROV_B_BOND"; do
        op="${pair#*=}"
        inreg="$(palw_provider_status a "$op" 2>/dev/null | _kv in_registry || true)"
        [ "$inreg" = "true" ] || warn "${pair%%=*} bond $op is not shown in_registry=true on node A yet — the build proceeds, but submit-lifecycle will fail unless the providers are registered (run register-providers.sh)."
    done

    # ---- 4. FREEZE DAA: pause the supporting miner, prove the sink is stationary
    log "freezing DAA: pausing the supporting miner ('$MINER_PID_NAME') so the registration epoch cannot drift while payloads are built offline"
    if is_running "$MINER_PID_NAME"; then
        stop_pid "$MINER_PID_NAME" || die "could not stop the supporting miner ('$MINER_PID_NAME') to freeze DAA."
    else
        warn "supporting miner '$MINER_PID_NAME' was not running under this harness; still verifying DAA is frozen before building."
    fi

    # From here on a failure leaves the miner paused (by design — do NOT resume;
    # submit-lifecycle does). Remind the operator on any non-success exit.
    register_cleanup 'if [ "${_LIFECYCLE_OK:-0}" != 1 ]; then warn "create-lifecycle did not finish: the supporting miner remains PAUSED (DAA frozen). Fix the error and re-run; submit-lifecycle resumes the miner and submits within the registration epoch."; fi'

    local d1 d2 rem drift
    d1="$(node_sink_daa a)" || die "could not read node A sink DAA to freeze the epoch (is node A up and synced?)."
    sleep "${FREEZE_SETTLE_SECS:-3}"
    d2="$(node_sink_daa a)" || die "could not re-sample node A sink DAA."
    # An ACTIVE in-process DNS validator (--validator-mode=active) self-produces beacon/attestation
    # blocks for liveness (~1 block/s), so exact d1==d2 is unattainable once the beacon is up even
    # with every algo-3 miner paused. A small BOUNDED forward drift is harmless: E is derived from
    # d2 and the headroom check below guards the epoch boundary. FREEZE_DRIFT_TOLERANCE (default 40,
    # << the 100-DAA epoch) bounds it. A large/unbounded drift still means a rogue fast producer.
    # (Validated live on devnet-111, 2026-07-25 — the exact-equality gate could never pass against
    # a single active validator, which is why the mock mint had never run end-to-end before.)
    drift=$(( d2 - d1 ))
    { [ "$drift" -ge 0 ] && [ "$drift" -le "${FREEZE_DRIFT_TOLERANCE:-40}" ]; } \
        || die "DAA drifted $drift ($d1 -> $d2) beyond FREEZE_DRIFT_TOLERANCE=${FREEZE_DRIFT_TOLERANCE:-40} after pausing the supporting miner — a rogue fast block producer is active (a stray miner or an external mining peer via PALW_CONNECT_PEERS). Stop it and re-run."

    E="$(current_epoch "$d2")" || die "could not derive current epoch from sink DAA $d2."
    local activation_delay active_window
    activation_delay="${PALW_ACTIVATION_DELAY_EPOCHS:-8}"
    active_window="${PALW_ACTIVE_WINDOW_EPOCHS:-16}"
    case "$activation_delay" in ''|*[!0-9]*) die "PALW_ACTIVATION_DELAY_EPOCHS must be a positive integer." ;; esac
    case "$active_window" in ''|*[!0-9]*) die "PALW_ACTIVE_WINDOW_EPOCHS must be a positive integer." ;; esac
    [ "$activation_delay" -gt 0 ] || die "PALW_ACTIVATION_DELAY_EPOCHS must be > 0."
    [ "$active_window" -gt 0 ] || die "PALW_ACTIVE_WINDOW_EPOCHS must be > 0."
    ACT=$(( E + activation_delay ))
    EXP=$(( ACT + active_window ))

    # Headroom: the manifest (registration_epoch=E) MUST be submittable within
    # epoch E. palw_epoch_length_daa = 100. If too little of epoch E remains,
    # refuse now (fail-closed) rather than build a manifest that submit-lifecycle
    # could never register in time.
    rem=$(( 100 - ( d2 % 100 ) ))
    if [ "$rem" -lt "${MIN_EPOCH_HEADROOM_DAA:-20}" ]; then
        die "registration epoch $E has only $rem DAA of headroom before its boundary (< MIN_EPOCH_HEADROOM_DAA=${MIN_EPOCH_HEADROOM_DAA:-20}); a manifest registered now could not be submitted within epoch $E. Restart the supporting miner (./supporting-miner.sh start), let DAA advance into a fresh epoch, then re-run create-lifecycle. (The miner has been paused; restart it to proceed.)"
    fi
    log "registration epoch E=$E pinned (frozen sink DAA=$d2; $rem DAA of headroom before the boundary). activation=$ACT expiry=$EXP."

    # ---- 4b. STAGING dir + ADR-0045 D3-b PCPB evidence ------------------------
    # The staging dir is created here rather than at step 6 because the PCPB
    # artifacts are inputs to every later step, not outputs of them.
    install -d -m 0700 "$LIFECYCLE_DIR" || die "cannot create lifecycle dir $LIFECYCLE_DIR."
    local staging
    staging="$(mktemp -d "$LIFECYCLE_DIR/.staging.XXXXXX")" || die "mktemp -d for staging failed under $LIFECYCLE_DIR."
    register_cleanup "rm -rf '$staging'"

    _pcpb_derive "$E" "$staging"

    # The draw — not this harness — decides which providers hold the two seats, so
    # adopt them. A leaf that named its own pair would die at clause 12 with the
    # seats it declared missing from the evidence.
    local drawn_a drawn_b
    drawn_a="$(_pcpb_field 0 provider_a_bond)"
    drawn_b="$(_pcpb_field 0 provider_b_bond)"
    [ "$drawn_a" != "$drawn_b" ] || die "the PCPB draw returned one provider for both seats; pcpb-witness should have refused this anchor."
    if [ "$drawn_a" != "$PROV_A_BOND" ] || [ "$drawn_b" != "$PROV_B_BOND" ]; then
        log "PCPB seated providers by beacon draw, not by configuration: A=$drawn_a B=$drawn_b (configured A=$PROV_A_BOND B=$PROV_B_BOND)."
    fi
    PROV_A_BOND="$drawn_a"; PROV_B_BOND="$drawn_b"
    _parse_bond PROV_A_BOND "$PROV_A_BOND"; A_TXID="$_TXID"; A_IDX="$_IDX"
    _parse_bond PROV_B_BOND "$PROV_B_BOND"; B_TXID="$_TXID"; B_IDX="$_IDX"
    # `die` inside a command substitution kills only the subshell, so re-assert
    # every resolved value here rather than trusting the helper to have stopped us.
    PROV_A_KEY_F="$(_pcpb_seed_for "$PROV_A_BOND")"
    PROV_B_KEY_F="$(_pcpb_seed_for "$PROV_B_BOND")"
    [ -s "$PROV_A_KEY_F" ] || die "no owner seed resolved for the drawn seat-A bond $PROV_A_BOND."
    [ -s "$PROV_B_KEY_F" ] || die "no owner seed resolved for the drawn seat-B bond $PROV_B_BOND."
    [ "$PROV_A_KEY_F" != "$PROV_B_KEY_F" ] || die "both seats resolved to the same owner seed $PROV_A_KEY_F — the bond -> seed table is wrong."
    RSPK_A="$(_pcpb_reward_spk "$PROV_A_KEY_F")"
    RSPK_B="$(_pcpb_reward_spk "$PROV_B_KEY_F")"
    case "$RSPK_A" in 0000*) : ;; *) die "could not derive a reward SPK for seat A from $PROV_A_KEY_F." ;; esac
    case "$RSPK_B" in 0000*) : ;; *) die "could not derive a reward SPK for seat B from $PROV_B_KEY_F." ;; esac
    [ "$RSPK_A" != "$RSPK_B" ] || die "both seats derived the same reward script — consensus pays provider_bond_lock_spk of each bond's own owner, so this would misroute the payout."
    # Batch-scoped seat record. submit-lifecycle signs each da-response with the
    # CHALLENGED provider's owner key, and the challenged provider is whichever
    # bond the draw seated — not the configured PROV_A/PROV_B identities, which
    # stay untouched for the other stages that key off them.
    state_set PALW_SEAT_A_BOND "$PROV_A_BOND"
    state_set PALW_SEAT_A_KEY  "$PROV_A_KEY_F"
    state_set PALW_SEAT_B_BOND "$PROV_B_BOND"
    state_set PALW_SEAT_B_KEY  "$PROV_B_KEY_F"
    log "seat A: bond=$PROV_A_BOND reward_spk=$RSPK_A"
    log "seat B: bond=$PROV_B_BOND reward_spk=$RSPK_B"

    i=0
    while [ "$i" -lt "$LEAF_COUNT" ]; do
        PCPB_A_COMMIT[$i]="$(_pcpb_field "$i" a_commit)"
        PCPB_A_COMMIT_EPOCH[$i]="$(_pcpb_field "$i" a_commit_epoch)"
        PCPB_SNAP_ROOT[$i]="$(_pcpb_field "$i" provider_snapshot_root)"
        PCPB_ASSIGN_ROOT[$i]="$(_pcpb_field "$i" assignment_proof_root)"
        PCPB_DISPATCH_KIND[$i]="$(_pcpb_field "$i" dispatch_kind)"
        PCPB_ISSUED[$i]="$(_pcpb_field "$i" receipt_v3_issued_epoch)"
        PCPB_CHALLENGE[$i]="$(_lc "$(_pcpb_field "$i" receipt_v3_job_challenge)")"
        _is_hex128 "${PCPB_CHALLENGE[$i]}" || die "pcpb-witness did not yield a 128-hex job challenge for leaf $i."
        i=$(( i + 1 ))
    done

    # ---- 5. per-leaf uniqueness + ticket fields --------------------------------
    # Distinct per-leaf commitments. job_nullifier / receipt_v3_job_challenge are
    # public leaf fields (NOT the ticket secret). The DA object's semantic inputs
    # (JOBSET/OUTC/GEMM/OPSCHED) are object-internal — the object's DERIVED
    # commitment then becomes the leaf's receipt_da_root + private_match_commitment
    # in step 6-pre below. A mock leaf with a RANDOM receipt_da_root can never
    # satisfy the certificate DA-availability gate (no real object exists behind
    # it to build a chunk proof from), so DAROOT/DALEN/DACHUNK/PMC are set from a
    # real DA object, not rand_hex.
    local i
    i=0
    while [ "$i" -lt "$LEAF_COUNT" ]; do
        JOB_NF[$i]="$(rand_hex 64)"
        JOBCHAL[$i]="$(rand_hex 64)"
        JOBSET[$i]="$(rand_hex 64)"
        OUTC[$i]="$(rand_hex 64)"
        GEMM[$i]="$(rand_hex 64)"
        OPSCHED[$i]="$(rand_hex 64)"
        ROUTE[$i]="$(zero128)"
        STATE_ROOT[$i]="$(zero128)"
        CANON_CU[$i]="$QUANTUM_COUNT"
        TOKEN_COUNT[$i]="$QUANTUM_COUNT"
        STOP_REASON[$i]="0"
        i=$(( i + 1 ))
    done

    if [ "$TICKET_MODE" = mock ]; then
        # Draw one random 64-byte nullifier per leaf into a 0600 file (NEVER on
        # argv/log), open its commitment + authority pk_hash via the helper now,
        # and keep the file for the post-manifest store-add.
        NF_TMPDIR="$(mktemp -d "$PALW_DATA_ROOT/keys/.nf.XXXXXX")" || die "mktemp -d for nullifier tmpdir failed under $PALW_DATA_ROOT/keys."
        chmod 0700 "$NF_TMPDIR" 2>/dev/null || true
        register_cleanup "rm -rf '$NF_TMPDIR'"
        local auth0=""
        i=0
        while [ "$i" -lt "$LEAF_COUNT" ]; do
            ( umask 077; rand_hex 64 > "$NF_TMPDIR/nf-$i.hex" ) || die "failed to generate a mock ticket nullifier for leaf $i."
            _mock_commit "$NF_TMPDIR/nf-$i.hex"
            TNC[$i]="$_MC_COMMIT"
            if [ "$i" -eq 0 ]; then auth0="$_MC_AUTH"; fi
            [ "$_MC_AUTH" = "$auth0" ] || die "mock-ticket returned inconsistent ticket_authority_pk_hash across leaves (leaf $i) — one authority key must sign all leaves."
            TAPKH[$i]="$_MC_AUTH"
            i=$(( i + 1 ))
        done
        log "TICKET_MODE=mock: opened ticket_nullifier_commitment for $LEAF_COUNT MOCK leaf/leaves (WIRING-ONLY, NON-inference); TicketSecretStore is populated after the manifest fixes the batch_id."
    elif [ "$TICKET_MODE" = real ]; then
        NF_TMPDIR="$(mktemp -d "$PALW_DATA_ROOT/keys/.real-nf.XXXXXX")" \
            || die "mktemp -d for real ticket material failed."
        chmod 0700 "$NF_TMPDIR" 2>/dev/null || true
        register_cleanup "rm -rf '$NF_TMPDIR'"
        REAL_PROOF_TMP="$NF_TMPDIR/real-provider-proof.json"
        # Re-attest BEFORE verifying: the gate must check the job this batch will
        # register, which D3-b pins to the chain's challenge.
        _real_reemit_receipts "${PCPB_CHALLENGE[0]}" "${PCPB_ISSUED[0]}" "$EXP" "$NF_TMPDIR"
        _real_verify_and_derive "$NF_TMPDIR/nf-0.hex" "$REAL_PROOF_TMP"
        JOB_NF[0]="$(_lc "$_RP_CHALLENGE")"
        [ "${JOB_NF[0]}" = "${PCPB_CHALLENGE[0]}" ] \
            || die "the verified receipts carry job challenge ${JOB_NF[0]} but clause 11 will re-derive ${PCPB_CHALLENGE[0]} — the re-attestation did not take."
        JOBSET[0]="$(_lc "$_RP_JOBSET")"
        OUTC[0]="$(_lc "$_RP_OUT")"
        GEMM[0]="$(_lc "$_RP_EXECUTION")"
        OPSCHED[0]="$(_lc "$_RP_SCHEDULE")"
        ROUTE[0]="$(_lc "$_RP_ROUTE")"
        STATE_ROOT[0]="$(_lc "$_RP_STATE")"
        CANON_CU[0]="$_RP_CU"
        TOKEN_COUNT[0]="$_RP_TOKENS"
        STOP_REASON[0]="$_RP_STOP"
        MODEL_PROFILE_ID="$(_lc "$_RP_MODEL")"
        DESCRIPTOR_ROOT="$(_lc "$_RP_PROOF")"
        TNC[0]="$(_lc "$_RP_COMMIT")"
        TAPKH[0]="$(_lc "$_RP_AUTH")"
        REAL_EXTERNAL_PAIR_ID="$(_lc "$_RP_PAIR")"
        REAL_RECEIPT_A_ID="$(_lc "$_RP_RECEIPT_A")"
        REAL_RECEIPT_B_ID="$(_lc "$_RP_RECEIPT_B")"
        log "TICKET_MODE=real: Qwen Receipt-v3 A/B ML-DSA-87 signatures, distinct workers, exact k=2 projection, and output tokens verified; pair=$_RP_PAIR."
        log "verified projection: model=$MODEL_PROFILE_ID CU=${CANON_CU[0]} tokens=${TOKEN_COUNT[0]} stop=${STOP_REASON[0]} proof=$DESCRIPTOR_ROOT."
    else
        # skip mode: fixed placeholders (never opened).
        i=0
        while [ "$i" -lt "$LEAF_COUNT" ]; do
            TNC[$i]="$TICKET_NF_PLACEHOLDER"
            TAPKH[$i]="$TICKET_AUTH_PLACEHOLDER"
            i=$(( i + 1 ))
        done
        log "TICKET_MODE=skip: placeholder ticket fields (never opened; submit-lifecycle uses --unsafe-skip-ticket-secret-check -> batch.status=active, no mint)."
    fi

    # 6-pre. Build a REAL DA object per leaf (author-time, batch_id=0). The object's
    #   derived commitment root/len/chunk_count + private_match_commitment become the
    #   leaf's receipt_da_* / private_match_commitment, so register_leaf_obligations
    #   creates obligations whose object_root a da-response chunk proof can satisfy
    #   (submit-lifecycle drives the challenge/response). The object stays at the
    #   author-time all-zero batch_id: the DA obligation/response path checks only the
    #   Merkle root of the SAME bytes we commit here (consensus/core/src/palw/da.rs
    #   register_leaf_obligations + apply_response), NOT the object's internal batch_id
    #   (that binding is only enforced by the optional P2P admission path, which is not
    #   on the certificate-gate critical path). network_id = NETSUFFIX = the node's
    #   params.net.suffix(); the object's network_id is not re-checked on this path but
    #   is set correctly for consistency.
    local objdir; objdir="$staging/da"
    install -d -m 0700 "$objdir" || die "cannot create DA object dir $objdir."
    # The seat keys were resolved in step 4b from the bonds the beacon drew — the
    # session authorization has to be signed by the owner of the bond the leaf
    # names, and that bond is no longer a configuration choice.
    [ -s "$PROV_A_KEY_F" ] || die "seat A owner seed not found: $PROV_A_KEY_F — the DA object's session authorization is signed by this owner key."
    [ -s "$PROV_B_KEY_F" ] || die "seat B owner seed not found: $PROV_B_KEY_F."
    case "$NETSUFFIX" in ''|*[!0-9]*) die "NETSUFFIX='$NETSUFFIX' is not a u32 network id (da-object-build --network-id must equal the node's params.net.suffix())." ;; esac
    GENESIS_NETWORK_ID="${PALW_GENESIS_NETWORK_ID:-${EXPECTED_GENESIS_HASH:-}}"
    _is_hex128 "$GENESIS_NETWORK_ID" || die "PALW_GENESIS_NETWORK_ID must be the node's 128-hex Header-v4 genesis identity."
    log "building $LEAF_COUNT real DA object(s) (da-object-build, batch_id=0, network_id=$NETSUFFIX, completed_at_epoch=$E) -> $objdir/<root>.palwobj"
    i=0
    while [ "$i" -lt "$LEAF_COUNT" ]; do
        local obj_out da_version da_root da_len da_chunks da_pmc
        if ! obj_out="$("$VAL" palw-payload da-object-build \
                --network-id "$NETSUFFIX" \
                --genesis-network-id "$GENESIS_NETWORK_ID" \
                --leaf-index "$i" \
                --provider-a-bond "$PROV_A_BOND" \
                --provider-a-owner-key "$PROV_A_KEY_F" \
                --provider-b-bond "$PROV_B_BOND" \
                --provider-b-owner-key "$PROV_B_KEY_F" \
                --valid-from-epoch "${PCPB_ISSUED[$i]}" \
                --valid-until-epoch "$EXP" \
                --completed-at-epoch "$E" \
                --job-nullifier "${JOB_NF[$i]}" \
                --job-set-commitment "${JOBSET[$i]}" \
                --model-profile-id "$MODEL_PROFILE_ID" \
                --runtime-class-id "$RUNTIME_CLASS" \
                --shape-id "$SHAPE_ID" \
                --quantum-count "$QUANTUM_COUNT" \
                --output-commitment "${OUTC[$i]}" \
                --canonical-gemm-trace-root "${GEMM[$i]}" \
                --operation-schedule-commitment "${OPSCHED[$i]}" \
                --route-root "${ROUTE[$i]}" \
                --state-root "${STATE_ROOT[$i]}" \
                --canonical-compute-units "${CANON_CU[$i]}" \
                --token-count "${TOKEN_COUNT[$i]}" \
                --stop-reason "${STOP_REASON[$i]}" \
                --out "$objdir/obj-$i.palwobj" 2>&1)"; then
            printf '%s\n' "$obj_out" >&2
            die "'palw-payload da-object-build' failed for leaf $i (see output above)."
        fi
        da_version="$(printf '%s\n' "$obj_out" | _kv object_version)"
        da_root="$(printf '%s\n'   "$obj_out" | _kv receipt_da_root)"
        da_len="$(printf '%s\n'    "$obj_out" | _kv receipt_da_object_len)"
        da_chunks="$(printf '%s\n' "$obj_out" | _kv receipt_da_chunk_count)"
        da_pmc="$(printf '%s\n'    "$obj_out" | _kv private_match_commitment)"
        case "$da_version" in ''|*[!0-9]*) printf '%s\n' "$obj_out" >&2; die "da-object-build leaf $i: non-integer object_version." ;; esac
        [ "$da_version" -eq 2 ] || { printf '%s\n' "$obj_out" >&2; die "da-object-build leaf $i produced object_version=$da_version; header-v4 requires DA object version 2."; }
        _is_hex128 "$da_root" || { printf '%s\n' "$obj_out" >&2; die "da-object-build for leaf $i did not print a 128-hex receipt_da_root (see above)."; }
        _is_hex128 "$da_pmc"  || { printf '%s\n' "$obj_out" >&2; die "da-object-build for leaf $i did not print a 128-hex private_match_commitment (see above)."; }
        case "$da_len"    in ''|*[!0-9]*) printf '%s\n' "$obj_out" >&2; die "da-object-build leaf $i: non-integer receipt_da_object_len." ;; esac
        case "$da_chunks" in ''|*[!0-9]*) printf '%s\n' "$obj_out" >&2; die "da-object-build leaf $i: non-integer receipt_da_chunk_count." ;; esac
        DAVERSION[$i]="$da_version"
        DAROOT[$i]="$(_lc "$da_root")"
        DALEN[$i]="$da_len"
        DACHUNK[$i]="$da_chunks"
        PMC[$i]="$(_lc "$da_pmc")"
        # Name the object file by its root so submit-lifecycle can resolve an
        # obligation's object_root -> bytes without a leaf_index side channel.
        mv "$objdir/obj-$i.palwobj" "$objdir/${DAROOT[$i]}.palwobj" \
            || die "failed to name DA object for leaf $i by its root."
        log "leaf $i DA object: version=${DAVERSION[$i]} root=${DAROOT[$i]} len=${DALEN[$i]} chunks=${DACHUNK[$i]}"
        i=$(( i + 1 ))
    done

    # 6a. author the UNBOUND leaf-set JSON.
    local last; last=$(( LEAF_COUNT - 1 ))
    log "authoring unbound leaf-set ($LEAF_COUNT leaf/leaves) -> leafset.json (schema misaka.palw.leaf-set.v1)"
    {
        printf '{\n'
        printf '  "schema": "misaka.palw.leaf-set.v1",\n'
        printf '  "leaves": [\n'
        i=0
        while [ "$i" -lt "$LEAF_COUNT" ]; do
            emit_leaf "$i"
            if [ "$i" -lt "$last" ]; then printf '    ,\n'; fi
            i=$(( i + 1 ))
        done
        printf '  ]\n'
        printf '}\n'
    } > "$staging/leafset.json" || die "failed to write $staging/leafset.json."
    [ -s "$staging/leafset.json" ] || die "leaf-set JSON came out empty ($staging/leafset.json)."

    # 6b. build the batch-manifest OFFLINE (no node RPC, no block). Records the
    #     content-derived batch_id and the restamped (batch-bound) leaves file
    #     that leaf-chunk consumes.
    log "building batch-manifest OFFLINE (registration_epoch=$E) -> manifest.borsh + leaves.batch.json"
    local man_out batch_id chunk_count mleaf act_nb exp_ep
    if ! man_out="$("$VAL" palw-payload batch-manifest \
            --network "$NETWORK" \
            --leaves-file "$staging/leafset.json" \
            --registration-epoch "$E" \
            --descriptor-root "$DESCRIPTOR_ROOT" \
            --audit-policy-id "$AUDIT_POLICY_ID" \
            --out "$staging/manifest.borsh" \
            --restamped-leaves-out "$staging/leaves.batch.json" 2>&1)"; then
        printf '%s\n' "$man_out" >&2
        die "'palw-payload batch-manifest' failed (see output above). Common causes: a prebound leaf (batch_id != 0), non-contiguous leaf_index, leaves not sharing model_profile_id/runtime_class_id, or a leaf registered_epoch != $E."
    fi
    [ -s "$staging/manifest.borsh" ]    || die "batch-manifest produced an empty manifest ($staging/manifest.borsh)."
    [ -s "$staging/leaves.batch.json" ] || die "batch-manifest produced an empty restamped-leaves file ($staging/leaves.batch.json)."

    batch_id="$(printf '%s\n' "$man_out"    | _kv batch_id)"
    chunk_count="$(printf '%s\n' "$man_out" | _kv chunk_count)"
    mleaf="$(printf '%s\n' "$man_out"       | _kv leaf_count)"
    act_nb="$(printf '%s\n' "$man_out"      | _kv activation_not_before_epoch)"
    exp_ep="$(printf '%s\n' "$man_out"      | _kv expiry_epoch)"

    _is_hex128 "$batch_id" || { printf '%s\n' "$man_out" >&2; die "could not parse a 128-hex batch_id from batch-manifest output (see above)."; }
    [ "$batch_id" != "$BATCH_UNBOUND" ] || die "batch-manifest returned an all-zero batch_id — the leaves were not bound."
    batch_id="$(_lc "$batch_id")"
    case "$chunk_count" in ''|*[!0-9]*) printf '%s\n' "$man_out" >&2; die "could not parse an integer chunk_count from batch-manifest output (see above)." ;; esac
    [ "$chunk_count" -ge 1 ] || die "batch-manifest reported chunk_count=$chunk_count (expected >= 1)."
    # Soft consistency checks (the manifest itself is the authority on the math).
    case "$mleaf"  in ''|*[!0-9]*) : ;; *) [ "$mleaf" = "$LEAF_COUNT" ] || warn "manifest leaf_count=$mleaf differs from LEAF_COUNT=$LEAF_COUNT." ;; esac
    case "$act_nb" in ''|*[!0-9]*) : ;; *) [ "$act_nb" = "$ACT" ] || warn "manifest activation_not_before_epoch=$act_nb differs from configured value $ACT." ;; esac
    case "$exp_ep" in ''|*[!0-9]*) : ;; *) [ "$exp_ep" = "$EXP" ] || warn "manifest expiry_epoch=$exp_ep differs from configured value $EXP." ;; esac
    log "batch-manifest OK: batch_id=$batch_id leaf_count=${mleaf:-$LEAF_COUNT} chunk_count=$chunk_count activation=${act_nb:-$ACT} expiry=${exp_ep:-$EXP}"

    # 6c. build every leaf-chunk OFFLINE from the RESTAMPED (batch-bound) leaves.
    local k chunk_out
    k=0
    while [ "$k" -lt "$chunk_count" ]; do
        log "building leaf-chunk $k/$(( chunk_count - 1 )) OFFLINE -> chunk-$k.borsh"
        if ! chunk_out="$("$VAL" palw-payload leaf-chunk \
                --network "$NETWORK" \
                --manifest-file "$staging/manifest.borsh" \
                --leaves-file "$staging/leaves.batch.json" \
                --chunk-index "$k" \
                --pcpb-witness-file "$PCPB_WITNESS_FILE" \
                --out "$staging/chunk-$k.borsh" 2>&1)"; then
            printf '%s\n' "$chunk_out" >&2
            die "'palw-payload leaf-chunk' failed for chunk-index $k (see output above)."
        fi
        [ -s "$staging/chunk-$k.borsh" ] || die "leaf-chunk $k produced an empty file ($staging/chunk-$k.borsh)."
        k=$(( k + 1 ))
    done

    # 6d. Populate the TicketSecretStore now that batch_id is fixed.
    #     Done BEFORE finalize so a store-add failure leaves the bundle unrecorded
    #     (idempotent rebuild) rather than a recorded bundle with a partial store.
    if [ "$TICKET_MODE" = mock ]; then
        log "populating TicketSecretStore ($TICKET_SECRET_FILE) for $LEAF_COUNT MOCK leaf/leaves (WIRING-ONLY, NON-inference; NOT palw_demo)"
        i=0
        while [ "$i" -lt "$LEAF_COUNT" ]; do
            _mock_store_add "$batch_id" "$i" "$NF_TMPDIR/nf-$i.hex"
            i=$(( i + 1 ))
        done
    elif [ "$TICKET_MODE" = real ]; then
        log "populating TicketSecretStore with the ticket derived from verified Qwen inference."
        _real_store_add "$batch_id" 0 "$NF_TMPDIR/nf-0.hex"
        install -d -m 0755 "$staging/real-provider" || die "cannot create staged real-provider proof directory."
        cp "$REAL_PROOF_TMP" "$staging/real-provider/proof.json" || die "cannot stage real provider proof."
        cp "$REAL_RECEIPT_A" "$staging/real-provider/receipt-a.json" || die "cannot stage receipt A."
        cp "$REAL_RECEIPT_B" "$staging/real-provider/receipt-b.json" || die "cannot stage receipt B."
        cp "$REAL_RESULT_A" "$staging/real-provider/result-a.json" || die "cannot stage worker result A."
        cp "$REAL_RESULT_B" "$staging/real-provider/result-b.json" || die "cannot stage worker result B."
    fi

    # ---- 7. finalize: move the staged bundle into place, then record state -----
    # The PCPB evidence is part of the bundle's provenance: it is what a reviewer
    # replays to see why these two providers and this challenge, and submit-lifecycle
    # has no way to re-derive it once the anchor's beacon window has moved on.
    mv "$PCPB_WITNESS_FILE" "$LIFECYCLE_DIR/pcpb-witness.borsh" || die "failed to finalize pcpb-witness.borsh."
    mv "$PCPB_FIELDS_FILE"  "$LIFECYCLE_DIR/pcpb-fields.json"   || die "failed to finalize pcpb-fields.json."
    mv "$staging/leafset.json"      "$LIFECYCLE_DIR/leafset.json"      || die "failed to finalize leafset.json."
    mv "$staging/manifest.borsh"    "$LIFECYCLE_DIR/manifest.borsh"    || die "failed to finalize manifest.borsh."
    mv "$staging/leaves.batch.json" "$LIFECYCLE_DIR/leaves.batch.json" || die "failed to finalize leaves.batch.json."
    k=0
    while [ "$k" -lt "$chunk_count" ]; do
        mv "$staging/chunk-$k.borsh" "$LIFECYCLE_DIR/chunk-$k.borsh" || die "failed to finalize chunk-$k.borsh."
        k=$(( k + 1 ))
    done
    # DA objects (named by root) — submit-lifecycle reads these to build the
    # da-response chunk proof that satisfies each obligation. Replace any stale dir.
    rm -rf "$LIFECYCLE_DIR/da"
    mv "$objdir" "$LIFECYCLE_DIR/da" || die "failed to finalize the DA object dir."
    if [ "$TICKET_MODE" = real ]; then
        rm -rf "$LIFECYCLE_DIR/real-provider"
        mv "$staging/real-provider" "$LIFECYCLE_DIR/real-provider" \
            || die "failed to finalize the real-provider proof bundle."
    fi

    state_set PALW_BATCH_ID    "$batch_id"
    state_set PALW_CHUNK_COUNT "$chunk_count"
    state_set PALW_LEAF_COUNT  "$LEAF_COUNT"
    state_set PALW_REG_EPOCH   "$E"
    state_set PALW_DA_OBJ_DIR  "$LIFECYCLE_DIR/da"
    state_set PALW_DA_REAL     "1"
    if [ "$TICKET_MODE" = real ]; then
        state_set PALW_REAL_EXTERNAL_PAIR_ID "$REAL_EXTERNAL_PAIR_ID"
        state_set PALW_REAL_RECEIPT_A_ID "$REAL_RECEIPT_A_ID"
        state_set PALW_REAL_RECEIPT_B_ID "$REAL_RECEIPT_B_ID"
        state_set PALW_REAL_PROOF_COMMITMENT "$DESCRIPTOR_ROOT"
        state_set PALW_REAL_MODEL_PROFILE_ID "$MODEL_PROFILE_ID"
        state_set PALW_REAL_CANONICAL_COMPUTE_UNITS "${CANON_CU[0]}"
        state_set PALW_REAL_TOKEN_COUNT "${TOKEN_COUNT[0]}"
    fi

    # ---- 8. honest summary -----------------------------------------------------
    _LIFECYCLE_OK=1
    log "create-lifecycle complete (STN-011): batch_id=$batch_id leaves=$LEAF_COUNT chunks=$chunk_count registration_epoch=$E (activation=$ACT expiry=$EXP). Bundle under $LIFECYCLE_DIR."
    if [ "$TICKET_MODE" = skip ]; then
        log "the leaf(s) are placeholders — no real inference was performed; palw_demo is NOT used."
        log "TICKET_MODE=skip: submit-lifecycle registers the leaf-chunk with --unsafe-skip-ticket-secret-check -> reaches batch.status=active but the block can NEVER be mined (no ticket)."
    elif [ "$TICKET_MODE" = mock ]; then
        log "the leaf(s) are mock wiring leaves — no real inference was performed; palw_demo is NOT used."
        log "TICKET_MODE=mock: TicketSecretStore populated -> a WIRING-ONLY, non-inference block becomes mineable via start-palw-miner.sh after submit reaches batch.status=active."
    else
        log "TICKET_MODE=real: verified Qwen k=2 proof bundle finalized at $LIFECYCLE_DIR/real-provider; the inference-bound ticket makes the algo-4 leaf mineable after activation."
    fi
    log "SUPPORTING MINER LEFT PAUSED (DAA frozen at epoch $E) — intentional. submit-lifecycle.sh resumes the miner and submits the carriers within epoch $E; do NOT let DAA advance past epoch $E before submitting."
    return 0
}

# ---------------------------------------------------------------------------
# Dispatch. Validate the argument before load_env so --help works unconfigured.
ACTION="${1:-}"
case "$ACTION" in
    -h|--help|help) usage; exit 0 ;;
    ""|create)      : ;;
    *)              usage; die "unknown argument '$ACTION' (this stage takes no argument, or 'create')." ;;
esac

load_env

# §13.3 storage SLO gate — refuse to register a NEW batch when disk free is
# below the STOP threshold (default 20%): a full disk mid-lifecycle corrupts
# RocksDB on both nodes. disk-slo.sh also appends a growth-history sample.
if [ -f "$SCRIPT_DIR/disk-slo.sh" ]; then
    bash "$SCRIPT_DIR/disk-slo.sh" gate lifecycle \
        || die "disk SLO gate refused a new lifecycle (see disk-slo.sh output above; §13.3)"
fi

do_create
