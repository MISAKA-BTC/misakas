# Frontend coverage record (lane A)

Owner: lane A — model frontend and task coverage. Branch `onboard/a-frontend`, base `febc07f24`, 19 commits (`model-onboarding:` /
`frontend:`), no consensus, params, fingerprint, IR-primitive or kernel change. Companion of
[`model-onboarding-implementation-matrix.md`](model-onboarding-implementation-matrix.md) (its §4 "Blocker families" takes its "after" column from §2 here).

Rules this record keeps: every number is measured on the offline census sample or says it is not; **unknown = GAP**; "header-only,
tensor bytes absent" is never "semantically unsupported"; nothing here reads a model name for a decision (a family is an adapter file
or a descriptor, never a branch on `model_type`); no sentence says that all Hugging Face models are supported.

## 0. How to read the numbers

* **Population and unit.** The snapshot `2026-10-03T131904Z` (`D_all` = 3,117,871 listed repositories). Counts are *estimated repositories
  of `D_all`*: the exact listing classification where the listing decides, and the two-phase sample (1,773 repositories; 850 → 897
  judged at shape depth) plus four cohorts of 150 (`task_unknown`, `base_unpinned`, `adapters`, `partial`) otherwise. The standard error of
  the shape-ready share of `D_all` is about 0.85 pp; a cohort row stands for 347–5,961 repositories, so a delta below a few thousand is a
  single row.
* **Before** is the binary built from `febc07f24` run on the same snapshot in this lane (not the matrix's "census update 1", which came from
  another build; the two differ by 0–17 % per bucket, shown in the first column of §2). **After** is the binary built from this branch at
  `138516d31` (the commit that adds this file changes no code). Both ran `palw-class census classify`, `gates --depth headers`,
  `gates --depth shape`, then `report.py` / `report_local.py` of `misaka-palw-sdk/tools/hf_census`, offline, 2–4 threads.
* **No network.** The census fetched `config.json`, safetensors / GGUF headers and index files. Anything the fetch never read cannot be
  measured: see §6.
* **Re-attribution.** When a repository stops later than before, the later bucket grows (`PARTIAL_TASK_ONLY`, `MODALITY_PROFILE_MISSING`,
  `COURT_BUDGET`, `ADAPTER_REFUSED`, `NOT_RUN_PIPELINE_ADMISSION` all went *up* because repositories that used to stop earlier now reach
  them). That is progress, not regression; the "flows" table in §2 is the attribution.

## 1. Task 1 — machine-readable classes

`misaka-palw-sdk/src/census/onboarding.rs` (`GapClassV1`, `classify_census_code_v1`, `classify_blocker_v1`); every census gate result
carries `class` and `class_reason`; the preflight's blockers map through the same table.

| class | meaning | examples (census code → class) |
|---|---|---|
| `FRONTEND_REQUIRED` | the importer / lowerer / adapter data cannot read or lower what the IR could express | `TENSOR_MISSING`, `TENSOR_SHAPE`, `CONFIG_KEY_UNREAD`, `TOKENIZER_MISSING`, `ARCH_REFUSED`, `ADAPTER_REFUSED`, `QUANT_DESCRIPTOR_MISSING`, `FORMAT_UNSUPPORTED` (no reader), `TASK_UNKNOWN` when a configuration names a class |
| `KERNEL_EXTENSION_REQUIRED` | a semantic primitive / relation / court is missing from every shipped kernel | `FEATURE_C` whose registry feature has a `Capability` protocol requirement; `ARCH_NEEDS_PRIMITIVE`; a missing job profile is recorded **as this class with the reason text `task profile`** (`TASK_PROFILE_REASON`) |
| `PROFILE_REQUIRED` | the declared task has no canonical job profile (counted separately; the lifecycle records it as the line above) | `MODALITY_PROFILE_MISSING`, `PARTIAL_TASK_ONLY` |
| `KERNEL_NOT_ACTIVE` | the kernel exists, the fence that activates it is not in force | `FENCE_NOT_ARMED`, shipped `KERNEL_NOT_ACTIVE` outcome |
| `LAYOUT_REQUIRED` | no exact layout is pinned | `PACK_NOT_VERIFIED`, "no layout can be derived" |
| `RESOURCE_REFUSED` | a mandatory bound is over its ceiling (never answered by raising a cap) | `COURT_BUDGET`, `CLOSE_TOO_LARGE`, `CONTEXT_BOUND`, `SEAT_MEMORY`, `HEADER_TOO_LARGE` |
| `EXTERNAL_BLOCKER` | the source material is missing, gated, unpinned or malformed | `MISSING_WEIGHTS`, `BASE_UNPINNED`, `GATED_ACCESS`, `CONFIG_MISSING`, `TASK_UNKNOWN` **with `arg = no-config`** |
| `NOT_RUN` | a depth or bytes the run did not read — **never a verdict, never a gap** | every `NOT_RUN_*` (`NOT_RUN_NEEDS_TENSOR_DATA`, `NOT_RUN_NEEDS_PICKLE_DIRECTORY`, …) |
| `UNMAPPED` | a code the table does not know; never a pass | a test keeps every published code out of it |

* The header-only rule is a test: `a_gate_that_was_not_run_is_never_a_gap_of_any_kind` (every `NOT_RUN_*` is `NOT_RUN` for every gate), and
  `GapClassV1::is_semantic_gap()` is `true` only for `KERNEL_EXTENSION_REQUIRED` and `PROFILE_REQUIRED`. A `TENSOR_MISSING` is the binder's.
* A feature is classified by the registry's protocol requirement, never by its name
  (`a_feature_is_classified_by_the_registrys_protocol_requirement_and_not_by_its_name`): this build's registry has 193 features, 20
  `Missing`, 2 `Specified`, **none with a `Capability` requirement** — every lowering gap the census meets is the frontend's, not the kernel's.
* The tokens equal the lifecycle's and the kernel's (`the_tokens_are_the_lifecycles_and_the_kernels`, with `misaka-palw-challenge` as a
  dev-dependency).
* **A correction made while measuring** (`fbfcba2ff`): 92 % of the `TASK_UNKNOWN` repositories (625,173 of 679,333, exact listing count) carry no
  configuration naming a model class. They were classed `FRONTEND_REQUIRED`, which said a reader was missing. They now carry `arg = no-config`
  and `EXTERNAL_BLOCKER`; the 54,160 that do name a class stay the frontend's.

Listing-level classes over all of `D_all` (the first failing technical gate, exact, final build): `EXTERNAL_BLOCKER` 1,502,389 (48.2 %),
`NOT_RUN` 968,600 (31.1 %; 889,254 not sampled, 79,346 pickle directory), `PROFILE_REQUIRED` 536,365 (17.2 %), `FRONTEND_REQUIRED`
110,517 (3.5 %: `TASK_UNKNOWN` with a class 54,160, `FORMAT_UNSUPPORTED` 39,666, `ADAPTER_UNCHECKED` 16,686). The sampled shape pass decides
the not-sampled ones.

## 2. Task 2 — blocker families, before / after

Estimated repositories of `D_all`. Headline (shape depth, static admission only; **0 `Final`-measured**, registration-ready 0):

* `shape_ready`: 560,426 (17.97 %, lb95 16.59 %) → **645,455 (20.70 %, lb95 19.09 %)**, +85,029.
  `lower` pass: 615,669 (19.75 %) → 713,544 (22.89 %, lb95 21.23 %).
* Download-weighted shape-ready: 9.01 % → 9.26 % (the gains are in long-tail repositories).
* Local-LLM frame (`report_local.py`, `L_files` = 934,271 units): shape-ready **57.60 % (lb95 55.49 %) → 66.63 % (lb95 63.06 %)** against the
  90 % target; lower pass 59.2 % → 68.9 %. 66,375 units (7.1 % of the frame) are *unmeasured pickle checkpoints*, counted as not passing (§6).
* Unique shape-ready spec digests 284 → 308 (distinct programs), weight sets 831 → 877; unique lower-pass spec digests 312 → 342.

| gate | code | matrix "before" (update 1) | before (this lane's baseline run) | after | delta |
|---|---|---:|---:|---:|---:|
| lower | `TASK_UNKNOWN` | 661,725 | 672,529 | 596,149 | -76,380 |
| lower | `MODALITY_PROFILE_MISSING` | 460,860 | 460,860 | 466,822 | +5,962 (re-attribution) |
| lower | `FORMAT_UNSUPPORTED` | 130,554 | 132,355 | 34,992 | -97,363 (**relabel**, see below) |
| lower | `NOT_RUN_NEEDS_PICKLE_DIRECTORY` | — | 0 | 19,223 | +19,223 |
| listing | `UNDECIDED_UNMEASURED` (was FAIL `FORMAT_UNSUPPORTED`, outside every cohort) | — | 0 | 78,140 | +78,140 |
| lower | `PARTIAL_TASK_ONLY` | 75,937 | 73,853 | 85,776 | +11,923 (re-attribution) |
| lower | `ARCH_REFUSED` | 37,877 | 39,607 | 35,210 | -4,397 |
| lower | `TOKENIZER_MISSING` | 32,470 | 32,517 | 29,174 | -3,343 |
| lower | `CONFIG_KEY_UNREAD` | 31,715 | 26,944 | 15,204 | -11,740 |
| lower | `NOT_RUN_NEEDS_TENSOR_DATA` | 23,600 | 23,600 | 440 | -23,160 |
| lower | `CUSTOM_CODE_UNMODELLED` | 19,558 | 19,558 | 19,558 | 0 |
| lower | `FEATURE_C` | 47,995 | 49,796 | 49,795 | -1 |
| lower | `ADAPTER_REFUSED` | — | 17,979 | 25,977 | +7,998 (re-attribution) |
| lower | `ADAPTER_UNCHECKED` | — | 24,757 | 24,757 | 0 |
| lower | `TENSOR_MISSING` | — | 12,234 | 5,696 | -6,538 |
| lower | `TENSOR_SHAPE` | — | 2,233 | 4,034 | +1,801 (re-attribution) |
| lower | `QUANT_DESCRIPTOR_MISSING` / `QUANT_REFUSED` | — | 11,352 / 4,800 | 11,354 / 4,800 | 0 |
| lower | `TASK_MISMATCH` (decoder tagged as an embedder) | — | 1,005 | 1,005 | 0 |
| admit | `NOT_RUN_PIPELINE_ADMISSION` | 41,417 | 41,869 | 48,753 | +6,884 (progress: embedding classes reach it) |
| admit | `COURT_BUDGET` | ≈11,000 | 12,866 | 18,828 | +5,962 (re-attribution) |
| source | `MISSING_WEIGHTS` / `BASE_UNPINNED` / `GATED_ACCESS` | — | 579,477 / 237,680 / 53,420 | same | 0 (external) |
| admit | `SHAPE_READY` | — | 560,426 | 645,455 | +85,029 |

**Flows** — the same rows, before stop → after stop (main sample first, then the cohorts); this is the attribution of the table:

| flow | est. repos | fix |
|---|---:|---|
| `CONFIG_KEY_UNREAD` → shape-ready 8,664, → `TOKENIZER_MISSING` 1,731, → pipeline admission 1,345 | 11,740 | F6 |
| `NOT_RUN_NEEDS_TENSOR_DATA` → shape-ready (5,276 main + 17,884 `task_unknown` cohort) | 23,160 | F1 |
| `TASK_UNKNOWN` → shape-ready (41,730 `task_unknown` + 5,402 `base_unpinned` cohort) | 47,132 | F3 |
| `TASK_UNKNOWN` → a later stop (`PARTIAL_TASK_ONLY` 11,923, profile 5,961, `COURT_BUDGET` 5,961, three more rows of 1,801) | 29,248 | F3 (not progress by itself) |
| `TOKENIZER_MISSING` → shape-ready 2,173 / pipeline admission 2,901 | 5,074 | F2 |
| `TENSOR_MISSING head.w` → shape-ready (9 of the 11 sampled rows) | 3,900 | F7 |
| `TENSOR_MISSING embed.norm.gain` → pipeline admission (5 of 5 rows) | 2,638 | F10 |
| `FORMAT_UNSUPPORTED` → `NOT_RUN_NEEDS_PICKLE_DIRECTORY` 19,224 (cohorts) / `UNDECIDED_UNMEASURED` 78,140 | 97,363 | F5 (**relabel, not a pass**) |
| `ARCH_REFUSED` → `ADAPTER_REFUSED` (a GGUF LoRA is an adapter) | 6,197 | F4 (diagnosis) |

## 3. Families — source, task, frontend status, first blocker, fix, tests

| # | family | source | task | frontend status before → after | first blocker before → after | commit(s) | tests |
|---|---|---|---|---|---|---|---|
| F0 | classification of every refusal | census, preflight | — | none → every gate carries a class | — | `a5012c74d`, `d437c5b51` (re-pins the 16 adapters whose identity had moved), `544634d96` (`task profile`), `fbfcba2ff` | `census::onboarding::tests` (7), `census::gates::tests::an_undeclared_task_says_…`, `adapter_pins` |
| F1 | GGUF `rope_freqs.weight` (llama.cpp's Llama-3 rope scaling) | GGUF | text-generation | read only when its bytes matched 3 known Llama-3 parameter sets, and **needed the bytes** → read as what it is, a table of frequency divisors (`rope_type = freq_factors`); header-only reads a table of ones and records `pending_tensor_data`; the program is byte-identical for every table | `NOT_RUN_NEEDS_TENSOR_DATA` 23,600 → 440 (residual: one sampled row, another small tensor) | `592845b08` | `tests/gguf_rope_freqs.rs` (4: header-only maps; an arbitrary table lowers; the program is independent of the values; a wrong length is refused by name), `rope::freq_factors_…` |
| F2 | tokenizer files | all | all | → a SentencePiece, WordPiece, tekken or tiktoken file counts as a tokenizer (`TOKENIZER_FILES_V1`) | `TOKENIZER_MISSING` 32,517 → 29,174 (residual: the repository carries none) | `a70bf195a` | `preflight::a_sentencepiece_wordpiece_or_tiktoken_file_is_a_tokenizer_and_nothing_else_is` |
| F3 | PEFT task and base task | adapters | all | an adapter declared a task only by `pipeline_tag` → every PEFT `task_type` is a task; an adapter that declares none carries its pinned base's head class (`inference-v3`) | `TASK_UNKNOWN` 672,529 → 596,149; 47,132 repositories reach shape-ready (the `task_unknown` and `base_unpinned` cohorts, adapters) | `aecdece3c` | `census::tasks`, `a_peft_adapter_declares_its_task_by_the_head_it_was_trained_with`, `an_adapter_with_no_declared_task_carries_the_task_of_its_pinned_bases_head_class` |
| F4 | GGUF LoRA | GGUF | — | named an unmapped architecture → named an adapter (`ADAPTER_REFUSED`: composing with a base is the open gap) | `ARCH_REFUSED` → `ADAPTER_REFUSED` (6,197) | `07787edd8` | `preflight::a_gguf_lora_is_named_as_an_adapter_and_not_as_an_architecture_the_frontend_failed_to_map` |
| F5 | PyTorch `pytorch_model.bin` (+ shards, ZIP64) | pickle | all | `FORMAT_UNSUPPORTED` → **read without running anything** (§4): preflight, conversion to the same artifact digest as the safetensors original | `FORMAT_UNSUPPORTED` 132,355 → 34,992 **by relabel**; 97,363 repositories are `NOT_RUN`, unmeasured (§6) | `c8b94df9f`, `0f39bd86a`, `f1085b369`, census `511580925` | `tests/torch_bin.rs` (18), `preflight` (2), `census::gates::a_pytorch_checkpoint_is_not_run_never_an_unsupported_format` |
| F6 | configuration keys | HF `config.json` | all | a key the adapter did not list was refused → a *transformers* class ignores a key it never reads (table `hf-config-keys-v1.json`, derived from transformers 5.17); refused again when the checkpoint carries tensors the class lacks, when the key is a storage marker, or when the class is custom | `CONFIG_KEY_UNREAD` 26,944 → 15,204 | `5384cbe3e` | `tests/hf_keys_ignored.rs` (4), `preflight::a_key_ignored_for_a_transformers_class_is_refused_again_when_…` |
| F7 | bitsandbytes output embedding | HF + bnb | text-generation | the head was assumed quantised unless a skip list named it → bitsandbytes' default skip keeps it in float; only an explicit list replaces that default (`lm_head = when_skip_given`) | `TENSOR_MISSING head.w` 4,334 → 434 | `de0a15ea6` | `quant_tensors` (the bnb head, 5 cases), `quantized` (34), `quant_decode_pins` |
| F8 | sequence classifiers, cross-encoder rerankers, reward models | HF | text-classification / text-ranking / reward | no head → `OUTPUT_CLASSIFY_V1`; 13 decoder families (Llama, Qwen2/3, Mistral, Gemma/2, Phi-3, Mixtral, Qwen3-MoE, OLMo-2, GPT-2, OPT) and BERT / RoBERTa / XLM-R / DistilBERT | `MODALITY_PROFILE_MISSING` stays (the profile row is the Lead's); the architecture is no longer the blocker | `60da06401`, `602c570fc`, `dc762a544` | `tests/seqcls.rs` (4: HF logits to 1e-5, integer within bound, three implementations + court, 18 fixtures), `sdk/tests/gen_classifier_class.rs` (2) |
| F9 | undeclared task with no configuration | census | — | `FRONTEND_REQUIRED` (wrong) → `EXTERNAL_BLOCKER` | `TASK_UNKNOWN` 679,333 → 54,160 frontend + 625,173 external (exact listing count) | `fbfcba2ff` | `census::onboarding`, `census::gates` |
| F10 | LayerNorm saved as `gamma` / `beta` | HF + TF-era exports | all BERT-lineage | the binder asked for `LayerNorm.weight` only → accepts the old spelling of a LayerNorm leaf, as transformers' `legacy` conversion does | `TENSOR_MISSING embed.norm.gain` 2,638 → 0 (5 of 5 sampled rows reach the lower gate) | `138516d31` | `tests/legacy_layernorm_names.rs` (2: a real fixture rewritten with the old names reads every tensor and computes the same bytes; the rule renames a LayerNorm leaf and nothing else) |

### Limits of the numbers above

* F1, F2, F3, F6, F7, F10 are **measured** end to end (rows moved to shape-ready or to the next gate). F4 and F9 are diagnoses. F5 and F8 have
  **no census effect**: F5 because the census fetched no pickle directory (the repositories are relabelled `NOT_RUN`, never counted as covered);
  F8 because `text-classification` and `text-ranking` keep `Profile::None` (the Lead's decision), so those repositories stop at the listing before any header is read.
* Each estimate is a sum of row weights (347–5,961 repositories per row): treat sub-family numbers as ± one row.

## 4. The pickle reader (F5), as the Lead specified

`misaka-palw-tir-lower/src/weights/torchzip.rs`. **Nothing is imported, called or executed.** A `pytorch_model.bin` is a ZIP of `data.pkl` and
raw storages: the reader parses the central directory (ZIP and ZIP64), runs a **symbolic** pickle interpreter over `data.pkl`, and returns
absolute-offset tensor entries (the shape a `SafetensorsFile` entry has); storages are read later by range.

* Allowlist of globals: `collections.OrderedDict`; `torch._utils._rebuild_tensor_v2`, `_rebuild_tensor`, `_rebuild_parameter`; the storage types
  `torch.{Float,Double,Half,BFloat16,Long,Int,Short,Char,Byte,Bool}Storage`. `REDUCE` is evaluated by this module for exactly those callables and
  refused for everything else; a `GLOBAL` / `STACK_GLOBAL` of anything else is refused *when it is read*, before it could be called. `BUILD` accepts only the
  state dict's `_metadata` (a mapping of strings, dropped). Every other opcode that constructs an object (`INST`, `OBJ`, `NEWOBJ`, `EXT*`, `PERSID`,
  buffers, sets) is refused. Every refusal is `FORMAT_UNSUPPORTED` with the opcode, global or form named.
* Hard bounds (`Limits`): pickle bytes, directory bytes, members, ops, stack, memo, container nesting, bytes allocated, string length, tensors, rank.
  A length prefix longer than the bytes left is refused *before* anything is allocated. A container becomes immutable the moment it is a member of
  another, so a pickle cannot build a cycle or a nesting deeper than the bound.
* Refused by name: the legacy (pre-1.6, non-zip) format, a multi-disk, encrypted or compressed archive, two members of one name, a local header that
  disagrees with the directory about the name, a member that runs past the end of the file, a tensor whose size and stride differ in rank, a negative
  extent, a non-empty backward-hook mapping, a persistent id that is not `(storage, type, key, location, numel)`, a strided (non-contiguous) view.
* Tests (`tests/torch_bin.rs`, 18): hostile pickles — unknown global, `REDUCE` on a non-allowlisted callable, `NEWOBJ` / `INST` / `EXT` / buffer opcodes,
  a length prefix longer than the bytes left, deep nesting, a huge memo, a value made a member of itself; archives that lie about their size, offsets
  or names; ZIP64; `mutated_pickles_and_archives_never_panic` (every byte of the fixtures mutated); the header read without the tensor data; tied
  weights and storage offsets; conversion to **the same artifact digest** as the safetensors original.

## 5. Task 3 — task-coverage matrix (what is covered end to end)

Fixtures are tiny seeded transformers 5.17 models (`tools/gen_hf_*_fixtures.py`, offline). "Covered" = float reference against transformers' own
output **and** the integer program against the float reference, **and** static admission. Inventory first: only the classifier / reranker / reward row was missing and was added.

| head / family | fixtures | covered | not covered — "backbone only" is not full-task support |
|---|---|---|---|
| dense decoder LM (Llama, Qwen2/3, Mistral, Gemma 1–4, Phi / Phi-3, OLMo 1–3, GPT-2 / Neo / J / NeoX, OPT, Bloom, Falcon, StarCoder2, Cohere 1–2, GLM, Granite, SmolLM3, Exaone4, …) | `tests/fixtures/hf/*` (99) | next-token logits vs HF, every checkpoint tensor consumed, three implementations, court; greedy / sampled text pipeline | — |
| GQA / MQA / MLA / sliding window / QK-norm / soft caps | llama, qwen2/3, mistral, gemma2/3, falcon_mq, deepseek_v2/v3, … | same | — |
| MoE (Mixtral, Qwen2/3-MoE, OLMoE, GraniteMoE, DBRX, gpt-oss, PhiMoE, JetMoE, Llama-4, ERNIE-4.5-MoE, GLM-4-MoE, LongCat, DeepSeek) | `hf/`, `fr01/`, `hf-quant/*_qwen3moe_*` | routed and shared experts, router scale; real-shape configurations admitted | — |
| recurrent, gated delta rule (Qwen3-Next, Qwen3.5, Kimi-Linear KDA) | `qwen3_next`, `qwen3_5`, `kimi_linear`, `qwen4_gdn_*` | float vs HF, integer vs float (`fidelity_tiny::recurrent!`), history windows | — |
| SSM: Mamba 1 / 2, Falcon-Mamba | `mamba`, `mamba2`, `falcon_mamba` | same | — |
| hybrids (Jamba, Zamba2, Nemotron-H, Falcon-H1, LFM2, GraniteMoE-Hybrid) | `jamba`, `zamba2`, `nemotron_h*`, `falcon_h1*`, `lfm2`, `fr01/granitemoehybrid` | same | — |
| RWKV | `rwkv` | RWKV-4 (`RwkvForCausalLM`, adapter `rwkv4`) | RWKV-5 / 6 / 7 are not modelled (GAP) |
| embedding, encoders | `hf-enc/{bert, roberta, xlm_roberta, distilbert, mpnet, clip_text}` | mean / CLS pooling, L2-normalise, `EmbeddingI32`; the full registration → job → replay → court path in `gen_embedding_e2e.rs`; real configurations at 128–512 tokens | sentence-transformers `Dense` layers, max / weighted pooling (refused by name) |
| embedding, decoder (last token) | `hf-enc/qwen3_embed` | lowering and admission (`as_last_token_embedder`) | **not wired into the census / preflight**: it needs `modules.json` and `1_Pooling/config.json` (not fetched). 1,005 repositories are `TASK_MISMATCH`, plus embedders without an `lm_head` |
| **classifier / reranker / reward (new)** | `hf-cls/*` (18) | logits vs HF to 1e-5; integer within bound; three implementations + court (encoders); `tir_admit_pipeline_v1`; **declared as an Embedding-profile class and admitted by the registration gate** (`gen_classifier_class.rs`); kernel route K2-TIR-v3 `KERNEL_NOT_ACTIVE` shipped / `ELIGIBLE_AT` armed | the **job profile**: an `EmbeddingI32 [1, labels]` row of unnormalised logits carries it today with no label map; a dedicated profile is the Lead's decision. Token classification, QA spans, fill-mask (MLM head), multiple choice: no head (GAP) |
| encoder-decoder text (T5 relu / gated, BART, mBART, Marian, Pegasus, LongT5) | `hf-encdec/*` | two-stage pipeline vs HF `generate` (greedy), teacher-forced KL, replay, admission | M2M100 / NLLB (no adapter: 2,578 repositories), SpeechT5, Parler |
| ASR (Whisper) | `hf-encdec/whisper` | encoder and decoder each vs HF, three implementations, admission of each stage | **no job input binding for audio** (`JobAudio`, FR-23): no pipeline; TTS and audio classification: GAP |
| VLM chat | `hf-vis/{llava, qwen2_vl, qwen2_5_vl, qwen3_5}` | tower + text stage as one pipeline vs HF greedy `generate` (`vision.rs`) | gemma3 / paligemma / mllama / llama4 / mistral3 / qwen3.5 VLMs are judged as the **text decoder only** (`PARTIAL_TASK_ONLY`: 85,776 repositories); Gemma4, Qwen3-VL and SmolVLM wrappers have no adapter |
| image towers and CNNs | `hf-vis/{vit, clip_vision, siglip_vision, convnext*, resnet*, mobilenet*}` | pooled / hidden features vs HF, calibrated integer program, admission, an `EmbeddingI32` image embedding | **image-classification heads are not read**: the CNN specs list the classifier among the tensors never read by design, the ViT adapter ignores `pooler.` only — *backbone only*. 30,862 `image-classification` repositories stop at the profile row |
| image generation | `hf-diff/sd3_tiny` | 13-stage pipeline, three-way identity; `gen_sd3_class.rs` | UNet pipelines (SD 1.x / XL): `GEN_UNET_SKIP_V1` and its companions are `Missing` with **no** kernel requirement — the frontend owes them (28,312 repositories) |

## 6. What the census cannot measure offline (GAP, with the size)

* **Pickle checkpoints.** 97,363 estimated repositories are `NOT_RUN` (listing level: 79,346 exact for pickle-only checkpoints; the rest are
  adapters whose base is a pickle, from the adapters cohort); in the local-LLM frame 66,375 units (7.1 %). Measuring them needs, per repository, the zip's
  central directory, `data.pkl`, and — because the reader checks each used member's local header — one 30-byte range per storage (~300 requests for a 7B).
  That is a fetch-side change plus a sampled cohort; **not built**, because it cannot be exercised without network.
* **Split GGUF sets** (4,630 frame units) and **`pytorch-adapter`** (`adapter_model.bin`, 4,978): decided as `FORMAT_UNSUPPORTED` at the listing, so no
  header was fetched; both are frontend work with the same measurement limit.
* **Text classification and ranking at shape depth.** Listing level (exact, no judgement): of the 155,608 repositories tagged `text-classification`,
  `text-ranking`, `zero-shot-classification` or carrying a `…ForSequenceClassification` class, **117,903 (75.8 %) name an architecture an adapter
  claims** (BERT 52,438, DistilBERT 30,746, RoBERTa 19,250, XLM-R 9,331, Llama 2,679, GPT-2 1,579, Qwen2 / 3 1,135, OPT 326, …): 79,742 ship
  safetensors, 31,410 only a pickle, 6,751 list no weights. That is an upper bound on frontend reach, not shape-readiness. Unclaimed and large:
  DeBERTa-v2 4,509, ModernBERT 2,829, ALBERT 2,222, ELECTRA 1,627, XLNet 1,529, CamemBERT 799 (a RoBERTa twin).
* **Real weights and `Final`.** No census row has a conformance pack, a seat or a `Final`: registration-ready is **0 measured**.
* **Quantised checkpoints with the config in a sibling file.** GPTQ / AWQ tensors with no `quantization_config` (≈1,300 repositories) and EXL2 (≈1,300)
  stop as `TENSOR_MISSING attn.q.w`; AutoGPTQ keeps its parameters in `quantize_config.json`, which the census does not fetch.

## 7. Task 4 — Wave-1 real local sources (`hf-ckpt/`)

Static admission at `--max-context 2048` and `8192` with the final build (`palw-class preflight --network testnet-12 … --depth shape`), then a
real-weights conversion (`palw-tir-convert`, one process each, RSS ≤ 1.91 GB). The conversions' calibration was the plumbing-only "random ids (seed 7)"
set: they prove **import, lowering, container and admission**, not fidelity; the court-grade calibration is lane C's pack step.

| model | family | frontend | convert / register / mine (2048, 8192) | admission | kernel route | converted artifact (digest, first 16 hex) |
|---|---|---|---|---|---|---|
| HuggingFaceTB/SmolLM2-1.7B-Instruct | dense, GQA (`llama`) | built-in data, level B | ok / ok / ok | `ADMISSIBLE_GENERIC` (1.03× on generic kernels) | K2-TIR-v1: shipped `KERNEL_NOT_ACTIVE`, armed `ELIGIBLE_AT` | `5ee4de6a26b4c6b2` (1,332 tensors, 1,780 MiB) |
| Qwen/Qwen3.5-0.8B | gated delta rule + vision wrapper (`vlm-qwen3-5`) | built-in data, level B | ok / ok / ok | `ADMISSIBLE_GENERIC` (1.05×) | same | `161721f14864f3fc` (1,980 tensors, 1,004 MiB) |
| ibm-granite/granite-3.1-1b-a400m-instruct | MoE (`granitemoe`) | built-in data, level B | ok / ok / ok | `ADMISSIBLE` (the generic-kernel estimate waits for a declared layout) | same | `7fc7c06a545377ec` (1,236 tensors, 1,344 MiB) |
| state-spaces/mamba-370m-hf | Mamba SSM (`mamba`) | built-in data, level B | ok / ok / ok | `ADMISSIBLE_GENERIC` (1.27×) | same | `02da34bb8519a926` (2,216 tensors, 556 MiB) |
| ai21labs/Jamba-tiny-dev | attention + Mamba + MoE (`jamba`) | built-in data, level B | ok / ok / ok | `ADMISSIBLE_GENERIC` (1.06×) | same | `71c8ddefe7ec49df` (1,196 tensors, 365 MiB) |
| microsoft/phi-1_5 | dense, partial rotary, parallel residual (`phi`) | built-in data, level B | ok / ok / ok | `ADMISSIBLE_GENERIC` (1.03×) | same | `2e59493a56881893` (1,170 tensors, 1,492 MiB) |

* **First blocker per family: none in the frontend, lowering or static admission.** The first blocker to *Active* is the same for all six and is
  not lane A's: `KERNEL_NOT_ACTIVE` (K2-TIR-v1 is `Implemented`, not active), then conformance / pack / seat / `Final` (lanes C, D).
* **What is out of the class**, as the preflight prints it: Qwen3.5's vision tower (153 tensors, 201 MB) and its multi-token-prediction heads — the
  registered class is the text decoder, and a prompt cannot carry an image (`PARTIAL_TASK_ONLY`).
* `KERNEL_EXTENSION_REQUIRED` found by Wave-1: **none**.

## 8. `KERNEL_EXTENSION_REQUIRED` list

Empty for everything lane A touched. (a) No Wave-1 family needed a primitive, relation or court the shipped kernels lack. (b) The 22 non-`Implemented`
registry features are all `NoReq`: lowering work, not protocol work. (c) The classifier head is a `[1, labels]` `Final` row of a `Linear` over existing
primitives. (d) The one kernel-side item is the **task profile** question for classifiers and rerankers, recorded as `KERNEL_EXTENSION_REQUIRED` with the
reason `task profile`, as the Lead specified — a consensus decision, not a missing semantic.

## 9. Remaining frontend blockers, by size

Local-LLM frame units (`L_files` = 934,271) after: `PARTIAL_TASK_ONLY` 87,474 (9.4 %, profile); `BASE_UNPINNED` 72,024 (7.7 %, external); pickle
unmeasured 66,375 (7.1 %, §6); `ARCH_REFUSED` 43,814 (4.7 %); `TOKENIZER_MISSING` 25,990 (2.8 %, the repository carries none); `ADAPTER_REFUSED`
25,977 (2.8 %); `FORMAT_UNSUPPORTED` 21,372 (2.3 %: `pytorch-adapter`, split GGUF, other); `COURT_BUDGET` 19,785 (2.1 %, resource);
`CUSTOM_CODE_UNMODELLED` 19,536 (2.1 %, repository code is never run); `GATED_ACCESS` 18,451 (2.0 %); `CONFIG_KEY_UNREAD` 15,176 (1.6 %);
`QUANT_DESCRIPTOR_MISSING` 12,239 (1.3 %).

Generic frontend families still open, with the evidence that sizes them:

1. **GGUF architectures with no mapping** (`ARCH_REFUSED`): `seed_oss` 5,961, `qwen35moe` 1,360, `gemma4` 995, `granite` / `granitemoe` / `lfm2` 879
   each, `bert` 585 (the BERT-family embedders), `deepseek2`, `olmo2`, `longcat`, `lfm2moe`. Frontend (key map + tensor map), no kernel need.
2. **LoRA on a fused projection** (`qkv_proj`, `gate_up_proj`, `c_attn`, `query_key_value`): 7,467 repositories of `ADAPTER_REFUSED` ("one `A` shared by slices
   of `B`"); `rsLoRA` with a non-square rank 2,489; GGUF LoRA 6,198; `fan_in_fan_out` 3,601 (GPT-2 LoRA targets are fused anyway).
3. **Decoder-as-embedder routing** (needs `modules.json` / the pooling): 1,005 `TASK_MISMATCH` plus embedders without an `lm_head` (`TENSOR_MISSING head.w`, 434).
4. **An adapter stored in a checkpoint that looks like a model** (`…lora_A.weight` tensors beside a model `config.json`): ≈2,600 repositories are blamed as
   `TENSOR_MISSING embed.table` / `attn.q.w`; the diagnosis is wrong (they are adapters with no `adapter_config.json`); not fixed.
5. **Pre-quantised descriptors**: `config/unknown` 6,954, HQQ 866, auto-round 866, NVFP4 / MXFP4 compressed-tensors 1,305, ModelOpt 441, EXL3 432.
6. **Encoder classifier twins**: DeBERTa-v2, ModernBERT, ALBERT, ELECTRA, CamemBERT (sizes in §6); heads not read at all: token classification (35,316
   repositories), question answering (14,988), fill-mask (21,884), image classification (30,862). Each needs the head, a fixture and a profile row.
7. **VLM wrappers**: `Gemma4ForConditionalGeneration` 4,778, `Qwen3VL…` 1,244, `SmolVLM…` 1,244, `NewModel` 2,200, `MetaClip2Model` 1,429: no adapter.
8. **M2M100 / NLLB** 2,578 (encoder-decoder, no adapter), ParlerTTS 1,299.

## 10. Cross-lane notes

* **Lane C (runtime pack):** the tokenizer `keep` list is `TOKENIZER_FILES_V1` in `artifact.rs` (SentencePiece `.model`, WordPiece `vocab.txt`, `tekken.json`,
  `.tiktoken`); the feature registry digest changed (`ROPE_FREQ_FACTORS_V1`, `OUTPUT_CLASSIFY_V1`) and so did the adapter-pack hash (16 adapters
  edited or added, re-pinned in `tests/golden/adapter_pins_v1.json`). A `pytorch_model.bin` pack converts to the same container digest as its safetensors original.
* **Lead:** (a) classifiers are carried by the Embedding profile today (`gen_classifier_class.rs`); the profile row for `text-classification` /
  `text-ranking` is the one switch that makes the 117,903 architecture-claimed repositories countable; (b) `UNDECIDED_UNMEASURED` +78,140 and
  `NOT_RUN_NEEDS_PICKLE_DIRECTORY` +19,223 are the pickle relabel — if the matrix keeps a `FORMAT_UNSUPPORTED (pytorch)` row, its after is *unmeasured*, not zero.
* **No frontend bug that makes an admitted class semantically wrong was found.** The over-refusals that mattered (F6, F7, F10) were the opposite error;
  each is bounded by a rule transformers itself applies, and each keeps a guard that refuses again when the checkpoint disagrees.

## 11. Reproduce

```sh
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --offline -p misaka-palw-tir-lower --lib
cargo test --offline -p misaka-palw-tir-lower --test gguf_rope_freqs --test torch_bin --test hf_keys_ignored --test seqcls --test legacy_layernorm_names \
  --test quant_tensors --test quantized --test adapter_pins --test real_configs --test encoders
cargo test --offline -p misaka-palw-sdk --lib
cargo test --offline -p misaka-palw-sdk --test preflight --test gen_classifier_class
# the census (offline, snapshot 2026-10-03T131904Z): classify, headers pass, shape pass, cohorts, then
#   misaka-palw-sdk/tools/hf_census/report.py       --classified-new … --cohort NAME:ROWS …
#   misaka-palw-sdk/tools/hf_census/report_local.py … --frame-headers-rows … --frame-shape-rows …
# a preflight of a local model directory:
palw-class preflight --network testnet-12 <dir> --depth shape --max-context 2048 --json
```

Last targeted run (this HEAD, all pass): `tir-lower --lib` 149, `gguf_rope_freqs` 4, `torch_bin` 18, `hf_keys_ignored` 4, `seqcls` 4, `legacy_layernorm_names` 2,
`adapter_pins` 2, `real_configs` 32, `quant_tensors` 19, `quantized` 34, `encoders` 15, `feature_registry` 5, `hf_fixtures` 100; `sdk --lib` 113, `preflight` 20,
`gen_classifier_class` 2. Known and not mine: `tests/adapters.rs` legacy-oracle cases (`paligemma`, `deepseek_v32`) fail at the base commit as well.
