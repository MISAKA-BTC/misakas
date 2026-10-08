# Model onboarding — implementation matrix

Owner: Lead/Integrator. Baseline: `pre` @ `082636b64`; integration branch `claude/g14-public-prosecution-integration-9bee39`.

Goal: make the models that cannot be added today, but are representable by an existing kernel, registrable **generically**
(no model-name consensus code); send only truly missing semantics to versioned kernel extensions; keep every Active/Rewardable
profile behind `PUBLIC_PROSECUTION_COMPLETE(plan, profile)` (G14, see `g14-integration-matrix.md`).

```text
STATIC SEMANTIC ADMISSION → COMMIT FIRST → FUTURE PALW WORK BEACON → INDEPENDENT PROBABILISTIC CHECK → mismatch → EXACT PUBLIC COURT
```

The beacon authorizes no semantics. One contract: `misaka-palw-challenge` (Lead-owned; RFC-0007 Part VI).

## 1. Frozen shared interfaces (Lead only; others propose)

| Interface | Where | Status |
|---|---|---|
| VerificationPlan identity | `misaka-palw-kernel` `VerificationPlanV1::root()` | existing, frozen |
| KernelDescriptor binding / class id | `misaka-palw-kernel` `ModelKernelBindingV1::class_binding_id()` | existing, frozen |
| PublicProsecution gate | `misaka-palw-kernel::gate::{public_prosecution_complete_v1, public_pipeline_prosecution_complete_v1}` | existing, frozen (security oracle) |
| `challenge_policy_id` | `misaka-palw-challenge::PostCommitChallengePolicyV1::id()` | landed `26102b9fd` |
| Beacon subject/domain ids | `misaka-palw-challenge::{SubjectKindV1, ChallengeSubjectV1, hash::DOMAIN_*}` | landed `26102b9fd` |
| Model lifecycle states/failures | `misaka-palw-challenge::lifecycle::{OnboardingStateV1, OnboardingFailureV1}` | landed `26102b9fd` |
| Conformance evidence identity | `misaka-palw-challenge::conformance::{ConformanceCommitmentV1::statement_root, BeaconConformanceEvidenceV1::id}` | landed `26102b9fd` |
| Static outcomes (registration) | `misaka-palw-kernel::outcome::RegistrationOutcomeV1` codes | existing; map to `OnboardingFailureV1` (lane A/D) |
| Consensus tags / wire / fingerprint | `PalwConsensusObjectV2`, params identities | Lead only; none added yet |

## 2. Lanes and ownership

| Lane | Domain (crates/files) | Branch |
|---|---|---|
| Lead | `misaka-palw-challenge`, central enums/tags/serialization/fingerprint, these matrices, merges, full regression | `claude/g14-public-prosecution-integration-9bee39` |
| A — frontend & tasks | `misaka-palw-tir-lower` (hf_config, hf_schema, gguf, weights, adapter, model, encoder, embedding), SDK census/preflight frontend paths | `onboard/a-frontend` |
| B — kernel / plan / G14 | `misaka-palw-kernel` (ledger, gate, families, courts), reusable semantic extensions | `g14/b-kernel` |
| C — beacon conformance & runtime pack | `misaka-palw-sdk/src/runtime_pack/*`, `tir_layout.rs`, `conformance.rs`, `pack bind-class`, pack gate | `onboard/c-beacon-pack` |
| D — real registration E2E & adversarial (+ real-node G14 carriage) | consensus tests (T12Chain), RPC reads, kernel-route fold wiring under Lead's tags | `g14/d-node-e2e` |

## 3. Model / family matrix (Wave 1 first)

Columns: source format · task · frontend · semantic coverage · VerificationPlan · layout · conformance · registration · beacon ·
G14 · availability · active status · exact blocker · owner. `—` = not yet measured (never PASS).

| Model / family | Source | Task | Frontend | Semantics | Plan | Layout | Conformance | Registration | Beacon | G14 | Availability | Active | Exact blocker | Owner |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| W1 Llama / dense decoder (SmolLM2-1.7B, phi-1_5 real) | safetensors (HF) | text-generation | PASS (headers, tensors bound 0 missing) | existing | shape-ready @2048/@8192 | — | — | — | — | K2-TIR-v1 ELIGIBLE_AT (KERNEL_NOT_ACTIVE shipped) | — | Dormant | pack/weights-depth/conformance not yet run | A→C→D |
| W1 Qwen GQA / MoE (Qwen3.5-0.8B, granite-3.1-1b-a400m real) | safetensors (HF) | text-generation | PASS (Qwen3.5 vision tower reported text-only scope) | existing | shape-ready @2048/@8192 | — | — | — | — | ELIGIBLE_AT (hyp.) | — | Dormant | same | A→C→D |
| W1 recurrent / SSM (mamba-370m, Jamba-tiny hybrid real) | safetensors | text-generation | PASS | existing | shape-ready @2048/@8192 | — | — | — | — | ELIGIBLE_AT (hyp.) | — | Dormant | RWKV not yet exercised | A→B |
| W1 VLM or encoder-decoder | safetensors | image-text-to-text / seq2seq | — | — | — | — | — | — | — | — | — | Dormant | `PARTIAL_TASK_ONLY` | A→B |
| kernel route fixtures (`dense_moe_v1`, pipeline VLM) | synthetic | text / VLM | n/a | K2 families | PASS | n/a | — | reference ledger only | not wired | reference PASS | n/a | Dormant | real-node carriage (G14 matrix) | B, D |

## 3a. Coverage Closure — Priority 0 (OB-P0): the onboarding path end to end on the real node

Record: `g14-node-e2e-record.md` §7. Branch `onboard/p0-conformance-carriage`. Dormant behind `palw_probabilistic_constraints_v1`.

| P0 item | What is on chain / in the tools | Test (real node unless noted) | Status |
|---|---|---|---|
| Conformance evidence carried on chain | tag 109 `ConformanceEvidenceV1` (Post / Refute), aux 39 (attempt + `OnboardingRecordV1`), aux 40 (material); chunked delivery judged on the whole | `g14_conformance_evidence_passes_only_after_...` | PASS |
| Beacon from the chain | `BeaconContextV1` frozen at the commitment; FUTURE OPV Finals (`FinalPathV1::PanelIndependent`, op 212's rows) of other classes; candidate + its kernel class excluded; BEACON_UNAVAILABLE when the window closes short | same; `g14_onboarding_a_class_is_bound_..._released_by_the_gate` | PASS (no network has OPV Finals yet: GAP §7.5-1) |
| Verification decides the state | in-fold: binding to the attempt + exact rebuild from outcomes + bits; window 80 DAA; LeafDecode / VectorTokens refutations; withheld = default; attempts counted, exhausted at 3 | `..._against_another_beacon_..._stale_evidence_is_invalid`, `..._refuted_withheld_..._attempts_are_exhausted`, `..._hostile_evidence_...`, `..._forged_outcome_list_...` | PASS (vector digests: GAP §7.5-2) |
| A commitment alone never passes | gate Ready only at G14_ELIGIBLE / ACTIVE_REWARDABLE | `g14_onboarding_a_class_is_bound_..._released_by_the_gate` (updated) | PASS |
| SDK signs tag 108 | `SignedRegistrationRequestV1` (one builder, detached JSON, `EnvelopeSigner`); fork digest from the client's params (F-C4R3-01(b)); CLI `misaka model onboard envelope-export / envelope-sign` | the happy path registers through it; SDK unit test; C4r3 PoC un-ignored | PASS |
| Public refetch | RPC 231 `getPalwConformanceEvidence` (rows of 39/40, program, policy, beacon state, gate); `ConsensusApi::palw_conformance_evidence_v1` | grpc / model round trips; integration arm (not-found / malformed) | PASS (no running-service test: GAP 12) |
| Fresh verification from public material | `fresh_verify_v1` (core) / `fresh_verify_from_reads_v1` (SDK, checks the artifact's inventory root) / `misaka model onboard verify` | agrees with the fold in the happy path; finds the forged leaf | PASS |
| Registration state fold | `OnboardingRecordV1` on chain: REGISTERED_DORMANT → CHALLENGE_PENDING → CONFORMANCE_PASSED → G14_ELIGIBLE → ACTIVE_REWARDABLE | all of the above; reorg / restart / pruned import | PASS |
| Honest residuals | beacon availability, vector digests, binding equality (GAP 1), FinalReward (GAP 5), interim numbers, chunk-lane capture (F-C4R3-03) | — | GAP (§7.5) |

## 4. Blocker families (census update 1, 2026-10-04; technical view, estimated repositories of `D_all` = 3,117,871)

Priority: generic frontend → generic task → layout/tooling → conformance → reusable semantic gap → one-off exotic.
Each fix records **before N / after M** here.

| Blocker family | Kind | Before (repos) | After | Notes / owner |
|---|---|---:|---:|---|
| `TASK_UNKNOWN` | generic task | 661,725 | — | A |
| `MODALITY_PROFILE_MISSING` (text-classification 138k, ASR 53k, RL 75k) | generic task | 460,860 | — | A (task coverage; classifier/reranker/audio fixtures) |
| `FORMAT_UNSUPPORTED` (pytorch pickle 79k) | generic frontend | 130,554 | — | A (no unsafe pickle execution) |
| `PARTIAL_TASK_ONLY` (image-text-to-text 63k) | generic task | 75,937 | — | A→B |
| `ARCH_REFUSED` (gguf missing keys, seed_oss, M2M100 no adapter) | generic frontend | 37,877 | — | A |
| `TOKENIZER_MISSING` | generic frontend | 32,470 | — | A |
| `CONFIG_KEY_UNREAD` (rope_interleaved, n_special, max_seq_len) | generic frontend | 31,715 | — | A |
| `NOT_RUN_NEEDS_TENSOR_DATA` (rope_freqs.weight) | frontend (header-only ≠ unsupported) | 23,600 | — | A |
| `CUSTOM_CODE_UNMODELLED` | frontend / semantic | 19,558 | — | A→B |
| `FEATURE_C` (GEN_UNET_SKIP_V1 28k) | semantic | 47,995 | — | B (only if reusable) |
| `NOT_RUN_PIPELINE_ADMISSION` | layout/tooling | 41,417 | — | C/D |
| `COURT_BUDGET` / `ADMISSION_EXCEEDS` (≥100B) | resource (RFC-0006) | ≈11,000 | — | not by raising caps |
| registration-ready (pack, seat, final) | — | 0 measured | — | C, D |

## 5. Repository-level completion checklist

1. frontend vs semantic gaps distinguished (`FRONTEND_REQUIRED` ≠ `KERNEL_EXTENSION_REQUIRED`) — partly (outcome codes exist); census re-run pending.
2. known-semantics models register without model-name consensus code — GAP (no real registration E2E yet).
3. exact layout preserved pack → preflight → registration — PASS for gate↔chain parity (lane D: 1,050 mutations, 30 classes × 33 heights, real checkpoints); pack→registration with real artifact bytes GAP (real-checkpoint fixtures use a synthetic artifact commitment).
4. runtime pack bounded on large artifacts — PARTIAL: streamed authentication of a 1.87 GB artifact (3 s hash pass) + sampled leaves; vector checks O(positions) heavy (GAP: tiled range evaluator, stored Merkle index).
5. static admission not replaced by beacon — PASS by construction (contract: beacon only after `StaticAdmitted`; tests).
6. challenge entropy fixed only after commitment — PASS (contract + pack tests, real checkpoint on synthetic facts); chain wiring PASS on the real node (OB-P0: on-chain commitment, beacon from folded OPV Finals, op 231 + op 212).
7. beacon PALW-native, no BFT/validator — PASS (contract); wired in the fold (OB-P0, sources: Panel-independent Finals only).
8. heartbeat/BASE-0 never entropy — PASS (contract tests).
9. fresh node replays beacon evidence — PASS (contract; `verify-conformance` fresh process re-derives beacon/seed/selection and re-executes); node facts RPC PASS (ops 231 + 212; `misaka model onboard verify`).
10. registered vs active separated — PASS (lifecycle); consensus state PASS (OB-P0: `OnboardingRecordV1` in aux table 39, gate on it).
11. Active profiles PUBLIC_PROSECUTION_COMPLETE — reference PASS (gate at class registration); real node GAP.
12. fresh outsider prosecution from public material — reference PASS; real node GAP.
13. real-node registration E2E — PASS to REGISTERED_DORMANT/Candidate→Active by the existing V2 lifecycle (mempool → template → fold → persisted tip → ConsensusApi reads; SmolLM2, Mamba, Granite MoE, Qwen3.5 GDN at 32,783 positions under shipped rules); RPC over the wire not exercised; no conformance/beacon/G14 states on chain (GAP).
14. restart/IBD/reorg same state/challenge — registration state PASS on the node (second-node replay, reorg incl. merge re-fold, pruned import, restart); challenge state not on chain (GAP).
15. adversarial registration tests — PASS for everything the current wire can express (27 adversarial + mutation differential, replay on another network, below-fence, duplicate, cap); GAP for challenge-policy substitution, conformance after artifact change, beacon reorder, heartbeat/self beacon, G14-incomplete activation, plan substitution (no consensus objects yet — lane D phase 3).

## 6. Change log

* 2026-10-08 — OB-P0 (branch `onboard/p0-conformance-carriage`): conformance evidence on chain (tag 109, aux 39/40, RPC 231), the beacon
  from future Panel-independent Finals in the fold, optimistic window with LeafDecode / VectorTokens refutations, attempts counted on the
  contract's `OnboardingRecordV1`, the gate on CONFORMANCE_PASSED; SDK tag-108 envelope + detached signing, fresh verifier, CLI
  `misaka model onboard`; F-C4R3-01(b) fixed (the envelope names the fork-id fired digest). Evidence core moved from the runtime pack to
  consensus-core (re-exported, encodings unchanged). Record: `g14-node-e2e-record.md` §7.

* 2026-10-08 — lane D phase 1 integrated (…`0bf26024f`): `docs/design/palw/registration-e2e-record.md`; fixed registration-status
  readers missing `ClassRegisteredTirV1` carriers. Findings F2 (mempool admits IR registrations the chain drops → fee lost; propose
  gate + signature/target at admission as node policy), F3 (dropped-carrier RPC diagnosis re-asks only the gate), F4 (RPC/SDK judge
  at tip DAA — safe direction), F5 (same weights under a second class id pass), F6 (model alias is a free claim). Consensus GAP list
  1–10 (challenge policy binding, conformance commitment per (class, artifact), beacon state, plan/kernel binding in the class id,
  G14 gate at activation, tokenizer/artifact bound to bytes) → lane D phase 3 after the G14 node wiring.

* 2026-10-08 — lane C final integrated (`8906aa73b..d059e8fef` as `…aaad290ac`): `pack commit-conformance` / `run-conformance` /
  `verify-conformance` over the shared contract; real SmolLM2-1.7B (1.87 GB class file): commit 5.5 s → WAITING_RANDOMNESS with
  2/3 works → locked → PASSED 771/771 checks (577 s, 4.1 GB RSS) → fresh-process verify re-executing everything PASS (627 s);
  `--no-rerun` never a pass; one flipped evidence bit → `EVIDENCE_FORGED`. Facts are SYNTHETIC (no node RPC for canonical beacon
  facts yet), policy is the unapproved reference policy, derived −log2 ε = 4 (conditional), legacy calibration stats, no HF source
  fidelity. Gaps: tiled range evaluator for 40-bit policies (≈56 vector draws ≈ 3 h), stored Merkle index, on-chain conformance
  commitment + beacon-facts RPC (lane D / Lead), contract proposals 1–8 (record §5).

* 2026-10-08 — lane A milestone 1 (branch `onboard/a-frontend`, not yet integrated): census gate results carry a machine class (`misaka-palw-sdk/src/census/onboarding.rs`; `NOT_RUN_*` never a semantic gap; PROFILE_REQUIRED = task profile, lifecycle KERNEL_EXTENSION_REQUIRED "task profile"); KERNEL_EXTENSION_REQUIRED list empty (845/847 sampled decoders ELIGIBLE_AT, 2 BOUNDS_EXCEEDED ≥70B); GGUF `rope_freqs.weight` → ROPE_FREQ_FACTORS_V1 (NOT_RUN_NEEDS_TENSOR_DATA headers pass est 5,715 → 440; GGUF frame 18 members → shape-ready); tokenizer file table (TOKENIZER_MISSING est 32,517 → 27,443); GGUF LoRA relabelled ADAPTER_REFUSED; pre-existing `adapter_pins` failure (16 adapters) recorded.

* 2026-10-08 — matrix created; contract `misaka-palw-challenge` landed.
