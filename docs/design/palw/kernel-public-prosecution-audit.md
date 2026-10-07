# Public prosecution on the kernel route — repository-level audit (G14)

Status: **repository-level G14/public prosecution implementation complete; external validation gates remain.**

This audit traces the property ADR-0173 and RFC-0015 §1.1 (G14) ask of the kernel route, as code in this repository:

> The producer and **every** fixed Panel seat collude. One ordinary bonded verifier outside the Panel — without producer-private
> state, Panel-private state, a served view or any seat's local capture, using only canonical public authenticated material —
> detects and localizes any fault an Active `VerificationPlanV1` covers (and every job / input / output / state / DA violation),
> and reaches an objectively adjudicated conviction, or a correctly classified availability/default outcome.

The vehicle is `misaka-palw-kernel` (ADR-0172). Its consensus rules are exercised on an **in-process chain**
(`ledger.rs`, `KernelLedgerV1`): a deterministic fold of blocks of bond, class, job, claim, Panel, proof, demand and response
transactions. Every outsider in the tests is a **fresh node**: it replays the block sequence from genesis and reads committed
values only from a public DA store of bytes (minus what the producer withholds) or from values served on chain in answer to
demands — the producer's objects are dropped first; no Panel capture, court internals, served view or unauthenticated witness is
reachable from it. Its probabilistic checks use **its own salt**, never the claim's public beacon.

## 1. The 20 categories

Each row: public material → independent verification → localization → authenticated bounded witness → objective terminal →
consensus result, with the code and the tests that exercise it. "Gaps" lists repository-level gaps (none remain) and external
gates.

| # | Category | Public material → verification → localization | Witness → terminal → consensus result | Code | Tests |
|---|---|---|---|---|---|
| 1 | Arithmetic / kernel | committed node values (DA / demand), public artifact, plan → `FreshVerifierV1::check_salted` (Freivalds over the checker's salt, exact families recomputed) → one instance or one `MatMul` scalar | scalar: 3 Merkle openings (row X, col W, row Y); instance: opened inputs/output → `scalar_court` / recompute → `Convicted`, reservation slashed, accuser share | `verify.rs`, `public.rs`, `ledger.rs` `adjudicate` | `k2_e2e` (every scalar lie, every exact family), `k2_public`, `k2_ledger` A |
| 2 | Quantization / rounding / range / carry | the plan's exact-result rule (`analyze_ranges`, `RangeRuleV1`) proves no wrap; shifts, rounding, clamps, narrowing are exact families | the opened instance recomputed under reference semantics → conviction | `check.rs`, `plan.rs`, `family.rs` | `k2_e2e` `a_false_value_in_every_exact_family…`, `k2_wide` (i128, aliasing mod 2^127−1) |
| 3 | Routing / TopK | router nodes are exact relations over committed scores | opened scores + indices recomputed → conviction | `family.rs` (TopK / MoE combine) | `k2_adversarial` `a_swapped_expert_choice…` |
| 4 | Memory / history / checkpoint / state | committed state rows, segment entry/exit state roots in the evidence object | `StateContinuity` court opens the window's prior rows; a fabricated boundary or root is **refused at inclusion** (`claim_structure_v1`) | `trace.rs` wiring, `evidence.rs` `check_evidence_v1`, `verify.rs` `claim_structure_v1` | `k2_e2e` (forged history row, fabricated segment boundary), `k2_adversarial` (permuted window), `k2_ledger` C (malformed evidence refused) |
| 5 | Job identity | the posted job (`KernelJobV1` / `PipelineJobPostV1`) and its id | claim naming no posted job, or another evidence root → refused at inclusion | `job.rs` `binding_fault_v1`, `ledger.rs` commit paths | `k2_ledger` C, `k2_ledger_pipeline` |
| 6 | Input / prompt binding | `job_input_root` over prompt ‖ delivered ids; pipeline `job_root` over every job fact | another input → `WrongInput`, refused at inclusion | `job.rs`, `pipeline.rs` `pipeline_job_root_v1` | `k2_ledger` C (borrowed trace), `k2_ledger_pipeline` (another job) |
| 7 | Output / token / logits binding | delivered ids; committed logits rows; pipeline `output_root` | `DecodeFaultV1` / `PipelineFaultWireV1::Decode`: one logits tensor opened, the job's rule applied → conviction; another output root → refused | `job.rs` `verify_decode_fault_v1`, `pipeline_public.rs` | `k2_ledger` C (last and mid-stream substitution), `k2_ledger_pipeline` (VLM substitution, `WrongOutput`) |
| 8 | Segment / pipeline boundary | per-stage evidence, stage input commitments, edge relations | edge fault: claimed input + upstream outputs authenticated, binding recomputed → `verify_edge_fault_v1` → conviction | `pipeline.rs`, `pipeline_public.rs`, `ledger.rs` | `k2_pipeline`, `k2_ledger_pipeline` (conditioning edge) |
| 9 | Media / VLM edge | job images and facts are public; `R` is `dist(R(seed, …))` over the job's public seed, bound by `random_binding` | image edge / `R` draw recomputed → conviction; another seed bound → refused | `pipeline_public.rs` `PipelineRandomV1` | `k2_ledger_pipeline` (false jitter, `R` from another seed, false image edge after an on-chain demand) |
| 10 | DA availability | demands per stage position; responses posted on chain | served → public; non-serving past the deadline → `ProducerDefault` (fixed penalty to demanders), claim `Unavailable`, never the fraud slash | `ledger.rs` `file_demand` / `respond` / `tick` | `k2_ledger` D, G; `k2_ledger_pipeline` |
| 11 | Self-consistent garbage trace | public weights | a trace consistent under other weights fails at the first product against the registered artifact → conviction | `verify.rs` | `k2_ledger` B; `k2_ledger_pipeline` (`R` swapped) |
| 12 | Borrowed trace | job/input roots | refused at inclusion (`WrongInput`) | `ledger.rs` | `k2_ledger` C, `k2_ledger_pipeline` |
| 13 | Malformed / fake opening | response bytes; filings | responses classified `malformed` / `wrong_bytes` / `wrong_root` / `fake_opening` / `partial` / `oversized`; garbage filings dismissed (fee); evidence a court could only call malformed refused at inclusion | `public.rs` `classify_position_response_v1`, `verify.rs` `claim_structure_v1` | `k2_ledger` B, C, G |
| 14 | Held / fused terminal | every node value is committed (row/column Merkle); a `MatMul` court opens one row/column (O(k + n)). A long-history (held) class's `Hist` window and its views are **derived** (`derived_nodes_v1`): committed, never served — anyone rebuilds them from the committed appended rows, so public bytes are linear in the claim's length, not quadratic | a wrong window commitment → `Misderived`, proved from the authenticated rows alone (the proof carries no producer window byte); a profile needing private weights/input/state, a FOLD prefix or a fused preimage has `PrivateMaterial` → never registers, never rewards; nothing substitutes a served view | `trace.rs` `derived_nodes_v1`, `verify.rs` (`derive`, `Misderived` court), `plan.rs` (budgets exclude derived), `gate.rs`, `ledger.rs` | `k2_ledger::a_history_window_is_never_served…`, `k2_adversarial` (permuted window), `k2_public` (fused profile gap), `k2_ledger` D |
| 15 | Producer non-response | demand deadline | `ProducerDefault` with the last response class; availability, not fraud | `ledger.rs` `tick` | `k2_ledger` D, G; `k2_ledger_pipeline` |
| 16 | Court / session pre-emption | direct proofs | adjudicated in the block that carries them whatever sessions are open; open demands settle **moot** and every bond returns; one session per stage position so no demander can crowd out another | `ledger.rs` `file_proof`, `settle_demands_moot` | `k2_ledger` E |
| 17 | Challenge vs Final race | lifecycle | a proof in the window's last block is applied before the tick; demands open only inside the window and live `court_deadline`, so Final ≤ window end + court deadline; after Final, a proof inside `liability_daa` still convicts | `lifecycle.rs`, `ledger.rs` `demand_window_open` | `k2_ledger` A, F |
| 18 | Duplicate proof / reorg / restart / IBD | block sequence | a claim is convicted once (`Duplicate`); the state is a pure fold (`replay`): restart, IBD and a reorg's branch reach the same root | `ledger.rs` | `k2_ledger` H, every `outsider()` (replay-root equality) |
| 19 | Collateral reservation / exit / double use | bonds | a claim reserves free collateral (no double use) until its liability horizon; an exiting bond backs nothing new and withdraws only after its delay with nothing reserved | `ledger.rs` `admit`, `Withdraw` | `k2_ledger` H |
| 20 | Unsupported relation / terminal | descriptors the binary implements; the plan | unknown kernel, unknown checker, a court that is not public, an unbounded path, a decode rule this version lacks → **admission failure**, never success | `gate.rs`, `check.rs`, `job.rs` | `k2_ledger` (class refusals), `k2_adversarial`, `k2_e2e` (plan forgeries), `k2_ledger_pipeline` (no decode rule) |

**"A probabilistic check failed" never convicts.** The ledger has no transaction that turns a receipt, a tally or a checker's
failure into a conviction: only `adjudicate` (an exact court over authenticated openings) convicts. The Panel's tally only
licenses Final (`PanelCovered`), and a fully colluding Panel licensing a false claim is exactly case A below.

## 2. Targeted end-to-end cases

| Case | What it shows | Test |
|---|---|---|
| A — producer + every Panel seat collude | the Panel signs the claim covered; one outside bond convicts it before Final (never finalizes) or, once Final and paid, inside the liability horizon (post-Final slash); past the horizon the reservation is released | `k2_ledger::a_full_panel_collusion_loses_to_one_outside_bond_before_final_and_after_it` |
| B — self-consistent garbage | weights perturbed, trace and greedy tokens recomputed consistently: convicted against the public artifact; dismissed accusations (wrong proof, junk bytes, a correct decode) cost a fee and change nothing | `k2_ledger::a_self_consistent_trace_under_other_weights…` |
| C — borrowed trace / substituted output | another job's valid trace → `WrongInput` at inclusion; another evidence, unknown job, out-of-range token, foreign commitments, fabricated boundary, misshapen commitments → refused; a substituted delivered id → decode court | `k2_ledger::a_borrowed_trace…`, `k2_ledger_pipeline::a_trace_drawn_from_another_seed…`, `…a_vision_language_claim…` |
| D — held / withheld material | the convicting value withheld: the outsider demands its positions in one round; an authentic row does not serve; the committed values served on chain convict; silence defaults. A held (long-history) window is never published at all: a permuted window is rebuilt from the rows and convicted as misderived; a response that serves a window is malformed | `k2_ledger::withheld_positions_are_demanded_in_one_round…`, `k2_ledger::a_history_window_is_never_served…`, `k2_ledger_pipeline` (vision stage withheld) |
| E — court pre-emption | spam demands on most positions; another bond still opens and joins; the direct proof convicts in its block; six sessions settle moot, every bond returns | `k2_ledger::open_demand_sessions_never_preempt…` |
| F — Final race | spam demands at the window's last block, served at their deadlines: Final ≤ window end + court deadline; a proof in the window's last block blocks Final; a last-block demand still reaches a post-Final conviction | `k2_ledger::spam_cannot_hold_final_past…` |
| G — DA classes | absent (timeout), malformed, wrong bytes, wrong root, fake opening, partial — each rejected by class; the default is a fixed penalty to the demanders, the claim `Unavailable`, no conviction; an authentic opening is not an acquittal | `k2_ledger::da_responses_are_classified…`, `k2_ledger::withheld_positions…` |

## 3. Admission: `PUBLIC_PROSECUTION_COMPLETE(plan, profile)`

Derived from code, never declared (`gate.rs`): `public_prosecution_complete_v1` (a program) and
`public_pipeline_prosecution_complete_v1` (every stage, every edge on the public edge court) return the bounds or every gap
(`NoCourt`, `PrivateCourt`, `UnknownChecker`, `PrivateMaterial`, `Unbounded`, `WrongDescriptor`, `NoEdgeCourt`, `Stage`). The
ledger's class registration **requires** it (a class without it never exists, so no job, claim, Final or reward does);
`reward_eligible_v1` references it for any other reward path. The live consensus route (Panel replay, held/fused classes under
ADR-0069) keeps its own rules; the kernel route's fence `palw_probabilistic_constraints_v1` is dormant and refuses arming.

### Resource bounds per path

| Bound | Derivation | Enforced |
|---|---|---|
| max public bytes | Σ positions × evidence bytes/position (derived windows excluded: linear in length) + artifact + commitments (Σ over stages) | gate ceiling |
| max opening bytes | the plan's worst court bytes (≤ the descriptor's `max_court_bytes`) | gate |
| max filing bytes | 2 × worst court bytes + header; an edge filing opens every upstream output | ledger, before any court runs |
| max response bytes | one stage position's committed values + stage inputs + wire headers | ledger, classified `oversized` |
| max localization rounds | 2: one round of demands (every missing stage position at once) and one direct proof | structure of the ledger |
| max court work | the plan's worst court work (≤ the descriptor's `max_court_work`) | gate |
| max verifier RAM | artifact + one position's values (Σ over stages) | gate ceiling |
| max retained state | commitments + per-claim rows until the liability horizon | gate ceiling |
| max concurrent sessions | one per stage position (Σ `max_positions`) | gate ceiling; ledger keys demands by `(claim, stage, position)` |
| deadline / inclusion | `court_deadline_daa` per demand; demands only in the window (or the liability horizon); `liability_daa > court_deadline_daa` (validated at genesis) | ledger |

## 4. External validation gates (`EXTERNAL_GATE_PENDING`)

None of these is a repository-level gap; each needs something outside this repository.

1. Independent soundness review of the composition, the alias bounds and the CRT argument (RFC-0011 §15.4, ADR-0172 §6).
2. Review of the beacon as unbiased under withholding (§15.3). (Outsiders do not depend on it: they check with their own salt.)
3. Real-hardware measurement of producer, verifier, DA, court and dispute load for 9B-8k, validated long context and Kimi K3
   (§15.6–15.7) — the bounds above are finite and enforced, their real values are measured there.
4. The real-chain G14 drill (RPC → fee → inclusion → fold → slash → blocked Final), censorship and mempool behaviour (RFC-0014 /
   RFC-0015), with the ledger rules above wired into the node.
5. Shadow comparison against the Panel route, audits, soak — and only then a proposal to give the fence a height.
