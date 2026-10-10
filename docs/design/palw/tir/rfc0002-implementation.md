# RFC-0002 implementation ledger

Goal: implement the full [RFC02 completion contract](../../../rfc/0002-palw-tensor-ir.md#ii14-completion-contract--2026-10-10)
from the 2026-10-10 review, preserving A1–A8 and the versioned kernel boundary. **Not complete.**
Baseline: integration `3730cc90f`, plus RFC10 audit `d6a0bd54d`; subsequently integrated
G14 prosecution/bounds and X8R/BUDGET integration through `123d8254e`. Work branch: `codex/rfc02-implementation`.

Statuses below describe inspected code/evidence, not assumed results from earlier progress reports.

| Requirement | Existing evidence / current action | Completion evidence still required |
| --- | --- | --- |
| Current specification | RFC02 now separates current requirements from historical primitive-count, replay, availability and seating notes; §II.14 incorporates all six review requirements | Keep formal specification and implementation aligned as versioned features land |
| Direct canonical TIR | SDK `tests/direct_tir_registration.rs` exercises byte-based admission and provenance-independent identity; the generic frontend now reproduces the same program/inventory/class ID through common admission | Same-binary real-checkpoint registration, independent conformance/claim/Final/redemption and all §II.11.4 mutations |
| Declarative frontend pack | Implemented content-addressed primitive/state grammar, strict config/source bindings, bounded integer/IEEE and inline virtual/tensors/blocks imports, pinned metadata, raw whole/split GGUF acquisition including public indexes, aggregate budgets, supplied unknown IDs and tokenizer identity, replayable receipts, predeclared HF-reference logit gates, staged SDK build/verify with source SHAs and all three engines, and common exact-class/beacon commit/run/verify; [format contract](tir-frontend-pack-v1.md) | Real HF-source fidelity, all advertised tasks/components and same-binary real-node §II.11.4 acceptance |
| Compiler expansion bounds | Shared work/allocation/depth budget now also protects general frontend variables/program/bindings; strict key tracking survives nested scopes; structural/constant bounds precede canonical encoding and weight reads | Maintain coverage as descriptor and versioned graph/dimension features land; existing bounds do not prove whole-node load safety |
| Bounded dimensions and sparse/state semantics | v1 has fixed/Hist dimensions and fixed-axis TopK; v2 stage programs exist | Versioned length commitments; efficient sparse/routing/state relations; complete evaluator/checker/court/evidence binding; long-context and boundary trials |
| Fidelity and reproducibility | ModelSpec runtime pack has source/frontend/artifact checks, executor vectors and logit fidelity; generic companion pins source SHA/compiler/executor revisions, reproduces canonical bytes/inventory, measures pinned HF-reference logits under predeclared revision/task/context/tolerances, and follows common beacon commitment/replay while retaining SOURCE_EQUIVALENCE_UNVERIFIED | Real HF-source provenance and checkpoint-scoped routing, task quality, long-context and runtime saturation measurements; real-node beacon evidence; named failures rather than broad PASS |
| Performance and resource contract | TIR admission derives operation/state/cone costs; SDK conformance and single-program preflight now reuse node prosecution/carrier/block bounds and choose v1/v2/segmented v4; integrated public replay and court bounds | Shared complete resource vector across registration/claim/proof/court; aggregate load enforcement; real-size execution/evidence/hash/storage/delivery ratios |
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

Remaining completion work includes the saved-format contracts beyond virtual descriptors, HF-reference/beacon integration,
versioned bounded dimensions/sparse/state relations, shared complete resource/economic accounting,
real-size fidelity/performance/aggregate-load evidence, complete modalities, public G14 and independent
Final/redemption, and A6/A8 coverage. The full RFC02 goal remains active.


## Verified increment — bounded packed descriptor composition, 2026-10-10

Generic frontend packs now embed content-addressed `quant_formats` with the existing data-only
virtual tensor grammar. Bindings explicitly map raw roles and config to a descriptor; there is no
model-family/name/registry selection. Decode uses the pinned descriptor's binary32 semantics, then
imports those exact bits using the declared integer scale/round/overflow. Receipts additionally pin
descriptor digests and raw role hashes. Compiler source pins include the decoder/descriptor sources.
The remaining saved-format contracts beyond virtual layouts stay open, including legacy tensors/
blocks layouts and parameters loaded from role JSON documents.

Descriptor preflight limits lexical/compiled expression size, nesting, tables, roles, vectors and
aggregate vector work before checkpoint reads. Shape/check expressions use headers/parameters only;
byte/shape/offset products use checked arithmetic. Config checks consume nested leaves, refusing
unread siblings and additional array elements. Streaming retains bounded pages per role, with no
whole-role cache fallback, and hashes all raw roles before/after conversion. Existing output survives
source changes or receipt mismatches. CLI stored/emitted/read byte counts expose conversion size and
I/O amplification separately; they do not prove runtime/evidence performance.

This increment also fixes the shared role reader's lane-indexed dimension bug: it previously used
lane zero's dimension for every lane, making otherwise valid descriptor output depend on chunk
boundaries. The new regression checks lane-specific expected values and exact artifact/receipt
identity at different block sizes. Existing pinned weight decodes and canonical lowering remain
unchanged in the tested corpus.

**118/118 targeted tests pass** with the normal test stack: 86 lowerer integration tests, 17 quant
unit tests, one IEEE import unit test, four direct-TIR/SDK tests and ten existing runtime-pack tests.

```sh
cargo test --locked -p misaka-palw-tir-lower \
  --test frontend_pack --test frontend_expansion --test quant_decode_pins \
  --test quant_tensors --test corpus_formats --test quant_corpus --test quantized --test golden_lowering
cargo test --locked -p misaka-palw-tir-lower --lib quantfmt
cargo test --locked -p misaka-palw-tir-lower --lib frontend_pack::stream
cargo test --locked -p misaka-palw-sdk --test direct_tir_registration --test runtime_pack
```

New coverage uses an unknown third-party nibble descriptor, MXFP4's independent decode vectors
(including expert axes), a generated safetensors checkpoint, exact Direct-TIR parameter/inventory/
class identity, peer rebuild, all three engines and court demand evaluation. Mutations cover false
vectors/headers, oversized expression/vector work, unknown roles, missing/unused descriptors, data/
lane-dependent shapes, unread nested config, source changes, unsupported saved-layout contracts and
block budgets that cannot hold the roles. These are fixture/interface checks, not real gpt-oss task
quality, public on-chain prosecution or independent Final/redemption evidence.

New frontend/SDK files pass `rustfmt --check`; `git diff --check` and edited Markdown link-target
checks pass. Existing compiler warnings remain. The full RFC02 goal is still active: HF-reference/
beacon integration, versioned dimensions/sparse/state relations, complete shared resource/economic
accounting, real-size fidelity/performance/aggregate-load measurements, complete modalities, fresh
outsider G14 and independent Final/redemption, and A6/A8 coverage are still required.

## Verified increment — bounded saved-format interpreter APIs, 2026-10-10

The `tensors` and `blocks` interpreters now prepare opaque, descriptor-pinned stream plans and
decode ranges of at most 1024 lanes through individual byte reads (at most 8 bytes per stored
value/field). Tensor decode preserves the original group-index, group-scale/zero/min and element
evaluation coordinates, including act-order and ragged groups. Block decode retains global row,
column, block and group coordinates across arbitrary range boundaries. No full code, group-index
or scale array is materialized by these APIs.

Plans pin role headers and refuse changed shape/dtype/presence or another descriptor before reads.
Tensor metadata expressions refuse lane dependencies; raw role metadata used for shape/checks
(compressed-tensors' I64 shape) and JSON role constants are snapshotted once with a 64 KiB total
cap. Metadata must already be loaded exactly by the caller. Range decode uses that snapshot,
including for metadata referenced in output expressions. The API does not claim to implement
bounded checkpoint metadata acquisition or source-change receipt checks for these layouts yet.
Field bounds, field offsets and row byte counts use checked arithmetic. Invalid ranges, short
reads, invalid groups/codes/zeros and non-finite decoded binary32 values receive explicit errors.
Compiler source pins include the new nested interpreter sources.

The new range tests cover all **31 block descriptors / 186 vectors** and **9 tensor descriptors /
40 vectors**, comparing resident decode bit patterns at chunk sizes 1, 7, 31 and 1024. Registry
loading also checks those independent pinned vectors (with its existing signed-zero convention).
Additional unknown descriptors exercise every global block coordinate, ragged/reordered group
coordinates, metadata snapshots, cross-format/header substitutions and refusal paths. A header-only
3 × 2^40 tensor test decodes one range without allocating its whole weight or group arrays.

**117/117 targeted tests pass**: 86 lowerer integration tests, 17 quant unit tests and 14 SDK tests.

```sh
cargo test --locked -p misaka-palw-tir-lower \
  --test quant_streamed_ranges --test frontend_pack --test quant_decode_pins --test quant_tensors \
  --test corpus_formats --test quant_corpus --test quantized --test golden_lowering
cargo test --locked -p misaka-palw-tir-lower --lib quantfmt
cargo test --locked -p misaka-palw-sdk --test direct_tir_registration --test runtime_pack
```

New interpreter/test files and the modified frontend source pin pass `rustfmt --check`;
`git diff --check` passes. Existing compiler warnings remain.

This increment adds interpreter APIs. Frontend/SDK tensors/blocks bindings, bounded metadata reads
and receipts remain open; existing generic descriptor bindings still accept virtual layouts only.
Real checkpoint fidelity, live-node permissionless admission/prosecution/Final, resource/economic
accounting and all remaining completion requirements in this ledger remain open. RFC02 is not complete.

## Verified increment — saved-format frontend/SDK bindings, 2026-10-10

Inline `tensors` and `blocks` descriptors now use the same generic frontend, raw source binding,
canonical artifact writer and SDK build/verify path as virtual descriptors. Formats are explicitly
selected by a local pack key and pinned digest, never a registry/model name. Block imports accept
an exact byte container or a logical-shaped opaque raw `TensorSource`; raw dtype labels confer no
priority. Tensor decoding preserves group-index/group/element semantics and supports bounded
I64 shape metadata and exact-number JSON role parameters, including nested bitsandbytes offsets.
Unaligned block fields can cross page boundaries without widening a source read past its budget.

Every binding header, required parameter instance, source inventory and ordinary TIR admission
passes before metadata acquisition. Metadata snapshots are capped at 64 KiB per plan and 64 MiB
across parameter instances; metadata read calls at 4M. The aggregate reservation happens before
any metadata read. Strict JSON path checks consume parameter leaves and explicit `metadata_inert`
paths only; ambiguous/unknown paths, siblings, extra array elements and deep documents refuse.
Raw metadata pins are checked at artifact conversion, closing the stale-compiled-metadata gap.
Sources are still hashed before/after conversion, and failures preserve previous output.
`compile_bounded` and both CLI/SDK callers use the chosen read budget across both phases; operational
read counters include metadata while artifact/receipt identity remains independent of chunk size.

Descriptor preflight now compiles all three layouts once, checks their vector geometry/work and
then runs the pinned self-vectors. Lexical expression tokens are bounded at 256 (with the existing
1024-byte/32-nesting and compiled 256-node/32-depth bounds), covering IQ1_M's stored decode formula.
AWQ and compressed-tensors descriptors explicitly check known packing/activation/group conditions;
calibration observer metadata is explicitly inert. Nested inert paths track the appropriate root
without consuming unknown sibling leaves. New descriptor digests intentionally record these
contracts; existing tested decoded weights and canonical lowering bytes stay identical.

**128/128 targeted tests pass**: 94 lowerer integration tests, 17 quant unit tests and 17 SDK tests.

```sh
cargo test --locked -p misaka-palw-tir-lower \
  --test frontend_pack --test quant_streamed_ranges --test quant_tensors --test quantized \
  --test golden_lowering --test corpus_formats --test quant_corpus --test quant_decode_pins
cargo test --locked -p misaka-palw-tir-lower --lib quantfmt
cargo test --locked -p misaka-palw-sdk --test direct_tir_registration --test runtime_pack
```

All 31 block and 9 tensor descriptors pass frontend vector preflight. Their first pinned vectors
import through the generic frontend, match explicitly rounded integer weights and run on all three
engines plus court-demand evaluation. The separate range suite retains all 226 vector comparisons.
Unknown tensor/block/JSON formats build from generated safetensors, rebuild on a peer, execute SDK
CLIs, reproduce Direct-TIR inventory/class identity and pass testnet-12 offline admission. The
generic frontend CLI separately rebuilds the bitsandbytes JSON fixture with the chosen byte budget.
These are storage/interface fixtures, not actual model task fidelity or a public node conviction.

Mutation coverage includes metadata/source changes, invalid source bytes, oversized metadata,
unknown/inert JSON roles, unread nested paths, unsupported AWQ packing and activation quantization.
A 1,200-binding graph requiring more than 64 MiB of metadata is refused before a source read.
Changed source/compiler/fidelity/conformance claims remain explicit SDK refusals. Formatter checks
on the changed frontend/interpreter/SDK files, `git diff --check` and local Markdown targets pass.
Existing compiler warnings remain.

The goal remains active and not complete. Native GGUF-container acquisition in the generic CLI/SDK,
HF-reference/beacon runtime-pack integration, versioned dimensions/sparse/state relations, complete
resource/economic accounting, real-size fidelity/performance/aggregate-load evidence, complete
modalities, public G14 and independent Final/redemption, and the full A6/A8 requirements remain open.

## Verified increment — generic native GGUF acquisition, 2026-10-10

`FrontendSource` now acquires a single GGUF without ModelSpec/family dispatch and exposes raw
names/bytes with reversed GGML dimensions as row-major shapes. A public descriptor supplies
unknown packed-type geometry; the binding's pinned local key still selects the decoder. Scalar
IEEE and integer types use the existing explicit imports, preserving I64 values beyond 2^53.
Native metadata joins an optional config sidecar, with duplicate/unread keys refused and exact
integer/finite float conversion. Embedded tokenizer metadata receives a canonical representation
ID when no external tokenizer file is chosen; this binds identity, not tokenizer/task fidelity.

Parsed header and cumulative logical allocation budgets are 64 MiB each, with 65,536 tensor/key
counts, rank four and alignment 64 KiB. Arithmetic, duplicate/overlap and truncation checks precede
weight acquisition. Unknown-type span lookup is now O(log N) per tensor instead of scanning the
entire table. The writer revalidates the header before reading and before replacing its output,
including a mutation during payload reads. The CLI and SDK share this acquisition contract.

The SDK pins the container plus present config/tokenizer sidecars and accepts an explicit GGUF
file or an unambiguous directory. Artifact build/verify uses a temporary until receipt, inventory,
all three engines and final source SHAs pass. Late conformance failure preserves the prior output;
source/frontend/pack output conflicts include Unix symlinks and hard links.

**82 distinct targeted tests pass** with the normal Rust test stack. The final frontend/SDK rerun
is **51/51** (33 frontend, eight Direct-TIR/SDK, ten existing runtime-pack). The same increment's
GGUF/metadata/RoPE, golden-lowering and streamed-range regressions are **31/31**:

```sh
cargo test --locked -p misaka-palw-tir-lower --test frontend_pack
cargo test --locked -p misaka-palw-tir-lower \
  --test gguf --test gguf_unmodelled --test gguf_rope_freqs \
  --test golden_lowering --test quant_streamed_ranges
cargo test --locked -p misaka-palw-sdk --test direct_tir_registration --test runtime_pack
```

Native acquisition fixtures cover all 31 block formats plus an unpublished architecture and
unknown GGML type, bounded reads at 8/127 bytes, three-way execution, court demand evaluation,
common inventory/class identity, peer rebuild and both CLIs. A binary64 value immediately below
0.5 rounds differently under exact IEEE import and an explicitly requested binary32 descriptor;
the storage label grants neither arithmetic priority. Further mutations refuse false type
declarations, giant headers/arrays, extent/byte/alignment overflow, zero dimensions, duplicate or
overlapping tensors, truncated data, non-finite metadata, changed headers/SHAs/tokenizers and
late conformance mismatches. These are interface/fixture results, not real model or node passes.
`cargo clippy --locked -p misaka-palw-tir-lower -p misaka-palw-sdk --lib` completes with existing
warnings; the new acquisition/publisher modules produce none. Formatter checks for the frontend,
CLI, SDK and changed test files, `git diff --check` and local Markdown targets pass.

RFC02 remains **not complete**. Split GGUF acquisition, HF-reference/beacon integration,
versioned length/sparse/state extensions, complete shared resource/economic accounting, real-size
fidelity/performance/load measurements, complete RFC03 tasks, fresh-outsider G14 and independent
Final/redemption, and A6/A8 coverage remain required by the original completion contract.


## Verified increment — split GGUF acquisition, 2026-10-10

The generic frontend and SDK now acquire a split checkpoint through any standard part, an
unambiguous directory, or a strict public `*.gguf.index.json` listing arbitrary local basenames.
Part zero may hold metadata without tensors; index order and the selected entry part grant no
priority. Every part must agree on the declared count and total tensors, use contiguous unique
part numbers and the same container version, and contribute globally unique tensor names.
Later model/tokenizer metadata must repeat the primary value exactly or be absent. Only the
three verified split transport fields are excluded from model configuration; sidecars cannot
inject them. The same values/configuration produce identical artifacts, receipts and tokenizer
identities whether stored whole or split.

The native reader uses shared 64 MiB header and logical allocation budgets and aggregate 65,536
metadata/tensor counts across the entire set. No part resets these budgets. Public indexes are
bounded to 2 MiB and 1024 distinct local basenames; traversal, duplicate names and unknown fields
refuse. Snapshot validation rechecks every part and the index before output publication. The SDK
pins the index, every part and present sidecars, including unknown descriptor-supplied GGML IDs.
A changed payload or reordered source index fails source-SHA verification without replacing the
prior published artifact. Output conflict guards cover every part.

**86 distinct targeted tests pass**: 36 generic frontend, nine Direct-TIR/SDK, ten existing
runtime-pack and 31 GGUF/metadata/RoPE/golden-lowering/streamed-range regressions. Commands are
unchanged from the preceding increment. The new SDK fixture reconstructs an arbitrary-name,
reordered three-part set with a metadata-only first part, exact I64 values above 2^53 and an
unknown self-tested packed type. Directory/index rebuilds, both CLIs, three-engine conformance,
common inventory and offline class admission pass. Refusal fixtures exercise missing/mismatched
parts, hidden metadata, duplicate global tensors, unsafe/oversized indexes and exhaustion of
shared header/allocation/count budgets. These are fixture/interface results, not real-checkpoint
or real-node Final evidence.

`cargo clippy --locked -p misaka-palw-tir-lower -p misaka-palw-sdk --lib`, formatter checks for the
changed frontend/SDK files, `git diff --check` and local Markdown target checks pass. Existing
clippy warnings remain; the new split acquisition and modified publisher produce none.

RFC02 remains **not complete**. HF-reference/beacon integration, versioned length/sparse/state
extensions, complete shared resource/economic accounting, real-size fidelity/performance/load
measurements, complete RFC03 tasks, fresh-outsider G14 and independent Final/redemption, and
A6/A8 coverage remain required by the original completion contract.


## Verified increment — generic fidelity and shared beacon tools, 2026-10-10

Independent frontend companions now bind an existing class and use the same artifact/layout,
static kernel admission, post-commit work beacon, selected checks and fresh evidence verifier
as a ModelSpec pack. Format adapters supply provenance only; no ModelSpec, model name or feature
registry is synthesized. An independent integer-import recipe has a typed absent calibration ID.
The compiler/executors/profile/frontend pins and exact class inputs are checked; a changed recipe,
raw-source SHA, reference/policy, implementation, vector or layout invalidates the commitment.
Explicit plan positions now refuse zero and values beyond class context/program history.

`attach-frontend-fidelity` adds public reference logits and a versioned policy to a **new**
companion, after its revision/task/context/units/thresholds are fixed and validated before the
measurement. It pins reference metadata/payload, coverage, policy and portable libm-v1 fit.
The source rebuild independently repeats the fit before artifact publication. Named gates are
slope, correlation, top-1, KL, maximum absolute error, RMSE and import-time weight saturation.
No pass flag certifies source identity, routing, runtime state saturation or full-task quality.
The reference provider's label is informational; generated fixture logits are not real HF evidence.

The shared HF reader now bounds acquisition, rejects malformed tokens/ragged rows, unsafe paths,
zero vocabulary, overflow, nonfinite values, truncated floats and unread payload tails. Portable
fit arithmetic is independent of integer execution; `libm` uses its already-locked 0.2.8 release.
These local producer limits do not close long-context streamed fidelity or consensus resources.

**74 targeted tests pass**: 36 generic frontend, ten Direct-TIR/SDK, ten existing ModelSpec
runtime-pack and 18 common beacon regressions. Independent safetensors and arbitrary-name split
GGUF fixtures attach reference data, rebuild from a peer, bind exact class identity, commit before
synthetic randomness, interrupt/resume the shared checks and reproduce evidence in a fresh CLI
process. Source/reference/policy/compiler/implementation/layout/vector substitutions, fake fidelity
flags/measurements and forged beacon results refuse. A missing class layout refuses before a
commitment; a different program and failed coverage/tolerance create no output pack. No-rerun
remains not a pass, and failed fidelity rebuilds preserve the prior published artifact.

```sh
cargo test --locked -p misaka-palw-tir-lower --test frontend_pack
cargo test --locked -p misaka-palw-sdk \
  --test direct_tir_registration --test runtime_pack --test runtime_pack_beacon
cargo clippy --locked -p misaka-palw-sdk --lib
```

Formatter, local Markdown target and diff checks pass. Existing unrelated compiler/clippy warnings
remain; changed recipe/fidelity/binding modules introduce none. These tests use deliberately
synthetic reference logits and beacon facts, unapproved challenge policy and hypothetically armed
kernels. They prove the tool interfaces, not real-checkpoint fidelity or node eligibility.

RFC02 remains **not complete**. Required evidence still includes real checkpoints and long-context
fidelity/routing/state/task quality; versioned bounded dimensions/sparse/state relations; complete
shared execution/proof/verifier/court/retention/delivery and aggregate-load resource accounting;
work/economic conservation; complete RFC03 modalities; fresh-outsider G14 and independent
Final/redemption; and A6/A8 measured coverage. The full original completion contract remains active.


## Integration and node-binding correction — 2026-10-10

Integrated the committed G14 prosecution/bounds branch through `123d8254e`, retaining the independent
frontend/fidelity/source-acquisition work. This imports segmented commitment-only courts and bounded
public replay, complete fixed-length generative claim binding, public legacy pursuit/answer history,
X8R root/slice execution, frozen beacon source sets, verifier fees and BUDGET safety caps. Their
consensus fences remain dormant; this merge does not certify release readiness or activate them.

Inspection found an actual tool/node mismatch: pack conformance used the artifact `graph_ir_root`,
while onboarding tag 107 compares the **kernel** program root. The shared ModelSpec/independent
companion binder now computes that kernel-domain root from canonical program bytes and derives the
same kernel plan. Artifact inventory, graph root and class identity retain their existing domains.
A stale commitment is refused and must be recomputed, not relabeled or reused after a beacon.

Census preflight and the conformance binder now select the oldest of v1/v2/segmented v4 that satisfies
semantic admission **and** the node's actual public-prosecution, carrier and per-block court limits.
They share `class_prosecution_bounds_v1` and the consensus-core policy/limits rather than local relaxed
numbers. Requested positions are not silently clamped. v4's separate OPV admission/economic requirement
is visible; public availability, activation and approved soundness remain independent chain facts.
v3/v5 input semantics are not substituted with static artifact parameters. Pipeline resource reporting,
encoder companions and the full shared execution/proof/retention/delivery vector remain unfinished.

**282 distinct selected tests pass**: 122 SDK unit tests, 40 SDK integration
checks (Direct-TIR, runtime packs, beacon conformance and sparse attention), 53 kernel tests,
50 core integration tests, 15 node segmented-court tests and two launch/release A-2 fence tests. The final exact-context binder
rerun passes all 28 Direct-TIR/beacon tests; it is a repeat, not additional coverage. `cargo check
--locked -p kaspad` passes with existing warnings. The node suite finishes in 814.49 s in the normal
debug test profile, including the 8,192-position/window-1024 producer commitment and cross-segment
continuity conviction, wrong-length claim refusal, outside-bond conviction, restart/reorg/pruned
import, spam liveness and v5 encoder conviction plus honest Final. It is fixture/node-fold evidence,
not real-checkpoint throughput or a shipping-network activation certificate.

```sh
cargo test --locked -p misaka-palw-sdk --lib
cargo test --locked -p misaka-palw-sdk \
  --test direct_tir_registration --test runtime_pack --test runtime_pack_beacon --test coverage_p2_dsa
cargo test --locked -p misaka-palw-kernel \
  --test k2_ledger --test k2_ledger_pipeline --test k2_real_scale \
  --test g14_reexecution --test g14_review_regressions --test verifier_pay
cargo test --locked -p kaspa-consensus-core \
  --test palw_bond_budget_e2e --test palw_exec_payload_v2_fence \
  --test lg14a_legacy_dispute_fold --test rfc0010_production_fold
cargo test --locked -p kaspa-consensus --lib t12_a2u_mixed_verdicts -- --test-threads=1
cargo test --locked -p kaspa-consensus --lib g14_k2s_ -- --test-threads=1
cargo check --locked -p kaspad
```

Tests using generated fixtures, config-only 9B geometry or hypothetically armed routes are not
real-checkpoint or shipping node activation evidence. Edited-source formatter, local Markdown
target and source diff checks pass. Imported historical raw logs retain their original formatting
(including trailing whitespace); their old results are not substituted for the runs above.

The full RFC02 goal, §II.11.4 real-node acceptance and all original remaining requirements in this ledger remain active; RFC02 is **not complete**.
