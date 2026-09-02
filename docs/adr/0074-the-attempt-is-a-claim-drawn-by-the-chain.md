# ADR-0074: The attempt is a claim, drawn by the chain

**Status:** PROPOSED (2026-09-02), decided by the user on two points that this ADR only spells
out: (1) the attempt lane adopts the beacon draw (ADR-0073 Decision 3c's open option), and
(2) **the beacon is a fact derived from the chain a node already holds — never an attestation, a
quorum, a validator set, or a finality overlay.** The last beacon this project built was wired to
validators and the design became BFT-dependent; this one is forbidden from reading anything a
validator says. Ships in the Relaunch 5 re-genesis (5d) together with ADR-0072 + D8 and
ADR-0073 Phase ①, deployed as a rolling host swap (the fingerprint moves, so un-wiped peers are
refused at handshake and cannot re-feed the old chain).
**Builds on:** ADR-0044 (the receipt lane: commit, beacon, quantum, spend), ADR-0072 (+ D8: the
self-drawn ticket and its pins), ADR-0073 (court → unit → weight → share).
**Amends:** ADR-0073 Decision 3 (this is the "one unit" and the "lottery discipline" made
concrete), ADR-0044 Decision 7 (CU weights withdrawn), ADR-0071/0072's premise that the attempt
lane is the primary lane.

## 1. Why a beacon, and why not a validator

The ADR-0072 review found that a ticket drawn from the executor's own hash is only as strong as
every field inside it being pinned; one unpinned u64 made one inference worth 2^64 draws. D8
closed the field, and the invariant ("every priced field is an equality against chain state, a
function of the execution the panel replays, or the position") is now a test. But the receipt
lane has no such surface at all: a claim is fixed on chain first, the beacon that draws it does
not exist yet, and the ticket is `H(beacon ‖ claim ‖ quantum)`. Nothing the executor controls is
inside the draw. That is the discipline both lanes are held to from here: **a draw is a paid
execution, drawn from a value the executor cannot vary after paying.**

A beacon is only as good as its independence from every party that wants a particular value.
The receipt lane's beacon is the first self-drawn (algo-6) chain block at or after the claim's
slot, derived by walking the chain (`derive_beacon_fact_v3`) and validated by two inequalities
(`validate_beacon_fact_v3`): its producer paid an inference for the block, so grinding a beacon
costs an inference per candidate, and its identity is a fact every node that validates the chain
computes for itself. Nobody signs it, nobody votes on it, nobody is asked. **A node that can
validate the chain can derive the beacon; a node that cannot has no business licensing a block.**
The validator-wired beacon of the past failed exactly here — it made block licensing depend on a
liveness assumption about a set of signers — and this ADR names that as the thing it forbids.

## 2. Decisions

**Decision 1 — Every paid unit of LLM work is a claim committed before its draw.**
The claim is the receipt lane's 0x4a commitment (`PalwFreePromptCommitmentV3`), in one of two
prompt modes carried on the job:
* `User` — the prompt is the user's; the ids travel in the payload (PublicDA) and in the job
  material, hash-bound to `prompt_token_ids_hash`. Today's lane, unchanged.
* `Canonical` — the prompt is the family's canonical prompt for the claim's own anchor,
  `fp_canonical_anchor_v1(job) = H(domain ‖ network ‖ class ‖ bond ‖ anchor_block ‖ anchor_daa ‖
  job_nonce)`, derived by the same `job_for_anchor` the attempt lane uses; the ids do NOT travel
  on chain (they are a pure function of the job) but DO travel in the job material, so seats and
  courts take one path for both modes. `prompt_token_ids_hash` must be the derived prompt's hash
  and `prompt_tokens` its length; a seat checks by deriving. Canonical claims are the work queue
  when nobody is asking: an executor keeps earning by running the network's own jobs, and those
  jobs are drawn, priced, verified and tried exactly as a user's.

**Decision 2 — The draw is the chain's beacon.** `fp_quantum_ticket_v3(network, beacon, claim,
quantum) ≤ receipt_target[class]`, with the beacon derived by `derive_beacon_fact_v3` — the
first algo-6 chain block at or after `final_daa + receipt_maturity_daa`, witnessed by the last
algo-6 block before the slot. Stated as law, because it was broken once: **the beacon MUST be a
function of the chain's own blocks and MUST NOT read an attestation, a quorum, a validator set,
a finality overlay, a DNS or MTP or VLT fact, or any value one party can set.** The two
inequalities are the whole validation; a node with the chain has everything.

**Decision 3 — The self-drawn lane stays, as the liveness-and-beacon lane.** Algo-6 blocks are
not retired: they are what makes beacons exist (the floor produces them at milliseconds per
inference, so a class with no self-drawn producer still gets beacons) and what makes beacons
ungrindable (a candidate beacon costs an inference under ADR-0072 + D8). Their share moves to the
claim lane under ADR-0073 Decision 4; their weight is unchanged by this ADR.

**Decision 4 — One inference, one claim.** A claim's work identity is
`H(domain ‖ class ‖ prompt_token_ids_hash ‖ decode_tokens_executed ‖ executor_bond)`; a
commitment whose work identity is already held by a claim that has not retired is refused at the
transition. Canonical claims are unique per (bond, nonce) by construction. What this prices: one
execution committed under N nonces, N retention values or N anchors was N lottery entries at the
cost of hashing. What it does not price: two bonds sharing one set of tiles (each re-hashes the
leaves under its own context) — collusion that costs each party a fee and `reserved` exposure per
claim, recorded here as the residual; it is closed by executor-dependent computation if it is
ever measured to pay.

**Decision 5 — The price is the capture's leaf count.** The commitment carries `work_leaves`,
which must equal the binding's `step_leaf_count`; `quanta = min(⌊work_leaves / QUANTUM_LEAVES⌋,
max_quanta)` and `pwu = quanta × QUANTUM_LEAVES`. `PalwFpCuWeightsV3`, `fp_cu_v3`, `QUANTUM_CU` and
`PWU_PER_QUANTUM` are withdrawn (their text stays in ADR-0044 as the record). The attempt lane's
pwu is already leaves (`pwu_per_inference = canonical_step_leaf_count` by genesis rule), so
`safe_weight` becomes one unit and ADR-0073 Decision 3 is discharged. The seat verifies the price
against the capture it has authenticated (`verify_material` → `capture_shape().step_leaf_count`),
not against a declared shape: the class state holds no profile, only the family can count leaves,
and a claim whose price the seats refuse never licenses — the same standing the roots have had
since ADR-0044. Shape-crafting is gone by construction: leaves are the work.

**Decision 6 — The floor's free-prompt lane bears weight** (ADR-0073 Decision 2 for
PALW-BASE-0): `palw_rc_fp_certified_families_v1` gains the floor from its drill
(`the_floor_free_prompt_lane_certifies_…`). QWEN25-A16 and QWEN36 join when their free-prompt
paths exist and drill (the registered A16 graph refuses `execute_free_prompt`; QWEN36 has none).

**Decision 7 — Rollout.** Job version 3 → 4 (the mode field and the price field are inside the
job and the commitment), the bundle's free-prompt params change shape, the certified free-prompt
set is non-empty: three fingerprint moves, folded into ONE re-genesis with ADR-0072 + D8 and
ADR-0073 Phase ①. Rolling swap per the Relaunch 5 runbook; the new testnet-11 fingerprint is read
from the pin test in an isolated worktree and handed to the fleet operator before any host moves.

## 3. What this costs, stated before it is measured

* **Cadence.** Unchanged in mechanism: claims draw per quantum against `receipt_target[class]`,
  which the per-class retarget already maintains for the receipt lane; canonical claims add
  supply to that lane, so the silent-lane renormalisation in `apply_class_retargets` stops firing
  for classes with an idle executor.
* **Latency to a licensed block** for a claim: bind + receipt + `receipt_maturity_daa` +
  `use_window` — hours at the frozen 120 s cadence, as today for user claims. The attempt lane
  keeps the chain moving in between (Decision 3).
* **Seat cost** is Phase ①'s (hashing plus sampled leaves); pricing adds one equality.
* **State.** One `BTreeMap` of work identities keyed to claims, released at claim retirement.

## 4. Invariants the tests must hold

* The beacon derivation reads only `PalwChainBlockFactV3` items (block, DAA, algo id) — no other
  input type exists on that path, and a test asserts the function's signature by construction.
* A commitment whose work identity is live is refused; the same commitment after the earlier
  claim retires is admitted.
* A canonical claim whose `prompt_token_ids_hash` is not the derived prompt's hash is not
  certified by any seat; one whose hash is derived certifies through the Phase ① path.
* `work_leaves ≠ capture_shape().step_leaf_count` → the seat refuses; equal → `Valid`.
* One floor free-prompt job and one floor canonical claim price in the same unit as one floor
  attempt: `pwu` values are comparable by ratio of leaves, not by lane.

## 5. Supersession

| Decision | Status after this ADR |
|---|---|
| ADR-0044 Decision 7 — CU weights price the lane | withdrawn (Decision 5) |
| ADR-0044 "only attempt-class blocks can be beacons" | kept and named as law (Decision 2) |
| ADR-0072 §3 "the attempt lane is still the canonical-job lane" | the canonical job is a claim too (Decision 1); the attempt lane is the liveness-and-beacon lane (Decision 3) |
| ADR-0073 Decision 3 (a, b, c) | discharged by Decisions 5, 5 and 2 respectively |
| ADR-0073 Decision 2 activation gate | met for PALW-BASE-0 (Decision 6); open for A16 and QWEN36 |

## 7. What landed (2026-09-02, branch `palw-adr0073-fp-weight`)

* **Decision 1:** `PalwFreePromptJobV3::prompt_mode` (`PALW_FP_PROMPT_MODE_USER` / `_CANONICAL`),
  `fp_canonical_anchor_v1`; the chain refuses a canonical payload that carries ids; seats, the
  court arm and the challenger derive and verify the canonical prompt (`fp_prompt_for_job`). The
  work queue is the node panel's: `--palw-canonical-claims`, `--palw-canonical-class`,
  `--palw-canonical-interval-daa`, funded from `--palw-fee-outpoint`; the commitment is assembled
  from the capture's own context (`palw_fp_commitment_from_context_v3`) and the `FPC1` material is
  retained and broadcast under the claim id.
* **Decision 2:** no code change — `derive_beacon_fact_v3` is the beacon; this ADR is its law.
* **Decision 3:** algo 6 unchanged (ADR-0072 + D8 on main).
* **Decision 4:** `fp_work_id_v1`, `PalwClaimStateV2::work_id`, the state's derived work-id index,
  `DuplicateWork`.
* **Decision 5:** `work_leaves` replaces `cu`; `fp_class_quantum_leaves_v1` (⅛ of the class's job,
  `PalwPwuRuleV2::canonical_leaves_v1`); the transition prices from `PalwStateParamsV2::with_fp_quanta`
  and the bundle's `validate()` holds the two readers equal; exposure = the claim's own pwu; the
  seat's pricing check is `capture_shape().step_leaf_count == work_leaves`; worker results carry
  `step_leaf_count`; CU weights, `QUANTUM_CU`, `PWU_PER_QUANTUM` withdrawn.
* **Decision 6:** `palw_rc_fp_certified_families_v1` = {PALW-BASE-0}, pinned from the floor's own
  drill; presets carry `with_fp_certified_classes`; `FreePromptLaneUncertified`.
* **Decision 7:** FP wire version 3 → 4, `PALW_STATE_V2_VERSION` 14 → 15, golden ids and roots
  re-taken; the preset fingerprints are re-pinned on the re-genesis build (rebased on main's
  ADR-0072 + D8) and handed to the fleet operator before any host moves.
* **Outside this repo:** MISAKA Studio's worker request must carry `prompt_mode` (wire version 4),
  and its rail must pass the class's canonical leaves to `build_fp_commitment_tx`.

