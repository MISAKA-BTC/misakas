# G14 completion matrix — the authoritative gap list (lane G14C, milestone 1)

Owner: lane G14C. Branch `g14/completion`, from the integration head `b8ae9412b` (2026-10-10). This document only answers one
question for every reward-bearing plan family: **which G14 conditions are proven by a test, at what level, and what is missing.**
It does not replace `g14-integration-matrix.md`, `g14-node-e2e-record.md` or the lane records. Those keep their history. For the
question of G14 completeness, this matrix supersedes their "Real node" columns.

## 0. The bar

**Scenario.** The producer and EVERY Panel seat collude: every seat signs `Valid` / receipts. One public bonded verifier outside
the Panel carries the case to one of three outcomes:
- an objective conviction, for computation fraud or a job, input, output or state violation covered by an active plan;
- the correct DA default, for withheld claim material;
- a dismissal, for an honest claim or a wrong challenge.

The verifier uses only public, authenticated material, never the producer's secret state. Under ADR-0177 the property is
conditional on the verifier having acquired the registered model. Court demands stay claim-specific.

**Columns.** These are RFC-0015 §1.1 conditions 1–8 and 10. Condition 9 (post-commit challenge / beacon) is lane OPVB's and is not
scored here.

| Col | Condition | The bar applied in this matrix |
|---|---|---|
| C1 | Ordinary public entry | The prosecuting bond registered after genesis, under the published `BondRegistered` qualification, maturity, collateral and fee rules. It is neither a genesis/operator bond nor a seat. |
| C2 | Fresh verifier | The verifier is built only from public reads after the claim is published. **Full:** a node or client started after the claim (synced by IBD or pruned import) builds the verifier from ITS OWN reads and prosecutes. **Partial:** a fresh in-process verifier is built from the producing node's read API. |
| C3 | Authenticated material only | Every byte is authenticated against on-chain roots or registered commitments. No `ServedView`, liar-internal instance, producer heap, seat capture or private endpoint. |
| C4 | Complete bounded localization | The fault is localized to a bounded exact terminal within the declared bytes, work and rounds. |
| C5 | Objective outcome | Conviction for a disclosed lie, DA default for withheld material, dismissal for an honest claim or a wrong challenge. |
| C6 | Permissionless filing | No pre-emption by another bond's session, the court budget, the chunk lane, or a front-runner. |
| C7 | Actual chain path | RPC read → sign → fee → mempool / carrier (or the node-less relay) → the node's own template → chain-block fold → conviction / slash → Final blocked. |
| C8 | Recovery and resources | Restart, IBD / pruned import, reorg, duplicate proof, exit, collateral hold, and the worst-case deadline. |
| C10 | Immutable identity, conditional prosecution, non-interference | The claim binds a fixed model id, root and spec. The verifier uses its own model copy, authenticated against the registered root. No acquisition / availability condition in consensus. Court demands are claim-specific. |

**Counting rules.** The Lead's rules, plus three this audit adds:
1. **Only a real-node chain-path test meets a cell** (**V-node**): a signed `0x4b` carrier, the mempool, the node's own template,
   the chain-block fold, the persisted tip, then a `ConsensusApi` read. The following levels are listed as supporting evidence only,
   never as G14:
   - **V-fold**: the state transition, or the processor fold without mempool or template;
   - **V-ref**: the kernel's in-process reference ledger;
   - **V-unit**: unit, gate or SDK tests.
2. A Panel-licensed test in which an honest seat exists does not cover the case. In every V-node kernel case below, all assigned
   interim seats sign, or the class is OPV and has no seats.
3. A liar-internal `ServedView` is not public material.
4. **(new) A genesis harness card is not an ordinary public bond.** Every G14 node test picks its outsider from testnet-12's eight
   genesis cards. `World::outsiders` returns "n bonded cards that are neither the producer nor one of the claim's seats". No test
   prosecutes with a bond that registered after genesis. So C1 is a GAP for every family.
5. **(new) A process-fresh outsider built from the producing node's own read API is only partial C2.**
   `g14_kernel_route_survives_a_pruned_import` replays a pruned import, but the importer never prosecutes.
6. **(new) A ConsensusApi read is not an RPC read.** Ops 210–212 / 230–231 are tested over the wire models and the gRPC conversion,
   not served by a running node to a client. So C7's "RPC" leg is a GAP for every family.

**Status words** (user, 2026-10-09):
- **implemented** — the code exists;
- **verified** — it compiled and its tests passed on a recorded run of that code;
- **armable** — nothing here is armable.

Cell notation: a level tag, then a test id from §7. Some ids are prefixed with a branch code (§1); the rest are on the integration
line.

## 1. What "verified" rests on (the recorded runs)

| Code | Branch @ head | Last recorded run of G14 tests | Status of the head |
|---|---|---|---|
| INT | `claude/g14-public-prosecution-integration-9bee39` @ `b8ae9412b` | `lead-v1` (10-09 08:02–09:18) over the code of `501c979ab` (every later commit up to `538d4364a` is docs). Node `g14_` / `da16_` / `r4x_`: **78 passed, 4 ignored**. The 4 are the open FAILs F-C4R3-02 ×2, -03 and -05. | The 10-10 merges (`pre`, SMALL, SHARD2, X12N) have **not been re-run**. Every INT "V" below means verified at `501c979ab`. |
| R4 | `g14/r4-fixes` @ `0ef5084d1` (G14R) | `g14r-m5` (10-09 09:22). Node `g14_` + `r4x_`: **75 passed, 1 FAILED** (`g14_opv_the_salted_seal_rows_roll_back_and_reapply_identically_across_a_reorg`). Kernel crate: all pass. The four c4r3 PoCs are un-ignored and pass. | The head is an unbuilt merge of `b8ae9412b`. The worktree has uncommitted test edits. |
| C4 | `adv/c4r4` @ `e2dd4e0c1` (C4R4; R4 + OB + its own fixes) | Record milestone 1: node `g14_` **71 pass**. Then `c4r4-m2-1010` (10-10 05:12): **`kaspa-consensus-core` does not compile** (`E0061`, 3 vs 4 args) and `k2_opv` has 1 FAIL. | Unbuilt or broken. |
| OB | `opv/bootstrap-beacon` @ `6e9dfa065` (OPVB) | `opvb-run4`: node **67 passed, 6 FAILED, 1 ignored**. The failures are 2 `opv_bootstrap` and 4 `r4x_` (typed classes are no longer admitted under derived eligibility). | `6e9dfa065` fixes all six. It is unbuilt; `opvb-run5` was queued at 10-10 05:10. |
| KS | `k2/real-scale` @ `98d7780ae` (K2S) | `k2s-e1-node`: the 5 `g14_k2s_` node tests pass. `k2s-e2`: kernel passes; SDK `k2s_encoder_court` **FAILS** (rc 101). | The head is the predecessor's K2-TIR-v5 WIP. A merge is in progress in the worktree. It does not merge cleanly with R4. |
| X8 | `rfc8/x8r-review` @ `6246e7942` (X8R) | Record `x8r-m7` / `x8r-m8` (10-09): consensus 13 (incl. the 2 composed real-node runs), core 71, fence 7. | Predates `b8ae9412b`. Not re-run. |

**Consequence.** On INT itself, C5 and C6 still carry three open, proven defects:
- **F-C4R3-02** — a self-inflicted default erases a provable fraud;
- **F-C4R3-03** — eight junk chunk groups hold every chunked prosecution off the chain;
- **F-C4R3-05** — OPV lane capture.

Their fixes are verified only on R4 at m5. In addition, F-C4R4-05, F-C4R4-03 and F-C4R4-10 are fixed only on C4, and F-C4R4-11
only on OB / C4. See GAP-00.

## 2. Verdict per family

**No reward-bearing plan family meets G14 today.** C1 (ordinary public entry) and the RPC leg of C7 are GAPs for every family. C2 is
only partial everywhere.

| # | Family | Where it would earn | Best evidence (branch) | Verdict |
|---|---|---|---|---|
| F1 | K2-TIR v1/v2 single-program classes, Panel-licensed (interim seats) and OPV | kernel route (`palw_probabilistic_constraints_v1`, `palw_panel_free_v1`; dormant) | Arithmetic (MatMul) and DA rows: V-node for C3–C6 and C8 (R4 for C5/C6). Other violation rows: V-ref only. | Not met: C1, C2, C7-RPC and node cases b–j missing; INT FAILs. |
| F2 | K2-TIR-v3 pipeline / media classes (stage, edge, VLM `R`, decode) | kernel route OPV | V-ref only. No pipeline claim on a node. No onboarding path (GAP-B4). | **Must not be reward-bearing** until GAP-20 and GAP-21 close. |
| F3 | K2-TIR-v4 segmented real-scale classes: 8k (canonical held), the 262k window, 2M | kernel route OPV only | KS: V-node S01–S05 on the history-free wide128 sketch at 8,192 positions. The 9B-8k program passes the v4 gate (V-unit). | Not met: C8 at node, canonical 8k held at node, lie types beyond MatMul. **2M: refused by the gate** (`BOUNDS_EXCEEDED`), so it must not be reward-bearing. |
| F4 | K2-TIR-v5 encoders and task heads (+ HFX's Head profile, dormant `palw_task_heads_v1`) | kernel route; a post-fence Gen class through the reward gate | KS WIP: V-unit test failing at the commit. No node test. | **Must not be reward-bearing** until GAP-40 closes. |
| F5 | Kernel-route profiles needing private / fused material (fused tiles, FOLD prefixes) | — | V-ref `k2_public::no_reward_opens_without_a_complete_public_prosecution_of_exactly_that_profile` (`PrivateMaterial` ⇒ never registers) | **Must not be reward-bearing**, by construction (fail-closed). |
| F6 | RFC-0004 typed roots: Memory / Retrieval / Composite | kernel route OPV + `palw_typed_roots_v1` | INT: V-node T02–T05 (conviction, default). Replay only for C8. | **Must not be reward-bearing**: no eligibility path (OB GAP-B16: Spec kinds are admitted only through a `cfg(test)` seam). Plus GAP-51 and GAP-52. |
| F7 | RFC-0008 EXEC work slices (legs) | `palw_exec_payload_v2` | X8: V-node E01 (outsider convicts a slice's kernel claim; suffix and root void) and E02 | Not met: slice DA default is fold-only, the REAL root is admitted through a seam, initial boundary is a DESIGN_GAP, plus C1, C2 and C7-RPC. |
| F8 | Onboarding conformance evidence (tag 109 Post / Refute / PostComplete) and the artifact binding (104 / 105) | the reward gate (OB `palw_reward_gate_v1`) | INT: V-node B01–B12. OB: B13–B24 (B15 and B19 failed at run 4; fixed, unbuilt). | Complete-check path: met in the fold (no outsider needed). Sampled path: **cannot gate rewards** (no digest court, GAP-B6, GAP-B7). Under the ruled 128-bit floor only complete checks pass (B22). |
| F9–F16 | Legacy V2 Panel route on testnet-12 (**not Panel=0 / not new-reward eligible** until the user answers GAP-80): FP attempt floor; FP held 8k / 2M (graph-v7 fused); FP free prompt; FP Job V4 decode; FP V5 / Gen (RFC-0003); TIR v1 (RFC-0002); TIR shard (RFC-0006); RFC-0004 Part I improvement evaluation | the old Panel route (live int-12); exempt `LEGACY_PANEL_ROUTE` under OB's gate | V-fold pieces only; seat filers; the held [C12] gaps are pinned. | **Cannot meet G14.** Must not be reward-bearing on any G14-gated channel (OPV, Panel=0, post-fence rewards). Whether they keep earning after the release is a **user decision** (GAP-80). |
| F17 | Post-fence V2 REAL work of a class the reward gate `Passed` | the V2 attempt lane, through OB's gate | none: the gate's evidence is the bound KERNEL class, not the V2 claim | **Hole, not met** (GAP-81). |
| — | BASE-0 base class | bonded fallback | — | Exempt (`BASE_FLOOR`); not useful-computation reward; outside G14 by design. |

## 3. Family matrices

### 3.1 F1 — K2-TIR v1/v2 single-program classes (Panel-licensed with every interim seat colluding, and OPV)

**3.1a Violations reached on the node.** Every listed test runs with every assigned interim seat signing, or as OPV.

| Violation (plan relation) | C2 fresh verifier | C3 auth. material | C4 localization | C5 outcome | C7 chain path |
|---|---|---|---|---|---|
| MatMul arithmetic lie (`FreivaldsM127`, `ExactRecompute`) | partial: V-node N01, O02 (verifier from read-API rows + public DA dir, own salt); full: GAP-02 | V-node N01; N05 (served on chain); X01 / X02 (node-less relay) | V-node N01 (one scalar, 3 openings) | V-node N01, O02 pre-Final; N02, O03 post-Final within liability. **INT FAIL X03 / X04** (F-C4R3-02) → R4 V-node X03 / X04 (m5) | V-node N01, O02, X01, X02 (sign, fee, mempool or relay, template, fold, slash; the lie never finalizes). RPC: GAP-03 |
| DA withholding (a demanded position) | partial: V-node N03, O04 | V-node N03 (signed demand) | V-node N03 (one position, one round) | V-node N03, O04: default, never a conviction. Plus F-C4R3-02 above. | V-node N03, O04 |
| Malformed / truncated / wrong-root / fake-opening responses | V-node N04 | V-node N04 | V-node N04 | V-node N04 (rejected, then default) | V-node N04 |
| Honest claim against a malicious challenger | — | — | — | V-node N10 (spam served; Final in bound), N18 (junk proofs dismissed, fee charged) | V-node N10, N18 |

**3.1b Violations NOT reached on the node.** For each row, C2–C5 are V-ref only (the kernel's in-process ledger; the outsider
replays from genesis with a public DA store). C7 is a GAP. All rows are under **GAP-10**.

| Violation | Reference evidence (V-ref, `misaka-palw-kernel/tests/…`) |
|---|---|
| Quantize / round / carry / range (exact families) | `k2_e2e::a_false_value_in_every_exact_family_is_recomputed_and_convicted` |
| `i128` accumulator (K2-TIR-v2 CRT, alias mod 2^127−1) | `k2_wide::every_scalar_lie_in_the_i128_product_is_caught_and_convicted`, `k2_wide::a_lie_that_aliases_to_zero_mod_2_127_minus_1_is_caught_by_the_second_modulus` |
| Routing / TopK / MoE expert | `k2_adversarial::a_swapped_expert_choice_is_caught_at_the_router` |
| History / state window (derived `Hist`, held) | `k2_ledger::a_history_window_is_never_served_and_a_misderived_window_is_convicted_from_the_rows_alone`, `k2_adversarial::a_permuted_history_window_is_caught_at_its_append` |
| Checkpoint / segment boundary (fabricated) | `k2_e2e::a_fabricated_segment_boundary_a_weaker_suite_or_another_output_is_refused_before_any_check` (refused at inclusion) |
| Job binding (copied claim, squatting) | `k2_ledger::a_bond_that_copies_a_published_claim_is_refused_and_a_failed_holder_frees_the_job`, `k2_ledger::an_uncovered_claim_never_locks_its_job_and_the_first_covered_claim_holds_it` |
| Input / prompt — borrowed trace of another job | `k2_ledger::a_borrowed_trace_is_refused_at_inclusion_and_a_substituted_output_is_convicted_by_the_decode_court` |
| Output / token (decode court) | the same test (last and mid-stream substitution) |
| Self-consistent garbage trace (other weights) | `k2_ledger::a_self_consistent_trace_under_other_weights_is_convicted_and_a_failed_check_alone_never_convicts` |
| A lie at every node of the reference classes | `k2_family_review::a_lie_at_every_node_of_the_reference_classes_is_localized_and_convicted`; C4R1 `adv_c4_route::a_single_lie_at_any_node_of_position_one_is_refused_at_inclusion_or_convicted_and_never_survives_clean` (on `adv/c4-e2e`) |
| Full collusion, reference | `k2_ledger::a_full_panel_collusion_loses_to_one_outside_bond_before_final_and_after_it` |

**3.1c Family-wide columns.**

| Col | Evidence | Status |
|---|---|---|
| C1 | Every outsider is a genesis card (`World::outsiders`). On the producer side, OPV admission capture **INT FAIL X05** (F-C4R3-05) → R4 V-node X05. | **GAP-01** |
| C6 | V-node:<br>- N06: five spam demand sessions never pre-empt a direct proof;<br>- N07: simultaneous challengers give one conviction and one duplicate;<br>- N16: the block budget bounds the block.<br>**INT FAIL X06** (F-C4R3-03, chunk lane) → R4 V-node R02 + X06.<br>F-C4R4-05 (one junk filing reserves a block's whole court): fixed on C4 only, V-ref `c4r4::f_c4r4_05_one_junk_filing_must_not_buy_a_whole_blocks_court`; no node case.<br>Bounty front-running (GAP-R7): R4 V-node X08b. That is an incentive (PRINCIPLES §6.6, ECON), not a conviction blocker. | Met on R4 except F-C4R4-05 (GAP-00) |
| C8 | V-node:<br>- N13: replay and reorg, rows / aux / collateral / queue restored exactly;<br>- N14: restart over the same DB;<br>- N15: pruned import through tail `0xEC`;<br>- N08: duplicate after replay;<br>- N11: no exit while liable;<br>- N09, N10, O05: Final bounded by window end + court deadline + grace;<br>- O07, O08.<br>R4: R01 (escrow Final reward across reorg / replay / redemption, V-node); **R04 FAILED at m5**.<br>A fresh node resuming mid-prosecution: GAP-02. Measured worst-case deadline: EXTERNAL (MEAS). | Met except R04, GAP-02, MEAS |
| C10 | Identity: the class id is the hash of (descriptor, program, plan, commitments, mode). V-ref: `k2_opv::the_mode_is_part_of_the_class_identity_and_an_optimistic_class_registers_only_where_the_policy_the_fence_and_the_gate_allow`.<br>Claim-specific demands: `FileDemand` names (claim, stage, position) only (V-node N03).<br>The model operand comes from the verifier's own copy: V-ref `k2_ledger_route::an_outsider_authenticates_the_public_artifact_against_the_registered_commitments`; implemented at node, never asserted there.<br>Non-interference: no test (GAP-04).<br>DA16's dormant artifact leases put availability back into consensus (GAP-05). No test enforces a claim-specific court scope across routes (GAP-06). | GAP-04, GAP-05, GAP-06 |

### 3.2 F2 — K2-TIR-v3 pipeline / media classes

| Violation | C2–C5 | C7 |
|---|---|---|
| Stage lie / edge lie / false draw of `R` (text-to-image) | V-ref `k2_ledger_pipeline::a_text_to_image_pipeline_with_r_finalizes_honest_and_a_stage_lie_an_edge_lie_or_a_false_draw_of_r_is_convicted`, `k2_pipeline::a_lie_in_every_stage_input_of_every_reference_pipeline_is_an_edge_fault_the_edge_court_convicts` | GAP-20 |
| Trace of another seed / another job (borrowed); `R` not the job's seed | V-ref `k2_ledger_pipeline::a_trace_drawn_from_another_seed_or_of_another_job_is_refused_and_an_r_that_is_not_the_jobs_seed_is_convicted` | GAP-20 |
| VLM substituted id (decode); withheld stages | V-ref `k2_ledger_pipeline::a_vision_language_claim_with_a_substituted_id_is_convicted_by_the_decode_court_and_withheld_stages_are_demanded` | GAP-20 |
| False image edge; withheld upstream rows | V-ref `k2_pipeline::a_false_image_edge_is_convicted_and_withheld_upstream_rows_are_unavailable` | GAP-20 |
| OPV pipeline lie, fresh outsider | V-ref `k2_opv_pipeline::a_lie_in_an_optimistic_pipeline_stage_is_localized_and_convicted_by_a_fresh_outsider` | GAP-20 |

Family-wide:
- **C1, C2, C7:** GAP-01 / 02 / 03, and no node claim at all.
- **C8:** no node case.
- **C10:** as F1.
- **Panel-licensed pipeline mode** has no receipt path on the node (node record §6 item 6), so it must not be reward-bearing.
- **Eligibility:** no onboarding path, so no pipeline class is ever derived-eligible (OB GAP-B4) — GAP-21.
- The header wire form (lane D GAP 6) is implemented on KS only.

### 3.3 F3 — K2-TIR-v4 segmented real-scale classes (KS)

| Violation | C2 | C3 | C4 | C5 | C7 |
|---|---|---|---|---|---|
| Element lie in one segment (wide128, 8,192 positions) | partial: V-node KS S02 | V-node S02 (reads exactly positions 1,199 and 1,200: 2,208 B) | V-node S02 (one element, filing 3,359 B) | V-node S02 (convicted, real bond slashed) | V-node S02 |
| Withheld segment | partial: S03 | V-node S03 | V-node S03 (exactly the two positions) | V-node S03 (default, never a conviction; demand bonds returned) | V-node S03 |
| Material served only on chain (nothing published off chain) | V-node S05 | V-node S05 (`Respond` parts read back from blocks) | V-node S05 | V-node S05 | V-node S05 |
| Honest element filed anyway | — | — | — | V-node S01 (dismissed, no slash; Final with no Panel) | V-node S01 |
| Prompt-tile lie (input binding, > 4,096 ids) | GAP | GAP | GAP | GAP | GAP-30 (only tiles honestly posted, S01) |
| Continuity-by-wiring / checkpoint lie (v4 drops v1's entry/exit roots; a court opens the predecessor position's write) | GAP: no test at any level | | | | GAP-30 |
| Garbage / borrowed trace, decode lie | GAP | | | | GAP-30 |
| **Canonical 8k held** (history-bearing attention, the 9B-8k program) | gate only: V-unit `k2_real_scale::k2s_huihui_qwen35_9b_8k_passes_the_real_scale_gate_and_the_carriers` (8.19 GB per prosecution, 2,294 parts a position, verifier RAM 8.19 GB) | GAP | GAP | GAP | **GAP-31** (the node class is history-free) |
| 262k (the 8k window sliding) | gate only (same test) | | | | GAP-31 |
| 2M | refused: `BOUNDS_EXCEEDED positions 2,097,152 > 262,144` | | | | must not be reward-bearing |

Family-wide:
- **C1, C2-full, C7-RPC:** GAP-01 / 02 / 03.
- **C6:** per-demander caps; direct proofs are not pre-empted (by design, unchanged). No node case for v4 — GAP-30.
- **C8:** only replay agreement (S01). Reorg, restart, pruned import and duplicates of tables 20–21 are GAP-30.
- **C10:** demands are per position (claim-specific); element courts take W leaves from the verifier's copy.
- **Merge:** KS conflicts with R4 (`ledger.rs`, `rows.rs`, `route.rs`, `processor.rs`); `SaltedCommitV1::Segmented` is owed at the merge (GAP-32).
- **Detection at real scale (SG-06):** the `q · P_run` draw, watcher fee and sublinear proofs (DESIGN_GAP) are PRINCIPLES §2/§6
  items, not G14 columns. Until they are set, v4 does not earn (readiness §3 item 4e).

### 3.4 F4 — K2-TIR-v5 encoders and heads (KS WIP)

`misaka-palw-sdk/tests/k2s_encoder_court.rs::k2s_v5_encoders_and_heads_are_judged_one_element_at_a_time_from_public_material`:
V-unit level, **FAILS** at `98d7780ae` (an honest filing of 2,673 B against a priced 2,592 B). The fix is not committed.

Every node column is GAP (GAP-40). This includes the court for `HEAD_DECODE_V1` (arg-max, multi-label, regression, best span).
HFX's Head profile is a Gen (V2 Panel route) profile. Past the fence it can earn only through the reward gate, and so only through a
kernel binding to a v5 class that does not exist yet.

### 3.5 F5 — profiles needing private material

Fused tiles, FOLD prefixes and private weights, input or state are refused at the gate (`PrivateMaterial`). No class, job, claim or
reward ever exists for them (V-ref `k2_public::no_reward_opens_without_a_complete_public_prosecution_of_exactly_that_profile`;
gate `public_prosecution_complete_v1`). This is listed so that the envelope is not silently narrowed: **no fused profile is
reward-bearing on the kernel route.**

### 3.6 F6 — RFC-0004 typed roots (INT, `r4x_typed_roots_e2e.rs`; OPV only)

| Violation | C2 | C3 | C4 | C5 | C7 |
|---|---|---|---|---|---|
| Memory: a lie in one update step | partial: V-node T02 | V-node T02 (step-0 pre-state on chain, rows, DA) | V-node T02 (the fault names the step; the same proof against another step is dismissed) | V-node T02 | V-node T02 |
| Memory: withheld pre-state (step `i ≥ 1`) | partial: T03 | V-node T03 | V-node T03 (exactly that position) | V-node T03 (default, not fraud) | V-node T03 |
| Memory: a lied token (decode court of its step) | V-ref `typed_roots::memory_a_lied_token_is_convicted_by_the_decode_court_of_its_step` | | | | GAP-51 |
| Memory: a lie that finalized; the line rolls back | V-ref `typed_roots::memory_a_lie_that_finalized_is_convicted_after_final_and_the_line_rolls_back` | | | | GAP-51 |
| Retrieval: wrong item, missed better item, withheld slice | partial: V-node T04 | V-node T04 | V-node T04 (one item) | V-node T04 | V-node T04 |
| Retrieval: misordered / short result | V-ref `typed_roots::retrieval_inclusion_refuses_a_misordered_or_short_result` (refused at inclusion) | | | | GAP-51 |
| Retrieval: a malformed snapshot leaf (unopenable) | **GAP** — the snapshot binding does not attest leaf well-formedness (R4X §10). A producer can state anything for such an id. | | | | GAP-52 |
| Composite: a lie in the tool stage | partial: V-node T05 | V-node T05 | V-node T05 (stage 0) | V-node T05 (a filing against the honest stage is dismissed) | V-node T05 |
| Composite: model-stage lie; logits→query edge | V-ref `typed_roots::composite_a_lie_in_the_model_stage_is_convicted_at_the_model_stage`, `typed_roots::composite_the_logits_to_query_edge_court_convicts_a_carried_query_that_is_not_the_upstream_logits` | | | | GAP-51 |

Family-wide:
- **C1, C2-full, C7-RPC:** GAP-01 / 02 / 03.
- **C6:** no node case — GAP-51.
- **C8:** replay only (`assert_replays` in T02–T05). Reorg, restart, pruned import of tables 22–24 and the line head, the Final
  race and duplicates are GAP-51.
- **Eligibility:** no onboarding path — OB GAP-B16, GAP-50. On OB the four node tests pass only through the `cfg(test)` seam
  (`6e9dfa065`, unbuilt).
- **C10:** artifact and snapshot attestation is the test hook `kernel_route_test_attest_artifact_v1`. The line head moving at Final
  must be squared with ADR-0175 (INTF's audit; not a G14C item).
- **POLICY 4d:** the memory line's value at risk.

### 3.7 F7 — RFC-0008 EXEC work slices (X8)

| Violation | C2 | C3 | C4 | C5 | C7 |
|---|---|---|---|---|---|
| A slice's computation lie (its kernel claim, OPV) | partial: V-node X8 E01 (verifier from the read API + the executor's published DA) | V-node E01 | V-node E01 (kernel scalar court) | V-node E01 (convicted; slice proven false; suffix, root and REAL claim void `WorkSliceProvenFalse`; root bond not charged) | V-node E01 (lane block, heartbeat anchor, fold) |
| Honest slices | — | — | — | V-node E02 (each verifies through an OPV Final; root ready; hold released) | V-node E02 |
| A slice's material withheld (its kernel claim defaults) | GAP at node; V-fold `exec_v2_fold_v1::amendment_1::a_defaulted_slice_voids_its_suffix_as_a_default_and_a_verified_one_convicted_later_still_voids_an_unsettled_root`, `…::a_verified_slice_whose_claim_forfeits_after_final_defaults_and_voids_an_unsettled_root` | | | | GAP-60 |
| Borrowed / unauthorised / skipping slice (job, input binding) | V-node E03 (anchored, credited nothing); V-fold `exec_v2_fold_v1::a_correct_slice_borrowed_from_another_root_or_job_is_refused_by_binding` | | | | met (E03) |
| Continuity (rule 4: slice `i+1`'s prompt = slice `i`'s prompt + output) | V-fold `exec_v2_fold_v1::each_admission_rule_refuses_by_name_in_the_specs_order_and_a_refusal_writes_nothing` | | | | GAP-60 |
| **Initial boundary** (slice 0's predecessor vs the REAL claim's output) | **DESIGN_GAP**: the root bond's declaration, unchecked (X8R §12.3) | | | | GAP-62 |

Family-wide:
- **C1, C2-full, C7-RPC:** GAP-01 / 02 / 03.
- **C8:** V-node E04 (restart; a replaying node agrees under any arrival order), E05 (IBD through both sync lists), E06 (reorg
  unanchors; credited once); the composed run replays.
- **Pruned import of the lane:** EXTERNAL drill.
- **Admission seam:** the REAL root's class is admitted through `exec_v2_test_admit_class_v1` (`cfg(test)`) — GAP-61.
- **Merge:** X8 predates `b8ae9412b` — GAP-63.
- **Pipeline-class slices:** refused by name (`VerificationKindUnsupported`). Not reward-bearing; consistent with F2.

### 3.8 F8 — onboarding conformance evidence and the artifact binding

| Violation | C2 | C3 | C4 | C5 | C7 |
|---|---|---|---|---|---|
| Forged leaf outcome (`LeafDecode` refutation) | V-node B03 (fresh verifier: SDK + core from public reads + the public artifact) | V-node B03 (an opening against the class's registered artifact root) | V-node B03 (one leaf) | V-node B03 (REFUTED, `CONFORMANCE_FAILED`) | V-node B03 |
| Forged vector tokens (`VectorTokens`) | V-node B03 | needs a Final, unconvicted claim of the bound kernel class. **A new class has none (GAP-B6).** | | V-node B03 | partial: GAP-70 |
| Forged vector logits / commit digests | **GAP**: no court (OB-P0 GAP 2) | | | | GAP-70 |
| Self-reported independent and backend results | **GAP** (GAP-B7; F-C4-11 class) | | | | GAP-70 |
| Forged outcome list | in-fold V-node B05 | | | V-node B05 | V-node B05 |
| Withheld evidence | V-node B03 (default at lock + 60, counted) | | | | V-node B03 |
| Evidence under another beacon / policy / scope / commitment | V-node B02 | | | V-node B02 (dismissed; rows untouched) | V-node B02 |
| Hostile refutations; refutation spam pre-empting proofs | V-node B04. C4: C4N1 (F-C4R4-10); OB / C4: C4N2 (F-C4R4-11, un-ignored, result not recorded) | | | | GAP-00 |
| Complete check (`PostComplete`, judged whole in the fold) | OB: B13, B14, B16 passed at run 4; B15 FAILED there (fixed at `6e9dfa065`, unbuilt) | no outsider needed: the fold runs every input | | | I (OB) |
| False artifact binding (104 vs the kernel root) | V-node B09, B10 (two disagreeing openings; the instance-set proof) | V-node B09 | V-node B09 | V-node B09 (binder slashed; the root never attested) | V-node B09 |
| Refusals, envelope expiry, ruleset binding | V-node B11, B12 | | | | V-node |
| Reward gate (OB) | — | — | — | OB B21–B24 (I: unbuilt head) | — |

Family-wide:
- **C1:** refuters are genesis cards (GAP-01).
- **C8:** V-node B07 (reorg, restart, pruned import of tables 39 / 40).
- **SDK fresh verifier:** it refuses v3 attempts (no public seal read, OB GAP-B17 — OPVB, condition 9). It also refuses complete-check
  attempts (GAP-B10).
- **C10:** the binding is refutable by any verifier holding the bytes (ADR-0177). DA16's lease gating is GAP-05.
- **Floor:** under `min_effective_bits = 128` only complete-check classes pass (OB B22), so today no real-size class can pass
  conformance.

### 3.9 F9–F16 — the legacy V2 Panel route (live on testnet-12)

The answer is the same for every legacy family, so it is given once. The evidence that exists is fold-level or seat-only, or pins
the gap.

| Col | Legacy V2 route under all-seats-collude | Evidence |
|---|---|---|
| C1 | Any Active bond may open a DA session (within the non-seat budget) or a court (`palw_court_v2`, no Panel approval). Consensus permits it. | V-fold `palw_operator_da_candidates.rs::a_claim_licensed_by_outside_signers_meets_the_operators_accusation_and_the_junk_is_convicted`; V-fold (dormant V3) `rfc0010_production_fold.rs::a_non_seat_public_bond_accuses_and_convicts_a_v3_bound_claim_exactly_as_it_would_a_v2_claim` |
| C2 / C3 | The claim's capture is served to seats (`FPC1` / `FPG1` / P2P pool). A non-seat reaches committed values only through on-chain DA units (`StepLeaf`, `StepRange`, `TirStepNode` / `TirRowNode`, `TirStepRun`). **No non-seat filer** builds the bisection for FP, Gen, TIR or improvement claims. The replay filer (P2-8b/d) and the held filer (P2-8e) are seat-only. The operator DA filer files row 0 only. | GAP (RFC-0014 P3) |
| C4 | **Held 8k / 2M (graph-v7, fused):** the three [C12] gaps are pinned — a canonical capture is past the cap, a lying fold's prefix is unreadable, and no DA unit discloses a committed fused tile (DA-3 refuses a fused `StepLeaf`, `DaUnitNeedsDissection`). The only end-to-end conviction reads the liar's own instance. | pins: `kaspad/src/palw_filer_held_e2e.rs::t54g_gap_a_a_canonical_8k_attempt_is_past_the_whole_capture_cap`, `…::t54g_gap_b_a_fresh_seat_cannot_bisect_a_lying_held_fold`, `…::t54g_gap_c_a_fresh_seat_cannot_build_the_opening_of_a_lying_held_fold`; excluded (`ServedView`): `…::t54g_a_garbage_fused_leaf_is_dissected_by_the_seats_node_and_its_producer_convicted` |
| C5 | Junk (no preimage) defaults (V-fold). **A self-consistent garbage trace answers the row-0 accusation**: the session is refuted and the non-seat accuser's exposure is burned (`palw_operator_da.rs` module doc). Step / logits / job / borrowed-root convictions exist in the fold, but the evidence is built from the producer's own execution, not acquired publicly. | V-fold `t46_false_valid_real_claim.rs::t46b_injected_step_fault_convicts_before_final`, `…::t46e_after_final`, `…::t46u_a_logits_fault_convicts_the_full_seat_only`, `…::t18z_a_job_fault_convicts_every_valid_signer`, `…::t18b_a_borrowed_root_answers_another_job`, `…::t18c_before_licence_the_executor_is_refuted` |
| C6 | `ExecutorRefuted` is refused while any court is open (`ClaimUnderSession`, RFC-0014 [S03]). One bond's session pre-empts another's direct proof. | GAP (RFC-0014 §7.4) |
| C7 | No chain-path test with a non-seat outsider. | GAP |
| C8 | **Past `palw_rcore_plus` only a seat's session pauses a V2 claim (V3S-08).** A bystander's accusation is outrun by the receipt and bind timeouts; the claim voids uncharged. The guard `palw_accusation_pending_v1` holds only V3 ends and lane-A part C, both dormant. | `t46_false_valid_real_claim.rs::t18c_ii_a_root_with_no_preimage_defaults` (doc of `t18c_ii_body`); V-fold dormant `palw_panel_part_c_g14_hold.rs::a_non_seat_da_session_holds_the_second_panels_expiry_and_the_default_wins`, `rfc0010_g14_guard.rs::a_pending_da_session_holds_a_v3_s2_licence_past_its_gate_and_the_default_wins`; seat-side restart / reorg V-fold `t46m_reorg_and_restart` |
| C10 | Claim-specific DA units exist. The class lifecycle (`Prefetching → Probation`) waits on seats proving readiness, which is model acquisition by seats. | — |

Per-family notes:
- **RFC-0006 TIR shard:** a non-seat watcher exists at node-function level
  (`kaspad/src/palw_panel/tir_shard_e2e.rs::a_non_seat_watcher_verifies_whole_shards_and_accuses_a_lie_as_itself`, V-unit). It reads
  the capture from a public provider pool, and the producer chooses whether to publish there. There is no chain path and no Final
  hold on V2.
- **RFC-0010 V3:** a Panel configuration, not a plan family. It changes who sits, not what a non-seat can prove.
- **RFC-0007 vertex / witness / mesh / capped:** Panel-side verification aids. They change nothing above.

**Verdict.** None of F9–F16 can meet G14 without RFC-0014 P1–P4 on the V2 route. That means a shared non-seat filer, a held DA
unit (`CommittedKernelWitness`), a bounded reservation that holds Final for a non-seat pursuit, and direct-proof priority over open
courts. Each is listed as must-not-be-reward-bearing on any G14-gated channel (§4). OB's gate already exempts them as
`LEGACY_PANEL_ROUTE`: they keep earning on the old route, per the user's 2026-10-09 ruling #5. That conflicts with PRINCIPLES §6
for earning after the full-activation release. **User decision, GAP-80.**

### 3.10 F17 — the hole in G14-for-rewards: post-fence V2 REAL work

OB's `palw_reward_gate_v1` returns `Passed` for a post-fence V2 class when four things hold:
- its onboarding gate is `Ready`;
- E1–E7 hold through its kernel binding;
- its context is inside the bound plan's `max_positions`;
- then "its REAL work is admitted on that ground".

But that REAL work is **V2 claims with V2 step-tree roots**. The kernel route convicts only kernel claims. So under all-seats-collude,
a lying V2 REAL attempt of a `Passed` class is prosecutable only by the legacy path (§3.9), which is not G14. X8R's slices cover the
continuation work, not the REAL root's own computation (the initial boundary, GAP-62). OB GAP-B14 names only the Final route.

**GAP-81:** decide that a `Passed` class's REAL work must itself be a kernel-route claim (or be bound 1:1 to a kernel claim of the
same job, the way slices are), or give the V2 claim a G14 path.

## 4. Must not be reward-bearing (RFC-0015 §1.1) — the full list; nothing is silently narrowed

1. **K2-TIR-v3 pipelines and media classes**, in both modes, until GAP-20 and GAP-21. Panel-licensed pipeline mode has no receipt
   path at all.
2. **K2-TIR-v4 at 2M**: refused by the gate. **v4 canonical 8k held and the 262k window** until GAP-31. Every v4 class until
   GAP-30, and until the detection policy (`q`, `P_run`; readiness 4e) is set.
3. **K2-TIR-v5 encoders and task heads** (and HFX's Head profile on the Gen route) until GAP-40.
4. **Every kernel-route profile needing private or fused material**: by construction.
5. **RFC-0004 typed roots (Memory / Retrieval / Composite)** until GAP-50, GAP-51 and GAP-52.
6. **Sampled conformance as a reward gate**: decided (§5a). Only the complete check gates rewards for the release.
7. **The legacy V2 Panel route (F9–F16)** on every G14-gated channel: not Panel=0, not new-reward eligible. Whether the old
   channel continues is GAP-80, with the user.
8. **V2-root claims of a kernel-bound class**: decided (§5a) — they never earn the new rewards; only kernel-route claims pass the
   per-claim reward gate (OPVB implements).
9. **K2-TIR v1/v2 single-program classes** until GAP-00, -01, -02, -03, -04, -05, -06 and -10. This is the nearest family.
10. **EXEC slice legs** until GAP-60 to -63 and F1's items. Pipeline-class slices are refused.

Economics are outside the G14 columns, but they block reward under PRINCIPLES §6.5 and §6.6:
- the verifier incentive / bounty capture (ECON);
- ADR-0176 budgets (BUDGET);
- OPV collateral from max gain ÷ detection;
- watcher absence (C4R4 O-C4R4-02).

## 5. GAP list — owner proposal and size

Sizes: **S** ≤ ½ day · **M** 1–2 days · **L** 3–5 days · **XL** > 1 week, or needs a design decision first. One cargo invocation per
milestone throughout.

| GAP | What is missing | Cols | Families | Owner | Size |
|---|---|---|---|---|---|
| GAP-00 | INT still carries the proven C5 / C6 defects F-C4R3-02 / 03 / 05. Their fixes are on R4 (m5 green except R04). The C4R4 fixes (F-C4R4-03 `a1473a3d9`, -05 `2dfcee86e`, -10 `63da18025`) are on C4 only, and F-C4R4-11 (`b5d4ba90c`) is on OB / C4. C4 does not compile at its head. Plus a node PoC for F-C4R4-05 (V-ref only). | C5, C6, C8 | F1, F3, F6–F8 | **G14R**: carry C4's three fixes onto `g14/r4-fixes`, fix R04, get one green build of the merged head; then the Lead integrates R4 → OB → C4 | M |
| GAP-01 | Ordinary public entry: prosecution by a bond registered after genesis through `BondRegistered` (published collateral, maturity, fees), matured, non-seat, non-operator, with every seat colluding. | C1 | all | **G14C** (one reusable harness helper + node test) | M |
| GAP-02 | Fresh node: a second node started after the claim (IBD, and separately a pruned import mid-prosecution) builds the outsider from its own reads and prosecutes through its own mempool or the relay. | C2, C8 | all | **G14C** | M |
| GAP-03 | RPC leg: ops 210–212 / 231 served by a running node to the outsider client; the prosecution submitted by RPC `submitTransaction` (node-less) → fold → conviction. | C7 | all | **G14C** (the `testing/integration` daemon harness with the fences armed through the `Config` seam) | M–L |
| GAP-04 | ADR-0177 non-interference: the same chain and proofs give identical consensus results when artifact peers and local acquisition vary (RFC-0014 §16.8). | C10 | all kernel | **G14C** | S |
| GAP-05 | DA16 (dormant, on INT) makes availability a consensus condition, which ADR-0177 withdraws: tag-150 `Artifact` leases, 104 binding only over ≥ 2 live leases, a lapsed pair attesting nothing (`da16_unanswered_challenges_slash_the_providers_and_a_lapsed_pair_attests_nothing_until_rebound`). Keep the claim-material provider court. | C10 | F1, F3, F6–F8 | **DA16** (DA16b re-scope) | M |
| GAP-06 | Claim-specific court scope: permitted units per route, a cumulative scope per claim and requester, and a test that no demand can name weight or artifact ranges (kernel `FileDemand`, the DA16 provider court, R4X spec demands). | C10 | all kernel | **DA16** (court-scope D2) | M |
| GAP-07 | Worst-case deadlines measured: `T_challenge ≥ T_beacon + T_fetch + T_check + T_localize + T_file + T_margin` per class; windows are INTERIM. | C8 | all | MEAS (EXTERNAL) | — |
| GAP-10 | Node cases for K2 v1/v2 violations b–j (§3.1b): exact families, `i128` CRT, routing/TopK/MoE, history window, boundary, job copy, borrowed input, decode, consistent garbage. On a multi-layer fixture, Panel-licensed (all seats) and OPV, pre- and post-Final. | C2–C5, C7 | F1 | **G14C** | L |
| GAP-20 | Pipeline claims on the node: stage / edge / VLM `R` / decode / withheld stage, after K2S's GAP-6 wire form lands. | C2–C5, C7, C8 | F2 | **G14C** (after K2S integrates) | L |
| GAP-21 | Pipeline onboarding / eligibility path (OB GAP-B4). | gate | F2 | **G14C** (OPVB reviews E1–E7) | M |
| GAP-30 | v4 at node: reorg / restart / pruned import / duplicate of tables 20–21; post-Final conviction; pre-emption and simultaneous filers; prompt-tile lie; continuity lie; garbage / borrowed; decode. | C5, C6, C8 | F3 | **K2S** | L |
| GAP-31 | v4 canonical 8k held: a history-bearing class (attention `Hist` windows) at 8,192 positions on the node, at real shape; the 262k window; real weights from H1 when available. | C2–C5, C7 | F3 | **K2S** | L–XL |
| GAP-32 | K2S ↔ R4 merge (`ledger.rs` / `rows.rs` / `route.rs` / `processor.rs`); `SaltedCommitV1::Segmented`. | — | F3 | **K2S** | M |
| GAP-40 | v5 encoders and heads: the SDK court test green; a node E2E; the `HEAD_DECODE_V1` court; the kernel binding a post-fence Head class needs. | all | F4 | **K2S** | L |
| GAP-50 | Typed-root eligibility: E1–E7 for `Spec` kinds (OB GAP-B16), replacing the `cfg(test)` seam. | gate | F6 | **G14C** (OPVB reviews) | M |
| GAP-51 | Typed roots at node: reorg / restart / pruned import of tables 22–24 and the line head; post-Final conviction with line rollback; Final race; pre-emption; decode lie; composite model-stage and logits→query edge. | C5, C6, C8 | F6 | **G14C** (lane R4X is not running) | M |
| GAP-52 | The retrieval snapshot binding attests every leaf well-formed (key length `D`, payload ≤ `P`). | C3, C5 | F6 | **DA16** (binding scope), or G14C | M |
| GAP-60 | Slice DA default at node: the withheld position of a slice's kernel claim → default → suffix and root void; rule-4 continuity at node. | C5, C7 | F7 | **X8R** | S–M |
| GAP-61 | Replace `exec_v2_test_admit_class_v1` with OB's `palw_reward_gate_v1` (`Cw::active_admitting_real`). | C7 | F7 | **X8R** (after OB integrates) | S |
| GAP-62 | DESIGN_GAP: link the initial boundary to the REAL claim's verified output (the same root cause as GAP-81). | C3, C5 | F7, F17 | **X8R** + Lead | M |
| GAP-63 | Merge `b8ae9412b`; rebuild. | — | F7 | **X8R** | S |
| GAP-70 | Sampled conformance: a court for vector logits / commit digests (OB-P0 GAP 2); vector refutation for a new class (GAP-B6); self-reported implementation results (GAP-B7). **Or** the Lead rules that sampled conformance never gates reward and only `PostComplete` counts, until a production policy exists. | C3–C5 | F8 | **Decided (§5a): only the complete check gates rewards; the digest court stays on the DESIGN list, nobody builds it now** | — |
| GAP-71 | SDK fresh verifiers: a public seal read for v3 attempts (GAP-B17, condition 9) → **OPVB** (S–M); a fresh complete-check verifier (GAP-B10) → **G14C** (S). | C2 | F8 | OPVB / G14C | S–M |
| GAP-80 | DECISION: the legacy V2 Panel route cannot meet G14 (§3.9). Does `LEGACY_PANEL_ROUTE` keep earning after the full-activation release (ruling #5) despite PRINCIPLES §6? If G14 is required there: RFC-0014 P1–P4 on V2 (shared non-seat filer, `CommittedKernelWitness`, a Final-holding reservation for non-seat pursuits, direct-proof priority over open courts), plus a chain-path non-seat E2E for each of F9–F16. | all | F9–F16 | **With the user (§5a)**; meanwhile "not Panel=0 / not new-reward eligible"; no XL work starts | XL |
| GAP-81 | A `Passed` class's V2 REAL work has no G14 path (§3.10). **Decided (§5a):** the reward gate is per CLAIM verification route. | all | F17 | **OPVB** implements | L |
| GAP-82 | Only if GAP-80 keeps legacy in scope: a chain-path E2E of `--palw-tir-shard-watch` with every seat colluding, plus a V2 Final hold. | C2–C8 | F15 | G14C | M |

### 5a. The Lead's decisions on the open points (2026-10-10, on milestone 1)

* **GAP-81 — the reward gate is per CLAIM verification route, not per class.** Only a claim whose work is verified on a
  G14-complete route (the kernel route) can pass `palw_reward_gate_v1`. V2-root claims of a kernel-bound class stay on the legacy
  channel under the old rules and never earn the new rewards. OPVB implements it.
* **GAP-70 — for the release, only the complete check gates rewards.** Sampled conformance stays a non-reward signal until a digest
  court exists. That court stays on the DESIGN list; nobody builds it now.
* **GAP-80 — the legacy Panel route goes to the user.** Until the user answers, this matrix treats F9–F16 as "not Panel=0 / not
  new-reward eligible", and no XL work on them starts.

## 6. Proposed order

1. **Now, in parallel (so every branch is green on `b8ae9412b`):**
   - GAP-00 (G14R);
   - GAP-32 (K2S);
   - GAP-63 (X8R);
   - the Lead integrates R4 → OB → C4 → KS → X8.

   G14C starts from the integrated tree, or from R4 if that integration is late.
2. **Decisions requested from the Lead / user now**, because they change scope: GAP-80 (legacy), GAP-81 (V2 REAL work of a
   `Passed` class) and GAP-70 (sampled conformance).
3. **G14C milestone 2 — the canonical G14 node harness:** GAP-01, GAP-02, GAP-03 and GAP-04 in one module of
   `consensus/src/pipeline/virtual_processor/tests/`, on the K2 single-program class, Panel-licensed and OPV. It covers:
   - a post-genesis bond;
   - a node started after the claim, IBD and pruned-import variants;
   - its own reads, and RPC where the harness allows it;
   - every seat signing `Valid`;
   - one non-seat public bond convicting pre- and post-Final;
   - a DA default and an honest-claim dismissal;
   - acquisition variance with identical roots.

   Every later family reuses this harness.
4. **G14C milestone 3:** GAP-10 (node violation cases b–j).
5. **In parallel on other lanes:**
   - K2S: GAP-30, then GAP-31, then GAP-40;
   - DA16: GAP-05, GAP-06, GAP-52;
   - X8R: GAP-60, GAP-61, GAP-62;
   - OPVB: GAP-71 (B17).
6. **G14C milestone 4:** GAP-50 and GAP-51 (typed roots).
7. **G14C milestone 5:** GAP-20 and GAP-21 (pipelines), once K2S's wire form is integrated.
8. GAP-70, GAP-80 and GAP-81 are decided or with the user (§5a). GAP-82 waits on GAP-80.

**The Lead's order for G14C (2026-10-10):** milestone 2 (GAP-01 to 04), then milestone 3 (GAP-10), then GAP-71b, then typed roots
(GAP-50/51), then pipelines (GAP-20/21) once K2S's pipeline header is integrated. `g14/r4-fixes` is merged into `g14/completion` only
once G14R reports it green; until then the harness is built on `b8ae9412b` with the conviction path's specifics behind an adapter.

## 7. Test index

Paths: `kre` = `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e.rs`, and `kre/<m>` = its child module `<m>.rs`.
A level is the level of the test, not of the cell.

**Kernel route, INT** (V-node, `lead-v1`):

| Id | Test |
|---|---|
| N01 | `kre::g14_kernel_route_a_covered_lie_is_convicted_by_an_outsider_through_the_real_path` |
| N02 | `kre::g14_kernel_route_a_covered_lie_is_convicted_after_final_within_the_liability_horizon` |
| N03 | `kre::g14_kernel_route_a_withheld_position_is_a_demand_then_a_default_never_a_conviction` |
| N04 | `kre::g14_kernel_route_malformed_wrong_root_and_fake_opening_responses_are_rejected_then_the_producer_defaults` |
| N05 | `kre::g14_kernel_route_a_served_position_completes_the_check_and_convicts` |
| N06 | `kre::g14_kernel_route_spam_demands_never_preempt_a_direct_proof_and_settle_moot` |
| N07 | `kre::g14_kernel_route_simultaneous_challengers_one_conviction_one_duplicate_and_one_slash` |
| N08 | `kre::g14_kernel_route_a_duplicate_proof_after_a_replay_changes_nothing` |
| N09 | `kre::g14_kernel_route_final_waits_the_proof_grace_so_a_late_served_lie_is_convicted_before_final` |
| N10 | `kre::g14_kernel_route_spam_demands_cannot_hold_final_past_window_end_plus_court_deadline_plus_proof_grace` |
| N11 | `kre::g14_kernel_route_a_bond_with_liability_cannot_exit_until_the_horizon_releases_it` |
| N12 | `kre::g14_kernel_route_a_chunked_object_is_signature_checked_at_the_completing_chunk` |
| N13 | `kre::g14_kernel_route_replay_and_reorg_reach_the_same_roots` |
| N14 | `kre::g14_kernel_route_survives_a_node_restart_over_the_same_database` |
| N15 | `kre::g14_kernel_route_survives_a_pruned_import` |
| N16 | `kre::g14_kernel_route_the_block_adjudication_budget_bounds_the_block_not_each_object` |
| N17 | `kre::g14_kernel_route_the_read_model_serves_a_claim_and_rows_that_rebuild_the_committed_root` |
| N18 | `kre::g14_kernel_route_hostile_objects_are_dropped_or_dismissed_and_never_stop_the_chain` |
| O01 | `kre::g14_opv_an_honest_claim_finalizes_with_no_panel_and_exports_a_panel_independent_beacon_fact` |
| O02 | `kre::g14_opv_a_lying_claim_is_convicted_by_a_fresh_outsider_before_final` |
| O03 | `kre::g14_opv_a_lie_that_finalized_is_convicted_within_liability_and_its_fact_is_withdrawn` |
| O04 | `kre::g14_opv_withheld_material_defaults_the_producer_burns_a_share_and_frees_the_job` |
| O05 | `kre::g14_opv_spam_demands_cannot_hold_final_past_the_hard_deadline` |
| O06 | `kre::g14_opv_registration_is_dropped_without_the_fence_the_admission_or_the_right_tag_and_the_legacy_route_coexists` |
| O07 | `kre::g14_opv_replay_and_reorg_reach_the_same_roots` |
| O08 | `kre::g14_opv_survives_a_node_restart_and_finalizes_after_it` |

**C4 round 3** (`kre/c4r3`):

| Id | Test | Status |
|---|---|---|
| X01 | `g14_c4r3_mandatory_3_a_relayed_lie_covered_by_every_seat_is_convicted_by_an_outsider` | V-node |
| X02 | `g14_c4r3_mandatory_3_opv_relayed_lies_are_convicted_before_and_after_final` | V-node |
| X03 | `g14_c4r3_a_self_inflicted_default_must_not_erase_a_provable_fraud` | INT: ignored FAIL F-C4R3-02. R4: un-ignored, pass at m5. |
| X04 | `g14_c4r3_opv_a_self_inflicted_default_must_not_erase_a_provable_fraud` | as X03 |
| X05 | `g14_c4r3_opv_two_bonds_must_not_be_able_to_hold_the_whole_opv_lane` | INT: ignored FAIL F-C4R3-05. R4: pass at m5. |
| X06 | `g14_c4r3_eight_junk_chunk_groups_must_not_hold_a_chunked_proof_off_the_chain` | INT: ignored FAIL F-C4R3-03. R4: pass at m5. |
| X07 | `g14_c4r3_control_a_chunked_proof_convicts_when_the_chunk_lane_is_free` | V-node |
| X08a | INT `g14_c4r3_observation_gap_r7_a_lifted_proof_takes_the_bounty_and_halves_the_colluders_loss` | observation |
| X08b | R4 `g14_c4r3_gap_r7_a_lifted_proof_pays_its_earliest_sealer_not_the_copyist` | V-node at m5 |

**G14R** (R4, `kre`):

| Id | Test | Status |
|---|---|---|
| R01 | `g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_across_reorg_replay_and_redemption` | V-node at m5 |
| R02 | `g14_kernel_route_the_routes_own_chunk_lane_is_per_bond_deposit_backed_and_bounded_by_its_target` | V-node at m5 |
| R03 | `g14_opv_a_claim_commits_only_over_its_salted_seal_and_its_salt_is_kept` | V-node at m5 |
| R04 | `g14_opv_the_salted_seal_rows_roll_back_and_reapply_identically_across_a_reorg` | **FAILED at m5** |

**C4R4** (C4):

| Id | Test | Status |
|---|---|---|
| — | kernel `misaka-palw-kernel/tests/c4r4.rs::f_c4r4_05_one_junk_filing_must_not_buy_a_whole_blocks_court` | V-ref (record m1) |
| — | kernel `…::f_c4r4_04_the_pre_final_default_liability_horizon_convicts_on_its_last_day_and_not_after` | V-ref |
| — | kernel `…::f_c4r4_02_obs_watcher_absence_leaves_an_unverified_lie_permanent_and_quantifies_the_loss` | V-ref observation |
| — | kernel `…::f_c4r4_07_obs_a_lying_producers_sybil_takes_the_bounty_of_its_own_conviction` | V-ref observation |
| C4N1 | `kre/conformance/c4r4::g14_c4r4_junk_conformance_refutations_must_not_spend_the_runs_reserved_for_proofs` | V-node (record m1) |
| C4N2 | `…::g14_c4r4_free_junk_refutations_must_not_carry_forged_evidence_through_its_window` | un-ignored after OB's fix; result not recorded |
| C4N3 | `…::g14_c4r4_a2_a_tag_113_carrier_below_its_fence_is_judged_as_undecodable_bytes` | ignored FAIL, an A2U item |

**K2S** (KS, `kre/real_scale`; V-node at `k2s-e1-node`):

| Id | Test |
|---|---|
| S01 | `g14_k2s_a_tiled_prompt_past_4096_ids_commits_as_a_multi_segment_claim_on_the_real_node` |
| S02 | `g14_k2s_a_lie_in_one_segment_is_localized_and_convicted_with_bounded_bytes` |
| S03 | `g14_k2s_a_withheld_segment_is_demanded_and_defaults_never_a_conviction` |
| S04 | `g14_k2s_the_mempool_runs_the_kernel_acceptance_gate` |
| S05 | `g14_k2s_a_demanded_position_served_on_chain_is_checked_from_the_blocks_and_convicted` |

KS kernel tests (V-ref / V-unit), in `misaka-palw-kernel/tests/k2_real_scale.rs`:
- `k2s_huihui_qwen35_9b_8k_passes_the_real_scale_gate_and_the_carriers`
- `k2s_a_reexecuting_verifier_finds_any_lie_with_certainty_reading_two_positions`
- `k2s_every_tile_and_part_constant_fits_the_node_carrier`
- `k2s_a_position_is_served_in_parts_and_its_demand_bonds_wait_for_the_grace`

**Typed roots, INT** (`consensus/src/pipeline/virtual_processor/tests/r4x_typed_roots_e2e.rs`; V-node at `lead-v1`; on OB they
failed at run 4, then fixed through the seam, unbuilt):

| Id | Test |
|---|---|
| T01 | `r4x_weights_only_spec_is_byte_for_byte_the_legacy_registration_and_the_unarmed_route_is_unchanged` |
| T02 | `r4x_memory_class_end_to_end_a_lie_in_one_step_is_convicted_and_memory_is_carried_across_two_jobs` |
| T03 | `r4x_memory_a_withheld_pre_state_is_classified_as_a_default` |
| T04 | `r4x_retrieval_a_wrong_item_and_a_missed_better_item_are_convicted_and_a_withheld_slice_defaults` |
| T05 | `r4x_composite_a_lie_in_the_tool_stage_is_convicted_at_that_stage` |

**EXEC slices** (X8; V-node per record `x8r-m7` / `x8r-m8`):

| Id | Test |
|---|---|
| E01 | `kre/conformance/exec_slices::x8_g14_an_outsider_convicts_a_slices_kernel_claim_and_its_suffix_and_root_void_on_the_real_node` |
| E02 | `kre/conformance/exec_slices::x8_g14_honest_slices_verify_through_kernel_finals_and_the_root_is_ready_on_the_real_node` |
| E03 | `consensus/src/pipeline/virtual_processor/tests/t12_exec_v2_carriage.rs::t12_exec_v2_an_unauthorised_a_skipping_and_a_borrowed_slice_are_anchored_and_credited_nothing` |
| E04 | `…::t12_exec_v2_lane_state_survives_a_restart_and_a_replaying_node_agrees_whatever_the_arrival_order` |
| E05 | `…::t12_exec_v2_ibd_carries_the_anchored_lane_blocks_through_both_sync_lists` |
| E06 | `…::t12_exec_v2_a_reorg_unanchors_the_lane_and_the_winning_branch_anchors_and_credits_it_once` |

Fold tests in `consensus/core/src/palw_state_v2/tests/exec_v2_fold_v1.rs` are cited in §3.7.

**Onboarding / conformance, INT** (V-node at `lead-v1`):

| Id | Test |
|---|---|
| B01 | `kre/conformance::g14_conformance_evidence_passes_only_after_an_unrefuted_window_and_the_class_activates` |
| B02 | `…::g14_conformance_evidence_against_another_beacon_policy_scope_or_commitment_is_refused_and_stale_evidence_is_invalid` |
| B03 | `…::g14_conformance_forged_evidence_is_refuted_withheld_evidence_defaults_and_attempts_are_exhausted` |
| B04 | `…::g14_conformance_hostile_evidence_is_dismissed_or_failed_spends_budget_and_never_stops_the_chain` |
| B05 | `…::g14_conformance_a_forged_outcome_list_fails_the_attempt` |
| B06 | `…::g14_conformance_evidence_is_dropped_by_name_without_the_fence` |
| B07 | `…::g14_conformance_rows_survive_reorg_restart_and_pruned_import` |
| B08 | `kre::g14_onboarding_a_class_is_bound_attested_registered_and_released_by_the_gate` |
| B09 | `kre::g14_onboarding_a_false_artifact_binding_is_refuted_by_two_disagreeing_openings` |
| B10 | `kre::g14_onboarding_a_binding_over_the_wrong_set_of_tensors_is_refuted_without_any_opening` |
| B11 | `kre::g14_onboarding_refusals_leave_the_rows_untouched` |
| B12 | `kre::g14_onboarding_a_signed_registration_envelope_expires_and_binds_its_ruleset` |

**OPVB** (OB; I — run 4 had 6 failures, fixed at `6e9dfa065`, unbuilt):

| Id | Test |
|---|---|
| B13 | `kre/opv_bootstrap::g14_opv_bootstrap_from_zero_finals_a_complete_check_seeds_the_beacon_and_a_sampled_class_becomes_eligible` |
| B14 | `…::g14_opv_bootstrap_a_class_that_cannot_be_checked_whole_is_refused_the_complete_check` |
| B15 | `…::g14_opv_bootstrap_a_block_of_hostile_complete_checks_spends_budget_and_never_stops_the_chain` (FAILED at run 4) |
| B16 | `…::g14_opv_bootstrap_without_a_complete_check_class_the_beacon_never_comes_and_the_chain_lives` |
| B17 | `…::g14_opv_bootstrap_eligibility_is_lost_when_the_artifact_binding_is_refuted` |
| B18 | `…::g14_opv_bootstrap_the_deny_list_takes_eligibility_away_and_never_grants_it` |
| B19 | `…::g14_opv_bootstrap_a_sealed_source_v3_beacon_locks_on_salted_seals_and_the_class_passes` (FAILED at run 4) |
| B20 | `…::g14_opv_bootstrap_a_withheld_v3_seal_vetoes_the_attempt_and_is_counted` |
| B21 | `kre/conformance::g14_rewards_an_onboarded_class_is_active_and_admits_real_attempts` |
| B22 | `…::g14_rewards_under_the_ruled_floor_a_two_bit_conformance_earns_nothing` |
| B23 | `…::g14_rewards_a_class_that_never_began_onboarding_never_activates` |
| B24 | `…::g14_rewards_a_legacy_panel_route_class_keeps_the_old_route_and_never_earns_opv_without_the_gate` |

**DA16, INT** (V-node at `lead-v1`), `kre/da16`:
- `da16_an_outsider_confirms_an_honest_binding_from_the_bytes_its_bonded_providers_serve`
- `da16_a_false_binding_is_refuted_from_the_bytes_and_the_bound_side_the_court_forces_out`
- `da16_unanswered_challenges_slash_the_providers_and_a_lapsed_pair_attests_nothing_until_rebound` (contrary to ADR-0177: GAP-05)
- `da16_a_transfer_needs_two_operators_through_the_bound_reserving_at_least_the_claim`
- `da16_a_transferred_claims_unanswered_demand_charges_every_provider_never_the_producer`
- `da16_a_common_mode_provider_outage_voids_the_claim_without_a_miner_slash`
- `da16_a_reorg_takes_the_transfer_back_and_the_producer_is_liable_again`
- `da16_a_false_computation_on_a_transferred_claim_still_convicts_the_miner`
- `da16_below_the_fence_court_objects_are_dropped_and_an_older_claim_stays_its_producers`

## 8. Change log

* 2026-10-10 — created (G14C milestone 1). Read only: no build was run for this milestone. Every level is taken from the recorded
  runs in §1.
* 2026-10-10 — milestone 2 (G14C, run `g14c-m2`, on `b8ae9412b` plus this branch): **verified** — the canonical harness
  `g14_kernel_route_e2e/canonical.rs`, 10 tests (`g14_canonical_*`, Panel-licensed with every seat signing and OPV): a bond registered
  after the claim through a real `BondRegistered` carrier from its own node (IBD and pruned-import starts), reads through the shared
  RPC builders of ops 210–212 (`kaspa_rpc_core::convert::palw_kernel`, which the RPC service now calls), conviction before and after
  Final, DA default, wrong challenge dismissed, and ADR-0177 non-interference; plus `conformance::g14_canonical_ops_231_and_212_…`
  (op 231 + 212 from an IBD node rebuild the SDK verdict). 11/11 pass; rpc-core 127 pass; rpc-service checks. Re-run of the rest of
  `g14_`/`r4x_` at `b8ae9412b`: 76 pass, 4 ignored (the known FAILs), 1 failed — `g14_registration_replay_on_a_second_node_and_across_a_reorg`,
  the known GAP-11 race (fixed on `g14/r4-fixes` by `39e4ea441`). GAP-01..04 move to verified at node level for F1 (the socket hop of
  GAP-03 stays a drill: a kaspad with the route armed is refused by validation).
