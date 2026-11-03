# A3 source-program scanner diagnostic audit — read-only

Audit base: worktree HEAD1b14d1ecc. No Core/worktree edits, Cargo, compilation, execution or allocations measured. Only this `/tmp/a3-source-diagnostic-audit.md` file is produced by this task. This is a logical requested-backing analysis, not allocator/RSS or model-computation validation.

## Result

The pre-quote scanner has a finite **input-independent literal diagnostic vocabulary**. The largest source-scanner message payload is **47 UTF-8 bytes**, from two existing literals. No diagnostic here embeds a peer name, a shape/parameter dump, a raw byte slice, an enum discriminant or an input-derived integer. This supports an audited **typed bootstrap diagnostic reservation before invoking the existing scanner**, without replacing its grammar or changing legacy checks/error order. A capped formatter is not needed merely to bound this scanner's text. A formatter added only after an existing `io::Error::new` would in any event be too late to cover that first allocation.

47 is a text-length bound, NOT the entire error owner's native backing. On the pinned Rust1.93.0 implementation a literal passed to `io::Error::new` creates a String buffer, a boxed String-error wrapper and a boxed Custom payload. Conversion to `NativeDecodeErrorV1::Decode(e.to_string())` may keep that old owner alive while constructing the new String. Resource conversion allocates its own fixed component/type/reason Strings. The finite bootstrap quote must count these typed owners and concurrent buffers; do not substitute an arbitrary8KiB allowance.

## Exact call graph and reachability

- `ledger_expansion.rs:582 source_tir_program_shape_v1(&[u8])` first calls `standalone_tir_program_shape_v1` at:604, then reuses the same generated `TirProgramV1::scan_native_wire_v1` at:606. Its local conversion closure is:590-600.
- `standalone_tir_program_shape_v1`:558 wraps a fixed stack u32 length prefix and borrowed bytes in `Cursor(prefix).chain(bytes)` at:561, then `NativeWireReaderV1`:562 → `program_blob<TirProgramV1>`:563 → `nested_blob`:234. The nested wrapper uses `Take<&mut NativeWireReaderV1>`:239. There is no arbitrary external `Read` implementation, filesystem syscall, OS error string or caller-supplied Error payload in this particular entrypoint.
- `program_blob`:375 has only fixed arithmetic diagnostics after the grammar pass. It does not execute interval/cone analysis; its broader analysis **quote** is not an analysis run. `standalone` additionally calls `step_space_shape_v1`:460, which consumes the same borrowed bytes and fixed generated field scanners.
- Generic Reader paths: `wire.rs:35 overflow`, :38 `wire_bad`, :60 step limit, :67 skip/frame bound, :101 underlying-memory read/read_exact, :124 bool, :162 Vec, :205 String. String scanning skips raw contents; it does not render or even UTF-8-decode names at this stage.
- Exact V1 schema is `wire_generated.rs:12640`. Reachable enum error sites are Cmp:12426, Prim:12553, Rounding:12573, Ref:12615, StateKind:12636, DType:12733, Dim:12750; all use the same26-byte unknown-enum literal. Struct macro `wire.rs:231-237` just propagates the first error; it adds no string. Primitive/array/tuple scans propagate too.
- `Option`'s `wire.rs:153` and zero-sized-Vec `wire.rs:172` are generic scanner diagnostics, but the present V1 program schema has neither an Option field nor a Vec of a zero-sized element. They are listed conservatively below and do not raise the maximum. Box/BTreeMap/BTreeSet scanner implementations at:157/:193/:199 are not reached by this program's wire grammar. `wire_budget.rs` and unrelated generated ledger/class schemas are likewise not in this direct source-program call graph.

## Literal construction inventory

Each listed `wire_bad` call becomes `io::Error::new(InvalidData, literal)` via `wire.rs:38-39`; no argument at these call sites is formatted from input. `wire.rs:36` uses OutOfMemory and :63 uses TimedOut. Source trailing bytes at ledger:608 is already an owned Decode String, not an io::Error.

| Site | Payload bytes | Literal | Reachability |
|---|---:|---|---|
| [ledger_expansion.rs:237](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:237) | 43 | `nested ledger codec exceeds exact frame/cap` | reachable/guard |
| [ledger_expansion.rs:242](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:242) | 34 | `nested ledger codec trailing bytes` | reachable/guard |
| [ledger_expansion.rs:244](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:244) | 24 | `nested byte Vec overflow` | reachable/guard |
| [ledger_expansion.rs:389](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:389) | 34 | `program canonical frame accounting` | reachable/guard |
| [ledger_expansion.rs:393](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:393) | 47 | `canonical program re-encoding capacity overflow` | reachable/guard |
| [ledger_expansion.rs:396](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:396) | 32 | `program workspace count overflow` | reachable/guard |
| [ledger_expansion.rs:397](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:397) | 27 | `program Vec growth overflow` | reachable/guard |
| [ledger_expansion.rs:398](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:398) | 25 | `program schedule overflow` | reachable/guard |
| [ledger_expansion.rs:419](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:419) | 31 | `program cone workspace overflow` | reachable/guard |
| [ledger_expansion.rs:426](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:426) | 35 | `program unrolled workspace overflow` | reachable/guard |
| [ledger_expansion.rs:440](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:440) | 23 | `constant lanes overflow` | reachable/guard |
| [ledger_expansion.rs:443](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:443) | 36 | `coexisting program analysis overflow` | reachable/guard |
| [ledger_expansion.rs:467](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:467) | 47 | `unknown TIR program version for StepSpace shape` | reachable/guard |
| [ledger_expansion.rs:492](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:492) | 38 | `unknown state kind for StepSpace shape` | reachable/guard |
| [ledger_expansion.rs:499](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:499) | 31 | `invalid state per_layer boolean` | reachable/guard |
| [ledger_expansion.rs:507](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:507) | 30 | `StepSpace state count overflow` | reachable/guard |
| [ledger_expansion.rs:514](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:514) | 29 | `StepSpace node count overflow` | reachable/guard |
| [ledger_expansion.rs:526](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:526) | 30 | `StepSpace shape trailing bytes` | reachable/guard |
| [ledger_expansion.rs:559](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:559) | 29 | `TIR program frame exceeds u32` | reachable/guard |
| [ledger_expansion.rs:565](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:565) | 37 | `standalone TIR program trailing bytes` | reachable/guard |
| [ledger_expansion.rs:570](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:570) | 37 | `standalone TIR owner framing overflow` | reachable/guard |
| [ledger_expansion.rs:576](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:576) | 34 | `TIR structural item count overflow` | reachable/guard |
| [ledger_expansion.rs:608](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:608) | 35 | `Source program shape trailing bytes` | reachable/guard |
| [wire.rs:36](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/wire.rs:36) | 38 | `native Borsh shape arithmetic overflow` | reachable/guard |
| [wire.rs:63](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/wire.rs:63) | 38 | `native Borsh shape CPU budget exceeded` | reachable/guard |
| [wire.rs:69](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/wire.rs:69) | 42 | `native Borsh field exceeds remaining frame` | reachable/guard |
| [wire.rs:128](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/wire.rs:128) | 21 | `invalid Borsh boolean` | reachable/guard |
| [wire.rs:153](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/wire.rs:153) | 20 | `invalid Borsh option` | generic helper only; not in current V1 grammar |
| [wire.rs:172](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/wire.rs:172) | 39 | `Borsh zero-size sequence is unsupported` | generic helper only; not in current V1 grammar |
| [wire_generated.rs:12426/12553/12573/12615/12636/12733/12750](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/consensus/core/src/palw_native_resource_v1/wire_generated.rs:12426) | 26 | `unknown Borsh enum variant` | V1 enum scanners; fixed literal |

The standard-memory `read_exact` failure is Rust's static READ_EXACT_EOF message,27 bytes. It has no custom allocating payload before conversion to String. This is a fact about the memory-reader call graph above; it would NOT hold if a new caller generalized this scanner path to arbitrary `Read` errors. ErrorKind-only, OS and arbitrary custom Error cases must not be silently included under the47-byte bound.

## Conversion/resource-construction inventory

| Site | Construction / retained owners | Input dependence |
|---|---|---|
| ledger_expansion.rs:590-600 | Consumes one io::Error. InvalidData/EOF → `NativeDecodeErrorV1::Decode(e.to_string())`. Old error may coexist until branch exit with returned String. | Text is one fixed scanner/EOF literal. No input display. |
| ledger_expansion.rs:591-593 | OutOfMemory → `NativeCoverageV1::overflow::<TirProgramV1>("Source program wire shape")`. | Two new Strings: fixed component plus compile-time type-name. Old io error retained conservatively. |
| ledger_expansion.rs:594-598 | TimedOut/Unsupported → `NativeCoverageV1::unsupported::<TirProgramV1>` with fixed component and fixed coverage reason. | Three new Strings; compile-time type-name, no input content. |
| ledger_expansion.rs:608 | Direct Decode String for fixed source trailing-bytes message. | Fixed35 bytes. |
| ledger_expansion.rs:610 | `err()` builds Overflow component `Source normal-form cardinality` for checked q arithmetic. | Fixed component plus compile-time TirProgramV1 type-name; no io error owner in this branch. |
| tir.rs:34-63 | `TirNativeRequestsV1::{exact_v1,vec_v1,add_v1}` can construct NativeCoverage before the final quote is returned. | Fixed components `TIR concrete array request`, `TIR growing Vec capacity`, `coexisting TIR typed requests` and `type_name::<T>()`. These must be covered by the same bootstrap reservation. |
| typed.rs:19-23 | Unsupported has component/native_type/reason Strings; Overflow has component/native_type Strings. | At these call sites only static literal or compile-time type-name. Other arbitrary callers of these generic constructors are outside this audit. |
| typed.rs:46-52 | Display interpolates those owned strings. | No Display/to_string of NativeCoverage is performed inside the audited source-shape path; formatting it later is a separate caller-owned operation. |

All concrete T used by source quote failures must be included in the compile-time type-name max, rather than guessing its length: `TirProgramV1`, `TirNativeRequestsV1`, u8, bool, usize, Dim, TensorType, BlockInfo, `BTreeNodeEnvelopeV1<&str,()>`, `BTreeNodeEnvelopeV1<(DType,Vec<u32>,Vec<u8>),()>`, `BTreeNodeEnvelopeV1<u16,()>`, `BTreeNodeEnvelopeV1<&str,BTreeSet<u16>>`. These come directly from `ledger_expansion.rs:610-653` and `tir.rs:34-63`. Use actual `std::any::type_name::<T>().len()` (no allocation) for each, not an input limit. The branches are alternatives: do not add all type-name payloads as if every error existed simultaneously.

## Derivable typed bootstrap envelope (proposal, not implemented)

Let `L=47`, explicitly generated/audited from the literal list above. Let `G(n)` be the existing pinned growing-byte-Vec request envelope from `TirNativeRequestsV1::vec_v1::<u8>(n)` (tir.rs:43-56), retaining its native element/minimum-capacity semantics. Let `S` be `size_of::<String>()`, `I=size_of::<io::Error>()`, `E=size_of::<NativeDecodeErrorV1<String>>()`. These are typed headers, not serialized lengths.

For the pinned1.93.0 std custom-error payload, the audited concrete fields are `ErrorKind` plus `Box<dyn std::error::Error + Send + Sync>` with alignment at least4. The inner string-error wrapper has one String field. A source-owner-local **same-field layout envelope**, or a checked per-field alignment/padding upper bound, can quote the corresponding boxed allocation requests without exposing/private-importing std types. Example mathematical envelope:

- `A=max(4, align_of::<ErrorKind>(), align_of::<Box<dyn Error+Send+Sync>>())`.
- `C=size_of::<ErrorKind>() + size_of::<Box<dyn Error+Send+Sync>>() + 2*(A-1)` bounds field-interior and trailing alignment padding for the two-field std Custom shape in this pinned build. Alternatively copy the field types/alignment into an audited private quote-only proxy and use its sizeof; this is a pinned std-layout dependency, not a cross-version public ABI claim.
- `B` is the single-field String-error boxed-header envelope (one String owner, with any pinned layout padding), separately from its byte buffer.
- Scanner InvalidData text branch: conservative logical peak `E + I + C + B + G(L) + G(L)` (old literal buffer/custom+StringError boxes and new Display String coexist). Header counts may be made tighter once the exact owner-local stack/returned-value inventory is chosen; do not drop the old owner merely because it will be consumed next.
- Reader EOF branch: `E + I + G(27)`; no old custom boxes/text allocation for the static standard EOF error.
- OutOfMemory/TimedOut conversion branches: `E + I + C + B + G(L)` for old scanner error ownership, plus the actual new component/type/reason String byte capacities. `E` already covers inline NativeCoverage String descriptors; do not add those descriptors twice.
- q-overflow branch without an io error: `E` plus exact/growing byte backing for its two static Strings. Enumerate the real component/type combinations and take the branch maximum.
- Source trailing branch: `E + G(35)`.

The maximum of these finitely enumerated branches is the bootstrap diagnostic reservation. Every numeric constant in it comes from a real type, an actual compiler/stdlib layout rule, or one literal's encoded text length. No input-size multiplier or unexplained8KiB slack is needed. This covers logical requested storage only: allocator bookkeeping, Rust compiler stack frames and physical RSS remain outside that promise. The actual `[u8;8192]` skip buffer at wire.rs:76 is a real typed stack array; it is NOT evidence for an8KiB heap-diagnostic allowance.

Pinned standard-library source evidence (only these implementation facts are imported from outside the repository): [io::Error::new and Custom at Rust1.93.0](https://github.com/rust-lang/rust/blob/1.93.0/library/std/src/io/error.rs#L183) show the boxed custom wrapper; [boxed str/String-error conversion](https://github.com/rust-lang/rust/blob/1.93.0/library/alloc/src/boxed/convert.rs#L551) shows the intermediate String and one-field boxed wrapper. [ToString implementation](https://github.com/rust-lang/rust/blob/1.93.0/library/alloc/src/string.rs#L2758) starts a separate growing String for generic Display. The exact toolchain is pinned by this repository's rust-toolchain.toml to1.93.0. These implementation details require re-audit if that pin changes; stable public io::Error ABI alone is insufficient to justify its boxed backing.

## Bootstrap ordering and zero-budget failure

The reservation must exist **before** `source_tir_program_shape_v1` is invoked, because its first malformed byte can cause `wire_bad` to allocate. Successful computation of `source_program_requests` at:663 does not retroactively fund a scanner error from:604/:606, and its later diagnostic terms:654-661 cannot solve the pre-quote branch.

A new explicit-local entrypoint with a possible zero/insufficient diagnostic budget needs a nonallocating bootstrap error outcome (e.g. a new source-only static/category/scalar error variant, or a caller-provided pre-funded diagnostic owner). Returning an owned `String`/NativeCoverage String merely to report that the diagnostic reservation is insufficient would recreate the same gap. Do not claim an allocation-free refusal while constructing one of those heap owners. With sufficient prepaid bootstrap capacity, invoke the unchanged scanner/check sequence and preserve its existing error classification/text; keep the diagnostic reservation until the returned failure object is dropped or explicitly transferred. Do not change legacy ledger pricing, version checks, generic scanner grammar or normal-form validator order for this new local entrypoint.

A counting/capped `fmt::Write` sink can bound any **subsequent** caller presentation of these typed errors, or make the second Display copy exact-sized after admission. It cannot prevent existing inner `io::Error::new` or `format!` allocations by itself. Given this scanner's finite literal set, a typed pre-funded bootstrap owner is sufficient and simpler than changing every existing scanner to a new capped text grammar. If arbitrary `Read`/custom Error payloads are introduced, the47-byte reasoning no longer applies: reject/generalize that boundary explicitly, or supply a source-only structured fixed-category error with bounded scalar fields before owned rendering.

## Separate post-quote normal-form diagnostics — not certified by this scanner audit

`source_tir_program_shape_v1` comments at ledger_expansion.rs:654-661 already acknowledge input-dependent later validator diagnostics: Dim shape Debug vectors, u8 permutations and bounded names. `TirProgramV1::decode_canonical` (tir/core/src/program.rs:157-166) uses generic Borsh then format errors/normal-form validation; `validate.rs` uses `format!` of names, shapes and scalar indices. Those are not executed by the preflight scanner itself. Their bound must be derived from the decoded shape/native quote before invoking the unchanged validator, rather than treating its unvalidated dimensions as MAX_RANK.

This audit does NOT establish that the existing diagnostic expression's `+1024` literal reserve and four concurrent writer envelopes are exact/sufficient for all validator paths; that requires an additional full format-site audit. Do not replace that expression or change the normal-form/ledger error order on the basis of this report. The immediate bootstrap gap can be closed independently from this later quote review.

## Unperformed validation / handoff

No code changes, Cargo, rustc compilation, allocator instrumentation or tests performed. The literal47-byte maximum was counted directly from the enumerated local source strings. The source entrypoint's memory-only Read reachability and the std1.93.0 wrapper structure were read-reviewed; no general claim for arbitrary scanner callers or other Rust versions is made. A1/A0 implementation should select its exact local error/lease API and verify the bootstrap minimum/one-below plus every fixed scanner failure without unbounded diagnostic allocation. Scanner success/normal-form admission/model execution/credit remain separate outcomes.
