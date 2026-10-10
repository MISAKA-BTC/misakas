# RFC-0002 implementation ledger

Goal: implement the full [RFC02 completion contract](../../../rfc/0002-palw-tensor-ir.md#ii14-completion-contract--2026-10-10)
from the 2026-10-10 review, preserving A1–A8 and the versioned kernel boundary. **Not complete.**
Baseline: integration `3730cc90f`, plus RFC10 audit `d6a0bd54d`. Work branch: `codex/rfc02-implementation`.

Statuses below describe inspected code/evidence, not assumed results from earlier progress reports.

| Requirement | Existing evidence / current action | Completion evidence still required |
| --- | --- | --- |
| Current specification | RFC02 now separates current requirements from historical primitive-count, replay, availability and seating notes; §II.14 incorporates all six review requirements | Keep formal specification and implementation aligned as versioned features land |
| Direct canonical TIR | SDK `tests/direct_tir_registration.rs` exercises byte-based admission and provenance-independent identity; the generic frontend now reproduces the same program/inventory/class ID through common admission | Same-binary real-checkpoint registration, independent conformance/claim/Final/redemption and all §II.11.4 mutations |
| Declarative frontend pack | Implemented content-addressed primitive/state grammar, strict config/source bindings, bounded streaming imports, replayable receipts and SDK companion build/verify with source SHAs and all three engines; [format contract](tir-frontend-pack-v1.md) | General quant-descriptor composition, HF-reference/beacon runtime-pack integration, all advertised tasks/components and real-node §II.11.4 acceptance |
| Compiler expansion bounds | Shared work/allocation/depth budget now also protects general frontend variables/program/bindings; strict key tracking survives nested scopes; structural/constant bounds precede canonical encoding and weight reads | Maintain coverage as descriptor and versioned graph/dimension features land; existing bounds do not prove whole-node load safety |
| Bounded dimensions and sparse/state semantics | v1 has fixed/Hist dimensions and fixed-axis TopK; v2 stage programs exist | Versioned length commitments; efficient sparse/routing/state relations; complete evaluator/checker/court/evidence binding; long-context and boundary trials |
| Fidelity and reproducibility | ModelSpec runtime pack has source/frontend/artifact checks, executor vectors and logit fidelity; generic companion pins source SHA/compiler/executor revisions and reproduces canonical bytes/inventory while reporting SOURCE_EQUIVALENCE_UNVERIFIED | Pre-run thresholds and checkpoint-scoped routing, task quality, long-context and saturation measurements; generic frontend HF-reference/beacon evidence; named failures rather than broad PASS |
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

`rustfmt --check`, `git diff --check`, and local Markdown link-target checks pass. The existing
`out_node` unused-assignment warning in `lower/cross.rs` remains.

## Verified increment — generic frontend and common SDK, 2026-10-10

Added the [declarative TIR frontend pack](tir-frontend-pack-v1.md), the `palw-tir-frontend` compiler,
and SDK `pack build-frontend` / `verify-frontend`. The primitive/state graph has no ModelSpec or
model-family dispatch. Header preflight binds every required parameter instance and refuses unread
config/tensors, ambiguous bindings and malformed byte/shape declarations before weight reads.
Integer imports preserve I64 bits; finite IEEE imports use explicit integer scaling/rounding and
reject or record saturation. Stored/encoded streaming buffers are each bounded by the requested
block budget. Rebuild mismatch preserves the prior output, and CLI/SDK output guards protect inputs.

The companion pack pins source SHA-256s/revision, compiler/executor source revisions, the fixed
compiler admission profile, build receipt, common streamed inventory and public conformance
vectors. All three implementations are mandatory. A verification report can be constructed only
after its named checks pass. It continues to report source equivalence, full task and live Final
as unverified; this is not the completed HF-reference/beacon runtime-pack contract.

**99/99 targeted tests pass** (85 selected lowerer tests, one IEEE unit test, three direct-TIR/SDK
tests and ten existing runtime-pack tests), with the ordinary test stack and no environment override:

```sh
cargo test --locked -p misaka-palw-tir-lower \
  --test frontend_pack --test frontend_expansion --test adapters --test golden_lowering
cargo test --locked -p misaka-palw-tir-lower \
  --test architecture_report --test encdec_adapters --test vision_adapters --test real_configs --test dsa_glm
cargo test --locked -p misaka-palw-tir-lower --lib frontend_pack::stream
cargo test --locked -p misaka-palw-sdk --test direct_tir_registration --test runtime_pack
```

The new fixture contains a static integer combination with per-layer I64 multipliers, fixed state
and windowed history. It produces the direct compiler's exact canonical bytes, runs 32 positions
on reference/ref2/node backend, and reproduces every node through the court demand evaluator at
positions 0/1/15/16/31 and checkpoint intervals 4/16. This is evaluator coverage, not an on-chain
conviction. The separate SDK fixture rebuilds from public source, derives the same inventory and
class ID as direct TIR and passes testnet-12 offline class admission; both new CLIs execute in tests.
Mutation checks refuse compiler expansion, unknown operations/attributes/dimensions/primitive sets,
hidden root/nested config, missing/extra/repeated tensors/bindings, false shapes/bytes, non-finite
or out-of-range conversion, source/frontend/profile/implementation/vector changes and oversized
conformance jobs. All 25 primitive attribute variants round-trip; this does not claim a new execution
implementation for them. Existing canonical lowering and ModelSpec pack checks still pass.

Remaining completion work includes general descriptor composition, HF-reference/beacon integration,
versioned bounded dimensions/sparse/state relations, shared complete resource/economic accounting,
real-size fidelity/performance/aggregate-load evidence, complete modalities, public G14 and independent
Final/redemption, and A6/A8 coverage. The full RFC02 goal remains active.
