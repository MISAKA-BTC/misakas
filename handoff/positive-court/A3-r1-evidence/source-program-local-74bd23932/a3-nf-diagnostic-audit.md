# A3 Borsh / canonical decode / NF diagnostic audit — READ ONLY

Scope: existing direct `source_tir_program_shape_v1` → independently quoted `TirProgramV1::decode_canonical` → version1 normal-form validator and its known owned-error conversions. Base source inspected in the shared A3 worktree (Core unchanged by this task). Dependency pins: Borsh1.5.1 / borsh-derive1.5.1 (Cargo.lock:1264-1267), Rust1.93.0. No worktree/Core edits, Cargo, compile, tests, allocator measurements or quote publication. Only this `/tmp/a3-nf-diagnostic-audit.md` is written.

## Conclusions for A1/A0

1. Oversized names are **not** echoed by check_name: validate.rs:40-43 renders fixed role + MAX_NAME_BYTES. Duplicate param/state names at:382/:411 are emitted only after size checks and use Display, so raw UTF-8/control bytes are bounded by128 and are not Debug-escaped inside the validator.
2. The real variable text is shape/perm Debug. `n=shape.structural_items_v1()` at ledger_expansion.rs:613 bounds actual unvalidated Vec elements, including Dim ranks and byte-valued permutations, before validation. The existing :659 coefficient60*n is adequate for the identified variable-message text bound **provided the complete typed scalar/literal and wrapper derivation below is kept**. This is a content/growing-buffer finding, not certification of the whole private quote for public use.
3. Borsh invalid UTF-8 uses FromUtf8Error Display → io::Error → io-error Display String → TirError; it never Debug-renders all malformed name bytes in that path. Original invalid-name byte Vec is an ordinary decode owner already included by raw.heap at:620. The two std boxed error wrappers are separate typed requests that have no explicit q line. They must not be justified merely by unused slack in four large byte-buffer envelopes.
4. Existing :660-661 reserves FOUR growing u8 buffer envelopes. That can cover the known String construction/conversion concurrency once each text/capacity bound is derived, but it does not explicitly inventory error-owner headers/std boxes or arbitrary future Debug/presentation copies. Missing attribution must be made explicit before exposing/using an all-native owner guarantee. No new8KiB or input-bytes×guess term is proposed.
5. A1's new source-local API is intentionally a **logical quote owner without host lease/NativeRetained**. Returned errors are a caller-retention boundary, NOT a new host-lease-drop bug. The Node startup owner must pre-sum retained error String capacities or transfer/report their logical backing. This report does not change old global decode_native_tir_program_v1 behavior, ledger pricing, flags, error order or normal-form rules.

## Actual decode/error call graph

- `program.rs:157-166`: byte cap; `borsh::from_slice` at:161-162 maps its owned io::Error by `e.to_string()` into `TirError::new(Encoding, String)`; canonical re-encoding at:163; `validate(&program)` at:166. `error.rs:48-56` moves an existing String into TirError; it does not clone it. `TirError` Display prefix is fixed enum-name Debug plus `: ` (`error.rs:41`); its msg is Display, not Debug.
- Normal-form `nf` (`validate.rs:32-34`) moves the message into TirError. `shape_err` (:36-38) constructs a new outer String while its inner `msg.into()` remains live. `check_node_type` (:132-350) returns a String, then :566 wraps it through shape_err. :523 wraps check_tensor_type the same way.
- Known terminal mapping is the existing profile test at `ledger_expansion.rs:673-676` / a caller's equivalent `.map_err(|e| Decode(e.to_string()))`: old TirError.msg and the new prefixed Display String coexist. A new local API must enumerate its actual mapping, not assume every caller's later format! is covered.
- Borsh1.5.1 local dependency source `de/mod.rs:361-367`: String::from_utf8 consumes the parsed byte Vec; invalid UTF-8 retains that Vec in FromUtf8Error while `err.to_string()` creates an ASCII message; Error::new then moves that String into boxed custom error ownership. `program.rs:162` later copies io-error Display text into TirError.msg. Those buffers/boxes can coexist with already parsed prefix fields.

## q line-by-line ownership map

Here q refers ONLY to `consensus/core/src/palw_native_resource_v1/ledger_expansion.rs:616-663`. “Covered” means a concrete line matches the identified owner; it does not certify every allocation of the decoder/compiler. The compiler, StepSpace and arbitrary presentation layers have separate quotes.

| q line(s) | Actual typed owner | Diagnostic relationship / status |
|---|---|---|
| 620 `2*raw.heap` | Parsed program's nested Vec/String element storage; extra cloned constant shapes/data | Covers partial Borsh prefix fields and invalid-UTF8 byte Vec. Does NOT specifically represent io::Error Custom/StringError boxes or formatted error headers. |
| 621 `exact<TirProgramV1>(1)` | Root program struct with its inline Vec/String descriptors | Program header only; not `TirError`, `NativeDecodeError<...>` or an arbitrary local error header. |
| 622 `exact<u8>(2*max(B,1024))` | Canonical re-encode Vec, B=actual encoded input length; pinned Borsh serializer initial capacity1024 and geometric growth | Serialization working buffer, NOT another diagnostic slot. It is sequential with NF invocation but may overlap a re-encode mismatch error; count it as already present, not free diagnostic slack. |
| 625 `Tree<&str,()>` | Parameter/state name sets live through validation | Stores borrowed references. Does not clone name characters or own any diagnostic string. |
| 626 `Tree<(DType,Vec<u32>,Vec<u8>),()>` | Constant-dedup set node headers, including inline Vec descriptors | Cloned shape/data element backing is in620. No diagnostic box/writer authority. |
| 627 `vec<BlockInfo>(blocks)` | Validator's role/window info | No diagnostic owner. |
| 632 `vec<TensorType>(2*MAX_CARRY)` | Retained pre carry signature + temporary layer carry-out headers | Explicitly needed before all node outputs/ranks validate. No String/box ownership. |
| 633 `vec<Dim>(carry*n)` | Actual malformed carry-shape clones | Correctly uses n, not MAX_RANK. Makes large shape Debug inputs possible but covers the shapes, not their formatted text. |
| 637 `vec<TensorType>(MAX_NODE_INPUTS)` | One node's operand type headers | No diagnostic String; input headers remain live while check_node_type creates its error. |
| 638 `vec<Dim>(MAX_NODE_INPUTS*n)` | Operand shape clones | Covers original data cloned for type inference. |
| 640-641 four `vec<Dim>(2*n+1)` | Gather concatenation, Select broadcasts, state-row inference and overlapping shape temporaries | Covers Vec<Dim> work retained when want_shape formats a mismatch. No formatted u8 characters/boxes. |
| 643 `vec<bool>(n)` | Transpose permutation-seen bits | Perm source is Vec<u8> included in raw.heap/structural n; formatted numeric Debug list belongs to661. |
| 645 `Tree<u16,()>` | Per-block state/effect sets | No diagnostic owner. |
| 646 `vec<bool>(nodes)` | Liveness visited bits | No diagnostic owner. |
| 650 `vec<usize>(dfs)` | Root/edge DFS stack | No diagnostic owner. |
| 652 `Tree<&str,BTreeSet<u16>>` | Three-category used map headers | No diagnostic owner. |
| 653 `Tree<u16,()>` | Original + cloned used-ID sets | No diagnostic owner. |
| 659 `D=60*n+1024` | TEXT allowance, currently documented but not split into per-format semantic constants | Variable coefficient can be checked from Dim/perm grammar below; fixed1024 must be backed by the finite literal/scalar census, not asserted by size alone. |
| 660-661 four `vec<u8>(D)` | Four growing String/writer byte buffers, capacity policy G(D) | Candidate coverage for known nested/String conversion payloads; NO explicit typed std io-error boxes, escaped caller Debug, unlimited wrapper strings or retained errors after return. |

## Oversized names, escaped Unicode and malformed input

`check_name` accepts1..=128 UTF-8 BYTES; it does not normalize contents. The oversize error omits the name entirely. Duplicate-name Display emits at most128 bytes, including literal controls/Unicode; no escaped amplification there. Block names are checked at:458 but are never interpolated into NF error strings. No program values/tokenizer/context should be changed to “sanitize” them.

Borsh's invalid UTF-8 message is an ASCII scalar summary. The pinned Rust Utf8Error holds `valid_up_to:usize` and `error_len:Option<u8>`; Display uses one of two fixed templates. Bound its index by the actual string byte length, conservatively by B=source bytes.len, and its error_len by the u8 type (at most3 decimal characters; no need to rely on a guessed diagnostic length). If d(x) is checked decimal digit count, use:

    U(B) = max(44 + 3 + d(B), 42 + d(B))

The44/42 terms are actual UTF-8 byte lengths of the two literal skeletons, with dynamic fields removed. A tighter bound may use actual parsed String length/valid-sequence error range, but no name payload/escaped source data appears in these displays. [Pinned Utf8Error representation/Display](https://github.com/rust-lang/rust/blob/1.93.0/library/core/src/str/error.rs#L107).

Debug is a DIFFERENT operation. `FromUtf8Error` Debug can expose byte data; `TirError`/NativeDecodeError derived Debug may Debug-escape its owned message String. That Debug path is not called by program.rs:162 or the known terminal e.to_string mapping. If a new owner/caller chooses Debug, it must budget that renderer separately. A valid UTF-8 string's Debug length can be computed allocation-free by a counting fmt sink / per-character EscapeDebug with UTF-8 byte widths. A conservative per-character rule also derives the usual2+6*N bound for N original UTF-8 bytes (ASCII controls can become six-byte Unicode escapes); this is a character-encoding bound, not an arbitrary source-byte×memory factor. It must NOT be applied blindly to an entire unvalidated program Debug tree. A capped writer after an already-created inner String cannot fund that inner allocation retroactively.

## Typed text derivation for real NF messages

Use `n=actual structural_items`, including collection entries AND byte-vector entries (ledger_expansion.rs:571-576). String character bytes need not enter n for NF text: unchecked long names are never echoed, and echoed names are already capped128. This also means invalid UTF-8's scalar index must use B/string length, not assume it is bounded by n.

- One Dim Debug is `H` (1 byte) or `Fixed(u32)` (6-byte prefix + at most10 decimal digits +1 closing byte =17). A nonempty Vec<Dim> of r elements therefore has at most `19*r` bytes (17*r +2*(r-1)+2 brackets); empty Vec has2. Use checked `Vdim(r)=max(2,19*r)`.
- A u8 permutation Debug element has at most3 digits; list separators/brackets give `Vu8(p)=max(2,5*p)`.
- `want_shape` at validate.rs:141 prints output + inferred shape. Output/source shapes are actual scanned ranks; Gather can repeat an input and concatenate two ranks, HistAppend can add one H. Conservative combined rank bound is `3*n+1`, so the variable part is at most `57*n+19+4` including both empty/bracket allowances. This matches the existing comment's actual-Gather need, without substituting MAX_RANK for malformed carry types.
- `cannot reshape` (:150) prints two actual shapes: conservative combined rank<=2*n, yielding at most38*n+4.
- `Transpose` (:159) prints a byte-valued perm of p<=n, plus a typed usize rank; it does not print UTF-8 text. `Broadcast` (:215) prints exactly two Dim values, bounded34 bytes.
- Every numeric scalar in these templates is u8/u16/u32/u64/i64/usize on the supported64-bit profile: decimal length<=20 (including i64 sign). Dtype names are at most4 bytes, primitive names at most10; context `what` for param/const/state-index is6+20 bytes, and role-only `what` strings are shorter. The only raw input string interpolated is a previously checked param/state name<=128 bytes.

For a conservative mechanically reproducible fixed-part census, take W=max(128,26,20,10,4)=128 for each NON-vector scalar/string slot, count literal bytes/slots for every one of the67 format! sites below, and keep shape_err's message as a nested term (not a128-byte slot). Largest direct scalar-only template bound is810 bytes at validate.rs:521 (42 literal bytes +6 slots*128). Largest plain returned literal is82 bytes at:330. Shape_err's prefix is17 literal bytes +3 non-message slots*128=401; add the inner message term. TirError Display adds at most12 bytes (10-character longest kind name +colon/space).

Thus for the known Display chain, ordinary scalar/direct messages<=822; want_shape+shape_err+TirError Display<=57*n+472; reshape wrapper<=38*n+438; transpose wrapper<=5*n+465; Dim broadcast/scalar inner wrappers are smaller. Each is <=60*n+1024 for all n>=0. The four existing D-sized growing-buffer SLOTS are therefore not obviously missing an unbounded name/perm/shape text case; their content bound can be made rigorous by this finite typed census. This does NOT justify hidden boxes/error headers or future arbitrary wrapping, and it does NOT authorize public publication of the current private NF quote.

## Formatter capacity / owner coexistence

Let G(x) be the pinned existing growing-u8-Vec policy (or its proposed pure public Option port); G is DATA QUOTE arithmetic, not a native authority/lease. format!'s initial String capacity estimate is determined by its fixed literal pieces; check this against the same literal census (on pinned std it is at most twice the literal-piece length). Growing writes of Dim/perm Debug require capacity for the final text bound, not merely its literal prefix. Use `G(max(text_bound, fixed_estimate))` per actual String slot, not a guessed source-file byte factor. Internal allocator relocation/transient RSS is outside this logical requested-capacity contract; independent Rust Vec/String owners are not.

Known simultaneous phases:

| Phase | Simultaneous Rust-owned allocations | Existing attribution / missing |
|---|---|---|
| Declaration error `check_param_shape(&format!(...))` | Short caller context String + returned NF message String, plus structural validation state | Two661 writer slots can cover payloads. Moving into TirError adds no String clone. Error value/root header is separate. |
| check_tensor_type/check_node_type → shape_err | Inner error String + outer prefixed String, while operand/inferred shape Vecs may still be live | Shapes:637-641; text:two661 slots. No third inner clone from msg.into when msg is already String. |
| NF TirError → outer e.to_string | TirError.msg + new kind-prefixed String | Two661 slots; old String survives while copied. Add actual outer error header if quote includes root owners. |
| Borsh invalid UTF-8 before io error return | Partially decoded fields + invalid-name Vec in FromUtf8Error + its ASCII Display String + boxed StringError + boxed io Custom | Vec storage620; one661 payload slot. **Two boxes are not explicitly inventoried.** |
| Borsh io error → TirError.msg | Old boxed String payload/boxes + new copied String; any decoder-local prefix owners still live conservatively | Two661 payload slots; **boxes remain explicit missing items**. |
| Encoding TirError → outer Decode(String) | Old TirError.msg + new `Encoding: ...` String | Two661 slots; prior io error already consumed/dropped, so do not incorrectly retain three generations of boxes forever. |
| Re-encode mismatch | Re-encoded Vec + fixed NF/Encoding error String | Re-encode622; diagnostic661. Future copies/presentation explicit. |

The four writer count is conservative for these specifically enumerated phases. A single error returns from the first failed branch; do not sum every format site. Conversely do not spend unused structural-array or spare writer bytes as unexplained authority for an unlisted typed box. [Pinned std custom io error shape](https://github.com/rust-lang/rust/blob/1.93.0/library/std/src/io/error.rs#L183), [boxed StringError conversion](https://github.com/rust-lang/rust/blob/1.93.0/library/alloc/src/boxed/convert.rs#L551), [format String construction](https://github.com/rust-lang/rust/blob/1.93.0/library/alloc/src/fmt.rs#L639).

## What is actually reachable after a successful source scan

The source scanner has already consumed every fixed field/tag, checked bool/enum variants and exact framing; unknown bool/enum/truncation/trailing-field Borsh failures should be unreachable for the SAME immutable bytes and matching pinned Borsh schema. They remain decoder-level constructors if another caller bypasses the preflight, so do not change their semantics or delete their quote casually. UTF-8 validation is NOT performed by the scanner's String skip, making invalid UTF-8 the concrete remaining Borsh diagnostic case. Borsh1.5.1 `de/mod.rs:138-140/:173-175` owns fixed unexpected-length errors, :309-311 formats a u8 invalid bool, and borsh-derive1.5.1 `internals/deserialize/enums/mod.rs:50-52` formats a u8 enum tag. No TIR V1 field uses Borsh Option/Result/socket errors; do not import their unrelated vocabulary into this direct call graph.

## Missing items / minimal resolution proposals (NO implementation)

“Not explicitly inventoried” is a proof/attribution gap, NOT a demonstrated numerical under-reservation. The current four G(D) buffers are deliberately very conservative; there may be enough aggregate margin for the typed boxes/headers. A valid resolution can retain the old amount/semantics and prove a checked phase inequality such as:

    existing_diagnostic_bytes = 4*G(D)
    needed_phase = max(
        G(inner) + G(outer) + actual_error_headers,
        G(utf8_message) + G(io_display_copy) + StringError_box + IoCustom_box
            + actual_error_headers,
        G(tir_message) + G(tir_display_copy) + actual_error_headers)
    require needed_phase <= existing_diagnostic_bytes + explicitly_retained_bootstrap_bytes

Here bootstrap bytes may be shared ONLY if the new local owner's contract explicitly retains that logical reservation through decode/NF, and both old/new error objects' concurrency is counted. `sizeof` the actual chosen error owner and pinned std field envelopes can establish the comparison without any guessed fixed allowance. Alternatively add a separately named local diagnostic term. Do not claim that every u8 writer reservation automatically authorizes any hidden allocation, or change existing ledger/normal-form/legacy quote order to implement this proof.

- Establish an explicit pinned typed **post-decode diagnostic owner line item or checked phase attribution** for the boxed std StringError and Custom payloads, with the actual existing fixed-memory ioError/new-String coexistence. Use same-field typed envelopes / checked alignment bounds as in the earlier scanner audit, gated to audited std1.93.0. Do not add anonymous8KiB slack or bless an unexplained fixed1024 as covering arbitrary wrappers.
- If q's contract is an all-native root+backing peak (it already counts TirProgramV1 root at621), explicitly add the actual error/result root headers and local-owner wrapper chosen by A1. If the profile intentionally counts heap-only and treats stack/return headers externally, say so rather than claiming those headers are already in621.
- Keep current private NF quote private. Encode a checked helper/census for actual shapes, scalar/literal message bounds, format initial capacities and the finite writer/box phase maximum. The proposed public growingVec helper can calculate a nonallocating capacity only; it does not establish format-site coverage or program validity.
- A1's logical local owner may report per-operation peak and actual retained error backing to Node startup. Since it is intentionally not a NativeRetained/host lease, Node's caller reservation/error-retention contract must cover the returned error for as long as it remains held. The old global decoder's lease behavior is historical separate scope and is NOT changed or characterized as a new local-owner bug here. Arbitrarily many retained errors/Debug strings are not controlled by one decode quote.
- Preserve original bytes/names/scalars, normal-form/ledger error order and classification. No new name rejection, normalization, lossy UTF-8, alternate parser, model review, pricing/flag or native activation is proposed. If a caller wants bounded diagnostic presentation, count/write into an admitted sink AFTER paying for existing inner error owners, or retain a structured error whose rendering has its own explicit cap.

## Unrun adversarial checks recommended to A1/A0

Overlong valid-UTF8 name (fixed error, no echo); short accepted duplicate name containing NUL/ESC/quotes/non-ASCII (Display raw vs explicitly requested Debug separately); invalid UTF-8 near the end of a large String (typed scalar index, original Vec plus io boxes); huge unvalidated carry rank processed in an earlier-index layer block; huge Transpose u8 permutation; Gather duplicate input ranks; shape_err inner/outer coexistence and terminal e.to_string; exact/one-below construction plus caller-retained error budget. Verify classifications/order with existing entrypoint golden behavior. No such tests were run by this audit.

## Complete format-site literal/slot census (data, not a generated validator)

There are67 format! sites in current validate.rs. Literal length counts UTF-8 bytes after replacing dynamic fields with empty strings; field count includes vector slots. This list plus known scalar/vector types above is a reproducible typed source audit, not executable parser/validator logic.

| validate.rs line | Literal bytes | Dynamic slots | Existing template |
|---|---:|---:|---|
| [37](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:37) | 17 | 4 | `block {block} node {node} ({}): {}` |
| [42](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:42) | 24 | 2 | `{what} name must be 1..={MAX_NAME_BYTES} bytes` |
| [54](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:54) | 16 | 3 | `{what}: rank {} exceeds {MAX_RANK}` |
| [59](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:59) | 30 | 2 | `{what}: dimension {d} outside [1, 2^24]` |
| [64](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:64) | 23 | 2 | `{what}: {n} elements exceed 2^40` |
| [71](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:71) | 16 | 3 | `{what}: rank {} exceeds {max_rank}` |
| [76](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:76) | 30 | 2 | `{what}: dimension {d} outside [1, 2^24]` |
| [81](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:81) | 23 | 2 | `{what}: {n} elements exceed 2^28` |
| [88](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:88) | 14 | 2 | `rank {} exceeds {MAX_RANK}` |
| [95](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:95) | 28 | 1 | `dimension {n} outside [1, 2^24]` |
| [135](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:135) | 34 | 2 | `out dtype {} must equal the input's {}` |
| [141](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:141) | 36 | 2 | `declared shape {:?} is not the inferred {:?}` |
| [144](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:144) | 16 | 1 | `{what} may not be i128` |
| [150](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:150) | 21 | 2 | `cannot reshape {:?} into {:?}` |
| [159](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:159) | 30 | 2 | `{perm:?} is not a permutation of rank {r}` |
| [173](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:173) | 14 | 4 | `[{start}, {start}+{len}) exceeds {n}` |
| [197](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:197) | 18 | 1 | `dimension {i} differs` |
| [202](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:202) | 23 | 1 | `concat axis must total {total}` |
| [215](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:215) | 31 | 2 | `dimension {d:?} cannot broadcast to {od:?}` |
| [304](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:304) | 19 | 3 | `[{lo}, {hi}] is not inside {}` |
| [318](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:318) | 18 | 2 | `k = {k} outside [1, {n}]` |
| [356](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:356) | 16 | 2 | `version {} is not {TIR_PROGRAM_VERSION_V1}` |
| [369](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:369) | 11 | 1 | `2..={MAX_BLOCKS} blocks` |
| [372](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:372) | 15 | 1 | `at most {MAX_LAYERS} layers` |
| [376](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:376) | 15 | 1 | `at most {MAX_PARAMS} params` |
| [382](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:382) | 29 | 1 | `param name {} is declared twice` |
| [385](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:385) | 22 | 1 | `param {j} may not be i128` |
| [387](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:387) | 6 | 1 | `param {j}` |
| [392](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:392) | 6 | 1 | `const {j}` |
| [394](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:394) | 32 | 4 | `const {j}: {} bytes for {n} elements of {}` |
| [398](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:398) | 34 | 1 | `const {j} duplicates an earlier const` |
| [402](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:402) | 20 | 2 | `{const_bytes} const bytes exceed {MAX_CONST_BYTES}` |
| [405](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:405) | 15 | 1 | `at most {MAX_STATES} states` |
| [411](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:411) | 29 | 1 | `state name {} is declared twice` |
| [414](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:414) | 29 | 1 | `state {j} must be i8, i16 or i32` |
| [418](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:418) | 6 | 1 | `state {j}` |
| [420](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:420) | 48 | 4 | `state {j}: range [{lo}, {hi}] must be inside {} and contain 0` |
| [424](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:424) | 6 | 1 | `state {j}` |
| [426](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:426) | 42 | 2 | `state {j}: window {window} outside [1, history_bound]` |
| [432](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:432) | 25 | 1 | `at most {MAX_STATES_PER_LAYER} per-layer states` |
| [460](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:460) | 18 | 2 | `block {bi}: 1..={MAX_NODES_PER_BLOCK} nodes` |
| [463](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:463) | 32 | 2 | `block {bi}: at most {MAX_CARRY} carried tensors` |
| [466](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:466) | 50 | 1 | `block {bi}: carry_out names a node that does not exist` |
| [477](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:477) | 43 | 1 | `block {bi}: carry-in is not the carry signature` |
| [482](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:482) | 44 | 1 | `block {bi}: carry-out is not the carry signature` |
| [511](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:511) | 40 | 1 | `block {bi}: histories with different windows` |
| [521](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:521) | 42 | 6 | `block {bi} node {ni} ({}): {} inputs, want {lo}..={hi} (NF-14)` |
| [529](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:529) | 42 | 2 | `block {bi} node {ni}: refs point strictly backward` |
| [534](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:534) | 20 | 2 | `block {bi}: no carry-in {k}` |
| [537](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:537) | 17 | 2 | `block {bi}: no param {j}` |
| [539](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:539) | 46 | 2 | `block {bi}: per-layer param {j} outside a layer block` |
| [544](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:544) | 17 | 2 | `block {bi}: no const {j}` |
| [548](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:548) | 17 | 2 | `block {bi}: no state {j}` |
| [550](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:550) | 56 | 1 | `block {bi}: a Hist state is read only through its HistAppend` |
| [553](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:553) | 54 | 2 | `block {bi}: state {j}'s per_layer must match the block's role` |
| [559](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:559) | 17 | 2 | `block {bi}: no input {j}` |
| [568](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:568) | 64 | 2 | `block {bi} node {ni}: a commit point is i8, i16, i32 or idx (PALW-TIR-5)` |
| [572](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:572) | 56 | 2 | `block {bi} node {ni}: every TopK is a commit point (PALW-TIR-11)` |
| [579](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:579) | 52 | 2 | `block {bi} node {ni}: the post block writes no state (NF-19)` |
| [583](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:583) | 54 | 2 | `block {bi}: state {state}'s per_layer must match the block's role` |
| [586](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:586) | 28 | 2 | `block {bi}: state {state} written twice` |
| [591](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:591) | 54 | 2 | `block {bi}: state {state}'s per_layer must match the block's role` |
| [594](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:594) | 31 | 2 | `block {bi}: history {state} appended twice` |
| [602](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:602) | 62 | 2 | `block {bi} node {ni}: the appended row is a commit point (PALW-TIR-14)` |
| [610](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:610) | 55 | 2 | `block {bi}: carry-out node {n} is a commit point (PALW-TIR-14)` |
| [637](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:637) | 20 | 2 | `block {bi} node {dead} is dead` |
| [660](/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet/misaka/palw/tir/core/src/validate.rs:660) | 25 | 1 | `a declared {what} is never used` |

## Boundary of this report

The original pre-quote fixed-scanner diagnostic bootstrap is separately audited in /tmp/a3-source-diagnostic-audit.md. This document audits post-quote version1 decode/NF diagnostics only, not V2 programs, interval/cone/gate/price analyses, compiler plan construction, arbitrary nested StepError/API wrappers or allocator RSS. It neither publishes private Core resource authority nor certifies the complete new local Source API. Only this markdown file was created; no source/toolchain/tests changed.
