# G14 completion matrix — the authoritative gap list (lane G14C, milestone 1)

Owner: lane G14C. Branch `g14/completion`, from the integration head `b8ae9412b` (2026-10-10). This document only answers one
question for every reward-bearing plan family: **which G14 conditions are proven by a test, at what level, and what is missing.**
It does not replace `g14-integration-matrix.md`, `g14-node-e2e-record.md` or the lane records. Those keep their history. For the
question of G14 completeness, this matrix supersedes their "Real node" columns.

**Latest review refresh:** §2b records the Codex branch through numbered public DA reacquisition and the public-history / tag-158 DA-worker follow-ups; it overrides older status descriptions only for the
specific cases it names. The milestone tables remain evidence history, not proof that every currently allowed profile is complete.

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

## 2a. Refresh against integration `2d93ac288` (2026-10-10, after milestones 2–3 and LG14-B / BUDGET merged)

The §3 tables below are the milestone-1 baseline. This table is the current state per family; where it differs, it wins. "V-node"
means a real-node chain-path test on `g14/completion` at this head (run `g14c-m6`: 45 tests — `g14_canonical`, `r4x_`, `g14_opv_bootstrap` — pass), with the canonical harness's post-genesis
bond, a node started after the claim (IBD or pruned import) and reads through the RPC ops' builders, unless noted.

| Family | C1 public entry | C2 fresh node | C7 RPC leg | Lie types at V-node | Recovery (C8) | Eligibility / reward gate | Open |
|---|---|---|---|---|---|---|---|
| F1 K2-TIR v1/v2 single program | V-node | V-node (IBD, pruned) | builders + JSON wire (socket hop: drill) | MatMul, quantization/rounding, decode, garbage, DA default, wrong-challenge dismissal; boundary / copy / borrowed refused at inclusion | V-node (parent N13–N15 + canonical) | OPVB derived (E1–E7) + per-claim reward gate (GAP-81 decided) | routing / history: pinned refused by carrier fit (K2S v4, GAP-30/31) |
| F2 pipelines | GAP | GAP | GAP | V-ref only | — | no onboarding path (GAP-B4) | waits for K2S's header wire form (GAP-20/21) |
| F3 K2-TIR-v4 real scale | — | — | — | K2S branch only | — | — | K2S (GAP-30/31/32) |
| F4 v5 encoders / heads | — | — | — | K2S WIP | — | — | K2S (GAP-40) |
| F5 private / fused material | refused by construction | | | | | never registers | — |
| F6 typed roots | V-node (fresh pruned node in the post-Final case) | partial | — | memory step lie, withheld pre-state, retrieval wrong / missed item, composite tool AND model stage, memory lie after Final with line rollback | replay, pruned import, real restart | **derived** (G14C GAP-50): Memory via its rule program, Composite via every component, Retrieval never; **re-gated at every commit** (this head) | retrieval snapshot statement (GAP-52 binding is DA16's; conformance for snapshots: DESIGN) |
| F7 EXEC slices | (X8 branch) | | | X8 branch | X8 branch | per-claim reward gate (GAP-81) | X8R not on this head |
| F8 conformance | V-node | V-node (op 231/212 from an IBD node) | builders | complete check: the fold judges; a fresh node re-verifies from reads + its own artifact (GAP-71b) | B07 | only the complete check gates rewards (GAP-70) | digest court: DESIGN |
| F9–F16 legacy V2 Panel route | fold-level (LG14-B: any bond) | — | — | **V-fold** (LG14-B, kaspad `lg14b_*`): garbage gather (tag 159), mid-layer matmul (tag 159), fused-tile lie (CKW → held dissection), withheld node / CKW → `ProducerWithholding`, honest vs malicious outsider; every seat colluding; [C12] gaps (a)–(c) closed at fold level | V-fold delta re-apply / revert / carriage | the user's GAP-80: legacy must meet G14 | **V-node on the canonical harness: GAP** — needs a held fixture class registered on the testnet-12 harness and LG14-A's common filer (the responder wiring); Final hold + pre-emption for non-seat pursuits (LG14-A); RPC 207–209 |

**BUDGET (ADR-0176) hooks on this head.** Wired behind `palw_bond_budget_v1` (dormant): the V2 attempt and merged attempt, riders, the
free-prompt commitment and receipt spends, Final (reward and weight clip), immature weight, retire / void / conviction close hooks,
and the model allocation (BUDGET's rows 1–5, 7, 9–11, 19). **Not calling them yet:**
- the kernel route's Final reward (hook H-1: reserve at kernel claim commit, consume at `FinalReward`) — the route's OPV producer
  rewards with it (H-2);
- RFC-0004 typed-root claims (H-1 for spec jobs);
- RFC-0008 EXEC reward legs / reward blocks (H-3; X8R not on this head);
- rule E's reads of budgeted Final weight (H-4; FINX);
- the FP admission pre-checks (H-5).

Until those call the engine, every kernel-route, OPV, typed-root and EXEC reward stays non-reward-bearing (ADR-0176), independently
of G14.

## 2b. Codex review refresh (`b6a90a0d0`, 2026-10-10)

Detailed changes, commands, failures and reruns are in the [audit implementation record](../../audit/g14-review-2026-10-10/implementation.md).
The **overall G14 result remains FAIL / incomplete**; no activation or new family-wide PASS follows from these repairs.

**Numbered public DA reacquisition follow-up:** the dormant LG14-A tag 156 now requests a fresh public copy of one historical Event/Held/LG14-B unit, signed over its reservation DAA, next session counter and expiry. Historical answered bits remain replay guards; new producer duties, authenticated answers and timeout use the pending reacquisition. Flat coverage is reported without expanding the unit list. The history reader preserves authenticated prefixes and reads new-tip answers despite an unavailable old suffix. These changes address part of the pruned-answer gap named below; they do not establish full C2/C7/C8. Permanently pruned FP job commitments, TIR/pipeline acquisition, maximum-input budgeting, full-node inclusion and largest-population response scheduling remain gaps. Final commands and evidence are in the audit implementation record.

**PanelDA input follow-up:** a Pending public filer can reserve and demand the binding and authenticated input before it has its own replica. New LG14-B input chunks bind the global token count, tile indices and input root; the producer answers from its retained input and the verifier executes its own registered model afterward. Input-only withholding yields the existing DA default. The actual worker-result handoff retains the input accumulator, reorg clears it, and input waiting spends no replay attempts. The small PanelDA fixture covers pre/post-Final conviction, post-Final withholding, missing historical input copies and honest replay; it is V-fold, with signature/class admission callbacks and a supplied public accepted payload. It does not close full C2/C7/C8, maximum-context 34-session budgeting, permanently pruned FP commitments, all-family registration or largest-population response/inclusion bounds. Final evidence is recorded in the audit implementation record.

**Canonical FP input follow-up (2026-10-11):** ordinary V3/V4 canonical count/root faults can be proved from an authenticated public job and binding before a successful replay. Contradiction 13 appends proof tag 2 behind the dormant public-filer fence. Count faults are cheap; root checks share the processor/fold heavy budget. The node retains the authenticated job under a small input ticket before backend ceiling derivation/full replay memory, and leaves canonical Pending cases eligible to retry honest replay. Core tests include 2M input roots and both job versions; the accepted FP/controller/fold fixture uses synthetic shape-correct commitments to isolate input faults, all Panel Valid, pre/post-Final conviction, a correct-input twin and binding-nonresponse default. It is V-fold, not full C2/C7/C8 or maximum-model execution. Prefix/evaluation/tensor job semantics, permanently pruned accepted jobs, all-family registration, maximum-population loads and responsibility-window inclusion remain open. Final results and logs are in the audit implementation record.

**Block CPU follow-up (2026-10-11):** whole-prompt roots now survive acceptance, nested rehearsals and final fold in a bounded block scope, independent of the process FIFO. The new five-carrier V-fold fixture computes 262,143 ids once despite cross-thread FIFO eviction, with 357,140 combined transient mass. It does not prove real UTXO spends, mined inclusion or a fresh-node service run. Related regression: 86 unique PASS; normal binary check and shipping 361 pins pass without drift. C2/C7/C8 remain open.

**Carrier rent follow-up (2026-10-11):** the production signer pays burned rent plus relay carriage and the priority/common submitter selects funding at the current object's minimum. A relay-sized remembered residue no longer prevents selecting mature, unlocked and free funding for a heavy proof; cheap moves retain their old floor. Five new V-unit tests verify real signed spends/fee/change and pricing clocks, with 103 related regression tests (108 total PASS), normal binary check and shipping 361 pins without drift. Evidence pricing fixtures do not prove model honesty/conviction, a live UTXO scan, ordinary mature bond registration, a complete service tick or mined inclusion. These remain part of C2/C7/C8 and the full-family acceptance audit.

**Fresh public service DA follow-up (2026-10-11):** a newly registered non-genesis bond waits the preset 1,000 DAA maturity, with real registration/possession signatures and collateral. An empty database validates 2,051 public blocks before the actual verifier service starts, independently replays the registered floor model, discovers the all-Panel-Valid false-root attempt through public Consensus API reads and submits its own reservation and binding demand through the PQ mempool. Production templates and the actual block pipeline include both; at DAA 2236 the public reads confirm ProducerWithholding, a positive producer DaDefault sanction and reservation release. Normal service exit/join and owner release pass after replacing three gossip callbacks' strong service captures with Weak captures. This is one V-node case plus 69 regressions (70 unique PASS); the normal binary check and fresh shipping 361 pins pass without drift. Harness keys/premine, skip PoW, inert EVM and test-only dormant-fence arming are explicit; the shipping activation validator still refuses arming. This closes the fresh service/inclusion gap only for this floor canonical binding-withholding case. Disclosed arithmetic/binding conviction, all-family admission, socket RPC, network IBD/pruned import, restart/reorg and maximum-profile/population deadlines remain open. The wrong roots are synthetic commitments, not a localized model-fault injection. Full scope and raw evidence are in the audit implementation record.

| Scope | New evidence / correction | Still open |
| --- | --- | --- |
| F1–F4 generation binding | `74d59907f`: generative jobs require the exact positive `max_new_tokens` count. Short correct execution is rejected by the real block fold; the complete execution reaches Final. Kernel regression covers single, pipeline and segmented routes. | A future EOS/stop rule needs its own committed semantics; this is a dormant consensus meaning change. |
| F3–F4 public replay and resources | `f89431d82`: authenticated public positions, independent registered-model replay, canonical tensor bytes, simultaneous RAM/state accounting, bounded whole-value courts for withheld small values. Kernel and real-node regressions pass. | 64-element hiding tiles, cumulative scope, large-profile admission, measured peak/time and maximum-response deadlines. The scope report is not a reward gate. |
| F9–F16 common legacy machinery, tested on small fixtures | `a4a62ec21`: isolated faults cannot be skipped by a matching midpoint; public answers authenticate before caching/learning. The earlier real-node final batch has 7 PASS and a corrected reorg rerun has 1 PASS; core 10, fold 7, filer 5, A2U 3 PASS. The DA worker builds signed tag-158 step/checkpoint/CKW from verified base0-codec material. The outsider common filer now uses independent base0-codec replay, authenticated public tag-158 history, subtree step descent, tag-159 model-copy/matmul recomputes and CKW fused opening. Actual controller tests: LG14-B 7 V-fold PASS (pre/post-Final recompute, fused conviction, withheld frontier/CKW default); filer 8 and reader API adapter 5 V-unit PASS. | This is not a per-family F9–F16 result. Public binding job/count/output proofs and large canonical attempt-input proofs are wired. Authenticated PublicDA/canonical FP input now starts independent replay; a real FP fold reaches a gather conviction before/after Final and a post-Final DA default (V-fold, test admission/signature callback). Checkpoint/trace localization, maximum-context PanelDA input budgeting, extended FP job/state semantics and prefix-state bootstrap, fresh-node full-service reads and inclusion, permanently pruned answers remain. Held-court history pagination and selected authenticated chunk caching are wired and fixture-tested; full-service restart/reorg/inclusion is still required. Step agreement never clears an execution mismatch. The unsupported-codec linear fallback refuses more than 34 sessions. No maximum-profile RAM/time or deadline completion is established. |
| C2 / C7 for legacy | A node started after the claim replays the chain and serves matching op-204/206 documents via the RPC builders and JSON roundtrip after conviction. The paginated history reader is fixture-tested. The fresh public service DA case above adds actual service reads, ordinary mature outsider registration, PQ mempool/template/block inclusion, objective default and normal exit for one floor canonical binding-withholding claim. | Full-family disclosed-fault prosecution, a socket RPC drill, network IBD/pruned import and full-service restart/reorg. Numbered reacquisition covers supported Event/Held/LG14-B units, but permanently pruned FP job commitments and TIR/pipeline bootstrap still need retrieval; reader retries alone are not retrieval. |
| C6 / C8 under a coalition | Reserved sessions and direct-proof priority exist. Shared 64-live / 256-lifetime reservation caps were removed; a new bond uses its own retained-record budget and collateral. V-fold crosses both old thresholds and pursues public PanelDA input to conviction/default. | This is not full-family admission or a global throughput result. State/response load scales with owners; all-response/chunk inclusion, largest-population CPU/RAM and adaptive deadline bounds remain. |
| Condition 9, beacon source set | `b6a90a0d0` merges `7d8c31270`: the eligible source set is frozen at the epoch commitment. Engine stages 7 and production fold 22 PASS; shipping repin after the merge is clean. | The new fixture supplies Reference eligibility and checks the Chain read; it does not replace real-model production eligibility or the external soundness gate. |

ADR-0177 D7's registered-model possession premise is confirmed by the user. GAP-80 has also been decided: legacy is in scope;
the old "awaiting user decision" descriptions below are historical. Required family coverage is not waived.

## 2c. G14-B: maximum-profile budgets and deadlines (2026-10-11, lane G14-B, branch `g14/b-budgets-deadlines`)

The gate (`gate.rs`) bounds what one prosecution reads, opens and holds. G14-B asks the other half of the bar: **can the pursuit
FINISH** — every search round, every chunk of every demanded response, the filing — before the claim's liability horizon, against a
producer that answers every demand on the last DAA it may? The derivations are code (`misaka-palw-kernel/src/budget.rs`); nothing
below is measured. Verifier compute enters only through the OPV policy's declared budgets (cold 10, check 10, localize 2, court 3,
carrier 2, reorg 2 DAA — claims an external gate must measure).

**Carriage (derived, pinned against consensus).** A route chunk is ≤ 100,000 B, ≤ 16 an object, around a 16 KiB wrapper. The tag-113
lane (`KernelRouteChunkV1`) was already end-to-end on the integration line (carrier, per-bond groups with deposits, TTL ≤ 64 DAA,
reassembly and application in the fold). A testnet-12 block carries `max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR` = 125,000
transaction bytes: **one full chunk a DAA, network-wide**. A maximal `Respond` (one 1 MiB part) is **11 chunks**; the consensus
chunker's real count equals the kernel's count.

**What changed** (implemented, verified by the tests below; dormant — the whole kernel route sits behind the never-armed
`palw_probabilistic_constraints_v1`; nothing is armable):

| Rule | Before | Now |
|---|---|---|
| Segmented position demand deadline | `filed + court_deadline` (20) for any position size | `filed + court_deadline + (chunks − 1)`. `chunks` = Σ over the position's parts of the route chunks of each part's `Respond`, from the class's deterministic layout (`seg_da::position_part_bytes_v1`), never from what the producer sends: a maximal answer buys no time |
| Post-Final demand admission | `daa + court + grace ≤ liability_until` | `daa + window + grace ≤ liability_until` (the counted window) |
| A class's liability horizon | `policy.liability_daa` (200) for every claim | segmented (v4/v5) classes: `max(200, the worst-case pursuit's need)` (`ClassRowV1::liability_daa`, derived at registration and at every row rebuild, never stored); every other class unchanged |
| Registration of a segmented class | gate bounds + carrier fit | also `budget::class_pursuit_v1`: refused **by name** (`PURSUIT_SCHEDULE: …`) when the claim's worst-case lifetime (commitment → end of liability) exceeds 436 = `PALW_BOND_BUDGET_W_MAX_DAA_V1`. ECON derives `W ≥ L` (L the longest claim lifetime), so no admissible `W` can carry more. An admitted class needs the chosen `W` ≥ its lifetime. `W` is not decided here: `class_pursuit_with_ceiling_v1` takes any ceiling, and the tests sweep 280/300/358/400/436 |
| Inline (v1–v3, typed) demand deadline | 20 | **unchanged** (open item 3) |

No wire type, tag, table, RPC or registry number is added. No int-12 object changes (A-2: nothing to classify; the A2U suites
stay green). Ledger values change only on the dormant route: `deadline_daa` of chunked segmented demands and `liability_until` of
deep segmented classes.

**The worst-case schedule** (`budget::seg_pursuit_schedule_v1`):
- **Rounds.** G14-V's search, consumed as a parameter: `⌈log2 min(P, 1024)⌉` probes (each a full position demand) plus the
  court's round (q − 1 and q together). That is **11** at P ≥ 1,024.
- **Producer.** It stalls every answer to the deadline.
- **Lifecycle.** The claim reaches Final as early as the lifecycle allows:
  - pre-Final demands hold Final until served + grace;
  - between a served demand and Final no demand opens (the dead zone);
  - Final happens in its block's tick, so the first post-Final demand is in the next block.
- **Result** (interim clock, one-chunk positions): Final +78, last demand +271, filing +300. The need is **223 DAA of liability**,
  for a lifetime of 301. The policy's own 200 cannot carry the implemented search (10 rounds would fit 200). ECON's `L = 280` (the
  interim kernel lifetime, 50 + 20 + 10 + 200) cannot either.

**Per-profile table.** Each route at the largest context it admits. Source: `g14b_every_shipped_profile_…`, which asserts the
gate's verdict equals the schedule's at every ceiling.

| Profile | Position material (last position) | Response window | Liability need / lifetime (DAA) | Verdict (registration ceiling 436; needs `W ≥` lifetime) |
|---|---|---|---|---|
| wide128 sketch @8,192 (v4; the node fixture) | 1 part, 1 chunk | 20 | 223 / 301 | **fits**; needs W ≥ 301 |
| dense-MoE windowed (window 16…1,024) @8,192 (v4; K2S GAP-31's node fixture at 1,024) | 1 part, 5 chunks; filing 4 chunks (9 at window 512) | 24 | 289 / 347 (294 / 352) | **fits**; needs W ≥ 347 (352) |
| fat fixture @64 (v4, 1.28 MB position) | 2 parts, 11 + 3 chunks (position 1) | 33 | 229 / 296 (simulated, 7 rounds) | fits |
| Largest position that fits, 11 rounds, lifetime ceiling 358 / 400 / 436 | ≤ 6 / 10 / 13 chunks | 25 / 29 / 32 | — | boundary (one more chunk refuses); at 280 / 300 nothing with 11 rounds fits |
| mamba-370m @32,783 (v4) | 386 parts / 3,379 chunks | 3,398 | 34,034 | **refused by name** |
| qwen3.5-0.8b @32,783 (v4) | 1,768 parts / 19,441 chunks | 19,460 | 194,656 | **refused by name** |
| Huihui-Qwen3.5-9B @8,192 (v4) | 2,029 parts / 22,244 chunks; filing 1,030,590 B = 11 chunks | 22,263 | 222,686 | **refused by name**: its own window outlasts every value in the range |
| granite-3.1-1b, smollm2-1.7b @32,783 | — | — | — | refused by the v4 gate (verifier RAM) |
| 9B @262,144 / @2,097,152 | — | — | — | refused (retained state / `check_plan` positions) |
| v5 encoder (1 position) | any | any | 0: the one demand is pre-Final and holds Final | fits; Final can be held for a whole window (open item 4) |
| Legacy held 8k (LG14-A/B) | 4 input units + 5 descent rounds (2^40 leaves) + 2 = 11 sessions | 1,200 a session (`W_disclose = window_challenge()`) | sessions open until 4,200 after acceptance | **FAIL: at most 4 sessions fit** |
| Legacy held 2M | 1,024 input units + 7 = 1,031 sessions | — | — | **FAIL: past the 34-session budget itself** |
| Legacy at the applied short window (120) | — | 120 | — | 35 sessions would fit (the 34 budget and the 8k profile) |

**Tests and results** (run on this branch):

- **Kernel crate, every test:** **PASS — 285 passed, 0 failed** (7 ignored, pre-existing). It includes:
  - `budget::tests` (9): chunk and carrier boundaries exactly at 16 chunks and one byte over; round counts; the 223-DAA boundary
    (`fits(223)` holds, `fits(222)` does not); the ceiling sweep, where at each of 280/300/358/400/436 the largest chunk count
    fits and one more is refused; filing chunks; one-round fit and the inline squeeze; the legacy session boundary; population.
  - `k2_real_scale::g14b_*` (3, V-ref):
    - the profile table;
    - the worst-case pursuit driven through `apply_block`: 11 demands, each answered on its last DAA; Final +78; the last demand at
      +271, on the boundary of the class horizon; conviction at +300 ≤ +301 — exactly the derived times;
    - the maximal response (11 + 3 chunks) lands on its counted deadline and is served, and the same position withheld defaults at
      the counted deadline, not later.
- **consensus-core:** **PASS — 51 passed, 0 failed** (filters `g14b_`, `palw_kernel_route_v1`, `palw_legacy_public_filer_v1`, `palw_lifecycle_objects_v2`: the A2U pins and `every_int12_wire_type_is_unchanged_or_classified` included). It includes `g14b_a_maximal_respond_rides_the_chunk_lane_in_the_chunks_the_kernel_counts`: the mirrors are pinned; testnet-12 carries one full chunk a block; a 1 MiB-part `Respond` is cut into 11 tag-113 chunks, reassembled by `chunk_completion_v1` one chunk a block inside the TTL, and lands on its counted deadline's last DAA, which the old fixed window would have missed. It also includes the legacy FAIL derivation `g14b_a_stalling_producer_…`.
- **Node (V-node):** **PASS — 19 passed, 0 failed** (filters `g14_k2s`, `g14b_`, `t12_a2u`; 904 s). It includes `real_scale::g14b_the_worst_case_pursuit_completes_on_the_node_inside_the_class_horizon`: two outside bonds run 11 demand rounds through mempool → template → fold while the colluding producer answers on each deadline's last DAA. Final +78, last demand +271, filing +300, horizon +301, class liability 223 — the derived DAAs exactly. All 17 existing `g14_k2s_*` node tests still pass under the new rules, including K2S GAP-31's dense-MoE class held at 8,192 (lifetime 347 ≤ 436), and so do the three `t12_a2u_*` suites.

**Largest population (what is provable).** The chain carries one chunk a DAA, whoever's (`concurrent_pursuits_bound_v1`). The
counted windows assume the demanded material gets that capacity. N simultaneous demands of `c`-chunk positions need `N·c` DAA of
carriage.
- **Provable:** a pursuit of `k` chunks in total fits `⌊H / k⌋` concurrent copies in a span `H`. At the 436 lifetime ceiling a maximal
  pursuit is 12 × 13 + 1 = 157 chunks, so 2 copies fit. One-carrier pursuits (wide128: about 24 small objects) are bounded by block
  bytes, not chunks.
- **Not provable here (UNKNOWN):** inclusion order under a coalition that fills blocks with paid traffic. There is no reserved byte
  lane for prosecution objects; the prosecution reserve counts court runs, not bytes. Closing this needs a reserved route byte share
  or a fee-priority argument.

**Open items:**
1. **Every real-shape v4 profile is refused.** At one chunk a DAA, no `W` in the range carries a position of more than 13 chunks
   (about 1.3 MB) through 11 rounds. Real-scale v4 needs a different carriage. Candidates:
   - off-chain DA with a provider court (DA16);
   - probe demands that serve only part 0;
   - the four open sessions used as a 5-ary search: 5 rounds instead of 11 (G14-V's call).
2. **Legacy: FAIL, not wired.** Reserved DA sessions each last `W_disclose` = 1,200 DAA (the unshortened `window_challenge()`), one at
   a time, until the hard deadline at 4,200. A stalling producer therefore caps a reserved pursuit at 4 sessions, below the 8k
   profile's 11 and the 34-session budget.
   - Options:
     - reserved sessions on the applied window (120 DAA: 35 sessions fit; but SA-3 asks `W_disclose ≥ 2 ×` finality);
     - concurrent reserved sessions;
     - retention extended for an open pursuit.
   - This is a consensus decision on a dormant fence of the live route; it is not taken here.
   - Derivation and test: `palw_legacy_public_filer_v1::g14b_a_stalling_producer_holds_the_reserved_pursuit_to_four_sessions_on_the_t12_lattice`.
3. **Inline deadlines are not chunk-counted.** Under the 20-DAA base deadline, a maximal one-object inline response (16 chunks) leaves
   the producer 5 DAA to react, less than the interim disclose budget (8). Counting the gate's worst-case response bound would move
   about 20 existing kernel tests' deadlines by 1–2 DAA, because that bound is far above the fixtures' real responses. Left for a lane
   that owns those tests.
4. **Pre-Final holds.** A pre-Final demand holds Final until its counted window ends. Two upper-bound reads do not count the window
   extension or the class liability:
   - the OPV row's reported `hard_deadline_daa` (window + court + grace);
   - the consensus `palw_kernel_claim_horizon_bound_v1` (chunk TTL, provider lease).

   Final claims use the exact `liability_until`.
5. **Verifier times are declared, not measured** (the OPV budgets). A slower real verifier shrinks every boundary above. For legacy,
   the 34-session count is the cap, not a measured descent. Sessions to acquire the FP job or the prefix state are not counted.
6. **Not changed here:**
   - v4 `max_retained_state` / `max_verifier_ram` (left open by codex);
   - the BUDGET engine's liability hold `H_L` (280). A class liable past it is held by the kernel ledger's own reservation, but
     ECON's L/W relation must be revisited when `W` is chosen.

   G14-S's hiding tiles will change position material. `position_part_bytes_v1` follows the layout, so the windows and needs follow
   automatically.
## 2d. G14-S — the court scope as a gate, and DA16b option (a) (2026-10-11)

Branch `g14/s-scope-gates` (from `8173dae20`). **Overall G14 result unchanged: FAIL / incomplete.** Nothing is armed: every rule is
inside dormant K2-TIR-v4/v5 bytes or behind `palw_provider_court_v1` (consumer-injected, not in any root below it); no live id moves.
Status words: *implemented* / *verified* (a recorded run of this code) / *armable* (nothing).

**What is enforced, and where.**

| Gate | Rule | Code |
| --- | --- | --- |
| Reward (per claim), K2-TIR-v4/v5 | a class whose `seg_scope_report_v2(program, plan.max_positions).court_scope_complete()` is false takes no claim: `COURT_SCOPE_INCOMPLETE …` (registers, never reward-bearing); the report is cached in `ClassRowV1::court_scope` | `court_scope.rs:91` `class_court_scope_v1`, called `seg_ledger.rs:281` (`commit_segmented_claim`); report `seg_scope.rs:276`, gate `seg_scope.rs:248` |
| Reward (per claim), K2-TIR-v1/v2 on the OPV route | past `palw_provider_court_v1` + `palw_panel_free_v1`: a class with any model relation (clear budget < ∞) takes no claim — its responses serve activations in the clear and the route has no commitment-only form | `court_scope.rs:91`, called `ledger.rs:1882` (`commit_claim`) |
| DA (what may be demanded) | ≤ 16 distinct units per (claim, requester bond) — demands, joins and segmented filings (`COURT_SCOPE_UNITS`); OPV v1/v2: a new position past the model's clear budget (`court_position_disclosure_v1`, 0 with an elementwise relation) is refused (`COURT_SCOPE_CLEAR_BUDGET`) | `court_scope.rs:131`, called `ledger.rs:2449` (v1/v2) and `seg_ledger.rs:378` (v4/v5); table 28 |
| DA (what may be served) | K2-TIR-v4/v5: no model-dependent value in the clear — withheld = node opening only, hiding = leaf hashes only (`ChunkBodyV1::Hidden = 3`); classify refuses any other form | `seg_da.rs:143` (layout), `seg_da.rs:361` (classify) |
| Court (what may be opened / recomputed) | a whole-value court only for a withheld node, a hiding-leaf court only for a hiding node, a hiding decode only for hiding logits; each operand in its source's form (a hiding value never by v3 leaves) | `element.rs:807`, `:829`, `:857`, operands `element.rs:644` |
| Filings and court runs | ≤ 4 dismissed filings per (claim, accuser): the 5th is refused free, before the court runs (`COURT_SCOPE_FILINGS`); a segmented filing's position counts in its filer's 16 units | `court_scope.rs:170`, called `ledger.rs:2166`; dismissal counted `ledger.rs:2190` |
| Consumer | `CourtScopePolicyV1 { activation_daa }` injected at every load past `palw_provider_court_v1` | `consensus/core/src/palw_kernel_route_fold_v1.rs:229`; A2U `StateEncoding` row (table 28) |

All bounds are per requester or per class, so no producer, seat or front-runner can spend an honest prosecutor's allowance (one
prosecution needs ≤ 12 units and one convicting filing).

**Option (a), implemented + verified at the ledger level (V-ref).** `hide.rs`: flat 64-element tiles of the row-major order, ragged
tail merged (64–127 elements a tile; a value < 64 elements is one leaf), `C_hide` over its own domains. A model-dependent value is
withheld (option (c)) or hiding — never clear (`clear_masked = 0` for every program). Hiding-leaf court recomputes ONE tile from
operands the filer supplies from its own re-execution (they authenticate only if equal to the producer's) and the filer's copy of the
params; price `element.rs:1899` (first/last tile, ≤ 127 `MatMul` columns: the ≈ 64× column cost), modes-aware plan price
`element.rs:1807`. Option (c) finished: whole-value courts accept hiding operands by their tiles; withheld/hiding values are empty
placeholders in assembled positions.

**New bounds (G14-B owns the derivation):**
- `SEG_HIDING_COURT_MAX_BYTES_V1 = 1,572,864 B` (one carrier less the 4,096-B segmented header and the envelope), work 2^36; a hiding
  court above it is *unbounded* (not in the plan's worst court; the class is incomplete).
- Leaf-hash DA: 64 B a hiding tile (`position_material_v1`).
- `verifier_ram_bound_v4` (`gate.rs:507`): hiding values are kept with the withheld ones; public positions no longer hold kept values,
  so the separate `withheld × local` term is dropped. 9B-8k: **59,958,548,592 B** (was 67,191,831,824 B).
- 9B-8k worst court **1,008,494 B** (a hiding FFN-down `MatMul` tile); position material **551,788,288 B in 548 parts**.
- Court scope: 16 units / 4 dismissals per requester per claim (INTERIM).

**9B-8k scope (derivation over the shipped program, no 9B run):** 176 clear, 11,150 withheld, 1,997 hiding occurrences; **48
unbounded** in four relations — linear-attention state `Mul [32,128,128]` 2.10 MB; softmax `ReduceMax` / `ReduceSum` over 8,192
scores 2.52 MB each; attention `P·V` `MatMul [4,4,256]` 11.29 MB. **The 9B-8k class is `COURT_SCOPE_INCOMPLETE`: not reward-bearing**
until those courts are filed in chunks (tag 113, G14-B) or lowered history-chunked.

**Tests (run `g14s-r1`/`r2`, this code):**
- kernel crate, all targets: **283 PASS, 0 FAIL, 7 ignored** (the 7 are pre-existing: F-C4R4-17 on the Panel-licensed world — reason
  updated — and the six exhaustive PESG-B runs, not run: `pesg_b_v4_route_a_samples_and_route_b_reexecution` uses the mode-aware commitments and is UNKNOWN). New: `k2_real_scale::g14s_a_hiding_tile_lie_is_convicted_pre_and_post_final_and_no_honest_tile_is`,
  `g14s_a_decode_lie_on_hiding_logits_is_convicted_from_two_tiles`, `g14s_wide128_d16_owes_no_short_leaf_and_a_lie_is_convicted_whole`,
  `g14s_dense_moe_rows_of_4_windows_hide_flat_and_an_unbounded_scope_takes_no_claim` (window 4: all withheld; 2^18 @16: 26 hiding,
  complete, a hiding-window lie convicted; @16,384: incomplete → claim refused `COURT_SCOPE_INCOMPLETE`),
  `g14s_9b_8k_hiding_sizing_is_derived_from_the_shapes`; `g14s_court_scope::g14s_f_c4r4_17_on_the_opv_route_the_clear_budget_refuses_and_the_class_takes_no_new_claim`,
  `g14s_a_filer_past_its_cumulative_scope_is_refused_and_an_honest_prosecutor_still_convicts`; `hide::tests` (2), `element::tests`
  (every hiding value's tiles dismissed honest, a lie in every value convicted by its mode's court).
- consensus-core (`palw_lifecycle_objects_v2`, `palw_court_scope`, `palw_kernel_route`, A2U table): **44 PASS**.
- node (`g14_kernel_route_e2e::real_scale`, `::da16`, `t12_a2u`): **27 PASS** after one fix (run `g14s-r1`: 26 PASS, 1 FAIL — the GAP-31 held test's streaming producer committed every value v3; with mode-aware commitments it PASSES in `g14s-r2`: the continuity lie in the 1,024-row k window at position 4,096 is localized and convicted on the real node, the verifier reading 48 B of clear values back from the blocks). The window at 1,024 rows is a hiding value by the scope derivation (its whole-value court exceeds 1 MiB); the test asserts the localization, not the court kind.
- SDK `k2s_encoder_court::k2s_v5_encoders_and_heads…` (hiding values judged by tiles): **1 PASS**.

**Open (honest):**
1. **Panel-licensed K2-TIR-v1/v2 classes** still serve model-dependent values in the clear; F-C4R4-17 stays an ignored FAIL on that
   world. Enforcing the clear budget at `palw_provider_court_v1` alone would refuse every DA16 Panel-licensed claim — **a Lead/user
   decision** (Level 2 for v1/v2 needs a commitment-only response form and courts on that route).
2. **9B-8k is incomplete** (four relation types above one carrier) — chunked filing or history-chunked lowering.
3. Hiding at node level: one V-node case (the GAP-31 held class above: a hiding k window convicted through the real chain path); every other hiding case is V-ref. Fresh-node / pruned-import / reorg runs with hiding values beyond that are not separately run.
4. The consensus reward gate (`palw_reward_gate_v1`, OPV eligibility — codex's) does not read the scope; the kernel's per-claim
   refusal drops such a claim at the fold, so no reward is reachable, but eligibility still reports the class.
5. Pipelines (K2-TIR-v3) and typed roots are not judged by `class_court_scope_v1` (consensus classifies their units, DA16b §7.3).
6. Values under 64 elements (5,744 occurrences in 9B, 378 in dense-MoE): their commitment is one leaf — a residual brute-force
   exposure of tiny low-entropy values, as before.
7. Hiding pricing runs at registration and class-row rebuild (the 9B gate + report ≈ tens of seconds in debug); not measured in release.
8. A refused filing is dropped by the fold, but its carrier bytes may still be mined; a filer only ever discloses its own copy.
## 2e. Lane G14-V — every covered violation through the real service (2026-10-11, branch `g14/v-all-violations`)

Base: integration `8173dae20` (codex LG14-A public filer merged). Overall G14 verdict stays **FAIL / incomplete**; nothing is
armed (every fence used below is test-armed in the harness with its shipping validation still refusing it, asserted in-test).
Levels: **V-node(svc)** = kaspad's actual `PalwPanelService` (the public verifier/filer service) on a node whose database was
created AFTER the claim and validated every public block; its own replay of the registered floor model; its own reservation,
demands and reporter filings through the actual PQ mempool, the production template and the real block fold; outcome read back
from consensus. **V-node(harness)** = the G14C canonical harness: a post-genesis bond on an IBD / pruned-import node, verifier built
from the RPC op builders — there is **no kaspad kernel-route verifier service**, so no kernel family reaches V-node(svc).

### 2e.1 What changed (implemented)

* **Authenticated subtree descent** (`kaspa_consensus_core::palw_auth_descent_v1`, node policy, consensus-inert, no wire type).
  Every authenticated opening (a `StepRange` answer, a single-leaf opening) determines its path and every sibling it carries at a
  known `(level, index)`; the descent compares only such authenticated SUBTREE ROOTS with the verifier's own nodes from the root
  down, always into the leftmost differing child, and reads the next range at the focus subtree's left edge. A matching subtree root
  covers its whole prefix (collision resistance) — a lone matching midpoint leaf is never trusted (codex's `a4a62ec21` rule kept:
  the old `PalwLegacyBisectV1` is untouched). **Bound:** reads of ≤ `2^r` leaves on a tree of height `h` take at most `h − r + 1`
  reads (`r = 10` for `StepRange`): binding read + reads + `StepLeaf` terminal ≤ `h − 7` sessions — **33 of the 34-session
  reservation at the 2^40 ladder top; fits up to 2^41 leaves**; past that `Unjudged`, never a guess.
* **The production filer uses it** (`kaspad/src/palw_fraud_filer.rs`): the codec-independent fallback (every case below
  `palw_legacy_held_da_v2`, or a non-base0 replay) is now `palw_fraud_filer_subtree_step_v1` instead of the linear scan that
  refused every claim past 32,768 leaves. The own tree is built once per replay (retained at `max(10, h − 20)`, folded below on
  demand from the verifier's own `held_step_range_answer_v1`). The history reader caches only the binding read and the ONE unit the
  case's descent reads now (`rcore_wanted`, backfilled like a selected LG14-B unit), learned into the case's authenticated tree and
  dropped — a stranger's other range demands never enter the cache.
* **Past an agreeing step tree** (both the LG14-B path and the fallback): the old `Err("checkpoint/trace/job localization is
  required")` is replaced by `PastStepTree` → `palw_fraud_filer_past_step_v1`. From the binding read's event and the own run: a
  committed logits lane the (agreeing) head did not output → `LogitsNotStepOutput` (12); rows all agreeing with a token that is not
  the row's selection → `ForgedOutput` (8); each held to the fold's own predicate, filed through the reporter door (R-3
  commit/reveal). Flat classes need no further session; tiled classes run an authenticated rows-tree then tile-tree descent of
  `Event` reads (≤ `log2(rows) + log2(tiles) + 2` reads). An honest claim yields nothing (V-fold below). A difference only in the
  checkpoint leg / activation leg / job with step and trace agreeing is **reported, not guessed** (job/legs/prompt faults are the
  direct door's, which runs first from the same binding).

### 2e.2 Per family × violation (verdicts at this head)

| Family / route | Violation (injected through the producer's own commit path) | Level | Outcome on chain | Evidence |
|---|---|---|---|---|
| F9–F16 legacy floor (BASE-0 attempt, 5,560 leaves, flat logits), common filer, fallback descent | MatMul element, isolated, honest values after (leaf 1853) | **V-node(svc)** pre-Final | CourtFraud, 4 demands (binding, 2 ranges, `StepLeaf`) | `g14v_step_lies_of_every_floor_op_kind_…` |
| 〃 | MatMul element at the op's first leaf (12) | **V-node(svc)** | CourtFraud, 3 demands | 〃 |
| 〃 | Embedding gather (leaf 0) | **V-node(svc)** | CourtFraud | 〃 |
| 〃 | Normalization / rounding (RmsNorm, leaf 1904) | **V-node(svc)** | CourtFraud | 〃 |
| 〃 | Nonlinearity (SiLU, leaf 2008) | **V-node(svc)** | CourtFraud | 〃 |
| 〃 | Honest twin | **V-node(svc)** | nothing filed, `Final` | 〃 |
| 〃 | Logits lane bent over an honest step tree (R1 residual) | **V-node(svc)** pre-Final | kind 4 `LogitsNotStepOutput` (past-step) | `g14v_trace_token_output_input_and_state_binding_lies_…` |
| 〃 | Forged decode token (re-committed, `NotSelected`) | **V-node(svc)** | kind 4 `ForgedOutput` (past-step) | 〃 |
| 〃 | Output root replaced | **V-node(svc)** | kind 4 `OutputMismatch` (direct door) | 〃 |
| 〃 | Input / prompt: relabelled prompt (another anchor's ids) | **V-node(svc)** | kind 4 via the direct door | 〃 |
| 〃 | Input / prompt: garbage prompt root | **V-node(svc)** | kind 4 via the direct door | 〃 |
| 〃 | State-leg identity: activation leg replaced (J6), checkpoint interval replaced (J7) | **V-node(svc)** | **DA default** (`ProducerWithholding`, producer sanctioned): the H-1 gate refuses the producer's own binding disclosure (it answers another job), so no conviction is reachable — the default is the objective outcome | 〃 |
| 〃 | MatMul element and logits lane **after Final** (node + service started after Final) | **V-node(svc)** | CourtFraud / kind 4 | `g14v_a_step_lie_and_a_logits_lie_are_convicted_after_final` |
| 〃 LG14-B armed (tag 157/158 frontier descent, tag 159 recompute) | MatMul element, embedding gather | **V-node(svc)** pre-Final | CourtFraud via `LegacyLeafRecomputedV2` | `g14v_lg14b_frontier_descent_convicts_step_lies_through_the_service` |
| 〃 | Honest claim forced into a mismatch verdict: step tree agrees → past-step → nothing provable, nothing filed | V-fold | — | `lg14b_an_honest_claim_survives_a_malicious_outsider` (updated) |
| 〃 | A lie anywhere in a claim past 32,768 leaves (the old Unjudged region) | **V-unit only** | located within ≤ 33 sessions | core `palw_auth_descent_v1::tests::*` (exhaustive n ≤ 70 × every position × r ∈ {1, 4, 1024}; disjoint multi-lies; 2^20 + 3 leaves with retained own tree), `the_subtree_engine_localizes_a_lie_anywhere_within_the_reservation` (70,001 leaves) |
| 〃 | Tiled logits (rows/tile descent glue) | implemented; **not verified** | — | the generic single-leaf descent is V-unit; no tiled legacy claim was run |
| 〃 | Routing (MoE) | **open** — the floor has no router; no legacy MoE class was run | | |
| 〃 | Checkpoint **state values** (KV chunk ≠ cache-write rows) with step and trace agreeing | **open** — not wired in the public filer (the floor commits 0 checkpoints); the checkpoint court and `StateChunk` units exist; the descent module applies to the checkpoint tree but the node terminal is not connected | | |
| F1 K2-TIR v1/v2 canonical class (`wide128`), Panel (every seat covering) and OPV | Embedding gather element, wide I128 MatMul accumulator element, clamp (range / saturating nonlinearity) — each pre- and post-Final, pruned-import node, commitments only | **V-node(harness)** | convicted, bounty to the participant, seats not charged | `canonical::g14_canonical_g14v_gather_matmul_and_clamp_lies_pre_and_post_final` (12 cases) |
| 〃 | quantization, decode, garbage; boundary / copy / borrowed refused at inclusion | V-node(harness) (G14C, not re-run here) | | §2a |
| 〃 | routing / history | refused by carrier fit (§2a) — **open** for conviction | | |
| F3 K2-TIR-v4 / F4 v5 (KS, merged) | element lie, withheld, input / borrowed / garbage, after Final, continuity across segments (8,192 held), v5 encoder lies | V-node(harness), re-run at this head: `g14_k2s_` **15 PASS** | | |
| F6 typed roots | memory / retrieval / composite (T02–T05) | V-node(harness), re-run: `r4x_` **11 PASS, 1 FAIL** — `r4x_weights_only_spec_is_byte_for_byte_the_legacy_registration_and_the_unarmed_route_is_unchanged` (the kernel-route mempool gate refuses a typed-root carrier below `palw_typed_roots_v1`; this branch does not touch that gate — integration-owned, likely the A-2 gate change `824e6da15`; UNKNOWN whether it fails on the base, not re-run there) | | |
| F2 pipelines, F5 fused/private | unchanged (§2a): F2 has no node claim; F5 never registers | open / by construction | | |

### 2e.3 Open, stated (requirements for other owners in brackets)

* No kaspad **kernel-route verifier service** exists: every kernel family (F1, F3, F4, F6) stops at V-node(harness). Building it
  (RPC-read verifier loop + filing through the service's mempool) is the next G14-V item.
* Large-claim localization is V-unit only; the floor's 5,560 leaves sit inside the old scan's reach. A V-node(svc) run needs a
  registered legacy class with > 32,768 leaves on the harness.
* Checkpoint-state localization (checkpoint tree → `StateChunk` → checkpoint court) and the tiled `ForgedOutputTiled` proof are
  not wired; tiled trace glue unverified.
* Activation-leg / checkpoint-interval lies end in the DA default after ~1,200 DAA (the disclose window), not a conviction — a
  public-binding-free conviction route for J6/J7 would need the binding to be publishable under the H-1 gate (consensus review).
* Session budgets: this lane did not change `gate.rs` or the 34-session constant. The subtree descent's bound (`h − 7`) fits it to
  2^41 leaves; [G14-B: confirm the maximum legacy step ladder ≤ 2^41 when deriving the budget].
* Restart / reorg / IBD resume of a mid-descent case: the descent state is a cache rebuilt from chain answers (reset on a branch
  change like LG14-B's); not exercised [G14-R].

## 2f. G14-R: recovery and the external path (2026-10-11, lane G14-R, branch `g14/r-recovery-resume`)

The bar's operational half: a public prosecution must **resume** and reach the same objective outcome when its verifier restarts,
its node reorgs / IBDs / imports a pruning snapshot, or its RPC connection drops. Base: integration `1ad909f76` (contains the
virtual-stall fix `9fbdb3f83` and G14-B). Overall G14 stays **FAIL / incomplete**; nothing is armed or armable by this lane.

**Design: rebuild, never persist.** Neither route keeps pursuit state a restart can lose or double:
- **Kernel route** — new `misaka-palw-remote::pursuit` (node policy, consensus-inert): `plan_kernel_pursuit_v1(route rows, pursuit,
  signed filing)` returns the step the chain still needs (`Seal`, `AwaitSeal`, `FileWhole`, `FileChunks{missing}`, `Demand`,
  `AwaitAnswer`, `Recheck`, `Done(Convicted|Defaulted)`, `TooLate`, `WouldBeDismissed`, `Refused`, `Stranded`, `NoClaim`) from op 211's
  rows alone (the ledger, the proof seals, the demand rows, the tag-113 chunk groups of aux table 41). Before it returns `File` or
  `Demand` it applies the object to a clone of the rebuilt ledger at the next DAA (after the signer's collateral sync the fold
  performs), so a filing the court would dismiss or refuse is reported, never sent. Nothing is cached across tips, so a reorg has
  nothing to invalidate. `kernel_pursuit_resume_bound_v1` is G14-B's tolerated downtime: the claim's horizon (pre-Final: window end;
  else `liability_until`; live claims count the policy liability, a class extension only lengthens it) less the remaining carriage
  (one chunk a DAA) and one fold.
- **Carriers must be deterministic.** A chunked filing's group id hashes the SIGNED object: a verifier that re-signs with fresh
  randomness after a restart strands its half-built group (deposit forfeited at TTL) and opens a second (a second deposit, one of two
  per-bond slots). Pursuit carriers are signed with FIPS 204 deterministic ML-DSA (`PURSUIT_DETERMINISTIC_RND_V1 = 0^32`); a foreign
  group of the opener's is reported `Stranded`, never duplicated.
- **Kernel ledger fix (dormant route, `misaka-palw-kernel::ledger`):** an IDENTICAL proof re-seal is now a no-op. Before, the second
  copy of the same `SealProof` (a restart with the seal in flight, or the losing branch's carrier merged back after a reorg) replaced
  the row and restarted its clock — delaying the verifier's own filing and handing bounty precedence to a copyist who sealed the same
  bytes in between. A re-seal of OTHER bytes still replaces and restarts (unchanged). No wire, tag, table or registry change; root
  changes only on the dormant kernel route in that case.
- **Legacy V2 route** — the common filer's book (`kaspad::palw_fraud_filer`) is already a cache rebuilt from the tip and the accepted
  blocks, with codex's branch invalidation (walk reset clears answers, frontiers, CKW, binding, localizer, `sent`, input bootstrap,
  and the filer's court-queue items; job/acceptance changes restart the case; stale blocking replays are drained, never applied). The
  reporter door is persisted and replans from rows on reorg. G14-R found no cache in the pursuit path left without invalidation and
  changed none of that logic; it adds the service-level and node-level evidence below.

**DAG reorg semantics (observed, recorded).** Under the shipped `palw_reorg_strict_economic_win` an all-economic tie reorgs only when
the incumbent side spans ≤ 2 DAA, so the reorgable step is the last carrier (carrying + folding block). And in a DAG the losing
branch is not discarded: the next block merges it, so its non-conflicting pursuit carrier is re-accepted. Measured on the kernel route:
right after the reorg the tip lacks chunk 0 and the plan re-derives `[0, 1, 2]`; one block later the merge has re-added it and the
plan re-derives `[1, 2]`. Funding must come from the virtual UTXO set (the losing branch's carrier sits in the virtual's merge set),
and a re-sent duplicate (identical seal: no-op; demand: free join; reservation: one per reserver; chunk part: dropped) costs no
deposit.

**Recovery point × route** (V-node = canonical real-node harness: mempool → template → block fold → persisted tip → read ops; V-svc =
the actual `PalwPanelService`; socket = real gRPC server + wRPC proxy on 127.0.0.1 driven by `KaspaRpcClient` / `GrpcClient`):

| Recovery point | K2-TIR v1/v2 (Panel / OPV) | K2-TIR-v4 segmented (OPV) | Legacy V2 public filer |
|---|---|---|---|
| Verifier restart | **V-node PASS** — restarted after the seal and after each chunk of a 3-chunk filing: no re-seal (an in-flight duplicate seal keeps its DAA), each chunk once, one group, conviction before Final, inside the resume bound | (covered by the node restart, below: a new process recomputes its own replay) | **V-svc PASS** — the actual service stopped after its reservation and restarted (same state dir): no second reservation, demand once |
| Node restart (same DB) | **V-node PASS** — between chunk 0 and 1; the half-built group off disk | **V-node PASS** — between the producer's `Respond` parts (partial answer set): open demands off disk, none re-filed | V-node (existing `lg14a_the_pursuit_survives_a_node_restart`) |
| Reorg | **V-node PASS** — last chunk reorged out → plan `[0,1,2]`; merge re-adds → `[1,2]`; one conviction, no deposit held; a DAG-IBD node agrees | **V-node PASS** — the first demand reorged out → re-derived (same position); the search continues | **V-node PASS** — post-Final reservation reorged out; a NEW verifier re-derives from the tip and convicts; ≤ 1 reservation of ours in Z's chain blocks; deposit refunded, collateral net zero; no unit demanded twice |
| Full IBD | **V-node PASS** — node started after the seal and a partial filing finishes the same group; also after a SERVED demand (re-check reads the served position off the rows, convicts; one demand) | **V-node PASS** — a DAG-IBD node (merged blocks included) started mid-search finishes it | **V-node PASS** — node started mid-descent (reservation, binding, ranges): new verifier re-learns binding/localizer from authenticated answers, convicts; one reservation, no unit twice. **V-svc PASS** — after the demand the node is replaced by an empty DB that re-validates every block; a third service reaches the DA default |
| Pruned import | **V-node PASS** (OPV) — after the demand: `AwaitAnswer` with the demand's own deadline, demanded once, the chain defaults it | **V-node PASS** — after a partial answer set: the search resumes on the importer and convicts | **V-node PASS** — after reservation + binding: the dispute record rides the carriage, conviction on the importer (harness keeps bodies ≤ P; answers needed are > P; bodies lost to real pruning = codex's reacquisition) |
| Real socket RPC, disconnect/reconnect | **socket PASS** — every read (op 211 paged, 210, 212, 231, 202) and every carrier (`submitTransaction` → the node's mempool check → template) over borsh wRPC; the proxy restarted twice (connection dropped) and the node behind it swapped for an IBD node mid-filing; JSON wRPC and gRPC serve the same op 210 document | (same transport; not separately driven) | **socket PASS** — ops 204/205/206 and 202 over borsh wRPC follow the pursuit across a proxy restart and a node swap (IBD node; a new verifier convicts); JSON frames the same op 204 document |
| Deadline (G14-B) | **V-node PASS** — down until the pre-Final resume bound: convicts before Final; down past Final: convicts in liability; down past liability: `TooLate`, nothing sent, nothing charged | bound function shared; not separately driven | legacy: the producer's session deadline is the chain's (a default needs no verifier action); the 34-session / hard-deadline limits are G14-B's FAIL rows, unchanged |

**What "socket" is and is not.** `consensus/src/pipeline/virtual_processor/tests/g14r_socket_rpc.rs` fronts a test node with an
`RpcApi` whose PALW ops run exactly the handlers' builders (`kaspa_rpc_core::convert::{palw_kernel, palw_legacy}`,
`palw_state_proof_v1`); the shipping `kaspa_grpc_server::Adaptor`, the `kaspa_wrpc_server` proxy (`grpc_proxy_address` mode, borsh /
JSON) and the real clients carry them. It is not `RpcCoreService` (that needs a p2p flow context, mining manager and index processor,
and a network passing `validate_palw_v2`, which refuses these fences), and it is not a separate process.

**Tests and results (2026-10-11, this branch; `buildslot.sh … --offline`, line-tables profile):**
- `misaka-palw-kernel --test g14r_reseal`: 1 PASS (identical re-seal keeps the first DAA; other bytes replace; files and convicts).
- `misaka-palw-remote --lib pursuit`: 2 PASS (V-unit).
- `kaspa-consensus --lib g14r_` (skip socket): **12 PASS** — `g14_kernel_route_e2e::recovery` 7, `real_scale::g14r_seg` 2,
  `lg14a_legacy_filer_e2e::g14r_resume` 3. Socket: **2 PASS** (`socket_rpc::g14r_kernel_v2_…`, `g14r_resume::g14r_lg14a_the_dispute_reads_…`).
- Regression `kaspa-consensus --lib -- g14_canonical lg14a_ g14_k2s_ t12_a2u g14_kernel_route_replay g14_kernel_route_survives g14b_`:
  **60 PASS**, 0 FAIL (A2U green).
- `kaspad --lib` (actual service): **`resume::g14r_the_actual_public_filer_service_resumes_after_a_restart_and_on_a_fresh_database_without_duplicates`
  PASS** (single-threaded run; service 1 reserved and stopped at DAA 1037; ten blocks with no service; service 2 rebuilt its case and
  demanded with no second reservation; the node was replaced at DAA 1054 by an empty database that re-validated every block; service 3
  reached the objective DA default — one reservation, no unit demanded twice, the default collected, the reservation released).
  `palw_fraud_filer::tests` 16 PASS. The pre-existing `fresh_public_bond_service_carries_reserved_da_to_objective_default` (codex) **FAILED
  on this host twice** (with 4 test threads and alone) at its own 360-second wall-clock limit, the reservation and demand on chain and the
  session deadline (≈1,200 DAA away) not yet reached: a host-speed limit of that test's loop (20 heartbeats an iteration), not a logic
  failure seen here — UNKNOWN until re-run on a quieter host; this lane did not change it.
- `misaka-palw-kernel` (all targets): **286 PASS**, 0 FAIL after the re-seal change.

**Live drill: not run — infeasible as specified, for these exact reasons.** (1) No drill flag arms `palw_legacy_public_filer_v1`
(nor `palw_legacy_held_da_v2`), and the kernel route's `palw_probabilistic_constraints_v1` is refused by every drill flag by design
(canonical harness doc) — a drill-only arming flag would be new arming code; (2) a pursuit needs a false claim that every seat signs
`Valid`: the drill has producer tamper flags but no seat-collusion knob, so honest drill seats refute the lie through their own duty
and the public filer has nothing left to carry (or races the seat court); (3) a non-genesis public bond needs the preset 1,000-DAA
maturity on the live cadence (genesis cards are exempt, but then every outsider is a drill card); (4) the kernel route
needs a registered K2 class on the drill, which is codex's onboarding path. The socket test above is this lane's external-path
evidence; a drill pursuit needs (1)+(2) decided first.

**Open (G14-R).** Kernel route has no node service yet — the planner is the library a remote/in-node verifier runs; wiring it into
`PalwPanelService` (tick, funding from the virtual UTXO set, deterministic signer) is not done. Legacy service-level reorg is V-unit
(codex) + V-node (engine), not V-svc. v4 deadline resume and v4 over the socket are not separately driven. Permanently pruned bodies
(reacquisition) remain codex's. The verifier's synced collateral is read from its own bond (the tests read the node's tip; a remote
verifier reads op 202's bond row).

## 2g. G14-K: the kernel route's public verifier inside kaspad's service (2026-10-11, lane G14-K, branch `g14/k-kernel-service`)

Base: integration `3076077f9` (G14-R merged). Overall G14 stays **FAIL / incomplete**. Nothing is armed or armable: the kernel route
sits behind `palw_probabilistic_constraints_v1` (refused by validation at every height, asserted in the tests); no consensus code
(only two new test modules under `consensus/src/pipeline/virtual_processor/tests/`), wire type, tag, table, RPC op or registry
number changed; no testnet-12 id moves. The court-lane round `u32::MAX − 13` is node-local bookkeeping, not a registry number. Levels as in §2e: **V-node(svc)** = kaspad's
actual `PalwPanelService` on a node whose database was created after the claims and validated every public block, its own replay of
the registered model, its carriers through the PQ mempool, the production template and the real block fold, outcomes read back from
consensus; **V-node(harness)** = the canonical consensus harness (mempool → template → fold), driven here through EXACTLY the
library functions the service calls.

**What was built (implemented).**
- `kaspad/src/palw_kernel_verifier.rs` (new, a `PalwPanelService` lane after the LG14-A filer): once a DAA it reads the route through
  the node's read API (`ConsensusApi::palw_kernel_route_v1`, op 211's source), rebuilds the ledger as the fold will see it
  (`pursuit::kernel_consumer_ledger_v1`: G14-S's court scope past `palw_provider_court_v1` and the verifier-pay terms injected, so the
  16-unit / clear-budget / 4-dismissal limits are in every dry run), selects every open claim (live, or Final inside liability) not
  produced by its own bond whose class's REGISTERED commitments one of its models opens to, re-executes, and drives
  `misaka_palw_remote::pursuit` (plan → sends). Carriers are signed deterministically (the service's signer is FIPS 204 `rnd = 0^32`
  = `PURSUIT_DETERMINISTIC_RND_V1`) and queued on the court lane, which funds each from the virtual UTXO set at the object's own
  minimum (`ensure_carrier_funding_v2`: mature, unlocked, free, chained through its change) and submits it to the mempool. Items are
  dated by the planner's `by_daa` (pre-Final deadline, else the liability horizon), so the lane's EDF carries the pursuit nearest its
  horizon first. Nothing persists: an object on chain is never re-planned, one in this node's mempool (a restart) is never queued twice,
  one queued is re-offered only after 6 DAA.
- The model: `--palw-kernel-model=<file>` (repeatable; `PalwPanelConfig::kernel_models`), a `PALWKM01` file (Borsh tensors). ADR-0177
  D7: the verifier holds the registered model; it is used for a class only when it opens to the class's registered
  `ParamCommitmentsV1` root (v1/v2 `of`, v4/v5 `of_v3`; a memory class's rule program). A model that does not, or a class it cannot
  check, is logged once and never filed against.
- `misaka-palw-remote::pursuit` (extended, nothing rewritten): `plan_kernel_pursuit_on_v1` (the planner over a caller-rebuilt ledger;
  `plan_kernel_pursuit_v1` now delegates), `kernel_consumer_ledger_v1`, `kernel_check_program_v1` (v1/v2 and pipelines:
  `OutsiderV1::check_computation`, no producer value read, nothing demanded of an honest producer), `kernel_check_segmented_v1` (own
  replay + adaptive search against block-served positions), `kernel_check_typed_v1` (typed memory, `SpecOutsiderV1::check_computation`),
  `kernel_class_model_root_v1`, `kernel_reassemble_objects_v1` (generic `ObjectChunk` and tag-113 lanes, any order, once),
  `kernel_tiled_prompt_v1` (prompt tiles authenticated against the job's root), `kernel_segmented_tokens_v1`, `KernelBlockMaterialV1`
  (`Respond` parts → assembled, authenticated positions + hiding leaves), `kernel_step_sends_v1`, `KERNEL_SERVICE_CHUNK_CAP_V1`.

**How the kaspad test registers a kernel class without a test hook.** kaspad's tests cannot reach consensus's `cfg(test)` seams
(attestation, OPV eligibility), and none was added. The F1 class goes through the PRODUCTION onboarding path: the V2 IR class
(`ClassRegisteredTirV1`, the SDK builder over `palw_v2_registration_terms`), `ArtifactBoundV1` (bonded), maturity (40 DAA), then
`RegisterClass` on the kernel route over the root the matured binding attests (`onboarding_attested_roots_v1`). That works for a
Panel-licensed K2-TIR v1/v2 class. A v4/v5 or typed class needs OPV, whose eligibility is DERIVED (conformance passed under a
Panel-independent beacon — codex's lane); that is why those families are V-node(harness) below.

**Per family × case (this branch).**

| Family | Case | Level | Outcome | Test |
|---|---|---|---|---|
| F1 K2-TIR-v2 (`wide128`, Panel-licensed, every interim seat signing a passing receipt) | embedding gather element, wide I128 MatMul accumulator element, clamp result — **pre-Final** | **V-node(svc)** | convicted before Final by the service (one seal, one whole filing each); producer's real bond slashed `claim_collateral`; no seat charged | `kernel::g14k_the_kernel_service_convicts_covered_lies_pre_and_post_final_and_the_honest_twin_finalizes` |
| 〃 | the same three lies **post-Final** (node and service started after Final) | **V-node(svc)** | convicted inside the liability horizon | 〃 |
| 〃 | honest twin (same class, same Panel collusion) | **V-node(svc)** | nothing filed against it; reaches Final (its reward paid from the poster's escrow) | 〃 |
| 〃 | service restarted mid-pursuit (after its seal is on chain; ten blocks with no service; same state dir) | **V-node(svc)** | the new process re-derives: no second seal (the seal kept its DAA), one filing, conviction before Final | `kernel::g14k_the_kernel_service_resumes_after_a_restart_without_a_second_seal` |
| 〃 | quantization, decode, garbage | V-node(harness) (G14C/G14-V; the service runs the same `check_computation`) | — | §2a, §2e |
| 〃 OPV mode | gather / MatMul / clamp | V-node(harness) only (§2e) | — | an OPV class cannot be registered on kaspad's test network without the derived eligibility |
| F1 K2-TIR-v1 descriptor | — | **open**: no v1-descriptor class was run (the canonical class is v2) | | |
| F3 K2-TIR-v4 segmented (`wide128` @8,192, OPV) | lie at position 1,200: the service core rebuilds the prompt from the chain's tiles, demands (adaptive search), reads the producer's `Respond` parts back from the blocks, localizes, seals, files | **V-node(harness)** | convicted; an honest twin of the same shape: nothing sent | `real_scale::g14k_service_core::g14k_the_service_core_convicts_a_segmented_lie_via_demands_and_respond_chunks_and_ignores_the_honest_twin` |
| 〃 | withheld `Respond` (producer silent) | **V-node(harness)** | `Done(Defaulted)` at the counted deadline: `Unavailable { producer_defaulted }`, producer charged, demand bonds returned, never a conviction | `real_scale::g14k_service_core::g14k_the_service_core_takes_a_withheld_segmented_answer_to_the_da_default` |
| 〃 | the service's own block walk (`walk_accepted_lifecycle_objects_v1` → reassembly) | implemented; **not executed** at V-node(svc) (no v4 class on kaspad's network) | | |
| F4 v5 encoders / heads | — | same code path as F3 (`kernel_check_segmented_v1`); **not run** here | | |
| F6 typed roots — memory | step lie pre-Final (pre-state from the chain's carried post-state) and post-Final (line rolls back); honest claim left alone | **V-node(harness)** | both lies convicted (the post-Final one rolls the line back to its head); the honest claim: nothing sent | `r4x_typed_roots_e2e::g14k::g14k_the_service_core_convicts_memory_step_lies_pre_and_post_final_and_leaves_an_honest_claim_alone` |
| F6 retrieval / composite, F2 pipelines | — | **not checked by the service** (more than one model, or a snapshot copy); reported once per claim, never filed | | |

**Tests and results (2026-10-11; `buildslot.sh cargo … --offline`, line-tables profile; test binaries run directly).**
- `misaka-palw-remote --lib pursuit`: **4 PASS** (2 G14-R + 2 new: both chunk lanes reassemble in any order once; each step sends
  exactly its objects).
- `kaspad --lib g14k_` (the actual service): **2 PASS** (283 s single-threaded; the main test: class through onboarding at DAA 44,
  six convictions between DAA 122 and 134 — the three pre-Final ones first, by EDF — honest twin Final at DAA 170; the restart test:
  seal at DAA 50, conviction at 56 after the restart). An earlier run of the same code FAILED one assertion of the test itself (it
  counted the poster's escrow payment of the honest twin's Final reward as a seat charge); the assertion was corrected, not the service.
- `kaspa-consensus --lib g14k_ g14r_ t12_a2u --skip socket`: **17 PASS** (g14k service core 2, G14-R 12 — the planner now delegates —,
  A2U 3).
- `kaspa-consensus --lib r4x_ g14k` (after the typed-root addition): **14 PASS, 1 FAIL** — the new typed test and the two segmented
  ones PASS, R4X/G14C 11 PASS; the FAIL is the known `r4x_weights_only_spec_is_byte_for_byte_the_legacy_registration_and_the_unarmed_route_is_unchanged`
  (recorded failing by G14-V in §2e.2; this lane does not touch the gate it trips on).
- `kaspad --lib` without the two service-e2e modules (`palw_service_public_e2e`, `palw_service_violations_e2e`, slow and untouched): **601 PASS, 3 FAIL**, the three in files this lane did not touch (pre-existing at the base, not re-run there): `palw_backends::…class_resolution_is_not_reachable_from_the_block_processing_path` (consensus/Cargo.toml already names `misaka-palw-sdk`), `palw_operator_da::…the_tick_wires_the_lane_before_the_carriers_and_behind_no_flag` (its source-text needle no longer matches the formatted line), `workspace_default_members::…` (`misaka-palw-panel` not in the default members).

**Open (honest).**
1. **V-node(svc) only for F1 Panel-licensed v2.** v4/v5, OPV-mode v1/v2 and typed classes cannot be registered on kaspad's test
   network until a class can be made OPV-eligible there through the derived rule (codex's registration/eligibility lane). Their
   service path is the same library code, verified at V-node(harness); the service's own accepted-block walk is not executed.
2. **Not wired:** pipelines (F2), composites and retrieval snapshots (more than one model / a snapshot copy), a filing that needs the
   tag-113 chunk lane at V-node(svc) (the canonical filing fits one carrier; chunking is V-unit + G14-R's V-node harness).
3. **The synced collateral is approximate**: the service reads its bond's collateral (op 202's row) less its route chunk deposits,
   not every non-kernel reservation the fold nets out; a bond also holding legacy reservations can see a dry run pass that the fold
   refuses (the carrier fee is lost, nothing else).
4. **Throughput:** the court lane carries one carrier in flight per bond (`MAX_INFLIGHT_CARRIERS = 1`); six pursuits took 12 DAA here.
   Under a coalition that fills blocks there is still no reserved byte lane (G14-B's UNKNOWN stands).
5. **Socket RPC**: the service reads through the in-process API (op 211's source), not a socket; G14-R's socket evidence is for the
   harness verifier.
6. **Live drill**: not run, for G14-R's reasons (no drill arming for the kernel route; no seat-collusion knob).

## 3. Family matrices (milestone baseline)

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
* 2026-10-10 — milestone 3 (G14C, run `g14c-m3`, on `86ade72ba` = r4-fixes merged): `Rules` switched to G14R's rules (accuser
  seal-then-reveal; a pre-Final default keeps the rest of the reservation liable; escrow-funded Final reward; the OPV admission fee and
  the F-C4R4-05 dismissal fee accounted). GAP-10 on the canonical harness, both modes, the participant on its own fresh node:
  **verified** — quantization/rounding (a rounding `Div` off by one), a substituted delivered token (decode court), a self-consistent
  garbage trace under other weights (convicted against the registered weights); **refused at inclusion** — a fabricated segment
  boundary, a copied claim (the job paid once), a borrowed trace of another prompt. 24/24 `g14_canonical_*` pass.
  **Residual (not reachable on the node):** routing (TopK) and history (derived window) need a class that has them; the sketch's
  dense + MoE class is taken by the kernel ledger but refused by the route's carrier fit — worst filing ~193 MB under K2-TIR-v2's
  whole-instance courts vs 1.58 MB carriable (pinned: `g14_canonical_a_routing_and_history_class_is_refused_by_the_node_carrier_fit_until_element_courts`).
  Owner: K2S (K2-TIR-v4 element courts, GAP-30/31). GAP-71b (SDK fresh complete-check verifier) waits on OPVB's `PostComplete`,
  which is not on the integration line.
* 2026-10-10 — G14C on `34b6c3f5e` (+ this branch): **GAP-71b verified** — `misaka_palw_sdk::onboarding_chain::fresh_verify_complete_check_v1`
  re-runs a complete check from op 231 / op 211 reads and the verifier's own copy of the registered artifact (the post is in no served
  row; a post passes the fold only if it IS the honest post, so the chain's verdict is checked by post id); node test
  `opv_bootstrap::g14_canonical_a_fresh_node_reverifies_complete_checks_from_rpc_reads_and_its_own_artifact` (an honest and a forged
  check, another model refused `ARTIFACT_NOT_REGISTERED`). **GAP-50 implemented + verified at node level** —
  `PalwKernelRouteStateV1::opv_spec_eligibility_v1` (consensus-core, dormant behind `palw_panel_free_v1` + `palw_typed_roots_v1`):
  `Weights`/`Memory` through the rule program's single-program registration, `Composite` through every stage's component,
  `Retrieval` never (`SNAPSHOT_NOT_ONBOARDABLE`, until a snapshot conformance statement exists); the OPV gate admits typed classes by
  it; test `r4x_typed_roots_e2e::g14c::r4x_g14c_typed_classes_are_eligible_only_through_their_components` (only the component is
  seamed). **GAP-51 verified** — a memory lie that finalized, convicted from a pruned-import node inside liability, the line rolled
  back; a composite model-stage lie convicted at that stage; the typed rows and the line across a real restart. Residual: typed
  claims are not re-gated at commit (a class whose component later loses eligibility keeps taking claims). Run `g14c-m4`: consensus-core
  `palw_opv_bootstrap` 6/6; node `g14_canonical`/`r4x_`/`g14_opv_bootstrap`/`g14_conformance`/`g14_onboarding`/`g14_rewards`/`da16_`
  66 pass, 1 fail not this lane's (`da16_scope_a_snapshot_is_never_demanded_…`: expects the producer to lose only the default
  penalty, but r4-fixes also burns the 1 BILI OPV admission fee); after the composite test's snapshot-copy fix, `r4x_g14c` 4/4.
  Integration note: `34b6c3f5e`'s cherry-pick names `ledger_cache` / `KernelLedgerCacheV1`, which exist only on `k2/real-scale`;
  this branch drops that field from `from_rows` so the tree compiles.
* 2026-10-10 — refresh against `2d93ac288` (§2a): typed claims re-gated at commit (`palw_kernel_route_fold_v1::opv_gate_v1`, both the
  salted and the unsalted typed reveal: a class whose component lost eligibility takes no new claim; test
  `r4x_typed_roots_e2e::g14c::r4x_g14c_a_typed_class_whose_component_lost_eligibility_takes_no_new_claim`, with a `cfg(test)` seam
  inverse `kernel_route_test_opv_ineligible_v1`); legacy rows updated to LG14-B's V-fold; BUDGET hook coverage listed. Integration
  compile fix: `2d93ac288` did not build — the BUDGET merge dropped the `Ok(())` and closing brace of LG14-B's
  `palw_legacy_held_signature_ok` in `processor.rs`; restored.
* 2026-10-11 — G14-B (§2c): maximum-profile budgets and deadlines. `misaka-palw-kernel::budget` (route carriage, chunk-counting windows, the worst-case pursuit schedule, legacy sessions, population). Segmented demands now count chunks. Segmented classes get a derived liability horizon and are refused by name past a 436-DAA lifetime. The legacy 4-session FAIL is derived. Kernel 285, core 51 and node 19 PASS.
* 2026-10-11 — G14-S (§2c, branch `g14/s-scope-gates`): the cumulative court scope enforced (table 28, per-requester units and dismissals, the OPV v1/v2 clear budget), the scope report made the per-claim reward gate (`COURT_SCOPE_INCOMPLETE`), DA16b option (a) flat 64-element hiding tiles with leaf-hash DA and the hiding-leaf / hiding-decode courts, option (c) finished; 9B-8k derived incomplete (four relation types above one carrier).
* 2026-10-11 — lane G14-V (`g14/v-all-violations` on `8173dae20`): §2c. Authenticated subtree descent replaces the linear legacy
  scan in the production filer; localization past an agreeing step tree (logits / forged token). Real-service V-node runs on the
  floor: 5 step-lie op kinds + honest twin, 7 trace/token/output/input/state-identity lies, 2 post-Final lies, 2 LG14-B lies — all
  PASS; kernel canonical gather / matmul / clamp ×(Panel, OPV)×(pre, post Final) PASS; `g14_k2s_` 15 PASS; `r4x_` 11 PASS / 1 FAIL
  (integration-owned, see §2c.2).
  Regression on this head: kaspad `palw_fraud_filer` 20, `accepted_objects_walk_tests` 7, `reporter_filer::tests` 21,
  `palw_filer_held::e2e` 27 (LG14-B honest-claim guard updated to the past-step semantics), core `palw_legacy` 20 +
  `palw_auth_descent` 5, consensus `t12_a2u` 3 and `lg14a_` 8 — PASS; normal `kaspad` lib build OK. Codex's
  `fresh_public_bond_service_carries_reserved_da_to_objective_default` FAILED twice on its 360 s wall-clock deadline under host load
  (load average 10–15; reached DAA ~2,036 of the 2,231 default) and PASSED (objective DA default at 2,236) with the deadline raised
  locally to 1,500 s — the raise is not committed; the test's wall-clock bound is load-sensitive.
* 2026-10-11 — G14-R (§2d): recovery and the external path. `misaka-palw-remote::pursuit` (kernel pursuit planner rebuilt from public rows, dry-run filings, resume bound, deterministic carriers); kernel ledger: identical proof re-seal is a no-op (dormant route); V-node recovery cases for K2-TIR v2 (Panel/OPV), K2-TIR-v4 and the legacy public filer (restart, reorg + DAG merge, full IBD, pruned import, deadline); a real gRPC/wRPC socket in front of a test node (ops 202, 204–206, 210–212, 231, submitTransaction) with disconnect/reconnect and node swap; the actual service resumes after a restart and on a fresh database. Consensus 14 + 60 regression PASS, kernel 286 PASS, kaspad service 1 PASS (+16 unit); codex's fresh-service test times out at 360 s on this host (UNKNOWN). Live drill not run (infeasible: no drill arming for the public filer / kernel route, no seat-collusion knob).
* 2026-10-11 — G14-K (§2g): the kernel route's public verifier inside kaspad's `PalwPanelService` (`palw_kernel_verifier`, `--palw-kernel-model`), driving `misaka-palw-remote::pursuit` (extended: consumer ledger, program / segmented / typed checks, block material, step sends). V-node(svc) for K2-TIR-v2 Panel-licensed with every seat colluding: gather / MatMul / clamp convicted pre- and post-Final, honest twin Final, restart mid-pursuit without a second seal (kaspad 2 PASS); the class registered through the production onboarding path (no test hook). v4 segmented (demand + Respond → conviction; withheld → DA default) and typed memory at V-node(harness) through the same library functions. No consensus change, nothing armed.
