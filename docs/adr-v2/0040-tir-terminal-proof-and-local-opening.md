# ADR-v2-0040 — TIR終端証拠・fold opening・局所裁定

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0002](../rfc-v2/0002-tensor-ir-and-shared-query-data.md) / [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

capture全体ではなく、認証された局所証拠から終端裁定を組み立てる判断を集約する。

## 2. 統合した判断の読み方

### 2.1

長いattention historyをそのままcloseへ運ばず、dissectionやfoldのopeningで局所化する経緯を読む。[旧ADR-0082](../adr/0082-the-close-is-flat-in-the-context.md)

### 2.2

producerが実際にcommitした値をserved materialから開き、challengerの正しい再計算値で相手のopeningを置き換えない。[旧ADR-0085](../adr/0085-the-close-is-assembled-from-what-was-served.md) [旧ADR-0173（public-verifier-dispute-completeness-is-misaka-purpose）](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)

### 2.3

foldを運ぶopeningとfused rowの裁定を対応づける。court kernelが存在することと、responderが必要資料を提供することは別の完成条件である。[旧ADR-0086](../adr/0086-the-opening-carries-the-fold-not-the-leaves.md) [旧ADR-0093](../adr/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)

## 3. 維持する境界・後続改定

backend内部のcaptureを使ったfixtureだけで公開検証者のE2E取得を満たしたことにしない。旧claim分割案の撤回は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)、後続のread-set拡張は[ADR-v2-0041](0041-long-context-court-decomposition.md)。

## 4. 隣接する判断との分担

算術は[ADR-v2-0013](0013-tir-arithmetic-and-determinism.md)、material認証形式は[ADR-v2-0025](0025-context-and-input-commitments.md)・[ADR-v2-0044](0044-shared-tir-query-data.md)、DA要求は[ADR-v2-0039](0039-data-availability-court-and-panel-da.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0082-the-close-is-flat-in-the-context.md](../adr/0082-the-close-is-flat-in-the-context.md) | L1–L1003 | L15–L44 |
| [0085-the-close-is-assembled-from-what-was-served.md](../adr/0085-the-close-is-assembled-from-what-was-served.md) | L1–L215 | L15–L24 |
| [0086-the-opening-carries-the-fold-not-the-leaves.md](../adr/0086-the-opening-carries-the-fold-not-the-leaves.md) | L1–L308 | L17–L21 |
| [0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md](../adr/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) | L1–L410 | L15–L31 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0082-the-close-is-flat-in-the-context"></a>

<details>
<summary>0082-the-close-is-flat-in-the-context.md — 原文 L1–L1003</summary>

[原文を開く](../adr/0082-the-close-is-flat-in-the-context.md) · SHA-256: `44df4a6ee97a33248aaa24f44eb55cf3ae9c9a21a9f987f61d2a3d8f93b708a5`

<!-- ADR_SOURCE_BEGIN:0082-the-close-is-flat-in-the-context.md -->
~~~~~~~~markdown
# ADR-0082: The close is flat in the context — attention is refuted by dissection, the capture is a fold, and the answer is what earns

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** IMPLEMENTED (2026-09-04) on `palw-adr0082-impl`, pending the testnet-11 Relaunch 5f
cut. **§10 is the implementation record**, and it is where every figure below that the measurement
MOVED is corrected in place — struck, restated, with the configuration and the stream that moved
it. ~~PROPOSED (2026-09-03).~~ Written against two things: the refutation of ADR-0080 and
ADR-0081 (their own status blocks, verified line by line against the tree), and the state of
`palw-testnet-5f` at `0c299e7a` read beside `palw-adr0080-verification-segment` at `ee1a7582`.
The goal it serves is the one ADR-0077 R0 states and ADR-0080/0081 tried to reach: a person types a
prompt of thousands of tokens, receives an answer of thousands of tokens at the runtime's own speed,
and the chain holds ONE claim whose verification costs a bounded number of bytes wherever bytes are
spent and a bounded amount of compute wherever compute cannot be avoided. That is the shape Ambient
reached with a tolerant proof model (ADR-0026's audit); this ADR reaches it with the exact one.
Nothing here is armed by this document. Every consensus change below is a ruleset move and lands
with a re-genesis on testnet-11, as every relaunch has; mainnet ships PALW off and is untouched.
**Builds on:** ADR-0026 (borrow Ambient's architecture, refuse its proof model), ADR-0028 (sampling
decides nothing; only the court convicts), ADR-0030 §3 (the checkpoint leg and its state chunk map),
ADR-0040 Decision E (exact integer accumulation: the order of accumulation cannot change the
result — the associativity this ADR spends), ADR-0044 F4–F6 (beacon, panel, ticket), ADR-0049
Decisions C/E/F (admission bounds the court from the graph; the tiled logits pin; one description
for engine, profile, adjudicator and inventory), ADR-0072 (the ticket is the execution), ADR-0074
Decision 5 (a quantum is a fraction of the canonical job's leaves), ADR-0075 (a class is seated by
`ClassLaneCertified`), ADR-0077 (R0, R1, Decision 8's sampled interval, Decisions 10–14, SA-4's
derived deadline), ADR-0078 (what was made from the answer is committed, never carried), ADR-0080 §1
and Decision 3 (the measurement and the economic invariant — the two things that survived), ADR-0081
§1.2 and Decision 3 (the prompt ids as a Merkle root — the one decision that survived), and ADR-0080
"design A" as landed on `palw-testnet-5f` (the chunk-group close ceiling).
**Amends:** ADR-0080 §3 and ADR-0081 §3 — withdrawn and replaced (§7). ADR-0077 Decision 8 (the seat
no longer fetches the history), Decision 11 (attention is no longer priced by an interval), Decision
12 (the ladder's depth stops binding), Decision 13 (the rows are sized by three derived bounds, not by
a rotary table). ADR-0080 design A (the chunk count is re-derived from the flat terms, and its
unlanded half on 5f is named as a prerequisite).

## 1. What was measured

Line numbers below are those of the two branches the status block names: `palw-testnet-5f` for
the transport, worker, kernel, profile and economics facts (§1.2, §1.4–1.6), and
`palw-adr0080-verification-segment` for the refutation, U-00 and U-04 facts (§1.1–1.2). Every
citation names the item as well as the line, so a moved line is still findable. **And every
figure in this ADR names the CONFIGURATION it was measured under** — which court (the shipped
binary ladder, or the dissection court with its arity), which prompt-id form, which evidence
route, which host load — because the same defect bit three subsystems on the day this was
written: a number measured under one configuration and quoted as if it held under another, never
caught by the person who wrote it down. A figure without its configuration is not a measurement
here; it is a hope with a decimal point.

### 1.1 The refutation of ADR-0080/0081 stands, and it was correct

Read against the tree, every finding in the two status blocks holds. `PalwDerivedArtifactV1` names
a singular `claim_id` and a singular `output_root` and the transition enforces the equality
(`palw_derived_v1.rs`); `output_commitment_v2` is a flat keyed hash under a per-job
`job_context_hash`, so no concatenation relation between two claims is checkable; `fp_work_id_v1`
keys on `(class_id, prompt_token_ids_hash, decode_tokens_executed, executor_bond)` with no state and
no parent, so a second segment is `DuplicateWork` (`palw_freeprompt_v3.rs:286`);
`checkpoint_genesis_prev_v2` derives the chain's genesis link from `job_context_hash` alone
(`palw_step_leg.rs:627`) and `verify_kv_anchor` is intra-claim in every check. The economic census
(`palw_economic_locus_v1.rs`) measured the rest: a `PalwSeatReceiptV2` is 4,772 bytes on the wire,
a `ReceiptLicensed` carrying three is 14,385, a 37-segment answer is 513,597 bytes of signatures
inside 532,245 bytes of consensus objects as a FLOOR, and the quanta band is reachable at the shipped
cap — 51,200 leaves weigh 12,800 pwu as one claim and 51,200 as four. **Segmentation of one answer
into N claims is not a design this chain can verify or afford, and nothing below re-opens it.**

What survives is what this ADR is built on: ADR-0080 §1's measurement (the 512 residue is the
context's WIDTH, not the history's length), ADR-0080 Decision 3's invariant (a job's reward,
ticket, weight and quanta are functions of its leaves and of nothing a producer can restructure),
and ADR-0081 Decision 3's Merkle prompt ids (`palw_prompt_ids_v1.rs`, behind the dormant
`palw_prompt_ids_merkle` fence: 2,048 → 472 bytes at `n_ctx` 512, 131,072 → 856 at 32,768, one
64-byte element more per doubling).

### 1.2 The graph-v4 tile flattens the opening and not the close, and the residue has a name

The U-00 stream measured the v3 map against the anchored court (`palw_context_ladder.rs`, tests
`the_v3_tile_flattens_the_opening_and_not_the_close`,
`with_the_interval_held_the_residue_is_the_probability_row`,
`the_tile_moves_the_widest_dense_row_from_thirty_to_two_hundred_and_twenty_three`,
`the_v3_map_is_not_priced_by_the_ladder_rule`):

| what | measured |
|---|---|
| the v3 checkpoint OPENING, dense, at `n_ctx` 1,000 / 4,096 / 32,768 | 18,432 / 18,432 / 18,432 — flat |
| the v2 opening at 1,000 / 4,096 | 1,026,048 / 4,196,352 — the whole history |
| the anchored v3 CLOSE at 1,000 / 2,000 / 4,096 | 228,769 / 414,033 / 806,577 — one straight line, slopes 185.3 then 187.3 bytes a position |
| the same close with the interval held at the tile (256 / 512 / 1,024 / 4,096) | 123,889 / 136,817 / 162,609 / 317,105 — slope `attn_heads × 4` ≈ 48–51 bytes a position |
| what the shipped ladder rule CHARGES a v3 class for the opening | 526,336 — the v2 price, 28.6× the honest 18,432; the rule has no `_for_map_v1` twin on the cache half |
| widest dense row inside 81,920: unfenced / anchored v2 / honest v3 | 21 / 30 / 223 |

The slope's name is the graph's. On both families the attention site is four committed rows per
position: `ATTN_SCORES` (`MatMulQuant`, out `KvPerHead`), `SoftMax` (out `KvPerHead`), the
probability requantization (`MulElem`, out `KvPerHead`) and `ATTN_VALUES` (`MatMulQuant`, reading
`Step(9)` and `CachedV`) — `palw_base0_profile.rs:310-313`, `palw_qwen36_profile.rs:467-470`.
`KvPerHead` resolves to `KvScaled { multiplier: attn_heads }`, and the cost walk resolves a
`KvScaled` width at the CONTEXT maximum (`palw_class_admission_v2.rs:134-140`) because a leaf of
`ATTN_VALUES` opens the whole probability row of its head. That row is `attn_heads × n_ctx` lanes
wide at every position. **Three context-wide rows are committed per attention site per position,
and one of them is opened whole by the node after it.** No state chunk map reaches a committed row;
a map addresses the CACHE, and the cache is not what is carried.

### 1.3 The same rows are why the leaf count is quadratic

A committed row of `attn_heads × kv_len` lanes is `attn_heads × kv_len / tile_len` leaves. Per
position that is linear in the position, so a job of `n` positions commits `O(n²)` attention
leaves on top of the `~99 k` (dense) / `~298 k` (hybrid) leaves a position costs today (ADR-0077
§1). At 131,072 positions and a 128-lane tile the dense tier's three rows alone are
`3 × 28 × 12 × 131,072 / 128 ≈ 1.0 M` leaves at the last position — ten times the base — and the
job, summed over its positions, is `~8 × 10¹⁰` leaves: sixteen times the `2^32` ladder, and past
any hashing budget. The `2^32` ladder ADR-0077
Decision 12 sized — "~14,000 positions of the hybrid and ~43,000 of the dense tier" — was sized
against the BASE count, which is the count a job has only if no context-wide row is committed.

### 1.4 What 5f landed, and what it did not

ADR-0080 "design A" on `palw-testnet-5f` prices a close in chunk-group parts:
`DEFAULT_MAX_CLOSE_CHUNKS = 27`, `DEFAULT_MAX_CLOSE_BYTES = 27 × 100,000 × 10 / 12 = 2,250,000`
(`palw_mode_v2.rs:259-269`). Under it the admission gate admits dense at 39 positions on the
shipped `2^22` ladder and **1,002** on a `2^32` one; the hybrid at 12 and **514**
(`palw_class_admission_v2::tests::the_widest_context_each_family_admits`). And the transport does
not carry it:

* `apply_object`'s `ObjectChunk` arm decodes the assembled group and admits ONLY
  `FamilyCertified` (`palw_state_v2.rs:7417`, `ChunkedObjectKindNotAllowed`); a `CourtClosed` split
  across carriers is refused in the block that completes it.
* `PALW_OBJECT_CHUNK_MAX_COUNT = 8` is enforced by the cutter (`:2917`) and the transition
  (`:7362`); the ceiling's own doc says so (`palw_mode_v2.rs` "What is NOT yet derived from this,
  and is a wall rather than a rounding").
* `misaka-cli palw court-close` (W13) refuses a split close by name and lists the three consensus
  changes it waits on (`misaka/apps/cli/src/palw_court.rs:84-125`); one of the three — `max_close_chunks`
  — landed with W3, the other two did not.
* The free-prompt worker builds its court from `PALW_STEP_MAX_LEAVES` (`2^22`) directly
  (`misaka/palw/base0/src/bin/palw-a16-fp-worker.rs:94`); the W1b fix that prices a job against the
  ruleset's ladder (`bb4f145b`) is not an ancestor of 5f. On the shipped commit path that is about
  38 positions, prompt and answer together, whatever the row admits.

So on 5f at `01163cd3` a 512 row is admitted, unprosecutable past one carrier, and unexecutable
past 38 positions. This is the defect class the tree keeps recording — a gate widened where the rule
did not move — and it is a prerequisite here, not a decision. **Who lands it (settled 2026-09-03
with the 5f integrator):** the transport is ADR-0080 W5 (`palw-adr0080-w5-closegroups`) — a court
close rides its OWN chunk-group table `court_close_groups` keyed `(session_id, side)`, deliberately
NOT ADR-0075 D14's `pending_chunks`, whose `PALW_OBJECT_CHUNK_MAX_COUNT = 8` bounds the certification
lane and prices its slot rent; the court's count is the ruleset's `max_close_chunks`; state v17 → 18
is W5's. **The implementation added two more version bumps beside it: v18 → 19 for the court
session's dissection phase (stream I) and v19 → 20 for the class state's `fused_attention` (fixer
FA2, §10.6); each moved the ADR-0043 state goldens, which are re-pinned at the cut.** W5 carries
four requirements its adversarial judges added, recorded here so they are the
ADR's and not a private brief: the declaration's clock is the session BACKSTOP, refused unless
`daa + 4 × count ≤ deadline` and never extended; a failed declaration loses on its OWN side (a lapse
at Terminal must not route to the accuser's slash); the chunk bytes live in ROOTED state, never a
node-local store; and `PALW_COURT_CLOSE_MAX_PER_BLOCK = 1`, counted in the acceptance walk in
transaction order, so "close bytes in this block" and "court CPU in this block" stay one number. The
executor-side half — the worker pricing a job against the ruleset's ladder (W1b, `bb4f145b`) — is
merged on this ADR's integration branch.

### 1.5 The practical wall is the capture, and no ADR has priced it

Measured on the dense tier's shipped commit path (the model-gate run of 2026-09-03): **5.66 s per
token captured against 0.060 s per token un-captured — 94×.** The capture holds every tile of every
node of every position, ~50 MB a position, ~25 GB at 512 (`fp_capture.rs` header — the header's own
figures; **U-01 then measured the real dense artifact at 13.46 MB a position retained by the capture
against 0.74 MB by the fold**, Decision 7, and fixer FB measured what the amended Decision 4's
per-position cadence SERIALISES, §10.3). The fold that
replaces it — `Base0SparseStepAccumulatorV1`, one retained node per `2^12` leaves, 64 KiB at `2^22`
and 64 MiB at `2^32`, tiles re-derived by replay — is written, tested against
`step_merkle_root_v1` for every leaf count, and **called by nothing outside its own module** (one
doc reference in `palw_step_leg.rs:238`). A person waiting 5.66 s a token is not using a practical
runtime, and no context width changes that.

### 1.6 What earns, what is chosen, what the lane carries

* **Leaves are credited whole, prefill included.** A commitment's `work_leaves` is the capture's
  `step_leaf_count` (`palw_freeprompt_v3.rs:1225`), the transition prices it as carried
  (`palw_state_v2.rs`, the free-prompt arm: `fp_quanta_v3(*work_leaves, quantum, cap)`), and the
  seat's replay is what binds it to the enumeration (`palw_backend.rs:379`). A pinned integer model
  is deterministic and causal: every leaf of a prefix is a pure function of the prefix, so the same
  bond re-submitting a 32,000-token prefix with one new token recomputes nothing and is credited
  everything. Weight is not yet on this lane (ADR-0073 Phase ④ is pending), which is exactly when
  the rule has to exist.
* **Decoding is `base0_decode_token_select_v1`: the argmax of the integer logits row, ties to the
  lowest index** (`palw_step_refute.rs:2825`). There is no sampler. The tree's own measurement of
  what a small model does at temperature zero is on record: "collapses to a handful of attractors
  (measured min-entropy ~3.1 bits over 60 seeds …)" (`params.rs:4307`). A thousand-token greedy
  answer is a repetition, not an artifact.
* **The lane's carriage is per claim:** five seats, three receipts of 4,627 bytes each, four
  interval replays per seat, one payout row drained at `PALW_V2_MAX_PAYOUTS_PER_BLOCK = 8` under a
  premise stated at the constant — "at most one new claim per block" (`palw_state_v2.rs:231`). At
  the frozen 120 s cadence that is 5,760 finalized claims a day before the queue stops draining, a
  ceiling that no context width moves.

### 1.7 Ambient at large context, from ADR-0026's audit

ADR-0026 measured Ambient's binary rather than its paper: the verifier is `/ambient/v1/inference/verify`
inside their llama.cpp fork; it takes a token window (`start_token` / `end_token`), teacher-forces
the sequence (`ambient_forced_tokens`), recomputes the window from the context it holds, and checks
the Merkle commitment; the comparison is tolerant (`logit_min_rbo_score`, `mlp_output_p95_abs_diff`).
Two things in that shape are right at any context and are taken here: **the verifier verifies a
window, and the verifier holds the context and recomputes — nothing context-sized is shipped to
it.** One thing is absent from it because Ambient never needs it: an exact DISPUTE. A tolerant
verdict is consumed as a probabilistic slash and is never bisected, so Ambient has no close to keep
small. PALW convicts by bisection to one leaf (ADR-0028), and the leaf's neighbourhood at an
attention site is the whole history — which is why "verify a window" alone does not make PALW
flat, and why §3 adds the one mechanism Ambient does not have.

## 2. The requirement

> **R4 — nothing a court carries, a seat fetches, or an executor commits grows with the context
> except through a logarithm; the context a class admits is bounded by compute and by the windows,
> never by bytes.** ADR-0080's R2 stands inside it: the length of the answer, the width of one
> verified unit and the input a class admits are three numbers — and the second of them is now a
> TILE, at every context, for every node of the graph.

```text
   a 100,000-token prompt ─┐                                the answer, streamed
   the answer, 4,000 tokens ┘   ONE inference, ONE claim    (kept by the user, ADR-0078)
                                       │
          ┌────────────────────────────┼────────────────────────────┐
          ▼                            ▼                            ▼
   the executor                   a seat (×5)                  the court
   commits O(1) rows per         holds the prompt,            bisects the leaf space k-ary,
   attention site — no row       RECOMPUTES the cache,        then DISSECTS the history k-ary:
   is context-wide; folds        replays k intervals,         each round k triples, each
   leaf hashes, keeps the        compares EXACTLY;            ~8 KiB; the bottom opens
   tree above level L;           fetches O(k·interval·row     ONE tile of K, ONE of V,
   retains the KV cache          + log leaves) bytes,         the query row and the output
   for the claim's life          never the history            tile — ~25–42 KB, at any n
```

## 3. Decisions

### Part A — the court is flat in the context

**Decision 1 — attention is ONE fused node per site, and its committed row is the output.** The
four nodes of §1.2 become one node kind, `AttnFused`, whose inputs are the rotated query row, the
K cache and the V cache, and whose committed row is `out` (`heads × d_head` codes, the row
`ATTN_VALUES` commits today) and nothing else. The scores, the row max, the exponent sum, the
probabilities and the requantized probabilities are INTERNAL to the op: computed by the executor in
any order it likes, never committed, never carried. The op's semantics is the composition of the
four shipped kernels — `a16_attn_scores` (W9), `a16_softmax_rows` (W11, `softmax_shifted` per
head), the `attn_probs` requantization and `a16_attn_values` (W10), `palw_base0_a16.rs:329-430` —
so its `kernel_semantics_id` is derived from theirs and an adjudicator that holds the four holds the
fifth. A class IS its graph (ADR-0049 Decision F), so this is graph v5: a NEW class id per family,
registered through ADR-0075's route or minted at the relaunch; the shipped v2/v3 rows are untouched
and stay exactly as narrow as they are. Consequence for the leaf count: an attention site costs
`⌈heads × d_head / tile_len⌉` leaves a position at every context — 24 on the dense tier at a 64-lane
tile, 32 on the hybrid at 128 — and the job's leaf count returns to the BASE count ADR-0077
Decision 12 was sized against: `~43,000` dense and `~14,000` hybrid positions under `2^32`
(~~measured by U-04 at the fence's ladder: dense 41,695, hybrid 13,105~~ — **corrected 2026-09-04,
fixer FD, audit D H-5: swept through the gate's own door instead of derived beside it
(`the_widest_context_each_family_admits`, `2^32` context-ladder fence, dissection court at the
derived arity, Merkle prompt ids, the joint clock), the graph-v5 families admit `n_ctx` 16,384
dense and 13,105 hybrid, and the refusal one position wider is the LADDER or the WINDOW and never
the close — which is the property Decisions 1–4 exist to buy, asserted rather than hoped**).
**A width clears the close
AND the ladder, and the two are different caps**: the RC bundle the 5f genesis freezes carries a
`2^26` ladder, not the fence's `2^32`, and at `2^26` the widest admissible dense **graph-v2** row is
**574** positions (`palw_class_admission_v2.rs`'s own ladder table, the
`palw_a16_context_row_profile_v1` sweep). The first draft quoted that number for the v5 row it was
not measured on; it stays here as the v2 figure it always was, and the v5 sweep is the corrected
line above. 512 fits with about twelve per cent of margin,
and 1,024 is not the next rung on the ladder side but a different cap (`2^28`). So on the close
side 1,024 costs 64 bytes more than 512; on the ladder side it needs a re-genesis. Every figure in
this ADR that quotes a 32-element Merkle path is the fence's number; the ruleset's is read from the
bundle, never typed.

**Decision 2 — a fused attention leaf is refuted by DISSECTION over the history, and nothing
context-wide is ever carried.** This is the mechanism Ambient does not have (§1.7) and the one the
tree itself named a year of ADRs ago: "carrying a model at that scale needs bisection WITHIN a
step's reduction" (`palw_mode_v2.rs:208`). The terminal adjudication of an `AttnFused` leaf is not
a one-shot recompute but a short interactive protocol over the history positions `0..p` of the
disputed head:

* The responder opens the committed `out` tile and states a ROOT CLAIM for the head:
  `(m*, S*)` — the row max and the exponent sum — and the partial value sums `V*[lane]` for the
  disputed tile's lanes. The court checks that the shipped finalization
  (`clamp16(a16_scale_round(V*[lane], m, shift) + zero)`) reproduces the opened `out` tile; a
  root claim that does not is refused before any round is played.
* Each round the responder discloses, for each of the `k` children of the disputed range, the
  triple `(max_c, S_c, V_c[lanes])`, and the court checks the FOLD before the challenger moves:
  `max = max(max_c)`, `S* = Σ S_c`, `V* = Σ V_c` over the children, all exact integer operations
  (ADR-0040 Decision E). A disclosure that does not fold to the parent's claim is a conviction. The
  challenger names the child it disagrees with.
* At the bottom — one history TILE of `PALW_ATTN_HISTORY_TILE_V4 = 16` positions — the court opens
  the head's query slice, the tile's K rows and V rows (the graph-v4 map, `tiled_kv_state_chunk_map_id_v3`,
  from the checkpoint at or before the tile, or the cache-write leaves after it) and RECOMPUTES the
  tile's triple with the shipped kernels: `s_j` by W9's dot and requantization; `max_j s_j`;
  `e_j = int_exp(((s_j − m*) << up).clamp(i32::MIN, 0))` and `Σ e_j`; `p_j = requant((e_j × int_recip(S*)) >> K)`
  and `Σ p_j × v_j[lane]`. Every one of those is per element given `(m*, S*)`
  (`palw_base0_ops.rs:306-338`: one max, one exp table, one reciprocal, then a per-element
  multiply and shift), which is what makes the tile's triple recomputable without the row.

If `m*` was lied about, the max fold finds the tile whose true max exceeds the claim; if `m*` is
honest and `S*` is not, the sum fold finds it; if both are honest and `out` is not, the value fold
finds it — with the SAME `(m*, S*)` used at every tile, so there is no rescaling and no rounding
between children. The exactness this rests on is the kernel's own premise:
`|p × v| ≤ 32,767² < 2^30`, so `2^32` terms stay inside `i64`'s `2^62`; the softmax sum is at most
`n × ONE = n × 2^24`, inside `i64` to `n < 2^39`. `A16_MAX_DOT_LEN = 2^18` bounds `kv_len` in
`a16_attn_values` today (`palw_base0_a16.rs:61, :396`) — "one power of two above the largest real
reduction in the family" — and moves with a wider row's class id, inside the `2^32` the premise
allows.

**Decision 3 — the court's dissection is k-ary, with k derived from the move budget.** The shipped
ladder is binary at a pinned midpoint (`palw_bisect.rs` header), and the clock runs per move:
`moves = 2 × ⌈log₂ leaves⌉ + terminal` (`PalwCourtParamsV2::worst_case_duration_daa`). At the
`2^32` ladder and the derived RC deadline of 45 DAA that is `(2 × 32 + 2) × 45 = 2,970` of the
`3,000`-DAA `window_court` (`PALW_RC_WINDOWS_V1`): the binary ladder already spends the whole
window, and Decision 2 adds rounds. A k-ary round discloses the `k` subtree roots below the current
node — `log₂ k` binary levels at once, authenticated by hashing them back up — so
`rounds = ⌈log₂ space / log₂ k⌉`, at `k × 64` bytes a move for the leaf space and
`k × (4 + 8 + 8 × tile_len)` for the history (a max, a sum and `tile_len` partial sums per child).
Worked at `k = 16`, the band the ADR was drafted against; the derivation below selects the
SMALLEST legal arity that fits, ~~which at the RC windows is **4** (measured by U-03's pin:
`2^32` in 16 rounds plus 131,072 positions in 7, `(2 × 23 + 2) × 45 = 2,160` DAA)~~ — **corrected
2026-09-04 (audit D H-2, audit A M-2, fixers FA and FD): only the HISTORY search is k-ary. A
session plays the shipped `PalwBisectLadderV1` over the leaf space, which is BINARY, and the
responder's root claim is a move nobody counted. With both corrected the honest count is
`2 × (⌈log₂ L⌉ + ⌈log_k (n / 16)⌉) + 2 + 1`, and what the RC's own configuration selects is
arity 2, not 4 or 16 — §10.4 has the derivation and the two configurations that disagree.** The
table below is the ADR's original worked band, with the two rows the correction moves restated
beneath the strike:

| space | binary rounds | 16-ary rounds |
|---|---|---|
| leaf space `2^32` (searched BINARY in the shipped ladder — the 16-ary column is what the first draft assumed) | 32 | ~~8~~ |
| history at 131,072 positions, 8,192 tiles | 13 | 4 |
| moves, both, plus terminal ~~92 / 26~~ → **93 / 75** (binary leaf ladder, plus the root claim) | 93 | 75 |
| DAA at the 45-DAA deadline ~~4,140 / 1,170~~ → **4,185 / 3,375 — BOTH over the 3,000-DAA window** | 4,185 | 3,375 |
| bytes a move: leaf space / history at a 64-lane tile | 128 / 1,048 | 1,024 / 8,384 |

The RC's own row is the configuration that ships, and it is a different one: `2^26` ladder, a
512-position history (32 tiles of 16), the RC's 42-DAA clock, an 8-lane disputed site →
**arity 2, 65 moves, 2,730 DAA, plus the 216-DAA assembly reserve = 2,946 of 3,000**
(`the_rcs_derived_deadline_selects_an_arity_for_its_own_row_and_none_for_the_fences`).

`k` is a `PalwCourtParamsV2` quantity (`dissection_arity`, binary on every court built today) —
inside `palw_ruleset_id_v2` like the ladder — and it is DERIVED at genesis as the smallest power of
two for which
`(2 × (⌈log_k L⌉ + ⌈log_k (n_max / tile)⌉) + terminal) × turn_deadline ≤ window_court` over the
widest row the ruleset will admit, with `turn_deadline` the SA-4 derivation. No preset writes a
chosen `k`. The bytes a move carries are inside one carrier at every `k ≤ 64` for a disputed tile
of 128 lanes and at every `k ≤ 32` for one of 256 (`palw_attn_dissect_arity_fits_carrier_v1` — a
property of the PAIR, applied by the derivation at the widest registered tile, never of the arity
alone). On a RUNNING chain the same value arrives by activation rather than by re-genesis: the
bare top-level fence `Params::palw_kary_court`, `None` on every shipped preset, under which the
court's arity becomes the derived value and the dissection arm becomes admissible — the
`palw_context_ladder` shape, which swaps a court parameter inside the ruleset id at its DAA. It
carries no companion value (the `palw_bond_maturity` hazard): what it selects is a pure function of
the ruleset every node already holds.

**Decision 4 — the bottom of the dissection opens tiles, so the anchor is tile-addressed and
priced as such.** ADR-0080 Decision 5 said the anchor must be tile-addressed; ADR-0081 §1.1 found
that on its own it flattens the opening and not the close; here it is exactly what the bottom of
Decision 2 needs and no more. Two things follow. A graph-v5 class registers the tiled map
(`tiled_kv_state_geometry_v3`) and the ladder rule prices the cache half by the CLASS's map — the
`_for_map_v1` twin the recurrence half already has and the cache half does not
(`the_v3_map_is_not_priced_by_the_ladder_rule`) — so a class is charged the 18,432-byte tile
opening it will carry, never the 526,336-byte history it will not. And the attention cache is
PREFIX-STABLE: a K or V row written at position `j` is the same bytes in every later checkpoint,
so the executor retains the cache once and recomputes any checkpoint's tiled root from it; no
per-checkpoint copy of the history exists to retain or to serve.

**Amended 2026-09-04 (stream K, audit A C-4/H-1, fixers FA and FB) — a tiled-map class commits a
checkpoint at EVERY position, prefill included, and the cache-write route is then REFUSED for it.**
The shipped cadence checkpoints per DECODE CALL, so no checkpoint covers any prefill position and
none covers a tile straddling the last checkpoint; at those positions the bottom's only route is
the cache-write one, and that route was measured at **149,953 bytes = 1.80 carriers** for a prefill
dispute (dense graph-v5 @ 512, RC `2^26` ladder, tiled v3 map, dissection court: 133,057 at `2^22`,
175,297 at the `2^32` fence). A checkpoint after every position takes the same dispute to
**40,461 bytes = 0.49 carriers at EVERY position class** — prefill, first decode, tile-aligned,
straddling, last — and the hybrid to 73,741. The cadence is read off the registered
`state_chunk_map_id` (`PalwCheckpointCadenceV1::PerPosition`), never declared, so a registrant
cannot buy a coarser leg. And the route is not the challenger's choice either: for a class whose
site says `every_position_is_checkpointed`, the cache-write route is refused by name
(`the_cache_write_route_is_refused_by_name_where_every_position_is_checkpointed`), because audit A
found a K/V SERIES SWAP admissible on it — a route that convicts an honest executor. The anchor is
at `p + 1`, not `p − 1` plus a residue: the residue is then empty and position 0 is anchored, which
is what took the dense close from 93,367 (2 chunks) to 82,719 (1) at arity 16. **What "the
executor retains the cache once" costs is §10.3, and it is not nothing**: retention is 0 state
bytes a position (142 leaf bytes), but the leaf hash binds the chunk INDEX, so a per-position
capture re-serialises 696,516,608 bytes a job at `n_ctx` 512 where the naive whole-cache form is
7,530,872,832.

**Decision 5 — the prompt ids ride as a Merkle root, armed with the first graph-v5 row.**
ADR-0081 Decision 3 as implemented (`palw_prompt_ids_v1.rs`), no longer optional: at 131,072 ids the
flat term is 524,288 bytes on EVERY close, and Decisions 1–4 leave it as the only context-linear
term of the PROMPT. ~~The fence `palw_prompt_ids_merkle` is armed in the same ruleset move as the
rows~~ — **corrected at the cut (2026-09-04): it is NOT armed with the first row.** At `n_ctx` 512
the flat ids are 82,080 bytes against an 83,333-byte carrier, so the registered row fits in either
form, and arming the fence moves every free-prompt job id for nothing. The Merkle form becomes
REQUIRED above about `n_ctx` 1,024, which is the width to arm it at; until then the fence refuses
arming until its inputs exist rather than sitting dormant and armable (fixer FD, audit D M-2), so
the dormancy is a rule and not an omission. Every moved id is re-pinned and listed when it does
arm, and `PalwFpMaterialV1` keeps carrying the ids whole
for the seats (ADR-0081 Y7). **Measured limit (U-04, Z0):** with the prompt ids Merkle-ized a
graph-v5 dense close is flat only to about `n_ctx` 4,096; at 32,768 the binding node becomes the
embedding gather, whose GENERATED-token ids (`decode × 4` bytes, the decode pin's flat id list)
are a second linear term this decision does not anchor. It is linear in the ANSWER's length, not
the prompt's, so "thousands of tokens" holds and "tens of thousands" does not until the output ids
ride the same tiled Merkle idiom — one more fence, the symmetric half of ADR-0081 Decision 3,
recorded in §8 as not decided here and not claimed.

**Decision 6 — the close ceiling is re-derived from the FLAT terms, and the chunk arm is a
prerequisite, not a hope.** With attention refuted by dissection, the widest term of a close is a
MODEL width, not a context: on the dense tier the SwiGLU down projection's operand
(`ffn_dim` 8,960 lanes; the ADR-0080 §1 binding node, 82,080 bytes with the flat ids and ~80,504
with the Merkle ones), on the hybrid one head's recurrence state (`k_dim × v_dim × 4 = 65,536`,
charged once since `4f859f9a`). `DEFAULT_MAX_CLOSE_CHUNKS` is therefore
`palw_close_chunks_for_bytes_v1(max over the registered families of the widest term)` — a
derivation, evaluated at genesis over the rows the genesis set registers. **Not at this cut
(2026-09-04 ruling, audit D M-5): the builder and its reporting test ship UNWIRED and
`DEFAULT_MAX_CLOSE_CHUNKS` stays the hand-supplied 27, because the genesis set now derives to 1
(dense) — and a ceiling tightened to the genesis set permanently refuses any later permissionless
class whose close exceeds 333,333 bytes, on a network whose whole admission story is that anyone
may register. Post-5f the builder reads `palw_shipped_court_rows_v1()`; the derivation is written,
tested and inert, which is the honest state and is §10.7's open item, not a claim in this
sentence.** Stated for BOTH genesis
sets, because two of them are on the table: for the graph-v2/v3 context rows design A was written
for, the widest term is still the context-linear attention close, and the derivation returns
design A's own numbers — **14** for `{floor, A16 dense 512}` (`ceil(1,154,673 × 1.2 / 100,000)`),
**27** with the QWEN36-v3 512 row, 1 for the floor alone; for graph-v5 rows (Decisions 1–5) the
widest term is a MODEL width and the derivation returns — measured by U-04's test
`the_close_ceiling_is_the_derivation_over_the_genesis_set` at `n_ctx` 512 under the dissection
court with Merkle prompt ids — ~~**80,504 bytes = 1 chunk** on the dense tier (binding node
`ffn_down`; 82,080 with flat ids) and **200,732 bytes = 3 chunks** on the hybrid (binding node the
recurrence `GatedDeltaNet`: its `interval × 5 refs` replay evidence, not attention), the set → 3~~
— **restated 2026-09-04 with the route and the arity that make each number true (stream K, stream
J, §10.1): the row the genesis registers closes at 81,599 bytes = ONE carrier, binding node
`attn[7]` `AttnFused`, at arity 2 (the arity the RC and devnet rulesets DERIVE with that row in
`genesis_objects`, off a `2^26` ruleset ladder and a 42-DAA clock), checkpoint evidence route,
tiled v3 map, Merkle ids, openings priced at the `2^32` ladder rules; the same
row at arity 16 is 82,719, and the whole 1,120-byte difference is one move's disclosure. The
binding node moved from `ffn_down` to the fused site because the bottom is now priced at the
checkpoint route (216,019 = 3 chunks was the same row priced at the cache-write route). The
hybrid is 274,460 bytes = 4 chunks at arity 16, still bound by the recurrence's replay evidence and
still unregistered.** The
fused dispute's bottom opening is ~~25,120 bytes dense and 42,016 hybrid on the checkpoint-tile
route (derived), 37,982 and 55,390 on the cache-write route~~ — **the first draft priced one head's
slice, and a checkpoint chunk holds the whole cache ROW (stream E): on the real registered row at
the `2^32` fence it is 41,997 dense and 75,277 hybrid on the checkpoint route, 175,297 and 139,777
on the cache-write one** (measured by borsh on the real object,
34 rows with their paths), flat at 512 / 4,096 / 32,768. "Flat" as a measurement rather than an
adjective (verified independently by a second session on a detached checkout): the dense graph-v5
close is ~~**80,504 bytes at `n_ctx` 512 and 80,696 at 4,096**~~ **82,719 at 512 and 82,911 at
4,096 under the amended Decision 4 at arity 16 (81,599 at 512 at the derived arity 2) — 192 bytes
for eight times the context either way: the SLOPE is what the measurement holds, and the intercept
moves with the evidence route and the arity**; at 32,768 it is 303,640 with the binding node moved
to `EmbedLookup` (Decision 5's
measured limit). Whatever it evaluates to, a row is admitted at a chunk count only when the transport
carries it — W5's own table (§1.4), never the certification lane's `pending_chunks` and its 8. The
CODE CONDITION, not a schedule: until W5 is in the ruleset, `max_close_chunks` is ONE and the
admission gate says so; an admitted row whose worst close no carrier can file is the 5f state §1.4
describes, and it is refused here by construction. The devnet carries its own count (1) and keeps
its minutes lattice, which is what turns `the_reserve_reads_the_rulesets_chunk_count` green
(`2694440c` on 5f: the assembly reserve is per-ruleset).

### Part B — the executor is flat in the context

**Decision 7 — the capture is a fold.** The free-prompt path folds each leaf hash the moment the
engine produces it (`Base0SparseStepAccumulatorV1`), retains the tree at
`PALW_BASE0_SPARSE_RETAIN_LEVEL_V1` and throws every tile away; an opening asked for later is
re-derived by replay from the checkpoint chunks (`fp_interval`). The executor's per-position cost
becomes the forward pass plus the hashing of the rows it commits, and that ratio — not a context
width — is the practical lane's first number. **Measured (U-01, the real dense artifact, §1.5's own
job of 26 prefill + 12 decode = 4,074,040 leaves, 110,109 leaves a position, one process, three
phases):** on a quiet host, the un-captured forward 0.0938 s a token; the dense capture 4.527 s a
token (48.3×; 13.46 MB a position retained); **the fold 0.356 s a token — 3.8× the forward, 12.7×
faster than the capture, 0.74 MB a position retained, 18.1× less.** On the same host at load ~20
the three were 0.214 / 5.806 / 0.491 (27.2× and 2.3×), which brackets §1.5's 5.66 s a token and
says both ends of the ratio move with the host. What the fold leaves above the forward is
0.77 µs a leaf, the hashing the step leg commits, as §4 predicted; nothing is attributable to
tiles, allocation or I/O. Of what the fold retains, 73% is checkpoint chunks — the term Decision 4
and Decision 9 remove. Under Decision 1 the rows to hash per
position are the base count; the fold's retained set at `2^32` is 64 MiB, and a deeper ladder
raises the retained level by the same derivation (`retain_level = ⌈log₂ leaves⌉ − 20` keeps it at
64 MiB). What the executor RETAINS for the claim's life is the KV cache (Decision 4) and, for a
recurrent family, its state at a SPARSE set of checkpoints: the DA obligation is to SERVE any
opening the leg names, not to STORE it, and a checkpoint between two retained ones is re-derived
by replaying the recurrence over at most the retained spacing. The spacing is a class quantity,
derived so that one re-derivation costs no more than the interval replay a seat already performs.

**Decision 8 — the executor prices a job against the ruleset's ladder.** W1b (`bb4f145b`) as
written: the worker reads `max_step_leaf_count` from the bundle it serves and counts a job without
walking it. A prerequisite (U-00): with it absent, no row wider than ~38 positions executes on any
class, whatever Parts A and C admit.

### Part C — the seat is bounded in bytes and priced in compute (Ambient's validator shape)

**Decision 9 — a seat recomputes the cache from the prompt it holds; it never fetches the
history.** ADR-0077 Decision 8 has the seat ask the executor for "the checkpoint chunk at the
interval's start". For an attention family that chunk IS the history — 7.5 GB on the dense tier
at 131,072 positions (`131,072 positions × 2 caches × kv_dim 256 lanes × 4 bytes × 28 layers`),
5.4 GB on the hybrid's ten attention layers (`kv_dim` 512) — and a seat whose bytes are the
history is a seat R1 and W10 forbid. The seat
holds the prompt ids already (on the commitment under `PublicDa`, in the served material under
`PanelDa`) and the committed output ids, so it can RECOMPUTE the job up to any interval's start with
the class's own kernels, exactly, and check the tiled root it computes against the checkpoint root
the executor committed — 64 bytes, not gigabytes. It then replays its `k` drawn intervals from its
own state and compares every committed row exactly, as Decision 8 says. Bytes per seat stay
`O(k × (interval × row + log₂ leaves))`; compute per seat becomes ONE forward pass of the job plus
`k` intervals — Ambient's validator cost, with Ambient's tolerance replaced by equality.
Consequences, stated as the derivations they are:

* The width a row admits on the seat side is bounded by the window a seat has to file in:
  `n_max ≤ window_receipt × rate_seat_prefill`, with `window_receipt` a ruleset window (600 DAA on
  the RC) and `rate_seat_prefill` the SA-4 measurement on the slowest fleet host for that class. No
  number is chosen. **The rule is enforced where seats are measured**: a row nobody can seat
  certifies nothing (ADR-0075), so the bound is checked by the certification drill that measures
  the replay floor (`palw-certify`'s route), which refuses to certify a width the slowest seat
  cannot recompute inside the window — not by `verify_class_admission`, which cannot read a fleet
  measurement and must not pretend to. The measured rate is recorded on the certification, beside
  the replay floor SA-4 already records there.
* A challenger in Decision 2's dissection needs the same state to name a child, and a challenger
  is a seat that recomputed (ADR-0077 Decision 8: "holding the refutation's inputs already").
  Nothing new is fetched for a dispute.
* `Incapable` stays the honest verdict for a seat without the artifact (the 5e stop form), and a
  seat without the compute for a row files it too: the panel's capability is a fact about the
  fleet, and a row nobody can seat certifies nothing (ADR-0075).

### Part D — what earns, what is chosen, what the lane can carry

**Decision 10 — on the free-prompt lane, quanta are earned by DECODE leaves; the prefill of a
user-chosen prompt is priced at zero.** The quantum is unchanged (ADR-0074 Decision 5:
`max(1, canonical_leaves / 8)`); the numerator of `fp_quanta_v3` becomes the leaves of the decode
calls, derived by the transition from the job it can enumerate — `prompt_tokens`,
`decode_tokens_executed` and the class profile — never from a count the executor carries. The
attempt lane's canonical job is untouched: its prompt is drawn from the beacon and cannot be
cached. Rationale is §1.6's arithmetic: a deterministic causal model makes every prefix leaf
recomputable at zero cost by the bond that computed it once, so prefill leaves are not scarce and
a subsidy on them is a subsidy on replay. Armed with the graph-v5 rows and not before: at today's
two-decode-token rows a decode-only numerator floors to `ZeroQuanta`, which is the census's lower
band edge and would refuse every honest job on the shipped classes. ADR-0077 Decision 14's
footprint rule is restated in the same unit: the canonical job's DECODE leaves are at least
`n_ctx / 8` positions' worth, so the widest admissible answer earns at most `8 × 8 = 64` quanta by
construction.

**Decision 11 — decoding is a seeded argmax, and the pin's refutation is unchanged.** The class
gains `decode_token_select_v2`: the committed token at a position is
`argmax_j (logit_j × T_ONE + T_q × G_j)`, ties to the lowest index, where `T_q` is the temperature
in Q24, `T_ONE = 2^24`, and `G_j` is a Gumbel variate read from a PINNED Q24 table indexed by the
top bits of a keyed hash of `(sampling_seed, position, j)` — a table in the class's kernel catalog
beside `int_exp`'s, so there is no transcendental in the rule. `T_q = 0` is byte-identical to
`base0_decode_token_select_v1`, which is what every shipped row keeps. Two properties make this the
ONLY sampler this court can carry: the key is a per-lane function, so the refutation stays the
two-disclosure form of the tiled logits pin — open the committed lane's tile and a beating lane's
tile, recompute two keys — at any vocabulary; and Gumbel-max samples exactly from
`softmax(logits / T)`, so a user gets a real distribution, not a heuristic. `(sampling_seed, T_q)`
are fields of `PalwFreePromptJobV3`, inside `fp_job_id_v3` and therefore inside the claim id: a
seed cannot be changed after the fact, and grinding one costs a whole inference per draw, which is
ADR-0072's rule kept (one inference, one ticket) rather than broken.

**Decision 12 — the lane's throughput ceiling is a derived, published number, and this ADR does
not raise it.** `min(PALW_V2_MAX_PAYOUTS_PER_BLOCK × blocks_per_day, panel replay capacity)` is
computed from the constants and the fleet's measured replay rate and printed on the practical
lane's page beside the stages a claim goes through. §1.6's 5,760 a day is the first term today;
raising it is a ruleset move whose premise the payout constant states at itself, owed its own
argument (`palw_economic_locus_v1`'s payout-queue row), and out of scope here.

### Part E — Ambient, named

**Decision 13 — what is borrowed and what is refused, so the next reader does not re-derive
it.** Borrowed, on ADR-0026's evidence: verification of a WINDOW (ADR-0077 Decision 8, kept); a
verifier that HOLDS the context and recomputes (Decision 9); a proof whose size does not depend on
the context (Decision 2 — Ambient's Merkle commitment per window, ours per tile); asynchronous
verification off the block path; bonds and slashing. Refused, as ADR-0026 refused them: the
tolerant comparison (every check here is equality inside a pinned integer class); "verify one token
is sufficient" (a seat's verdict convicts nobody, ADR-0028); a verifier compiled into the runtime
(the fused op's semantics is the composition of four golden-frozen kernels in `consensus-core`, and
the runtime is an adapter beneath it). Added, because exactness needs it and tolerance does not:
the dissection.

## 4. What this costs, stated before it is measured

* **Chain bytes.** Per claim: unchanged (`out` rows are the rows `ATTN_VALUES` commits today;
  three context-wide rows per site are REMOVED from the step space). Per close: bounded by
  Decision 6's flat terms — one or two carriers on the shipped families, re-derived at genesis.
* **Court moves.** ~~`2 × (⌈log_k L⌉ + ⌈log_k (n / 16)⌉) + 2`: 26 at `k = 16`, `L = 2^32`,
  `n = 131,072`; 1,170 DAA at the 45-DAA deadline against 3,000. A dispute at 131,072 positions
  costs the court FEWER moves than a dispute at 512 costs it today.~~ **Corrected 2026-09-04
  (audit D H-2, audit A M-2): the leaf ladder is binary and the root claim is a move, so it is
  `2 × (⌈log₂ L⌉ + ⌈log_k (n / 16)⌉) + 2 + 1` — 75 moves at `k = 16`, `L = 2^32`, `n = 131,072`,
  3,375 DAA at the 45-DAA deadline, which is PAST the 3,000-DAA window; the RC's shipped
  configuration (`2^26`, its 512 row, the 42-DAA clock) is 65 moves and 2,730 DAA, 2,946 with the
  assembly reserve. And the last sentence inverts: a dispute at 131,072 positions costs the court
  MORE moves than one at 512 — thirteen history rounds against five at arity 2 — what is bounded
  is the GROWTH, logarithmic in the history, and at the `2^32` fence that growth no longer fits the
  RC's window at any arity (§10.4).**
* **The bottom opening.** `4 × d_head` (the query slice) + `2 × 16 × 4 × kv_dim` (one K tile, one V
  tile — a checkpoint chunk holds the whole cache ROW, `attn_kv_heads × attn_head_dim`, because the
  map addresses `(kind, layer, position)` and not the head; the ADR's first draft priced one head's
  slice, which under-bounds the object by `kv_heads`) + `4 × tile_len` (the output tile) + paths at
  the ladder's depth. Measured on the real objects (U-03, borsh, `kv_heads` 1, 64-position history):
  the checkpoint route 19,027 bytes at `d_head` 128 and 36,435 at 256; the cache-write route
  (one leaf per row, for the rows after the last checkpoint) 37,985 and 55,393. ~~Inside one carrier
  at every context on both routes; the derivation prices the larger.~~ **Not on both routes, on the
  real registered row: at the `2^32` fence the cache-write route is 175,297 dense and 139,777
  hybrid, over the 83,333-byte carrier — Z3 was FALSE there, which is what the amended Decision 4
  answers. With a checkpoint at every position the whole prefill dispute is 40,461 bytes = 0.49
  carriers at every position class (RC `2^26` ladder), the cache-write route is refused for such a
  class, and the derivation prices the route the ruleset can actually play.**
* **Executor time.** The forward pass plus hashing the base leaf count — `~1.3 × 10¹⁰` leaf hashes
  for a 131,072-position dense job, the same order as the forward's own work. U-01 gives the ratio.
* **Executor retention.** The KV cache for the claim's life (`claim_retirement`, 3,000 DAA on the
  RC): 7.5 GB dense, 5.4 GB hybrid attention at 131,072 (Decision 9's arithmetic); plus a
  recurrent family's sparse checkpoints at `heads × k_dim × v_dim × 4` each — 65,536 bytes a
  head — at the derived spacing. **The amended Decision 4's per-position cadence retains 0 state
  bytes a position on top of that (142 leaf bytes) and SERIALISES 696,516,608 bytes a job at
  `n_ctx` 512 — a cost the first draft did not price at all (§10.3).**
* **Seat time.** One forward pass of the job plus `k` interval replays — at 131,072 positions on
  the hybrid's shipped CPU path, hours; on the dense tier, minutes. Decision 9's derivation makes
  that the width the row admits rather than a surprise the panel discovers.
* **Latency to the person.** Unchanged in kind: the answer streams from the runtime (ADR-0077
  Decision 2) and the chain's stages follow; Decision 7 is what makes the stream run at the
  runtime's speed rather than at the capture's.
* **Identity.** Graph v5 is a new class id per family; `k`, `max_close_chunks`, the prompt-id
  fence, the decode-leaf numerator and the sampling fields are ruleset moves. One re-genesis on
  testnet-11 carries all of them; mainnet is untouched.

## 5. Invariants the tests must hold

```
Z0   No committed row of a graph-v5 class has a context-shaped width: no node's out_len is
     KvScaled, and derive_court_cost_v1 charges every node a close independent of n_ctx except
     for the ⌈log₂⌉ prompt-id term (Decision 5). Swept at 512, 4,096, 32,768 and 131,072.
Z1   AttnFused is the four shipped kernels: for every (q, K, V) fixture the fused op's out equals
     ATTN_VALUES(requant(SoftMax(ATTN_SCORES(q, K))), V) byte for byte, at every kv_len up to the
     class's bound, on every backend (the ADR-0040 differential harness).
Z2   The dissection convicts every forgery and acquits every honest execution: a lie in m*, in
     S*, in any child triple or in any bottom tile is found by the fold or the recompute, and an
     honest responder is never convicted — swept over history lengths, tile boundaries (ragged
     last tile), and every k the derivation can select.
Z3   The bottom opening of an AttnFused dispute is inside one carrier at every context and every
     registered d_head and tile_len; the per-move disclosure is inside one carrier at every k the
     derivation selects.
Z4   The court's worst-case moves, with both search spaces, fit window_court at every preset:
     (2 × (⌈log_k L⌉ + ⌈log_k (n_max / 16)⌉) + terminal) × turn_deadline ≤ window_court, with k,
     turn_deadline and n_max read from the ruleset, never typed.
Z5   A seat verifies a claim fetching O(k × (interval × row + log₂ leaves)) bytes at every context,
     and the checkpoint root it recomputes equals the committed one on honest material; on
     tampered material the mismatch names the checkpoint.
Z6   The capture's per-position cost is independent of n_ctx and of retention: the fold's retained
     set is ≤ 64 MiB at every ladder the ruleset admits, and an opening re-derived by replay equals
     the opening a dense capture would have served, byte for byte.
Z7   Economic invariance (ADR-0080 Decision 3, as the census tests it): a job's quanta, pwu and
     ticket inputs are functions of its DECODE leaves alone on the free-prompt lane; two jobs with
     the same decode leaves and different prompt lengths earn identically; the attempt lane's
     canonical job earns exactly what it earns today.
Z8   decode_token_select_v2 at T_q = 0 equals decode_token_select_v1 on every logits row; at
     T_q > 0 the two-disclosure refutation convicts a token that is not the argmax of the keyed
     row and acquits the one that is; the seed and T_q are inside fp_job_id_v3.
Z9   ADR-0044 F4–F6 hold verbatim: neither k, nor a row's n_ctx, nor the sampling fields enter the
     beacon, the panel or the ticket; the existing golden vectors pass unchanged.
Z10  A row wider than the chunk arm carries is refused at admission BY NAME while the arm admits
     only FamilyCertified or caps the count below the ruleset's (Decision 6); a worker whose ladder
     is not the ruleset's refuses to serve a row the ruleset admits (Decision 8).
Z11  The class's admitted width satisfies all three bounds at once — the close (Decision 6), the
     ladder (Decision 1's count under the ruleset's L), and the seat's window (Decision 9) — and
     the refusal names which bound refused.
```

## 6. Order of work

Nothing below is built before the measurement above it returns; U-00 and U-01 are not this ADR's
design and are its precondition.

| unit | content | done when |
|---|---|---|
| U-00 | **5f's other half — owned by the 5f integrator, not this ADR.** ADR-0080 W5 lands the court's own chunk-group table with the four requirements §1.4 records; W1b is merged on this ADR's branch; until W5 is in the ruleset, `max_close_chunks = 1` on every preset | Z10 green; `misaka-cli palw court-close` files a two-carrier close on devnet and the transition applies it |
| U-01 | **Measure the capture.** Decision 7's fold wired into the free-prompt worker; the per-token ratio against the un-captured forward on the §1.5 host and job | the ratio is a number in this ADR; Z6 green |
| U-02 | Decision 1 — `AttnFused` in the IR, the profile projection, the engine (`engine_a16.rs`), the adjudicator, the fuzz gate; graph-v5 profiles for both families | Z1 green; no `KvScaled` row projects from a v5 profile (Z0's first half) |
| U-03 | Decision 2 — the dissection arm in `palw_step_refute` / the court object; Decision 3 — the k-ary ladder in `palw_bisect` and the move derivation in `PalwCourtParamsV2`; Decision 4 — the cache half of the ladder rule reads the class's map | Z2, Z3, Z4 green |
| U-04 | Decision 6 — `max_close_chunks` derived from the flat terms; `derive_court_cost_v1` prices `AttnFused` by its bottom opening and its per-move disclosure | Z0 green at all four contexts on both families |
| U-05 | Decision 9 — the seat's recompute path (`kaspad/src/palw_panel.rs`), the checkpoint-root check, the per-class `rate_seat_prefill` in the row and at admission | Z5, Z11 green on devnet with three seats |
| U-06 | Decisions 10, 11, 12 as TESTS before features: the decode-leaf numerator, `decode_token_select_v2`, the published ceiling | Z7, Z8 green; the census's rows re-asserted |
| U-07 | Decision 5 — the prompt-id fence armed with the rows; every moved id re-pinned and listed | the u04 blast-radius test passes at the new pins. **Landed as the FORM, not the arming: the fence is dormant at the 512 row and refuses arming until its inputs exist (§10.7)** |
| U-07b | Decision 5's symmetric half — the OUTPUT ids as a tiled Merkle root, without which the close stops being flat above `n_ctx` ~4,096 (§8) | **open; not in this cut** |
| U-08 | the graph-v5 rows registered through ADR-0075's route and drilled (`misaka-palw-fp-devnet-e2e.sh`): dense and hybrid at 512, then 8,192, then the width Z11 admits | ONE job id in three places (ADR-0077 §6's gate) at each width, with a dispute drilled to the bottom tile at the widest. **Partly: the DENSE 512 row is registered and the devnet drill's stages 1–3 pass; 4–8 are blocked by name on the shipped artifact's all-zero tokenizer commitment; the hybrid is not registered at all (§10.7)** |
| U-09 | the pages: `testnet11-join-mining.md`'s prompt section states the three bounds, the stages, the ceiling of Decision 12, and what "private unless disputed" means | no page promises a width a bound refuses |

**Done when** a person gives a graph-v5 row a prompt and receives an answer whose combined length
is the width Z11 admits — thousands of tokens on the dense tier, on the fleet as measured — streamed
at the runtime's speed; the chain holds one claim for it; five seats certified it having fetched a
bounded number of bytes each and recomputed the rest; a deliberately corrupted execution at that
width was convicted through a dissection whose every move fit one carrier; and the same leaves as
one claim and as any other arrangement of the same answer earn the same, because there is only one
arrangement left.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0080 §3 Decisions 1, 2, 4, 5, 6, 7 — one job as N verification segments | **withdrawn** (refuted before implementation; §1.1). Decision 5's "tile-addressed anchor" survives as Decision 4 here, in its honest role: the bottom of a dissection |
| ADR-0080 §1 and Decision 3 | kept: the measurement is §1.2's starting point and the invariant is Z7, as the census tests it |
| ADR-0080 design A (`DEFAULT_MAX_CLOSE_CHUNKS = 27`) | kept as the BRIDGE and re-derived (Decision 6): the count is `chunks(widest term over the genesis set)` — 14/27 for the graph-v2/v3 context rows, ~~one to three~~ **1 (dense, 81,599 B at arity 2) and 4 (hybrid, 274,460 B)** for graph-v5 rows; the transport is W5's own table, the 5f integrator's. **The constant stays 27 by hand at this cut and the derivation ships inert — Decision 6, audit D M-5** |
| ADR-0081 §3 Decisions 1, 2, 4–9 — the prompt as a prefill state chain | **withdrawn**. What a long prompt needed was never a chain of prefill segments; it was a court that never carries the history (Decision 2) and a seat that recomputes it (Decision 9) |
| ADR-0081 Decision 3 — Merkle prompt ids | kept; ~~ARMED with the rows~~ **implemented and DORMANT at the cut — the flat ids are 82,080 B against an 83,333-byte carrier at `n_ctx` 512, and the fence refuses arming until its inputs exist (Decision 5, corrected)** |
| ADR-0081 §1.1 — "graph-v4 makes a per-leaf attention opening tile-sized" | half right, as its own status block found: the OPENING is tile-sized, the CLOSE was not; Decision 2 is what makes the close tile-sized |
| ADR-0077 Decision 8 — the seat asks the executor for the interval's checkpoint chunk | amended by Decision 9: the seat recomputes the state and fetches the rows it compares; the draw, the exactness and the verdict's weight are unchanged |
| ADR-0077 Decision 11 — admission prices the checkpoint interval | kept for the recurrence; for attention the interval is not the price — the dissection's bottom is (Decision 4, 6) |
| ADR-0077 Decision 12 — `COURT_MAX_STEP_LEAVES = 2^32`, binary rounds | the constant stands; the ladder's DEPTH stops being the binding constraint because rounds are k-ary (Decision 3), and the leaf COUNT returns to the base count Decision 12 was sized against (Decision 1) |
| ADR-0077 Decision 13 — rows at 512, 2,048, 8,192 per family | re-sized: a row's width is the minimum of three derived bounds (Z11), not a rotary table; 512 is the first drill and no longer a ceiling |
| ADR-0077 Decision 14 — the canonical footprint is at least `n_ctx / 8` | restated in decode leaves (Decision 10) |
| ADR-0077 §8 — KV continuation across turns | still not decided (§8) |
| ADR-0074 Decision 5 — the quantum is an eighth of the canonical job | honoured; on the free-prompt lane the numerator is decode leaves (Decision 10) |
| ADR-0072 — the ticket is the execution | honoured: a sampling seed is inside the job id, so one inference is one draw (Decision 11) |
| ADR-0040 Decision E — the order of accumulation cannot change the result | spent, not amended: it is the premise that lets a dissection reorder a reduction across tiles and children |
| ADR-0026 — borrow Ambient's architecture, refuse its proof model | honoured and itemized (Decision 13); the dissection is the one part Ambient's shape lacks because tolerance never disputes |
| ADR-0028 — sampling decides nothing; only the court convicts | honoured: a seat's recompute convicts nobody; the court's dissection does |
| `palw_bisect` — "the pinned midpoint of the disputed interval; ≈ log₂(space) rungs" | amended by Decision 3: the midpoint becomes `k − 1` pinned cut points; the contract's pinned index space, round bound and terminal handoff are otherwise unchanged |
| `base0_decode_token_select_v1` — the argmax with lowest-index ties | kept as `T_q = 0` of `decode_token_select_v2` (Decision 11) |

## 8. What is deliberately not decided

* **The output ids as a tiled Merkle root — U-07b**, the symmetric half of ADR-0081 Decision 3
  (Decision 5 names it and does not take it). Measured limit, U-04's Z0 sweep at the `2^32` fence
  with Merkle prompt ids: the dense graph-v5 close is flat to about `n_ctx` 4,096 and is 303,640
  bytes at 32,768, where the binding node becomes `EmbedLookup` and the GENERATED ids (`decode × 4`
  bytes) are the term. Linear in the ANSWER, not the prompt, so "thousands of tokens" holds and
  "tens of thousands" does not. Not in this cut, not fenced, listed in §10.7.
* **KV continuation across jobs** (`ContinueFrom{parent_receipt}`). ADR-0077 §8's exclusion stands.
  Decision 10 removes the reason it looked urgent — a re-sent history no longer earns anything —
  and Decision 9 removes the reason it looked cheap: a seat would have to trust a parent's cache
  or recompute it anyway.
* **Who pays for a long prompt.** Decision 10 says the chain does not. Whether the requester pays
  the executor a fee for prefill, in what unit, and through what market, is a product decision
  outside consensus. ADR-0077 SA-1's exposure budget is the only bound a public gateway has today.
* **The value of `k`, of `max_close_chunks`, of a row's width.** All derived (Decisions 3, 6, 9,
  Z11); the derivations are written and the numbers are not.
* **Beyond a million positions.** Decision 1's base count puts the dense tier at `2^32` leaves
  near 43,000 positions and the hybrid near 14,000; `2^36` reaches eight and sixteen times that at
  ONE more 16-ary round each, but the executor's hashing and the seat's recompute grow linearly
  and the artifact's rotary table must be re-converted wider. Those are measurements, not
  decisions, and U-08's last row is where they are taken.
* **Whether prefill should earn a DISCOUNTED rate rather than zero.** Zero is the rule that
  cannot be gamed; a discount is a rule that can be, at the discount's rate. Not opened here.
* **A fleet mode in which seats FETCH the cache** over a private network. Refused as a protocol
  default (R4); a deployment may do it below the protocol and gains nothing in consensus.
* **Integer GPU kernels, encrypted DA, receipt transfer** — ADR-0077 §8's list stands.

## 9. Number hygiene

This is ADR-0082; ADR-0081 is the last on this branch (`palw-adr0080-verification-segment`), and
`main`'s README records 0080 as the next free number with 0080 and 0081 resident on branches. A
concurrent claimant renumbers the later writer, per ADR-0036 Decision 5.

## 10. Implementation record — 2026-09-03 / 2026-09-04

Numbered 10 and not 8: §8 and §9 were already spent, on what is not decided and on the numbering,
and renumbering a section other documents cite is how a reference stops resolving.

Built on `palw-adr0082-impl` by thirteen streams and seven fixers over two days, from a base of
`palw-testnet-5f` plus `palw-adr0080-verification-segment`. Head at writing `76b790f4`:
consensus-core **1,814 passed / 3 failed** — the three are the genesis pins the 5f integrator
re-pins at the freeze — misaka-palw-base0 **328 / 0 / 2**, cli 74/0, kaspad lib 67/0.

**§1's discipline applied to this section: every figure below names the COURT (shipped binary, or
the dissection court and its arity), the LADDER, the class's MAP, the EVIDENCE ROUTE, the PROMPT-ID
FORM, and the stream that measured it.** Several figures in the first draft were wrong for exactly
the reason §1 gives — quoted under a configuration they were not measured in — and every one of
them is struck in place above and restated, never overwritten: Decision 1's fence widths and its
574, Decision 3's selected arity and its move table, Decision 5's arming, Decision 6's close and
bottom-opening figures and its "flat" sweep, §4's move and bottom-opening bullets, §7's two rows.

### 10.1 The close: one carrier, and the arity is part of the number

Dense graph-v5, the row the genesis registers, all under: dissection court, tiled v3 state chunk
map, checkpoint evidence route, Merkle prompt ids, per-position checkpoints (Decision 4 as
amended), Merkle output ids NOT taken.

All five rows below price openings at the same ladder rules (`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`,
`2^32`); what differs is the ARITY and the route.

| row | arity | close | carriers | binding node |
|---|---|---|---|---|
| dense graph-v5 @ 512 — **the registered row**, priced under the ruleset that registers it | **2**, derived | **81,599 B** | **1** | `attn[7]` `AttnFused` |
| the same row, priced under the sweep's court | 16 | 82,719 B | 1 | `attn[7]` `AttnFused` |
| the same row at `n_ctx` 4,096 | 16 | 82,911 B | 1 | `attn[7]` `AttnFused` |
| the same row, bottom charged at the **cache-write** route (before stream K) | 16 | 216,019 B | 3 | — |
| hybrid graph-v5 @ 512 — **unregistered** | 16 | 274,460 B | 4 | the recurrence's replay evidence |

`82,719 − 81,599 = 1,120` bytes, and the row's own test attributes the whole of it to the per-move
disclosure at arity 16: the arity is not a footnote on a close figure, it is a term in it. The RC
and the devnet rulesets both DERIVE 2 with the v5 row in `genesis_objects` (window 3,000, turn
deadline 42, ruleset ladder `2^26`, 512 positions at a 16-position tile, an 8-lane disputed site) —
not the 4 an earlier sweep reported, not the 16 the ADR worked its table at — and the test prints
its whole configuration on the line beside the number
(`misaka/palw/base0/src/classes.rs`, `consensus/core/src/palw_context_ladder.rs`), so a reader who
moves the arity is told by how much.

Corrected above: Decision 6's 80,504 / 200,732 and its "flat" sweep; §4's bottom-opening bullet.

### 10.2 Decision 4 amended: a checkpoint at every position, and one route refused

Stream K, audit A, fixers FA and FB. Measured at the RC's `2^26` ladder on the dense graph-v5 row
with the tiled v3 map:

| the dispute's bottom opening at | route | bytes | carriers |
|---|---|---|---|
| a PREFILL position (the shipped per-decode-call cadence covers none of them) | cache-write | **149,953** | 1.80 |
| the same at the `2^22` / `2^32` ladders | cache-write | 133,057 / 175,297 | 1.60 / 2.10 |
| every position class — prefill, first decode, tile-aligned, straddling, last | **per-position checkpoint** | **40,461** | **0.49** |
| the hybrid, same | per-position checkpoint | 73,741 | 0.89 |

The anchor is at `p + 1` (residue empty, position 0 anchored), which took the close from 93,367
(2 chunks) to 82,719 (1) at arity 16. Retention is **0 state bytes a position** — the fold keeps
no chunk list at all (`the_folds_retention_is_constant_and_the_alternative_is_quadratic`; a
chunk-retaining capture at 16 positions would hold 1,114,112 B) — and 142 leaf bytes a position.

**And the cache-write evidence route is REFUSED for a class that checkpoints every position.**
Audit A found a K/V SERIES SWAP admissible on that route: the openings were bound to the query but
to no coordinate of their own, so an executor could answer with the wrong series and a correct
recompute would convict an honest responder. The fix binds the series to the `KCacheWrite` /
`VCacheWrite` roles and refuses the route by name where `every_position_is_checkpointed`
(`the_cache_write_route_is_refused_by_name_where_every_position_is_checkpointed`). This closed the
arming blocker on `palw_kary_court`: the checkpoint route is sound, and it is now the only route a
graph-v5 class plays. Z3, which was FALSE on the cache-write route at the `2^32` fence
(175,297 dense / 139,777 hybrid), holds.

### 10.3 What the capture costs — the half Decision 4 asserted and did not price

Fixer FB, audit B H-3, measured at `n_ctx` 512 on the dense shape (28 attention layers,
`kv_row = 2 × 128 × 4 = 1,024` B), `the_per_position_capture_touches_one_tile_a_position`:

| form | bytes serialised per job |
|---|---|
| whole-cache, one checkpoint's chunks re-serialised at every position | **7,530,872,832** |
| with per-chunk hash memoisation | **696,516,608** — 11× less |

**The residual is the MAP's, not the memo's.** The leaf hash binds the chunk INDEX
(`state_chunk_leaf_hash_v1(map_id, index, bytes)`) and the tiled map indexes
`(kind · layers + layer) · chunks_per_slice + block`, so when a slice grows a block — once every
`PALW_ATTN_HISTORY_TILE_V4 = 16` positions — every later index MOVES and a chunk whose bytes did
not change is a different leaf. Only a second copy of the cache or an index scheme that does not
move could remove it, and this capture is allowed neither. Decision 4's claim that the cadence
"costs the executor nothing" is corrected here and in the code's own doc: it costs 696.5 MB of
serialisation a job at 512, retains none of it, and the arithmetic that says so is asserted as an
ORDER against derived bounds rather than pinned to a constant.

### 10.4 The ladder, the clock and the arity, derived jointly

Fixers FA and FD, audit A M-2 / audit D H-2b/H-2c. Two corrections compose: the leaf ladder is
BINARY (a session plays the shipped `PalwBisectLadderV1`; only the history search is k-ary), and
the responder's root claim is a move. Honest count:
`2 × (⌈log₂ L⌉ + ⌈log_k (n / 16)⌉) + 2 + 1`, and the derivation selects on
`moves × deadline + assembly_reserve ≤ window_court` — the same inequality admission admits on,
which is what it was not before (the derivation handed admission a value admission then refused).

| configuration | arity | moves | DAA | verdict |
|---|---|---|---|---|
| **RC as it ships** — `2^26`, its own 512 row (32 tiles), 42-DAA clock, 27-carrier reserve | **2** | 65 | 2,730 + 216 = **2,946** | fits, 54 DAA of room |
| the RC at a 4,096 row | 4 | 63 | 2,646 + 216 | fits (arity 2 is 71 moves, 2,982 + 216 — misses by 198) |
| **`2^32` fence, 131,072 row** — the configuration §3's table is worked in | **none** | 73 (cheapest, arity 32/64) | 3,066 before the reserve | **refused, `None`** |
| every shipped preset, zero history | 2 | its own worst case | inside its own window | unchanged by this ADR |

The RC's 42-DAA clock is not typed anywhere: `palw_court_turn_deadline_for_history_v1(3,000, 2^26,
2, 27, 512, 16)` returns `(2, 42)` — the joint form, counting the history rounds and the root claim
in its divisor. The older `palw_court_turn_deadline_v1` returns 51 for the same window, which is
the clock for a ruleset with NO history to dissect, and at 51 no arity fits at all. Both are kept,
because a ruleset without a fused row genuinely derives the second one.

The class WIDTHS under this court (fixer FD, audit D H-5, `2^32` fence, swept through the gate's
own door): graph-v5 dense admits `n_ctx` **16,384**, hybrid **13,105**, and the refusal one wider
is `DeeperThanTheLadder` or `CourtWindowTooShort` — never the close. `574` is and always was the
**graph-v2** row's width at `2^26`.

### 10.5 What a fused row costs with NO dissection court

Measured by the 5f integrator at `2^26` through `derive_court_cost_shaped_v1` (the shaped entry;
the plain walk at `2^22` refused both rows, which is what made the `2^22` sweep of §10.6 D2
load-bearing):

| row | close under a court with no dissection | chunks | against the RC's 2,250,000-byte ceiling |
|---|---|---|---|
| graph-v2/v3 dense @ 512 | 1,999,729 | 24 | fits |
| **graph-v5 dense @ 512** | **3,446,708** | **42** | **over by fifteen carriers** |

So the sentence this ADR and the genesis card both carried — a fused row on a ruleset whose
`palw_kary_court` is dormant is "admitted and unprosecutable" — is not what the code does. It is
**refused at acceptance**, twice over: by name before anything is priced
(`FusedAttentionNeedsTheKaryCourt` — *"the class carries a fused attention site and this ruleset's
court has no dissection to try it with"*), and by fifteen carriers if it ever reached the walk.
`PricedForADifferentCourt { priced, court }` refuses the other direction, a row priced for an arity
the court does not run.

### 10.6 The audit of 2026-09-03, and what closed it

Five auditors on a read-only checkout of `89a991ab` — A court soundness, B capture and checkpoints,
C economics and the split close, D admission, fences and layering, E the fused op and the engine —
then the fixers FA and FA2, FB, FC, FD and FD2, FE. Criticals and their closures:

* **A (court), five criticals, all bindings that were missing.** The root claim's history length is
  DERIVED, never read off the wire (truncation acquitted, over-declaration was a DoS); `S*` is
  validated into a band and the kernel moved to `i128` (an unvalidated `S*` overflowed `e × recip`
  and PANICKED inside block validation — a one-block network kill); bottom openings carry full
  COORDINATES on query, K and V (decode rows addressed by call), so a query bound to nothing is
  refused; the anchor is DERIVED (`palw_checkpoint_covered_for_step_v1`, `WrongAnchor` and
  `RowsAfterOnAPerPositionClass`) rather than "any checkpoint in the leg"; and the root claim has a
  CLOCK at Terminal, so silence no longer wins challenger-side. Beside them: `MixedEvidenceRoutes`,
  `out_tile` verified, and the move count of §10.4.
* **B (capture), three criticals.** The fold now ANSWERS for chunks — `base0_checkpoint_chunks_at_v1`
  had no caller, so every graph-v5 seat's material check would have been `Mismatch`, no interval
  would open and no anchor would exist; ONE cadence unit — the interval, the seat and the panel
  counted DECODE CALLS where the leg counts POSITIONS, which is `CheckpointRootMismatch` on every
  honest v5 claim (a fifth site was found by the tests written for the first four); and the hybrid
  v5 producer PUSHES its checkpoints (`qwen36_push_checkpoint_v1`) instead of filing
  `CheckpointCaptureIncomplete`.
* **C (economics), one critical and the close's economics.** The court's one slot a block is spent
  only by an ADMITTED move — it was spent by an unauthenticated object before validation, so one
  transaction a block starved every close completion and convicted the honest declarer; the
  declaration is legal at Terminal only; the assembly deposit (`palw_close_assembly_deposit_v1(n)` =
  1,250,000 sompi × n) is charged on EVERY non-delivering ending, through the removal funnel rather
  than at two of five sites; chunk deltas are O(1) in the group (26 arrivals journal 2,611,201 bytes
  against 67,709,897 in the swap form); and one prompt is one live claim per bond —
  `fp_work_id_v1` is `(class, prompt, bond)` with `decode_tokens_executed` DROPPED, so one
  inference truncated twice is one work id and not two tickets.
* **D (admission and fences), no criticals, five highs.** One spelling of the arity; the v6 gate on
  the acceptance path AND on the genesis door (a fused row under a dormant fence is refused at
  both); and the two dormant fences `palw_fp_decode_rules` and `palw_prompt_ids_merkle` REFUSE
  arming until their inputs exist, rather than being armable against a transition that would refuse
  by name.
* **D2, the sweep that followed it: the executor's `2^22` no longer bounds the chain.** Six more
  sites carried `PALW_STEP_MAX_LEAVES` where they meant the ruleset's ladder — the free-prompt
  door, the walk's `validate_*_under_ruleset_v3`, the certificate count, the catalog entry and
  `derive_court_cost_v1`'s six callers, the schedule, and the step leg. The last one was
  CONVICTING an honest 512-row producer by name (`StepLeafCountNotCanonical` above `2^22`), and the
  registration helper counted the same way, so no chain could have registered the row at all
  ("4223328 exceeds 4194304").
* **E (fused op and engine), three criticals shared with A.** The shipped constructor now COMPILES
  THE PLAN: `Qwen25A16Backend::new` had `plan: None` and refused the v5 row by traced route
  ("per-layer declares 24 against 27 recorded"), so the fused node executed only through
  `from_registered_profile`. A second cross-machine determinism digest pins the fused corpus
  (a16 `236888781074c28a…`, qwen36 `7e5f14c2b0d66fc5…`).

**State version 19 → 20**: the class state records `fused_attention`, written at the one
construction site and carried on `ClassRegistered`, because the court has to answer "is the
terminal leaf `AttnFused`" from chain state to give the root claim its clock. The genesis door
refuses a fused row without an admission carriage, one disagreeing with the catalog, or one whose
carriage is not the class.

### 10.7 Status, and what is open — by name

The ADR is **IMPLEMENTED on `palw-adr0082-impl`, pending the testnet-11 Relaunch 5f cut.** The
genesis registers the **graph-v5 dense 512 row** (`a16_graph_v5_row_v1`, model id
`Qwen/Qwen2.5-1.5B/graph-v5@512`, canonical job (63, 2), 6,630,544 leaves, class id derived and
printed by its own test rather than typed). It REPLACES the graph-v2 `n_ctx` 16 dense row, which
can serve no free-prompt job at all — the shortest real prompt is 134 tokens — and it takes the
existing dense share constant; no new one is minted. The hybrid stays unregistered (4 chunks, and
the split acceptance is not in this cut). At genesis `palw_kary_court` is armed (`always()`;
without it §10.5 refuses the row), `palw_context_ladder` and `palw_uncertified_weightless` are
armed, and `palw_prompt_ids_merkle` and `palw_fp_decode_rules` are dormant and refuse arming.

Open, each by its own name, so none of them is discovered:

* **U-07b — the output ids as a tiled Merkle root** (§8's new bullet). Not in this cut.
* **Derive the worker's class from the artifact it loads.** The worker names it with a `MODEL_ID`
  constant today; the artifact is what decides which graph it can execute, and the constant is the
  sixth instance this cycle of one quantity spelled in two places.
* **Wire the close-chunk derivation to the shipped rows** (`palw_shipped_court_rows_v1()`).
  Decision 6's builder is written, tested and INERT; `DEFAULT_MAX_CLOSE_CHUNKS` stays 27 by hand
  for the reason recorded at Decision 6.
* **The seat's replay from the prompt under zero retention.** A folded interval's executor replays
  from the prompt for each opening it serves — one forward pass per opening. The alternative keeps
  the live cache across the claim's life, which is a retention shape change at the anchored branch
  of `base0_open_fp_interval_sparse_v1`, and it is not in this cut.
* **The drill's stages 4–8.** Stages 1–3 pass; 4 onward are blocked BY NAME on the shipped
  artifact's all-zero tokenizer commitment, and need the re-bound artifact.
* **The fourth family `PALW-QWEN25-A16-V5`.** Fusion replaces four kernels with one, so no profile
  reaches both kernel sets and a union family cannot be drilled; the family's kernel ids are read
  off the fused fixture's own profile. Until it merges, the row's coverage gate is red by an
  equality naming family `PALW-QWEN25-A16` / kernel `09b81d17ed5a73ef`, and the row would register
  weightless.
* **Two fixers were in flight when this section was written** (2026-09-04, 18:35 JST): the genesis
  registration BUILDER that mints the row's `ClassRegistered` with its admission carriage, and the
  remaining `2^22` sites on the base0 seat, replay, drill and tool paths.
* **U-09 — the dissection responder does not exist in any shipped binary.** `CourtAttnRootClaimed`
  is constructed nowhere outside `palw_state_v2`'s test module; `kaspad`'s panel has no arm that
  builds one. Decision 2's clock at a fused `Terminal` therefore convicts every accused dense-tier
  producer by silence, honest or guilty, for the price of one bond and a 42-DAA wait — and the
  opening-rung mercy beside it cannot apply, because a converged ladder has `round() > 0` by
  construction. Recorded as O-8 by the 2026-09-05 mainnet audit as "a passive liveness gap waiting
  on a feature", and re-found by the 2026-09-06 pass as a funded, steerable, evidence-free attack
  that a card ARMS (C-2/H-5). Mitigated, not closed, behind `Params::palw_court_responder_coverage`
  (`None` on testnet-11 and devnet, so both remain exposed): past that fence the fused terminal's
  silence ends the session without convicting or fining anyone. **The fence is a placeholder for
  this item and is retired by activation the day the responder ships**;
  `no_shipped_binary_files_a_court_attn_root_claimed` is the pin that goes red on that day.

## Mission alignment amendment — 2026-10-07

flat-in-contextなcloseとchunk上限は維持するが、上限内のproofの存在と外部生成可能性は別gateである。held/FOLD/checkpointのprefixやcommitted fused outputをproducer内部から読まないfresh verifierで確認する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0082-the-close-is-flat-in-the-context.md -->

</details>

<a id="source-0085-the-close-is-assembled-from-what-was-served"></a>

<details>
<summary>0085-the-close-is-assembled-from-what-was-served.md — 原文 L1–L215</summary>

[原文を開く](../adr/0085-the-close-is-assembled-from-what-was-served.md) · SHA-256: `f9494fbcd0b9cbbb3998e52f5f0dcfa658dee98e528cfc844ee3f07147548ae1`

<!-- ADR_SOURCE_BEGIN:0085-the-close-is-assembled-from-what-was-served.md -->
~~~~~~~~markdown
# ADR-0085: The close is assembled from what the executor served — a disputed tile, not a capture

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED (2026-09-04). §6 items 1–3 LANDED 2026-09-04 (see §7); items 4–5 are being built under the 2026-09-05 goal, with ADR-0086 Decision 6's transport (the block-leaves annex) riding the same lane. Consensus-inert
by construction: the refutation object (`PalwExecutionStepRefutationV1`), the adjudicator
(`check_execution_step_refutation_v1`) and every court object are unchanged; what changes is how a
party that holds no capture ASSEMBLES the close, and what the executor serves it.
**Builds on:** ADR-0084 (the seat needs the answer's ids, never the history; the material lane
serves the envelope for a capture over the cap), ADR-0077 Decision 8 / SA-2 (the interval lane,
authenticated), ADR-0082 Decisions 4, 7 and 9 (the tile-addressed bottom; the fold; the seat
recomputes the cache), ADR-0073 Decision 1c (a free prompt is carried into the close), audit3 S-01
(the close is assembled from the ACCUSED's bytes by whichever party makes it).
**Amends:** nothing. ADR-0084 §8 named this as U-07c and left it undecided; this decides it.

## 1. What was measured

On the ADR-0084 branch, a v5 attempt claim's material is **784,693,059 bytes** (the loopback
devnet, 2026-09-04 19:30 JST — the card's 748 MB to the byte), the executor stages a 336-byte
`ATA1` beside it, the transport refuses to announce it, and a seat obtains the envelope, binds it
and can certify without the capture. The court cannot close the same claim from the challenger's
side. `kaspad/src/palw_panel.rs`'s terminal arm reads:

```
let Some(accused) = accused_capture.as_deref() else {
    // **And ASK for it.** … pull_for_close.push(duty.claim_id) …
    "the close needs the ACCUSED capture and this node holds none — pulling"
```

and the pull now answers with the envelope, which `verify_material` does not match, so the arm
stalls at that line until `window_court` ends. `refutation_for_free_prompt_index(&accused_bytes,
index, ids)` (`qwen25_a16_backend.rs::refutation_with_prompt`) needs the WHOLE retention: it
re-derives every step tile (`tiles_from_material_v1`, which on a fold replays the run), the rows
root over every logits row, and the checkpoint chunks. For a class over the cap the only party that
holds those bytes is the accused. An honest executor closes its own case; a dishonest one does not
close it, and nobody else can. ADR-0084 moved that failure from certification to the court; this
ADR removes it from the court.

## 2. What a close actually needs, term by term

`base0_refutation_from_capture_capped_v1` builds the refutation from six things. Where each can
come from when the challenger holds no capture:

| term | in the refutation | source without the capture |
|---|---|---|
| the binding | `binding` | every interval opening carries it (`Base0FpIntervalOpeningV{1,2}.binding`), and it is what `execution_root` commits |
| the target's output tile | `output_preimage`, `output_opening` | **the accused's, and only the accused's**: for a real fraud the challenger's recompute differs at exactly this leaf, so its own tile opens against nothing. Must be SERVED. The opening path is derivable from the interval's range opening (leaf hashes + siblings, `step_merkle_range_siblings_v1`) once the leaf hash is known |
| the activation inputs (prior tiles of the same interval) | `inputs` rows opened against the step leg root | the challenger's own replay of the interval (`base0_fp_replay_interval_v1` with tiles captured), CHECKED against the accused's committed leaf hashes in the range opening — the dissection stops at the FIRST disagreement, so every prior leaf of the accused equals the challenger's; a prior leaf that does not is a different first fault, and the seat's `Fault { leaf_index }` names it |
| the KV anchor (checkpoint operands) | `kv_checkpoint` | the challenger's recomputed state at the interval's checkpoint (ADR-0082 D9), whose tiled root the seat already compared to the accused's committed checkpoint leaf; the leaf and its opening come with a V2 opening (`Base0FpCheckpointClaimV1`) |
| the weights | `operand_openings_for` | the challenger's own artifact, against the class root — unchanged |
| the decode pin | `decode_tokens: TiledV1 { rows_root, generated_token_ids }` | `generated_token_ids` off the ADR-0084 envelope; `rows_root` is a 64-byte value the accused committed inside `trace_root` (`tiled_logits_outer_root_v1(ctx, rows, rows_root, generated)`) and is verifiable against the binding by `check_tiled_decode_pin`. Must be SERVED — it is a function of every row |
| the prompt | `prompt_token_ids` | the envelope (`FPA1`) or the anchor's derivation (attempt lane) — ADR-0084 |

So exactly two things must come from the accused: **the disputed tile's preimage** and **the rows
root**. Everything else the challenger recomputes and checks against roots the accused committed.

## 3. Decisions

**Decision 1 — the interval opening gains a close annex, served on the same authenticated lane.**
`Base0FpIntervalOpeningV3` = V2 + `close: Option<Base0FpCloseAnnexV1 { rows_root: Hash64,
disputed: Vec<(u64, PalwStepTileLeafV1)> }>`. The executor's `open_retained_interval` consults the
chain for open court sessions on the claim (`palw_court_duties_v2` names the session and its
`terminal_index`); when a terminal index falls inside the interval being opened, the annex carries
that leaf's tile and the rows root. No new P2P message and no new request tag: a challenger asks
for the interval containing the terminal index over the existing `PalwIntervalOpeningRequest`,
and the answer is one opening, bounded by the same cap (`PALW_INTERVAL_OPENING_MAX_BYTES`; a tile
is one `PalwStepTileLeafV1`, kilobytes). A V3 opening without an annex is a V2 opening to every
existing reader: the seat's replay ignores the annex.

**Decision 2 — the refutation is assembled from an opening and the challenger's own replay.**
`base0_refutation_from_opening_v1(profile, ctx, opening_v3, replay: &Base0StepTilesV1 (the
challenger's tiles for the interval, hashed and CHECKED leaf by leaf against `opening.range.
leaf_hashes` up to the target), checkpoint_state, target, prompt_ids, cap)`:
1. every leaf hash of the challenger's replay below the target must equal the accused's committed
   hash in the range opening — otherwise the first unequal leaf is the real target and this call
   refuses by name (the caller reopens the court's question there);
2. the output tile is the annex's, its opening is derived from the range opening;
3. the inputs are the challenger's prior tiles with openings derived the same way; the KV anchor
   is the recomputed state's chunks opened against the checkpoint leaf the V2 anchor names;
4. the pin is `TiledV1 { rows_root: annex.rows_root, generated_token_ids }`, which
   `check_tiled_decode_pin` binds to the binding before any arithmetic.
The object this produces is byte-for-byte the object the capture path produces for the same
(claim, index) — pinned by a test that builds both from one fixture run.

**Decision 3 — the terminal arm tries the opening path before it pulls.** When the closer holds no
matching capture: derive the interval containing `terminal_index`, request it (the interval lane
the seat already drives), and on arrival assemble with Decision 2. The whole-capture pull stays as
the fallback for classes whose captures fit. The stall reason changes from "holds none — pulling"
to "waiting for the interval's close annex", which is a fact about the accused's serving.

**Decision 4 — a seat's `Fault { leaf_index }` is a court case it can prosecute.** The seat that
found the first unequal leaf during its interval replay already holds the challenger's side of
Decision 2's inputs (its own tiles for that interval and its recomputed state). ADR-0077 Decision 8
says a sampled verdict never slashes; this ADR does not change that. It says the seat may open a
court AT that leaf with `--palw-challenge`, and Decision 3 lets it close.

## 4. Costs

* Per close: one interval opening (≤ 4 MiB by the lane's cap, kilobytes in practice) plus the
  challenger's replay of one interval with tiles captured — the interval's rows, not the run's.
* Executor: one chain read (open sessions on the claim) per interval served; one tile per open
  session in the interval.
* Nothing on chain moves.

## 5. Invariants the tests must hold

```
X1  For one fixture run, base0_refutation_from_opening_v1 and base0_refutation_from_capture_capped_v1
    produce byte-identical refutations at every main-step leaf, on both retention forms.
X2  A tampered tile at leaf t: the seat's replay names Fault { t }; the refutation assembled from
    an opening whose annex carries the accused's tile at t convicts under
    check_execution_step_refutation_v1; the same assembly with the challenger's own tile at t
    refuses to assemble (its hash is not the committed one).
X3  A V3 opening without an annex verifies as a V2 opening on every existing seat path.
X4  The annex is served only for a leaf an open court session names; a request for any other
    interval carries none.
```

## 6. Order of work

1. `Base0FpIntervalOpeningV3` + `Base0FpCloseAnnexV1`, encode/decode, X3.
2. Per-interval replay with tiles captured (the fold's `dense_capture_from_fold_v1` restricted to
   one interval), and the leaf-path derivation from a range opening.
3. `base0_refutation_from_opening_v1`, X1, X2.
4. Executor: the annex in `open_retained_interval` (chain read of open sessions), X4.
5. Challenger: the terminal arm's opening path (Decision 3); Decision 4's seat-opened court.

## 7. Implementation record (2026-09-04, `palw-adr0084-served-answer`)

**Landed — §6 items 1–3, base0 and consensus-core only, no node wiring:**

| item | where | what |
|---|---|---|
| 1 | `misaka/palw/base0/src/fp_interval.rs` | `Base0FpIntervalOpeningV3` = v2 + `Option<Base0FpCloseAnnexV1 { rows_root, disputed: Vec<Base0FpDisputedLeafV1 { leaf_index, tile, anchor }> }>`, magic `MSKFPIV3`; the any-decoder hands every seat the v2 view (X3); `base0_fp_interval_close_annex_v1` is the closer's read. The annex names, per disputed leaf, the checkpoint claim its step is anchored to — the leaf and its opening against the checkpoint leg root are the accused's leg's, and an interval opening's own anchor names only the interval's START |
| 2 | `consensus/core/src/palw_step_leg.rs`; `fp_interval.rs` | `step_opening_from_range_v1` and `step_range_siblings_from_range_v1` (paths derived from a range opening, pinned byte-identical to the built ones over every leaf/sub-range of every leaf count ≤ 24/20); `base0_fp_replay_interval_tiles_v1` and `Base0FpIntervalKernelsV1::replay_interval_tiles` (the one required verb; the seat's hash check is provided over it); `base0_fp_challenger_replay_tiles_capped_v1` (the seat's derivation up to the replay, returning the v2 opening and the challenger's tiles) |
| 3 | `misaka/palw/base0/src/legs.rs` | `base0_refutation_from_opening_capped_v1(profile, ctx, intervals: &[(opening, replay)], annex, target, prompt, pin, recomputed_chunks, cap)`; `LegError::CloseFromOpening(&str)`. **X1 holds on the floor fixture**: at every main-step leaf of every interval, the refutation equals the capture path's byte for byte, and a forged annex tile is refused by name |

**Two things the implementation taught the design (recorded in the code):**

* **A step at an interval's first call reads the call before it**, whose tiles sit in the PREVIOUS
  interval's range — so the closer holds two served intervals, the disputed one first and its
  predecessor, and the builder finds each input's holder among them (an input run that straddles two
  ranges is refused by name; none occurred on the fixture).
* **The annex's anchor IS the anchoring decision.** The capture path anchors when the executor's leg
  holds a checkpoint covering the call before the disputed one and builds the long form otherwise;
  the executor applies the same rule when it fills the annex, and the closer anchors exactly when an
  anchor was served.

**Landed — §6 items 4–5 (2026-09-05, node wiring on `kaspad/src/palw_panel.rs`, base0 seams in
`fp_interval.rs`, four backend methods):**

| item | where | what |
|---|---|---|
| 4 | `open_retained_interval` → `PalwExecutionBackendV1::open_fp_interval_with_close` | The executor reads the open court sessions on the claim off the CHAIN (`palw_court_duties_v2` under its own bond, `terminal_index`), never off the request, and serves the interval with `Base0FpCloseAnnexV1` for the named leaves inside it. `base0_fp_close_annex_v1` builds the annex from the accused's own tiles — replayed for the interval from a fold (`base0_fp_interval_tiles_from_fold_capped_v1`), held by a dense retention, the seed row off the opening itself — with the capture path's anchoring rule (a cache-reading step past call 0 carries the checkpoint before it); `base0_fp_interval_opening_with_close_v1` attaches it to the V4 (or V3) bytes. A leaf outside the interval is another interval's; a leaf no session names is served no tile. |
| 5 (D3) | the terminal arm → `close_source_from_served_intervals_v1` → `refutation_from_served_intervals` | When the closer holds no capture matching the claim's roots and the claim is free-prompt: the interval owning the terminal leaf (`fp_interval_of_leaf_v1`) and the one before it are looked up in the held openings; the primary must carry an annex naming the leaf. Missing ones are asked for on the interval lane (25-DAA throttle per pair, `intervals_for_close`, after the loop like the pull) and the stall reads "waiting for the interval's close annex". Held, `base0_refutation_from_served_intervals_capped_v1` replays each with the family's kernels from the seat state (memo warmed as the row check warms it), hands ADR-0085's builder the V2 views and tiles, the annex, the tiled pin (`rows_root` from the annex, the answer's ids off the served payload), and the primary interval's anchor chunks. Attempt-lane claims keep the pull. |
| 5 (D4) | `interval_seat_outcome_v1` → `seat_faults` → the challenger's half | A seat's `Fault { leaf }` / `FaultInRange` is remembered per claim (bounded); the challenger's loop prosecutes a remembered claim whether or not `--palw-challenge` challenges everything (the disputable-claims list is still the chain's, the fault only selects). With ADR-0086 Decision 6 the block is refined to a leaf first. |

Pinned at the base0 layer (`a_close_from_served_intervals_is_the_close_from_the_capture_and_the_annex_changes_no_verdict`):
on the floor fixture, for every main-step leaf of every interval, the served path's close equals
the capture path's byte for byte under the same tiled pin; the annexed V4 verifies exactly as the
plain one (X3 on V4); an annex naming another leaf does not close this one. What is NOT
exercised: a live court reaching a close through this path — that still needs a faulty producer
on a devnet (the two-node court harness §7's first paragraph names does not exist).

**What the implementation taught.** The chunks a served-interval close hands the builder are the
primary interval's anchor state — the checkpoint before the interval's first call — which on a
per-call class at checkpoint interval 1 (graph-v5) is the checkpoint before every step in the
interval. On a wider cadence a mid-interval step's anchor is a state the seat's replay passes
through and does not keep; the builder's anchor check refuses it by name rather than closing
wrongly. Recorded in §8.

**2026-09-11 — against a liar ([ADR-0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) §8.5).**
The pin above runs an honest executor, and the path it pins could not close a lying interval. A
V4 opening carries no leaf hashes, so the replay walked the committed root with the closer's OWN
leaves, and a lie inside the interval is exactly what stops that walk. The close now also holds
the executor's leaves for the one block the closer's leaves differ in (ADR-0086 Decision 6's
block-leaves lane). It substitutes them once they fold to the opening's digest, and walks the root
over the committed range. The node asks for that block beside the interval. Against the drill's
one-tile lie on the graph-v5 dense row, the served close is the capture path's object and
convicts.

## 8. What is deliberately not decided

* A served-interval close for a mid-interval step on a class whose checkpoint cadence is wider
  than one call: the seat state the builder needs is transient in the replay; keeping it is a
  recompute-kernel change, taken when such a class exists.
* Whether the annex should ride the answer envelope instead (`rows_root` could; the disputed tile
  cannot, because it is not known until a court names a leaf).
* The attention dissection's bottom (`PalwAttnDissectBottomV1`) already opens tiles on chain for a
  fused node; whether Decision 2 should read the bottom disclosure for those nodes instead of the
  annex is left to the implementation, which will find out which is smaller.

## 9. Number hygiene

This is ADR-0085; ADR-0084 is the last on this branch. A concurrent claimant renumbers the later
writer, per ADR-0036 Decision 5.

## Mission alignment amendment — 2026-10-07

close assembled from served materialという原則を普通の非Panel bondまで適用する。close annexの取得をseatだけに限定したり、court前に必要なmaterialが取れない循環を残したりしない。served valuesはproducerのcommitmentに認証する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0085-the-close-is-assembled-from-what-was-served.md -->

</details>

<a id="source-0086-the-opening-carries-the-fold-not-the-leaves"></a>

<details>
<summary>0086-the-opening-carries-the-fold-not-the-leaves.md — 原文 L1–L308</summary>

[原文を開く](../adr/0086-the-opening-carries-the-fold-not-the-leaves.md) · SHA-256: `9036e60905a3551a24295b1a911381a30dee036ef176f479e528ac64cfe20380`

<!-- ADR_SOURCE_BEGIN:0086-the-opening-carries-the-fold-not-the-leaves.md -->
~~~~~~~~markdown
# ADR-0086 — the opening carries the fold, not the leaves

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED 2026-09-05, implemented consensus-inert on `palw-adr0084-served-answer`
(no fence; the fingerprint does not move). Supersedes the wire form of ADR-0077 Decision 8's
interval opening (V1–V3) for SERVING; the consensus root rule those openings walk
(`step_range_opening_root_capped_v1`) is untouched. Requested by the operator on 2026-09-05 as
ADR-0084 §7.2's first option.

## 1. What was measured (devnet runs 3–6 on `palw-adr0084-served-answer`, 2026-09-04/05)

* A graph-v5 attempt claim's interval 0 opened as **424,359,978 bytes** (node-0, 14 s to
  assemble) and the transport dropped it without a line; the asking seat stayed at "0 openings
  held" until its receipt deadline. The class has 103,008 leaves per prefill position (122,024 at
  the position that selects a token) and 122,024 per decode call; `PalwStepRangeOpeningV1`
  carries 64 bytes per leaf of its range, so interval 0 (the prefill and the first decode call)
  is 424 MB of leaf hashes and a decode interval 7.8 MB, against
  `PALW_INTERVAL_OPENING_MAX_BYTES = 4 MiB`.
* The same class's anchor is CHUNKED: its map is `integer_kv_state_chunk_map_id_v2`, its cadence
  per decode call, and a V1 opening's `anchor.chunks` is the whole KV state at the covered call —
  57 KB per position at 32-bit elements, ≈3.6 MB at the 63-token prefill and ≈33 MB at n_ctx 512.
  A decode interval's V1 opening on this class is therefore ≈12 MB at 16 tokens and ≈41 MB at 512.
* **The seat already computes both of those things itself.** Before it verifies any opening, the
  panel recomputes the checkpoint's state root from the claim's own ids
  (`checkpoint_root_for_context_v1`, memoized as `Base0FpSeatStateV1`, chunks included) and
  refuses the opening when the roots differ; then the verifier replays every leaf of the interval
  (`kernels.replay_interval`) and compares them one by one against the carried hashes. The 424 MB
  carries what the seat is about to compute anyway.
* What the seat cannot compute: the producer's Merkle frontier around the range (`siblings`,
  at most two per level — ≤ 46 hashes on this class), the producer's committed logits row at the
  anchor call (the seed row tiles, which decode to the token the interval starts from — 18,992
  tiles, ≈1 MB on this class, bounded by the vocabulary and not by the job), the checkpoint claim
  (a leaf and its path), and the binding.
* The seat's ceiling `palw_fp_interval_opening_ceiling_v1` prices an opening as
  `positions × row_bytes`; the transport cap's own comment says a family whose openings exceed it
  raises the cap before registration. Neither happened for graph-v5, and no cap reaches 424 MB.

## 2. The requirement

An interval opening's size is bounded by the tree's depth, the fold's block count over the
range, the class's seed row and the checkpoint claim — never by the range's leaf count, never by
the job's position count, and never by the state's size. A seat's verdict keeps its meaning:
Valid exactly when the producer's committed range is the honest recompute. A fault keeps an
address the court can act on. Nothing in consensus changes.

## 3. Decisions

**Decision 1 — the V4 opening carries the fold's digests and the frontier, and no leaf hashes.**
`Base0FpFoldRangeOpeningV1 { first_leaf_index, leaf_count, retain_level, block_roots, siblings }`
replaces `PalwStepRangeOpeningV1` in the served form: `siblings` is exactly the consensus range
opening's sibling sequence (left-then-right per level, bottom-up); `block_roots` are the fold's
retained nodes (ADR-0082 Decision 7's level, `palw_base0_sparse_retain_level_v1(cap)`) for the
blocks lying wholly inside the range, in order; `retain_level` names that level so the form
describes itself. `Base0FpIntervalOpeningV4 { version: 4, interval_index, binding, range,
seed_row_leaf_count, seed_row_tiles, anchor: Option<Base0FpCheckpointClaimV1>, close }`, magic
`MSKFPIV4`. On graph-v5 a decode interval is ≈2 KB of digests, ≤ 3 KB of siblings, the seed row
and the claim; interval 0 is ≈104 KB of digests and no seed row.

**Decision 2 — every V4 anchor is the named claim; chunks never ride.** ADR-0082 Decision 9's
rule for the flat classes becomes the rule for every class: the seat starts its replay from the
state it recomputed for the checkpoint check, which it holds with its chunks
(`Base0FpSeatStateV1.chunks`). `HistoryNotAdmissible` stays what it is for V1 openings; V4 has
nothing to be inadmissible.

**Decision 3 — the seat's leaves are its own.** Verification hashes the served seed row tiles
into the range's first leaves, replays the interval for the rest, assembles a
`PalwStepRangeOpeningV1` from those leaves and the served siblings, and walks the consensus root
rule unchanged. Before the walk it folds its own leaves at `retain_level` over the blocks wholly
inside the range and compares them with `block_roots`; a block that differs is the fault's
address — `Base0FpIntervalSeatVerdictV1::FaultInRange { first_leaf_index, leaf_count }`, the
block clipped to the range — and a root that differs with every digest equal names the range's
edges the same way. The panel treats `FaultInRange` as it treats `Fault`: it files nothing, logs
the address, and the court's bisection is what convicts (ADR-0085 Decision 4 unchanged).

**Decision 4 — both openers serve V4, and the dense tree is built at the ruleset's level.**
The dense opener rebuilt its tree at the constant level 12 while the fold retention keeps
`palw_base0_sparse_retain_level_v1(cap)`; a digest list at two levels would be two wire forms.
The dense opener now builds at the ruleset's level (the root is level-independent — ADR-0084
§7.1 proved the two spellings one rule), reads `block_roots` off `retained_nodes()`, takes
`siblings` from `range_opening_v1` and drops `leaf_hashes`. The fold opener's span replay is
unchanged (it replays the covering blocks' calls to derive the edge siblings); replaying only the
edge calls is an optimisation this ADR does not take.

**Decision 5 — V4 is served for every class from this build; V1–V3 stay decodable.** An old
seat decodes V4 as `Unverifiable` and files nothing, which is what it files today for a class
whose V1 opening never arrived; a class whose material fits the transport cap is licensed by the
whole-capture pull regardless. No request-side version is added — the executor cannot know the
asker's build, and the two outcomes it could produce are the same as the two it produces now.

**Decision 6 — the court's address is a block, then a leaf.** A `FaultInRange` becomes a court
case through the annex lane ADR-0085 Decision 1 defined: a block-leaves request (claim, interval,
block first index) is answered with that block's ≤ 4,096 leaf hashes (≤ 256 KB, inside the
opening cap), the challenger names the leaf from its own replay, and the disputed tile and its
path ride ADR-0085's annex as before. This ADR lands the request and response forms and the
assembly from own leaves plus a served block; the P2P wiring is listed with ADR-0085's items 4
and 5.

**Decision 7 — the seat's ceiling is restated in the opening's own units.**
`palw_fp_interval_opening_ceiling_v1` prices `blocks × 64 + 2 · depth · 64 + seed row + claim +
header`; the transport cap stays 4 MiB. A ceiling in `positions × row_bytes` was shaped to agree
with a form that no longer exists.

**Not taken: the seed from the seat's own recompute.** The prefix recompute feeds the claim's
ids to `forward_no_capture` and never computes a logits row, so the boundary check "the seed is
the anchor call's argmax" needs the producer's row today. Having the recompute select at its last
call would drop the seed row (≈1 MB on graph-v5) for one logits projection per interval; it is
the next cut, not this one.

## 4. What this costs, stated before it is measured

* Opener: no per-leaf vector is serialised; the dense opener still rebuilds 6.6 M leaf hashes
  from tiles to build its tree for interval 0 (≈5 s in a release build), the fold opener still
  replays the covering calls. Memoising the dense tree is not taken here.
* Seat: the replay it already does, plus one fold over its own leaves (hashing ≈ leaves/2 nodes,
  a few hundred milliseconds on interval 0) and the same root walk over its own leaf hashes.
* Transport: graph-v5 decode interval ≈1.1 MB (the seed row dominates), interval 0 ≈110 KB;
  both inside the 4 MiB cap with the class at n_ctx 512 or 2048 — the digest count grows with the
  range, 64 bytes per 4,096 leaves.
* Court: a block-leaves answer is ≤ 256 KB; the on-chain refutation is unchanged (tile + path).

## 5. Invariants the tests must hold

* **X1 (frozen form).** `MSKFPIV4`, version 4; bytes of another family and of V1–V3 are refused
  by the V4 decoder; the any-form decoder returns `Digests` for V4 and the old variants for V1–V3.
* **X2 (one form from two retentions).** On the a16 fixture, the V4 opening from the dense
  tuple equals the V4 opening from the fold byte for byte, for every interval.
* **X3 (the seat's own leaves walk to the root).** Every interval of a floor capture verifies
  `Valid` through a V4 opening with the seat's replayed leaves and the served siblings; the
  served bytes carry no leaf hash.
* **X4 (size is the fold's, not the range's).** Over a synthetic space of 2^22+1 leaves an
  opening of the last 1,000,000 leaves is under 200 KB, and its size does not change when the
  range's leaves are re-randomised.
* **X5 (a fault has an address).** A leaf the producer did not compute yields
  `FaultInRange` naming the block that holds it, on the dense and on the fold route; a tampered
  sibling yields `FaultInRange` naming an edge.
* **X6 (the address becomes a leaf).** From a served block-leaves answer and the seat's own
  replay the challenger names the leaf, and the refutation assembled from it verifies
  (ADR-0085 X1's builder over V4 inputs).
* **X7 (the old seat is safe).** A V4 opening handed to the V1–V3 verifier is `Unverifiable`,
  never `Mismatch` and never `Fault`.
* **Y (the lane, live).** On the loopback devnet a graph-v5 free-prompt claim whose material is
  over `PALW_MATERIAL_MAX_BYTES` reaches `Valid` receipts from seats that fetched V4 openings
  and no material, and is licensed.

## 6. Order of work

1. Wire form, codec, any-form decoder, X1.
2. Assembler and both openers, the dense tree at the ruleset's level, X2, X4.
3. Seat verifier over V4 (own leaves, digests, `FaultInRange`), the panel's handling, X3, X5, X7.
4. Block-leaves annex forms and the challenger's assembly, X6.
5. Ceiling restated; docs; devnet Y; fleet.

## 7. Implementation record (2026-09-05, `palw-adr0084-served-answer`)

* **Landed** (`3d56b57a`, `62fec794`): the V4 form and codec; `Base0FpFoldRangeOpeningV1` with
  the whole-block rule (a tail block counts as whole when the range reaches the tree's end);
  both openers serve V4 and the dense opener builds its tree at the ruleset's level; the V4 seat
  verifier over the seat's own leaves with `FaultInRange`; the panel and the seat harness carry
  the new verdict; the challenger's replay hands ADR-0085's builder a V2 view over its own leaves;
  `Base0FpBlockLeavesV1` with `cut_v1`, `folds_to_v1`, `name_the_leaf_v1` and
  `base0_fp_range_with_served_block_v1` (Decision 6's library half); the seat ceiling in the
  form's units; the RC floor's recompute kernels lifted into a backend seam so the floor holds
  its own state like every class. X1–X7 pass; the full base0 suite is 343 green.
* **One deviation from Decision 1's letter.** The verifier accepts any `retain_level` in
  `[PALW_BASE0_SPARSE_RETAIN_LEVEL_V1, PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1]` rather than the
  ruleset's exact level: the digests' level is the producer's retention level, which the form
  describes, and a seat handed a different ladder than the producer (the fixture at
  `COURT_MAX_STEP_LEAVES` against a fold made at the default) must still walk the root, which is
  level-independent. The floor at 12 keeps the digests to one per 4,096 leaves; the ceiling keeps
  the size. Byte-identity between the dense and the fold route (X2) holds when both are built
  under one cap, which is what production does.
* **What the tests taught.** The V4 dispatch was first inserted inside the V3 branch of the
  any-form decoder by a regex that swallowed a doc comment, so every V4 opening fell to the V1
  decoder and came back `Unverifiable` — found by a probe that decoded a served opening directly.
  The floor backend's verifier passed no seat state at all (it had relied on carried chunks), so
  every anchored floor interval was `Unverifiable` under Decision 2 until it learned the seam the
  other families use. A floor free-prompt run can stop at EOG before its budget, so its binding's
  context is not the one `fp_job_context_v1` derives from the job; the memo is keyed by the
  binding's, and the test memoizes under it.
* **Decision 6's transport landed (2026-09-05, with ADR-0085 items 4–5).** A block-leaves request
  rides the interval lane under a packed index — bit 31, the interval in bits 30–16, the block in
  bits 15–0 (`base0_fp_block_leaves_request_index_v1`; a plain interval index never has bit 31
  set) — so the lane's solicitation, slots and byte cap apply unchanged and no message was added.
  The executor's resolver decodes the address and answers `Base0FpBlockLeavesV1` from the same span
  replay the opener runs (`open_fp_block_leaves`; a dense retention hashes its tiles). The seat, on
  `FaultInRange`, asks for the block and — once held — names the leaf against its own replay
  (`fp_name_the_leaf_v1`: the served block must fold to the digest the opening carries, else it is
  refused by name; `None` says every served leaf is the seat's own, and the executor served a block
  it did not commit), records the leaf for the challenger's half (ADR-0085 Decision 4) and files
  nothing, as Decision 3 says. **Not landed:** the seed row's replacement by the seat's own
  selection (§3, not taken); the fold opener's edge-only replay.
* **Devnet run 9 (2026-09-05 10:15–11:02 JST, 300 tokens, the manifest fix on).** The commitment
  landed at 10:52:10 and the drill's `WAIT` stopped the nodes at 11:02 — before the panel binding
  (`anchor_delay` 20 DAA) at that hour's cadence of one block per ~3 minutes. Neither the run 8
  defect (openings never arriving) nor its absence was observed; the seat never drew. The attempt
  lane on the same run licensed every v5 claim by Decision 7's replay. Run 10 waits long enough.
* **Devnet run 10 (2026-09-05 11:25–12:54 JST, 300 tokens, `d7c7ce06b11d6fa3`): the run 8 defect
  reproduced and is understood.** Node-1 drew `[198, 110, 132, 72]` of 299 and asked node-0 every
  ~100 s for 22 minutes; node-0 logged neither "opened" nor "does not open" for the claim; the
  claim ended `Unavailable`. The cause is arithmetic, and run 7 already showed it: a fold
  retention carries no checkpoint chunks (a per-call KV state of this class is ~34 MB, so 300 of
  them cannot ride a 183 MB material — the bulk is the logits rows), so the executor's span
  replay (`base0_replay_span_leaves_v1`) starts at GENESIS for every interval and captures tiles
  as it goes; run 7 served interval 17 in 94 s, 10 in ~60 s, 4 and 0 in ~30 s — about 5 s per
  decode call — and interval 198 costs ~18 minutes, interval 298 ~27, past the seat's 120 s
  solicitation window many times over. The offline probe (`probe_open_interval`, release, the real
  artifact) on run 10's material measured it: interval 0 **21 s**, interval 1 **22 s**, interval
  57 **298 s** (≈5.2 s per call), interval 187 **1,991 s** (≈10.6 s per call — the cache grows with
  the position, so the slope steepens), RSS past 4 GB from the captured tiles; interval 253 was
  stopped after 55 minutes. Two things land here: refusals on the interval lane now log at the default level
  (`[palw-interval] refused …: NoAllowance|Stale|NotBonded|NotHeld|Oversized`; a throttled re-ask
  stays quiet), so the next run reads what node-0 answered. **The fix is the next cut, not this
  commit:** the executor must open an interval from the anchor state it RECOMPUTES without tile
  capture and memoizes — `base0_fp_seat_state_memoized_v1`, the seat's own machinery (Decision 2
  for the executor's side) — and replay only the interval's call(s) with tiles; a plain forward
  pass is ~0.15 s per call (the D7 replay licenses 64 positions in 7–10 s), so interval 198 opens
  in ~30 s. Until then a free-prompt claim past ~20 decode tokens on a fold class cannot be
  licensed by the interval lane, and Studio's 256-token cap is the honest limit for a different
  reason than the manifest.
* **The cut landed (`594015b9`, 2026-09-05 13:30 JST).** `base0_open_fp_interval_sparse_anchored_capped_v1`
  takes an anchor-state closure (`Base0FpAnchorStateForV1`); when the retention carries no
  checkpoint chunks the span replay resumes from the recomputed state of the span's STARTING
  interval's anchor (the span covers whole blocks and may start in the call before), else from the
  prompt as before. The three backends build the closure over their recompute kernels and
  `base0_fp_seat_state_memoized_v1` — a plain forward pass, no capture, memoized under the
  binding's context. The same probe on run 10's material: interval 0 **17 s**, 1 **17 s**, 57
  **19 s** (was 298), 187 **69 s** (was 1,991), 253 **90 s**, 298 **54 s** — every interval inside
  the 120 s window, the interval's own tile replay (~17 s a call) now the floor. Pinned by
  `an_interval_opened_from_the_recomputed_anchor_is_the_interval_opened_from_genesis` on a fold
  fixture wide enough to straddle retained blocks: byte-identical to the genesis walk, the closure
  consulted exactly where the span's starting interval has an anchor, a seat licenses it. The
  interval lane also refuses a re-ask for a `(claim, interval)` whose opener is still in flight:
  a seat re-asks every ~100 s and `served_recently` covered 10 s, so each re-ask had started
  another minute-long opener for the same pair.
* **Devnet run 11 (2026-09-05 13:35–14:48 JST, 300 tokens, `afa75b9e`): Y6 at 300 tokens.** The
  claim `7ed38205…` (prefill 16, decode 300) bound at 14:37; node-1 drew `[34, 105, 134, 209]` of
  299 and node-0 opened them in **17 s, 43 s, 49 s and 65 s** (1.11 MB each), node-1 held all
  four, "4 interval(s) replayed against this seat's own recomputed state — no history fetched",
  and **filed a `Valid` receipt at 14:47:50** — ten minutes after the binding, where runs 8 and 10
  had nothing after thirty. Node-2 (no artifact) filed `Incapable`, as every run.
* **Deployed to testnet-11 (2026-09-05 04:50–05:06Z):** kaspad `56f77a3d75f376de` on seat2, the
  .113 node / seat4 / pool slots 01–04, ibm node1 and node0 (SIGINT, respawn, 21–150 s each); the
  pool's `palw-a16-fp-worker` `69c0e194955cc41c` (the manifest fix) on .113 with `misaka-pool-fp@04`
  restarted. **What testnet then showed is a wall this ADR does not own:** a 300-token free-prompt
  claim on graph-v5 is ~41 M leaves, and exposure is `pwu × slash 5` ≈ 207 M sompi — a bond may
  hold half its collateral, so one such claim needs ≥ 414 M sompi of collateral plus whatever is
  already in flight. The pool's slot-04 bond was sized by the class's CANONICAL job (6.6 M leaves
  → 265 M for four claims): its gateway answered a 300-token request in 229 s and queued no
  commitment (`bond_active false`, room 32.7 M), and the two long jobs it had submitted earlier
  that day (03:06Z, 03:11Z) never reached the chain for the same reason. The pool now accepts a
  caller-sized bond (`POST /pool/v1/slots {"mode":"fp","bond_collateral":…}`) and slot-05 was
  created at 600 M sompi and funded 8.5 MSK from the faucet wallet; the 300-token proof on testnet
  rides that slot.
* **Devnet Y, live (run 7, 2026-09-05 07:17–08:14 JST, `62fec794`, three nodes on this Mac).** A
  graph-v5 free-prompt claim of 40 tokens (`d9ad65d0…`) staged a **24,423,539-byte** fold
  capture — over `PALW_MATERIAL_MAX_BYTES`, the drill's own line: "ADR-0084 Y1 is live". The
  panel bound on every node at 08:08; node-1 (holds the artifact) drew intervals `[17, 4, 0, 10]`
  of 39, asked node-0, and held four served V4 openings: interval 17 **1,110,705 bytes**,
  interval 4 **1,110,513**, interval 0 **33,655**, interval 10 **1,110,449** — the seed row
  (the vocabulary-wide logits row) is the decode intervals' whole size, as §4 said, and interval
  0 is the prefill's digests and the binding. Then "4 interval(s) replayed against this seat's
  own recomputed state — no history fetched" and **a `Valid` receipt** at 08:13:46, five minutes
  after the draw. Node-2 (no artifact) filed `Incapable`. On the attempt lane the same build
  served interval 0 of a v5 claim as **108,791 bytes** where run 5 had served 424,359,978, and
  Decision 7's replay kept licensing (the line now reads "6630544 leaves replayed, priced 0, 7s").
  Nothing over the cap moved on either lane.

* **Found alongside, not this ADR's:** the free-prompt lane's retained-trace manifest was the
  attempt lane's (`attempt_trace_manifest_root_v1(trace_root, 1)`, chunk count 1), so every
  free-prompt run past 256 tokens was refused by the verifier's chunk-count rule and every shorter
  one carried a root of the wrong lane's function; `0001f34c` gives the lane its own derivation
  (card §10p). It bears on this ADR only in that a 300-token claim is now the devnet's check for
  both (run 8).

## 8. What is deliberately not decided

* The seed row's replacement by the seat's own selection (above).
* Edge-only replay in the fold opener.
* Raising `PALW_INTERVAL_OPENING_MAX_BYTES`, which nothing here needs.
* The court's refutation walkers' ladder bound (ADR-0084 U-08) — landed behind
  `Params::palw_court_ladder` since (ADR-0084 §7.5).

## 9. Number hygiene

This is ADR-0086. The README's next free number was 0086 after 0085's row; it becomes 0087
with this row.

## Mission alignment amendment — 2026-10-07

fold openingの小ささは取得可能性・局所化可能性の代わりにならない。悪意あるFOLD prefix/stateをfresh verifierが認証された境界から反証でき、必要なleaf preimageを公開取得または客観的非開示裁定できることを追加する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0086-the-opening-carries-the-fold-not-the-leaves.md -->

</details>

<a id="source-0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing"></a>

<details>
<summary>0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md — 原文 L1–L410</summary>

[原文を開く](../adr/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) · SHA-256: `2e03bda07a88a4f4842fd25ccceeb44750aadb5fd90de1531a7480aa2c37f830`

<!-- ADR_SOURCE_BEGIN:0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md -->
~~~~~~~~markdown
# ADR-0093 — The court can try a fused row; the responder is what is missing

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-06 (design only). **§6 steps 2–4 IMPLEMENTED 2026-09-11, together**
  (§9): both A16 families answer a fused dissection — the root claim, every round, the
  challenger's choice and the bottom's close — with every number computed by the court's own
  kernels, drilled on real captures before anything relies on it (§7's condition). Step 5 is
  settled by a fact the design did not have: the exemption it would narrow was never armed on any
  preset, and is now pinned unarmed. Step 6 — the fleet upgrade that makes testnet-11's dense
  graph-v5 producers defendable — is the operator's. **Amended 2026-09-11 (§10):** Decision 6
  (admission refuses an undissectable fused class past its own fence, `None` everywhere) and
  Decision 7 (a forged FOLD is bottomed from the challenger's honest prefix and the root claim's
  own tile — no row served by the accused) are built; §9's straddling finding is corrected.
* Builds on: [0082](0082-the-close-is-flat-in-the-context.md) Decisions 2 and 3 (the k-ary history
  dissection and its arity), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md)
  (adjudicability is the price of weight),
  [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) §7 (which names this as the
  one thing it deliberately did not decide).
* Amends: nothing. It supplies the half of ADR-0082 Decision 2 that was specified and never built.
* Supersedes nothing.

## 0. The sentence this ADR is

Every move of the fused-attention dissection exists in consensus and every piece of its arithmetic
is written twice — once to fold and once to check — but **no shipped binary can produce the first
move**, so on a fused class an honest producer is convicted by silence and a dishonest one is
never convicted at all. This ADR specifies the producer, and says why it is a backend verb over
one history tile rather than a new court.

## 1. What the audit found, and what it did not

The 2026-09-06 mainnet audit's C-2 (with H-5) reads: *a stranger convicts any dense-tier producer
with no evidence.* The mechanism is not a missing rule. It is a missing party.

* `PalwConsensusObjectV2::CourtAttnRootClaimed`, `CourtAttnDissected` and `CourtAttnChildChosen`
  are consensus objects with acceptance rules, signatures and fold arithmetic
  (`palw_state_v2.rs:3003`, `:3032`, and the variant after them).
* `palw_attn_dissect_v1`'s arithmetic is complete in both directions: `palw_attn_fold_v1` composes
  `k` children into their parent and `palw_attn_fold_check_v1` refuses a composition that does not
  reproduce it, with `palw_attn_child_ranges_v1` splitting a range the same way on both sides.
* The arity is derived, not chosen — `palw_court_params_at_v2` reads the widest registered site and
  `palw_court_arity_v1` returns the first arity that fits the clock and the carrier, or refuses
  (ADR-0092 Decision 3).
* And `git grep` finds **no construction of `PalwAttnRangeClaimV1` outside the fold and the test
  modules**. The only production producer is the fold, which needs children before it can make a
  parent.

The consequence is stated in the tree's own words at `palw_state_v2.rs:7800-7812`: on a fused site
an `Arithmetic` whole-row close is refused by `check_close_cost_v2` and an `AttnDissection` close is
refused `NoDissection`, so *every* terminal leaf of a fused class is undefendable. The
2026-09-06 audit fixes stopped the conviction, behind a fence and in the direction that does not
burn an honest producer's collateral. They did not make the court work.

## 2. What the responder actually has to compute

Less than it looks, and that is the whole reason this ADR is worth writing rather than deferring.

`PalwAttnRangeClaimV1` is three fields — `max: i32`, `exp_sum: i64`, `v_acc: Vec<i64>` — the running
statistics of an online softmax over a range of key positions. `palw_attn_fold_v1` composes them.
So a responder that can produce the triple for **one history tile** can produce every claim in the
dissection by folding, at every rung, without the backend knowing what a court is:

```
tile claims (from the backend, per PALW_ATTN_HISTORY_TILE_V4 positions)
        │  palw_attn_fold_v1
        ▼
   child claims  ──►  parent claim  ──►  …  ──►  root claim
```

That is the decision this ADR exists to take: **the backend's obligation is one tile, and the
court's shape is the fold's.** A backend does not implement a dissection; it answers what its own
attention computed over a contiguous run of positions, which is a quantity a streaming attention
kernel already forms and discards.

## 3. Decisions

**Decision 1 — the responder's backend obligation is a single verb over one history tile.**

```rust
/// The online-softmax statistics this family's attention formed over `[first, first + count)`
/// of the disputed site's key positions. `None` by default: a family that has not implemented
/// it cannot take a dissection's turn, and `supports_court` must say so.
fn attn_tile_claim(
    &self,
    material: &[u8],
    site: &crate::palw_attn_court_v1::PalwAttnBottomSiteV1,
    first: u64,
    count: u64,
) -> Option<crate::palw_attn_dissect::PalwAttnRangeClaimV1>;
```

`PalwAttnBottomSiteV1` is the site type the court already derives — `palw_court_v2::palw_attn_dispute_site_v2`
builds it, and it carries the disputed leaf's own coordinate, the head's slice within a cache row
and the job's declared prefill, so the backend is told *which* attention it is being asked about and
never has to choose. The lanes the root claim reports (`head`, `lane_first`, `lane_count`) and the
history width (`history_positions` = `kv_len` at the disputed position) are read off the same site,
which is what stops the responder and the challenger describing two different rows.

Defaulted to `None`, exactly as `disclose_trace_event` is defaulted to an error, and for the reason
the 2026-09-06 audit's C-5 repair established: a family that cannot answer must be visible as one
before a court is armed over it, not after it has been convicted.

**Decision 2 — when a responder exists, the mercy that excuses its silence must be narrowed, and
that is a second activation.** What the 2026-09-06 audit actually landed for C-2/H-5 is not an
assembly refusal: `palw_court_responder_coverage` gates an arm in the fold (`palw_state_v2.rs:8287`)
that, past the fence, ends a fused-terminal session which `owes_the_dissection_opening` without
convicting or fining anybody — `rearm_after_unanswered_opening`. It is mercy for a move *no party
in the tree can make*.

The moment `attn_tile_claim` ships for a family, that sentence stops being true of it, and mercy
that outlives its reason is indistinguishable from a court that cannot convict. So this ADR's
landing has a second half: the arm must ask whether **this claim's class** has a responder, not
whether the release has none. Two consequences, both deliberate:

* it is a consensus-validity change in the convicting direction, so it is its own fence and its own
  height — never a silent narrowing riding the responder's release;
* until it is armed, a family that CAN answer is still excused if it does not, which is a worse
  place to stop than either end. §6's order of work puts it last for that reason, and §5's
  invariant 3 is what says the responder works before anyone relies on it.

**And `supports_court()` must be SPLIT, not widened.** A first draft of this ADR said it should
come to mean both verbs. It must not: it is one boolean over two unrelated turns, it already means
"this family can disclose and take an arithmetic turn" after the C-5 repair gave all three families
`disclose_trace_event`, and `kaspad/src/palw_producer.rs:735` and `palw_panel.rs:2675` branch on it.
Widening it before any family implements `attn_tile_claim` would flip every family to `false` and
report a disclosure gap that does not exist. So the dissection turn gets its own predicate,
`supports_dissection()`, defaulted `false` and true exactly where `attn_tile_claim` is implemented.
Two turns, two answers.

**Decision 3 — the panel files the moves; the backend never sees a session.** The panel's court arm
gains: on an accusation at a fused site, fold the tile claims into the root and file
`CourtAttnRootClaimed` with the derived arity, the binding, the out tile and the operand openings;
on each `CourtAttnChildChosen`, split the named child's range with `palw_attn_child_ranges_v1`,
fold each part, and file `CourtAttnDissected`. Both signed ML-DSA-87 under the CLAIM's bond key, as
the objects require.

**Decision 4 — a wrong tile claim must be as expensive as no claim, and the design must not make
it cheaper.** A responder that files claims which do not fold to its own root convicts itself by
`palw_attn_fold_check_v1` without any execution being replayed. That is the existing rule and this
ADR does not soften it. The consequence for the implementer is the reason §7 refuses to ship this
half-built: an attention kernel instrumented to emit *approximately* the right statistics is worse
than one that emits none, because the family then answers and loses.

## 4. What this buys, and what it does not

It closes C-2's second half: a fused class becomes defendable, so a court over it is a court and not
a clock. It does **not** close C-2's first half by itself — `palw_kary_court` is `always()` on
`palw_rc_base_params` and on devnet with the graph-v5 row in genesis, so those chains are exposed
until either this responder ships to every seat or the fence is scheduled off. That sequencing is an
operator decision and ADR-0092 §7 already records it.

## 5. Invariants the tests must hold

1. A family whose `attn_tile_claim` is `None` reports `supports_court() == false`, and an assembly
   arming a k-ary court over it is refused — the same shape as the C-5 coverage fence.
2. For every registered fused class, the tile claims fold to the root the responder files:
   `palw_attn_fold_check_v1` accepts the responder's own tree at every rung. A drill vector, not a
   fixture — the audit's own lesson is that a court drilled on an n_ctx-32 toy geometry cannot
   exercise a width-dependent defect.
3. An honest responder wins: a full session played against a correct producer ends `NoFaultFound`,
   at the widest registered site, inside `window_court` at the derived arity.
4. A dishonest producer loses at the rung where its claim stops folding, and the challenger's
   choice reaches that rung in `ceil(log_arity(positions/tile))` rounds and no more.

## 6. Order of work

1. The trait verb and its `None` default, plus `supports_dissection()` beside `supports_court()` and
   a test that pins every shipped family at `false` today — small, behaviour-neutral, and it makes
   the gap visible in the type system rather than in a comment. Decision 2's narrowing of the fold's
   mercy arm is NOT here; it is step 5.
2. `attn_tile_claim` for one family, with invariant 2's drill vector. The floor class first: it is
   the one whose arithmetic is integer end to end.
3. The panel's two arms, against that family.
4. The remaining families, each with its own drill vector.
5. Decision 2's narrowing — the mercy arm asks whether this claim's class has a responder — behind
   its own fence and at its own height, once every registered family answers.
6. Only then, the operator decision in §4.

## 7. Why this ADR ships no code

Because the failure mode of a half-built responder is worse than the failure mode of none. Today a
fused class cannot answer, and the audit's fixes make that a refusal rather than a conviction. A
family that answers with statistics that are close but not exact convicts itself under Decision 4 —
`palw_attn_fold_check_v1` is exact integer arithmetic and does not care why the numbers disagree —
and it does so while holding the claim's collateral. So the order in §6 is not a preference: each
step must be drilled before the next, and the first family's drill vector is what says whether the
kernel instrumentation is exact.

## 8. Number hygiene

0093 is free: 0092 is the highest in `docs/adr/`, and `git grep -n "ADR-0093"` finds no citation.
Next free number after this one is 0094.

## 9. As built (2026-09-11)

On `feat/adr-0099-sharded-seat`, after ADR-0102, in one piece — §7 asked that no half ship.

**Decision 1, smaller than designed: a family reads, the court's kernels compute.** The trait
verb is `attn_site_evidence(material, narrowed, carried_prompt)`, not `attn_tile_claim`: a
family returns what its capture COMMITTED about the site — the binding, the opened output tile
and query row, the four registered narrowings opened against the class root, the head's query
slice and the layer's K and V series, and the bottom's evidence (the anchor checkpoint with every
chunk of its state, or the cache-write rows) — and `palw_attn_responder_v1` computes the root
(`a16_attn_root_claim_v1`), every range (`a16_attn_tile_triple_v1` folded by `palw_attn_fold_v1`),
the challenger's choice and the bottom, the same calls `check_attn_dissect_bottom_v1` makes.
Decision 4's hazard — statistics "close but not exact" that convict their own producer — has no
family code to live in. `attn_tile_claim` is retired. `supports_dissection` is true where the
verb exists AND the class's fused sites can be dissected (`palw_fused_sites_are_dissectable_v1`);
`has_fused_site` says whether there is anything to dissect.

**Whose rows.** The evidence is read from the capture's OWN committed rows — a dense capture's
retained tiles, or a fold re-executed and required to reproduce its own root — so a challenger's
bottom opens the accused's commitments, a forged output tile included; the checkpoint leg comes
from a re-execution (the per-position cadence retains none) and the anchor's state from the
seat's recompute kernels, each refused unless it roots to the capture's own binding. Openings
are walked off `PalwStepMerkleTreeV1`, the step tree built once and pinned path-for-path against
`step_merkle_path_v1` (a bottom and its rows are ~a thousand paths over millions of leaves).

**Decision 3, the panel.** Four moves in `kaspad/src/palw_panel.rs`, before the ladder's arms: the
responder's root claim at a fused terminal (the arity the ruleset derives, signed under the
claim's key) and its rounds; the challenger's choice — the first child its OWN execution's
recompute does not reproduce, and silence when every child does (an honest disclosure has no
winning choice); and the bottom's close, assembled from the ACCUSED capture (pulled when not
held), filed by a party only when the verdict is its own win. Evidence is cached per session and
capture; the court dedup keys give the dissection its own round space (`court_move_round_v1`), so
a round that restarts at 0 is never taken for a ladder move already sent. The court duty view
carries the phase and the class's fused bit (`PalwCourtDutyV2::{dissection, fused_class}`).

**Decision 2 / step 5, settled by a fact.** `palw_court_responder_coverage` — the mercy this ADR
would narrow — is `None` on every preset: it was never armed. So there is nothing to narrow, and
the kaspad pin that was to go red "the day a responder exists" now says the opposite thing it
should: the panel builds all four moves, and the exemption stays unarmed on every preset. The
consequence is the one §4 feared, now answerable: on a chain whose k-ary court is armed (the RC
base and devnet, graph-v5 in genesis), a fused terminal's silence is a conviction, and before
this build no binary could file the root claim — an honest dense graph-v5 producer brought to a
fused leaf lost its reservation by silence. A node running this build answers; one that does not
still cannot, which is why step 6 is a fleet upgrade. The producer now refuses to underwrite a
fused claim it could not defend (`has_fused_site && !supports_dissection` where the k-ary court is
armed), as it already refused one the DA court could default.

**Found on the way.**

* **A fused tile wider than a head is undissectable, and admission never asked.** The fusion
  inherits the tile of the attention-table node it replaces, which the hybrid tables budget per
  geometry: at the fuzz corpus's tiny geometry (the fixture the hybrid test executes) that is the
  whole 64-lane row over 16-lane heads, so its graph-v5 fused leaf is refused by the court as
  `FusedTileStraddlesHeads`, and admission (which checks the query slice's tile, not the
  output's) does not refuse such a class. Graph-v6 (ADR-0102) cuts that one node's row at the
  head, so its fused leaves are dissectable at every geometry by construction; a backend whose
  class fails the check reports `supports_dissection() == false` and is not produced under where
  the k-ary court is armed. The admission refusal is §10's Decision 6.
  *Corrected 2026-09-11:* this bullet first said the tile was "512 on every hybrid geometry",
  generalizing from the fixture. Probed over the shipped geometries (2B and 35B-A3B: 8 lanes;
  27B: 4 lanes; every `n_ctx` from 8 to 131,072), the graph-v5 fused tile is inside its 256-lane
  head — so the shipped graph-v5 hybrid rows ARE dissectable and their backends say so; what
  graph-v6's head-width tile buys is that the property no longer depends on the budget.
* **The gap guard read the wrong half of a file.** It cut "production" at the first
  `#[cfg(test)]`, and `qwen25_a16_backend.rs` carries a test-only helper a thousand lines above its
  trait impl — so the guard could never have gone red for that family. It now cuts at the first
  test module and asserts the trait impl is inside what it reads.

**Tests** (§5): invariant 2 and 3 on REAL captures —
`a_real_fused_capture_answers_its_dissection_exactly_and_a_forged_row_is_convicted` (the dense
graph-v5 row executed; both layers, both heads, every court tile of a two-tile history acquitted
from the anchor; a negative control in which one weight-carrying input code moved breaks the
fold) and `a_real_hybrid_fused_capture_answers_its_dissection_exactly_and_a_forged_row_is_convicted`
(graph-v6 through the composed anchor; graph-v5's refusal pinned by name); through the CHAIN —
`the_parties_moves_from_evidence_acquit_the_honest_and_convict_the_forged_through_the_chain`
(ADR-0082's drill with every hand-written claim replaced by the evidence functions); invariant 4 —
the forged arms, where the least lie that finalizes to a forged tile is followed into the child it
hides in and convicted at that bottom; invariant 1 — `a_family_takes_the_dissections_turn_with_both_verbs_or_neither`
and the producer guard.

**Not done.**

* A live devnet drill of the dissection: the panel's arms are compile-checked, source-pinned and
  built from functions the tests play, but no node has yet filed a root claim on a running chain.
  The classic ladder in front of it is carrier-bound on one host and needs six panel seats, here
  six holders of the 1.8 GB A16 artifact — a fleet drill.
* **A forged claim whose capture is a FOLD cannot yet be bottomed.** The free-prompt lane retains a
  fold (no rows), and the evidence reads the accused's rows by re-executing — which reproduces the
  honest execution, not the forged one, so the evidence refuses by name ("a folded capture keeps
  no rows, and its re-execution is not the same execution"). The attempt lane retains dense
  captures and is unaffected. The missing piece is the bottom's twin of ADR-0085's close from
  served intervals: the accused's rows around the one tile, served with their paths.
* ~~The admission refusal for classes whose fused sites cannot be dissected~~ — §10, Decision 6.
* Step 6.

## 10. Amended 2026-09-11: Decisions 6 and 7

**Decision 6 — admission refuses a fused class no dissection can try, past its own fence.**
`Params::palw_fused_dissectable` (`None` on every preset; `Some`-only in the fingerprint, the fence
schedule and the fork-id probe, so no shipped fingerprint moves). Past it,
`verify_class_admission_v8` refuses a fused class whose output tile is not inside one head —
`FusedTileStraddlesHeads { tile_len, d_head }`, the output half of
`palw_fused_sites_are_dissectable_v1` (`palw_fused_output_tiles_are_one_heads_v1`), the predicate
the backends' `supports_dissection` already reads, so the gate and the backend are one spelling. A
genesis row is judged likewise when the fence is armed from genesis
(`palw_genesis_holds_undissectable_fused_class_v1` in `validate_palw_v2`). It is a fence and not a
fix because a live chain may already hold such a class. No shipped row fails it — the dense graph-v5
row's fused tile is 8 lanes over 128, the hybrid graph-v5's 8 (2B, 35B-A3B) or 4 (27B) over 256 at
every `n_ctx` probed, graph-v6's the head itself — so the class it exists for is a REGISTRANT's
graph: the dense row with its fused tile widened to two heads, which the gate admitted before this
fence and the court then refuses to dissect. Pinned both ways
(`an_undissectable_fused_class_is_refused_by_its_fence_alone`,
`the_dissectable_fused_fence_is_dormant_visible_when_armed_and_judges_genesis_from_genesis`).

**Decision 7 — a forged fold is bottomed from the challenger's honest prefix and the root claim's
own tile; the accused serves nothing.** The hole was worse than §9 said: at `Terminal` the move is
a close the accused never files against itself, so the whole-session backstop ends an unclosed
session on the CHALLENGER's side — a bottom the challenger cannot assemble acquits the forger. A
served-interval bottom (ADR-0085's twin, as §9 guessed) would not close it, because a forger need
not serve. What the challenger holds is enough without the accused:

* the court narrowed to the FIRST leaf the challenger could not reproduce (its choice rule is the
  first child its own execution does not reproduce), so every committed leaf before the disputed
  one is a leaf the challenger's own re-execution computes;
* the accused's root claim — which it must file, or its silence convicts — carries the committed
  output tile at that leaf, opened against the accused's own root.

`PalwStepPrefixTreeV1` rebuilds the accused's tree over `[0, i]` from exactly those two: nodes wholly
inside the prefix from the prefix's leaves, the node just right of it at each level from the right
siblings on `i`'s own path, every left sibling on that path compared with the prefix's node as it
is built. Every row a bottom opens (the query row, the cache rows, the output tile) is at or before
`i`, and the leaves after `i` — which a fold does not keep and a forger need not reveal — are never
needed. The evidence refuses by name unless the rebuilt root IS the binding's. The checkpoint leg is
the fold's own retained leaves (`base0_checkpoint_leg_of_retention_v1`, now the one spelling of
"which retention a fold's leg has", shared with the interval lane's anchor). The trait verb takes the
tile (`attn_site_evidence(…, accused_out_tile)`); the panel's close reads it off the chain
(`attn_root_out_tiles_from_chain_v1` over `walk_accepted_lifecycle_objects_v1`: the accepted
`CourtAttnRootClaimed` for the session, walking the selected chain's accepted lifecycle transactions
down to the session's own opening height) and keeps only a tile whose opening proves against the
claim's root at the narrowed leaf. Nothing on chain moves.

Tests: `the_prefix_and_the_last_leafs_path_open_every_leaf_before_it_against_the_accused_root` and
`a_prefix_that_is_not_the_committed_one_is_refused_or_roots_elsewhere` (every leaf count to 40,
every `i`, every `j`, a forged suffix); `a_forged_fold_is_bottomed_from_the_honest_prefix_and_the_root_claims_tile`
(dense tier: one forged execution retained both ways — the fold alone refused by name, the fold with
the tile equal to the dense capture's evidence opening for opening, a tile that is not the accused's
refused, the least lie convicted from the fold's evidence); the same equality on the hybrid tier
inside its fused test; `the_walk_visits_what_the_chain_accepted_newest_first_and_stops_at_the_height`
(the chain read on a mock chain); the production wiring pinned in the panel's source test.

**Corrected.** §9 said the attempt lane (dense) was unaffected. It is for the drill's forgery (a
committed tile altered, the execution itself honest) and not in general: the evidence reads the
checkpoint leg from a re-execution, and a forger whose downstream execution follows its lie commits
different checkpoint leaves after the disputed call — the re-execution's leg does not root to the
binding, the evidence refuses by name, and the backstop acquits. And it was worse for the lane
testnet-11 actually runs: a graph-v5 attempt capture at 512 is ~748 MB, over the material cap, so a
challenger never holds the accused's capture at all, and the close waited on a pull that is never
answered. Carrying the leaves in the dense material (the fix first proposed here) would not have
reached that challenger, and a forger chooses what it serves — hence Decision 8 and the filing route
below.

**The filing route (Decision 7, widened).** The challenger's close no longer needs the accused's
capture: `attn_site_evidence_from_filing` builds the bottom's evidence from the accused's FILING —
the binding and opened tile its root claim carries (and the anchor, below) — and this node's own
re-execution of the accused's job, through the same `PalwStepPrefixTreeV1`. The panel's close uses
the accused's capture when it holds one and falls to the filing when it holds none or the capture
cannot bottom the accused; it keeps asking for the capture, and no longer waits on it. Pinned on the
dense tier: the drill forgery's filing yields the dense capture's evidence opening for opening and
convicts (`a_filing_bottoms_the_accused_without_its_capture_and_its_anchor_convicts_a_deep_forger`).

**Decision 8 — the root claim carries the anchor its bottom will need, past its own fence.** The one
path no challenger can rebuild once an execution followed its lie is the path to the accused's anchor
checkpoint (its right siblings are the accused's later checkpoints). The party that holds it for
certain is the responder, at the one move it cannot skip: move 1, where silence convicts. So a new
object, `CourtAttnRootClaimedAnchored` (tag 42, appended last), is the root claim plus that anchor,
opened against the claim's own checkpoint leg; the fold checks it against the site with the bottom's
own anchor check (`palw_attn_anchor_is_the_sites_v1`, one spelling with `verified_anchor_v1`) and
opens the same phase as the plain form, storing nothing new. `Params::palw_attn_anchored_root`
(`None` on every preset, `Some`-only in the fingerprint, threaded through the fold's extras and the
sync walker, refused unless `palw_kary_court` is armed at or below it): before it the anchored form
is refused and nothing moves; past it the plain form is refused at a site whose bottom reads a
checkpoint. The panel files the anchored form past the fence and reads it back off the chain for the
close (`attn_root_filings_from_chain_v1`). Arming it refuses a root claim from every build before
this one, so it waits for the network's upgrade. Pinned: through the fold
(`the_anchored_root_claim_opens_past_its_fence_and_the_plain_one_is_refused_there` — dormant, armed,
a wrong counter and a forged leaf), the tag (`the_anchored_root_claim_is_tag_42_and_the_plain_one_keeps_its_tag`),
the fence (`the_anchored_root_fence_is_dormant_visible_when_armed_and_needs_the_kary_court`) and on
the dense tier a forger whose checkpoints after the disputed call differ: its own capture cannot be
bottomed, the filing without an anchor is refused naming this Decision, and the filing with its
anchor convicts; a forged anchor opens nothing.

*Tag coordination:* `feat/adr-0103-held-context` (branched from this one at `3e9c3b59`) appends
three objects at 42–44; whichever lands on `main` second re-appends after the other. Nothing has
been filed under any of them on any chain (every fence is `None`), so a renumber moves no history.

**Still not done:** a live devnet drill; step 6 (the fleet upgrade); arming Decision 8.

## Mission alignment amendment — 2026-10-07

modelがwideでもcourt経路が存在するという過去の改修記録を維持し、全canonical profileで外部参加者が証拠を取得・生成できるという一般保証へ読み替えない。producerがcloseできることとchallengerがcloseできることを分ける。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md -->

</details>

