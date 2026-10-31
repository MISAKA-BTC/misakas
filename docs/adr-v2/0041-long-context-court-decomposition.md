# ADR-v2-0041 — 長文脈court・sparse read set・dispute分解

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

長文脈TIRのcourtが実際に読む範囲・通信量・期限を、局所分解の設計として整理する。

## 2. 統合した判断の読み方

### 2.1

read setとconviction doorについての選択肢を、必要な取得量と保持責任の違いとして保持する。[旧ADR-0189](../adr/0189-tir-court-read-set-door-options.md)

### 2.2

sparse fetch、designated answers、tenure分離の案を、誰がどの原資料を期限内に供給するかに結び付ける。[旧ADR-0190](../adr/0190-tir-door-sparse-fetch-designated-answers-tenure-split.md)

### 2.3

dispute-local decompositionにより局所的な裁定へ落とす案を記録する。ただし局所化だけで全分岐の資金・材料予約が完成するわけではない。[旧ADR-0191](../adr/0191-tir-dispute-local-decomposition-long-context.md)

## 3. 維持する境界・後続改定

原文はいずれもProposed系の設計・比較を含む。選択肢を併合して新しい一案を採択した扱いにしない。

## 4. 隣接する判断との分担

共通Court契約は[ADR-v2-0042](0042-court-service-and-versioned-clock.md)、shared queryは[ADR-v2-0044](0044-shared-tir-query-data.md)、有料originalは[ADR-v2-0047](0047-funded-original-and-role-authorization.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0189-tir-court-read-set-door-options.md](../adr/0189-tir-court-read-set-door-options.md) | L1–L58 | L3–L6 |
| [0190-tir-door-sparse-fetch-designated-answers-tenure-split.md](../adr/0190-tir-door-sparse-fetch-designated-answers-tenure-split.md) | L1–L122 | L3–L9 |
| [0191-tir-dispute-local-decomposition-long-context.md](../adr/0191-tir-dispute-local-decomposition-long-context.md) | L1–L252 | L3–L13 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0189-tir-court-read-set-door-options"></a>

<details>
<summary>0189-tir-court-read-set-door-options.md — 原文 L1–L58</summary>

[原文を開く](../adr/0189-tir-court-read-set-door-options.md) · SHA-256: `2108181592ff428afd1caeab0fc03406f3f96d3b33186bec38c8c1c7a2c7f8b0`

<!-- ADR_SOURCE_BEGIN:0189-tir-court-read-set-door-options.md -->
~~~~~~~~markdown
# ADR-0189 — TIR court read set and the conviction door: options (Proposed)

- **Decision:** Proposed (2026-10-18). Nothing adopted, nothing implemented, no fence, no pin, no rule changed.
- **Scope:** the Huihui-Qwen3.5-2B 32K class, the cone term of the conviction door (`palw_tir_frame_cost_v1`, `cone_runs_bound_v1`).
- **Evidence:** [read-set-1018](../evidence/palw/g14-measure-2026-10-18/read-set-1018.md) (partial), [door-breakdown-1018](../evidence/palw/g14-measure-2026-10-18/door-breakdown-1018.md), [cone-runs-analysis-1018](../evidence/palw/g14-measure-2026-10-18/cone-runs-analysis-1018.md).
- **Related:** [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md), [ADR-0181](0181-design-freeze-and-controlled-change.md), PRINCIPLES §11 (a structural change needs an RFC and an ADR and explicit adoption).

## Context

The shipped door for this class is 1,062,231 DAA against a limit of 3,120. The cone term is 448 serial turns (`cone_runs_bound_v1`,
a superset bound that saturates at `ceil(114,664 / 256)`) times 2,346 DAA per turn. The measured read set of one disputed step is
much smaller (decode 2: at most 8 runs, 97 leaves, 1.17 MB wire). That number is observed over 44 samples; it is not a bound.

## Options (variables and assumptions are explicit; door numbers are from `read-set-1018.md`)

1. **Sparse leaf fetch in one round.** Fetch only the operand leaves, not 256-leaf runs, in one serial turn. Door 11,296 to 12,537
   (decode-2 observed inputs). Needs: a derived bound on the read set for every legal job (not yet shown), and sibling-path
   accounting.
2. **Request batching of the same claim's independent turns.** Remove the per-run serial margin by issuing all runs of one claim in
   one round. Door 26,197 (parallel, bound body). Needs: the same claim/root/coordinates only.
3. **Material sharing across custodians and owners (same claim/root/coordinates only).** Door 17,359 serial, 11,658 with option 1.
   Not to merge different owners' requests, and not to merge each signer's liability or default.
4. **Non-cone terms.** Trace tiles 4,209, output/reorg 2,346 each, owner queue 1,536 (total 11,120). Any option that reaches
   3,120 must change these too. None is analysed here.

## Result of the options as measured

No option reaches 3,120. The zero-cone floor is 11,223, so the non-cone terms dominate once the cone term is reduced.

## Unmet conditions (why this is Proposed only)

- The read set is observed on decode 2 (6 positions) only. The decode-30 run reached 28 runs at 33 positions. A derived bound covering
  every legal job, every dispute kind and every checkpoint/history boundary is not shown.
- State and HistTile kinds are not measured (checkpoint 164, h_tile 32).
- No params-free structural analyser; the real court's i128 params still do not fit in 32 GB.
- Non-cone terms are not re-derived per option.
- No adversarial test of: whole collusion plus one external verifier, default on missing answers, restart and reorg recovery.

Adopting any option is a user decision. The minimal change that reaches 3,120 is not yet identified.

## Update 2026-10-18 (analysis only)

- Non-cone option ceilings and combinations: [nonconone-door-options-1018](../evidence/palw/g14-measure-2026-10-18/nonconone-door-options-1018.md).
  With the cone at zero the door is 11,223; the non-cone terms must fall by 8,103 DAA. Even with trace, reorg and owner queue all
  at zero (ceiling) the door is 3,132, because the output check (2,346) remains.
- The owner queue removal changes the decided tenure rule. It is a **user decision** and is not adopted here.
- Still unmet: derived upper bound over all legal jobs (no params-free analyser yet), State/HistTile measurement (not run: decode 30
  took more than 30 minutes per kind), the residual costs of each option, and adversarial tests. No fence, no rule, no implementation.

## Update 2026-10-18 (derived upper, analysis only)

- [derived-upper-1018](../evidence/palw/g14-measure-2026-10-18/derived-upper-1018.md): a value-free window rule for the cone
  (`q <= p <= q+C+h+1`, HistTile `q < p`) is checked against the real evaluator on fixture classes (2,583 disputed leaves, 14,382
  operands, 0 violations). Over every disputed position of the 32K class the superset is 116,166 runs (door 272.5 M DAA), or
  20,850 runs if completed history tiles are contiguous (checked on fixture only; door 48.9 M DAA). The shipped formula gives
  95,309 at 32K (door 223.6 M DAA), which is below the per-leaf superset: it is sound only under the same contiguity.
- Open items still unmet: element-level derivation; State and HistTile on the real class (not measured); model (parameter) operand
  bounds; contiguity proof for all layouts; the adversarial tests. Adopting any option remains a user decision.
~~~~~~~~
<!-- ADR_SOURCE_END:0189-tir-court-read-set-door-options.md -->

</details>

<a id="source-0190-tir-door-sparse-fetch-designated-answers-tenure-split"></a>

<details>
<summary>0190-tir-door-sparse-fetch-designated-answers-tenure-split.md — 原文 L1–L122</summary>

[原文を開く](../adr/0190-tir-door-sparse-fetch-designated-answers-tenure-split.md) · SHA-256: `3f81782c9c596a9fc9dd043e4c4431e897ee205e274d31552de40edbdca71b3a`

<!-- ADR_SOURCE_BEGIN:0190-tir-door-sparse-fetch-designated-answers-tenure-split.md -->
~~~~~~~~markdown
# ADR-0190 — TIR conviction door: sparse fetch, designated answers, tenure split (Proposed)

- **Decision:** Proposed (2026-10-18). Nothing adopted in code, no fence, no pin, no rule changed. Number taken: 0190 (next free in
  `docs/maintenance/number-registry.json`; 0188 is taken on `codex-free/d2-x3`, 0189 is the options ADR on this lane).
- **Scope:** Huihui-Qwen3.5-2B 32K class; the conviction door of the public court (`palw_tir_frame_cost_v1`).
- **Evidence:** [door-breakdown-1018](../evidence/palw/g14-measure-2026-10-18/door-breakdown-1018.md),
  [read-set-1018](../evidence/palw/g14-measure-2026-10-18/read-set-1018.md), [nonconone-door-options-1018](../evidence/palw/g14-measure-2026-10-18/nonconone-door-options-1018.md),
  and the structural window rule of `release/t12-16000` @ 484e8fa67 (`derived-upper-1018`, read only, not in this branch).
- **Related:** [ADR-0189](0189-tir-court-read-set-door-options.md), [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md).

## Decided by the user (2026-10-18), constraints of this ADR

1. Keep the 3,120 DAA bound. Do not exclude the model (Huihui). Arming only after every gate passes.
2. Separate the evidence-retention period from the slot-occupancy period (today idle grace = `max(reorg_retention, inclusion_margin)`).
   Never treat an honest owner waiting for an answer as idle; never substitute 0 for the wait.
3. Designated custodians answer each request; reuse only authenticated bodies of the same claim/root/coordinates; never punish only for
   not being designated; a silent designated set reaches DA default and refusal within a finite deadline; signers of a false Valid stay
   liable.

## Derivation of the door: per claim, not per class (answer to the open question)

The door is computed **per claim**. `PalwTirPublicSourceV1` is built from the claim's own header and job at
`consensus/core/src/palw_tir_public_source_v1.rs:226`:
`let cone_runs = verified.space.cone_runs_bound_v1(&job, header.step_leaf_count, RUN)`. The door reads it through
`source.cone_runs_v1()` in `tir_public_localization_turns_v1` (`palw_tir_frame_cost_v1.rs:351`) and `tir_public_localization_path_v1`
(`:416`). The class maximum enters only the clock/price bounds (`maximum_custodians`, `maximum_leaf_bytes`).

Consequences:
- The measured door 1,062,231 is the door of the **measured short job** (prefill 4, decode 2, 114,664 leaves, cone 448 = the job's own cap).
- The refusal `DoorBeyondLiabilityHorizon` is checked per claim, so a claim with a long job is refused at a larger door. At 32,768
  positions the shipped `cone_runs_bound_v1` is 95,309 runs (analyser, `derived-upper-1018`), i.e. about 2.2e8 DAA on the serial structure;
  the per-leaf sound derived upper is 116,166 runs. **Under the current serial turn structure a full-context claim cannot pass 3,120.**
- The same door must therefore be evaluated for both: the measured short job (448 cap) and the 32K maximum (95,309 cap / 116,166 derived).

## Re-derivation of the non-cone terms (the 3,132 claim is not an impossibility proof)

`tir_public_localization_path_v1` (`palw_tir_frame_cost_v1.rs:424-427`) prices the output / stop-rule check and the reorg restart at
`widest = path.2`, the largest span on the path, which is the cone-run span 2,346. The output check has no request of its own in the code:
it is one turn with no body defined, so 2,346 is a placeholder reuse, not a measured or derived cost. Its real request (the trace tile,
69 DAA, or a leaf) would cost about 69 DAA. The reorg restart is priced as one turn at the widest span; whether a reorg replays one turn
or the whole path is not decided in the code and must be stated before the restart term is changed. These two terms are therefore the
trace of the 3,132 figure and are open, not proven.

Re-derived spans (script `door/sens.py`, reproduces the 1,062,231 path and the 12,537 option C value):
- one-round sparse span, body = L x 8,192 B: L = 97 gives 907 DAA; L = 256 gives 2,313 DAA.
- one-round span at the observed wire 1,169,221 B: 1,314 DAA (= option C).

## Residual terms (not assumed zero)

- **Uncovered:** the gather / model-parameter operand reads are not bounded by the structural analyser (`derived-upper-1018`). They are an
  explicit uncovered term; no value is assumed.
- **Uncovered:** the derived read-set bound is per leaf over the window rule `q <= p <= q+C+h+1`; the tile-contiguity figure (20,850) holds
  only on fixtures and is not a bound for the class.
- **Auth paths, signatures, control carriers, retention, re-fetch, queue:** each is priced in the span (control 14,320 B per carrier,
  133 controls per 2 MB turn). Signature and retention costs of designated answers are not yet measured.
- **Tenure split (point 2):** the owner queue term (1,536 DAA) changes when the wait is separated from retention; the new value is not
  derived yet.

## Option comparison on the same inputs (ceilings unless stated)

The table in `nonconone-door-options-1018` remains the reference: with the cone at zero the door is 11,223; the non-cone terms must fall by
8,103 DAA. No combination reaches 3,120 with the cone at its one-round floor, because the output check and the trace tiles still add
about 4,200 DAA of serial terms unless they are batched too.

## Decision for the next step (not adopted here)

The design that can satisfy both the 3,120 bound and the 32K maximum must make the **number of serial rounds independent of the run
count**: bounded rounds of batched requests, bytes per round bounded by carrier capacity, designated answers, and the output check and
reorg restart priced at their own request spans. This ADR does not select the final design; it records that the per-claim door at 32K
cannot pass under the current structure.

## Unmet conditions

- No implementation. No dormant fence exists for sparse fetch, designated answers or the tenure split.
- No test yet reproduces the failure on the real class in this lane beyond the existing `door_breakdown_on_a_real_class` (asserts the
  refusal at 3,120; baseline run was started, see final report).
- The 32K maximum door is not measured; only derived from the analyser lane.
- Gather/model-parameter reads are uncovered (above).
- Whole collusion plus one external verifier, default on missing answers, and restart/reorg/IBD identity are not tested.

## Chosen design (Proposed; not implemented)

- **Bounded rounds.** The disputed step's read set is fetched in R = 2 serial rounds, independent of the run count. Round 1: one batched
  request for the whole derived operand closure of the disputed leaf. Round 2: only the value-dependent follow-ups that the window rule
  cannot pre-compute (Gather index, Select branch, TopK selection). Bytes per round are bounded by carrier capacity; a round that exceeds it
  is split into parts inside the same round, not into more rounds.
- **Designated answers.** Each request has a designated custodian recorded in consensus state with obligation, deadline, handover order and
  default. Authenticated bodies are reused only for the same claim, root and coordinates. Signers of a false Valid stay liable even if they
  did not carry the body. Silence of every designated custodian gives DA default and reward/credit refusal at a finite deadline.
- **Tenure split.** Evidence retention and slot occupancy are separate periods. An honest owner waiting for an answer is not idle. Handover
  keeps pursuit state, evidence and charges.
- **Output/stop and reorg.** Priced at their own request spans (trace tile 69 DAA or a leaf), never at the widest span of the path.

## Door formula (every term)

Door = prep + acq + margin_round(R) + transport(round 1 bytes, shared once per authenticated body) + transport(round 2 bytes)
     + output_check_span + reorg_span + owner_queue + descent + rows/tiles + IDs + filing
     + **uncovered: gather/param operand bytes (not bounded, explicit)**
     + auth paths + signatures + control carriers + retention + re-fetch

Only the terms with a measured or derived value can be filled. The remaining ones are named and not assumed zero.

## Numbers from arithmetic (not from tests)

| case | read-set body | transport (shared once) | transport (56 chains) |
|---|---:|---:|---:|
| measured short job, observed max 97 leaves | 794,624 B | 7 DAA | 356 DAA |
| 32K maximum, derived window 4,915,476 leaves | 40,267,579,392 B | 322,141 DAA | 18,039,876 DAA |

Threshold: 3,120 DAA at 125,000 B/DAA allows 390,000,000 B shared once, which is 47,607 leaves. The derived 32K window is about 103 times
that. The dominant term at 32K is the transport of the read-set bytes, not the rounds.

## Concrete unmet condition

The 32K maximum door is not under 3,120 under the current analyser bound. The condition to meet is a demand-driven operand-closure byte bound
for the disputed step of at most 390 MB (shared once) at 32K, or the bound must be shown to be at most 47,607 leaves. The analyser gives only
the window superset, and gather/param reads are not bounded. Until that is derived, 3,120 cannot be shown at the 32K maximum.

## What was not done in this lane

- No implementation of the rounds, designation state, tenure split, or dormant fence.
- The baseline test `door_breakdown_on_a_real_class` (run `b99g16ncm`) had not finished under the shared lock when this was written.
~~~~~~~~
<!-- ADR_SOURCE_END:0190-tir-door-sparse-fetch-designated-answers-tenure-split.md -->

</details>

<a id="source-0191-tir-dispute-local-decomposition-long-context"></a>

<details>
<summary>0191-tir-dispute-local-decomposition-long-context.md — 原文 L1–L252</summary>

[原文を開く](../adr/0191-tir-dispute-local-decomposition-long-context.md) · SHA-256: `be6e68bcd9208655c398e33565824820db3cd9328e9732b96ae8607fc00599d0`

<!-- ADR_SOURCE_BEGIN:0191-tir-dispute-local-decomposition-long-context.md -->
~~~~~~~~markdown
# ADR-0191 — TIR court: dispute-local decomposition for long-context claims (Proposed)

- **Decision:** Proposed (2026-10-18). No fence is armed, no constant is pinned, no shipped rule changed. Step B (designated answers
  and tenure split) is implemented DORMANT behind a sibling fence in the same change; the bounded-round path of ADR-0190 is untouched.
- **Number:** 0191 (next free after 0190 in `docs/maintenance/number-registry.json`).
- **Scope:** every legal TIR job up to ctx 2,097,152 (graph-v7@2097152), not one 32K class. The 3,120 DAA door bound is kept, not raised.
  No class is excluded; 32K classes keep their IDs and semantics.
- **Related:** [ADR-0189](0189-tir-court-read-set-door-options.md), [ADR-0190](0190-tir-door-sparse-fetch-designated-answers-tenure-split.md),
  [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md). Closure bound over 32K..2M: lane on `release/t12-16000`
  (`closure-bound-1018`); its numbers are to be substituted into the cone-free terms below when it lands.
- **Principles:** no cryptographic proof of computation is used (PRINCIPLES §8). A hash, a checkpoint commitment or a Merkle opening
  identifies data and authenticates it; it is never treated as evidence that the computation was correct. Correctness is established
  by a deterministic recomputation of ONE dispute, and by the public evidence that localises it.

## Problem

ADR-0190 gave the door per claim, and the serial structure of the court makes it grow with the run count: the cone term is
`cone_runs × 2,346 DAA` (448 serial turns at the 32K cap). Its bounded-round variant removes the run count from the rounds but
still ships the whole read set in one body. The read set grows with the context (the attention history, the GDN state, the
operand closure), so the transport term grows with ctx: at 32K the derived window is 4,915,476 leaves = 40 GB (ADR-0190), and
no constant of the round structure can fix a term that is linear in bytes.

A door that must stay under 3,120 for every legal job therefore needs (a) a number of dependent rounds that is logarithmic in the
context, and (b) a per-round body that is bounded by a constant independent of the context, not by the claim.

## Mechanism

### (1) Locate the dispute in logarithmic dependent rounds

The owner and the challenged party hold committed positions. Every committed state (checkpoint, history tile, GDN block, logits tile)
is a leaf of a fixed-arity commitment tree over the claim's program. The dispute is localised by **b-ary bisection** (b = 16) over:

- positions of the claim (`N` positions): `R_pos = ceil(log_b N)`;
- history tiles of the attention reduction (`N / h` tiles, `h` = history tile size): `R_attn = ceil(log_b (N/h))`;
- the operations of one step (at most 2^10 leaves, the step-tree depth `PALW_TIR_STEP_NODE_DEPTH_V1`): `R_op = ceil(log_b 1024)`;
- the vocabulary tiles of the output (`V / 4,096` lanes, `PALW_LOGITS_TILE_LANES`): `R_tile = ceil(log_b (V/4096))`.

Each bisection round sends the `b-1` sibling digests of one level (a constant body, about 1 KB). The number of rounds is the sum of
the four terms and is independent of the run count. The checkpoint interval `C` does not add a round: the per-block commitments
published at every position replace the replay from the last checkpoint (see (2)).

### (2) Adjudicate ONE dispute by a small deterministic recomputation

After localisation the disputed operation has operand blocks of constant size, and the adjudication is one recomputation of that
operation from them:

- **Attention** is a reduction over history tiles. Bisection stops at one tile of `h` positions; the tile's partial softmax
  (running max, sum, weighted value) is checked against the committed partials of its neighbours. Body: `h × kv_row` bytes.
- **GDN state** is a reduction over state rows. The state at position `p` is the committed block at `p-1` plus one bounded update
  (the token's own k, v, decay, beta); the output `o = Sᵀq` is a reduction over row blocks, bisected to one block of `b_s × d_v`.
  Replay from a checkpoint is NOT used: the commitment at every position makes the state an input of one update, so `C` is absent
  from the transport.
- **Matmul** is decomposed into row and column blocks of the operand; one block of each operand is the body.
- **Gather** (embedding rows, index-selected KV rows) reads one row; **Select / TopK** branch on a value that is inside the adjudicated
  block, so the value-dependent follow-up is the second round of (1), not an extra term.
- **Output / stop rule** checks its own tile (`4,096` lanes × 2 bytes).

The adjudication body is therefore `B_adj = h·kv_row + b_s·d_v·(state elem) + (matmul blocks) + leaf + logits tile`, a constant that
depends on the model shape and the tile sizes, not on ctx. Only one dispute is adjudicated at a time per claim; the others queue.

### (3) Sparse fetch, batching, designated custodians

- Only the blocks named by the current round are requested. Independent requests of the same round are batched into one carrier.
- Answers are **designated**: each request has a designated custodian, obligation, deadline, handover order and default in consensus
  state (`palw_tir_designated_answers_v1`, Step B below). Non-designated silence is never punished. All designated silent →
  finite DA default and refusal of reward/credit at a finite deadline.
- Authenticated bodies are reused only for the same claim, root and coordinates. Signers of a false Valid stay liable even when
  they did not carry the body or were handed over.

### (4) Reorg recovery

A reorg replays only the adjudication round and the rounds whose commitments are no longer on the chain. Authenticated material of
the bisection (digests, opened paths) that is still valid under the current root is reused, so the restart term is one adjudication,
not the whole path (ADR-0190 priced it at the widest span, which is replaced here by the adjudication's own span).

## Door formula

Notation: `span(B)` is the turn span of one request whose answer body is `B` bytes (`tir_public_turn_span_v1`). Its slope is taken from
the two measured one-round points of ADR-0190 (97 leaves → 907 DAA, 256 leaves → 2,313 DAA), i.e. `span(B) ≈ 44 + ceil(B / 920)`.
This is a fit and a declared assumption; it must be re-measured on the shipped binary before any arming.

```text
Door(N, h, V, b, B_bis, B_adj, B_out, M) =
      prep                                   (legacy preparation door, counted, not re-derived here)
    + owner_queue                            (1,536 at 32K, legacy; tenure split may reduce it, not derived)
    + (R_pos + R_attn + R_op + R_tile) · span(B_bis)          bisection rounds, each carrying b-1 digests
    + span(M · B_adj)                        adjudication (one per dispute; M concurrent claims share the carrier)
    + span(B_out)                            output / stop rule (one tile)
    + span(B_adj)                            reorg restart: adjudication only (bisection reused)
    + uncovered_terms                        (none left in the operand closure by construction; see Unmet)
```

`B_bis = 1,024`, `B_adj = h·kv_row + b_s·d_v·2 + matmul blocks + leaf + logits tile`. For the declared shape of the Huihui-2B class
(`kv_row = 4,096` B, GDN block `16 × 128 × 2 B` per head at 16 heads = 65,536 B, matmul blocks 131,072 B, leaf 8,192 B, logits tile
8,192 B, `h = 16`, `V = 248,320`, `b = 16`, `M = 1`) the adjudication body is `B_adj = 278,528 B`.

| term | 32K (N = 32,768) | 2M (N = 2,097,152) |
|---|---:|---:|
| rounds R_pos, R_attn, R_op, R_tile | 4, 3, 3, 2 | 6, 5, 3, 2 |
| bisection = rounds × span(1,024) = rounds × 46 | 552 | 736 |
| adjudication span(278,528 B) (also the restart) | 347 | 347 |
| output span(8,192 B) | 53 | 53 |
| reorg restart (adjudication only) | 347 | 347 |
| owner queue (legacy, not reduced here) | 1,536 | 1,536 |
| **sum, excluding prep and concurrency** | **2,835** | **3,019** |
| headroom to 3,120 for prep, M > 1, and unmeasured terms | 285 | 101 |

Arithmetic from `door-arith` in this ADR; no measurement. **Superseded by the table in "Derived terms" below**, which adds prep, the M-admission and the owner queue. `C` appears only in the commitment count (`N / C` checkpoint commitments
and `N` block commitments of the claim), not in the rounds or the body. At 2M, `C = 65,536` and `N / C = 32`.

Concurrency: `M` concurrent disputes of one claim enlarge the adjudication carrier to `M · 278,528` B, i.e. about `303·M` DAA; with
the 1,536 owner queue this leaves room for `M = 1` only at 2M. **The mechanism therefore needs an admission rule that serialises
adjudications per claim** (the owner queue already admits at most `K_owner` owners; each admitted owner adjudicates one dispute).
That rule is not derived here and is listed as unmet.

## Comparison with ADR-0190

| | ADR-0190 bounded-round (2 rounds) | ADR-0191 dispute-local (log rounds) |
|---|---|---|
| serial rounds | 2 (cone), independent of runs | ceil(log_b) per dimension, 14 at 32K, 18 at 2M (incl. adjudication and output) |
| per-round body | read set of the whole cone, grows with ctx | constant (1 KB bisection; `B_adj` 278,528 B) |
| transport at 32K | 322,141 DAA (40 GB shared) | 303 DAA (adjudication) |
| 2M | not bounded (window grows linearly) | 3,019 at M = 1, excluding prep |
| dependence on C | replay from checkpoint | none in transport (per-block commitments) |
| uncovered | gather / param operand bytes | model shape constants (`kv_row`, `b_s`, matmul block); no unbounded term |

ADR-0190's 390 MB threshold no longer applies: the transport is bounded by a block, not by the claim.

## Uncovered terms and what is counted

- **Counted, not hidden:** detection (the public evidence that localises the dispute), preparation (`tir_public_preparation_door_v1`),
  signatures and control carriers (14,320 B per carrier), retention, designated-answer deadlines (handover), the owner queue.
- **Not derived:** `prep` (its value at 32K and 2M); `kv_row`, `b_s`, the matmul block and the gather rows of the shipped Huihui and
  graph-v7 programs (the constants above are declared assumptions, to be replaced by the program's own shape bound); M and its
  admission rule; the owner queue after the tenure split; the span slope (fit, not measured on the shipped binary).
- **Closure bound:** the other lane's derivation over 32K..2M supplies the operand closure of a dispute; this ADR uses the block
  bound instead, which is independent of it. If its closure shows a block larger than the one above, the door changes by
  `span(ΔB)` per adjudication and is re-counted.

## Derived terms (update, 2026-10-18; arithmetic with declared assumptions)

Each missing term is a function. The inputs marked "declared" are the parameters of the existing frame (`inclusion_margin_daa = 4`,
`acquisition_daa = 1`, `control_bytes = 14,320`, `parallel_bytes_per_daa = 125,000`, `reorg_replay_moves = 1`, `K_owner = 8`,
`PALW_TIR_FRAME_PREPARATION_DEBITS_PER_PASS_V1 = 1`). They are not measured on the shipped binary.

- **prep(maxc)** = `acq + margin · S + ceil(maxc · S · control_bytes / BPD)` with `S = (reorg + 1)·1 + 1 = 3` serial units.
  At `maxc = 6` or `8`: `1 + 12 + 3 = 16` DAA. It does not depend on ctx, C or h.
- **Concurrency**: with `M` adjudication carriers admitted per claim at once, the adjudication and the restart each cost
  `span(M · B_adj)`. Non-queue door `D(R, M) = R · span(1,024) + span(M·B_adj) + span(B_out) + span(M·B_adj)`, `R = R_pos+R_attn+R_op+R_tile`.
  - 32K (R = 12): `M = 1` → 1,299; `M = 2` → 1,905; `M = 8` → 5,537.
  - 2M (R = 16): `M = 1` → 1,483; `M = 2` → 2,089; `M = 8` → 5,721.
  - So at 2M, `M = 1` is the only admissible value under 3,120. A rule that admits one adjudication carrier per claim at a time is
    required. The waiting it imposes on the other `K−1` owners is `(K−1) · span(B_adj) = 2,429` DAA if serialised naively, which
    exceeds the budget. Admission therefore must be a queue whose wait is bounded by the owner queue term, not by stacking carriers.
    That admission rule is NOT derived; it is the open design point.
- **Owner queue after the tenure split**: the split changes the tenure from `max(grace, window / 2K)` to `grace + span(B_adj)` for an
  owner that is awaiting (it never goes idle while awaiting, and it rotates when its request completes). The queue term is still
  `ceil((q+1)/K) · tenure'` with `q = open_window · (parallel / control)`. Its value depends on `open_window` and `grace`, neither
  of which is derived. The condition it must meet at 2M with `M = 1` is `queue' ≤ 3,120 − 16 − 1,483 = 1,621`. The legacy 1,536 meets
  it with 85 DAA of headroom. Whether the split keeps it there is not shown.

### Table with all terms (2M, `M = 1`; the 32K column is the same, excluding the 32K round count)

| term | 32K | 2M |
|---|---:|---:|
| prep (derived above) | 16 | 16 |
| bisection rounds | 12 rounds × 46 = 552 | 16 × 46 = 736 |
| adjudication (`M = 1`) | 347 | 347 |
| output | 53 | 53 |
| restart (adjudication only) | 347 | 347 |
| owner queue (legacy value; condition `≤ 1,621` at 2M) | 1,536 | 1,536 |
| **total at `M = 1`** | **2,851** | **3,035** |
| headroom to 3,120 | 269 | **85** |

The headroom at 2M is 85 DAA, which is conditional: it holds only if the queue after the split stays at or below 1,621, and only
with `M = 1` enforced by an admission rule that is not yet derived. It is not a guarantee.

## Admission and the owner queue: derived (2026-10-19, supersedes the queue headroom above)

**Correction.** The table above counted the owner queue at its legacy 1,536 and the headroom of 85 DAA at 2M. That is withdrawn. Once
adjudications are serialised per claim, an owner that queues waits for every earlier holder's budget, so the queue term is not the
legacy term. The derivation below replaces it.

- **Admission (implemented, dormant, pure: `palw_tir_adjudication_slot_v1`)**: one slot per claim (`M = 1`). A holder that is silent
  past the adjudication budget is reclaimed AT its deadline and the next owner takes the slot there; a reclaimed owner re-queues at the
  tail at most once more, then is refused the slot on that claim. Budget `B_budget = span(B_adj)`.
- **Queue after the split**: position `p` (1-based) waits `(p − 1) · span(B_adj)`, worst `(K − 1) · span(B_adj)` with `K = 8`
  (`PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1`). The legacy `ceil((q+1)/K) · tenure` term is not used by this admission.
- **Door** (declared inputs as above, `prep = 16`, `R_32K = 12`, `R_2M = 16`, bisection 46 DAA per round, output 53):

  `D = prep + R · 46 + 53 + span(B_adj) (own) + span(B_adj) (restart) + (K − 1) · span(B_adj)`
  `  = prep + R · 46 + 53 + (K + 1) · span(B_adj)`

| | 32K | 2M |
|---|---:|---:|
| `B_adj = 278,528` (current block shape), `span = 347` | **3,744** | **3,928** |
| headroom to 3,120 | **−624** | **−808** |
| largest `B_adj` with `K = 8` that fits 3,120 | 214,000 B (span 277) | 195,000 B (span 256) |
| `K = 4` at `B_adj = 278,528` | — | **2,540** (fits) |

**Result: the door as declared does NOT meet 3,120 at 32K or 2M.** It fits only if (a) the adjudication block is at most about
195 KB at 2M (214 KB at 32K), or (b) at most 4 owners contend per claim (`K = 4` passes at 2M with 580 DAA headroom). Both are design
choices and are NOT adopted here. The span fit is an assumption; the conclusion depends on it.

Test evidence: `palw_tir_adjudication_slot_v1` (7 tests, M = 1, FIFO grant, duplicate refused, silent holder reclaimed at its deadline,
re-occupation capped, worst wait `(K−1)·budget`, pure in events and time).

## Final terms: block shrink under K_owner = 8 (2026-10-19)

Decision (goal authority): `K_owner = 8` is kept (fewer owners would reduce outside verifiers' access). The adjudication block is
shrunk instead by bisecting deeper. Each extra level divides every reduction block (attention tile, GDN state block, matmul block) by
16 (rounded up), adds one round of 46 DAA, and never divides the floor (one leaf + one logits tile = 16,384 B).

Door (declared fit `span(B) = 44 + ceil(B / 920)`; prep 16; output 53; `K = 8`):

`D(levels) = 16 + 53 + (R + levels)·46 + (K + 1)·span(B(levels))`, `B(levels) = 16,384 + ceil(65,536/16^L) + ceil(65,536/16^L) + ceil(131,072/16^L)`

| extra levels L | B (bytes) | span | 32K door (R = 12) | 2M door (R = 16) |
|---:|---:|---:|---:|---:|
| 0 | 278,528 | 347 | 3,744 | 3,928 |
| 1 | 32,768 | 80 | 1,387 | 1,571 |
| **2** | **17,408** | **63** | **1,280** | **1,464** |
| 3 | 16,448 | 62 | 1,317 | 1,501 |
| 7 | 16,385 | 62 | 1,501 | 1,685 |

**Minimum at L = 2 for both sizes.** At 2M: `D = 1,464 ≤ 3,120` with headroom 1,656. Dominant term at L = 2: the carrier count
`(K+1)·span = 9·63 = 567`, then the rounds `18·46 = 828`. The shortfall of the no-shrink design (808 DAA at 2M) is closed by two levels.

Implemented as a pure function (dormant, derivation only): `palw_tir_dispute_door_fit_v1` and `palw_tir_dispute_door_minimum_v1`
in `palw_tir_frame_cost_v1` (test `the_dispute_door_is_within_3120_at_32k_and_2m_at_the_minimising_level`).

**Not proven:** the span fit at block sizes near 17 KB (the fit is from two points at 0.8 MB and 2.1 MB; its intercept of 44 DAA
dominates at this size, so the result depends on it); the recomputation of the 17 KB block from authenticated operands is the
existing step machinery and is not re-derived here; no check is skipped and no hash or checkpoint is taken as proof.

## Unmet conditions

1. Shape constants of graph-v7@2097152 (kv row, state block, matmul blocks) from the program, not declared.
2. **Met by the final terms above (two extra levels, B = 17,408 B, 2M door 1,464):** the earlier shortfall at `K = 8`, `B_adj = 278,528` was 3,744 / 3,928 (see "Admission and the owner queue"). Either the block shrinks to about 195 KB at 2M or `K ≤ 4` is adopted. Both are design decisions for the user; the derived prep (16) is a declared-parameter value, to be measured.
3. Span slope re-measured on the shipped binary.
4. Adversarial tests: whole collusion + one external verifier; default on missing answers; restart and reorg identity across the fence.

## Decision requested

Adopt the log-round, block-bounded dispute as the design for the long-context door (replacing the per-claim read set of ADR-0190),
after the unmet conditions 1–3 are closed. No arming is requested by this ADR.

## Implementation

Carrier: `PalwConsensusObjectV2` tag 245 `TirDesignatedEventV1` (`PALW_TIR_DESIGNATED_EVENT_TAG_V1`), dormant behind `palw_tir_designated_answers_v1`. A-2 pattern as `MemoryRoutingV2` (tag 244): the kind is owned by the new fence slot `PalwLifecycleKindFenceV1::TirDesignatedAnswersV1`, and below the fence it rides unjudged as the dormant payload (the validate walk, the fold and `may_ride` treat it as tag 244 does). The fold's in-force check is `false` on every network, so the fold never writes it.

Unmet: the designated registry is not a field of the consensus state, so an event that reaches the validate walk while the fence is armed is refused with "not wired into the consensus state". Dormant everywhere: no shipped preset arms `palw_tir_designated_answers_v1`, and no rule is applied yet.
~~~~~~~~
<!-- ADR_SOURCE_END:0191-tir-dispute-local-decomposition-long-context.md -->

</details>

