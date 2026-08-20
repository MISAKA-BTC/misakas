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
- **Status:** **substrate green** (PR-01, `98e69283`): the V2 transcript binds the commitment
  totally — the finalizer consumes `Expand(root)`, so one flipped commitment bit is a failed PoW,
  and `palw_attempt_v2` tests pin it. The V1 (algo-4) path keeps its mixed-in binding until the
  mode lands; no network demands V2 yet, by design. **ADR-0042:** Decision 3a.

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
- **Status:** red (unit, `consensus/pow`). **Greens in:** PR-04 (admission wiring; a matching fix is
  already in flight on `palw-only-v4` — reconcile, do not duplicate). **ADR-0042:** Decision 6.

### P0-3 — a fresh PALW tip is always "unresolved," so fork choice never engages
- **Invariant:** W3 fork choice actually uses PALW weight.
- **Evidence:** `virtual_processor/processor.rs:2597-2619,2642-2666,2729-2743,2772-2808`;
  `palw_schedule.rs:318-323`; `palw_chain_weight.rs:99-131,189-201`; `processor.rs:6751-6761`. A tip
  requires a panel anchor at `accepted_daa + delta_bind`, which a fresh tip cannot have ⇒ `None` ⇒
  whole chain rejected ⇒ silent blue-work fallback.
- **Red test:** `palw_v2_fresh_tip_is_provisional_weighted` — a candidate tip with a valid commitment
  and no descendant must receive a positive `live` weight (`Provisional`), not `UnresolvedBlock`.
- **Status:** red (integration). **Greens in:** PR-06 (state machine). **ADR-0042:** Decision 2.

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
- **Status:** red (integration). **Greens in:** PR-08. **ADR-0042:** Decision 9. *(The store rename
  `header_selected_tip → header_download_hint` is a mechanical guard in the same PR.)*

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
- **Status:** red (integration). **Greens in:** PR-06/PR-07. **ADR-0042:** Decision 7.

### P0-8 — the arithmetic court is unadjudicable on a normal full node
- **Invariant:** W1 (full node adjudicates every dispute with no LLM).
- **Evidence:** `palw_step_refute.rs:269-329,418-444`; `processor.rs:2652-2677,3036-3049,8565-8575,
  2758`; `docs/adr/0038-...md:385-391,639-643`. `PalwNoWeightsV1` returns `None` for every row ⇒
  every dispute `Unadjudicable`; a real oracle only where the node holds the artifact ⇒ partition.
- **Red test:** `palw_v2_matmul_fraud_convicts_without_model` — give a full node with **no model** a
  proof-carrying refutation (operands + weight row + quant params + Merkle proofs) of a wrong MatMul
  step; assert a conviction, not `Unadjudicable`.
- **Status:** red (unit once evidence is proof-carrying; `consensus/core`). **Greens in:** PR-07.
  **ADR-0042:** Decision 8. *(Runtime-removal half is Decision 4 / P0-W1 below.)*

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
- **Status:** red (integration). **Greens in:** PR-04. **ADR-0042:** Decision 6.

---

## Additional blockers (audit §追加)

### W1 — the full node still runs the model
- **Evidence:** `pow/src/palw_admission.rs:134-136`; `pow/src/lib.rs:256-275`;
  `pow/tests/palw_admission_fixture.rs:82-85`.
- **Guard:** `palw_v2_consensus_has_no_runtime_dependency` — a CI test over the crate dependency
  graph asserting `consensus`/`misakad` have **no** edge to any model-runtime crate. **Greens in:**
  PR-02. **ADR-0042:** Decision 4.

### Per-class DAA / lifecycle unwired
- **Evidence:** `virtual_processor/utxo_validation.rs:1576-1613`;
  `palw_class_daa.rs:300-306,1261-1284`. One registered class; retarget vector fixed empty; epoch
  budget derivation-only.
- **Red test:** `palw_v2_class_freeze_redistributes_share_deterministically`.
- **Status:** red (integration). **Greens in:** PR-09. **ADR-0042:** Decisions 1, 5.

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
