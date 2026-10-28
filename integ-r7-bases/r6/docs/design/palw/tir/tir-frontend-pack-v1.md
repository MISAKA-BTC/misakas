# Declarative TIR frontend pack v1

Implemented off-chain in `misaka-palw-tir-lower::frontend_pack`, independently of `ModelSpec`, model
names and the built-in feature registry. It emits the same frozen `TirProgramV1` and `PALWTIR1`
container as a direct compiler. Consensus encodings, primitives, class kinds and fences are unchanged.

**Status:** fixtures cover compilation, streamed conversion, reproducible receipts, SDK inventory,
three-implementation conformance, court demand evaluation and offline class admission. Real
checkpoint/full-task fidelity, fresh-seat public prosecution, on-chain Final and redemption remain
open in the [implementation ledger](rfc0002-implementation.md). Author-declared `full` scope is
provenance; it grants no support or fidelity verdict.

## Pack document

`format` is `misaka.palw.tir-frontend-pack.v1`. Top-level fields:

| Field | Meaning |
| --- | --- |
| `id` | Informational 1–128 byte label; no registry lookup |
| `scope` | `{task, completeness: "full" or "partial", components: [...]}`; intended task/components |
| `defaults` | Optional config defaults; their use is recorded |
| `inert` | Optional list of explicitly ignored config keys |
| `vars` | Optional object of named expressions/fragments; all globals are forced and bounded |
| `program` | Expression producing the frozen program grammar below |
| `bindings` | Expression producing one source binding per required parameter instance |
| `quant_formats` | Optional object of pinned, data-only stored-format descriptors; no registry selection |

Unknown document/grammar/binding fields are refused. Variables/fragments use the existing
[bounded expression language](model-adapter-v1.md): config/variable reads, finite map/repeat,
arithmetic, object/list composition and tensor-header queries. These construct a static program;
they introduce no runtime control, I/O, native code, operation definitions or executable plugins.
Nested config scopes inherit strict key tracking. This route has **no implicit HF inert-key list**;
the author must read a key or explicitly declare it inert, including `model_type`.

Identity is BLAKE2b-512, keyed by `MISAKA/PALW/TIR/FRONTEND/PACK/V1`, over canonical JSON
(sorted keys, compact spelling, integral floats normalized). Paths, whitespace and key order do
not enter it. All contents, including scope/labels, do. This identity enters provenance/receipts;
class identity still comes from canonical TIR, layout, tokenizer and inventory.

## Program grammar

`frontend_pack::program::Program::of(&p)` exports a direct `TirProgramV1` into this JSON view;
`Program::compile` restores canonical bytes and checks normal form. Fields mirror v1:

- `version`, `prim_set_id` (64-byte hex), `token_bound`, `history_bound`;
- `params`: `{name, dtype, shape: [u32, ...], per_layer}`;
- `consts`: `{dtype, shape, data}`; data is exact little-endian bytes as hex;
- `states`: `{name, kind: {"Fixed": {lo, hi}} or {"Hist": {window}}, dtype, shape, per_layer}`;
- `blocks`: `{name, carry_in: [{dtype, shape}], nodes, carry_out: [node, ...]}`;
- each node: `{prim, inputs, out: {dtype, shape}, commit}`;
- `schedule`: `{pre, layers: [block, ...], post}`, `logits`, `logits_scheme_id` (64-byte hex).

Node shapes contain positive fixed dimensions or `"H"`. References have one tag: `{"Node": n}`,
`{"CarryIn": n}`, `{"Param": n}`, `{"Const": n}`, `{"State": n}` or `{"Input": n}`.
Node operation objects use `op`. All 25 existing operations and exact attributes are exposed:

| Operations | Attributes beyond `op` |
| --- | --- |
| Reshape, Broadcast, Cast, Add, Sub, Mul, MatMul, Log2Floor, IntExp, IntRsqrt, IntLn, Select | none |
| Transpose | `perm` |
| Slice | `axis`, `start` (length is the declared output shape) |
| Concat, ReduceSum, ReduceMax | `axis` |
| Iota | `axis`, `start`, `step` |
| Gather | `axis`, `batch_dims` |
| Div | `rule`: `floor`, `half_up`, `half_away_from_zero` |
| Clamp | `lo`, `hi` |
| Compare | `cmp`: `eq`, `ne`, `lt`, `le`, `gt`, `ge` |
| TopK | `axis`, `k` |
| StateWrite, HistAppend | `state` |

Unknown operations/dtypes/dimensions/primitive sets require an extension or receive an explicit
encoding refusal. Invalid references, shapes, ranges or cost fail common TIR admission. `H`, rank,
TopK, state and history retain v1's exact rules. This compiler does not extend patch/frame lengths
or history bounds.

## Source bindings and arithmetic

Bindings are `{param: u16, layer: u16 or null, source: "tensor.name", import: ...}`. Required
instances come from the ordinary inventory, including scheduled per-layer instances. Each appears
once. All source tensors must be consumed with exact shapes/byte counts; missing/extra/repeated
or mismatched bindings fail before weight reads. Tied source tensors may feed explicit instances;
their raw digests must agree during conversion.

The ordinary imports are:

```json
{"kind": "integer"}
```

Copies I8/I16/I32/I64/U8/U16/U32/U64 values exactly into the declared TIR integer type, refusing
out-of-range narrowing. I64 never passes through f32 or f64.

```json
{"kind": "fixed_point", "shift": 12, "round": "half_away_from_zero", "overflow": "reject"}
```

Reads finite BF16/F16/F32/F64 as IEEE bits, multiplies the exact value by `2^shift`, then applies
the named rounding rule using integers. `shift` is -64..64. `overflow` is `reject` or `saturate`;
the latter records clipped values. NaN/infinity always refuse. No platform float arithmetic
determines emitted weights. This conversion specification does not prove source-model task
quality, routing agreement or state fidelity.

A third import composes a descriptor embedded under `quant_formats.<id>`, using the existing
`misaka.palw.quant-format.v1` **virtual** (1–4 output axes), **tensors** or **blocks** layout. The format label is local to the
pack; no built-in name, `quant_method`, registry membership, suffix inference or model name selects it.
Each role names an actual raw tensor explicitly. A descriptor binding omits `source`:

```json
{"param": 0, "layer": null, "import": {
  "kind": "descriptor", "format": "my-packed-format",
  "roles": {"blocks": "experts.blocks", "scales": "experts.scales"},
  "config": {}, "shift": 12, "round": "half_away_from_zero", "overflow": "reject"
}}
```

The descriptor's closed DSL produces binary32 values using its pinned decode semantics. The second
step imports those exact binary32 bits with the stated integer shift/round/overflow. This explicitly
includes binary32 rounding between storage decoding and integer conversion; it does not promise the
higher precision of an original source format. Saving packed weights, agreeing on integer execution
and preserving model quality are separate claims.

Every descriptor carries at least one decode vector, which the loader checks against the pinned
binary32 expected bytes after bounded preflight. All 31 built-in block and 9 tensor descriptors,
the MXFP4 virtual descriptor and previously unknown formats are covered by tests. Author-supplied
vectors establish internal consistency only; they confer no source-fidelity or on-chain authority.
Missing/unknown roles, wrong types/ranks/bytes, unused descriptors, unknown config leaves and basic
binding inventory errors refuse before checkpoint reads. The whole graph first passes ordinary TIR
admission. Header preflight is separate from metadata preparation; a decoded shape mismatch that
requires metadata refuses after that bounded preparation, before artifact conversion.

Nested parameter/check paths consume just their specified leaves; inert keys must be explicitly
declared. Virtual shape/check expressions use headers/parameters only. Tensor metadata expressions
may also read small roles, such as compressed-tensors' I64 shape or bitsandbytes' JSON quant state;
lane-dependent metadata is refused. `params.<name>.from_role` retains the existing exact-number JSON
reader, including float offsets. Every JSON leaf must be read by a declared parameter or explicitly
listed under the binding's `metadata_inert` role/path map:

```json
{"kind":"descriptor","format":"public-nf4",
 "roles":{"weight":"saved.weight","absmax":"saved.absmax","qstate":"saved.qstate"},
 "config":{},"metadata_inert":{"qstate":["quant_type"]},
 "shift":12,"round":"half_away_from_zero","overflow":"reject"}
```

The actual role set comes from that pinned descriptor. Unknown inert roles, malformed paths,
unread siblings/additional array elements and deeply nested JSON refuse. Small metadata used by a
plan is read in the chosen byte budget, snapshotted once and pinned as raw bytes. Changes between
compilation and artifact conversion refuse; raw sources are also hashed before/after conversion.
All binding headers and aggregate metadata/read-count budgets are checked before metadata reads.

A block binding uses exactly `roles: {"data":"raw.name"}`, empty config and no metadata inert map.
Its logical shape is the TIR parameter: leading axes flatten to rows and the last axis is tiled by
the descriptor's block elements. The source is an I8/U8 byte container, or a raw `TensorSource` with
the same logical shape. A scalar storage dtype must additionally have the exact declared scalar
byte count. The source bytes must exactly equal the computed block bytes. No dtype name selects
the decoder. Local safetensors can carry raw block bytes; native GGUF containers use the raw
acquisition contract below. The separate legacy ModelSpec GGUF importer remains available.

Tensor group-index, scale/zero/min and element passes preserve their original coordinates. Blocks
retain global row/column/block/group coordinates, including when a range splits a group or row.
Output expressions may use lane-dependent dimension indexes; the shared reader evaluates each lane.
These storage contracts do not establish actual checkpoint fidelity or completed RFC02 support.

## Bounds and reproducibility

Pack text is at most 2 MiB. Evaluation shares the existing 4M work, 64 MiB cumulative logical
allocation, 32 evaluator-call, 96 JSON-depth and list bounds across variables/program/bindings.
Config logical input is bounded to 64 MiB (including native tokenizer metadata); JSON sidecar text
remains capped at 2 MiB. Source inventory is capped at 65,536 names. Before encoding, the
compiler checks v1's declarations, ranks, names, inputs/carries and total 64 KiB constant data.
Canonical decoding enforces the 256 KiB program ceiling. The caller supplies `TirAdmitInputsV1`,
so provenance cannot enlarge execution/court ceilings.

At most 16 inline descriptors are accepted, each <=256 KiB, with <=16 roles/tables, <=32 block fields/parameters/
checks/vectors, <=64 KiB table data and at least one vector. A vector has <=8192 decoded values and
<=64 KiB per raw role. All vectors together consume <=4M expression-lane work. Expressions have
<=1024 bytes, <=256 lexical tokens and <=32 delimiter nesting before parsing; compiled expressions
have <=256 nodes and <=32 node depth. Shape/byte products and offsets use checked arithmetic.
These are compiler/import limits; they do not replace the TIR execution/checker/court resource contract.

Tensor metadata snapshots total at most 64 KiB per plan and 64 MiB across the compiler's parameter
instances. Metadata read calls total at most 4M. JSON metadata also obeys the depth/logical input
bound before the exact-number parser is entered. `compile_bounded` accepts the same `block_bytes`
budget as conversion; `compile` uses a 1 MiB metadata-read default. Converting with a smaller read
budget than was used during metadata compilation refuses and asks for bounded recompilation.
CLI and SDK callers pass their chosen budget to both phases. Reported source-read bytes/peak include
metadata acquisition; those operational measurements are excluded from reproducible identity.

Conversion uses raw byte ranges with no whole-tensor/f32 fallback. `block_bytes` is 8..16 MiB;
conversion pages and encoded buffers are each bounded by it even when widening I8 to I64. The container
writer adds its ordinary buffer/header. Read boundaries do not enter artifact identity. Failure
removes the temporary artifact and preserves previous output. Header/inventory changes, short
ranges, changed tied tensors and receipt mismatches are explicit refusals.

Descriptor conversion retains one page per present role; their total raw cache is <=`block_bytes`.
The budget must hold at least eight bytes per present role. It decodes <=1024 values per batch;
intermediate expression columns have a fixed bound (<=2 MiB logical column data plus coordinates,
constants and normal container buffers), and the decoded/encoded buffers are independently bounded.
Ordinary weight roles have no whole-role load or `DescribedSource` cache fallback; the separate
metadata snapshots obey the explicit caps above. Raw roles are hashed in bounded
scans before and after conversion; mismatches refuse before replacing the output. Strided layouts
can incur extra cache misses. CLI `source_bytes`, `tensor_bytes` and `source_read_bytes` expose stored
size, emitted integer size and actual raw read traffic, including the pinning scans. Read traffic
and peak read size are operational measurements excluded from reproducible identity; these are
not execution/proof/hash/retention/delivery benchmarks.

## Native GGUF acquisition

Both the generic CLI and SDK companion accept a GGUF file, any part of a standard split set,
an explicit `*.gguf.index.json`, or an unambiguous directory, including a `config.json` sidecar.
Standard sets use `<arbitrary-prefix>-00001-of-00003.gguf`; a directory containing multiple sets
or ordinary checkpoint files alongside GGUF requires an explicit file/index path. A directory's
`model.gguf.index.json` selects its enumerated native set. Selection and decoder validity have
no model-family/name dispatch.

Arbitrary part names use a public acquisition index, outside consensus:

```json
{"format":"misaka.palw.gguf-checkpoint.v1","parts":["weights-B.bin","metadata.bin","weights-A.bin"]}
```

The index has exactly these fields, at most 2 MiB and 1–1024 distinct local basenames (no path
separators, traversal or absolute paths). Ordering grants no priority: headers identify the parts.
All three `split.no`, `split.count` and `split.tensors.count` integer fields are required together;
part numbers are contiguous from zero, declarations agree, and the global tensor count is checked.
A merged standalone file may declare count zero; a one-part set declares count one. Part zero
may be metadata-only. Container versions agree and tensor names are globally unique. Later parts
may omit primary metadata or repeat it exactly, with only transport keys and part-local alignment
exempted. Additional/conflicting model or tokenizer metadata refuses. These conventions match the
[upstream split writer](https://github.com/ggml-org/llama.cpp/blob/master/tools/gguf-split/gguf-split.cpp).

Only the three verified transport fields are excluded from the effective model configuration;
a sidecar cannot inject them. Other `split.*` keys remain subject to strict read/inert validation.
For the same payloads and effective metadata, whole and split sources yield identical artifacts,
receipts and tokenizer/class identities. SDK source SHAs still pin every physical part and the
index, so a changed partition or index requires a fresh companion pack.

`FrontendSource` exposes the stored tensor names unchanged. Reversing GGML dimensions yields the
logical row-major shape; bytes stay untouched. No Q/K permutation, gain adjustment, expert split
or architecture rewrite is implicit. Such transforms belong in the submitted graph/frontend.
Scalar GGML F32/F16/BF16/F64 and I8/I16/I32/I64 expose their stored dtype; packed tensors expose
an opaque `GGML:<id>:<elements>:<bytes>` storage label. The public registry supplies byte geometry
only. Packed decoding still uses the binding's explicit local descriptor key and digest.
Scalar IDs follow the [GGML storage enum](https://github.com/ggml-org/llama.cpp/blob/master/ggml/include/ggml.h),
checked against the official header on 2026-10-10.

A new GGML ID can be supplied by a self-tested inline block descriptor with exactly one
`ids: [{"scheme":"ggml","id":...}]` entry. A registry collision or scalar storage redefinition
refuses; unknown types without supplied geometry refuse by tensor name/ID. Registry membership
does not select a decoder, establish source equivalence or affect consensus admission.

Native metadata keys are literal config keys, such as `$cfg: "future.block_count"`. They join an
optional sidecar, which cannot duplicate/override a native key. Every key must be read or
explicitly listed as inert, including tokenizer and architecture metadata; values enter the
effective config digest. Integer metadata retains its full width; finite stored F32 values widen
exactly to F64 for JSON. Non-finite metadata refuses.

An external tokenizer file follows the existing byte-hash contract. When none is supplied/found,
all embedded `tokenizer.*` values are hashed as canonical JSON
`{"format":"misaka.palw.gguf-tokenizer.v1","metadata":{...}}` through `tokenizer_id_of`.
No tokenizer information yields the existing zero ID. This pins the representation and class
identity; it does not certify a tokenizer algorithm or complete text-task fidelity.

Acquisition shares parsed header bytes and cumulative logical header allocation budgets of
64 MiB each across every part, with aggregate tensor/metadata-key counts of 65,536, rank four
and per-part alignment at 64 KiB. Each part consumes the same budget before allocation;
opening another part cannot replenish it. Existing per-string,
array and nesting limits also apply. Counts/allocation budgets precede allocation; dimension/byte/
offset/alignment arithmetic is checked. Zero dimensions, duplicate names/keys, overlapping tensor
ranges and truncated data refuse. Unknown tensor bounds use a sorted lookup rather than a
quadratic scan. The writer revalidates every native header and the index before conversion and before publishing,
so a header or index change during acquisition preserves the prior output. Payload read metrics retain
their stated block budget; header parsing is a separate bounded acquisition cost.

The `misaka.palw.tir-frontend-build.v1` receipt pins frontend/config/program/tokenizer/artifact
digests, compiler version and compiler/reader/dependency source digest, intended scope, assumed
defaults, source tensor metadata/raw digests, descriptor digests and saturation count. An independent rebuild checks
every field before replacing its output. It is a build receipt, never a fidelity certificate.

```sh
palw-tir-frontend --frontend-pack frontend.json ./checkpoint \
  --out model.palwtir --record build.json --block-bytes 1048576
palw-tir-frontend --frontend-pack frontend.json ./public-checkpoint \
  --out peer.palwtir --record peer-build.json --expect-record build.json --block-bytes 8192
```

## SDK companion pack

`palw-class pack build-frontend` writes `frontend.json` and `frontend-runtime-pack.json`, using
the versioned companion `misaka.palw.runtime-pack.tir-frontend.v1`. The existing ModelSpec pack
v1 schema/golden outputs remain intact. The companion pins source-file SHA-256s/revision, the
build receipt, common streamed artifact/inventory manifest, admission profile, executor versions
and source revisions, and conformance vectors. All three existing implementations must agree on
tokens, logits and every commit. Empty vectors refuse; receipts cap positions at 4096 and jobs
at 256, so they do not certify arbitrary long-context performance.

```sh
palw-class pack build-frontend --model ./checkpoint --frontend-pack frontend.json \
  --out model.palwtir --pack ./pack --vectors tokens.json --decode 4 --revision exact-revision
palw-class pack verify-frontend --model ./public-checkpoint --artifact model.palwtir \
  --pack ./pack --rebuild-out peer.palwtir --block-bytes 8192
```

The companion accepts local safetensors directories with each shard pinned, and whole/split GGUF
files or indexes with every part, index and optional config/tokenizer sidecar pinned. Verification
checks SHAs/compiler/executor/profile pins, rebuilds without uploaded executable code, derives
the common inventory and reruns all three engines. Named build/conformance checks are separate
from `SOURCE_EQUIVALENCE_UNVERIFIED`, full task and live Final. A pack cannot supply a boolean
to promote equivalence. Artifacts are staged until receipt, inventory, conformance and final source
SHA checks pass; late failure preserves the prior published artifact and removes the temporary.
Output guards cover source/sidecar aliases, including Unix symlinks and hard links.
The common HF-reference/beacon tool integration is specified below. Real-checkpoint fidelity,
full-task support, approved policy and real-node acceptance remain part of RFC02 completion.


## Reference-logit fidelity and the common beacon protocol

An independent frontend companion can now carry public HF-reference data and a strict,
predeclared empirical policy. This is a separate named check from integer execution, source
identity, complete task quality, routing, runtime state saturation and live Final.

```sh
palw-class pack attach-frontend-fidelity --pack ./pack --artifact model.palwtir \
  --hf-reference ./hf-reference --fidelity-policy policy.json --out ./fidelity-pack
palw-class pack verify-frontend --pack ./fidelity-pack --model ./checkpoint \
  --artifact model.palwtir --rebuild-out peer.palwtir
palw-class pack bind-class --pack ./fidelity-pack --artifact declared.palwtir \
  --network testnet-12 --out ./bound-pack
# The existing commit/run/verify-conformance commands accept this bound companion directly.
```

The shared commitment derives `program_root` in the **kernel program-root domain**, as required
by onboarding tag 107 and the registered kernel record. The container's `graph_ir_root` remains
in the separate artifact/class domain; substituting it refuses static admission. The plan is
re-derived over the kernel root and exact requested positions. Existing commitments made by a
binary using the artifact root in this field require recomputation and a new commitment attempt.

Single-program selection considers v1, v2 and segmented v4, oldest first, only after both semantic
checks and the node's public-prosecution, carrier and per-block court limits pass. A semantic-only
success with an uncarryable response is refused. Preflight does not silently shorten a requested
context. v4 reports its separate OPV chain admission/economics requirement; v3 pipeline and v5
encoder job-input semantics are not inferred by pretending their inputs are stored weights.
Availability, soundness-policy approval, chain activation and Final remain separate evidence.

`attach-frontend-fidelity` acquires and validates policy **before** loading reference logits or
executing a position, then writes a new directory only after coverage and tolerance pass. There
is no allow-out-of-tolerance flag. The policy schema is `misaka.palw.tir-frontend-fidelity.v1`,
with exactly these fields:

| Field | Rule |
| --- | --- |
| `schema`, `fit_math` | This schema and `libm-v1`; the fit uses portable software exp/log |
| `checkpoint_revision`, `task` | Nonempty revision equal to the companion's pinned revision, task equal to its declared scope |
| `context` | Positive measurement ceiling within program history bound; every supplied sequence fits it |
| `minimum_sequences`, `minimum_positions` | Positive coverage floors fixed before measurement |
| `logits` | Existing runtime-pack `LogitsSection`: convention, exact `scale_bits` and slope/correlation/top-1/KL tolerances |
| `max_abs`, `rmse_max` | Additional finite, nonnegative logit-error ceilings |
| `max_import_saturated_values` | Import-time weight saturation ceiling, checked against the conversion receipt |

Every noninteger policy/fit number uses the existing 16 lowercase IEEE-binary64 hex-bit encoding.
Scales are finite and positive, slope bounds ordered and positive, correlation/top-1 floors in
[0,1], and KL/error ceilings finite and nonnegative. `q24-natural-v1` fixes scale exactly 2^-24;
`legacy-greedy-only` records the tool's explicit scale. An unmeasurable/degenerate/nonfinite fit
refuses. A context ceiling is **not** evidence that its maximum length was measured: sequence
lengths, counts and position totals are public in the reference sidecar. These basic gates do not
close the required real-context/routing/task-quality evaluation.

The new companion carries the exact policy and digest, canonical reference metadata and float32
payload pinned by byte count/BLAKE2b-256, producer information and measured metrics. The source
provider's label is informational; copied logits cannot certify their derivation from HF. The
shared reader accepts existing sidecars/audit/fixture formats, with 2 MiB JSON, 256 MiB binary,
256 sequences, 4096 total positions and 64M logit-value acquisition ceilings. These are local
producer-tool limits, not consensus model/context limits. Noninteger/out-of-range tokens, ragged
rows, zero vocabularies, extent overflow, nonfinite values, truncated floats, extra unread logits
and unsafe payload paths refuse. Long-context streamed fidelity remains required by RFC02.

`verify-frontend` checks the policy/reference pins and independently repeats the portable fit
against its staged source rebuild before publication. False fit/coverage records and altered
policy/reference bytes refuse; late failure preserves the prior artifact. A success reports
`reference_logits: WITHIN_PREDECLARED_TOLERANCE`. Source equivalence, routing fidelity, runtime
state saturation, task quality, full task and live Final remain explicitly unverified.

`bind-class` supports both manifest formats without choosing a new layout or synthesizing a
ModelSpec. It checks the source inventory/tokenizer and computation (only the existing logits
scheme declaration may differ), then pins the declared artifact digest, exact layout and class ID
in a new companion. Any fidelity sidecars are preserved. Missing layouts, foreign programs,
changed weights/tokenizers and output-directory reuse refuse.

The existing `commit-conformance`, `run-conformance` and `verify-conformance` commands share one
artifact/layout/static-admission/challenge/evidence path for both companion formats. Exactly one
manifest is required; there is no fallback from a malformed frontend companion to ModelSpec.
Frontend/compiler/executor/profile pins are checked before commitment. Independent recipes use
a typed absent calibration ID, since explicit imports do not claim ModelSpec calibration. The
source root binds the entire recipe, reference/policy/fit, revision, raw-source SHAs, descriptors
and declared layouts; the implementation set also pins the compiler. Every artifact/tokenizer/
class/layout/kernel/plan root is recomputed. Explicit plan positions cannot exceed class context
or program history and cannot be zero.

A changed recipe, reference, scope, policy, implementation or artifact requires a new pre-beacon
commitment. Interrupted checks resume only under matching bindings; forged results fail fresh
reruns, and `--no-rerun` is never a pass. The shared tests use **synthetic** work facts and an
**unapproved** policy under hypothetically armed kernels. This is tool-protocol evidence, not
real node history, an approved soundness bound, an outsider G14 conviction or Final/redemption.
