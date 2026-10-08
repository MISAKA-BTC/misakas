# PALW-TIR — the task-head profile (`Head`), v1 design (HFX, 2026-10-08)

*Lane HFX, branch `hf/x-blockers`. Status: **design + off-chain pieces built; the consensus profile waits for the Lead's allocation**
(the fence name, the profile tag, the two appended Borsh variants). Nothing here is armed, scheduled or in any shipped ruleset.*

## 0. 日本語での要約

- NLU / vision の「head」系タスク(text-classification 149,560、token-classification 35,316、fill-mask 21,884、QA、zero-shot、
  image-classification 30,862、object-detection 7,103、segmentation 4,535 …)は census で `MODALITY_PROFILE_MISSING` だった。
  frontend は sequence classifier を既に Embedding-profile の行として lower できるが、**それは「ベクトル」であってタスクではない**。
- 本設計は RFC-0003 の生成クラス層に **`Head` profile** を足す: 入力(ids / 画像)、出力(`EmbeddingI32 [rows, labels]` の logits)、
  label map の root、pair 入力の区切り、MLM の位置、そして **decode 規則(`HEAD_DECODE_V1`)** をクラスが固定する。
  court が判定するのは logits テンソル(既存の generic output root)で、label は検証済み logits の純関数(argmax 等)なので、
  logits が正しければ答えも一意に正しい。新しい output kind は作らない(`output_set_id` 不変 = 稼働中の `palw_gen_v1` 指紋は不変)。
- consensus に見えるもの(profile tag 6、offers の追記 variant、job body の追記 variant、fence `palw_task_heads_v1`)は
  **休眠 fence の後ろ**。割り当ては Lead に依頼中。

## 1. What a class of the profile is

A `Head` class is an RFC-0003 generative class (`PalwGenClassV1`, carried by `ClassRegisteredGenV1`, tag 68 — unchanged) whose
`profile` is `Head` and whose offers carry a `PalwGenHeadOffersV1`. It computes ONE canonical tensor per job:

| task (`HeadTaskV1`) | HF head classes | input | output header (`EmbeddingI32`) | rows |
| --- | --- | --- | --- | --- |
| `SEQUENCE = 1` | `…ForSequenceClassification` (classifier, reward, regression) | text ids | `[1, labels]` | the pooled row (`[CLS]` / last token) |
| `PAIR = 2` | `…ForSequenceClassification` over a pair: cross-encoder rerankers, NLI (zero-shot) | `a ‖ sep ‖ b` | `[1, labels]` | pooled |
| `TOKEN = 3` | `…ForTokenClassification` | text ids | `[L, labels]` | every padded row |
| `SPAN_QA = 4` | `…ForQuestionAnswering` (extractive) | `question ‖ sep ‖ context` | `[L, 2]` (start, end) | every padded row |
| `MASKED_LM = 5` | `…ForMaskedLM` (fill-mask) | text ids + one position | `[1, vocab]` | the row at the position |
| `IMAGE = 6` | `…ForImageClassification` (ViT, ConvNeXt, ResNet, …) | one image slot | `[1, labels]` | pooled |
| `DETECTION = 7` | DETR-style set prediction (`…ForObjectDetection`) | one image slot | `[queries, labels + 1 + 4]` | per query: class logits ‖ box |
| `SEGMENTATION = 8` | `…ForSemanticSegmentation` | one image slot | `[h·w, labels]` at the head's stride | per output pixel |

`labels` is the length of `id2label` (the label map is committed, §2); a regression head is `SEQUENCE` with `problem = REGRESSION`
and one label. A zero-shot classifier is a `PAIR` class whose offers name the entailment label (§3).

## 2. What the class commits to (offers, appended)

```text
PalwGenProfileOffersV1::Head(PalwGenHeadOffersV1)        // Borsh variant 3 (appended after None 0, Image 1, Embedding 2)
PalwGenHeadOffersV1 {
    task: u8,                      // HeadTaskV1
    problem: u8,                   // 1 single-label (softmax / arg-max), 2 multi-label (sigmoid per label), 3 regression (raw)
    labels: u32,                   // the output's last extent: id2label's length; 2 for SPAN_QA; the vocabulary for MASKED_LM
    label_map_root: Hash64,        // H64(key "misaka-palw/head/label-map/v1", le32(n) ‖ (le32(len) ‖ utf8 label)*); zero for SPAN_QA / MASKED_LM
    pair_separator: Vec<u32>,      // PAIR / SPAN_QA: the ids between the two texts ([SEP], </s></s>); empty otherwise
    entailment_label: Option<u32>, // PAIR as zero-shot NLI: the label index that means "entailment"
    position_scalar: Option<u8>,   // MASKED_LM: the job scalar that carries the masked row's index (prefix length + the user's position)
}
```

The class id already binds `borsh(offers)` (`class_terms`), so the label map, the separator and the decode inputs are part of the
class's identity; two classes that differ in their label strings are two classes.

## 3. The decode rule (`HEAD_DECODE_V1`): a pure function of the verified output

The court never judges a label. It judges the output tensor (the generic generative output root, RFC-0003 §I.3). The decode is a
function of that tensor and the class's offers, pinned by version (its digest enters the fence's `head_set_id`):

* **arg-max** is the smallest index among the maxima (no float, no tie randomness); single-label → the arg-max label; multi-label →
  every label whose logit is > 0 (sigmoid > ½ ⇔ logit > 0, exactly, in integers); regression → the logit times the class's unit.
* `TOKEN`: per real row (rows `< count`), arg-max; the "simple" aggregation (consecutive equal `B-`/`I-` tags of one entity)
  is a function of the per-row labels and the label strings; sub-word grouping needs the tokenizer's offsets, which the class's
  tokenizer id binds (outside the court, deterministic).
* `SPAN_QA`: the best span `(s, e)` over the context rows (after the first separator), `s ≤ e < s + max_answer`, maximising
  `start[s] + end[e]` in integers, ties to the smallest `(s, e)`; no-answer when the `[CLS]` pair wins (SQuAD 2.0).
* `MASKED_LM`: the top-k ids by logit, ties to the smaller id.
* `PAIR` as zero-shot: one job per candidate label (the hypothesis template applied by the gateway); the decision is the candidate
  whose entailment logit is largest (single-label) or every candidate whose entailment logit exceeds its contradiction logit
  (multi-label) — a function of the verified rows of the jobs.
* `DETECTION`: per query, arg-max over `labels + 1` (the last is "no object"); boxes are the query's last four values in the class's
  unit. `SEGMENTATION`: per output pixel, arg-max; the upsampling to the input size is the gateway's (declared, outside consensus).

Probabilities (softmax) are presentation, never consensus. Two honest verifiers of a verified output produce the same decision.

## 4. The job (appended body variant)

```text
PalwGenBodyV1::Head(PalwGenHeadBodyV1)                   // Borsh variant 2 (after Image 0, Embedding 1)
PalwGenHeadBodyV1 {
    input: PalwGenEmbeddingInputV1,   // Text { token_ids_hash, tokens } | Image(ref) — the Embedding body's input enum, reused
    task: u8,                         // MUST equal the class's
    position: u32,                    // MASKED_LM: the masked id's index in the user's ids (< tokens); 0 otherwise
    output: u8,                       // EmbeddingI32 = 3
}
```

Acceptance (by name, never rewritten): body kind = class profile; `task` = the class's; text tokens in `[1, max_prompt_tokens]`;
`position < tokens` for `MASKED_LM`, else 0; the image at its slot's size; the seed all zeros (`SeedNotUsed`): no class of the profile
draws randomness. A pair is ONE id list (`a ‖ separator ‖ b`, formed by the gateway); the class reads it as one prompt. The pipeline
job: the prompt ids; `scalars[position_scalar] = |prefix| + position` for `MASKED_LM`.

## 5. The fence and what it changes

* `Params::palw_task_heads_v1: Option<PalwTaskHeadsFenceV1 { activation, head_set_id, ceilings: PalwGenProfileCeilingsV1 }>` —
  dormant (`None` on every preset), hashed Some-only into `consensus_params_id` / `consensus_schedule_id`, collapsed from
  `Some(never())`, needs `palw_gen_v1` in force at or below it; mirrored on the V2 bundle as `palw_gen_range_twin_v1` is.
* Below the fence a `Head` class is refused at the registration gate by name (`ProfileNotArmed(Head)`); the fold's second lock reads
  the mirror. **No byte of an existing class, job or ruleset changes**: the profile tag, the offers variant and the body variant are
  appended; `output_set_id` is unchanged (no output kind is added — `EmbeddingI32 [rows, labels]` carries every head), so the armed
  `palw_gen_v1` fingerprint is unchanged.
* Admission is the pipeline admission under the fence's own `Head` ceilings (initially testnet-12's Embedding ceilings); the court,
  the claim fold, the worker and the gateway accept `Head` wherever they accept `Embedding` (same output kind, same output root).

## 6. What exists today (this branch) and what does not

| piece | status |
| --- | --- |
| `…ForSequenceClassification` lowering (encoder, decoder) | existed (`OUTPUT_CLASSIFY_V1`, `tests/seqcls.rs`) |
| `…ForTokenClassification`, `…ForQuestionAnswering` lowering (BERT, RoBERTa / XLM-R, DistilBERT) | **built** (`OUTPUT_TOKEN_LOGITS_V1`, `tests/heads.rs`: HF logits, integer program, pipeline, court, admission) |
| `…ForMaskedLM` at a position | **not built**: needs the task-driven adapter choice (a `…ForMaskedLM` checkpoint is read today as its encoder for sentence embedding) and a third lifted input (`input.pos`) |
| BERT-type pair segments (token type 1 after the first separator) | **not built** (`ENC_PAIR_SEGMENTS_V1`): a `PAIR` / `SPAN_QA` class over a model with `type_vocab_size > 1` is refused by the profile until it is |
| vision heads (`IMAGE`, `DETECTION`, `SEGMENTATION`) | the ViT / CNN stage programs exist (`read_vision`, `read_cnn`); the heads are not lowered |
| the `Head` profile in consensus (offers, body, acceptance, fence) | **waits for the allocation** |
| the census row (`census::tasks`) | the tasks map to `Profile::GenHead` (fence `palw_task_heads_v1`): `FENCE_NOT_ARMED` at every shipped ruleset; a hypothetical ruleset judges the declared class |

## 7. Allocation requested from the Lead

1. the fence name `palw_task_heads_v1` and its `Params` field (dormant everywhere);
2. `PalwGenProfileV1::Head = 6`;
3. `PalwGenProfileOffersV1::Head` (Borsh variant 3) and `PalwGenBodyV1::Head` (Borsh variant 2), both appended;
4. no new output kind and no new registration object tag (confirm).
