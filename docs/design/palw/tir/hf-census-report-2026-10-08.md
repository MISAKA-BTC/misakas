# PALW-TIR — Hugging Face census report, 2026-10-08 (COV-P4: coverage after the onboarding closure)

*Lane COV-P4, branch `cov/p4-census`. Method: `hf-census-v1.md` (§4d is this report's addendum). Snapshot `2026-10-03T131904Z` (T0 =
2026-10-03 13:19:04 UTC, `D_all` = 3,117,871 repositories), the method's own probability sample (seed `misaka-palw-hf-census/2026-10-03T131904Z/v1`,
n = 1,773, and the three bucket cohorts of n = 150) re-judged at the shape depth by `palw-class census gates` built from tree `d368707e0`
(`a98cc4e31` + the pipeline-class preflight, COV-P4), at two testnet-12 rulesets: **A = DAA 6,900** (today) and **B = DAA 9,000** (the first height at which every
scheduled fence is in force). Header-only, offline: the snapshot's saved headers were re-read; **no Hub request was made**. Rights policy `none`; the
technical view is reported. Machine-readable twin: `docs/rfc/evidence/0011-hf-census-coverage-2026-10-08.json` (arithmetic checked by
`node docs/rfc/evidence/0011-census-coverage-check.mjs --check`).*

## 概要(日本語)

- **達成率(shape-ready = 宣言 task 全体を宣言 context で shape 深度の preflight が通した repo。登録ではない)**: `D_all` 3,117,871 に対し
  **16.65 %(片側 95 % 下限 15.74 %)**(DAA 9,000。DAA 6,900 でも 16.64 %、差は 451 repo)。
  「重みがあり・非 gated・adapter の base が解決でき・model task を持つ」`D_b`(= `D_all` の 51.49 %、1,605,403 repo)に対しては
  **32.34 %(下限 30.56 %)**。update 2 と同じ plain な cohort 展開では 18.12 %(下限 16.81 %)で、update 2 の 17.96 %(16.58 %)と標本誤差内で一致。
- **「登録済み」(full-task Active/Final)は 0**。実在する受理済み登録は private devnet 上の Qwen2.5-0.5B-Instruct 1 件のみで、512 / 32,768 positions の短 context、dormant、synthetic beacon、G14 は GAP。
  RFC-0011 の受け入れ条件(登録済み / `D_all` の片側 95 % 下限 ≥ 90 %)は **未達**。
- **90 % は `D_all` では到達不能**: external(MISSING_WEIGHTS 18.93 %・GATED 1.71 %・ADAPTER_BASE_MISSING 7.58 %・NO_MODEL_TASK 20.28 %)が
  48.51 %。ソフトウェアで閉じられる失敗を全て閉じても **51.49 %** が上限。task を持たない repo も閉じられると仮定しても 71.77 %。
- **update 2 比の実体は退行と改善の相殺**: 同じ標本・同じ重みで update 2 の row を今日の frame で集計すると 18.01 % → 今回 16.65 %。
  GGUF の foreign namespace 拒否(`mradermacher.*` など。P1 の「非 architecture namespace 拒否」)が **約 81,883 repo(2.63 %)を失わせ**、frontend 修正(config key / rope_freqs / task 推定 / head.w / tokenizer / nextn)が約 +1.3 pt を戻した(標本 2,223 repo 中 63 が新たに shape-ready、17 が脱落)。
  inert な provenance namespace の登録簿(data)が最大の「安い」改善。
- **preflight 修正**: encoder–decoder(T5 系)の class を `verify_gen_class_admission_v1` で判定し、判定できない route は `PIPELINE_CLASS_UNDECLARED` を返す。`unknown` で exit 0 にならない。
  flan-t5-small@0fc9ddf7 は 512 positions で `CLOSE_SIZE_OVER_CAP`(生成 close sizing work が 2^26 を超える)で拒否される。census も同じ判定を使うので T5 系は UNTESTED でなくなった(enc-dec の約 28,265 repo のうち shape-ready は 451)。

## 1. Results

### 1.1 The headline, and what each rate counts

| | DAA 6,900 (today) | DAA 9,000 (every scheduled fence) |
| --- | ---: | ---: |
| `D_all` (all public repositories) | 3,117,871 | 3,117,871 |
| `D_b` (weight-accessible, valid target model), estimated | 1,605,403 (51.49 % of `D_all`) | 1,605,403 (51.49 % of `D_all`) |
| **shape-ready / `D_all`** (point; one-sided 95 % LB) | 518,706 = **16.64 %** (LB 15.73 %) | 519,157 = **16.65 %** (LB 15.74 %) |
| **shape-ready / `D_b`** (point; one-sided 95 % LB) | **32.31 %** (LB 30.53 %) | **32.34 %** (LB 30.56 %) |
| shape-ready / `D_files` (listing-level) | 23.08 % (LB 21.82 %) | 23.10 % (LB 21.84 %) |
| admitted at 2,048 only (short context; **not counted**) | 508 (0.02 %) | 508 (0.02 %) |
| download-weighted shape-ready (normal-approx. LB) | 8.21 % (LB 7.95 %) | 8.21 % (LB 7.95 %) |

* The funnel (ruleset B): `source` PASS 72.08 % of `D_all` (exact, from the listing) → `lower` PASS 18.60 % (LB 17.68 %) → `admit` PASS at the primary context = **shape-ready 16.65 %** (LB 15.74 %).
  `pack`, `seat`, `final` are not run by a header census; **registration-ready is not measured, and nothing here is inferred to be registered**.
* **Like-for-like with update 2** (same sample, same weights, same estimator, today's listing frame): update 2's rows give 18.01 % (LB 17.17 %); the current tree gives 16.65 % (LB 15.74 %). Update 2 as published (plain expansion): 17.96 % (LB 16.58 %); this run, plain: 18.12 % (LB 16.81 %).
  The plain and post-stratified estimators differ by 1.5 points: the plain expansion multiplies each of the 150 members of a bucket cohort by N/n (≈ 6,000 repositories a member) *including members the current listing already decides*; the post-stratified estimate uses the exact listing verdict for those and the cohort only for the members the listing leaves undecided (§5). We headline the post-stratified one and publish both.

### 1.2 The registration columns — never conflated

Each column is its own count over `D_all`. Only the last is a registration in RFC-0011 §7's sense.

| Column | Count | Source / meaning |
| --- | ---: | --- |
| listed-only (the listing alone decides the verdict) | 2,228,617 exact | `D_all` minus the header-read part |
| header-read (represented by the sampled units, estimated) | 889,254 est. (1937 units) | judged at the shape depth, no weight byte read |
| shape-ready, ruleset A (DAA 6,900) | 518,706 (16.64 %) | source + lower + admit PASS at the declared context ≤ 8,192 |
| shape-ready, ruleset B (DAA 9,000) | 519,157 (16.65 %) | the same at every scheduled fence |
| … of which **at the model's declared context** | 133,745 (4.29 %, LB 3.80 %) | declared positions ≤ 8,192 (or none declared) |
| … of which **at the testnet cap 8,192 while the model declares a wider context** | 385,412 (12.36 %) | a short-context substitution for RFC-0011 §7 (a class declares its own context; testnet-12's are 8k) |
| … of which no fleet seat tier holds the class (`SEAT_MEMORY`) | 194,995 (6.25 %) | registrable, never mineable on today's tiers (3.5 / 8 GiB shares) |
| short-context only (admitted at 2,048, not at the primary) | 508 | its own stratum; **not counted** in shape-ready |
| text-only (`PARTIAL_TASK_ONLY`: the text stage lowers, the image/audio stage does not) | 77,993 repositories carry the code (exact); the text stage alone admits for 25 of the 150 sampled from the baseline's 52,091 ≈ 8,682 | **not credited**: the repository's task is the full input task |
| synthetic-beacon conformance PASS | 2 repositories | H1 evidence: Qwen2.5-0.5B-Instruct, SmolLM2-1.7B-Instruct (facts SYNTHETIC, policy UNAPPROVED) |
| unit-test / fixture PASS | fixtures, no repo@sha | 72 of 100 curated light specs (`0011-existing-coverage-audit.json`), the pinned Huihui-9B refusals, `tests/preflight_pipeline_class.rs` |
| dormant registration (accepted, private devnet) | 1 | `Qwen/Qwen2.5-0.5B-Instruct@7ae55760` at **512 of its 32,768 positions**; pack verified; registering node, restart and fresh IBD agree; kernel route `KERNEL_NOT_ACTIVE`; G14 gates GAP |
| independently verified reuse of a registered class id / root | 0 | none evidenced; a similar name never counts |
| **registered: full-task Active/Final** | **0** | RFC-0011 §7 numerator. The one accepted registration is short-context and dormant on a private devnet |

The H1 evidence is on branch `onboard/h1-hf-closed-loop` (tip `286225802`), **not an ancestor of the integration HEAD this branch started from**; read from the git object store
(`docs/evidence/hf-onboarding/20261008-*`). Its runs are on a private salted testnet-12 drill genesis, not the public network. Highest levels of the other runs: SmolLM2-1.7B L2 (pack verified),
Qwen3-0.6B / R1-Distill-Qwen-1.5B / DeepSeek-V2-Lite / Qwen1.5-MoE L1, flan-t5-small and SmolVLM L0 (the preflight defect fixed here).

**RFC-0011 §7's bar**: the one-sided 95 % lower bound of `registered_full_task / D_all` must be ≥ 0.90. Measured numerator: 0 of 3,117,871. The upper bound any rate over `D_all` can reach is 51.49 % (§1.4).

### 1.3 Blockers by bucket, both denominators

| Bucket | family | DAA 6,900 (today): repos | % `D_all` | % `D_b` | DAA 9,000 (every scheduled fence): repos | % `D_all` | % `D_b` |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `SHAPE_READY` | success(shape) | 518,706 | 16.64 % | 32.31 % | 519,157 | 16.65 % | 32.34 % |
| `MISSING_WEIGHTS` | external | 590,290 | 18.93 % | outside `D_b` | 590,290 | 18.93 % | outside `D_b` |
| `GATED` | external | 53,420 | 1.71 % | outside `D_b` | 53,420 | 1.71 % | outside `D_b` |
| `ADAPTER_BASE_MISSING` | external | 236,370 | 7.58 % | outside `D_b` | 236,370 | 7.58 % | outside `D_b` |
| `FRONTEND` | software-closable | 330,726 | 10.61 % | 20.60 % | 330,726 | 10.61 % | 20.60 % |
| `NEW_KERNEL` | software-closable | 536,365 | 17.20 % | 33.41 % | 536,365 | 17.20 % | 33.41 % |
| `QUANT_FORMAT` | software-closable | 59,251 | 1.90 % | 3.69 % | 59,251 | 1.90 % | 3.69 % |
| `RESOURCE` | software-closable | 38,251 | 1.23 % | 2.38 % | 37,800 | 1.21 % | 2.35 % |
| `UNTESTED` | software-closable | 122,105 | 3.92 % | 7.61 % | 122,105 | 3.92 % | 7.61 % |
| `NO_MODEL_TASK` | external | 632,388 | 20.28 % | outside `D_b` | 632,388 | 20.28 % | outside `D_b` |

*Bucket map* (`tools/hf_census/buckets.py`, one table; a `NOT_RUN_*` code is `UNTESTED` and never a pass): `GATED_ACCESS`, `REPO_DISABLED` → `GATED`; `MISSING_WEIGHTS`, `WEIGHTS_INCOMPLETE`, `REPO_UNREACHABLE`,
`FETCH_FAILED`, `HEADER_INVALID` → `MISSING_WEIGHTS`; `BASE_UNPINNED` (absent / ambiguous / not in snapshot / base gated) → `ADAPTER_BASE_MISSING`; `TASK_UNKNOWN(no-config)`, `CONFIG_MISSING`, `CONFIG_INVALID` → `NO_MODEL_TASK`
(a ninth bucket, external: no model task to be registered as); `MODALITY_PROFILE_MISSING`, `PARTIAL_TASK_ONLY`, a `FEATURE_C` the registry says needs a capability, `FENCE_NOT_ARMED` → `NEW_KERNEL`;
`FORMAT_UNSUPPORTED`, `QUANT_DESCRIPTOR_MISSING`, `QUANT_REFUSED` → `QUANT_FORMAT`; `COURT_BUDGET`, `CLOSE_TOO_LARGE`, `ADMISSION_EXCEEDS`, `CONTEXT_BOUND`, `COURT_WINDOW_EXCEEDED`, `DA_LADDER_EXCEEDED`, `HEADER_TOO_LARGE`, `SEAT_MEMORY` → `RESOURCE`;
the reader's refusals (`ARCH_REFUSED`, `CONFIG_KEY_UNREAD`, `TOKENIZER_MISSING`, `TENSOR_*`, `CUSTOM_CODE_UNMODELLED`, `ADAPTER_*`, `TASK_UNKNOWN` with a configuration, `ADMISSION_REFUSED`) → `FRONTEND`.
A self-check compares each bucket with the row's machine class (`onboarding.rs`): 0 disagreements, 0 unmapped codes.
`D_b` (denominator b) is `D_all` minus the four external buckets: weights present and complete, public and not gated, an adapter's base pinned in the snapshot, and a model task or a configuration that names a model class.

### 1.4 External versus software-closable, and the ceiling

| | DAA 6,900 (today) | DAA 9,000 (every scheduled fence) |
| --- | ---: | ---: |
| external failures (missing / gated / unresolvable source, no model task) | 1,512,468 (48.51 %) | 1,512,468 (48.51 %) |
| software-closable failures (`FRONTEND` + `NEW_KERNEL` + `QUANT_FORMAT` + `RESOURCE` + `UNTESTED`) | 1,086,698 (34.85 % of `D_all`, 67.69 % of `D_b`) | 1,086,247 (34.84 % of `D_all`, 67.66 % of `D_b`) |
| **maximum achievable rate over `D_all`** if every software-closable failure were fixed | **51.49 %** (SE 0.05 %) | **51.49 %** (SE 0.05 %) |
| … even if every repository with no model task were also closable | 71.77 % | 71.77 % |

Reading: **48.51 % of all public repositories fail for a reason nothing in this tree can supply** — 18.93 % have no usable weights, 1.71 % are gated, 7.58 % are adapters whose base is absent, ambiguous or gated, and 20.28 % declare no task and carry no configuration that names a model class.
RFC-0011 §7 says it in advance: if such repositories alone exceed 10 %, no converter or extra CPU makes the all-public-listings target true. They are 48.51 %.
The 90 % bar over `D_all` is **unreachable**; the honest headline over the repositories that are weight-accessible and form a valid target model is the `D_b` rate. (Counting the 20.28 % with no model task as closable would lift the ceiling only to 71.77 %.)

### 1.5 The earlier analysis's dimension

| Dimension | DAA 6,900 (today): repos | % `D_all` | DAA 9,000 (every scheduled fence): repos | % `D_all` |
| --- | ---: | ---: | ---: | ---: |
| `success` | 518,706 | 16.64 % | 519,157 | 16.65 % |
| `external` | 1,512,468 | 48.51 % | 1,512,468 | 48.51 % |
| `frontend-only` | 176,071 | 5.65 % | 176,071 | 5.65 % |
| `feature-only` | 213,906 | 6.86 % | 213,906 | 6.86 % |
| `protocol-envelope` | 574,616 | 18.43 % | 574,165 | 18.42 % |
| `untested` | 122,105 | 3.92 % | 122,105 | 3.92 % |

### 1.6 Where repositories stop, by code

Ruleset B (DAA 9,000), estimated repositories; ruleset A differs only in `CLOSE_TOO_LARGE` / shape-ready (one BART-mini member).

| Bucket | Gate | Code | Repositories | % `D_all` | Leading arguments |
| --- | --- | --- | ---: | ---: | --- |
| `NO_MODEL_TASK` | lower | `TASK_UNKNOWN` | 625,173 | 20.05 % | `no-config` 625,173 |
| `MISSING_WEIGHTS` | source | `MISSING_WEIGHTS` | 579,477 | 18.59 % |  |
| `SHAPE_READY` | admit | `SHAPE_READY` | 519,157 | 16.65 % |  |
| `NEW_KERNEL` | lower | `MODALITY_PROFILE_MISSING` | 458,372 | 14.70 % | `text-classification` 149,560, `reinforcement-learning` 75,402, `automatic-speech-recognition` 41,607 |
| `ADAPTER_BASE_MISSING` | source | `BASE_UNPINNED` | 236,370 | 7.58 % | `absent` 120,903, `base_gated` 106,897, `not_in_snapshot` 8,570 |
| `FRONTEND` | lower | `ARCH_REFUSED` | 115,380 | 3.70 % | `gguf-namespace:mradermacher` 81,004, `gguf-arch:seed_oss` 5,984, `encdec-no-adapter:M2M100ForConditionalGeneration` 2,578 |
| `UNTESTED` | lower | `NOT_RUN_NEEDS_PICKLE_DIRECTORY` | 97,360 | 3.12 % | `pytorch` 97,360 |
| `NEW_KERNEL` | lower | `PARTIAL_TASK_ONLY` | 77,993 | 2.50 % | `image-text-to-text` 64,627, `any-to-any` 11,576, `visual-question-answering` 761 |
| `FRONTEND` | lower | `TASK_UNKNOWN` | 54,160 | 1.74 % |  |
| `GATED` | source | `GATED_ACCESS` | 53,420 | 1.71 % | `manual` 34,213, `auto` 19,187, `VibeThinker-3B.Q8_0.gguf` 10 |
| `FRONTEND` | lower | `FEATURE_C` | 43,811 | 1.41 % | `GEN_UNET_SKIP_V1` 28,312, `an adapter for `NewModel`` 2,200, `an adapter for `MetaClip2Model`` 1,429 |
| `QUANT_FORMAT` | lower | `FORMAT_UNSUPPORTED` | 42,231 | 1.35 % | `pytorch` 9,395, `pytorch-adapter` 8,221, `onnx` 4,959 |
| `FRONTEND` | lower | `TOKENIZER_MISSING` | 29,174 | 0.94 % |  |
| `FRONTEND` | lower | `ADAPTER_REFUSED` | 25,422 | 0.82 % |  |
| `UNTESTED` | admit | `NOT_RUN_PIPELINE_ADMISSION` | 23,033 | 0.74 % |  |
| `FRONTEND` | lower | `CUSTOM_CODE_UNMODELLED` | 19,558 | 0.63 % | `remote-code:QuasarForCausalLM` 3,460, `remote-code:PhiForCausalLM` 2,162, `remote-code:SDARForCausalLM` 1,730 |
| `FRONTEND` | lower | `CONFIG_KEY_UNREAD` | 18,136 | 0.58 % | `audio_config` 3,412, `max_seq_len` 2,166, `quantization` 1,298 |
| `FRONTEND` | lower | `ADAPTER_UNCHECKED` | 16,686 | 0.54 % |  |
| `RESOURCE` | admit | `CLOSE_TOO_LARGE` | 16,300 | 0.52 % | `generative close sizing work` 16,300 |
| `RESOURCE` | admit | `COURT_BUDGET` | 12,559 | 0.40 % | `IR tile multiply-accumulates` 9,472 |
| `QUANT_FORMAT` | lower | `QUANT_DESCRIPTOR_MISSING` | 12,220 | 0.39 % | `config/unknown` 7,387, `hqq` 866, `config/auto-round` 866 |
| `MISSING_WEIGHTS` | source | `WEIGHTS_INCOMPLETE` | 9,904 | 0.32 % | `model` 9,863, `model.safetensors` 11, `diffusion_pytorch_model` 6 |
| `RESOURCE` | admit | `ADMISSION_EXCEEDS` | 8,939 | 0.29 % | `max_position_macs` 6,321, `max_job_step_leaves` 2,618 |
| `NO_MODEL_TASK` | lower | `CONFIG_MISSING` | 6,781 | 0.22 % | `unet` 19, `vae` 1, `tags` 1 |
| `FRONTEND` | lower | `TENSOR_MISSING` | 5,696 | 0.18 % | `attn.q.w` 3,477, `embed.table` 1,785, `head.w` 434 |
| `QUANT_FORMAT` | lower | `QUANT_REFUSED` | 4,800 | 0.15 % | `CT_FP8_CHANNEL` 1,749, `GPTQ` 1,298, `AWQ` 865 |
| `FRONTEND` | lower | `TENSOR_SHAPE` | 1,696 | 0.05 % | `mlp.up.w` 1,264, `embed.table` 432 |
| `UNTESTED` | lower | `NOT_SAMPLED_CELL` | 1,206 | 0.04 % | `task_unknown/NOT_RUN_NEEDS_PICKLE_DIRECTORY` 1,206 |
| `FRONTEND` | lower | `TASK_MISMATCH` | 1,005 | 0.03 % | `feature-extraction→text-generation` 522, `sentence-similarity→text-generation` 483 |
| `MISSING_WEIGHTS` | source | `HEADER_INVALID` | 474 | 0.02 % | `model` 434, `aceinstruct-72b-q2_k.gguf` 10, ` DeepSeek-R1-Llama-8B.BF16.gguf` 10 |
| `UNTESTED` | lower | `NOT_RUN_NEEDS_TENSOR_DATA` | 440 | 0.01 % | `a tensor of the file` 440 |
| `MISSING_WEIGHTS` | source | `FETCH_FAILED` | 435 | 0.01 % | `layer23-shared.safetensors` 434, `(shards)` 1 |
| `NO_MODEL_TASK` | lower | `CONFIG_INVALID` | 434 | 0.01 % |  |
| `UNTESTED` | lower | `NOT_RUN_NEEDS_WEIGHTS` | 66 | 0.00 % |  |
| `RESOURCE` | source | `HEADER_TOO_LARGE` | 1 | 0.00 % | `model.safetensors.index.json` 1 |

Estimates built on few sampled repositories (the `TASK_UNKNOWN` cohort's undecided cell has about 25 members; each stands for ≈ 6,000 repositories) are quantised: read the leading arguments, not the last digit.

## 2. Deltas against update 2

| | update 2 (published) | now (B, plain expansion) | now (B, post-stratified) |
| --- | ---: | ---: | ---: |
| `D_all` technical shape-ready | 17.96 % (LB 16.58 %) | 18.12 % (LB 16.81 %) | **16.65 %** (LB 15.74 %) |
| `D_files` shape-ready | 25.30 % (LB 23.34 %) | — | 23.10 % (LB 21.84 %) |
| `lower` PASS | 19.73 % (LB 18.31 %) | — | 18.60 % (LB 17.68 %) |
| the same rows, today's frame (post-stratified): shape-ready | 18.01 % (LB 17.17 %) | | 16.65 % |

**Member level** (the 2,223 sampled and cohort repositories judged in both runs): **63 became shape-ready, 17 stopped being shape-ready, 2,143 unchanged.** All 17 losses are one cause:
`ARCH_REFUSED(gguf-namespace:mradermacher | duynt)` — a GGUF whose metadata carries a foreign namespace is refused since the P1 change that refuses non-architecture namespaces (`prism.hadamard` declares a weight-space rotation; `mradermacher.convert_type`,
a quantizer's bookkeeping key, declares nothing). Scaled to the population that is **≈ 81,883 repositories (2.63 %)**. The 63 gains, by the blocker each had in update 2: configuration keys read 23, GGUF `rope_freqs` 10
(`ROPE_FREQ_FACTORS_V1`), task inference v3 10, `head.w` tensor mapping 9, tokenizer files 5, GGUF `nextn` 5, one pipeline class — the frontend fixes of lane A in numbers.

By code (estimated repositories, update 2 as published against ruleset B now):

| gate / code | update 2 | now | Δ | why |
| --- | ---: | ---: | ---: | --- |
| `lower/FORMAT_UNSUPPORTED` | 132,355 | 42,231 | −90,124 | `pytorch_model.bin` is no longer "an unsupported format" |
| `lower/NOT_RUN_NEEDS_PICKLE_DIRECTORY` | 0 | 97,360 | +97,360 | … it is `NOT_RUN` (UNTESTED): the zip directory and `data.pkl` are outside the 10-03 network policy |
| `lower/ARCH_REFUSED` | 39,607 | 115,380 | +75,773 | 81,883 of it is the GGUF foreign-namespace refusal; the rest GGUF architectures, M2M100, … |
| `lower/NOT_RUN_NEEDS_TENSOR_DATA` | 23,600 | 440 | −23,160 | `ROPE_FREQ_FACTORS_V1` reads `rope_freqs` as a structural factor table |
| `admit/NOT_RUN_PIPELINE_ADMISSION` | 41,853 | 23,033 | −18,820 | encoder–decoders are judged by the generative admission (below) |
| `admit/CLOSE_TOO_LARGE` + `ADMISSION_EXCEEDS` (new codes at this gate) | 0 | 16,300 + 8,939 | +25,239 | the encoder–decoder verdicts: T5 / Marian close sizing, BART-large `max_position_macs` |
| `lower/CONFIG_KEY_UNREAD` | 27,449 | 18,136 | −9,313 | keys read or made inert |
| `lower/ADAPTER_UNCHECKED` | 24,757 | 16,686 | −8,071 | adapters composed with their pinned base |
| `lower/TENSOR_MISSING` | 12,234 | 5,696 | −6,538 | `head.w` mapping |
| `lower/FEATURE_C` | 49,796 | 43,811 | −5,985 | |
| `lower/TOKENIZER_MISSING` | 32,470 | 29,174 | −3,296 | tokenizer files table |
| `admit/SHAPE_READY` | 559,985 | 519,157 | −40,828 | net of the above |

The listing frame also moved with the tree: `D_files` (source PASS) is 2,247,414 against 2,213,693 (adapter bases resolved through the renames), and the baseline's `FORMAT_UNSUPPORTED` repositories that the listing no longer decides
(78,140 `pytorch_model.bin` repositories) are counted **not passing** (`UNTESTED`), never estimated.

## 3. The top 10 software-closable blockers — the work list

Ranked by estimated repositories (ruleset B), over `FRONTEND`, `NEW_KERNEL`, `QUANT_FORMAT`, `RESOURCE` and `UNTESTED`. The last column is the generic feature or profile that would close the family; none is a model-name rule.

| # | Bucket | Blocker family | Repositories (% `D_all`) | Basis | Leading codes | The generic change that would close it |
| ---: | --- | --- | ---: | --- | --- | --- |
| 1 | `NEW_KERNEL` | NLU heads (classification / token / QA / fill-mask) | 225,210 (7.22 %) | exact (listing) | `MODALITY_PROFILE_MISSING(text-classification)` 149,560, `MODALITY_PROFILE_MISSING(token-classification)` 35,316, `MODALITY_PROFILE_MISSING(fill-mask)` 21,884 | a canonical job profile for encoder heads: label / span / token logits as the committed output, the label set in the class (the frontend already lowers a sequence classifier as an Embedding-profile class); a `tasks.rs` row per head + one RFC-0003 profile |
| 2 | `NEW_KERNEL` | RL / robotics / tabular / time series / video / graph | 114,772 (3.68 %) | exact (listing) | `MODALITY_PROFILE_MISSING(reinforcement-learning)` 75,402, `MODALITY_PROFILE_MISSING(robotics)` 32,059, `MODALITY_PROFILE_MISSING(text-to-video)` 2,590 | no importer for these frameworks (SB3 zips, sklearn, torch pickles) and no canonical job: a per-family profile each; the least generic of the closable blockers |
| 3 | `UNTESTED` | PyTorch `pytorch_model.bin` (zip directory + pickle not read) | 97,360 (3.12 %) | 15 sampled repos | `NOT_RUN_NEEDS_PICKLE_DIRECTORY(pytorch)` 97,360 | a bounded range read of the zip central directory and `data.pkl` (no weights, no pickle execution): outside the 10-03 network policy (headers of safetensors/GGUF only) — a policy decision, then `weights::torchzip` already reads it |
| 4 | `FRONTEND` | GGUF files carrying a foreign metadata namespace (`mradermacher.*`, uploader bookkeeping) | 81,883 (2.63 %) | 28 sampled repos | `ARCH_REFUSED(gguf-namespace:mradermacher)` 81,004, `ARCH_REFUSED(gguf-namespace:duynt)` 879 | a data registry of inert provenance namespaces for GGUF metadata (quantizer / uploader keys change no math; a namespace that declares a transform, like `prism.hadamard`, stays refused) |
| 5 | `NEW_KERNEL` | vision-chat / any-to-any (the text stage lowers, the other stage is not credited) | 77,993 (2.50 %) | exact (listing) | `PARTIAL_TASK_ONLY(image-text-to-text)` 64,627, `PARTIAL_TASK_ONLY(any-to-any)` 11,576, `PARTIAL_TASK_ONLY(visual-question-answering)` 761 | tower adapters for the wrappers in the census (Qwen3.5, Gemma 3/4, Qwen3-VL, LLaVA-NeXT, Florence-2, Mistral3, Mllama) + the generative close sizing for a tower (`palw_gen_range_twin_v1` / tile-class sizing) + arming `palw_gen_v1`'s Text profile with image slots; Qwen2/2.5-VL at 196 px already admits under the dormant fences |
| 6 | `NEW_KERNEL` | audio tasks (ASR / TTS / audio classification) | 59,183 (1.90 %) | exact (listing) | `MODALITY_PROFILE_MISSING(automatic-speech-recognition)` 41,607, `MODALITY_PROFILE_MISSING(text-to-speech)` 7,553, `MODALITY_PROFILE_MISSING(audio-classification)` 5,159 | a `JobAudio` binding (FR-23: feature frames) and an audio output kind: the Whisper / wav2vec2 stage programs lower, no protocol job supplies frames |
| 7 | `NEW_KERNEL` | vision heads (classification / detection / segmentation / dense) | 55,581 (1.78 %) | exact (listing) | `MODALITY_PROFILE_MISSING(image-classification)` 30,862, `MODALITY_PROFILE_MISSING(object-detection)` 7,103, `MODALITY_PROFILE_MISSING(image-segmentation)` 4,535 | the CNN / ViT route (`read_cnn`, `read_vision`) exists as stage programs: needs RFC-0003 Image-input job profiles for class labels, boxes and masks (JobImage is bound) and their output kinds |
| 8 | `FRONTEND` | a model class with no declared task (`pipeline_tag` absent) | 54,160 (1.74 %) | exact (listing) | `TASK_UNKNOWN` 54,160 | task inference from the remaining head classes and from `architectures` (inference-v3 reads causal LM, T5, vision-chat, sentence encoders, adapter bases); a head-class → task table row per family |
| 9 | `QUANT_FORMAT` | weights only as onnx / tensorflow / flax / pytorch-adapter / split GGUF | 42,231 (1.35 %) | 2 sampled repos | `FORMAT_UNSUPPORTED(pytorch)` 9,395, `FORMAT_UNSUPPORTED(pytorch-adapter)` 8,221, `FORMAT_UNSUPPORTED(onnx)` 4,959 | an ONNX initializer reader and a TF/Flax checkpoint reader (headers by range), a split-GGUF reader, an `adapter_model.bin` reader |
| 10 | `FRONTEND` | no tokenizer file beside the checkpoint | 29,174 (0.94 %) | 69 sampled repos | `TOKENIZER_MISSING` 29,174 | bind the tokenizer of the pinned base (vocab size checked) |

Notes on the list. (1) Item 4 is a **regression this census found**, cheap to reverse and the single largest gain per line of work: `mradermacher.*` is the biggest GGUF quantizer's bookkeeping; a data registry of inert provenance namespaces
(and a refusal that stays for namespaces that declare a transform) returns up to ≈ 81,883 repositories toward shape-ready (all 17 sampled members it refuses were shape-ready in update 2). (2) Item 3 is a policy question before it is a feature: `pytorch_model.bin` is read by the frontend without running the pickle; the census does not fetch the zip directory.
(3) The encoder–decoder family is just outside the top 10: 16,300 repositories stop at the generative close sizing cap on the encoder stage and 5,813 at `max_position_macs` (an encoder is one position of the pipeline);
only a BART-mini admits, and only at DAA 9,000 where `palw_gen_range_twin_v1` is in force (§4). (4) `NEW_KERNEL` items 1, 2, 5, 6, 7 need versioned job profiles behind a fence; they are the largest *consensus* work, not the cheapest.

## 4. The preflight fix: a pipeline class is judged, or a blocker is named

**Defect (H1, `google/flan-t5-small@0fc9ddf7`, `--depth shape --max-context 512`)**: convert OK (adapter `t5`, two RFC-0003 programs, each admitted alone); register and mine printed `unknown (no program to judge)`; exit code 0.
Cause: a data route (encoder–decoder, vision tower, CNN, diffusers) has no IR program, so `chain::judge` never ran, and `Report::registrable()` treated `unknown` as not blocked.

**Fix** (`misaka-palw-sdk/src/preflight/pipeline.rs`, `misaka-palw-tir-lower/src/model/route.rs`, no consensus change, no fingerprint, params id or schedule id moves):
* the class is declared shape-only (`encdec_pipeline_v1`: the encoder reads the job's `TokenSource::Source`, the decoder is the text stream with the decoder-start id forced; placeholder artifact root and tokenizer id; the source price floor derived by admission per layout) and asked of the node's own gate,
  `palw_gen_registration_preflight_at_v1` (`verify_gen_class_admission_v1`), at the judged height, over a short list of layouts; the first refusal's chain code is the blocker (`CLOSE_SIZE_OVER_CAP`, `ADMISSION_EXCEEDS` with `max_position_macs` / `max_job_step_leaves`, `FENCE_NOT_ARMED(palw_gen_v1)`, …) with the numbers;
* a route this build cannot declare shape-only (a fixed feature-frame source, an encoder alone, a vision / CNN / diffusers route) is `PIPELINE_CLASS_UNDECLARED`, which the census maps to `NOT_RUN_PIPELINE_ADMISSION` (UNTESTED), and the command exits non-zero;
* `Report::registrable()` is false at the shape depth when register or mine is `unknown`; the seat is sized from the admitted stages, or from the stages admitted alone;
* `Options::pipeline_admission` is set by `palw-class preflight`, `misaka model preflight` and the census; `Options::default()` keeps the old behaviour so that `tests/golden/corpus_preflight_v1.json` (hashed by the RFC-0011 audit) is byte-identical — checked: the 22 pinned route entries still match, `0011-audit-coverage.mjs --check` and `--self-test` pass.

**Measured**: flan-t5-small at 512 positions is **refused** at DAA 6,900 and at 9,000: `CLOSE_SIZE_OVER_CAP(generative close sizing work)` — 67,108,865 against the cap 67,108,864 (the censored `cap + 1`), at stage 0's terminal closes (the encoder's stacked cross K/V),
under every layout tried (commit tiles 256 … 16; at 16 lanes `max_job_step_leaves` 5,998,592 > 4,194,304 instead). Exit non-zero. A tiny T5 at 32 positions is admitted, registrable, and shape-ready in the census row.
Regression tests (`misaka-palw-sdk/tests/preflight_pipeline_class.rs`, 7; `preflight::pipeline` unit test, 1): the H1 case judged at both heights (never `unknown`, never registrable with an `unknown` stage), the pinned flan-t5-small verdict, the fence by name below DAA 5,300, the undeclarable route (Whisper), the legacy default,
and the census admit gate reading the same verdict (tiny T5 shape-ready; flan-t5-small `CLOSE_TOO_LARGE`, class `RESOURCE_REFUSED`; an undeclared context is 512, not 8,192).

**What the two rulesets change**: one sampled repository — a BART-mini — is refused at DAA 6,900 by the element-twin close sizing (`CLOSE_TOO_LARGE`) and admitted at 9,000 where `palw_gen_range_twin_v1` is in force: ≈ 451 repositories, the whole difference between A and B (16.64 % and 16.65 %).
The other fences of the 9,000 schedule (`palw_model_court_window`, `palw_receipt_spend_v4`, `palw_audit_1004_v1`) change no sampled verdict. Encoder–decoder repositories that reach the route ≈ 28,265: `CLOSE_TOO_LARGE(generative close sizing work)` 16,300, `ADMISSION_EXCEEDS(max_position_macs)` 5,813, `ADMISSION_EXCEEDS(max_job_step_leaves)` 2,618, `TOKENIZER_MISSING` 2,210, `SHAPE_READY` 451, `NOT_RUN_PIPELINE_ADMISSION` 440.

## 5. Method notes and limits

* **Estimator.** §4d of `hf-census-v1.md`. Exact for the 2,228,617 repositories the listing decides (by the current tree's listing verdict); Horvitz–Thompson over the stratified header sample; the bucket cohorts post-stratified by the listing verdict; Korn–Graubard lower bounds;
  a sampled repository without a row is a failure (nonresponse = 0). Reproducing update 2 from its rows with this code gives its published total (559,985.2) exactly and its bound to 0.001 point.
* **`D_b`'s header-decided part is estimated** (879,175 of 1,605,403); its listing-decided part is exact from the snapshot.
* **Shape-ready is a header-depth statement.** `pack`, `seat`, `final` need weights or a chain; 874 distinct weight sets stand behind 879 shape-ready sampled repositories and 309 distinct spec digests (mirrors are counted per repository; population counts are an upper bound on distinct checkpoints).
  The primary context is `min(declared, 8,192)`; 385,412 of the 519,157 shape-ready repositories declare a wider context and are not full-context.
* **Not re-measured here**: the local-LLM frame of RFC-0002 §II.12 (update 2: 58.43 %, LB 56.29 %) — a different frame with its own Ollama and GGUF supplements; the strict (rights) view, still 0; download-weighted figures are given for continuity only (8.21 %, normal-approximation bound).
* **Vision-chat (`PARTIAL_TASK_ONLY`)**: 150 sampled; the text stage alone admits for 25 (≈ 8,682 repositories, text-only, not credited). The declared vision-chat class under the dormant
  fences (`palw_gen_v1` Text profile with an image slot, FP Job V5) was probed for 81 members: **0 admit**. Re-probed with the range twin that is in force at DAA 9,000 (`PALW_CENSUS_GEN_RANGE_TWIN=1`), the first 29 members of the cohort
  (4 of them Qwen2/2.5-VL, 7B–32B) admit 0; the rerun was stopped there (one FP8 member held the ordered output for 30+ minutes on a machine at load 130). Update 4 measured 12 of 150 Qwen2/2.5-VL members admitted at 196 px on its own tree; **that is not reproduced on this tree
  and was not diagnosed here** (the 7B / 32B refusals are `max_job_macs` at 8,192 and `max_job_step_leaves` at 4,096) — flagged for the Lead. **None is counted as shape-ready or registered.**
* **Listing-depth rows.** `classified-v3` (3,117,871 records, 0 unreadable) was produced by the `a98cc4e31` binary before the pipeline-class commits; the listing path (`classify` → `evaluate` without a store) is identical in `d368707e0` (the commits touch only the fetched-repository paths), so its rows are the tree's. The shape-depth rows carry `tree: d368707e0`.
* **Network**: 0 requests. Every row was judged from the snapshot's saved metadata, configuration and safetensors / GGUF headers.

## 6. Reproduction

```sh
# tree d368707e0 (cov/p4-census); the sample, cohorts and fetched headers are the snapshot's own
python3 -I misaka-palw-sdk/tools/hf_census/compact_rows.py classified.jsonl.gz > p4/compact-classified.jsonl
palw-class census classify --snapshot 2026-10-03T131904Z --tree d368707e0 < <(gzip -dc dall.listing-v2.jsonl.gz) | gzip > p4/classified-v3.jsonl.gz
python3 -I misaka-palw-sdk/tools/hf_census/compact_rows.py p4/classified-v3.jsonl.gz > p4/compact-v3.jsonl
python3 -I misaka-palw-sdk/tools/hf_census/shadow_dirs.py --snapshot . --out p4 --cohort adapters --cohort task_unknown --cohort base_unpinned --cohort partial
palw-class census gates --snapshot 2026-10-03T131904Z --tree d368707e0 --depth shape --height 6900|9000 --dirs p4/<set>.dirs --judge-budget-secs 600
python3 -I misaka-palw-sdk/tools/hf_census/coverage_report.py --snapshot . --compact-base p4/compact-classified.jsonl --compact-new p4/compact-v3.jsonl --rows a=p4/rows/a --rows b=p4/rows/b --seed … --out p4/report
python3 -I misaka-palw-sdk/tools/hf_census/assemble_evidence.py …  &&  node docs/rfc/evidence/0011-census-coverage-check.mjs --check
node docs/rfc/evidence/0011-audit-coverage.mjs --check      # unchanged: the old evidence is not rewritten
```
