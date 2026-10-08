# Coverage closure P1/P2 — record (COV-P1P2, 2026-10-08)

Branch `cov/p1p2-huihui-features` (base `9050b06bc`, integration HEAD `5048b7880` merged in `23a8f51d2`). User directive "Model
Onboarding — Coverage Closure", Priorities 1 and 2. Every verdict below names its kind; they are **never** merged:

* **shape-ready** — lowered from config + headers, the program builds;
* **unit-test PASS** — a test in this repository passes;
* **synthetic** — an artifact root, beacon or fact made up for the test;
* **dormant registration** — the class is accepted by the gate and folded on the real node path at 0‰ (Registered/Dormant), no
  conformance, no beacon, no G14 activation;
* **full-task Active/Final** — never claimed here.

No unarmed fence, no DAA-9,000 list entry, no `public_consensus_params_id` or schedule id moved. No allocation was used.

## 1. P1 — `huihui-ai/Huihui-Qwen3.5-9B-abliterated` @`05b9e7c9b978ba29bdb8f50a49c30e4b91183339`

Inputs: config.json, the index and the four safetensors HEADERS (19,306,310,880 B in all) by HTTP range, committed as
`misaka-palw-tir-lower/tests/fixtures/vlm-generic/huihui-qwen3.5-9b` (no weight byte). Text decoder: **32 layers = 24 Gated DeltaNet
+ 8 full attention** (not 30 GDN: that count is Qwen3.6-35B-A3B's), hidden 4096, 8,953,803,264 text parameters (vision 456 M and MTP
243 M are not the text class). Probe: `misaka-palw-sdk/tests/coverage_p1_huihui_9b.rs` (ignored; header snapshot).

### 1.1 The four past refusals, pinned (`misaka-palw-sdk/tests/coverage_p1_huihui_refusals.rs`, 5/5 PASS)

| # | Past refusal | Exact structured refusal now | Commit point | Where it stands |
|---|---|---|---|---|
| 1 | "Court 5,102 DAA > 3,000" | `COURT_WINDOW_TOO_SHORT {needed 5102, window 3000}` from the OLD call `verify_class_admission_v6(…, false)` on the 9B's graph-v5 row (graph-v1/v2 rows: 5,060) — the LADDER clock, `83 × 42 + 1,616` | `palw_attn_court_admits_row_v1` (ladder) | the shared probe `palw_admission_probe_v1` reads the HELD clock (≤ 2,919 to 2^32 positions) and admits the court; a source guard pins that neither `model_class.rs` nor `sdk.rs` calls v6 again |
| 2 | `TIR_EXCEEDS_CEILING` 67,108,865 vs 67,108,864 | `TirExceeds {limit "IR close sizing work", value 2^26+1, cap 2^26}` — **`cap + 1` is the sentinel** of `palw_tir_carried_closes_admit_form_v1` | admission v10 step 9 (`palw_tir_admission_v1.rs:750`), the ELEMENT twin below `palw_tir_fence2` | **cause: tooling.** `TirOfflineGateV1` judged at `palw_tir_v1`'s own height (t12 DAA 2,000), below fence2 (3,600): `declare-layout` sized with the element twin. Measured at layout logits 256 / h_tile 32 / C 298 / 8,192: RANGE twin **41,490,156 steps** (0.618 × cap), costliest commit point `(1, 298)` (the GDN mixer output, whose cone replays the Fixed state over C) at 24,238,464; the ELEMENT twin crosses the cap at that same point and is **> 2.1·10^9 steps** (censored at a 2^31 cap after 472 s). **Fixed (`fa13939b9`):** the offline gate judges at the first height where every scheduled fence is in force (the preflight's default) |
| 3 | `COURT_COST_EXCEEDS_CEILING` 16,842,752 vs 16,777,216 | `CourtCostExceedsCeiling {what "IR tile multiply-accumulates", got 16842752, ceiling 16777216}` | admission v10 step 5 (`:638`), the logits commit point at the DEFAULT 4,096-lane tile (2^24 = 4,096 lanes × 4,096 hidden, + 65,536 in the cone) | a 2,048-lane tile clears it; the SDK search already narrows it (admitted at 256 lanes) |
| 4 | `HELD_CLASS_UNANSWERABLE` | `HeldClassUnanswerable {why: Recurrent {layers: 24}}` | the processor's attribution check beside the gate (ADR-0152 §4-ter C5, `processor.rs:11031`, `palw_registration_attribution_v1`), **legacy `ClassRegistered` arm only** | below DAA 5,300 the v2 row passes the gate and meets C5; past it the v2 row meets `GDN_MAP_ASSUMES_EQUAL_HEADS {16, 32}` and the v3 row passes the gate and meets C5 again. The IR route does not ask C5 (its GDN state is a Fixed state replayed from checkpoints, not a held site) |

### 1.2 Verdicts per acceptance case

**(a) 9B-8k text (the requested deployment) — registrable on testnet-12 today, IR route.**

| Rule (processor order) | Source | Verdict at DAA 7,000 |
|---|---|---|
| `palw_tir_v1` in force | `processor.rs:12761` | PASS (fence 2,000; no fence between 5,585 and 9,000) |
| one IR registration a block | `processor.rs:7873` | capacity, not a refusal |
| chain's target; signature, Active bond | `:12772`, `:12773` | stateful (the SDK builds it) |
| court window | `:12783` | network window 3,000 (held clock, model window inactive) — PASS |
| admission v10 steps 1–8 | `palw_tir_admission_v1.rs:501 … 737` | PASS: 32,640 B program; history bound SMALL (not held); context 8,192 ≤ 2^18; tile MACs, cone work at C 298, close bytes; J5b canonical (1,023, 2) ≤ 4,096 ids; ladder; DA reach; pwu; id; share 0‰ (no certified family covers the IR prims) |
| step 9 carried closes (range twin) | `:750` | PASS: 41,490,156 steps; worst close **3,171,766 B of 3,200,000** (0.9 % margin); root claim 67,013 of 100,000 |
| share rule | `processor.rs:12838` | required 0‰ — PASS |
| fold (`apply_class_registration_v1`) | `palw_state_v2.rs` | stateful (duplicate, slash value, activation ≤ 4,000 ahead, exposure + 1 MSK burn) |
| calibration length (tooling) | `tir_layout.rs::tir_calibration_covers_context_v1` | needs one calibration sequence ≥ 8,192 tokens (`calibrated_context` is now recorded by the streamed converter: integration `5048b7880`) |
| seat memory (mining, not registration) | `preflight/chain.rs` | not a registration rule |

Layout: tile 64, logits tile 256, h_tile 32, checkpoint interval 298 (also admitted at 512, 256, 16 and **32,783** — C 149 — at DAA
5,585 and 9,000). **Real node (dormant registration, synthetic root):** lane D's E2E carries
`fixtures/g14/shipped/huihui-qwen3.5-9b-8k.json` — `g14_real_checkpoints_at_32783_positions_mined_under_the_whole_release`: gate Ok,
chain accepted at DAA 41 of the compressed release, mempool → template → fold → persisted tip → reads → activation flip + a second
node's replay; `…_across_the_shipped_schedule`: gate and chain agree at 33 heights (refused below 3,600, admitted from it). 3/3 PASS
(743 s). **Not shown:** real artifact bytes (H1, 9B weights), conformance, beacon, G14, Active.

**K2 route at 8,192 (the kernel route's resource/court admission, `palw_probabilistic_constraints_v1` dormant, armed in the probe):**
`check_plan_v1` **PASS** (K2-TIR-v1, ε ≤ 2^-226; v2 2^-150; 838 relations; per position: verifier work 2.66·10^10, evidence
1,536,521,593 B, 2,210 probabilistic instances; artifact 3,508,121,330 B; worst court 2,034,245,636 B / 1,017,122,817 work). The real
node's fold then refuses, in order: (1) **"the artifact is not attested public by the consumer's registry"** — `attested_artifacts` is
empty in production (OB-P0's conformance/availability fact); (2) `public_prosecution_complete_v1`: public bytes **12,597,678,100,210 >
2^40**, retained state **6,985,614,336 > 2^32** (13,323 committed node values a position × 64 B × 8,192), concurrent sessions **8,192
> 1,024**; (3) `carrier_fit_v1` (a 2 GB worst court opening against `PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1`).

General fixes (design only — they change the frozen kernel gate or need a fence):
* **Per-position aggregate commitment** (one Merkle root over a position's node values, row/column openings as `merkle.rs` does for a
  MatMul): retained state 8,192 × 64 B = **524 KB** (from 6.99 GB).
* **Bounded segments** of ≤ 1,024 positions with continuity roots: concurrent sessions ≤ **1,024** per segment claim (8 segments at 8k).
* **Producer-vs-validator split of public bytes:** a prosecution fetches one position's evidence (**1.54 GB**) plus openings, not the
  claim's 12.6 TB; the whole-claim bytes are a producer/DA-retention resource with its own bound and availability sampling.
* **Court opening width:** the 2 GB worst opening is a whole-row court of the 248,320 × 4,096 head/table; a row-tiled opening (the
  logits tile of the IR court) bounds it near the carrier.

**(b) Full source (`Qwen3_5ForConditionalGeneration`: vision + 262,144) — not registrable; text-only is not full task.**
* The vision stage is a separate generative (RFC-0003) class; a text class is partial-task coverage and is labelled so.
* IR at 262,144: at the default tile the logits tile MACs (refusal 3); at every narrower logits tile the **first exact refusal is
  `CLASS_NOT_ATTRIBUTABLE`**: "a 32,767-id canonical prompt is past J5b's inline bound of 4,096 ids" (admission v10 step 6,
  `palw_tir_admission_v1.rs:678`). General fix: **authenticated prompt-ID tiles** for IR canonical jobs (RFC-0011 §4.B) — a consensus
  change under a new fence: design only.
* K2 at 262,144: `check_plan_v1` **BOUNDS_EXCEEDED**: claim evidence 1,288,999,796,584,178 B > 2^50; gate: public 1.29·10^15 B,
  retained 2.24·10^11 B, sessions 262,144. General fix: bounded segments with per-segment plan bounds.

**(c) Validated 2M — not registrable, and not a property of the source.** The source declares 262,144 positions; 2M needs a
versioned positional extension with fidelity evidence before any gate. IR (held program, history bound 2^21): first exact refusal
`TIR_CLASS_REFUSED` "a program at the held history bound is not admitted by IR v1" (step 2), then the network `max_context` 262,144
(step 3), then J5b (262,143 ids). K2: BOUNDS_EXCEEDED claim evidence 1.03·10^16 B > 2^50; gate: public 1.03·10^16, retained
1.79·10^12, sessions 2,097,152. General fixes: the three context changes of RFC-0011 §4.B together (IR held bindings, prompt tiles,
the network context cap) plus K2 segments — new fence; design only.

### 1.3 Expected 9B-8k artifact

8,953,803,264 text parameters at the measured W8 ratio (SmolLM2-1.7B: 1.87 GB class file) ≈ **9.8 GB**; peak disk during a streamed
conversion ≈ source 19.3 GB + artifact 9.8 GB + transient chunk store ≤ 9.8 GB ≈ **39 GB**, steady ≈ 29 GB.

## 2. P2 — common features

### 2.1 Text decoder inside a multimodal wrapper (`vlm-generic`, `369187c20`)

One data-driven route for a `…ForConditionalGeneration` no adapter names: the wrapper's `text_config` read by the decoder adapter its
`model_type` selects; the decoder prefix found in the index (`$tensor_prefix_scan`; two candidates refused by name); head found;
tying by the wrapper's flag or a headless index (an unstated flag with a head present is refused). The transformers key table now
lists every wrapper type with a `text_config` (277 types; the 126 existing entries byte-identical). SmolVLM-256M (Idefics3): 273 text
tensors bound from real headers, 198 vision/connector tensors unread by design; Huihui: the same decoder as `vlm-qwen3-5`. **Text-only
class: partial-task coverage**, never full-task. Tests: `tests/vlm_generic.rs` 7/7.

SmolVLM's text class through the probe (`coverage_p1_huihui_9b`, real headers): **IR gate ADMITS at 512 positions, DAA 7,000**
(logits tile 512). K2-TIR-v1 at 512: the kernel gate PASSES, then **`carrier_fit_v1` REFUSES** — the worst court filing is
113,609,103 B against `PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1` = 1,583,616 (worst court work 28,385,857 ≈ the 49,280 × 576 head:
the same general cause as the 9B's — row-tiled court openings); the gate itself passed (public 24.2 GB, 512 sessions). At 8,192: public bytes 2.15·10^12 and 8,192 sessions refused (per-position roots and
segments, as §1.2).

### 2.2 FR-09 — DeepSeek sparse attention, per family

| Family | Status | Evidence |
|---|---|---|
| `deepseek_v32` | lowering + adapter existed; **unit-test PASS**; IR gate **admits** the tiny fixture at 128 positions at DAA 7,000 (6 dissected commit points, ≤ 9 reductions each of 16); K2-TIR-v1 plan **ELIGIBLE_AT** (hyp.; shipped `KERNEL_NOT_ACTIVE`) | `tests/dsa.rs`, `misaka-palw-sdk/tests/coverage_p2_dsa.rs`. The corpus' "refuted: float_vs_hf" is `torch.topk`'s unpinned tie order (IR: lowest index), not a semantic gap. Released **DeepSeek-V3.2 FP8 is refused by name: `quantization_config.scale_fmt = ue8m0` is unread by `FP8_BLOCK`** (a descriptor decision); its bf16 config reads |
| `glm_moe_dsa` | **new adapter (data)** `glm-moe-dsa` extends `deepseek-v32`: ONE indexer, rotaries interleaved (rope layout as data); fixture: float = HF to 8.3e-7 with the IR tie rule, integer top-1 1.000 / KL 0.00094; IR gate admits @128; K2 ELIGIBLE_AT | `tests/dsa_glm.rs` 5/5; GLM-5 (real config) reads; **GLM-5.1+/5.3 refused by name: cross-layer top-k sharing (`indexer_types` `shared`) = `ATTN_TOKEN_INDEXER_SHARED_V1`, not lowered** |
| `hy_v4` | same indexer (half-split rope) **with shared layers by default** (full at 0 and every 4th) → `ATTN_TOKEN_INDEXER_SHARED_V1` | read, not built |
| `axk2` | the DeepSeek-V3.2 indexer; beyond FR-09 an MLA **query gate** (`q_gate_proj` over `[q_resid, q_compressed]`) — a separate feature | read, not built |
| qwen4-exp QSA | **a different pattern**: block-level selection — an indexer over mean-pooled key BLOCKS, a fixed-K `TopK` over the `Fixed` `[blocks]` axis, the incomplete tail visible; DSA is token-level, a counting threshold over `H`. Shared: only the masked-dense application (`Select(vis, logits, MIN)` before the softmax) | nothing to share in the selection; reported, not merged |

**K2 relation for the counting-threshold selection.** The selection lowers to `Compare`/`ReduceSum` over `H`, `Select`/`ReduceMax`
over 15 candidates, 8–10 passes; K2-TIR-v1's plan covers every node with an implemented family (exact recompute, instance court), so
the selection needs no new family. A cheaper relation — check the committed `τ'` by two counts, `#{κ ≥ τ'} ≥ k` and `#{κ > τ'} < k`,
O(2H) instead of O(~150·H) — would be a new checker (a kernel extension under the Lead's allocation): design only.

**Court at `τ'` and the masked rows.** `τ'` is a commit point (two `i32` lanes); its cone (≤ 10 reductions at `2^18`, ≤ 9 at the
fixtures' 128) is dissected by the k-ary court like an attention history cone; the masked rows are ordinary attention logits after a
`Select`. Admission v10 admits both fixtures, so the obligations O-1…O-5 hold at that shape.

**What still stands between a DSA model and a registration:** (1) `ATTN_TOKEN_INDEXER_SHARED_V1` for GLM-5.1+/HY-V4 — a later layer
reads the earlier full layer's selection, which needs a cross-occurrence carry of the full layer's query/weights and `τ'` and a read
of its key-history state: TIR-structure support (consensus-visible) — design only; (2) every public DSA checkpoint is ≥ 321 B
parameters (DeepSeek-V3.2 671 B, GLM-5.x 744 B, GLM-5.3-Flash 321 B): far past the per-position MAC/court ceilings — a resource route
(RFC-0006 sharding / K2 segments), not FR-09; (3) FP8 `scale_fmt`.

### 2.3 Custom quant: the off-chain deterministic decoder path

The path exists as data (`quantfmt`: one interpreter, a descriptor per format, test vectors from an independent implementation
checked at load, descriptor name + digest in the artifact's provenance; an unknown type refused by name with its id and tensors —
`QUANT_NO_DESCRIPTOR`). Added:

* **PTQ1_0 (ggml 143) and PQ2_0 (ggml 142)** — descriptors from PrismML-Eng/llama.cpp **@`7dffb158de30ebb8ef9d64f33c6b0b2d7c1e6313`**
  (release `prism-b10685`), read as text with the user's approval, never built or run (files fetched: `ggml/src/ggml-common.h`
  `96539c9b…`, `ggml/src/ggml-quants.c` `46dd353f…`, `src/models/qwen35.cpp` `5881e237…`, `src/llama-graph.{cpp,h}` `e787cd5a…`/`47a57026…`,
  `src/llama-model.cpp` `bef2985a…`, `src/llama-impl.{h,cpp}` `c9346ca1…`/`b01ad5b5…`, sha256). PTQ1_0: 128 weights per 28 B — `qs[24]`
  (5 base-3 digits a byte, stages of 16 then 8 bytes), `qh[2]`, fp16 `d` last; `W = d·(digit − 1)`. PQ2_0: fp16 `d` first, `qs[32]` of
  2-bit codes, `W = d·(q − 1)`. Independent check: `tools/prism_quant_reference.py` (pure Python, `-I`, a literal transcription of the C
  loops) wrote the descriptors' test vectors, and on the REAL Mitsuba file the descriptor's decode of the first 8 rows of three tensors
  equals the reader's byte for byte (BLAKE2b of the f32; ternary counts equal). `tests/prism_quant.rs` 3/3 (1 ignored, run).
* **The class commits to the decoded integers** (code, zero 1, fp16 scale per 128 — the TQ1_0/Q2_0 lowering path); the court sees
  only those.
* **Open gap:** provenance records the descriptor name and digest and the converter version, not yet the SOURCE FILE hash; a
  `source_files: [{name, bytes, hash}]` entry in the converter's meta is the small follow-up (convert.rs is integration's now).

### 2.4 Weight-space rotation (`WEIGHT_ROTATION_HADAMARD_V1`, `e27535d69`)

From the same source: `build_lora_mm` computes `y = W'·(H·(s ⊙ x))` for every listed weight; the token row is restored as
`s ⊙ (H·z)`; `H[i][j] = (−1)^popcount(i & j)/√B`. **Not absorbed** (`ssm_alpha`/`ssm_beta` read the same activation unrotated;
`ffn_down`/`ssm_out`/`attn_output` sit after nonlinearities), so it is an **activation op**, a generic frontend feature: spec
`input_rotations`/`embed_rotation` (serialised only when set: every existing spec digest unchanged), HL `Op::BlockLinear`, TIR batched
`MatMul` of the param's `[n/B, B, B]` `i8` rows (±127 exact) with the codes `[n/B, B, 1]`, one narrowing — existing primitives, no
kernel extension. The GGUF reader validates the declaration as the producer does and finds the rotated roles from the BINDING (no
role table); non-uniform layers, a rotated weight no projection reads, experts, a tied head, a tiled-order GDN fold and any unknown
key are refused by name. **Proof:** a tiny Qwen3.5 folded by an independent pure-Python script computes the unrotated model to
2.5·10^-5 on a logit scale of 43; its integer program top-1 0.986 / KL 0.0028. `tests/weight_rotation.rs` 3/3 (+1 ignored).

### 2.5 Mitsuba (`isichan-ai/Mitsuba_and_HiMitsuba-27B-GGUF` @`33e63d45`, PTQ1_0, text decoder only)

* The repository answers **HTTP 401** to anonymous requests since ~18:20 on 2026-10-08 (private or gated after H1's approved
  download); the local copy, sha256-verified against metadata captured while public (apache-2.0), is the evidence — devnet and
  toolchain use only. README/docs read as text; no bridge script or repo code was downloaded or run.
* Frontend: `qwen35` mapped; `nextn_predict_layers 1` → the MTP block `blk.64` (15 tensors) **dropped by name** (the main forward
  never reads it; llama.cpp loads it "but not executed in the main pass"); `prism.hadamard` modelled (401 weights, 14 HL roles found
  from the binding, embedding restored); type 143 decoded by data. Before this work the reader would have silently ignored `prism.*`.
* Staged check (`mitsuba_first_layers_integer_follows_float_with_the_rotation`, ignored; 4-layer prefix + head, streamed float
  reference with lazy large params; release build, `/usr/bin/time -l`): the real file's first 4 layers (3 GDN + 1 attention,
  all 14 roles rotated, embedding restored) + the 248,320-row head — calibrated on 136 sites, 5,215 MiB materialised, **integer vs
  float top-1 0.958, KL 0.00366 (max 0.0073) over 24 positions** (bound top-1 ≥ 0.9, KL ≤ 0.05), 575 s. Max RSS 10,677,436,416 B
  (9.94 GiB — at the 10 GB line that governs Python; this is the Rust test, not killed, 0 swaps); the f32 head of the float
  reference (~5 GB) dominates, so a smaller prefix would not lower it — a row-streamed head would. A prefix is a staged check of
  the frontend on real rows, never the class.
* **IR gate on the FULL file** (probe `coverage_p1_huihui_9b` with the GGUF as input, 8,192 positions, DAA 7,000, `COV_P1_SIZE=1`;
  headers only, no weight read): program 32,811 B, 4 blocks, 158 params, 4 states (2 fixed), 53 commit points. The default layout
  is refused `COURT_COST_EXCEEDS_CEILING` (20,971,520 > 16,777,216: logits tile 4,096 × hidden 5,120); logits tiles 2,048/1,024/256
  at interval 272 are refused `TIR_EXCEEDS_CEILING` (the sizing's `2^26 + 1` sentinel: work past the cap); the search finds **logits
  tile 256, h_tile 32, interval 136 → ADMITTED** (118 s). Range-twin sizing (fence2, live from 3,600): **56,636,451 steps = 0.844×
  the 2^26 cap**, costliest point (2, 225) at 30,750,454 steps (a `Reshape → [Fixed(6144)]`, 3 h-reductions); worst close
  2,292,203 B (≤ 3,200,000), worst root claim 68,333 B (≤ 100,000). So the 27 B text class fits the IR gate at 8k with 16 % sizing
  headroom; a larger model of this shape will not without a finer interval.
* **Artifact estimate:** 29,098,688,419 B params (+ 32,811 B program): the ternary codes are committed a byte each (27 B × ~1.07).
  A packed 2-bit code tensor would be ~4× smaller — an artifact-format opportunity (consensus-visible: param encoding), not done.
* **Verdict:** shape-ready / unit-test PASS for the text decoder's frontend, quantisation and rotation; **IR gate ADMITS the full
  27 B text class at 8,192 (DAA 7,000, synthetic root, headers only)**. Not registered: no real artifact (29 GB; H1 / disk), no
  calibration, no conformance, no G14; the K2 route has the 9B's general refusals (§1.2); the vision stage (mmproj) is out of scope
  — text-only, partial task.

## 3. Tooling fixes (off-chain) and guards

* `TirOfflineGateV1` judges where every scheduled fence is in force (`fa13939b9`) — refusal 2's cause.
* GGUF: an unmodelled metadata namespace is refused by name (`a0f914b27`) — the silent-acceptance hole `prism.*` found.
* GGUF: `nextn_predict_layers` drops the MTP blocks by name, each required to carry `nextn.*` tensors.
* Diagnostics: `palw_tir_worst_closes_trace_v1` / `…_range_trace_v1` record the sizing's work per commit point (RFC-0011 §4.A
  `cap_exceeded_at`); they decide nothing.
* `GgufModel::with_prefix_layers(n)` — a staged check of a large file, never a class.

## 4. Tests (targeted; pass counts)

| Test | Count |
|---|---|
| `misaka-palw-sdk` `coverage_p1_huihui_refusals` | 5/5 |
| `misaka-palw-sdk` `coverage_p2_dsa` | 2/2 |
| `misaka-palw-sdk` lib `tir_layout` 4/4; `direct_tir_registration` 2, `improve_composite_tools` 5, `improve_eval_exec` 4, `runtime_pack` 9, `runtime_pack_beacon` 17, `tir_residency` 1; lib `lineages`/`preflight`/`tir_registration` 18 | all pass |
| `kaspa-consensus` lib `g14_real_checkpoint*` (lane D, with the 9B-8k fixture) | 3/3 |
| `misaka-palw-tir-lower`: `vlm_generic` 7, `gguf_unmodelled` 4, `prism_quant` 2 (+1 ignored, run), `weight_rotation` 3 (+1 ignored, run on the real Mitsuba file), `dsa_glm` 5, `gguf` 12, `gguf_rope_freqs` 4, `golden_lowering` 2, `real_configs` 32, `adapters` 3, `adapter_pins` 3, `hf_keys_ignored` 4, `dsa` 8, `quant_corpus` 1, `quant_decode_pins` 1, `determinism` 2, `torch_bin` 18; lib `adapter`/`hf_schema`/`hf_config` 6, `quantfmt` 17 | all pass |

## 5. Allocations needed

None used. Would be needed for: a cheaper selection checker (K2 family/checker id); `ATTN_TOKEN_INDEXER_SHARED_V1` (TIR structure:
cross-occurrence carry and state read → prim-set/version + fence); K2 per-position aggregate commitments / segments / per-prosecution
public-bytes bound (kernel gate, Lead + B); authenticated IR prompt tiles (consensus, fence).

## 6. Open gaps

* 9B: real artifact (H1), calibration ≥ 8,192 tokens, conformance/beacon, G14 — none run; registration shown dormant with a synthetic
  root only.
* K2 route for the 9B: the four general fixes above (design).
* Source-file hash in the converter's provenance.
* `ATTN_TOKEN_INDEXER_SHARED_V1`; AXK2's MLA query gate; FP8 `scale_fmt`.
* Mitsuba as a 27 B class: IR gate admits at 8k (0.844× sizing cap); real artifact (29 GB, byte-per-ternary-code) not built;
  the repo is 401 (local copy only).
