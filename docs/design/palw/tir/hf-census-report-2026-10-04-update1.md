# PALW-TIR — HF census, update 1 (2026-10-04 afternoon): the H4 blockers re-measured

*Same snapshot (`2026-10-03T131904Z`), sample, seed and method as `hf-census-report-2026-10-04.md`; judged by `palw-class` at
`23f8a9feb` (the sample's headers and shape passes re-run in full: 1,773 / 836 rows; three bucket cohorts of n = 150 judged at the
shape depth; the frame's GGUF supplement re-run). Metadata and headers only; rights policy `none`; no fence armed.*

## 概要(日本語)

- H4 の最大 blocker だった **COURT_BUDGET(frame 20.9 %)と ADMISSION_EXCEEDS(8.6 %)の大半は preflight 側のバグ**で、チェーンの規則でも天井でもありませんでした: (1) layout 探索が IR flag day の高さ(DAA 2,000)の規則で判定していて、palw_tir_fence2(3,600)の range-twin sizing が効かず、各 logits tile で sizing 上限 2^26 に当たり、最も広い layout の拒否を報告していた; (2) 宣言 context が C の class を履歴上限 2^18 の窓で lower していた(max_window = C で同値); (3) DA 応答可否の twin も旧い高さで判定していた。3 件とも off-chain(SDK)の修正で、consensus の変更も fence もなし。
- 修正後の残り COURT_BUDGET/ADMISSION_EXCEEDS は 100B 超(DeepSeek-V3/R1・gpt-oss-120b・Qwen3-235B・Mistral-Large 123B 等)だけで、D_all の 0.35 %、frame の 1.4 %。これが RFC-0006(layer×position cell)の対象になる実残余です。
- **technical shape-ready**: D_all 2.42 % → **17.60 %(LB 16.23 %)**、D_files 3.41 % → 24.79 %(LB 22.85 %)。**local-LLM frame**: 9.71 % → **57.22 %(LB 55.18 %)**。90 % は未達。strict は 0 のまま。

## 1. Before / after

| | H4 (`34f7f0721`) | Update 1 (`23f8a9feb`) |
| --- | ---: | ---: |
| `D_all` technical shape-ready | 2.42 % (LB 2.20 %) | **17.60 % (LB 16.23 %)** |
| `D_files` technical shape-ready | 3.41 % (LB 3.10 %) | **24.79 % (LB 22.85 %)** |
| `D_all` `lower` PASS | 9.31 % (LB 8.93 %) | 19.30 % (LB 17.90 %) |
| download-weighted shape-ready | 4.07 % (LB 2.43 %) | 8.84 % (LB 7.98 %) |
| local-LLM frame, TIR shape-ready of `L_files` | 9.71 % (LB 8.92 %) | **57.22 % (LB 55.18 %)** |
| strict (rights `none`), registration-ready, TIR `Final`, GVM `Final` | 0 | 0 |

| Change (commit) | Bucket measured | Before | After |
| --- | --- | --- | --- |
| Layout search judged at the judged height (`a25a79fe0`), lowered window = declared context and DA twin at the judged height (`ddfaf6c8e`) | eligible sampled decoders judged at the shape depth | 151 of 469 judged shape-ready (32 %; `COURT_BUDGET` 219, `ADMISSION_EXCEEDS` 93, budget spent 5) | 822 of 836 shape-ready; 12 `COURT_BUDGET` + 2 `ADMISSION_EXCEEDS`, every one ≥ 100 B parameters except one 8 B 2-bit export |
| PEFT adapter composed with its pinned base (`3b6cc1164`) | `ADAPTER_UNCHECKED` cohort, N = 186,671, n = 150 | 0 shape-ready | 88 / 150 shape-ready |
| Training-tool keys inert, `trust_remote_code` classified, partial-rotary/attention_bias keys read (`e9c39983b`, `956302828`, `23f8a9feb`) | headers pass, same 1,773 | 684 eligible; `CONFIG_KEY_UNREAD` 272 rows | 836 eligible; `CONFIG_KEY_UNREAD` ~100 |
| Task inference v2 (`cb1152dc5`) | `TASK_UNKNOWN` cohort, N = 894,223, n = 150 | 0 | 19 / 150 shape-ready (≈113 k repositories; mostly untasked LLM GGUFs) |
| Base resolution v2: renames followed (`apply_renames.py`) | `BASE_UNPINNED` cohort, N = 270,091, n = 150 | 0 | 0 / 150 — the bucket is `absent` (no base declared), `base_gated` (a gated base whose terms the census does not accept) and `ambiguous`; renames resolved 53,368 references elsewhere |

The 2,048 → primary implication was re-checked under the new rules implicitly: the after pass judged every eligible class at its
primary context unless 2,048 refused on a monotone code (14 rows).

## 2. Results (update 1)

### Denominators

| | Repositories | Downloads (30 days) |
| --- | ---: | ---: |
| `D_all` | 3,117,871 | 2,885,554,744 |
| `D_files` (source PASS on the listing) | 2,213,693 (71.00 %) | 2,792,085,613 |
| `D_rights`, policy `none` (the headline's) | 0 | 0 |
| `D_rights`, policy `permissive-card-v0` (**proposal, not adopted**) | 588,050 (18.86 %) | 2,201,818,269 |

### The funnel (estimates with one-sided 95 % lower bounds)

| Stage | Strict (rights `none`) | Technical, of `D_all` | Technical, of `D_files` | Technical, of `D_rights` (proposal) |
| --- | ---: | ---: | ---: | ---: |
| `source` PASS | 0 | 71.00 % (exact) | 100 % | 100 % |
| `lower` PASS | 0 | 19.30 % (LB 17.90 %) | 27.19 % (LB 25.20 %) | 20.80 % (LB 17.36 %) |
| `admit` PASS at the primary context (**shape-ready**) | 0 | 17.60 % (LB 16.23 %) | 24.79 % (LB 22.85 %) | 16.49 % (LB 13.44 %) |
| admitted at 2,048 only (its own stratum) | 0 | 0.016 % | | |
| `pack`, `seat`, `final` | 0 | not run (weights / a chain) | | |
| **registration-ready** | **0** | **0 measured** (`pack` not run) | | |

Download-weighted (normal-approximation bounds): `lower` 10.97 % (LB 10.09 %), shape-ready 8.84 % (LB 7.98 %).

Sample: n = 1,773 (seed `misaka-palw-hf-census/2026-10-03T131904Z/v1`), phase-1 rows 1,773, nonresponse 0; eligible for the shape depth 836, judged prefix 836 (unjudged eligible estimated at 0 repositories, counted as not passing).

### Where repositories stop (technical view, estimated repositories; the first gate that fails or cannot be run)

| Gate | Code | Repositories | Share of `D_all` | Share of downloads | Leading arguments |
| --- | --- | ---: | ---: | ---: | --- |
| lower | `TASK_UNKNOWN` | 661,725 | 21.22 % | 0.05 % |  |
| source | `MISSING_WEIGHTS` | 579,477 | 18.59 % | 0.34 % |  |
| admit | `SHAPE_READY` | 548,883 | 17.60 % | 8.84 % |  |
| lower | `MODALITY_PROFILE_MISSING` | 460,860 | 14.78 % | 32.11 % | `text-classification` 138,000, `reinforcement-learning` 75,391, `automatic-speech-recognition` 53,296 |
| source | `BASE_UNPINNED` | 270,091 | 8.66 % | 0.07 % | `absent` 127,843, `base_gated` 95,432, `ambiguous` 25,208 |
| lower | `FORMAT_UNSUPPORTED` | 130,554 | 4.19 % | 3.57 % | `pytorch` 78,883, `base:pytorch` 18,667, `pytorch-adapter` 4,978 |
| lower | `PARTIAL_TASK_ONLY` | 75,937 | 2.44 % | 11.90 % | `image-text-to-text` 62,580, `any-to-any` 11,574, `visual-question-answering` 758 |
| source | `GATED_ACCESS` | 53,420 | 1.71 % | 2.72 % | `manual` 34,213, `auto` 19,187, `VibeThinker-3B.Q8_0.gguf` 10 |
| lower | `FEATURE_C` | 47,995 | 1.54 % | 1.73 % | `GEN_UNET_SKIP_V1` 28,312, `an adapter for `Gemma4ForConditionalGeneration`` 2,978, `an adapter for `NewModel`` 2,200 |
| admit | `NOT_RUN_PIPELINE_ADMISSION` | 41,417 | 1.33 % | 1.84 % |  |
| lower | `ARCH_REFUSED` | 37,877 | 1.21 % | 8.52 % | `gguf-missing:llama.embedding_length` 6,080, `gguf-arch:seed_oss` 5,962, `encdec-no-adapter:M2M100ForConditionalGeneration` 2,578 |
| lower | `TOKENIZER_MISSING` | 32,470 | 1.04 % | 0.05 % |  |
| lower | `CONFIG_KEY_UNREAD` | 31,715 | 1.02 % | 0.85 % | `rope_interleaved` 3,038, `n_special` 2,600, `max_seq_len` 2,166 |
| lower | `NOT_RUN_NEEDS_TENSOR_DATA` | 23,600 | 0.76 % | 0.18 % | `rope_freqs.weight (256 bytes)` 21,841, `rope_freqs.weight (128 bytes)` 1,319, `a tensor of the file` 440 |
| lower | `ADAPTER_UNCHECKED` | 21,156 | 0.68 % | 0.05 % |  |
| lower | `CUSTOM_CODE_UNMODELLED` | 19,558 | 0.63 % | 6.00 % | `remote-code:QuasarForCausalLM` 3,460, `remote-code:PhiForCausalLM` 2,162, `remote-code:SDARForCausalLM` 1,730 |
| lower | `ADAPTER_REFUSED` | 16,178 | 0.52 % | 0.01 % |  |
| lower | `TENSOR_MISSING` | 12,234 | 0.39 % | 0.10 % | `head.w` 4,334, `attn.q.w` 3,477, `embed.norm.gain` 2,638 |
| lower | `QUANT_DESCRIPTOR_MISSING` | 11,352 | 0.36 % | 16.30 % | `config/unknown` 6,954, `hqq` 866, `config/auto-round` 866 |
| admit | `COURT_BUDGET` | 11,065 | 0.35 % | 0.29 % | `IR tile multiply-accumulates` 8,017 |
| source | `WEIGHTS_INCOMPLETE` | 9,904 | 0.32 % | 0.07 % | `model-00001-of-00010.safetensors` 1,303, `model` 1,133, `model-00001-of-00002.safetensors` 869 |
| lower | `CONFIG_MISSING` | 6,285 | 0.20 % | 0.13 % | `unet` 19, `vae` 1, `tags` 1 |
| source | `REPO_UNREACHABLE` | 5,962 | 0.19 % | 0.00 % | `http_404` 5,962 |
| lower | `QUANT_REFUSED` | 4,800 | 0.15 % | 0.48 % | `CT_FP8_CHANNEL` 1,749, `GPTQ` 1,298, `AWQ` 865 |
| lower | `TASK_MISMATCH` | 1,005 | 0.03 % | 0.40 % | `feature-extraction→text-generation` 522, `sentence-similarity→text-generation` 483 |
| admit | `SHAPE_READY_AT_2048_ONLY` | 508 | 0.02 % | 0.00 % |  |
| source | `HEADER_INVALID` | 474 | 0.02 % | 0.00 % | `model-00002-of-00014.safetensors` 434, `aceinstruct-72b-q2_k.gguf` 10, ` DeepSeek-R1-Llama-8B.BF16.gguf` 10 |
| source | `FETCH_FAILED` | 435 | 0.01 % | 0.02 % | `layer23-shared.safetensors` 434, `(shards)` 1 |
| lower | `CONFIG_INVALID` | 434 | 0.01 % | 0.06 % |  |
| lower | `TENSOR_SHAPE` | 432 | 0.01 % | 0.00 % | `embed.table` 432 |
| lower | `NOT_RUN_NEEDS_WEIGHTS` | 66 | 0.00 % | 0.00 % |  |
| source | `HEADER_TOO_LARGE` | 1 | 0.00 % | 0.00 % | `model.safetensors.index.json` 1 |

Unique feature sets: 303 spec digests among the 961 sampled repositories that pass `lower`; among the judged shape-ready repositories, 277 spec digests and 818 distinct weight sets for 822 repositories.

### The local-LLM frame (§II.12)

| | Units |
| --- | ---: |
| Hugging Face, listed (text generation, chat with other inputs, untasked LLM-shaped GGUF) | 1,031,689 |
| Hugging Face, in `L_files` | 919,547 |
| … of which untasked GGUF (supplementary sample) | 143,607 |
| Ollama library units (text models; embedding models excluded: 21) | 735 |
| Ollama units in `L_files` (a manifest naming the weights) | 716 |
| **`L_listed`** / **`L_files`** | **1,032,424** / **920,263** |

TIR `Final`: **0 measured**. TIR shape-ready over `L_files`: **57.22 %** (one-sided 95 % LB 55.18 %) against the 90 % target — **not met**.

| Route | Gate | First TIR blocker | Units (est.) | Share of `L_files` | Leading arguments |
| --- | --- | --- | ---: | ---: | --- |
| TIR | admit | `SHAPE_READY` | 526,566 | 57.22 % |  |
| UNSUPPORTED | source | `BASE_UNPINNED` | 90,030 | 9.78 % | `base_gated` 52,218, `ambiguous` 25,208 |
| TIR | lower | `FORMAT_UNSUPPORTED` | 85,946 | 9.34 % | `pytorch` 47,352, `base:pytorch` 18,667 |
| UNSUPPORTED | lower | `PARTIAL_TASK_ONLY` | 77,634 | 8.44 % | `image-text-to-text` 64,278, `any-to-any` 11,574 |
| TIR | lower | `ARCH_REFUSED` | 46,481 | 5.05 % | `gguf-missing:llama.embedding_length` 6,080, `gguf-arch:gemma4` 4,731 |
| TIR | lower | `CONFIG_KEY_UNREAD` | 29,843 | 3.24 % | `rope_interleaved` 2,602, `n_special` 2,600 |
| TIR | lower | `TOKENIZER_MISSING` | 26,872 | 2.92 % |  |
| TIR | lower | `NOT_RUN_NEEDS_TENSOR_DATA` | 22,948 | 2.49 % | `rope_freqs.weight (256 bytes)` 19,275, `rope_freqs.weight (128 bytes)` 3,234 |
| TIR | lower | `CUSTOM_CODE_UNMODELLED` | 19,536 | 2.12 % | `remote-code:QuasarForCausalLM` 3,460, `remote-code:PhiForCausalLM` 2,162 |
| UNSUPPORTED | source | `GATED_ACCESS` | 18,451 | 2.00 % | `manual` 11,752, `auto` 6,679 |
| TIR | lower | `ADAPTER_REFUSED` | 16,178 | 1.76 % |  |
| TIR | lower | `QUANT_DESCRIPTOR_MISSING` | 12,239 | 1.33 % | `config/unknown` 6,939, `ggml/31` 957 |
| TIR | admit | `COURT_BUDGET` | 12,022 | 1.31 % | `IR tile multiply-accumulates` 8,974 |
| UNSUPPORTED | source | `WEIGHTS_INCOMPLETE` | 9,641 | 1.05 % | `model-00001-of-00010.safetensors` 1,303, `model` 926 |
| TIR | lower | `TENSOR_MISSING` | 9,548 | 1.04 % | `head.w` 4,334, `attn.q.w` 3,477 |
| GVM_FALLBACK | lower | `FEATURE_C` | 8,504 | 0.92 % | `an adapter for `Gemma4ForConditionalGeneration`` 2,978, `an adapter for `SmolVLMForConditionalGeneration`` 1,244 |
| UNSUPPORTED | source | `MISSING_WEIGHTS` | 7,980 | 0.87 % |  |
| TIR | lower | `QUANT_REFUSED` | 4,800 | 0.52 % | `CT_FP8_CHANNEL` 1,749, `GPTQ` 1,298 |
| UNSUPPORTED | lower | `MODALITY_PROFILE_MISSING` | 1,823 | 0.20 % | `image-to-text` 1,823 |
| TIR | admit | `NOT_RUN_PIPELINE_ADMISSION` | 873 | 0.09 % |  |
| TIR | admit | `ADMISSION_EXCEEDS` | 508 | 0.06 % | `max_position_macs` 508 |
| TIR | lower | `CONFIG_MISSING` | 505 | 0.05 % | `vae` 1 |
| UNSUPPORTED | source | `HEADER_INVALID` | 466 | 0.05 % | `model-00002-of-00014.safetensors` 434, `aceinstruct-72b-q2_k.gguf` 10 |
| UNSUPPORTED | source | `FETCH_FAILED` | 435 | 0.05 % | `layer23-shared.safetensors` 434, `(shards)` 1 |
| TIR | lower | `CONFIG_INVALID` | 434 | 0.05 % |  |
| TIR | lower | `TENSOR_SHAPE` | 432 | 0.05 % | `embed.table` 432 |
| TIR | lower | `NOT_RUN_NEEDS_WEIGHTS` | 32 | 0.00 % |  |

| Ollama units | |
| --- | ---: |
| UNSUPPORTED, PARTIAL_TASK_ONLY / NEEDS_JOB_PROFILE (vision chat: the image stage needs an RFC-0003 profile) | 169 |
| TIR, not judged (the weights' header is inside a registry blob; this census reads no blob) | 547 |
| UNSUPPORTED (no manifest naming the weights) | 19 |


## 3. What is left, largest first (local-LLM frame)

| Bucket | Share of `L_files` | Generic change | Kind |
| --- | ---: | --- | --- |
| `BASE_UNPINNED` (`base_gated` 5.7 %, `ambiguous` 2.7 %) | 9.8 % | gated base: policy (terms are never accepted); ambiguous: pin by the PEFT `base_model_name_or_path` when it names one of the card's bases | census / policy |
| `FORMAT_UNSUPPORTED` (`pytorch` 5.1 %, `base:pytorch` 2.0 %) | 9.3 % | a pickle-free reader for `.bin` headers is outside the network policy (headers allowed for safetensors/GGUF only) | **policy question** |
| `PARTIAL_TASK_ONLY` (vision chat) | 8.4 % | an RFC-0003 image-input profile for chat models | consensus profile, dormant fence |
| `ARCH_REFUSED` (GGUF architectures: gemma4, seed_oss, mistral3, phi2…; GGUF LoRA files) | 5.1 % | per-architecture GGUF → HF mappings, each verified on real tensors (needs weights to check numerically) | off-chain |
| `CONFIG_KEY_UNREAD` (`rope_interleaved`, `n_special`, `max_seq_len`) | 3.2 % | read each where it changes the math | off-chain |
| `TOKENIZER_MISSING` | 2.9 % | tokenizer from the pinned base (vocab size checked) | census + preflight |
| `NOT_RUN_NEEDS_TENSOR_DATA` (`rope_freqs`, 128–256 B) | 2.5 % | read ≤ 4 KiB of tensor data | **policy question (pending)** |
| `COURT_BUDGET` / `ADMISSION_EXCEEDS` (≥ 100 B) | 1.4 % | RFC-0006 layer×position cells | consensus, dormant fence |
| `FEATURE_C` → `GVM_FALLBACK` queue | 0.9 % | RFC-0005 §II.10.1 fallback class | NOT_RUN (no class exists) |
