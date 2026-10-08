# RFC-0015 Panel=0 (`OptimisticPublicVerification`) on the kernel route — implementation record

Branch `rfc15/x15-panel-free` (base `29a4028a4`). Owner: agent X15. Reference level only: `misaka-palw-kernel` (a deterministic ledger
fold) plus the dormant fence in `consensus/core`. **Nothing here is wired into the node and nothing is armed.** RFC-0015 §13.3
`ACTIVATION_ALLOWED` is not evidenced; this record states what exists, what a consumer (lane D) must wire, and what is still open. It
does not say the RFC is complete.

Statuses as in `g14-integration-matrix.md`: **PASS** = implemented and exercised by a named test at that level; **GAP** =
repository-implementable and not done; **EXTERNAL_GATE** = needs something outside the repository. Unknown is GAP.

## 1. What was built

| Piece | Where | What it is |
|---|---|---|
| Mode in the class identity | `kernel/src/mode.rs` | `VerificationModeV1 { PanelLicensed = 0, OptimisticPublicVerification = 1 }`; `class_id_for_mode_v1`: `PanelLicensed` keeps the historical class id byte for byte, any other mode is `H(domain; legacy id, mode)`. |
| Wire | `kernel/src/route.rs` | Tags 13 `RegisterClassV2` and 14 `RegisterPipelineClassV2` carry the mode. Tags 1–12, `AuthV1`, `KernelRefusalV1`, `SettlementInstructionV1` are untouched. `PanelLicensed` under 13/14 is refused (tags 1/2 are that mode's one registration path). |
| Lifecycle | `kernel/src/lifecycle.rs` | `ClaimStateV1::Challengeable { since, window_end }` (discriminant 10) and `ClaimEventV1::OpenChallengeWindow`. A tally event in it (or in a dispute resuming to it) is an **error**, not a no-op; `ChallengeBound` / `Checking` / `ProbabilisticPass` are never visited. |
| Policy | `kernel/src/opv.rs` | `OpvPolicyV1 { activation_daa, window, budgets, economics, carrier }`, a genesis constant (`KernelLedgerV1::with_opv_policy`), validated against `LedgerPolicyV1`. Relations, never network values. |
| Ledger rules | `kernel/src/ledger.rs` | OPV registration gates, OPV admission of a claim (reservation, caps, window, job holding), Panel-tally refusal, OPV default burn, live-claim index, `AdmitOptimisticClass`. |
| Views | `kernel/src/opv.rs` | `opv_claim_view` (discovery clock), `final_receipt` / `final_receipts` + `FinalReceiptV1::to_work_final_event` (RFC-0010 beacon sources), `opv_invariants`. |
| State root | `kernel/src/state.rs`, `opv.rs` | Historical root unchanged while no OPV policy is set; with one, `StateRootPartsV2 { v1 root, policy, admitted, classes, claims }`. |
| Fence | `consensus/core/src/palw_panel_free_v1.rs` + `params.rs`, `fork_id_v1.rs`, `lib.rs` | `palw_panel_free_v1: Option<ForkActivation>`: `None` on every preset, Some-only in both fingerprints, `Some(never())` collapsed in the handshake identity, visited by `for_each_fence`, settable by the fork-id probe, refused by `validate_palw_panel_free_v1`. |

## 2. Requirement → code → test → status

Tests: `K` = `misaka-palw-kernel/tests/k2_opv.rs`, `KP` = `k2_opv_pipeline.rs`, `L` = `src/lifecycle.rs` unit tests, `O` =
`src/opv.rs` unit tests, `F` = `consensus/core/tests/rfc0015_panel_free.rs`. Every ledger test runs through the mini consensus consumer
(strict encodings, refused objects leave the root byte-identical, settlement instructions applied to a bond book that must equal the
ledger's own view and balance) and then `opv_invariants()`. Verifiers are built fresh from the replayed chain and public DA bytes with
their own salt (`outsider`).

| RFC-0015 | Requirement | Code | Test | Status |
|---|---|---|---|---|
| §4.1 | Mode is part of class/claim identity; the same program under another mode is another class | `mode.rs`, `register_class{,pipeline}` | `K the_mode_is_part_of_the_class_identity…` (a, c, d, e), `KP a_pipeline_class_binds_its_mode…`, `mode::tests` | PASS (reference) |
| §4.1 | A producer cannot freely pick the lighter mode; the network policy admits the class | `LedgerTxV1::AdmitOptimisticClass`, `opv_class_admitted` | `K the_network_policy_admits_a_class…` | PASS (reference); source of the admission = GAP (lane D) |
| §4.1 | Panel tally never fabricated; no `PanelBound`; empty receipts / quorum 0 / missing tally never count | `apply_panel_tally` refusal, `Challengeable`, `OpenChallengeWindow` | `L a_challengeable_claim_has_no_tally…`, `K empty_receipts_a_zero_quorum…`, `K an_honest_optimistic_claim_finalizes…` | PASS (reference) |
| §4.2 | Registration needs the fence reached, PUBLIC_PROSECUTION_COMPLETE (always), class bounds that fit the carriers | `opv_register_gate`, existing gate, `opv_class_economics` (`carrier_fit_v1`) | `K …the_mode_is_part… (a, b, f)` | PASS (reference) |
| §4.2 | Concurrent claims / aggregate gain / required collateral | `opv_admission`, `opv_claim_capacity`, `opv_live_counts`, `opv_unsettled_gain` | `K live_claim_caps_and_the_aggregate_gain…`, `K the_reservation_is_the_policys…` | PASS (reference) |
| §4.2 | Material manifest bound to roots, retention | evidence/trace-root binding at inclusion (unchanged), `liability_daa` | `K a_lying_optimistic_claim…`, `K withheld_material…` | PASS (reference); public DA fetch RPC = GAP |
| §4.2 | Unique work identity | one claim per job; `canonical_work_id = H(class, job)` | `K a_junk_squatter…`, `K a_copied_optimistic_claim…` | PASS (reference) |
| §4.2 | Fee | the consumer's transaction fee | — | GAP (lane D) |
| §4.3 | Receipts optional, never a Final condition | none read | `K an_honest_optimistic_claim_finalizes…` | PASS (reference) |
| §5 | Discovery: Challengeable time, start cutoff, Final floor, hard deadline, open sessions | `opv_claim_view` | `K an_honest_optimistic_claim_finalizes…`, `K spam_demands_and_joins…` | PASS (reference); RPC = GAP (lane D) |
| §6.1 | `start + B_cold + B_check + B_localize + B_disclose + B_court + B_carrier + B_reorg ≤ hard deadline`, validated, not hoped | `OpvPolicyV1::validate` (first-step-in-window, disclosure-in-deadline, proof-in-grace, and the global sum) | `O the_example_policy_satisfies_every_relation_and_each_relation_is_load_bearing`, `O the_clock_facts_a_fresh_verifier_plans_by` | PASS (reference); budget values = EXTERNAL_GATE |
| §6.1 | No re-start of the hard deadline by another bond / session / resend / duplicate | window fixed at admission; demands only inside the window; join shares the open demand's deadline; a served position cannot be demanded again | `K spam_demands_and_joins_cannot_hold…` (sweep of the adversary's filing time) | PASS (reference) |
| §6.2 | Final = window + horizon expired, no unresolved accepted dispute, retention/DA, unique work | `Challengeable → WindowClosed → Final`, `ProofGrace` | `L an_optimistic_claim_finalizes_by_the_explicit_window_rule…`, `K an_honest_optimistic_claim_finalizes…`, `K a_proof_in_the_windows_last_block…` | PASS (reference) |
| §6.2 | Voided claim never Final; honest claim's wrong challenge is dismissed boundedly | conviction / default paths; `dismissed_proof_fee` | `K a_lying_optimistic_claim…`, `K an_invalid_challenge_is_dismissed…` | PASS (reference) |
| §6.3 | Spam / censorship bounds: aggregate caps, shared progress, price of saturating the court | live caps; `censorship_cost` vs maximum gain at registration | `K a_class_whose_prosecution_could_be_censored…`, `K live_claim_caps…` | PASS (reference); inclusion assumption = EXTERNAL_GATE |
| §7 | Monitoring is a market, not a guarantee | stated limit (`FinalAssuranceV1::statement`) | `O the_assurance_statement_never_claims_correctness` | n/a (operating assumption) |
| §8.1 | Reservation ≥ maximum gain + default penalty; no Panel locks; no signer-lock division; no double use of a collateral; clocks consistent with liability | `OpvEconomicsV1`, `required_reservation`, free-collateral reservation, `liability_daa` validation | `O …reservation_rule…`, `K the_reservation_is_the_policys…`, `K live_claim_caps…` | PASS (reference); values = EXTERNAL_GATE |
| §8.1 | Nominal slash ≠ collected | slash clamped to the bond's collateral, rest released | `K a_collateral_another_subsystem_took…` (a) | PASS (reference) |
| §8.2 | Conviction bounty from the collected reservation; fake-fraud and fake-default loops cost their players | `accuser_reward_permille < 1000`, `default_burn_permille` | `K a_collateral_another_subsystem_took…` (b, c), `K withheld_material…` | PASS (reference) |
| §8.2 | Same proof by several bonds / relays never punishes twice | `Duplicate` | `K a_lying_optimistic_claim…`, `K replay_and_reorg…` | PASS (reference) |
| §8.2 | Proof front-running (a copied proof filed first takes the bounty) | none: no accuser-side seal in the kernel (both modes) | — | **GAP** (design: an accuser `SealProof`, new tag) |
| §9 | Final facts for work-slice / beacon readers; Panel-independent path | `FinalReceiptV1`, `to_work_final_event` | `K an_optimistic_final_is_a_panel_independent_beacon_source…` | PASS (reference); consumers = GAP (lane D) |
| §11.2 | Old classes / claims keep their rules and ids; root unchanged until an OPV policy is set | `class_id_for_mode_v1`, `root()` dispatch | `K panel_licensed_classes_behave_exactly_as_before…`, every existing kernel test and golden | PASS (reference) |
| §11.2 | Unknown version / mode never success | strict decode | `K route_tag_13…` | PASS |
| §11.3 | Display: window, deadline, pending court/DA, assurance; never "proved" | `OpvClaimViewV1`, `FinalAssuranceV1` | `K an_honest_optimistic_claim_finalizes…` | PASS (reference); explorer/SDK = GAP |
| §13.1 | Activation refused until the gate evidence exists | fence refusal | `F rfc0015_every_preset_is_dormant…` | PASS |
| §13.2 | Fresh outsider convicts a lying OPV claim before and after Final | — | `K a_lying_optimistic_claim…` | PASS (reference); real node = **GAP** |
| §13.2 | Withheld → DA/default; never Final; post-Final default forfeits the reservation | — | `K withheld_material…` | PASS (reference); real node = GAP |
| §13.2 | Squatter, spam, carrier delay, all-slot occupation | — | `K a_junk_squatter…`, `K spam_demands…` | PASS (reference); carrier delay on a real node = GAP |
| §13.2 | restart / IBD / reorg / duplicate proof | `replay`, V2 root | `K replay_and_reorg…` | PASS (reference); IBD / pruning carriage = GAP |
| §13.2 | Panel-licensed claims unchanged across the upgrade | dormant root, ids, receipts | `K panel_licensed_classes_behave_exactly_as_before…` | PASS (reference) |
| §13.2 | Largest admitted class / context / layout | — | — | **GAP** / EXTERNAL_GATE (see finding 2) |
| §13.2 | Pipeline classes (tag 14) | `register_pipeline_class(mode…)`, `from_public_bytes_in_mode` | `KP` (2 tests) | PASS (reference) |

## 3. Decisions worth reviewing

1. **Job holding.** An OPV claim holds its job from its first reveal (`job_claims` is set at admission, not at a tally). A junk claim is
   prosecutable by any bond from that block, so squatting costs the squatter its whole reservation (conviction) or `default_penalty`
   (withheld, demanded by the producer it blocks). The seal-then-reveal rule still stops a copyist.
2. **Window.** Fixed at admission as `base_challenge_window + verification_horizon`; demands open only inside it, each lives
   `court_deadline_daa`, a service holds Final for `proof_grace_daa`; so Final ≤ window end + court deadline + grace, whatever is
   filed (`OpvClaimViewV1::hard_deadline_daa`, swept in `K spam_demands…`).
3. **Economics as relations.** `reservation ≥ max(gain + default_penalty, ⌈gain·1000 / assumed_detection⌉)`, `gain = claim_reward +
   work_credit + external_gain_bound`; the bounty is a share (< 100 %) of the collected slash; a pre-Final default burns
   `default_burn_permille` of its penalty; a class is refused if saturating the court budget through window + liability costs no more
   than its maximum gain. `assumed_detection_permille` makes the monitoring assumption an explicit, auditable number.
4. **Root dispatch.** `root()` is the historical root while `opv.policy` is `None`, so every existing golden and every Panel-licensed
   ledger is byte-identical; with a policy it is `StateRootPartsV2`, which wraps the historical root. The live-claim index is derived
   and not committed (`opv_rebuild_live`).
5. **Admission by the network.** `LedgerTxV1::AdmitOptimisticClass` (consumer-derived, like `AttestArtifact`) is required before a class
   registers under the mode; a ledger with no policy refuses it (a set outside the root would let nodes diverge silently).

## 4. Findings

1. `LedgerTxV1` gained a variant (`AdmitOptimisticClass`): consumers with an exhaustive match need an arm.
2. **Real classes may not be OPV-admissible today.** The route's own ceilings are filing 64 MiB, response 128 MiB, claim commitments
   128 MiB. The toy fixture declares a 2^18-row history, hence a worst opening ≈ 32 MB, a position response ≈ 193 MB and a worst decode
   filing (which `carrier_fit_v1` sizes by the response envelope) of the same size: its OPV registration is **refused by design**
   (`not carriable`). The OPV test world declares a 64-row history. A held / long-context class needs chunked carriage or a tighter
   decode-filing envelope before it can register without a Panel; this is the "largest admitted class" row of §13.2 and stays open.
3. `OpvPolicyV1::validate` pins the caps to the route's ceilings: a policy cannot declare a carrier the route cannot decode.
4. `RegisterPipelineClassV2` decodes the pipeline before its class id (hence the admission check) is known; the cost is the legacy
   tag 2's and is bounded by `charge`. Tag 13 checks admission before decoding.
5. The post-Final default flag (`forfeited_after_final`) is tracked for OPV claims only; a Panel-licensed claim's row has none (adding
   it would change every historical claims root). Its `PostFinalDefault` receipt remains the record.
6. A global `max_live_claims_total` bounds retained commitments but is also a denial-of-service lever for a rich attacker with many
   bonds filling it with real work; it is a policy number, to be sized as throughput × (window + liability).

## 5. What lane D must wire (all GAP until done)

* **Fence → policy.** At genesis build the kernel ledger with `with_opv_policy(OpvPolicyV1 { activation_daa: <palw_panel_free_v1 height, None if absent>, … })`. Gate route tags 13 / 14 on the fence at decode / mempool (the ledger also refuses without a policy or before the height).
* **Admission source.** Derive `AdmitOptimisticClass { class }` from authenticated chain state (the class census / ActiveRewardable state of RFC-0014 §3.3), with `single_class_id_v1` (or `class_id_for_mode_v1` of a pipeline binding) as the id.
* **Objects and deltas.** Consensus objects for tags 13 / 14 in the 110–119 range's successors; fold arms calling `apply_object` (unchanged contract); apply / revert for the OPV state (`OpvStateV1`: `admitted`, `classes`, `claims`; all Borsh) in the Some-only root block and the pruning snapshot / IBD carriage; call `opv_rebuild_live()` after a restore; commit `root()` (V2 form).
* **Bonds.** `sync_bond` only for mature, qualified bonds (the ledger has no bond age); keep `kernel reserved ≤ synced collateral` (an OPV reservation is larger than the Panel route's).
* **Readers.** RPC for `opv_claim_view`, `final_receipt(s)` and the pending / assurance display; build RFC-0010 beacon events with `to_work_final_event` (the consumer supplies positions, `validity_independent`, `depends_on_profiles`, and a Panel licence only for Panel-licensed Finals); do not start a Panel check for an OPV claim (`claim_challenge_subject` still describes it).
* **Carrier lane / fee / budgets.** Real caps in `OpvPolicyV1::carrier`; transaction fee and mempool priority for demands, responses and proofs.
* **§13.2 node E2E** from a fresh verifier through RPC → signed demand / proof → mempool → template → fold → conviction / default → slash → Final blocked → restart / IBD / reorg equality. None of it exists.

## 6. EXTERNAL_GATE_PENDING (not stop reasons)

* G14 PASS on a real node for every rewarded profile (RFC-0014 §13); the 8k held / fused cap, prefix and committed-tile gaps.
* Measurement of `cold_material`, `check`, `localize`, `disclose`, `court`, `carrier`, `reorg_slack` budgets on real hardware and the DAA conversion; the worst-case `dispute_hard_deadline` on a real network.
* Choice and review of the economic values: reservation, `work_credit`, `external_gain_bound`, `assumed_detection_permille`, caps, bounty, default burn; the panel-free collateral and monitoring-economics review.
* Independent soundness review of the composition, the inclusion / censorship assumption, adversarial node tests (delayed honest proofs), the migration drill, a shadow period, and a separately coordinated activation schedule (RFC-0015 §13.3).

## 7. How to reproduce

```text
export CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0
cargo test --offline -p misaka-palw-kernel --release                  # 47 lib + all integration tests incl. k2_opv, k2_opv_pipeline
cargo test --offline -p kaspa-consensus-core --test rfc0015_panel_free   # the fence
```
