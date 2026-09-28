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
> Status: **Draft for review (Gate 1)**. `prim_set_id` is not fixed until freeze (RFC §1.2).

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
- An *error* aborts the whole evaluation it occurs in (§9.1). Which error class is reported is
  diagnostic; **only success versus error is normative** (PALW-TIR-34).

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
| commit point | a node with `commit = true`; its value is committed in the step leg |
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

The worst case used by the size caps and by §7/§8 is `H = W`. Every tensor's element count at the
worst case MUST be `≤ 2^28`.

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
  prim_set_id:      [u8; 64]            the network's hash of the prim-set descriptor (§6.0)
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

### 3.5 Params and consts

A param is bound to the artifact inventory by `name` (per-layer params at each layer that runs a
block referencing them; the binding convention is chapter 03's, extended in Gate 2). The reference
evaluator receives each param instance as a tensor and MUST check that it has exactly the declared
dtype and shape. **Params take their dtype's full range**: weights are not trusted (§7).

A const is `data`, the little-endian encoding of `Π shape` elements of `dtype`.

### 3.6 Identity (PALW-TIR-7)

`graph_ir_root = H(encode(program))` for the network's keyed hash. The canonical encoding (§4) is the
program: every field above is part of the identity, including names, the order of nodes (it fixes
the node slots) and the commit flags. The identity MUST NOT depend on anything outside the program:
thread counts, repacking, backends, fused kernels, tile shapes a backend chooses, or the physical
layout of the artifact beyond the param binding.

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
- NF-1 `version = 1`; `history_bound ∈ {2^18, 2^21}`; `token_bound ≥ 1`.
- NF-2 `1 ≤ |blocks| ≤ 16`; `|schedule.layers| ≤ 1024`; `|params| ≤ 4096`; `|states| ≤ 64`, of which
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
- NF-19 At most one `StateWrite` per state per block and at most one `HistAppend` per state per block.
- NF-20 The input of every `HistAppend` is a committed node or a carry-in (PALW-TIR-14).
- NF-21 Every carry-out node is committed.
- NF-22 **No dead node**: every node is reachable backwards (through `Node` refs) from a root — a
  committed node, a `StateWrite`, a `HistAppend`, a carry-out node or (in `post`) the logits node.

Canonical attribute forms are enforced by the type rules: a `Transpose` permutation is a
permutation, a `Clamp` has `lo ≤ hi` inside `out.dtype`, and so on (§6). NF-1..22 are exactly the
checks of `misaka_palw_tir::validate::validate`.

## 6. The primitives (PALW-TIR-1)

### 6.0 The set

Twenty-five primitives, every one named and defined by mathematics, none by an architecture or a
model (freeze criterion 1). The *prim-set descriptor* whose network hash is `prim_set_id` is the
ASCII string

```
palw-tir/v1/spec=04b-tensor-ir/rev1/q=24/prims=0:Reshape,1:Transpose,2:Slice,3:Concat,4:Broadcast,5:Iota,6:Gather,7:Cast,8:Add,9:Sub,10:Mul,11:MatMul,12:ReduceSum,13:ReduceMax,14:Div,15:Clamp,16:Log2Floor,17:IntExp,18:IntRsqrt,19:IntLn,20:Compare,21:Select,22:TopK,23:StateWrite,24:HistAppend
```

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

All three read and write **Q24**; their inputs MUST NOT be `i128` (the value is used as a 64-bit
quantity); their outputs obey the exact-result rule. Constants (all decimal integers):

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
`[−24·LN2_Q, 39·LN2_Q)`.

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

`tir_admit_v1` (Gate 2) assigns every tensor an integer interval `[lo, hi]` and refuses the program
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
malformed commitment (the committer loses), exactly as a value outside the dtype is today. This is
what makes the analysis sound for cones evaluated from committed values rather than recomputed
ones. (The Gate 1 evaluator checks dtypes; it gains the interval check with `tir_admit_v1`.)

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

Per position the analysis reports the sums, the **state bytes** (`Σ` Fixed `B(state)` and Hist
`W · B(row)` over all instances), the **peak live bytes** (nodes in index order; an output is live
from its node to its last consumer, or to the end of the occurrence if it is a root), and the **court
worst case** (§10.3). All are linear in `H`. The coefficients that turn the vector into time are
benchmarked in Phase D; these formulas upper-bound the reference evaluator's work.

## 9. The reference evaluator

### 9.1 A step (PALW-TIR-28)

Input: the program, the params, the run state `(pos, Fixed values, Hist rows)`, and `token`.

1. Fail (class `Position`) if `pos ≥ history_bound`; fail (class `Operand`) if the token is needed
   and `token ≥ token_bound`.
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

### 9.2 Cone evaluation (PALW-TIR-30)

`eval_cone(block, layer, target, env)` evaluates one node of one occurrence from supplied values —
the court's entry point. `env` gives: the token and `pos`; the carry-in values; the `Fixed` values at
the start of the position; for each `Hist` state the prior rows (exactly `min(pos, window − 1)`,
oldest first); and **supplied** values for any nodes of the occurrence. The evaluator computes the
backward closure of `target` that stops at every supplied node (a supplied node's value is taken, not
recomputed, after checking it has the node's declared dtype and shape at the running `H`), in node
index order, by §6 and §9.1(2). A value the closure needs and nobody supplied fails (class
`Missing`). `(block, layer)` MUST be an occurrence of the schedule. Effects are not applied.

Evaluating a commit point's cone with every other commit point of the occurrence supplied from an
honest step reproduces the step's committed value — `tests/programs.rs` checks this for every commit
point of every position of five programs.

### 9.3 Errors (PALW-TIR-34)

Every malformed program, every out-of-range value, every overflow and every missing operand is an
error, never an abort of the implementation. The classes (diagnostic): `Encoding`, `NormalForm`,
`Shape`, `Overflow`, `Index`, `Divisor`, `Operand`, `Missing`, `Position`.

## 10. Commitment and the court

### 10.1 Commit points (PALW-TIR-14)

Required commit points, enforced by normal form:

- every block's carry-out nodes (NF-21) and the logits (NF-6);
- every `TopK` (NF-18) — a selection is an opened value, never a bisection target (ADR-0052 B);
- the input row of every `HistAppend` (NF-20), so later positions' cones can open it.

**State.** A `Fixed` state is committed at checkpoint positions in the checkpoint leg (every `C_j`
positions for state `j`), not at every position; between checkpoints the court replays the state's
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
consts, `State` values (from the checkpoint leg or replay), carry-ins (the previous occurrence's
committed carry-out), inputs, and — for a `HistAppend` in the cone — the history's prior rows (each
the committed row of an earlier position). The court opens the leaves and runs §9.2.

### 10.3 Court feasibility

For every commit point, the cone's cost (§8) MUST fit the court's ceilings
(`derive_court_cost_v1`: ≤ 16 Mi terminal MACs per tile, ≤ 8 operands, ≤ 27 close chunks), at three
levels — IR node → canonical chunk → tile: a `MatMul` output is committed in tiles of `tile_len`
dot products; a reduction over `H` is dissected by partial results over canonical chunks of `H`;
a cone crossing a `State` ref stops there, and admission derives each `Fixed` state's checkpoint
interval `C_j` as the largest `C` with `C × cost(update cone of j per position)` within the ceiling.

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

The composite operations every family needs are subgraphs. The first templates live in
`misaka_palw_tir::builder`; they grow into `tir_library_v1` in Gate 2. Where a template reproduces a
live kernel, conformance is **tested**: `tests/kat_base0.rs` (the frozen BASE-0 KAT digest) and
`tests/legacy_mirror.rs` (verbatim transcriptions of the A16/Q36 kernels, on random and
type-extreme inputs).

### 11.1 The legacy rounding rules as segments

| legacy rule (04a) | PALW-TIR segment | conformance |
| --- | --- | --- |
| `RoundingShiftRight(x, s)` (`s` clamped to 31) | `Div_HalfAwayFromZero(x, 2^s)` | KAT digest |
| `RoundingShiftRight64(x, s)` | `Div_HalfAwayFromZero(x, 2^s)` | KAT digest |
| `SRDHM(a, b)` | `Clamp_i32(Div_HalfUp(Mul(a, b), 2^31))` (the MIN·MIN saturation is the clamp) | KAT digest |
| `Requantize(acc, m, s, z)` (BASE-0 op 2) | `Clamp[−128,127](Add(Div_HAFZ(SRDHM(acc, m), 2^min(s,31)), z))` | KAT digest |
| `Rescale(acc, m, s)` (BASE-0 op 9) | `Clamp_i32(Div_HAFZ(Mul(acc, m), 2^min(s,62)))` | KAT digest |
| `IntRecip(v)` | `Div_Floor(Mul_i128(IntRsqrt(v), IntRsqrt(v)), 2^24)` | KAT digest |
| A16 narrowing `a16_scale_round(x,m,s).saturating_add(z).clamp(lo,hi)` | `Clamp[lo,hi](Add_i128(Clamp_i64(Div_HAFZ(Mul_i128(x, m), 2^min(s,62))), z))` | mirror |
| a variable shift `s` (per channel, per token — the lift of ADR-0102) | `2^s = Gather([2^0 … 2^62], Clamp_idx(s, 0, 62))`, then `Div` or `Mul` | mirror |

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
| head mapping `k → v` heads | grouping: `Reshape[k,1,d] → Broadcast[k,r,d] → Reshape[v,d]`; tiling: `Reshape[1,k,d] → Broadcast[r,k,d] → Reshape[v,d]` | (the live kernel tiles; §14) |
| causal conv window | `Concat(State[w−1, C], row)` → `Slice` keeps the last `w−1` → `StateWrite`; `ReduceSum(window ⊙ taps)` | `q36_ssm_conv` (data layout differs; Gate 2) |

### 11.3 Long-context RoPE without a `history_bound × rope_dims` table

The rotary angle `θ_j(pos) = pos · ω_j` for `pos = hi·2^b + lo` is `θ_j(hi·2^b) + θ_j(lo)`, so two
tables `T_hi[hi][j] = (cos, sin)(hi·2^b·ω_j)` and `T_lo[lo][j] = (cos, sin)(lo·ω_j)` of `2^b` and
`history_bound / 2^b` rows give

```
hi = Div_Floor(pos, 2^b);  lo = pos − hi·2^b                       (idx, exact)
cos = Clamp(Div_Floor(ch·cl − sh·sl, 2^24), ±ONE);  sin = Clamp(Div_Floor(sh·cl + ch·sl, 2^24), ±ONE)
```

Each product is `i64`; the sum is `i128`, because the tables are params and take the full `i32`
range (two `i32·i32` products sum to `2^63`) — the pattern that overflows `i64` in the live
`q36_rope_partial` (corpus §10.3).

At `2^18` positions that is two 512-row tables instead of one 262,144-row table (at `2^21`, three
128-row tables). The gathers' index ranges are provable (`hi < 2^(18−b)`, `lo < 2^b`). YaRN, NTK,
"llama3", LongRoPE and linear scaling only change `ω_j` (and an attention factor folded into the
logit scale): they are table data, not primitives.

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
  terms do not), `TopK` ties, and every error class a primitive can raise; plus seeded random cases.
- **`programs/name.json`** (`format = palw-tir-v1/program-vectors/1`): `program_borsh_hex` (a
  canonical program, §4), `params` (`param`, `layer` or null, `le_hex` = the tensor's little-endian
  bytes; dtype and shape from the declaration), `steps[]` (for each position: `pos`, `token`,
  `logits`, and `commits[]` = every commit point with `slot`, `block`, `layer`, `node`, `value`), and
  `cones[]` (an `eval_cone` case: `block`, `layer`, `target`, `token`, `pos`, `carry_in`, `fixed`,
  `hist_prior`, `supplied`, `expect`). The state primitives are pinned here: `fixed-state-saturation`
  (`StateWrite`, per-layer instances), `hist-window` (`HistAppend` with window 3, an `Iota` over `H`),
  and five whole models (dense GQA 2-layer, sliding + global, GDN with 2 key / 4 value heads, Mamba2,
  top-2 MoE with a shared expert).
- **`encoding.json`** (`format = palw-tir-v1/encoding-vectors/1`): byte strings with `expect` =
  `ok` or the refusal class — a valid program and its mutations (trailing byte, truncation, version 2,
  a `bool` of 2, an unknown primitive tag, a dead node, a forward reference, a declared shape that is
  not the inferred one, a per-layer mismatch, an uncommitted logits node, a forbidden
  `history_bound`).

`cargo test -p misaka-palw-tir --test golden` regenerates every file and requires identical bytes;
`TIR_BLESS=1` rewrites them, which is a change of the semantics and is reviewed as one.

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
- **PALW-TIR-12 (cost).** No worst-case cost of §8 MAY exceed the network's ceiling.
- **PALW-TIR-13 (court cones).** Every commit point's cone MUST fit the court ceilings (§10.3);
  admission MUST derive each `Fixed` state's checkpoint interval.
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
- **PALW-TIR-33 (committed operands).** A committed value outside its node's proven interval is a
  malformed commitment.
- **PALW-TIR-34 (totality).** Every failure is an error, never an abort. Success versus failure is
  normative; the error class is diagnostic.
- **PALW-TIR-35 (golden vectors).** An implementation MUST reproduce every vector of §12.
- **PALW-TIR-36 (no structured control).** v1 has no `BoundedScan`, `BoundedMap` or `BoundedReduce`:
  no corpus family has a recurrence inside a position, heads/experts/channels are batched as tensor
  axes, and a reduction with a user body is order-dependent unless proved associative and exact.

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
4. **Fixed state is committed at checkpoints, not per position.** RFC §6's "the input of every
   StateWrite" as a per-position commit point would exceed the step-leaf cap for a GDN state
   (`v_heads · d_v · d_k` lanes per layer per position); the per-position commit points are the
   update cone's inputs instead (§10.1).
5. **`token_bound`** is a program field (the embedding `Gather` needs a provable index range).
6. **Committed operands are checked against proven intervals** (PALW-TIR-33), so the range analysis
   is sound for cones evaluated from commitments; CarryIn takes the full dtype range.
7. **The 2^28-element cap is for computed tensors only.** Applied to params (RFC §5.4 lists it for
   "any tensor") it would refuse every real vocabulary embedding and every large MoE layer; params
   get a sanity bound of 2^40 elements instead (NF-8).

Open items for Gate 2 and later: the param binding for per-layer params and for legacy 17-byte A16
triples (the IR artifact stores `m`, `s`, `z` as separate tensors); the checkpoint leg's encoding of
`Fixed` states; `tile_len` per commit point and the canonical chunking of `H`; the cost coefficients
(Phase D); the court's operand-interval check (PALW-TIR-33) and who loses on it.
