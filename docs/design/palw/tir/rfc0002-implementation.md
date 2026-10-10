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


## Producer commitment streaming — 2026-10-10

The long-context node run above exposed producer commitment generation as a live hot path.
v3 Merkle leaf construction now reads contiguous rows/strided columns directly and encodes their
unchanged dtype-width little-endian preimage through a fixed **4 KiB** buffer. It no longer allocates
an index vector plus an i128 value vector for every leaf (up to 96 KiB for 4096 values). Existing
leaf headers, domains, widths, row/column order, odd-node tree rules, roots, openings and descriptor
identities are unchanged. Opening APIs still materialize their requested payloads as before.

An independent oracle retains the original scalar updates and index selection. All frozen dtypes,
empty/scalar/batched/zero/strided/ragged shapes, byte-buffer and tile boundaries, complete dual roots,
serialized leaf openings and range proofs match it exactly. The producer benchmark fixes its
geometries/criteria before measurement: identical roots; no case with >10% median regression;
history and wide-logits each with >=10% median speedup. It alternates old/new order over nine trials,
eight commitments per trial (1024 for short rows), with the old allocation/lifetime/root clones
retained. On this host's normal debug profile it passes:

| Synthetic tensor | Old median batch (ms) | New median batch (ms) | New / old |
| --- | ---: | ---: | ---: |
| I16 history, 1024 × 16 | 14.193 | 11.260 | 0.7934 |
| I32 logits, 131072 | 69.743 | 39.106 | 0.5607 |
| I8 strided batch, 2 × 3 × 257 × 16 | 21.602 | 16.996 | 0.7868 |
| I128 accumulator, 3 × 4097 | 29.515 | 25.414 | 0.8611 |
| I16 short row, 1 × 8 | 7.239 | 6.535 | 0.9027 |

These are local producer microbenchmarks, not checkpoint execution/proof/hash/retention/delivery
ratios, a release-performance gate, or economics evidence. This does not solve history commitment
reuse, prepared proof paths, full shared resources, real models, complete tasks or G14 deployment.

**59 functional tests and the one manually invoked benchmark pass**: five Merkle tests (including
all existing leaf/range/overflow cases), seven element-court tests, six adversarial tests, 13
segmented public-replay/DA/court tests and 28 independent/ModelSpec SDK beacon regressions. The
fresh verifier still convicts changed routing/history/output, dismisses honest elements, honors
priced filing/carrier bounds and reconstructs identical class/artifact roots. The config-only
9B-8k gate still passes its existing per-prosecution bounds; it remains geometry evidence.

```sh
cargo test --locked -p misaka-palw-kernel --lib merkle3
cargo test --locked -p misaka-palw-kernel --lib element::tests
cargo test --locked -p misaka-palw-kernel --test k2_real_scale --test k2_adversarial
cargo test --locked -p misaka-palw-kernel --lib producer_hash_cost_against_original_scalar_updates \
  -- --ignored --nocapture
cargo test --locked -p misaka-palw-sdk --test direct_tir_registration --test runtime_pack_beacon
cargo clippy --locked -p misaka-palw-kernel --lib
```

Clippy, changed-source formatter, source diff and local Markdown targets pass; six pre-existing
kernel clippy warnings remain. The 15 node-fold cases above were run on `07b76fe2c` before this
hash optimization. They are not claimed as an additional node run of the optimized producer;
compatibility here rests on the original-preimage oracle and the listed court/SDK regressions.

The original full RFC02 goal remains active and **not complete**.


## Verified increment — shared claim admission and proof work, 2026-10-10

The dormant `palw_probabilistic_constraints_v1` route previously counted structural claim
acceptance as an adjudication with zero work. It now derives a tariff from the registered
`max_commit_bytes`, the existing 64 KiB carrier allowance and the public class schema: 16 work
units per bounded byte. Single-program schemas price every declared artifact instance (including
unused instances), independently of the registrant's commitment subset. Pipeline and typed
schemas include their stage/component records. The calculation uses checked arithmetic and a
counting Borsh writer; no temporary schema/trace serialization is required to count the tariff.
This tariff is deterministic chain accounting, not a claim about measured CPU instruction cost.

All four claim kinds, including salted reveals, check the class carrier envelope before trace
copies; seal readiness and OPV capacity precede charged structure work. Duplicate/unready
reveals remain free. Ready malformed traces spend work without changing rooted state. A refused
budget charge spends nothing and retains the seal for retry. Registration checks both admission
fit and worst-court fit in the reserved proof-work share. `BlockBudgetV1::charged_v1` is also the
node onboarding fold's transition, so persisted blue-score-scoped work cannot be bypassed by
alternating onboarding and kernel objects. Both the work sum and the run sum reject overflow.
The scratch field/row retains its existing `court_work` name and encoding; plan bytes, descriptor
semantics, work credit and state-root grammar are unchanged. Consensus presets/fences remain
unarmed: this is a pre-activation revision of the fenced kernel route, not a live fork.

SDK program admission uses the same program tariff and reserved proof-work ceiling. Pipeline
SDK reporting still needs its complete node resource gate; no new claim of full pipeline
preflight equivalence is made. The whole resource vector, graph-dependent semantic validation
costs, producer/evidence budgets and network-wide retained-state accounting remain incomplete.
A zero-reserve policy explicitly offers only a full-block court ceiling; it provides no reserved
work guarantee. Production policy retains its 500-permille reserve. Two existing isolated
full-block court/pricing tests explicitly select zero reserve; reserve liveness is tested by the
new aggregate scenario rather than by weakening the production limit.

New adversarial checks exercise two simultaneously sealed jobs, refusal before the run ceiling,
a fresh replaying outsider's conviction from the remaining work, the honest claim's next-block
retry, a ready wrong trace charged without state change, a class-oversized trace inside the global
route envelope, impossible admission/proof-reserve registration, and `u64::MAX` work exhaustion.
These are generated fixtures, not real-checkpoint performance or shipping activation evidence.


Validation: **173 distinct tests pass** (123 kernel, 28 SDK, 8 consensus-core route tests,
14 consensus node-path tests), plus `cargo check --locked -p kaspad`. The extra overflow rerun
is a repeat, not a 174th test. The C4 suite retains one existing ignored test. Final source
formatting/diff checks pass. The completed SDK/node test executables were checked for Cargo
freshness without changing their SHA-256, after the final source edits.

```sh
cargo test --locked -p misaka-palw-kernel --test k2_ledger_route --test k2_ledger \
  --test k2_ledger_pipeline --test k2_real_scale --test typed_roots --test c4r4 \
  --test k2_rows --test k2_opv --test k2_opv_pipeline
cargo test --locked -p misaka-palw-sdk --test direct_tir_registration --test runtime_pack_beacon
cargo test --locked -p kaspa-consensus-core --lib palw_kernel_route
cargo test --locked -p kaspa-consensus --lib g14_k2s_ -- --test-threads=1 \
  --skip g14_k2s_a_history_bearing_class_held_at_8192_positions_convicts_a_continuity_lie_across_segments
cargo check --locked -p kaspad
```

Logs: `/tmp/rfc02-admission-final-all.log`, `/tmp/rfc02-admission-overflow-final.log`,
`/tmp/rfc02-admission-sdk-final.log`, `/tmp/rfc02-admission-core-final.log`,
`/tmp/rfc02-admission-node-courts.log`, `/tmp/rfc02-admission-node-check-final.log`.
The 8192-position history scenario was **not rerun in this increment**; its earlier pass on
`07b76fe2c` does not establish that scope on the new revision. Node-path evidence uses test
activation/public-artifact hooks and generated weights, not shipping activation or a real
checkpoint. Public economic admission, full modalities and A6/A8 coverage remain open.

A separate baseline integration issue was found while checking formatting: gateway
`src/main.rs`'s scoped candidate spawn is missing the closing map closure, so repository-wide
formatting cannot parse that unchanged file. It needs repair/build verification before gateway
public-task drills. Unrelated formatter edits were discarded; no gateway/source changes are
included in this resource increment.

## Verified increment — coherent public onboarding reads and gateway build, 2026-10-10

`misaka model onboard verify` now consumes one op-211 row snapshot rather than combining
op-231 attempt/program reads with later op-212 Finals and op-211 seals. The SDK checks fixed
tip DAA, roots, header and declared count across pages, strictly advancing ordered rows/cursors,
exact completeness and a 128 MiB retained wire-byte ceiling. It reconstructs both served roots,
then derives the attempt, evidence, bound kernel program, attributed Finals and v3 source seals
from that same state. The requested candidate, descriptor, program, plan and parameter link
must agree before fresh verification. Public inventory roots and kernel parameter roots remain
separate domains. No model-name/frontend allowlist is introduced.

The earlier gateway parser failure is repaired: the scoped candidate spawn closes its map
closure. Its harness uses the current `ClaimBudget` API and `serve_connection` arguments.
Validation: five new SDK snapshot tests pass, covering page drift, repeated/false cursors,
duplicate/omitted rows, oversized declarations, incomplete material, root mutation and
self-consistent substituted bindings. The gateway binary suite passes 162 tests with one
existing ignored test. SDK and `misaka` CLI compilation pass with existing warnings.

```sh
cargo test --locked -p misaka-palw-sdk --test onboarding_snapshot
cargo test --locked -p misaka-palw-gateway --bin misaka-palw-gateway
cargo check --locked -p misaka-palw-sdk
cargo check --locked -p misaka-cli --bin misaka
```

This is generated-weight snapshot/harness evidence, not a live-node or real-checkpoint PASS.
The served roots still need caller node/state-proof trust. The byte ceiling does not bound
total verifier RAM. Strict tip matching fails closed when a node advances between pages;
pinned/scalable public snapshots remain open. The runtime pack's v1 file-facts runner remains
separate from the actual chain's attributed/sealed-source policies, and has no RPC adapter;
its stale claim that no public node reads exist is corrected. Complete-check fresh replay,
real-checkpoint fidelity/performance, permissionless public conviction and independent
Final/redemption remain open; RFC02 is not complete and no dormant fence is activated.


## Verified increment — complete-check public replay and first pinned real-weight baseline, 2026-10-10

The same op-211 snapshot now dispatches by the committed policy: sampled reads retain their
beacon verifier, while complete-check reads carry the bound plan's position ceiling and kernel
parameter root into exhaustive replay. `misaka model onboard verify --artifact` reaches the
complete-check verifier. The SDK checks the chain's enumerable-domain limits before collecting
tensor bytes, requires the file's exact program, and authenticates its weights against the
registered inventory. A substituted passing post, program or same-labelled different weights
is refused/contradicted. Snapshot binding also checks the kernel link's challenge policy.

Validation: seven snapshot tests, two new direct-TIR CLI tests, one existing detached-signing
unit test, and the updated node complete-check test pass (11 distinct tests in this increment).
The node test uses the actual acceptance fold, IBD and paged op-211 RPC response builders/wire
forms with the SDK snapshot dispatch, for an honest and forged complete-check post. It still
uses generated weights and test activation, not shipping activation or a live RPC socket.
`cargo check --locked -p misaka-cli --bin misaka` passes with existing warnings.

```sh
cargo test --locked -p misaka-palw-sdk --test onboarding_snapshot --test kernel_preflight_cli
cargo test --locked -p misaka-palw-sdk --lib onboarding_chain
cargo test --locked -p kaspa-consensus --lib \
  g14_canonical_a_fresh_node_reverifies_complete_checks_from_rpc_reads_and_its_own_artifact -- --test-threads=1
cargo check --locked -p misaka-cli --bin misaka
```

`palw-class kernel-preflight <program.tir> --positions N --json` is an offline direct canonical
TIR entry: it needs no model configuration, model name, official frontend or weights. It reports
shipping/hypothetical kernel outcomes using the node's same prosecution/carrier/block limits,
at exactly N (no silent clamp). The tests refuse zero, oversized/noncanonical inputs and an
excessive context. This command grants neither registration nor economic eligibility.

The local Qwen2.5-1.5B-Instruct checkpoint's complete model.safetensors, config.json and
tokenizer.json match the official repository revision `989aa7980e4cf806f80c7fef2b1adb7bc71aa306`:
LFS SHA-256 for weights and Git blob SHA-1 for config/tokenizer, plus local SHA-256 for all three.
Missing generation/tokenizer configuration and other repository files remain listed. The files'
SHA-256 values were checked unchanged again after both measurements. The generic data adapter
lowered all real weights into a 14,011-byte TIR program and 1,835,516,100 bytes of tensor payload
(1,835,572,288-byte PALWTIR1 file); the original assets were not modified.

The predeclared short pilot uses four authored calibration sequences (121 measured positions)
and four distinct evaluation sequences (123 positions), at most 32 tokens/window, `libm-v1`
and headroom16=2.0. Its thresholds were top-1 >= 0.95, mean KL <= 0.01 and absolute perplexity
delta <= 0.02. Actual results: top-1 0.9674797, mean KL 0.00136794, perplexity delta -0.00434252.
The first run took 45.35 s in the release build. A second run reused the calibration, persisted
the same artifact and reproduced every metric; one full position's logits matched the independent
integer reference byte for byte (54.03 s total). These are measured real-weight diagnostics
against the native f32 HL evaluator, **not** an independent Hugging Face reference, long-context
coverage, instruction/translation/code quality, public conviction or Final/redemption.

At the pre-revision measurement, the real program did **not** pass the hypothetical public node
gate even at 32 positions: K2-TIR-v4 reported guaranteed proof work `629171712 > 536870912`.
No work limit was raised and no fence was activated. The following Gather pricing revision
supersedes this static refusal; the registered real-weight public-court/evidence path remains open.
The preliminary source/metric/bit-match result does not make this checkpoint supported.
All predeclared inputs, source hashes, material/binary hashes, results, logs and the refusal are
saved under `evidence/qwen25-real-*`. The large artifact and venv are local ignored `target/`
material, not repository payload. Source identity is conditional on the official HTTPS metadata
observation; it is not chain-enforced model acquisition. Full RFC02 scope and the remaining
A6/A8, feature, fidelity, resource/economic and public execution/court/Final gates remain open.

### Whole-value Gather pricing revision 2 (2026-10-10)

The real Qwen2.5 pilot's dominant whole-value court is the final 151,936-output lookup-table
Gather. For a rank-one table, axis 0, batch_dims 0 and an output shape equal to the index tensor's
shape, the output flat position is exactly the index tensor's flat position. The element court
now reads that index, checks its range, then reads the table directly. It allocates no strides or
coordinates per output. Every other Gather retains its coordinate evaluator. Invalid indices
still refuse before reading the table, and missing operands have the same error and dependency
reads as before. Integer semantics and all commitment preimages are unchanged.

Whole-value price revision 2 charges this specialized evaluator 16 rather than 32 abstract work
units per output, before the existing factor 64. Operand authentication and both output commitment
trees retain their complete byte prices. These units are a bounded consensus tariff, not CPU
instructions or milliseconds. No court/block/reservation limit is raised. The DA16b withholding
predicate deliberately retains the previous conservative price: lowering execution cost does not
change which values are owed in clear or committed form. The dormant v4 **and v5 descriptor
semantics digests change** to identify the revised admission price; rebuilding a plan changes its
root/class identity. This preactivation revision does not reinterpret a class under an unchanged
kernel identity, arm a network fence, or change v1–v3 descriptors.

The committed real pilot `evidence/qwen25-real-pilot-program.tir` is the exact 14,011-byte program
(SHA-256 `16a0de98fe52cdf47364392551b15bb85a7aad76acd8b14853b1803074bca8f1`). Its same 32-position
plan now has worst court bytes 629,606 and work 473,589,248, down from 629,171,712. It fits the
unchanged node guaranteed proof reservation 536,870,912. Direct TIR CLI and SDK report hypothetical
`ELIGIBLE_AT`, shipping `KERNEL_NOT_ACTIVE`. The new static regression uses that exact canonical
program without a model name, configuration, frontend or weights. This supersedes the previous
pilot's **static** resource refusal only. It proves no registration, source acquisition, public
real-weight conviction, economic eligibility, long-context fidelity or Final/redemption.

Differential tests cover all six dtypes and scalar through rank-four indices, the independent
whole-tensor TIR evaluator, the frozen previous coordinate evaluator and identical dependency
reads, out-of-range indices, missing operands, axis/batch variants, and unchanged conservative
withholding across small/large shapes. A synthetic 151,936-output court dismisses an honest claim,
convicts a fresh verifier's output-lie filing and refuses substituted/omitted param openings;
its 608,815-byte filing is within its bound. A separate 151,936-output i64 lookup measurement
includes both output commitment trees and compares all roots to both references. Six alternating
warmed **debug** trials measured medians 106.61 ms (frozen coordinates) and 25.96 ms (direct flat
lookup). These are local synthetic diagnostics, not real-checkpoint court latency or a
hardware-independent performance guarantee. Raw profiles, verdicts and logs are saved in
`evidence/qwen25-real-gather-v2-*` and `evidence/qwen25-real-kernel-preflight-gather-v2.json`.

Reproduction:

```sh
cargo test --locked -p misaka-palw-kernel --lib --test k2_real_scale --test typed_roots
cargo test --locked -p misaka-palw-sdk --lib preflight::kernel
cargo test --locked -p misaka-palw-sdk --test kernel_preflight_cli
cargo test --locked -p kaspa-consensus --lib \
  g14_canonical_a_fresh_node_reverifies_complete_checks_from_rpc_reads_and_its_own_artifact -- --test-threads=1
cargo run --locked -p misaka-palw-kernel --example profile-courts -- \
  docs/design/palw/tir/evidence/qwen25-real-pilot-program.tir 32
cargo run --locked -p misaka-palw-sdk --bin palw-class -- kernel-preflight \
  docs/design/palw/tir/evidence/qwen25-real-pilot-program.tir --positions 32 --json
```

Validation: 90 kernel unit tests, 13 segmented ledger/scale tests, 15 typed-root tests, five SDK
preflight tests, two direct CLI tests and one canonical node complete-check test passed (126
distinct tests). The existing manual producer hash benchmark remains ignored.

Full RFC02 remains incomplete. The next checkpoint evidence must bind real artifact operands,
registered roots, authenticated public material, fresh checking/court and Final through the node
path. Independent floating reference/task fidelity, long context and other common model features,
A6/A8 coverage and aggregate resource/economic gates remain required; synthetic/static success
must not replace them.

### Bounded real-artifact v3 parameter-root preparation (2026-10-10)

`merkle3_stream` builds the unchanged v3 dual-root commitment directly from narrow row-major
bytes. It reads every tensor byte once, hashes row tiles as they arrive, and retains one 4096-row
batch of column hash states. Column hashes are placed in canonical `(line, tile)` order before
folding. Both trees, odd-node promotion, scalar/flat/batched shapes, dtype widths, and the shared
leaf prefix are identical to decoded hashing. No full tensor or i128 expansion is allocated.

The algorithm derives a checked workspace bound (leaf hashes/folding, column states, one
<=64-KiB byte buffer and fixed slack) before allocating or reading payload. The SDK's
`kernel_params` validates the program and checks every model instance's workspace plus the
aggregate payload limit **before reading any instance**. A source failure returns no completed
map. It supports independent range sources and direct container-file reads. The latter fill the
kernel's buffer without an extra read-ahead allocation. These are explicit off-chain preparation
limits, not a node resource ceiling or a restriction on all models by this host's capacity.
The bound excludes program/instance metadata, the caller's cache, allocator overhead and process
RSS; the actual process RSS is measured separately.

Preparation's parameter roles are selected by the descriptor's memory model, not inferred from names: v4
prepares all declared model instances, while v5 validates its encoder binding and excludes the
job's ids/count slots. Unknown memory models refuse. The resulting keyed param/layer map is the
existing `ParamCommitmentsV1`, byte-compatible with decoded `of_v3`; its root is a **kernel param
root**, not the legacy inventory root or a claim/evidence root. A verifier must compare it to an
authenticated class binding. Computing a root grants no registration, source fidelity, model
availability, activation, economic credit or Final.

The public CLI entry `palw-class kernel-params` takes a canonical PALWTIR1 artifact, explicit
workspace/payload caps, and an output file. It needs no model name, ModelSpec, source configuration
or official compiler. It writes the Borsh parameter map atomically only after all instances succeed;
it refuses replacing the source artifact and keeps a previous output intact on preparation errors.
`--encoder` selects the v5 job-input roles explicitly. The output reports descriptor/program/param
roots, model declaration/instance counts, payload, workspace and elapsed time. No network is armed.

Actual measurement on the existing 32-token Qwen2.5 pilot artifact (same source revision and
artifact/program SHA-256 as the earlier evidence), using the debug CLI on this Mac:

| Method | Instances / payload | Algorithm elapsed | Maximum process RSS |
|---|---|---:|---:|
| Raw-width streaming, 64 MiB tensor workspace cap | 1,636 / 1,835,516,100 bytes | 57.985 s | 38,223,872 bytes |
| Decoded reference, one full tensor at a time, 8 GiB reference cap | Same; every instance compared | 223.809 s | 4,131,471,360 bytes |

The streamed algorithm's largest workspace bound is 27,275,264 bytes. The command's 4-GiB
aggregate payload limit and 64-MiB tensor workspace limit were fixed before the measurement.
All 1,636 decoded tensor roots and the complete param/layer map root match:
`1c07cbbadb1ec7166361a6ffdda43c7c5740183dbb8ecd9659e7c4c5c906cf5495ac5df4fd25617829628b3cec7969e43eeee06bed08c0beeaa51d76b023f729`.
The artifact SHA-256 was observed early during the streaming run and again after both runs,
unchanged at `435ed8fcddf68a4fc7dd22792dfe912fdbc3a9b4ab1a6803a711fcb1ff834a64`. This is a measured
full real-weight commitment computation and decoded-hashing comparison; it is **not** an
independent float-model reference, integer execution check, public-node conviction or Final.
The decoded reference is a diagnostic with a separate, deliberately much larger memory cap;
ordinary root preparation uses the streaming path.

Reproduction (the artifact stays in local ignored material):

```sh
cargo build --locked -p misaka-palw-sdk --bin palw-class --example check-kernel-params-reference
/usr/bin/time -l target/debug/palw-class kernel-params \
  target/rfc02-real/qwen25-pilot/qwen25-32.palwtir \
  --tensor-workspace-mib 64 --payload-limit-mib 4096 \
  --out target/rfc02-real/qwen25-pilot/kernel-params-v3.borsh --json
/usr/bin/time -l target/debug/examples/check-kernel-params-reference \
  target/rfc02-real/qwen25-pilot/qwen25-32.palwtir \
  target/rfc02-real/qwen25-pilot/kernel-params-v3.borsh 8192
cargo test --locked -p misaka-palw-kernel --lib merkle3
cargo test --locked -p misaka-palw-sdk --lib kernel_params
cargo test --locked -p misaka-palw-sdk --test kernel_params_cli --test kernel_preflight_cli
```

Eight kernel root/proof tests, three SDK preparation tests and four CLI tests pass (15 distinct
checks; the existing manual hardware hash benchmark remains ignored). Tests cover all dtypes,
scalar through rank-four/batched shapes, odd and partial leaves, simultaneous 4097x4097 row/column
tile boundaries, original leaf authentication, overflow/cap/source failures, all layer identities,
weight changes, explicit encoder roles, and output/source preservation. The real 112,864-byte
Borsh parameter map, source observations, reports and comparison/validation logs are recorded in
`evidence/qwen25-real-params-v3-*`. No weight payload is committed to the repository.

Full RFC02 scope remains open. These exact real program and parameter roots now supply the
registration/evidence preparation needed for the same-node public execution/checker/court path.
That path, authenticated public material, outsider conviction and independent Final/redemption
must still be measured with real artifact operands; root preparation cannot stand in for those
gates. All previously listed fidelity, context, feature, resource/economic and A6/A8 gates remain.


### Exact-context real-artifact node executor comparison (2026-10-10)

The generic node conformance function now refuses a request of zero positions or more than the
canonical program's `history_bound`, before acquiring any reference parameter or creating an
execution. It previously clamped the request to a different length. A successful report now
covers exactly the caller's requested length. This changes local conformance diagnostics, not
integer semantics, commitment preimages, descriptors, network activation or economic rights.

`misaka-palw-sdk/examples/check-kernel-node-execution.rs` accepts an arbitrary canonical
PALWTIR1 artifact and a prepared decoder v3 parameter map. It checks positive explicit hash,
payload and reference-tensor limits, refuses the requested context before payload reads,
recomputes every instance commitment using the bounded streamed producer, and compares the
complete map with the supplied map before execution. It then uses `TirArtifactV1` and
`tir_executor_conformance_against_v1`: the same typed CPU executor code used by the node,
against the integer reference interpreter with lazy one-tensor-at-a-time acquisition. It
consults no ModelSpec, config, model name, or official compiler registry. This diagnostic is a
local comparison; it does not replace the registered-model authentication in public checkers
or provide an immutable-source guarantee. Source SHA-256 observations before and after the
completed measurement both match the established real artifact.

The protocol was fixed before execution in
[evidence/qwen25-real-node-execution-protocol.json](evidence/qwen25-real-node-execution-protocol.json).
It fixes two positions, tokens `[13, 7932]`, a 64-MiB hash-workspace cap, 4-GiB total model payload
cap and 8-GiB reference-tensor cap. The latter covers the largest raw tensor plus its i128
expansion and a small allocation allowance, **not** total process/verifier RAM. Acceptance
requires all 1,636 actual parameter instances to match the prior map and exact bit agreement
at every compared commit and full-vocabulary logits for both requested positions. No latency
or throughput admission criterion is claimed by this diagnostic.

The actual full-weight Qwen2.5 pilot artifact passed those criteria. All **790 commit points**
and both positions' **151,936-vocabulary logits** match. The second position carries the first
position's KV/history state into both executions. Roots match the previously published
program, descriptor and real parameter map; no reduced or generated weights were substituted.

| Measurement | Observed value |
|---|---:|
| Requested / compared positions | 2 / 2 |
| Parameter instances authenticated first | 1,636 |
| Commit points compared | 790 |
| Reference tensor acquisitions | 3,490 |
| Execution comparison elapsed | 381.881 s |
| Process wall time, including parameter authentication | 440.00 s |
| Maximum process RSS | 6,442,958,848 bytes |
| Largest decoded reference tensor | 3,733,979,136 bytes |
| Largest raw-plus-decoded reference tensor bound | 4,201,775,104 bytes |

This was a local debug run with the workspace's existing package optimizations, CPU execution,
fused kernels off and no Metal feature. Process RSS includes hashing, reference evaluation,
typed execution and the mapped artifact; it is **not isolated producer memory or ordinary
verifier memory**. SHA-256 and parameter authentication read the file before comparison, so
this is not a cold-cache measurement. The comparison time combines reference and typed
execution, and does not establish the producer's throughput or economic profitability.

Reproduce over the existing real artifact and prepared map:

```sh
cargo test --locked -p misaka-palw-tir-exec --features node --test node_conformance
cargo build --locked -p misaka-palw-sdk --example check-kernel-node-execution
/usr/bin/time -l target/debug/examples/check-kernel-node-execution \
  target/rfc02-real/qwen25-pilot/qwen25-32.palwtir \
  target/rfc02-real/qwen25-pilot/kernel-params-v3.borsh 2 64 4096 8192
```

All four node conformance tests passed: the node suites' exact commits/logits, a different
weight rejected by position, deterministic in-range prompts, and invalid context requests
refused before reference acquisition. The measurement's result, raw run log, tests, build,
source observations and code/binary hashes are saved under `evidence/qwen25-real-node-execution-*`.
The verdict is
[evidence/qwen25-real-node-execution-verdict.json](evidence/qwen25-real-node-execution-verdict.json).

**Full RFC02 scope remains open.** These two positions prove local actual-weight integer
execution agreement. They do not prove the complete 32-position pilot context, task quality,
independent source floating-model fidelity, same-node-binary registration, authenticated
public claim material, outsider conviction, independent Final/redemption, or A6/A8 and economic
gates. The next node evidence must bind the actual inventory and kernel roots and carry an
actual-weight segmented claim through public checking and its terminal court. Preserve all
remaining shared-feature, complete-task, resource aggregate and coverage requirements.


### Native segmented trace and actual-weight delivery court (2026-10-10)

`misaka-palw-sdk::kernel_execution::native_trace_v3` now bridges an arbitrary canonical
PALWTIR1 decoder artifact to the existing v3 node commitments and segmented position/segment/
claim roots. It consults no ModelSpec, model name or official compiler. It rederives the exact
plan, rejects wrong plans, vocabulary and context before execution, and uses the same static
prosecution/carrier/block admission ceilings as the node. Dormant kernels can be prepared
locally; other static refusals are not bypassed, and activation is not granted.

The node's typed CPU executor visits **every node**, including nodes without a legacy commit
flag. The sink checks occurrence, node, declared type, resolved shape and element count. A
wider native storage buffer is allowed only when every mathematical value fits the declared
wire dtype, before converting it to canonical little-endian bytes. The bounded raw v3 hasher
preserves the existing commitment preimages. A callback receives one complete position after
the native step succeeds; an incomplete failed step is discarded. Earlier values are not
needed for root construction. Only position roots survive streaming; decoded position capture
is optional and **private**, and caller retention is separately budgeted.

Explicit local limits cover the hashing buffers and current commitment/root/capture workspaces.
They exclude native executor buffers, mapped model weights, caller-retained visits and process
overhead, so they are not a new whole-process or ordinary-verifier RAM guarantee. Kernel
resource prices, wire forms, descriptors, activation fences and economic accounting are unchanged.
`position_path_of_roots_v1` generates the established segment path from the retained roots alone;
odd leaves and segment boundaries match the full retained-commitment implementation.

`decode_fault_from_logits_v3` builds a delivery-lie proof from the selecting logits and an
authenticated node opening. It authenticates position/node paths, declared type/shape and value
commitment before returning even a clean result. It opens at most the delivered/rival leaves,
self-grades through the existing terminal Decode court, and requires no producer state or model
parameter values. Model acquisition and disclosure policy remain separate caller obligations.

The generic `prepare-kernel-decode-court` example authenticates all actual parameter instances
against a prepared v3 map, computes the actual inventory root through the node artifact API,
runs a fixed full prompt and one Greedy output, and saves a small reproduction bundle. Only the
final position is privately captured. The bundle contains the canonical program, exact plan,
parameter commitment map, roots, public prompt/output ids and a delivery proof. It contains no
private position values or raw weights. It is off-chain reproduction data, not a consensus
object, an attestation or proof of registration.

The actual-weight protocol was fixed before execution in
[evidence/qwen25-real-native-court-protocol.json](evidence/qwen25-real-native-court-protocol.json):
32 positions of the established lowered pilot, one Greedy output, a 64-MiB hash-workspace cap,
1-GiB trace-workspace cap and 4-GiB aggregate parameter payload cap. The tokens are fixed in the
protocol; no lengths or limits were substituted during execution. Every actual model instance
matches the previously recorded parameter map. Source SHA-256 observations before and after the
completed run match the established 1,835,572,288-byte artifact.

| Actual observation | Result |
|---|---:|
| Positions completed from initial state | 32 |
| Nodes committed per position / total | 7,658 / 245,056 |
| Native execution plus node hashing/capture elapsed | 169.916 s |
| Whole process wall time, including authentication/inventory | 228.26 s |
| Whole process maximum RSS | 2,143,010,816 bytes |
| Derived local trace workspace bound | 255,478,864 bytes |
| Largest per-node hash workspace bound | 19,489,760 bytes |
| Actual inventory leaves | 1,445,797 |
| Honest Greedy token / substituted token | 1 / 2 |
| Terminal delivery proof | 17,924 bytes |
| Public reproduction bundle | 161,675 bytes |

The descriptor, program, 32-position plan and parameter roots match the prior real pilot. The
actual inventory root is `5c02fc51ad06f17066c64dca3b79c4bbf986fdd399b7e583a6384ff8d00580d13aeefb2f42c3cbec3a1499f639b6463e792ac40394ba590bc902e1b65fbeed9a`;
it is a distinct domain from the kernel parameter root. The resulting claim root is
`06f55704be0edec0b0b5d6e4f83a1b791037259d222aef829d1ba5f907977779ea99209d14f92ebde51e1f8e443be3cae1a0a2e9052fa0c1b41f65b769bf69d9`.
Full roots, source/code/binary hashes and raw observations are recorded in
[evidence/qwen25-real-native-court-verdict.json](evidence/qwen25-real-native-court-verdict.json).

The selecting logits are not normally withheld by this program's existing scope. The existing
court convicts the substituted delivered token and dismisses a weaker-rival filing against the
honest output as `NoFault`. A **separate test process**, using only the committed public bundle
and parameter commitments, replays that conviction and dismissal and refuses copied-position
and tampered-commitment openings. It has no full weights, producer process or private trace.
This proves offline consumption of the actual delivery witness; it does not prove public RPC
acquisition or the correctness of the claim's internal computation.

Validation: four kernel segment/path tests, three SDK native-trace/court tests and one actual
public-bundle integration test passed (**8 distinct tests**). Five diagnostic checks refuse
zero/oversized positions, zero workspace and replacing either input file, and preserve any
previous complete output. The trace differential test compares every value and root through a
sliding history window and MoE; it is synthetic evidence, separate from the actual-weight run.
The initial test build ran out of disk space; only inactive completed caches inside this
worktree's ignored incremental directory were removed, preserving sources, actual artifacts,
evidence and working/recent cache sessions, before the successful final tests/build.

Reproduce using the existing actual artifact and prepared map:

```sh
cargo test --locked -p misaka-palw-kernel --lib seg::tests
cargo test --locked -p misaka-palw-sdk --lib kernel_execution
cargo build --locked -p misaka-palw-sdk --example prepare-kernel-decode-court
/usr/bin/time -l target/debug/examples/prepare-kernel-decode-court \
  target/rfc02-real/qwen25-pilot/qwen25-32.palwtir \
  target/rfc02-real/qwen25-pilot/kernel-params-v3.borsh 32 64 1024 4096 \
  target/rfc02-real/qwen25-pilot/native-decode-court-32.borsh
cargo test --locked -p misaka-palw-sdk --test real_native_decode_court
```

This is local debug CPU execution with existing package optimizations, every-node observation,
fused kernels off and no Metal feature. SHA-256/authentication/inventory reads warm the artifact
before execution. RSS is for the complete process, not court/verifier RSS. The elapsed time
combines native computation and commitment generation; no source-model throughput ratio or
latency/economic admission criterion is inferred from it.

**Full RFC02 remains incomplete.** The native trace covers the lowered pilot's full 32-position
job, not the source checkpoint's original 32,768 context or complete task quality. It does not
establish independent floating-source fidelity or whole-claim computational integrity. The next
node test must register/onboard these actual program, inventory and kernel roots, carry the
actual-weight claim and public delivery witness through the mempool, blocks, public RPC and
court, and verify independent settlement. Fresh checking of state/weight/routing lies, longer
contexts, all remaining shared features and complete tasks, aggregate resources, A6/A8 and
economic gates remain required. Shipping activation fences remain off.


### Requested-claim identity and actual-weight node delivery mechanics (2026-10-10)

The segmented public-record API now has `view_for_claim(header, requested_id, producer)`.
The verifier anchors `header` in its synced class state and supplies the requested claim id
independently of the served record. The method rebuilds `KernelClaimV1` from the job, producer,
**all delivered ids**, and evidence root and checks its canonical id. A header/trace-only check
cannot authenticate the final delivered id, which is never fed back. This fixes the public
client boundary without changing consensus bytes, hashes, descriptors, activation or economics.
The method also refuses noncanonical/duplicate parameter entries, a substituted inline prompt,
incorrect exact generation length or vocabulary, context bounds and evidence input bindings.
It supports both segmented decoder and encoder records. It verifies identity and consistency,
**not chain inclusion**: a JSON answer and its self-reported roots are not an authenticated state
proof. The existing header-only `view` remains available for callers already inside trusted
ledger state; callers of a requested public claim should use the stronger method.

The new node test consumes the committed actual-weight 32-position bundle from §14.10. It
loads no full weights, private producer capture or ModelSpec. Its exact program, plan and all
1,636 parameter commitments are registered on the existing segmented route. Registration data
uses ordinary 32,000-byte `ObjectChunk` payloads and the node's unchanged storage/compute mass
ceilings. Job prompt tiles, salted claim seal/reveal and proof carriers go through the mempool,
node template and block fold. RPC op 210's production response builder is serialized through
JSON, then the outsider rebuilds the requested claim view against the synced class row. The
17,924-byte recorded delivery witness is checked against this view, rather than the producer's
offline `SegClaimContextV1` labels.

Acceptance assertions cover a copied-position filing causing no conviction/slash, the genuine
substituted-token proof convicting position 31 and slashing the claim reservation once,
duplicate conviction causing no second slash, a weaker-rival filing against honest delivery
being dismissed, honest Final and coinbase redemption once, and a second independent node
replaying the blocks to the same tip, PALW state, route rows, per-block delta roots and both
producer/accuser payout outcomes.

**The existing test seams remain explicit.** Activation validation is bypassed by the parent
harness; parameter-root attestation and OPV eligibility are `cfg(test)` hooks. With the hook
excluded from the derived eligibility view, this actual class is `NotOnboarded`. Its stateful
program is refused by the small-stateless `palw_complete_check_domain_v1` bootstrap. Its V2
inventory root is a separate domain, remains unregistered in this mechanics test, and is not
substituted for the kernel parameter root. This test therefore establishes the actual delivery
witness's node court/settlement mechanics, not model onboarding, a DA acquisition service,
shipping permissionless activation, source fidelity or whole-computation G14.

One concrete registration gap is visible in the current code: segmented v4/v5 registration
requires OPV; OPV registration requires derived eligibility; eligibility requires the class's
`KernelBoundV1`/conformance row; `KernelBoundV1` requires an already registered kernel class.
The legacy Panel pre-registration used by v2 cannot bootstrap a segmented class. A conformance-
only pre-registration path must break this cycle without granting claims, Finals, rewards or
beacon-source rights before their independent gates pass. The release additionally refuses
sampled conformance as reward eligibility pending its digest/refutation courts (GAP-70). None
of these gates is relaxed by the new test or record API.

Validation passed: **97 final kernel tests**, two SDK identity/delivery tests, one final v5
encoder regression and the final actual-weight node test (**101 distinct tests**; repeat runs
not counted again). One existing manual hash benchmark remains ignored. The SDK identity tests
preceded the later pricing-only change; their API/test sources are unchanged, and the final
kernel/node checks cover the pricing change. The final node trial passed in **104.02 s**,
including independent replay and redemption. Node-package incremental caching was disabled
locally after confirmed terminal disk failures; optimizations and node limits were preserved.
Commands, code/binary hashes, diagnostic failures and scoped results are saved in
[evidence/qwen25-real-node-delivery-verdict.json](evidence/qwen25-real-node-delivery-verdict.json).
The protocol is [evidence/qwen25-real-node-delivery-protocol.json](evidence/qwen25-real-node-delivery-protocol.json).

Initial diagnostics are described separately from successful evidence. The first trial used a
100,000-byte registration chunk and was refused at block transient storage mass 614,992 over
500,000; transport was reduced, without increasing the limit. A later trial changed the global
eligibility hook after an initially refused registration, so replay admitted that earlier
object and diverged. The final test keeps all mock eligibility facts fixed from the beginning
on both nodes and evaluates the real no-hook refusal separately. This is a corrected harness
history, not a waived replay assertion. A SDK build also exhausted disk; only completed inactive
incremental caches inside this worktree were removed, preserving sources, artifacts, compiled
binaries, evidence and every working cache session before retrying. After the pricing source
changed, an obsolete completed SDK core archive was also removed; it is a regenerable build
cache, and its already linked test executable remains available.

**Full RFC02 remains incomplete.** Actual onboarding and authenticated public DA acquisition,
all state/weight/routing/internal relation faults, original/full contexts, independently measured
source/task fidelity and performance ratios, remaining shared features and complete RFC03 tasks,
aggregate resources/economic caps and A6/A8 coverage remain required. Keep the full objective
and conditional model acquisition (A) unchanged.


The first corrected fixed-hook node run passed all court, Final, redemption and independent
replay assertions in **408.66 s**. A one-second live stack observation during that run located
`tick_kernel_route_v1 → load_ledger → from_rows → class_prosecution_bounds_v1 → axis_cover`.
The old pricing path materialized a leaf's flat-index vector only to obtain its length, for
both trees and every priced operand, each time the authenticated class row was reconstructed.
`LayoutV3::leaf_element_count` now derives the same length in constant space with checked
tile-start/subtraction arithmetic. `axis_cover` uses it; leaf selection, tree choice, wire
pricing, actual proof values and hashes are unchanged. No resource ceiling is increased.

Three focused checks passed: exhaustive row/column counts at scalar/empty/tile/batch boundaries,
invalid coordinates and the largest non-overflowing scalar line; differential pricing against
the original index-vector oracle for every dtype and the actual embedding/projection geometries;
and the **exact recorded 32-position plan root**. The complete kernel suite and a second actual
node run validate the final optimized code below. Node-harness timings include registration,
carriers, Final wait and replay and are not producer/model throughput, source task performance,
isolated verifier RSS or an economic admission benchmark. The stack observation is diagnostic,
not a sampling claim about every possible workload.


Reproduce the final node and identity paths:

```sh
cargo test --locked -p misaka-palw-kernel --lib
cargo test --locked -p misaka-palw-sdk --test real_native_decode_court
G14_TRACE=1 cargo test --locked -p kaspa-consensus --lib \
  --config 'profile.dev.package.kaspa-consensus.incremental=false' \
  recorded_actual_weight_delivery_convicts_through_rpc_carriers_and_an_honest_claim_redeems_once -- --nocapture
cargo test --locked -p kaspa-consensus --lib \
  --config 'profile.dev.package.kaspa-consensus.incremental=false' \
  g14_k2s_v5_an_encoder_class_on_the_node_convicts_its_lies_and_finalizes_an_honest_claim -- --nocapture
```

The confirmed terminal failed node compiler's cache was removed even though its name still
ended in `-working`; the process/session was already terminal, and no live compiler cache was
removed. The preserved baseline executable and final logs distinguish code/profile scopes.
No model weights, private trace captures or compiled executables were committed.

## 2026-10-11 — conformance-only OPV metadata, with no premature execution rights

The registration/onboarding metadata cycle is resolved by a distinct public object,
`RegisterConformanceClass` (outer 110, inner 21, route wire version 1). Its checked metadata
lives in ledger table 28 as `(preparing bond, ClassRecordV1)` under the eventual canonical OPV
class id. It uses the same class validation as ordinary registration, omitting only the
eligibility prerequisite that depends on acquiring conformance. The existing route and
`palw_panel_free_v1` fences still apply; neither network activation validation nor GAP-70's
sampled release-reward refusal was changed.

Preparation validates canonical program bytes, exact plan, active known descriptor, ranges,
parameter-instance coverage, independently attested root, prosecution bounds, court budget,
OPV economic/resource envelope and node carrier fit. It spends an ordinary adjudication run
and the derived admission work under the same block budget as claims, without borrowing the
proof reserve. Each preparing bond can hold at most eight candidates. A derived bounded owner
index avoids scanning all candidates for that actor's cap; reconstruction rebuilds that index
and refuses wrong class keys, absent policy/bond, over-cap ownership or duplicate execution
registrations. This is a bounded preparation catalog, not evidence that every remaining
network-wide storage, capital and honest-liveness requirement is complete.

The class-collection and OPV execution sets are unchanged until promotion. `PostJob`,
`PostTiledJob`, seals/claims, escrow, rewards, Final facts/weight and beacon sources cannot use
candidate metadata as an execution registration. Onboarding's metadata reads and derived
eligibility can resolve a candidate, retaining the exact V2 program/artifact/plan binding and
all conformance, clocks, policy and resource checks. The SDK's authenticated paged snapshot
reconstruction accepts the same metadata for conformance verification, without asserting
that it grants execution eligibility. Ordinary `RegisterClassV2` must still pass derived
eligibility before promoting the identical class id and removing the candidate. Promotion
can be signed by another bonded actor: preparation does not assign model ownership or an
operator/registration priority. Kernel-ledger withdrawal removes that actor's candidates.

Table 28 adds a versioned root extension only while nonempty. The previous root, including
OPV, segmented, typed, beacon and verifier-pay extensions, remains byte-identical when there
are no candidate rows. The table's collection domain and conditional extension domain are
recorded in RFC §14.12. Older route discriminants and registration records are unchanged.

Verification includes actual signed node carriers, matured real artifact-binding rows,
refused premature registration/job, complete conformance judged in the fold, a fresh IBD
node's public paged RPC snapshot plus its own independently authenticated artifact, rejection
of another artifact, promotion by a different actor, honest Final, one-time coinbase
redemption, and independent replay. The node fixture is **small, synthetic and stateless v2**,
with dormant fences explicitly armed through the existing test configuration seam. It uses
**no OPV eligibility or parameter-attestation hook**. Segmented v4 preparation, no ordinary or
tiled job rights, per-bond/shared-block caps and row/root replay are additionally exercised
at the kernel level. This is not a full real-checkpoint onboarding or a large/stateful
conformance result. A stateful candidate still cannot use the small complete-check policy;
sampled conformance still cannot authorize release rewards.

Final verification: **187 distinct tests passed**: 97 kernel library tests (one existing
manual hash benchmark ignored), 22 route/state tests, 28 OPV tests, 15 segmented ledger tests,
3 row tests, 13 node onboarding tests, 2 fence-table tests and 7 SDK snapshot tests. Repeats
are not counted twice. The corrected final suites all exited successfully.

Final results and exact source digests are in
[`rfc02-conformance-candidate-verdict.json`](evidence/rfc02-conformance-candidate-verdict.json).
The full RFC02 goal remains open: large/stateful release conformance and actual-weight
onboarding; original-checkpoint/full-task/full-context fidelity; all required common features
and independent G14 paths; complete shared resource/capital accounting and honest liveness;
and shipping activation under the unreduced gates remain required.

Reproduction:

```sh
cargo test --locked -p misaka-palw-kernel --lib \
  --test k2_real_scale --test k2_rows --test k2_ledger_route --test k2_opv
cargo test --locked -p kaspa-consensus --lib \
  --config 'profile.dev.package.kaspa-consensus.incremental=false' \
  --config 'profile.dev.package.kaspa-consensus-core.incremental=false' \
  opv_bootstrap -- --nocapture
cargo test --locked -p kaspa-consensus-core --lib \
  --config 'profile.dev.package.kaspa-consensus-core.incremental=false' \
  --config 'profile.dev.package.kaspa-consensus-core.debug=0' \
  every_kernel_inner_kind_has_exactly_one_row -- --nocapture
cargo test --locked -p misaka-palw-sdk --test onboarding_snapshot \
  --config 'profile.dev.package.kaspa-consensus-core.incremental=false'
```

The core fence tests keep the normal optimization and debug-assertion settings but omit
package debug symbols to reduce local build output. Node tests retain their normal debug
symbols. Compilation failures from local ENOSPC were recovered without changing source
limits or activation terms. An old unused-tag assertion was updated for inner tag 21. Cache
cleanup also removed two transitive archives needed at node linking; those were regenerated
before accepting the final verification. No other task's checkout/cache, model weights or
private traces were removed or committed.


## 2026-10-11 — bounded v3 artifact identity and actual-weight registration without identity hooks

The V2 inventory/kernel bridge previously offered only the legacy whole-row proof. That
opening cannot authenticate a v3 tile commitment, leaving the two-root identity statement
without a bounded v3 byte refutation. Tag 105 now appends proof discriminant 2, `TileV3`.
It authenticates the full carried commitment map, a bounded tile of its named instance, and
one canonical V2 inventory opening against the class's root, then judges metadata or common
bytes. Row and strided column coordinates are compared exactly. The column court also
convicts a forged column tree paired with a truthful row root; truthful row comparisons
alone would leave that false binding unrefutable.

The new wrapper checks rank/value/path lengths before allocating their bodies (4/4096/64)
while keeping the kernel leaf's wire bytes unchanged. Inventory pieces/path lengths are
checked before hashing (32768/32). Legacy proof discriminants, encodings and fold behavior
are unchanged. The new tariff is filing bytes plus stored canonical program bytes plus
16 bytes per opened element. It spends one run and this work in the existing shared block
budget, before decoding the stored program or running the judge. Charged no-fault judgements
pay the ledger's proportional dismissal tariff from challenger free collateral, persist
budget/fee and leave the honest binder's reservation intact. Over-budget objects run no
court; a fresh block can run one again. A valid fault uses the existing one-time reservation
slash and reporter share. Dormant shipping fences and sampled release gating are unchanged.

The generic SDK producer streams an inventory opening from ranges with a single 32 KiB
piece buffer, bounded read-ahead and sibling-subtree frontiers. It retains neither a full
weight tensor nor a vector of all 1,445,797 inventory hashes. The public core builder uses
the same judge as the node. Model acquisition remains conditional and creates no provider
availability obligation. The trial includes a public authenticated opening of the bound
tensor. Possession of the true model alone cannot reveal an arbitrary opaque false root's
opening; closing that withholding case remains a binding/prosecution requirement.

The actual trial uses all 1,636 parameters from the pinned Qwen2.5 source already recorded
above, at the same 32-position scope. The original lowered artifact had an unset logits
scheme and was correctly refused by V2 admission. The existing SDK `declare-layout` produced
a separate declared artifact with the explicit tiled scheme, tile length 128, h_tile 2 and
checkpoint interval 65536, without changing parameter declarations or bytes. The new
canonical program is committed separately: its SHA-256 is
`a4de8e80ecd4c8dfcbf6ad2faac54779609c373afb40602f1372d2bd63c2d516`.
The declared container SHA-256 is
`ff19dd4e4e5a1f18199c4ee5fde8015a5d7883b46080f2530bf02d51ee6db288`.
Both inventory and v3 parameter roots remain equal to the earlier authenticated source.
The original artifact/program, native trace and delivery-court records remain separate;
their old plan/claim roots are not substituted for this declared program's roots.

Every instance was reauthenticated from bounded raw reads before producing the public
witness. Changing the first element of parameter 3, layer 0, changes only that tensor's
commitment. Each honest/false filing is **114,371 bytes**; the bundle, including declared
class and complete parameter maps, is **369,866 bytes**. The final preparation took **58.61 s**
wall time with **32,604,160 bytes maximum RSS** on this host. These measurements cover root
preparation and witness generation, not execution, full computation prosecution, conformance
or performance at the original model context.

The actual node accepts the signed V2 declaration, holds the true binding, charges an
outsider's no-fault filing without slashing the binder, convicts another class's false
binding once, matures only the true root, and prepares the full actual v4 candidate through
32 KiB ordinary object chunks. `KernelBound` resolves that candidate. No parameter-attestation
or OPV eligibility hook is called. Execution classes/jobs/claims remain empty, release
eligibility remains held and the small-stateless complete-check domain refuses the actual
stateful program. An independent node replays the carrier history and committed state.
This is actual-weight identity/metadata onboarding evidence, **not completed reward-bearing
onboarding** or a model/task/context support PASS.

Final verification: **22 distinct tests passed** — 9 core onboarding tests (6 new bounded
court tests), 6 streamed-inventory SDK tests, 5 actual-node binding tests (3 new, 2 regression),
and 2 central allocation/dormancy checks. All final sessions exited successfully. The first
node build had test-helper visibility/type errors, then the undeclared artifact was refused
for its unset scheme; both were corrected before accepting the final results. Old completed
compilation objects in this checkout were removed to reclaim disk space; archives, live
compiler output, other checkouts, model sources and private traces were preserved.

The protocol, source digests, raw final logs, witness hash and limitations are recorded in
[`rfc02-artifact-tile-verdict.json`](evidence/rfc02-artifact-tile-verdict.json).
Full RFC02 remains open: large/stateful release conformance (GAP-70), the v5 bridge's explicit
model-weight/job-input role contract, complete real-computation G14, original-checkpoint
full-task/full-context fidelity, all required common features and shared capital/resource
and honest-liveness bounds, and shipping activation under the unreduced gates.

Reproduction (the containers remain local source artifacts):

```sh
palw-class declare-layout --network testnet-12 --out qwen25-32-declared.palwtir \
  --max-context 32 --tile-len 128 --h-chunk 2 --logits-scheme tiled --logits-tile 128 \
  qwen25-32.palwtir
cargo run --locked -p misaka-palw-sdk --example prepare-artifact-tile-court -- \
  qwen25-32-declared.palwtir params.borsh new-bundle.borsh 64 4096
cargo test --locked -p kaspa-consensus-core --lib \
  --config 'profile.dev.package.kaspa-consensus-core.incremental=false' \
  --config 'profile.dev.package.kaspa-consensus-core.debug=0' palw_onboarding -- --nocapture
cargo test --locked -p misaka-palw-sdk --lib \
  --config 'profile.dev.package.kaspa-consensus-core.incremental=false' tir_stream -- --nocapture
cargo test --locked -p kaspa-consensus --lib \
  --config 'profile.dev.package.kaspa-consensus.incremental=false' \
  --config 'profile.dev.package.kaspa-consensus-core.incremental=false' artifact_binding -- --nocapture
```

## Descriptor-scoped model inventory foundation — 2026-10-11

The v5 root producer excludes the last two job-input parameters, but old 104/105 judge every
parameter in the legacy V2 class inventory. Feeding an honest model-only PC map into that
old instance-set judge therefore reports a missing tensor. This was reproduced explicitly;
the old statement is not silently reinterpreted to hide the mismatch.

The new `PalwTirModelInventoryV2` borrows the complete structurally validated program and
accepts exact current v4/v5 descriptors. It selects the prefix by the descriptor's defined
role contract, rejects altered semantics even with the same memory-model field, and exposes
only derived model instances, leaf count/index and row visits. No caller-selected prefix is
public. Legacy layout functions use the same private piece/position logic with the full
parameter set, preserving their byte/hash grammar. The pure v2 judge uses this scope for
instances and opening positions; v1 remains all-parameter. New v2 byte comparisons require
bounded v3 tiles. The SDK streams model-only roots and public openings with the same bounded
piece buffer and Merkle-frontier algorithm as the historical inventory.

Final verification: **22 distinct tests passed** (6 core inventory, 9 onboarding/court,
6 streamed-root/opening regressions and 1 weighted fixture test covering both encoders).
All final sessions exited successfully. Verification and source hashes are recorded in
`evidence/rfc02-model-inventory-verdict.json`.
The weighted BERT and XLM-R fixtures exercise lowering/materialisation, exactly matching the
existing v5 PC producer, materialised reference roots, no job-input reads, honest no-fault,
changed/missing/surplus model weights, copied-root/path rejection and early invalid-shape/
descriptor rejection. The core schedule/piece regression checks all coordinates against the
historical v4 inventory. These are mechanics tests, not full-size encoder/task support PASS.

**Still required:** a new authenticated node statement binding descriptor/program/model root/
PC root, its rooted collateral/reservation and maturity, shared admission/refutation budgets,
matching candidate attestation and actual-node replay tests. No new object/table allocation or
shipping activation is made by this foundation. The old v5-to-104 bridge is still unsuitable;
full RFC02 remains open under the complete scope in §II.14.

## Signed descriptor-scoped node binding — 2026-10-11

The foundation's missing signed/rooted binding consumer now has dedicated inner 22–24
under outer 110 plus dormant PanelFree. That snapshot allocated aux 43–45, which overlapped
the provider court; the current consumer corrects model statements to 46–48 before shipping
activation. Historical 104/105 stay all-parameter.
The exact descriptor/program/model root/PC root statement carries a real bond reservation;
its fixed-size header permits bounded fee/deadline/collateral reads without copying program
bytes. Each bond retains at most eight statement records. Both withdrawal duty gates and
committed collateral include its clocked reservation; no new global closing scan is needed.

Binding and refutation use the existing shared block budget, reserving prosecution room
against admissions. Refutation charges before proof or stored-program decoding, bounds all
public proof counts before allocation, pays only the existing 49% bounty on valid fraud,
and charges the attacker for an invalid filing. The candidate consumer matches the explicit
matured scope before ordinary v5 range/plan/prosecution/carrier admission, stores its statement
link, and removes the temporary root before persisting ledger rows. No attestation or
eligibility hook is used. The signer of the binding receives no registration/claim priority.

The bounded SDK captures are fixture diagnostics, not full checkpoint certificates:
`rfc02-bert-model-artifact-court.borsh` and `rfc02-xlmr-model-artifact-court.borsh`. The BERT
capture has 51 model parameter declarations and **92 tensor instances**; those quantities
must not be confused. The node trial checks pending admission refusal, honest no-fault fee,
matching v5 candidate admission, another descriptor's refusal, absence from global attested
rows, liability release, a changed weight's once-only conviction, invalid-proof flood costs,
a bounded catalog/shared collateral, and independent replay. It creates no execution classes,
jobs, claims, Finals or rewards.

Final verification: **31 distinct tests passed**, including four actual-node cases with
independent replay, three bounded-model tests, nine legacy onboarding regressions, six
streamed-root tests, four route codec tests, the encoder fixture export, and four
allocation/wire/dormancy checks. Final logs, source/fixture/capture/binary digests and measured outcomes are recorded in
`evidence/rfc02-model-binding-verdict.json`. Initial node attempts failed on local disk
capacity, a private test helper reference and a declaration/instance count assertion. The
wire-pin regression exposed a stale tag-105 pin already present at the baseline. Comparing
both source manifests confirmed that no existing proof type changed in this patch; the
pin now records the previously added bounded v3 tile court. Outer 110/113 pins remain
unchanged, and the new inner variants have explicit PanelFree allocation above; only
successful final runs count. Completed ignored compilation objects/caches were reclaimed
in this checkout after their owners were confirmed terminal; model inputs, binaries,
archives, live compiler output and other checkouts were retained. The controlled node build
uses package debug symbols disabled, with normal optimization/debug assertions and unchanged
consensus/network limits.

Full RFC02 remains open: scoped generic release conformance and execution promotion,
large/stateful GAP-70, complete actual computation G14 (including opaque/withheld parameter
roots), original checkpoint/task/context fidelity and every required common feature, full
shared resource/capital accounting and honest liveness, and reviewed shipping activation.

## Descriptor-scoped finite-domain conformance reference — 2026-10-11

The preceding goal turn made implementation progress: signed model statement consumer,
31 checks and verified push at `32092793b`. The reference review was read again before
this change; the complete RFC02 goal and all §II.14 requirements remain active.

The old complete-check is a V2 all-parameter, stateless single-token enumeration. It cannot
be reinterpreted as encoder sequence or stateful-prefix conformance. The new pure domain
is derived from the exact v4/v5 descriptor and checked program/plan. It enumerates every
nonempty token word at all lengths within the program's encoder axis or decoder plan,
not just repeated tokens or selected prompts. The reference starts each word independently,
checks source coordinates before allocation, derives source and v3 PC roots jointly,
injects encoder ids/count as inputs, and hashes all typed intermediate/state values.
Local work and memory envelopes are derived before tensor decoding/replay. No new wire
allocation, consensus row, activation setting or eligibility exception is introduced.

Final verification: **26 distinct checks passed**: six new conformance tests, six inventory
regressions, three signed-model decoder/header tests, nine legacy onboarding/court tests,
and the wire-pin and shipping-dormancy checks. The new tests cover all 14 binary words up
to length three for an encoder and a stateful decoder, independent prefix initialization,
hidden intermediate corruption with unchanged logits, incorrect source/PC/role roots,
malformed inventories, resource caps and refusal of the saved BERT/actual Qwen programs
whose input vocabularies exceed enumeration. Verification and limitations are recorded in
`evidence/rfc02-model-conformance-verdict.json`.
These are mechanics checks, not full-checkpoint support or independent runtime-pack
certification. The next integration must authenticate the exact statement/candidate,
charge the derived work before judging, persist a scope-bound result and maintain every
eligibility, G14, source-fidelity, capital/resource and shipping gate. Real vocabularies and
large/context domains remain outside exhaustive enumeration and require GAP-70's scalable
complete digest/refutation contract; nothing here relaxes that requirement.


## Authenticated finite-domain conformance consumer — 2026-10-11

Inner 25 now authenticates a complete source inventory against an exact matured model
statement and candidate. It charges shared admission work and the submitter's fee before
stored-program/proof decoding, limits judged runs to two per blue-score block, and stores
only a fixed scope-bound result. Invalid posts consume the actor's budget and collateral,
not the binder's reservation. Successful conformance can select a truthful binding even
when the first candidate declared a false source root. Ordinary inner 13 then derives
execution eligibility from that checked scope, without model/compiler identity hooks or
persistent global PC attestation. The eligibility predicate rechecks the existing standing,
denial, prosecution, carrier, court economics and complete-policy conditions.

The earlier model aux allocation overlapped the provider court's 43–45. Model statement,
bond index and candidate link now use 46–48; checked conformance and block count use 49–50.
This correction precedes shipping activation. It is not a live-state migration.

**62 distinct tests passed:** 32 core, four kernel route and 26 actual-node cases. The new
three node cases cover encoder/stateful-decoder registration, exact scope/denial checks,
candidate exclusion from beacon sources, first-registrant poisoning, bad role roots,
invalid-post flood charging, honest recovery, fresh public verification, an actual element
conviction, honest encoder Final and independent replay. Twenty-three existing node cases
cover scoped model binding, OPV bootstrap, public read/promotion and provider courts.
Those regressions ran before the last four-byte wire-ceiling correction; the final boundary
roundtrip, core checks and three new node cases passed after it. Raw logs, source/binary
hashes and explicit limitations are in `evidence/rfc02-scoped-consumer-verdict.json`.
The first node compile referenced a private field; the test now uses the public accessor.

These are synthetic mechanics tests. They do not establish real-checkpoint fidelity,
independent runtime-pack certification, a complete network resource/liveness proof, or
full RFC02 completion. Large domains still need GAP-70; all §II.14 feature, checkpoint/task/
context, capital/resource, G14, independent redemption and activation requirements remain
mandatory. Shipping fences remain dormant.


## Authenticated candidate trace statements and public courts — 2026-10-11

Inner 26/27 provide a separate typed trace assertion and bounded refutation for an admitted
scoped candidate, including vocabularies and inventories outside finite enumeration. The
ordinary signed consumer authenticates the poster/challenger. A preceding execution class,
job, claim or Final is unnecessary. Public input/delivery, exact model binding and program/
plan/PC, network/ruleset and segmented roots determine its statement identity. All three
claimed implementation roots must match that typed statement. Legacy flat vector digests
retain their old grammar. Equal claimed roots do not establish independent runtime identity.

Aux 51/52 store the fixed liability/court header and an eight-entry lifetime bond catalog.
Each distinct assertion reserves the poster's 100 BILI for 200 DAA, shared with its other
commitments and both withdrawal duty checks. Judged assertions, including duplicates, pay
the actor's fee. Refutations reserve shared block prosecution work before variable decoding;
invalid judged proofs cost only their submitter, and excess filings are not judged. Admission
requires the worst program/wire plus court envelope to fit reserved proof work and the carrier.
Nested counts/leaves/paths are bounded before allocation. WholeValue is outside this grammar.
A valid fault slashes the poster once, with only the existing collected-slash 49% bounty;
self-operator bounties are refused and the separate source binder is not charged.

The node trials use synthetic T=2048 encoder and stateful-decoder candidates; the latter
contains a 4 MiB source. A fresh verifier uses its own matching model and the public statement,
convicts a changed intermediate with a dependency proof below 32 KiB, and independently
replays the once-only slash and coinbase redemption. Other trials cover copied proofs, bad
role/plan roots, catalog/capital/withdrawal duty and clocked release, duplicate-post charging
and invalid-proof floods at the existing shared block cap. These are mechanics checks, not
actual checkpoint fidelity, full context/task support or runtime independence certification.

The first node attempts exposed a fixture using the last delivered token as fed input and
an assertion that all five flooded filings pay despite the four-judgment block cap. The
fixture/input and fee expectation were corrected without raising any limit. Final verification
results: **69 distinct tests passed** (36 core, 29 actual-node, four kernel route). All runs
use the final production/test code. Source/binary/log hashes are recorded in
`evidence/rfc02-model-vector-verdict.json`.

This is a non-reward signal. It grants no complete-domain conformance, execution admission or
sampled eligibility. Public trace demand/default, discovery/transport tooling, whole-statement
soundness/policy, legacy flat-digest courts and independent runtime certification remain open.
Opaque/withheld source-PC relations, actual checkpoint/task/context and feature coverage,
full resource/capital/liveness accounting and every other RFC02 §II.14 requirement remain
mandatory. Shipping fences stay dormant and the full RFC02 implementation goal stays active.
