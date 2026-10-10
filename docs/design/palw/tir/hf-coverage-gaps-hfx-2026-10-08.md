# PALW-TIR — HF coverage lane HFX: what closed, and the precise gaps that did not (2026-10-08)

*Lane HFX, branch `hf/x-blockers`. Companion to `task-heads-profile-v1.md`. Census basis: `hf-census-report-2026-10-08.md` (COV-P4),
snapshot `2026-10-03T131904Z`, re-measured offline in `snapshots/…/hfx/` (no Hub request). Columns are never merged: shape-ready
(a header verdict), text-only, short-context, synthetic beacon, unit-test PASS, dormant registration, full-task Active/Final.*

## 1. Closed in this lane (generic, by name)

| # | change | kind | census family (2026-10-08) |
| --- | --- | --- | --- |
| 1 | `GGUF_INERT_PROVENANCE_V1`: a data registry of inert provenance namespaces (`mradermacher.*`, `duynt.*`), key-listed, scalar-only; `prism`, `comfy`, `adapter`, `dspark`, `yvex` held never-inert by a test | frontend (data) | `ARCH_REFUSED(gguf-namespace:*)` ≈ 81,883 |
| 2a | `OUTPUT_TOKEN_LOGITS_V1`: token classification and extractive QA over a bidirectional encoder (BERT, RoBERTa / XLM-R, DistilBERT adapters) | lowering + data | `token-classification` 35,316, `question-answering` |
| 2b | the `Head` task profile (`palw_task_heads_v1`, dormant and refused when armed by this build; profile tag 6, offers variant 3, body variant 2), its A-2 predicate and decode rule | consensus (dormant) | every head task: `PROFILE_NOT_ARMED` (was `MODALITY_PROFILE_MISSING`) |
| 2c | the bidirectional encoder's class declared shape-only and judged by the generative admission (an embedding as `Embedding`, a head as `Head`, hypothetically); a span head over a token-type table refused by name (`ARCH_NEEDS_FEATURE(ENC_PAIR_SEGMENTS_V1)`) | preflight / census | `NOT_RUN_PIPELINE_ADMISSION` of every embedding repository |
| 5 | task inference v4: transformers 5.17's auto-model tables (814 classes listed under exactly one task), llama.cpp @030ebb55's converter registrations (31 GGUF architectures); a GGUF of a llama.cpp model architecture is a model class (never `no-config`) | census (data) | `TASK_UNKNOWN` 54,160; the D_complete leak |
| — | a causal LM tagged summarization / translation / text2text is served by the text decoder class (the Lead's decision) | census | GenText-tagged decoders |
| 6 | `ENC_PAIR_SEGMENTS_V1`: BERT-type segment ids (type 1 after the first separator) computed IN the program from the job's ids (`lower::bidir::BidirExtras::pair_sep`); a span head over a token-type table is declared, not refused | lowering | `question-answering` over BERT / ALBERT (3,977+ repositories) |
| 7 | `OutputSpec::MaskedLm`: a `…ForMaskedLM` head (HEAD_TRANSFORM_V1 over every row, the tied vocabulary projection) for BERT, RoBERTa / XLM-R / CamemBERT and DistilBERT, chosen by the TASK (`fill-mask`; `hf_schema::masked_lm_adapter_for`), never by architecture alone (the same checkpoint is also a sentence embedder) | lowering + data | `fill-mask`: BertForMaskedLM 7,209, RobertaForMaskedLM 4,444, DistilBertForMaskedLM 2,769, CamembertForMaskedLM 1,433, XLMRobertaForMaskedLM 1,004 repositories (listing, by architecture) |
| 8 | image classifiers: `VisionOut::Classify` (ViT, `vit-imgcls`), `CnnHead` (ResNet, ConvNeXt, MobileNetV1 / V2: the classifier is no longer ignored), the shape-only image class (`model::route::lower_image_class_shape_v1`) judged as the `Head` profile's IMAGE task and its K2-TIR-v3 route reported | lowering + preflight + data | `image-classification` (ViT 13,561, ResNet 2,012, ConvNeXt 393, MobileNet ≈ 170 of 29,479) |
| 9 | task heads for ALBERT, DeBERTa-v2 / v3 (the ContextPooler) and CamemBERT (`albert-*`, `deberta-v2-*`, the RoBERTa head's match lists) | data | `text-classification` / `token-classification` / `question-answering` / zero-shot NLI (DeBERTa NLI is the largest family of zero-shot) |
| 10 | the legacy `LayerNorm.gamma` / `.beta` spelling UNDER the checkpoint's prefix (`bert.embeddings.LayerNorm.gamma`): the rename was tried on the un-prefixed name only, so every old-spelling task checkpoint was `TENSOR_MISSING(embed.norm.gain)` | frontend | 7.4 % of the head-task frame in the head cohort |
| 11 | a module BUFFER (`embeddings.position_ids`, an I64 range) is no weight of the class: it counted as "a bound tensor stored as a type no descriptor claims" (`QUANT_NO_DESCRIPTOR(safetensors/I64)` on MiniLM, MPNet, SapBERT … — 28 sampled rows) | preflight | sentence-transformers |
| 12 | `Options::base_tokenizer` / `--tokenizer-bases`: a class with no tokenizer file binds its pinned base's, accepted only for the same declared vocabulary | preflight + census | `TOKENIZER_MISSING` — measured small (below) |
| 13 | the bidirectional encoder's class judged under **K2-TIR-v5** beside the generative verdict (`preflight::kernel::encoder_route_of`; lane K2S `k2-real-scale.md` §12) | preflight | every encoder / head class the generative route refuses by position-sized bounds |

## 2. Vision (item 3): what exists, and the gap

**Exists**: the ViT and CNN routes (`hf_schema::read_vision`, `read_cnn`; `lower::vision`, `lower::cnn`) lower the backbone's stage
programs and admit them alone; `JobImage` (binding tag 6) binds an image slot; the `Head` profile defines `IMAGE`, `DETECTION` and
`SEGMENTATION` tasks with their output headers (`[1, labels]`, `[queries, labels + 1 + 4]`, `[h·w, labels]`).

**Closed for classifiers (2026-10-10, row 8 above):** a ViT's class row through the final layernorm and `classifier`, a CNN's pooled vector through its linear `classifier` (`classifier.1` of ResNet's `Sequential`) are lowered, checked against transformers' logits (float 1e-7, integer cosine 0.976–0.99998), declared as a one-stage `JobImage` pipeline and judged. DETR's set-prediction head, SegFormer's all-MLP decoder and the backbones' Embedding-profile class are not.

**Gap 1 (as of 2026-10-08) — the heads are not lowered.** `lower::cnn` lists a checkpoint's classifier among the tensors it never reads by design, and
the ViT route stops at the encoder's rows. Needed, generically: (a) a pooled image head — the CNN's global average pool (a `ReduceSum`
over the spatial axes and a `Div`) or ViT's `[CLS]` row / mean, the final norm, then `classifier` to `labels` logits (one adapter row
per family: ResNet `classifier.1`, ConvNeXt `classifier` after `layernorm`, ViT `classifier` after `layernorm`, MobileNet
`classifier`); (b) DETR's set-prediction head (`class_labels_classifier` + the 3-layer `bbox_predictor` MLP with a sigmoid table) over
the decoder's query rows — needs the DETR transformer decoder over a `Fixed` query axis, which no route lowers today; (c) a semantic
segmentation head (SegFormer's all-MLP decoder: per-stage linear, a fixed-ratio resize as a pinned `MatMul`, concat, fuse) — the
resize is RFC-0003 §II.4's pinned resampling matrices. (a) is the bulk (`image-classification` 30,862 of the 55,581); (b) and (c)
are new decoders.

**Gap 2 (as of 2026-10-08; closed for classifiers 2026-10-10) — a vision class is not declarable shape-only.** `preflight::model` routes a vision / CNN class to
`PIPELINE_CLASS_UNDECLARED` ("lowers from the checkpoint's weights (calibration)"). As item 2c showed for the bidirectional encoder,
a program's STRUCTURE does not depend on the weights or the calibration (they fill params at materialisation), so the same
shape-only declaration applies: `JobImage` at the processor's declared size, the backbone, the head. Not built here.

**The vision-chat tower adapter with generative close sizing (77,993 `PARTIAL_TASK_ONLY`)** is not built in this lane. The census
already probes the declared VLM class under the dormant fences (`census::vlm`); COV-P4 found 0 of 81 admit and flagged that update 4's
12 / 150 Qwen2/2.5-VL admissions are not reproduced on the current tree (`max_job_macs` at 8,192, `max_job_step_leaves` at 4,096). The
next step is the diagnosis of that regression on the range twin (`palw_gen_range_twin_v1`), then one tower adapter per wrapper family
(`find_vision_for` / `tower_of` data rows). Nothing is credited: the text stage alone stays the separately scoped text-only column.

## 3. Audio (item 4): a design gap, by decision

`JobAudio` does not fit this lane: it is a pipeline-format change and a court-input change, not an adapter. Precisely:

1. **The binding.** `misaka_palw_tir::pipeline::Binding` gains `JobAudio { index: u8 }` (tag 7, appended): PCM `i16` `[frames, channels]`
   at the class's declared rate, into an `External` input whose interval contains `[−32768, 32767]`. The pipeline's normal form
   (`validate_pipeline`, NF-P10's "every bound input is read") and its strict decode must learn the tag; a pipeline carrying it is
   bytes the int-12 build cannot decode — the same A-2 class as the `Head` variants, so it needs its own fence and the same
   object-level predicate (`palw_object_needs_*`) at isolation, the acceptance walk, the fold and both chunk assemblies.
2. **The job.** `AudioInputRefV1 { input_root, sample_rate, channels, frames }` (RFC-0003 §II.5) beside `ImageInputRefV1`, the root being
   §I.3.2's construction over the PCM bytes; offers declare one slot (rate, channels, frames, tile length). Decoding and resampling are
   the gateway's, declared.
3. **The court.** Admission opens an audio input at 2 bytes a lane; a court reads a lane only from a tile proven under the job's
   `input_root` (PALW-TIR-48's image rule, for `i16`).
4. **The front end as integer TIR** (FR-23): framing as a `Gather` with a pinned index table, the Hann window and the DFT as `MatMul`s
   with pinned tables, power (`Mul`/`Add`), the mel filterbank (`MatMul`), `IntLn`, Whisper's clamp-and-scale. Whisper's encoder already
   reads the frames (`EMBED_FRAMES_CONV1D_V1`, `encdec_frames_encoder_v2`); wav2vec2's convolutional feature encoder reads raw PCM and
   needs only the binding.
5. **Output kinds.** ASR emits text (the `Text` profile's stream); audio classification is the `Head` profile's `SEQUENCE` over an
   audio input (`PalwGenEmbeddingInputV1` would gain an `Audio` arm — another appended variant); TTS needs `PcmI16` output, already a kind.

Census: `automatic-speech-recognition` 41,607, `text-to-speech` 7,553, `audio-classification` 5,159 stay `MODALITY_PROFILE_MISSING`.

## 4. Formats (item 6)

| form | repositories (listing, exact) | state |
| --- | ---: | --- |
| `pytorch_model.bin` only | 97,360 (`NOT_RUN_NEEDS_PICKLE_DIRECTORY`, UNTESTED) | **blocked on a user decision** (below) |
| `pytorch` (other) / `pytorch-adapter` | 9,395 / 8,221 | the frontend reads a plain torch.save; the census's network policy is the gap |
| `onnx` | 4,959 | gap: an ONNX initializer reader (protobuf `graph.initializer` names, dims, data type; external-data files) mapping initializer names to the configuration's tensors |
| `gguf-split` | 3,765 | gap: a split reader merging the shards' tensor tables (shard 1 holds the metadata; `split.count` / `split.no`), header-only by range per shard |
| `tensorflow` / `flax` | 2,220 / 648 | gap: an `.h5` (HDF5 superblock + B-tree of datasets) and a `msgpack` (Flax) header reader; both store names and shapes before the data |
| `openvino`, `numpy`, `zip`, `pickle` | 1,321 / 564 / 972 / 106 | not a model format this build lowers (a runtime's IR, arrays, archives) |
| no tokenizer beside the checkpoint | 29,174 (`TOKENIZER_MISSING`) | gap: bind the pinned base's tokenizer when the base resolves in the snapshot and the vocabulary sizes agree |

**`pytorch_model.bin`: the policy needed** (the user's decision, not implemented here). The offline parser exists and runs nothing:
`misaka_palw_tir_lower::weights::torchzip::read_header` reads the zip's end-of-central-directory record, the central directory, the
`byteorder` member and `data.pkl`, interprets the pickle with an allow-listed, non-executing machine (`interpret_pickle`, bounded by
`Limits`), and reports names, dtypes and shapes without reading a tensor. The census would need, per repository: (1) one range read of
the file's last 64 KiB (the EOCD and, for most checkpoints, the whole central directory); (2) one range read of `data.pkl` (its offset
and size are in the directory; typically 10–500 KiB); (3) 16 bytes of `byteorder`; (4) per storage member, its 30-byte local header
plus name — or none, if the census takes the tensor shapes from the pickle alone and never asks where the data starts. That is
anonymous Hub resolver traffic over a file the 10-03 policy does not list (it names safetensors and GGUF headers only). Asked of the
user: may the census range-read `pytorch_model.bin`'s zip directory and `data.pkl` (never tensor data, never executing anything)?

## 5. Remaining for the `Head` profile to be a running task (not only a registrable one)

* the declaration tool (`misaka-palw-sdk::gen_class`) builds `Head` classes from a checkpoint (it builds `Embedding` ones today);
* the free-prompt door for a `Head` job: the header-context half refuses one below the fence (built); past it the isolation door
  already admits the body (it decodes) — no further threading is needed;
* the worker (`misaka-palw-base0::gen_worker`) and the gateway (`misaka-palw-gateway::tensor`) run and submit a `Head` job as they do an
  embedding job (same output kind); the node's court treats a `Head` class as a tensor class (built);
* ~~the masked-LM head~~ **built 2026-10-10 as `[L, vocab]` over every row** (no third lifted input: the row at the masked position is what
  a user reads). The profile's own MASKED_LM task (`[1, vocab]` at a job-scalar position) is a different class form this lowering does not
  build; the generative judge asks the real shape as the `TOKEN` task (labels = the vocabulary);
* ~~BERT-type pair segments~~ **built 2026-10-10** for `SPAN_QA`; a `PAIR` sequence class (zero-shot NLI, a cross-encoder) over a model
  with token types is still a `SEQUENCE` class over the concatenated ids — the same function only without token types;
* the vision heads: classifiers **built 2026-10-10** (§2); detection and segmentation heads, and Swin / BEiT / DeiT / SigLIP / ConvNeXtV2 /
  EfficientNet / MobileViT classifiers are not;
* **reward is not claimed**: a `Head` class's verification coverage, public material, Panel-independent prosecution (G14) and
  collateral are this profile's PRINCIPLES §6 conditions and are not shown by this lane. The fence is refused when armed until the
  full-activation release. (2026-10-10, ADR-0177: G14 is conditional on the verifier having acquired the registered model, and the
  effective detection probability can be 0 for a model nobody serves; ADR-0176: any reward would draw on `palw_bond_budget_v1`,
  which does not exist yet.)

## 6. Re-measurement (RFC-0011 §18's three numbers), 2026-10-09

Offline, the snapshot `2026-10-03T131904Z`'s saved listing and headers, **no Hub request**; the same sample (n = 1,773) and the three
bucket cohorts, post-stratified as in COV-P4 (`coverage_report.py`). m3 judged every row at tree `1c4091a1d` with fresh judge
caches; m4 re-judged only the six sampled 8,192-position encoders at `2f0c42240` (below) — every other row is m3's. Machine-readable
twin: `docs/rfc/evidence/0011-hf-census-hfx-2026-10-09.json` (the m4 coverage report and the reclassification rules' effects).

| | m1 (tree `cf7b81424`, GGUF inert provenance) | **m3 / m4 (tree `1c4091a1d`; the 8k-encoder cells at `2f0c42240`)** |
| --- | ---: | ---: |
| 1. shape-ready / `D_all` (3,117,871), one-sided 95 % LB | 19.27 % (LB 18.56 %), 600,913 | **19.18 % (LB 18.43 %), 598,000** |
| … over `D_b` = `D_complete` (est.) | 37.43 % (LB 36.04 %) of 1,605,403 | 37.23 % (LB 35.76 %) of 1,606,334 |
| 2. registered full task, full advertised context / `D_complete` | 0 (LB 0.00 %) | **0 (LB 0.00 %)** |
| 3. mined and a funded `Final` / `D_complete` | 0 (LB 0.00 %) | **0 (LB 0.00 %)** |
| ruleset A (DAA 6,900) and B (DAA 9,000) | 19.26 % / 19.27 % | 19.18 % / 19.18 % |

Numbers 2 and 3 are counts of evidence (H1, `onboard/h1-hf-closed-loop`): no full-task, full-context registration exists; the one
accepted registration (Qwen2.5-0.5B-Instruct at 512 of its 32,768 positions, private devnet, dormant) is not counted. Shape-ready is a
header verdict and never enters them. Number 1's denominator is `D_all`; the 90 % bar is on number 2.

**Why m3 is 0.09 point below m1 although no sampled verdict got worse** (decomposed by re-running the estimator with one input
swapped at a time, ruleset B):

* the gates (`1c4091a1d`'s binary on the same frame): **+1,001** — 24 sampled decoder LMs tagged translation / summarization are now
  judged by the text decoder class and pass (the Lead's decision), one BART-mini no longer passes at DAA 9,000 (below);
* the listing (task inference v4 on the same rows): **−3,915** — repositories a cohort cell used to credit at the cell's average pass
  rate are now decided exactly by the listing (a transformers auto-model class with one task: mostly head tasks, `PROFILE_NOT_ARMED`),
  so the estimate is the more exact one, not a regression.

**m1 read 715 cached judgments from an earlier tree.** Its judge caches were seeded from runs before the 2026-10-08 withdrawal of
testnet-12's DAA 9,000 schedule, and the cache key named the network and the height but not the ruleset: the BART-mini member's
"admitted at DAA 9,000" (the whole A/B difference, ≈ 451 repositories) is such a stale judgment. m3 used fresh caches, and on today's
tree A and B are the same ruleset for every sampled class. The cache key now carries the judged network's `consensus_params_id` and
`consensus_schedule_id` (`preflight::ruleset_tag`), so a cache from another schedule can no longer answer.

**The reclassification rules, each separately** (exact, from the listing; `reclass_effects.py`):

| rule | repositories moved | into `D_complete` |
| --- | ---: | ---: |
| `head-profile` (`MODALITY_PROFILE_MISSING` → `PROFILE_NOT_ARMED`, both `NEW_KERNEL`) | 266,162 | 0 |
| `inferred:hf-automap` (transformers 5.17.0's auto-model tables) | 4,909 | 0 |
| `inferred:gguf-converter` (llama.cpp @030ebb55's converter) | 607 | 605 |
| `gguf-model-class` (a GGUF of a llama.cpp model architecture stays in `D_complete` as `TASK_UNKNOWN`) | 326 | 326 |

**What the encoder route measured (2c).** `NOT_RUN_PIPELINE_ADMISSION` fell from 23,159 to 538 repositories: a bidirectional
encoder's class is now judged instead of left untested. **No sampled encoder class is admitted.** Of the embedding classes that reach
admission (40 fetched rows, sample and cohorts, ruleset B): 24 are refused by the generative close sizing cap (`CLOSE_TOO_LARGE`,
67,108,865 against 2^26 — the censored `cap + 1`), 15 by `max_position_macs` (the whole padded sequence is ONE position of the
pipeline: e.g. 1.68·10^11 > 2^37), and even `stsb-bert-tiny` by the generative tile MACs (2^25 against 2^24).

**The 8,192-position encoders.** Six sampled encoders (BGE-M3 / XLM-R lineage) do not lower: the attention scores exceed the IR's
2^28-element shape cap. m3 booked them `FRONTEND / ARCH_REFUSED`; they are a size limit of the declared context, so the preflight's encoder branch now names it `SHAPE_OVER_CAP` (the census's `CONTEXT_BOUND`,
`RESOURCE`; test `census::codes::tests::an_encoder_over_the_element_cap_is_a_resource_refusal`). m4 re-judged only those six rows at
both rulesets: all six are now `lower / CONTEXT_BOUND(2^28 elements)`, class `RESOURCE_REFUSED`; the estimate moves **2,199
repositories from `FRONTEND / ARCH_REFUSED` (35,520 → 33,321) to `RESOURCE / CONTEXT_BOUND`** (FRONTEND 8.15 %, RESOURCE 1.76 % of
`D_all`); shape-ready is unchanged (598,000), and the report's self-check finds 0 class disagreements and 0 unmapped codes.

**The heads.** The `Head` profile's hypothetical admission
(the fence armed for the judgment, testnet-12's proposed head ceilings = the Embedding ceilings) refused all 5 heads that were
fetched and lowered (4 BERT / RoBERTa / DistilBERT classifiers of the task-unknown cohort: `CLOSE_SIZE_OVER_CAP`; `BAAI/bge-reranker-large`:
`ADMISSION_REFUSED(GEN_CLASS_REFUSED)`). Head-task repositories with a declared task are decided by the listing and are not in the
sampling frame, so 5 is all the census fetched — a small count, but the refusal is a size cap every 512-position encoder meets.

**Under testnet-12's ceilings, arming `palw_task_heads_v1` admits no encoder or head class.** This is a **court-cost question**, not
a frontend one: a bidirectional encoder's class is ONE pipeline position over the whole padded sequence, so its close, its position
MACs and its tile MACs all scale with the sequence and exceed the generative ceilings the head fence copies (the Embedding ones). What
would let such a class be admitted — a court whose close covers a tile of rows instead of the sequence, or ceilings sized for an encoder
with the collateral that goes with them (PRINCIPLES §6, conditions 1, 5 and 6) — is the court owner's decision and is not proposed
here (the Lead routes it to K2S, the tiled-prompt court's owner). Until it is decided, the `Head` profile closes the listing's
"no profile" verdict (`MODALITY_PROFILE_MISSING` → `PROFILE_NOT_ARMED`) and nothing else: no head or encoder repository becomes
shape-ready by it. This is the largest precise gap this lane found.
