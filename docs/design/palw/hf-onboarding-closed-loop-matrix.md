# HF onboarding — closed-loop matrix (H1)

Owner: H1 (HF Onboarding Closed-Loop). Branch `onboard/h1-hf-closed-loop` (base `89ffb1fb7`, plus the cherry-picked fixes named
below). Every cell is a measurement on a private devnet with a pinned checkpoint, or it says why it is not. **Separate columns** keep
apart what must never be read as one another: header/shape-ready, text-only scope, short context, synthetic-beacon conformance,
dormant registration (Candidate), and full-task Active/Final.

Evidence per run: `docs/evidence/hf-onboarding/<run-id>/{model,environment,preflight,conversion,pack-verification,registration,
consensus-state,g14-gates,failures}.json`, `reproduction.md`, `summary.md`. Runner: `tests/hf-onboarding/` (see §5).

## 1. Levels

| level | what it takes here |
|---|---|
| L0 source | the pinned revision readable on the Hub (metadata only), and a local checkpoint's every required file matching it (LFS sha256 / git blob id) |
| L1 shape | `misaka model preflight hf://REPO@REV --depth shape --max-context N`: convert **and** register ok, from headers only |
| L2 artifact | full checkpoint → calibration → `palw-class pack build --declare` → `pack verify --strict --rebuild --model` on the **declared** class file: VERIFIED, nothing failed, nothing skipped (HF reference fit included) |
| L3 registered | client U's own signed `ClassRegisteredTirV1` folded on the devnet (U: own key, own funds, own bond, no node, no seat; RPC only) |
| L4 replayed | A, B, C agree on the class row, the registry row and the `classes` state proof at one pinned block; a node restarted over its DB agrees; a fresh node by IBD agrees; U proves the registration against the pin; a 1-bit-modified artifact is refused by `pack verify` and by the registration's pack gate |
| L5 eligible | conformance + DA + G14 hold **on the chain** — this build has no chain state for any of the three (G14_INCOMPLETE; on-chain conformance waits for OB-P0) |
| L6 useful work | a real claim executes, is verified, goes Final, reward attributed — only once L5 exists |

The checkpoint on this Mac (L0, L2) is the tester's own copy of the pinned revision. That is a precondition of building an artifact
here, not a chain condition: the chain does not check that anyone serves, seeds or holds a model (ADR-0177), and no level above says
"available". L3/L4 concern a registration, and a registration is permanently immutable (ADR-0175): the harness never "updates" a
registered class; changed content (a different calibration policy, a new revision, a re-quantisation) is a NEW registration with its
own class id, and the parent's row never moves. The stage `onboard.sh resubmit` (alias `reregister`) is only the duplicate probe: the SAME
registration object through another carrier, which the chain must not write twice.

Vocabulary of a class on the devnet: the class table's `status: Active` is a row marker; the registry's `state` is what counts, and
every class here is `Candidate` ("registered, and not admitted — no seat outside the registrant's own identity has proved it is ready
to verify this class"). A `Candidate` is a dormant registration, never full-task Active or Final.

`SYNTHETIC_BEACON_CONFORMANCE_PASS` (lane C's commit → synthetic facts → run → fresh verify) is a separate column: it is never on-chain
conformance and never L5.

## 2. Devnet

Salted testnet-12 drill (ADR-0152 §8.2), loopback only, its own genesis and keyring written by the binary under test. The shipped
release's flag days compressed exactly like the combined drill (`audit-combined/dc.sh`): `--palw-drill-fence-at 6 --fence2-at 10
--fence3-at 14 --tir-at 16 --tir2-at 24 --int11-at 26` — kaspad's own log lists every fence as MOVED from its shipped height
(750/1000/1300/1700/2000/3600/5300/5395/5490/5585), none ARMED from dormant. `palw_bond_maturity` (1,000) has no drill flag and stays at
1,000 (the devnet runs below it). Nodes: A (seat 0, floor producer + heartbeat clock), B (seat 1, heartbeat, U's RPC), C (keyless
observer), D1–D6 (seats 2–7); transient: the bond registrar, a fresh IBD node Z. `--ram-scale 0.3`, 2 GiB host share per node.

Run r1: integration `89ffb1fb7` (kaspad sha256 `6e9b1936…`, misaka `211e708d…`; palw-class rebuilt at `be5808434` = `89ffb1fb7` +
`5048b7880` (calibrated_context) + `fa13939b9` (offline gate height), sha256 `e789d9c4…`), genesis `76a7e10d…`, params `f2da0947…`,
Apple M1 Max 32 GiB, macOS, shared with other agents (load 30–370).

## 3. Model matrix

Evidence directories are under `docs/evidence/hf-onboarding/`. Fit = the pack builder's measurement against the float32 Hugging Face
reference (tolerance: slope [0.97, 1.03], corr >= 0.99, top-1 >= 0.8, mean KL <= 0.1). All L4 rows: pack `verify --strict --rebuild` VERIFIED
with nothing skipped, registration folded as `Candidate`, A/B/C agree on class row + registry row + `classes` state proof, a restarted node and a
fresh IBD node agree, a 1-bit-modified artifact is refused, `SYNTHETIC_BEACON_CONFORMANCE_PASS` (synthetic facts, UNAPPROVED test policy).
None is eligible (L5) or produced useful work (L6): the chain has no G14 / on-chain conformance state for any of them.

| model @ revision | scope, context | level | evidence | fit (KL / top-1) | note |
|---|---|---|---|---|---|
| Qwen/Qwen2.5-0.5B-Instruct @7ae55760 | text, 512 | **L4** | `20261008-qwen25-0.5b-r1` | 0.0024 / 0.969 | lifecycle probe: GAP-6 (Probation 0/10 finals, 8 ready seats); duplicate-resubmit finding routed to C1 |
| unsloth/Llama-3.2-1B-Instruct @5a8abab4 | text, 512 | **L4** | `20261008-llama32-1b-r1` | 0.0102 / 0.938 | |
| HuggingFaceTB/SmolLM2-1.7B-Instruct @31b70e2e | text, 512 | **L4** | `20261008-smollm2-1.7b-r1` | 0.0791 / 0.938 | closest to the KL limit; the reorg probe's result is routed to the fork-choice lane (internal) |
| Qwen/Qwen3.5-0.8B @2fc06364 | text, 512 (gated-delta hybrid) | **L4** | `20261008-qwen35-0.8b-r2` | 0.0120 / 1.000 | |
| zai-org/glm-edge-1.5b-chat @7b201d3c | text, 512 | **L4** | `20261009-glm-edge-1.5b-r1` | 0.0016 / 1.000 | artifact 1,642,067,712 B, VERIFIED with `--rebuild` |
| huihui-ai/Huihui-Qwen3.5-9B-abliterated @05b9e7c9, 8k text | text decoder only (vision tower, MTP head not converted), 8,192 | **L1** | `20261008-huihui-qwen35-9b-r1`, `20261010-huihui-qwen35-9b-fit-diagnosis` | **0.268 / 0.688: refused** | the real 8k build ran to the end (calibration 1 x 8,192, conversion 9,649 MiB) and the fit gate refused it; four-layer diagnosis in the evidence; no artifact, nothing registered |
| isichan-ai/Mitsuba_and_HiMitsuba-27B-GGUF (PTQ1_0) @33e63d45 | text decoder only, 512 | **L1** | `20261009-mitsuba-27b-ptq1-r2` | not run | convert ok, register ok from headers; L2 = disk gap (29,098,688,419 B artifact; peak 60-90 GB); the strict verify also needs a float reference, and no Hugging Face checkpoint of these weights was downloaded (a reference computed from the GGUF itself would be the lowering grading itself) |
| Qwen/Qwen3-0.6B, deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B, HuggingFaceTB/SmolVLM-256M-Instruct | header-only | L1 | `20261009-qwen3-0.6b-l1`, `20261009-r1-distill-qwen-1.5b-l1`, `20261009-smolvlm-256m-l1` | not run | SmolVLM is text-class (partial-task) only |
| Qwen/Qwen1.5-MoE-A2.7B-Chat, deepseek-ai/DeepSeek-V2-Lite-Chat | header-only | L1 | `20261009-qwen15-moe-a2.7b-l1`, `20261009-deepseek-v2-lite-l1` | not run | convert + register ok; `SEAT_MEMORY_SHORT` (no seat tier holds the class) |
| Qwen4-exp (in-repo synthetic fixture) | header-only | L1 | `20261009-qwen4-exp-fixture-l1` | not run | synthetic weights, 64-id tokenizer |
| google/flan-t5-small, DeepSeek-V4 (in-repo fixture) | header-only | L0 | `20261009-flan-t5-small-l1`, `20261009-deepseek-v4-fixture-l1` | not run | `CLOSE_SIZE_OVER_CAP` (+ `COURT_COST_OVER_CEILING`, `QUANT_NO_DESCRIPTOR` for V4) |

Three columns, never merged: **synthetic-beacon conformance** (the five L4 rows only, synthetic facts), **dormant registration**
(registry `Candidate`; the same five rows), **full-task Active/Final** (none).

## 4. Failures routed

From `failures.json` of each run (category, code, owner):

| code | category | owner | seen at |
|---|---|---|---|
| `HF_FIT_OUT_OF_TOLERANCE` | CONFORMANCE_FAILED | A (lowering quality) / H1 (calibration policy) | Huihui-9B 8k |
| `ARTIFACT_DISK_GAP` | TEST_INFRASTRUCTURE_FAILED | H1 (disk) | Mitsuba 27B |
| `SEAT_MEMORY_SHORT` | LAYOUT_OR_RESOURCE_REFUSED | C | 9B, Mitsuba, Qwen1.5-MoE, DeepSeek-V2-Lite |
| `CLOSE_SIZE_OVER_CAP`, `COURT_COST_OVER_CEILING` | LAYOUT_OR_RESOURCE_REFUSED | C | flan-t5-small, DeepSeek-V4 fixture |
| `QUANT_NO_DESCRIPTOR` | FRONTEND_REQUIRED | A | DeepSeek-V4 fixture |
| `NOT_RUN_PIPELINE_ADMISSION` | TASK_UNSUPPORTED | A -> B | flan-t5-small |
| `DUPLICATE_DROPPED_FEE_SPENT`, `E-MODEL-UNKNOWN`, `NODELESS_BOND_REGISTRATION_ABSENT` | client / registration tooling | C1 / Lead | Qwen2.5-0.5B |
| `HUB_UNREADABLE` (HTTP 401 on the metadata read) | HF_ACCESS_FAILED | external | Mitsuba |

Fork-choice findings of the reorg probe (SmolLM2) are internal to that lane and are not described here.

## 5. Runner

```bash
bash tests/hf-onboarding/devnet.sh up            # salted t12 drill, the release's fences compressed
bash tests/hf-onboarding/devnet.sh user          # U: key, one funding send, own bond (transient registrar — a WORKAROUND, not node-less)
RUN=<id> bash tests/hf-onboarding/onboard.sh source|preflight|artifact|conformance|register|observe|resubmit|lifecycle|reorg-register|summary <model-id>
```

Stages run from a snapshot of the runner (an edit never reaches a stage in flight). The artifact cache is keyed by (revision, palw-class
sha256, context, calibration spec, build options); `CACHE=0` builds into a fresh directory (the clean-source run).
