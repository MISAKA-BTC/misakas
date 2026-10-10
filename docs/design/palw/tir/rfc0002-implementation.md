# RFC-0002 implementation ledger

Goal: implement the full [RFC02 completion contract](../../../rfc/0002-palw-tensor-ir.md#ii14-completion-contract--2026-10-10)
from the 2026-10-10 review, preserving A1–A8 and the versioned kernel boundary. **Not complete.**
Baseline: integration `3730cc90f`, plus RFC10 audit `d6a0bd54d`. Work branch: `codex/rfc02-implementation`.

Statuses below describe inspected code/evidence, not assumed results from earlier progress reports.

| Requirement | Existing evidence / current action | Completion evidence still required |
| --- | --- | --- |
| Current specification | RFC02 now separates current requirements from historical primitive-count, replay, availability and seating notes; §II.14 incorporates all six review requirements | Keep formal specification and implementation aligned as versioned features land |
| Direct canonical TIR | SDK `tests/direct_tir_registration.rs` exercises byte-based admission and provenance-independent identity | Same-binary real-checkpoint registration, independent conformance/claim/Final/redemption and all §II.11.4 mutations |
| Declarative frontend pack | Existing `model-adapter.v1` maps onto built-in ModelSpec features; it is not the general TIR frontend-pack contract | Implement bounded primitive graph/state composition, source/tensor mapping, content-addressed pack and reproducible CLI/runtime-pack flow |
| Compiler expansion bounds | Implemented cumulative subtree/UTF-8 copying, generated list allocation, pairwise work and lazy-variable recursion bounds in `adapter::expr`; nine adversarial/compatibility tests pass | Reuse the same protection in the general frontend-pack compiler; this does not complete that compiler |
| Bounded dimensions and sparse/state semantics | v1 has fixed/Hist dimensions and fixed-axis TopK; v2 stage programs exist | Versioned length commitments; efficient sparse/routing/state relations; complete evaluator/checker/court/evidence binding; long-context and boundary trials |
| Fidelity and reproducibility | Runtime pack has source/frontend/artifact checks, executor vectors and logit fidelity | Pre-run thresholds and checkpoint-scoped routing, task quality, long-context and saturation measurements; named failures rather than broad PASS |
| Performance and resource contract | TIR admission derives operation/state/cone costs; runtime and court limits are distributed | Shared complete resource vector across registration/claim/proof/court; aggregate load enforcement; real-size execution/evidence/hash/storage/delivery ratios |
| Work/economic conservation | Existing canonical work, RFC8/BUDGET and ADR176 paths must be audited at their call sites | Root/slice/rider/model-copy uniqueness, shared capital/window accounting, no splitting or excess-PWU rights gain, role-swap tests |
| Public dispute completeness | Kernel and TIR court modules exist; this alone proves no detection or end-to-end gate | Fresh outsider from public material through all relation classes, bounded terminal court, invalid-proof flood and honest liveness trials |
| Real models and coverage | Dated census reports exist; fixture/structural passes are not full-task passes | Revision/task/context/storage/arithmetic/plan records, real sizes, complete modalities via RFC03, independent Final references, A6 and A8 denominators/confidence/residuals |

The source review names dense baselines, MoE/MXFP4, GDN/KDA with unequal head counts, sparse indexers,
compressed attention/residuals, hybrid Mamba states, multimodal stages and T5/CLIP encoders. Each must
be tracked by exact checkpoint/task/context and required feature; a family name is not evidence.

Completion audit must inspect each row's actual code path, test output and real-node artifact.
Missing external checkpoints, deployment evidence or unapproved soundness profiles stay open;
they do not justify weakening the requirement or enabling a dormant consensus fence.

## Verified increment — frontend bounds and specification, 2026-10-10

**75/75 selected tests pass** using the normal Rust test stack and no environment override:

```sh
cargo test --locked -p misaka-palw-tir-lower \
  --test frontend_expansion --test adapters --test golden_lowering \
  --test architecture_report --test encdec_adapters --test vision_adapters \
  --test real_configs --test dsa_glm
```

The new tests refuse a sub-512-byte adapter requesting 2^30 expanded values, excessive strings and
object keys, cached/lazy variable amplification, oversized generated lists, quadratic unique work,
and variable/operator recursion. Small ordinary expressions keep their exact results. Dispatch is
split by operator family so the recursion guard runs before a normal worker's stack is exhausted.
The golden-lowering suite checks unchanged canonical program bytes and existing artifact programs.
Configuration, decoder/encoder/vision adapter, unknown-key/tensor, and DSA tests cover compatibility.

The first broad run found a pre-existing fixture-inventory failure: GLM-5 and GLM-5.3 were exercised
in `dsa_glm.rs` but lacked the expectations required by `real_configs.rs`. Added explicit geometry/
interleaved-RoPE and shared-index refusal assertions there; the rerun passes. These are configuration
and tiny-fixture results, not real-checkpoint, long-context or real-node completion evidence.

`rustfmt --check` for the changed evaluator and new test, `git diff --check`, and local Markdown
link-target checks pass. The existing `out_node` unused-assignment warning in `lower/cross.rs` remains.

Next implementation: the general content-addressed primitive/state frontend-pack interface, its
streaming artifact/runtime-pack/CLI integration and shared node admission/court conformance. Preserve
the remaining rows' full scope; this verified increment does not close RFC02 or its active goal.
