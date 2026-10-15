# Genesis 8 — status of the genesis model set (2026-10-16)

Status vocabulary: **implemented (unverified)** = source written and committed, not compiled or run by the lane that wrote it;
**converted** = an artifact exists with recorded hashes; **e2e adjudicable** = every layer kind of the model has a route on which a
public outside verifier can localize a computation fault and a deterministic court convicts it (PRINCIPLES §1–§3, §8: sampled
deterministic re-execution and spot checks, never a cryptographic proof of computation). Nothing in this document is a PASS.

Branch: `integ/genesis10`. Registry (pins, sizes, LFS SHA-256): `scripts/genesis/genesis8_models.json`. Consensus list (dormant):
`consensus/core/src/palw_genesis8_catalog_v1.rs`.

## 1. The set (user decisions 2026-10-15)

The user chose ten models, then dropped #7 (Gemma 4 26B-A4B, overlapping #8) and #9 (Qwen3-32B, overlapping #6), then switched
every source to the uncensored variant (huihui abliterated; Dolphin where no huihui variant exists), then fixed #5 to the BF16
release and #6 to the bf16 GGUF of a community SFT, then made #8 the existing **official-derived** PALWQ361 runtime (the only entry
that is not an uncensored variant). Ids keep the original numbering.

**Scope: text generation only** (user policy 2026-10-15): token ids in, next-token logits out. Vision towers and projectors
(mmproj), image/audio/video embedders and processors, and multi-token-prediction (MTP) heads are not computed, not covered, not
admitted as claims and not rewarded; image paths will not be enabled. Refused by name in the artifact (`check_text_scope_v3`: a
program with a job input, an out-of-scope param, tensor or file, or a program kind other than TIR/A16/PALWQ361) and on the claim
route (`palw_genesis8_admits_input_v1`); the pipeline never fetches mmproj or processor configs.

| # | Source @ pinned revision | Architecture | License | Route | Conversion | e2e adjudication | Artifact repository |
|---|---|---|---|---|---|---|---|
| 1 | `dphn/Dolphin3.0-Qwen2.5-1.5B` @ `60c996e2` | Qwen2ForCausalLM (dense) | Apache-2.0 | BASE-0 A16 (legacy step court) | **converted** (legacy `.palwart`, `qwen25-convert --a16 --n-ctx 8192` @ 0b1c11b87: 1,799,359,436 B, sha256 `894e6914…5438`, artifact digest `50e8acbe…c217`, tokenizer `c41926f5…96df`, FAITHFUL top-1 46/57 top-5 55/57); PALWART3 wrap of the same payload: implemented (unverified), not run | covered (A16 kernels in the shipped step court) | `Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW` (legacy file @ `816208c4`) |
| 2 | `huihui-ai/Huihui-Qwen3.5-2B-abliterated` @ `b2e291a6` | Qwen3_5ForConditionalGeneration (GDN hybrid; text decoder) | Apache-2.0 | TIR (K2-TIR) | a run was started by the coordinator with an earlier pipeline (`--gdn-core-wide`, 8k calibration, PALWART3 direct); result not recorded here. **Re-run recommended** with the current pipeline: that run's token file embeds the machine's paths (part of the root), its manifest predates the scope and calibration records, and its served context (8,192) differs from the new GDN default (2,048, §4.1) | covered at the family level (K2-TIR; tests written) — kernel not active | `Misakachain/Huihui-Qwen3.5-2B-abliterated-PALW` |
| 3 | `huihui-ai/Huihui-Qwen3.5-9B-abliterated` @ `05b9e7c9` (**tentative**: no official Qwen3.8 9B) | Qwen3_5ForConditionalGeneration | Apache-2.0 | TIR | not run | as #2 | `Misakachain/Huihui-Qwen3.5-9B-abliterated-PALW` |
| 4 | `huihui-ai/Huihui-gemma-4-12B-it-abliterated` @ `060ea173` | Gemma4UnifiedForConditionalGeneration (dense; text decoder) | Apache-2.0 | TIR | not run (adapter added) | text path covered at the family level; image path out of scope (text generation only) | `Misakachain/Huihui-gemma-4-12B-it-abliterated-PALW` |
| 5 | `huihui-ai/Huihui-gpt-oss-20b-BF16-abliterated` @ `b73ec897` | GptOssForCausalLM (MoE, sinks) | Apache-2.0 | TIR | not run | covered at the family level | `Misakachain/Huihui-gpt-oss-20b-BF16-abliterated-PALW` |
| 6 | `huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF` @ `a0b17c71`, file `…-bf16.gguf` (54.66 GB, sha256 `fcddc1ff…1d53`); base: community SFT `nerkyor/Qwen3.8-27B-Coder390-…` | GGUF `qwen35` (GDN hybrid) | Apache-2.0 | TIR | not run; end-to-end streaming implemented (unverified) | as #2 (see the `output_gate_type` note, §6) | `Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW` |
| 8 | `Misakachain/Qwen3.6-35B-A3B-PALW-runtime` @ `bbc2f6c3`, file `palw-runtime/qwen36-35b-a3b.palwq36` (36,492,831,232 B, sha256 `5fe1d15c…8b29`): the existing **official-derived** PALWQ361 runtime of `Qwen/Qwen3.6-35B-A3B`, not an uncensored variant | Qwen3_5MoeForConditionalGeneration (GDN hybrid + MoE; text decoder) | Apache-2.0 | PALWQ361 (legacy step court, Q36 profile) | **wrap only** (not re-converted): the file streamed anonymously at the pinned commit, its LFS SHA-256 checked before the index is written, carried byte for byte with its legacy id as the inner root (`palwart pack --from-url`); implemented (unverified), not run | the Q36 step court covers its layer kinds; GDN solo/hybrid v2 court merged from the GDN lane (unverified) | the same repository, added under `palwart/` (the runtime's own files are never replaced) |
| 10 | `huihui-ai/Llama-3.3-70B-Instruct-abliterated` @ `fa133346` | LlamaForCausalLM (dense) | Llama 3.3 Community License | TIR | not run | **not required** (user decision); the families are covered | `Misakachain/Llama-3.3-70B-Instruct-abliterated-PALW` (name begins with "Llama"; card says "Built with Llama"; LICENSE, USE_POLICY.md, NOTICE ship with it) |

#6's `mmproj-model-bf16.gguf` (the vision projector) is listed among the registry's excluded files and never fetched (scope).
#8's dedup check (`genesis10_pipeline.py dedup-check --model 8`, metadata only): the repository is Xet-enabled, the legacy file is
stored as Xet chunks (`4aab592f…`), its LFS id matches the pin and `hf_xet` is installed — every segment but the last is an exact
byte slice of that file, so the re-upload is **expected** to be cheap (new bytes ≈ the chunks at the 17 segment cuts plus the
small sections). An expectation, not a measurement.

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
| | conversion | — | not needed: the published official-derived runtime is wrapped byte for byte |
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
  for **any** pinned calibration set — calibration decides only how well the integer program fits the float model (a range set
  too low clips, too high loses resolution).
* **The minimum.** For exactness: any pinned, non-empty token set. For fit: enough ordinary text that each site's maximum is
  reached (the position-0 sink and the large activations ordinary prose triggers). A recurrent (Gated DeltaNet) program is the
  exception: its state's magnitude grows with the position, so it needs one sequence as long as the context it serves (freeze-v1
  §5.2; Jamba-tiny drifted ×193 at 4,096 positions with a 128-token calibration, ×0.45 with one 4,096-token sequence added). The
  converter refuses a context longer than the longest calibration sequence, so for #2 #3 #6 **the calibration length is the
  served context**.
* **Default `--calib-positions 2048`**: recurrent classes are calibrated on one 2,048-token sequence and served at 2,048
  (`--calib-positions 8192` serves 8,192 at about four times the calibration time). Dense classes (#4 #5 #10): four 512-token
  texts (2,048 positions, run in parallel), served at 8,192 — unchanged.
* **Reproducible.** The texts are git blobs at one pinned commit (`calibration_texts` in the registry; extracted with `git
  cat-file blob` and checked, so a later edit of a document does not move a conversion). The token file is canonical JSON with
  repository-relative names only: it travels in the artifact, so its bytes are part of the root. The manifest's
  `conversion.calibration` records the texts (path, commit, blob, SHA-256, token count), the tokenizer (repo@revision, SHA-256),
  the `tokenizers` version, lengths, served and calibrated context, the policy (`headroom16/32/resid`, `linear_weight_bits`,
  `gdn_core_wide`), the token file's SHA-256 and the platform. Off that platform, rebuild from the carried statistics
  (`palw-tir-convert --stats-in calib-stats.json`, same context, flags, manifest and calibration files); re-measured statistics can
  differ in the last bits where `exp`/`tanh` come from another libm.
* **All cores.** Before: one calibration sequence ran on one driver thread; only matrix rows were split across threads, each row
  a single chain of dependent f64 adds (latency-bound), and attention and the Gated DeltaNet recurrence ran head by head on one
  thread (#2 measured ≈330 % CPU). Now `linear_raw` runs four rows per thread with independent accumulators (x converted to f64
  once), and attention and Gated DeltaNet run their heads on separate threads. Every row and head is the same operations in the
  same order, so results are **bit-identical** to the one-thread kernels (`linear_raw_is_the_sequential_fold_bit_for_bit`,
  `attention_and_gated_delta_across_threads_are_the_one_thread_result` in `misaka/palw/tir/lower/src/float_ref/tests.rs`, written,
  not run): statistics and artifacts do not depend on the core count.
* **Row-streamed calibration is off by default** (`--calib-lazy-mib 0`). With it on, a dense tensor larger than its cache
  (512 MiB) is read again, whole, at every position (`ParamStore::view`); for #6's streamed GGUF that is a re-download of about
  1 GB per position and layer. The layer-major run already holds one occurrence's params at a time (at most 5.1 GB: #6's f32
  embedding or head), which fits 32 GB for every genesis model. The statistics are the same either way.
* **Fit guard** (`fit` step, before the index upload): `palw-tir-fidelity --exec --stats-in` on the conversion's own statistics
  and pinned held-out text (README.md and docs/PRINCIPLES.md, 2 × 64 tokens); tolerance mean KL ≤ 0.1 and top-1 ≥ 0.8 (the L2
  default tolerance) against the lowering's float reference. Out of tolerance → the index, manifest and card are not uploaded.
  It runs where the source is on disk and the integer program fits in RAM (`--fit-max-gb 20`): #2, #3, and #4 when its shard is
  on disk. #5 (22.6 GB), #6 (streamed) and #10 are recorded as "not measured", with the reason, and need a larger host.

Estimates (unmeasured; M1 Max 8P+2E, 32 GB). Base: the 9B's measured calibration (one 8,192-token sequence, 18,360 s ≈ 3.5 GMAC/s)
and the #2 run (7 layers × 8,192 positions in ≈1,600 s ≈ 2.1 GMAC/s); the threaded kernels are expected at 3–5× (four-row
accumulators ≈4× per core on the dot products, bounded by memory bandwidth for #6's f32 layers; heads in parallel):

| | 2,048 positions, before | 2,048, after | 8,192, after |
|---|---|---|---|
| #2 calibration | ~20–25 min | ~6–10 min | ~25–40 min |
| #6 calibration | ~4–4.5 h | ~1–1.5 h | ~4–6 h |
| #6 whole streamed run (calibration, conversion pass, ≈140–165 GB of range reads, 29.5 GB of segments uploaded) | ~6 h | ~2.5–4 h | ~6–9 h |
| #6 peak RAM (row streaming off) | — | ~8–12 GB (5.1 GB f32 embedding/head + its bf16 read + windows) | ~+0.5 GB |

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
| 8 | 36.5 GB (legacy `.palwq36`) | 36.5 GB | 75.0 GB | ~4 GB | wrap only; streamed with `palwart pack --from-url` (LFS SHA-256 checked); Xet dedup expected |
| 10 | 141.1 GB | 76.2 GB | 219.3 GB | 14.1 GB | 30 shards |

The pipeline keeps a 30 GB free-disk floor (`--min-free-gb`) for the validation agent that shares the disk.

RAM (unmeasured estimates): calibration runs layer by layer, one occurrence's params resident (row streaming off, §4.1); the 9B 8k
run measured 21.4 GB RSS with an mmapped checkpoint (mapped pages count). #6 (streamed, no mmap): ~8–12 GB at 2,048 positions (the
largest buffers: the f32 embedding / head, 5.1 GB each, and its bf16 read); calibration logits are dropped as they are made and
attention scores exist one position at a time.

## 5. Open items

* **The fit of #5, #6 and #10** is not measured on this host (§4.1); measure it on a host with the source on disk and RAM for the
  integer program before the index is relied on.
* **Long-context drift of the GDN classes** at their served context is not part of the fit guard (2 × 64 held-out tokens):
  `palw-tir-fidelity --drift` with one evaluation sequence of the served length measures it (#2 is cheap enough to try).
* **Registration binds the PALWART3 root**: today a TIR class binds its inventory root; the next network's registration must bind the
  container root and the court must open the inner inventory leaves under the inner root it commits. Consensus work, not started.
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

* Nothing here was compiled. The float reference's threaded kernels (`linear_raw`, `attention`, `gated_delta_traced`) must be
  bit-identical to the one-thread forms; the tests compare bits, not tolerances, against verbatim copies of the old kernels.
* The highest-risk changes: `GgufFile` now holds `Option<Arc<dyn ByteSourceV1>>` (the trait carries a
  `Debug` supertrait for the derive); `ConvertRequest`/`ConvertOutcome` gained fields (all constructors go through
  `ConvertRequest::new` / `convert_model`); new built-in adapters change `adapter::builtin::pack_hash()` (recorded by runtime packs,
  not compared on verify); `misaka-palw-tir-artifact` gained `sha2` (Cargo.lock updated by hand).
* Qwen3.8's safetensors config carries `output_gate_type: "swish"`; the GGUF has no such key and is lowered with the qwen35
  attention output gate, as llama.cpp runs it. Compare against a float reference before genesis.
* The Gemma-4 Unified adapter reads `hidden_size_per_layer_input` and `enable_moe_block` only to refuse a config that enables them;
  `vocab_size_per_layer_input` is inert.
