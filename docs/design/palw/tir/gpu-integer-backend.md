# PALW-TIR on a GPU — an exact integer backend (RFC-0002 §7 F-6, Phase G prototype)

Status: **prototype, measured (2026-10-01; position batching 2026-10-02, §11)**. Node software only: nothing here reaches consensus, a
class id, a fingerprint or the release build. Lane M3, branch `rfc6/gpu-shard` (off `rfc4/int` @
`ce04e5c22`), crate `misaka-palw-tir-gpu/` — an **isolated cargo workspace** with its own
`Cargo.lock` (not a root member; the root lock and the release build are untouched). It reads
`misaka-palw-tir` and `misaka-palw-tir-exec` by path, read-only.

## 概要(日本語)

- **結論。** PALW-TIR の値はすべて整数で、丸めは名前付きの lossy site だけ、和は証明済み範囲内で順序に依存しない。
  だから GPU は「近似 backend」ではなく **参照評価器とビット一致する厳密 backend** になる。wgpu(Metal/Vulkan)で
  25 primitive すべてを実装し、M1 Max 上で CPU executor・参照評価器・golden vector と全件ビット一致した
  (IntExp は定義域の全 360,501,485 入力、ランダム 5,700 ケース、random program 400 本、program vector 7 本は全ノード)。
- **規則は一つ: device は CPU executor が持つのと同じ refined `NodePlan` の証明の範囲でしか計算しない。** `Fast64` は
  wrapping i64(Z/2^64 は環で、真の和が i64 に入るので順序によらず正確)、`Pn64` は正負を分けて検査、内側ループは
  operand 区間から導いた `c = ⌊(2^31−1)/max|term|⌋` 項ごとの i32 chunk。証明が無いノードは CPU の kernel で実行する。
- **i64 は native(`SHADER_INT64` 必須)、i128 は 2 語の自前ライブラリ。** naga 30 の Metal 出力は i64 の `/` `%` を
  コンパイルできない(fail-closed)ので 64/128-bit 除算は自前の long division。
- **速度。** 位置の batch(seat が commit 済み job を検証する形、prefill)は CPU kernel の 13〜40 倍(GEMM 最大
  384 GMAC/s)。1 位置ずつの decode は primitive 1 個 = dispatch 1 回なので dispatch 律速で、CPU executor と同等。
  decode を速くするのは fused kernel(narrowing・norm・softmax の chain を 1 kernel に)— RFC-0002 Phase G の次の段。
- **位置 batch の replay(§11、実装済み)。** commit 済み job の全 token を持つ seat は、occurrence を 1 つずつ chunk 内の
  全位置に対して 1 回で実行する(projection は位置をまとめた 1 回の GEMM、因果 attention は H を padding して
  mask した batched MatMul)。Qwen2.5 形 2 層・256/512/1024 位置の job 全体で CPU executor と commit・logits が全位置一致し、
  **CPU executor の 11〜48 倍、1 位置ずつの device executor の 12〜16 倍**(負荷 75〜85 の共有機)、dispatch は
  1 位置 502 回から job 全体で chunk あたり 520 回(1 位置 1〜2 回)に減った。Fixed state の再帰(GDN・Mamba-2)を書く
  occurrence だけは chunk の中で位置ごとに実行する。
- **未実装で仕様だけ書いたもの:** tir-exec 側の dispatch seam(`KernelBackendV1` trait)と、memory ledger の device pool の扱い。

## 1. Why exact, and why a float engine cannot be

A float engine's result depends on the reduction order, the FMA contraction and the kernel a vendor
picked for the shape: Strata's IQ-quant single-token and multi-token kernels round differently, which
is why it needs a "reproducibility mode". PALW-TIR has none of that (spec 04b):

- every value is an integer of a declared dtype (PALW-TIR-2);
- information is lost only at named sites, each with one rule — `Div` under Floor / HalfUp /
  HalfAwayFromZero, `Clamp` and `StateWrite` (Saturate), `Log2Floor`, the three fixed-iteration
  transcendentals, selection (PALW-TIR-3);
- an exact sum's value is order-independent, and its success depends only on the sum of its positive
  terms `P` and of its negative terms `N` (the order-free rule, PALW-TIR-24).

So any implementation that reproduces each lossy site on the same value and never lets an
accumulator lose information computes the reference's function byte for byte. RFC-0002 F-6 says it;
this prototype does it.

## 2. API, device admission, dependency

| choice | why |
| --- | --- |
| **wgpu 30.0.1** (MSRV 1.87 ≤ the repository's 1.93 pin), features `metal`, `vulkan`, `wgsl` only | one WGSL source for Metal (macOS) and Vulkan (Linux/NVIDIA/AMD); WGSL integer semantics are defined (two's complement, wrapping); buffer access is bounds-checked by naga's backends (a garbage index after a failure reads garbage, never out of bounds) |
| not CUDA | NVIDIA only, and no Mac |
| not Metal directly | Mac only |
| not OpenCL | deprecated on macOS |
| not a compute library (Burn, candle) | float-first; the byte-identity rule needs integer kernels written against this IR |

**A device is admitted only with `SHADER_INT64`** (native `i64`/`u64` in WGSL: Metal MSL ≥ 2.3 on an
Apple3+/Metal3 GPU, `shaderInt64` on Vulkan). A host without it has no TIR device and runs the CPU
executor — the backend is an accelerator, never a requirement. The M1 Max (24-core GPU) offers it,
with a 4 GiB binding limit, 32 KiB of workgroup memory and 1,024 invocations a workgroup.

**Finding: naga 30's Metal output cannot compile 64-bit `/` or `%`.** Its division helper calls
`metal::select(rhs, 1, …)` with an `int` (or `uint`) literal, which is ambiguous between the `int` and
`long` overloads. It fails closed — a pipeline error, never a wrong value — and the kernels use no
64-bit division at all (§4). Every other 64-bit operation we use (add, sub, mul, shifts,
`countLeadingZeros`, `abs`, `min`, `max`, bitcasts, conversions) was checked against Rust before
anything was built on it. naga's MSL backend casts signed operands to unsigned for `+ − ·` and wraps
`neg`/`abs`, so WGSL's wrapping semantics hold on Metal (C++ signed overflow would be UB).

## 3. Storage forms and layouts

WGSL has no 8- or 16-bit integers, so a tensor sits in one of six **forms** (`tensor.rs`). A form is a
storage decision, never a value decision.

| form | elements | holds |
| --- | --- | --- |
| `S32` | one signed word | computed `i8`/`i16`/`i32` values, sign-extended — the lane form commitments use |
| `U32` | one unsigned word | `idx` |
| `I64` | one `i64` | `i64`, and an `i128` node whose values the plan proved to fit `i64` (the CPU's `store`) |
| `I128` | two `u64` words, low first | computed `i128` values and `i128` consts |
| `P8` | four per word | `i8` params, as the artifact stores them |
| `P16` | two per word | `i16` params |

Layouts are the CPU executor's (`misaka_palw_tir_exec::layout::Layout`): `Reshape` of a contiguous
value, `Transpose`, `Slice`, `Broadcast`, a gather by a host-known scalar (the token, a const) and a
history window are views over their input's buffer, and every kernel reads its operands through
strides. Outputs are written whole lanes, never bytes, so no two invocations share a word.

## 4. The rule: the plan decides, not the backend

A kernel takes the CPU executor's **refined** `NodePlan` — `TirPlan::refine` with the actual ranges of
the params the executor holds — and computes under exactly that plan's proof. The proof comes from spec
04b §7's transfer functions.

| the plan says | the device does |
| --- | --- |
| `work = I64` (every operand and exact result inside `i64`) | wrapping `i64` per element |
| `work = I128` | wrapping 128-bit (`W`) per element, unless `checked_arith` |
| `checked_arith` (the result may not fit `i128`) | **not run**: the CPU kernel |
| `check_out` (the result may leave the declared dtype) | compare each element with the dtype's bounds; `Overflow` |
| `check_operand` (a `Gather` index may leave its axis; a divisor may be below 1) | check before the data load; `Index` / `Divisor` |
| `Acc::Fast64` (every partial sum of every order inside `i64` and the dtype) | any association in wrapping `i64`; `i32` chunks when the operand intervals allow (below) |
| `Acc::Pn64` (partials inside `i64`, maybe not the dtype) | `P` and `N` apart in `i64`, then `P ≤ max`, `N ≥ min` |
| `Acc::Fast128` | any association in `W` |
| `Acc::Pn128` (nothing proved) | `P` and `N` in `W`, each partial checked against the dtype's bound as it grows (this is the CPU's checked accumulation, and it also catches a sum past `i128`) |
| `identity` (a `Clamp` that cannot fire, a `Cast` that cannot fail) | the input itself, when it is already held in the node's storage form |

**The ring argument.** Wrapping add, sub and mul on `w`-bit words are operations of the ring
`ℤ/2^w`. If the true result of a whole sum lies inside the `w`-bit two's complement range, then any
association, order or grouping of wrapping operations — including partials that wrap on the way —
yields exactly that result. `Fast64`/`Fast128` are exactly the statement that it does. So the device
may tile, split K, reduce in a tree or across a workgroup in any order. Order-freedom needs no further
argument.

**The `i32` chunk.** A GPU's fast path is 32-bit. With operand intervals `[a.lo, a.hi]`, `[b.lo, b.hi]`
inside `i32` and `T = max |corner products| ≤ 2^31 − 1`, a partial of `c = ⌊(2^31 − 1)/T⌋` terms is
inside `i32` (`|partial| ≤ c·T`), so its wrapped `i32` value is exact. It is then widened and
accumulated in `i64` (or `W`). `c` comes from the plan's intervals, i.e. the actual weight range of the
artifact, not from the dtypes: for `i8` weights in `[−127, 127]` against `i16` codes `c = 516`. The
GEMV widens every ⌊c/16⌋ loads of sixteen terms, the GEMM every ⌊c/32⌋ K-tiles of 32. For `i16 × i16`
(`c = 2`) the terms go straight to `i64`.

**Failures** are reported as the CPU executor reports them. A failing element records
`atomicMax(S[slot], ~(index·4 + class))` (classes 1 `Overflow`, 2 `Divisor`, 3 `Index`), so the
node's status word ends holding the **first failing element in output order** — the element whose
class the reference (`collect` stops at the first error) and the CPU kernels report. Across a step,
the first failing slot is the step's error. A node that fails cannot leave the next nodes reading out
of bounds: storage access is bounds-checked, and a failed step's values are discarded (spec 04b §9.1:
effects only after the whole step succeeded).

## 5. Exactness, primitive by primitive

"Exact by construction" means the device computes the same mathematical function with no rounding
or range question. "Needed care" names what the transcription had to get right, and the test that
holds it (§7).

| primitive | exact by construction | needed care |
| --- | --- | --- |
| Reshape, Transpose, Slice, Broadcast | views; a materialisation is a strided copy | element offsets `< 2^32` checked on the host for every operand's extent |
| Concat | strided copies into regions of the output | — |
| Iota | `start + step·i` in `i64` when the interval fits, else in `W` (`|step·i| ≤ 2^87`) | the dtype check where the plan keeps it |
| Gather | an index map; a scalar host-known index is a view | the index check BEFORE the data load (a garbage index is never dereferenced); `batch_dims` per output dim |
| Cast | a copy | the dtype check (`check_out`) |
| Add, Sub, Mul | ring ops in the plan's width | `checked_arith` stays on the CPU |
| MatMul | ring sums (§4); GEMV, tiled GEMM, general, split-K | the `i32` chunk bound from operand intervals; split-K partials are subset sums (inside `i64` by the proof); `Pn` checks on the whole `P` and `N` (narrow) or incrementally (`W`); 128-bit terms are exact products of two `i64` (64×64 from four 32×32 partials) |
| ReduceSum | as MatMul | as MatMul; a workgroup tree for long axes |
| ReduceMax | `max` is exact in any order | — |
| **Div** | the three rules on the quotient and the remainder, never a nudged shift | **rounding shifts:** a power-of-two divisor is a shift; `HalfUp` adds bit `s−1` of `x`; `HalfAwayFromZero` rounds the MAGNITUDE (`|i64::MIN| = 2^63` held unsigned) and negates; the tie test is `r ≥ d − r` (never `2r ≥ d`, which overflows); **no 64-bit `/`**: `udiv64` is a 32-bit hardware division when both operands fit, else restoring long division (`d ≥ 2^63` short-cut); the floor of a negative dividend is `−⌊(|x|−1)/d⌋ − 1`; the remainder `x − q·d` is exact in the ring because it lies in `[0, d)`; the same in 128 bits (`w_div_rule`) |
| **Clamp, StateWrite** | `min(max(x, lo), hi)` | **saturation:** bounds as `i64`/`W`, in the node's working width (the CPU saturates its bounds into its own width the same way) |
| Log2Floor | `63 − clz` / `127 − clz` | `x ≤ 0 → −1` (no `firstLeadingBit(0)` corner) |
| **IntExp, IntRsqrt, IntLn** | the fixed-iteration algorithms of 04b §6.5, transcribed from `scalar.rs` | every intermediate's bound restated (`t² < 2^50`, Newton values `< 2^26`, products `< 2^56`); 32-bit divisions only where both operands are proved non-negative and `< 2^32` (`−x/LN2_Q`, the seed index, the series' `term/d`); the one wide division (`IntLn`'s `t`) through `udiv64`; `IntRsqrt`'s normalising loops bounded (40 iterations; they run at most 31); no `select` over a shift (both arms are evaluated) |
| Compare, Select | exact | in 128 bits: signed compare on two words |
| **TopK** | the DEFINITION, counted: element `t` is kept iff fewer than `k` elements beat it under (value descending, index ascending); kept indices written in ascending order by a per-row pass | **tie-break:** no sort, so no unstable order can move a tie; the second pass makes the order the index order |
| HistAppend | a copy of the committed row into the history buffer; the window is a view | the buffer grows or compacts by copying live rows to a fresh buffer (never an overlapping copy) |

Two more things needed care everywhere:

- **Shifts.** WGSL takes a shift amount modulo the width, so a shift by 64 is a shift by 0. Every shift
  amount is a value proved in `[0, w)`, and 128-bit shifts branch at 64.
- **Packed loads.** `extractBits` on a signed word sign-extends the byte or half-word. Nothing is ever
  written narrower than a word.

## 6. Kernels and measured speed

Kernels (`wgsl.rs`, generated per operand forms and attributes, cached per source):

- **elementwise**, one element per invocation over broadcast strides, `i64` or 128-bit;
- **GEMV vec4**: 32 invocations per four packed `i8` rows. Each step loads sixteen weights of each row
  (one `vec4<u32>`) and sixteen activation lanes once for the four rows. `K % 16 = 0`, rows 16-aligned,
  activations as `S32` lanes;
- **GEMM**: 64×64 tiles, 16×16 invocations each holding a 4×4 block in named registers (sixteen `i32`
  partials, sixteen `i64` totals), a K tile of 32 staged as `i32`, packed weights staged a word at a time;
- **general MatMul**: any strides and forms, one output per invocation, or **split-K** (256-term slices
  plus a second pass) for long contractions with few outputs — the values of an attention over a long
  history;
- **reductions**: one invocation, or one workgroup tree for axes ≥ 512;
- **TopK**: rank, then emit;
- **ragged variants** for a batch of positions (§11): the elementwise kernels, the reductions and the
  general MatMul mask a padded history axis — `wgsl::RAGGED_MASK`, `ReduceKey.ragged`,
  `MatMulKey.rag` — and are otherwise the same source.

Measured with `tir-gpu-bench` on the M1 Max. Every device result was byte-compared with the CPU
executor's kernel before it was timed. Weights were resident and the operands synthetic, at the real
shapes. **The host was shared**: load average 75–98 (other lanes' builds and tests, and a 9-node
local drill), and the CPU kernel ran on 3 rayon threads. CPU figures are therefore the low end, and
the first, least-loaded run's are quoted beside them. Device figures varied by about ±30 % between runs.

| shape (one dispatch unless noted) | device | CPU kernel (3 threads, loaded; first run) |
| --- | --- | --- |
| decode GEMV `[8960, 1536]·[1536]` (gate/up) | 0.21–0.41 ms (34–64 GB/s of weights) | 1.7–5.3 ms |
| decode GEMV `[1536, 8960]·[8960]` (down) | 0.12–0.20 ms (70–115 GB/s) | 1.1–9.4 ms |
| the seven projections of one 1.5B layer | 1.46 ms | 5.7–33 ms |
| LM head `[151936, 1536]·[1536]` | 1.5–2.8 ms (84–153 GB/s) | 8.1 ms (29 GMAC/s, first run) – 33 ms |
| MoE expert `[768, 2048]·[2048]`, `[2048, 768]·[768]` | 0.11–0.21 ms, 0.08–0.15 ms | 0.5–7.7 ms |
| **a batch of 16 positions** `[8960, 1536]·[1536, 16]` | 2.6–3.3 ms (67–85 GMAC/s) | 36–62 ms |
| **64 positions** | 3.2–3.7 ms (237–275 GMAC/s) | 61–123 ms |
| **256 positions** | 9.2–9.3 ms (**379–384 GMAC/s**) | 210–327 ms (×30–36) |
| `[1536, 8960]·[8960, 256]` | 10.0–11.2 ms (315–352 GMAC/s) | 167–346 ms |
| MoE expert over 256 positions `[768, 2048]·[2048, 256]` | 1.9–2.3 ms (172–216 GMAC/s) | 22–98 ms |
| attention of one position, kv 2 × 6 groups × 128, H = 4,096 | 0.98–1.1 ms (scores + values; values split-K) | 2.1–12.3 ms |
| attention, H = 32,768 | 4.1 ms | 7.6–37.6 ms |
| elementwise sites over 151,936: `Mul`, `Div` by `2^s`, `Clamp`, `IntExp`, `IntRsqrt` | 0.75–1.35 ns/element | 3.6–19.5 ns/element |
| `Div` by an odd divisor (long division, dividends past `2^32`) | 2.2–6.2 ns/element | 2.5–38.9 ns/element |

**Whole programs** (`tir-gpu-layers`: `tir-exec-bench`'s Qwen2.5-1.5B-shaped program — the dense
lowering's conventions, synthetic weights of the real shapes — over 2 or 4 layers; the logits were
equal at every position before anything was timed):

| | 2 layers | 4 layers |
| --- | --- | --- |
| nodes per position | 543 | 1,027 |
| device nodes / views / CPU fallbacks per position | 471 / 72 / 0 | 889 / 138 / 0 |
| dispatches per position | 502 | 948 |
| median step, CPU executor vs device executor | 50.8 vs 50.6 ms | 86.6 vs 86.6 ms, then 220 vs 110 ms (load 89) |
| of the device step: recording on the host / submission to readback | — | 30 ms / 112 ms |

**Reading.**

- **A batch of positions is where the device wins**, by an order of magnitude or more. That is the
  shape of verification: a seat replaying a committed job holds every token, and a layer shard
  verifying from committed boundary rows (RFC-0006) holds every position of its layers at once.
- **Decode at batch 1 is dispatch-bound.** One primitive node is one dispatch. A 16-element dispatch
  costs 22 µs of host recording, and its share of submission-to-readback GROWS with the dispatches in
  one submission: 55 µs each at 100, 168 µs each at 1,000. That is wgpu-core's per-submission tracking
  of a fresh parameter buffer and bind group per dispatch, so caching them is the first fix. At this granularity the device step is at
  parity with the CPU executor (which needs 55–68 ms a token for the whole 1.5B on this Mac,
  `b966a4b29`). The remedies are the ones RFC-0002 §7 already plans, in order:
  1. fused kernels for the hot chains — the A16 narrowing (`Mul → Div_HAFZ(2^s) → Clamp`) into the
     producer's epilogue, the wide RMS norm, the two-pass softmax — matched on the canonical program
     as `fused/` already matches the gated delta step;
  2. bind groups and parameter words cached per (occurrence, node, H) across steps (shapes are
     static, except `H`);
  3. fewer passes, and subgroup reductions.

  None of these changes a byte; each is held by the same gate (§7).
- **The bigger lever: batch positions into each dispatch** (built since: §11). A seat's replay is one forward, a layer at a
  time over all positions (ADR-0117 "a draw is one forward"; `forward_prefill_planned` in
  `misaka-palw-base0` does exactly this for the legacy engines). If the device executor evaluates each
  occurrence for ALL positions of a job before the next, dispatches per claim scale with
  **nodes × layers** instead of **nodes × layers × positions**. Today that is 948 dispatches per position
  at four 1.5B layers; for a 512-position job it would be the same 948 for the whole job, 512 times
  fewer. Every projection also becomes the GEMM this section measured at 13–43× the CPU kernel. The
  per-position semantics are unchanged: a layer's carry-in at position `p` is its predecessor's
  carry-out at `p`, and its `Hist` rows are appended in position order. What it needs:
  1. a **batched causal attention** kernel — the scores and values of position `p` read history rows
     `0..=p` only, through the two-pass softmax (max, then exact sums), every lossy site per position;
  2. `Fixed`-state recurrences (the GDN, Mamba and RWKV scans) stepped in position order inside the
     layer pass. They are sequential in `p` but cheap next to the projections, and a chunked-parallel
     form is a fused kernel under F-2.

  This is also exactly the shape of a **layer shard's verification from committed boundary rows**
  (RFC-0006): there every position's carry-in and every earlier history row is a committed leaf, so
  even the recurrence's carry between positions is opened rather than recomputed at the segment's
  first position.
- **Before i128 support** every wide-norm node fell back: 118 synchronisations a position, and the
  step was 4× slower than the CPU. With i128 on the device there are none.

## 7. Conformance — the F-4 gate for this backend

All of it runs with `cargo test --release` in `misaka-palw-tir-gpu/` (≈ 2 minutes on the M1 Max). It
is meant to run on every device class a node deploys on, because naga, the driver and the GPU
compiler are part of the computation's trusted base.

| suite | what | result (M1 Max, Metal) |
| --- | --- | --- |
| `intlib` | the WGSL library vs `misaka_palw_tir::arith` | **IntExp on all 360,501,485 inputs of `(−31·LN2_Q − 2) ..= 2`**; Log2Floor/IntExp/IntRsqrt/IntLn on 200,870 samples (rails, every power of two ± 2, every bucket edge, 200,000 random); `Div` under all three rules on 3,175,386 pairs through every path; the first-failing-element rule |
| `intlib128` | the 128-bit library vs Rust `i128` and `arith::div_round` | `+ − ·`, compare, select, clamp, log2 on 21,279 samples; `Div` under all three rules on 993,050 pairs each (divisors up to `2^127 − 1`, shifts past 64, exact halves past 64 bits) |
| `vectors` | the golden primitive vectors (`consensus-vectors/tir-v1/primitives/`) | 106 program-node cases on the device, 16 of them failing with the vector's class; 32 cases are not program nodes (type errors, `i128` params) |
| `random` | 5,700 random primitive applications as one-node programs: reference, CPU executor and device under the CPU's refined plan, operands at random in param (packed) or computed (lane) form | every case on the device equal byte for byte (or failing with the same class): 5,472 device runs (the other 228 are not program nodes), 1,106 of them failures; no fallback in any family |
| `fast_paths` | the vec4 GEMV and the GEMM at every row-group, tile and chunk-flush boundary, batches, the rails | 131 cases equal |
| `batch` | `BatchReplay` (§11) vs `TirExecutor` stepping the same tokens until the first failure: how many positions succeed, the failure's class, every value of every successful position (every node, and the staged-lanes path) and the logits | **the seven program vectors** at chunks of 1, 2, 3, 5 and the whole job (chunk boundaries before, inside and after a window's first slide), both sink modes: 340 positions equal and equal to the vectors, Fixed-state occurrences per position and the rest batched, 0 fallbacks; **400 random programs** (`TIR_GPU_BATCH_PROGRAMS`; 3,000 run once: see §11) at random chunks of 1–64; **the Qwen2.5-shaped program** at a small geometry over 40 positions in chunks of 7, 16 and 40, every occurrence batched, 0 fallbacks |
| `programs` | `GpuExecutor` vs `TirExecutor`, every node of every position ("every node" sink) and every commit point (the staged-lanes path) | **the seven program vectors** (dense GQA, sliding + global, GDN 2 key / 4 value heads, Mamba-2, top-2 MoE with a shared expert, Fixed-state saturation, a 3-row history window): equal to the CPU executor and to the vectors' committed values and logits, 15,738 device node evaluations, 0 fallbacks; **400 random programs** of the CPU executor's own generator (`misaka-palw-tir-exec/tests/common/progen.rs`, included by path): 725 steps equal, 1,015 fail in both with the same class, 40,287 device node evaluations, 60 checked-`i128` nodes on the CPU |

## 8. The dispatch seam `misaka-palw-tir-exec` would need (specified, not implemented)

`GpuExecutor` (`misaka-palw-tir-gpu/src/exec.rs`) is today a second executor that mirrors
`TirExecutor`. To make the GPU a backend of the ONE executor a node runs, `misaka-palw-tir-exec` would
grow this seam. It is node software: nothing in it reaches consensus.

```rust
/// A place values live and kernels run (RFC-0002 §7 F-6). The CPU executor is the trivial backend.
pub trait KernelBackendV1 {
    type Value: Clone;                                   // storage + Layout (a view is a Value too)
    fn name(&self) -> String;                            // "wgpu/metal Apple M1 Max" — logs and the gate
    /// Hold a param instance or const for the executor's life (uploaded once).
    fn hold(&mut self, data: Slice<'_>, shape: &[usize]) -> Result<Self::Value, Refused>;
    /// The structural views: Reshape of a contiguous value, Transpose, Slice, Broadcast, a gather by a
    /// host-known scalar, a history window.
    fn view(&self, v: &Self::Value, layout: Layout) -> Self::Value;
    /// Record one computing node under `plan` — the CPU executor's refined NodePlan, never a weaker
    /// one — or refuse it. Recording never blocks.
    fn record(&mut self, plan: &NodePlan, ins: &[&Self::Value], out_shape: &[usize], slot: u32)
        -> Result<Self::Value, Unsupported>;
    /// The state primitives write the backend's run state (a Fixed instance's pending value; a
    /// history row into the window).
    fn state_write(&mut self, plan: &NodePlan, inst: u32, x: &Self::Value, slot: u32) -> Result<Self::Value, Unsupported>;
    fn hist_append(&mut self, inst: u32, row: &Self::Value, pos: u32, slot: u32) -> TirResult<Self::Value>;
    /// Stage a commit point's lanes for one readback per step.
    fn stage_commit(&mut self, v: &Self::Value, slot: u32) -> CommitHandle;
    /// Wait for everything recorded; the first failing slot and its class.
    fn sync(&mut self) -> Option<(u32, TirError)>;
    /// A value on the host (staged lanes after `sync`; a fallback's operands).
    fn read(&mut self, v: &Self::Value) -> Buf;
    /// Apply (ok) or drop (failed) the step's effects: Fixed swaps, history commits.
    fn end_step(&mut self, ok: bool);
    /// Bytes held now (params, state, histories, working set) — what the ledger reserved.
    fn resident_bytes(&self) -> u64;
}
```

`TirExecutor::step` keeps every rule it has. Occurrences run in schedule order and nodes in index
order. A view is `backend.view`. A computing node is `backend.record`. On `Unsupported`, the executor
first calls `sync` (an earlier failure is the step's error, because the CPU would have stopped there),
reads the operands, runs its own kernel and `hold`s the result. At the end of the step: `sync`, read the
staged commits, deliver them to the sink in slot order, then `end_step(ok)`. Rules for any backend:

- **B-1.** A backend runs a node only under the plan's proof (`work`, `acc`, the checks). Refusing is
  always allowed and always correct.
- **B-2.** Every value a commit point, a carry or the logits holds is byte-identical (lanes).
- **B-3.** A failure is the first failing slot, and within it the first failing element's class.
- **B-4.** Effects apply only after the whole step succeeded.
- **B-5.** The F-4 gate (§7) passes on the device it runs on. A node runs a quick subset at startup (the
  intlib samples, the golden vectors, two program vectors: a few seconds) and refuses the backend on
  any difference. The full gate runs per device class in CI.

A fused GPU kernel is one more `FusedKernelV1::run` over the same operands (`fused/mod.rs` already
says so). It is matched on the canonical program and enabled only where its region's refined plan has
no check left. The same gate holds it.

## 9. The memory ledger's device pool

`kaspad/src/palw_memory_ledger.rs` already has `PalwMemoryPoolV1::Device(u8)`, `arm_device_share_v1`
and `device_ledger_v1`, "a device's memory when a GPU backend arrives". How a backend uses it:

- **Arming.** At backend start, `arm_device_share_v1(index, bytes)`. `bytes` is the operator's
  declared device budget, or failing that the device's own figure less a margin — Metal's
  `recommendedMaxWorkingSetSize`, or Vulkan's device-local heaps. These are reached through wgpu-hal;
  wgpu 30 does not expose them directly.
- **Unified memory (Apple silicon, integrated GPUs).** Device bytes ARE host bytes. Arming a separate
  device pool would let the host pool and the device pool each grant the same gigabytes. So on a
  unified-memory adapter (`DeviceType::IntegratedGpu`, or Metal's `hasUnifiedMemory`) **no device pool
  is armed**, and the backend's reservations are taken in the Host pool under a role of their own
  (`"device:<role>"`). A discrete GPU arms `Device(i)`. A duty there holds two reservations: its
  device bytes there and its host bytes (staging, the CPU fallback's working set) in Host, under one
  RAII guard that releases both.
- **What a duty reserves on the device** (`PalwRoleMemoryNeedV1` gains a device half):
  - the params it holds, in their packed forms (`i8`/`i16` packed: the artifact's own bytes);
  - the state and histories, as **lanes**: an `i16` K/V cache costs twice its host bytes on the device
    today. A packed history form (`P16` rows) would remove that, and is the first memory optimisation
    to make;
  - the step's working set: the peak live bytes of spec 04b §8, in device forms, plus the staged
    commit lanes;
  - nothing for pipelines (kilobytes).
- **Residency (lane M2) and layer shards (RFC-0006).** A tiered-residency backend holds a resident
  set of layers on the device and streams the rest. A layer-sharded seat (RFC-0006) holds only its
  shard's layers and histories. Either way the device reservation is the resident set's bytes, not
  the model's. This is what makes a 24 GiB-class device useful for a model that does not fit it.

## 10. Limits and open questions

1. **The trusted base grows.** naga, the Metal/Vulkan driver and the GPU compiler now compute
   consensus-relevant values on a node that uses the backend. The defence is §7 on every device class,
   plus the startup self-test. A node whose backend differs convicts only itself (the court runs the
   reference), never the network.
2. **Checked `i128` arithmetic** (a result that may not fit `i128`) stays on the CPU. It is rare in
   real programs: 60 node evaluations in 400 random programs, none in the corpus programs measured.
3. **A param larger than one binding (4 GiB) or with more than `2^32` elements** is held on the host and
   its consumers fall back. Splitting a tensor across bindings is mechanical, not done.
4. **Step speed.** Position batching is built (§11): dispatches per job from nodes × layers ×
   positions down to nodes × layers × chunks. What remains is per-dispatch work — the fused chains
   (the LM head's narrowing over the whole vocabulary is now most of a replay's device time) — and
   decode at batch 1, which batching does not touch. The same batched block evaluation, fed committed
   carry-ins and committed history rows instead of recomputing them, is a layer shard's verification
   (RFC-0006).
5. **Not measured:** Vulkan on NVIDIA. The code has no Metal-specific path. The gate must run there
   before anyone relies on it.

## 11. Position-batched replay — a job, a layer at a time over all its positions (built)

`BatchReplay` (`misaka-palw-tir-gpu/src/batch.rs`) is §6's bigger lever. A seat replaying a committed
job — a claim's prefill-and-decode run, a court's step leg, RFC-0006's cell over a position segment —
holds every token before it starts. So it runs the occurrences in schedule order, and each occurrence
ONCE over a chunk of positions: ADR-0117's "a draw is one forward", what `forward_prefill_planned`
in `misaka-palw-base0` does for the legacy engines. The per-position semantics are unchanged: a
layer's carry-in at position `p` is its predecessor's carry-out at `p`, and history rows are appended
in position order.

**The batched form.** A value whose per-position shape is `S` is one tensor `[T] ++ S` for the chunk's
`T` positions. A value that depends on no position (params, consts and what is computed from them
alone) is shared: computed once per replay, as `S`. A history axis `H` is padded to the chunk's
largest, `H_max = min(p0 + T, W)`. Position `p`'s elements past `H_p = min(p + 1, W)` are padding.

**Why padding never reaches a valid element.**

- Every kernel whose output has `H` masks the padding before any load: stored 0, never read, never
  failing. This is `wgsl::RAGGED_MASK`: `P[60]` names the `H` axis, `P[61]` the position axis, `P[62]`
  is `W` and `P[63]` the chunk's first position.
- A reduction over `H` stops at `H_p` (`ReduceKey.ragged = 1`). A `MatMul` that contracts `H` sums
  `H_p` terms (`MatMulKey.rag` bit 4). A `MatMul` whose `M` or `N` is `H` masks those rows or columns
  (bits 1 and 2).
- Nothing else can read padding. By the type rules (`validate.rs`), `H` never broadcasts against a
  constant; it is never sliced, concatenated, gathered or ranked along; a reshape keeps it a factor
  with the same row-major split on both sides (`reshape_compatible`); and a value loses `H` only
  through a reduction or a contraction over it.
- So each valid element is computed from exactly the operands the per-position executor gives it,
  under the same refined plan. The fast kernels (GEMV, GEMM, split-K) are not ragged; they run only
  where no `H` is involved.

**Failures, in the CPU executor's order.** A batched node's status word holds its first failing
element in `[T] ++ S` order. That order is position-major, so the element's quotient by the
per-position count is its position. Within the position the row-major order is the CPU's, because
padding never fails. A per-position node (below) has one status word per position. The replay's
failure is the least `(position, slot)` over every word and every host-side check (the step checks of
spec 04b §9.1 first). Values reach the sink only for the positions before it, in the order
`TirExecutor` hands them to its sink.

**Causal attention needs no new primitive.**

1. `HistAppend` writes the chunk's rows into the instance's row store, `[positions] ++ row`.
2. Every position's window `[H, ..row]` is one view of that store:
   - stride 0 along the positions while every position still sees every row from the first
     (`p0 + T ≤ W`), each position's later rows being padding;
   - stride one row once every window is full (`p0 ≥ W − 1`);
   - in the one chunk where the windows start to slide, a gather by host-built row indices.
3. The scores `q·kᵀ` are then one batched `MatMul` over `[T, kv]` whose `N` is `H_max`, masked.
4. The two-pass softmax reduces over `H_p`, and the values `p·v` contract over `H_p`.

**Projections fold into one product.** `W[M, K]·x[K, 1]` at every position is `W·X` with `X[K, T]` a
view of the batched `x`: one register-blocked GEMM (`gemm:p8`, §6). Its `[M, T]` result is viewed as
`[T, M, 1]`. Its elements are not position-major, so the fold is taken only where the plan proves the
sum cannot fail (`Acc::Fast64`/`Fast128`). `x[1, K]·W[K, N]` folds into `X[T, K]·W` whatever the plan,
since that result is position-major.

**What stays per position.**

- An occurrence that writes a `Fixed` state (a GDN or Mamba-2 scan, a counter) runs position by
  position, still inside the chunk's recording. Every state value lives in a row store
  `[positions + 1] ++ shape`, row `p` the value at the START of position `p`. So an occurrence that
  only READS a state (`post` reading a global state `pre` wrote) batches again.
- An occurrence whose batched values would pass rank 4 runs per position too.
- A node the device does not run (checked `i128` arithmetic) runs on the CPU executor's kernel,
  position by position, on each position's valid elements. A synchronisation first finds the earliest
  failure so far, and the CPU kernel is handed only positions before it. It therefore only ever sees
  operands the CPU executor would have handed it.
- A program the batched form cannot hold at all is replayed by `GpuExecutor`, position by position:
  a param past one binding, a rank-4 carry, or a value past one binding at a single position.

**Memory.** A chunk's values are on the device together. Each occurrence is one submission, waited
for, so the peak is about one occurrence's values. The chunk size bounds it: the LM head's `i64`
products alone are `vocab × chunk × 8` bytes. Peak process footprint, which also counts the CPU
executor's and the device's copies of the 0.56 GiB of params: 4.3 GB at chunks of 256, 6.3 GB at 512.

### Conformance (`tests/batch.rs`)

| programs | result (M1 Max, Metal) |
| --- | --- |
| the seven program vectors, chunks of 1, 2, 3, 5 and the whole job, every-node and commit sinks | 70 replays, 340 positions equal to the CPU executor and to the vectors' commits and logits. Occurrences: dense GQA, sliding + global, top-2 MoE and the 3-row window all batched; GDN, Mamba-2 and the saturation program per position in their state-writing layers. No fallback, no sequential replay |
| 400 random programs of the CPU executor's generator, random chunks of 1–64, every third in every-node mode | 1,011 positions equal; 249 replays fail at the same position with the same class; 94 replays span more than one chunk; 1,958 occurrence-chunks batched, 788 per position; 17 checked-`i128` nodes on the CPU kernel; 15 replays by `GpuExecutor` (a rank-4 carry) |
| the same generator, 3,000 programs (`TIR_GPU_BATCH_PROGRAMS=3000`), once | 8,052 positions equal; 1,780 replays fail alike; 723 span more than one chunk; 16,662 occurrence-chunks batched, 5,160 per position; 192 checked-`i128` nodes on the CPU kernel; 105 replays by `GpuExecutor` (a rank-4 carry). Seed 2525 found a defect shared with `GpuExecutor`: a `TopK` over a transposed `i8` param was materialised in the packed form, which no kernel writes. It is fixed and the seed is run every time |
| the Qwen2.5-shaped program at `d` 64 (2 layers, 4 heads, 2 KV heads, `ff` 96, vocab 384), 40 positions, chunks of 7, 16 and 40, both sinks | equal at every position; every occurrence batched, no fallback; 520 dispatches for the whole job at chunk 40 (13 a position) against the step executor's 502 a position |

### Measured (`tir-gpu-replay`)

The Qwen2.5-1.5B-shaped program over 2 layers, with synthetic weights of the real shapes
(`tir-gpu-layers`' program). The CPU executor steps the longest job once; a shorter job's time is its
prefix. Every position's committed values and logits were compared (a digest per position, in sink
order) before anything was printed. The host was shared (load average 75–85), and the CPU executor
ran on 3 rayon threads. The two runs below differ mostly in how loaded the CPU was.

| positions | CPU executor | device step executor (51.5 ms median × positions) | batched replay, chunks of 256 | × CPU | × step |
| --- | --- | --- | --- | --- | --- |
| 256 | 10.2 s | 13.2 s | 0.94 s | 11× | 14× |
| 512 | 25.0 s | 26.4 s | 2.25 s | 11× | 12× |
| 1,024 | 62.1 s | 52.7 s | 3.20 s | 19× | 16× |

| positions | CPU executor (more loaded) | batched replay, chunks of 512 | × CPU |
| --- | --- | --- | --- |
| 256 | 41.9 s | 0.87 s | 48× |
| 512 | 80.9 s | 1.80 s | 45× |
| 1,024 | 109.7 s | 4.23 s | 26× |

- **Dispatches.** 520 for a whole chunk at 2 layers (543 nodes per position), against 502 per
  position for the step executor: 1.0–2.0 a position.
- **Where the device time goes** (1,024 positions, chunks of 512): `pre` 0.08 s, each layer 0.33 s,
  `post` 1.84 s. The LM head's GEMM over the 151,936-row vocabulary is about 0.7 s of `post`. Its
  per-channel narrowing over `[T, 151,936]` — three elementwise passes (`Mul`, `Div` by `2^s`,
  `Clamp`) and the commit's staging — is most of the rest. Handing the committed values to the sink is 0.25 s per 256 positions: 224 K lanes per
  position, digested on both sides.
- **The CPU executor's attention grows with the history** (10.2 → 25.0 → 62.1 s for ×2 positions
  each time). The batched replay's cost per position stays about flat (3.7, 4.4, 3.1 ms), because its
  time is the projections and the LM head.

**Next, in order:**

1. the narrowing epilogue fused into the GEMM's store (F-2: `Mul → Div_HAFZ(2^s) → Clamp` per channel),
   which removes the passes that dominate `post`;
2. a transposed GEMM store, so that the folded projection is position-major in memory and its
   consumers read it coalesced, and so that a `Pn64` projection can fold too;
3. packed history rows (`P16`, §9);
4. a chunked-parallel form of the `Fixed`-state recurrences, as a fused kernel, so that GDN and
   Mamba-2 layers batch as well.

None of these changes a byte, and each is held by the same `batch` suite.
