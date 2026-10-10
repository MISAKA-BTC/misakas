# LG14-B — the legacy V2 Panel route under G14: public localization and the held/fused DA gaps

Lane LG14-B, branch `g14/legacy-held-da` (from integration `b594620dd`). User decision GAP-80 (2026-10-10): the legacy V2 Panel
route, ARMED on testnet-12, must also meet G14. This lane owns RFC-0014 §4–§5 on that route: hierarchical commitment and
independent localization, and the three canonical 8k held/fused DA gaps [C12]. Lane LG14-A (`g14/legacy-filer`) owns the common
filer, the non-seat reservation and Final (§6–§7).

Status words are the user's (2026-10-09): **implemented** (the code exists), **verified** (it compiled and its tests passed on a
recorded run), **armable** (also economics, external review and activation conditions). Nothing here is armable.

Allocations (Lead): fence `palw_legacy_held_da_v2`; object tags 157–159; deltas 205–209; carriage tail 0xE3; RPC ops 207–209.
This design needs tags 157–159 and the fence. It adds no state table, so deltas 205–209 and tail 0xE3 stay unused (§9.4).

## 0. The bar, and what is actually missing

The producer and every Panel seat collude. One public bonded verifier outside the Panel, registered after genesis, holding its
own copy of the registered model (ADR-0177: G14 is conditional on that), and using only public authenticated material, must reach:

- an objective conviction for a computation, job, input, output or state lie;
- the correct DA default when the producer withholds claim material;
- a dismissal of an honest claim, or of its own wrong challenge.

The adjudicators for the TERMINALS already exist on the V2 route and are armed on testnet-12:

| Lie | Existing terminal |
|---|---|
| a non-fused step | one-move court (`ShardCourtAccused`), `ExecutorRefuted` `StepArithmetic` (5), after Final too |
| a fused-attention step | the held dissection (`ShardCourtAccused` → `NeedsDissection` → ADR-0103 D5, ADR-0082 dissection) |
| a checkpoint's state | `CheckpointAccused` (a chunk against the rows it claims to hold) |
| a logits row on an honest step tree | `ExecutorRefuted` `LogitsNotStepOutput` (12) |
| a token not selected by its row | `ForgedOutput` (8) / `ForgedOutputTiled` (11) |
| the output root | `OutputMismatch` (10) |
| the job, class, seed, context, prompt root, trace root, activation leg, checkpoint profile (J1–J7) | `IdentityMismatch` (9) |
| an attempt prompt past J5b's inline bound | `PromptNotAnchored` (13) |
| junk (no preimage) | the R-core+ DA court's default (DA-7) |

What is missing is the **acquisition**: every builder of those terminals reads the producer's SERVED capture (seats only), and on
a held class it cannot even do that ([C12]):

- **(a) the whole-capture cap.** A canonical 8k attempt is about 105.5M step leaves, past `PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1`
  (2^26). The seat's replay refuses to lay it out, so the case settles `Never`.
- **(b) the prefix rung.** The bisection's rung is a flat hash over every leaf below the index (`base0_bisect_prefix_state_v1`).
  For a fold, base0 answers it only by an honest re-execution, which a lying fold's roots refuse. So the first rung is
  `Unreadable`.
- **(c) the fused tile.** The held dissection opens with the COMMITTED tile and its opening. A fold does not retain it, and no
  DA unit discloses it: DA-3 refuses a fused `StepLeaf` (`DaUnitNeedsDissection`), and a `StepRange` opens hashes, not tiles.

A non-seat also cannot reach a consistent garbage trace today. Row 0 opens, the session is refuted, and nothing descends.

This lane adds the acquisition and the descent, and it keeps every terminal as it is.

## 1. The authenticated tree (RFC-0014 §4.1)

**No second execution root.** The tree is the claim's existing commitment. The binding (`PalwStepBindingV2`, authenticated by
`verify_binding_v1` against the claim's `execution_root`) carries:

- `step_merkle_root` over `step_leaf_count` leaves. The node rule is `step_merkle_leaf_v1(i, leaf_hash)` at level 0 and
  `step_merkle_node_v1(l, r)` above, with an odd last node promoted. The held fold retains this tree's nodes at `retain_level`.
- `checkpoint_merkle_root` over `checkpoint_count` checkpoint leaves, under the same discipline.
- `full_logits_trace_root`: the logits rows and the generated ids. The Event units already reach these.
- `job_context`: the job id, the prompt root, and the counts.

So claim → segment → retained block → step means walking DOWN the step tree: from the root, by frontiers, through the retained
level, to a leaf. The leaf order is the pinned enumeration: call-major → position → global node slot → tile, with the KV aux
series after all main leaves. That order is topological for the forward pass, because a step reads only earlier positions and
earlier slots of its own position. So **the first divergent leaf is a step whose inputs all agree with the honest run.**

## 2. Comparison and descent (RFC-0014 §4.2)

The verifier runs the claim's job on its own model copy. It is an honest producer of the same job, with its own fold retention,
streamed exactly as a producer streams (closing gap (a), §4).

1. **Binding.** It obtains the claim's binding from the chain: the answer to an Event demand of row 0, which every claim with a
   trace must answer. Silence is the DA default, and junk ends there.
2. **Identity.** It compares the binding with its own. Any J1–J7 fault means the answer was refused by the identity rule, and
   the session defaults. `PromptNotAnchored` (13) is filed directly from the public binding.
3. **Roots.** It compares the step, checkpoint and trace roots with its own, step root first.
4. **Descent.** It demands `StepNode { level: height, index: 0 }` (§3) and receives the node's frontier
   `PALW_LEGACY_HELD_NODE_DEPTH_V2 = 9` levels below it. It compares the frontier with its own nodes, names the first that
   differs, and demands that one next. After at most `⌈height / 9⌉` rounds it holds the leaf-node frontier and the first divergent
   leaf `i`.

   For the canonical 8k row (≈105.5M leaves, height 27) that is three node rounds: 27 → 18 → 9 → 0. Four rounds reach 2^36
   leaves.

   **Never a liar-internal view.** Every frontier is checked by hash arithmetic against the claim's root before it is read (the
   fold checks it before the answer lands). The comparison is against the verifier's OWN nodes. It never re-derives the served
   fold under the producer's execution.
5. **Openings without the producer.** The verifier can build the authenticated opening of ANY leaf `j ≤ i` itself. A sibling
   on `j`'s path is one of two things:
   - a node wholly before `i`: identical to the honest tree, so the verifier's own;
   - a node containing `i`, or the sibling of `i`'s ancestor: inside one of the revealed frontiers.

   A node wholly after `i` is never on the path of a leaf before it. `PalwLegacyNodeViewV2` implements this rule and checks it
   against the full tree.
6. **The terminal** (§5). A non-fused leaf goes to the leaf recompute (tag 159), which needs nothing more from the producer. A
   fused leaf goes to `KernelWitness` (§3), then `ShardCourtAccused` → the held dissection.
7. **The other roots.** If the step roots agree but the checkpoint roots differ, the verifier walks the checkpoint tree by
   `CheckpointNode` to the first divergent checkpoint, takes a `StateChunk` (an existing unit, any bond), and files
   `CheckpointAccused` with the step rows opened from its own tree, which equals the claim's. If the step and checkpoint roots
   agree but the trace differs, Event units give the row and tile, and `LogitsNotStepOutput` / `ForgedOutputTiled` follow.

**A checkpoint lie.** A false checkpoint from which both executions reproduce the same error is not taken as correctness. The
honest verifier never resumes from the producer's checkpoint. It compares the checkpoint leaves with its own, so the first
false boundary is found as its own unit.

## 3. The units (wire; past the fence only)

`PalwLegacyHeldUnitV2`, appended to `PalwDaUnitV1` as `LegacyHeldV2(..)`. It enters a DA session only through tag 157.

| Unit | Answer (tag 158) | Checked by (hash arithmetic only) | Bytes |
|---|---|---|---|
| `StepNode { level ≥ 1, index }` | the frontier `min(level, 9)` levels down + siblings to the step root | `palw_tir_step_node_reaches_v1` over `(step_leaf_count, step_merkle_root)` | ≤ 512 × 64 + 64 × 64 + binding |
| `CheckpointNode { level ≥ 1, index }` | the same over the checkpoint tree | the same over `(checkpoint_count, checkpoint_merkle_root)` | the same |
| `KernelWitness { leaf }` (CKW) | the committed half of the leaf's refutation: output tile + opening, the canonical input rows + range openings (none at a fused leaf), the decode pin / prompt tile the leaf reads | §3.1 | ≤ the close ceiling (80 KiB), else not admissible (§6) |

- **Tag 157 `LegacyHeldDemandedV2`.** The claim, the unit, the accuser and the claim's binding (authenticated against
  `execution_root`), with the accuser's ML-DSA-87 signature over `H(domain ‖ network ‖ claim ‖ unit ‖ accuser)` under the new
  context `misaka-palw/legacy-held-da/v2/demand`. It opens an R-core+ DA session (`open_da_session_rcore_v1`) that names that
  one unit, with no draws. Every gate, budget, clock and record of DA-1…DA-8 applies unchanged: the stage, the retention, the
  standing, DA-8's caps, DA-6's exposure, and a seat session's pause.
- **Tag 158 `LegacyHeldAnsweredV2`.** The claim, the unit, the binding and the answer, signed by the discloser under
  `misaka-palw/legacy-held-da/v2/answer`. The discloser is the producer, or a bond liable on the claim (X7). The answer is checked
  against the claim's roots, then joins `answered`, and every session whose units are now all answered is refuted (DA-4, DA-6),
  exactly as tag 55.
- **Tag 159 `LegacyLeafRecomputedV2`** (§5.1): the leaf recompute.

**Who must answer.** The producer. A covering `Valid` signer is NOT charged S4 for a default of these units (`palw_da_unit_covered_by_v1`
returns `false` for them). A seat's automatic answering does not build them yet, and DA-7's signer liability rests on that
answering. A default of these units is the producer's S1, or S3 plus the vesting row after Final.

### 3.1 `KernelWitness`: retrieval, never adjudication

The answer is verified by membership alone, `palw_legacy_ckw_check_v2`:

- the binding is the claim's;
- the output preimage's coordinate is the leaf's canonical one, and it hashes (`step_tile_leaf_hash_ctx_v1`) to the opened leaf
  hash, which walks to `step_merkle_root`;
- at a non-fused leaf, the input rows are exactly the canonical set (`canonical_input_leaves_v1_anchored`), each hashed and
  range-opened to the step root;
- at a fused leaf, the history is absent (the dissection's);
- the id carriages verify against the binding.

The kernel is NOT run. Rules:
- A correct answer satisfies the DA obligation and passes to the exact court: `ShardCourtAccused`, built by the verifier with ITS
  OWN artifact openings.
- No answer is the objective DA default (DA-7), never a fraud conviction.
- Bytes that do not authenticate are not an answer: the object is refused, and the obligation stands until its deadline.

**The dissection root with bounded parts.** The witness's parts are the leaf's canonical rows: part 0 is the output tile, part
`k ≥ 1` is input row `k − 1`. Their root IS the claim's step root, so no producer-chosen root exists to be wrong.
- The part count is derived from the coordinate.
- Each part is bounded by its row's lanes and a range opening.
- The whole answer must fit the ruleset's close ceiling.

A leaf whose committed half does not fit is outside the bound, and its plan is not DisputeAdmissible (§6).

## 4. The retained material (RFC-0014 §4.3) and gap (a)

- **The producer** answers from its fold retention (`Base0FpMaterialV2`):
  - a frontier at or above `retain_level` from the retained vector;
  - a frontier below it, and every CKW, by replaying the covering block from its checkpoints (`held_step_range_answer_v1`,
    `fp_leaf_refutation_v1`).

  Hash-only retention does not excuse silence: a producer that cannot re-derive what it committed defaults. Disk-backed,
  streamed or chunked retention is allowed.
- **The verifier never lays out the whole capture.** It runs its own job in fold mode (`Base0SparseStepAccumulatorV1`), holding
  the retained vector and one block in flight. Each comparison asks its own retention for the nodes of one covered span, and
  each opening asks it for the leaves of at most two edge blocks.

  Worst-case resident set: `⌈n / 2^L⌉ × 64 B + 2^L × 64 B + one block's replay`. For n ≈ 105.5M and L = 12 that is 1.65 MB of
  nodes plus the replay working set, against the whole capture's 6.75 GB of leaf hashes alone. (a) is closed by construction:
  `whole_capture_memory_need_v1` is never asked.

## 5. Exact semantics (RFC-0014 §5.3) and the terminals

### 5.1 Tag 159 — the leaf recompute (non-fused leaves)

`PalwLegacyLeafRecomputeV2`:
- the claim, the executor bond (the claim's), the accuser (Active, at or above the floor, never the producer), the binding;
- the leaf's opening, `PalwStepOpeningV1`: the leaf HASH and its path, with NO preimage;
- the canonical input rows (preimages + range openings), the prompt tile / decode pin the leaf reads, and the accuser's own
  artifact openings against the class's registered `artifact_root`;
- the accuser's signature.

The court recomputes the output row with the SAME kernel and order as the one-move court (`run_program` inside
`check_execution_step_leaf_hash_v1`, the shared body of `check_execution_step_refutation_opened_capped_v1`), so rounding,
saturation and accumulation order are the kernel's own. It builds the canonical tile leaf at the leaf's coordinate, hashes it,
and compares:
- **different** → `ExecutorGuilty` → the court-verdict conviction funnel (`convict_by_court_verdict_v1`): live → voided
  `CourtFraud`; Final → reversed, S3/U3;
- **equal** → `FalseAccusation`: the accuser is charged the one-move court's charge (`reserved.min(floor)`).

Why this terminal, and not CKW, at a non-fused leaf:
- **ADR-0177 D2.** At an embedding gather (`ModelCopy`), the committed tile of an HONEST producer is a copy of registered weights.
  The court must never compel it, at any count. With the recompute, the model operand is the verifier's own and the producer
  discloses only hashes.
- **Garbage starts there.** A trace computed with the wrong weights first diverges at leaf 0, the position-0 embedding gather.

It is a direct objective proof. Past the fence it lands whatever court or session is open on the claim, and the conviction closes
them (`void_claim`, RFC-0014 §7.4).

### 5.2 The fused leaf

`KernelWitness { leaf }` → the committed tile → the verifier checks, with the court's own kernel (`a16_attn_finalize_v1` over its
own honest history), that the tile is not the attention of its history. Then it files `ShardCourtAccused`, with the CKW's
committed half, its own artifact rows and the prompt tile, and the bound verdict defers to the held dissection.

The dissection is ADR-0082's, unchanged:
- the producer's root claim must finalize to the committed tile;
- the verifier names the first child that is not its own;
- the bottom convicts `CourtHeldVerdict`.

The held route's moves read only the chain (the root claim, the rounds) and the verifier's own replay.

### 5.3 Bounds that make a plan not DisputeAdmissible

`palw_legacy_held_terminal_fits_v2(profile, ctx, leaf, ceiling)` derives, without running anything, the bytes of:
- the CKW answer at a fused leaf;
- the tag-159 accusation at a non-fused leaf: input rows, artifact rows, prompt tile.

A class whose largest terminal exceeds the ceiling is reported as not admissible for this route. The unanswerable held class
(2M, `held_class_is_unanswerable_v1`) stays refused, as today.

## 6. The cases

| Case | Path | Outcome |
|---|---|---|
| junk root, no material | row-0 Event demand | DA default (existing DA-7) |
| consistent garbage trace (row 0 answers) | row 0 → binding → StepNode descent → leaf 0 (gather) → tag 159 | conviction `CourtFraud` |
| lie in one fused tile (canonical 8k shape) | descent → CKW → `ShardCourtAccused` → held dissection | conviction `CourtHeldVerdict` |
| lie in one non-fused mid-layer step | descent → tag 159 with self-built openings | conviction |
| borrowed trace (another job's real trace) | row 0: the only preimage of `execution_root` names another job, so J-5 refuses every answer | DA default. Conviction via `IdentityMismatch` (9) when the lender's binding is public (it self-authenticates against `execution_root`). The lender's own claim is untouched |
| job / input binding (J1–J7), prompt past J5b | as borrowed; `PromptNotAnchored` (13) from the public binding | default / conviction |
| checkpoint / state | CheckpointNode descent → `StateChunk` → `CheckpointAccused` | conviction (existing terminal) |
| output / logits / token | Event units → `LogitsNotStepOutput` / `ForgedOutputTiled` / `OutputMismatch` | conviction (existing terminals) |
| routing | the held dense classes have no routing step. A routing decision is a step leaf of a TIR class, on the TIR route's own units (`TirStepNode` / `TirStepLeaf`) | n/a here |
| withheld StepNode / CheckpointNode / CKW | no answer by the deadline | DA default (S1, or S3 + vesting row), never fraud |
| honest claim, malicious verifier | every frontier reproduces; tag 159 recomputes equal | the session is refuted (exposure held, burned at retirement); a tag-159 filer is charged |
| verifier's own replay faulty | its frontier differs from an honest claim's, tag 159 recomputes equal, and the CKW check says the tile is the attention of the history | nothing is convicted; the verifier pays |

## 7. Court scope (ADR-0177 D2)

- `StepNode` / `CheckpointNode`: `ClaimWitness` (hashes only).
- `KernelWitness`: `ClaimTrace`, plus `ClaimInput` for a prompt tile.
  - It never carries artifact openings.
  - It is refused at a leaf whose output is a copy of the model: the `Embed` gather (`palw_legacy_ckw_leaf_is_model_copy_v2`).
- Tag 159: the model operand is the verifier's own, authenticated against the registered `artifact_root`
  (`VerifierOwnCopy`).
- **Cumulative bound.**
  - Model bytes compelled through these units: 0, at any number of demands.
  - Per claim: DA-8's non-seat budget (≤ 3 open, ≤ 16 sessions over the claim's life), each session naming one unit.
  - Every unit is answered once (`DaUnitAlreadyAnswered`), and an answered unit is public and reusable by every verifier.
- **The relational residual** (DA16 §7: enough activations reveal a relation) is bounded by those 16 units per claim. CKW at a
  non-fused leaf is never needed (tag 159 replaces it).
- DA16's central predicate `palw_court_scope_v1` is not on any branch yet. When it lands, the switch is: register
  `LegacyHeldStepNode`, `LegacyHeldCheckpointNode` (`ClaimWitness`, `Demanded`) and `LegacyKernelWitness` (`ClaimTrace`,
  `Demanded`) in its inventory, and call `palw_court_demand_allowed_v1` at tag 157's gate.

## 8. Fence, A-2, byte identity

`palw_legacy_held_da_v2` follows `palw_provider_court_v1`'s pattern:
- `None` on every preset;
- hashed Some-only into the params id and the schedule id;
- `Some(never())` collapsed;
- a `palw_fences_v1()` entry and a fork-id probe arm;
- `validate_palw_legacy_held_da_v2` refuses any armed height.

The processor resolves it once (`palw_legacy_held_da_v2_at`) and hands the fold `PalwTransitionExtrasV1::legacy_held_da_v2_active`.

**A-2.** Below the fence, an object of tags 157–159 is dropped by name in the acceptance walk, first and charged nothing, as the
live int-12 build skips bytes it cannot decode. The fold refuses it as the second lock. The same applies to any int-12 object
that carries the appended unit (`MaterialDisclosedV2 { unit: LegacyHeldV2(..) }`, `DefaultAccusedTirStep` naming it):
`palw_object_is_legacy_held_da_v2`. Nothing writes the new unit into state below the fence.

**Byte identity.** An unarmed test folds the same objects with the fence unset and set to `never()`, and asserts identical
roots and deltas. `scripts/t12-repin.sh --shipping --drift-only` reports no drift.

## 9. Interfaces with other lanes

1. **LG14-A** (filer, reservation, Final). Until its filer exists, the verifier engine here is a pure planner
   (`PalwLegacyLocalizerV2`), driven by the E2E through the existing filing path, with the fence test-armed. The switch: LG14-A's
   `palw_fraud_filer` calls the planner's `next_v2` and the builders.
   - **Final.** A non-seat DA session does not pause a claim (DA-5). The descent's rounds can outlast `window_challenge_at` (120
     DAA on testnet-12) against a stalling producer. LG14-A's reservation must hold Final for an accepted pursuit.
   - **After Final.** The non-fused terminal (tag 159) convicts after Final. The fused terminal does not: a held dissection opens
     only on a live claim (`WrongPhase`). Its post-Final coverage needs LG14-A's Final hold.
   - **Budgets.** DA-8's non-seat lifetime cap (16 per claim, shared by every non-seat) can be exhausted by a producer's Sybils.
     That is LG14-A's no-pre-emption item.
2. **DA16.** The court-scope predicate switch (§7).
3. **G14C.** The canonical node harness (a post-genesis bond, a fresh node, RPC). The E2E here is fold-level until it lands (§10).
4. **A2U** rows to add to `PALW_A2_KIND_FENCE_TABLE_V1`:
   - `ObjectTags{157,159}` → `palw_legacy_held_da_v2` (LG14-B);
   - the appended `PalwDaUnitV1::LegacyHeldV2` as `CarriedAppended { fence: palw_legacy_held_da_v2 }`, with a guarded owner arm
     for tags 55 (`MaterialDisclosedV2`) and 67 (`DefaultAccusedTirStep`) carrying it, and an `Int12Inner` row;
   - a `StateEncoding` row for `PalwDaUnitV1` inside `da_sessions` / `da_claims` (written only past the fence).

### 9.4 Unused allocations

Deltas 205–209, tail 0xE3 and RPC ops 207–209 are not used. The sessions, records and answered sets are R-core+'s own tables.
The public read is the existing DA-session read plus the block bodies that carry the answers. An RPC op returning a claim's
legacy-held pursuit (sessions, units, answered, the answer bytes' block) belongs with G14C's RPC leg. It is listed as remaining.

## 10. Verification plan and levels

1. **V-unit** (consensus-core):
   - unit checks and every tamper;
   - the node view against a full tree for random `(n, i, j)`;
   - the frontier planner reaching the first divergent leaf on random lies;
   - the CKW membership check;
   - the tag-159 recompute (guilty and honest);
   - the bounds;
   - the fence pattern;
   - the scope refusals.
2. **V-fold** (kaspad, the held 8k fixture through the real transition, `apply_palw_transition_v2_with_extras` with the fence
   test-armed). The verifier's material is ONLY the chain's objects plus its own honest instance. No `ServedView`, no liar
   instance on the verifier's side. The producer's responder is the liar's own instance: that is the producer's obligation.
   Scenarios:
   - garbage → leaf-0 recompute;
   - a fused lie → CKW → held dissection → conviction;
   - a mid-layer non-fused lie;
   - withheld units → default;
   - an honest claim against a malicious verifier;
   - before and after Final;
   - the unarmed twin.
3. **V-node** (mempool → template → chain-block fold): needs G14C's canonical harness and a held fixture class on the testnet-12
   harness. Remaining (§11).

## 11. Remaining after this lane (stated, not hidden)

- V-node chain-path E2E, a post-genesis bond, a fresh / IBD node that prosecutes, and restart / reorg on the node (G14C's harness).
- The Final hold and pre-emption freedom for a non-seat pursuit, and the fused terminal after Final (LG14-A).
- A seat's / honest producer's automatic answering of the new units on the kaspad tick. The responder functions exist and are
  tested; wiring them into the DA duty loop is node policy for LG14-A's common engine.
- RPC ops 207–209.
- DA16's court-scope predicate switch.
- The measured worst-case deadline (MEAS):
  - rounds = `⌈height / 9⌉ + 1`;
  - each round ≤ `W_disclose`;
  - plus the verifier's cold model fetch and full honest run, which for the 9B-8k row is MEAS's 12.6–16 TB full-claim check
    figure (SG-06).

  A single honest verifier's full re-execution is not cheap at real scale. That is §2's probabilistic-coverage question, not
  this lane's acquisition.

## 12. Status (2026-10-10, on integration `2dd839709`)

- **Verified, at V-unit and V-fold.** Consensus-core: 7 unit tests, plus the existing step-refute, DA and court suites, which the
  refactors ride. kaspa-consensus: the gate test for tags 157–159, and A2U's mixed-verdict pin test on both rulesets.
- **Verified, kaspad V-fold E2E** (6 `lg14b_*` tests plus T54g's 12). Three colluding seats license, and a fresh outsider uses only
  chain objects, its own replica and the public job:
  - the fused lie: descent → CKW → held dissection → `CourtHeldVerdict`;
  - the garbage gather: tag 159, before and after Final;
  - the mid-layer matmul: tag 159 from openings the outsider builds itself;
  - withholding a descent node or the CKW → `ProducerWithholding`;
  - an honest claim survives a malicious outsider and reaches Final;
  - below the fence, every new object is refused and the state is untouched.

  Every block's delta re-applies and reverts, and its carriage reloads (V-fold reorg / restart / IBD).
- **Measured on the fixture:** 11,612 leaves, height 14, the verifier retaining level 12; 2 node rounds; 3 blocks replayed.
- **Derived for the canonical 8k row:** 3 rounds; about 1.9 MB resident, against 6.75 GB of leaf hashes.
- **Repin:** no drift.
- **A2U:** registered.
  - `ObjectTags{157,159}` with the allocation entry.
  - `PalwDaUnitV1` classified as `CarriedAppended`, with guarded owner arms for tags 55, 67 and 83, plus a sample.
  - The `Int12Inner` row, and the wire pins for tags 157–159.
  - Pin 113 added: it was missing on integration.
- **ADR-0177 D2 for the legacy units (DA16b §7.5).** Done for item 1: past the fence a held `StepLeaf` answer owes no artifact rows
  and is checked by membership only. Items 2 and 3 remain: dissection root-claim parameter openings, and readiness (the latter is a
  user decision).

## 13. Codex node responder follow-up (2026-10-10)

The production DA duty path now dispatches legacy units through `palw_da_claim_answers_v1` →
`palw_legacy_held_capture_answer_v2` → `palw_da_built_answer_object_v1`. It uses the existing verified-material loader and memory
reservation, produces step/checkpoint frontiers and CKW from dense or folded base0-codec retention, authenticates with the consensus
predicate, enforces the close ceiling, signs in the tag-158 context and checks the lifecycle ride rule. Obsolete tag-158 answers
leave the queue when their unit is no longer owed. Existing tag-55 answers still use their original builder.

DA policy **8 V-unit PASS**, LG14-B **6 V-fold PASS**, IR regression **19 PASS**, existing dispatch pin **1 PASS**, filer **7 V-unit
PASS**. The honest-fold fixture now builds step/checkpoint/CKW through the actual node worker and carrier builder before applying
the transition. The dense fixture also verifies planted-material recovery, model-copy refusal, wrong claim/unit, missing signing
key and close ceiling. Commands, fixture corrections and logs are in the [audit implementation record](../../audit/g14-review-2026-10-10/implementation.md).

This supersedes §11's unwired-producer item only. The outsider filer's descent, public tag-158 history reads and terminal wiring
remain open, as do production-service V-node execution, non-base0 codecs, per-family admission and maximum-profile RAM/time and
deadline evidence. The fence and consensus encoding are unchanged.
