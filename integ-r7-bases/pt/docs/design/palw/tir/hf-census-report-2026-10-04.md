# PALW-TIR — Hugging Face census report, 2026-10-04 (RFC-0002 §II.10 / §II.12, R6/A6, A8)

*Lane HF, branch `rfc2/hf-majority`. Method: `hf-census-v1.md`. Snapshot `2026-10-03T131904Z` (T0 = 2026-10-03 13:19:04 UTC),
sample seed `misaka-palw-hf-census/2026-10-03T131904Z/v1`, rows judged by `palw-class` at tree `577c61f` (headers) / `34f7f0721`
(the same lowering and admission code, plus the judgment budget and cache), testnet-12 parameters at DAA 5,585. No weight byte was
read. Every number below can be recomputed offline from the snapshot's manifest and rows.*

## 概要(日本語)

- **headline(strict, rights policy `none`)= 0**。rights policy が未採択で D_rights が空のため。さらに census は `pack`(重み)・`seat`/`final`(チェーン)を走らせないので、**registration-ready は 0 と測定**(推論で PASS にしない)。
- **technical funnel**: D_all 3,117,871 repo のうち source PASS 71.0 %(全数)、lower PASS 9.3 %(LB 8.9 %)、**shape-ready 2.42 %(LB 2.20 %)**。D_files 比で 3.41 %(LB 3.10 %)。過半数(A6)には遠い。
- **D_all を支配しているのは listing だけで決まる失敗**: TASK_UNKNOWN 28.7 %・MISSING_WEIGHTS 18.6 %・MODALITY_PROFILE_MISSING 13.1 %・BASE_UNPINNED 8.7 %・ADAPTER_UNCHECKED 6.0 %・FORMAT_UNSUPPORTED 3.4 %(計 ≈79 %)。これが A6 との本当の距離。
- **local-LLM frame(§II.12)の方が近期の指標として意味がある**: L_files 896,435 単位で TIR shape-ready 9.71 %(LB 8.92 %)、90 % 目標は未達。最大の TIR blocker は COURT_BUDGET 20.9 %・ADAPTER_UNCHECKED 19.0 %・CONFIG_KEY_UNREAD 11.3 %・ADMISSION_EXCEEDS 8.6 %。
- 2,048 → primary の含意を実地確認: 30 件中 29 件が primary でも同じ code で拒否、反例 0、budget 切れ 1。

## 1. Results

### Denominators

| | Repositories | Downloads (30 days) |
| --- | ---: | ---: |
| `D_all` | 3,117,871 | 2,885,554,744 |
| `D_files` (source PASS on the listing) | 2,213,693 (71.00 %) | 2,792,085,613 |
| `D_rights`, policy `none` (the headline's) | 0 | 0 |
| `D_rights`, policy `permissive-card-v0` (**proposal, not adopted**) | 585,176 (18.77 %) | 2,201,445,796 |

### The funnel (estimates with one-sided 95 % lower bounds)

| Stage | Strict (rights `none`) | Technical, of `D_all` | Technical, of `D_files` | Technical, of `D_rights` (proposal) |
| --- | ---: | ---: | ---: | ---: |
| `source` PASS | 0 | 71.00 % (exact) | 100 % | 100 % |
| `lower` PASS | 0 | 9.31 % (LB 8.93 %) | 13.12 % (LB 12.58 %) | 9.73 % (LB 8.47 %) |
| `admit` PASS at the primary context (**shape-ready**) | 0 | 2.42 % (LB 2.20 %) | 3.41 % (LB 3.10 %) | 2.38 % (LB 1.87 %) |
| admitted at 2,048 only (its own stratum) | 0 | 0.417 % | | |
| `pack`, `seat`, `final` | 0 | not run (weights / a chain) | | |
| **registration-ready** | **0** | **0 measured** (`pack` not run) | | |

Download-weighted (normal-approximation bounds): `lower` 9.25 % (LB 9.07 %), shape-ready 4.07 % (LB 2.43 %).

Sample: n = 1,773 (seed `misaka-palw-hf-census/2026-10-03T131904Z/v1`), phase-1 rows 1,773, nonresponse 0; eligible for the shape depth 684, judged prefix 427 (unjudged eligible estimated at 0 repositories, counted as not passing).

### Where repositories stop (technical view, estimated repositories; the first gate that fails or cannot be run)

| Gate | Code | Repositories | Share of `D_all` | Share of downloads | Leading arguments |
| --- | --- | ---: | ---: | ---: | --- |
| lower | `TASK_UNKNOWN` | 894,223 | 28.68 % | 9.77 % |  |
| source | `MISSING_WEIGHTS` | 579,477 | 18.59 % | 0.34 % |  |
| lower | `MODALITY_PROFILE_MISSING` | 407,207 | 13.06 % | 32.10 % | `text-classification` 120,116, `reinforcement-learning` 75,391, `automatic-speech-recognition` 35,411 |
| source | `BASE_UNPINNED` | 270,091 | 8.66 % | 0.12 % | `absent` 120,893, `base_gated` 105,230, `not_in_snapshot` 29,519 |
| lower | `ADAPTER_UNCHECKED` | 186,671 | 5.99 % | 0.17 % | `CAUSAL_LM` 167,193, `SEQ_2_SEQ_LM` 178, `FEATURE_EXTRACTION` 59 |
| admit | `COURT_BUDGET` | 121,531 | 3.90 % | 1.60 % | `IR tile multiply-accumulates` 61,416 |
| lower | `CONFIG_KEY_UNREAD` | 107,258 | 3.44 % | 2.62 % | `unsloth_fixed` 32,466, `unsloth_version` 21,212, `partial_rotary_factor` 6,103 |
| lower | `FORMAT_UNSUPPORTED` | 105,665 | 3.39 % | 3.55 % | `pytorch` 78,883, `onnx` 4,413, `pytorch+tensorflow` 3,090 |
| admit | `SHAPE_READY` | 75,588 | 2.42 % | 4.07 % |  |
| source | `GATED_ACCESS` | 53,420 | 1.71 % | 2.72 % | `manual` 34,213, `auto` 19,187, `VibeThinker-3B.Q8_0.gguf` 10 |
| lower | `PARTIAL_TASK_ONLY` | 52,091 | 1.67 % | 11.75 % | `image-text-to-text` 38,734, `any-to-any` 11,574, `visual-question-answering` 758 |
| admit | `ADMISSION_EXCEEDS` | 50,808 | 1.63 % | 2.55 % | `max_state_bytes` 33,655, `max_position_macs` 17,152 |
| lower | `ARCH_REFUSED` | 49,772 | 1.60 % | 13.62 % | `config-key:_num_labels` 7,183, `remote-code:QuasarForCausalLM` 3,460, `encdec-no-adapter:M2M100ForConditionalGeneration` 2,578 |
| lower | `FEATURE_C` | 44,262 | 1.42 % | 1.73 % | `GEN_UNET_SKIP_V1` 28,312, `an adapter for `NewModel`` 2,200, `an adapter for `Gemma4ForConditionalGeneration`` 1,733 |
| lower | `TOKENIZER_MISSING` | 28,551 | 0.92 % | 0.05 % |  |
| admit | `NOT_RUN_PIPELINE_ADMISSION` | 25,629 | 0.82 % | 1.67 % |  |
| admit | `SHAPE_READY_AT_2048_ONLY` | 12,998 | 0.42 % | 0.02 % |  |
| lower | `QUANT_DESCRIPTOR_MISSING` | 11,350 | 0.36 % | 15.46 % | `config/unknown` 6,954, `hqq` 866, `config/auto-round` 866 |
| source | `WEIGHTS_INCOMPLETE` | 9,904 | 0.32 % | 0.07 % | `model-00001-of-00010.safetensors` 1,303, `model` 1,133, `model-00001-of-00002.safetensors` 869 |
| lower | `TENSOR_MISSING` | 7,875 | 0.25 % | 0.10 % | `head.w` 3,470, `attn.q.w` 3,044, `embed.norm.gain` 879 |
| lower | `CONFIG_MISSING` | 6,285 | 0.20 % | 0.13 % | `unet` 19, `vae` 1, `tags` 1 |
| lower | `NOT_RUN_NEEDS_TENSOR_DATA` | 5,715 | 0.18 % | 0.07 % | `rope_freqs.weight (256 bytes)` 3,957, `rope_freqs.weight (128 bytes)` 1,319, `a tensor of the file` 440 |
| lower | `QUANT_REFUSED` | 4,800 | 0.15 % | 0.48 % | `CT_FP8_CHANNEL` 1,749, `GPTQ` 1,298, `AWQ` 865 |
| admit | `NOT_RUN_JUDGMENT_BUDGET` | 3,139 | 0.10 % | 0.01 % |  |
| lower | `TASK_MISMATCH` | 1,005 | 0.03 % | 0.40 % | `feature-extraction→text-generation` 522, `sentence-similarity→text-generation` 483 |
| admit | `CLOSE_TOO_LARGE` | 709 | 0.02 % | 0.00 % | `IR close sizing work` 709 |
| source | `HEADER_INVALID` | 474 | 0.02 % | 0.00 % | `model-00002-of-00014.safetensors` 434, `aceinstruct-72b-q2_k.gguf` 10, ` DeepSeek-R1-Llama-8B.BF16.gguf` 10 |
| source | `FETCH_FAILED` | 435 | 0.01 % | 0.02 % | `layer23-shared.safetensors` 434, `(shards)` 1 |
| lower | `CONFIG_INVALID` | 434 | 0.01 % | 0.06 % |  |
| lower | `TENSOR_SHAPE` | 432 | 0.01 % | 0.00 % | `embed.table` 432 |
| lower | `NOT_RUN_NEEDS_WEIGHTS` | 66 | 0.00 % | 0.00 % |  |
| lower | `CUSTOM_CODE_UNMODELLED` | 5 | 0.00 % | 0.00 % | `custom_pipeline` 5 |

Unique feature sets: 270 spec digests among the 780 sampled repositories that pass `lower`; among the judged shape-ready repositories, 59 spec digests and 122 distinct weight sets for 123 repositories.

### The local-LLM frame (§II.12)

| | Units |
| --- | ---: |
| Hugging Face, listed (text generation, chat with other inputs, untasked LLM-shaped GGUF) | 1,002,365 |
| Hugging Face, in `L_files` | 895,719 |
| … of which untasked GGUF (supplementary sample) | 143,607 |
| Ollama library units (text models; embedding models excluded: 21) | 735 |
| Ollama units in `L_files` (a manifest naming the weights) | 716 |
| **`L_listed`** / **`L_files`** | **1,003,100** / **896,435** |

TIR `Final`: **0 measured**. TIR shape-ready over `L_files`: **9.71 %** (one-sided 95 % LB 8.92 %) against the 90 % target — **not met**.

| Route | Gate | First TIR blocker | Units (est.) | Share of `L_files` | Leading arguments |
| --- | --- | --- | ---: | ---: | --- |
| TIR | admit | `COURT_BUDGET` | 187,185 | 20.88 % | `IR tile multiply-accumulates` 97,797 |
| TIR | lower | `ADAPTER_UNCHECKED` | 170,501 | 19.02 % | `CAUSAL_LM` 167,050, `SEQ_2_SEQ_LM` 69 |
| TIR | lower | `CONFIG_KEY_UNREAD` | 100,981 | 11.26 % | `unsloth_fixed` 32,466, `unsloth_version` 21,212 |
| TIR | admit | `SHAPE_READY` | 87,076 | 9.71 % |  |
| UNSUPPORTED | source | `BASE_UNPINNED` | 83,614 | 9.33 % | `base_gated` 49,179, `absent` 12,933 |
| TIR | admit | `ADMISSION_EXCEEDS` | 76,657 | 8.55 % | `max_state_bytes` 51,846, `max_position_macs` 24,811 |
| TIR | lower | `ARCH_REFUSED` | 63,401 | 7.07 % | `gguf-arch:gemma4` 4,731, `remote-code:QuasarForCausalLM` 3,460 |
| TIR | lower | `FORMAT_UNSUPPORTED` | 62,302 | 6.95 % | `pytorch` 47,352, `gguf-split` 4,630 |
| TIR | lower | `PARTIAL_TASK_ONLY` | 52,091 | 5.81 % | `image-text-to-text` 38,734, `any-to-any` 11,574 |
| TIR | lower | `TOKENIZER_MISSING` | 23,390 | 2.61 % |  |
| TIR | lower | `NOT_RUN_NEEDS_TENSOR_DATA` | 22,948 | 2.56 % | `rope_freqs.weight (256 bytes)` 19,275, `rope_freqs.weight (128 bytes)` 3,234 |
| UNSUPPORTED | source | `GATED_ACCESS` | 14,353 | 1.60 % | `manual` 8,588, `auto` 5,745 |
| TIR | lower | `QUANT_DESCRIPTOR_MISSING` | 12,239 | 1.37 % | `config/unknown` 6,939, `ggml/31` 957 |
| UNSUPPORTED | source | `WEIGHTS_INCOMPLETE` | 9,543 | 1.06 % | `model-00001-of-00010.safetensors` 1,303, `model-00001-of-00002.safetensors` 869 |
| UNSUPPORTED | source | `MISSING_WEIGHTS` | 7,843 | 0.87 % |  |
| TIR | lower | `TENSOR_MISSING` | 6,947 | 0.77 % | `head.w` 3,469, `attn.q.w` 3,044 |
| TIR | admit | `NOT_RUN_JUDGMENT_BUDGET` | 6,011 | 0.67 % |  |
| TIR | lower | `QUANT_REFUSED` | 4,800 | 0.54 % | `CT_FP8_CHANNEL` 1,749, `GPTQ` 1,298 |
| GVM_FALLBACK | lower | `FEATURE_C` | 4,770 | 0.53 % | `an adapter for `Gemma4ForConditionalGeneration`` 1,733, `an adapter for `PicoDecoderHF`` 867 |
| UNSUPPORTED | lower | `MODALITY_PROFILE_MISSING` | 1,823 | 0.20 % | `image-to-text` 1,823 |
| TIR | admit | `NOT_RUN_PIPELINE_ADMISSION` | 873 | 0.10 % |  |
| TIR | admit | `CLOSE_TOO_LARGE` | 709 | 0.08 % | `IR close sizing work` 709 |
| TIR | lower | `CONFIG_MISSING` | 505 | 0.06 % | `vae` 1 |
| UNSUPPORTED | source | `HEADER_INVALID` | 466 | 0.05 % | `model-00002-of-00014.safetensors` 434, `aceinstruct-72b-q2_k.gguf` 10 |
| UNSUPPORTED | source | `FETCH_FAILED` | 435 | 0.05 % | `layer23-shared.safetensors` 434, `(shards)` 1 |
| TIR | lower | `CONFIG_INVALID` | 434 | 0.05 % |  |
| TIR | lower | `TENSOR_SHAPE` | 432 | 0.05 % | `embed.table` 432 |
| TIR | lower | `NOT_RUN_NEEDS_WEIGHTS` | 32 | 0.00 % |  |

| Ollama units | |
| --- | ---: |
| TIR, PARTIAL_TASK_ONLY (vision chat: the image stage is not computed by the class the preflight produces) | 169 |
| TIR, not judged (the weights' header is inside a registry blob; this census reads no blob) | 547 |
| UNSUPPORTED (no manifest naming the weights) | 19 |


### The 2,048 → primary implication, checked (the lead's guards)

Guard 1 — the argument per code (also in `census::gates::IMPLIED_AT_WIDER_CONTEXT`): `COURT_BUDGET` (per-tile MACs and cone work
depend on the tiles; the layout search offers the same power-of-two tiles at every context at least the tile; the dissection's root
claim and rounds only grow with the history), `COURT_WINDOW_EXCEEDED` (duration non-decreasing in the context), `CLOSE_TOO_LARGE`
(a carried close spans a tile and the history it reads), `DA_LADDER_EXCEEDED` (step leaves proportional to positions),
`FENCE_NOT_ARMED`/`ARTIFACT_ROOT_KNOWN` (context-independent). `CONTEXT_BOUND` is not implied: its primary is run.

Guard 2 — checked for real (`verify_implied.py`, seed `…/v1/verify-implied`, the cheap rows ≤ 4 B parameters, 36 eligible):
**30 judged at their primary context: 29 refused there too (29 with the same blocking code), 0 admitted, 1 inconclusive** (the
layout search spent its 600 s budget; not counted as agreement). Rows: `report/verify_implied.json`.

## 2. Reading the numbers

- **Strict = 0 is the honest headline.** It moves only with a rights policy decision (`permissive-card-v0` would make
  `D_rights` 585,176 repositories, 18.8 %) and with real `pack`/`Final` runs. Registration-ready is never inferred from shape-ready.
- **The listing-level failures are most of `D_all`.** About 79 % of the Hub stops before any header is read: no declared task,
  no weights, a task with no canonical job (classification, ASR, RL…), an adapter whose base is not pinned, a non-safetensors/GGUF
  format. None of these is a lowering problem; most are not "LLMs a node could run" at all. That is why §II.12's frame is the
  more meaningful near-term measure.
- **Shape-ready at 2,048 only** (0.42 % of `D_all`) is its own stratum and is not in the 8k headline.
- **Mirrors**: collapsed by digest only where digests were read (the sample: 122 weight sets for 123 shape-ready repositories), so
  population counts are an upper bound on distinct checkpoints.
- **Phase 2**: 427 of the 684 eligible sampled repositories judged (the random-order prefix); the shape pass is paused (disk, §4)
  and resumes from its place; later reports tighten the bounds without changing the method.

## 3. The largest measured blockers (H5 queue, generic changes only)

| Bucket (frame share) | Generic change | Status |
| --- | --- | --- |
| `COURT_BUDGET` 20.9 % / `ADMISSION_EXCEEDS` 8.6 % | real-size admission: court work and state caps (consensus parameters) | needs a decision: only a dormant fence can move a cap; measure first which ceiling binds per size band |
| `ADAPTER_UNCHECKED` 19.0 % | compose a PEFT adapter with its pinned base (RFC-0004 attach) | implemented `3b6cc1164`; cohort (n = 150) fetched, before/after pending |
| `CONFIG_KEY_UNREAD` 11.3 % | training-tool bookkeeping keys inert (`unsloth_*`, `_num_labels`, …) | implemented `e9c39983b`; cohort re-run pending |
| `BASE_UNPINNED` 9.3 % | follow renamed bases (Hub redirect) | `resolve_renames.py` running (metadata only) |
| `ARCH_REFUSED` 7.1 % | GGUF arch mappings (gemma4, qwen2vl/qwen3vl, mistral3, phi2, …); `trust_remote_code` → `CUSTOM_CODE_UNMODELLED` | classification fix `956302828`; mappings not started |
| `NOT_RUN_NEEDS_TENSOR_DATA` 2.6 % | read `rope_freqs.weight` (128–256 bytes) | **policy question** (§4) |

## 4. Decisions asked of the lead / user

1. **Rights policy**: adopt `permissive-card-v0` (or another) for `D_rights`, or keep strict = 0.
2. **≤ 4 KiB tensor-data reads** for GGUF `rope_freqs.weight` (2.6 % of the frame waits on 128–256 bytes of data; the current
   policy forbids any data-section byte).
3. **Ollama blob headers**: 547 of 716 Ollama units are not judged because their GGUF header sits inside a registry blob; a
   bounded range read of a blob's header would be a "pull" under the current rule.
4. **Disk**: free space fell to 11–14 GB at 08:30 (a release build elsewhere); the census was stopped per the rule (< 15 GB) and
   its own target (2.9 GB, rebuildable) deleted. Census store 2.6 GB.
