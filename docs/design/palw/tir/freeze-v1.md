# PALW-TIR v1 — the freeze (RFC-0002 Phase E)

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

`prim_set_id` of the candidate: `PRIM_SET_ID_V1` = `61fa4aa57adfc79053c5e517515e50c7…` (BLAKE2b-512
keyed `misaka-palw/tir-prim-set-id/v1` over the descriptor `rev2`, `misaka-palw-tir/src/prim.rs`).

## Summary

| # | criterion | status | the evidence in one line |
| --- | --- | --- | --- |
| 1 | no model-specific primitive | **holds** | 25 primitives named by mathematics; 57 HF architectures + 7 golden + 5 corpus programs lower to them; the union over the 57 is all 25 |
| 2 | minimality | **holds** (25 < the 30–50 target, argued) | corpus-v1 §8.2 per primitive; nothing added since Gate 1 |
| 3 | stability under the last family | **holds** | the last families (MLA + group-limited routing, RWKV-4; RWKV-6/7 in the library) forced no primitive; `prim.rs` has held 25 since `ac7b96ae7` |
| 4 | three-way bit identity | **holds** | golden 239/239 on ref2; 1,824 positions × 57 HF programs equal on reference/ref2/exec; admission and demand differentials 0 disagreements; D-F1: 13,846 positions of the genesis 8k artifact equal to the legacy engine |
| 5 | fidelity | **not yet**: all 57 fixtures pass; 6 of 7 real checkpoints meet their family's row (Qwen2.5, SmolLM2, Phi-1.5, Granite-MoE, Jamba, Qwen3.5); Mamba-370m misses; drift: Jamba meets it with a long calibration (×0.45), Qwen3.5's long-calibration run is open — lowering/calibration, not IR (§5.2, §8) | e.g. Qwen2.5-1.5B top-1 0.973 / KL 0.0014 / ppl +0.53 % |
| 6 | legacy conformance | **holds** | 38 integer catalogue kernels + `RequantizeByToken` byte-identical on the live code; the whole dense A16 tier by D-F1 (9,373,742 legacy node rows equal) |
| 7 | static verifiability | **holds with one stated window** | `tir_admit_v1` admits every corpus and HF-lowered program at the legacy court's ceilings; DeepSeek-V3 needs a window ≤ `2^16` |

Open before the freeze (§8): Mamba's real-checkpoint row and the recurrence drift of Qwen3.5 and
Jamba (quantisation and calibration in the lowerer — none needs a primitive), and Gemma-3 / Mamba-2 /
RWKV-4 real checkpoints (none available under the download rules). Criteria 1–4, 6 and 7 hold.

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
where a learned position table is shorter), reference evaluator vs the f32 reference.

| family (thresholds top-1 / mean KL / ppl Δ) | fixtures | top-1 | mean KL, max (median) | max |ppl Δ| | meets |
| --- | --- | --- | --- | --- | --- |
| C1/C2 dense (≥ 0.85 / ≤ 0.10 / ≤ +5 %) | 39 | 0.903–1.000 | 2.3e-3 (1.4e-4) | 2.07 % | yes |
| C3/C8 MoE (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | 9 | 0.972–1.000 | 5.2e-4 (1.0e-4) | 0.77 % | yes |
| C4 GDN / C7 GDN hybrids (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | 4 | 0.931–0.986 | 1.2e-2 (3.1e-3) | 1.29 % | yes |
| C5 SSM (≥ 0.80 / ≤ 0.15 / ≤ +8 %) | 3 | 0.944–1.000 | 1.4e-4 (6e-5) | 0.73 % | yes |
| C7 Jamba (loosest component) | 1 | 0.944 | 7.1e-4 | 0.41 % | yes |
| C6-adjacent RWKV-4 (≥ 0.75 / ≤ 0.20 / ≤ +10 %) | 1 | 1.000 | 6e-5 | 0.04 % | yes |

### 5.2 Real checkpoints (trained weights: the quantisation is good enough)

One public checkpoint of ≤ 3B per family where one exists (§9 lists what was downloaded, with
revisions). Calibration 8 × 128 tokens and evaluation 2 × 128 held-out tokens of this repository's
`docs/*.md` (named per run), tokenised offline with each checkpoint's own `tokenizer.json`;
integer program vs the f32 reference, teacher-forced.

| family | checkpoint | top-1 | mean KL (max) | ppl float → integer | meets |
| --- | --- | --- | --- | --- | --- |
| C1 dense (Qwen2 arch) | Qwen/Qwen2.5-1.5B-Instruct (on disk) | 0.973 | 0.00138 (0.0063) | 60.37 → 60.69 (+0.53 %) | yes |
| C1 dense (Llama architecture) | `HuggingFaceTB/SmolLM2-1.7B-Instruct` | 0.973 | 0.00303 (0.0246) | 47.98 → 47.71 (−0.56 %) | yes |
| C1 Phi (partial rotary, parallel block) | `microsoft/phi-1_5` | 0.980 | 0.00046 (0.0057) | 94.98 → 95.08 (+0.10 %) | yes |
| C3/C8 MoE (GraniteMoE: 32 experts, top-8) | `ibm-granite/granite-3.1-1b-a400m-instruct` | 0.969 | 0.01061 (0.3241) | 51.00 → 52.45 (+2.85 %) | yes |
| C5 SSM (Mamba) | `state-spaces/mamba-370m-hf` | **0.758** | **0.20887** (0.6941) | 74.42 → 87.77 (**+17.94 %**) | **no** |
| C7 hybrid (Jamba: attention + Mamba + MoE) | `ai21labs/Jamba-tiny-dev` | 0.973 | 0.00396 (0.0524) | 354.36 → 354.08 (-0.08 %) | yes |
| C4/C7 gated delta + gated attention (Qwen3.5) | `Qwen/Qwen3.5-0.8B` | 0.953 | 0.00674 (0.0419) | 66.87 → 67.42 (+0.81 %) | yes |

The integer side ran on the typed backend (`--exec`), each run cross-checked against the reference
evaluator on its first two positions (equal); peak RSS 1.7–4.7 GB, except Qwen3.5-0.8B's 7.7 GB,
where the reference evaluator's `i128` copy of the 248,320-row embedding for the cross-check is most
of it (Jamba-tiny-dev is AI21's development model: its float perplexity is 354).

**Mamba misses its row** — a quantisation problem, not an IR one (corpus-v1 §9): the tiny Mamba
fixtures (random weights) agree at 0.94–1.00 and the program is the reference evaluator's value on
both backends, so the scan is lowered right; it is the trained checkpoint's activations that 16-bit
codes at one static scale per site do not hold. Diagnosis (`palw-tir-fidelity --sites 40
--site-positions 12`, the traced float run against the reference evaluator): the error is already
6–9 % (relative L2) inside layer 1 and flat from there — the gated product `y·SiLU(z)` 8–10 %, the
scan output 6.5–8 %, the in-projection's `x` and `z` halves, the conv/SiLU gate and the next norms
6.5–7 % — so it is not the `i32` scan state (its output error equals its input's) but the chain of
16-bit elementwise sites between the in-projection and the out-projection, whose per-channel ranges
a trained Mamba spreads far wider than a projection's inputs (where the outlier split holds them).
The fix is in the lowering, not the IR: carry the mixer's per-channel path (`x_in` → causal conv →
SiLU → scan → gate) on the `i32` rail, with the Q24 `silu` template instead of a 16-bit table —
what the gated-delta lowering already does for its conv and decays. Open (§8).

**Recurrence drift** (corpus-v1 §9: KL at position 4,096 ≤ 1.5 × KL at 128, C4–C7), measured as the
mean KL over positions 64–192 against 3,968–4,096 of one 4,096-token held-out sequence
(`docs/palw-rc-threat-model.md`), with the calibration above (8 × 128 tokens):

| family | checkpoint | KL 64–192 | KL 3,968–4,096 | ratio | over all 4,096 positions | meets |
| --- | --- | --- | --- | --- | --- | --- |
| C4/C7 gated delta | `Qwen/Qwen3.5-0.8B` | 0.00614 | 0.02150 | **3.50** | top-1 0.938, KL 0.0131, ppl +0.14 % | **no** |
| C7 hybrid | `ai21labs/Jamba-tiny-dev` | 0.00212 | 0.41083 | **193** | top-1 0.914, KL 0.0736, ppl −2.86 % | **no** |
| C7 hybrid, **long calibration** | `ai21labs/Jamba-tiny-dev` | 0.00210 | 0.00094 | **0.45** | top-1 0.983, KL 0.00144, ppl +0.12 % | **yes** |
| C5 SSM | `state-spaces/mamba-370m-hf` | — | — | — | not run (its 128-position row already misses; stopped for time) | — |

With the short calibration both recurrent checkpoints drift past 1.5×. **The cause is the
calibration, not the recurrence:** the 8 × 128-token set sizes every static scale of a state or a
long-context activation on short-context magnitudes, which saturate by position 4,096. Adding ONE
held-out 4,096-token sequence to the calibration (`docs/palw-mainnet-audit-2026-08-28.md`, a
different document from the evaluation's) takes Jamba-tiny-dev from ×193 to ×0.45 — the late window
now agrees better than the early one — and its 4,096-position fidelity to top-1 0.983 / KL 0.0014 /
ppl +0.12 %. The policy that follows: a recurrent class is calibrated on at least one sequence as
long as its evaluated context. Qwen3.5-0.8B's re-run with it is the open measurement (§8).

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

## 8. Open before the freeze

1. **Mamba (C5) misses its real-checkpoint row** (top-1 0.758, KL 0.209, ppl +17.9 %): the mixer's
   16-bit elementwise path loses the trained model's per-channel range (§5.2); the fix is a wide
   (`i32`) mixer path in the lowering — no primitive, no IR change.
2. **Recurrence drift:** with a 128-token calibration Qwen3.5-0.8B drifts ×3.5 and Jamba-tiny-dev
   ×193; one long calibration sequence brings Jamba to ×0.45 (§5.2). Qwen3.5-0.8B's re-run with the
   long calibration is the open measurement (about an hour on this host: its f32 reference over
   4,096 positions is the slow part).
3. **One run passed the 8 GB memory rule**: Qwen3.5-0.8B's first 4,096-position drift peaked at
   13.8 GB (both sides' 4,096 × 248,320 logits rows kept). Fixed: on the typed backend each integer
   row is compared and dropped (tir/lower, this commit's predecessor); the float side's rows remain
   (4 GB for that vocabulary).
4. **No real checkpoint under the download rules** for Gemma-3 (every official checkpoint is gated),
   Mamba-2 (no official HF-format checkpoint of ≤ 3B: `mistralai/Mamba-Codestral-7B-v0.1` is 7B,
   `state-spaces/mamba2-*` are `mamba_ssm` checkpoints in `.bin`) and RWKV-4 (every official `RWKV/`
   repository is `pytorch_model*.bin` only). Their fixtures pass (§5.1); a real run needs the user to
   accept Gemma's licence, or to allow a converted `.bin` (loaded without pickle) or a larger or
   community-ported checkpoint.
5. RWKV-5/6/7 fidelity cannot be measured offline (§1).

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
1 × 4,096 (`docs/palw-rc-threat-model.md`), each with the checkpoint's own `tokenizer.json`, offline.

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
