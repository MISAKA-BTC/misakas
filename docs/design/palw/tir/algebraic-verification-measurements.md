# Algebraic verification of PALW-TIR executions — measurements (RFC-0007 Part II, CP3)

Status: measured 2026-10-01 on branch `rfc7/algebraic` (lane M4), crate `misaka-palw-tir-sketch`.
Reproduce with:

```text
cargo run --release -p misaka-palw-tir-sketch --features measure --bin tir-sketch-measure
cargo run --release -p misaka-palw-tir-sketch --features measure --bin tir-sketch-measure -- --sites Qwen2.5-1.5B
```

The tool lowers real `config.json` files (`misaka-palw-tir-lower/tests/configs/real/`) to PALW-TIR
programs **without weights**. It then runs the cost model (`misaka_palw_tir_sketch::cost`) on those
programs and times the kernels on this machine. No weight of any model is read or written. The
kernels run on synthetic buffers of 512 MiB at most.

## 0. The answer

**Algebraic verification pays for decode on a seat that does not hold the class in RAM. It never
pays for prefill on ordinary links. For a seat that does hold the class in RAM, it pays only above
about 1–2 Gbps.** In detail:

- **The check itself is cheap.** A weight product is checked against its secret sketch in
  `|out| + rows·K` field operations instead of `|out|·K` multiply-accumulates. The `[8960, 1536]`
  MLP projection costs 1.0 ms to recompute and 8 µs to check (§2). The sketch store is **0.3–1.2 %
  of the weights it stands for**. A seat holding 1.4 GB plus 0.9 GB of sketches checks Qwen3-Next-80B
  (80.7 GB). A seat holding 3.6 GB plus 2.1 GB checks DeepSeek-V3 (674 GB).
- **The cost moves into bytes.** The seat must obtain every served accumulator: **6.7 MB per
  decode token for Qwen2.5-1.5B, 17–20 MB for the 30–80B MoE classes, 138 MB for DeepSeek-V3**
  (raw 8-byte values; 3.9 / 9–11 / 84 MB packed to their proven widths). That is 150–270× fewer
  bytes than the weights a recompute reads per token, but those bytes cross a network link, not a
  memory bus.
- **Break-even link speeds**, decode token at H = 1,024 (raw / packed):

  | class | weights in RAM | weights on NVMe (3 GB/s) |
  | --- | --- | --- |
  | Qwen2.5-1.5B | 1.44 / 0.83 Gbps | 0.09 / 0.05 Gbps |
  | Qwen3-30B-A3B | 2.16 / 1.21 Gbps | 0.13 / 0.07 Gbps |
  | Qwen3-Next-80B-A3B | 2.18 / 1.15 Gbps | 0.13 / 0.07 Gbps |
  | DeepSeek-V3 (671B) | 1.52 / 0.93 Gbps | 0.09 / 0.06 Gbps |
  | qwen4_exp-like (§7, analytic) | ≈ 2.6 / 1.4 Gbps | ≈ 0.16 / 0.08 Gbps |

  For a prefill job of 1,024 positions the break-even is **2.0–5.5 Gbps** even against an NVMe seat,
  because a batched recompute reads each weight once while the witness grows with every position.
- **Per link** (decode): at **1 Gbps** algebraic verification wins 5–10× for a seat whose weights
  stream from NVMe, and ranges from 2× slower to slightly faster (0.5–1.2×) for a RAM-resident seat.
  At **100 Mbps** it is roughly even against NVMe with raw values (0.7–1.1×) and wins 1.2–1.9× with
  packed ones; it loses against RAM. At **5 Mbps** it loses everywhere, by an order of magnitude or
  more (9–25×).
- **Long context does not rescue it.** At H = 32,768 the activation × activation work dominates.
  `Q·Kᵀ` must be recomputed: serving its scores costs 8 bytes for every `d` = 128 multiply-adds it
  saves, and that never pays. `P·V` is cheap to check with per-row history sketches, but halving the
  attention work is the whole gain (§5).
- **For testnet-12's binding classes** (Qwen2.5-class dense, the 8k class prefill-heavy) it does not
  pay. The verification-supply problem of lane P's report is better addressed by Part I (carriage,
  aggregation) and by scheduling. Part II's value is **feasibility**: seats can verify classes they
  cannot hold, which is how a qwen4_exp-sized class gets enough seats at all.

## 1. What is measured, and how

**Programs.** Each class is its Hugging Face `config.json`, parsed (`parse_config_str`), built into
the HL graph and lowered by `lower::lower` with default options (`history_bound` 2^18). The programs
are exact at published shapes: blocks, nodes, operand shapes, dtypes and proven intervals. The
lowering's integer choices apply:

- `i8` weight codes for projections, with `i16` activation codes;
- a 16-column outlier product beside every A16 projection (`K = 16`);
- an `i16` tied head for Qwen2.5;
- MoE expert stacks gathered by the committed `TopK`.

**Costs** are counted per position at history length `H` by `tir_position_cost_v1` (crate docs):

- a **recompute** spends every `MatMul`'s multiply-accumulates (MACs) and reads every weight the
  position touches (a routed weight: only its gathered experts);
- an **algebraic check** spends each served `MatMul`'s check terms. These are `|out| + A-rows·K`,
  per modulus, plus the operand's sketch for a fresh vector: one history row per position with
  per-row history sketches;
- the check also recomputes exactly the products its policy does not serve, and the elementwise work
  of every other node it needs. **Views are not work.** `Reshape`, `Transpose`, `Slice`, `Broadcast`
  and the history window are strided views in the typed backend;
- the **elementwise work** (norms, narrowings, softmax, GDN state updates) is the same on both
  paths, and both times include it.

**Served bytes** are given raw (8 bytes per `i64` accumulator) and packed (`⌈log2(span + 1)⌉` bits of
the node's proven interval). The intervals assume type-worst-case weights, because the program was
lowered without weights. The packed figure is therefore an upper bound: refined intervals from real
weights pack tighter.

**Policies** (`TirCheckPolicyV1`):

- weight products with contraction `K ≥ 64` are served. The 16-column outlier products are
  recomputed from their weights, which a seat holds: their outputs would cost 8 bytes each to
  serve against 16 MACs to compute;
- activation × activation products follow one of three rules:
  - **R** — all recomputed;
  - **S** — all served, with per-row history sketches;
  - **S≥1024** — served when the contraction is ≥ 1,024. That serves `P·V` from a history of 1,024
    positions and always recomputes `Q·Kᵀ`, whose contraction is the head width.

  S≥1024 is the recommended policy and the default.

**The seat priced.** The seat is 8 cores at the one-thread rates of §2, RAM read at the one-thread
streaming rate, and an NVMe at 3 GB/s. The links are 1 Gbps, 100 Mbps and 5 Mbps. The machine is a
10-core, 32 GB Mac running six other agents and a 9-node drill: load average 45–90 during the runs.
All-thread rates measured under that load are contention, not capacity, so the model does not use
them.

## 2. Kernel rates on this machine

| kernel | one thread | all threads (contended) |
| --- | --- | --- |
| typed-backend GEMV `i8 [8960,1536]·i16 [1536,1] → i64` (the recompute) | 14.9 GMAC/s | 6.2 GMAC/s |
| Freivalds check, `P61` signed multiply-add (`dot_i64_bounded`, branchless) | 1.30 G terms/s | 0.33 G terms/s |
| sketch build, streamed `i8` rows (`S += v[r]·W[r, :]`) | 1.09 G weights/s | — |
| a narrowing (HAFZ + clamp), the elementwise proxy | 0.33 G elements/s | — |
| streaming read, 512 MiB | 48.1 GB/s | 20.5 GB/s |

Per product:

- The `[8960, 1536]` projection costs 13.8 M MACs, or 0.92 ms on one core.
- Its check costs 10.5 k terms, or 8 µs: 115× less.
- Building its sketch once per epoch costs 12.6 ms.
- A check term costs about 12× a MAC: it is a 64×64→128-bit multiply-add with a deferred Mersenne
  reduction, against the backend's vectorised `i8` dot product. That is why the check wins per
  product and not per term.

A first version of the check kernel split terms by sign and ran at 0.43 G terms/s. The
branchless signed form, sized by the operand's proven bit width, runs 3× faster. The seat knows
those widths from the refined plan.

## 3. The classes, and what a sketching seat holds

| class | params | held by a sketching seat | stood for by sketches | sketch store | weight sites served / needing ≥ 2 moduli | largest single served weight (escalation fetch) |
| --- | --- | --- | --- | --- | --- | --- |
| Qwen2.5-1.5B (dense, 28 L) | 1.84 GB | 525 MB | 1.31 GB | 10.5 MB (0.59 %) | 197 / 0 | 467 MB (the tied `i16` head) |
| Qwen3-30B-A3B (128 experts, top-8) | 31.1 GB | 871 MB | 30.2 GB | 249 MB (0.82 %) | 385 / 0 | 311 MB (the head) |
| Qwen3-Next-80B-A3B (512 experts, top-10, GDN hybrid) | 80.7 GB | 1.38 GB | 79.4 GB | 924 MB (1.16 %) | 697 / 0 | 311 MB (the head) |
| DeepSeek-V3 (256 experts, top-8, MLA) | 674 GB | 3.64 GB | 670 GB | 2.05 GB (0.31 %) | 843 / 0 | 927 MB (the head) |

- **Held** means what the exact half of the check reads: norms, narrowing multipliers, activation
  tables, RoPE tables, outlier columns and the token embedding. Qwen2.5's 525 MB is almost all the
  tied 467 MB embedding: the pre block gathers one row of it per position. A seat that receives that
  row with its inventory opening (a 3 KB row plus a ~20-level path) holds about 60 MB instead.
- **No weight site of these classes needs a second modulus.** Every `i64` accumulator's proven
  interval is below `2^61`. The wider rungs exist for wide fixed-point products, `i128` nodes and
  activations wider than `i32` (the tests' `wide_v1`).
- The **sketch store** is `(K + |free|)·8` bytes per weight site per expert. It is smallest for wide
  outputs and largest for narrow expert matrices (`K = 2048` against `N = 512`).

## 4. Decode: one token

Policy S≥1024. **Recompute** is `max(MACs ÷ 8 cores, weight bytes ÷ storage) + elementwise`.
**Check** is `terms + exact products + elementwise`. **Link** is the served bytes (raw) over the link.

| class | H | recompute MACs | weight bytes read | served raw (packed) | recompute, RAM | recompute, NVMe | check | link 1 Gbps / 100 Mbps / 5 Mbps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Qwen2.5-1.5B | 1,024 | 1.64 G | 1.82 GB | 6.7 MB (3.9) | 43 ms | 611 ms | 5.9 ms | 54 ms / 538 ms / 10.8 s |
| Qwen2.5-1.5B | 32,768 | 4.37 G | 1.82 GB | 6.7 MB (3.9) | 87 ms | 655 ms | 62 ms | 54 ms / 538 ms / 10.8 s |
| Qwen3-30B-A3B | 1,024 | 3.45 G | 3.07 GB | 16.6 MB (9.3) | 79 ms | 1.04 s | 17.5 ms | 133 ms / 1.33 s / 26.6 s |
| Qwen3-30B-A3B | 32,768 | 15.9 G | 3.07 GB | 16.6 MB (9.3) | 350 ms | 1.24 s | 276 ms | 133 ms / 1.33 s / 26.6 s |
| Qwen3-Next-80B-A3B | 1,024 | 3.71 G | 3.59 GB | 20.0 MB (10.5) | 159 ms | 1.28 s | 86 ms | 160 ms / 1.60 s / 32.0 s |
| Qwen3-Next-80B-A3B | 32,768 | 6.83 G | 3.59 GB | 20.0 MB (10.5) | 185 ms | 1.31 s | 125 ms | 160 ms / 1.60 s / 32.0 s |
| DeepSeek-V3 | 1,024 | 45.4 G | 36.8 GB | 138 MB (84) | 861 ms | 12.4 s | 138 ms | 1.10 s / 11.0 s / 220 s |
| DeepSeek-V3 | 32,768 | 315 G | 36.8 GB | 138 MB (84) | 4.22 s | 13.9 s | 2.84 s | 1.10 s / 11.0 s / 220 s |

How to read it:

- **A decode recompute is memory-bound.** Qwen2.5-1.5B computes in 14 ms but reads 1.82 GB. The
  "in RAM" column assumes the seat keeps the whole class resident: 80.7 GB for Qwen3-Next and
  674 GB for DeepSeek-V3. That is not a real seat. ADR-0112's budget is a fifth of the artifact, so
  for the large classes the NVMe column is the realistic one.
- Qwen3-Next's check time is mostly the GDN recurrence's elementwise state update: 226 M elements
  per token, shared with the recompute. A fused kernel (`fused/gdn_step.rs`) shrinks both paths
  alike.

## 5. Long-context attention: what to serve

H = 32,768, one decode token.

| class | policy | check terms | exact MACs (recomputed products) | served raw | check | link at 1 Gbps |
| --- | --- | --- | --- | --- | --- | --- |
| Qwen2.5-1.5B | R (recompute `Q·Kᵀ` and `P·V`) | 1.3 M | 2.83 G | 6.4 MB | 73 ms | 51 ms |
| Qwen2.5-1.5B | S (serve both) | 23.4 M | 10.3 M | 94.8 MB | 52 ms | 758 ms |
| Qwen2.5-1.5B | **S≥1024** (serve `P·V` only) | 12.4 M | 1.42 G | 6.7 MB | 62 ms | 54 ms |
| DeepSeek-V3 | R | 29.7 M | 278 G | 106 MB | 3.92 s | 846 ms |
| DeepSeek-V3 | S | 810 M | 44 M | 4.23 GB | 1.66 s | 33.9 s |
| DeepSeek-V3 | **S≥1024** | 290 M | 147 G | 138 MB | 2.84 s | 1.10 s |

- Serving `Q·Kᵀ`'s scores costs `8 × heads × H` bytes per layer: 88 MB per token for Qwen2.5 at
  32k. Each score saves only `d` = 128 MACs. **Scores are never worth serving.**
- `P·V`'s output is `heads × d` per layer, whatever `H` is. With per-row history sketches
  `S_V[h] = Σ_d σ[d]·V_h[d]`, each computed once when the row is appended, its check costs
  `heads × (H + d)` instead of `heads × H × d`. **`P·V` is always worth serving**: 0.3 MB of the
  witness, half the attention work removed.
- So long-context attention halves under algebra and no more: `Q·Kᵀ` stays a recompute.
  (Per-row history sketches are modelled here, not built in the prototype; see RFC-0007 §II.3.)

## 6. Prefill: a job of M positions

Policy S≥1024. **Recompute** reads every weight once for the batch.

| class | M | recompute MACs | served raw (packed) | recompute, RAM | check | link 1 Gbps / 100 Mbps |
| --- | --- | --- | --- | --- | --- | --- |
| Qwen2.5-1.5B | 512 | 0.7 T | 2.64 GB (1.41) | 7.4 s | 1.8 s | 21 s / 211 s |
| Qwen2.5-1.5B | 1,024 | 1.4 T | 5.29 GB (2.83) | 16.1 s | 4.9 s | 42 s / 423 s |
| Qwen3-30B-A3B | 1,024 | 3.0 T | 14.2 GB (7.4) | 36.8 s | 13.8 s | 113 s / 1,132 s |
| Qwen3-Next-80B-A3B | 1,024 | 3.4 T | 18.8 GB (9.8) | 114 s | 87 s | 151 s / 1,508 s |
| DeepSeek-V3 | 1,024 | 41.1 T | 107 GB (55) | 416 s | 113 s | 858 s / 8,581 s |

The witness grows by the decode figure for every position. A batched recompute amortises the weight
reads, so even a 674 GB class streamed once from NVMe (225 s) is cheaper than its 107 GB witness at
1 Gbps (858 s). **A prefill-heavy job is recomputed.** For the large classes, the cheaper route to a
long prompt is to check the prompt's KV rows by sampling or shards (RFC-0006), not by algebra.

## 7. qwen4_exp-like: analytic, from Qwen3-Next

The user's description of `qwen4_exp`:

- 48 layers × 512 experts, top-10, hidden 2,048, expert inter 512;
- ≈ 77 B in experts, plus a 51 B hashed n-gram table: "125 B";
- hyper-connections (4 streams, rank 320) and sparse block attention.

Its MoE geometry is Qwen3-Next-80B-A3B's exactly, so the figures start from Qwen3-Next's measured
ones (H = 1,024, S≥1024) and add:

| term | added per decode token | how |
| --- | --- | --- |
| hyper-connections | +0.82 M served values (+6.5 MB raw, ≈ +3.3 MB packed), +0.5 G MACs, +0.51 GB weights read | 48 × 2 halves × (down 320 + up 8,192 + inject 4) outputs; ≈ 10.5 M params per layer |
| n-gram table (51 G entries) | 16 rows per n-gram layer gathered; nothing to sketch (a `Gather`, not a `MatMul`) | the seat holds the table (51–102 GB) **or** receives each row with its inventory opening: 256 B + 29 × 64 B ≈ 2.1 KB, about 34 KB per n-gram layer per token |
| sparse block attention | small: an index projection and block-score products over committed blocks | not modelled |

| figure | qwen4_exp-like |
| --- | --- |
| params | ≈ 132–183 GB (80.7 + 0.5 + 51 at 1–2 bytes/entry) |
| held by a sketching seat | ≈ 1.4 GB, plus the table unless its rows are served |
| sketch store | ≈ 0.93 GB |
| served per decode token | ≈ 26.5 MB raw (≈ 14 MB packed) + 34 KB × n-gram layers |
| recompute per token | ≈ 4.1 GB read: ≈ 1.45 s from NVMe, ≈ 173 ms if all 132–183 GB are in RAM |
| check per token | ≈ 90 ms |
| break-even link | ≈ 0.16 Gbps raw / 0.08 packed against NVMe; ≈ 2.6 / 1.4 Gbps against RAM |

At 1 Gbps a sketching seat verifies a qwen4_exp-like decode token in ≈ 0.30 s (raw) or ≈ 0.20 s
(packed), against ≈ 1.45 s for a seat streaming from NVMe. That is 5–7× faster, with 2.3 GB of
residency against 132–183 GB. The hypothesis of "~20 MB of trace against ~5.6 GiB of weights,
~280×" holds for the bytes: 26.5 MB against 4.1 GB is 155×. It does not hold for the time, because
a 1 Gbps link is 24× slower than an NVMe. The time gain is 5–7×, and the residency gain is 60–80×.

## 8. Verdict per class and link

Decode tokens; "RAM" means the seat holds the whole class resident.

Each cell is the speed-up of algebraic verification over a recompute (above 1 it pays), raw / packed.

| class | 1 Gbps | 100 Mbps | 5 Mbps |
| --- | --- | --- | --- |
| Qwen2.5-1.5B, RAM | 0.7× / 1.2× — even | 0.08× / 0.13× — recompute | recompute |
| Qwen2.5-1.5B, NVMe | **10× / 16× — algebra** | 1.1× / 1.9× | 0.06× — recompute |
| Qwen3-30B-A3B, NVMe | **6.9× / 11× — algebra** | 0.8× / 1.4× | recompute |
| Qwen3-Next-80B-A3B, NVMe | **5.2× / 7.5× — algebra** | 0.8× / 1.4× | recompute |
| DeepSeek-V3, NVMe | **10× / 15× — algebra** | 1.1× / 1.8× | recompute |
| qwen4_exp-like, NVMe | **4.8× / 7.2× — algebra** | 0.7× / 1.2× | recompute |
| any class, prefill-heavy job | recompute (break-even 2–5.5 Gbps) | recompute | recompute |

The policy that follows is a seat's own choice, made per class and per job and never consensus.
Algebra is for decode tokens of classes the seat does not hold in RAM, over a link of 100 Mbps or
more. Recompute covers prefill and resident classes. A producer serves the witness on request
(RFC-0007 §II.7).

## 9. For testnet-12 and the capacity plan

- t12's licensing is bound by seat receipts. Lane P measured a replay at 3–8 s for the floor class
  and 280–380 s for the 8k class. Both are Qwen2.5-class dense models, and the 8k class is
  prefill-heavy. For them algebra **does not cut the seat's time** on realistic links: the 8k job's
  witness alone is several GB.
- The capacity steps (×10 → ×100 → ×1000 claims per bond) are a carriage and aggregation problem
  (RFC-0007 Part I) and a scheduling problem (lane P's F2). Part II lets more seats serve a large
  class (a seat needs 2–6 GB, not 80–674 GB). It does not make each small-class receipt cheaper.
- **Coverage, not speed.** For a dense class whose seat holds the weights, the cheapest full-coverage
  check is still a full recompute: 43 ms per token against 60 ms algebraic at 1 Gbps. Algebra raises
  coverage only where a full recompute is infeasible.

## 10. Escalation: what a failed check costs a seat without the weights

A failed check shows that the witness is wrong, not that the claim is. To name the first divergent
committed row, the seat recomputes the failing node exactly. That needs that node's weight rows:

- **The fetch.** In the worst case this is the largest served weight: 311–927 MB, the vocabulary
  head (§3). For an expert product it is the gathered experts' slices, 1–3 MB each. Rows come as
  inventory leaves of ≤ 32 KiB with their paths. Fetching the head's 467 MB over 1 Gbps takes about
  4 s, once per failed claim — and a failed claim's producer is slashed.
- **Bounding the fetch.** A seat may keep **block sketches** for its largest weights: `B` row blocks
  per site, each sketched separately, costing `B × K` entries. A failing block then names `|W| / B`
  rows. For the 151,936-row head at `B = 256`, that is 256 × 1,536 × 8 B = 3.1 MB of extra sketches
  and a fetch of 1.8 MB instead of 467 MB.
- **Or hand it off.** The interval goes to a seat that holds the class: a full-coverage holder or the
  layer shard of RFC-0006. RFC-0007 §II.8 specifies both paths.

## 11. Limits of these numbers

- The rates are one machine's under heavy load; the 8-core seat is an extrapolation from one-thread
  rates. Both the recompute and the check parallelise trivially, so the ratios hold where absolute
  times do not.
- The NVMe figure (3 GB/s) is an assumption, not a measurement; a residency hit rate above zero
  moves the large classes toward the RAM column.
- Packed bytes use type-worst-case weight intervals (an upper bound); a production witness could also
  entropy-code the low bits (accumulators are near-Gaussian well inside their bounds), which these
  numbers do not credit.
- The elementwise rate is a scalar proxy (0.33 G elements/s); the typed backend's fused kernels are
  faster. That term is the same on both paths.
- `Q·Kᵀ` is recomputed under the recommended policy. A seat with a very fast link (≥ 10 Gbps) and a
  slow CPU could serve it; the table shows what that costs (§5, policy S).
- The prototype checker itself (reference evaluator for the exact half, `i128` tensors) is a
  correctness instrument, not a timing one; its tests run tiny fixtures in milliseconds.
