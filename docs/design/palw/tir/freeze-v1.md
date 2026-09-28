# PALW-TIR v1 — the freeze (RFC-0002 Phase E)

## Freeze declaration

**PALW-TIR v1 is frozen as of 2026-09-28.** The RFC-0002 coordinator decided it on this document's
evidence.

* **`prim_set_id` (final):**
  `61fa4aa57adfc79053c5e517515e50c7ae7c036abc43ff931053144691ba31c9212539a93514f831a8472ca75a39b094177736aa5818eeae547bb31fbf89f589`.
  It is `BLAKE2b-512`, keyed `misaka-palw/tir-prim-set-id/v1`, over `prim_set_descriptor_v1()`
  (`palw-tir/v1/spec=04b-tensor-ir/rev2/q=24/prims=…`). That descriptor covers the 25 primitives of
  `misaka-palw-tir/src/prim.rs` (`PRIM_SET_ID_V1`; the set was last touched in `a2e6ec35a`).
* **The criteria** (RFC-0002 §1.2; the table under Summary):
  * Criteria 1–4, 6 and 7 hold.
  * Criterion 5 holds for every family's fidelity row: 57/57 tiny fixtures and 7/7 real
    checkpoints.
  * Its recurrence-drift column held at the freeze for Jamba (C7). It missed for Qwen3.5-0.8B
    (C4/C7, ×4.00) and Mamba-370m (C5, ×5.64) at 4,096 positions. Both were recorded as
    **quantisation gap, lowering work open**, as RFC-0002 open question 3 prescribes: a family that
    cannot meet its thresholds "is a quantisation problem, not an IR problem".
  * **After the freeze** (§5.2): the post-freeze precision work (tir/lower `b60e91ef5`, `1f9c2fc80`,
    `b5f892be4`, `251f77f23`; no primitive) takes the long-context factor to ×1.47 (Qwen3.5) and
    ×1.46 (Mamba). The single-document ratios are ×2.15 and ×1.67.
  * **The drift column's definition** (decided 2026-09-29 by the RFC-0002 coordinator, corpus-v1
    §9). Drift is measured like-for-like: the same target tokens (positions 3,968–4,096) scored at
    4,096 context against short context. A row passes if the ratio is ≤ 1.5, or, secondarily, if
    KL at 4,096 ≤ 0.01. A change of text difficulty is not drift. All three rows pass on the ratio:
    * Qwen3.5: 0.01227 / 0.00836 = ×1.47.
    * Mamba: 0.00035 / 0.00024 = ×1.46. It also passes the floor.
    * Jamba: 0.00091 / 0.00148 = ×0.61.
  * **Criterion 5 is met**, with no exception. §9's 16 × 4,096 sample is scheduled for a quiet
    window, for the record (§8).
* **What the freeze fixes, and what it leaves open.** Fixed: the primitive set, its semantics (spec
  04b rev2) and the descriptor; a change to any of them is v2, with a new `prim_set_id`. Open: the
  lowering, calibration, library forms and quantisation. Those change programs and artifacts, never
  `prim_set_id`.

## The evidence

RFC-0002 §1.2: PALW-TIR v1 is frozen, its `prim_set_id` fixed, the moment freeze criteria 1–7 first
hold together. This document states, criterion by criterion, the evidence (commits, tests, numbers)
and what is still open. It is written on `tir/lower` and cites the other lanes at these heads:

| lane | branch | head cited |
| --- | --- | --- |
| IR, library, admission, conformance | `tir/core` | `23c6d4efd` (lean forms), `b9d75a4e5` (admission A1–A7) |
| independent second implementation | `tir/ref2` | `834591ebd` (A1–A7 resolved, demand D1–D5), `a287e413d` |
| typed backend (node) | `tir/node` | `a944fadf6`, `a62de02b8`, `b966a4b29` |
| consensus integration | `tir/phase-f` | `0b22fd8e4` (F5), `23cf9235c` and `827187f34` (04b §9.4) |
| HF lowerer, F3, F10, D-F1 offline | `tir/lower` | this commit |

`prim_set_id` (final since 2026-09-28): `PRIM_SET_ID_V1` = `61fa4aa57adfc79053c5e517515e50c7…`, in
full under the freeze declaration above.

## Summary

| # | criterion | status | the evidence in one line |
| --- | --- | --- | --- |
| 1 | no model-specific primitive | **holds** | 25 primitives named by mathematics; 57 HF architectures + 7 golden + 5 corpus programs lower to them; the union over the 57 is all 25 |
| 2 | minimality | **holds** (25 < the 30–50 target, argued) | corpus-v1 §8.2 per primitive; nothing added since Gate 1 |
| 3 | stability under the last family | **holds** | the last families (MLA + group-limited routing, RWKV-4; RWKV-6/7 in the library) forced no primitive; `prim.rs` has held 25 since `ac7b96ae7` |
| 4 | three-way bit identity | **holds** | golden 239/239 on ref2; 1,824 positions × 57 HF programs equal on reference/ref2/exec; admission and demand differentials 0 disagreements; D-F1: 13,846 positions of the genesis 8k artifact equal to the legacy engine |
| 5 | fidelity | **met**: every fidelity row (57/57 fixtures, 7/7 real checkpoints), and every drift row under the like-for-like definition decided 2026-09-29 (the same tokens at 4,096 vs short context ≤ ×1.5; floor ≤ 0.01 secondary): Qwen3.5 ×1.47, Mamba ×1.46, Jamba ×0.61 (§5.2) | Mamba-370m 1.000 / 0.00021 / −0.14 %; Qwen2.5-1.5B 0.980 / 0.0008 / +0.53 % |
| 6 | legacy conformance | **holds** | 38 integer catalogue kernels + `RequantizeByToken` byte-identical on the live code; the whole dense A16 tier by D-F1 (9,373,742 legacy node rows equal) |
| 7 | static verifiability | **holds with one stated window** | `tir_admit_v1` admits every corpus and HF-lowered program at the legacy court's ceilings; DeepSeek-V3 needs a window ≤ `2^16` |

This document's own verdict, before the decision, was "not yet": the drift column misses for C4 and
C5. The coordinator froze v1 instead under RFC-0002 open question 3. Both misses are lowering
precision (§5.2), and every earlier miss was fixed without touching the IR: Mamba's tied head, and
Jamba's calibration.

## 1. Criterion 1 — no model-specific primitive

The 25 primitives (spec 04b §6, `misaka_palw_tir::prim::PRIM_NAMES_V1`), by kind: **structure**
Reshape, Transpose, Slice, Concat, Broadcast, Iota, Gather · **exact arithmetic** Cast, Add, Sub,
Mul, MatMul, ReduceSum, ReduceMax · **lossy** Div (three rounding rules), Clamp, Log2Floor ·
**fixed-iteration transcendentals** IntExp, IntRsqrt, IntLn · **selection** Compare, Select, TopK ·
**state** StateWrite, HistAppend. None reads an architecture's parameters or conventions; the tripwire
is `prim::tests::no_primitive_is_named_after_a_model` (tir/core). RMSNorm, LayerNorm, softmax (with
sinks, soft-capping), RoPE (every scaling), attention (GQA/MQA/sliding/ALiBi/MLA), the delta rule,
the selective scans, WKV-4/6/7, routing and the MoE combine are library templates or the lowerer's
composites — plain primitives in the program bytes.

**Every architecture and the primitives its program uses** (`misaka-palw-tir-lower/tests/admission.rs`,
`every_lowered_architecture_uses_only_the_v1_primitives`, over the lowered programs of the 57 HF
tiny fixtures; per fixture in Appendix A):

| family | architectures (HF `model_type`) | primitives used |
| --- | --- | --- |
| C1/C2 dense and attention variants | llama, llama (linear RoPE, tied), mistral (sliding), mistral3 VLM, qwen2 (dynamic NTK, sliding), qwen3 (YaRN), gemma, gemma2, gemma3, gemma3 VLM, phi, phi3 (LongRoPE), gpt2, gpt_bigcode (MQA, MHA), gpt_neo, gpt_neox (parallel, sequential), gptj, falcon (ALiBi, MQA, new decoder), bloom, mpt, opt (pre/post-LN), starcoder2, stablelm (parallel), olmo, olmo2, cohere, cohere2, nemotron, exaone4, granite, smollm3, llava | 15 (GPT-2, OPT) to 19 (Gemma 2: + Compare, Select for soft-capping); ALiBi adds Iota |
| C3/C8 MoE and routing | mixtral, qwen2_moe (shared expert), qwen3_moe, olmoe, granitemoe, gpt_oss (sinks, clamped SwiGLU), deepseek_v2, deepseek_v2 (lite), deepseek_v3 (MLA, group-limited top-k, sigmoid + selection bias) | 18 (+ TopK) to 22 (DeepSeek: + Broadcast, Compare, Iota, Select for the group mask); gpt-oss uses Cast |
| C4 gated delta / C7 hybrids | qwen3_next, qwen3_5, qwen3_5 VLM, qwen3_5_moe | 22–23: + StateWrite, IntLn (softplus), Log2Floor (L2 and wide norms), Broadcast (head maps) |
| C5 state-space | mamba, falcon_mamba, mamba2 | 19–20: StateWrite, IntLn, no HistAppend |
| C7 hybrid | jamba (attention + Mamba + MoE) | 22 |
| C6 recurrent | rwkv (RWKV-4) | 15: StateWrite, Compare/Select (the running max), no HistAppend |
| **all 57** | | **25 — every primitive is used, none else** |

The hand-written corpus programs of tir/core (`consensus-vectors/tir-v1/programs/`: `dense-gqa-2layer`,
`sliding-global`, `hist-window`, `moe-top2-shared`, `gdn-k2-v4-grouped`, `mamba2`,
`fixed-state-saturation`) are programs over the same 25 by construction (NF-1 decodes no other tag).

**RWKV-5/6/7** are expressible: `tir_library_v1` has `rwkv6_step` (`S ← diag(w)·S + kᵀv`, bonus `u`;
RWKV-5 is the same step with a static per-head decay) and `rwkv7_step` (the generalised delta rule
`S ← S(diag(w) − k̂ᵀ(a⊙k̂)) + vᵀk`), with `exp_neg_exp_q24` for the data-dependent decay, each checked
against a closed-form float recurrence in `misaka-palw-tir/tests/library.rs` (within 3·10⁻³ and
5·10⁻³ relative over the tested steps). Their **fidelity is unverified offline**: RWKV-5/6 exist
only as remote code (`modeling_rwkv5.py`/`modeling_rwkv6.py`) and RWKV-7 in flash-linear-attention,
not in `transformers` 5.17, so no pinned reference can be run here; `palw-tir-check` names them
`NOT_LOWERABLE` (remote code) and the fixtures stop at RWKV-4.

## 2. Criterion 2 — minimality

corpus-v1 §8.2 argues each of the 25 against every composition of the others under "equal court
cost" (the same MAC and transcendental counts, the elementwise count within a small constant, no extra
materialised tensor of the output's size), and lists what was dropped as a composition (`Split`,
`Pad`, `Neg`, `Abs`, `Min`, `Max`, `ShiftLeft`, `ShiftRight`, `DivConst`, `Requantize`, `Rescale`,
`Narrow`, `IntRecip`, `IntSigmoid`, `ArgMax`, `ReduceMin`, `BatchedMatMul`, `Widen`, `StateRead`/
`HistRead`). Phase E added none: the lowerer's and the library's composites (§1) are all primitive
compositions. 25 is below the RFC's 30–50 target; the target was a hypothesis and criterion 2 the
rule (corpus-v1 §8.2's closing paragraph).

## 3. Criterion 3 — stability under the last family

The last families lowered for real were, on the lowerer, **DeepSeek's MLA with group-limited and
sigmoid + selection-bias routing, and RWKV-4** (`abbcc1240`, "all 57 HF tiny fixtures lower"), after
the selective scans (`66b37eff6`) and the gated-delta hybrids and gpt-oss (`18535d74d`); in the library,
the RWKV-6/7 steps and the Mamba-1/2 steps (`925d4a3dd`). None forced a primitive: `misaka-palw-tir/src/prim.rs`
has held the same 25 wire tags since the skeleton (`ac7b96ae7`), and its only later change
(`a2e6ec35a`) moved `prim_set_id` to the spec's key over descriptor `rev2` (ref2 finding F14), not the
set. The additions the corpus forced came before Gate 1 and are general (corpus-v1 §6.2: `Log2Floor`,
`i128` inside a region, `Iota` over `H`, `MatMul` batch dimensions). What the last families did need
was **node budget**, not primitives: the lean narrowing and unit rows (§7, tir/core `23c6d4efd`).

## 4. Criterion 4 — three-way bit identity

`reference evaluator (misaka-palw-tir) == independent second implementation (misaka-palw-tir-ref2) ==
every backend that ships (misaka-palw-tir-exec)`, byte for byte, at every commit point.

| evidence | where | numbers |
| --- | --- | --- |
| golden vectors (primitives, programs, encoding, cone refusals) | ref2 `38eeff984`, `81946fec3`; exec `ce76d93ee`, `7cd2df291` | ref2 239/239 with error classes; exec 138 primitive cases as const and as param operands, the 7 programs' steps, commits and cones |
| random differential, reference vs ref2 | ref2 `294a332ad`, `8c04c5aa6` | 40k single-primitive cases at range extremes; random programs' steps, commits, cones and run states; byte and structural mutations; x25 run: 0 disagreements; 36,025 single-defect cone environments equal |
| random differential, reference vs exec | exec `6c175bd41`, `4771e27c0` | 20,000 programs × up to 8 steps and 6,000 mid-run starts; every value, success and failure class equal |
| corpus programs on exec | exec `ce76d93ee` | the 5 corpus programs at every node (61k node values), hostile full-range weights, 48-position runs, 510 mutated programs |
| node leg and resumption | exec `ecf752c84`, `a62de02b8` | 52 jobs, 10,167 leaves equal to the reference's through the consensus builder; 416 resumed runs, 41,566 leaves equal |
| demand evaluation (the court) | ref2 `a287e413d`, `addbe3971`, `834591ebd`; phase-f `23cf9235c`, `827187f34` | 672/672 demand vectors; x25 differential of 776,251 cases, 0 disagreements; the text gaps D1–D5 decided in 04b §9.4 |
| **the HF-lowered programs on all three** | tir/lower `9fd31c36f` (`tests/three_way.rs`) | **57 architectures × 32 positions = 1,824 positions: logits and every commit (slot, block, layer, node, value) equal on reference, ref2 and exec** |
| **the legacy A16 engine and its IR program (D-F1)** | tir/lower `53d396a11` (`palw-tir-equiv`) | testnet-12's genesis `graph-v7@8192` artifact: see §6 |

Admission (criterion 7's function) was also implemented twice: ref2's `tir_admit_v1` from 04b §10.3
agreed with the first on 487,500 random, range-safe and mutated admissions except the replay group
rule (A1: 30,319 cases, 22 verdicts), decided in 04b by tir/core `b9d75a4e5` and pinned by
`consensus-vectors/tir-v1/admission.json`; after it, ref2 reproduces the 29/29 admission vectors and
the 487,500 x25 cases with 0 disagreements, refusals equal in limit and value (`834591ebd`).

## 5. Criterion 5 — fidelity against the float original

Reference pinned as Gate 1 decided: `transformers` 5.17, eager attention, per-position decode; the
f32 reference of the lowerer (`float_ref`) matches it to ~1e-6 on every fixture (hf-coverage §3.9).
Thresholds: corpus-v1 §9.

### 5.1 Every architecture, tiny fixtures (random weights: the lowering is right)

`tests/fidelity_tiny.rs`: calibration 6×32 random tokens, evaluation 3×24 others (72 positions; 48
where a learned position table is shorter), reference evaluator vs the f32 reference. Measured on
the freeze candidate's lowering (tir/lower `da38f3b03`: gathered tables at per-row `i16`, tied heads
reading them).

| family (thresholds top-1 / mean KL / ppl Δ) | fixtures | top-1 | mean KL, max (median) | max |ppl Δ| | meets |
| --- | --- | --- | --- | --- | --- |
| C1/C2 dense (≥ 0.85 / ≤ 0.10 / ≤ +5 %) | 39 | 0.903–1.000 | 1.8e-3 (7e-5) | 1.09 % | yes |
| C3/C8 MoE (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | 9 | 0.958–1.000 | 2.4e-4 (5e-5) | 0.68 % | yes |
| C4 GDN / C7 GDN hybrids (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | 4 | 0.944–0.972 | 6.7e-3 (1.7e-3) | 2.41 % | yes |
| C5 SSM (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | 3 | 0.958–1.000 | 8e-5 (< 1e-5) | 1.02 % | yes |
| C7 Jamba (loosest component) | 1 | 0.944 | 3.6e-4 | 0.05 % | yes |
| C6-adjacent RWKV-4 (≥ 0.75 / ≤ 0.20 / ≤ +10 %) | 1 | 1.000 | 4e-5 | 0.01 % | yes |

### 5.2 Real checkpoints (trained weights: the quantisation is good enough)

One public checkpoint of ≤ 3B per family where one exists (§9 lists what was downloaded, with
revisions). Calibration 8 × 128 tokens and evaluation 2 × 128 held-out tokens of this repository's
`docs/*.md` (named per run), tokenised offline with each checkpoint's own `tokenizer.json`;
integer program vs the f32 reference, teacher-forced.

Measured on tir/lower `251f77f23`, the freeze plus the post-freeze precision work. That work
changes only the programs with a selective scan or a gated delta (Mamba, Jamba, Qwen3.5); the other
rows are the freeze's, with identical programs. Thresholds (top-1 / mean KL / ppl Δ) are from
corpus-v1 §9:

| family (thresholds) | checkpoint | top-1 | mean KL (max) | ppl float → integer | meets |
| --- | --- | --- | --- | --- | --- |
| C1 dense, Qwen2 architecture (≥ 0.85 / ≤ 0.10 / ≤ +5 %) | Qwen/Qwen2.5-1.5B-Instruct (on disk) | 0.980 | 0.00077 (0.0057) | 60.37 → 60.69 (+0.53 %) | yes |
| C1 dense, Llama architecture (same) | `HuggingFaceTB/SmolLM2-1.7B-Instruct` | 0.973 | 0.00112 (0.0253) | 47.98 → 47.98 (+0.00 %) | yes |
| C1 Phi, partial rotary and parallel block (same) | `microsoft/phi-1_5` | 0.980 | 0.00039 (0.0050) | 94.98 → 95.13 (+0.16 %) | yes |
| C3/C8 MoE, GraniteMoE: 32 experts, top-8 (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | `ibm-granite/granite-3.1-1b-a400m-instruct` | 0.957 | 0.00949 (0.3089) | 51.00 → 51.93 (+1.83 %) | yes |
| C5 SSM, Mamba (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | `state-spaces/mamba-370m-hf` | 1.000 | 0.00021 (0.0014) | 74.42 → 74.31 (−0.14 %) | yes |
| C7 hybrid, Jamba: attention + Mamba + MoE (loosest component: C5's) | `ai21labs/Jamba-tiny-dev` | 0.961 | 0.00342 (0.0351) | 354.36 → 351.89 (−0.70 %) | yes |
| C4/C7 gated delta + gated attention, Qwen3.5 (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | `Qwen/Qwen3.5-0.8B` | 0.965 | 0.00602 (0.0436) | 66.87 → 67.13 (+0.39 %) | yes |

The integer side ran on the typed backend (`--exec`). The runs of the five downloaded checkpoints
other than Qwen3.5 were cross-checked against the reference evaluator on their first two positions,
and were equal. Qwen2.5 and Qwen3.5 were not cross-checked. The reference evaluator widens a
gathered table to `i128`: 3.7 GB for Qwen2.5's 151,936 rows and 4.1 GB for Qwen3.5's 248,320, and
the one cross-checked Qwen3.5 run peaked at 7.7 GB, against the 8 GB rule. The typed backend's
identity to the reference evaluator is criterion 4's evidence. Each run here peaked at 2.1–4.9 GB
RSS, running one at a time, offline, on a host shared with a testnet drill. Jamba-tiny-dev is AI21's development model: its
float perplexity is 354.

Jamba's row moved from 0.973 / 0.00289 at the freeze to 0.961 / 0.00342 with the precision work.
That is 3 tokens of 254 on AI21's development model. Its 4,096-position run improved (top-1 0.984
→ 0.985, KL 0.00136 → 0.00117). Mamba's row went from 0.988 / 0.0012 to 1.000 / 0.00021, and
Qwen3.5's from 0.961 / 0.00682 to 0.965 / 0.00602.

The sample is smaller than corpus-v1 §9 proposed. §9 proposed 256 × 512 tokens plus 16 × 4,096 of a
pinned multilingual corpus. These runs are 2 × 128 tokens per row and 1 × 4,096 for drift, all
English Markdown, sized by the memory and time rules. The margins are far wider than that sample's
error. The lowest top-1 is 0.957, against a threshold of 0.80. With 254 scored positions, the
binomial standard error of a top-1 near 0.96 is about 0.012. Every mean KL is at least ten times
below its bound.

**Mamba's miss was the tied head, not the mixer — fixed in the lowering.** The earlier version of
this section (`8689dcfde`) blamed the mixer's 16-bit elementwise path and proposed an `i32` mixer
path. That diagnosis was wrong, and the `i32` path is not built. The record:

1. Under the first lowering (per-row `i8` for every weight, the embedding included), Mamba-370m
   scored top-1 0.758, KL 0.209, ppl +17.9 %, and every site from layer 1 on was 6–9 % off.
2. The site errors did not come from the sites' own narrowing. A site's own 16-bit narrowing error,
   from its calibrated absmax and rms, is 0.01–0.2 % (0.8 % at worst). A float simulation located
   the loss instead: `transformers` on the same tokens, with only the weights quantised per-row. The
   embedding table at `i8` costs little as a lookup (top-1 0.973, KL 0.0044), but as the head tied
   to it, it reproduces the whole loss (0.750 / 0.208). Every other weight at `i8` together gives
   0.957 / 0.0043. The site errors were the embedding's 4.2 % row error carried down the residual
   stream.
3. `0344a2fa3` put gathered tables at per-row `i16` and meant a tied head to read the same codes.
   The set of tables it built looked at a gather's first input, which is the token, so the set was
   empty. The gather moved to `i16` (sites fell to 0.5–2 %, the embedding site to 0.011 %), but the
   head declared its own `i8` copy of the table. The row stayed at 0.742 / 0.210 / +19.1 %.
4. A dump of the post block settled the rest. The integer logits equal the `i8` table times the
   integer final-norm output to 6·10⁻⁴. The float head on that same integer input gives top-1 1.00
   and KL 0.0012 over the diagnosed positions.
5. `da38f3b03` reads the table from the gather's second input. A tied head now reads the one `i16`
   table: an `i16 × i16` `MatMul`, exact in `i64`, with the head's MAC count unchanged. Mamba's
   artifact drops the copy (2,505 → 2,504 tensors, 504.3 → 455.2 MiB). A test pins one `i16`
   `embed.table` on each of the 24 tied fixtures.

**Mamba-370m scored top-1 0.988, KL 0.0012, ppl −0.23 % at the freeze**, against C5's 0.80 / 0.15 /
+8 %. After the post-freeze precision work it scores 1.000 / 0.00021 / −0.14 %: the scan's output
rides the `i32` rail, and its projections read per-row `i16` weights (below).
The mixer's remaining mid-layer errors (`mamba.C`, layers 28–38: 2–8 %) come from the `i8`
projections. The float simulation, with `i8` weights and f32 elementwise math, gives the same
numbers (layer 34: 0.084 simulated vs 0.083 integer; layer 31: 0.041 vs 0.043). An `i32` mixer path
would therefore change no number that matters.

No primitive changes. Each program changes only in the head's weight operand (one param fewer). The
Mamba, FalconMamba, Mamba2 and Jamba fixtures use the same primitives as before (19, 19, 20 and
22). All 57 fixtures pass, and the three implementations agree on all of them (`three_way.rs`).

Untied models' programs are byte-identical. Their digests before and after are equal for Phi-1.5
and Jamba-tiny-dev, so those rows, and Jamba's drift, stand as measured. The five tied checkpoints
(Qwen2.5, SmolLM2, Granite, Qwen3.5 and Mamba) were re-run.

**The calibration-length rule** (tir/lower `45671e998`). A program with a recurrence is calibrated
on at least one sequence as long as the longest context it is evaluated or served at. "A
recurrence" means any `Fixed` state: a selective scan, a gated delta rule, a WKV state, a conv
window, a token shift. So the rule binds every recurrent and hybrid class (Mamba, Jamba, Qwen3.5,
Qwen3-Next, RWKV), and attention-only programs are not bound by it. Enforcement:

* The lowerer's `fidelity::check_calibration_length` states the rule, and `palw-tir-fidelity`
  refuses a shorter calibration. `--allow-short-calibration` measures anyway and writes `"rule":
  "waived"` into the result.
* Every result records `calibration.length_rule` (`met`, `not recurrent` or `waived`, with the
  longest calibration sequence and the context).
* An artifact's provenance records its `calibrated_context` and the rule as applied
  (`calibration_length_rule`, tir/lower `7b6fe2bda`). `palw-class declare-layout` (tir/node,
  `misaka_palw_sdk::tir_layout::tir_calibration_covers_context_v1`) refuses a recurrent program
  whose layout `max_context` exceeds `calibrated_context`, or whose provenance records none, unless
  the rule was waived.

The test `a_recurrent_program_is_calibrated_as_long_as_its_context` pins the rule.

**Recurrence drift** (corpus-v1 §9: KL at position 4,096 ≤ 1.5 × KL at 128, for C4–C7). It is
measured as the mean KL over positions 64–192 against positions 3,968–4,096 of one held-out
4,096-token sequence (`docs/palw-rc-threat-model.md`), on the typed backend. "Long" calibration is
the 8 × 128 set plus one held-out 4,096-token sequence (`docs/palw-mainnet-audit-2026-08-28.md`, a
different document from the evaluation's).

The **context factor** is a content control. The same last 256 tokens are evaluated as a
256-position sequence, and their KL over positions 128–256 is compared with the long run's late
window. It separates what the long context does from what the tail of the document is.

| family | checkpoint | lowering, calibration | KL 64–192 | KL 3,968–4,096 | ratio | the same tokens at a short context (context factor) | over all 4,096 positions | meets |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C4/C7 gated delta | `Qwen/Qwen3.5-0.8B` | first lowering, 8 × 128 | 0.00614 | 0.02150 | 3.50 | — | top-1 0.938, KL 0.0131, ppl +0.14 % | no |
| C4/C7 gated delta | `Qwen/Qwen3.5-0.8B` | `da38f3b03` (freeze), long | 0.00620 | 0.02480 | 4.00 | 0.0104 (×2.4) | top-1 0.938, KL 0.0148, ppl +0.52 % | no |
| C4/C7 gated delta | `Qwen/Qwen3.5-0.8B` | `b60e91ef5` (`v` 8 bits finer), long | 0.00572 | 0.01227 | **2.15** | 0.00836 (**×1.47**) | top-1 0.950, KL 0.00845, ppl +0.04 % | **ratio no — quantisation gap narrowed, lowering work open**; context factor within 1.5 |
| C5 SSM | `state-spaces/mamba-370m-hf` | `da38f3b03` (freeze), long | 0.00113 | 0.00640 | 5.64 | 0.00136 (×4.7) | top-1 0.970, KL 0.0025, ppl −0.57 % | no |
| C5 SSM | `state-spaces/mamba-370m-hf` | `1f9c2fc80` (scan output on the `i32` rail), long | 0.00390 | 0.00432 | 1.11 | 0.00330 (×1.31) | top-1 0.961, KL 0.00373, ppl −0.06 % | yes |
| C5 SSM | `state-spaces/mamba-370m-hf` | `251f77f23` (+ `i16` rows for `out_proj`, `x_proj`, `dt_proj`), long | 0.00021 | 0.00035 | **1.67** | 0.00024 (**×1.46**) | top-1 0.988, KL 0.00028, ppl −0.02 % | **ratio no, at a late KL of 3.5e-4, 12× below the row above's**; context factor within 1.5 |
| C7 hybrid | `ai21labs/Jamba-tiny-dev` | first lowering, 8 × 128 | 0.00212 | 0.41083 | 193 | — | top-1 0.914, KL 0.0736, ppl −2.86 % | no |
| C7 hybrid | `ai21labs/Jamba-tiny-dev` | `da38f3b03` (freeze), long | 0.00253 | 0.00081 | 0.32 | — | top-1 0.984, KL 0.00136, ppl +0.08 % | yes |
| C7 hybrid | `ai21labs/Jamba-tiny-dev` | `251f77f23`, long | 0.00248 | 0.00091 | **0.37** | 0.00148 (×0.61) | top-1 0.985, KL 0.00117, ppl +0.03 % | **yes** |

Over all 4,096 positions, every run meets its family's fidelity row.

**The calibration explains Jamba's first miss (×193 → ×0.32).** A short calibration sizes every
static scale of a state, or of a long-context activation, on short-context magnitudes, and those
saturate by position 4,096. The calibration-length rule above removes that cause. It did not fix
Qwen3.5 or Mamba, whose misses both had the rule met.

**Where the rest came from, measured on the integer program.** `palw-tir-fidelity --site-windows
64..192,3968..4096` runs the traced float model and the typed backend position by position and
reports every site's error in each window.

* **Mamba: one site saturated.** Layer 29's gated product `y·silu(z)` goes from 2.0 % error early
  to 34 % late. Its scan (1.2 % → 0.9 %) and gate (1.1 % → 1.3 %) do not grow, and every later
  layer inherits the jump. In float, the product reaches 45.6 late in the document, against its
  16-bit site's clamp of 20.4. Nineteen positions saturate from position 3,569 on, and one each at
  layers 39 and 44. A scan's output grows with the context it has integrated, and one static
  16-bit scale calibrated on shorter contexts cannot hold it. The fixes:
  1. `1f9c2fc80`: the scan delivers its output on the `i32` rail. Its product with the gate's code
     is exact in `i64`, narrowed once to `i32` with 256× the `i32` headroom (still 2^14 finer than
     the code it replaces), and `out_proj` reads it wide.
  2. `b5f892be4`: that projection reads per-row `i16` weights, recovering the precision the
     outlier split had carried. The two are exclusive.
  3. `251f77f23`: the projections that set the step size and `B`/`C` (`x_proj`'s parts and
     `dt_proj`) read per-row `i16` weights. At `i8`, their input-correlated rounding biases each
     channel's decay. In float that alone drifts ×5.8; it was real but smaller than the saturation
     (KL 0.0006 late).
  The artifact grows from 455 to 556 MiB. The 128-position row went from 0.988 top-1 / KL 0.0012 to
  1.000 / 0.00021 (§5.2).
* **Qwen3.5: no single site.** The gated-delta cores' errors double from the early window to the
  late one across the layers (layer 10: 5.1 % → 11.3 %; layers 16–18: ×2.2–×2.6) and cascade
  through the residual. A float replay of the step with the lowering's roundings
  (`tools/drift_gdn_replay.py`) located the cause: the read `w = S·k`, `v − w` and the β product's
  shift were rounded on `v`'s 16-bit grid and written back into the state at every step. With them
  8 bits finer the replay is flat. `b60e91ef5` has `v` enter the step 8 bits finer. It costs one
  node (the largest blocks: 468 of 512) and changes the context factor from ×2.4 to ×1.47.

**The single-document ratio, its content, and its conditioning.** Two things limit what one
document's ratio can say:

* **Content.** Qwen3.5's last 128 tokens score KL 0.00836 even at a short context, 1.46 × the early
  window's 0.00572. The ratio of this one document therefore cannot fall below about 1.46, whatever
  the recurrence does, and 1.47 × 1.46 is the 2.15 measured.
* **Conditioning.** A ratio of two small KLs is ill-conditioned near zero. Mamba's final lowering
  cuts the early window's KL 19× (0.00390 → 0.00021) and the late window's 12× (0.00432 → 0.00035).
  The ratio therefore rises from 1.11 to 1.67, although every number improved and the context
  factor is 1.46.

corpus-v1 §9 proposed 16 × 4,096 sequences, which average such tails out. The context factor
separates the recurrence from the text and from the noise floor: Qwen3.5 ×1.47, Mamba ×1.46,
Jamba ×0.61.

**The rule that settles the column** (decided 2026-09-29, now corpus-v1 §9):
- Drift is measured like-for-like: the same target tokens (the last 128 of a 4,096-token held-out
  document) scored at 4,096 context against at short context (positions 128–256 of the document's
  last 256 tokens).
- A row passes if that ratio is ≤ 1.5, or, secondarily, if KL at 4,096 ≤ 0.01 nats.
- A change of text difficulty is not drift. The early window (64–192) scores different tokens and is
  reported only.

| row (lowering) | KL at 4,096 (3,968–4,096) | same tokens at short context | ratio | floor | early window 64–192 | passes |
| --- | --- | --- | --- | --- | --- | --- |
| Qwen3.5-0.8B (`b60e91ef5`) | 0.01227 | 0.00836 | **×1.47** | 0.0123 > 0.01 | 0.00572 | **yes** (the ratio) |
| Mamba-370m (`251f77f23`) | 0.00035 | 0.00024 | **×1.46** | 0.00035 ≤ 0.01 | 0.00021 | **yes** (both) |
| Jamba-tiny-dev (`251f77f23`) | 0.00091 | 0.00148 | **×0.61** | 0.00091 ≤ 0.01 | 0.00248 | **yes** (both) |

One more lowering option was tried for Qwen3.5 after the rule was set: per-row `i16` weights for
the gated delta's decay and β projections (`89c4fbcd9`, the Mamba lesson). It left the late window
at 0.0128 (×2.22; context factor ×1.51) and was reverted (`403b38c6a`).

## 6. Criterion 6 — legacy conformance

Each integer catalogue kernel is a `tir_library_v1` segment, byte-identical to the live code:
`misaka-palw-tir-conformance` (tir/core `07c3e82ca`) calls `palw_base0`, `palw_base0_ops`,
`palw_base0_a16` and `palw_qwen36_ops` and requires byte identity on seeded random operands and the
type extremes of each kernel's own domain, every segment admissible under the §7 range rules.
Coverage is itself a test: the conformed descriptors hashed with `kernel_semantics_id_v1` are exactly
the integer part of `catalogued_kernel_ids_v1()` (**38 kernels**) plus `fenced_kernel_ids_v1()`
(**RequantizeByToken**). The seven float kernels are out of scope (criterion 6's own wording), and the
four fenced Kimi K3 arms are recorded as defective and never claimed. The lean forms tir/core added
for this lowerer are each proved equal to their template and to the live kernel (`23c6d4efd`).

**The whole dense A16 tier, composed (D-F1, `palw-tir-equiv`).** Kernels conformed one by one could
still be composed wrongly; D-F1 checks the composition on the artifact a network registered.
testnet-12's genesis `Qwen/Qwen2.5-1.5B/graph-v7@8192` artifact is the 512-wide genesis artifact
(`~/Downloads/bound-candidate.palwart`) with its rotary table regenerated at 8,192 positions — it
then **pairs with the root testnet-12 registered for the row** (`88096dc1…`), i.e. it is the
registered artifact. Converted to `PALWTIR1` by F3 (`palw-a16-to-tir --respan 8192`), the A16 mirror
program (which commits every one of the row's node rows: 2 pre, 24 a layer with the fused attention
site, 3 post) runs on the typed backend beside the legacy engine over the plan compiled from the
row's registered profile:

| jobs | positions | logits rows equal | legacy node rows equal to their IR commit points | legacy / IR ms a position |
| --- | --- | --- | --- | --- |
| canonical (1,023 + 2, the backend's prompt for the zero anchor) + 32 random prompts (lengths 1–1,023, seed 0x5EED, + 2 decoded) | 13,846 | **13,846 / 13,846** | **9,373,742 / 9,373,742** | 105.6 / 116.2 (the IR side also hands every commit to the comparison) |

**EQUAL** everywhere (51 minutes, 3.96 GB peak RSS, M1 Max shared with a testnet drill). The IR
program commits 56 values a position the legacy row does not (the fused site's logit and
probability codes, two a layer); they are covered by the logits' equality and by the conformance of
`a16_attn_fused`. Reproduce: `palw-a16-to-tir --artifact bound-candidate.palwart --respan 8192
--out genesis-8k.palwtir` (80 s; file digest `686be1d9…`), then `palw-tir-equiv --network testnet-12
--artifact bound-candidate.palwart --respan --tir genesis-8k.palwtir --prompts 32`.

## 7. Criterion 7 — static verifiability

`tir_admit_v1` (tir/core `ccd8f6aae`, `b9d75a4e5`; spec 04b §10.3) — normal form, types, ranges,
per-position costs, every commit point's court cone (box demand, dissection over `H` at `h_chunk`),
every `Fixed` state's checkpoint interval, its own work capped — admits:

* the corpus programs and a Qwen2.5-1.5B-shaped decoder (tir/core tests; 0.25–0.7 ms each);
* **every one of the 57 HF-lowered fixture programs and every lowerable real configuration** at the
  legacy court's ceilings with `tile_len` 64 and `h_chunk` 64 (`tests/admission.rs`); the worst
  terminal tile is Falcon-40B's 2.6 Mi MACs of 16 Mi, the largest block 467 nodes of 512 (Qwen3.5-MoE
  and Qwen3-Next's gated-delta + MoE layer, which keeps outlier splits only for values read by at most
  three projections);
* the six real checkpoints of §5.2, with every one of their tensors bound (programs of 248–1,031
  nodes, no block needing a budget fallback; `C` from 16 for Mamba-370m to 65,536);
* the one refusal, **DeepSeek-V3 at a `2^18` window**: `max_position_macs` 2.26e12 > 2^40. The ceiling
  is not revised: 671B parameters attend over 128 heads × 576 latent lanes and the attention MACs
  scale with the window — at `2^17` it is still refused, at `2^16` it is admitted with 5.93e11 MACs a
  position (`deepseek_v3_is_admitted_at_a_window_of_2_16`, `LowerOpts::max_window`). A DeepSeek-V3
  class declares `max_context ≤ 65,536`.

Two facts the lowering had to meet for this (tir/lower `b3fa53859`, `a82df827f`): NF-8's `2^28`
elements per node caps a history window at `2^28 / row` (a 4,096-lane MHA row: `2^16`), and NF-12's
512 nodes a block needs the lean narrowing and unit rows (now library forms). Under testnet-12's
provisional `palw_tir_v1` ceilings (`max_macs_per_position` 2^37, `max_cone_work` 2^16),
`check-architecture` refuses by name the models whose position exceeds 2^37 MACs at `2^18`
(Falcon-40B, DeepSeek-V3): a network's ceiling, not v1's.

## 8. Open after the freeze (lowering work; none of it changes `prim_set_id`)

1. **Recurrence drift: settled** by the like-for-like definition (decided 2026-09-29; §5.2 and
   corpus-v1 §9). What remains is for the record:
   * **§9's 16 × 4,096 sample**, scheduled for a quiet window rather than a night with release builds
     and a drill on this host. It takes about 7 hours, one checkpoint at a time, each run under
     8 GB. The runner, `tir-lower-target/fidelity/real/run-sample16.sh`, and its token files are
     prepared.
   * **Qwen3.5's margin** (×1.47 against 1.5). Lowering options left: `v` finer at its source (a
     Q24 SiLU on the conv's `v` part) and the conv path's own precision. The decay and β projections
     at `i16` did not help (reverted, `403b38c6a`).
2. **No real checkpoint under the download rules** for Gemma-3 (every official checkpoint is
   gated), Mamba-2 and RWKV-4.
   * Mamba-2 has no official HF-format checkpoint of ≤ 3B: `mistralai/Mamba-Codestral-7B-v0.1` is
     7B, and `state-spaces/mamba2-*` are `mamba_ssm` checkpoints in `.bin`.
   * Every official `RWKV/` repository is `pytorch_model*.bin` only.

   Their fixtures pass (§5.1). A real run needs the user to accept Gemma's licence, or to allow a
   converted `.bin` (loaded without pickle), or a larger or community-ported checkpoint. C6 therefore
   has fixture evidence only.
3. RWKV-5/6/7 fidelity cannot be measured offline (§1).
4. **The evaluation sample** is smaller than corpus-v1 §9's proposal (§5.2): one drift sequence per
   family, which is why the content control of §5.2 was needed.

**Candidates for v2** (each changes the dtype table or a primitive, so it needs a new `prim_set_id`;
v1 is not changed, and none of these blocks a v1 class):

1. **A packed 4-bit dtype** (`i4`, two codes a byte, readable by `MatMul` and `Gather`). v1's
   narrowest type is `i8`, so a pre-quantised checkpoint's 4-bit code costs a whole byte in the
   artifact (hf-coverage §19, recorded 2026-09-29):
   * GPTQ, AWQ and GGUF `Q4_*`: ≈ 1.03 bytes a weight at group 128, against ≈ 0.52 in the packed
     file — about 2× the file. It is still ≈ 0.5× the fp16 weights, and the codes stay exact.
   * 5- and 6-bit codes (GGUF `Q5_*`, `Q6_K`) also cost a byte: ≈ 1.5× and ≈ 1.2× their files.
   * A packed `i4` would bring 4-bit artifacts down to about the file's size. It changes spec 04b's
     dtype table and the descriptor, and every implementation's `MatMul`/`Gather` operand path
     (reference, ref2, exec).

Closed since the previous version of this document:

* Mamba's fidelity row: the tied head (§5.2).
* The 8 GB memory rule. The float side's logits rows (the 13.8 GB run) and a calibration's
  discarded logits (Qwen3.5's long-calibration drift peaked at 8.25 GB, `5e9e240c9`) are no longer
  held. The rows and the other drift runs stayed at or below 4.9 GB, and the float simulations of
  §5.2 at or below 7.3 GB.

## 9. Real checkpoints: the list, and what was downloaded

**Rules (the user's approval, relayed 2026-09-28):** one public, non-gated checkpoint per family,
≤ 3B, from the official org (or the org that publishes the HF-format port); only `config.json`,
`generation_config.json`, tokenizer files and `*.safetensors` (plus the index) — never `*.bin`,
`*.pt`, `*.pth`, pickle, or anything needing `trust_remote_code`; `huggingface_hub` from the venv
with `HF_HUB_OFFLINE` unset for the download command only; under
`~/Downloads/MISAKA-wt-b/hf-ckpt/<repo-id>/`; at most 20 GB in total; fidelity one checkpoint at a
time under 8 GB RSS.

**The list** (sizes as I knew them before looking; the download command re-checks gating, remote
code, formats and sizes against the hub and records the revision):

| family | candidate (fallback) | expected bf16 size | licence / gating as known |
| --- | --- | --- | --- |
| C1 dense, Llama architecture | `HuggingFaceTB/SmolLM2-1.7B-Instruct` (`SmolLM2-360M-Instruct`) | 3.4 GB (0.7) | Apache-2.0, not gated |
| C1 Phi | `microsoft/phi-1_5` (PhiForCausalLM, the phi-2 architecture at 1.4B) | 2.8 GB | MIT, not gated |
| C1 Gemma-3 | `google/gemma-3-1b-it` | 2.0 GB | Gemma terms — **gated**: not downloaded |
| C3 MoE | `ibm-granite/granite-3.1-1b-a400m-instruct` (GraniteMoE, 32 experts, top-8) | 2.7 GB | Apache-2.0, not gated |
| C5 Mamba | `state-spaces/mamba-370m-hf` (`mamba-130m-hf`) | 1.5 GB (fp32 as stored) | Apache-2.0, not gated |
| C5 Mamba-2 | none official in HF format at ≤ 3B | — | not downloaded (§8) |
| C6-adjacent RWKV-4 | `RWKV/rwkv-4-430m-pile` (`rwkv-4-169m-pile`) | ~0.9 GB | Apache-2.0; safetensors on the main branch uncertain — skipped if only `.bin` |
| C7 Jamba | `ai21labs/Jamba-tiny-dev` | ~0.6–1.3 GB | believed Apache-2.0 and not gated; checked |
| C4/C7 gated delta | `Qwen/Qwen3.5-0.8B` (`Qwen/Qwen3.5-2B`) | ~1.7 GB (~4.5) | Apache-2.0, not gated; existence checked |

**What the hub said** (the download command, 2026-09-28, `huggingface_hub` 1.33.0, no token sent):
`google/gemma-3-1b-it` is gated (`manual`) — not downloaded. Every official RWKV-4 repository
(`RWKV/rwkv-4-169m-pile`, `-430m-pile`, `-1b5-pile`, `-3b-pile`, `rwkv-raven-1b5`) and the official
Mamba-2 checkpoints (`state-spaces/mamba2-130m`, `-370m`) carry only `pytorch_model*.bin` — not
downloaded (never `*.bin`). The other six passed every rule (not gated, `*.safetensors` at the top
level, no `auto_map`).

**Downloaded** — `~/Downloads/MISAKA-wt-b/hf-ckpt/<repo>/`, pinned to the revision, the download
command's own record (bytes as written):

| repo | revision | files (bytes) | total bytes |
| --- | --- | --- | --- |
| `state-spaces/mamba-370m-hf` | `b519127f5bfaaa1c27dd938dad051ec360972b23` | `config.json` 917, `generation_config.json` 137, `model.safetensors` 1,486,118,288, `tokenizer.json` 2,113,837, `tokenizer_config.json` 4,793 | 1,488,237,972 |
| `ai21labs/Jamba-tiny-dev` | `ed303361004ac875426a61675edecf8e9d976882` | `config.json` 1,005, `generation_config.json` 132, `model.safetensors` 637,428,728, `special_tokens_map.json` 946, `tokenizer.json` 4,245,070, `tokenizer.model` 1,124,714, `tokenizer_config.json` 14,283 | 642,814,878 |
| `Qwen/Qwen3.5-0.8B` | `2fc06364715b967f1860aea9cf38778875588b17` | `config.json` 2,907, `merges.txt` 3,353,259, `model.safetensors-00001-of-00001.safetensors` 1,746,942,600, `model.safetensors.index.json` 50,900, `tokenizer.json` 12,807,982, `tokenizer_config.json` 16,709, `vocab.json` 6,722,759 | 1,769,897,116 |
| `microsoft/phi-1_5` | `77aa61eeac94fbf33d492b9f2744c98b42d5b5eb` | `added_tokens.json` 1,080, `config.json` 736, `generation_config.json` 74, `merges.txt` 456,318, `model.safetensors` 2,836,578,696, `special_tokens_map.json` 99, `tokenizer.json` 2,114,924, `tokenizer_config.json` 237, `vocab.json` 798,156 | 2,839,950,320 |
| `ibm-granite/granite-3.1-1b-a400m-instruct` | `0da7a48b0276d500ce5922fd2b33944091fc6c09` | `added_tokens.json` 87, `config.json` 889, `generation_config.json` 132, `merges.txt` 441,810, `model.safetensors` 2,669,283,096, `special_tokens_map.json` 701, `tokenizer.json` 3,475,806, `tokenizer_config.json` 8,072, `vocab.json` 776,995 | 2,673,987,588 |
| `HuggingFaceTB/SmolLM2-1.7B-Instruct` | `31b70e2e869a7173562077fd711b654946d38674` | `config.json` 908, `generation_config.json` 132, `merges.txt` 466,391, `model.safetensors` 3,422,777,952, `special_tokens_map.json` 655, `tokenizer.json` 2,104,556, `tokenizer_config.json` 3,764, `vocab.json` 800,662 | 3,426,155,020 |
| **all six** | | | **12,841,042,894** (of the 20 GB allowed) |

Each lowers, binds every checkpoint tensor (0 unused) and is admitted by `tir_admit_v1`
(`palw-tir-check --weights`), after three config keys each of which `transformers` 5.17 does not read
were accepted as inert (tir/lower `9ff0fc899`: the Mamba conversions' `d_model`/`d_inner`/`dt_rank`/
`ssm_cfg`, Qwen3.5's `mlp_only_layers`/`mamba_ssm_dtype`/`attn_output_gate: true`, SmolLM2's
`transformers.js_config`). Token files: calibration 8 × 128 tokens (`docs/archival.md`,
`docs/crescendo-guide.md`, `docs/connecting-ethereum-tooling.md`, `docs/node-liveness-probe.md`, two
chunks each), evaluation 2 × 128 (`docs/README.md`, `docs/evm-differences-from-ethereum.md`), drift
1 × 4,096 (`docs/palw-rc-threat-model.md`), the long calibration's added 1 × 4,096
(`docs/palw-mainnet-audit-2026-08-28.md`), each with the checkpoint's own `tokenizer.json`, offline.

## Appendix A. The primitives of every lowered architecture

From `every_lowered_architecture_uses_only_the_v1_primitives` (`tests/admission.rs`): the 57 HF
tiny-fixture architectures (fixture directory names), their family, how many of the 25 primitives
their lowered program uses, and which it does not use.

| fixture | family | used | not used |
| --- | --- | --- | --- |
| `bloom` | C1/C2 | 16 | Slice, Concat, Broadcast, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `cohere` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `cohere2` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `deepseek_v2` | C3/C8 | 22 | Cast, IntLn, StateWrite |
| `deepseek_v2_lite` | C3/C8 | 18 | Broadcast, Iota, Cast, IntLn, Compare, Select, StateWrite |
| `deepseek_v3` | C3/C8 | 22 | Cast, IntLn, StateWrite |
| `exaone4` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `falcon_alibi` | C1/C2 | 16 | Slice, Concat, Broadcast, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `falcon_mamba` | C5 | 19 | Broadcast, Iota, Cast, ReduceMax, TopK, HistAppend |
| `falcon_mq` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `falcon_new` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gemma` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gemma2` | C1/C2 | 19 | Broadcast, Iota, Cast, IntLn, TopK, StateWrite |
| `gemma3` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gemma3_vlm` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gpt2` | C1/C2 | 15 | Slice, Concat, Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gpt_bigcode` | C1/C2 | 15 | Slice, Concat, Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gpt_bigcode_mha` | C1/C2 | 15 | Slice, Concat, Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gpt_neo` | C1/C2 | 15 | Slice, Concat, Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gpt_neox` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gpt_neox_seq` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `gpt_oss` | C3/C8 | 21 | Broadcast, Iota, IntLn, StateWrite |
| `gptj` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `granite` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `granitemoe` | C3/C8 | 18 | Broadcast, Iota, Cast, IntLn, Compare, Select, StateWrite |
| `jamba` | C7 | 22 | Broadcast, Iota, Cast |
| `llama` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `llama_linear_tied` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `llava` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `mamba` | C5 | 19 | Broadcast, Iota, Cast, ReduceMax, TopK, HistAppend |
| `mamba2` | C5 | 20 | Iota, Cast, ReduceMax, TopK, HistAppend |
| `mistral3_vlm` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `mistral_window` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `mixtral` | C3/C8 | 18 | Broadcast, Iota, Cast, IntLn, Compare, Select, StateWrite |
| `mpt` | C1/C2 | 16 | Slice, Concat, Broadcast, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `nemotron` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `olmo` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `olmo2` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `olmoe` | C3/C8 | 18 | Broadcast, Iota, Cast, IntLn, Compare, Select, StateWrite |
| `opt` | C1/C2 | 15 | Slice, Concat, Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `opt_postln_proj` | C1/C2 | 15 | Slice, Concat, Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `phi` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `phi3_longrope` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `qwen2_dynamic` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `qwen2_moe` | C3/C8 | 18 | Broadcast, Iota, Cast, IntLn, Compare, Select, StateWrite |
| `qwen2_sliding` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `qwen3_5` | C4/C7 | 22 | Iota, Cast, TopK |
| `qwen3_5_moe` | C4/C7 | 23 | Iota, Cast |
| `qwen3_5_vlm` | C4/C7 | 22 | Iota, Cast, TopK |
| `qwen3_moe` | C3/C8 | 18 | Broadcast, Iota, Cast, IntLn, Compare, Select, StateWrite |
| `qwen3_next` | C4/C7 | 23 | Iota, Cast |
| `qwen3_yarn` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `rwkv` | C6 (RWKV-4) | 15 | Transpose, Slice, Concat, Broadcast, Iota, Cast, ReduceMax, IntLn, TopK, HistAppend |
| `smollm3` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `stablelm` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `stablelm_parallel` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
| `starcoder2` | C1/C2 | 17 | Broadcast, Iota, Cast, IntLn, Compare, Select, TopK, StateWrite |
