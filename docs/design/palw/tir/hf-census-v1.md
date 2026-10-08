# PALW-TIR — the Hugging Face census, method v1 (RFC-0002 §II.10, R6/A6)

*Lane HF, branch `rfc2/hf-majority`. The method behind every dated census report (`hf-census-report-*.md`). It measures; it
claims nothing a row does not show. Code: `misaka_palw_sdk::census` (`palw-class census classify|gates`) and
`misaka-palw-sdk/tools/hf_census/` (the HTTP side, the sample, the statistics).*

## 概要(日本語)

- **単位** は公開 model repo の `repo_id@sha`(1 repo 1 票)。`D_all` は T0 時点で `/api/models` が列挙する全公開 repo(gated・重みなし含む)。
- **6 gate**(source → lower → pack → admit → seat → final)を 1 行 1 repo の JSONL(`misaka.palw.hf-census-row.v1`)に記録。失敗 gate ごとに安定 `blocking` code、その後ろは `NOT_RUN_AFTER_<GATE>`、何も失敗していないのに走らない gate は理由つき `NOT_RUN_*`(PASS を推論しない)。
- **2 つの視点**: strict(rights policy を適用。既定 `none` は何も確認しないので D_rights=0)と technical(rights を外した測定)。headline は strict。
- **段階**: listing だけで決まる失敗(gated・重みなし・task に profile なし・読めない format など)は全 D_all について厳密に数える。残り(undecided)は層化無作為標本で header を取得して preflight を回す。pack は重みが要るので census では常に NOT_RUN、seat/final は chain が要るので NOT_RUN — **registration-ready は census では測れない(0)**。census が測る最も深い段は `shape-ready`(source・lower・admit@shape が PASS)。
- **ネットワーク方針**(ユーザー決定): Hub API・config・index・tokenizer metadata・safetensors header(8 byte + JSON)・GGUF header(metadata と tensor info のみ)だけ。重みは 1 byte も取らない。gated の規約は受諾しない。repo のコードは実行しない。匿名・rate limit 遵守・生レスポンス保存。

## 1. The unit, the snapshot and `D_all`

- **Unit**: one public model repository at one commit, `repo_id@sha`, counted once whatever it publishes.
- **Snapshot** (`enumerate_snapshot.py`): at the UTC second `T0`, walk `GET /api/models?limit=1000&sort=createdAt&direction=1`
  with every `expand[]` the census reads (`sha`, `siblings`, `gated`, `disabled`, `pipeline_tag`, `library_name`, `config`,
  `transformersInfo`, `safetensors`, `gguf`, `cardData`, `baseModels`, `tags`, downloads, likes, dates) and the `Link` cursor
  (keyed by the repository's ObjectId, so the walk is in creation order and stable). Every page is saved as received with its
  URL, status and SHA-256; the walk resumes from its saved cursor.
- **`D_all`** = the listed repositories whose ObjectId precedes `T0`, each at the `sha` its page reported. Not seen: private
  repositories (not enumerable) and repositories deleted or made private between `T0` and their page. A repository updated in
  that interval is pinned at the newer commit (its page's time is recorded).
- **`D_files`** = the `D_all` repositories whose `source` gate passes on the technical view: public, not disabled, not gated, a
  weight file present, an adapter's base pinned in the snapshot, the listed shard sets complete. At the listing depth for every
  repository; the sampled ones add the fetched checks (inventory at the pinned commit, the index's shards present, each file at
  least as long as its header declares).
- **`D_rights`** = the `D_files` repositories whose rights are confirmed under the named policy. The default policy `none`
  confirms nothing (a card's license is evidence, not a decision), so `D_rights` is empty and the strict view stops every
  repository at `source` with `RIGHTS_UNCONFIRMED`. `permissive-card-v0` is a **proposal**: the card license (and every resolved
  base's) is one of a short list of licenses that grant use, modification and redistribution without field-of-use restrictions;
  a report computed under it says so wherever it is used.

## 2. The row and the gates

One `misaka.palw.hf-census-row.v1` per repository (`census::gates::CensusRowV1`): the task, the strata, the selected artifact,
downloads, the rights decision, `technical` and `gates` (six gate results each), `stopped_at`, `shape_ready`,
`shape_ready_at_retry`, `registration_ready` (never true in a header census), the declared context, the preflight summary
(level, spec digest, blockers), the weights' content identity (LFS ids), the ruleset (network, height, context rule, source tree,
task-table digest).

A gate result is `{gate, status: PASS|FAIL|NOT_RUN, blocking, arg, codes, evidence, depth}`. Gates are evaluated in order; after
a `FAIL` every later gate is `NOT_RUN_AFTER_<GATE>`. A gate that cannot be decided although nothing failed before it is
`NOT_RUN` with its reason: `NOT_RUN_NOT_SAMPLED` (only the listing was read), `NOT_RUN_NEEDS_WEIGHTS` (pack; a diffusers route
that lowers from weights), `NOT_RUN_NEEDS_TENSOR_DATA` (a GGUF whose mapping reads a small tensor's data, e.g. Llama-3's
`rope_freqs.weight`, which this census's network policy does not read), `NOT_RUN_PIPELINE_ADMISSION` (an RFC-0003 pipeline
class whose admission cannot be asked from headers: since 2026-10-08 an encoder–decoder is asked — `preflight::pipeline`, §4d — and this code stays for a route this build cannot declare shape-only, `PIPELINE_CLASS_UNDECLARED`, and for embedding / image classes), `NOT_RUN_NEEDS_CHAIN` (seat, final).

### 2.1 The codes

`census::codes` holds the vocabulary and **the one table from the preflight's codes (§II.2.4) to the gate codes**; a test scans
the preflight's source for every `Blocker::new` code and fails on one the table does not map. Where §II.10.3 names a condition
its name is used; otherwise the preflight's published code is kept.

| Gate | Codes, in blocking order |
| --- | --- |
| source | `REPO_UNREACHABLE`, `REPO_DISABLED`, `GATED_ACCESS`, `MISSING_WEIGHTS`, `BASE_UNPINNED(absent\|ambiguous\|not_in_snapshot\|base_gated)`, `FETCH_FAILED(file)`, `HEADER_TOO_LARGE(file)` (> 64 MiB, never read), `HEADER_INVALID(file)`, `WEIGHTS_INCOMPLETE(file)`, `RIGHTS_UNCONFIRMED(policy)` |
| lower | `TASK_UNKNOWN`, `MODALITY_PROFILE_MISSING(task)`, `PARTIAL_TASK_ONLY(task)`, `FORMAT_UNSUPPORTED(format)`, `ADAPTER_UNCHECKED`, `CONFIG_MISSING`, `CONFIG_INVALID`, `CUSTOM_CODE_UNMODELLED(module)` (← `REMOTE_CODE`), `FEATURE_C(feature)` (← `ARCH_NEEDS_FEATURE`, `ARCH_NEEDS_PRIMITIVE`), `ARCH_REFUSED`, `CONFIG_KEY_UNREAD(key)`, `QUANT_DESCRIPTOR_MISSING(scheme/id)` (← `QUANT_NO_DESCRIPTOR`, `QUANT_KNOWN_UNDESCRIBED`), `QUANT_REFUSED`, `TENSOR_MISSING`, `TENSOR_SHAPE`, `TOKENIZER_MISSING`, `TASK_MISMATCH`, `PREFLIGHT_PANIC` |
| pack | `PACK_NOT_VERIFIED` (full depth only); otherwise `NOT_RUN_NEEDS_WEIGHTS` |
| admit | `FENCE_NOT_ARMED(name)`, `CONTEXT_BOUND` (← `CANONICAL_JOB_OUT_OF_BOUNDS`), `COURT_WINDOW_EXCEEDED`, `DA_LADDER_EXCEEDED`, `COURT_BUDGET` (← `COURT_COST_OVER_CEILING`), `CLOSE_TOO_LARGE` (← `CLOSE_SIZE_OVER_CAP`), `ADMISSION_EXCEEDS`, `ADMISSION_REFUSED` (with the on-chain `PalwClassAdmissionError` code in its evidence), `ARTIFACT_ROOT_KNOWN` |
| seat | `SEAT_MEMORY` (← `SEAT_MEMORY_SHORT`: no tier of the network holds the replay), `READY_SEATS_SHORT`, `INDEPENDENCE_SHORT`; otherwise `NOT_RUN_NEEDS_CHAIN` |
| final | `NOT_RUN_NEEDS_CHAIN` |

**One deviation from §II.10.3's table**, said here: `QUANT_DESCRIPTOR_MISSING` is listed there under `pack`. The census meets it
at `lower`, where the shape-only lowering stops because the stored weights cannot be bound to the program, so it is reported at
`lower`. `pack` keeps the codes that need the weights.

### 2.2 The declared task and its profile (`census::tasks`)

"The whole declared task must have a canonical job": the task is the card's `pipeline_tag`, else the Hub's
`transformersInfo.pipeline_tag`, else — only — a causal-LM architecture name (`…ForCausalLM`, `…LMHeadModel`) or a PEFT
`CAUSAL_LM` task type; nothing else is inferred (`TASK_UNKNOWN`). The table maps each tag to the profile that would carry it in
this build: `text-generation` → the RFC-0002 decoder class (`palw_tir_v1`); text2text/translation/summarization → RFC-0003's text
pipeline; feature-extraction/sentence-similarity/image-feature-extraction → the embedding profile; text-to-image → the image
profile (all three `palw_gen_v1`); image-text-to-text and the other image/audio/video-plus-text chat tasks → `PARTIAL_TASK_ONLY`
(the preflight lowers the text stage only); every other task (classification, detection, segmentation, masked LM, audio,
video, RL, tabular) → no profile (`MODALITY_PROFILE_MISSING`). The table is versioned and its digest is in every row.

### 2.3 The selected artifact (`census::listing::select`)

One artifact per repository, by a published rule over the file list: a diffusers pipeline (`model_index.json`); an adapter
(`adapter_config.json` with no full checkpoint of its own, or a safetensors file the Hub relates to its base as an adapter);
the transformers checkpoint at the root (`config.json` with `model.safetensors.index.json` or `model.safetensors`); a diffusers
component at the root; the shallowest subdirectory holding such a checkpoint; other root safetensors; the best-ranked **single**
GGUF (Q8_0, Q6_K, Q5_K_M, … the floats last; never an `mmproj`; a split set only when no single file exists); weights only in
other formats (`FORMAT_UNSUPPORTED`); none (`MISSING_WEIGHTS`). Safetensors outrank a GGUF in the same repository.

### 2.4 The declared context (`census::gates::ContextRule`)

A class is a model at a context. The primary context is `min(the model's declared maximum positions, 8,192)` (testnet-12's
classes are 8k; `assumed` 8,192 when the configuration declares none). Admission at 8,192 costs ~30× the CPU of 2,048 (Qwen2.5-
0.5B: 44.5 s against 1.5 s, release build), so the census judges **2,048 first** and the primary only when 2,048 admits — or
refuses on a code a wider context could change. A refusal at 2,048 on a code in `IMPLIED_AT_WIDER_CONTEXT` is recorded as the
primary's `FAIL`, "refused at 2,048 on limits that only grow with the context; not run at the primary" — never a pass by
inference. The argument, per code: `COURT_BUDGET` (per-tile MACs and cone work depend on the tiles, and the layout search offers
the same power-of-two tiles at every context at least the tile; the dissection's root claim and rounds only grow with the
history), `COURT_WINDOW_EXCEEDED` (the dissection's duration is non-decreasing in the context), `CLOSE_TOO_LARGE` (a carried
close spans a tile and the history it reads), `DA_LADDER_EXCEEDED` (the longest job's step leaves are proportional to its
positions), `FENCE_NOT_ARMED`/`ARTIFACT_ROOT_KNOWN` (context-independent). `CONTEXT_BOUND` is excluded (both contexts are inside
the canonical job's bounds) and its primary is run. The implication is **checked**: a seeded subset of implied rows is judged at
the primary for real (`verify_implied.py`) and the agreement is published. A class admitted at 2,048 only is its own stratum
(`shape_ready_at_retry`), never merged into the 8k headline.

The chain's judgment of identical programs is shared (`preflight::JudgeCache`, keyed by the program bytes, the artifact and leaf
estimates and every option): fine-tunes of one configuration cost one judgment.

## 3. Fetching (`fetch.py`)

Per sampled repository: `GET /api/models/<id>/revision/<sha>?blobs=true` (the inventory: sizes, git blob ids, LFS SHA-256); the
plan's metadata files whole (≤ 16 MiB; the resolver's redirect followed by one bounded range); safetensors headers by two exact
ranges (8 bytes, then the JSON's length; > 64 MiB refused by name); GGUF headers by ranges that **never pass the header's end**:
each request asks only for bytes the structure parsed so far proves exist (every remaining key/value takes at least 13 bytes,
every tensor info 32, every array element its minimum), so not one byte of the data section is fetched. A GGUF header is stored
at its length with its large arrays' contents zeroed (nothing the preflight reads is in them; the SHA-256 of the bytes as fetched
is recorded). A repository is written to a temporary directory and renamed when complete. Rate limits: the Hub's own headers
(`ratelimit: "api";r=…;t=…`, anonymous quotas 500 API and 3,000 resolver requests per 300 s); the census uses at most 50–80 % of
each, backs off on 429/5xx, sends no token (a token on this machine is not read: `trust_env=False`, no `huggingface_hub`).

## 4. The sample and the estimates (`sample.py`, `report.py`)

- **Decided** repositories (a gate `FAIL`s on the listing alone) are counted exactly.
- **Undecided** repositories are sampled: strata task group × format × size band; a certainty stratum of the most downloaded
  undecided repositories (inclusion probability 1); in every other stratum a simple random sample without replacement,
  proportional with a floor per stratum. The draw is reproducible from the published seed alone (each repository's key is
  BLAKE2b-64(seed ‖ 0 ‖ repo_id); a stratum takes its n_h smallest keys), and a larger sample extends it (nested).
- **Estimates**: Horvitz–Thompson over the strata; a sampled repository without a row counts as a failure (nonresponse is never
  dropped). One-sided 95 % lower bounds by Korn–Graubard (Clopper–Pearson at the effective sample size p̂(1−p̂)/v̂).
  Download-weighted shares use the denominator's exact download total and a normal-approximation bound, said so.

## 4b. Two-phase shape estimates, bucket cohorts and re-classification (added 2026-10-04)

- **Phase 2** is a random-order prefix of the eligible sampled repositories (`select_shape.py`, BLAKE2b-64(seed ‖ 0 ‖ "shape" ‖ 0 ‖
  repo)); `estimate.TwoPhase` gives the double-sampling estimate and variance, so any judged prefix is an unbiased estimate.
- **A generic change is measured on the bucket it targets** (§II.10.4.5): `sample_cohort.py` draws a seeded SRS (n = 150) of the
  repositories the baseline stopped at one listing-depth code, re-plans them with the build under test (from `dall.listing-v2` when
  the change is in base resolution), fetches them (metadata and headers only) and judges them at the shape depth. `report.py
  --cohort NAME:ROWS` replaces the bucket's exact baseline count by the cohort's expansion (N/n per member, a stratum of its own).
- **`--classified-new`**: every other listing-decided repository takes the build under test's listing verdict; one the new build
  leaves undecided outside every cohort is counted as not passing (`UNDECIDED_UNMEASURED`), never estimated.
- **Base resolution v2** (`resolve_renames.py`, `apply_renames.py`): a base reference the snapshot does not hold is asked of the
  Hub once (no redirect followed); a renamed one is resolved to the repository it redirects to, at that repository's snapshot sha.
- **Task inference v2** (`census::listing::task_of`): a missing task is taken from a transformers head class
  (`…ForSequenceClassification` → text-classification, `T5ForConditionalGeneration` → text2text, the vision-chat
  `…ForConditionalGeneration` list → image-text-to-text, sentence encoders `BertModel`… → feature-extraction) or a GGUF
  `general.architecture` (llama.cpp's decoder names → text-generation). Never the code, the name or the card text. Its version
  enters the task table's digest.

## 4c. Preflight corrections found by the census (2026-10-04)

1. **The layout search judged at the IR flag day's height**, not at the preflight's: on testnet-12 it ran without
   `palw_tir_fence2` (DAA 3,600), sized closes with the element twin (2^26-step cap hit by every hidden-4096 decoder) and reported the
   widest layout's refusal. The H4 report's `COURT_BUDGET` bucket (3.9 % of `D_all`, 20.9 % of the frame) was this artefact.
2. **The lowered history window** was the program's history bound; a class declared at C positions is now lowered with
   `max_window = C` (`ADMISSION_EXCEEDS(max_state_bytes|max_position_macs)` was this).
3. **The DA-answerability twin** read the flag day's height too. All three are off-chain (SDK) changes; no consensus rule changed.

## 4d. The coverage census of 2026-10-08: primary blockers in the Model Onboarding buckets, two denominators (COV-P4)

`tools/hf_census/{buckets,compact_rows,shadow_dirs,coverage_report,render_coverage,assemble_evidence}.py`; report `hf-census-report-2026-10-08.md`,
evidence `docs/rfc/evidence/0011-hf-census-coverage-2026-10-08.json` (arithmetic checked by `0011-census-coverage-check.mjs`).

* **One primary blocker per repository** (RFC-0011 §16.4), in the user's buckets: `MISSING_WEIGHTS`, `GATED`, `ADAPTER_BASE_MISSING`, `FRONTEND`,
  `NEW_KERNEL`, `QUANT_FORMAT`, `RESOURCE`, `UNTESTED`, and a ninth, `NO_MODEL_TASK` (no declared task and no configuration naming a model class,
  or no readable configuration). `buckets.py` is the one table (census code → bucket → the earlier analysis's dimension: `external`,
  `feature-only`, `frontend-only`, `protocol-envelope`, `untested`); a `NOT_RUN_*` code is `UNTESTED` and is never a pass. External = the first
  three and `NO_MODEL_TASK`; software-closable = the other five (`NEW_KERNEL` needs a consensus flag day, `RESOURCE` a rule or a new route).
* **Denominator (b), `D_b`** = `D_all` minus the external buckets: weights present and complete, public and not gated, an adapter's base pinned in
  the snapshot, a model task or a configuration naming a model class. The listing-decided part is exact from the snapshot; the part only a header
  decides (an unreachable repository, an unreadable header, an incomplete shard, a missing configuration) is estimated from the sample. The rate over
  `D_b` is a ratio estimator (linearised variance, Korn–Graubard bound). The ceiling of any rate over `D_all` is `|D_b| / |D_all|`.
* **The sample is the method's, re-judged.** The sampled repositories' fetched directories are re-read with the current listing record
  (`shadow_dirs.py`: symlinks plus a fresh `listing.json`; the fetched directories are never written) and judged at the shape depth, at each ruleset,
  by `palw-class census gates`. No header is fetched.
* **One refinement of the estimator, stated.** The baseline's three bucket cohorts (n = 150 each) estimate their frames; here a frame member that
  the current tree's *listing* already decides is counted exactly, and the cohort estimates only the members the listing leaves undecided,
  post-stratified by the listing verdict's cell (`NOT_RUN_NOT_SAMPLED` / `NOT_RUN_NEEDS_PICKLE_DIRECTORY`). A cell with frame members and no cohort
  member is counted as not passing (`NOT_SAMPLED_CELL`). The plain expansion of §4b is computed beside it (`plain_method`); on update 2's rows the two
  agree within one standard error.
* **An encoder–decoder with no declared positions is declared at 512** (`ENCDEC_ASSUMED_CONTEXT`), not the IR decoders' assumed 8,192, where its
  encoder's attention scores exceed a node's 2^28 elements. A pipeline class is judged by the generative admission (`preflight::pipeline`), not left
  `NOT_RUN_PIPELINE_ADMISSION`; a route this build cannot declare shape-only keeps that code.
* **Rulesets.** `--height 6900` (testnet-12 today) and `--height 9000` (the first height at which every scheduled fence is in force:
  `palw_model_court_window`, `palw_gen_range_twin_v1`, `palw_receipt_spend_v4`, `palw_audit_1004_v1`).

## 5. Feasibility of the RFC's "header preflight for every `D_all` entry"

§II.10.2 asks for the header-only preflight of **every** `D_all` entry with readable metadata. Measured on 2026-10-03: the
listing of all 3,117,871 repositories took 3,118 API requests (64 minutes at half the anonymous quota). A header fetch costs one
API request (the inventory at the pinned commit) and ~2–10 resolver requests per repository; the anonymous quotas (500 API, 3,000
resolver per 300 s) bound that at ~4–6 k repositories an hour, about 4–5 days for the 561,568 undecided repositories — feasible as a
background job, not as an evening's run. The preflight itself is the binding cost: the shape-depth judgment runs admission v10
many times per class (the layout search) — 1.5 CPU-seconds for a 0.5B at 2,048 positions, 44.5 s at 8,192, and minutes for a 7B
refused at 2,048 — so the full undecided population is days of CPU even with the judgment shared among identical programs. The
feasible design is therefore: the **exact listing-depth census of all of `D_all`** (every listing-decided failure counted, no
sampling error there), plus the **stratified header sample** of the undecided part, extended (nested) as time allows.
