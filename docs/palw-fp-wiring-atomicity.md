# The remaining PALW wiring, and why it lands as one unit (ADR-0044 FP-08, ADR-0042 PR-08)

Status, updated: **Units A–E are landed and wired, and FP-09's drill has run against them.**
This document was written before any of it, because the wiring is where every P0 in this
project's audit history was born; it now records what each unit turned out to be.

## What the units cost, in defects found

Writing them under this discipline surfaced six defects *before* they could ship. The first three
are one family — an object that was not bound to its position or its state:

1. **The spend envelope had no header-position binding** (found while designing Unit B): one
   signature would have minted unlimited block identities, P0-1's shape arriving through the lane
   with no PoW at all. Fixed by the `challenge` field, recomputed at admission.
2. **The walk tracked its position beside the state** (found by Unit C's own integration test):
   an `at` field set to the block just processed is right after an apply and WRONG after a
   revert, where the walk stands at that block's parent. The anchor written from such a walk
   claimed a position the state did not hold. Fixed by deriving the position from the state's own
   `last_point`, which makes the drift unrepresentable.
3. **`palw_candidate_order` panicked on an unknown hash** (found by Unit D's prerequisite test):
   "no opinion" delivered as a panic, on a path whose callers see peer-supplied hashes.

The last three came from Units D and E and from running the drill:

4. **Unvalidated candidates cannot be ordered** (Unit D): a tip with no delta has no PALW
   standing, so ranking the sink search's heap by the comparator would be ranking by a state
   nobody computed. The first draft did exactly that and could not even mine. The heap stays a
   SEARCH order; the authority sits at the point the sink MOVES.
5. **ADR-0043 §2's frozen preimage was stale** (Unit E): it listed the state root without
   `receipt_targets` or `receipt_epoch_counters`, which `state_root` has covered since the receipt
   lane landed. The code was right; the written record — the thing a second implementation would
   be built from — was not.
6. **The rail could not build a transaction at all** (FP-09's drill): it stated a placeholder
   catalog root, and the gate that checks the root against the class list landed later, in Unit C.
   The rail smoke was green at `2c264313` and red from `d10e23b7` onward, unnoticed for the whole
   interval. **A real-model script no CI runs is a test that stops being true silently** — which
   is the argument for `scripts/misaka-palw-fp-fleet-drill.py` being one command.

None of these would have been caught by reading the code; each was caught by a test, or a drill,
written to state a property the unit had to hold.

## The original finding (resolved by Units A and B)

Both lanes' PoW arms were absent from the finalizer: `calculate_l1_tag` had arms for algo 1/2/3/4/5
and fell to `Err(UnknownAlgoId)` for **6 (attempt) and 7 (receipt)** alike, so a `ConsensusV2`
network demanded algo 6 and then rejected every algo-6 header — *including* under
`skip_proof_of_work`, because the error returns before the skip is consulted.

The temptation was to add a small tag arm and move on. That would have been the mistake: an algo
id with a tag but no admission is a *half-defined consensus semantics*, which is what produced
P0-1 (a solved PoW minting unlimited identities), P0-3 (fresh tips unweighable), P0-4
(sink-dependent weights) and P0-5 (two canonical-chain views in one node). Both arms landed with
their carriage codecs, their shape gates and their stateless admission, in one commit.

## The atomic units

Nothing inside a unit may ship without the rest of that unit on the same branch, behind the same
mode.

### Unit A — the attempt lane's PoW (ADR-0042 PR-10's remainder) — **LANDED**

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

### Unit B — the receipt lane's PoW and admission — **LANDED**

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

### Unit C — candidate-scoped state in the pipeline — **LANDED (with two carve-outs, below)**

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

### Unit D — one fork-choice authority — **NOT STARTED; its prerequisite is landed**

`palw_fork_authority_v2`'s four functions wired into virtual tip selection, IBD commit, pruning
ceiling and the deep-reorg gate **together**, with the header processor's store renamed to a
download hint (ADR-0042 Decision 9). Wiring one site is P0-5: two canonical-chain views inside
one node.

What was missing until now was the *capability* every one of the four needs: a candidate's
standing under the one comparator, derived from that candidate's own chain.
`VirtualStateProcessor::palw_candidate_order` is that, landed and tested (including the losing
branch of a fork, which a sink-scoped read gets wrong). Unit D is therefore now a single coherent
change — call it from four places — rather than a research problem.

### Unit C's two carve-outs, stated precisely

The walk runs, stores its deltas, survives reorgs, and seeds its anchor at genesis. Two things
inside Unit C are deliberately still absent:

1. **The block's own attempt/spend work is not applied.** `palw_apply_block` folds the block's
   accepted free-prompt commitments and passes `PalwBlockWorkV3::None`. Applying the work
   requires admitting it against this very state — the stateful admission call sites — and
   admitting-then-applying must be one step.
2. **The state root is not in the header.** `palw_state_root` beside `overlay_commitment_root`
   (ADR-0043 owns the hash ordering) is what makes a peer's state checkable rather than
   recomputed-and-hoped.

Folding objects before admitting work is safe **in that order**: the transition refuses an object
naming an absent bond or a frozen class, so the derived state can only ever be a SUBSET of what a
complete wiring holds, never a superset. The reverse order — applying work before it is admitted —
would not be.

### Unit E — the pruning point's PALW state (ADR-0042 Decision 5) — **LANDED**

1. One decoder for the carriage, shared by the block validator and the import gate, keyed on the
   BUNDLE's lane ids (`palw_fp_carriage_v3`).
2. Capture at pruning-advance, walking forward from the PREVIOUS pruning carriage — the anchor
   tracks the sink, a full pruning depth above.
3. The import gate: `into_state(_, Some(committed_root))` against the root a child header of the
   pruning point committed, and a REFUSAL when no such header exists.
4. The wire pair (`RequestPalwPruningCarriage` / `PalwPruningCarriage`), both routes registered —
   request on the serving side, response on the IBD side, because a reply nobody subscribed to
   disconnects the peer.

## What is safe to do before the units

Everything landed so far, and this list is the reason each piece was chosen:

- pure objects and identities (FP-01), an algo id nothing demands (FP-02), the state machine and
  its reorg gate (FP-03/08c), admission as a pure predicate (FP-04), the bundle and its startup
  invariants (FP-05/09a), the sidecars (FP-06/07), retention (FP-08a), signing purposes (FP-08b),
  and the transaction codec + builder (FP-08d).
- Each is either unreachable from consensus or gated behind a mode no preset sets, and each
  brings its own test that the *next* layer must not break.

## Pre-existing test-suite state, measured (so a wiring PR is not blamed for it)

Measured 2026-08-20 on this branch AND on its base `palw-v2`, with identical results — these
failures predate the free-prompt work and are not caused by it:

| Suite | Result on `palw-freeprompt-v3` | Result on `palw-v2` (base) |
|---|---|---|
| `kaspa-consensus-core --lib` | 1074 passed, 0 failed | — (1046 before FP) |
| `kaspa-consensus --lib` | 217 passed, 0 failed | 207 passed |
| `cargo clippy -p kaspa-consensus --lib` | byte-identical warning list | (the baseline) |
| every other workspace crate | passed | passed |
| `kaspa-testing-integration --lib consensus_` | 18 passed, **2 failed** (`bounded_merge_depth_test`, `indirect_parents_test`) | 18 passed, **the same 2 failed** |
| `kaspa-testing-integration --lib` (full, parallel) | aborts in the daemon tests | aborts likewise (different signal, same suite) |

Anyone wiring Units A–D should re-measure this table first: the two red consensus-integration
tests are the *existing* baseline, and the daemon-suite abort is an environment/harness issue in
the same binary. A wiring PR is clean when it leaves this table unchanged, not when the suite is
green.

## The fleet drill (FP-09) — run

`scripts/misaka-palw-fp-fleet-drill.py` is one command over the whole path that exists: the three
real-model sidecar smokes, the **seam** (a transaction the rail really built, read by the
consensus extractor and accepted by the state machine), the consensus-side V2 wiring tests, and
the CU calibration. Run and results are in `docs/palw-fp-fleet-drill.md`.

What it reaches: `inference -> artifact -> signed tx -> extractor -> state machine -> Provisional
claim`, with the CU weights measured on both backends and the shipped 1 : 64 shown to clear the
binding ratio with ~5× headroom.

What it does not: certification. `Provisional -> PanelBound -> ReceiptLicensed -> Final` needs the
panel's overlay rounds on more than one node, and only a `Final` claim can be spent by a receipt
block. That is the next drill, and `WORST_CASE_COURT` — still declared rather than measured — is
the constant it replaces first.
