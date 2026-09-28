# PALW spec — 04b. PALW Canonical Tensor IR v1 (PALW-TIR)

> **Normative.** This chapter defines PALW-TIR v1 completely: the types, the program and its
> canonical byte encoding, the twenty-five primitives and their exact integer semantics, the
> reference evaluator, and the rules admission and the court will apply. An engineer who reads only
> this file must be able to build an implementation that is bit-identical to the reference evaluator
> (`misaka-palw-tir`) on every program and every input, and must be able to emit programs other
> implementations accept. The golden vectors under `consensus-vectors/tir-v1/` (§12) are part of
> the specification: where the prose and a vector disagree, the disagreement is a defect of this
> text and is resolved before freeze.
>
> Source: RFC-0002 (PALW Canonical Tensor IR v1), Gate 1 of its implementation. Applies to IR
> classes past the dormant fence `palw_tir_v1`. Legacy `PalwShapeProfileV3` classes keep chapter 04
> and 04a unchanged. The integer rules of [04a](04a-integer-arithmetic.md) are reused exactly where
> the legacy kernels use them; §10 shows how each legacy rule is an IR segment.
>
> Status: **Draft for review (Gate 2)**, revision 2: the findings F1–F15 of the independent second
> implementation (`docs/design/palw/tir/ref2-findings.md`) are applied — the cone environment (§9.2),
> the state-writer rule (NF-19), `prim_set_id` (§6.0, NF-1) and the error-class table (§9.3) — and the
> single step tree of Phase F (D7, §10.1). The review's editorial follow-ups N1–N4 (§9.2, §9.3, §12,
> §3.6) and admission (`tir_admit_v1`, §10.3) are in this text too; they change no value any
> program or primitive evaluates to, so the descriptor's `rev` stays 2. The primitive set and its
> semantics are frozen only at RFC-0002 Phase E; `prim_set_id` names this revision (§6.0).

Contents: §0 conventions · §1 terminology · §2 types · §3 the program · §4 the encoding ·
§5 normal form · §6 primitives · §7 ranges · §8 costs · §9 evaluation · §10 commitment and the court ·
§11 library templates (informative) · §12 golden vectors · §13 rules · §14 deviations from RFC-0002.

---

## 0. Conventions

- **MUST**, **MUST NOT**, **MAY** as in RFC 2119.
- All arithmetic in this chapter is on mathematical integers ℤ. No value is ever a float.
- `⌊a / b⌋` is floor division (toward −∞) for `b ≥ 1`; `a mod b = a − b·⌊a / b⌋ ∈ [0, b)`.
- `clamp(v, lo, hi) = min(max(v, lo), hi)`.
- Shapes are row-major: for shape `[d0, …, d(r−1)]` the element at multi-index `(i0, …, i(r−1))`
  is at linear position `Σ_k i_k · s_k` with `s_(r−1) = 1`, `s_k = s_(k+1) · d_(k+1)`. A rank-0
  tensor (scalar) has exactly one element.
- `ONE = 2^24`. "Q24" means an integer read as `value / 2^24`.
- An *error* aborts the whole evaluation it occurs in (§9.1). **Success versus error is
  normative** (PALW-TIR-34); every rule names the class of its refusal (§9.3), so two conforming
  implementations also report the same class for an input that breaks one rule, and no class ever
  decides a verdict (§9.3).

## 1. Terminology — an IR, not a VM

A PALW-TIR program is a finite static DAG of nodes grouped in blocks, a static layer schedule, and a
scan over positions whose trip count is the job's length. It has **no program counter, branch, jump,
loop or call**; every shape except the history length `H` is a constant and every cost is known at
registration (PALW-TIR-18).

| Term | Meaning |
| --- | --- |
| program | a `TirProgramV1` (§3) |
| block | a list of nodes with a carry-in signature and carry-out nodes |
| node | one primitive application: prim + attributes, operands, declared output type, commit flag |
| position | one token; a program computes the logits of one position |
| step | the evaluation of one position: `pre`, then every layer block in schedule order, then `post` |
| occurrence | one block evaluated at one place in the schedule: `(block, layer)`; `layer` is `None` for `pre`/`post` |
| run | steps at positions `0, 1, …, T−1` from the initial state |
| commit point | a node with `commit = true`; its value is committed as leaves of the step tree |
| cone | the part of an occurrence a commit point's value depends on, back to other commit points and leaves (§10.2) |
| reference evaluator | the implementation that walks the graph (§9); `misaka-palw-tir` is the first one |

## 2. Types

### 2.1 Element types (PALW-TIR-20)

| dtype | tag | range | bytes | committable | param | state/hist | const |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `i8` | 0 | `[−2^7, 2^7−1]` | 1 | yes | yes | yes | yes |
| `i16` | 1 | `[−2^15, 2^15−1]` | 2 | yes | yes | yes | yes |
| `i32` | 2 | `[−2^31, 2^31−1]` | 4 | yes | yes | yes | yes |
| `i64` | 3 | `[−2^63, 2^63−1]` | 8 | **no** | yes | no | yes |
| `i128` | 4 | `[−2^127, 2^127−1]` | 16 | **no** | **no** | no | yes |
| `idx` | 5 | `[0, 2^32−1]` | 4 | yes | yes | no | yes |

A *value of dtype t* is an integer inside t's range. Element bytes (consts, params, history rows,
vectors) are little-endian two's complement (`idx`: little-endian unsigned).

`i64` and `i128` are **internal**: they exist so that exact accumulators and fixed-point products fit
without a fused primitive, and they never cross a commit point, a carry, a state or a history
(PALW-TIR-5). `i128` is never a param.

### 2.2 Dimensions and `H` (PALW-TIR-21)

A dimension is `Fixed(n)` with `1 ≤ n ≤ 2^24`, or `H`. A shape has rank `≤ 4` and at most one `H`.

`H` is the history length of the block the tensor lives in. A block's *window* `W` is the window
shared by every `Hist` state the block appends to (all such states MUST declare the same window;
a block that appends to none has no window and no tensor of it may contain `H`). At position `pos`,

```
H = min(pos + 1, W)
```

The worst case used by the size caps and by §7/§8 is `H = W`. Every **node output**'s element count
at the worst case MUST be `≤ 2^28`; so is every const's and every state's (NF-8). A `Hist` state's
row has rank `≤ 3` and `≤ 2^28` elements, and its window is capped through its `HistAppend` output
`[W] ++ row` (a row of 1,024 elements admits a window of `2^18`, one of 1,025 does not). Params are
artifact tensors and are capped at `2^40` elements instead (NF-8, §14 deviation 7).

### 2.3 Broadcasting (PALW-TIR-22)

Two shapes `a`, `b` broadcast to `c` of rank `max(|a|, |b|)`: align trailing dimensions, treat
missing leading dimensions as `Fixed(1)`, and for each aligned pair take `x` if both are equal, the
other one if one of them is `Fixed(1)`, otherwise there is no broadcast (a shape error). `H` equals
only `H`; `Fixed(1)` broadcasts to `H`. At evaluation, an input dimension of extent 1 reads index 0
for every output index along that axis.

## 3. The program `TirProgramV1`

### 3.1 Structure

```
TirProgramV1 {
  version:          u16                 = 1
  prim_set_id:      [u8; 64]            PRIM_SET_ID_V1, the keyed hash of the prim-set descriptor (§6.0)
  token_bound:      u32                 tokens are idx values in [0, token_bound)
  history_bound:    u32                 2^18 or 2^21; positions are in [0, history_bound)
  params:           [ParamDecl]         tensors bound to the artifact inventory
  consts:           [ConstDecl]         small inline tensors, ≤ 64 KiB in total
  states:           [StateDecl]         Fixed and Hist states
  blocks:           [Block]
  schedule:         Schedule            { pre: u8, layers: [u8], post: u8 }
  logits:           u16                 index of the logits node in the post block
  logits_scheme_id: [u8; 64]            the class's logits commitment scheme (as in 04)
}
ParamDecl { name: String, dtype, shape: [u32], per_layer: bool }
ConstDecl { dtype, shape: [u32], data: [u8] }                      little-endian elements
StateDecl { name: String, kind: Fixed{lo: i64, hi: i64} | Hist{window: u32}, dtype, shape: [u32], per_layer: bool }
Block     { name: String, carry_in: [TensorType], nodes: [Node], carry_out: [u16] }
Node      { prim: Prim, inputs: [Ref], out: TensorType, commit: bool }
Ref       = Node(u16) | CarryIn(u8) | Param(u16) | Const(u16) | State(u16) | Input(u8)
TensorType { dtype, shape: [Dim] },  Dim = Fixed(u32) | H
```

### 3.2 Operands (`Ref`) and their types

| Ref | refers to | type |
| --- | --- | --- |
| `Node(i)` | node `i` of the same block, `i` < the referring node's index (strictly backward) | that node's `out` |
| `CarryIn(k)` | the block's `k`-th carry-in | `carry_in[k]` |
| `Param(j)` | param `j`; if `per_layer`, its instance at the running layer | `(dtype, shape)` of the declaration |
| `Const(j)` | const `j` | `(dtype, shape)` of the declaration |
| `State(j)` | `Fixed` state `j`'s value **at the start of the position** (per-layer instance if `per_layer`) | `(dtype, shape)` of the declaration |
| `Input(0)` | the token | `idx`, rank 0 |
| `Input(1)` | the position `pos` | `idx`, rank 0 |

A `Hist` state has no `Ref`: it is read only through its `HistAppend` (§6.6).

### 3.3 Schedule, occurrences and node slots

A step evaluates the occurrences

```
o_0 = (pre, None),   o_(1+l) = (layers[l], l) for l in 0..L,   o_(L+1) = (post, None)
```

in that order, where `L = |layers| ≤ 1024`. The carry-out of `o_i` is the carry-in of `o_(i+1)`.
`pre` has no carry-in; every layer block has carry-in and carry-out equal to the *carry signature*
(the types of `pre`'s carry-out nodes); `post` has the carry signature as carry-in, no carry-out, and
its node `logits` is the step's result.

The **node slot** of node `n` in occurrence `o_i` (the RFC §6 commitment coordinate `node_slot`) is

```
slot(o_i, n) = Σ_(j < i) |nodes(block(o_j))|  +  n
```

A per-layer param or state used by a block has one instance per layer at which that block runs,
indexed by the layer number `l`.

### 3.4 States (PALW-TIR-10)

- **`Fixed { lo, hi }`** — a tensor of the declared shape and dtype, initially all zeros (0 MUST be
  in `[lo, hi]`). `Ref::State(j)` reads its value at the start of the position. At most one
  `StateWrite` per state per block; its output (the input saturated to `[lo, hi]`) becomes the
  state's value for the next position. A state no block writes keeps its value.
- **`Hist { window }`** — an append-only history of rows of the declared shape and dtype, initially
  empty. Exactly the `HistAppend` node of the block appends to it (at most one per state per block);
  its output is the last `H = min(pos + 1, window)` rows, oldest first, this position's row last.
  `1 ≤ window ≤ history_bound`; full attention uses `window = history_bound`.

A `Fixed` or `Hist` state's `per_layer` MUST equal the role of every block that uses it: per-layer
states only in layer blocks, global states only in `pre`/`post`.

**No two nodes of one step write the same state instance** (NF-19). A block writes a state at most
once (one `StateWrite` or one `HistAppend`), and `post` writes no state at all — so a global state is
written only by `pre` (`post` may read it) and a per-layer instance only by the one occurrence of its
layer. The next position's value of every instance is therefore defined by exactly one node, whatever
order an implementation applies effects in.

### 3.5 Params and consts

A param is bound to the artifact inventory by `name` (per-layer params at each layer that runs a
block referencing them; the binding convention is chapter 03's, extended in Gate 2). The reference
evaluator receives each param instance as a tensor and MUST check that it has exactly the declared
dtype and shape. **Params take their dtype's full range**: weights are not trusted (§7).

A const is `data`, the little-endian encoding of `Π shape` elements of `dtype`.

### 3.6 Identity (PALW-TIR-7)

A program's identity is the keyed hash of its canonical encoding (§4), under the chain's `Hash64` id
discipline — **BLAKE2b with a 64-byte output, keyed by the 32 ASCII bytes
`misaka-palw/tir/graph-ir-root/v1`, over `encode(program)`** (no length prefix, no terminator):

```
graph_ir_root = BLAKE2b-512(key = "misaka-palw/tir/graph-ir-root/v1", encode(program))
prim_set_id   = BLAKE2b-512(key = "misaka-palw/tir-prim-set-id/v1",   prim-set descriptor)   (§6.0)
```

The second line is §6.0's definition, repeated: the same discipline, keyed by the 30 ASCII bytes
`misaka-palw/tir-prim-set-id/v1`, over the descriptor's ASCII bytes, giving `PRIM_SET_ID_V1`.

The canonical encoding is the program: every field above is part of the identity, including names,
the order of nodes (it fixes the node slots), the commit flags, `prim_set_id` (which MUST be
`PRIM_SET_ID_V1`, NF-1) and `logits_scheme_id`. The identity MUST NOT depend on anything outside the
program: thread counts, repacking, backends, fused kernels, tile shapes a backend chooses, or the
physical layout of the artifact beyond the param binding.

*Vector.* Every program vector of §12 carries its `graph_ir_root_hex`; the 470-byte program of
`programs/fixed-state-saturation.json` (its `program_borsh_hex`) has

```
graph_ir_root = dcc4422a07d843bd575c689b18c8b2e41812d69589ca97749d722b2ba987ea80
                eb8ad3ffa4dcac6672befc8ecc0c909f6f0bc09711336095def307d6cfa89c88
```

A *class* is more than its program — its commitment layout, its weights (the artifact root) and its
tokenizer — and its identity is Phase F's `tir_class_id_v1`, which hashes `graph_ir_root` with them
under a key of its own (`docs/design/palw/tir/phase-f-integration.md` §2.3,
`consensus/core/src/palw_tir_class_v1.rs`). The IR itself never hashes: `misaka-palw-tir` exposes the
canonical bytes and the caller keys them.

## 4. The canonical byte encoding — the public interface (PALW-TIR-19)

Importers, compilers and second implementations target PALW-TIR through these bytes and nothing
else. The encoding is **Borsh** with the following exact rules; every type below is listed with its
field order and every enum with its tags. There is no other valid encoding of a program.

### 4.1 Primitive encodings

| type | encoding |
| --- | --- |
| `u8` | 1 byte |
| `u16`, `u32`, `u64`, `i64` | 2, 4, 8, 8 bytes, little-endian (`i64` two's complement) |
| `bool` | 1 byte: `0x00` false, `0x01` true; **any other byte is invalid** |
| `[u8; 64]` | 64 bytes, no length prefix |
| `[T]` (sequence) | `u32` LE element count `n`, then `n` encodings of `T` |
| `String` | `u32` LE byte length, then that many bytes, which MUST be valid UTF-8 |
| struct | its fields in the order listed, concatenated, no padding |
| enum | one `u8` tag, then the fields of that variant in the order listed; an unknown tag is invalid |

### 4.2 The program's types, field by field

```
TirProgramV1 := version u16 · prim_set_id [u8;64] · token_bound u32 · history_bound u32 ·
                params [ParamDecl] · consts [ConstDecl] · states [StateDecl] · blocks [Block] ·
                schedule Schedule · logits u16 · logits_scheme_id [u8;64]
ParamDecl    := name String · dtype DType · shape [u32] · per_layer bool
ConstDecl    := dtype DType · shape [u32] · data [u8]
StateDecl    := name String · kind StateKind · dtype DType · shape [u32] · per_layer bool
StateKind    := tag 0: Fixed · lo i64 · hi i64
              | tag 1: Hist  · window u32
Block        := name String · carry_in [TensorType] · nodes [Node] · carry_out [u16]
Node         := prim Prim · inputs [Ref] · out TensorType · commit bool
TensorType   := dtype DType · shape [Dim]
Dim          := tag 0: Fixed · n u32
              | tag 1: H
DType        := tag 0 i8 | 1 i16 | 2 i32 | 3 i64 | 4 i128 | 5 idx
Ref          := tag 0: Node u16 | 1: CarryIn u8 | 2: Param u16 | 3: Const u16 | 4: State u16 | 5: Input u8
Schedule     := pre u8 · layers [u8] · post u8
Rounding     := tag 0 Floor | 1 HalfUp | 2 HalfAwayFromZero
Cmp          := tag 0 Eq | 1 Ne | 2 Lt | 3 Le | 4 Gt | 5 Ge
```

### 4.3 `Prim`: tags and attribute layouts

| tag | primitive | attribute fields, in order |
| --- | --- | --- |
| 0 | `Reshape` | — |
| 1 | `Transpose` | `perm [u8]` |
| 2 | `Slice` | `axis u8 · start u32` |
| 3 | `Concat` | `axis u8` |
| 4 | `Broadcast` | — |
| 5 | `Iota` | `axis u8 · start i64 · step i64` |
| 6 | `Gather` | `axis u8 · batch_dims u8` |
| 7 | `Cast` | — |
| 8 | `Add` | — |
| 9 | `Sub` | — |
| 10 | `Mul` | — |
| 11 | `MatMul` | — |
| 12 | `ReduceSum` | `axis u8` |
| 13 | `ReduceMax` | `axis u8` |
| 14 | `Div` | `rule Rounding` |
| 15 | `Clamp` | `lo i64 · hi i64` |
| 16 | `Log2Floor` | — |
| 17 | `IntExp` | — |
| 18 | `IntRsqrt` | — |
| 19 | `IntLn` | — |
| 20 | `Compare` | `cmp Cmp` |
| 21 | `Select` | — |
| 22 | `TopK` | `axis u8 · k u32` |
| 23 | `StateWrite` | `state u16` |
| 24 | `HistAppend` | `state u16` |

A primitive's target shape or type is NOT an attribute where the node's `out` already states it
(`Reshape`, `Broadcast`, `Iota`, `Cast`, `Slice`'s length): one fact, one place, one encoding.

### 4.4 Decoding (PALW-TIR-7)

`decode_canonical(bytes)` MUST: refuse more than 262,144 bytes; decode strictly by §4.1–4.3 (every
byte consumed, no trailing byte, every tag known, every `bool` 0 or 1, every `String` UTF-8); require
that re-encoding the decoded program reproduces `bytes` exactly; and require the normal form of §5.
Only then is the program admitted to the further analyses (§7, §8, §10) of `tir_admit_v1`.

The example program of `consensus-vectors/tir-v1/encoding.json` (`valid`) is a byte-exact instance of
this layout; its mutations pin the refusals.

## 5. Normal form (PALW-TIR-6/7/8)

A decoded program is in normal form iff every rule below holds. Violations are refusals, never
panics.

**Program**
- NF-1 `version = 1`; `prim_set_id = PRIM_SET_ID_V1` (§6.0); `history_bound ∈ {2^18, 2^21}`;
  `token_bound ≥ 1`.
- NF-2 `2 ≤ |blocks| ≤ 16` (`pre` and `post` are distinct, NF-3); `|schedule.layers| ≤ 1024`; `|params| ≤ 4096`; `|states| ≤ 64`, of which
  at most 16 are `per_layer`.
- NF-3 `pre`, `post` and every `layers[l]` index an existing block; `pre ≠ post`; neither `pre` nor
  `post` appears in `layers`; every block is `pre`, `post` or in `layers`.
- NF-4 `pre.carry_in` is empty; `post.carry_out` is empty; every block has `≤ 8` carry-ins and
  carry-outs and every carry-out index names an existing node.
- NF-5 The carry signature (the out types of `pre`'s carry-out nodes) has committable dtypes and no
  `H`; every layer block's `carry_in` and carry-out types, and `post`'s `carry_in`, equal it exactly.
- NF-6 `logits` indexes a node of `post` that is committed, has a committable dtype and no `H`.

**Declarations**
- NF-7 Param names, and state names, are 1..=128 bytes and unique among params (resp. states); block
  names are 1..=128 bytes. Params are not `i128`.
- NF-8 Every param, const, `Fixed` state shape has rank `≤ 4` (a `Hist` row: `≤ 3`) and dimensions in
  `[1, 2^24]`; consts and states have `≤ 2^28` elements, params `≤ 2^40` (an artifact tensor — a
  152,064 × 3,584 embedding, a 256-expert weight — is read by rows and tiles, and its size is the
  artifact's business, chapter 03).
- NF-9 A const's `data` has exactly `elements × width(dtype)` bytes; the consts total `≤ 65,536`
  bytes; no two consts are identical in `(dtype, shape, data)`.
- NF-10 States are `i8`, `i16` or `i32`. `Fixed`: `lo ≤ 0 ≤ hi`, both inside the dtype. `Hist`:
  `1 ≤ window ≤ history_bound`.
- NF-11 Every declared param, const and state is used by some node (as an operand, or as the target
  of a `StateWrite`/`HistAppend`).

**Blocks and nodes**
- NF-12 Every block has 1..=512 nodes.
- NF-13 A block's `HistAppend` targets all declare the same window (the block's window, §2.2).
- NF-14 Every node has the arity of its primitive (§6), `≤ 8` inputs, and every `Ref` exists:
  `Node(i)` with `i` less than the node's own index, `CarryIn(k)` with `k < |carry_in|`, `Input(j)`
  with `j ≤ 1`, `State(j)` naming a `Fixed` state.
- NF-15 Per-layer params are referenced only from layer blocks; a state is referenced (read, written
  or appended) only from blocks whose role matches its `per_layer` (§3.4).
- NF-16 Every node's declared `out` is a legal tensor type (§2.2, `H` only if the block has a window)
  and equals the type the primitive's type rule (§6) gives for its operand types.
- NF-17 A committed node has a committable dtype (PALW-TIR-5).
- NF-18 Every `TopK` node is committed (PALW-TIR-11).
- NF-19 **No two nodes of one step write the same state instance**: at most one `StateWrite` per
  state per block, at most one `HistAppend` per state per block, and **`post` contains no
  `StateWrite` and no `HistAppend`** (so a global state is written only by `pre`, NF-15; §3.4).
- NF-20 The input of every `HistAppend` is a committed node or a carry-in (PALW-TIR-14).
- NF-21 Every carry-out node is committed.
- NF-22 **No dead node**: every node is reachable backwards (through `Node` refs) from a root — a
  committed node, a `StateWrite`, a `HistAppend`, a carry-out node or (in `post`) the logits node.

Canonical attribute forms are enforced by the type rules: a `Transpose` permutation is a
permutation, a `Clamp` has `lo ≤ hi` inside `out.dtype`, and so on (§6). NF-1..22 are exactly the
checks of `misaka_palw_tir::validate::validate`; §9.3 gives the class each one reports.

## 6. The primitives (PALW-TIR-1)

### 6.0 The set

Twenty-five primitives, every one named and defined by mathematics, none by an architecture or a
model (freeze criterion 1). The *prim-set descriptor* whose network hash is `prim_set_id` is the
ASCII string

```
palw-tir/v1/spec=04b-tensor-ir/rev2/q=24/prims=0:Reshape,1:Transpose,2:Slice,3:Concat,4:Broadcast,5:Iota,6:Gather,7:Cast,8:Add,9:Sub,10:Mul,11:MatMul,12:ReduceSum,13:ReduceMax,14:Div,15:Clamp,16:Log2Floor,17:IntExp,18:IntRsqrt,19:IntLn,20:Compare,21:Select,22:TopK,23:StateWrite,24:HistAppend
```

and `prim_set_id` is its keyed hash under the chain's `Hash64` id discipline (the one
`kernel_semantics_id_v1` uses): **BLAKE2b with a 64-byte output, keyed by the 30 ASCII bytes
`misaka-palw/tir-prim-set-id/v1`, over the descriptor's ASCII bytes** (no length prefix, no
terminator):

```
PRIM_SET_ID_V1 = 61fa4aa57adfc79053c5e517515e50c7ae7c036abc43ff931053144691ba31c9
                 212539a93514f831a8472ca75a39b094177736aa5818eeae547bb31fbf89f589
```

A program MUST declare exactly this value (NF-1; `encoding.json` case `prim_set_id_not_v1`). The
`rev` component names the revision of this text; revision 2 is the text after the second
implementation's findings.

| kind | primitives | may sit in an order-free region |
| --- | --- | --- |
| **S** structure / indexing | Reshape, Transpose, Slice, Concat, Broadcast, Iota, Gather | yes |
| **E** exact arithmetic | Cast, Add, Sub, Mul, MatMul, ReduceSum, ReduceMax | yes |
| **L** lossy | Div (Floor / HalfUp / HalfAwayFromZero), Clamp (Saturate), Log2Floor | no |
| **T** integer transcendentals (Q24) | IntExp, IntRsqrt, IntLn | no |
| **X** selection | Compare, Select, TopK | no |
| **State** | StateWrite (Saturate), HistAppend | no |

Everything else — `Requantize`, `Rescale`, `RoundingShiftRight`, `SRDHM`, `IntRecip`, `IntSigmoid`,
SiLU, GELU, softmax, RMSNorm, LayerNorm, L2Norm, RoPE, attention, the gated delta rule, the
selective scan, WKV, the conv window, routing and the MoE combine — is a library subgraph (§11).
There is no `BoundedScan`, `BoundedMap` or `BoundedReduce` (PALW-TIR-36).

### 6.1 Rules common to every primitive

- **The exact-result rule (PALW-TIR-23).** Each primitive below defines a mathematical integer for
  every output element. If that integer is not a value of `out.dtype`, the evaluation fails with an
  error (class `Overflow`). Nothing wraps; nothing saturates except `Clamp` and `StateWrite`, whose
  definitions include the saturation.
- **Operands** are values of their dtypes and have the shapes the type rule states; the output has
  exactly `out`'s shape at the running `H`.
- Every output element's value is defined independently of evaluation order.

In the entries below, `x`, `a`, `b`, `c`, `d` are operands in input order; `out` is the node's
declared type; `n(t)` is the element count of `t`; "row-major" iterations are over the output.

### 6.2 Structure and indexing (kind S)

**`Reshape` (0)** — 1 input. *Type:* `out.dtype = x.dtype`; if neither shape has `H`, the element
counts are equal; if both have exactly one `H`, the products of the dimensions before `H` are equal
and the products after `H` are equal; otherwise a type error. *Value:* the elements of `x` in
row-major order, reinterpreted in `out.shape`.

**`Transpose` (1)** `perm` — 1 input. *Type:* `perm` is a permutation of `0..rank(x)`;
`out.shape[i] = x.shape[perm[i]]`; `out.dtype = x.dtype`. *Value:* `out[i_0, …] = x[j]` where
`j[perm[k]] = i_k` for every `k`.

**`Slice` (2)** `axis, start` — 1 input. *Type:* `axis < rank`; `x.shape[axis]` and
`out.shape[axis]` are both `Fixed` (a slice never touches `H`), `len = out.shape[axis]`,
`start + len ≤ x.shape[axis]`; every other dimension equal; `out.dtype = x.dtype`. *Value:*
`out[… i_axis …] = x[… i_axis + start …]`.

**`Concat` (3)** `axis` — 2 to 8 inputs. *Type:* all inputs and `out` have the same dtype and rank;
every dimension other than `axis` equal to `out`'s; along `axis` every input is `Fixed` and the
extents sum to `out.shape[axis]`. *Value:* the inputs laid end to end along `axis`, in input order.

**`Broadcast` (4)** — 1 input. *Type:* `rank(x) ≤ rank(out)`; aligned on trailing dimensions, each
input dimension equals the output's or is `Fixed(1)`; `out.dtype = x.dtype`. *Value:* §2.3.

**`Iota` (5)** `axis, start, step` — no input. *Type:* `axis < rank(out)` (so `rank ≥ 1`); any
`out.dtype`. *Value:* `out[i] = start + step · i_axis`; the exact-result rule applies (e.g. an
`idx` Iota with a negative value fails). With `step = 0` it is a fill.

**`Gather` (6)** `axis, batch_dims` — inputs `data`, `indices`. *Type:* with `A = axis`,
`B = batch_dims`: `A < rank(data)`, `B ≤ A`, `B ≤ rank(indices)`,
`data.shape[0..B] = indices.shape[0..B]`, `data.shape[A]` is `Fixed` (no gather along `H`),
`indices.dtype ≠ i128`, `out.dtype = data.dtype`, and

```
out.shape = data.shape[0..A] ++ indices.shape[B..] ++ data.shape[A+1..]
```

*Value:* let `m = rank(indices) − B`. For an output multi-index `o`:

```
ii = o[0..B] ++ o[A .. A+m]                  (the indices' multi-index)
v  = indices[ii]
error (class Index) unless 0 ≤ v < data.shape[A]
out[o] = data[ o[0..A] ++ [v] ++ o[A+m ..] ]
```

### 6.3 Exact arithmetic (kind E)

**`Cast` (7)** — 1 input. *Type:* `out.shape = x.shape`; any dtypes. *Value:* `out[i] = x[i]`
(exact-result rule: a value outside `out.dtype` fails).

**`Add` (8), `Sub` (9), `Mul` (10)** — 2 inputs. *Type:* `out.shape` is the broadcast of the
operand shapes (§2.3); any dtypes. *Value:* `a[i] + b[i]`, `a[i] − b[i]`, `a[i] · b[i]` on the
broadcast elements, exactly (exact-result rule).

**The order-free sum rule (PALW-TIR-24).** For an exact reduction producing an output element from
terms `t_1 … t_n`, let `P = Σ_{t_k > 0} t_k` and `N = Σ_{t_k < 0} t_k`. The evaluation fails (class
`Overflow`) unless `N ≥ min(out.dtype)` and `P ≤ max(out.dtype)`; otherwise the value is `P + N`.
`P` and `N` are the extreme partial sums over every order and every grouping of the terms, so the
rule holds exactly when every partial sum of every association fits — the condition that lets an
implementation reorder the reduction (§6.8). An implementation whose own accumulator is narrower than
`P` or `N` MUST detect the condition some other exact way; it MUST NOT report success where this
rule fails.

**`MatMul` (11)** — inputs `a`, `b`. *Type:* `rank(a), rank(b) ≥ 2`; neither operand is `i128` or
`idx`; `out.dtype ≠ idx`; `a.shape[−1] = b.shape[−2]` (both `Fixed` and equal, or both `H`); the batch
dimensions `a.shape[..−2]`, `b.shape[..−2]` broadcast to `batch`, and
`out.shape = batch ++ [a.shape[−2], b.shape[−1]]`. *Value:* for every batch index `β` (mapped into
each operand by broadcasting) and every `(r, c)`:

```
terms = [ a[β_a, r, t] · b[β_b, t, c]  for t in 0..K ]      (K = a.shape[−1])
out[β, r, c] = order-free sum of terms into out.dtype
```

**`ReduceSum` (12)** `axis` — 1 input. *Type:* `axis < rank`; `out.shape` is `x.shape` with
`shape[axis] = Fixed(1)` (the axis is kept); any `out.dtype`. *Value:* the order-free sum of the
elements along `axis`.

**`ReduceMax` (13)** `axis` — 1 input. *Type:* as `ReduceSum`, and `out.dtype = x.dtype`. *Value:*
the maximum along `axis` (every extent is `≥ 1`).

### 6.4 Lossy primitives (kind L)

**`Div` (14)** `rule` — inputs `x`, `d`. *Type:* `out.shape` is the broadcast of the operand
shapes; any dtypes. *Value:* for each element, error (class `Divisor`) unless `d ≥ 1`; then

```
Floor:             q = ⌊x / d⌋
HalfUp:            q = ⌊x / d⌋ + (1 if 2·(x mod d) ≥ d else 0)          -- = ⌊x/d + 1/2⌋
HalfAwayFromZero:  m = |x|;  q' = ⌊m / d⌋ + (1 if 2·(m mod d) ≥ d else 0);  q = sign(x) · q'
```

then the exact-result rule for `q`. These are the three rounding rules of 04a C1 at every width:
`Floor` is the internal `>> k`; `HalfAwayFromZero` with `d = 2^s` is `RoundingShiftRight` (rounding
the MAGNITUDE — never the `(x ± 2^(s−1)) >> s` form); `HalfUp` with `d = 2^31` is gemmlowp's SRDHM
rounding (§11.1). A division by `2^s` is how PALW-TIR shifts right; there is no shift primitive.

**`Clamp` (15)** `lo, hi` — 1 input. *Type:* `lo ≤ hi`, both values of `out.dtype`;
`out.shape = x.shape`; any input dtype. *Value:* `clamp(x, lo, hi)`. This is the `Saturate` rule and
the only narrowing that loses information.

**`Log2Floor` (16)** — 1 input. *Type:* `out.shape = x.shape`; any dtypes. *Value:* `⌊log2 x⌋`
(the index of the highest set bit) for `x ≥ 1`, and `−1` for `x ≤ 0`.

### 6.5 Integer transcendentals (kind T, PALW-TIR-26)

*Type* (all three): 1 input, of any dtype but `i128` (the value is used as a 64-bit quantity);
`out.shape = x.shape`; any `out.dtype`. All three read and write **Q24**, elementwise; their outputs
obey the exact-result rule. Constants (all decimal integers):

```
K = 24, ONE = 16777216, LN2_Q = 11629080
POLY2_A = 6014632, POLY2_B = 22699573, POLY2_C = 5771362
RSQRT_SEED[0..16] = 15395829, 14307657, 13421772, 12682383, 12053107, 11509075, 11032629, 10610843,
                    10234005, 9894662, 9586980, 9306325, 9048957, 8811825, 8592409, 8388608
```

Every `>>` below is floor division by a power of two; every `/` is floor division of non-negative
integers. Intermediates are mathematical integers (they fit 64 bits for every input; 128 bits is
always enough).

**`IntExp` (17)** — `exp(x / 2^24) · 2^24` for `x ≤ 0` (ADR-0040 F1):

```
x' = min(x, 0)
if x' ≤ −31·LN2_Q:  return 0
z  = (−x') / LN2_Q                         -- 0 ≤ z ≤ 30
p  = x' + z·LN2_Q                          -- −LN2_Q < p ≤ 0
t  = p + POLY2_B
poly = ((POLY2_A · ((t·t) >> 24)) >> 24) + POLY2_C     -- the SHIFTED SQUARE form, not Horner
return Div_HalfAwayFromZero(poly, 2^z)
```

`IntExp(0) = 16,781,800` is its maximum over all inputs; it is non-decreasing inside a
range-reduction bucket but NOT across bucket edges (`IntExp(−LN2_Q + 1) < IntExp(−LN2_Q)`).

**`IntRsqrt` (18)** — `2^24 / √(v / 2^24)` (ADR-0040 F2); `v ≤ 0` gives 0:

```
if v ≤ 0: return 0
e = ⌊(Log2Floor(v) − 24) / 2⌋
m = v >> 2e  if e ≥ 0,  v · 2^(−2e) if e < 0            -- m ∈ [2^24, 2^26)
i = min(15, max(0, ((m − ONE) · 16) / (3·ONE)))
y = RSQRT_SEED[i]
repeat 3 times:
    y2  = (y·y) >> 24
    my2 = (m·y2) >> 24
    y   = (y · (3·ONE − my2)) >> 25                     -- floor, also of a negative product
    if y ≤ 0: y = 1
return y >> e  if e ≥ 0,  y · 2^(−e) if e < 0
```

The output lies in `[0, 68,719,472,640]`; the maximum is `IntRsqrt(1) = 2^36 − 2^12`.

**`IntLn` (19)** — `ln(x / 2^24) · 2^24` (ADR-0052 D); `x ≤ 0` gives 0:

```
if x ≤ 0: return 0
s  = Log2Floor(x) − 24
m  = x >> s  if s ≥ 0,  x · 2^(−s) if s < 0            -- m ∈ [2^24, 2^25)
t  = ((m − ONE) · 2^24) / (m + ONE)                    -- 0 ≤ t < ONE/3
t2 = (t·t) >> 24
term = t;  sum = t
for d in 3, 5, 7, 9, 11:  term = (term·t2) >> 24;  sum = sum + term / d
return 2·sum + s·LN2_Q
```

For `x ≥ 1`, `IntLn(x) ∈ [s·LN2_Q, (s+1)·LN2_Q)`; over `i64` inputs the output is in
`[−24·LN2_Q, 39·LN2_Q)`. *Margin (informative):* the series part `2·sum` stays in
`[0, 11,629,070]` for every mantissa in `[2^24, 2^25)` — only **10 units** below `LN2_Q` (checked
exhaustively by the second implementation). The half-open bound, and so §7's `IntLn` interval, rests
on that margin; a change of constants must re-check it.

These are bit-for-bit `palw_base0::int_exp`, `palw_base0::int_rsqrt` and
`palw_qwen36_ops::q36_int_ln` (the latter returns "no value" for `x ≤ 0`; PALW-TIR defines 0, which is
what every legacy caller maps it to). `tests/kat_base0.rs` reproduces the frozen BASE-0 KAT digest
through them.

### 6.6 Selection (kind X, PALW-TIR-11)

**`Compare` (20)** `cmp` — 2 inputs. *Type:* `out.shape` is the broadcast; `out.dtype = i8`.
*Value:* `1` if `a cmp b` holds (Eq `=`, Ne `≠`, Lt `<`, Le `≤`, Gt `>`, Ge `≥`) else `0`.

**`Select` (21)** — inputs `c`, `a`, `b`. *Type:* `out.shape` is the broadcast of all three; any
dtypes. *Value:* `a[i]` if `c[i] ≠ 0` else `b[i]` (exact-result rule on the chosen value).

**`TopK` (22)** `axis, k` — 1 input. *Type:* `axis < rank`; `x.shape[axis] = Fixed(n)` (never `H`);
`1 ≤ k ≤ n`; `out.shape` is `x.shape` with `shape[axis] = Fixed(k)`; `out.dtype = idx`. *Value:* for
every row along `axis`, order the indices `0..n` by (value descending, index ascending), keep the
first `k`, and emit the kept indices **in ascending index order**. Ties therefore go to the lowest
index, and the committed set does not depend on how equal values were ordered (ADR-0052 B). Every
`TopK` is a commit point (NF-18).

### 6.7 State (PALW-TIR-10)

**`StateWrite` (23)** `state` — 1 input. *Type:* the state is `Fixed{lo, hi}`; `x.shape` and
`out.shape` equal the state's shape; `out.dtype` is the state's dtype; any input dtype. *Value:*
`clamp(x, lo, hi)` (the `Saturate` rule). *Effect:* after the step succeeds, this value is the
state's value at the next position (per-layer instance at the running layer).

**`HistAppend` (24)** `state` — 1 input `row`. *Type:* the state is `Hist{window}`; `row` has
exactly the state's dtype and row shape; `out = (state dtype, [H] ++ row shape)`. *Value:* with
`prior` = the rows appended at the previous `min(pos, window − 1)` positions (oldest first),
`out = prior ++ [row]` along the first axis — `H = min(pos + 1, window)` rows. *Effect:* after the
step succeeds, `row` is appended; rows older than `window − 1` positions are no longer visible.

### 6.8 Order-free regions (PALW-TIR-4)

An *order-free region* is a maximal connected set of nodes of kinds S and E, none of which is a
commit point. Inside a region an implementation MAY reassociate, reorder, regroup, tile, vectorise or
parallelise the arithmetic, because (a) no node in it rounds, saturates, divides or selects, and (b)
admission proves, and the evaluator checks (PALW-TIR-24), that every partial sum of every order fits
its declared type. A region ends at every L, T, X and State node and at every commit point.
Everywhere else an implementation MUST produce exactly the values this chapter defines; it MAY
compute them any way it likes (fused kernels, F-1/F-2 of RFC-0002 §7).

## 7. Range analysis (PALW-TIR-9) — the transfer functions

`tir_admit_v1` (§10.3) assigns every tensor an integer interval `[lo, hi]` and refuses the program
unless every obligation below holds. The rules are normative now so that the analysis is identical
on every node, and they are already executable: `misaka_palw_tir::interval::analyze_ranges`.
`tests/intervals.rs` checks them sound against the reference evaluator (operands drawn anywhere
inside random intervals, endpoints included, never leave the transferred interval and never
overflow) and shows the five corpus test programs admissible.

**Leaves.** `Param`: its dtype's full range (weights are untrusted). `Const`: `[min data, max data]`.
`State` (Fixed): `[lo, hi]`. `CarryIn`: its dtype's full range. `Input(0)`: `[0, token_bound − 1]`.
`Input(1)`: `[0, history_bound − 1]`. `H` at its worst case `W` (for `Iota` and extents).

**Nodes** (`x`, `a`, `b`, `c`, `d` operand intervals; `corners(f)` = min and max of `f` over the four
endpoint pairs):

| primitive | output interval | obligation |
| --- | --- | --- |
| Reshape, Transpose, Slice, Broadcast | `x` | — |
| Concat | union of the inputs | — |
| Iota | `[min(e_0, e_n), max(e_0, e_n)]`, `e_i = start + step·i`, `n` = extent − 1 | ⊆ out dtype |
| Gather | `data` | `indices ⊆ [0, data.shape[axis] − 1]` |
| Cast | `x` | ⊆ out dtype |
| Add / Sub / Mul | `[a.lo+b.lo, a.hi+b.hi]` / `[a.lo−b.hi, a.hi−b.lo]` / `corners(a·b)` | ⊆ out dtype |
| MatMul | `[K·min(T.lo, 0), K·max(T.hi, 0)]`, `T = corners(a·b)`, `K` = contraction extent | ⊆ out dtype |
| ReduceSum | `[n·min(x.lo, 0), n·max(x.hi, 0)]`, `n` = axis extent | ⊆ out dtype |
| ReduceMax | `x` | — |
| Div | `corners(q(x, d))` for the node's rule | `d.lo ≥ 1`; ⊆ out dtype |
| Clamp | `[clamp(x.lo), clamp(x.hi)]` | — |
| Log2Floor | `[f(x.lo), f(x.hi)]` | ⊆ out dtype |
| IntExp | `[0, 0]` if `x.hi ≤ −31·LN2_Q`, else `[0, 16781800]` | ⊆ out dtype |
| IntRsqrt | `[0, 0]` if `x.hi ≤ 0`, else `[0, min(68719472640, ONE·2^(−e))]` with `e = ⌊(Log2Floor(max(x.lo, 1)) − 24) / 2⌋` (`ONE >> e` for `e ≥ 0`) | ⊆ out dtype |
| IntLn | `[0, 0]` if `x.hi ≤ 0`; else `[s_lo·LN2_Q, (s_hi + 1)·LN2_Q − 1]` with `s_lo = Log2Floor(max(x.lo, 1)) − 24`, `s_hi = Log2Floor(x.hi) − 24`, widened to include 0 when `x.lo ≤ 0` | ⊆ out dtype |
| Compare | `[0, 1]` | — |
| Select | union of `a` and `b` | ⊆ out dtype |
| TopK | `[0, n − 1]` | — |
| StateWrite | `[clamp(x.lo), clamp(x.hi)]` to the state's range | — |
| HistAppend | the row's interval | — |

Soundness: MatMul and ReduceSum bound every partial sum in every order (the obligation is exactly
PALW-TIR-24's condition for every admissible input); `Div`'s quotient is monotone in each operand
for `d ≥ 1`, so its extremes are at the corners; `Log2Floor` is monotone; the transcendental bounds
are the proved output ranges of §6.5 (constant intervals for `IntExp` and `IntLn`'s series part,
because `IntExp` is not monotone across bucket edges; `IntRsqrt` uses its input's lower bound, because
its value is `y·2^(−e(v))` with the Newton value `y ∈ [1, ONE]` for every input and `e(v)`
non-decreasing — so `IntRecip(ONE + e)` in a sigmoid is provably at most `ONE`). No other obligation exists: shifts are divisions by proved-positive divisors,
indices are proved in range, sizes are capped by NF-8/§2.2.

**Stating a range the analysis cannot see.** Interval analysis loses correlations: `pos − ⌊pos/2^b⌋·2^b`
is `pos mod 2^b` but its interval reaches below 0; a softmax probability is at most `2^24`-ish because
the row maximum contributes `IntExp(0)` to the sum, but `e · IntRecip(sum)` has a much wider interval.
A lowerer states such a fact with a `Clamp` that never fires (`Clamp(pos − hi·2^b, 0, 2^b − 1)`,
`Clamp(p, 0, 2^25)`): it is part of the program and of its identity, costs one elementwise op, and
changes no value for any input, because the fact holds for every input. The library templates of §11
carry these clamps.

**Committed operands (PALW-TIR-33).** A node's proven interval is also the domain of its committed
value: when the court opens a commit point, a value outside the node's proven interval is a
malformed commitment, exactly as a value outside the dtype is. **Every committed operand is the
executor's statement** — step leaves (commit points), state checkpoint leaves (inside `[lo, hi]`),
Hist tile leaves (inside the row node's interval), carry-ins (the previous occurrence's carry-out, a
commit point) — so a violation convicts the executor, whichever leaf the challenger disputed; a
challenger cannot manufacture one, because the values are opened against the executor's roots.
Params are not committed operands: they take their dtype's full range (the registrant's, §3.5). This
is what makes the analysis sound for cones evaluated from committed values rather than recomputed
ones: after the check, no cone value can leave its interval, so no evaluation of an admitted program
can overflow.

## 8. Cost formulas (PALW-TIR-12)

Per node, at the worst case `H = W`, with `E(t)` the element count and `B(t) = E(t) · width(dtype)`:

| primitive | MACs | elementwise ops | transcendental evals | bytes read | bytes written |
| --- | --- | --- | --- | --- | --- |
| Reshape, Transpose, Slice, Concat, Broadcast, Cast | 0 | `E(out)` | 0 | `Σ B(in)` | `B(out)` |
| Iota | 0 | `E(out)` | 0 | 0 | `B(out)` |
| Gather | 0 | `E(out)` | 0 | `E(out)·width(data) + B(indices)` | `B(out)` |
| Add, Sub, Mul, Div, Clamp, Log2Floor, Compare, Select | 0 | `E(out)` | 0 | `Σ B(in)` | `B(out)` |
| MatMul | `E(out) · K` | 0 | 0 | `B(a) + B(b)` | `B(out)` |
| ReduceSum, ReduceMax | 0 | `E(x)` | 0 | `B(x)` | `B(out)` |
| IntExp, IntRsqrt, IntLn | 0 | 0 | `E(out)` | `B(x)` | `B(out)` |
| TopK | 0 | `E(x) · k` | 0 | `B(x)` | `B(out)` |
| StateWrite | 0 | `E(out)` | 0 | `B(x)` | `B(out)` |
| HistAppend | 0 | `E(out)` | 0 | `B(out)` | `B(row)` |

`E(t)` and `B(t)` are at the block's worst case `H = W` (`H = 1` in a block without a window). A
`Gather`'s `width(data)` is the gathered tensor's dtype width; `K` is `a.shape[−1]` (at `H = W`).

**Per position** (`tir_admit_v1`, `misaka_palw_tir::admit`):

- the **cost** is the sum of the node costs over the occurrences of §3.3 — `pre`, `layers[l]` for
  every `l`, `post` — each occurrence costing its block's node costs;
- the **state bytes** are `Σ` over state instances of `B(state)` for a `Fixed` state and
  `window · B(row)` for a `Hist` state, where a global state has one instance if any block uses it
  and a per-layer state one per layer `l` whose block `layers[l]` uses it (reads, writes or appends);
- the **peak live bytes** are the maximum over blocks of the largest, over node indices `t`, of
  `Σ B(out_i)` over nodes `i ≤ t` still live at `t` — node `i` is live from `i` to its last consumer
  in the block, or to the block's last node if it is a root (a commit point, a `StateWrite`, a
  `HistAppend`, a carry-out, the logits);
- the **commit lanes** are `Σ E(out)` over the commit points of every occurrence, and the **step
  leaves** `Σ ⌈E(out) / tile_len⌉` (the commit-point tiles of one position, §10.3).

The coefficients that turn the vector into time are benchmarked in Phase D; these formulas
upper-bound the reference evaluator's work.

## 9. The reference evaluator

### 9.1 A step (PALW-TIR-28)

Input: the program, the params, the run state `(pos, Fixed values, Hist rows)`, and `token`.

1. Fail (class `Position`) if `pos ≥ history_bound`. The token is **needed** when some node of the
   program reads `Input(0)`; then fail (class `Operand`) if `token ≥ token_bound`. Both checks come
   before any node is evaluated, so the class never depends on evaluation order.
2. For each occurrence in order (§3.3): evaluate every node in index order; a node's operands are
   the values of its refs (§3.2) — a missing per-layer or global param, or one whose dtype, shape
   or values do not match its declaration, fails (class `Missing`/`Operand`); a `Fixed` state's value
   must have the declared dtype and shape and lie in `[lo, hi]`.
3. The step's result is the logits node's value and the value of every commit point, with its slot.
4. **Only if every node of every occurrence succeeded**, apply the effects: each `StateWrite` value
   becomes its state instance's value, each `HistAppend` row is appended (keeping at most
   `window − 1` rows for the next position), and `pos` becomes `pos + 1`. A failed step changes
   nothing.

The initial run state is `pos = 0`, every `Fixed` instance all zeros, every `Hist` instance empty.
A run is steps at positions `0 … T−1`.

**Run-state completeness.** An implementation's run state MAY omit a `Fixed` instance that has never
been written (its value is then all zeros) and a `Hist` instance with no rows (then empty); from the
initial state the runs are identical either way. This convenience belongs to the run state only: a
cone's environment is opened from commitments, and there an absent value is never implied (§9.2).

### 9.2 Cone evaluation (PALW-TIR-30)

`eval_cone(block, layer, target, env)` evaluates one node of one occurrence from supplied values —
the court's entry point. Effects are not applied.

**The request.** `(block, layer)` MUST be an occurrence of the schedule and `target` a node of
`block`; otherwise the request is refused (class `Malformed`).

**The environment** gives: the token (or none) and `pos`; the carry-in values; the `Fixed` values
at the start of the position, by state; for each `Hist` state, its prior rows — exactly
`min(pos, window − 1)` of them, oldest first, **possibly none**; and **supplied** values for nodes of
the occurrence. Two environments are malformed and refused (class `Malformed`) before anything is
evaluated:

- one that supplies **`target` itself**: the target is always evaluated — a court that took the
  disputed value from the environment would "recompute" exactly the claim under dispute;
- one with an entry at an index that is **not a node of `block`**.

Entries for nodes of `block` that the closure does not reach are ignored, whatever their type. So
are the carry-in, `Fixed` and history entries the closure does not read, whatever their key, kind or
type: a key that names no carry-in or no state, a `Fixed` entry keyed by a `Hist` state or the
reverse, a history of any length for a `HistAppend` that is supplied rather than evaluated. Only what
the closure reads is checked (the table below).

**The closure** is the backward closure of `target` through `Node` refs that stops at every supplied
node; it is evaluated in node index order by §6. Before any node is evaluated: fail (class
`Position`) if `pos ≥ history_bound`; and if a node of the closure reads `Input(0)`, fail (class
`Missing`) if the environment has no token and (class `Operand`) if `token ≥ token_bound`. Then every
value the closure reads is checked as in §9.1(2):

| value read | absent | ill-formed |
| --- | --- | --- |
| a supplied node | — (then it is computed) | declared dtype, shape (at the running `H`) and values, else `Operand` |
| a carry-in | `Missing` | declared dtype, shape and values, else `Operand` |
| a `Fixed` value (`Ref::State`) | **`Missing` — never implied** (not zeros, not the initial value) | declared dtype, shape and values, and every value in `[lo, hi]`, else `Operand` |
| a history (a `HistAppend` in the closure) | **`Missing`, even when zero rows are needed** (`pos = 0`, or `window = 1`): the court always supplies it, possibly empty | exactly `min(pos, window − 1)` rows, else `Position`; each row's dtype, shape and values, else `Operand` |
| a param | `Missing` | declared dtype, shape and values, else `Operand` |
| a node value nobody supplied that the closure needs | `Missing` | — |

*Values* means: exactly `Π shape` elements, each a value of the declared dtype (an `i8` param holding
200, a supplied node outside `i32`, a supplied node with too few elements — all `Operand`).

Evaluating a commit point's cone with every other commit point of the occurrence supplied from an
honest step — and every carry-in, `Fixed` value and history of the occurrence supplied — reproduces
the step's committed value.

`tests/programs.rs` checks this for every commit point of every position of five programs; the
`refusals[]` of the program vectors (§12) pin every refusal above.

### 9.3 Errors (PALW-TIR-34)

Every malformed program, every out-of-range value, every overflow and every missing operand is an
error, never an abort of the implementation. Each rule reports exactly one class:

| rule | class |
| --- | --- |
| §4.4 decoding: the 262,144-byte cap, a truncated or trailing byte, an unknown tag, a `bool` not 0/1, invalid UTF-8, bytes that do not re-encode to themselves | `Encoding` |
| NF-1 … NF-15, NF-17 … NF-22 (including NF-14's arity and `Ref` existence) | `NormalForm` |
| NF-16: a type rule of §6 fails, or a declared `out` differs from the inferred type | `Shape` |
| the exact-result rule (§6.1), the order-free sum rule (§6.3) | `Overflow` |
| a `Gather` index outside its axis (§6.2) | `Index` |
| a `Div` divisor below 1 (§6.4) | `Divisor` |
| a value handed to the evaluator that is not what its declaration says: a param, carry-in, `Fixed` value (or one outside `[lo, hi]`), history row or supplied node of the wrong dtype, shape or values; a token `≥ token_bound` | `Operand` |
| a value the evaluation needs and nobody provided: a param, a token a cone reads, a carry-in, a `Fixed` value or a history in a cone, a node value | `Missing` |
| `pos ≥ history_bound`; a history with the wrong number of prior rows | `Position` |
| a cone request that names no occurrence or no node; an environment that supplies the target or an index that is no node (§9.2) | `Malformed` |

An input that breaks exactly one rule reports that rule's class (every vector breaks one); an input
that breaks several reports the class of one of them.

**Outside a program** — a primitive evaluated alone, as the primitive vectors of §12 are — there is
no normal form, and a wrong number of operands (an `Add` of one, a `Concat` of nine) is a failure of
the primitive's §6 type rule: `Shape` (vectors `error_arity_1` of `Add`/`Sub`/`Mul`, `error_arity_9`
of `Concat`). Inside a program the same defect is NF-14's, `NormalForm`: the normal form is checked
before any type.

**What a class leads to (Phase F).** No class decides a verdict — the class is a label, and the
success-versus-failure bit is what consensus reads:

- at registration, any failure of decoding, normal form or `tir_admit_v1` refuses the class
  (`TirProgram(class)`), whatever the class;
- in the court, the arm runs the evaluator only after verifying every opened unit and checking every
  committed operand against its proven interval (PALW-TIR-33, which convicts the executor BEFORE any
  evaluation). After that, `Missing` and `Malformed` are the only classes honest bytes can meet — the
  evidence does not serve the cone — and the close is refused (`InputSetNotCanonical`, nobody
  slashed); `Overflow`, `Index`, `Divisor`, `Operand` and `Position` are unreachable for an admitted
  program, and meeting one is an interpreter defect that also refuses the close (`Unadjudicable`,
  nobody slashed). Neither refusal convicts or acquits anyone.

### 9.4 Demand evaluation (PALW-TIR-30)

`eval_demanded(target, elements, source, limits)` computes chosen elements of one tensor of a run
from exactly the elements they read — the court's evaluator (Phase F, `palw_tir_court_v1`). Its
values are §6's: every element is defined independently of every other (§6.1), so each value is the
element `eval_cone` computes from the same values. A verdict depends on three more things, which this
section fixes as well: **which values the evaluation reads** (its requests — the operand set a
refutation must carry), **what it costs** (its work) and **when it refuses**. An implementation that
follows this section reproduces all three for every request; the golden vectors below pin them.

**Contexts.** A node runs in a *context* `(p, o)`: a position `p` and an occurrence `o` — an index
into the occurrence list of §3.3 (`0` is `pre`, `1 + l` is layer `l`, `L + 1` is `post`). In a
context, `H = min(p + 1, W)` for the block's window `W` (`H = 1` in a block that appends to no
history), and every tensor has its shape at that `H`. The context's *layer* is `o − 1` for a layer
occurrence and none otherwise; a per-layer param or state is read at the context's layer, a global
one as its single instance.

**The request** is a *target*, a list of element indices (row-major at the target's `H`; the values
are returned in list order; repeats are allowed and cost nothing) and *limits*
`(max_elements, max_terms)`. The target is either

- `node(p, o, n)` — node `n` of context `(p, o)`; or
- `state_after(p, j, l)` — `Fixed` state `j`'s instance at layer `l` (none for a global state) after
  position `p`, which is what a checkpoint at `p` holds.

The request is refused before anything is read: class `Position` if `p ≥ history_bound`; class
`Malformed` if `o` names no occurrence, `n` no node of its block, an element is not below the
target's element count, or `state_after` names a state that is not `Fixed` or an instance no run
holds — a layer for a global state; none, or a layer `≥ L`, for a per-layer one; or a layer whose
scheduled block neither reads, writes nor appends to the state (a run holds a per-layer instance only
where its block references it, and a global state's one instance always: normal form uses every
declared state).

**The source** answers five questions, and nothing else is ever read:

| question | answer |
| --- | --- |
| `node(p, o, n, i)` | element `i` of committed node `n` of context `(p, o)` |
| `param(j, l, i)` | element `i` of param `j`'s instance at `l` |
| `state(p, j, l, i)` | element `i` of `Fixed` instance `(j, l)` at the START of `p` — a value, or `Replay` |
| `hist_row(p, j, l, r, i)` | element `i` of the row appended to `Hist` instance `(j, l)` at position `r`, as read at `p` (`r < p`) |
| `token(p)` | the token of position `p` |

Any answer may be a refusal; the evaluation then fails with class `Missing` — whatever reason the
source gives: a source's own class is never read, so two sources refusing one question for different
reasons fail the evaluation alike — and never substitutes a value. The questions asked, with their arguments, are the evaluation's **requests**. They are a
**set**: an implementation may ask a question twice, and the order it asks in is not part of the
result.

**Leaves.** A node `n` of a context `c` is a *leaf* iff it is a commit point — except the target of a
`node` request, in the target's context, which is always computed (a court that read the disputed
value would "recompute" the claim under dispute). A leaf's element `i` is `source.node(c, n, i)`,
which must be a value of the node's dtype (else `Operand`). Every other element is computed by §6
from the operand elements its index map names (below). Each element of each context — leaf or
computed — is evaluated at most once per request.

**Operands.** An operand element of a node of context `(p, o)` is:

- `Node(m)`, element `i`: element `i` of node `m` of `(p, o)` — a leaf or computed, as above;
- `CarryIn(k)`, element `i`: element `i` of node `carry_out[k]` of the previous occurrence's block, in
  context `(p, o − 1)` — a commit point (NF-21), hence a leaf — which must be a value of the carry-in's
  declared dtype (else `Operand`);
- `Param(j)`, element `i`: `source.param(j, l, i)` with `l` the context's layer for a per-layer param
  and none otherwise; a value of the param's dtype (else `Operand`);
- `Const(j)`, element `i`: the declared data;
- `State(j)`, element `i`: the instance's value at the start of `p` (Fixed-state replay, below);
- `Input(0)`: `source.token(p)`, which must be `< token_bound` (else `Operand`); `Input(1)`: `p`.

**Index maps.** For an output element with multi-index `o` over the output's shape at the context's
`H` (flat index `e`), the operand elements read are, in this order:

| primitive | operand elements read |
| --- | --- |
| `Reshape`, `Cast`, `Clamp`, `Log2Floor`, `IntExp`, `IntRsqrt`, `IntLn`, `StateWrite` | `x` at the same flat index `e` |
| `Transpose perm` | `x[j]` with `j[perm[k]] = o[k]` for every `k` |
| `Slice axis, start` | `x[o′]`, `o′` = `o` with `o′[axis] = o[axis] + start` |
| `Concat axis` | input `q`, the first whose cumulative extent along `axis` exceeds `o[axis]`, at `o` with `o[axis]` reduced by the extents of the inputs before `q` |
| `Broadcast` | `x[bc(o)]` |
| `Iota` | none |
| `Add`, `Sub`, `Mul`, `Div`, `Compare` | `a[bc(o)]`, then `b[bc(o)]` |
| `Select` | `c[bc(o)]`; then ONLY the chosen operand: `a[bc(o)]` if `c ≠ 0`, else `b[bc(o)]` |
| `Gather A, B` | `v = indices[o[0..B] ++ o[A..A+m]]` (`m = rank(indices) − B`); then, unless §6.2's `Index` check fails, `data[o[0..A] ++ [v] ++ o[A+m..]]` |
| `MatMul` | for `t = 0 … K − 1`: `a[β_a, r, t]` and `b[β_b, t, c]` (`K = a.shape[−1]`) |
| `ReduceSum axis`, `ReduceMax axis` | `x[o′]` for `o′` = `o` with `o′[axis] = t`, `t = 0 … n − 1` |
| `TopK axis, k` | the whole row: `x[o′]` for `o′[axis] = t`, `t = 0 … n − 1`; the row's `k` selected indices are every slot of the row at once |
| `HistAppend state` | with `R` = the row's element count, row `t = e div R`, lane `w = e mod R`: if `t = H − 1`, the input's element `w` in this context; otherwise `source.hist_row(p, j, l, p + 1 − H + t, w)` (`j` the state, `l` the context's layer for a per-layer history), a value of the state's dtype (else `Operand`) |

`bc(o)` maps `o` into an operand by broadcasting (§2.3: dimensions aligned on the right; an operand
dimension of extent 1 is read at 0). For `MatMul`, `(r, c)` are `o`'s last two indices and `β` its
batch prefix, mapped into each operand by broadcasting. `n` is the reduced extent at the context's
`H`.

**Fixed-state replay.** The value of instance `(j, l)` at the start of `p`, element `i`, is
`source.state(p, j, l, i)`:

- **a value** — which must be of the state's dtype and in `[lo, hi]` (else `Operand`);
- **`Replay`** — then: if `p = 0`, the evaluation fails (class `Missing`: nothing precedes position 0;
  a source supplies the initial zero at position 0 itself). Otherwise the value is the instance's
  **writer**'s output at `p − 1`: the `StateWrite` of state `j` in the block of occurrence `1 + l`
  (per-layer) or `0` (global) — unique by NF-19 — its element `i` in context `(p − 1, that occurrence)`,
  a leaf if the writer is a commit point and computed otherwise, and in `[lo, hi]` (else `Operand`). If
  nothing writes the instance, its value is carried unchanged: the value at the start of `p − 1`, asked
  of the source in turn.

A `state_after(p, j, l)` target's element `i` is the writer's element `i` in context
`(p, the writer's occurrence)` — a leaf if the writer is committed — which must lie in `[lo, hi]`
(else `Operand`), as every `Fixed` value the evaluation reads does (a computed write is clamped into
it; a committed one is checked); or, for an instance nothing writes, the value at the start of `p`.

**Work** is two counts:

- `elements`: one per computed (non-leaf) element — each counted once, however often it is read; a
  `TopK` counts once per ROW, since one evaluation determines every slot of the row;
- `terms`: for each computed element, `K` for a `MatMul`, the reduced extent `n` (at the context's
  `H`) for a `ReduceSum`, a `ReduceMax` or a `TopK` row, and 0 otherwise; plus, for each distinct
  `(p, j, l, i)` whose value at the start of `p` the evaluation needs for an instance nothing writes
  (a `State` operand in a context at `p`, or an element of `state_after(p, j, l)`), one term for each
  position the walk passes: `p − q` for the largest `q ≤ p` at which the source answers a value, and,
  when it answers `Replay` all the way down, `p` — the walk then fails at position 0 (`Missing`), and
  a `max_terms` below `p` makes it `WorkLimit` first.

Leaves, params, consts, tokens, history rows and supplied state values cost nothing.

**The boundary.** The evaluation **succeeds** iff every element it evaluates succeeds (§6, and every
check above, every source question answered) and its work is within the limits:
`elements ≤ max_elements` and `terms ≤ max_terms`. Otherwise it **fails**, with the class of a failing
element or `WorkLimit`. In the court both are refusals (§9.3), and as everywhere the class is a label:
success-versus-failure is what consensus reads. A successful evaluation's values, work and request set
are functions of the request and the source's answers alone — not of the order an implementation
evaluates in — because every element and every answer is.

**Charging.** An implementation MUST stop, with `WorkLimit`, no later than when a count first
exceeds its limit, and SHOULD charge an element before it reads the element's operands, so that no
request makes a verifier spend more than the limits. The reference evaluator
(`misaka_palw_tir::demand`) charges each element when it first scans it, before any operand is
fetched, and evaluates with an explicit stack of frames `(context, node, element)` rather than native
recursion (a 512-node chain, or a replay across thousands of positions, costs no native stack). It
caps the frames it pushes at `6 · max_terms + 24 · max_elements`; one scan of an element pushes at
most `2 · terms + 2` frames, so the cap is never reached while the work is within the limits and it
is not part of the boundary.

`tests/demand.rs` checks the values against `eval_cone` for every node of every position of every
program vector, the replay against the run, and the order-independence of the work.

**Golden vectors** (`demand/<program>.json`, `format = palw-tir-v1/demand-vectors/1`). One file per
program vector:

- `program` names the program vector whose `params` and `steps` are the source's committed data:
  `node(p, o, n, i)` is element `i` of the commit of `steps[p]` whose `(block, layer)` is occurrence
  `o`'s and whose `node` is `n`; `token(p)` is `steps[p].token`; `hist_row(p, j, l, r, i)` is element
  `i` of the committed node the row is — the `HistAppend`'s input node in context `(r, 1 + l)` (or
  `(r, 0)` for a global history), or, when the input is `CarryIn(k)`, node `carry_out[k]` of the
  previous occurrence at `r`; `param(j, l, i)` is from `params`.
- `states` lists the `Fixed` values the source supplies — `pos`, `state`, `layer` (or null) and
  `value` — here every instance a block references, at every even position (the initial zeros at 0
  included); every other `(p, j, l)` answers `Replay`. A case may carry its own `states`, which then
  replaces the file's.
- A question the files do not answer is refused (`Missing`). A case may list `withhold`: questions
  (in the form of `requests`, without `indices`) the source refuses, each with the reason it gives
  (`raises`, a class that is not `Missing`); the evaluation fails `Missing` all the same.
- `cases[]`: `name`, `target` (`{"node": {pos, occurrence, node}}` or
  `{"state_after": {pos, state, layer}}`), `elements`, `limits` (`max_elements`, `max_terms`), and
  optionally `withhold`, and either `expect` — `values` (in `elements`' order), `work` (`elements`,
  `terms`) and `requests`, the
  request set grouped by question: one entry per question with every argument but the element index
  (`{"node": {pos, occurrence, node}}`, `{"param": {param, layer}}`,
  `{"state": {pos, state, layer}}`, `{"hist_row": {pos, state, layer, row_pos}}` or
  `{"token": {pos}}`) and `indices`, the indices asked as inclusive runs (`"0-3,7"`; absent for a
  token), sorted by question in that order and then by argument (a null layer first) — or
  `expect_error` (a class of §9.3, or `WorkLimit`).
- The cases: every commit point of every position (the first, second, middle and last two
  elements, or all of them when there are at most six), a sample of uncommitted nodes at the last
  position, every referenced `Fixed` instance after every position (the odd positions replay from the
  even one before), the first case with the most terms at exactly its work and one short in each
  count, and the refusals — an element outside the target, an occurrence or a node that does not
  exist, `p = history_bound`, a position the run did not reach, `Replay` at position 0, a `Hist` state
  named by `state_after`, and, for each kind of question some case asks, the first such question
  withheld by the source with a reason that is not `Missing`.

`cargo test -p misaka-palw-tir --test demand_vectors` regenerates every file from the program vectors
and requires identical bytes; `TIR_BLESS=1` rewrites them, which is a change of the semantics and is
reviewed as one.

### 9.5 H dissection (PALW-TIR-37)

A commit point whose cone reduces over the history costs `O(H)` to recompute, and at a long context
one tile of it does not fit a close. The court then **dissects** the history: the responder states
the totals of every reduction over `H` in the tile's cone, the history is cut into ranges, the
responder states every reduction's partial over each range, and the challenger follows a range that
it disputes down to one `h_tile` of positions, which the court evaluates. This section defines the
arithmetic of that exchange — which reductions, which of their elements, what "evaluated over a
range" means, how partials fold, what the bottom compares — so that two implementations reach the
same verdict on every move. It extends §9.4: everything not stated here is §9.4's.

The protocol that carries the exchange on the chain — its objects, messages, clocks and charges — is
ADR-0082's with the attention triple replaced by "every reduction over `H`"; §9.5.9 states the parts an
independent implementation must reproduce byte for byte, and the rest is Phase F's.

#### 9.5.1 Reductions over `H` and dissected leaves

A node `r` of block `b` **reduces over `H`** iff

- it is a `ReduceSum` or a `ReduceMax` whose operand's declared shape has `H` at its `axis`; or
- it is a `MatMul` whose first operand's declared shape has `H` as its last dimension (the
  contraction axis).

The operand is `Node(j)` or `CarryIn(k)` (its declared `out`, resp. the carry-in's declared type); a
reduction whose operand is any other `Ref` does not reduce over `H` (a param, const or state has no
`H`).

Let commit point `n` of block `b` have a committed tile in context `(p, o)` (§10.3, *Committed tiles*).
Its **cone** is §10.2's: the nodes of `b` reachable backwards from `n` through `Node` refs without
passing through another commit point (the walk stops AT a commit point other than `n` and does not
enter it). Its **reductions** `r_1 < r_2 < … < r_m` are the nodes of the cone that reduce over `H`, in
node index order — `n` itself included if it reduces over `H`; another commit point never is (it is
a leaf of the cone). The tile is **dissected** iff `m ≥ 1`. A state checkpoint leaf and a Hist tile
leaf are never dissected (they are opened values).

The **site** of a dissected tile is, per reduction `r_i`:

- its **fold**: `Max` for a `ReduceMax`, `Sum` for a `ReduceSum` or a `MatMul`;
- its **bound**: the interval §7 proves for `r_i` (a partial over any sub-range of the history lies
  in it: an exact partial sum's positive and negative parts are sub-sums of the total's, and a
  partial maximum is one of the maximised values);
- its **element count** `E_i`: the element count of `r_i`'s output at the context's `H`;

and, for the whole site, `H = min(p + 1, W)` of block `b` (§9.4, *Contexts*) and the class's
`h_tile` (positions per history tile of its commitment layout, `≥ 1`). The site is a function of the
program, the layout and the tile's coordinate; nothing in it is supplied by a mover. A site with
`m > 16` is refused (`PALW_TIR_DISSECT_MAX_REDUCTIONS`); admission refuses a program that has one
(§9.5.6).

#### 9.5.2 Supplied nodes and range evaluation

`eval_range(ctx, target, elements, supplied, range)` is §9.4's `eval_demanded` of target
`node(p, o, target)` with two changes:

1. **Supplied nodes.** `supplied` is a set of node indices of the target's block, none equal to
   `target`. In the target's context `(p, o)` — and only there — every node of `supplied` is a *leaf*
   exactly as a commit point is: its element `i` is `source.node(p, o, n, i)`, which must be a value
   of the node's dtype (else `Operand`); it is never computed, costs nothing and its operands are
   never read. (A supplied node that the evaluation never reaches is never asked for.)
2. **The range.** With `range = (from, to)`, the target MUST reduce over `H` (§9.5.1) and
   `0 ≤ from < to ≤ H` must hold (else the request is refused, class `Malformed`, before anything is
   read). The target then reduces over the history indices `t ∈ [from, to)` only:
   - `MatMul`: for `t = from … to − 1`, `a[β_a, r, t]` and `b[β_b, t, c]` are read (in that order,
     `t` ascending) and summed exactly under the order-free rule of §6.3 (PALW-TIR-24), in the
     node's declared dtype;
   - `ReduceSum axis`: `x[o′]` with `o′[axis] = t` for `t ∈ [from, to)`, summed exactly under the same
     rule;
   - `ReduceMax axis`: the maximum of the same `x[o′]`.
   Its work is `to − from` terms per computed element of the target (instead of `H`). Every other
   node — including every other reduction over `H` that the evaluation computes, which is not
   possible in a dissection (they are supplied), but is defined — reduces over its whole axis.

A request whose `range` is absent is `eval_demanded` with supplied nodes. The requests, the work
and the refusals are §9.4's, with the supplied nodes' elements among the `node` requests (they ARE
`source.node` questions: a source answers them from the dissection's claimed values).

#### 9.5.3 The root claim: the finalize and the element closure

A **range claim** is, for each reduction `r_i` of a site, one integer per element of an element
list `L_i`: `v_i[e]` for `e ∈ L_i`. A **root claim** is `(L, T)`: the element lists and the
**totals** `T_i[e]`, the responder's statement of `r_i`'s element `e` over the whole history
`[0, H)`. Integers are unbounded in this definition (a wire form carries them as `i128`); each is
checked against its bound.

**The finalize.** The tile's elements (§10.3: the `tile_len`-value run of `n`'s value at `H`,
row-major, the last one ragged) are evaluated by `eval_range(ctx, n, tile elements, S, none)` with
`S = {r_1, …, r_m} \ {n}`, every supplied value answered from `T` (`source.node(p, o, r_i, e) =
T_i[e]` for `e ∈ L_i`; any other element of a supplied node is refused, so the evaluation fails
`Missing`). If `n` itself reduces over `H` (`n = r_m`), the finalize of element `e` is `T_m[e]`
itself: `L_m` must contain every tile element and nothing is evaluated for them.

**The element closure** `C = (C_1, …, C_m)` of a tile is the least family of element sets such that

- every element of a supplied reduction that the finalize reads is in its set (for `n = r_m`, `C_m`
  contains the tile's elements); and
- for every `i` and every `e ∈ C_i`, every element of a reduction `r_j ≠ r_i` read by the **probe**
  `eval_range(ctx, r_i, [e], S_i, (0, 1))` with `S_i = {r_1, …, r_m} \ {r_i}` — `r_i`'s element `e`
  over the one history index `t = 0`, every OTHER reduction of the site supplied — is in `C_j`.

It is computed by iterating the second rule to a fixpoint (each set only grows and is bounded by
`E_j`). Why one index suffices: an `H`-local node's term at history index `t` reads another
reduction's output as an `H`-free operand (§10.3, *Dissectability is structural*), whose element is
fixed by the output index alone — the same element at every `t` — provided no read of a reduction's
output inside the `H`-local region is data-dependent (§9.5.6, obligation O-2). The probe needs `H ≥ 1`,
which always holds.

**Admitting a root claim.** The claim rides with the tile's **finalize carriage** — the form of a
cone close (§10.4): the binding, the tile's opening and preimage, and exactly the evidence the
finalize and the probes read (every other reduction's value comes from `T`, never from evidence). Its
checks are a cone close's, up to the evaluation: the binding speaks about the committed execution,
the tile opens under it, and every carried leaf lies in its proven interval — but a carriage whose
own checks CONVICT (a binding fault, a malformed leaf, a committed value outside its interval,
PALW-TIR-33) is not a root claim: that tile is closed by a cone close, not dissected. Then, and each
failure refuses the claim — a refused MOVE, never a verdict; the order decides only which refusal
is reported:

1. the tile is dissected and is the one the dispute narrowed to;
2. shape: `m` lists and `m` value lists; each `L_i` strictly ascending with every element `< E_i`;
   `|v_i| = |L_i|`; `Σ |L_i| ≤ 4096` (`PALW_TIR_DISSECT_MAX_VALUES`);
3. every `T_i[e]` lies in `r_i`'s bound;
4. the finalize, with `T` supplied, **reproduces the committed tile** value for value, reading
   exactly the evidence the claim carries;
5. the element closure computed with `T` supplied is **exactly `L`**: no claimed element goes
   unread, and no read element is unclaimed (an unclaimed read is refused by the source, so the
   evaluation fails).

Steps 4 and 5 read only the tile's cone at one history index per reduction element: their work is
bounded by the tile's box demand at `H = 1` plus the finalize (§10.3), never by `H`.

#### 9.5.4 Rounds: the cut, the partials and the fold

The history is `T_h = ⌈H / h_tile⌉` **history tiles**, tile `τ` covering positions
`[τ · h_tile, min((τ + 1) · h_tile, H))`. A dispute is over a range of tiles `(first, count)`,
initially `(0, T_h)`, with a claim — initially the root's totals `T`.

**The cut** at arity `k` (a power of two, `2 ≤ k ≤ 64`) of a range `(first, count)` with `count ≥ 2`
is `w = ⌈count / k⌉` and the children `(first + s, min(w, count − s))` for `s = 0, w, 2w, …` while
`s < count` — between 2 and `k` children, in order, the last possibly shorter. A range of one tile
has no cut: it is the **bottom**. The number of rounds to reach the bottom from `T_h` tiles is the
number of times `count ← ⌈count / k⌉` is applied before `count ≤ 1`.

**A round.** For each child `(f, c)` the responder states a range claim over positions
`[f · h_tile, min((f + c) · h_tile, H))`: for each `r_i` and each `e ∈ L_i` (the root's lists, at
every level) the partial `P_i[e] = eval_range(ctx, r_i, [e], S_i, range)` with
`S_i = {r_1, …, r_m} \ {r_i}` supplied **from the root's totals `T`** — never from the parent's or a
child's claim. A round is admitted iff it has one claim per child, each of the root's shape, every
value inside its reduction's bound, and the children **fold** to the claim of the range under
dispute, for every `(i, e)`:

- `Sum`: `Σ_child P_i[e] = claim_i[e]` exactly (unbounded integers);
- `Max`: `max_child P_i[e] = claim_i[e]`.

A round that does not fold is the responder's self-contradiction; it is refused as a move, and the
responder's turn runs on (its silence then loses). The challenger then names one child — by its index
in the cut, at the dispute's round — and the named child's range and claim become the dispute's; the
round counter advances. A dispute opened on `T_h ≤ 1` tiles is at the bottom at once, and no dispute
takes more rounds than the cut's own recurrence from `T_h` (a choice past it is refused).

#### 9.5.5 The bottom

At the bottom — one tile `τ`, positions `[from, to)`, and the claim `claim` the dispute narrowed
to — the court evaluates, for each `i` in order and each `e ∈ L_i` in order,
`eval_range(ctx, r_i, [e], S_i, (from, to))` with `S_i` supplied from the ROOT's totals `T`, and
compares the list of values (flattened, `i` major, then `e`) with `claim` flattened alike:

- the first index at which they differ convicts the executor (`ComputationMismatch { value_index }`
  on the dissected leaf);
- none: no fault is found and the challenger is defeated.

The bottom rides with its own carriage in the cone close's form, the tile being the dissected one:
its checks are a cone close's, and there its convictions STAND (a carried leaf outside its proven
interval convicts, PALW-TIR-33, before anything is evaluated). The evaluation reads exactly the
evidence the bottom carries — the tile's history rows, the `H`-free leaves of the `H`-local region,
and nothing past the range — and a carriage with anything more or less is refused.

#### 9.5.6 Admission obligations and bounds

A cone of a registered class is dissected only where the network's court can play the exchange (the
k-ary court, Phase F); without it a cone that reduces over `H` is adjudicated whole, and its tile at
`H = W` must fit like any other (§10.3). Where it is dissected, admission refuses the class, naming
the commit point, unless every dissected cone meets:

- **O-1 (count).** At most 16 reductions over `H` in the cone.
- **O-2 (no data-dependent read of a reduction).** No computed `Gather` of the cone whose indices
  carry `H` has a data operand that depends (through computed nodes of the cone) on a reduction over
  `H`, and no computed `Select` whose condition carries `H` has a value operand (`a` or `b`) that
  does — the two reads whose element is chosen by a history-varying VALUE rather than by the output
  index (a `Select` reads its condition, then only the chosen operand, §9.4). A value operand the
  condition reads itself at the same element is exempt: the condition a `Compare` of the `Select`'s
  shape with that operand, of the same shape, as a direct input — a shifted softmax's
  clamp-by-select `select(x − m < floor, floor, x − m)` reads `m` through its condition whichever it
  chooses. An `H`-free read is the same at every history index, so only these matter; with O-2 the
  one-index probe of §9.5.3 names every element any history index reads, and the element closure is
  exact.
- **O-3 (`H`-free totals).** Every reduction over `H` of the cone has an `H`-free output.
- **O-4 (the bottom fits).** The cone's terminal is one history tile — the box demand of the tile at
  `H = min(h_tile, W)` (§10.3, *Dissection*) — and its work, its opened bytes and its
  multiply-accumulates fit the court's ceilings, as every terminal does; its close, as carried, is
  carriable (PALW-TIR-38).
- **O-5 (the exchange fits).** The claim's value count is bounded by `V`, the box demand of §10.3 at
  `H = 1` arriving at the cone's reductions: from the tile's `tile_len` elements (capped at the
  node's count), each computed node in descending index order passes `d · K` to each operand of a
  `MatMul` (`K` its first operand's last extent), `d · x.shape[axis]` to a reduction's, `⌈d / k⌉ ·
  x.shape[axis]` to a `TopK`'s and `d` to every other operand, each operand's demand capped at its
  element count; `V` is the sum over the reductions of what arrives (capped likewise). Then `V ≤
  4096`; a round at the court's arity `k` — `6 + k · (4 + 4m + 16V)` bytes with `m` reductions, plus
  the move's frame of 4,764 bytes — fits one lifecycle carrier (100,000 bytes); so does the root
  claim — the 16 KiB close frame, the terminal's opened bytes, `20V` and the frame (the program is
  referenced by the class, never carried); and
  the whole exchange over `⌈max_context / h_tile⌉` tiles at arity `k` fits the court window, by the
  rule that sizes the network's arity (ADR-0082 Z4: its moves at one rung window each, plus the
  close's assembly reserve, strictly inside the window; under the held regime without the ladder's
  rounds).

#### 9.5.7 Why a lie is always convictable (informative)

Take the honest run's values `h_i[e]` of every reduction over `[0, H)`. The reductions are in node
order and refs are strictly backward, so `r_i`'s cone reads only reductions `r_j` with `j < i`. If a
root claim `T` differs from `h` somewhere, let `i` be the first reduction with `T_i ≠ h_i`: every
`r_j`, `j < i`, is supplied at its honest value, so `r_i` evaluated with the others supplied from `T`
is `h_i`, and `T_i` is a false statement about an evaluation the court can repeat. At every round the
children of a false claim contain a false child: for `Sum`, honest children sum to the honest value,
not to the claim; for `Max`, a claim above the honest maximum needs a child that claims it, and one
below it leaves the child that holds the maximum under-claimed. Following false children reaches a
bottom whose evaluation differs from the claim. Conversely, an honest responder's every claim is an
evaluation, so every bottom matches and the challenger is defeated. A committed tile that is not the
honest one finalizes only from a `T ≠ h` (the finalize is a function of `T`), so a forged tile is
always convictable; a false `T` that happens to finalize to the honest tile (a softmax is
shift-invariant in its maximum) is convicted too — it is still a false claim.

#### 9.5.8 Golden vectors

`dissect/<program>.json` (`format = palw-tir-v1/dissect-vectors/1`), one per corpus model with a
dissected tile (today the dense GQA and the sliding + global models). `job` is the run the cases are
cut from — the program vector's model on the prompt `prompt` (`prefill` tokens) with `generated` fed
back (`decode` tokens), history tiles of `h_tile` positions. Each case is a dissected tile of the
last two positions: `leaf` (its step-leaf `index`, `pos`, `occurrence`, `node`, `first_element` and
`values`, the tile's length), `site` (`reductions`, `folds` = `sum`/`max`, `bounds` = the proven
`{lo, hi}` of each, `h`, `h_tile`, `counts` = each `E_i`), the element closure `elements` (the
lists `L_i`), the honest `totals`, `finalize` (the tile the totals finalize to — the committed one),
`cut` (the first cut at arity 2: each child's `tiles` = `[first, count]`, its `positions` = `[from,
to)` and its `partials`, every other reduction supplied from the totals), and `bottom` (the last
tile, reached by naming the last child at every round: its `positions` and the partials the court
evaluates there). `cargo test -p kaspa-consensus-core --test palw_tir_dissect_vectors` regenerates
every file and requires identical bytes, checking each case against the court as it goes (the root
claim is admitted, every round folds, the bottom finds no fault); `TIR_BLESS=1` rewrites them, which
is a change of the semantics and is reviewed as one.

#### 9.5.9 The carriage on the chain (Phase F)

The exchange rides as ADR-0082's does, as three lifecycle objects and one close proof, appended after
Phase F's IR objects (each refused below `palw_tir_v1` and, for the moves, where the k-ary court is
not armed):

- `CourtTirRootClaimed { session_id, root, arity, signature }` (object tag 64): the root claim
  `{ version = 1, elements, totals, finalize }` (`elements: Vec<Vec<u32>>`, `totals` a range claim
  `{ partials: Vec<Vec<i128>> }`, `finalize` the carriage above); `arity` MUST be the ruleset's
  derived dissection arity. Like every IR binding on the chain, the carriage's binding carries its
  class with the program EMPTY: the chain puts back the registered class's program before reading
  it, and refuses a carried one;
- `CourtTirDissected { session_id, round, signature }` (65): `round = { version = 1, children }`,
  one range claim per child of the cut, in order;
- `CourtTirChildChosen { session_id, choice, signature }` (66):
  `choice = { version = 1, session_id, round: u32, child: u8 }`;
- the close proof `TirDissection { bottom }` (proof tag 9), graded against the dispute's phase;
  `ComputationMismatch` is `ExecutorGuilty`, no fault `ChallengerDefeated`.

The executor signs the root claim and every round, the challenger every choice, with ML-DSA-87 under
ADR-0082's responder and challenger contexts, over messages that open with their own domains (so no
signature over another court's move is one over these):

- root claim: `"misaka-palw/tir/dissect/root/v1" ‖ session_id ‖ borsh(version, elements, totals)`;
- round: `"misaka-palw/tir/dissect/round/v1" ‖ session_id ‖ le32(the dispute's round) ‖ borsh(round)`;
- choice: `"misaka-palw/tir/dissect/choice/v1" ‖ borsh(choice)`.

The clock is ADR-0082's. At the dissected leaf the executor owes the root claim (or a close that
acquits it); within the dispute it owes each round, and the challenger each choice, within one rung
window of the previous move; silence past its window loses the dispute for the silent party (the
executor's at the root claim under ADR-0082's mercies for an opening nobody could have answered,
unchanged). At the bottom nobody is clocked: the challenger files the bottom (or the executor an
acquitting one), and a dispute nobody closes ends at the session's backstop on the challenger's side.
Under the held regime, where no bisection is played, a one-move accusation whose cone close names a
dissected tile (and carries nothing else) opens the dispute at that tile, the executor's root claim
its first move.

## 10. Commitment and the court

### 10.1 Commit points (PALW-TIR-14)

Required commit points, enforced by normal form:

- every block's carry-out nodes (NF-21) and the logits (NF-6);
- every `TopK` (NF-18) — a selection is an opened value, never a bisection target (ADR-0052 B);
- the input row of every `HistAppend` (NF-20), so later positions' cones can open it.

**State — one step tree (Phase F D7).** An IR class has ONE step tree and no separate checkpoint
leg. A `Fixed` state is committed at checkpoint positions — every `C` positions of the class's
declared commitment layout, `C ≤ C_j` for every state `j` (§10.3) — as ordinary **step leaves**
(state checkpoint leaves, tiled like any row), not at every position; a `Hist` state's rows are the
per-position commit points of NF-20 and are also committed, every `h_tile` positions, as **Hist tile
leaves** of the same tree. All leaves are in position order, so the ladder's first divergent leaf is
always adjudicable from leaves that precede it. Between checkpoints the court replays the state's
update cone from the per-position commit points it reads. For that replay to be possible, every leaf
of a `StateWrite`'s cone other than `State`, `Param`, `Const` and `Input` refs MUST be a commit point
or a carry-in — which holds by construction, because a cone stops at commit points and carry-ins are
commit points. Lowerers add further commit points (e.g. the conv row, the gates) until §10.3 passes.

Committed values are 4-byte lanes (`i8`/`i16` sign-extended, `idx` unsigned); `i64`/`i128` never
are (PALW-TIR-5).

### 10.2 Cones (PALW-TIR-13)

The cone of commit point `n` in occurrence `o` is the set of nodes of `o` reachable backwards from
`n` through `Node` refs without passing through another commit point, together with its *leaves*:
the other commit points it reaches (opened values), params (opened against the artifact root),
consts, `State` values (from a state checkpoint leaf, advanced by replay), carry-ins (the previous
occurrence's committed carry-out), inputs, and — for a `HistAppend` in the cone — the history's prior rows (each
the committed row of an earlier position). The court opens the leaves and runs §9.2.

### 10.3 Court feasibility — `tir_admit_v1` (PALW-TIR-12, PALW-TIR-13)

Admission (`misaka_palw_tir::admit::tir_admit_v1`) is a pure function of the program's canonical
bytes and three network inputs: `tile_len` (values per step leaf, `1 ≤ tile_len ≤ 2^16`), `h_chunk`
(positions per canonical `H` chunk, a power of two in `[1, 2^16]`), and the **ceilings** (the terminal
tile's MACs, transcendentals, opened bytes and committed operands; the position's MACs and
transcendentals; the state bytes; the step leaves per position; the longest checkpoint interval;
the cone work). Inputs out of range are refused. In order, it:

1. decodes the bytes (§4.4) and checks the normal form (§5) and the types (§6) — class of §9.3;
2. analyses the ranges (§7) — every node's interval is also the domain of its committed values
   (PALW-TIR-33);
3. computes the §8 cost of every node and the per-position quantities of §8, and refuses a position
   cost, state bytes or step leaves past their ceilings;
4. derives every commit point's cone and its terminal cost (below), and refuses one past a ceiling;
5. derives every `Fixed` state's checkpoint interval (below), and refuses a state one position of
   whose replay is past the terminal ceiling; and, across steps 4 and 5, refuses a program whose
   cone work (below) passes `max_cone_work`;
6. returns the intervals, the node costs, the per-position quantities, the cones and the intervals
   `C_j` with `C = min_j C_j` (the ceiling's cap if the program has no `Fixed` state).

A refusal past a ceiling names the limit, where, the value and the cap; which of several broken
ceilings a refusal names is not normative (only admission's success is).

**Admission's own work** has a ceiling of its own. Decoding, the normal form, the ranges and the
per-position quantities are linear in the program's bytes and its schedule; what is not is the
**cone work** — `Σ`, over every commit point's cone (§10.2) and every `StateWrite`'s update cone
(below), of the cone's node count plus the number of its nodes' operand refs. A program whose cone
work passes `max_cone_work` is refused; since the sum only grows, an implementation stops counting —
and costing — at the first cone that passes it, so admission never does more than the ceiling's work.
The normal form's caps alone allow about 2.7 M (every one of ~250 commit points of a 512-node block
reaching a 240-node chain, in 14 layer blocks); a Qwen2.5-1.5B-shaped program
(28 layers, `d = 1536`, vocabulary 151,936, window `2^18`) has 905. The reference implementation
(release build, Apple M1 Max) admits that program in about 0.4 ms, each corpus program in under
1 ms, and refuses the caps' worst case at the starting ceiling `2^20` in about 50 ms (all of it,
uncapped, takes about 110 ms).

**Committed tiles.** A commit point's value, flattened row-major at the running `H`, is committed as
step leaves of `tile_len` values (the last ragged): `⌈E(out) / tile_len⌉` tiles.

**A tile's cost: box demand.** The cone of commit point `n` is §10.2's. The court recomputes one tile
of `n`, so it evaluates only the elements that tile demands. Admission bounds that work by counts:
`d(n) = min(tile_len, E(n))`; then, for the cone's nodes in DESCENDING index order, each node `i`
with `d(i) > 0` passes to each operand `x` the demand

| node `i` | demand on operand `x` (always capped at `E(x)`) |
| --- | --- |
| `MatMul` | `d(i) · K` (for `a` and for `b`) |
| `ReduceSum`, `ReduceMax` along axis `ax` | `d(i) · x.shape[ax]` |
| `TopK` along `ax`, `k` | `⌈d(i) / k⌉ · x.shape[ax]` |
| every other primitive (including `Gather`'s data and indices, `HistAppend`'s row) | `d(i)` |

A demand on a node of the cone adds to its `d` (capped at its `E`); a demand on anything else is a
**leaf demand** on that leaf (another commit point, a carry-in, a `Fixed` state, a param, a const, an
input), summed and capped at the leaf's element count. A `HistAppend` with `d > 0` also puts a demand
of `min(d, E(out) − E(row))` on its history's **prior rows**. The tile's cost is `Σ` over the cone's
nodes with `d(i) > 0` of the §8 formula with every element count replaced by the demanded count
(`MatMul`: `d·K` MACs; `ReduceSum`/`ReduceMax`: the operand's demand in elementwise ops; `TopK`: the
operand's demand times `k`; the transcendentals: `d`; the rest: `d` elementwise ops; bytes read =
`Σ` operand demands × their widths; bytes written = `d × width(out)`). The tile's **opened bytes** are
`Σ` leaf demands × width, a committed leaf (commit point, carry-in, state, history) at 4 bytes a lane.
Its **operands** are the distinct committed leaves. For the LM head this is `tile_len · d` MACs, not
the vocabulary: a court tile is a tile of the product.

**Dissection (PALW-TIR-32).** A cone that contains a reduction over `H` — `ReduceSum`/`ReduceMax`
along an `H` axis, a `MatMul` contracting `H` — is **dissected**: the court narrows the history by
claimed partial results over the canonical chunks `[c · h_chunk, (c+1) · h_chunk)` of the window's
index space, and its terminal step recomputes one chunk. Its terminal cost is the box demand of one
tile at `H = min(h_chunk, W)`; a cone with no such reduction has the terminal cost of its tile at
`H = W`. Each terminal cost MUST fit the tile ceilings. §9.5 defines the exchange — which values are
claimed, how they fold, what the bottom evaluates — and the obligations admission checks for it.

**Every terminal close is carriable (PALW-TIR-38).** A class with a dissected tile clocks its
executor at every terminal leaf (ADR-0082 C-5: the clock cannot tell a dissected leaf from another),
so the acquitting close of every tile — the whole tile's for a cone that is not dissected, the
bottom's for one that is — MUST be one the chain can carry: its bytes as carried (the close frame,
16 KiB, and the tile's opened bytes; the program is referenced by the class, never carried) at most
`min(max_close_chunks, 32) × 100,000` — the chunks the fold assembles (its bitmap addresses 32) of
one carrier each, 3,200,000 bytes on testnet-12, whose ruleset's close ceiling (202 chunks) is wider
than that. A class past it is refused, naming the bytes and the cap; the registrant declares smaller
tiles. Under the tiled logits scheme the logits node's tile length MUST divide the scheme's 4,096
lanes (a step tile then lies inside one trace tile, at an offset, and the logits consistency check
compares it with that part), so a large vocabulary's head can be tiled finer: at `d_model = 1,536`
(Qwen2.5-1.5B A16) a logits tile reads 1,536 weight bytes a lane and 2,048 lanes is the largest
admissible divisor. The boundary, on that class's FFN gate tile (1,536 weight bytes and its
per-channel tables a lane): 2,045 lanes are admitted, 2,046 refused (3,200,470 bytes as carried >
3,200,000) — `misaka-palw-base0/tests/tir_a16_admission_dissected.rs` pins both.

**Checkpoint intervals.** The *update cone* of `Fixed` state `j` in a block that writes it is the
cone (§10.2) of its `StateWrite` node (counted once in the cone work). Replaying `j` over positions
needs the values of every state its update reads: the **replay closure** is the smallest set of
states containing `j` and every state read (`Ref::State`) by an update cone of a member, in the same
block. The replay is **split into `G` groups** when every member's shape has the same first
dimension `G > 1` and every node of the closure's update cones is *aligned* — its output's axis 0
has extent `G`, and element `[g, …]` depends on the closure's states only through their elements
`[g, …]` — by these rules, with a leaf other than a closure state *free*:

- a node whose operands are all free is free; a node with a non-aligned operand, or whose output's
  axis 0 is not `G`, is not aligned;
- an operand is *aligned in place* if it is free, or aligned with the output's rank and axis 0 `G`;
- elementwise primitives, `Cast`, `Clamp`, `Log2Floor`, the transcendentals, `Select`, `Compare`,
  `Broadcast`, `StateWrite`: aligned iff every operand is aligned in place;
- `Transpose`: iff `perm[0] = 0`; `Slice`, `Concat`: iff `axis ≠ 0` and every operand is aligned in
  place; `ReduceSum`, `ReduceMax`, `TopK`: iff `axis ≠ 0`; `Reshape`: iff the operand's axis 0 is also
  `G`; `Gather`: iff `axis ≠ 0`, the indices are free and the data is aligned in place;
- `MatMul`: at rank ≥ 3, iff both operands are aligned in place (the groups are a batch axis); at
  rank 2, iff `b` is free and `a` is aligned in place (the groups are the rows of `a`);
- `Iota` and `HistAppend` are never aligned.

Otherwise `G = 1`. One group's replay of one position costs `⌈c / G⌉` for each component `c` of
`Σ` over the closure's update cones of their nodes' §8 costs, and

```
C_j = min( ⌊max_tile_macs / macs⌋, ⌊max_tile_transcendentals / transcendentals⌋, max_checkpoint_interval )
```

(a zero component imposes no bound). Over the blocks that write `j`, the smallest `C_j` counts; a
`C_j` of 0 is a refusal. The class's commitment layout checkpoints every `C ≤ min_j C_j` positions
(Phase F D5). The delta rule of a GDN layer whose per-position operands are commit points splits per
head; the corpus GDN and Mamba-2 layers, whose conv output is not committed, replay the conv window
with the state and do not split — admission is conservative, never optimistic.

**Dissectability is structural (PALW-TIR-32).** Call a tensor with an `H` axis *H-local* if its
element at history index `t` depends only on history index `t` of its `H`-carrying operands and on
`H`-free values. Every v1 primitive that produces an `H`-carrying output from `H`-local operands
produces an `H`-local output, because no primitive mixes history positions except by reducing `H`
away: `Slice`, `Concat`, `TopK` and `Gather` cannot act along `H` (§6), `Reshape` keeps `H` as its
own axis, and a `MatMul` over `H` contracts it. `HistAppend` and an `Iota` over `H` are H-local. So
every reduction over `H` (`ReduceSum`/`ReduceMax` along `H`, `MatMul` contracting `H`) reduces an
H-local tensor: exact (order-free) sums and maxima of per-position terms, which the court dissects
by chunk given the claimed values of the `H`-free operands (the multi-pass softmax is exactly this:
the maximum, then the exponent sum against the claimed maximum, then the value sum against both —
ADR-0082). A program that rounds a running sum over `H` cannot be written in v1.

### 10.4 The court (PALW-TIR-15)

A terminal refutation of an IR class recomputes the disputed tile by evaluating its cone (§9.2) over
PALW-TIR v1 from opened leaves; `court_catalog_root` commits to `prim_set_id`. The court knows the
primitives and nothing about models.

## 11. Library templates (informative)

The composite operations every family needs are subgraphs: `tir_library_v1`,
`misaka_palw_tir::library` — builders that append plain primitives, so a program built with a template
and the same program written out by hand are the same bytes (its catalogue is `LIBRARY_V1`). Where a
template reproduces a live kernel, conformance is **tested against the live code**: the crate
`misaka-palw-tir-conformance` (test-only, a dependency of nothing) runs every integer kernel of the
court's catalogue — all 38 — and the fenced `RequantizeByToken` against its segment, byte for byte, on
seeded random operands and the type extremes of each kernel's own domain, and checks every segment
admissible under §7; its coverage test hashes the list and requires it to be exactly the integer
catalogue plus the fenced kernel. `tests/kat_base0.rs` reproduces the frozen BASE-0 KAT digest. The
seven float kernels are out of scope (an integer IR cannot express them), and the fenced Kimi K3 arms
are recorded as defective (corpus-v1 §10.4) and are not a conformance target.

### 11.1 The legacy rounding rules as segments

| legacy rule (04a) | PALW-TIR segment | conformance |
| --- | --- | --- |
| `RoundingShiftRight(x, s)` (`s` clamped to 31) | `Div_HalfAwayFromZero(x, 2^s)` | KAT digest |
| `RoundingShiftRight64(x, s)` | `Div_HalfAwayFromZero(x, 2^s)` | KAT digest |
| `SRDHM(a, b)` | `Clamp_i32(Div_HalfUp(Mul(a, b), 2^31))` (the MIN·MIN saturation is the clamp) | KAT digest |
| `Requantize(acc, m, s, z)` (BASE-0 op 2) | `Clamp[−128,127](Add(Div_HAFZ(SRDHM(acc, m), 2^min(s,31)), z))` | KAT digest |
| `Rescale(acc, m, s)` (BASE-0 op 9) | `Clamp_i32(Div_HAFZ(Mul(acc, m), 2^min(s,62)))` | KAT digest |
| `IntRecip(v)` | `Div_Floor(Mul_i128(IntRsqrt(v), IntRsqrt(v)), 2^24)` | KAT digest |
| A16 narrowing `a16_scale_round(x,m,s).saturating_add(z).clamp(lo,hi)` | `Clamp[lo,hi](Add_i128(Clamp_i64(Div_HAFZ(Mul_i128(x, m), 2^min(s,62))), z))` | conformance crate |
| a variable shift `s` (per channel, per token — the lift of ADR-0102) | `2^s = Gather([2^0 … 2^62], Clamp_idx(s, 0, 62))`, then `Div` or `Mul` | conformance crate |

The intermediate `i64` saturations of the A16 kernels are `Clamp` nodes on `i128` values; they are
part of the function on adversarial parameters, and the segments reproduce them.

### 11.2 Composite operations

| template | expansion (sketch) | legacy kernel reproduced |
| --- | --- | --- |
| `IntSigmoid(x)` | `e = IntExp(−|x|)`; `r = IntRecip(ONE + e)`; `num = Select(x ≤ 0, e, ONE)`; `Div_Floor(num·r, 2^24)` | `int_sigmoid` |
| `SiLU(x)` | `Clamp_i32(Div_Floor(x · IntSigmoid(x), 2^24))` | `silu` (op 6, `q36/silu`) |
| softmax, wide (`up`) | `m = ReduceMax`; `d = Clamp(x − m, i32::MIN >> up, 0)`; `IntExp(Clamp(d·2^up, i32::MIN, 0))`; exact `ReduceSum`; `IntRecip`; `Div_Floor(e·recip, 2^24)` | `softmax_shifted` (op 5W; `up = 0` is op 5) |
| RMSNorm, A16 | `mean = Div_Floor(Σx²·2^24, n)`; `r = IntRsqrt(Clamp_i64(mean) + eps)`; `Clamp_i32(x·r)` | `a16_rms_norm` |
| RMSNorm, wide rows | `Σx²` in `i128`; exponent by `Log2Floor`, even shift into `[2^24, 2^26)`, `IntRsqrt`, product shifted back | `q36_rms_norm_wide` |
| L2Norm (Q15 out) | `Log2Floor` of `Σx²`, mantissa into `[2^24, 2^26)`, `IntRsqrt`, product shifted by `9 + e` | `q36_l2_norm` |
| LayerNorm | **exact centring**: `c = n·x − Σx` (no division), then the RMSNorm of `c` with `eps' = n²·eps` (at the input scale²) | — (new; §14) |
| RoPE, adjacent pairs | `Div_Floor(a·cos − b·sin, 2^24)`, `Div_Floor(a·sin + b·cos, 2^24)`, clamp | `a16_rope`, `rope_table` |
| RoPE angles at long context | two pinned tables of `2^b` rows: `pos = hi·2^b + lo`, angle addition in Q24 (§11.3) | — (new) |
| softplus, refined exp, decay | the `IntExp`/`IntLn` compositions of ADR-0052 | `q36_softplus`, `q36_exp_refined`, `q36_decay` |
| gated delta rule step | decay (`Div_HAFZ(S·decay, 2^24)`, clamp ±(2^31−1)), `S·k` (MatMul), read narrowing, `sat64(v − w)`, `sat64(delta·β)`, delta narrowing clamped ±(2^24−1), rank-one write (`Mul` by `2^ws` or `Div_HAFZ` by `2^−ws`), `StateWrite`, `S·q`, out narrowing | `q36_gdn_step` |
| router top-k | softmax (wide), `TopK` (committed), gather, exact sum, `IntRecip`, renormalise | `q36_router_topk` |
| MoE combine | `MatMul(w[1,k], Y[k,width])` — ONE exact accumulator — then the A16 narrowing | `q36_moe_combine` |
| head mapping `k → v` heads | grouping: `Reshape[k,1,d] → Broadcast[k,r,d] → Reshape[v,d]`; tiling: `Reshape[1,k,d] → Broadcast[r,k,d] → Reshape[v,d]` | the live kernel tiles, over llama.cpp's V-reordered artifact — HF's function (corpus §5) |
| causal conv window | `Concat(State[w−1, C], row)` → `Slice` keeps the last `w−1` → `StateWrite`; `ReduceSum(window ⊙ taps)` (`causal_conv`) | `q36_ssm_conv` (its own layout, as the segment of the same name) |

### 11.3 Long-context RoPE without a `history_bound × rope_dims` table

The rotary angle `θ_j(pos) = pos · ω_j` for `pos = hi·2^b + lo` is `θ_j(hi·2^b) + θ_j(lo)`, so two
tables `T_hi[hi][j] = (cos, sin)(hi·2^b·ω_j)` and `T_lo[lo][j] = (cos, sin)(lo·ω_j)` of `2^b` and
`history_bound / 2^b` rows give

```
hi = Div_Floor(pos, 2^b);  lo = pos − hi·2^b                       (idx, exact)
cos = Clamp(Div_Floor(ch·cl − sh·sl, 2^24), ±ONE);  sin = Clamp(Div_Floor(sh·cl + ch·sl, 2^24), ±ONE)
```

Each product is `i64`; the sum is `i128`, because the tables are params and take the full `i32`
range (two `i32·i32` products sum to `2^63`) — the pattern that overflowed `i64` in the live
`q36_rope_partial` (corpus §10.3; fixed on `rcore/hf-court-total` by forming the products in
`i128`).

At `2^18` positions that is two 512-row tables instead of one 262,144-row table (at `2^21`, three
128-row tables). The gathers' index ranges are provable (`hi < 2^(18−b)`, `lo < 2^b`). YaRN, NTK,
"llama3", LongRoPE and linear scaling only change `ω_j` (and an attention factor folded into the
logit scale): they are table data, not primitives (`rope_angles_two_level`).

LongRoPE and dynamic NTK choose their frequencies by the forward call's length in HF. Their
canonical semantics is **per-position decode** (RFC-0002 Gate 1 decision 4): the frequency set is a
function of the absolute position — table sets selected by `Select` on a position threshold
(`rope_angles_by_position`) — which is what HF computes when it decodes one position per call; the
criterion-5 reference is pinned to `transformers` 5.17 with eager attention, decoding per position.

## 12. Golden vectors (`consensus-vectors/tir-v1/`, PALW-TIR-35)

All files are UTF-8 JSON; every integer is a **decimal string** (so no reader loses precision);
tensors are `{"dtype": "i32", "shape": [2, 3], "data": ["1", "-2", …]}` in row-major order.

- **`primitives/NN-Name.json`** (`format = palw-tir-v1/primitive-vectors/1`), one file per
  stateless primitive (tags 0–22): `cases[]` each with `name`, `prim` (`name`, `attrs` as
  `[key, value]` pairs, and **`borsh_hex`: the canonical encoding of the `Prim`**, §4.3), `inputs`
  (tensors), `out` (`dtype`, concrete `shape`), and either `expect` (a tensor) or `expect_error` (the
  class). A conforming implementation evaluates the primitive on the inputs with the given output
  type and must succeed with `expect` exactly, or fail. Edges covered: exact halves and negative
  halves under all three division rules (including the SRDHM halves and the `RoundingShiftRight(−64,
  1)` regression), `i32`/`i64`/`i128` minima and maxima, every `IntExp` range-reduction bucket edge,
  the `IntRsqrt` seed basin, the order-free rule (a `MatMul` whose total fits but whose positive
  terms do not), `TopK` ties, every error class a primitive can raise, and a wrong operand count
  (`error_arity_*`: `Shape` outside a program, §9.3); plus seeded random cases.
- **`programs/name.json`** (`format = palw-tir-v1/program-vectors/3`): `program_borsh_hex` (a
  canonical program, §4), `graph_ir_root_hex` (its identity, §3.6), `params` (`param`, `layer` or
  null, `le_hex` = the tensor's little-endian bytes; dtype and shape from the declaration),
  `steps[]` (for each position: `pos`, `token`, `logits`, and `commits[]` = every commit point with
  `slot`, `block`, `layer`, `node`, `value`), `cones[]` (an `eval_cone` case: `block`, `layer`,
  `target`, `token`, `pos`, `carry_in`, `fixed`, `hist_prior`, `supplied`, `expect`; the environment
  is complete, §9.2), and `refusals[]` (an honest cone environment with one defect — `what` names it
  — and `expect_error` = the class of §9.3: the target supplied, an index that is no node, `pos =
  history_bound`, a `Fixed` value absent, a history absent at the last position and at position 0
  where zero rows are needed, the token absent or at `token_bound`; `token` is null when absent).
  The state primitives are pinned here: `fixed-state-saturation` (`StateWrite`, per-layer
  instances), `hist-window` (`HistAppend` with window 3, an `Iota` over `H`), and five whole models
  (dense GQA 2-layer, sliding + global, GDN with 2 key / 4 value heads, Mamba2, top-2 MoE with a
  shared expert).
- **`encoding.json`** (`format = palw-tir-v1/encoding-vectors/1`): byte strings with `expect` =
  `ok` or the refusal class — a valid program and its mutations (trailing byte, truncation, version 2,
  a `bool` of 2, an unknown primitive tag, a dead node, a forward reference, a declared shape that is
  not the inferred one, a per-layer mismatch, an uncommitted logits node, a forbidden
  `history_bound`, a `prim_set_id` other than `PRIM_SET_ID_V1`, a single block), and the NF-19 cases:
  a global state written by `pre` (valid), by `post`, by both, and a global history appended by both.

`cargo test -p misaka-palw-tir --test golden` regenerates every file and requires identical bytes;
`TIR_BLESS=1` rewrites them, which is a change of the semantics and is reviewed as one.

- **`demand/<program>.json`** and **`dissect/<program>.json`**: §9.4's demand evaluation and §9.5.8's
  history dissection, each regenerated by its own test as its section states.

## 13. Rules

- **PALW-TIR-1 (the primitive set).** An IR class's execution MUST be the evaluation of its
  `TirProgramV1` over the twenty-five primitives of §6. No other operation has consensus meaning.
- **PALW-TIR-2 (integers only).** Every value on the consensus path MUST be an integer of a declared
  dtype. No float, no libm, in the evaluator or in any backend (ADR-0040 A).
- **PALW-TIR-3 (named loss).** Only `Div` (by its rule), `Clamp` and `StateWrite` (Saturate),
  `Log2Floor`, the three transcendentals, and `Select`/`Compare`/`TopK` (which discard by choosing)
  MAY lose information. Every other primitive MUST be exact.
- **PALW-TIR-4 (order-free regions).** An implementation MAY reorder arithmetic only inside an
  order-free region (§6.8). Everywhere else it MUST produce exactly the defined values.
- **PALW-TIR-5 (no wide lane).** `i64` and `i128` MUST NOT be committed, carried, stored in state or
  history; `i128` MUST NOT be a param.
- **PALW-TIR-6 (bounded structure).** A program MUST be a DAG with strictly backward refs, a static
  schedule of at most 1024 layers, rank ≤ 4, and no symbolic dimension other than `H`.
- **PALW-TIR-7 (canonical form and identity).** Admission MUST refuse bytes that are not the unique
  encoding (§4) of a program in normal form (§5). `graph_ir_root` MUST commit to the whole encoding
  and to nothing outside it.
- **PALW-TIR-8 (shapes).** Admission MUST infer every node's type (§6) and refuse any mismatch with
  its declared `out`.
- **PALW-TIR-9 (ranges).** Admission MUST establish the §7 intervals from type-worst-case params and
  refuse any unmet obligation. A program it cannot prove is refused.
- **PALW-TIR-10 (state).** `Fixed` states MUST declare their range and `StateWrite` MUST saturate to
  it; `Hist` states MUST be append-only and read only through `HistAppend`'s window (§6.7).
- **PALW-TIR-11 (selection).** Selections MUST break ties to the lowest index; `TopK` MUST return its
  set in index order; every `TopK` MUST be a commit point.
- **PALW-TIR-12 (cost).** No worst-case per-position quantity of §8 MAY exceed the network's
  ceiling (`tir_admit_v1`, §10.3).
- **PALW-TIR-13 (court cones).** Every commit point's terminal cost — one tile by box demand, or one
  canonical `H` chunk for a dissected cone — MUST fit the tile ceilings (§10.3); admission MUST derive
  each `Fixed` state's checkpoint interval `C_j` by its replay closure and groups.
- **PALW-TIR-14 (commit points).** Carry-outs, logits, every `TopK`, and every `HistAppend` row MUST be
  commit points (§10.1).
- **PALW-TIR-15 (the court).** A terminal refutation of an IR class MUST recompute the disputed tile by
  evaluating its cone (§9.2); `court_catalog_root` MUST commit to `prim_set_id`.
- **PALW-TIR-16 (canonical work).** The work vector (PALW-WK-1) of an IR class MUST be derived from
  the program's structure; commit points and `tile_len` MUST NOT change it.
- **PALW-TIR-17 (backends).** A backend MAY implement any subgraph natively, provided every commit
  point is byte-identical to the reference evaluator. A fused kernel MUST NOT appear in any consensus
  object.
- **PALW-TIR-18 (an IR, not a VM).** A program MUST NOT contain control flow; its only iteration is
  the scan over positions, whose trip count is the job's length.
- **PALW-TIR-19 (the encoding is the interface).** The byte encoding of §4 is complete and stable:
  any tool that emits it targets PALW-TIR without this repository's code. A change to it is a new
  program version.
- **PALW-TIR-20..22 (types, `H`, broadcasting)** as §2.1–2.3.
- **PALW-TIR-23 (exact results).** Every primitive's result MUST fit its declared dtype, or the
  evaluation fails; only `Clamp` and `StateWrite` saturate.
- **PALW-TIR-24 (order-free sums).** An exact reduction MUST fail unless its positive-term sum and its
  negative-term sum both fit the declared dtype (§6.3).
- **PALW-TIR-25 (division).** `Div` MUST implement exactly the three rules of §6.4 for divisors `≥ 1`
  and fail for smaller ones.
- **PALW-TIR-26 (transcendentals).** `IntExp`, `IntRsqrt` and `IntLn` MUST be the fixed-iteration
  algorithms and constants of §6.5; no convergence test, no libm.
- **PALW-TIR-27 (TopK).** As §6.6.
- **PALW-TIR-28 (a step).** As §9.1: effects apply only after the whole step succeeded; the initial
  state is all-zero `Fixed` values and empty histories.
- **PALW-TIR-29 (histories).** `HistAppend` MUST return exactly the last `min(pos + 1, window)` rows,
  oldest first.
- **PALW-TIR-30 (cones).** As §9.2.
- **PALW-TIR-31 (the window of a block).** All histories a block appends to MUST share one window,
  which defines `H` for the block.
- **PALW-TIR-32 (dissectability).** Every reduction over `H` is dissectable (§10.3); admission need
  not search for violations, because the type rules exclude them.
- **PALW-TIR-33 (committed operands).** A committed value outside its node's proven interval (a
  state checkpoint leaf outside `[lo, hi]`) is a malformed commitment and convicts the executor, who
  committed it.
- **PALW-TIR-34 (totality).** Every failure is an error, never an abort. Success versus failure is
  normative; each rule reports the class §9.3 names, and no class decides a verdict.
- **PALW-TIR-35 (golden vectors).** An implementation MUST reproduce every vector of §12.
- **PALW-TIR-36 (no structured control).** v1 has no `BoundedScan`, `BoundedMap` or `BoundedReduce`:
  no corpus family has a recurrence inside a position, heads/experts/channels are batched as tensor
  axes, and a reduction with a user body is order-dependent unless proved associative and exact.
- **PALW-TIR-38 (carriable closes).** As §10.3: every terminal close of a class — a whole tile's or a
  dissected cone's bottom — fits the chunks the chain assembles; under the tiled logits scheme the
  logits tile length divides 4,096.
- **PALW-TIR-37 (H dissection).** A committed tile whose cone reduces over `H` is dissected as §9.5
  states: its reductions and site are the program's; a root claim is admitted only if it finalizes
  to the committed tile and its element lists are exactly the element closure; every round folds
  exactly to the claim under dispute, each partial computed against the ROOT's totals; the bottom
  evaluates one history tile and convicts on the first differing value. Admission refuses a
  dissected cone that breaks O-1 to O-5.

## 14. Deviations from RFC-0002 and open items

Deviations (each argued in `docs/design/palw/tir/corpus-v1.md`):

1. **Twenty-five primitives, below the 30–50 target.** Minimality (freeze criterion 2) removed
   `Split`, `Pad`, `Neg`, `Abs`, `Min`, `Max`, `ShiftLeft`, `ShiftRight`, `DivConst`, `Requantize`,
   `Rescale`, `Narrow`, `IntRecip`, `IntSigmoid`, `ArgMax`, `ReduceMin`, `BatchedMatMul`, `Widen`
   (→ `Cast`), `StateRead`/`HistRead` (→ a `Ref` and the merged `HistAppend`). The candidates
   `Scatter`, `Sort`, `IntSigmoid` and `BoundedScan` are out (and `BoundedMap`, `BoundedReduce`).
2. **Additions.** `i128` as an internal dtype; `Log2Floor`; `Div` with a tensor divisor (one
   primitive for every rounding shift, per-channel and per-token shifts, means and exact
   renormalisation); `Iota` over `H`; `Gather` with `batch_dims`.
3. **`HistAppend` returns the window** (`HistRead` merged into it) — one node per history, with the
   dependency order explicit.
4. **Fixed state is committed at checkpoints, not per position, as leaves of the one step tree.**
   RFC §6's "the input of every StateWrite" as a per-position commit point would exceed the step-leaf
   cap for a GDN state (`v_heads · d_v · d_k` lanes per layer per position); the per-position commit
   points are the update cone's inputs instead, and the checkpoints are step leaves — no checkpoint
   leg (§10.1, Phase F D7).
5. **`token_bound`** is a program field (the embedding `Gather` needs a provable index range).
6. **Committed operands are checked against proven intervals** (PALW-TIR-33), so the range analysis
   is sound for cones evaluated from commitments; CarryIn takes the full dtype range.
7. **The dissection is over every reduction over `H`, not the attention triple.** RFC-0002 and
   ADR-0082 dissect the fused attention's three quantities; §9.5 dissects whatever reductions the
   program's cone has (at most sixteen), each folding exactly, with the attention as one instance
   (PALW-TIR-37).
8. **The 2^28-element cap is for computed tensors only.** Applied to params (RFC §5.4 lists it for
   "any tensor") it would refuse every real vocabulary embedding and every large MoE layer; params
   get a sanity bound of 2^40 elements instead (NF-8).

Revision 2 (this text) applied the second implementation's findings: a cone never implies a
`Fixed` value or a history (F1, F3), never takes its target from the environment (F2), refuses an
index that is no node (F4); NF-19 forbids two writers of one state instance and any write in `post`
(F5, F6); the position and token bounds and the carry-in checks of a cone are stated (F7, F8); the
transcendentals have a type clause (F9); "needed" is defined (F10); the run state may omit untouched
instances (F11); the caps sentence (F12) and the block count (F13) are corrected; `prim_set_id` is
defined and checked (F14); every rule has a class (F15). The editorial items left after revision 2
are applied too: unread environment entries are ignored whatever their key (N1, §9.2), an arity
error outside a program is `Shape` (N2, §9.3), every row of §9.2's table checks values (N3), and
`graph_ir_root`'s hash and key are stated, with a vector (N4, §3.6).

Open items: the param binding for per-layer params (the IR artifact stores a legacy 17-byte A16
triple as three typed tensors `m`, `s`, `z`, repacked at conversion — a re-registered legacy class
gets a new inventory root with the same numbers); the cost coefficients (Phase D); the network values
of the admission ceilings (Phase F's `palw_tir_v1` fence — the legacy terminal ceiling of 16 Mi MACs
per tile and a cone work of `2^20` are the starting points); vectors of admission's derived numbers
(cones, tiles, `C_j`) for the program vectors, which §12 does not have yet; the element-level demand closure of the court's replay (Phase F's
demand evaluator), which may split a replay that §10.3's axis-0 rule conservatively keeps whole. PALW-TIR-33 is settled:
every committed operand — step leaves, state checkpoint leaves, carry-ins — is the executor's, and a
value outside its node's proven interval convicts the executor (Phase F §2.7).
