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
  the commitment root, assert the PoW verification now **fails**. Today it passes (PoW ignores the
  commitment) ⇒ **red**.
- **Status:** **RED (integration).** *Corrected 2026-08-20 by the follow-up audit — this row read
  "substrate green" on a claim the tree did not support.* Two distinct facts were conflated:
  * The **transcript** is now total, and more strongly than the row claimed: `commitment_root_v2`
    is `H(attempt_id_v2(attempt))`, so the priced set and the identity set are the same set by
    construction (fix C4). It had NOT been: PR-06 added `trace_manifest_root` /
    `trace_chunk_count` / `trace_retention_daa` to the attempt without adding them to the six-field
    transcript, so one solved nonce minted unlimited sibling identities for the price of a
    re-signature. `every_priced_field_moves_the_pow_tag` now derives its field list from an
    exhaustive destructuring, so a field added tomorrow does not compile until it is priced.
  * The **finalizer** still has no arm for `POW_ALGO_ID_PALW_COMMITTED_V2`, and nothing carries a
    `PalwAttemptEnvelopeV2` from a header to a verifier. `l1_tag_v2` and `commitment_root_v2` have
    no non-test callers. So "the finalizer consumes `Expand(root)`" describes an intention, not a
    code path, and the named red test `palw_v2_commitment_mutation_invalidates_pow` does not exist.
  Since `a460cdd7` wired the DEMAND side into four pipeline gates, that gap was a total liveness
  failure waiting on a config change (fix C1): `check_algo_id_known` no longer claims to verify
  algo 6, and `PalwConsensusParamsV2::validate` refuses a ruleset whose algorithm this binary
  cannot finalize. A V2 network now fails to boot instead of accepting genesis and rejecting every
  block after it. This row goes green when the finalizer arm, the wire carrier and the named test
  land together. **ADR-0042:** Decision 3a.

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
  panel at the exact anchor slot. The no-show COLLATERAL penalty
  (`palw_v2_panel_noshow_is_slashed`) stays red for PR-07: the chain-scoped fact exists (a
  `ReceiptTimeout`-voided claim beside its panel record names who owed a verdict), and the
  `Unavailable` verdict keeps a seat reporting withheld data from ever being that no-show.
  **ADR-0042:** Decision 7.

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
  and every count and root, so pinning it pins all of them. **Still owed:** an end-to-end
  `palw_v2_matmul_fraud_convicts_without_model` — no test anywhere asserts an `ExecutorGuilty`
  conviction through the full path. Ladder no-show defaults are deliberately NOT acceptable V2 objects
  until the ladder itself is chain-carried — a forged default would void honest claims on
  demand; the system stays closed meanwhile (arithmetic conviction when data is held, the
  panel's `Unavailable` quorum when it is withheld, the `window_court` backstop when a challenge
  is abandoned). **ADR-0042:** Decision 8. *(Runtime-removal half is Decision 4 / P0-W1 below —
  closed in PR-02.)*

### P0-9 — bisection court incomplete (soundness + liveness)
- **Invariant:** disputes terminate; deep fraud is prosecutable in-window.
- **Evidence:** `palw_facts.rs:1866-1922`; `palw_schedule.rs:160-206` (10 rounds / 1024 steps);
  `README-ADR0038.md:35-39`; `docs/adr/0038-...md:570-573,609-623`. Committed measurement
  `d1891333` already shows the 10-round ladder cannot reach the pinned model.
- **Red tests:** `palw_v2_bisection_reaches_terminal_verdict` (interval-1 ⇒ one-step adjudication);
  `palw_v2_bisection_challenger_timeout_defaults`; `palw_v2_bisection_responder_timeout_defaults`;
  `palw_v2_bisection_midpoint_must_be_in_commitment`; `palw_v2_ladder_depth_covers_measured_trace`
  (rounds = `ceil(log2(step_leaf_count)) + terminal`).
- **Status:** red (mixed: schedule-depth is unit, terminal/default/midpoint are integration).
  **Greens in:** PR-07. **ADR-0042:** Decision 8.

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
| **C1** | `consensus/pow/src/lib.rs:345` | `a460cdd7` wired only the DEMAND side: four pipeline gates require `pow_algo_id == 6`, while `calculate_l1_tag` has no arm for 6 and falls to `UnknownAlgoId`. A V2 network booted — every Decision 1 invariant held — accepted its parentless genesis, then rejected every block after it, its own miner's included; the pruning-proof path failed identically, so IBD could not recover. `check_algo_id_known` listed 6 under a doc-comment calling it "every algo this binary can verify". | **fail-closed** — 6 removed from `check_algo_id_known`; `PalwConsensusParamsV2::validate` reads that list and refuses a ruleset whose algorithm this binary cannot compute. Opens automatically when the finalizer arm lands. |
| **C2** | `consensus/core/src/palw_state_v2.rs:1153` | The frontier advanced only when the GLOBAL unresolved set was empty — and step 4 of every apply inserts the block's own claim, so on a chain producing work it never moved. `pruning_ceiling_v2` froze with it. Worse than inert: a fork carrying no attempts at all had an empty unresolved set at every block, so it advanced its frontier for free and **outranked a chain that had matured real work** (reproduced: 60 empty blocks reach frontier 60 against an honest chain stuck at 1; `decide_deep_reorg_v2` said `Allow`). | **fixed** — the frontier is the deepest block whose PALW work is `Final` with nothing unresolved below it, which is the definition `palw_fork_choice` already stated. A chain that matured nothing has no frontier however long it grows. 2 regression tests, mutation-checked. |
| **C3** | `consensus/core/src/palw_court_v2.rs:201` | See the corrected P0-8 row: an accuser-authored binding could harvest a shape-family conviction against an honest executor. | **fixed** — `check_execution_root_binding` pins the binding to the claim's own `execution_root` before any fault is read. |
| **C4** | `consensus/core/src/palw_attempt_v2.rs:139` | See the corrected P0-1 row: six of fourteen fields priced, three of the rest unconstrained. A PR-01 closure re-opened by PR-06. | **fixed** — `commitment_root_v2 = H(attempt_id_v2)`; the exhaustive test cannot fall behind the struct. |
| **C5** | `consensus/core/src/palw_panel_v2.rs:84` | One `quorum` licensed BOTH opposite transitions — `Valid` → `ReceiptLicensed` and `Unavailable` → `ProducerDefaulted` — with only `1 ≤ quorum ≤ seat_count` enforced. At `seat_count = 4, quorum = 2` both reach quorum simultaneously and the check ORDER decides. `vlt.rs` has carried `quorum_is_strictly_above_two_thirds` since its own audit; the panel had no analogue. | **partly fixed** — `2·quorum > seat_count` enforced at construction, so the two quorums are provably disjoint. **Still open:** `Unavailable` carries no request, deadline or proof and costs its signers nothing; `operator_id` is self-declared and unauthenticated, so seat-aggregation dedup is defeatable; an unwanted panel is re-rollable for free via `BindTimeout`; and no slash primitive exists anywhere in the tree. |
| **H1** | `consensus/core/src/palw_state_v2.rs` retarget | See the corrected per-class DAA row. | **fixed**; the second half — the retargeted class target still has **no consumer on the V2 lane** — remains open. |
| **H2** | `consensus/core/src/palw_mode_v2.rs:211` | `palw_ruleset_id_v2` hashes only `PalwConsensusParamsV2`, which has no cadence field, no fork-choice version, no trace-format version and no signature-context version — all named in Decision 11's preimage. Every window in the bundle is DAA-denominated, so the cadence is what gives them wall-clock meaning: two networks could share a ruleset id and run different rules. | **partly fixed** — `validate_palw_v2` refuses a V2 network that is not at the frozen 120 s cadence (ADR-0038 Decision H, enforced at construction as that decision requires). **Still open:** widening the id's preimage; and `worst_case_court_duration_daa` remains operator-attested rather than derived from the catalog's measured trace length, so both window inequalities bound the bundle against the operator's own claim. |
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
* **Decision 7, "splitting collateral across bonds does not manufacture extra panel seats."** False
  as implemented, because `operator_id` is unauthenticated. Retract it or make it true.
* **Decision 1's catalog-coverage and D5e clauses** are not in the startup gate; the module doc
  defers them to the RC genesis loader, which holds the catalog preimage. The ADR still lists them
  as boot invariants.
