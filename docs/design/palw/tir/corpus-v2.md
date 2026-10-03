# PALW-TIR — the architecture corpus v2: can any model be added by data alone?

| Field | Value |
| --- | --- |
| Status | measured on 100 curated architectures plus a census of 111 further causal-LM families (transformers 5.17.0 / diffusers 0.40, tiny random-init fixtures), on `tir/corpus` after the merge of `tir/generic` `239179132`; the numbers below are regenerable reports, not prose |
| Lane | H (corpus and coverage), branch `tir/corpus` off `tir/generic` `1a4964205` |
| Question | RFC-0002's aim is that **no model needs a core developer**. One more supported model is worthless; only infrastructure that admits them all counts. This document measures it |
| Harness | `misaka-palw-tir-lower/tests/corpus_v2.rs` + `tools/corpus/` (fixture generator, manifest, adapters, blockers, report renderer) |
| Report | `tools/corpus/report.json` (the 100 curated entries) and `tools/corpus/census_report.json` (the census), machine-readable, one object per entry |
| Related | [`feature-requests.md`](feature-requests.md) (what is missing, precisely), [`hf-coverage.md`](hf-coverage.md) (the per-family lowering record), [`model-adapter-v1.md`](model-adapter-v1.md) (the data format), [`corpus-v1.md`](corpus-v1.md) (the primitive-set corpus, a different thing) |

## 1. What is measured

The harness plays a **third party**. It adds models with DATA ONLY — model-adapter files and (later) quant-format
descriptors — and never edits Rust. When data cannot express a model that is a missing generic *feature*; it is
reported, precisely, as a feature request (`feature-requests.md`) for the lowering lane. Nothing in the lowering crate was edited
for this document.

For every corpus entry the harness builds a tiny random-init model with `transformers`/`diffusers` (fixed seed,
weights re-randomised so every feature is live and rounded to bf16, saved, reloaded fresh in fp32 with eager
attention) and asks the generic frontend for its **support level**:

* **Level A** — the standard keys and tensor names suffice: the reader's own template (`standard-decoder`,
  itself data) reads the model with no adapter. *Confirmed* only if the model then passes every later stage;
  a template that reads a class without refusing and then produces a different function is **refuted** (§4.2).
* **Level B** — a data adapter (`misaka.palw.model-adapter.v1`) suffices: a built-in file, one this lane wrote
  (`tools/corpus/adapters/*.json`, `tools/corpus/census-adapters/*.json`, fed to the reader as a user-supplied file), or one the
  harness *synthesised* from the fixture (the convention search, §6). No Rust, no protocol change.
* **Level C** — a capability is missing: a *feature* of the lowering (lowerable with the existing 25 primitives),
  a *route* (a whole kind of model with no data-driven lowering), a *protocol* capability beyond the IR, a
  *primitive*, or a reference that is not in `transformers` (*remote* code). `tools/corpus/blockers.json` classifies
  every Level C entry; each class names the feature request that closes it.

A model counts at the level of the **first route whose whole pipeline holds**; a route that reads but fails a later
stage is recorded under `refuted_routes` and the entry falls through to the next route or to C.

The stages (one JSON object per entry in the report):

| stage | what it checks |
| --- | --- |
| read | `read_model` on the config and the checkpoint's tensor names: Level, features used, features MISSING |
| lower | HL graph → TIR program; primitives and commit-point roles reached |
| admit | `tir_admit_v1` at the legacy court's ceilings (cones, per-position cost, checkpoint interval) |
| bind | every checkpoint tensor is read by the program (an unread tensor is a feature we might be missing) |
| float_vs_hf | the Rust float reference against `transformers`' own logits on the same bf16 weights (≤ 1e-4 of the logit scale, same argmax away from ties) |
| int_vs_float | calibration, materialisation, the integer program (typed backend) against the float reference: top-1 and KL per class (dense ≥ 0.9 / 0.01, MoE ≥ 0.85 / 0.02, hybrid ≥ 0.8 / 0.03) |
| three_way | reference evaluator ↔ `misaka-palw-tir-ref2` ↔ `misaka-palw-tir-exec`: logits and every commit point equal at every position (2 sequences × 12 positions) |
| court | the **court property**: every commit point of every position is reproduced by the cone evaluator from the other commit points of its occurrence, the carries, the params and the state the position started from — on the reference evaluator and on the typed backend |

**Court coverage** is defined structurally. The court "knows the primitives and nothing about models"
(spec 04b §10.4): a terminal refutation recomputes one tile by evaluating the commit point's cone over the 25
primitives from opened leaves. So coverage is (a) every commit point of every lowered model replays equal through
that one generic path, (b) the commit-point roles of §10.1 and the primitives reached are all within it, and (c) no
source file of the IR, the evaluators or the court names a model (`no_court_or_ir_source_names_a_model`).

**The census.** A curated corpus is a choice. To measure the tail without choosing, `tools/corpus/census_def.py` takes every
`model_type` of `MODEL_FOR_CAUSAL_LM_MAPPING_NAMES` (168 in transformers 5.17) that is not already a corpus entry (111),
shrinks the family's default config to a tiny model (`gen_fixtures.auto_tiny`) and sends it through the same eight stages.
**Safety (incident 2026-10-01).** A model is never instantiated to find out how big it is: the config is first built on the
`meta` device and its parameters counted (over 20 M parameters, or any single tensor over 64 MB, and it is *not tiny*: the
keys are shrunk explicitly, or the family is recorded as having no automatic tiny config); only then is it really built, one
family per subprocess, with a 120 s timeout, an RSS ceiling of 4 GB polled by the parent, two threads. The excluded families
(composites, drafters, encoder-decoder halves) and the ones with no automatic tiny config are listed in §3.2, never dropped silently.

**What the weights and shares are, and are not.** The hub cannot be queried offline. `share` (text generation
only) is `hf-coverage.md` §2's estimate of an architecture's share of decoder-only text-generation repositories
by count (±30 % relative). `usage` is a coarse ordinal tier (`vh` 8, `h` 4, `m` 2, `l` 1) from this lane's
recollection of hub download rankings through 2025/26, ±1 tier. Neither is measured. The usage-weighted column
is therefore an *indication*; the counts are exact for the corpus as defined.

## 2. The corpus

100 architectures: 57 decoder-only text models (28 dense, 16 MoE, 13 hybrid/SSM/recurrent), 10 encoders and
embedding models, 7 encoder–decoders, 8 vision-language models (the text stage; the vision stage is RFC-0003),
4 vision towers, 6 image-generation models (diffusers), 5 audio models, 3 remote-code families. It contains every
family `hf-coverage.md` lists and the 2025–26 families transformers 5.17 ships that were not in the pack
(`arcee`, `apertus`, `helium`, `hunyuan_v1_*`, `seed_oss`, `ernie4_5*`, `bitnet`, `persimmon`, `diffllama`, `cwm`,
`gemma3n`, `dbrx`, `jetmoe`, `minimax_m2`, `longcat_flash`, `deepseek_v32`, `deepseek_v4`, `falcon_h1`,
`granitemoehybrid`, `zamba2`, `nemotron_h`, `lfm2`, `kimi_linear`, …). Storage formats (GPTQ, AWQ, GGUF, FP8,
MXFP4, …) are a separate axis, covered by the quant-format descriptors (lane F); they are not counted as
architectures here (§8).

<!-- BEGIN GENERATED: corpus -->
| # | id | category | `architectures[0]` | usage | share | why it is in the corpus |
| ---: | --- | --- | --- | :-: | ---: | --- |
| 1 | `llama` | text/dense | `LlamaForCausalLM` | vh | 30 % | the lineage a third of text-generation repositories derive from (Llama 1-3.3, TinyLlama, Vicuna, Yi, DeepSeek-LLM, SmolLM, CodeLlama) |
| 2 | `qwen2` | text/dense | `Qwen2ForCausalLM` | vh | 20 % | q/k/v bias, tied head: the second-largest family (Qwen1.5/2/2.5, QwQ, R1-Distill-Qwen) |
| 3 | `mistral` | text/dense | `MistralForCausalLM` | vh | 8 % | sliding-window GQA (Mistral, Zephyr, OpenHermes, Nemo) |
| 4 | `gpt2` | text/dense | `GPT2LMHeadModel` | vh | 7 % | learned positions, Conv1D weights, fused c_attn: the pre-Llama lineage's head |
| 5 | `qwen3` | text/dense | `Qwen3ForCausalLM` | vh | 6 % | per-head QK-norm (the dense Qwen of 2025) |
| 6 | `gemma2` | text/dense | `Gemma2ForCausalLM` | h | 2 % | sandwich norms, score and logit soft-caps, alternating windows |
| 7 | `gemma3_text` | text/dense | `Gemma3ForCausalLM` | vh | 3 % | 5:1 sliding/global, two rope tables, QK-norm (text of Gemma-3 1B and the VLM) |
| 8 | `gemma4_text` | text/dense | `Gemma4ForCausalLM` | h | unknown | 2026 flagship dense+MoE: per-layer inputs, K=V globals, KV sharing in the E models |
| 9 | `phi3` | text/dense | `Phi3ForCausalLM` | h | 2 % | fused qkv and gate_up, LongRoPE (the 128k variants): per-position rope semantics |
| 10 | `gpt_neox` | text/dense | `GPTNeoXForCausalLM` | h | 2 % | parallel residual with two LayerNorms, partial rotary, per-head fused qkv (Pythia, Dolly, RedPajama) |
| 11 | `opt` | text/dense | `OPTForCausalLM` | m | 1.5 % | learned positions at pos+2, ReLU MLP, (OPT-350m) post-LN and a projected embedding |
| 12 | `falcon` | text/dense | `FalconForCausalLM` | m | 1 % | kv-group fused qkv, parallel attention, ALiBi in the RW variants |
| 13 | `starcoder2` | text/dense | `Starcoder2ForCausalLM` | m | 0.5 % | LayerNorm + biases, plain GELU MLP, window (code models) |
| 14 | `cohere2` | text/dense | `Cohere2ForCausalLM` | m | 0.3 % | parallel residual from ONE bias-free LayerNorm, interleaved rope on sliding layers only |
| 15 | `granite` | text/dense | `GraniteForCausalLM` | m | 0.5 % | muP-style multipliers on embedding, branches, attention and logits |
| 16 | `glm4` | text/dense | `Glm4ForCausalLM` | m | 0.3 % | fused gate_up, partial interleaved rope, post-attention/post-MLP norms (GLM-4-0414, Z1) |
| 17 | `smollm3` | text/dense | `SmolLM3ForCausalLM` | m | 0.3 % | NoPE every n-th layer |
| 18 | `arcee` | text/dense | `ArceeForCausalLM` | l | unknown | Llama with a non-gated ReLU² MLP (AFM-4.5B) |
| 19 | `apertus` | text/dense | `ApertusForCausalLM` | l | unknown | xIELU activation with learned per-layer parameters, QK-norm, llama3 rope (Swiss AI) |
| 20 | `helium` | text/dense | `HeliumForCausalLM` | l | unknown | Llama-shaped (Kyutai) with INTERLEAVED rotary pairs: reads cleanly as Level A and is wrong, because the pairing is class code |
| 21 | `hunyuan_v1_dense` | text/dense | `HunYuanDenseV1ForCausalLM` | m | unknown | Llama + QK-norm placed after the rotation (Hunyuan-7B/4B/1.8B) |
| 22 | `seed_oss` | text/dense | `SeedOssForCausalLM` | l | unknown | Llama with q/k/v bias and a bias-free output projection (ByteDance Seed-OSS) |
| 23 | `ernie4_5` | text/dense | `Ernie4_5ForCausalLM` | m | unknown | Llama with GLM-style interleaved rotary pairs (ERNIE 4.5 dense) |
| 24 | `bitnet` | text/dense | `BitNetForCausalLM` | l | unknown | ternary weights with per-token int8 activation quantisation and sub-layer norms (BitNet b1.58) |
| 25 | `persimmon` | text/dense | `PersimmonForCausalLM` | l | unknown | LayerNorm, QK layer-norm, ReLU², partial rotary, per-head fused qkv (Adept) |
| 26 | `diffllama` | text/dense | `DiffLlamaForCausalLM` | l | unknown | differential attention: the difference of two softmax maps |
| 27 | `cwm` | text/dense | `CwmForCausalLM` | l | unknown | Llama with per-layer sliding windows and llama3 rope (Code World Model) |
| 28 | `gemma3n_text` | text/dense | `Gemma3nForCausalLM` | h | unknown | AltUp streams, LAuReL, per-layer embeddings, activation sparsity, KV sharing (Gemma-3n E2B/E4B) |
| 29 | `mixtral` | text/moe | `MixtralForCausalLM` | h | 1 % | softmax top-2 of 8 experts, renormalised |
| 30 | `qwen2_moe` | text/moe | `Qwen2MoeForCausalLM` | m | 0.5 % | sigmoid-gated shared expert, dense first layer |
| 31 | `qwen3_moe` | text/moe | `Qwen3MoeForCausalLM` | h | 0.5 % | QK-norm + renormalised top-k, no shared expert (Qwen3-30B-A3B, 235B) |
| 32 | `granitemoe` | text/moe | `GraniteMoeForCausalLM` | l | unknown | top-k of logits then softmax; fused expert tensors |
| 33 | `deepseek_v3` | text/moe | `DeepseekV3ForCausalLM` | h | 0.5 % | MLA + sigmoid routing with selection bias and group-limited top-k (DeepSeek-V3/R1, Kimi-K2) |
| 34 | `gpt_oss` | text/moe | `GptOssForCausalLM` | vh | unknown | attention sinks, clamped SwiGLU, interleaved fused experts, YaRN without truncation |
| 35 | `llama4_text` | text/moe | `Llama4ForCausalLM` | h | unknown | chunked attention, NoPE layers with query temperature, top-1 sigmoid MoE scaling the expert input |
| 36 | `glm4_moe` | text/moe | `Glm4MoeForCausalLM` | h | unknown | GLM-4.5/4.6: DeepSeek-V3 routing over GQA with partial rotary |
| 37 | `dbrx` | text/moe | `DbrxForCausalLM` | l | unknown | fused expert tensors, QKV clip, bias-free LayerNorm (Databricks) |
| 38 | `jetmoe` | text/moe | `JetMoeForCausalLM` | l | unknown | MoE for BOTH the MLP and the attention (mixture-of-attention heads) |
| 39 | `ernie4_5_moe` | text/moe | `Ernie4_5_MoeForCausalLM` | m | unknown | softmax router with a correction bias, shared experts, interleaved rotary (ERNIE-4.5-21B/300B-A47B) |
| 40 | `hunyuan_v1_moe` | text/moe | `HunYuanMoEV1ForCausalLM` | m | unknown | Hunyuan-A13B: QK-norm, shared expert, top-k routing |
| 41 | `minimax_m2` | text/moe | `MiniMaxM2ForCausalLM` | m | unknown | sigmoid router + bias, QK-norm over the whole projection, partial rotary (MiniMax-M2) |
| 42 | `longcat_flash` | text/moe | `LongcatFlashForCausalLM` | l | unknown | MLA, zero-computation experts and a shortcut-connected dense branch (Meituan LongCat-Flash) |
| 43 | `deepseek_v32` | text/moe | `DeepseekV32ForCausalLM` | h | unknown | DeepSeek sparse attention: a learned lightning indexer picks the top-k keys (V3.2) |
| 44 | `deepseek_v4` | text/moe | `DeepseekV4ForCausalLM` | h | unknown | 2026 DeepSeek: hyper-connections, compressed sparse attention, hash-routed experts |
| 45 | `qwen3_next` | text/hybrid | `Qwen3NextForCausalLM` | h | 0.5 % | gated delta rule (3:1 with attention), sigmoid-gated attention, shared-expert MoE |
| 46 | `qwen3_5_moe` | text/hybrid | `Qwen3_5MoeForCausalLM` | h | unknown | Qwen3.5 (split GDN projections) with MoE |
| 47 | `jamba` | text/hybrid | `JambaForCausalLM` | m | unknown | Mamba-1 + attention + MoE interleaving |
| 48 | `mamba` | text/hybrid | `MambaForCausalLM` | m | unknown | selective scan (state-spaces/mamba-*-hf) |
| 49 | `mamba2` | text/hybrid | `Mamba2ForCausalLM` | m | unknown | state-space duality, grouped B/C, gated RMSNorm (Codestral-Mamba) |
| 50 | `falcon_mamba` | text/hybrid | `FalconMambaForCausalLM` | l | unknown | Mamba with weightless B/C/dt norms |
| 51 | `rwkv` | text/hybrid | `RwkvForCausalLM` | l | unknown | RWKV-4: a recurrent model with no attention at all |
| 52 | `falcon_h1` | text/hybrid | `FalconH1ForCausalLM` | m | unknown | attention and Mamba-2 in PARALLEL inside every layer, muP multipliers (TII) |
| 53 | `granitemoehybrid` | text/hybrid | `GraniteMoeHybridForCausalLM` | m | unknown | Granite 4: Mamba-2 and attention layers with a shared-expert MoE per layer |
| 54 | `zamba2` | text/hybrid | `Zamba2ForCausalLM` | l | unknown | Mamba-2 backbone with ONE shared attention block reused at several depths |
| 55 | `nemotron_h` | text/hybrid | `NemotronHForCausalLM` | m | unknown | single-block layers (Mamba-2, attention, MLP or MoE) in a pattern string |
| 56 | `lfm2` | text/hybrid | `Lfm2ForCausalLM` | m | unknown | gated short convolutions interleaved with attention (Liquid LFM2) |
| 57 | `kimi_linear` | text/hybrid | `KimiLinearForCausalLM` | m | unknown | Kimi Delta Attention (channel-wise gated delta rule) with MLA layers (Kimi-Linear-48B-A3B) |
| 58 | `bert` | encoder | `BertModel` | vh | unknown | the most downloaded encoder family: bert-base-uncased and every all-MiniLM / bge / e5 / gte sentence embedder built on it |
| 59 | `roberta` | encoder | `RobertaModel` | vh | unknown | positions from padding_idx + 1; the base of XLM-R, CamemBERT, BGE-M3 |
| 60 | `xlm_roberta` | encoder | `XLMRobertaModel` | vh | unknown | multilingual encoder and embedder backbone (BGE-M3, multilingual-e5) |
| 61 | `distilbert` | encoder | `DistilBertModel` | h | unknown | BERT without token types under its own names |
| 62 | `mpnet` | encoder | `MPNetModel` | vh | unknown | all-mpnet-base-v2: bidirectional + a T5-bucket relative-position bias |
| 63 | `deberta_v2` | encoder | `DebertaV2Model` | h | unknown | disentangled attention (content-to-position and position-to-content terms): DeBERTa-v3 is the usual classification / NLI / reranker backbone |
| 64 | `albert` | encoder | `AlbertModel` | m | unknown | one layer's weights shared across the depth, factorised embedding |
| 65 | `modernbert` | encoder | `ModernBertModel` | h | unknown | 2024-25 encoder: RoPE, GeGLU, alternating local/global attention, bias-free LayerNorm |
| 66 | `nomic_bert` | encoder | `NomicBertModel` | h | unknown | a RoPE BERT with a fused gated MLP: nomic-embed-text, a leading open embedder |
| 67 | `clip_text` | encoder | `CLIPTextModelWithProjection` | vh | unknown | the text tower of CLIP and every Stable Diffusion text encoder; causal with learned positions |
| 68 | `t5` | encdec | `T5ForConditionalGeneration` | vh | unknown | relative-position buckets, RMSNorm without a mean, unscaled scores: the encoder-decoder head (t5-small/base) |
| 69 | `t5_gated` | encdec | `T5ForConditionalGeneration` | vh | unknown | T5 v1.1 / Flan-T5 / UL2: gated-GELU FFN, an untied head (also the text encoder of Flux, SD3 and PixArt) |
| 70 | `bart` | encdec | `BartForConditionalGeneration` | h | unknown | post-norm, learned positions at pos+2: summarisation and the BART/mBART family |
| 71 | `mbart` | encdec | `MBartForConditionalGeneration` | m | unknown | pre-norm BART with final norms: multilingual translation |
| 72 | `marian` | encdec | `MarianMTModel` | h | unknown | sinusoidal positions: the Helsinki-NLP/opus-mt translation models (thousands of repositories) |
| 73 | `longt5` | encdec | `LongT5ForConditionalGeneration` | l | unknown | T5 with local / transient-global encoder attention: long-input summarisation |
| 74 | `t5_encoder` | encdec | `T5EncoderModel` | vh | unknown | T5EncoderModel alone: the text encoder of Flux, SD3, PixArt, Wan, Sana (encoder-only reuse of a seq2seq checkpoint) |
| 75 | `llava` | vlm | `LlavaForConditionalGeneration` | vh | unknown | LLaVA 1.5/1.6, the CLIP-tower + MLP projector + Llama pattern behind most open VLMs |
| 76 | `qwen2_vl` | vlm | `Qwen2VLForConditionalGeneration` | vh | unknown | M-RoPE (3-D positions), a native-resolution vision tower, patch merger |
| 77 | `qwen2_5_vl` | vlm | `Qwen2_5_VLForConditionalGeneration` | vh | unknown | window-attention RMSNorm/SwiGLU tower + M-RoPE: the default open VLM of 2025 |
| 78 | `qwen3_vl` | vlm | `Qwen3VLForConditionalGeneration` | h | unknown | DeepStack: intermediate vision features are ADDED into the early text layers' hidden states; interleaved M-RoPE |
| 79 | `gemma3_vlm` | vlm | `Gemma3ForConditionalGeneration` | vh | unknown | SigLIP tower + Gemma-3 text (the 4B/12B/27B are VLMs) |
| 80 | `paligemma` | vlm | `PaliGemmaForConditionalGeneration` | h | unknown | PREFIX-LM attention: the image and prompt tokens attend bidirectionally, the answer causally |
| 81 | `idefics3` | vlm | `Idefics3ForConditionalGeneration` | h | unknown | SmolVLM / Idefics3: pixel-shuffle connector over a SigLIP-style tower, Llama text |
| 82 | `mllama` | vlm | `MllamaForConditionalGeneration` | h | unknown | text layers CROSS-ATTEND to vision states (Llama-3.2-Vision) |
| 83 | `clip_vision` | vision | `CLIPVisionModelWithProjection` | vh | unknown | the image tower of CLIP, LLaVA, Stable Diffusion image-conditioning and zero-shot classification |
| 84 | `siglip_vision` | vision | `SiglipVisionModel` | h | unknown | the image tower of Gemma-3, PaliGemma, Idefics3; attention-pooling head |
| 85 | `vit` | vision | `ViTForImageClassification` | vh | unknown | image classification at hub scale: ViT, DeiT, BEiT, DINOv2 share this encoder |
| 86 | `resnet` | vision | `ResNetForImageClassification` | vh | unknown | CONVOLUTIONAL classification (ResNet, ConvNeXt, EfficientNet, timm): the largest family by downloads with no attention at all |
| 87 | `unet2d_condition` | image-gen | `UNet2DConditionModel` | vh | unknown | Stable Diffusion 1.x/2.x: the denoiser of the largest image-generation family (convolutions, GroupNorm, cross-attention, timestep embedding) |
| 88 | `unet_sdxl` | image-gen | `UNet2DConditionModel` | vh | unknown | SDXL: added text/time conditioning and a transformer stack per resolution (the most used open image model) |
| 89 | `dit` | image-gen | `DiTTransformer2DModel` | m | unknown | the DiT: patchified latents, adaLN-Zero, class conditioning (ancestor of PixArt, SD3, Flux) |
| 90 | `flux` | image-gen | `FluxTransformer2DModel` | vh | unknown | MMDiT with 3-axis RoPE, double- and single-stream blocks (FLUX.1 dev/schnell) |
| 91 | `sd3` | image-gen | `SD3Transformer2DModel` | h | unknown | MMDiT with joint text-image attention and adaLN (Stable Diffusion 3/3.5) |
| 92 | `vae_kl` | image-gen | `AutoencoderKL` | vh | unknown | every diffusion pipeline decodes through a convolutional VAE with a mid-block self-attention |
| 93 | `whisper` | audio | `WhisperForConditionalGeneration` | vh | unknown | speech recognition: log-mel front end, two strided Conv1d, sinusoidal-position encoder, cross-attending decoder |
| 94 | `wav2vec2` | audio | `Wav2Vec2ForCTC` | vh | unknown | raw-waveform CTC: a conv feature extractor, a grouped-conv positional embedding, a transformer |
| 95 | `speecht5` | audio | `SpeechT5ForTextToSpeech` | m | unknown | text-to-speech: a mel decoder with a pre-net/post-net (a vocoder follows) |
| 96 | `musicgen` | audio | `MusicgenForConditionalGeneration` | m | unknown | music generation: an autoregressive decoder over several EnCodec codebooks (delay pattern) and a T5 text encoder |
| 97 | `encodec` | audio | `EncodecModel` | m | unknown | the neural audio codec under MusicGen, Bark, Moshi-class models: causal conv encoder/decoder + residual vector quantisation |
| 98 | `chatglm3` | remote-code | `ChatGLMModel` | h | unknown | trust_remote_code: THUDM/chatglm3-6b is among the most downloaded Chinese chat models; its forward lives in the repository, not in transformers |
| 99 | `internlm2` | remote-code | `InternLM2ForCausalLM` | m | 0.5 % | trust_remote_code Llama-lineage with fused wqkv (InternLM2/2.5) |
| 100 | `minicpm` | remote-code | `MiniCPMForCausalLM` | m | 0.3 % | trust_remote_code with muP-style scale_emb / scale_depth / dim_model_base |
<!-- END GENERATED: corpus -->

## 3. Results

<!-- BEGIN GENERATED: summary -->
| | entries | share of corpus | usage-weighted |
| --- | ---: | ---: | ---: |
| Level A — the config alone | 6 | 6 % | 9 % |
| Level B — a thin data adapter | 84 | 84 % | 80 % |
| Level C — a capability is missing | 10 | 10 % | 11 % |
| **A + B (expressible with existing features)** | **90** | **90 %** | **89 %** |
| *of the A + B, encoder-decoders read by a data adapter and proven to lower and be admitted only (no weights stage in this harness): `t5`, `t5_gated`, `bart`, `mbart`, `marian`* | 5 | 5 % | |
| *of the A + B, read-level only (the reference is remote code, no weights to run): `longt5`, `t5_encoder`, `clip_vision`, `siglip_vision`, `vit`, `resnet`, `sd3`, `vae_kl`, `whisper`, `internlm2`, `minicpm`* | 11 | 11 % | |
| *A + B with every stage proven on weights* | 74 | 74 % | |

Level C by kind of gap (this lane's classification, `tools/corpus/blockers.json`):

| kind | entries | meaning |
| --- | ---: | --- |
| protocol | 6 | a protocol capability beyond the IR is needed (an input binding, a second history, a stage kind) |
| feature | 2 | a generic FEATURE is missing; lowerable in principle with the existing primitives |
| route | 2 | no data route exists for this kind of model yet (the lowering is Rust per family, or absent); existing primitives suffice |

Court coverage: 81547 commit points over 73 lowered models reproduced by the cone evaluator from their opened leaves (reference evaluator and typed backend; 0 failures); 25 of the 25 primitives and 6 commit-point roles reached.

| category | A | B | C | total |
| --- | ---: | ---: | ---: | ---: |
| audio | 0 | 1 | 4 | 5 |
| encdec | 0 | 7 | 0 | 7 |
| encoder | 0 | 10 | 0 | 10 |
| image-gen | 0 | 2 | 4 | 6 |
| remote-code | 0 | 3 | 0 | 3 |
| text/dense | 6 | 22 | 0 | 28 |
| text/hybrid | 0 | 12 | 1 | 13 |
| text/moe | 0 | 15 | 1 | 16 |
| vision | 0 | 4 | 0 | 4 |
| vlm | 0 | 8 | 0 | 8 |
<!-- END GENERATED: summary -->

### 3.1 Per entry

<!-- BEGIN GENERATED: results -->
| id | category | level | route / adapter | features | missing / why not | failed stage |
| --- | --- | :-: | --- | ---: | --- | --- |
| `llama` | text/dense | **A** | standard template (no adapter) | 8 |  |  |
| `qwen2` | text/dense | **A** | standard template (no adapter) | 9 |  |  |
| `mistral` | text/dense | **A** | standard template (no adapter) | 8 |  |  |
| `gpt2` | text/dense | **B** | built-in adapter `gpt2` | 11 |  |  |
| `qwen3` | text/dense | **A** | standard template (no adapter) | 10 |  |  |
| `gemma2` | text/dense | **B** | built-in adapter `gemma2` | 14 |  |  |
| `gemma3_text` | text/dense | **B** | built-in adapter `gemma3-text` | 15 |  |  |
| `gemma4_text` | text/dense | **B** | built-in adapter `gemma4-text` | 24 |  |  |
| `phi3` | text/dense | **B** | built-in adapter `phi3` | 8 |  |  |
| `gpt_neox` | text/dense | **B** | built-in adapter `gpt-neox` | 10 |  |  |
| `opt` | text/dense | **B** | built-in adapter `opt` | 11 |  |  |
| `falcon` | text/dense | **B** | built-in adapter `falcon` | 8 |  |  |
| `starcoder2` | text/dense | **B** | built-in adapter `starcoder2` | 11 |  |  |
| `cohere2` | text/dense | **B** | built-in adapter `cohere2` | 12 |  |  |
| `granite` | text/dense | **A** | standard template (no adapter) | 10 |  |  |
| `glm4` | text/dense | **B** | built-in adapter `glm4` | 11 |  |  |
| `smollm3` | text/dense | **B** | built-in adapter `smollm3` | 9 |  |  |
| `arcee` | text/dense | **B** | third-party adapter `arcee` (tools/corpus/adapters) | 7 | the checkpoint does not fit the reading |  |
| `apertus` | text/dense | **B** | built-in adapter `apertus` | 11 |  |  |
| `helium` | text/dense | **B** | third-party adapter `helium` (tools/corpus/adapters) | 7 |  |  |
| `hunyuan_v1_dense` | text/dense | **B** | third-party adapter `hunyuan-v1-dense` (tools/corpus/adapters) | 10 |  |  |
| `seed_oss` | text/dense | **B** | third-party adapter `seed-oss` (tools/corpus/adapters) | 8 |  |  |
| `ernie4_5` | text/dense | **B** | third-party adapter `ernie4-5` (tools/corpus/adapters) | 9 |  |  |
| `bitnet` | text/dense | **B** | built-in adapter `bitnet` | 8 |  |  |
| `persimmon` | text/dense | **B** | third-party adapter `persimmon` (tools/corpus/adapters) | 12 |  |  |
| `diffllama` | text/dense | **B** | built-in adapter `diffllama` | 10 |  |  |
| `cwm` | text/dense | **A** | standard template (no adapter) | 9 |  |  |
| `gemma3n_text` | text/dense | **B** | built-in adapter `gemma3n-text` | 20 |  |  |
| `mixtral` | text/moe | **B** | built-in adapter `mixtral` | 9 |  |  |
| `qwen2_moe` | text/moe | **B** | built-in adapter `qwen2-moe` | 12 |  |  |
| `qwen3_moe` | text/moe | **B** | built-in adapter `qwen3-moe` | 11 |  |  |
| `granitemoe` | text/moe | **B** | built-in adapter `granitemoe` | 11 |  |  |
| `deepseek_v3` | text/moe | **B** | built-in adapter `deepseek-v3` | 15 |  |  |
| `gpt_oss` | text/moe | **B** | built-in adapter `gpt-oss` | 14 |  |  |
| `llama4_text` | text/moe | **B** | built-in adapter `llama4-text` | 20 |  |  |
| `glm4_moe` | text/moe | **B** | built-in adapter `glm4-moe` | 17 |  |  |
| `dbrx` | text/moe | **B** | built-in adapter `dbrx` | 11 |  |  |
| `jetmoe` | text/moe | **B** | built-in adapter `jetmoe` | 11 |  |  |
| `ernie4_5_moe` | text/moe | **B** | built-in adapter `ernie4-5-moe` | 15 |  |  |
| `hunyuan_v1_moe` | text/moe | **B** | third-party adapter `hunyuan-v1-moe` (tools/corpus/adapters) | 13 |  |  |
| `minimax_m2` | text/moe | **B** | third-party adapter `minimax-m2` (tools/corpus/adapters) | 12 |  |  |
| `longcat_flash` | text/moe | **B** | built-in adapter `longcat-flash` | 14 |  |  |
| `deepseek_v32` | text/moe | **C** | built-in adapter `deepseek-v32` (refuted: float_vs_hf) | 15 | DeepSeek sparse attention: a token-level top-k indexer → FR-09 | float_vs_hf |
| `deepseek_v4` | text/moe | **B** | built-in adapter `deepseek-v4` | 20 |  |  |
| `qwen3_next` | text/hybrid | **B** | built-in adapter `qwen3-next` | 19 |  |  |
| `qwen3_5_moe` | text/hybrid | **B** | built-in adapter `qwen3-5-moe` | 18 |  |  |
| `jamba` | text/hybrid | **B** | built-in adapter `jamba` | 11 |  |  |
| `mamba` | text/hybrid | **B** | built-in adapter `mamba` | 7 |  |  |
| `mamba2` | text/hybrid | **B** | built-in adapter `mamba2` | 6 |  |  |
| `falcon_mamba` | text/hybrid | **B** | built-in adapter `falcon-mamba` | 7 |  |  |
| `rwkv` | text/hybrid | **B** | built-in adapter `rwkv4` | 8 |  |  |
| `falcon_h1` | text/hybrid | **B** | built-in adapter `falcon-h1` | 16 |  |  |
| `granitemoehybrid` | text/hybrid | **B** | built-in adapter `granitemoehybrid` | 12 |  |  |
| `zamba2` | text/hybrid | **B** | built-in adapter `zamba2` | 15 |  |  |
| `nemotron_h` | text/hybrid | **C** | built-in adapter `nemotron-h` (refuted: float_vs_hf) | 16 | layers that are a single block (no mixer) → FR-14 | float_vs_hf |
| `lfm2` | text/hybrid | **B** | built-in adapter `lfm2` | 12 |  |  |
| `kimi_linear` | text/hybrid | **B** | built-in adapter `kimi-linear` | 15 |  |  |
| `bert` | encoder | **B** | built-in adapter `bert` | 13 |  |  |
| `roberta` | encoder | **B** | built-in adapter `roberta` | 13 |  |  |
| `xlm_roberta` | encoder | **B** | built-in adapter `roberta` | 13 |  |  |
| `distilbert` | encoder | **B** | built-in adapter `distilbert` | 12 |  |  |
| `mpnet` | encoder | **B** | built-in adapter `mpnet` | 13 |  |  |
| `deberta_v2` | encoder | **B** | built-in adapter `deberta-v2` | 12 |  |  |
| `albert` | encoder | **B** | built-in adapter `albert` | 14 |  |  |
| `modernbert` | encoder | **B** | built-in adapter `modernbert` | 11 |  |  |
| `nomic_bert` | encoder | **B** | built-in adapter `nomic-bert` | 10 |  |  |
| `clip_text` | encoder | **B** | built-in adapter `clip-text` | 10 |  |  |
| `t5` | encdec | **B** | built-in encdec adapter `t5` (lower + admit only) ; core Rust route | 9 |  |  |
| `t5_gated` | encdec | **B** | built-in encdec adapter `t5` (lower + admit only) ; core Rust route | 8 |  |  |
| `bart` | encdec | **B** | built-in encdec adapter `bart` (lower + admit only) ; core Rust route | 12 |  |  |
| `mbart` | encdec | **B** | built-in encdec adapter `mbart` (lower + admit only) ; core Rust route | 13 |  |  |
| `marian` | encdec | **B** | built-in encdec adapter `marian` (lower + admit only) ; core Rust route | 12 |  |  |
| `longt5` | encdec | **B** | built-in encdec adapter `longt5` (read + lower + admit) | 10 |  |  |
| `t5_encoder` | encdec | **B** | built-in encdec adapter `t5-encoder` (read + lower + admit) | 7 | the checkpoint does not fit the reading |  |
| `llava` | vlm | **B** | built-in adapter `vlm-llama` | 7 |  |  |
| `qwen2_vl` | vlm | **B** | built-in adapter `vlm-qwen2-vl` | 9 |  |  |
| `qwen2_5_vl` | vlm | **B** | built-in adapter `vlm-qwen2-vl` | 9 |  |  |
| `qwen3_vl` | vlm | **B** | third-party adapter `qwen3-vl` (tools/corpus/adapters) | 10 |  |  |
| `gemma3_vlm` | vlm | **B** | built-in adapter `vlm-gemma3` | 14 |  |  |
| `paligemma` | vlm | **B** | built-in adapter `vlm-gemma` | 10 |  |  |
| `idefics3` | vlm | **B** | third-party adapter `idefics3` (tools/corpus/adapters) | 7 |  |  |
| `mllama` | vlm | **B** | built-in adapter `mllama` | 9 |  |  |
| `clip_vision` | vision | **B** | built-in vision adapter `clip-vision` (read + lower + admit) ; core Rust route | 8 |  |  |
| `siglip_vision` | vision | **B** | built-in vision adapter `siglip-vision` (read + lower + admit) ; core Rust route | 7 |  |  |
| `vit` | vision | **B** | built-in vision adapter `vit` (read + lower + admit) ; core Rust route | 8 |  |  |
| `resnet` | vision | **B** | built-in cnn adapter `resnet` (read + lower + admit) | 6 |  |  |
| `unet2d_condition` | image-gen | **C** | - |  | GEN_UNET_SKIP, GEN_RESNET_TIME_COND, GEN_SPATIAL_TRANSFORMER, ATTN_CROSS, GEN_SAMPLER_EPS → FR-22 | read |
| `unet_sdxl` | image-gen | **C** | - |  | GEN_UNET_SKIP, GEN_RESNET_TIME_COND, GEN_SPATIAL_TRANSFORMER, ATTN_CROSS, GEN_SAMPLER_EPS, EMBED_ADDITION_TEXT_TIME → FR-22 | read |
| `dit` | image-gen | **C** | - |  | EMBED_CLASS_LABEL, GEN_BLOCK_ADALN_ZERO, EMBED_POSITION_SINCOS, GEN_OUTPUT_LEARNED_SIGMA, GEN_SAMPLER_EPS → FR-22 | read |
| `flux` | image-gen | **C** | - |  | POS_ROPE_AXES, ATTN_QK_NORM_JOINT, GEN_BLOCK_SINGLE_STREAM, EMBED_GUIDANCE → FR-22 | read |
| `sd3` | image-gen | **B** | built-in diffusers adapter `diffusers:sd3-transformer` (read + lower + admit) | 6 |  |  |
| `vae_kl` | image-gen | **B** | built-in diffusers adapter `diffusers:autoencoder-kl-decoder` (read + lower + admit) | 4 |  |  |
| `whisper` | audio | **B** | built-in encdec adapter `whisper` (read + lower + admit) | 11 |  |  |
| `wav2vec2` | audio | **C** | - |  | an adapter for `Wav2Vec2ForCTC` → FR-23 | read |
| `speecht5` | audio | **C** | - |  | an encoder–decoder adapter for `SpeechT5ForTextToSpeech` → FR-23 | read |
| `musicgen` | audio | **C** | - |  | an encoder–decoder adapter for `MusicgenForConditionalGeneration` → FR-23 | read |
| `encodec` | audio | **C** | - |  | an adapter for `EncodecModel` → FR-23 | read |
| `chatglm3` | remote-code | **B** | built-in adapter `chatglm3` | 11 | the checkpoint does not fit the reading |  |
| `internlm2` | remote-code | **B** | built-in adapter `internlm2` | 9 |  |  |
| `minicpm` | remote-code | **B** | built-in adapter `minicpm` | 12 |  |  |
<!-- END GENERATED: results -->

### 3.2 The census: the same question on the tail

<!-- BEGIN GENERATED: census -->
| outcome | families | share of the 111 |
| --- | ---: | ---: |
| Level A (the standard template alone) | 1 | 1 % |
| Level B through the built-in adapter pack (nobody wrote anything for this run) | 24 | 22 % |
| Level B through a third-party adapter (written by this lane for this table, data only: `tools/corpus/census-adapters/`) | 24 | 22 % |
| Level B through a synthesised adapter (convention search) | 0 | 0 % |
| **A + B** | **49** | **44 %** (of the 85 buildable: 58 %) |
| Level C (read refused or a later stage failed) | 36 | 32 % |
| no automatic tiny config (listed below) | 8 | 7 % |
| excluded: not a standalone text decoder (listed below, with the reason) | 18 | 16 % |

Why the Level C families are Level C (read-stage evidence, `census_report.json`):

| reason | families | which |
| --- | ---: | --- |
| config keys the template does not model | 25 | `big_bird`, `bigbird_pegasus`, `blenderbot_small`, `camembert`, `cpmant`, `data2vec_text`, `doge`, `electra`, `ernie`, `fuyu`, `gpt_neox_japanese`, `hrm_text`, `megatron_bert`, `minimax_m3_vl_text`, `moshi`, `mvp`, `pegasus`, `recurrent_gemma`, `rembert`, `roberta_prelayernorm`, `roc_bert`, `roformer`, `trocr`, `xglm`, `xlm_roberta_xl` |
| layer type `deepseek_sparse_attention` (FR-09) | 3 | `axk2`, `glm_moe_dsa`, `hy_v4` |
| not a causal language model class (no adapter) | 3 | `ctrl`, `xlm`, `xlnet` |
| layer type `linear_attention` not modelled | 2 | `minimax`, `olmo_hybrid` |
| later stage: float_vs_hf | 1 | `biogpt` |
| cross-attention (decoder half of an encoder-decoder) | 1 | `prophetnet` |
| rope parameters keyed by layer type, layer has none | 1 | `zaya` |

| convention the template lacks | families it blocks | the keys |
| --- | ---: | --- |
| MoE hyper-parameters and layer pattern | 2 | `norm_topk_prob`, `num_experts`, `num_experts_per_tok`, `num_local_experts`, `routed_scaling_factor`, `router_jitter_noise` |
| nested VLM wrapper (text_config, vision_config, token ids) | 1 | `image_token_id`, `text_config` |
| bias switches | 2 | `use_bias` |
| norm epsilon / norm kind aliases | 13 | `layer_norm_eps` |
| activation aliases | 7 | `activation_function`, `hidden_activation` |
| dimension aliases (n_embd, n_head, n_layer, ...) | 7 | `d_model`, `ffn_dim` |
| everything else (family-specific keys) | 25 | 79 distinct keys |

Variants (a second tiny config where the first cannot exercise a feature real checkpoints use; not counted as families): `granitemoeshared_shared` is Level B; `mimo_v2_flash_unscaled` is Level B; `biogpt_unscaled` is Level B; `nanochat_std_rope` is Level B; `cohere2_moe_shared_sum` is Level B; `cohere2_moe_shared_avg` is Level C (NOT_LOWERABLE(Cohere2MoeForCausalLM: a shared expert combined by `average` needs a scale on the shared expert (not a spec field yet) [in variable `com).

What this lane has read of the Level C families (`tools/corpus/census_blockers.json`; the rest are listed by the reader's refusal above and are NOT classified):

| class | families | which |
| --- | ---: | --- |
| feature | 24 | `axk2`, `big_bird`, `biogpt`, `camembert`, `data2vec_text`, `doge`, `electra`, `ernie`, `glm_moe_dsa`, `gpt_neox_japanese`, `hrm_text`, `hy_v4`, `megatron_bert`, `minimax`, `minimax_m3_vl_text`, `olmo_hybrid`, `recurrent_gemma`, `rembert`, `roberta_prelayernorm`, `roc_bert`, `roformer`, `xglm`, `xlm_roberta_xl`, `zaya` |
| route | 8 | `bigbird_pegasus`, `blenderbot_small`, `fuyu`, `moshi`, `mvp`, `pegasus`, `prophetnet`, `trocr` |
| legacy | 4 | `cpmant`, `ctrl`, `xlm`, `xlnet` |

Feature requests the census names (a family may name two): FR-29: 11; FR-18: 7; FR-09: 4; FR-34: 2; FR-01: 1; FR-10: 1; FR-16: 1; FR-19: 1; FR-23: 1; FR-30: 1; FR-32: 1.

No automatic tiny config (the family's defaults are over the size guard or its tiny build fails; recorded, not dropped): `bamba`, `cohere_compass_text`, `dots1`, `inkling_text`, `lfm2_moe`, `qwen4_exp`, `xlstm`, `zamba`.

Excluded, with the reason: `blenderbot` (the decoder half of an encoder-decoder (its `ForCausalLM` is not a standalone model)); `blt` (a byte-latent transformer (n-gram hash tables of 12 GB at the default; its patcher is a second model)); `emu3` (an image-generating multimodal model with a nested VQ config); `gemma3n` (the multimodal wrapper (needs `timm`); its text tower is the corpus entry `gemma3n_text`); `gemma4` (the multimodal wrapper; its text tower is the corpus entry `gemma4_text`); `gemma4_assistant` (an assistant/drafter head with no vocabulary of its own); `gemma4_unified` (the multimodal wrapper; the text tower is `gemma4_text`); `gemma4_unified_assistant` (an assistant/drafter head with no vocabulary of its own); `git` (a captioning decoder over a CLIP tower (nested vision config)); `got_ocr2` (an OCR VLM with a nested SAM-style tower); `llama4` (the multimodal wrapper; its text tower is the corpus entry `llama4_text`); `musicgen_melody` (a composite audio model; the decoder alone is not a text family); `phi4_multimodal` (a text+vision+audio model with nested towers); `plbart` (the decoder half of an encoder-decoder); `qwen3_5` (the multimodal wrapper; the text tower is `qwen3_5_text`); `qwen3_5_moe` (the multimodal wrapper; the text tower is the corpus entry `qwen3_5_moe`); `reformer` (bidirectional by default; the causal mode needs `is_decoder` and LSH chunking); `xmod` (an encoder with per-language adapters; the causal head needs a default language).
<!-- END GENERATED: census -->

Reading the table. The census is held out from the *curated list*, not from lane G's built-in adapter pack: the pack is the data
that already exists, and 21 of the 83 buildable families are covered by it without anyone writing anything for this run (the
pack was written for the families people use; the census is what is left when the famous ones are in the corpus). The ten
third-party adapters of §4.6 were written by this lane for census families (`tools/corpus/census-adapters/`, 15–40 lines of JSON
each), and every one of them lowers, is admitted and passes every stage including the court.

## 4. What the measurement says

(See the sections below; each claim is backed by an entry in the report or a named test.)

### 4.1 The pipeline holds wherever the reader says yes

Of the 63 entries that read at Level A or B, 56 have weights to run and **every one** passes all later stages: the program is
admitted, the float reference equals `transformers` to ≤ 3e-7 of the logit scale (encoders: ≤ 1e-4 of the embedding), the
integer program agrees with it, the three implementations are bit-identical at every position and every commit point, and
every commit point replays through the generic court path (53,687 replays on 56 lowered models, 25 of 25 primitives reached,
0 failures). Two are remote-code families whose reference cannot be run offline or pinned (`internlm2`, `minicpm`: Level B at
the read stage only, FR-24); five are encoder–decoders read by lane G's built-in adapters of kind `encdec` and proven to lower
and be admitted only (`t5`, `t5_gated`, `bart`, `mbart`, `marian`: the harness has no weights stage for an encoder–decoder yet, and the
adapter's spec equals the Rust parser's up to the last bit of one float, `1/sqrt(d)`). The same holds for the census families rescued
by third-party adapters (§4.6). The IR, the evaluators and the court are model-free in exactly the sense the freeze claims: no new
primitive was needed for anything in the corpus or the census, and no Level C entry is blocked by one (the research passes behind
`feature-requests.md` — decoder mixers, attention variants, encoders, encoder–decoders and vision towers, diffusion, audio — each end in
the same sentence: it decomposes into the existing 25).

**Re-measured on the merge of `tir/generic`** (twice: `fbbef69c6`, then `239179132`). The first merge (generic feature lowerers, LOGITS_Q24_V1,
quant descriptors) moved nothing in the corpus: the 53 stayed 53, every stage, which is the regression gate. The second (WEIGHTS_EXPR_V1, encdec
adapters, FR-02/07/25/26) moved the corpus from A 6 / B 47 to **A 6 / B 57**: `dbrx`, `ernie4_5_moe`, `granitemoehybrid` (built-in adapters now),
`hunyuan_v1_dense` and `hunyuan_v1_moe` (this lane's adapters with `qk_norm_after_rope`), and the five encoder–decoders. It also moved the
census (`aria_text`, `codegen`, `granitemoeshared` with a real shared expert: weight expressions). One regression was a property of the fixture:
lane G's LOGITS_Q24_V1 refuses logits of more than 120 natural-log units, and the random tiny `minicpm3` reached 305 with `scale_emb` 12 and
`dim_model_base` 256; the census config now keeps the logits near 30.

### 4.2 Level A is real but unconfirmed by the config alone

Six decoders read with no adapter and hold (`llama`, `qwen2`, `mistral`, `qwen3`, `granite`, `cwm`). Three more were
read by the standard template **without any refusal** and are wrong:

| class | what the config cannot say | what happens |
| --- | --- | --- |
| `ArceeForCausalLM` | the MLP is ungated (`up_proj`, ReLU², `down_proj`) | reads as gated; only a bind with weights notices (`missing tensor mlp.gate_proj`) |
| `HeliumForCausalLM` | rotary pairs are interleaved (class code) | reads, binds, lowers, is admitted — and computes a different function (float vs HF: 28 % of the logit scale) |
| `BitNetForCausalLM` | attn/ffn sub-layer norms | reads as Llama; four tensors per layer pair are never read |

The rope pairing, the norm placement and the MLP gating are properties of the *class code*, not of `config.json`.
A config-only reader can not know them; a tensor-name reader can know some (gating, sub-norms) and not others
(pairing). **No chain rule is violated** — the chain verifies the execution of the registered integer program (bit
identity), not the mapping from the HF name — but a Level A model that was never compared with its HF original is a
useless model sold under a famous name. Level A must therefore print as *unconfirmed* unless a fixture check passed,
and the reader should refuse when a tensor index is given and any tensor is left unread (FR-26).

### 4.3 Data alone works, and it is cheap

This lane wrote **twenty-nine** adapters as third-party data files that lower, are admitted and pass every stage (or, for the six variants below, are Level B on a second tiny config). Eight for the
corpus: `seed-oss`, `ernie4-5` (a use_bias switch and interleaved pairs), `persimmon` (per-head fused qkv, LayerNorm with bias, a
per-head q/k LayerNorm, ReLU²), `arcee`, `helium`, `minimax-m2` (sigmoid router with correction bias, whole-projection q/k norm),
`qwen3-vl` and `idefics3` (the text stage of a VLM wrapper under its own tensor prefix). Nineteen for the census (§4.6):
`vaultgemma`, `hyperclovax`, `jais2`, `flex_olmo`, `solar_open`, `glm4_moe_lite`, `youtu`, `axk1`, `hy_v3`, `exaone_moe`,
`minicpm3`, `granite_swa`, `granitemoe_swa`, `granitemoeshared`, `mellum`, `cohere2_moe`, `gemma4_unified_text`, `openai_gpt`, `bert_generation`,
`aria_text`, `codegen`. Each is 15–50 lines of JSON. Of the eleven census adapters nine held on the first attempt and two on the second (a hub
tensor name that differs from the module path; a head scale only the float-vs-HF stage notices); the eight corpus adapters took
one or two attempts each. The census adapters are mostly *extends* of a built-in (`gemma2`, `glm4-moe`, `deepseek-v3`,
`olmoe` + `mixin-post-norm`): a derived family is a few lines.
Six more corpus adapters read, lower and are admitted and stop at **one** well-defined step (§4.4) — they are ready for the
feature that closes them.

### 4.4 Where data stopped, and what changed when lane G built the answers

The first measurement found three precise results; all three were requests (FR-01, FR-07, FR-25) and all three are **closed on `tir/generic`**:

1. **Weights were not data** (`dbrx`, `granitemoehybrid`, `ernie4_5_moe` stopped at *bind*). `spec.hf.weights` — a weight expression per HL
   parameter: slices, reshapes, transposes, strided takes — makes a layout a few lines of an adapter. The three are built-in adapters now and pass every
   stage; a registrant's own `aria_text` (stacked, transposed experts), `codegen` (a fused qkv in four partitions, `[q|v|k]`) and `granitemoeshared`
   (a fused shared expert) are Level B with no Rust.
2. **The lowering silently ignored a flag the spec accepts** (`selection_bias` under softmax scoring). Fixed, with a refusal for every
   flag the lowerer would not apply (FR-07/FR-26): `ernie4_5_moe` passes.
3. **A refusal could not be overridden** (`GraniteMoeHybridForCausalLM`; later `MiniCPM3ForCausalLM` and `CodeGenForCausalLM`). A user adapter may now
   override a built-in refusal and the report says so ("user adapter overrides built-in refusal: …", FR-25): `minicpm3` and `codegen` are Level B that way.

What data still cannot say is in the requests that remain open (§5).

### 4.5 The big gap is routes, not features

37 entries are Level C. Fifteen are decoder-side **features**, each small and unique to one or two families
(§6). Fourteen are **routes**: bidirectional encoders beyond BERT's layer structure, the two remaining encoder–decoders (LongT5, a bare T5
encoder), vision towers, convolutional nets, DiT-style image generation. Two of those kinds still have their lowering only as Rust per family
(`parse_vision`, `lower::bidir`), and image generation has none, so a new ViT-like, CLIP-like or ModernBERT-like model needs a core
developer (the encoder–decoders moved to data on `tir/generic`: five entries). That is the infrastructure gap that matters most: these routes are
where the hub's repository counts are (CLIP/SigLIP, ViT/ResNet classification, sentence embedders). Seven need a **protocol**
capability: five audio models (an audio input binding, a front end, a job body) and the two UNets (a per-occurrence carry
signature: skip connections cannot be carried today, FR-22 C1); one is remote code. **No Level C entry needs a new
primitive** — every decomposition in `feature-requests.md` lands on the 25. What decides *registrability* of the real-size
models is not the lowering but three limits of the program format and the admission ceilings: one carry signature per program,
tensor rank ≤ 4 and ≤ 16 blocks, and the per-position ceilings (`max_step_leaves` 2^22, `max_position_macs` 2^40): T5-XXL
(Flux's and SD3's text encoder), Whisper-large, Flux and the 1024² VAE decoder do not fit one position today, and each has a
named lever (per-commit tile lengths, a stat-only softmax commit, per-level stage splits, an image/audio ceiling set).

### 4.6 The long tail: the census

The census (§3.2) asks the question again on every causal-LM family of transformers 5.17 that is not one of the 100 curated
entries. Four results.

1. **58 % of the buildable families are expressible by data today** (49 of 85): one at Level A (`ministral`), 24 through the built-in pack and
   24 through this lane's adapters; **44 % of all 111** once the families with no automatic tiny config (8) and the excluded ones
   (18: composites, drafters, encoder-decoder halves) are counted (the standard-template families are in the
   curated corpus), and the convention search synthesised none: the refusals are on keys that *can* change the math (experts, MLA
   ranks, norm kinds, layer patterns) or on whole layer types, which the search by design does not guess.
2. **The 36 Level C families, by what this lane has read** (`tools/corpus/census_blockers.json`): 25 need a feature,
   8 are a route (decoder halves of encoder–decoders, a VLM, a speech model), 4 are historical LMs. The named features: **FR-29** the BERT lineage's
   prediction head (12 families, `ForCausalLM` modes hardly anyone serves, plus `modernbert_decoder`); **FR-09** DeepSeek sparse attention
   (`axk2`, `glm_moe_dsa`, `hy_v4`, plus the corpus's `deepseek_v32`); **FR-30** a separate attention output gate (`minimax`; `afmoe` and `laguna` became Level B when FR-30 landed);
   **FR-34** the embedding scale on tokens only (`biogpt`, `xglm`); (**FR-31** `mimo_v2_flash`, **FR-35** `nanochat` and the **FR-29** `modernbert_decoder` head also landed on tir/generic c3f1c27ca: all Level B, every stage);
   and one-offs read but not built (`minimax` lightning attention FR-32, `olmo_hybrid`, `recurrent_gemma`, `zaya`, `doge`, `hrm_text`).
   **The prediction test.** Before running, this lane expected an adapter to hold for ten decoder families. Six did (`cohere2_moe`, `granite_swa`,
   `granitemoe_swa`, `granitemoeshared`, `mellum`, and `aria_text` once FR-01 landed); `biogpt` and `mimo_v2_flash` did not and the cause was
   the one found by reading (FR-34; FR-31), proved by variants that remove only that gap (`biogpt_unscaled`, `mimo_v2_flash_unscaled`: Level B, every stage);
   `afmoe` and `laguna` were read to need FR-30 and not run. The same method rescued five more (`gemma4_unified_text`, `openai_gpt`,
   `bert_generation`, `minicpm3`, `codegen`). The 49 are therefore a **floor** for what data can do and the 36 an **upper bound** for what needs a feature.
3. **The per-layer-type rope convention is necessary but not sufficient.** Six census families are refused because
   `rope_parameters` is keyed by layer type; `a-candidates/standard-v3.json` (a 30-line data file: the rope of layer *i* is the one of
   its type, with that type's partial-rotary factor) removes the refusal for five and moves **none** to Level B — each then stops at
   its own next convention (MoE keys for `laguna`, `mellum`, `mimo_v2_flash`; Gemma-4's per-layer config; ModernBERT-decoder's
   prediction head and key names). `zaya` uses other layer-type names (`hybrid`, `hybrid_sliding`).
4. **A refusal list written for remote code outlives the code.** `MiniCPM3ForCausalLM` is refused by name as "remote code, not modelled
   yet", but the class transformers 5.17 ships is expressible with existing features: the adapter holds once the refusal is
   bypassed (FR-25, second instance).

### 4.7 The encoder census

The same method on the encoder and embedding lineage: every masked-LM family of transformers 5.17 that is not a corpus entry (37), through its base model as a
bidirectional encoder, under the same guards (`census_def.py probe-enc`, `census_enc_v2.json`, `census_enc_report.json`). 25 build a tiny fixture; 12 do not
(`esm`, `esmc`, `flaubert`, `funnel`, `luke`, `modernvbert`, `neomme`, `perceiver`, `reformer`, `squeezebert`, `xlm`, `xmod`: sizes over the guard or configs that need a hand-made tiny config).

* **None reads with no adapter**: the standard template is a decoder, the built-in pack holds only the corpus's six encoders. All 25 are refused at `read` (`has no adapter and is not a causal language model class`).
* **Three are Level B by this lane's data**, in 6 to 12 lines each, every stage including the court: `camembert` and `data2vec_text` (RoBERTa's encoder under another name or prefix) and `ernie` (BERT with the task-type embeddings refused).
  So **3 of 25 (12 %)** by data today, all in the part of the lineage that is *BERT with another name*.
* **The other 22** are not examined one by one; by what their classes are: pre-LayerNorm BERTs (`megatron_bert`, `roberta_prelayernorm`, `xlm_roberta_xl`) and `roformer`/`rembert`/`convbert`/`mobilebert`/`squeezebert`/`electra`
  (factorised embeddings: FR-17's embedding projection order) are probably BERT-shaped data once the rows lowerer reads the whole spec (FR-17 step 2); `deberta` (disentangled attention),
  `eurobert`/`jina_embeddings_v3` (rope, bidirectional, GQA), `longformer`/`big_bird`/`nystromformer`/`mra`/`yoso` (sparse or approximated attention) and `fnet` (no attention) are features of FR-17;
  `layoutlm`, `tapas`, `roc_bert` carry extra embedding tables. This is a reading of the class names, not a measurement: the number FR-17 unlocks is *at most* 22 of 25, and the honest low
  estimate is the dozen BERT-shaped ones.
* **Hub weight differs from family count**: the encoder families that matter by downloads (BERT, RoBERTa/XLM-R, DistilBERT, MPNet, ModernBERT, the sentence-embedders built on them) are in the corpus, where 6 of 10 are Level B today; the census is the long tail.

## 5. Targets, honestly

| criterion | target | measured |
| --- | --- | --- |
| expressible with existing features (A or B) | ≥ 90 % | **63 %** of the curated corpus (77 % of its 57 text-generation entries; 63 % usage-weighted); **52 %** of the 85 buildable census families (40 % of all 111); **62 %** of the 142 decoder-only families (57 curated + 85 census) |
| needing a new feature | a few % | 15 % of the corpus (decoder-side features), 14 % routes, 7 % protocol, 1 % remote code |
| needing a new primitive | very few | **0** identified (corpus, census, and the five research passes) |
| court coverage | 100 % | 100 % of what lowers (53,687 / 53,687 commit points on 56 lowered models, 0 model-specific paths; 25 / 25 primitives reached) |

The target is **not met**. The ranked feature requests (`feature-requests.md`; size is a rough guess of the lowering
lane's effort: S days, M one to two weeks, L weeks, XL months) show what would move it:

<!-- BEGIN GENERATED: frs -->
| FR | feature | size | entries it names | entries |
| --- | --- | :-: | ---: | --- |
| FR-22 | image generation: conv/GroupNorm/adaLN/joint attention, denoise loop, VAE; UNets need carry pass-through (C1) | XL | 4 | `unet2d_condition`, `unet_sdxl`, `dit`, `flux` |
| FR-23 | audio: STFT/mel front end, Conv1d, codec/vocoder, audio input and output bindings | XL | 4 | `wav2vec2`, `speecht5`, `musicgen`, `encodec` |
| FR-09 | token-level top-k indexer attention (DeepSeek sparse attention) | L | 1 | `deepseek_v32` |
| FR-14 | layers without a mixer, ungated and latent MoE experts (Nemotron-H) | M | 1 | `nemotron_h` |
<!-- END GENERATED: frs -->

FR-17/18/19 (encoders, encoder–decoders, vision towers from a spec) are the single largest lever by entries and by
hub weight; FR-01 plus the small decoder features (FR-02..07, 14, 15, 25) are cheap; FR-22/23 are the long pole.

## 6. Level A uplift (what would let more models need no adapter)

<!-- BEGIN GENERATED: uplift -->
The standard template (no adapter) reads 10 of the 66 decoder-route entries without refusing; for the rest it names the keys it does not model:

| convention the template lacks | entries it blocks | the keys |
| --- | ---: | --- |
| MoE hyper-parameters and layer pattern | 14 | `decoder_sparse_step`, `first_k_dense_replace`, `mlp_only_layers`, `moe_intermediate_size`, `moe_topk`, `n_group`, `n_routed_experts`, `n_shared_experts`, `norm_topk_prob`, `num_experts`, `num_experts_per_tok`, `num_local_experts`, `num_nextn_predict_layers`, `routed_scaling_factor`, `router_jitter_noise`, `shared_expert_intermediate_size`, `topk_group` |
| MLA dimensions | 2 | `kv_lora_rank`, `q_lora_rank`, `qk_head_dim`, `qk_nope_head_dim`, `qk_rope_head_dim`, `v_head_dim` |
| nested VLM wrapper (text_config, vision_config, token ids) | 7 | `boi_token_index`, `eoi_token_index`, `image_token_id`, `image_token_index`, `mm_tokens_per_image`, `text_config`, `video_token_id`, `vision_config`, `vision_end_token_id`, `vision_start_token_id` |
| bias switches | 10 | `add_bias_linear`, `attention_out_bias`, `bias`, `enable_bias`, `qkv_bias`, `use_bias` |
| norm epsilon / norm kind aliases | 9 | `_remove_final_layer_norm`, `do_layer_norm_before`, `layer_norm_elementwise_affine`, `layer_norm_eps`, `layer_norm_epsilon`, `norm_epsilon` |
| activation aliases | 6 | `activation`, `activation_function`, `hidden_activation`, `mlp_hidden_act` |
| dimension aliases (n_embd, n_head, n_layer, ...) | 5 | `d_model`, `ffn_dim`, `ffn_hidden_size`, `max_seq_len`, `n_embd`, `n_head`, `n_heads`, `n_inner`, `n_layer`, `n_layers`, `n_positions`, `word_embed_proj_dim` |
| rope / position keys | 4 | `alibi`, `multi_query`, `new_decoder_architecture`, `no_rope_layer_interval`, `original_max_position_embeddings`, `use_parallel_residual` |
| everything else (family-specific keys) | 21 | 100 distinct keys |
<!-- END GENERATED: uplift -->

**Measured.** `tools/corpus/a-candidates/standard-v2.json` is a 14-line data file that extends the built-in template with
the two conventions a checkpoint's *tensor names* reveal (the MLP is gated exactly when `mlp.gate_proj` exists; bias
switches under other names and bookkeeping keys of new families are inert, because the biases are read from the `.bias`
tensors). Run with `PALW_CORPUS_STANDARD=<file>` it moves `arcee` and `seed_oss` to Level A with every stage holding;
`helium` and `ernie4_5` still fall back to a Level B adapter (the rope pairing, refuted at `float_vs_hf`), `hunyuan_v1_*`
stay C (the norm placement) and `bitnet` stays C (sub-layer norms, refuted at `bind`). None of the 39 built-in adapters
becomes redundant: their families differ in names, layouts and scoring, not just in switches.

A key group blocks an entry if the template refuses one of its keys; removing the group is *necessary* for the entry
to become Level A, not sufficient (the third column of every entry is checked by the later stages). Two things bound
what any convention can do. The rope pairing, the norm placement after or before the rotation, and the router scoring
are class code: no config key or tensor name reveals them (§4.2: Helium reads cleanly and is wrong). So the realistic
ceiling of "no adapter" is the Llama-shaped families whose conventions are readable from names and shapes: gating
(plain vs gated MLP from `gate_proj`), biases (from the `.bias` tensors), q/k norm scope (from the shape), tying (from
`lm_head`), the sliding pattern, MoE hyper-parameters when the router is a softmax top-k. Conventions that are
tensor-name-driven (gating, biases, sub-norms) move `arcee`, `seed_oss`, `ernie4_5`, `bitnet` (once FR-03 exists) and
the three nested-VLM text stages that need only a different tensor prefix; the MoE group is the biggest by entries but
each MoE family also chooses a scoring rule, so it moves only the softmax-top-k ones.

**Measured, second candidate.** `a-candidates/standard-v3.json` adds one more convention to v2: when `rope_parameters` is keyed by layer
type, layer *i* takes the rope of its own type, with that type's partial-rotary factor. It removes the refusal of five of the six census
families that have it and moves **none** to Level B: each stops at its own next convention (§4.6). The coordinator's decision is that it is
**not adopted** in the standard template — there is no evidence it matters — and the file stays in `a-candidates/` as the record of the
experiment.

The way past the template ceiling is not a bigger template but a **fixture-verified convention search** — kept, by decision, as a
**development tool** in `tools/corpus` (documented here), not a product feature: given the tiny HF fixture a
registrant has to produce anyway, enumerate the finite convention switches, keep the combination whose float reference matches the
fixture to 1e-4, and emit the adapter. A prototype is in the harness (`synthesize()` / `benign_key()` in `tests/corpus_v2.rs`; off with
`PALW_CORPUS_NO_SYNTH`): candidate = `standard-decoder` + the v2 conventions + every unknown key whose name is on a benign list treated as
inert + the finite switch the harness can verify (rope pairing, Half or Interleaved). With the four hand-written adapters removed it
re-derives `arcee` and `seed_oss` (Half), `helium` and `ernie4_5` (Interleaved) as Level B with no human adapter, every later stage
holding; on the census it synthesises **none**, because the 51 refusals are on keys that can change the math, which a search over
*benign* keys must not guess. Extending the search to norm kind (RMS or LayerNorm with the `layer_norm_eps` key), activation aliases and
router keys would reach the next layer of families (about 15 of the census's `layer_norm_eps` and activation-alias refusals), at the cost
of a larger candidate space; the verification step (float reference against HF to 1e-4 on the fixture the registrant must produce anyway)
is what makes that safe. It is data and a harness, not new protocol.

## 7. Reproducing

```bash
export CARGO_TARGET_DIR=~/Downloads/MISAKA-wt-b/corpus-target CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_TEST_THREADS=2
# fixtures (offline, one model at a time, two threads); the light specs are committed under tools/corpus/specs
HF_HUB_OFFLINE=1 OMP_NUM_THREADS=2 ~/Downloads/MISAKA-wt-b/tir-venv/bin/python misaka-palw-tir-lower/tools/corpus/gen_fixtures.py --specs
# quick: manifest + read/lower/admit from the committed light specs, plus the static "no model in the court" scan
cargo test -p misaka-palw-tir-lower --test corpus_v2
# full: every stage on the weights (about 4 minutes, debug build)
PALW_CORPUS_REPORT=report.json cargo test -p misaka-palw-tir-lower --test corpus_v2 corpus_v2_full -- --ignored --nocapture
python3 misaka-palw-tir-lower/tools/corpus/report.py report.json --md docs/design/palw/tir/corpus-v2.md
```

`PALW_CORPUS_ONLY=id,id` restricts the run, `PALW_CORPUS_FIXTURES` and `PALW_CORPUS_ADAPTERS` redirect the heavy
fixtures and the adapter directory (used to verify a feature request with a patched copy of a checkpoint),
`PALW_CORPUS_STANDARD=<file>` reads Level A with a candidate template instead of the built-in one, and `PALW_CORPUS_NO_SYNTH`
switches the convention search off.

The census (guarded: meta-device sizing first, one family per subprocess, RSS 4 GB, two threads):

```bash
cd misaka-palw-tir-lower/tools/corpus
export HF_HUB_OFFLINE=1 OMP_NUM_THREADS=2
~/Downloads/MISAKA-wt-b/tir-venv/bin/python census_def.py probe > census_probe.jsonl   # which families derive a tiny model
~/Downloads/MISAKA-wt-b/tir-venv/bin/python census_def.py reprobe [id ...]              # re-run the ones with overrides
~/Downloads/MISAKA-wt-b/tir-venv/bin/python census_def.py manifest                      # census_v2.json
~/Downloads/MISAKA-wt-b/tir-venv/bin/python gen_fixtures.py --entries census_v2.json --out ~/Downloads/MISAKA-wt-b/corpus-fixtures-census --specs --specs-dir census-specs
C=$PWD  # then, from the repository root, one cargo command at a time:
PALW_CORPUS_MANIFEST=$C/census_v2.json PALW_CORPUS_SPECS=$C/census-specs PALW_CORPUS_FIXTURES=~/Downloads/MISAKA-wt-b/corpus-fixtures-census \
  PALW_CORPUS_ADAPTERS=$C/census-adapters PALW_CORPUS_REPORT=$C/census_report.json \
  cargo test -p misaka-palw-tir-lower --test corpus_v2 corpus_v2_full -- --ignored --nocapture
python3 report.py report.json --md ../../../docs/design/palw/tir/corpus-v2.md --census census_report.json
```

## 8. The second axis: storage formats

A model is also a way its weights are stored. The pack's formats (lane F: the ggml types, GPTQ, AWQ, FP8 block scales,
compressed-tensors, MXFP4, NVFP4) are descriptor files; this lane wrote two more as a third party
(`tools/corpus/formats/`, generated by `gen_formats.py`, loaded by `tests/corpus_formats.rs`) and read the rest.

| format | by data today | evidence |
| --- | --- | --- |
| ggml block types (Q*, IQ*, TQ*, MXFP4, NVFP4), GPTQ, AWQ, FP8 block scales, compressed-tensors | yes (the pack) | lane F's vectors from the libraries' own dequantisers |
| MLX affine, 2/4/8-bit | **decode: yes** (`mlx_affine.json`); **announcement: no** (FR-33a: its config has no `quant_method`) | descriptor loads and passes its three vectors (`tests/corpus_formats.rs`); the vectors are a numpy transcription of the layout (MLX is not available offline) |
| bitsandbytes int8 (LLM.int8) | **decode: yes** (`bnb_int8.json`, which refuses 4-bit by a config check) | loads and passes its two vectors (same caveat) |
| bitsandbytes nf4/fp4, double quantisation | **no**: the weight is a flat `[N/2, 1]` tensor and its `[out, in]` shape is stored only in a JSON blob (FR-33b) | read from the library's serialisation |
| HQQ, Quanto, torchao | no: metadata blobs and tensor subclasses | not attempted |
| EXL2/EXL3, AQLM, Marlin repacks | plausible (indexed reads, at most two codebooks, a fixed permutation); not attempted | not attempted |

Two limits are in the registry, not in any descriptor: a format can be announced only by `quantization_config.quant_method`, and a
`tensors` descriptor can read the weight's shape only from its role tensors (FR-33). Hub usage of these formats is not measured here.

## 9. Open

* **Not built by this lane, read to need a feature** (§4.6): `afmoe`, `laguna` (FR-30), the BERT-lineage decoders (FR-29), DSA (FR-09), `minimax` (FR-32), `olmo_hybrid`,
  `recurrent_gemma`, `zaya`, `doge`, `hrm_text`; and the 8 families with no automatic tiny config (`bamba`, `cohere_compass_text`, `dots1`, `inkling_text`, `lfm2_moe`, `qwen4_exp`,
  `xlstm`, `zamba`: lane G's hand-made tiny configs rescued `qwen4_exp_text` and `ministral`; the rest need one).
* Encoder–decoders have no weights stage in this harness (lower + admit only); vision, encoder and audio routes are probed the same way. The fidelity records are in
  `hf-coverage.md` §13, §17. The research passes (`feature-requests.md`) are analysis, not runs: their sizes are the research's own arithmetic.
* A storage-format corpus beyond the two descriptors of §8, with the libraries' own dequantisers where they can be run; hub usage of formats is not measured.
* LoRA/PEFT adapters (candidate = parent + adapter) as a third axis: `lora.rs` exists; not part of the 100.
* An **encoder census** (the masked-LM and embedding families of transformers 5.17 through the same meta-device guard) to size what FR-17 unlocks by family count.
* **Decided (coordinator, 2026-10-01).** Lane G takes the requests in the order FR-01, FR-18, FR-17, FR-19, FR-02, FR-09 after its CP2 (FR-01, FR-02, FR-07, FR-18 phase 1,
  FR-25, FR-26 have landed on `tir/generic` and are measured here); FR-22 and FR-23 go with RFC-0003's lane D; FR-25 and FR-26 are accepted and implemented in the harness;
  per-layer-type rope is not adopted in the standard template; the convention search stays a dev tool. The corpus is re-measured after each landing of `tir/generic`
  (merged into `tir/corpus`) and after every move of 10 points or more.
