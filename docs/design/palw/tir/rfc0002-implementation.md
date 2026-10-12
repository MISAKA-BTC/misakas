# RFC-0002 implementation ledger

Goal: implement the full [RFC02 completion contract](../../../rfc/0002-palw-tensor-ir.md#ii14-completion-contract--2026-10-10)
from the 2026-10-10 review, preserving A1–A8 and the versioned kernel boundary. **Not complete.**
Baseline: integration `3730cc90f`, plus RFC10 audit `d6a0bd54d`; subsequently integrated
G14 prosecution/bounds and X8R/BUDGET integration through `123d8254e`. Original work branch: `codex/rfc02-implementation`; this review increment is isolated on
`codex/rfc02-review-completion`, based on `392fe80f7`.

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
| Flat-v5 typed node component | Explicit dormant inner 31/32 candidate statements bind the independently derived terminal scope; court folds recheck it, use shared block budgets and share original collateral/eight-statement catalogs with v2 | General public acquisition/localization, proof pre-emption seals, complete execution/checker/DA/network resource vector and execution Final; component tests are not full checkpoint/G14 acceptance |
| Flat-v5 native public acquisition | Dormant inner 33/34/35 fetch authenticated node/hash units into public read-API state; bounded service/default, original collateral and four requester slots shared with v2 | Deployed RPC/transport, retention/pruning, whole-job acquisition/deadline guarantees, proof pre-emption seals, complete shared resources/DA/network load and execution Final; no full real-checkpoint/privacy/liveness acceptance |
| Protected flat evidence seals v6 | Explicit versioned statement and appended route kinds; persisted signature-derived actor seals, deposit/retirement accounting, fixed actor/global quotas, proof-reserve work, explicit expiry and atomic conviction/payout; native validation is recorded below | Complete transport/deadline and adversarial aggregate honest-liveness evidence; production activation; frozen v3 remains historically unsealed |
| Protected public snapshot and seal export | Common op-211 JSON reader, coherent complete rows checked against a separate immutable network/ruleset/tip/root anchor, native context/scope rebuild and exact mature own-seal export; actual node JSON component evidence below | Automatic locally authenticated anchor/state-proof acquisition, live public transport, whole-job retention/deadlines and full aggregate resources remain open; remote self-consistency is not chain membership |
| Flat-v5 streamed detector | Generic exact replay consumes public node/hash paths and acquired registered weights; all 25 primitive, state/history, encoder and decode synthetic trials; the native late-leaf case now uses the generic detector | Shared complete verifier/retry/hash/delivery accounting, optimized acquisition/localization, deadlines/honest liveness, all-feature real-node cases and real-checkpoint/full-task execution Final |
| Flat replay v6 local session budget | New projection binds unchanged v5 scope; native read API and verifier independently derive i128 tensor payload, parameter/trace hash and response bounds; one session charges retries and cached reads | Full peak memory/CPU/metadata/transport/retention accounting, shared consensus admission and per-block/per-bond/network aggregate enforcement, real-size deadline/liveness evidence |
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

`misaka/palw/sdk/examples/check-kernel-node-execution.rs` accepts an arbitrary canonical
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


## Candidate point-unit DA and bounded whole-value prosecution — 2026-10-11

The preceding turn made progress with candidate statement/refutation consumers, 69 passing
checks and verified push at `e453ce83c`. The reference review was read again; the complete
RFC02 objective and all §II.14 requirements remain active.

Authenticated inner 28–30 now request, answer and explicitly close one trace unit. Fixed
node coordinates and a single segment root define the response duty. Axis 0/1 serves an
eligible bounded v3 tile; axis 2 serves only the node commitment/path. Both the existing
withheld mask and Level-1 model-byte mask apply: copying a weight or a model-only value
cannot become a permitted request just because its whole-value envelope exceeds the old
price cap. Parameters and file/inventory ranges have no demand grammar. A verifier still
acquires its model conditionally outside consensus. Large model-only relations beyond the
existing WholeValue price cap remain a G14 gap requiring a bounded court, not disclosure.

Aux 53–56 store requests, four reusable requester-owned slots, served-unit receipts and
an outstanding count per vector. Admission reserves shared work and burns the requester's
fee before program/trace decoding. It fixes the policy response deadline and proof grace
inside the vector's liability horizon. The requester deposit joins ordinary committed
capital and both withdrawal duty gates. Pending requests keep the poster's reservation
held past its horizon until explicitly closed; no network-wide deadline scan is added.
A correct answer decodes no program or trace and consumes no shared adjudication run,
matching the existing bounded segmented response lane. Invalid answers cost their actor.
Collected requester deposits compensate service; late/shared replies cannot retroactively
erase another request's earlier default. Anyone may close, but an unavailable default's
49% collected-slash bounty belongs to the original requester's recorded payout, not the
closer. Availability failure and proven integer refutation are separate once-only outcomes.
Slot replacement deletes the old settled row, preventing stale-generation reuse.

The existing WholeValue fault is now decoded with bounded rank, type, dimension product,
exact tensor byte length, leaf/path/input counts and total carrier. Stored trace bytes are
also included in the request/refutation work price and admission's worst proof envelope.
The program's derived court/filing bounds remain authoritative; no optional tensor bypasses
them. This enables a priced withheld relation to be checked from public openings and the
verifier's own model, including a committed whole-tensor input, without an earlier Final.

The node trials retain synthetic T=2048 and the decoder's 4 MiB source. They publish node
openings and model-independent tiles, read the response carriers back from the selected
chain and authenticate them against the public receipt. With only those replies and its
own model/re-execution, a fresh verifier constructs a sub-32 KiB WholeValue proof and the
node convicts once; independent replay agrees. The first trial assumed the initial Gather
was withheld. The conservative mask showed otherwise at this source size; the adversarial
trace now corrupts a later priced relation with a withheld committed operand. Neither the
source size nor the mask was relaxed. The test also refuses model-only tile requests when
the whole-value price cap leaves that node outside the withheld mask.

Other trials cover malformed answers, paid service, requester slot/capital/duty checks,
valid responses alongside exhausted proof runs, lateness, post-horizon collateral holding,
copy-safe explicit default, cancellation/refund, generation replacement, and later shared
service that must not erase an earlier unanswered deadline. These are mechanics tests, not
checkpoint/task/context support, independent runtime identity or a complete disclosure,
network resource, economic or honest-liveness proof. The policy fee floor is not a measured
producer serving-cost certificate. Discovery/transport tooling, carrier behavior at every
maximum, legacy flat-digest courts, whole-statement soundness, opaque source-PC fidelity,
real checkpoint/common-feature coverage and every other §II.14 requirement remain open.
Shipping fences remain dormant; no sampled reward or new execution eligibility is granted.

**81 distinct tests passed:** 39 core, 31 actual-node and 11 kernel. The scope filter
also matches one segmented-scope regression run by the second filter; that repeat is
counted once (82 total passing executions). The final eight node cases ran after the
public block-reader and explicit saturation assertion were added. The 23 legacy node
regressions ran with identical production code before that last test-only assertion.
Final results, source/binary hashes and logs are recorded in
`evidence/rfc02-vector-da-verdict.json`.


### Candidate parameter-Gather dependency courts — 2026-10-11

The initial DA trial's unpriced first Gather exposed a real gap: the frozen
whole-value estimate opens every leaf of the source parameter, even when the
relation reads only a few selected rows. The new candidate-only whole court
prices those authenticated dependency rows, including trailing dimensions,
full paths and tiles. Rank-one scattered reads retain a whole tile per distinct
index. The index operand, exact reference evaluation and both output hashes
keep their full prices. There is no new primitive or arithmetic, and no model,
compiler or operator name is used for admission.

`model_vector_withheld_mask_v2`, `check_model_vector_positions_v2` and
`verify_model_vector_fault_v2` agree on this candidate contract. The v4/v5
execution claim mask/checker/court, canonical plans and reward eligibility are
unchanged. Candidate posting reserves the maximum of established class bounds
and additional candidate courts. Each refutation must fit its stored header's
work and filing ceiling and its own dependency opening bound; oversized
redundant openings cannot borrow another relation's larger envelope. An invalid
in-memory leaf with no wire form dismisses instead of panicking.

The synthetic decoder keeps its 2048-by-2048 I8 source (4 MiB) and a false first
Gather. A fresh model holder acquires only node openings through signed demands,
reads their answer carriers back from the selected chain, recomputes with its
own model and files the selected registered parameter row. No producer Gather
value or full weight file is required by the court. The trial also checks an
honest statement's public opening, substituted weight bytes, repeated redundant
openings, rejection by the frozen claim court, once-only poster slash/bounty and
independent node replay. Encoder and decoder trials retain the earlier committed
whole-operand case. Cost tests cover ranks 1–4, scattered and duplicate indices,
full coverage, partial tiles and a very large declaration without allocation.

This is a bounded parameter-Gather component. Other axes/batch dimensions,
large committed data sources and large whole outputs still need further courts;
opaque source-PC fidelity, complete-domain policy/soundness, runtime independence,
actual checkpoint/task/context/fidelity and common-feature coverage, RPC/transport,
and the complete resource/liveness/economic contract remain open. These tests do
not certify any real checkpoint or reward eligibility. The RFC02 goal remains
active and shipping fences remain dormant. See RFC02 §14.20.

**94 distinct tests passed (95 executions):** 39 core, 31 actual-node and 24
kernel unit/integration cases. One repeated allocation test is counted once;
three zero-test filter queries are excluded and their intended cases were run by
actual name. All final checks used the final production code. The 4 MiB decoder's
selected-row proof is **3,288 bytes** against a **21,602-byte** opening bound and
**10,019,328 abstract work units**; these are synthetic court measurements, not
real-model performance/fidelity or network serving-cost results. Full logs,
source/binary hashes, commands and limitations are recorded in
`evidence/rfc02-candidate-gather-verdict.json`.


### Staged flat-v4 material for large withheld values — 2026-10-11

Existing v3 rows/columns and their final tiles may expose fewer than 64 elements.
Publishing those leaf hashes would violate the agreed hiding-width rule. The
new `merkle4` module provides a separate commitment grammar with flat 64-element
leaves and the short final tail merged into the previous leaf (maximum 127).
It is not selected by any descriptor, plan, candidate, route, claim or reward.
V3 and the frozen v4/v5 execution/candidate contracts retain their meaning.

The root-only raw-width builder uses fixed scratch; the optional retained tree
prices live digest/vector capacity and offers a pre-allocation retention limit.
Public hash openings and filer-owned value openings have bounded, strict codecs.
An authenticated public hash can be compared with the model holder's cached tree
and localized within the derived logarithmic reply bound, including odd carried
nodes. Each probe reads one digest per level without copying the whole tree.
Independent one-buffer/recursive hashing agrees with the streamed stack root.
Malformed shapes, overflow, short tensors, wrong metadata/type/index/value,
wrong path length, surplus bytes and unsupported commit dtypes are refused.

This is the first material milestone of the large-value path, not a completed
feature, eligible class, actual checkpoint result or model-privacy proof. A
versioned profile, primitive dependency-leaf court and worst resource bounds,
node demand/default/verification integration, fresh model-holder prosecution
and independent Final/redemption still remain. The 64-element width does not
prove entropy or safety for predictable values/cumulative disclosures. These
requirements and the entire original RFC02 goal remain active; shipping stays
dormant. See RFC02 §14.21 and the retained §4.2 milestone sequence.


**40 distinct tests passed:** six final flat-v4 cases and 34 existing v3,
element-court, scope and route-wire regressions. One manual v3 producer timing
benchmark stays ignored and is not counted. The legacy regressions used
unchanged legacy production code before the final flat-v4-only rank-bound
correction; the six flat tests ran after that correction. Logs, source/binary
hashes, exact verification timing and selection audit are recorded in
`evidence/rfc02-flat-v4-verdict.json`. No actual-node flat court/DA/Final
verification is claimed.

### Staged flat-profile dependency-leaf court — 2026-10-11

`flat_court4` extends the flat material with an explicit mixed trace profile and
a terminal relation court: large committable node values use flat-v4, other
intermediates retain v3, and parameter commitments retain v3. No descriptor,
class, candidate-v2 or node route selects this profile. Consumers must explicitly
bind its ID and its complete typed statement before admission.

The court receives an authenticated output leaf digest, not output values. It
recomputes all 64..127 elements with the frozen evaluator from an authenticated,
sorted, duplicate-free union of operand leaves. Source coordinates come from
validated wiring, including carry-in, parameter/layer, fixed-state initialization
and the previous writer, current history row and previous window. The model
holder's recorder uses the same evaluator over its own operands. Court reads use
cached layouts and binary search without a full tensor or element map. Per-leaf
bounds and strict codecs cap carriers, dependency leaves, line/sort scratch,
reference work and metadata; large unbounded lines fail before evaluation.
Root-only hashing of a resident tensor avoids retaining a second Merkle tree.

Synthetic component trials cover all 25 primitives, every leaf and merged tail
of multiple sizes, honest dismissal, substituted weights/routing, missing and
borrowed sources/profile/job input, fixed state and growing/sliding history,
the 1023→1024 boundary, public token tiles, nonzero-axis batched Gather,
authenticated semantic refusal and wide v3 intermediate operands. Truncations,
unknown versions, huge counts, duplicate/excess paths/leaves and surplus bytes
are refused. These are kernel component trials, not actual-node prosecution,
real checkpoint support, model-privacy entropy results or complete security.

Typed node admission, all-profile malformed/decode/small/wide-node courts, hash-only
DA/default/slot budgets and complete statement resources, public-chain acquisition,
fresh prosecution and independent Final/redemption remain open, along with the
original §II.14 checkpoint/task/context/fidelity, economics and liveness contract.
The RFC02 goal remains active and shipping fences stay dormant. See §14.22.

**116 distinct kernel tests passed (131 passing executions):** the complete
library run passed 114 cases, then the final eleven flat-court cases and six
material cases ran against the final binary. Nine initial court cases and six
material repeats are counted once. One manual producer timing benchmark is
ignored and excluded. The complete run precedes final flat-only profile/job/
codec guards and test additions; its other production paths are unchanged,
with only formatting of shared wiring afterward. The final 17 targeted cases
cover the final flat implementation. Commands, exact names, source/binary
hashes, logs, timing and limits are in `evidence/rfc02-flat-court-v4-verdict.json`.
No actual-node flat-profile or full RFC02 completion is claimed.

### Staged all-dtype flat-v5 fault courts — 2026-10-11

RFC02 §14.23 adds a separate internal evidence profile for every node dtype.
Large I64/I128 intermediates now use bounded flat leaves as well; wide commit
points remain prohibited. Small nodes use a whole-value relation court.
Authenticated malformed headers and greedy decode faults have separate courts,
with position binding and deterministic lower-index tie handling. Output
values are not supplied to the leaf/whole courts. The existing flat-v4 namespace
and staged APIs retain their meaning through shared versioned implementation.

Six new court tests and two new independent material tests exercise these
components, together with regression tests for the shared implementation.
The final full-library run passed **124 tests**, with one manual timing benchmark
ignored and excluded. The production library check also passed. Exact source/binary hashes are recorded in
`evidence/rfc02-flat-v5-verdict.json`. Synthetic tests establish component
behavior only. Worst-job admission, typed node binding, hash-unit DA, aggregate
budgets, actual-node prosecution and independent Final/redemption remain open.
No shipping fence is activated; the complete RFC02 goal remains active.

### Staged flat-v5 resource admission — 2026-10-11

RFC02 §14.24 adds a canonical, independently checked terminal-court scope
bound to program/profile/job mode and maximum extent. One reachable block-node
entry counts all layer instances. Derivation reuses the filing price contract,
validates wiring once, caches program size, covers both sides of the 64-element
node grammar boundary and every possible 127-element tail, and allocates no
parameter/value/tree/position image. Malformed and decode budgets join the
maximum court vector. Trace, commitment and flat material prices are recorded
separately from economic work/credit.

The shared flat court price now also charges reference line/sort work for
constants and public inputs, which require no Merkle leaves. Oversized constant
TopK is refused before sorting. Synthetic trial coverage includes all primitive
samples, growing/sliding/transposed wide histories, every actual position/leaf
price, 1,024 layers with state, exact encoder job bindings, changed declarations,
and huge abstract extents without images. A new court regression authenticates
the 63→64→65 grammar transition and window sliding without producer output
values. The final full-library run passed **132 tests**, with one manual timing
benchmark ignored and excluded. The production library check, formatting
check and diff whitespace check passed. Commands, source/binary hashes and
logs are recorded in `evidence/rfc02-flat-scope-v5-verdict.json`.

The terminal scope has no shipping consumer yet. Typed statement binding,
execution/checker/DA integration and aggregate budgets, actual-node acquisition,
fresh prosecution and independent Final/redemption remain open, along with the
full original RFC02 checkpoint/task/context/fidelity, security, privacy,
economics and honest liveness requirements. Shipping fences remain dormant.

### Dormant flat-v5 typed actual-node component — 2026-10-11

RFC02 §14.25 adds explicit non-reward candidate statements and court folds
behind the existing route/PanelFree fences, without changing shipping presets.
The node independently derives the full admitted terminal scope and persists
exact proof/work/scratch limits with program/profile/PC/job/commitment identity.
Courts reconstruct the scope and reject any difference. Fixed metadata prices
and reserves shared work before variable stored program/PC/post/fault decoding.
The old and new catalogs share eight statements per bond, the same original
collateral and the existing block/court budgets. Live reservations also prevent
exit and ordinary reuse of that capital.

Synthetic actual-node tests convict large leaf and small whole relation lies,
dismiss borrowed proofs without harming an honest statement, refuse self-bounty,
redeem the existing challenger payout exactly once and reproduce it on an
independent replay node. Hostile profile/scope/program/PC/role substitutions and
court floods pay only their actors; mixed v2/v3 catalogs cannot double the cap.
The filer uses its own model copy, public job inputs and output commitment/hash
openings, with no producer output values. A helper supplies those public hash
openings in these tests; public acquisition is not implemented or claimed.
The verified selection passes **38 distinct tests**: 11 actual-node tests
(three new), nine core codec/accounting tests (two new), 14 A-2 tests and four
kernel route tests. Production core compilation, targeted formatting and diff
whitespace checks pass. Earlier failed development tests are excluded.
Commands, distinct test names and source/binary hashes accompany this increment
in `evidence/rfc02-flat-node-v3-verdict.json`.

Frozen v2 vectors preserve their wire/root semantics; the separate outer v3
wrapper and route kinds select flat-v5 explicitly. This is candidate evidence,
not execution registration, a full conformance certificate or an execution
Final. Existing candidate admission still restricts the available large shapes.
Public flat hash DA/localization/retention/default/session rules, pre-emption
seals, full execution/checker/DA/network accounting and actual acquisition through
node APIs remain open, together with the original real checkpoint/task/context/
fidelity, privacy, economics and honest liveness requirements. The full RFC02
goal remains active and incomplete.

### Dormant flat-v5 native public hash acquisition — 2026-10-11

RFC02 §14.26 adds inner 33/34/35 for node/hash-unit requests, bounded authenticated
answers and explicit availability default. No answer grammar carries clear model
values. Demand admission rechecks the stored typed scope and caches fixed node/
segment/type/shape metadata. Responding requires no program, PC, trace or value
decoding and does not consume public adjudication slots. v2/v3 requests share
four owned requester slots and their original collateral; a flat vector has a
32-open-request bound. Service collects the requester's deposit for the poster.
Unresolved requests retain poster liability past its horizon; default marks
unavailability separately from integer conviction and pays only the authenticated
requester. Published replies remain in the public native read API after slot reuse.

The actual-node synthetic verifier reads public node/hash openings through that
API, independently computes from its own model copy, adaptively localizes a late
false leaf and files a successful bounded court without producer output values.
Independent replay reproduces the public replies and ordinary challenger payout
redemption. A second trial checks shared slots in both directions, refusal of a
small-node leaf request, invalid answers, service after the court count is spent,
slot recycling, held collateral and one objective default. This is native API
evidence; it does not establish deployed RPC transport or execution Final.

**43 distinct selected tests pass**: 13 actual-node tests, 12 core tests,
14 A-2 tests and four kernel route tests. Five cases are new. The production
core check, targeted formatting and diff whitespace checks pass. New wire tests
cover all truncations, surplus/version changes, nested path/rank counts and
exact unit/shape/namespace authentication; the public reader checks stored
length and digest before returning bytes. Initial development wire-count and
disk-full build failures are excluded. Final node/check commands disable only
incremental caching to fit local disk; arithmetic, optimization/debug profile,
runtime stack and consensus configuration are unchanged. Commands, names,
logs and final source/binary hashes: `evidence/rfc02-flat-hash-da-v3-verdict.json`.

General streamed first-fault verification and localization over all relations,
RPC/transport, retention/pruning, proof pre-emption seals, complete shared
resource/aggregate DA/network bounds and worst-job localization/deadline liveness
remain open. The original checkpoint/task/context/fidelity, privacy, economics,
execution Final/redemption and A6/A8 requirements are unchanged. Shipping fences
remain dormant, and the full RFC02 goal stays active.


## Verified increment — generic streamed flat-v5 detection, 2026-10-11

The new `flat_detect5::check_flat_claim_v5` uses the registered acquired model
and authenticated job to replay the frozen semantics. A public provider exposes
only node commitments and hash paths. The detector checks nodes in execution
order, requests missing/invalid units, localizes a large divergent value within
the declared hash-reply bound and constructs the existing terminal filing from
its own operand values. Small values and greedy decode use the existing v5
whole/decode courts. It wire-roundtrips and court-checks every emitted filing.

Six new tests exercise all 25 primitives, state/history and generated-id feedback,
I128/small downstream values, inline/tiled encoder ids/padding/count, decode ties
and out-of-logits ids, bad model acquisition, substituted jobs and missing/borrowed
public material. The native late-leaf case now drives this same generic detector
using persisted public answer bytes and independently acquired weights, then
convicts, redeems and reproduces the result on an independent replay node. Other
primitive/state/encoder cases remain synthetic kernel trials, not complete real
checkpoint or all-feature native acceptance.

**151 distinct tests passed:** the full kernel suite has 138 passes and one
pre-existing ignored manual hardware benchmark; all 13 model-conformance native
tests pass. Production consensus-core compilation, formatting and diff checks
pass. Commands use `CARGO_INCREMENTAL=0` for compiler caching only, with unchanged
arithmetic, build profile, runtime stack and network policy. Exact test names,
logs and source/binary hashes are in the
[verdict](evidence/rfc02-flat-stream-v5-verdict.json).

The linear node scan, reference hashing, full own operand trees, extra previous
position copy and replay after each demand must still be priced in the complete
resource vector and checked at real sizes. Whole-job DA/transport/retention and
deadline/honest-liveness bounds, proof pre-emption seals, execution Final and the
remaining full RFC02 acceptance rows stay open. No shipping fence is armed.


## Verified increment — local replay resource envelope/session v6, 2026-10-11

Added `flat_replay6`: a separate projection anchored to the unchanged v5 scope.
Native read API and verifier independently derive reference i128 position/operand/
history payload, acquired-parameter and trace/tree hash allowances, node/hash
reads and ten-KiB native reply reservations. `FlatReplaySessionV6` preserves chosen
local tensor/hash/read/attempt ceilings across replies, charges withheld or
inconsistent retries and cached calls, and stops before delegation beyond quotas.
Limits return an explicit result, never a clean verdict or a conviction.

The shared registered-model replay adapter now refuses acquired dtype/shape/count
substitution before canonical-value scanning and hashing, even with the substituted
tensor's supplied commitment. The provider remains responsible for its own bounded
acquisition allocation/transport. Seven new tests exercise these guards and
independently compare actual primitive/state/history payloads with the projection.
The late-leaf actual-node trial obtains the native envelope, retains one session
through DA replies, checks reservations and convicts/redeems with independent replay.

**77 distinct tests pass:** 51 related kernel, one legacy descent, 12 core wire/
identity and 13 actual-node model-conformance tests. Production consensus-core
compilation, formatting and diff checks pass. Compiler caching alone is disabled
with `CARGO_INCREMENTAL=0`; arithmetic, profiles, stack and network policy are
unchanged. Exact commands, logs, source/binary hashes and build provenance are
in the [verdict](evidence/rfc02-flat-replay-v6-verdict.json). The final native run
was repeated after the metadata guard changed; earlier runs are not added to the
count. Core codec tests cover unchanged wire/identity code, not that replay guard.

These are local budget and synthetic/node component results. Payload is not peak
allocator memory; complete CPU, metadata/prompt/path hashing, transport/retention,
consensus and aggregate load accounting, whole-job deadlines/honest liveness,
equivalent-proof pre-emption, checkpoint fidelity/tasks/performance, execution
Final and the other full RFC02 requirements remain open. Historical scope and
statement roots, route grammars and shipping fences stay unchanged.


## Verified increment — sealed evidence contract and pre-emption counterexamples, 2026-10-11

`flat_seal6` adds a staged, salt-bearing vector/actor/exact-carrier commitment and
requires the authenticated filer's own mature seal before a deferred court. No
unsealed fallback or other-sealer beneficiary search exists. Fixed local total
and actor quotas, immutable occupied slots, clipped horizons, checked time math,
one-use successful consumption and explicit constant-key expiry bound this book.
Failed or honest proofs leave their row unspent. Rejected seal metadata and
oversized carriers cannot enter the expensive court callback.

Seven new kernel tests include distinct authentic decode rivals with the same
conviction, including an out-of-range delivery. A new actual-node counterexample
adds a redundant authentic prompt tile to an inline-bound encoder filing. Both
carriers convict the same relation and have distinct bytes/digests. The local v6
contract rejects the changed carrier, but **frozen native v3 remains unsealed**:
the earlier copyist receives 49 mega, the original finder receives zero, the
poster loses its 100 mega reservation once, and independent replay agrees.
This test reproduces a security failure; its green test status is not a protected
payout acceptance claim.

**166 distinct tests pass:** the full kernel suite has 152 passes and one
pre-existing ignored manual hardware benchmark; all 14 native model-conformance
tests pass. Production consensus-core compilation, formatting and diff checks
pass. Final commands, exact test names, logs, source/binary hashes and limits are
in the [verdict](evidence/rfc02-flat-seal-v6-verdict.json). Earlier filtered runs
are excluded. Compiler caching alone is disabled with `CARGO_INCREMENTAL=0`.

The next required change is a versioned protected consensus statement and
seal/reveal path, with authenticated persisted rows, original capital, shared
processing quotas, fees and atomic once-only payout, refusing the old unsealed
bypass. Merely adding optional seals beside inner 32 cannot close the gap.
Network capacity/honest inclusion, complete shared resources, execution Final,
real checkpoint/task/fidelity/performance, A6/A8 and all full RFC02 requirements
remain open. No frozen route/statement grammar or dormant shipping fence changes.


## RFC02-T takeover — merge onto the G14 integration line and the protected candidate refutation (P0), 2026-10-11

Lane RFC02-T (Claude) took RFC-0002 over from codex. Branch `rfc02/takeover`: the integration line
`claude/g14-public-prosecution-integration-9bee39` @ `3076077f9` merged with `codex/rfc02-implementation` @ `392fe80f7`
(36 commits on the older integration `123d8254e`), then the P0 below. Nothing is armed; no testnet-12 id moves.

### Merge: semantic conflicts and how both behaviours were kept

* **Class rows.** Registration derives G14-B's `liability_daa` and G14-S's `court_scope` for every row, then codex's admission-work
  check, then either the execution class or the conformance-only candidate. A candidate carries both fields; an unbounded pursuit is
  carried as `0` (`ledger::candidate_liability_daa_v1`) instead of refused, because a candidate holds no claim and every real-shape v4
  profile is refused at one route chunk a DAA (refusing would make actual-weight candidates impossible; codex's
  `artifact_tile` test caught it). Promotion is an ordinary registration and is refused by name. Stored candidates rebuild with the
  same rule (`rows::candidate_class_row_of`).
* **Claim path.** `CommitClaim` / `CommitSegmentedClaim` run G14-S's `class_court_scope_v1` gate, then codex's
  `check_claim_carrier_v1`, then codex's duplicate → reveal → capacity → admission-work charge order.
* **Overheads.** One derivation each: FileProof / Respond envelopes from `budget::*_ENVELOPE_BYTES_V1` (G14-B; byte-identical to
  codex's literals), the commit envelope from `gate::CLAIM_CARRIER_OVERHEAD_BYTES_V1` (2^16, the value both lanes had).
* **`element::PreparedV1`.** Integration's mode form (`modes`, `kept`: Clear / Withheld / Hiding) is kept; codex's `withheld` mask
  becomes a profile: `set_candidate_v2_scope` sets the candidate trace v2 modes (withheld where `model_vector_withheld_mask_v2`,
  else clear, never hiding — every candidate commitment is v3). Codex's `node_count` / `without_scope` / `pub(crate)` accessors are kept.
* **Kernel ledger table 28 collision.** G14-S's `TABLE_COURT_SCOPE_V1 = 28` stays; codex's conformance candidates moved to **29**
  (`TABLE_CONFORMANCE_CLASSES_V1`). Both root extensions apply in a fixed order (28, then 29), each only when its table is non-empty, in
  `rows::root_of_rows` and `state::root`. Registry row appended to `remaining-rfc-integration-matrix.md`; A2U `StateEncoding` row.
* **Semantic fixes the textual merge missed** (found by the builds/tests, not by conflict markers):
  - G14-S raised the whole-value court price (hiding-tile operands); codex's candidate v2 pricing reused it. Candidate operands are
    never hiding, so `seg_scope::whole_value_court_cost_in` drops the hiding-tile term for the candidate profile only.
  - The verifier's own replay committed under the frozen modes even for candidates (C_hide vs the candidate's v3: divergence
    everywhere). `seg_detect::replay_positions_v1` now takes the profile's modes; new candidate entry points
    `prepare_model_vector_reexecution_v2`, `check_model_vector_by_reexecution_v2`, `reexecute_model_vector_v2` and
    `element::prove_model_vector_element_v2`.
  - G14-S appended `WholeOperandV1::hidden`; codex's bounded candidate fault decoder (`palw_model_vector_v2`) now reads an empty
    vector and refuses anything else.
  - codex tests updated to the merged APIs (`SegMaterialV1::commitments(program, p)`, `seg_commitments_of_trace_v1(program, …)`;
    candidate producers commit v3). Codex's plan pin `0e31e5b9…` for the actual 32-position Qwen2.5 pilot is re-pinned `00df9d85…`
    (G14-S prices are part of the plan); the allocation-free leaf pricing still equals the index-vector oracle on the real shapes.
  - Codex's `vocabulary_sized_lookup_whole_court…` test: under the frozen profile that value is now hiding (asserted); the flat-gather
    whole court is exercised under the candidate profile.
  - `k2_ledger_route`'s "64 zero bytes are not an object" loop: inner 30 / 35 (settle) ARE one digest; they decode and are refused by
    rule (pre-existing on codex's branch).
  - `misaka-palw-sdk` encoder route lacked codex's `verification_mode_requirement`; `misaka-palw-gateway` had a duplicated `})` from
    the textual merge.
* **Not passed — ignored with reason:** `real_scale::actual_native::recorded_actual_weight_delivery…` replays a bundle codex recorded
  on the pre-G14-S frozen profile (old plan, no hiding modes). Re-recording needs the actual weights (not in the repository).
* Docs `g14-completion-matrix.md` (integration rows, codex's F3–F4 additions) and `k2-real-scale.md` (codex's §14 subsections before
  G14-S's §15) keep both; headings stay unique.

### P0: frozen candidate refutations were unsealed — the protected route

**Counterexample (codex, `flat_seal6`, re-run).** Below the new fence the frozen route is unchanged:
`flat_v3_node_preemption_counterexample_and_staged_v6_seal_rejects_equivalent_carrier` still shows the copyist's equivalent
re-encoding (a redundant authentic prompt tile) collecting 49 and the finder zero. It is the frozen behaviour, not a protected payout.
The same gap holds for inner 23 (model artifact binding) and 27 (model vector v2): all three pay the first included filing.

**Change** (`consensus/core/src/palw_model_refutation_seal_v4.rs`, fold `palw_model_refutation_seal_fold_v4.rs`), behind the new
dormant Params fence **`palw_model_refutation_seal_v1`** (Some-only hashed, `never()` collapse, refused when armed; needs the kernel
route and `palw_panel_free_v1` at or below it):

* inner **36** `SealModelRefutationV4 { slot, seal }`, `seal = H(kind ‖ target ‖ kernel-bond(signer) ‖ salt ‖ H(proof))` — target,
  proof and salt hidden; the actor is the authenticated signer, never a field. Rows in kernel route aux table **64**, key
  `(bond, slot)`, 4 slots per bond, fixed size, constant-key reads. A live row reserves the route policy's `seal_deposit` of the bond's
  original collateral (in `palw_bond_committed_v1` and both withdrawal gates); posting spends the block's shared route budget (never
  the proof reserve). An identical re-seal is a no-op (G14-R's rule); another seal cannot take a live slot; an expired slot is reusable.
* inner **37** `RevealModelRefutationV4 { slot, kind, target, salt, proof }` — admitted only with the signer's OWN seal in that slot,
  at least `claim_seal_delay_daa` old and unexpired (`seal_ttl_daa`), opening to exactly these bytes (one bounded digest before any
  decode). Then the kind's frozen court runs unchanged (`refute_outcome`: block budget, dismissal fee, persisted envelope, same-operator
  refusal, atomic slash-and-pay, `refuted` flag → once only). The seal is consumed when a court ran (proven or dismissed).
* Past the fence inner 23 / 27 / 32 are refused by name before any court, fee or row. No unsealed fallback, no beneficiary search.
* A-2: inner 36–37 ride the inner-kind gate (`PalwKernelInnerFenceV1::ModelRefutationSealV1`); below the fence the mempool and the
  fold refuse them and nothing is written. No int-12 wire type changes (tag 110 is post-int-12; no `PALW_INT12_WIRE_CHANGES_V1` row).

**Economics (interim policy values, not measured):** reservation 100, bounty 49 % = 49, seal deposit 1 (refundable, locked ≤ TTL).
The honest finder's cost is carrier fees; its reveal pays 49. A copyist needs a mature seal of its own bytes before the finder's
reveal is included; it learns the bytes only from that reveal, so it must keep the finder's reveal out of every block for
`claim_seal_delay_daa` (stated censorship residual; the maturity is a policy constant to set with the economics, interim 1 DAA). A
poster refuting itself through a bond of another operator loses 51 + fees; the same operator is refused. If a poster pre-empts an
honest finder with its own sealed self-refutation, the finder's reveal is refused before any court (no fee): it loses only carrier
fees and keeps its deposit — it does not recoup them (residual).

**Not closed by this change (UNKNOWN / not tested here):** the kernel claim court's own `FileProof` (G14-R4 GAP-R7) pays the earliest
sealer of the EXACT bytes, else the filer. If a `ProsecutionV1` admits an equivalent re-encoding (as the flat filing's redundant
prompt tile does), a copyist that is included first would collect there too; that is G14's route and was not tested by this lane.
Deferred priority across sealers (earliest seal wins within a contest window, to blunt short censorship) is not implemented.

### Tests (targeted; PASS / FAIL / UNKNOWN)

Merge commit (merge-only tree): kernel all targets **371 PASS / 0 FAIL / 8 ignored**; consensus-core
`palw_lifecycle_objects_v2` 31 PASS; node `t12_a2u` 3 PASS; `g14_kernel_route_e2e::model_conformance` 14 PASS (codex's actual-node
tests, including the frozen counterexample); `::model_artifact` 4 PASS; `::artifact_tile` 3 PASS; `::real_scale` 18 PASS + 1 ignored
(above); `g14r_` 14 PASS; sdk `kernel_execution` 3 PASS; sdk `k2s_encoder_court` 3 PASS + 1 **UNKNOWN** (the worst-geometry frontier
search was stopped after ≈51 CPU-minutes, not completed); `cargo check --workspace --all-targets --exclude misaka-palw-worker` PASS.

P0 commit: kernel `route::` 5 PASS (seal/reveal round-trip and ceilings), `k2_ledger_route` 22 PASS; consensus-core
`palw_model_refutation_seal` 3 PASS (seal binding, row, fence dormant / refused when armed / Some-only hashed), `palw_lifecycle_objects_v2`
31, `fork_id` 21, `palw_verifier_pay` 2, `params::` 152 PASS; node `model_conformance` **16 PASS** — the two new actual-node tests
`flat_v4_sealed_refutation_pays_the_original_finder_and_refuses_the_copyist_and_the_unsealed_bypass` (unsealed 32 refused for both;
finder's seal and deposit; identical re-seal no-op; copyist without a seal refused; same-block race → finder paid 49 once, poster −100;
copyist's later mature reveal refused before any court, no fee; self-operator reveal refused; deposits released at expiry; replay node
identical) and `model_refutation_seal_objects_are_inert_below_their_fence` (mempool refuses 36/37 below the fence; no row) —
`model_artifact` 4, `t12_a2u` 3, `g14r_` 14 PASS; `cargo check -p kaspa-consensus-core -p kaspa-consensus -p misaka-palw-kernel
--all-targets` PASS. The A2U int-12 replay script (`scripts/verify/a2u-int12-replay.sh`) was not run.

Status: the protected refutation is **implemented and verified on the synthetic actual-node harness**, NOT armable (its fence is
refused when armed, like every RFC fence until the single activation release).

### What remains (RFC02-T's list, honestly)

* P0 residuals above: censorship of the finder's reveal for the maturity window; a pre-empted honest finder recovers no carrier fees;
  the kernel claim court's `FileProof` equivalent-copy question (G14); the interim policy values (maturity 1 DAA, TTL 100, deposit 1)
  need the economics ruling; no RPC op exposes seal rows yet (the native read API does: `model_refutation_seal_v4`).
* Item 2 (flat-v5 courts to node registration / DA / Final, RPC reads, retention, execution Final / redemption) — not started in this
  lane's session.
* Item 3 (all-relation verification and the complete resource vector at real sizes, consuming `budget.rs`) — not started.
* Re-record the actual-weight delivery bundle under the merged frozen profile (needs the weights).
* Left with codex: all-family canonical registration/eligibility without test exceptions; public bootstrap of FP job/input/initial
  state and re-acquisition after permanent pruning.


## RFC02-T second round — kernel FileProof re-encoding (P0 residual), flat-v5 pursuit gate, seal RPC, 2026-10-11

Branch `rfc02/takeover`, fast-forwarded to the integration line `5cb381187` (which already contained the first round, codex G14
`4f1120d9e`, G14-K and G14-L), then the items below. Nothing is armed; no testnet-12 id moves.

### P0 residual: the kernel route's own FileProof — YES, the copy theft applies there too (now closed past the fence)

**Finding.** A `ProsecutionV1` is not a canonical encoding of its conviction. Two concrete re-encodings of one authentic
`ProsecutionV1::Kernel` proof convict identically: (a) every non-MatMul fault kind ignores `FaultProofWireV1::scalar`; (b) a MatMul
scalar court reads only the three openings, so an extra (unread) `prior_rows` entry changes the bytes. GAP-R7 pays the earliest mature
sealer of the EXACT bytes, else the filer, and G14-R's identical re-seal no-op does not help (the copy has other bytes). So a copyist
that reads a sealed finder's filing and is included first is paid as the filer and the finder's filing is a `Duplicate`.
Segmented filings very likely admit the same (a redundant prompt tile, as codex showed for flat filings); not separately tested.

**Counterexamples (before):** kernel `sealed_bounty::rfc02t_frozen_gap_r7_a_reencoded_copy_filed_first_takes_the_sealed_finders_bounty`
and the real-node test `g14_kernel_route_e2e::rfc02t_sealed_bounty::…` (first half, MatMul scalar proof with an extra prior row):
the copyist is paid the 49 %, the sealed finder 0, and the replaying node agrees.

**Rule (after; `misaka_palw_kernel::sealed_bounty`, consumer-injected past the existing fence `palw_model_refutation_seal_v1`):**
any authentic filing still convicts (court liveness unchanged — unsealed and direct filings keep working). The bounty is paid at once
only if some bond holds a mature seal of the convicting bytes (GAP-R7 unchanged). Otherwise the filer's share is HELD in kernel ledger
table 31 for the route's `seal_ttl_daa`; a bond whose own seal on the claim predates the conviction, is mature and opens to the proof it
now files may contest it with that proof against the convicted claim (court budget and scaled dismissal fee as any filing) and takes
the held share once; with no contest by the window's end the original filer is paid. A copyist cannot hold such a seal: it learns the
bytes only from the finder's filing. `LedgerEventV1` 40 `BountyHeld` / 41 `HeldBountyPaid`; the tick expires windows; table 31 is in
the route's rows and root extension only once non-empty (A2U row).

**Tests (after):** kernel `sealed_bounty` — held then paid to the earlier sealer in the same block; a seal posted after the conviction
and a proof that does not open the seal are refused free; with no contest the filer is paid at the window's end, once; exact sealed
bytes still pay GAP-R7's sealer immediately. Real node: the second half of the node test — the copy's conviction stands (producer
slashed once), its share is held, the finder's own sealed proof filed in the next block takes it, the copyist's retry pays nothing,
replay identical. Residuals: a finder censored for the whole window loses the share to the filer; its seal must still be live.

### Item 3 (part): a flat-v5 candidate's pursuit on G14-B's clock

`misaka_palw_kernel::flat_pursuit5::flat_pursuit_v5` derives the worst-case public pursuit of a flat-v5 candidate statement from
`crate::budget` (`PursuitClockV1::of_opv`, `response_window_daa_v1`, `route_object_chunks_v1`): ⌈log2 min(P,1024)⌉ position halvings
+ ⌈log2 N⌉ node halvings + ⌈log2 L⌉ leaf halvings + one operand round, each a one-chunk (10 KiB) reply at its base deadline, then the
protected filing (seal maturity, prove, chunks). Past the fence the flat post fold admits a statement only if that lands inside its
200-DAA horizon (`FLAT_PURSUIT`). Binary rounds over four slots is an upper bound; verifier compute enters only through the OPV
policy's declared budgets. Derived numbers (kernel test `flat_pursuit`, kernel test policy + `opv_example`, not measured): the
toy dense-MoE at 5 positions needs 15 rounds (3 position + 9 node + 2 leaf + 1) of a 20-DAA window and lands at DAA 394 — it would
NOT fit a 200-DAA horizon under those terms; at the actual 32-position Qwen2.5 pilot shapes the flat-v5 scope itself refuses
("flat relation exceeds court resource caps"), so no real-size candidate is admissible yet. Under the node's interim route/OPV terms
codex's small flat fixtures are admitted (the node tests pass with the gate active). The 200-DAA statement horizon vs these
pursuits is an open sizing decision (horizon, per-round deadline, k-ary rounds over the four slots).

### Item 2 (part): RPC op 213 `getPalwModelRefutationSeals`

One bond's seal rows (slot, seal, posted/mature/expires, deposit, live, revealable), its reserved deposit and the aux root; wRPC +
gRPC 1300/1301, service handler, builder and parser in `kaspa_rpc_core::convert::palw_kernel` (the node test carries the builder's
answer through JSON), integration test arm. Registry rows appended.

### Not done (honest)

* Item 2 proper: flat-v5 courts are still candidate statements — no execution registration, DA-to-Final connection, retention/pruning
  rules or execution Final/redemption for flat-v5. Not started.
* Item 3 proper: all-relation verification at real sizes and the complete resource vector (CPU, metadata hashing, transport,
  retention, aggregate load) — only the pursuit deadline is derived here.
* Segmented-filing re-encoding not separately demonstrated (the held-bounty rule is mechanism-level and covers every `ProsecutionV1`).

### Wrap-up (2026-10-11): lane RFC02-T stops; codex continues RFC-0002

The user decided codex keeps implementing RFC-0002 (`codex/rfc02-review-completion`); this lane stops here and did NOT merge that
branch. Status of this second round: the kernel FileProof re-encoding check is **PASS as a finding and as a fix on the synthetic
harness** (kernel `sealed_bounty` 4 tests, real-node `g14_kernel_route_e2e::rfc02t_sealed_bounty` before/after). Implemented and tested
but **WIP / not reviewed**: the flat-v5 pursuit gate (sizing open, real-size scope refuses) and RPC op 213 (wRPC/gRPC wiring compiled and
round-tripped through JSON in the node test; the integration-test arm and the gRPC conversion are compiled, not run against a node).
Pre-existing on the integration line and not touched here: consensus-core `class_verify_deadline::td0_the_final_floor_has_one_producer`
fails on `5cb381187` (G14-L's line 31592).
## Verified increment — protected native flat statements and seal/reveal v6, 2026-10-11

The new explicit protected statement and inner kinds 36–39 implement the consumer
half of the former local seal contract. The unchanged v3 body is wrapped with
version 6; a distinct trace-root domain binds its fixed seal policy. Independent
admission, declared implementation roots, persisted body/view reconstruction and
terminal courts all use that protected root. Inner 32 refuses this identity,
while historical v3 encodings and their reproduced counterexample remain intact.

The signing bond determines the actor. Fixed persisted rows bind that actor,
vector, salt-bearing exact-carrier seal and immutable clocks. Four slots per
bond and 1,024 total slots constrain pending state. Posting charges shared work
and reserves the original policy deposit; common committed-capital/live-duty/
retirement/withdrawal queries include it. Explicit bounded expiry forfeits an
unspent expired deposit or releases it after a decided vector, removes the row
and decrements the catalogs/count. Pending expired rows cannot evade liability
by merely advancing DAA or replacing a seal.

A single proof-reserve debit covers metadata/hash and the admitted court before
variable decoding. Registration prices that same worst debit. Invalid seals or
courts pay the authenticated actor; false/honest proofs retain their unspent
slot. A successful fold atomically consumes the seal, releases its deposit,
settles the original vector collateral once and credits the revealing actor's
490-permille share. Separate source/statement/actor/clock/profile/resource checks
remain mandatory. No reward registration or execution Final is granted.

**171 distinct tests pass:** full kernel 153, actual-node model-conformance 17,
and one small integration codec/identity/fence test. The kernel's pre-existing
manual hardware benchmark remains ignored. Production consensus-core checking,
changed-file formatting and diff checks pass. Final commands, exact names,
logs, binary/source hashes, earlier corrected test failures and build provenance
are in the [verdict](evidence/rfc02-protected-native-v6-verdict.json).

The three new native trials verify both encoder and decoder candidate courts,
old-refute bypass refusal, copied seals, alternate equivalent carriers, late
immature seals, failed court retention, bounded actor slots and original capital,
explicit expiry/forfeiture, one total payment including already redeemed outputs,
independent-node replay, and a mature reveal using the proof reserve after an
admission flood. These are synthetic actual-node component results. The separate
codec test checks bounded counts, truncation/trailing bytes, distinct identities,
new route roundtrips and their dormant owning gates.

The full RFC02 goal remains active. Needed next: expose the protected path through
the public SDK/CLI/transport and whole-job acquisition; validate retention,
worst-job deadlines and honest inclusion under aggregate invalid-proof/seal load;
complete the shared resource/economic contract and versioned length/sparse/state
semantics; obtain real checkpoint fidelity/task/performance and independent
execution Final/redemption evidence; meet A6/A8. Existing v3 statements remain
historically unsealed. No shipping fence is activated.

## Verified increment — protected candidate SDK/CLI preparation, 2026-10-11

`flat_candidate_v6` and `palw-class flat-v6 post|seal|reveal|expire` now prepare
reviewable signed protected objects using the existing network/bond/bytes message
and ML-DSA context. Post preparation bounds serialization and exact decoding,
requires all source implementation records to agree on the original v3 root
for the supplied class/binding, preserves job/evidence/scope, and only then binds
those records to the new protected root. Divergence cannot be silently rewritten
as declared agreement. This is a declaration migration, not proof that three
implementations executed independently.

Seal preparation writes its public commitment separately from a private local
network/vector/bond/salt/exact-proof record. The CLI draws the salt from the OS
RNG, creates files exclusively with Unix mode 0600, bounds raw and nested counts,
redacts private proof/salt from reports and debug output, and never overwrites
existing files. Ordinary I/O errors clean up only newly created outputs. Reveal
preparation requires the same network/bond and passes the supplied fixed row and
height through the common mature/unspent/exact-carrier guard. That supplied row
is explicitly unverified until obtained from authenticated local chain state;
native acceptance independently authenticates its own row and DAA.

Large signed objects can be split with the existing bounded generic carriage,
keeping the exact whole-object signature. The CLI writes the whole and listed
parts for `misaka palw submit-object`; it submits nothing. The large-carrier trial
checks only chunk reassembly/signature using a deliberately hostile proof, not
court authenticity or timely consensus inclusion. The shared generic eight-group
table still needs a protected proof transport and aggregate liveness evidence.

**23 distinct selected tests pass:** six new SDK/CLI tests and all 17 native
model-conformance tests. Both encoder/decoder protected node trials now use the
SDK's post preparation and private seal/reveal helpers, then convict, settle
original capital once, redeem and agree on independent-node replay. Other trials
retain actor quotas/expiry and reserved proof processing after admission flood.
Production SDK/CLI checking and four-file targeted formatting pass. The two
pre-existing SDK entry-file formatting differences match the baseline exactly;
diff whitespace checks pass. Final commands, exact test names, source/binary/log
hashes and corrected development compile errors are in
[the verdict](evidence/rfc02-flat-sdk-v6-verdict.json).

The unchanged kernel's 153 passes and the core codec's one pass remain recorded
in the preceding protected-native verdict; they were not rerun for SDK-only
production additions. Together with the updated 17 native and six SDK tests,
these are 177 distinct passing component tests, not full RFC02 acceptance.

Full RFC02 remains active. Public RPC/acquisition and authenticated snapshot/row
discovery, retention/pruning, protected large-proof transport, whole-network
invalid-proof/seal saturation and worst-job inclusion/deadlines still need work.
The complete shared resource/economic contract, versioned length/sparse/state
semantics, actual checkpoint/task/context/fidelity/performance, independent
execution Final/redemption and A6/A8 requirements are unchanged. No shipping
fence is activated and historical v3 statements retain their unsealed semantics.

## Verified increment — mature actor-owned protected proof chunks, 2026-10-11

Append kernel chunk target 3 `FlatProofV6 { vector, actor }` to the existing
outer-113 lane. Parts require the authenticated opener's own mature unspent seal
and an open protected vector. Every continuation rechecks those fixed facts;
the deadline is strictly before seal expiry/vector liability. The assembled
inner must be that actor's signed protected reveal of that vector, and both
signature layers, exact group hash and final salt/proof/context/court checks
remain mandatory. Missing or borrowed actors cannot open a group.

The route's existing two groups per signing bond, original per-part deposit and
64-DAA clipped TTL provide the actor's room independently of the generic shared
eight-slot table. Accumulated assembly is bounded before variable decode to
one-MiB proof plus 16-KiB signed envelope. Shared metadata processing reserves a
conservative part + three assembly-byte allowances before variable group decode;
a live mature target uses the proof reserve. Invalid fixed targets use admission
capacity and pay the authenticated actor's tariff. Existing malformed-group
rehearsal/drop behavior remains; final court charges its own protected debit.
Opening/continuing separately checks that the completing part plus maximum
native reveal debit fit together within guaranteed work and two runs. It refuses
insufficient/overflowing envelopes before opening a group; candidate admission
alone does not grant this transport capability.

Common per-bond slot/deposit queries now walk only the exact bond-key prefix and
fixed headers. The closing step reads fixed expiry/deposit headers and preserves
strict `now > expires` forfeiture without copying all pending proof bodies every
block. The global fixed-row scan remains a separate aggregate resource concern.

The SDK verifies the assembled network/key signature and supplied own mature seal,
then signs the existing kernel parts. CLI large reveals use this path and retain
the whole object and listed parts for review. The generic SDK splitter explicitly
refuses large protected reveals; posts continue using generic transport. Offline
row/height trust remains `UNVERIFIED_REMOTE_STATE` until locally authenticated.

**29 distinct selected tests pass:** 18 native model-conformance tests, seven
SDK/CLI tests, two core codec tests and two legacy chunk regressions. Production
core/SDK/CLI checking, seven-file targeted formatting and diff whitespace checks
pass. The unchanged kernel evidence contributes 153 passes and one ignored
hardware benchmark; the combined recorded component total is 182 distinct
passes, not full RFC02 acceptance. Final validation is recorded in
[the verdict](evidence/rfc02-flat-chunk-v6-verdict.json). The native synthetic
trial forces a valid bounded court into multiple 1-KiB parts, holds all eight
generic groups open, refuses missing/borrowed actors, holds/releases original
chunk deposits, convicts and pays once, and agrees on independent replay. The
SDK test verifies maturity/actor/network refusal, both signatures and byte-exact
assembly; core tests check target codec/truncations, inner actor/vector binding,
bond-prefix capital and strict fixed-header expiry.

This is protected transport component evidence, not a real checkpoint larger
than a carrier or whole-network inclusion/retention acceptance. Honest inclusion
under aggregate mature-invalid-proof/seal load, all-target fixed metadata/state
and expiry-sweep bounds, complete shared resources/economics, whole-job public
acquisition/retention, actual checkpoint/task/fidelity/performance, versioned
long/sparse/state dimensions, execution Final and A6/A8 remain open. Shipping
fences stay dormant; full RFC02 remains active.

## Verified increment — protected public snapshot and exact seal export, 2026-10-11

Reuse the common op-211 snapshot collector for protected reads. The exposed
`into_route_v1` preserves its complete/coherent page, ordered row/cursor,
served-root and header-clock checks; onboarding retains all downstream policy
and program binding checks. The protected reader additionally compares tip,
network/ruleset and both roots to a separate caller-authenticated anchor and
keeps that anchor immutable after construction.

Candidate reads require the requested protected vector/class/model statement and
independently reconstruct the native typed context, resource scope and parameter/
input bindings. Seal export requires the exact bond's mature unspent row and an
open candidate at the anchored tip. Reveal preparation/native acceptance still
check private exact bytes, current state/clock/bond status and the terminal fault.
Matching an anchor copied from remote pages cannot authenticate chain membership.

The common SDK parser consumes the actual op-211 JSON response grammar, bounds
JSON/hex/counts before additional binary copies, and rejects malformed/unknown
fields, tables and terminal cursors. Offline `palw-class flat-v6 snapshot` accepts
one response per NDJSON line plus a separate anchor file. It writes only the exact
checked seal row, using exclusive mode-0600 creation, and reports the anchored
height with `MATCHED_SUPPLIED_ANCHOR_CHAIN_TRUST_REQUIRED`; it sends no carrier.
The existing 128-MiB decoded collector ceiling is a local reader limit, not peak
RAM, whole-job consensus capacity or a producer constraint.

**36 distinct selected tests pass:** all 18 native model-conformance tests, four
new snapshot/CLI tests, seven protected SDK/CLI tests and seven existing
onboarding-snapshot regressions. Production SDK/CLI checking, seven-file targeted
formatting, CLI help and whitespace checks pass. Final validation is recorded in
[the verdict](evidence/rfc02-flat-snapshot-v6-verdict.json). The protected native
chunk trial now reconstructs its candidate and mature seal from the actual op-211
response builder and JSON wire grammar, with its separately authenticated local
consensus anchor, before conviction, capital settlement and independent replay.
SDK fixture trials cover anchor/root/scope/identity/clock/row substitutions,
bounded malformed RPC material, exact offline export/no-overwrite and incomplete
snapshots; their local roots are not node admission or real-checkpoint evidence.

Automatic local anchor/state-proof acquisition and public network transport,
retention/pruning, whole-job honest inclusion/deadlines, all shared aggregate
resources/economics, versioned long/sparse/state semantics, actual checkpoint/
task/context/fidelity/performance, execution Final and A6/A8 remain open.
Shipping fences stay dormant; full RFC02 remains active.

## Verified increment — bounded TopK selection and point counterexamples, 2026-10-11

The v1 reference and typed backend preserve the same score-descending, lower-id
on tie and ascending-output-set rule. Small K uses a K-candidate heap; growing K
uses repeated at-most-2K-index partitions, with a direct known set for K=N. The
backend reads original native storage through its layout without whole-source
widening/gather. This preserves O(K) selection memory and O(N+K log K) execution
comparison growth for resident scores, rather than introducing an N log K
regression when K grows. It changes no primitive id, canonical work or v1 limit.
The pinned [Rust 1.93.0 partition contract](https://raw.githubusercontent.com/rust-lang/rust/1.93.0/library/core/src/slice/mod.rs)
supplies the in-place linear partition bound. Remote callback acquisition uses
the separately priced local checker heap; repeated score reads are not assumed.

Add explicit `topk-point-court/v7`, its 128-KiB bounded exact fault codec, an
independently derived relation scope and a prepaid ordinary scan session. Range,
adjacent non-increasing order (including duplicates), and an authenticated omitted
rival that dominates a selected member cover the complete frozen TopK set rule.
At most three selected-output and two score leaves authenticate to exact node/
parameter roots and the same axis row. Missing/borrowed/wrong-profile/changed-root/
malformed/over-budget material dismisses; no axis reconstruction occurs in court.
Registered i64 parameters stay legal, i128 parameters stay forbidden, and computed
i128 node scores use the existing flat-v5 typed grammar. No type rule is relaxed.

The scanner checks complete work and auxiliary limits before callbacks and spends
an attempted acquisition even when a read fails. Its 32-MiB auxiliary ceiling and
64K temporary-memory price exclude source/output residency and all whole-job/
network material. Filing, metadata/hash court work and scratch are separately
bounded and a cheap declaration cannot replace independent derivation. This is
not the required unified all-plan/aggregate resource vector.

**200 distinct selected tests pass:** 153 existing kernel tests plus six new point
court tests, 13 TIR tests, nine backend tests, one independent-reference
40,000-case differential test, and 18 actual-node model-conformance regressions.
The existing hardware microbenchmark remains ignored. Ten new tests cover all
native widths, axes/ranks, offset/transpose/slice/broadcast views, K=64/65,
partition tails and K=N, 2,268 exhaustive small score/set combinations, codec/
root/coordinate/dtype/scope refusal, acquisition precharging and an actual large
synthetic fixed axis. Golden vectors reproduce byte for byte; 2,000 backend
programs, 600 mid-run programs and 300 post-write cases agree with the reference.
Independent ref2 includes 1,786 TopK cases. Targeted seven-file formatting and
whitespace checks pass. Exact commands, names, source/binary/log hashes, measured
material, corrected development fixture failures and limits are in
[the verdict](evidence/rfc02-topk-point-v7-verdict.json).

The synthetic N=10,485,760, K=2, one-position source has 41,943,040 logical i32
bytes (167,772,160 bytes in the reference's i128 Tensor representation). The
native typed kernel reads 41,943,040 bytes without widening scratch. Its single
debug measurement is 222 ms, not end-to-end/checkpoint throughput. The wrong
selection's authentic terminal filing is 17,485 bytes, with independently derived
ceilings of 37,744 filing bytes, 4,934,017 court work, 617,312 court scratch bytes,
251,658,288 ordinary scan work and 128 selection auxiliary bytes. Whole-source
acquisition/hash/retention/delivery costs are not replaced by that small filing.

Existing native tests rerun candidate admission, protected seal/chunk/public
snapshot, reserved proof processing, actor capital/payout and independent replay
on the updated v1 implementation. They do not exercise a v7 node route: no current
post, reveal, descriptor, default decoder or economic/Final lane opts in. Frozen
flat-v5 whole-axis scope still rejects the large relation. The prior protected
component pool plus these new/updated groups records 222 distinct component
passes; this is not RFC02 acceptance or checkpoint/task coverage.

Full RFC02 remains active. The next consumer must bind all relations and normal
verification, the explicit court id, public acquisition/localization and admitted
resources/transport. Versioned token/patch/audio lengths and H-axis sparse/state
semantics, shared per-block/per-bond/network resources and economics, full public
retention/inclusion/deadlines, real checkpoint/task/context/fidelity/performance,
fresh outsiders, independent execution Final/redemption and A6/A8 remain required.
Missing score material adds no availability default: conditional external mode A
and all dormant shipping fences remain unchanged.

## Verified increment — complete mixed-court graph scope and independent replay, 2026-10-11

Add an explicit v7 court-set/resource identity over unchanged flat-v5 internal
and v3 parameter commitments. Complete metadata admission derives every reachable
block node, layer multiplicity, maximum positions, state source and history
shape, ordinary and TopK courts, malformed headers and delivered decode. Scope
declarations must match every independently derived field. Isolated TopK success
cannot bypass another oversized reduction or omit a relation. Program hashing
for TopK prices is shared across unrolled occurrences rather than repeated.

Public/constant/initial-zero sources retain bounded v5 relation courts; registered
parameters and committed node/state-writer sources use point v7. A mixed state
policy prices both kinds and derives the selected kind from exact position and
occurrence wiring. Frozen per-layer state roles and normal-form type rules are
preserved. The all-position price covers earlier sub-64 legacy row paths even
when the final internal layout is flat. A v5 ordinary proof cannot bypass a
point-selected relation within the new mixed scope. Existing v5/v6 scope roots,
proof decoders, node routes and envelopes retain their prior behavior.

Add a separate bounded v7 mixed-fault codec/dispatcher and a complete ordinary
replay session. The shared reference engine independently loads registered
parameters and job ids, checks ordinary nodes and decode, and acquires selected
ids only when TopK differs. New value replies are raw bytes bounded at 16 KiB
before decoding, with one exact authenticated output leaf and node. Wrong-version,
missing, oversized or borrowed replies yield demands. The verifier's own scores
and state produce the bounded counterexample; every emitted fault roundtrips
and passes the independently authenticating mixed court.

The session binds the complete scope and reserves whole parameter/trace hashing
and TopK scan attempts before any provider callback. Each node/hash/selected
reply consumes its read and maximum byte allowance before delegation. Demands,
bad artifacts and retries do not regain counters. Source storage, i128 reference
tensor payloads, selection temporary memory, history, hash work, reply bytes and
attempts are separate dimensions. Reused v6 reference-cost formulas are data
inside the new v7 envelope, not a bypass of v6's frozen admission. These remain
component prices, not complete allocator/RSS, all-primitive CPU, deadline,
per-block/per-bond or network-wide resource/economic guarantees.

**187 distinct selected tests pass:** 153 existing kernel tests, six point-court
tests extended with actual whole-graph replay, ten new complete-graph tests and
18 actual-node v3/v6 compatibility tests. The existing hardware benchmark remains
ignored. Graph trials cover metadata-only 1,024-layer sparse Gather/state work,
all declaration fields and an independently oversized reduction; honest and
late-fault sparse/state replay; range/duplicate/dominance; source/value/codec
substitution; precharged retries and callback refusal; H=1/63/64/127/128 history
layouts; initial/per-layer prior-state writer court selection; decode and exact
one-position public encoder inputs. Ten-file formatting and whitespace checks
pass. Test names, commands, source/doc/binary/log hashes, measured material and
development fixture corrections are in
[the verdict](evidence/rfc02-flat-graph-v7-verdict.json).

The actual synthetic N=10,485,760, K=2 **one-position graph** replays honestly and
prosecutes a wrong committed selection. Its mixed filing is 17,488 bytes, while
the reference source tensor is 167,772,160 bytes and its complete scan is charged
251,658,288 with 128 temporary selection bytes. The failure reads three selected
values. Source/hash/evidence costs remain priced independently of that small
filing. This is an executed fixed axis, not a ten-million-token job/checkpoint.
Separately, metadata-only TopK→Gather→reshape/reduce→state update admission at
1,024 layers and 2,097,152 maximum positions includes 10 template relations and
15,038,676,992 instances. Worst court ceilings are 39,283 bytes, 15,073,040 work
and 8,438,451 scratch bytes; conservative TopK scan work is 540,432,058,363,674,624.
No weights, tokens, trace arrays or maximum-length execution are materialized
for that large envelope.

The updated protected-component pool records 232 distinct component passes;
this does not establish full RFC02 coverage. No native v7 post, seal/reveal,
chunk/DA or economics/Final route is implemented by this increment. The 18 real
node regressions validate old-route compatibility only. Next, bind the complete
court set/scope and value acquisition to protected native candidate admission,
actor-owned prosecution, transport reservations, shared budgets and original
capital. Versioned token/patch/audio lengths and H-axis sparse/state semantics,
the complete shared resource vector and economics, real checkpoint/task/context/
fidelity/performance, fresh outsider execution, independent Final/redemption and
A6/A8 remain open. Full RFC02 stays active. Conditional external mode A and
dormant shipping fences remain unchanged; no model-availability consensus
obligation is introduced.

## Verified increment — explicit native candidate, bounded lengths and history relation, 2026-10-11

Native tag 40 and SDK preparation/signing now bind a non-reward v7 candidate
to a Matured/Final model binding, exact descriptor/program/parameter roots and
complete independently derived mixed-court plan. Role descriptors remain v4
decoder/v5 encoder; the separate v7 court-set and scope/replay identities carry
the new semantics. The bounded manual decoder checks program and parameter
counts before allocation. Native binding checks consume ordinary work and
original actor collateral on accepted and invalid attempts; kernel canonical
preparation is separately prepaid. The proof-reserved allowance remains
protected. Table 29 has its own conditional root extension and restoration
rederives every declaration. Old/new candidates share eight owner slots and
withdrawal cleanup; no execution class, eligibility, job or Final right is
created.

The bounded length API binds a maximum-shape canonical v1 program and at most
4,096 ordered fixed-axis substitutions for token/patch/audio. Both maximum and
actual graphs pass type/range/complete-scope derivation; the immutable admission
binds actual counts/program/positions/scope/replay. Text decoder and encoder
counts are exact; unbound media is refused. Adversarial execution found a real
production trace parameter-validation omission. Buffered, streamed and stage
paths now enforce declared shape/dtype/payload/value and per-layer namespaces
before evaluation, matching the frozen interpreter without changing valid
results or roots.

An isolated v8 history-selection relation adds output extent min(Kcap,H),
H=min(p+1,window), exact deterministic ties and window-local/absolute position
mapping. Its O(N log K) own scan and O(K) selection memory are bounded before
allocation. A separately authenticating point court checks range/order/dominance
faults against exact graph/job/source/output roots with at most two source and
three output leaves. Sessions precharge scan/hash and every 16-KiB value reply;
missing/bad acquisition and retries consume allowance. The source HistAppend's
correctness remains a separate graph constraint, and v1 H-axis TopK is not
relaxed. This is not a complete typed v8 graph or native consumer.

**220 selected component tests pass:** 153 kernel library tests, five candidate
tests, ten mixed-graph tests, six point-court tests, five length tests, three
typed-trace tests, eight history tests, 21 actual-node conformance tests and
nine SDK candidate tests. One existing hardware benchmark is ignored. The
final kernel build/run rechecks the shared trace fix. Native/SDK candidate
registration binaries were built before that later trace fix and isolated
history module declaration; their tested registration code is unchanged, and
the next native-consumer integration must reverify the final shared runtime.
Do not imply one uniform native build of all parallel work from these separate
runs. The protected component pool gains 26 new tests to 258; this is not
checkpoint/task/Final coverage. Source preimages, build/run commands, logs and
binary hashes are preserved in
[the candidate/length verdict](evidence/rfc02-candidate-length-v7-verdict.json).

Candidate registration metadata accepts a synthetic N=10,485,760, K=2 program
at 2,097,152 maximum positions. It contains 307 program bytes, costs 162,124,457
registration work and derives a 37,875-byte worst filing / 4,964,969 court work
envelope. No weights, tokens or maximum-length trace are executed for this
registration test. History metadata reaches 10,485,760 positions, but its
executed tests use bounded synthetic windows, including all six dtypes,
warmup/rollover/flat thresholds and 966 exhaustive small selection tuples.

Actual Qwen metadata separately refuses both the 7,557,621,401 preparation
tariff against the 536,870,912 allowance and an ordinary relation's terminal
court caps. Do not count the synthetic candidate as real Qwen acceptance.
Native v7 posts/prosecution/chunks/DA are being integrated separately. Complete
media input bindings, typed v8 graph/state continuity, all-primitive and total
RSS/public-storage/deadline/per-block/per-bond/network resources, original
capital/reward conservation, actual tasks/contexts and independent hook-free
Final/redemption remain open. Mode A and dormant shipping fences are preserved;
the full RFC02 goal stays active.

## Verified diagnostic — independent actual Qwen source fidelity, 2026-10-11

The actual pinned Qwen2.5-1.5B BF16 source was executed offline through built-in
HF Transformers/Torch float32 eager CPU, rather than the local HL frontend.
Source/model/output bounds and exact token/job pins are checked before model
construction, and file bytes are checked before/after execution. These bounds
cover declared tensor payloads, not total Torch construction/allocator/RSS.
Four Python adversarial preallocation tests and six actual-reference protocol/
pin/token/threshold refusal checks pass. The latter enter no integer execution
and report no fabricated result. The frozen 123-position,
four-sequence context-32 protocol meets every predeclared source-fidelity
threshold: top1 96.747967%, mean KL 0.001367878, slope 0.998682249, correlation
0.999847941, maxabs 1.506592512, RMSE 0.070122785. The exact source and integer
commands, protocol/artifact/source/logit hashes and measured code/binary
identities are in [the source observation](evidence/qwen25-independent-source-reference.json)
and [the result](evidence/qwen25-independent-source-fidelity-result.json).

Whole-process time/RSS: HF source 26.67s / 6,366,314,496B; integer comparison
242.18s / 2,151,088,128B. Import/load/hash and concurrent builds may affect these
separate process observations. They do not certify practical service throughput
or execution/proof/hash/store/delivery economics. This run establishes empirical
fidelity for diagnostic next-token logits only, not integer bit equality,
original 32K context or a complete chat/code/translation task. Its actual v7
metadata refusal is retained in
[the admission audit](evidence/qwen25-flat7-metadata-audit.json).

[The checkpoint/cohort audit](evidence/rfc02-checkpoint-cohort-acceptance-audit.json)
retains D_all=3,117,871, D_files_listing=2,247,414 (72.081687%) and all missing/
gated/nonresponding failures. Shape-ready figures cannot become admission/Final
passes. Original A6 confidence/sustained participation conditions and A8's
complete-task active cohort remain required, alongside the user's >=90% A6/A8
goal. The older delivery experiment's cfg(test) activation/attestation/eligibility
seams are explicit: no-hook onboarding is NotOnboarded. Complete-task support
records remain zero, and full RFC02 acceptance remains incomplete.

## Verified correction — exact staged candidate metadata pricing, 2026-10-11

The v7 candidate preparation tariff now reserves B² + 64B + 8,832P + 16,384
before parameter-root hashing and canonical decode. The paid typed program
provides its actual scheduled node count N without expanding wiring or tensor
values; 16N*1,024 is reserved before range/scope/replay derivation, which reuses
that decode. Declared real parameter-instance counts are checked before building
the per-layer set. Both charges consume one attempt, require the combined
per-attempt and shared ordinary allowance, retain work spent on rejected input
or retries, and cannot enter the protected proof reserve. Canonical PWU, reward
weight, valid plan identities and terminal court ceilings do not change.

Five new adversarial cost tests pass on the final frozen pricing source. They
cover the maximum repeated schedule, arithmetic overflow, malformed canonical
bytes, invalid artifact authentication, invalid scope, shared-budget exhaustion,
one-attempt accounting and retained proof allowance. The five existing candidate
tests also pass on the staged tariff; their earlier log predates a later
diagnostic-label update and redundant-borrow cleanup. The latter cleanup is
included in the final five-test run. Compiled binaries are retained and hashed;
the independent source/binary/build/run/input pins and this limited build scope
are in [the pricing verdict](evidence/rfc02-staged-price-v7-verdict.json).
The protected component pool gains five new tests, from 258 to 263. This count
does not establish checkpoint/task or Final coverage.

The actual pinned Qwen metadata has B=14,011, P=1,636, 314 template nodes and
7,658 scheduled nodes: 211,670,361 predecode work + 125,468,672 scheduled metadata
work = 337,139,033, within the existing 536,870,912 ordinary allowance. The frozen
v7 ordinary scope still refuses with "flat relation exceeds court resource caps".
Its program/parameter files are hashed before and after the metadata audit; no
source weights are loaded or authenticated, and no class or execution right is
created by that tool. The 307-byte synthetic candidate's complete preparation
price is now 139,113 + 49,152 = 188,265; the historical 162,124,457 tariff and real
Qwen's 7,557,621,401 refusal remain preserved as earlier observations.

Native complete-graph postings and optional maximum/actual length rederivations
still need their scheduled metadata included in the unified resource vector;
bounded terminal chunk assembly also needs header-specific preallocation before
its price can be reduced. These are separate consumer/resource work. The new
ordinary point court and full reference/transport/economic envelope are under
integration, not established by the candidate-price correction. Complete typed
H/media jobs, actual tasks/contexts, independent honest execution Final and A6/A8
remain open. The full goal is active; mode A and dormant shipping fences remain.

## Verified component — native mixed v7 public prosecution, 2026-10-11

Candidate tag40 now connects to explicit post/seal/reveal/expiry/DA tags41..47
using one independently rederived full scope7/replay7 namespace. Tables68..76
persist the protected marker, own seals, catalogs and bounded public DA. The
old/new catalogs share original capital and eight statements; seals share
four-per-bond/1,024-global bounds in both admission orders, requester slots share
four and open demands are bounded at32. Public 16-KiB answers use saved
coordinates/type/shape/root and decode no variable model. Own tag113 target4
chunks survive generic saturation, bind the actual actor/vector seal, and
require two guaranteed runs for the last chunk plus court. Old unsealed
refutations, copied/borrowed/early seals and invalid paid evidence refuse.

Six new native fixtures cover independent sparse selection/state fault finding,
own seals and exact original-capital payout, selected-value default/horizon,
protected chunks and proof allowance after an admission flood, fixed text
length identity, and a fresh actual non-genesis participant. That participant
registers its carrier-funded bond after full IBD/pruning import, obtains real
public op211 page conversions plus DA evidence, runs its own checker, convicts
and redeems an actual UTXO. Independent full IBD recreates the credit; pruning
comparison retains all existing deltas and the complete tip/rows. The fixture
uses a 65-axis TopK/Gather/Reduce/state graph with two positions, not an actual
checkpoint or maximum-context execution. There is no live TCP RPC measurement.

The exact recorded builds pass 27 native +16 SDK +41 core +155 kernel tests,
239 distinct with one ignored hardware benchmark. The new count is13 (six
native, three SDK, two DA core, seal and outer-codec unit tests), bringing the
protected component pool from263 to276. This is not model/task/A6/A8 coverage.
The compiled native kernel includes the trace-provider fix and staged candidate
pricing. Core additionally repairs static allocation declarations36..47 and
the tag113 test pin; runtime dispatch and fences are identical to the native
build. Later graph9/resource10/settlement changes are not included. Kernel lib
staging includes the native seal7 export only; its compiled source also declared
unused point9/scope9 modules retained in the dependency preimage archive. Those
separate module implementations are committed only with their own verification.
Immutable binary/source hashes, 553-file compiled-source manifests and retained
fixture failures are in
[the native verdict](evidence/rfc02-native-mixed-v7-verdict.json).

Clean candidate verification still grants no execution class, eligibility,
reward or ExecutionFinal right. Actual-model honest Final/redemption, dynamic
maximum/actual jobs, typed H/media graph continuity, persisted unified
resource/economic ceilings, complete checkpoint/task/context fidelity and
A6/A8>=90% remain open. The full goal stays active; source R remains externally
acquired under mode A and shipping fences remain dormant.

## Retained failure — independent free-running task diagnostic, 2026-10-11

The actual pinned Qwen BF16 source and integer artifact each feed their own
outputs for three frozen arithmetic/translation/code prompts at context32.
Eight outputs per job and 75 total fed positions are executed even after EOS,
as specified before measurement. All three jobs are retained. Raw token
agreement22/24=91.666667% fails the frozen95% threshold; exact jobs2/3 meet only
their separate threshold. Source and integer exact-answer fractions are both
1/3, failing the frozen100% requirement. Arithmetic differs after EOS;
translation emits `Hello.` rather than exact `Hello`; code exhausts eight
tokens before completing its fenced answer or emitting EOS. The integer and
independent pinned-tokenizer scorer both return exit1 and preserve their output.

Whole-process source time/RSS is15.62s/6,367,248,384B; integer time/RSS is
206.33s/1,974,124,544B. Four adversarial protocol/scoring tests pass (0.001s),
covering output-feedback context accounting, exact schema/pins/bounds, EOS/token/
job identity and independent aggregate/answer recomputation. They add no native
component-pool or checkpoint-task acceptance credit. The four new tool sources,
protocol and immutable binary were frozen; raw failure outputs, commands,
source/binary/input SHA pins and concurrent-build limits are in
[the failed generation verdict](evidence/qwen25-independent-greedy-task-verdict.json).
The passing 123-position source-logit diagnostic remains independent evidence
for a different teacher-forced test. This free-running failure cannot be
substituted for A6/A8, relabeled as a90% cohort pass or removed from later
acceptance records. Actual task/maximum-context/native Final and complete
RFC02 acceptance remain open; the goal stays active.

## Verified components — complete ordinary point graph and resources, v9/v10

Ordinary scalar9 dependency courts, full scope9, mixed court9 and independent
locator/replay9 preserve frozen TIR/PC-v3/flat-v5 commitments. Registered MatMul
second parameters use their exact column layout. The scope samples every
scheduled occurrence and the history initialization/63-64 grammar boundaries,
prices flat tails at127, and bounds metadata visits before receipts. The mixed
API independently rechecks the complete scope and adds metadata work/storage
to the terminal, retaining1 MiB/2^30/32 MiB/1,024-leaf ceilings. Typed ordinary
DA supports all six dtypes, with16-KiB values/512-byte headers. The locator uses
own replay to find first faults without a producer-supplied corrupted coordinate;
missing/borrowed evidence creates demands or refusals. Local sessions prepay
reference/TopK/hash/read/reply/retry costs. Seven point and four graph tests pass.

Resource10 independently derives21 axes from the exact program/PC/positions and
full scope9, using the same complete-court bound helper. Its borrowed raw-image
consumer checks width/dtype/shape before i128 expansion, authenticates each
parameter and atomically reserves fixed plan/block/bond/network allowances before
callbacks. Eight tests pass, including all25 primitives,84 per-axis/per-scope
precallback refusals, subject/root tampering, atomicity and failed/retried source
reads. Actual Qwen32 metadata reports4,343,239,673B owned peak payload and
1,835,516,100B raw parameter storage; this is a derived reference envelope,
not measured RSS, executed native quota or model admission.

Checked settlement now rejects total overflow and unfunded payout/burn/Final
before mutation, applies a receipt batch atomically and checks balance before
commit. Existing externally slashed reservations still support clamped original
debit and release. Seven new conservation tests +22 route +28 OPV +6 verifier
+2 pipeline tests pass (65 total). One obsolete codec test had expected all
64-byte tag payloads malformed; known digest-only tags30/35/47 correctly refuse
as rule/authorization failures. The repaired fixture changes no runtime codec.
An initial blanket reservation-overhang guard was corrected after its legacy
failure; those trials remain retained.

Twenty-six tests are new across point7+graph4+resource8+settlement7, bringing
the protected component pool from276 to302. Existing regressions are not counted
again. The earlier native7 full155 kernel run predates these changes; an
interrupted later broad run is not credited as a complete pass. Exact final
source/binary pins, selected commands and limitations are recorded in
[the resource/settlement verdict](evidence/rfc02-reference-resources10-and-settlement-verdict.json)
and the separate point/graph/checkpoint observations. None of these local
components creates native quota persistence, retention/transport service,
execution-class rewards or Final. Native9/job11, full typed H/media continuity,
actual task/context/fidelity acceptance and A6/A8 remain open; the goal is active.

## Verified actual checkpoint components — ordinary court and PC source, v9

Two independently opened mapped executors each evaluate all7,658 nodes of the
pinned Qwen2.5-1.5B integer artifact for token4498. The finder independently
locates the first divergent node after35 agreeing roots, without receiving the
producer's mutated node coordinates. A fixed first-leaf reply provides the
authenticated divergent element; arbitrary-leaf hash descent/public service is
not established by this trial. Selected matrix instance(7,layer0) independently
authenticates to the registered PC; the17,944-byte point proof convicts and a
missing-input proof dismisses. Its whole mixed API is23,284 filing bytes,
151,708,672 work and21,316,001 scratch. Whole-process16.30s/RSS3,995,418,624B is
observed locally, separately from the32-MiB terminal court bound. A prior
quadratic scan passed76.06s and retains its reconstructed, SHA-verified source.

The independent raw-width streamed all-PC check compares all1,636 actual
integer-artifact instances to their registered v3 roots. It reads1,835,516,100B
payload, bounds helper workspace at27,275,264B under32 MiB, and observes81.07s/
RSS1,853,112,320B. There is no full i128 weight expansion in that check. Both
trials preserve artifact SHA before/after. This verifies integer artifact/PC
continuity; original BF16 float fidelity and complete task quality remain
separate measurements. No native binding/eligibility/Final or cohort pass is
inferred from offline source continuity.

Actual model metadata scope has7,658 admitted relations at32 positions and a
complete-API worst141,228 bytes/228,106,240 work/24,346,393 scratch, within the
unchanged terminal caps. The actual point trial runs one position only. Frozen
source, immutable executables/test binaries, commands, proof and all limits are
in [the checkpoint observation](evidence/qwen25-real-courts9-observation.json).
Native9/job11 integration, actual honest Final/redemption, complete tasks and
long contexts, H/media continuity and A6/A8 remain open. The full goal is active.

## Verified component — bounded authenticated external inputs, v12

`flat_inputs12` specializes a frozen-v2 external-input stage through the exact
canonical v1 view and the existing bounded T/P/A map. A present modality must
bind a real external axis; source weights and persistent states cannot resize.
Maximum and actual shapes independently pass validation, interval analysis
and complete scope9. Template/admission/job-input roots bind actual counts,
positions and real/virtual parameter namespaces. Both maxima and actuals are
bounded at16 inputs,16 MiB raw/32 MiB i128 payload and16,384 real PC instances.

The acquisition helper authenticates exact real PC keys/root and job/input
counts before callbacks, then atomically reserves raw/expanded/attempt
allowances. Returned dtype/shape/bytes are checked before expansion, followed
by intervals and exact input roots; failed reads keep their spent allowance.
Authenticated values and combined PC have private fields and readonly accessors.
The length7 change is visibility-only for its existing exact specialization;
its frozen public v7 behavior is unchanged.

Five new synthetic tests pass. Patch3/audio5 produces16/32/48 at three positions
under independent arithmetic, InterpreterV2 and the v1 trace. Complete court9
authenticates patch/audio reductions, the preceding-position state read and
its write under the same scope; honest output dismisses, wrong output convicts,
and borrowed input/state openings refuse. Wrong identities/source/counts/scope
and insufficient budgets refuse before callbacks; hostile images and intervals
retain the full attempt. The protected component pool increases from302 to307;
pending quota11/native9/new locator/SDK tests are counted in separate records.
Exact source preimages, retained immutable test binary and command/log hashes
are in [the component verdict](evidence/rfc02-typed-inputs12-verdict.json).

This is job-constant external input acquisition, without per-position/random/
earlier-stage sources or post-write effects. The local budget covers callback
attempts/raw/i128 payload only, not scope preparation, PC/tree hashing, scratch,
or provider-internal work. Native job binding/codec/transport/retention and the
shared resource10/quota11 envelope remain required before media execution or
Final. Source-PC namespace continuity grants no original BF16 fidelity, actual
media task, network reward or A6/A8 credit. The full RFC02 goal remains active.

## Verified actual components — generic public localization and context128

The ordinary locator validates complete scope and requested coordinates before
callbacks, then authenticates public headers/hashes and descends to an unknown
leaf. Six-dtype header authentication retains authentic malformed metadata for
the appropriate court. Two new graph tests pass, six graph tests total. The
protected component pool increases from307 to309; pending candidate9/SDK/quota11/
execution11/native tests are recorded separately and are not credited here.

The actual Qwen fixture independently matches all7,658 honest node roots. It
finds occurrence1/node27/element1500 after35 agreeing roots using nine public
queries and5,853 returned bytes. Its17,816-byte point proof convicts through the
same complete scope9/court9; missing evidence requests acquisition or dismisses.
The authentic/false R-PC pair covers one actual tile, not model-wide original
arithmetic equality. Exact source and immutable binary identities, fixture/proof
hashes and retained pricing snapshots appear in
[the generic observation](evidence/qwen25-real-generic9-observation.json).
Native service/prosecution and honest execution Final remain independent gates.

Fresh lowering produces a distinct context128 program with both history windows
changed, the same1,636 parameter instances and independently reconstructed four
positional fills. All borrowed raw parameters authenticate to their unchanged
registered roots; the32-position calibration is reused explicitly. The fixed
two-sequence/256-position comparison to independently loaded original weights
passes the unchanged predeclared gates: top1=.984375, meanKL=.0023050215582,
slope=.9977334375, correlation=.9995311810. Integer auditing/execution observes
574.41s/RSS2,438,201,344B and source inference8.61s/RSS6,544,982,016B. These are
different diagnostic workloads, not a production speed or peak-memory contract.

[The context128 observation](evidence/qwen25-borrowed-context128-observation.json)
retains the frozen protocol, newly canonical program, exact tokens, immutable
executable and tool hashes, source outputs and initial scale/disk refusals.
Full concurrently changing dependency compiler preimages are not claimed by
these example receipts. This passes a context128 teacher-forced diagnostic,
without original context32768, complete free-running tasks, media, native
Final/redemption or A6/A8 credit. The prior greedy-task FAIL remains retained;
the full goal stays active.

## Verified components — funded execution and persisted quota, v11

The checked class binds matured authenticated source identities, candidate9
maximum scope/replay/resource10 and fixed policy/horizons/reward/reserve. Each
text job fixes its actual scope/resource/quota roots, complete fed positions,
requester/producer/nonce and original escrow debit. Semantic job identity ignores
funding/admission aliases; the native permanent requester/nonce guard is a
separate required anchor. Private job state allows one claim and one settlement.
Final pays original escrow after retention/proof/DA/answer closure. Faults refund
the same escrow and pay finder/burn only from actually collected original slash.
Exact identities, checked totals, duplicate payout and anchored rewind guards
run before mutation. Trusted late close/refund preserves access to original
funding after DAA jumps without reopening source/computation proof windows;
late trace-DA default uses its immutable established deadline.

Twelve execution tests pass at the frozen quota20 snapshot (392s including
cargo-lock wait, with the test runner reporting0.00s). Two new tests restore original classes after revocation while fixing
private admission permission to false, preserve original escrow recovery and
refuse all changed original roots/policy. No revocation flag creates a fault;
fresh funded policy requires all future guarantee kinds. The initial ENOSPC
archive build ran no tests and is retained alongside
the passing immutable executable in
[the execution verdict](evidence/rfc02-execution11-verdict.json). Its owned
source/binary identities are fixed; full concurrent compiler dependency preimages
are not claimed. This receipt verifies pure trusted-boundary arithmetic/state,
not actual original-coin authentication or native Final.

Quota20 passes all21 axes/four cap scopes, rooted actual/class/program/PC/input
bindings, exact producer reserved actions, public challenger ordinary debits,
paid failed work, flood-resistant mature guarantees, bounded snapshot grammar,
trusted equal-DAA/rollback behavior and expired current-ordinary terminal close.
The late private token binds exact state/epoch and a genuine conserved book;
failed bodies retain debit and old guarantees are never restored.
[Its verdict](evidence/rfc02-persisted-quota11-isolated-verdict.json) preserves
earlier10/11/12/14/17/19 binaries and failures but counts only20 current distinct
tests. Twelve execution plus20 quota tests increase the protected component pool
from309 to341; pending SDK/candidate/native tests are not counted here.

Native-rooted quota restoration refuses conserving-but-forged counter snapshots.
A prepaid close leaves a subject open until an authenticated terminal book and
atomic coin application authorize its private token. Native metadata restore and
class-maximum court/chunk assembly costs are reserved separately from actual job
resources. The priced stored-post cap still needs its native pre-copy guard.

Atomic native original coins, policy/nonce/quota persistence, UTXO redemption,
mature challenger entitlements and retained-source lifecycle remain required.
Bounded quota-row collection now requires paid per-row work and independent
permanent native spent-request/job anchors; same-interval consumption and live
guarantees remain intact. Native must preserve these anchors atomically so a
collected row cannot recreate baseline credit. Existing funded jobs retain
restore-only class access after source revocation. The initial
job grammar rejects patch/audio; shared media resource/transport/retention and
actual media Final remain open. No actual complete task or A6/A8 credit is added;
the full goal remains active.

## Verified actual diagnostic — complete free-running tasks, context128

A new fixed protocol predates outputs and admits three complete arithmetic,
translation and Python-function jobs, with maximum32 generated tokens and first
EOS included. Cap without EOS is incomplete; semantic rules and95% token/
2-of-3 exact-job/100% source-and-integer quality gates are frozen. Missing IDs
count unequal. The old context32/eight-output FAIL remains unchanged.

Original source and integer both complete/answer3-of-3 correctly. All17 output
IDs match exactly, with EOS-included counts3/2/12 and exact-job agreement3/3.
The independent tokenizer/semantic scorer passes. Six meaningful Python
validation/scoring controls pass separately from the kernel component pool.
Source12.28s/RSS6,480,707,584B and integer261.35s/RSS2,009,333,760B include different
workloads; integer includes all1,636 PC-instance authentication, fresh lowering
and dev-profile execution. Neither result certifies backend performance.

[The complete-task observation](evidence/qwen25-borrowed-context128-complete-task-observation.json)
preserves the predeclared protocol, original and integer outputs, independent
scores, immutable binary and before/after owned source identities, retained
source replay and controls. Full concurrent Rust dependency compiler preimages
are not claimed. These three short24/20/36-token prompts use the fresh128 graph
and reused32 calibration; original context32768 and other real model families,
sparse/media/routing/state feature acceptance remain open.

EOS-stop here is a distinct offline diagnostic, without changing native9 or
the first exact fixed-count job11 grammar. It adds no accepted active-kernel
cohort record, native honest Final/redemption or A6/A8 credit. Full population
denominators and the user's90% targets remain unchanged; the goal is active.

## Native mixed-court conformance and bounded candidate restoration, v9

The explicit v9 path binds candidate/post/typed selected DA/own seal/reveal/chunk
and settlement to one full scope and court. Ordinary native ledger projection
prepays at most128 fixed candidate headers, preserving large raw rows outside
the journal diff. Candidate-local court/DA rechecks its authenticated full scope
without copying/deriving all candidates. Public bulk import keeps independent
validation with bounded row/raw-byte/aggregate-work preflight.

Both v7 and v9 owner withdrawal now retain candidate metadata while protected
claims or pending own proofs/DA need it. Expired metadata retires only without
live dependencies; catalog bounds and normal admission caps remain unchanged.
The SDK independently derives v9 scope/replay/resource10 from public metadata.

An actual-model Qwen fixture proves selected DA, own seal/reveal, original
collateral finder payout and IBD replay on native nodes. That fixture passes a
prepared actual-model proof to its finder; the separate fresh outsider consumer
must execute once and derive its unknown-coordinate proof from public data.
Neither this conformance route nor its slash payout is honest funded-job Final,
active-model population coverage or A6/A8 completion.

The coherent native milestone passes51 selected core regressions,35 real-node
conformance regressions,134 SDK tests and five candidate-import/adversarial
tests. The new distinct tests are five candidate, four SDK, three typed-DA and
eight node tests; these20 increase the protected component pool from341 to361.
Existing regression tests and previous quota snapshots are not counted again.
Exact source preimages, immutable binaries, trial failures and test-only
inventory corrections are retained in
[the native milestone verdict](evidence/rfc02-native9-typed-da-node-verdict.json).

## Native11 fixed headers, preparation/Fund and streaming outsider components

[The native header component receipt](evidence/rfc02-execution11-native-headers-verdict.json)
records26 pure tests (14 additional test functions relative to execution12),
8.40s compilation and0.01s reported test runtime, plus a26-test immutable replay.
Executable SHA256 is
`4f78af6225b6228883e8e05893c5a7f19f270e7e14d85211a69995b3049f4988`.
Owned execution/resources/test sources were archived before this compile and
stable afterward. The full concurrent dependency closure is not claimed;
initial17/23/25 trial outputs, initial source archive and compile refusal remain.

The light private native class rechecks the original complete candidate scope
without reading an unused class resource quantity vector. Actual resources are
independently derived during Prepare. The fixed preparation record binds the
exact prompt/input, actors, canonical nonce, reward and plan; Fund checks its
independent original native root, current block hash/DAA eligibility and original
debit. A readonly fixed class header supports existing book/refund/terminal
checks without scope reconstruction and grants no new admission. Immutable
class identity commits original PC-derived source court bounds, and the quota
factory covers computation and source maxima. Native must prepay its actual
selected-row reads/codecs/hash/copies under normal limits.

Historical original court settlement pins exact subject/R/PC/proof/inclusion,
so a newly submitted late proof or refuted source flag cannot obtain a refund or
bounty. A verified global R/PC theorem can block another live exact-scope Final;
the original finder alone receives a reward when native operator identity is
independent. Same-operator collection burns the full slash and refunds original
requester escrow. Pure native anchors and coins remain trusted boundary fixtures;
no actual native funded Final/redemption is credited by this receipt.

[The streaming R outsider receipt](evidence/rfc02-mapped-R-ordinary9-local-consumer-verdict.json)
records12 stable-owned-source SDK controls, executable SHA256
`2174f2857d1f7112767d4b0cc6c378887399b56418f013b5ec13f20f9295b006`.
The consumer authenticates the exact public snapshot and original R window
before its own execution and bounded unknown-node/leaf search. It predebits all
local source/reference/reply/hash/retry axes, keeps source acquisition external
(mode A), and explicitly returns a separate-court requirement for TopK/history/
Decode. It has no prepared fraud proof or coordinate input. The final immutable
fresh-node test now passes: all1,445,797 R leaves and1,636 PC instances are
authenticated, its own7,658-node execution locates the unknown ordinary fault
through43 demands/44 attempts, and its own17,816-byte proof passes native
seal/maturity/reveal, original100 slash,49 actual finder UTXO and independent
IBD. Runtime155.22s/RSS2,669,395,968B describes this exact diagnostic, not model
throughput. Its initial local IndexSteps refusal and diagnostic rebuilds are
retained; protocolv2 changed that local budget before the next output while
preserving network/court limits.

The coordinated immutable node/SDK build preserves1,442 actual local compiled
inputs selected from Cargo JSON artifacts and .d dependency files. Full prebuild
source preimages are archived; the unexported native11 fold is explicitly
excluded. The actual1, SDK12 and existing35 native regressions all pass in
[the final coherent outsider receipt](evidence/rfc02-actual-outsider9-final-verdict.json).
The actual-model test is selected explicitly with `--ignored --exact`; the
ordinary regression filter cannot substitute for that run. The earlier local
SDK8 and intermediate quota33/37 counts are superseded, not added again.

[The protected quota receipt](evidence/rfc02-protected-quota11-isolated-verdict.json)
records40 passing controls,20 additional functions over the pushed quota20.
Fixed selected rows are1500/749/1763/1716 bytes before roots/framing; an actual
native consumer must authenticate their keys, roots and absence and apply their
small journal writes. The full2.4MiB map cannot be hidden under this row tariff.
The rooted per-job retained-seal cap is4, independently of global1024/actor4.
Source court costs include bounded PC, parameter tile/inventory and hash axes.

[The native carrier receipt](evidence/rfc02-native11-bounded-carriers-verdict.json)
records five bounded-codec/identity controls. Only the carrier module is
exported: native11 live Fund/Commit/court/DA/Final routes remain unactivated.
The unexported fold draft has no implementation-completion credit.

Unknown-coordinate discovery still needs a paid pre-proof entitlement when
ordinary DA is exhausted. Current protected DA2 applies only to a known-proof
owner; the actual diagnostic uses43 ordinary demands and therefore proves no
flood-resilient discovery guarantee. Admission must independently price the
producer's lifetime deliveries and the fixed readonly terminal reserve before
these new routes can become live. The accepted native Final model cohort
remains0; long-context/model/task/storage/kernel coverage, full funded native
execution and A6/A8≥90% remain open.

The protected distinct component pool increases361→413:14 execution,20 quota,
12 SDK, five carrier and one actual fresh-node functions. Existing35 regression
cases, the26 post-quota40 repeat, prior local snapshots and immutable replays
add zero. These413 tests measure component engineering controls, not model
coverage or a completion percentage. The dated acceptance audit preserves all
population denominators and native honest-Final record count0.


## `rfc02/merge-final` — RFC02-T and codex's final head onto the integration line, 2026-10-12

Branch `rfc02/merge-final` from the integration line `319c3695e`: `rfc02/takeover` @ `8ba135501` merged first, then codex's final
`codex/rfc02-review-completion` @ `9519dc89f` (14 commits on `392fe80f7`). Codex has stopped; nothing below re-implements its work.

* **Held bounties (RFC02-T) live in table 27, kind 4** (`VerifierPayRowV1::HeldBounty`, beside O2's held default share). They
  were table 31 on `rfc02/takeover` (31 is R6781's EXEC reward holds); the Lead's trial merge put them at 32, which is
  `PALW_KERNEL_ROUTE_FIRST_AUX_TABLE_V1`. The route's writer (`write_kernel_row`) files a table-32 row as an AUX row without
  clearing the ledger cache. That was the debug assertion "the cached kernel ledger is not the ledger of the rows" the Lead's node test
  hit. Kernel ledger tables must stay below 32, and after 27–31 only 0 and 19 were free; both went to codex's registries.
* **Codex renumbered** (codex's side, per the registry):
  * kernel inner kinds 36–55 → **41–60**: v6 41–44, v7 45–52, v9 53–60; 36/37 are RFC02-T's, 38/39 G14R's (reserved), 40 RFC-7's;
  * kernel ledger tables v7 29 → **0** (loaded after the row loop, since its rows name bonds) and v9 30 → **19** (G14-R4's never-used
    number);
  * consensus aux tables 64–105 → **65–106**; 64 is RFC02-T's refutation seals.
  * Root extensions apply in a fixed order: 27, 28, 29, 30, 31, then flat-v7 (0), then flat-v9 (19), each only once its table holds a
    row. Codex's `LedgerEventV1` 33/34 and `PalwKernelChunkTargetV1` 4/5 were free.
  * A2U `KernelInner` rows (41–44, 45–52, 53–60) and one `StateEncoding` row were added; the tag pins in codex's tests were updated.
* Both protection mechanisms stay and do not overlap. Codex's v6/v7/v9 own-sealed flat proofs protect the new statement kinds.
  RFC02-T's inner 36/37 protects the frozen 23/27/32 refutations past `palw_model_refutation_seal_v1`. The sealed-bounty rule protects
  the kernel `FileProof`. RFC02-T's flat-v5 pursuit gate now also applies to codex's protected v6 posts past that fence.
* No int-12 wire type changed: every new kind is inside tag 110 or 113, and both are post-int-12.
