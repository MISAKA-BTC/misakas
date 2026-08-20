# PALW-RC threat model and red-test register

Companion to **ADR-0042**. This is the authoritative list of the attacks the PALW mainnet-candidate
ruleset must defeat, and the tests that prove each is defeated. It exists so that PR-00's completion
condition — *"the attack tests are red on the current implementation"* — is a concrete, reviewable
artifact rather than a claim.

Baseline: audit `9cfcbf99` / `docs/palw-critical-audit-2026-08-19-ja.md`. Every P0 below is quoted
against that audit's file:line evidence, carried forward to the `palw-v2` branch.

## How to read the status column

- **red (unit)** — a self-contained test in `consensus/core` or `consensus/pow` that compiles today
  and *fails* against current behaviour; PR-01+ turns it green. Write these first.
- **red (integration)** — the defect lives in the virtual-processor wiring, so the failing test needs
  the block-pipeline harness. The external audit proved these by control-flow because it had no
  `cargo`; the red test lands with the PR that builds the wiring (it is red the moment the harness
  exists against the pre-fix code, green after the fix in the same PR).
- **guard** — an invariant assertion (startup / construction / CI graph) that has no "attack" shape;
  it fails closed by refusing to build or boot.

The distinction is deliberate: do **not** fake a unit test for an integration-level defect by
mocking away the very sink/candidate divergence that is the bug. A dishonest green is worse than an
honest "red (integration), lands in PR-N."

---

## The register

### P0-1 — commitment not bound to the PoW ticket
- **Invariant:** ADR-0038 W2 (one ticket = one inference, bound to network/header/nonce).
- **Evidence:** `hashing/header.rs:26-29,90-130,372-385`; `pow_layer0.rs:412-452`;
  `pow/src/palw_admission.rs:113-141`; `pow/src/lib.rs:223-225,260-325`;
  `palw_block_commitment.rs:341-360`. Live path uses `calculate_pow_layer0` over
  `(pre_pow_hash, timestamp, nonce, network_id)`; `l1_tag_bytes` (which binds the root) is never
  called on the live path.
- **Attack A:** solve one PALW nonce, then swap `trace_root`/`output_root`/bond to mint unlimited
  sibling identities under the same PoW.
- **Red test:** `palw_v2_commitment_mutation_invalidates_pow` — build a valid header, flip one bit of
  the commitment root, assert the PoW verification now **fails**.
- **Status:** **green (integration).** *History: corrected to RED on 2026-08-20 by the follow-up
  audit (the row had read "substrate green" on a claim the tree did not support); closed the same
  day when the finalizer arm, the wire carrier and the named test landed together — the exact
  trio the RED text demanded.*
  * The **transcript** is total, and more strongly than the original row claimed:
    `commitment_root_v2` is `H(attempt_id_v2(attempt))`, so the priced set and the identity set
    are the same set by construction (fix C4). `every_priced_field_moves_the_pow_tag` derives its
    field list from an exhaustive destructuring, so a field added tomorrow does not compile until
    it is priced.
  * The **finalizer arm** exists: `StateLayer0::calculate_l1_tag`'s `POW_ALGO_ID_PALW_COMMITTED_V2`
    arm computes `l1_tag_v2(commitment_root_v2(attempt))` — and enforces the challenge equation
    itself (carried `challenge` == `challenge_v2(domain, pre_pow_hash, timestamp, nonce, class,
    bond)`), so EVERY path that computes PoW — the pruning-proof path included, which never reaches
    stateful admission — refuses an attempt re-mounted at another position (`PalwV2ChallengeMismatch`).
  * The **wire carrier** exists: on an algo-6 header, `Header::palw_commitment` carries the
    `PAV2`-magic Borsh envelope (7,897 bytes at real ML-DSA-87 lengths, inside the 8,192 cap —
    size pinned by `a_real_envelope_fits_the_header_wire_cap`); `StateLayer0::new` decodes it
    once; a missing/undecodable carrier is the named `PalwV2AttemptMissing`, a failed PoW and
    never a panic; `check_palw_commitment_shape`'s algo-6 arm REQUIRES a decodable envelope,
    independent of V1's `bound` fence (V2's binding is intrinsic).
  * The **named test exists and is deterministic**: content fields move the Layer-0 digest (the
    solution does not transfer — asserted as digest movement, not a target miss, so it cannot
    flake), challenge-equation fields and re-mounted positions are refused outright, carrier
    absence is named, and an exhaustive destructuring forces every future field into a bucket.
    Mutation-checked twice: deleting the challenge equation from the arm and replacing the root
    with a constant each make the test fail.
  * `check_algo_id_known` lists 6 again, in the same commit — so the C1 boot refusal
    (`PalwConsensusParamsV2::validate`) opened exactly as its doc promised
    (`the_runnability_gate_opened_with_the_finalizer_arm`).
  * **Deliberately NOT landed — Decision 3c as written.** The signature stays outside the priced
    identity (flipping it moves neither `attempt_id` nor the digest; the named test pins this),
    but the block-identity digest still hashes the RAW carrier bytes, signature included, rather
    than `attempt_id`. That retention is a decision, not an omission: with identity =
    `attempt_id`, a third party who flips one signature bit produces the SAME block id with an
    invalid witness, and the first-seen invalid copy poisons the honest block's id in every
    known-invalid cache — a zero-cost censorship primitive the ADR text does not address. With
    raw-bytes identity, a flipped-signature copy is a DIFFERENT id that dies alone at admission;
    only the bond holder can mint valid-signature siblings of their own block (ML-DSA-87 signing
    is hedged), each sharing one PoW but deduplicated at the claim (`claim_id = attempt_id`).
    3c needs its own design pass (a mutated-witness path that rejects without caching id
    invalidity) before it can land safely. **ADR-0042:** Decision 3a; 3c deferred with cause.

### P0-2 — block-commitment ML-DSA-87 signature never verified at admission
- **Invariant:** ADR-0038 W8 (no bond, no block — the *holder's* authorization).
- **Evidence:** `palw_block_commitment.rs:241-265,281-306`; `pow/src/palw_admission.rs:113-141`;
  `virtual_processor/utxo_validation.rs:699-720`; `pow/tests/palw_admission_fixture.rs:50-60,88-106`
  (fixture admits a `0x5A`-filled signature). `validate_shape` checks only length;
  `validate_executor_bond_v1` checks only that the named bond is Active.
- **Attack:** attacker with no stake writes any Active bond outpoint + a length-correct garbage
  signature and passes W8.
- **Red test:** `palw_v2_foreign_bond_garbage_signature_rejected` — admission of a commitment whose
  signature does not verify under the bond record's key must return `CommitmentSignatureInvalid`.
- **Status:** **green** — twice over. The V1 lane's fix landed as `82d2db44` (inherited on this
  branch: `verify_mldsa87` under the bond record's key, `CommitmentSignatureInvalid`, the family's
  own context). The V2 lane (PR-04): `palw_v2_foreign_bond_garbage_signature_rejected` pins both
  faces — a foreign key on a victim's bond is `BondKeyMismatch` (stateful item 2) even when the
  signature verifies under the carried key, and a garbage signature is refused statelessly over
  the attempt id in `PALW_ATTEMPT_V2_MLDSA87_CONTEXT`. **ADR-0042:** Decision 6.

### P0-3 — a fresh PALW tip is always "unresolved," so fork choice never engages
- **Invariant:** W3 fork choice actually uses PALW weight.
- **Evidence:** `virtual_processor/processor.rs:2597-2619,2642-2666,2729-2743,2772-2808`;
  `palw_schedule.rs:318-323`; `palw_chain_weight.rs:99-131,189-201`; `processor.rs:6751-6761`. A tip
  requires a panel anchor at `accepted_daa + delta_bind`, which a fresh tip cannot have ⇒ `None` ⇒
  whole chain rejected ⇒ silent blue-work fallback.
- **Red test:** `palw_v2_fresh_tip_is_provisional_weighted` — a candidate tip with a valid commitment
  and no descendant must receive a positive `live` weight (`Provisional`), not `UnresolvedBlock`.
- **Status:** **substrate green** (PR-03/PR-06): the named test passes — a descendant-less claim is
  `Provisional` with `β·pwu` live weight the moment it applies, no panel required; the panel gates
  only `PanelBound → ReceiptLicensed → Final` (`palw_panel_v2`). The V1 weigher's anchor demand
  stands until PR-08 retires that substrate. **ADR-0042:** Decision 2.

### P0-4 — same DAG, different node sink ⇒ different candidate weight
- **Invariant:** W3 (equal DAGs ⇒ equal weights everywhere).
- **Evidence:** `processor.rs:2540-2569,2671-2677,2742-2758,6840-6983,2163-2181`;
  `docs/adr/0038-...md:511-517,640-645`. Weight reads the node's mutable `bond_view`/carriage/
  capability at the previous sink, not the candidate point.
- **Attack (divergence D):** two nodes with different applied sinks pick different tips for the same
  DAG — permanent partition.
- **Red test:** `palw_v2_weight_invariant_under_prior_sink` — apply branch A then weigh candidate C;
  apply branch B then weigh candidate C; assert identical `palw_state_root` + `(safe, live)`.
- **Status:** **substrate green** (PR-03): `palw_v2_weight_invariant_under_prior_sink` passes
  against `PalwChainStateV2` — two books with different prior branches produce identical
  `state_root` and candidate order for the same chain, and the restart / IBD-start / reorg
  differentials pass beside it. The V1 weigher's sink reads remain in the tree until PR-08
  retires that substrate; nothing routes weight through them on any V2 network before then.
  **ADR-0042:** Decision 5, ADR-0043.

### P0-5 — header-selected tip / IBD / pruning use blue work, virtual uses PALW
- **Invariant:** one canonical-chain authority.
- **Evidence:** `header_processor/processor.rs:436-449` (both sides `palw = None`);
  `docs/adr/0039-...md:205-222`; `palw_chain_weight.rs:90-96`.
- **Red test:** `palw_v2_all_selection_sites_agree` — feed one DAG; assert virtual tip == header
  authority == IBD-complete tip == pruning point selection.
- **Status:** **substrate green** (P0-5 comparator + rename `319a4c97`, PR-08 authority): the
  named test `palw_v2_all_selection_sites_agree` passes — virtual selection, IBD commit
  (strict-win-or-keep), the deep-reorg gate, the pruning ceiling (never past the safe frontier)
  and restart recovery all answer through `palw_fork_authority_v2`'s functions over the one
  comparator, identically for opposite application orders, and all prefer the matured chain over
  the heavier immature pile. The store rename landed with P0-5. Pipeline sites consume these
  functions when `PalwConsensusMode::ConsensusV2` exists to demand them (PR-10) — a dead handle
  in today's blue-work pipeline would be surface without semantics. **ADR-0042:** Decision 9.

### P0-6 — conviction / equivocation verified under the wrong signature context
- **Invariant:** W5 (conviction voids the convicted block's weight) — slashing state and work state
  must agree.
- **Evidence:** `palw_facts.rs:433-510`; `processor.rs:2692-2699` (closure hardcodes
  `PALW_RECEIPT_MLDSA87_CONTEXT`); `palw_carriage.rs:846-880,913-976`; `palw_slash.rs:93-95`
  (correct `PALW_S_MLDSA87_ATTESTATION_CONTEXT`); `processor.rs:3019-3028`.
- **Attack C:** a valid equivocation certificate slashes the bond via the attestation path, but the
  weight resolver verifies the same signature under the *receipt* context, fails, leaves
  `convicted_before_close = false`, and the fabricated block keeps its fork-choice weight.
- **Red test:** `palw_v2_equivocation_voids_fork_choice_weight` — inject a valid equivocation cert;
  assert the target block's live weight drops to 0 in the *same* view that slashes the bond.
- **Guard companion:** `palw_v2_no_contextless_signature_closure` — a CI/type guard that no single
  verifier closure is constructed for more than one object family (receipt / commitment / attestation
  / court each get a typed verifier).
- **Status:** red (integration) + guard. **Greens in:** PR-01 (typed contexts) → asserted PR-07.
  **ADR-0042:** Decisions 3, 6.

### P0-7 — panel cannot exclude the executor; no-show unpunished
- **Invariant:** W8 (independent verification; bonded accountability).
- **Evidence:** `processor.rs:2658-2666` passes `executor_bond_outpoint.transaction_id`, but
  candidates carry `validator_pubkey_hash` — different namespaces; `operator_root` always `None`;
  `palw_job_panel.rs:99-115,151-180`; `palw_facts.rs:585-675` (`panel_duty_v1` computes no-show but
  "live slash path does not exist"). **Note:** the panel *module* is already correct on `palw-v2`'s
  base; the defect is the *caller*.
- **Attack:** producer earns a seat on its own work; multi-bond operator aggregates seats; a verifier
  withholds receipts with no penalty.
- **Red test:** `palw_v2_executor_excluded_from_own_panel` — assemble a candidate set including the
  executor's own bond; assert the drawn panel never contains it (by validator key hash AND operator
  id). `palw_v2_panel_noshow_is_slashed` — an assigned seat past deadline loses collateral.
- **Status:** **exclusion green** (PR-06): `palw_v2_executor_excluded_from_own_panel` passes —
  `derive_panel_v2` reads bond, operator and key from the one registry (no second namespace to
  diverge in), dedups operators, and `validate_panel_bound_v2` accepts only the exact derived
  panel at the exact anchor slot. **The no-show COLLATERAL penalty went green 2026-08-20**
  (`palw_v2_panel_noshow_is_slashed`). `slash_dissenting_seats` charged a seat that answered the
  wrong way; `slash_silent_seats` now charges the one that answered nothing, on all three paths
  that end a panel's duty — the panel licenses without it, the panel defaults the producer
  without it, or the receipt window closes with no concluding object at all. Before this a bond
  could take seats forever, file nothing and pay nothing, so the exposure a seat is supposed to
  put behind its verdict was only at risk if it chose to speak. The charge is exactly what a
  refuted answer costs (`claim.reserved`): pricing silence below a lie makes silence the better
  play. A seat with something to say is never a no-show — `Valid` and `Unavailable` are both
  answers, and reporting withheld data is what the `Unavailable` verdict is for. `BindTimeout` is
  deliberately untouched: no panel was assigned, so nobody owed an answer
  (`palw_v2_a_claim_that_never_bound_a_panel_slashes_no_seats`). Verified non-vacuous by making
  the charge a no-op. It also caught a fixture habit — twelve tests licensed a claim with
  `receipts: Vec::new()`, a panel concluding without a word from the seat it had bound.
  **ADR-0042:** Decision 7.

### P0-11 — the lifecycle objects have no way onto a chain (found 2026-08-20)

- **Invariant:** a claim that does honest work reaches `Final`, so PALW weight is a measure of
  work rather than a constant.
- **Evidence:** `palw_fp_objects_v3.rs` — `palw_fp_objects_from_accepted_txs_v3` produces exactly
  one object kind, `FreePromptCommitted`, and it is the ONLY extractor
  (`processor.rs`'s `palw_v2_objects_of_block` calls nothing else). Grep the tree for
  `PalwConsensusObjectV2::PanelBound`: every construction is inside a `mod tests`. The same holds
  for `ReceiptLicensed`, `ProducerDefaulted`, `CourtOpened`, `CourtClosed`, the two rung moves,
  and every `BondRegistered` that is not the genesis list.
- **What it means.** A V2 network boots with the genesis class and genesis bond, admits attempts,
  and then cannot advance a single one: no block can carry a `PanelBound`, so every claim sits
  `Provisional` until `window_bind` lapses and voids as `BindTimeout`. `safe_weight` never grows,
  the safe frontier never leaves the zero point, and PALW weight — the network's entire fork
  choice — is permanently zero. No bond but the genesis one can ever register, so the panel
  registry has one member and the court has no challenger.
- **Why the lattice tests did not catch it.** They are correct and they are the wrong shape to
  see this: each one hands the transition an object list it built in-process, which is the one
  thing a chain cannot do. The gap is between "the state machine accepts this object" and
  "something puts this object in a block", and nothing tested the second half.
- **Red test:** `palw_v2_without_a_lifecycle_carriage_no_claim_can_ever_finalize` — accept an
  attempt, then run blocks carrying only what a real block can carry (nothing), and assert the
  claim voids at `BindTimeout` with `safe_weight` still zero. It asserts today's behaviour
  deliberately; when the carriage lands it fails, and the fix is to rewrite it as the liveness
  test it was always describing.
- **What closing it needs:** a transaction carriage for the lifecycle objects, the same shape
  `FreePromptCommitted` already has — a wire form, an extractor, and an acceptance check per kind
  (several of which now exist: `validate_panel_bound_v2`, `check_court_open_acceptance_v2`,
  `adjudicate_court_close_v2`, the two rung checks). This also subsumes the C5 tail: "nobody has
  an incentive to bind someone else's claim" is a question about who WOULD; today nobody CAN.
- Release-blocker for any network that carries weight. **ADR-0042:** Decisions 7 and 8.

### P0-8 — the arithmetic court is unadjudicable on a normal full node
- **Invariant:** W1 (full node adjudicates every dispute with no LLM).
- **Evidence:** `palw_step_refute.rs:269-329,418-444`; `processor.rs:2652-2677,3036-3049,8565-8575,
  2758`; `docs/adr/0038-...md:385-391,639-643`. `PalwNoWeightsV1` returns `None` for every row ⇒
  every dispute `Unadjudicable`; a real oracle only where the node holds the artifact ⇒ partition.
- **Red test:** `palw_v2_matmul_fraud_convicts_without_model` — give a full node with **no model** a
  proof-carrying refutation (operands + weight row + quant params + Merkle proofs) of a wrong MatMul
  step; assert a conviction, not `Unadjudicable`.
- **Status:** **substrate green, with a soundness defect since fixed and a test still owed.**
  *Corrected 2026-08-20.* The mechanism is as described — evidence is proof-carrying
  (`PalwProvenOperandsV1` against the class's registered artifact root), `adjudicate_court_close_v2`
  is the V2 consumer, and anything unadjudicable REFUSES the close. What the row missed is that the
  refutation was bound to the *refutation itself* and to the claim's PUBLIC `trace_root`, and
  nothing else: `check_step_refutation_v1` reads a whole family of faults out of the binding alone
  ("convict from the binding alone" — a `shape_profile` that fails `validate_shape`, a
  non-canonical `step_leaf_count` or `checkpoint_count`), and on the V2 lineage the binding is
  written entirely by the accuser. Any registered bond could copy the public trace root, attach a
  deliberately invalid profile with `operand_openings: vec![]`, and take `Ok(ExecutorGuilty)` —
  voiding an honest claim as `CourtFraud` and slashing its bond, at the cost of one message
  (reproduced end to end). Closed (fix C3) by carrying the executor's own `execution_root` in the
  attempt and the claim, and requiring `binding.committed_execution_root` to equal it before any
  fault is read: `verify_binding` recomputes that root from the job context, both profile hashes
  and every count and root, so pinning it pins all of them. **The owed test landed 2026-08-20, in
  two halves, and writing it found two defects that made the conviction impossible.**
  `palw_v2_matmul_fraud_convicts_without_model` (arithmetic layer) builds a real BASE-0 execution,
  corrupts one committed MatMul value, and convicts with `ComputationMismatch { value_index: 3 }`
  from weights that arrive ONLY as artifact openings — the same refutation against `NoWeights` is
  `Unadjudicable`, which is what makes the conviction mean something.
  `palw_v2_matmul_fraud_convicts_a_claim_and_slashes_its_bond_without_a_model` (court + state)
  carries it through `adjudicate_court_close_v2` and the transition: the claim is `Voided
  { CourtFraud }` and the executor's bond is debited. Verified non-vacuous by registering the
  class at a different artifact root — the opening stops proving and the conviction disappears.
  **The two defects:** the step leg refused any 32-bit value whose f32 reinterpretation is
  non-finite, and so did the adjudicator's preimage check. BASE-0 commits int32 codes, and every
  integer in `[-8_388_608, -1]` has the all-ones exponent — so the RC's own liveness floor could
  not commit a step leg at all, and any BASE-0 step that reached the court would have been
  convicted of `StepNonFinite` for containing a negative number. Closed by
  `PalwShapeProfileV3::lane` (`Float32` / `Int32`): the finiteness rule is a float-lane rule and
  now says so. It is inside `shape_profile_id`, so a class cannot reinterpret its own lanes
  without changing identity. Ladder no-show defaults are deliberately NOT acceptable V2 objects
  until the ladder itself is chain-carried — a forged default would void honest claims on
  demand; the system stays closed meanwhile (arithmetic conviction when data is held, the
  panel's `Unavailable` quorum when it is withheld, the `window_court` backstop when a challenge
  is abandoned). **ADR-0042:** Decision 8. *(Runtime-removal half is Decision 4 / P0-W1 below —
  closed in PR-02.)*

### P0-8b — nothing recomputes `execution_root` from a real execution (found 2026-08-20)

- **Invariant:** the root the court binds a refutation to is a fact about an execution, not a
  value its producer chose.
- **Evidence:** `execution_commitment_root_v2` (`palw_step_leg.rs:481`) composes four roots
  including a **step leg**; `PalwStepLegBuilderV1` wants one leaf per
  `(call_index, node_slot, position, tile_index)`. The worker's v2-legs path builds
  `execution_commitment_root_v1` — no step leg — and the shim exposes activation taps and logits
  rows, not per-kernel tile outputs. Grep: `PalwStepBindingV2` has no producer outside tests.
- **What it means on each lane.** The attempt envelope's `execution_root` is bound into
  `attempt_id` and therefore into the PoW, so it is immutable after solving and unforgeable
  against a DIFFERENT block — but it is whatever the miner wrote. On the free-prompt lane the
  same gap is fail-closed and visible: `apply_palw_transition_v3` refuses a null root
  (`UnadjudicableCommitment`), so no free-prompt claim can be admitted at all today. The quiet
  lane is the worse one.
- **Status:** **open — and the blocker is one level below the shim.** Two halves are done:
  the consensus derivation (`palw_fp_execution_v3` — the context, the root by the court's own
  function, refusals for runs that could not have happened) and the model geometry a step space is
  built from, now MEASURED rather than declared (`palw-worker --mode geometry`, added 2026-08-20;
  on the pinned Qwen3.5-2B: 24 layers, hidden 2048, 8 heads, 2 kv heads, head dim 256, rope type
  40, and the pins agree with the model).
- **What is actually missing, stated precisely.** Not "tile capture" alone. **No
  `PalwShapeProfileV3` exists for any real model** — every instance in the tree is a test fixture,
  and the worker does not reference the type at all. A profile is the step space's DEFINITION: the
  per-layer node table, each node's `kernel_semantics_id`, its tile length and its data inputs.
  Until one exists there is no step space to capture tiles INTO, so the capture cannot be written
  first. The order of work is: (1) write the Qwen3.5-2B profile from the measured geometry and the
  pinned tree's own graph, (2) instrument the shim to emit per-node tile outputs against it,
  (3) build the step leg and hand `palw_fp_execution_root_v3` its fourth root.
- **Step (1) started 2026-08-20, and the measurement immediately found a defect in the type it
  has to be written into.** `palw-worker --mode geometry` now dumps the pinned GGUF's whole
  metadata block, and the file's tensor table was read directly. What the model actually is:
  architecture `qwen35`, 24 layers with `full_attention_interval = 4` (six attention layers,
  eighteen GatedDeltaNet), hidden 2048, ffn 6144, 8 heads / 2 kv heads / head dim 256,
  `rms_eps = 1e-6`, `rope.dimension_count = 64`, **`rope.freq_base = 1e7`**, ssm conv kernel 4 /
  group count 16 / inner 2048 / state 128 / dt rank 16.
  - **`weight_dtype: u8` could not describe it.** `ffn_down.weight` is `Q6_K` on twelve layers
    and `Q4_K` on the other twelve; `attn_v.weight` is `Q6_K` on four of six attention layers.
    The split is the quantizer's imatrix heuristics, not a rule. One byte per node declares one
    dtype for every layer that node covers, so a profile written into the old type would have
    declared the wrong arithmetic for half its layers — and a court recomputing against it would
    convict honest producers there, because `Q4_K` and `Q6_K` dequantize through different block
    layouts. Fixed: `weight_dtypes: Vec<u8>`, one byte per covered layer, none of them zero.
    `shape_profile_id` moved, as a consensus change to the identity must.
  - **A second trap, caught before it was used.** `shim_rope_freq_base` returned
    `llama_model_rope_freq_scale_train` — the context-extension *scale* (1.0 here), not the
    *base* (1e7). Nothing in Rust called it yet, so it never mis-registered anything; it would
    have the moment the profile was written. There is no `llama_model_rope_freq_base` in the
    pinned header at all, which is why the metadata dump exists.
- **The work order above named the wrong model for the RC floor, and the measurement is what
  showed it.** Writing a Qwen3.5-2B profile means naming a `kernel_semantics_id` per node, and
  `verify_palw_genesis_v2` only accepts a class whose reachable kernels are all in
  `catalogued_kernel_ids_v1()` — the seventeen `KERNEL_CATALOG` really recomputes. Ten of those
  are `PALW-BASE-0`'s integer ops; the other seven are L2Norm, fused RMS-norm, SwiGLU, sigmoid,
  softplus and the two GatedDeltaNet cores. **There is no float quantized matmul in the catalog
  at all** — the only matmul is `base0/matmul-quant/i8xi8-i32-exact` — and no float RoPE and no
  float softmax. The pinned model's every layer is Q4_K/Q5_K/Q6_K matmuls, and its six attention
  layers are IMRoPE + softmax. So a faithful Qwen3.5-2B profile would name kernels this build
  cannot adjudicate, and the coverage gate would refuse the class — correctly.
- **That is by design, and `palw_base0_ops` says so in its own first paragraph:** BASE-0's nine
  ops were "chosen for closability rather than for parity with the float classes' graph", because
  "integerising GatedDeltaNet, interleaved-multimodal RoPE and fused SwiGLU would reproduce the
  catalog problem this class exists to escape." The RC's permanently-Active liveness floor is
  BASE-0, so **the profile the RC genesis needs is BASE-0's, not the pinned float model's** — and
  BASE-0's is authorable today, because every op in it already has an adjudicator.
- **Still open, split by which class it belongs to.**
  - *BASE-0 (blocks the RC genesis):* its shape profile — node table, tile lengths, `input_refs`,
    the `kernel_semantics_id` per node — plus the catalog entry built from it
    (`canonical_step_leaf_count` is counted from the profile, not chosen) and the artifact root.
    Every instance in the tree today is still a fixture.
  - *The pinned float model (blocks the FP lane, not the RC floor):* adjudicators for quantized
    float matmul, IMRoPE and float softmax must exist BEFORE its profile can be written, and the
    node tables, the per-node tile capture in the shim, and the leg after that. The GDN half is
    the larger unknown of the two: eighteen of twenty-four layers are GatedDeltaNet.
- Gates adjudicability on BOTH lanes, so it is a release-blocker for any network that carries
  weight.

### P0-9 — bisection court incomplete (soundness + liveness)
- **Invariant:** disputes terminate; deep fraud is prosecutable in-window.
- **Evidence:** `palw_facts.rs:1866-1922`; `palw_schedule.rs:160-206` (10 rounds / 1024 steps);
  `README-ADR0038.md:35-39`; `docs/adr/0038-...md:570-573,609-623`. Committed measurement
  `d1891333` already shows the 10-round ladder cannot reach the pinned model.
- **Red tests:** `palw_v2_bisection_reaches_terminal_verdict` (interval-1 ⇒ one-step adjudication);
  `palw_v2_bisection_challenger_timeout_defaults`; `palw_v2_bisection_responder_timeout_defaults`;
  `palw_v2_bisection_midpoint_must_be_in_commitment`; `palw_v2_ladder_depth_covers_measured_trace`
  (rounds = `ceil(log2(step_leaf_count)) + terminal`).
- **Status:** **GREEN (unit) 2026-08-20** — all five named tests exist and pass in `palw_bisect`.
  The row read "red (mixed: schedule-depth is unit, terminal/default/midpoint are integration)";
  the integration framing was wrong, because `PalwBisectLadderV1` IS the machine — termination,
  the two defaults and the midpoint rule are properties of it, and a pipeline harness would only
  have wrapped them. Termination is asserted over EVERY divergence in a 16-wide space and the
  located index is checked to be the one the verdicts steered to (a fixture that terminated
  anywhere would have proved nothing); the two defaults assert the SILENT party by name and the
  absorbing transition (a no-show that left the ladder movable was the 2026-08-17 finding); the
  midpoint rule sweeps seven wrong indices plus both endpoint echoes and asserts the refused move
  mutates nothing; the depth test walks real ladders up to `PALW_BISECT_MAX_SPACE` and compares
  the rungs taken against `ceil(log2(N))` measured from the walk rather than a restated formula,
  then pins `PalwCourtParamsV2::bisection_rounds` to the same number so a ruleset cannot declare a
  shallower ladder than the one that runs. Verified non-vacuous by injection: never reaching
  `Terminal` fails the termination test with `RoundBudgetExceeded`, a no-show that does not
  abandon fails the responder default, and dropping the midpoint comparison fails the midpoint
  test. What remains for PR-07 is the terminal OPENING's consumer (the court reading a located
  index into a verdict), which is `adjudicate_court_close_v2`'s side, not the ladder's.
  **ADR-0042:** Decision 8.

### P0-10 — one bond backs unbounded immature work
- **Invariant:** collateral bounds work at risk.
- **Evidence:** `palw_block_commitment.rs:48-62`; `palw_chain_weight.rs:90-131`;
  `docs/adr/0039-...md:350-376`; `palw_class_daa.rs:300-306` (epoch budget "no enforcement point").
- **Attack B:** grind fake roots, stack many `Provisional` blocks on one bond before the first slash,
  lever collateral many times.
- **Red test:** `palw_v2_bond_exposure_ceiling_enforced` — reserve `Σ immature_pwu × slash_per_pwu`
  per bond; assert the N+1th commitment that would exceed `collateral × max_exposure_ratio` is
  rejected at admission.
- **Status:** **substrate green** (PR-04): the named test passes — reservation at claim creation
  (PR-03's accounting), the inclusive ceiling refusal at admission item 8, and release-on-resolve
  re-opening exactly the held headroom. The V1 weigher carries no ceiling until PR-08 retires it;
  no V2 network exists before the bundle. **ADR-0042:** Decision 6.

---

## Additional blockers (audit §追加)

### W1 — the full node still runs the model
- **Evidence:** `pow/src/palw_admission.rs:134-136`; `pow/src/lib.rs:256-275`;
  `pow/tests/palw_admission_fixture.rs:82-85`.
- **Guard:** `palw_v2_consensus_has_no_runtime_dependency` — a CI test over the crate dependency
  graph asserting `consensus`/`misakad` have **no** edge to any model-runtime crate.
- **Status:** **green** (PR-02, `955422d7`): the driver code moved to `misaka-palw-pow-driver`,
  kaspa-pow keeps only a set-once runtime slot, and `no_model_runtime_edge.rs` fails on any
  declared edge — normal, build, dev, optional included — from the consensus crates to a
  runtime-reaching crate (mutation-checked red on both paths). `PalwUnavailable` is a failed PoW,
  not a panic. **ADR-0042:** Decision 4.

### Per-class DAA / lifecycle unwired
- **Evidence:** `virtual_processor/utxo_validation.rs:1576-1613`;
  `palw_class_daa.rs:300-306,1261-1284`. One registered class; retarget vector fixed empty; epoch
  budget derivation-only.
- **Red test:** `palw_v2_class_freeze_redistributes_share_deterministically`.
- **Status:** **substrate green, after a ratchet fix.** *Corrected 2026-08-20:* the expectation was
  `share × Σ(realized production)` against the FULL share table, so any permille held by a class
  that did not produce — frozen, unstaffed, or idle — made every class that DID produce a permanent
  over-producer. The same verdict every boundary, in the same direction, `max_factor` bounding each
  step and nothing bounding the walk: measured at 4^12 over twelve boundaries, ending at zero,
  where `ZeroPreviousTarget` rejects every subsequent block deterministically. Fixed (H1) by
  normalizing over the classes that actually competed in the closed span and skipping classes that
  produced nothing, so expectations sum back to the realized total; genuine competition is
  unchanged and still retargets both ways. A target floor of 1 stands behind it. Otherwise:
  the retarget runs inside `apply_palw_transition_v2` at
  every global epoch boundary — V1's `retarget_over_span_v1` reused whole (share of REALIZED
  production, one-class no-op preserved), frozen classes skipped deterministically (the target
  freezes with the class), idle classes measured as zero, empty epochs measuring nothing. The
  shared differential scenario crosses a boundary, so restart / IBD-cut / prior-sink / reorg
  invariance all exercise it. Redistribution ON freeze (the named red test's live-share shift)
  is a params-table question the atomic bundle answers at PR-10's startup gate: shares are
  params, exactly-1000 enforced, and a frozen class's share is a bundle change — never a silent
  runtime shift. **ADR-0042:** Decisions 1, 5.

---

## Determinism suite (release condition 12) — the invariance axes

One DAG, and for every axis below the derived `palw_state_root`, `safe`/`live` weight, `safe_frontier`,
selected tip, bond state and class state must be identical:

```
prior sink (branch A applied vs branch B applied)     · block arrival order (randomized)
evidence arrival order (randomized)                   · restart position
archival vs pruned                                    · genesis IBD vs pruning-point IBD
x86-64 vs arm64                                        · DB insertion order
```

These land as the differential harness in PR-03 and grow through PR-08. ADR-0039's
`once_decided_a_block_stays_decided` and `carriage_accepted_after_the_window_changes_nothing` are the
seed; the missing axes it names as "not provable at this level" (pruning point, IBD start height) are
exactly what PR-03's candidate-scoped state makes testable.

## Fail-closed suite (no consensus panic on untrusted input)

`runtime absent` · `model artifact absent` · `malformed proof` · `oversized evidence` ·
`unknown opcode` · `unsupported class` · `corrupted DB row` · `peer disconnects mid-stream`.
Each must be a typed rejection, never a panic. Lands alongside PR-02 (runtime) and PR-07 (court).

---

## Follow-up audit, 2026-08-20 — findings C1…C5, L1…L2, H1…H3

An independent adversarial audit of the RC substrate at `a460cdd7` (13 parallel slices, every
finding re-adjudicated against HEAD) returned NO-GO on five criticals, two of which the register
above reported as green. The rows for P0-1, P0-8 and the per-class DAA are corrected in place; the
rest are recorded here. Fixes landed on `palw-rc-audit-fixes`.

### Live on a shipped preset

| # | Where | Defect | Status |
|---|---|---|---|
| **L1** | `misaka-palw-pow-driver/src/lib.rs:377` | Every fork/exec errno — EAGAIN, ENOMEM, EMFILE — became `PalwUnavailable`, which `run_worker_with_retry` returns without spending an attempt and which both consumers price as a **failed PoW**. A node under momentary memory or fd pressure rejected an HONEST block and never retried. Live on testnet-11 and devnet (`pow_palw_activation: always()`); the same file's doc and both call sites already claimed the opposite. | **fixed** — `classify_spawn_error` classifies by errno; only `NotFound` / `PermissionDenied` stay permanent, everything else (unrecognized included) is retryable. 4 regression tests. |
| **L2** | `kaspa-pq-validator-core/src/lib.rs:1541,1631` | Both anti-equivocation stores inserted into the in-memory index BEFORE the durable write and never rolled back. After a flush error the store believed a record existed that disk did not have; being cached for the process lifetime, the lie was never re-read, and the next request for the same key took `AllowRebroadcast`, released a signature and recorded nothing. A restart forgot the commitment entirely. The live instance is the `kaspa-pq-validator run` sidecar's `SignedEpochStore`. | **fixed** — the index becomes a function of what is durable in both stores. Regression test fails the write with EISDIR and asserts `Allow`, not `AllowRebroadcast`; mutation-checked. |

### Reachable the moment `ConsensusV2` is switched on

| # | Where | Defect | Status |
|---|---|---|---|
| **C1** | `consensus/pow/src/lib.rs:345` | `a460cdd7` wired only the DEMAND side: four pipeline gates require `pow_algo_id == 6`, while `calculate_l1_tag` has no arm for 6 and falls to `UnknownAlgoId`. A V2 network booted — every Decision 1 invariant held — accepted its parentless genesis, then rejected every block after it, its own miner's included; the pruning-proof path failed identically, so IBD could not recover. `check_algo_id_known` listed 6 under a doc-comment calling it "every algo this binary can verify". | **closed** — first fail-closed (6 delisted from `check_algo_id_known`; `PalwConsensusParamsV2::validate` refuses a ruleset this binary cannot compute), then opened by landing the trio the P0-1 row demanded: the algo-6 finalizer arm (`Expand(commitment_root_v2)` + the challenge equation in-arm), the wire carrier (`PAV2` envelope in `Header::palw_commitment`, decoded by `StateLayer0::new`, demanded by the shape gate) and `palw_v2_commitment_mutation_invalidates_pow` (mutation-checked). 6 is re-listed and the boot gate opened in that same commit (`the_runnability_gate_opened_with_the_finalizer_arm`). What algo-6 blocks still lack on the V2 lane is the STATEFUL side (admission/transition wiring — PR-10), tracked by the rows above, not by this one. |
| **C2** | `consensus/core/src/palw_state_v2.rs:1153` | The frontier advanced only when the GLOBAL unresolved set was empty — and step 4 of every apply inserts the block's own claim, so on a chain producing work it never moved. `pruning_ceiling_v2` froze with it. Worse than inert: a fork carrying no attempts at all had an empty unresolved set at every block, so it advanced its frontier for free and **outranked a chain that had matured real work** (reproduced: 60 empty blocks reach frontier 60 against an honest chain stuck at 1; `decide_deep_reorg_v2` said `Allow`). | **fixed** — the frontier is the deepest block whose PALW work is `Final` with nothing unresolved below it, which is the definition `palw_fork_choice` already stated. A chain that matured nothing has no frontier however long it grows. 2 regression tests, mutation-checked. |
| **C3** | `consensus/core/src/palw_court_v2.rs:201` | See the corrected P0-8 row: an accuser-authored binding could harvest a shape-family conviction against an honest executor. | **fixed** — `check_execution_root_binding` pins the binding to the claim's own `execution_root` before any fault is read. |
| **C4** | `consensus/core/src/palw_attempt_v2.rs:139` | See the corrected P0-1 row: six of fourteen fields priced, three of the rest unconstrained. A PR-01 closure re-opened by PR-06. | **fixed** — `commitment_root_v2 = H(attempt_id_v2)`; the exhaustive test cannot fall behind the struct. |
| **C5** | `consensus/core/src/palw_panel_v2.rs:84` | One `quorum` licensed BOTH opposite transitions — `Valid` → `ReceiptLicensed` and `Unavailable` → `ProducerDefaulted` — with only `1 ≤ quorum ≤ seat_count` enforced. At `seat_count = 4, quorum = 2` both reach quorum simultaneously and the check ORDER decides. `vlt.rs` has carried `quorum_is_strictly_above_two_thirds` since its own audit; the panel had no analogue. | **fixed** — `2·quorum > seat_count` at construction makes the two quorums provably disjoint; `Unavailable` names `{chunk_index, requested_daa}` and is checked against the attempt's committed chunk count, the panel's existence, the signing time and the producer's retention window; every receipt carries a `signed_daa` checked against `[bound_daa, bound_daa + window_receipt]` and against the block carrying it, with every field inside the signed message; `operator_id` is derived from an operator KEY and each identity must carry its own `min_collateral_sompi` (which the bundle held and nobody read); and `slash_bond` exists, debiting collateral and recording the loss, applied to `CourtFraud`, `ProducerWithholding` and to whichever side of a receipt set the quorum refutes — the amount being `claim.reserved`, the stake the claim itself named. **The two former "still open" tails closed** (`a7be964e`, ADR-0042 Amendments §A3): the "free" re-roll was a finding that overstated itself — a claim is one block, so a re-roll is another solved PoW; the abandoned block's pwu leaves both weights permanently and every void forfeits the reward escrow; the class epoch budget is counted at acceptance and never refunded; and a claim cannot be re-bound, with `anchor_delay > 0` making mining-time panel shopping structurally impossible (`abandoning_a_panel_costs_a_block_its_reward_and_its_epoch_budget` measures all three costs). Sybil is now STATED as the bound it is — at most ⌊X / min_collateral⌋ identities — in Decision 7's amended wording. **Corrected at the 2026-08-20 integration:** all three of those costs rest on "a claim is a BLOCK", and an ADR-0044 free-prompt commitment rides a TRANSACTION — measured on the merged tree, a `BindTimeout` there released the reservation in full, moved no counter, debited no bond, and the next commitment was accepted in the very next block, so the re-roll was priced at one transaction fee. That lane now holds the abandoned claim's collateral for `fp_abandon_hold_daa` after the void (ADR-0042 §A4): a delay, never a confiscation, whose effect is that N concurrent redraws need N × the reservation — the same currency the Sybil bound speaks, so the two compose (`an_abandoned_free_prompt_claim_holds_its_collateral`). **Narrower open remains:** binding is permissionless but nobody is PAID to bind someone else's claim, so in practice the producer decides whether its own claim proceeds. |
| **H1** | `consensus/core/src/palw_state_v2.rs` retarget | See the corrected per-class DAA row. | **fixed**, both halves. The second half closed in `a7be964e`: admission item 6b draws `class_ticket_v2` from the attempt's commitment root — a function of the whole attempt, ungrindable without a new nonce (= new PoW) — and compares it INCLUSIVELY against the candidate chain's class target; a weight-bearing class with no target is refused as `ClassTargetMissing` (`the_class_target_is_what_admits_a_block_of_that_class`). The ticket is domain-separated from the L1 tag and asserted to be, so the lottery is a second difficulty, not the PoW tag renamed. |
| **H2** | `consensus/core/src/palw_mode_v2.rs:211` | `palw_ruleset_id_v2` hashes only `PalwConsensusParamsV2`, which has no cadence field, no fork-choice version, no trace-format version and no signature-context version — all named in Decision 11's preimage. Every window in the bundle is DAA-denominated, so the cadence is what gives them wall-clock meaning: two networks could share a ruleset id and run different rules. | **fixed** — the bundle now carries `cadence_target_time_per_block_ms`, `fork_choice_version`, `trace_format_version` and `signature_contexts_root`, each pinned to what the binary implements, so they are simultaneously inside the id and incapable of varying; the contexts are committed as their own BYTES rather than a version number, so editing a context string without re-minting the id is a startup failure; the network's own cadence is required to equal the bundle's; and `worst_case_court_duration_daa` is replaced by `PalwCourtParamsV2 {max_step_leaf_count, turn_deadline_daa, terminal_rounds}` with ADR-0042 Decision 8's formula `(ceil(log2(leaves)) + terminal) × turn_deadline`, overflow being a refusal. **The "asserted, not read" tail closed** (`a7be964e`): `PalwClassCatalogV2` is the catalog artifact's shape and `verify_against_catalog` the gate — root recomputes, BASE-0 and every share-bearing class present, court coverage checked against THIS BUILD's adjudication table (`verify_catalog_coverage_v1`), and `court.max_step_leaf_count ≥ catalog.max_step_leaf_count()`, so understating the ladder to shrink `window_court` contradicts the catalog the same bundle commits to. **What remains is the CALLER:** the RC genesis loader (PR-10) is the place that holds the artifact and must invoke this gate at boot. |
| **H3** | `consensus/core/src/palw_state_v2.rs` `ClassRegistered` | `slash_value_per_pwu == 0` was refused nowhere. `reserved = pwu × slash_value_per_pwu`, so at zero every claim reserves zero and any number of immature claims fits under any ceiling — P0-10's remedy silently evaluates to no cap. | **fixed** — `ZeroSlashValue` at registration. **Still open:** `pwu_rule` is a `MaxPerAttempt` ceiling rather than a derivation, so Decision 6 item 6 bounds rather than checks — which makes PALW weight a collateral measure rather than a work measure; and the class epoch budget is a permanent hard halt with no startup invariant sizing it. |

### A correction the audit made to itself

`consensus_params_id` for **mainnet** differs from `origin/main` (`fc55b73e…` → `9110ee1c…`), but **not
because of the RC**: `git log -L` on the pin attributes the last move to `569752bd` (algo-5 Ollama)
and an earlier one to `899fefda` (`tkn: TokenParams` entering the hashed `DnsParams`). PR-01…PR-10
and both follow-ups moved it zero times. Merging this branch is a mainnet flag day for reasons that
predate the RC, and reverting the PALW lines alone would not restore neutrality.

### ADR-0042 clauses that should be amended rather than implemented

* **Decision 1, "challenge window > worst-case court duration."** Not implemented, deliberately.
  The ADR's clause assumes a design where a court must finish inside the window that decides
  maturity; the implementation chose a stronger one — an open court SUSPENDS
  `ReceiptLicensed → Final` entirely, so the court is bounded by `window_court` and never races the
  challenge window. Adding the inequality would force every honest claim to wait a full worst-case
  prosecution before maturing, for no safety gained. The ADR should say which mechanism carries the
  guarantee.
* **Decision 7, "splitting collateral across bonds does not manufacture extra panel seats."** Was
  false as implemented; now true in a bounded form that the ADR should state instead of the
  absolute one. `operator_id` is derived from a key, so two bonds share an operator exactly when
  they name the same key; and each identity must carry `min_collateral_sompi` of its own. So
  splitting X collateral yields at most X / min_collateral identities — dedup makes seats scarce,
  the floor makes identities cost. Sybil is bounded, not prevented, and no rule can prevent it.
* **Decision 1's catalog-coverage and D5e clauses** are not in the startup gate; the module doc
  defers them to the RC genesis loader, which holds the catalog preimage. The ADR still lists them
  as boot invariants.
