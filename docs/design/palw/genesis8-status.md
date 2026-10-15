# Genesis 8 — status of the genesis model set (2026-10-15)

Status vocabulary: **implemented (unverified)** = source written and committed, not compiled or run by the lane that wrote it;
**converted** = an artifact exists with recorded hashes; **e2e adjudicable** = every layer kind of the model has a route on which a
public outside verifier can localize a computation fault and a deterministic court convicts it (PRINCIPLES §1–§3, §8: sampled
deterministic re-execution and spot checks, never a cryptographic proof of computation). Nothing in this document is a PASS.

Branch: `integ/genesis10`. Registry (pins, sizes, LFS SHA-256): `scripts/genesis/genesis8_models.json`. Consensus list (dormant):
`consensus/core/src/palw_genesis8_catalog_v1.rs`.

## 1. The set (user decisions 2026-10-15)

The user chose ten models, then dropped #7 (Gemma 4 26B-A4B, overlapping #8) and #9 (Qwen3-32B, overlapping #6), then switched
every source to the uncensored variant (huihui abliterated; Dolphin where no huihui variant exists), then fixed #5 to the BF16
release and #6 to the bf16 GGUF of a community SFT. Ids keep the original numbering.

| # | Source @ pinned revision | Architecture | License | Route | Conversion | e2e adjudication | Artifact repository |
|---|---|---|---|---|---|---|---|
| 1 | `dphn/Dolphin3.0-Qwen2.5-1.5B` @ `60c996e2` | Qwen2ForCausalLM (dense) | Apache-2.0 | BASE-0 A16 (legacy step court) | **converted** (legacy `.palwart`, `qwen25-convert --a16 --n-ctx 8192` @ 0b1c11b87: 1,799,359,436 B, sha256 `894e6914…5438`, artifact digest `50e8acbe…c217`, tokenizer `c41926f5…96df`, FAITHFUL top-1 46/57 top-5 55/57); PALWART3 wrap of the same payload: implemented (unverified), not run | covered (A16 kernels in the shipped step court) | `Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW` (legacy file @ `816208c4`) |
| 2 | `huihui-ai/Huihui-Qwen3.5-2B-abliterated` @ `b2e291a6` | Qwen3_5ForConditionalGeneration (GDN hybrid; text decoder) | Apache-2.0 | TIR (K2-TIR) | a run was started by the coordinator with this branch's pipeline (`--gdn-core-wide`, 8k calibration, PALWART3 direct); its result is not recorded here | covered at the family level (K2-TIR; tests written) — kernel not active | `Misakachain/Huihui-Qwen3.5-2B-abliterated-PALW` |
| 3 | `huihui-ai/Huihui-Qwen3.5-9B-abliterated` @ `05b9e7c9` (**tentative**: no official Qwen3.8 9B) | Qwen3_5ForConditionalGeneration | Apache-2.0 | TIR | not run | as #2 | `Misakachain/Huihui-Qwen3.5-9B-abliterated-PALW` |
| 4 | `huihui-ai/Huihui-gemma-4-12B-it-abliterated` @ `060ea173` | Gemma4UnifiedForConditionalGeneration (dense; text decoder) | Apache-2.0 | TIR | not run (adapter added) | text path covered at the family level; **image path missing** | `Misakachain/Huihui-gemma-4-12B-it-abliterated-PALW` |
| 5 | `huihui-ai/Huihui-gpt-oss-20b-BF16-abliterated` @ `b73ec897` | GptOssForCausalLM (MoE, sinks) | Apache-2.0 | TIR | not run | covered at the family level | `Misakachain/Huihui-gpt-oss-20b-BF16-abliterated-PALW` |
| 6 | `huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF` @ `a0b17c71`, file `…-bf16.gguf` (54.66 GB, sha256 `fcddc1ff…1d53`); base: community SFT `nerkyor/Qwen3.8-27B-Coder390-…` | GGUF `qwen35` (GDN hybrid) | Apache-2.0 | TIR | not run; end-to-end streaming implemented (unverified) | as #2 (see the `output_gate_type` note, §6) | `Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW` |
| 8 | `huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated` @ `8f0ee727` | Qwen3_5MoeForConditionalGeneration (GDN hybrid + MoE) | Apache-2.0 | PALWQ361 (legacy step court, Q36 profile) | **blocked**: no safetensors→PALWQ361 converter (the legacy `qwen36-convert` reads GGUF only) | the Q36 step court covers its layer kinds; GDN solo/hybrid v2 court merged from the GDN lane (unverified) | `Misakachain/Huihui-Qwen3.6-35B-A3B-abliterated-PALW` |
| 10 | `huihui-ai/Llama-3.3-70B-Instruct-abliterated` @ `fa133346` | LlamaForCausalLM (dense) | Llama 3.3 Community License | TIR | not run | **not required** (user decision); the families are covered | `Misakachain/Llama-3.3-70B-Instruct-abliterated-PALW` (name begins with "Llama"; card says "Built with Llama"; LICENSE, USE_POLICY.md, NOTICE ship with it) |

#6 also pins `mmproj-model-bf16.gguf` (0.93 GB, the vision projector) in the registry; it is not converted (the genesis class is
the text decoder).

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
| | vision tower, MTP head | not lowered (text decoder class) / dropped by design | unchanged |
| Gemma-4 Unified (#4) | wrapper `Gemma4UnifiedForConditionalGeneration` | **refused** (no decoder adapter) | built-in `vlm-gemma4-unified` + `gemma4-unified-text` |
| | sliding/full attention, full layers' own head width and single KV head, k = v, proportional partial rope, per-head q/k norms, weightless v norm, sandwich norms, layer scalar, GeGLU (tanh), final logit soft-cap 30 | covered by `gemma4-text` | covered through the new adapters |
| | image path (patch LayerNorm, patch_dense, 2-D position table, image tokens attended bidirectionally) | missing (`ATTN_PREFIX_LM_V1`, FR-20) | **missing** (text-only class) |
| | audio embedder | not in scope | not in scope |
| gpt-oss (#5) | attention sinks, alternating sliding 128 / full, YaRN (truncate=false), top-k router with softmax over the k, clamped SwiGLU (α 1.702, limit 7), expert biases, fused interleaved experts | covered | covered |
| | MXFP4 expert weights | decoded to floats then re-quantised (not integer-exact) | not needed: #5 is the BF16 source; `MXFP4_EXACT_INT` section kind reserved |
| Qwen3.6-35B-A3B (#8, PALWQ361) | GDN, 4-tap conv, L2 norm, partial RoPE, MoE top-k router (one per table), grouped wide matmuls | covered by the Q36 step court | + GDN solo/hybrid v2 court and composed maps (GDN lane, merged, unverified) |
| | conversion from safetensors | **missing** | **missing** (blocked; §5) |
| | TIR route alternative: fused experts `mlp.experts.gate_up_proj [E, 2I, D]` | unbound (per-expert names only) | unbound (probe test written, ignored) |
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
SHA-256 against the LFS ids, anonymous) → convert (PALWART3) → record → upload (`hf upload`, one commit per segment) → verify
(remote SHA-256 / git blob) → cleanup; results in `<work>/genesis10_results.json`. `--dry-run`, `--model N`, `--all`,
`--stream`, `--local-source`, `--no-upload`, `--keep`. The HF token is never read or printed.

What it needs:

* binaries: `cargo build --release -p misaka-palw-tir-artifact --bin palwart` and
  `cargo build --release -p misaka-palw-tir-lower --features remote --bin palw-tir-convert` (the `remote` feature only for #6's
  HTTPS-streamed GGUF; both crates build without the consensus and kernel crates);
* `hf` CLI (`/Users/wata/Downloads/hf-upload-venv/bin/hf`) and a valid login (`hf auth login --force`) for uploads — `--stream`
  uploads during conversion, so the login must be valid before it starts;
* a Python with `tokenizers` for the calibration tokens (`/Users/wata/Downloads/MISAKA-wt-b/tir-venv/bin/python`);
* `curl` (#6 streaming).

Disk plan (estimates from the registry: output ≈ 1.08 × parameters as int8 codes; `plan` / `plan --stream` print them):

| # | Input | Output (est.) | Peak disk, whole | Peak disk, `--stream` | Notes |
|---|---|---|---|---|---|
| 1 | 1.8 GB (legacy A16 file) | 1.8 GB | 5.6 GB | 5.9 GB | no re-quantisation |
| 2 | 4.5 GB | 2.5 GB | 9.0 GB | 8.7 GB | single shard |
| 3 | 19.3 GB | 10.4 GB | 31.7 GB | 14.9 GB | 4 shards, two resident |
| 4 | 23.9 GB | 12.9 GB | 38.8 GB | 28.1 GB | one 24 GB shard |
| 5 | 41.8 GB | 22.6 GB | 66.4 GB | 14.0 GB | 9 shards |
| 6 | 54.66 GB (bf16 GGUF) | 29.5 GB | 86.2 GB | **~4 GB** | HTTPS range reads, never stored; ≈2.5–3× the file downloaded (two passes; lexicographic layer order in the file) |
| 8 | 71.9 GB | 38.8 GB | 112.7 GB | 12.1 GB | blocked (no converter) |
| 10 | 141.1 GB | 76.2 GB | 219.3 GB | 14.1 GB | 30 shards |

The pipeline keeps a 30 GB free-disk floor (`--min-free-gb`) for the validation agent that shares the disk.

RAM (unmeasured estimates): calibration runs layer by layer with large params bound lazily (`--calib-lazy-mib 64`); the 9B 8k run
measured 21.4 GB RSS with an mmapped checkpoint. #6 at 8k: ~15–22 GB (the largest single buffers: the f32 embedding / head,
~5.1 GB each; 8k logits up to ~8 GB; a full-attention layer's scores ~6.4 GB if materialised). `--context 4096` halves the
activation terms.

## 5. Open items

* **#8 converter**: a safetensors → PALWQ361 path (map the HF names of the fused experts and the GDN tensors onto the PALWQ361 writer,
  shard-streamed), or the TIR route with a fused-expert binding in `qwen3-5-moe.json`. Neither is written. **Option needing a user
  decision:** huihui publishes the same abliterated weights as GGUF (`huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF` @
  `acd5abe0`, f16 71.07 GB or Q8_0 37.80 GB, Apache-2.0, base `Qwen/Qwen3.6-35B-A3B`); the frozen `qwen36-convert` reads `qwen35moe`
  GGUF over HTTPS ranges (`--url --header`) and now drops the trailing MTP block (`nextn_predict_layers`, the same rule as the TIR
  GGUF reader). Its output is the legacy `.palwq36` written locally (~33–36 GB) and then packed (`palwart pack`, segments uploaded
  and deleted): with ~64 GB free and the 30 GB floor that does not fit today; a two-pass writer (triples first, then the file streamed
  into segments) would remove the local file.
* **Registration binds the PALWART3 root**: today a TIR class binds its inventory root; the next network's registration must bind the
  container root and the court must open the inner inventory leaves under the inner root it commits. Consensus work, not started.
* **K2-TIR activation**: the kernels are implemented, not active; arming them is a fenced decision outside this lane.
* **Image paths** (#2 #3 #6 vision towers, #4 image embedder): not in the text-decoder classes.
* **#3** awaits the user's decision; **#6**'s calibration tokenizer is the Qwen3.8 tokenizer from
  `huihui-ai/Huihui-Qwen3.8-27B-abliterated@739e3c5b`, checked id by id against the GGUF's own vocabulary before use.
* **Past refusal of Huihui-Qwen3.5-9B** (RFC-0011 P1): the fixture stays (`misaka/palw/tir/lower/tests/fixtures/vlm-generic/
  huihui-qwen3.5-9b`, `misaka/palw/sdk/tests/coverage_p1_huihui_refusals.rs`). Causes: (a) the conversion fit refusal
  (HF_FIT_OUT_OF_TOLERANCE) — fixed by `gdn_core_wide`, which every genesis GDN conversion sets; (b) refusals 1–3 (court clock,
  close sizing, logits tile) — resolved under the rules in force, pinned by those tests; (c) refusal 4 (`HELD_CLASS_UNANSWERABLE`) —
  a property of the legacy held route, not of the TIR route the 9B uses; (d) `SEAT_MEMORY_SHORT` — a mine/readiness estimate against
  the fixed t12 seat tiers (3.5 / 8 GiB), not a conversion refusal: RFC-0006 layer-sharded seats or capable seats address it.

## 6. Compile and semantic risks

* Nothing here was compiled. The highest-risk changes: `GgufFile` now holds `Option<Arc<dyn ByteSourceV1>>` (the trait carries a
  `Debug` supertrait for the derive); `ConvertRequest`/`ConvertOutcome` gained fields (all constructors go through
  `ConvertRequest::new` / `convert_model`); new built-in adapters change `adapter::builtin::pack_hash()` (recorded by runtime packs,
  not compared on verify); `misaka-palw-tir-artifact` gained `sha2` (Cargo.lock updated by hand).
* Qwen3.8's safetensors config carries `output_gate_type: "swish"`; the GGUF has no such key and is lowered with the qwen35
  attention output gate, as llama.cpp runs it. Compare against a float reference before genesis.
* The Gemma-4 Unified adapter reads `hidden_size_per_layer_input` and `enable_moe_block` only to refuse a config that enables them;
  `vocab_size_per_layer_input` is inert.
