# PALW-TIR — the architecture corpus v2: can any model be added by data alone?

| Field | Value |
| --- | --- |
| Status | measured on 100 architectures (transformers 5.17.0 / diffusers 0.40, tiny random-init fixtures); the numbers below are a regenerable report, not prose |
| Lane | H (corpus and coverage), branch `tir/corpus` off `tir/generic` `1a4964205` |
| Question | RFC-0002's aim is that **no model needs a core developer**. One more supported model is worthless; only infrastructure that admits them all counts. This document measures it |
| Harness | `misaka-palw-tir-lower/tests/corpus_v2.rs` + `tools/corpus/` (fixture generator, manifest, adapters, blockers, report renderer) |
| Report | `tools/corpus/report.json` (machine-readable, one object per entry) |
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
* **Level B** — a data adapter (`misaka.palw.model-adapter.v1`) suffices: a built-in file, or one this lane wrote
  (`tools/corpus/adapters/*.json`, fed to the reader as a user-supplied file). No Rust, no protocol change.
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
| Level B — a thin data adapter | 47 | 47 % | 46 % |
| Level C — a capability is missing | 47 | 47 % | 45 % |
| **A + B (expressible with existing features)** | **53** | **53 %** | **55 %** |
| *Level C, but lowered today by a per-family Rust route (not data): `t5`, `t5_gated`, `bart`, `mbart`, `marian`, `clip_vision`, `siglip_vision`* | 7 | 7 % | |
| *supported today by any route (A + B + Rust routes)* | 60 | 60 % | |

Level C by kind of gap (this lane's classification, `tools/corpus/blockers.json`):

| kind | entries | meaning |
| --- | ---: | --- |
| route | 21 | no data route exists for this kind of model yet (the lowering is Rust per family, or absent); existing primitives suffice |
| feature | 20 | a generic FEATURE is missing; lowerable in principle with the existing primitives |
| protocol | 5 | a protocol capability beyond the IR is needed (an input binding, a second history, a stage kind) |
| remote | 1 | the reference semantics are remote code outside transformers (cannot be pinned) |

Court coverage: 47880 commit points over 45 lowered models reproduced by the cone evaluator from their opened leaves (reference evaluator and typed backend; 0 failures); 25 of the 25 primitives and 6 commit-point roles reached.

| category | A | B | C | total |
| --- | ---: | ---: | ---: | ---: |
| audio | 0 | 0 | 5 | 5 |
| encdec | 0 | 0 | 7 | 7 |
| encoder | 0 | 6 | 4 | 10 |
| image-gen | 0 | 0 | 6 | 6 |
| remote-code | 0 | 2 | 1 | 3 |
| text/dense | 6 | 17 | 5 | 28 |
| text/hybrid | 0 | 7 | 6 | 13 |
| text/moe | 0 | 9 | 7 | 16 |
| vision | 0 | 0 | 4 | 4 |
| vlm | 0 | 6 | 2 | 8 |
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
| `arcee` | text/dense | **B** | third-party adapter `arcee` (tools/corpus/adapters) | 7 |  |  |
| `apertus` | text/dense | **C** | - |  | xIELU activation (learned per-layer parameters) → FR-05 | read |
| `helium` | text/dense | **B** | third-party adapter `helium` (tools/corpus/adapters) | 7 |  |  |
| `hunyuan_v1_dense` | text/dense | **C** | third-party adapter `hunyuan-v1-dense` (tools/corpus/adapters) (refuted: float_vs_hf) | 9 | q/k RMS norm applied AFTER the rotation → FR-02 | float_vs_hf |
| `seed_oss` | text/dense | **B** | third-party adapter `seed-oss` (tools/corpus/adapters) | 8 |  |  |
| `ernie4_5` | text/dense | **B** | third-party adapter `ernie4-5` (tools/corpus/adapters) | 9 |  |  |
| `bitnet` | text/dense | **C** | standard template (no adapter) (refuted: bind) | 7 | attn_sub_norm before o_proj and ffn_sub_norm before down_proj → FR-03 | bind |
| `persimmon` | text/dense | **B** | third-party adapter `persimmon` (tools/corpus/adapters) | 12 |  |  |
| `diffllama` | text/dense | **C** | - |  | differential attention (head-pair difference, 2d RMS group norm, derived lambda) → FR-04 | read |
| `cwm` | text/dense | **A** | standard template (no adapter) | 9 |  |  |
| `gemma3n_text` | text/dense | **C** | - |  | RESIDUAL_ALTUP, RESIDUAL_LAUREL, FFN_ACTIVATION_SPARSITY → FR-12 | read |
| `mixtral` | text/moe | **B** | built-in adapter `mixtral` | 9 |  |  |
| `qwen2_moe` | text/moe | **B** | built-in adapter `qwen2-moe` | 12 |  |  |
| `qwen3_moe` | text/moe | **B** | built-in adapter `qwen3-moe` | 11 |  |  |
| `granitemoe` | text/moe | **B** | built-in adapter `granitemoe` | 11 |  |  |
| `deepseek_v3` | text/moe | **B** | built-in adapter `deepseek-v3` | 15 |  |  |
| `gpt_oss` | text/moe | **B** | built-in adapter `gpt-oss` | 14 |  |  |
| `llama4_text` | text/moe | **B** | built-in adapter `llama4-text` | 20 |  |  |
| `glm4_moe` | text/moe | **B** | built-in adapter `glm4-moe` | 17 |  |  |
| `dbrx` | text/moe | **C** | third-party adapter `dbrx` (tools/corpus/adapters) (refuted: bind) | 10 | experts stored flat [E*I, D]; everything else is data → FR-01 | bind |
| `jetmoe` | text/moe | **C** | - |  | mixture-of-attention: routed per-expert q/o projections → FR-06 | read |
| `ernie4_5_moe` | text/moe | **C** | third-party adapter `ernie4-5-moe` (tools/corpus/adapters) (refuted: bind) | 14 | selection bias [1,E] needs a squeeze; softmax router ignores selection_bias → FR-01, FR-07 | bind |
| `hunyuan_v1_moe` | text/moe | **C** | third-party adapter `hunyuan-v1-moe` (tools/corpus/adapters) (refuted: float_vs_hf) | 12 | q/k RMS norm applied AFTER the rotation → FR-02 | float_vs_hf |
| `minimax_m2` | text/moe | **B** | third-party adapter `minimax-m2` (tools/corpus/adapters) | 12 |  |  |
| `longcat_flash` | text/moe | **C** | - |  | zero-computation experts and shortcut-connected MoE → FR-08 | read |
| `deepseek_v32` | text/moe | **C** | - |  | DeepSeek sparse attention: a token-level top-k indexer → FR-09 | read |
| `deepseek_v4` | text/moe | **C** | - |  | hyper-connections, compressed sparse/heavily compressed attention, hash routing, sqrt-softplus scoring → FR-10 | read |
| `qwen3_next` | text/hybrid | **B** | built-in adapter `qwen3-next` | 19 |  |  |
| `qwen3_5_moe` | text/hybrid | **B** | built-in adapter `qwen3-5-moe` | 18 |  |  |
| `jamba` | text/hybrid | **B** | built-in adapter `jamba` | 11 |  |  |
| `mamba` | text/hybrid | **B** | built-in adapter `mamba` | 7 |  |  |
| `mamba2` | text/hybrid | **B** | built-in adapter `mamba2` | 6 |  |  |
| `falcon_mamba` | text/hybrid | **B** | built-in adapter `falcon-mamba` | 7 |  |  |
| `rwkv` | text/hybrid | **B** | built-in adapter `rwkv4` | 8 |  |  |
| `falcon_h1` | text/hybrid | **C** | - |  | MIXER_PARALLEL_BRANCH, SCALE_MUP → FR-11 | read |
| `granitemoehybrid` | text/hybrid | **C** | third-party adapter `granitemoehybrid` (tools/corpus/adapters) (refuted: bind) | 11 | fused shared-expert gate/up tensor; a built-in refusal cannot be overridden → FR-01, FR-25 | bind |
| `zamba2` | text/hybrid | **C** | - |  | ATTN_SHARED_BLOCK → FR-13 | read |
| `nemotron_h` | text/hybrid | **C** | - |  | LAYER_FFN_ONLY → FR-14 | read |
| `lfm2` | text/hybrid | **C** | - |  | gated short convolution mixer → FR-15 | read |
| `kimi_linear` | text/hybrid | **C** | - |  | Kimi delta attention (channel-wise gated delta rule) → FR-16 | read |
| `bert` | encoder | **B** | built-in adapter `bert` | 12 |  |  |
| `roberta` | encoder | **B** | built-in adapter `roberta` | 12 |  |  |
| `xlm_roberta` | encoder | **B** | built-in adapter `roberta` | 12 |  |  |
| `distilbert` | encoder | **B** | built-in adapter `distilbert` | 11 |  |  |
| `mpnet` | encoder | **B** | built-in adapter `mpnet` | 12 |  |  |
| `deberta_v2` | encoder | **C** | - |  | an adapter for `DebertaV2Model` → FR-17 | read |
| `albert` | encoder | **C** | third-party adapter `albert` (tools/corpus/adapters) (refuted: panic) | 13 | factorised embedding in the bidirectional lowerer (it panics) → FR-17, FR-26 | panic |
| `modernbert` | encoder | **C** | - |  | an adapter for `ModernBertModel` → FR-17 | read |
| `nomic_bert` | encoder | **C** | - |  | an adapter for `NomicBertModel` → FR-17 | read |
| `clip_text` | encoder | **B** | built-in adapter `clip-text` | 10 |  |  |
| `t5` | encdec | **C** | core Rust route |  | an adapter for `T5ForConditionalGeneration` → FR-18 | read |
| `t5_gated` | encdec | **C** | core Rust route |  | an adapter for `T5ForConditionalGeneration` → FR-18 | read |
| `bart` | encdec | **C** | core Rust route |  | an adapter for `BartForConditionalGeneration` → FR-18 | read |
| `mbart` | encdec | **C** | core Rust route |  | an adapter for `MBartForConditionalGeneration` → FR-18 | read |
| `marian` | encdec | **C** | core Rust route |  | an adapter for `MarianMTModel` → FR-18 | read |
| `longt5` | encdec | **C** | - |  | an adapter for `LongT5ForConditionalGeneration` → FR-18 | read |
| `t5_encoder` | encdec | **C** | - |  | an adapter for `T5EncoderModel` → FR-18 | read |
| `llava` | vlm | **B** | built-in adapter `vlm-llama` | 7 |  |  |
| `qwen2_vl` | vlm | **B** | built-in adapter `vlm-qwen2-vl` | 9 |  |  |
| `qwen2_5_vl` | vlm | **B** | built-in adapter `vlm-qwen2-vl` | 9 |  |  |
| `qwen3_vl` | vlm | **B** | third-party adapter `qwen3-vl` (tools/corpus/adapters) | 10 |  |  |
| `gemma3_vlm` | vlm | **B** | built-in adapter `vlm-gemma3` | 14 |  |  |
| `paligemma` | vlm | **C** | - |  | ATTN_PREFIX_LM → FR-20 | read |
| `idefics3` | vlm | **B** | third-party adapter `idefics3` (tools/corpus/adapters) | 7 |  |  |
| `mllama` | vlm | **C** | - |  | ATTN_CROSS → FR-21 | read |
| `clip_vision` | vision | **C** | core Rust route |  | an adapter for `CLIPVisionModelWithProjection` → FR-19 | read |
| `siglip_vision` | vision | **C** | core Rust route |  | an adapter for `SiglipVisionModel` → FR-19 | read |
| `vit` | vision | **C** | - |  | an adapter for `ViTModel` → FR-19 | read |
| `resnet` | vision | **C** | - |  | an adapter for `ResNetModel` → FR-19 | read |
| `unet2d_condition` | image-gen | **C** | - |  | Conv2d, GroupNorm, timestep embedding, cross-attention, denoise loop → FR-22 | read |
| `unet_sdxl` | image-gen | **C** | - |  | as the SD UNet plus added text/time conditioning → FR-22 | read |
| `dit` | image-gen | **C** | - |  | patchify, adaLN-Zero, class conditioning, denoise loop → FR-22 | read |
| `flux` | image-gen | **C** | - |  | MMDiT double/single blocks, 3-axis RoPE, flow-matching loop → FR-22 | read |
| `sd3` | image-gen | **C** | - |  | MMDiT joint attention, adaLN, flow-matching loop → FR-22 | read |
| `vae_kl` | image-gen | **C** | - |  | convolutional encoder/decoder with a mid-block attention → FR-22 | read |
| `whisper` | audio | **C** | - |  | an adapter for `WhisperForConditionalGeneration` → FR-23 | read |
| `wav2vec2` | audio | **C** | - |  | an adapter for `Wav2Vec2ForCTC` → FR-23 | read |
| `speecht5` | audio | **C** | - |  | an adapter for `SpeechT5ForTextToSpeech` → FR-23 | read |
| `musicgen` | audio | **C** | - |  | an adapter for `MusicgenForConditionalGeneration` → FR-23 | read |
| `encodec` | audio | **C** | - |  | an adapter for `EncodecModel` → FR-23 | read |
| `chatglm3` | remote-code | **C** | - |  | REFERENCE_REMOTE_CODE → FR-24 | read |
| `internlm2` | remote-code | **B** | built-in adapter `internlm2` | 8 |  |  |
| `minicpm` | remote-code | **B** | built-in adapter `minicpm` | 11 |  |  |
<!-- END GENERATED: results -->

## 4. What the measurement says

(See the sections below; each claim is backed by an entry in the report or a named test.)

### 4.1 The pipeline holds wherever the reader says yes

Of the 53 entries that read at Level A or B and pass, every one passes **all** later stages: the program is admitted,
the float reference equals `transformers` to ≤ 3e-7 of the logit scale, the integer program agrees with it, the three
implementations are bit-identical at every position and every commit point, and every commit point replays through
the generic court path (47,880 replays on 45 lowered models, 25 of 25 primitives reached, 0 failures). The IR, the
evaluators and the court are model-free in exactly the sense the freeze claims: no new primitive was needed for
anything in the corpus, and no Level C entry below is blocked by one.

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

This lane wrote **eight** adapters as third-party data files; each lowers, is admitted and passes every stage on
its first or second attempt: `seed-oss`, `ernie4-5` (a use_bias switch and interleaved pairs), `persimmon`
(per-head fused qkv, LayerNorm with bias, a per-head q/k LayerNorm, ReLU²), `arcee`, `helium`, `minimax-m2`
(sigmoid router with correction bias, whole-projection q/k norm), `qwen3-vl` and `idefics3` (the text stage of a
VLM wrapper under its own tensor prefix). Each is 15–40 lines of JSON. Six more adapters read, lower and are
admitted and stop at **one** well-defined step (§4.4) — they are ready for the feature that closes them.

### 4.4 Where data stops: three precise results

1. **Weights are not yet data.** `dbrx`, `granitemoehybrid` and `ernie4_5_moe` read and lower as data and stop at
   *bind*: DBRX stores its experts flat (`[E·I, D]`, the down matrix transposed), Granite-4 fuses the shared
   expert's gate and up into one tensor (the binder hard-codes `MlpLayout::Separate` for it), ERNIE stores its
   selection bias as `[1, E]`. With that one storage difference removed in a copy of the fixture
   (`tools/corpus/patch_checkpoint.py`), `dbrx` and `granitemoehybrid` pass **every** stage, court included. The
   enums `QkvLayout`/`MlpLayout` are the wrong shape for this; `Src` (`weights/mod.rs`) is already a total
   expression tree and only needs a surface in the adapter language (FR-01).
2. **The lowering silently ignores a flag the spec accepts.** `RouterSpec.selection_bias` is applied only under
   sigmoid scoring (`float_ref::route`, `lower::lower_route`); with softmax scoring it is dropped without a
   refusal. ERNIE-4.5-MoE needs it; the corpus shows a 17 % error against `transformers` (FR-07, FR-26).
3. **Two refusals are stale or unreachable.** `refusals.json`'s entry for `GraniteMoeHybridForCausalLM` names
   `MIXER_LAYER_PATTERN_HYBRID_V1`, but the layer pattern *is* expressible (the third-party adapter proves it); and
   `read_model` checks the refusal list before any adapter, so no user adapter can ever override one (FR-25).

### 4.5 The big gap is routes, not features

47 entries are Level C. Twenty are decoder-side **features**, each small and unique to one or two families
(§6). Twenty-one are **routes**: bidirectional encoders beyond BERT's layer structure, encoder–decoders, vision
towers, image generation. For three of those kinds the lowering exists — as Rust per family (`parse_encdec`,
`parse_vision`, `lower::bidir`), not as data — so a new T5-like, ViT-like or ModernBERT-like model needs a core
developer. That is the infrastructure gap that matters most: these routes are where the hub's repository counts
are (T5/BART/Marian translation, CLIP/SigLIP, ViT/ResNet classification, sentence embedders). Five audio models
need a protocol capability (an audio input binding) and a front end, one is remote code. **No Level C entry
needs a new primitive** — every decomposition sketched in `feature-requests.md` lands on the 25.

## 5. Targets, honestly

| criterion | target | measured |
| --- | --- | --- |
| expressible with existing features (A or B) | ≥ 90 % | **53 %** of the corpus (68 % of the 57 text-generation entries; 55 % usage-weighted) |
| needing a new feature | a few % | 20 % (decoder-side features), 21 % (routes) |
| needing a new primitive | very few | **0** identified |
| court coverage | 100 % | 100 % of what lowers (47,880 / 47,880 commit points, 0 model-specific paths) |

The target is **not met**. The ranked feature requests (`feature-requests.md`; size is a rough guess of the lowering
lane's effort: S days, M one to two weeks, L weeks, XL months) show what would move it:

<!-- BEGIN GENERATED: frs -->
| FR | feature | size | entries it names | entries |
| --- | --- | :-: | ---: | --- |
| FR-18 | encoder-decoders lowered from a spec (cross-attention, relative bias, local attention) | L | 7 | `t5`, `t5_gated`, `bart`, `mbart`, `marian`, `longt5`, `t5_encoder` |
| FR-22 | image generation: conv/GroupNorm/adaLN/timestep blocks, denoise loop, VAE | XL | 6 | `unet2d_condition`, `unet_sdxl`, `dit`, `flux`, `sd3`, `vae_kl` |
| FR-23 | audio: front end, Conv1d, codec/vocoder, audio input and output bindings | XL | 5 | `whisper`, `wav2vec2`, `speecht5`, `musicgen`, `encodec` |
| FR-17 | bidirectional encoders lowered from a ModelSpec (not BERT-shaped Rust) | L | 4 | `deberta_v2`, `albert`, `modernbert`, `nomic_bert` |
| FR-19 | vision towers and convolutions lowered from a spec | L | 4 | `clip_vision`, `siglip_vision`, `vit`, `resnet` |
| FR-01 | weights as data: tensor expressions (slice, reshape, squeeze, transpose, stack) in adapter `names` | M | 3 | `dbrx`, `ernie4_5_moe`, `granitemoehybrid` |
| FR-02 | q/k norm applied after the rotation | S | 2 | `hunyuan_v1_dense`, `hunyuan_v1_moe` |
| FR-03 | sub-layer norms (norm before o_proj, norm before down_proj) | S | 1 | `bitnet` |
| FR-04 | differential attention | M | 1 | `diffllama` |
| FR-05 | learned pointwise activation (xIELU) | S | 1 | `apertus` |
| FR-06 | mixture-of-attention (routed per-expert q/o projections) | M | 1 | `jetmoe` |
| FR-07 | selection bias under softmax routing (+ refuse a flag the lowerer would drop) | S | 1 | `ernie4_5_moe` |
| FR-08 | zero-computation experts and shortcut-connected MoE | M | 1 | `longcat_flash` |
| FR-09 | token-level top-k indexer attention (DeepSeek sparse attention) | L | 1 | `deepseek_v32` |
| FR-10 | DeepSeek-V4 set: hyper-connections, compressed attention, hash routing, sqrt-softplus | L | 1 | `deepseek_v4` |
| FR-11 | parallel mixers in one layer (attention + Mamba-2) and muP multipliers | M | 1 | `falcon_h1` |
| FR-12 | Gemma-3n set: AltUp, LAuReL, activation sparsity | L | 1 | `gemma3n_text` |
| FR-13 | shared attention block with per-depth LoRA (Zamba2) | L | 1 | `zamba2` |
| FR-14 | layers without a mixer (a layer that is one block) | S | 1 | `nemotron_h` |
| FR-15 | gated short-convolution mixer | M | 1 | `lfm2` |
| FR-16 | Kimi delta attention | L | 1 | `kimi_linear` |
| FR-20 | prefix-LM attention | M | 1 | `paligemma` |
| FR-21 | cross-attention layers in a decoder | L | 1 | `mllama` |
| FR-24 | a way to pin a remote-code reference | policy | 1 | `chatglm3` |
| FR-25 | user adapters override built-in refusals; drop the stale Granite-hybrid refusal | S | 1 | `granitemoehybrid` |
| FR-26 | lowerer robustness and Level A honesty (panics, dropped flags, unread tensors) | S | 1 | `albert` |
<!-- END GENERATED: frs -->

FR-17/18/19 (encoders, encoder–decoders, vision towers from a spec) are the single largest lever by entries and by
hub weight; FR-01 plus the small decoder features (FR-02..07, 14, 15, 25) are cheap; FR-22/23 are the long pole.

## 6. Level A uplift (what would let more models need no adapter)

<!-- BEGIN GENERATED: uplift -->
The standard template (no adapter) reads 10 of the 65 decoder-route entries without refusing; for the rest it names the keys it does not model:

| convention the template lacks | entries it blocks | the keys |
| --- | ---: | --- |
| MoE hyper-parameters and layer pattern | 13 | `decoder_sparse_step`, `first_k_dense_replace`, `mlp_only_layers`, `moe_intermediate_size`, `moe_topk`, `n_group`, `n_routed_experts`, `n_shared_experts`, `norm_topk_prob`, `num_experts`, `num_experts_per_tok`, `num_local_experts`, `num_nextn_predict_layers`, `routed_scaling_factor`, `router_jitter_noise`, `shared_expert_intermediate_size`, `topk_group` |
| MLA dimensions | 2 | `kv_lora_rank`, `q_lora_rank`, `qk_head_dim`, `qk_nope_head_dim`, `qk_rope_head_dim`, `v_head_dim` |
| nested VLM wrapper (text_config, vision_config, token ids) | 6 | `boi_token_index`, `eoi_token_index`, `image_token_id`, `image_token_index`, `mm_tokens_per_image`, `text_config`, `video_token_id`, `vision_config`, `vision_end_token_id`, `vision_start_token_id` |
| bias switches | 8 | `attention_out_bias`, `bias`, `enable_bias`, `qkv_bias`, `use_bias` |
| norm epsilon / norm kind aliases | 8 | `_remove_final_layer_norm`, `do_layer_norm_before`, `layer_norm_elementwise_affine`, `layer_norm_eps`, `layer_norm_epsilon`, `norm_epsilon` |
| activation aliases | 5 | `activation`, `activation_function`, `hidden_activation` |
| dimension aliases (n_embd, n_head, n_layer, ...) | 5 | `d_model`, `ffn_dim`, `ffn_hidden_size`, `max_seq_len`, `n_embd`, `n_head`, `n_heads`, `n_inner`, `n_layer`, `n_layers`, `n_positions`, `word_embed_proj_dim` |
| rope / position keys | 4 | `alibi`, `multi_query`, `new_decoder_architecture`, `no_rope_layer_interval`, `original_max_position_embeddings`, `use_parallel_residual` |
| everything else (family-specific keys) | 18 | 60 distinct keys |
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

The way past that ceiling is not a bigger template but a **fixture-verified convention search**: given the tiny HF
fixture a registrant has to produce anyway, enumerate the finite convention switches (rope pairing, MLP gating, norm
placement, bias map, router scoring, tensor prefix), keep the combination whose float reference matches the fixture to
1e-4, and emit the adapter. The harness already contains every piece (a reader that takes adapter text, the float
reference against HF, the bind check): the search is a loop around them. It is data and a harness, not new protocol,
and it turns "write an adapter" into "run a command" for every family that is a combination of existing features.

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
fixtures and the adapter directory (used to verify a feature request with a patched copy of a checkpoint).

## 8. Open

* Storage formats (GPTQ/AWQ/GGUF/FP8/MXFP4 exist as descriptors; bitsandbytes, MLX, EXL2, HQQ, compressed-tensors,
  torchao are not) as a second corpus.
* LoRA/PEFT adapters (candidate = parent + adapter) as a third axis: `lora.rs` exists; not part of the 100.
* The encoder–decoder and vision routes are probed to a lowered, admitted program only; their fidelity records are
  in `hf-coverage.md` §13, §17.
