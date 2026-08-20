# The remaining PALW wiring, and why it lands as one unit (ADR-0044 FP-08, ADR-0042 PR-08)

Status: the substrate is landed and tested; the pipeline wiring below is **not** started. This
document exists because the wiring is where every P0 in this project's audit history was born,
and because the safest thing to write before wiring is the list of things that must not be
wired one at a time.

## The finding this document records

Both lanes' PoW arms are absent from the finalizer today. `StateLayer0::calculate_l1_tag`
(`consensus/pow/src/lib.rs`) has arms for algo 1/2/3/4/5 and falls to
`Err(UnknownAlgoId(other))` for **6 (attempt) and 7 (receipt)** alike. A `ConsensusV2` network
therefore demands algo 6 at the header gate and then rejects every algo-6 header as
`InvalidPoW` — *including* under `skip_proof_of_work`, because the error returns before the skip
is consulted. This is deliberate ("the arm … lands with it in PR-10") and it is also why a V2
network cannot currently mine or accept a single block.

The temptation is to add a small tag arm and move on. That would be the mistake: an algo id with
a tag but no admission is a *half-defined consensus semantics*, and half-defined semantics is
what produced P0-1 (a solved PoW minting unlimited identities), P0-3 (fresh tips unweighable),
P0-4 (sink-dependent weights) and P0-5 (two canonical-chain views in one node).

## The atomic units

Nothing inside a unit may ship without the rest of that unit on the same branch, behind the same
mode.

### Unit A — the attempt lane's PoW (ADR-0042 PR-10's remainder)

1. `calculate_l1_tag` arm for algo 6: `Expand(commitment_root)` from the header's carried
   attempt envelope (`l1_tag_v2`), **and** the decode of that envelope from
   `header.palw_commitment` under the V2 codec (today `check_palw_commitment_shape` decodes the
   V1 PBC1 shape; a V2 header carries a `PalwAttemptEnvelopeV2`).
2. The header-shape gate arm that requires the envelope be present and well-formed on a V2
   network (empty is currently the rule).
3. Stateless admission at the header stage: `validate_stateless_v2` + `validate_signature_v2`.
4. The one property that makes the lane sound, as a test: mutating any priced field fails the
   PoW *through the live path*, not only through the unit-test helper.

Without (1) no V2 block validates; without (2)+(3) a V2 block validates with an unchecked
envelope, which is P0-1 restated.

### Unit B — the receipt lane's PoW and admission

1. `calculate_l1_tag` arm for algo 7: `Expand(fp_spend_id_v3)` from the header's carried spend
   envelope, and the decode of that envelope (a V3 codec, distinct from both V1 PBC1 and the
   attempt's V2 — the shape gate currently refuses non-empty carriage for algo 7 precisely so
   this cannot half-exist).
2. **The target-comparison semantics for a nonce-free block.** This is the subtle one and the
   reason Unit B cannot be "just another arm": a receipt block's header hash is costlessly
   malleable by its producer, so a bits comparison is a filter the producer grinds through for
   free while honest software stalls on it. The lottery for algo 7 is
   `check_palw_receipt_spend_admission_v3` item 5 — the quantum ticket against the receipt
   target — and the finalizer path must treat the digest as identity binding with the target
   comparison satisfied by construction. Wiring algo 7 into the ordinary
   `digest <= target` branch would be a live defect, and it would look like correctness.
3. Stateful admission at the block stage, against the candidate chain's `PalwChainStateV2`:
   all eight items, with the beacon fact supplied by the pipeline (below).
4. Block weight: `pwu_per_quantum` at `Final` stage on acceptance — no ramp, no revision.

### Unit C — candidate-scoped state in the pipeline

1. A store for `PalwStateDeltaV2` keyed by chain block, written in the same `WriteBatch` as the
   block's UTXO data, plus a materialized anchor and a carriage row at the pruning point.
2. The walk in `calculate_utxo_state_relatively`: `revert_delta_v2` on the reverse leg beside
   `as_reversed()`, `apply_delta_v2` on the forward leg beside `with_diff_in_place`, and
   `apply_palw_transition_v3` in the `KeyNotFound` arm — in lockstep with the UTXO diff, exactly
   as `ActiveBondView` already walks.
3. Objects from carriage: accepted FP commitment transactions (subnetwork `0x4a`) become
   `FreePromptCommitted` objects in the block's deterministic acceptance order; panel/receipt/
   court objects likewise from their carriage.
4. **Beacon facts from the chain, not from the block.** `PalwBeaconFactV3` must be constructed
   by the pipeline from its own candidate chain — "the first attempt-class chain block at or
   after the slot", with `prev_attempt_daa` as the checkable witness. A fact taken from the
   spending block's own bytes would be the producer asserting its own randomness.
5. `palw_state_root` in the header, verified beside `overlay_commitment_root`.

The reorg-equivalence gate for this unit is already committed
(`palw_state_v2::tests::fp_reorg_by_delta_equals_building_the_winning_branch_fresh` and its two
siblings): the wiring is written against tests that are red on regression, rather than
discovered to have forked in the field.

### Unit D — one fork-choice authority

`palw_fork_authority_v2`'s four functions wired into virtual tip selection, IBD commit, pruning
ceiling and the deep-reorg gate **together**, with the header processor's store renamed to a
download hint (ADR-0042 Decision 9). Wiring one site is P0-5: two canonical-chain views inside
one node.

## What is safe to do before the units

Everything landed so far, and this list is the reason each piece was chosen:

- pure objects and identities (FP-01), an algo id nothing demands (FP-02), the state machine and
  its reorg gate (FP-03/08c), admission as a pure predicate (FP-04), the bundle and its startup
  invariants (FP-05/09a), the sidecars (FP-06/07), retention (FP-08a), signing purposes (FP-08b),
  and the transaction codec + builder (FP-08d).
- Each is either unreachable from consensus or gated behind a mode no preset sets, and each
  brings its own test that the *next* layer must not break.

## Sequencing note for the fleet drill (FP-09 stage 2)

The drill needs Units A–D on a devnet preset carrying
`palw_fp_devnet_bundle_v3`. Until then the honest drill is the one already run: the sidecar path
end to end on the real model (`scripts/misaka-palw-fp-v3-worker-smoke.py`,
`scripts/misaka-palw-fp-gateway-smoke.py`), which measures everything that does not require a
chain — one inference producing answer and commitment, arm equality, retention, and the
transaction the executor rail would submit.
