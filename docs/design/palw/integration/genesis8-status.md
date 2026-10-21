# Genesis 8 — status of the genesis model set (2026-10-16)

Status vocabulary: **implemented (unverified)** = source written and committed, not compiled or run by the lane that wrote it;
**converted** = an artifact exists with recorded hashes; **e2e adjudicable** = every layer kind of the model has a route on which a
public outside verifier can localize a computation fault and a deterministic court convicts it (PRINCIPLES §1–§3, §8: sampled
deterministic re-execution and spot checks, never a cryptographic proof of computation). Nothing in this document is a PASS.

Branch: `integ/genesis10`. Registry (pins, sizes, LFS SHA-256): `scripts/genesis/genesis8_models.json`. Consensus list (dormant):
`consensus/core/src/palw_genesis8_catalog_v1.rs`.

## 1. The set (user decisions 2026-10-15 and 2026-10-16)

The user chose ten models, then dropped #7 (Gemma 4 26B-A4B, overlapping #8) and #9 (Qwen3-32B, overlapping #6), then switched
every source to the uncensored variant (huihui abliterated; Dolphin where no huihui variant exists), then fixed #5 to the BF16
release and #6 to the bf16 GGUF of a community SFT. On 2026-10-16 the user moved #8 to huihui's abliterated Qwen3.6-35B-A3B GGUF
(Q8_0), rebuilt at 32K into a new repository; the earlier official-derived note no longer applies and **every entry is an
uncensored variant**. Ids keep the original numbering.

**Served context: 32,768 tokens for every genesis model** (user decision 2026-10-16). The GDN classes (#2 #3 #6) are calibrated on
one 32,768-token sequence (freeze-v1 §5.2); the dense TIR classes (#4 #5 #10) declare 32,768; #1 is re-converted with the A16
converter at `--n-ctx 32768`; #8 is rebuilt with `qwen36-convert --context 32768` (a PALWQ361 artifact bakes its context into its
header and rotary table; the earlier 512-position runtime repository stays as history).
Memory design for the long context: [RFC-0020](../../../rfc/0020-long-context-calibration-memory.md).

**Scope: text generation only** (user policy 2026-10-15): token ids in, next-token logits out. Vision towers and projectors
(mmproj), image/audio/video embedders and processors, and multi-token-prediction (MTP) heads are not computed, not covered, not
admitted as claims and not rewarded; image paths will not be enabled. Refused by name in the artifact (`check_text_scope_v3`: a
program with a job input, an out-of-scope param, tensor or file, or a program kind other than TIR/A16/PALWQ361) and on the claim
route (`palw_genesis8_admits_input_v1`); the pipeline never fetches mmproj or processor configs.

| # | Source @ pinned revision | Architecture | License | Route | Conversion | e2e adjudication | Artifact repository |
|---|---|---|---|---|---|---|---|
| 1 | `dphn/Dolphin3.0-Qwen2.5-1.5B` @ `60c996e2` | Qwen2ForCausalLM (dense) | Apache-2.0 | BASE-0 A16 (legacy step court) | **to re-convert at 32K**: `qwen25-convert --a16 --n-ctx 32768` (the A16 width is part of the artifact digest), then carried in PALWART3 — implemented in the pipeline (`converter: a16`), not run. History: the 8k legacy `.palwart` published 2026-10-15 (`qwen25-convert --a16 --n-ctx 8192` @ 0b1c11b87: 1,799,359,436 B, sha256 `894e6914…5438`, artifact digest `50e8acbe…c217`, FAITHFUL top-1 46/57 top-5 55/57) stays in the repository | covered (A16 kernels in the shipped step court); A16's calibration is the converter's fixed 57-token prompt — no long-context check exists for the A16 engine | `Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW` (added under `palwart/`) |
| 2 | `huihui-ai/Huihui-Qwen3.5-2B-abliterated` @ `b2e291a6` | Qwen3_5ForConditionalGeneration (GDN hybrid; text decoder) | Apache-2.0 | TIR (K2-TIR) | a run was started by the coordinator with an earlier pipeline (`--gdn-core-wide`, 8k calibration, PALWART3 direct); result not recorded here. **Re-run with the current pipeline at 32K**: that run's token file embeds the machine's paths (part of the root), its manifest predates the scope and calibration records, and its served context (8,192) is not the genesis 32,768 | covered at the family level (K2-TIR; tests written) — kernel not active | `Misakachain/Huihui-Qwen3.5-2B-abliterated-PALW` |
| 3 | `huihui-ai/Huihui-Qwen3.5-9B-abliterated` @ `05b9e7c9` (**tentative**: no official Qwen3.8 9B) | Qwen3_5ForConditionalGeneration | Apache-2.0 | TIR | not run | as #2 | `Misakachain/Huihui-Qwen3.5-9B-abliterated-PALW` |
| 4 | `huihui-ai/Huihui-gemma-4-12B-it-abliterated` @ `060ea173` | Gemma4UnifiedForConditionalGeneration (dense; text decoder) | Apache-2.0 | TIR | not run (adapter added) | text path covered at the family level; image path out of scope (text generation only) | `Misakachain/Huihui-gemma-4-12B-it-abliterated-PALW` |
| 5 | `huihui-ai/Huihui-gpt-oss-20b-BF16-abliterated` @ `b73ec897` | GptOssForCausalLM (MoE, sinks) | Apache-2.0 | TIR | not run | covered at the family level | `Misakachain/Huihui-gpt-oss-20b-BF16-abliterated-PALW` |
| 6 | `huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF` @ `a0b17c71`, file `…-bf16.gguf` (54.66 GB, sha256 `fcddc1ff…1d53`); base: community SFT `nerkyor/Qwen3.8-27B-Coder390-…` | GGUF `qwen35` (GDN hybrid) | Apache-2.0 | TIR | not run; end-to-end streaming implemented (unverified) | as #2 (see the `output_gate_type` note, §6) | `Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW` |
| 8 | `huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF` @ `acd5abe0`, file `…-ggml-model-Q8_0.gguf` (37,801,096,416 B; f16 at 71.07 GB is the alternative); base `Qwen/Qwen3.6-35B-A3B` | GGUF `qwen35moe` (GDN hybrid + MoE; text decoder) | Apache-2.0 | PALWQ361 (legacy step court, Q36 profile) | **to rebuild at 32K, streamed both ends** (`converter: palwq36`, implemented, unverified, not run): a measuring pass (`qwen36-convert --reference-only --measured-out`, the GGUF by HTTPS range, whole-file SHA-256 against the LFS id, every window's SHA-256 recorded), then an output pass (`--measured-in --url-windows-in --out FIFO`) into `palwart pack --from-pipe` (2 GiB segments uploaded and deleted). The trailing MTP block is dropped; the stored Q8_0 is dequantized and re-quantized per channel with power-of-two group exponents (not integer-exact). History: `Misakachain/Qwen3.6-35B-A3B-PALW-runtime` @ `bbc2f6c3` (512 positions; its profile pins a Claude-4.7-Opus-abliterated Q4_K_M) stays as it is | the Q36 step court covers its layer kinds; GDN solo/hybrid v2 court merged (unverified) | **new** `Misakachain/Huihui-Qwen3.6-35B-A3B-abliterated-PALW` |
| 10 | `huihui-ai/Llama-3.3-70B-Instruct-abliterated` @ `fa133346` | LlamaForCausalLM (dense) | Llama 3.3 Community License | TIR | not run | **not required** (user decision); the families are covered | `Misakachain/Llama-3.3-70B-Instruct-abliterated-PALW` (name begins with "Llama"; card says "Built with Llama"; LICENSE, USE_POLICY.md, NOTICE ship with it) |

#6's `mmproj-model-bf16.gguf` (the vision projector) is listed among the registry's excluded files and never fetched (scope).
#8's `mmproj-model-f16.gguf` is excluded likewise. In the #8 GGUF the tensors are stored head first (`output.weight`,
`output_norm.weight`, `token_embd.weight`), then `blk.0` … `blk.40` in numeric order (`blk.40` is the MTP block): the hashed reader
catches up over the head when the embedding is read first and hashes the rest in order, so each pass downloads about one file.

## 2. Coverage: architecture × layer kind

`covered` = a route exists on which the kind lowers and is checked and adjudicated (TIR: every primitive's constraint family has a
checker and a court in K2-TIR-v1…v4; legacy: a step-court kernel). "Before" is `integ/rfc02-rfc06-12-g14` @ 52ee224b7, "after"
is this branch. The K2-TIR kernels are **implemented, not active** on any network: activation is a separate fenced decision.

| Architecture (genesis #) | Layer kind | Before | After |
|---|---|---|---|
| Qwen2 dense (#1, A16) | embedding, RMSNorm, RoPE θ=1e6, GQA attention with q/k/v bias, SwiGLU, tied head | covered (legacy A16) | covered; container wraps the A16 payload unchanged |
| Qwen3.5/3.8 GDN hybrid (#2 #3 #6) | Gated DeltaNet (in_proj qkv/z/a/b, causal conv1d, L2 q/k, softplus·A decay, sigmoid β, delta-rule state, gated RMSNorm) | lowers; state = RecurrentState/StateContinuity; **9B fit refused** at the default GDN grid (KL 0.268) | `--gdn-core-wide` on the CLI and in every genesis GDN conversion (KL 0.0039 on the full 9B, `docs/evidence/hf-onboarding/20261010-…-fit-diagnosis`) |
| | full attention with sigmoid output gate, (1+w) qk-norm, partial interleaved mRoPE (text) | covered | covered |
| | MLP SwiGLU, head | covered | covered |
| | GGUF source (#6): `qwen35.attention.recurrent_layers` | **refused** (unmapped key) | accepted when it agrees with `full_attention_interval` |
| | vision tower, MTP head | not lowered (text decoder class) / dropped by design | out of scope: refused by name (`check_text_scope_v3`, `palw_genesis8_admits_input_v1`) |
| Gemma-4 Unified (#4) | wrapper `Gemma4UnifiedForConditionalGeneration` | **refused** (no decoder adapter) | built-in `vlm-gemma4-unified` + `gemma4-unified-text` |
| | sliding/full attention, full layers' own head width and single KV head, k = v, proportional partial rope, per-head q/k norms, weightless v norm, sandwich norms, layer scalar, GeGLU (tanh), final logit soft-cap 30 | covered by `gemma4-text` | covered through the new adapters |
| | image path (patch LayerNorm, patch_dense, 2-D position table, image tokens attended bidirectionally) | missing (`ATTN_PREFIX_LM_V1`, FR-20) | out of scope (text generation only; refused by name) |
| | audio embedder | not in scope | out of scope (refused by name) |
| gpt-oss (#5) | attention sinks, alternating sliding 128 / full, YaRN (truncate=false), top-k router with softmax over the k, clamped SwiGLU (α 1.702, limit 7), expert biases, fused interleaved experts | covered | covered |
| | MXFP4 expert weights | decoded to floats then re-quantised (not integer-exact) | not needed: #5 is the BF16 source; `MXFP4_EXACT_INT` section kind reserved |
| Qwen3.6-35B-A3B (#8, PALWQ361) | GDN, 4-tap conv, L2 norm, partial RoPE, MoE top-k router (one per table), grouped wide matmuls | covered by the Q36 step court | + GDN solo/hybrid v2 court and composed maps (GDN lane, merged, unverified) |
| | conversion | — | `qwen36-convert` from the abliterated Q8_0 GGUF at `--context 32768`, two passes, streamed both ends (bit-identical to the one-pass file: `misaka/palw/base0/tests/qwen36_streamed_output.rs`) |
| Llama 3 (#10) | llama3 rope, GQA, SwiGLU | covered | covered; real-scale court/seat resources unmeasured (not required) |
| all TIR-route classes | RFC-0006 cell coverage (`palw_tir_cell_coverage_v1`, every relation/routing/carry/state/door) | per class | asserted on the real headers of #2 #3 #4 #5 #10 (`misaka/palw/sdk/tests/genesis8_coverage.rs`, written) |
| all classes | registration binding of the PALWART3 root | — | **missing** (§5) |

Per-architecture end-to-end tests (written, not run) — `misaka/palw/kernel/tests/genesis8_court.rs`: a tiny checkpoint of each
architecture (Qwen2, Qwen3.5 GDN default and `gdn_core_wide`, Qwen3.5-MoE, Gemma-4 Unified, gpt-oss, Llama 3) is lowered,
calibrated and materialised as the converter does; the reference evaluator produces the honest trace; under K2-TIR-v2 the honest
claim passes, a lie at one committed value of every constraint family the program uses (dense matrix, nonlinear, selection, quant
range, exact arithmetic, recurrent state) is localized to that primitive instance and convicted by a court holding only public
material, and the same accusation against the honest claim is dismissed.

## 3. One artifact format: PALWART3

`<stem>.palwart` (index: magic `PALWART3`, version 3, borsh header, trailing root) plus `<stem>.<k:05>.palwseg` segments (2 GiB by
default), `misaka/palw/tir/artifact/src/palwart_v3.rs`. Sections: canonical manifest (source repo + revision pin, license,
attribution, conversion), program record (TIR program + kernel id = graph-IR root, or the A16 / PALWQ361 engine), the weight
section (`base0-a16`, `palwq361-int`, `tir-inventory`; `mxfp4-exact-int` reserved) with the legacy identity as its inner root,
tokenizer, chat template, source metadata, license texts, calibration inputs. The root covers content only (not offsets, segment
cuts, layout or provenance). Legacy `.palwart` (PALWB0A1/A2), `.palwtir` (PALWTIR1/2) and `.palwq36` (PALWQ361) stay readable under
their own magics; `palwart extract-legacy` re-assembles them byte for byte. The catalog's `artifact_root` is the PALWART3 root.

Tools: `palwart` (light crate, no consensus dependency): `pack`, `inspect`, `verify`, `root`, `sniff`, `extract-legacy`;
`palw-tir-convert --palwart-*` writes the container directly while converting; `misaka_palw_sdk::palwart_deep` re-derives the inner
roots with the consensus functions.

## 4. The pipeline: `scripts/genesis/genesis10_pipeline.py`

Steps per model, recorded in `<work>/state/<id>.json` (resumable): preflight → download (pinned revision, include patterns,
SHA-256 against the LFS ids, anonymous) → convert (PALWART3) → record → fit (the quality guard, §4.1) → upload (`hf upload`, one
commit per segment) → verify (remote SHA-256 / git blob) → cleanup; results in `<work>/genesis10_results.json`. `--dry-run`,
`--model N`, `--all`, `--stream`, `--local-source`, `--no-upload`, `--keep`, `--calib-positions N`, `--no-fit`. The HF token is
never read or printed.

### 4.1 Calibration: what it is for, how long, how fast

* **What it measures.** For every site of the float reference, the activation range: absmax, the maxima at position 0 and at later
  positions, per-channel maxima, sum of squares. The converter turns these into each site's fixed-point scale; weights are
  quantised per row independently of them. The integer program, and every check and court over it, is exact and deterministic
  for **any** pinned calibration set — calibration decides only how well the integer program fits the float model.
* **The length.** A recurrent (Gated DeltaNet) program needs one sequence as long as the context it serves (freeze-v1 §5.2; the
  converter refuses a longer context). With the 32K decision, #2 #3 #6 calibrate on **one 32,768-token sequence** (the first 32K
  tokens of RFC-0002 at the pinned blob). Dense classes (#4 #5 #10) keep four 512-token texts and declare 32,768: their sites have
  no state growing with the position; the long fit check below tests them at 32K where it can run. `--context N` overrides the
  served context (and a recurrent class's calibration length) for experiments.
* **Reproducible.** Texts pinned as git blobs at one commit; a canonical token file with repository-relative names (it is in the
  root); the manifest's `conversion.calibration` records texts, tokenizer, `tokenizers` version, lengths, served and calibrated
  context, policy, the token file's SHA-256 and the platform. Off that platform, rebuild from the carried statistics
  (`--stats-in`); re-measured statistics can differ in the last bits where `exp`/`tanh` come from another libm.
* **Calibration measured elsewhere** (`--calib-stats-in FILE`): statistics from a calibration-only machine
  (`palw-tir-fidelity <model> --calib calib-tokens.json --calibrate-only --stats-out calib-stats.json [--gdn-core-wide]` on this
  run's token file) are read, not re-measured; the converter still checks the calibration-length rule on the token file
  (`palw-tir-convert` now accepts `--stats-in` with `--calib`).
* **Speed, bit for bit.** The float reference runs every block **node by node over runs of 32 positions** (each matrix read once
  per run; `PALW_FLOAT_POSITION_BLOCK=1` for position by position), with `linear_raw` / `linear_raw_many` on independent
  accumulators and attention / Gated DeltaNet heads on separate threads. Attention stays one query position at a time over every
  visible key (absolute causal rule) — not chunked attention. Statistics and logits are bit-identical to the one-thread,
  position-by-position reference (`misaka/palw/tir/lower/tests/position_blocks.rs`, `float_ref/tests.rs`; written, not run).
* **Memory** ([RFC-0020](../../../rfc/0020-long-context-calibration-memory.md)): consumed carries (one full-sequence carry instead
  of two: #2 256 MiB, #3 512 MiB, #6 640 MiB saved at 32K); row-streamed calibration off by default (with it on, a dense tensor
  over its cache is re-read whole at every position; a warning says so); no 32K × 32K score matrix on any genesis path (audit,
  RFC-0020 §5.1); opt-in bit-identical carry spill (`--spill-carries-dir`); `--ram-accounting` writes live/peak bytes by kind and
  stage and the resident set to `<model dir>/ram-<step>.jsonl`.
* **Fit guard** (`fit` step, before the index upload): (1) `palw-tir-fidelity --exec --stats-in` on pinned held-out text
  (README.md, docs/PRINCIPLES.md; 2 × 64 tokens): mean KL ≤ 0.1, top-1 ≥ 0.8; (2) **the long-context check**: the next 32K tokens
  of RFC-0002 (held out from the calibration's first 32K), every position compared as it is made (`--drift-windows 32`): mean KL
  ≤ 0.1, top-1 ≥ 0.8, every window's mean KL ≤ 0.1, late/early ≤ 1.5 (waived when the late window's KL ≤ 0.01). Out of tolerance
  → the index, manifest and card are not uploaded. Runs where the source is on disk and the integer program fits
  (`--fit-max-gb 20`): #2, #3, #4. #5 (22.6 GB), #6 (streamed; 29.5 GB) and #10 are recorded as not measured, with the reason.
  `--no-fit-long` skips the long check (recorded).

Estimates at 32K (unmeasured; M1 Max 8P+2E, 32 GB; base: the 9B's measured 3.5 GMAC/s and #2's 2.1 GMAC/s before the kernel
changes; ±50%; RFC-0020 §9):

| | Calibration 32K | Conversion | Long fit (float + integer) | Peak RAM (calibration) |
|---|---|---|---|---|
| #2 | 1.5–2 h | ~10 min | 1.5–2 h + ~0.5 h | 2.5–3.5 GB (fit ~5–6 GB) |
| #3 | 4.5–7 h | ~30 min | 4.5–7 h + ~1 h | 5–6 GB (fit ~16 GB) |
| #6 (streamed GGUF) | 13–19 h | 1.5–3 h (HTTPS re-read, upload) | not on this host | 8–11 GB |
| #8 (PALWQ361, streamed both ends) | measuring pass 20–40 min | output pass 35–70 min (≈ 37 GB uploaded) | not on this host | ~7–9 GB (a layer's dequantized experts, 1.5 GiB window cache, ~0.75 GB of parameter rows ×2) |

#6 on this Mac: 15–22 h in all — under 24 h only just, and uncertain. Fastest correct path: a calibration-only machine (64 cores,
≥ 64 GB, the GGUF on disk) running `palw-tir-fidelity --calibrate-only --stats-out` (≈3–4 h, also able to run the long fit),
then this Mac converts with `--calib-stats-in` (1.5–3 h).

What it needs:

* binaries: `cargo build --release -p misaka-palw-tir-artifact --bin palwart`,
  `cargo build --release -p misaka-palw-tir-lower --features remote --bin palw-tir-convert --bin palw-tir-fidelity` (the `remote`
  feature only for #6's HTTPS-streamed GGUF) and, for #1 and #8, `cargo build --release -p misaka-palw-base0 --bin qwen25-convert
  --bin qwen36-convert`;
* `hf` CLI (`/Users/wata/Downloads/hf-upload-venv/bin/hf`) and a valid login (`hf auth login --force`) for uploads — `--stream`
  uploads during conversion, so the login must be valid before it starts;
* a Python with `tokenizers` for the calibration tokens (`/Users/wata/Downloads/MISAKA-wt-b/tir-venv/bin/python`);
* `curl` (#6 streaming).

Disk plan (estimates from the registry: output ≈ 1.08 × parameters as int8 codes; `plan` / `plan --stream` print them):

| # | Input | Output (est.) | Peak disk, whole | Peak disk, `--stream` | Notes |
|---|---|---|---|---|---|
| 1 | 3.1 GB (bf16 safetensors) | ~1.8 GB (A16 at 32K) | ~8.5 GB | ~8.5 GB | `qwen25-convert` reads the checkpoint whole and writes the A16 file before packing |
| 2 | 4.5 GB | 2.5 GB | 9.0 GB | 8.7 GB | single shard |
| 3 | 19.3 GB | 10.4 GB | 31.7 GB | 14.9 GB | 4 shards, two resident |
| 4 | 23.9 GB | 12.9 GB | 38.8 GB | 28.1 GB | one 24 GB shard |
| 5 | 41.8 GB | 22.6 GB | 66.4 GB | 14.0 GB | 9 shards |
| 6 | 54.66 GB (bf16 GGUF) | 29.5 GB | 86.2 GB | **~4 GB** | HTTPS range reads, never stored; ≈2.5–3× the file downloaded (two passes; lexicographic layer order in the file) |
| 8 | 37.8 GB (Q8_0 GGUF) | ~37 GB | ~43 GB (local GGUF, output still piped) | **~5 GB** | GGUF by HTTPS range, read twice (measure, then output; ≈ 2 × 37.8 GB downloaded); one 2 GiB segment and the ~0.75 GB measuring record on disk |
| 10 | 141.1 GB | 76.2 GB | 219.3 GB | 14.1 GB | 30 shards |

The pipeline keeps a 30 GB free-disk floor (`--min-free-gb`) for the validation agent that shares the disk.

RAM (unmeasured estimates, §4.1 table): calibration runs layer by layer, one occurrence's params resident; at 32K one
full-sequence carry (#6 640 MiB), one attention layer's history (#6 256 MiB), per-query scores only. The 9B 8k run measured 21.4 GB
RSS with an mmapped checkpoint (mapped pages count). Measure with `--ram-accounting`.

## 5. Open items

* **#8's fit at 32K is not measurable on this Mac:** no tool compares the PALWQ361 engine with the f32 reference at windows along a
  long sequence (`qwen36-run --reference` scores the converter's 8-token prompt), the f32 reference over 32K positions of the
  35B-A3B MoE is another full read of the GGUF and hours of compute, and the integer engine needs the ~37 GB artifact on local
  disk against 32 GB of RAM. Recorded as not measured in the run; the measuring pass's log carries the reference's next-token
  check. Needs a ≥ 64 GB host and a windowed comparator for the legacy engine (not written).
* **#8's calibration** is the legacy converter's own 8-token prompt with the analytic horizon correction of the GDN state exponents
  to 32,768 (`qwen36_calibrate::horizon_growth_bits`), not a 32K sequence: the freeze-v1 §5.2 rule binds the TIR route, and the
  PALWQ361 route has no 32K evidence until the fit above is measured.
* **The fit of #5, #6 and #10** is not measured on this host (§4.1); measure it on a host with the source on disk and RAM for the
  integer program before the index is relied on.
* **#1 at 32K**: the A16 route has no long-context check (qwen25-convert's FAITHFUL report covers its 57-token prompt).
* **Registration binds the PALWART3 root** — implemented, dormant (2026-10-18, `integ/g14-tir-pricing-r2` `87af99d36`): the
  registrant binds a registered TIR class to its container (tag 175 `TirClassContainerBoundV1`, fence
  `palw_tir_container_identity_v1`, None on every preset); the chain recomputes the container root from its opening and accepts it
  only if the one weight section's inner root is the class's inventory root and the program record is the class's program and
  tokenizer; past the fence an unbound class's TIR original is refused. Tests: `palw_tir_container_v1::tests`,
  `tir_fp_lane_past_the_container_fence_…`, `misaka-palw-sdk tests/tir_container_binding` (a real packed container). Not done: the
  restart/IBD/reorg restore of the row, and IR attempt-lane claims are not gated by it.
* **K2-TIR activation**: the kernels are implemented, not active; arming them is a fenced decision outside this lane.
* **#3** awaits the user's decision; **#6**'s calibration tokenizer is the Qwen3.8 tokenizer from
  `huihui-ai/Huihui-Qwen3.8-27B-abliterated@739e3c5b`, checked id by id against the GGUF's own vocabulary before use.
* **Past refusal of Huihui-Qwen3.5-9B** (RFC-0011 P1): the fixture stays (`misaka/palw/tir/lower/tests/fixtures/vlm-generic/
  huihui-qwen3.5-9b`, `misaka/palw/sdk/tests/coverage_p1_huihui_refusals.rs`). Causes: (a) the conversion fit refusal
  (HF_FIT_OUT_OF_TOLERANCE) — fixed by `gdn_core_wide`, which every genesis GDN conversion sets; (b) refusals 1–3 (court clock,
  close sizing, logits tile) — resolved under the rules in force, pinned by those tests; (c) refusal 4 (`HELD_CLASS_UNANSWERABLE`) —
  a property of the legacy held route, not of the TIR route the 9B uses; (d) `SEAT_MEMORY_SHORT` — a mine/readiness estimate against
  the fixed t12 seat tiers (3.5 / 8 GiB), not a conversion refusal: RFC-0006 layer-sharded seats or capable seats address it.

## 6. Compile and semantic risks

* Nothing here was compiled. The float reference's threaded kernels (`linear_raw`, `linear_raw_many`, `attention`,
  `gated_delta_traced`) must be bit-identical to the one-thread forms; the tests compare bits, not tolerances, against verbatim
  copies of the old kernels.
* The node-by-node runs of positions (`Session::eval_occurrence_run`) are the largest semantic risk: an attention must read exactly
  the history rows a position-by-position run would have appended (`hist_rows`), and a block with any other stateful op must fall
  back. `tests/position_blocks.rs` compares statistics digests and logits with the old two-sequence, position-by-position
  algorithm (copied verbatim) on seven architectures, and the existing fidelity / golden tests run through the new path by default;
  `PALW_FLOAT_POSITION_BLOCK=1` restores position by position without a rebuild.
* The highest-risk changes: `GgufFile` now holds `Option<Arc<dyn ByteSourceV1>>` (the trait carries a
  `Debug` supertrait for the derive); `ConvertRequest`/`ConvertOutcome` gained fields (all constructors go through
  `ConvertRequest::new` / `convert_model`); new built-in adapters change `adapter::builtin::pack_hash()` (recorded by runtime packs,
  not compared on verify); `misaka-palw-tir-artifact` gained `sha2` (Cargo.lock updated by hand).
* Qwen3.8's safetensors config carries `output_gate_type: "swish"`; the GGUF has no such key and is lowered with the qwen35
  attention output gate, as llama.cpp runs it. Compare against a float reference before genesis.
* The Gemma-4 Unified adapter reads `hidden_size_per_layer_input` and `enable_moe_block` only to refuse a config that enables them;
  `vocab_size_per_layer_input` is inert.
