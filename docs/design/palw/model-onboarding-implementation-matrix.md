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
| W1 Llama / dense decoder | safetensors (HF) | text-generation | — | — | — | — | — | — | — | — | — | Dormant | first real-checkpoint E2E not run | A→C→D |
| W1 Qwen GQA / MoE | safetensors (HF) | text-generation | — | — | — | — | — | — | — | — | — | Dormant | same | A→C→D |
| W1 recurrent / SSM (GDN, Mamba, RWKV) | safetensors | text-generation | — | — | — | — | — | — | — | — | — | Dormant | recurrent state relation coverage unmeasured | A→B |
| W1 VLM or encoder-decoder | safetensors | image-text-to-text / seq2seq | — | — | — | — | — | — | — | — | — | Dormant | `PARTIAL_TASK_ONLY` | A→B |
| kernel route fixtures (`dense_moe_v1`, pipeline VLM) | synthetic | text / VLM | n/a | K2 families | PASS | n/a | — | reference ledger only | not wired | reference PASS | n/a | Dormant | real-node carriage (G14 matrix) | B, D |

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
3. exact layout preserved pack → preflight → registration — GAP (D parity test).
4. runtime pack bounded on large artifacts — GAP (C).
5. static admission not replaced by beacon — PASS by construction (contract: beacon only after `StaticAdmitted`; tests).
6. challenge entropy fixed only after commitment — PASS (contract tests); wiring GAP (C/D).
7. beacon PALW-native, no BFT/validator — PASS (contract); wiring GAP.
8. heartbeat/BASE-0 never entropy — PASS (contract tests).
9. fresh node replays beacon evidence — PASS (contract `verify_work_beacon_v1`); node wiring GAP.
10. registered vs active separated — PASS (lifecycle); consensus state GAP (D).
11. Active profiles PUBLIC_PROSECUTION_COMPLETE — reference PASS (gate at class registration); real node GAP.
12. fresh outsider prosecution from public material — reference PASS; real node GAP.
13. real-node registration E2E — GAP (D).
14. restart/IBD/reorg same state/challenge — contract PASS; node GAP.
15. adversarial registration tests — GAP (D).

## 6. Change log

* 2026-10-08 — matrix created; contract `misaka-palw-challenge` landed.
