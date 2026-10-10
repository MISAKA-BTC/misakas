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

Two imports are implemented:

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

Packed quant formats/source transforms beyond these imports remain explicit refusals here.
Existing descriptor/importer and Direct TIR routes remain available; generic frontend-pack
descriptor composition and independent decode vectors still need integration evidence.

## Bounds and reproducibility

Pack text is at most 2 MiB. Evaluation shares the existing 4M work, 64 MiB cumulative logical
allocation, 32 evaluator-call, 96 JSON-depth and list bounds across variables/program/bindings.
Config logical input is bounded to 2 MiB; source inventory to 65,536 names. Before encoding, the
compiler checks v1's declarations, ranks, names, inputs/carries and total 64 KiB constant data.
Canonical decoding enforces the 256 KiB program ceiling. The caller supplies `TirAdmitInputsV1`,
so provenance cannot enlarge execution/court ceilings.

Conversion uses raw byte ranges with no whole-tensor/f32 fallback. `block_bytes` is 8..16 MiB;
stored and encoded buffers are each bounded by it even when widening I8 to I64. The container
writer adds its ordinary buffer/header. Read boundaries do not enter artifact identity. Failure
removes the temporary artifact and preserves previous output. Header/inventory changes, short
ranges, changed tied tensors and receipt mismatches are explicit refusals.

The `misaka.palw.tir-frontend-build.v1` receipt pins frontend/config/program/tokenizer/artifact
digests, compiler version and compiler/reader/dependency source digest, intended scope, assumed
defaults, source tensor metadata/raw digests and saturation count. An independent rebuild checks
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

The companion currently accepts local safetensors directories with each shard pinned. Verification
checks SHAs/compiler/executor/profile pins, rebuilds without uploaded executable code, derives
the common inventory and reruns all three engines. Named build/conformance checks are separate
from `SOURCE_EQUIVALENCE_UNVERIFIED`, full task and live Final. A pack cannot supply a boolean
to promote equivalence. HF-reference/beacon pack integration remains part of RFC02 completion.
