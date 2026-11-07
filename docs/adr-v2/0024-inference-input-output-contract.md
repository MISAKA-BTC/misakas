# ADR-v2-0024 — 推論Job・prompt・構造化出力・入口契約

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 2本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0001](../rfc-v2/0001-inference-and-generative-jobs.md)

## 1. この文書の役割

ユーザーが要求した推論と、claimがcommitする入出力を一つの契約として整理する。

## 2. 統合した判断の読み方

### 2.1

常駐workerと単発Jobが同じroot・capture意味を持つ方向を記録し、APIで返した回答とcourtで裁ける計算が別製品にならないようにする。[旧ADR-0077](../adr/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)

### 2.2

入口のrequest shape、tool／構造化出力、生成制御と返却envelopeを、対応・拒否の境界ごとにまとめる。[旧ADR-0096](../adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)

### 2.3

ユーザーへの応答成功、計算実行、claim受理、認証、支払は別段階として読む。対応していない入力を黙って別の意味に変換しない。[旧ADR-0096](../adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) [旧ADR-0122](../adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)

## 3. 維持する境界・後続改定

画像・動画等の延期範囲は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)に従う。元資料のAPI実装記録は、この整理時点で再実行した互換性試験ではない。

## 4. 隣接する判断との分担

materialの認証は[ADR-v2-0025](0025-context-and-input-commitments.md)、回答課金は[ADR-v2-0026](0026-canonical-work-and-answer-accounting.md)、Remote clientは[ADR-v2-0029](0029-remote-client-mining-and-host-sandbox.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md](../adr/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) | L1–L611 | L15–L37 |
| [0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md](../adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) | L1–L685 | L15–L36 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try"></a>

<details>
<summary>0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md — 原文 L1–L611</summary>

[原文を開く](../adr/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) · SHA-256: `f3c46dd6b8192fa186aa5efddc5ef9afc0191c32300878dd35babe5393026f37`

<!-- ADR_SOURCE_BEGIN:0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md -->
~~~~~~~~markdown
# ADR-0077: A prompt a person would type is a claim the court can try

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED (2026-09-02). Written against a measurement, not a design review: the
network's stated purpose is a person using their own local LLM on a prompt of their own, with that
one inference mining (ADR-0044 §Context, ADR-0073 §1), and on the live testnet-11 (Relaunch 5e)
the widest such prompt a registered class admits is **eight tokens, answer included**, on a lane
that holds 100‰ of the cadence, weighs nothing as a block, and has never been run on the fleet.
Nothing here is armed by this document. Its one requirement is R0 (§2): the practical local
LLM and the mining runtime are the SAME inference — not two binaries that each hold half of the
purpose. Phase A is executor-side and consensus-inert. Phase B
moves the ruleset (a re-genesis on testnet-11, as every relaunch has been; mainnet ships PALW off
and is untouched). Phase C is ADR-0073 Decision 4's activation, sequenced here and not re-decided.
Phase D is refused by admission until the ruleset move that carries its rules.
**Builds on:** ADR-0044 (the receipt lane; F1–F16), ADR-0049 Decisions C/E/F (admission bounds
the court from the class's geometry; decode is adjudicable through the tiled logits pin; engine,
profile, adjudicator and inventory are one description), ADR-0070 (the model tiers' step spaces
are adjudicable end to end), ADR-0073 (Phase ① landed; Decision 2 decided; Phase ③ landed through
ADR-0074 Decision 5; Phase ④ pending), ADR-0074 Decisions 1 and 5 (`User` and `Canonical` prompt
modes; the quantum is an eighth of the canonical job), ADR-0075 (certification is a consensus
object; a class is seated by `ClassLaneCertified`), ADR-0076 (the attempt seed is `share · pwu`;
the receipt lane seeds on its own), ADR-0026 (borrow Ambient's architecture, refuse its proof
model), ADR-0028 (sampling decides nothing; only the court convicts).
**Amends:** ADR-0044 Decision 8 (PublicDA is the only weight-bearing mode) and Decision 10 (v1 is
non-streaming); the "streaming" and "encrypted prompts" rows of ADR-0044's *not decided* list;
ADR-0073 Phase ① 1e (a seat hashes the whole capture), on the free-prompt lane.

> **Security amendment appended (2026-09-02)** — see the last section (SA-1…SA-7): a public gateway spends the operator's exposure and the spend is bounded; interval openings are served to bonded requesters only; F1 covers the prompt side of the stream; the turn deadline is derived, not chosen; `PanelDa`'s enforcement is a licence until ADR-0062 lands; the persistent runtime re-verifies what it mapped.

> **ADR-0144 alignment (2026-09-21).** R0 and Phase A stay: one local inference is the answer and the claim. A public commercial gateway is out of PALW reward (P1). Decision 15 waits on ADR-0073's 0144 amendment. See the end section.

## 1. What was measured

The three classes testnet-11 registers, read off `canonical_classes_v1` /
`qwen36_canonical_classes_v1` and the profiles they project:

| class | model | `n_ctx` (prompt + answer, total) | free-prompt worker | canonical job | `pwu_per_inference` |
|---|---|---|---|---|---|
| PALW-BASE-0 `f1c5635c…` | integer floor — not a language model | 12 | none | (11, 2) | 7,708 |
| PALW-QWEN25-A16 `71bbb755…` | Qwen2.5-1.5B, graph-v2 | 16 | `palw-a16-fp-worker` | (14, 2) | 1,589,424 |
| PALW-QWEN36 `5bd9ae3d…` | Qwen3.6-35B-A3B, graph-v3 | 8 | `palw-qwen36-fp-worker` | (7, 2) | 2,685,360 |

Against those widths, the gateway's frozen plain-marker template costs ~9 tokens by itself
(`docs/palw-fp-on-registered-classes.md`, measured on the A16 tokenizer). On the hybrid the
template alone does not fit and `palw-qwen36-fp-worker` refuses the job at its own length check;
on the dense tier a browser prompt is "a few tokens each way — the pipeline proof, not the product
width". The worker binaries map the artifact **per job** (`load()` inside `run_job`; ~8 minutes
for the 33 GiB hybrid artifact, the 5e runbook's own number), the gateway spawns a process per
request and refuses `stream: true`, and no drill, test or runbook runs gateway → worker → node →
panel → receipt block: the 5e fleet table has no gateway, `testnet11-join-mining.md` does not
contain the word "prompt", two of the three FP smoke scripts pass the gateway a flag it no longer
accepts, and the llama.cpp worker all three drive omits two fields the wire type now requires and returns the
null execution root the transition refuses (`UnadjudicableCommitment`).

On the chain side, a certified claim is spendable at `final_daa + receipt_maturity` — with
`anchor_delay` 20, `window_challenge` 1,200 and `receipt_maturity` 400 that is ~1,620 DAA after
the commitment lands, **~54 hours** at the frozen 120 s cadence; the receipt lane holds 100‰ of
cadence (`ATTEMPT_SHARE_PERMILLE = 900`); and a receipt block carries no chain position
(`algo_id_carries_no_chain_position(7)` is `true`, ADR-0073 Decision 4a unlanded). Prompts are
public (`PublicDa` is the only mode the worker executes). Mainnet has no PALW at all.

### Why eight and sixteen: the ceilings are the court's, and both are ruleset facts

`verify_class_admission_v2` admits a class only if its worst close fits the RC court, and
`derive_court_cost_v1` derives that worst close **over the class's longest job** — the job an
attacker would pick. Three of its terms are linear in `n_ctx`:

* the KV history a `MatMulQuant` at an attention site reads — one range run per position, each
  `kv_dim` wide, each with its own bounded sibling set;
* the recurrence's replay — `positions = n_ctx` for every `GatedDeltaNet` ref, because the
  shipped replay is genesis-anchored: `gdn_core_genesis_replay` walks every prior position, and
  the class registers no state chunk map (`state_chunk_map_id` is the sentinel; the profile's own
  comment: "the anchor consumption is wired for attention and not yet for the recurrence");
* the prompt ids and the generated-id pin, `n_ctx × 4` each, on every close that addresses a gather.

The bound is `DEFAULT_MAX_CLOSE_BYTES = 80 KiB`, the mempool's standard-transaction mass mirrored,
because a close is a transaction. Eight positions of the hybrid hold its worst close at ~90 % of
that (the profile's derivation); n_ctx 20 is the last value the dense tier's close admits.

Independently, `worst_case_step_leaf_count_v1` enumerates `n_ctx − 1` prefill positions and one
decode call and refuses anything above `PALW_STEP_MAX_LEAVES = 2^22`. The hybrid's canonical (7, 2)
counts 2,685,360 leaves, ~298 k per position, so the ladder alone admits ~14 positions of it; the
dense tier's (14, 2) counts 1,589,424, ~99 k per position, ~42. The cap's own comment records that
it was "chosen when BASE-0's graph was eighteen steps per layer" and that "sizing a ladder to the
class set of the day is how that happens". It is inside `palw_ruleset_id_v2`.

Neither ceiling is a bug in an executor. A class's width IS the width its court can try, so
widening the lane is a court change first, and the executor-side gaps are a product that has never
been assembled. This ADR does both, in that order.

## 2. The requirement, and the principle

Two things exist in this tree today, separately. `misaka-palw-serve` is a practical local LLM: the
same A16 engine a class registers, speaking OpenAI at the artifact's full 512 positions — and its
own header says a run served there is "NOT a claim anyone can adjudicate or mine"
(`court_capable: false`). The family workers behind the gateway mine with a prompt — and admit 8
or 16 positions. Each is real; neither is the purpose, and a tree that keeps both is a tree in
which "practical" and "mines" are two products that never meet. So this ADR has one requirement,
and every decision below is in its service:

> **R0 — one inference, at the width a person uses, is one answer and one claim.** There is one
> runtime; it answers at the width the class registers; every answer it gives is captured and
> committed by the same run; and the only reasons a commitment does not reach the chain are the
> chain's — no bond, no exposure room, a class the chain does not certify — never a runtime mode.
> "Practical" is therefore a property of the class table (Phase B), not of a side binary, and
> "mines" is a property of every served inference (Phase A), not of a narrow one. Neither phase
> is done on its own: R0 is measured on ONE job id (§6).

> **R1 — the artifact goes to the user; the receipt goes to the chain.** What the chain carries
> for a job is bounded independent of what the job produced: a commitment of hashes and counts,
> the prompt ids (until Phase D), one spend envelope per quantum. What a seat verifies is bounded
> the same way: one checkpoint interval, replayed exactly and compared exactly. What the court
> opens is one leaf's neighbourhood, bounded by the same interval. The artifact — a chapter, a
> file of JSON, the code and stats for ten NPCs, a mesh — never touches the chain, and the weight
> the chain credits is the counted leaves of the computation that produced it, never the bytes it
> produced.

```text
"design ten NPCs for this game"
              │
              ▼
     the runtime — ONE inference
              │
      ┌───────┴────────┐
      ▼                ▼
 the artifact      a compact receipt
 to the user       to PALW
 (JSON, code, a    (roots and counts on the
 mesh: saved and   chain; one interval opened
 used, never on    to each seat; weight in
 the chain)        counted leaves)
```

The receipt is already compact on the output side — the commitment carries `output_root` and
counts, never the tokens. What still scales with the job today is the capture a seat fetches
WHOLE (`verify_material` hashes all of it), and the prompt ids until Phase D. In Ambient's terms a
validator verifies a token window; in PALW's terms a seat replays one checkpoint interval and a
court tries one leaf — the shape ADR-0026 borrowed, with the proof model it refused still refused:
exact comparison inside a pinned integer class, and conviction only through the court.

**Where this ADR ends.** It ends at the receipt: an inference at width, captured, certified,
spent. It does not verify what a person builds FROM the answer. "Design ten NPCs" answered as JSON
is a claim; the game that loads that JSON, the mesh a renderer makes of it, an image, a CAD part, a
compiled program, a MIDI file, a simulation run — each is a DERIVED artifact, the model's output
run through a deterministic transformer, and the chain's relationship to it is ADR-0078's: commit
what the model made and what was made from it, keep neither. Implementing this ADR alone does not
make "generate a 3D model with Qwen3.6, verified end to end" true; it makes the inference half of
that sentence true, at a width where the other half is worth building.

**The principle that delivers R0 and R1:** the court's cost is a function of the checkpoint interval,
never of the context; the ladder is sized to a prompt a person would type; and everything the
person touches is one pipeline that a drill runs end to end. Nothing about how a beacon is drawn, how a panel is seated, how a ticket
is compared or how a claim is disputed changes — F4, F5, F6 and F15 of ADR-0044 hold verbatim,
which is the whole reason a wide prompt is safe here and forbidden on the attempt lane (the win is
a quantum ticket against the class's receipt target; the prompt does not decide the lottery).

## 3. Decisions

### Phase A — the executor side, consensus-inert

**Decision 1 — one runtime: the server is the worker.** `misaka-palw-serve` is retired and its
role moves into the family workers, which gain `--mode v3-serve`: the artifact is mapped once, the
manifest handshake happens once, and jobs arrive as the SAME framed `PalwFpWorkerRequestV3` /
`PalwFpWorkerResultV3` frames over a persistent stream, one generation at a time (a single engine
and a single KV cache — the rule `misaka-palw-serve` stated, kept). Every job it answers is
captured; there is no un-captured chat path left in the tree, and `court_capable: false` ceases to
be a state a runtime can be in. The served width is the class's registered `n_ctx`, read from the
catalog row, never the artifact's rotary span: a runtime that answered wider than the court admits
would be exactly the two-products split R0 exists to close — which is also why Phase A alone does
not satisfy R0, and Phase B is what makes the one width a practical one. Per-job retention
(`material.bin`, the manifest) is unchanged, and `v3-job` stays as the one-shot form the drills and
the replay arm use. Pinned: a job's roots through `v3-serve` are byte-identical to the same job's
roots through `v3-job`.

**Decision 2 — the answer streams; the commitment does not.** The gateway accepts
`stream: true` and forwards tokens as the worker decodes them (a side channel the worker writes
beside its result frame). The commitment exists only at completion, as ADR-0044 Decision 10 says.
What binds the two: the gateway re-renders the committed `output_token_ids` when the frame arrives
and, if the rendering is not the bytes it streamed, closes the stream with an error and writes NO
commitment — a worker that shows one answer and commits another is a worker whose commitment is
not the user's inference, and F1 lives or dies on that. Streaming is UX; the consensus object is
untouched.

**Decision 3 — the gateway reads the chain it commits to.** Today it reads a hand-refreshed
`anchor.json` and an `identity.json` whose `class_id` nothing checks (ADR-0075 §4 records the gap).
It now reads, from its node over RPC: the class registry, the free-prompt-certified set (genesis
and `ClassLaneCertified`), the executor bond's status and exposure room, and a fresh anchor. A job
on a class the chain does not certify is still answered — the answer is the product — but its
commitment is marked unsubmittable and never leaves the outbox, and `/health` says so by name:
`registered`, `fp_certified`, `bond_active`, `exposure_room`. A gateway with no bond at all is
served the same way: the answer, a capture, a commitment, and the chain-side reason it stays in
the outbox — R0's "never a runtime mode", made visible.

**Decision 4 — one handoff: answer, commitment, signature, submission, capture.** After the
frame: the commitment is signed (the `kaspa-pq-signer` sidecar, or a local seed for devnets — the
rail's two forms, unchanged), the 0x4a transaction is submitted through the node, and the capture
is staged into the node's retention directory as `<claim>.material` in the same step —
automatically, per job, with retention until the claim retires (`CLAIM_RETIREMENT`). The gateway
calls the library the CLI's `fp-submit` calls; it does not shell out to it, and the CLI command
stays as the manual form. A job whose bond has no exposure room for one more claim is answered and
queued, not submitted and refused at the transition.

**Decision 5 — the family workers are the gateway's workers; the llama arms go.**
`misaka-palw-worker`'s `v3-manifest` / `v3-job` modes are deleted: consensus refuses their null
execution root, no registered class runs on that runtime (ADR-0053), and the crate is outside
`default-members` so nothing noticed it stop compiling. The three FP smokes are pointed at
`palw-a16-fp-worker`, given the gateway's current flags, and one of them runs in CI against a
fixture-sized A16 artifact; the pinned 1.7 GiB artifact is the drill's, not CI's.

**Decision 6 — the template speaks the model's own control tokens, segment-wise.** The v1
plain-marker template exists because a tokenizer running with `parse_special = false` would
tokenize ChatML markers as prose. The family tokenizer encodes SEGMENTS: markers are emitted as
the model's special ids by the gateway, user text is encoded with specials disabled, and the two
are concatenated as ids — untrusted text can never smuggle a control token, and the model sees the
template it was trained on, so EOG fires and an answer ends where it ends instead of at the
ceiling. The template id moves (`…/chat-segments/v1`); consensus sees ids only, so this is Phase A.

**Decision 7 — the drill is a chain, not a harness.** `misaka-palw-fp-devnet-e2e.sh`, shaped
like `misaka-palw-certify-devnet-e2e.sh`: N validators from one build, one of them a gateway with
a family worker under a devnet bond, a browser-shaped request, then every node read back for the
same `FreePromptCommitted`, the same `PanelBound`, the same `ReceiptLicensed`, `Final`, and a
receipt block accepted by all of them. It runs on a devnet preset whose windows are minutes, because
the in-harness finding stands — a single-chain `TestConsensus` does not accrue the DAA the windows
need — and a multi-node chain does. This drill is the gate for arming Phase B on testnet-11.

**Decision 8 — the seat verifies one interval; the executor opens it.** A free-prompt seat no
longer fetches the whole capture and hashes it (ADR-0073 1e's `verify_material` arm). It draws `k`
checkpoint intervals from the claim's beacon and its own seat index — unpredictable when the
commitment was fixed, different per seat — and asks the executor for each interval's opening: the
checkpoint chunk at the interval's start (opened against the checkpoint leg root), the committed
rows of the interval (opened against the step leg root), and the ids the interval consumed and
produced (opened against the prompt hash and the decode pin). The seat replays the interval from
the chunk with the class's own kernels and compares every row EXACTLY — the class is a pinned
integer computation, so "close" is not a verdict. Every row equal: the seat files `Valid`. A row
unequal: the seat files nothing and opens a court at that leaf, as any bonded challenger may,
holding the refutation's inputs already. Nothing served: the two-sided quorum's
`ProducerDefaulted` arm, exactly as capture withholding reaches it today. The executor retains the
whole capture under the DA obligation (`trace_manifest_root`, chunk count, retention deadline)
and serves any opening on request until the claim retires; it never ships the capture whole. Bytes
per seat become `O(k × (interval × row + log₂ leaves))`, independent of the job — R1's
verification half. This is Ambient's "verify a window" shape and not its proof model (ADR-0026): a
sampled verdict here licenses a claim that stays disputable for the whole challenge window, it
never slashes anyone, and conviction runs only through the court's bisection to one leaf — which
is why ADR-0026's warning that "verify one token" is the weakest security claim does not apply to
a seat whose verdict convicts nobody. The attempt lane's seats keep hashing their canonical-job
captures until Decision 14 makes those captures large, at which point the same arm serves both
lanes. Consensus-inert: the receipt object and its signature are unchanged; what a seat checked
before signing is the seat's own duty.

**Decision 9 — the public pages say how.** `testnet11-join-mining.md` gains "mining with your own
prompts" (the bond, the worker, the gateway, what is public, and the four stages `submitted`,
`bound`, `certified`, `spent` shown by name — a block does not follow a prompt, and any interface
on this lane says so). The stale headers are corrected: `palw-fp-on-registered-classes.md` ("Nothing
here is implemented"), `palw-freeprompt-gateway.md` ("none does yet, by design"),
`palw-fp-wiring-atomicity.md` ("not started").

### Phase B — the court prices the checkpoint, not the context (one ruleset move)

**Decision 10 — history is replayed from a checkpoint, on both kinds of layer.** For a class that
registers a state chunk map, a refutation at position `p` opens the checkpoint chunk at
`c = ⌊(p − 1) / interval⌋ · interval` and replays at most `interval` positions after it; the
genesis-anchored long form is REFUSED for such a class, so a challenger cannot pick whichever route
convicts (the equivalence the floor's test already holds: anchored and long reach the same
verdict). This is what the attention path does today under the v2 map geometry (the corrected A16
sweep exercises it). The decision extends it to the recurrence: the hybrid registers a state chunk
map whose rows are one head's `k_dim × v_dim` state plus the conv window
(`integer_kv_state_chunk_map_id_v2` is the shape for the dense cache; the recurrence gets its own
layout id, in the checkpoint profile and therefore in the class id), the worker captures the leg
against it, and `gdn_core_genesis_replay` gains its anchored twin.

**Decision 11 — admission prices the checkpoint interval, not the context.**
`derive_court_cost_v1` prices the anchored form for a class with a map: KV refs and the
recurrence's `positions` run over `min(n_ctx, interval)` positions plus ONE checkpoint-chunk
opening per history-reading ref; terminal MACs likewise. The pessimistic price of the checkpoint
sentinel stays for a class without a map — such a class cannot widen, and a low price would read as
approval. Consequence: `max_close_bytes` becomes a function of `interval`, and `interval` becomes
the one court knob a class registers that its context does not move.

**Decision 12 — the ladder is sized to an artifact, not a chat turn.** `COURT_MAX_STEP_LEAVES`
moves from `2^22` to `2^32`. The gate stays the rule — but **this paragraph stated it wrong, and
the correction changes the decision** (measured 2026-09-03, `palw_court_deadline.rs`). The shipped
clock counts MOVES, not rounds: `PalwCourtParamsV2::worst_case_duration_daa`
(`palw_mode_v2.rs:361`) is `(2 × bisection_rounds + terminal_rounds) × turn_deadline_daa`, a round
being a disclosure AND a verdict (audit M2-24). So the gate is
`(2 × ⌈log₂ leaves⌉ + terminal) × turn_deadline < window_court`, and at `2^32` with the shipped
deadline that is `(2 × 32 + 2) × 60 = 3,960 > 3,000` — **it does not fit.** The arithmetic below
was a round count where the code counts moves, so this Decision as first written could not ship.
What fits: `66 × turn_deadline < 3,000` gives `turn_deadline ≤ 45`, and the superseded "60 → 40"
proposal would have undercut it — an unanchored 8,192-position row costs `8,192 × 572 + 10,000` =
exactly 40 DAA and would sit on its own floor with zero margin. So the bound is pinned from both
ends: the ladder from above, SA-4's replay floor from below.

**45 was that bound while a move was the only thing a turn paid for. It is 42 now, and the missing
term is ADR-0080's.** A court window also pays for close ASSEMBLY — the blocks a mover spends
carrying a split close are blocks no move gets — and that reserve comes off the top before a clock
is derived: `(3,000 − 216 − 1) / 66 = 42`, where 216 is `2 × 4 × max_close_chunks` read from
`PalwCourtParamsV2`. The 45 above is this Decision's arithmetic with the reserve set to zero, which
was true of the tree it was written against and is not true of the shipped ruleset.

This is recorded rather than overwritten because the two numbers were briefly used as evidence for
each other. When the reserve was mis-sized at a value that made the ladder derive 45, that agreement
looked like two independent derivations converging; it was one derivation and one accident. The
check that survives is the other one: **at the shipped `2^22` ladder the RC derives exactly its
shipped 60**, and that holds with the reserve in. For reference, the shipped `2^22` ladder
is `(2 × 22 + 2) × 60 = 2,760 < 3,000` — 8 % margin, and today it is this upper bound, not SA-4's
lower one, that binds. At today's per-position counts `2^32`
leaves is ~14,000 positions of the hybrid and ~43,000 of the dense tier — room for an
8,192-position row of either, with the per-position growth the attention reductions' `KvScaled`
widths add; the class table, not this ADR, states each row's admitted `n_ctx`, and the derivation
refuses what does not fit, as it does now. A Merkle path grows to 32 elements — 2 KiB per opened
leaf — inside the close budget. The constant is inside the ruleset id and its comment says it
"cannot be raised afterwards"; that is correct, and a ruleset move is what a testnet-11 relaunch
is.

**Decision 13 — the context ladder: rows at the artifact's rotary span, by the ADR-0075 route.**
The dense artifact's rotary table covers 512 positions (`max_position` 512, the converter's
default); the hybrid's "still covers 512". So the first practical rows are
`Qwen/Qwen2.5-1.5B/graph-v2` at `n_ctx` 512 and `Qwen3.6-35B-A3B/graph-v3` at `n_ctx` 512 — each a
NEW class id, because a class IS its graph, registered and seated through `palw-certify drill|bind`
and `misaka-cli palw submit-object` once Phase B's court is the shipped one. 512 is the width
`misaka-palw-serve` serves today, so this is the number at which the practical runtime and the
mineable one become one row — R0's width. The 8- and 16-token rows stay on chain exactly as they
are. Wider than 512 needs a re-converted artifact with a wider table, and takes the same route:
the ladder this ADR plans is 512 → 2,048 → 8,192 positions per family, one row each, because an
artifact a person keeps — a file of JSON, a source file, a chapter — is thousands of tokens, not
hundreds. The bundle's own caps move with the ladder in Phase B's one ruleset move:
`MAX_PROMPT_TOKENS` (512), `MAX_DECODE_TOKENS` (1,024) and the V2 trace-event cap (4,096) go to
the ladder's top, so a row the court admits is never refused by a cap sized for the 16-token era.

**Decision 14 — the canonical job grows with the context, so the cap stays a jackpot bound.** A
quantum is `pwu_per_inference / 8` leaves (ADR-0074 Decision 5) and a receipt is capped at 64 quanta
(`MAX_QUANTA_PER_RECEIPT`). With the hybrid's (7, 2) canonical job, a 512-token job is ~450 quanta,
capped to 64 — 86 % of real work certified and uncounted. The cap is the per-receipt jackpot bound
ADR-0044 Decision 5 meant; it must not become a tax on ordinary use. So registration requires the
canonical job's footprint to be at least `n_ctx / 8`: the widest admissible job then earns at most
`8 × 8 = 64` quanta by construction, and `verify_class_admission_v2` refuses a row that violates
it. Consequences, stated: `pwu_per_inference` grows with the canonical job; the attempt seed follows
it (ADR-0076 Decision 1 — cadence is unchanged, the draw rate falls and the seed compensates); a
claim's reserved exposure `pwu_per_inference × slash_value_per_pwu` grows with it, so the bond floor
a producer needs is re-derived by the existing rule, and a canonical hybrid inference at
1.75 tok/s takes ~40 s for a 64-position job instead of ~9 s — inside the 120 s cadence.

### Phase C — the weight (sequenced, not re-decided)

**Decision 15 — weight follows measured supply.** ADR-0073 Decision 4 (a receipt block has a
chain position; the share leaves 900‰ on a schedule bounded by what the lane produces) activates
when a Phase-B class has produced receipt blocks on testnet-11 through Decision 7's path for one
full retarget span, so 4b's schedule has a supply to follow rather than a promise. Until then a
spent quantum adds exactly what it adds today. Order, as ADR-0073 states it: court, price, weight,
share — this ADR sits between price and weight.

### Phase D — a prompt that is not published

**Decision 16 — `PanelDa` (privacy mode 2).** The job carries `prompt_token_ids_hash` as it does
now; the commitment transaction carries NO ids. The ids travel with the capture the executor
already serves to its panel (`<claim>.material`, `request_palw_material`); a seat checks
`H(ids) == prompt_token_ids_hash` before it reads anything else, and files `Valid` only for a claim
whose ids it holds. Withholding is the two-sided quorum's `ProducerDefaulted` arm — the same arm
capture withholding reaches today (ADR-0073 Phase ① 1a/1e) — and a court close that addresses a
gather carries the ids as it does now, so a disputed prompt becomes public. The honest name for
this is *private unless disputed*: five seats see the prompt, a dispute publishes it, and nothing
here is confidentiality. The gateway shows the mode and that sentence before first use (ADR-0044
Decision 8's obligation, carried). Weight: the same as `PublicDa` — the panel still replays from
data it holds, which is what the weight rests on. Admission refuses mode 2 until the ruleset move
that carries these rules, exactly as the worker refuses every non-PublicDA mode today ("a mode the
panel cannot replay must not execute").

## 4. What this costs, stated before it is measured

* **Capture bytes.** A seat fetches `k` interval openings (Decision 8), `O(k × (interval × row +
  log₂ leaves))` — independent of the job. The executor retains the whole capture, which at
  8,192 positions is gigabytes per job: retention is the executor's disk, priced by the retention
  deadline, and the Decision 7 drill measures it.
* **Commitment bytes.** `PublicDa` carries `n_ctx × 4` bytes of ids — 2 KiB at 512 — inside a
  standard transaction. `PanelDa` carries none.
* **Court time.** A rung is a turn pair and the clock runs PER MOVE, so 34 rounds is 66 moves, not
  34 (corrected 2026-09-03 — see Decision 12). At the shipped `2^22` ladder that is
  `(2 × 22 + 2) × 60 = 2,760` DAA, ~92 hours worst-case honest prosecution, inside `WINDOW_COURT`
  with 8 % to spare; at Decision 12's `2^32` it is 3,960 at a 60-DAA deadline and does NOT fit —
  42 does, once ADR-0080's close-assembly reserve is charged to the window (45 without it; see
  Decision 12). `MAX_CLAIM_EXPOSURE_DAA` is unchanged.
* **Executor time.** The hybrid decodes at ~1.75 tok/s on a 24 GiB M4 Pro: a 300-token answer is
  ~3 minutes, streamed. The dense tier is interactive (~30 tok/s). Integer GPU kernels remain the
  practical-runtime plan's next stage (§8).
* **Bonds.** Decision 14 raises per-claim exposure by the canonical job's growth; a producer on a
  practical row needs the collateral the existing rule derives for it.
* **Identity.** Phase B moves every V2 preset's fingerprint: a re-genesis on testnet-11. Mainnet is
  byte-for-byte untouched (PALW off). Phases A and D move nothing until their own gates.

## 5. Invariants the tests must hold

```
W0   R0: every answer the runtime gives has a capture and a commitment from the same run; the
     served n_ctx equals the class's registered n_ctx; a commitment that does not reach the chain
     names a chain-side reason (no bond, no exposure room, uncertified class) in /health; no
     binary in the tree answers a chat without a capture.
W1   For a class with a registered state chunk map, derive_court_cost_v1 at n_ctx = interval,
     2·interval and 8·interval yields the same max_close_bytes and max_terminal_macs.
W2   The anchored refutation and the long form reach the same verdict on honest material, for
     attention AND the recurrence; the long form is refused for a mapped class.
W3   The widest job a class admits earns ≤ max_quanta_per_receipt quanta; registration refuses
     a row whose canonical footprint is under n_ctx / 8.
W4   (⌈log₂ COURT_MAX_STEP_LEAVES⌉ + terminal) × turn_deadline < window_court, at every preset.
W5   The streamed bytes equal the rendering of the committed output ids, or no commitment is
     written (one inference, one commitment — F1 through the stream).
W6   A job's roots through v3-serve equal its roots through v3-job, byte for byte.
W7   The devnet drill ends in a receipt block accepted by every node, on a chain whose DAA has
     advanced through bind, challenge and maturity.
W8   PanelDa: a seat holding no ids cannot file Valid; a hash mismatch is refused by name;
     withholding reaches ProducerDefaulted; a court close still carries the ids.
W9   No decision here changes how a beacon, a panel or a ticket is derived: ADR-0044 F4–F6 and
     F15 hold verbatim, pinned by the existing golden vectors.
     (Naming note, 2026-09-03: do NOT grep the tree for "W9" to find this invariant's test. The
     only W9 strings in the repository are the A16 kernel op of the same name —
     `palw_base0_a16.rs:320`, `engine_a16.rs:28`, `kernels.rs:578` — so a grep returns three false
     positives and an unwary reader records the invariant as held. The goldens W9 rests on do
     exist; nothing asserts them under this label.)
W10  R1: the bytes the chain carries per claim and the bytes a seat fetches per claim are
     bounded independent of decode_tokens_executed; a seat's comparison is exact equality; a
     sampled verdict never slashes — conviction is the court's alone.
```

## 6. Order of work

| unit | content | done when |
|---|---|---|
| P-01 | Decision 1 — `v3-serve` on both family workers; `misaka-palw-serve` retired | W0 and W6 green; the hybrid answers a second job without re-mapping |
| P-02 | Decision 2 — SSE side channel + the re-render check | W5 green, with a deliberately mismatching worker refused |
| P-03 | Decision 3 — the gateway reads registry, certified set, bond, anchor over RPC | `/health` names all four; an uncertified class answers and never submits |
| P-04 | Decision 4 — sign, submit, stage capture, retain; shared with the CLI | a browser request ends in `FreePromptCommitted` on a devnet node with `<claim>.material` present |
| P-05 | Decision 5 — llama v3 arms deleted; smokes repointed; one in CI | `cargo build` + the CI smoke green on a fixture artifact |
| P-06 | Decision 6 — segment-wise chat template | EOG observed on the dense tier; the F1 golden test extended to specials |
| P-07 | Decision 7 — the devnet drill | W7 green on three nodes |
| P-08 | Decision 8 — interval openings served to seats; the whole-capture arm retired on the FP lane | a seat verifies a claim fetching `O(k × interval)` bytes whatever its length; W10 green |
| P-09 | Decision 9 — pages | the join-mining page has the section; the three stale headers are gone |
| P-10 | Decision 10 — recurrence state map + anchored replay | W2 green for `GatedDeltaNet` |
| P-11 | Decision 11 — `derive_court_cost_v1` prices the interval | W1 green |
| P-12 | Decision 12 — `COURT_MAX_STEP_LEAVES = 2^32`; the bundle caps at the ladder's top | W4 green; ruleset id moves once for P-10..P-14 |
| P-13 | Decision 13 — the 512 rows, drilled and bound on devnet, then testnet-11; then 2,048 and 8,192 | each row `ClassLaneCertified` on the free-prompt lane |
| P-14 | Decision 14 — the footprint rule at registration | W3 green |
| P-15 | Decision 15 — ADR-0073 Phase ④ activation | one retarget span of measured receipt supply on a 512 row |
| P-16 | Decision 16 — `PanelDa` | W8 green; drilled; its own ruleset move |

**Done when** the Decision 7 drill, on the widest Decision 13 row registered at the time, shows
ONE job id in three places: the gateway's streamed answer to an artifact-shaped request — a JSON
or code generation that fills the row, kept by the client and never sent to a node — the node's
`FreePromptCommitted` → `Final` for that claim, certified by seats that fetched interval openings
and nothing whole, and a receipt block spending one of its quanta, accepted by every node. R0 is a
property of a single inference, so it is measured on a single job id — never on one log of a
practical runtime beside another log of a mining one. Until that job id exists, this ADR is not
implemented, whatever else has landed.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0044 Decision 8 — `PublicDa` is the only weight-bearing mode | amended by Decision 16: `PanelDa` bears weight once armed; encrypted modes stay a future ADR |
| ADR-0044 Decision 10 — v1 is non-streaming | amended by Decision 2 |
| ADR-0044 *not decided* — streaming | decided (Decision 2) |
| ADR-0044 *not decided* — encrypted prompts | partly decided (Decision 16 is served, not encrypted); ML-KEM/ZK forms remain not decided |
| ADR-0044 Decision 5 — `max_quanta_per_receipt` bounds the jackpot | honoured; Decision 14 keeps it a jackpot bound rather than a tax |
| ADR-0049 Decision C — admission bounds the court from the geometry | honoured; the bound's form changes (Decision 11) |
| ADR-0073 Decision 4 — receipts gain position and share | sequenced by Decision 15, not re-decided |
| ADR-0074 Decision 5 — the quantum is an eighth of the canonical job | honoured; Decision 14 sizes the canonical job |
| ADR-0073 Phase ① 1e — a seat hashes the whole capture and samples `k` leaves | amended by Decision 8 on the free-prompt lane: the seat fetches `k` interval openings and hashes nothing whole |
| ADR-0026 — borrow Ambient's shape, refuse its proof model | honoured: Decision 8 is the window-verification shape with exact comparison and a court behind it; "verify one token is the weakest claim" stands, because a seat's verdict convicts nobody |
| ADR-0044 Decision 9 — `max_prompt_tokens` / `max_decode_tokens` are bundle caps | honoured; Decision 13 moves them with the ladder, in the ruleset move |
| `misaka-palw-serve` — "a run served here is … NOT a claim anyone can adjudicate or mine" | retired by Decision 1; no un-captured serving path remains |
| `palw_qwen36_profile` — "n_ctx 8 … a larger context returns when the recurrence's replay is checkpoint-anchored" | that return is Decisions 10–13; the row itself is unchanged |
| `palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES` — "cannot be raised afterwards" | correct; raised by a ruleset move (Decision 12) |

## 8. What is deliberately not decided

* **Integer GPU kernels.** ADR-0053 removed the float Metal path; no integer GPU kernel exists.
  The hybrid stays a streamed, minutes-per-answer tier until the practical-runtime plan's next
  stage lands. A class's speed is a host fact and never reaches consensus.
* **KV continuation across turns** (`ContinueFrom{parent_receipt}`). Each request re-sends its
  history and pays prefill for it; correct, and not cheap. Its own trace semantics, its own ADR.
* **The windows.** ~54 hours from commitment to spendability is bind, challenge and maturity —
  fraud-proof safety, not a UX knob. The product shows the stages by name instead.
* **Private eligibility and receipt transfer** — ADR-0044's list stands.
* **Derived artifacts.** A 3D scene, an image, a CAD part, a program, a map, a MIDI file, a
  simulation — what a deterministic transformer makes of the model's output — are ADR-0078's:
  committed as a derivation bound to the claim, never carried, never weighed here.
* **Encrypted DA.** `PanelDa` is served to five seats in the clear. Encrypting to seats (ML-KEM)
  is a later ADR if seat leakage is measured to matter.

## 9. Number hygiene

This is ADR-0077; ADR-0076 is the last committed on `main`. A concurrent claimant of 0077 renumbers
the later writer, per ADR-0036 Decision 5.

## Security amendment (2026-09-02) — hardening before Phases A–D are built

**SA-1 — A public gateway spends the operator's exposure, and the spend is bounded.** Under
Decision 4 a stranger's prompt becomes the operator's claim: it reserves `claim_exposure` on the
bond and, if the pipeline is faulty, forfeits it. Rules: (a) a per-source quota and a global
public-job budget expressed as a fraction of the bond's exposure room, so the operator's own claims
are never starved; (b) a queued commitment expires with its anchor and is never submitted stale;
(c) the operator may mark a source class "answer, never commit"; (d) the loss bound is stated in
the gateway's own `/health`: at most `claim_exposure` per claim and at most the exposure ceiling
ratio of collateral in flight (`FreePromptExposureCeiling`, 500‰ on the RC).

**SA-2 — Interval openings are served to bonded requesters only, bounded and rate-limited.**
Decision 8 turns the executor into a server of openings; an unauthenticated opening endpoint is a
bandwidth amplifier against every producer. A request carries the requester's bond key and
signature — a seat of the claim's panel or an Active bond acting as challenger — is refused
otherwise, is bounded to `O(interval × row + log₂ leaves)` bytes per opening, and is rate-limited
per bond. The same rule covers `request_palw_material` today.

**SA-3 — F1 covers the prompt side of the stream.** Decision 2 checks the streamed answer against
the committed output ids; the gateway must also commit exactly the prompt ids it built (segments,
specials, user text) and abort — no commitment — if the displayed prompt and the committed ids
diverge. W5 is extended to both sides.

**SA-4 — The turn deadline is derived, not chosen.** The implementation found Decision 12's
"2,040" counted rounds where the shipped clock counts moves ((64 + 2) × 60 = 3,960 > 3,000) and
proposes `court_turn_deadline` 60 → 40. A deadline is a slashing rule for the honest responder: it
must be ≥ the worst-case honest replay of one interval on the slowest fleet host plus
`2 × NETWORK_DELAY_BOUND`, derived and pinned per class row, and the ladder depth
`(⌈log₂ leaves⌉ + terminal)` must fit `window_court` under that deadline — otherwise the court
convicts by clock.

*Which cost "replay" is, settled 2026-09-03 rather than assumed.* The derivation
(`consensus/core/src/palw_court_deadline.rs`) prices a replayed position at a decoded token's
milliseconds, and the obvious objection is that a committed run measures **90× slower** than a bare
forward — 5.66 s/token captured against 0.060 s/token not — which would make every floor here two
orders of magnitude too small. **It does not, and the reason is that a court replay is not a
committed run.** `gdn_core_anchored_replay_v1` (`palw_step_refute.rs:4228`) returns the output row
and the recurrence state and contains **no hash, no digest and no leaf hashing anywhere in its
body**: it is pure arithmetic. The 90× belongs to the EXECUTOR, which hashes every leaf of every
step into the tree as it produces. So a seat's replay is `interval × forward`, the 0.060 figure,
and the floors stand.

One term remains **unmeasured and is named rather than buried**: a responder does not only replay,
it also produces an OPENING at the disputed step, and that opening's leaves must be hashed. Read
structurally that is ONE step's leaves, so it is additive — `interval × forward + one step's leaf
hashing` — rather than a multiplier on `interval`. Until somebody measures it the floor is
conservative, which is the direction a slashing rule should err in; a deadline that is too generous
costs a slow prosecution, a deadline that is too tight convicts an honest responder.

**SA-5 — Decision 16's enforcement is a licence, not a punishment, until ADR-0062 lands.** With
ADR-0065 Decision 4 armed, id withholding under `PanelDa` yields abstention → no quorum → redraw →
timeout void without slash. The mode's disclosure text says so. And seats, gateways and workers log
no prompt ids or text by default — "private unless disputed" is false if the default log is a
disclosure.

**SA-6 — Decision 1's persistent runtime re-verifies what it mapped.** Mapping once is the point;
the artifact is opened read-only, its digest verified at map time (ADR-0079 Decision 9),
re-verified when the file's identity (device, inode, size) changes, and an I/O fault on a mapped
page is a `JobFailed`, never a crash of the supervisor or the node.

**SA-7 — Decision 14's exposure growth is refused at the gateway, not discovered at the
transition:** when a claim's `claim_exposure` would exceed the bond's room, the gateway answers and
does not submit (Decision 4's queue), and `/health` names it.

## Implementation note, 2026-09-06 — Decision 16's transport (`f7363db9`)

Decision 16 shipped every rule but the transport: the payload rule, the seat's admission, the
withholding arm and the fences existed; a producer still announced a mode-2 material to every peer,
a relay forwarded it, a serve answered any bonded requester, and the extraction walk passed a
literal `false` for the fence. Now: the mode is read off a payload's prefix
(`palw_fp_privacy_mode_peek_v1`); a mode-2 material is never announced, never relayed
(`PalwGossipAdmit::Private` — a solicited copy reaches only the asking node's inbox), and served
only to `PalwChainStateV2::claim_readers_v2` — the executor, the bound panel's seats and the
challengers of open sessions — on both the capture lane and the interval-opening lane (a prefill
interval's opening carries the prompt's embeddings). The gateway files mode-2 under
`--privacy panel-da`, refused where `panel_da_armed` is false, with the disclosure sentence printed
verbatim at boot; the commitment builder carries no ids on chain and the submitter stages the
worker's beside the material. Armed on a carded mainnet from genesis; dormant on testnet-11 and
devnet. Design record: `docs/palw-private-prompts-design-2026-09-05.md`.

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P1: rewardable inference
is local; a localhost OpenAI-compatible endpoint is in scope; an internet-facing gateway that sells
somebody else's GPU is not.

* **Kept, and still the product path.** R0 (the practical local LLM and the mining runtime are the
  same inference). Phase A (executor-side, consensus-inert): stream the answer, commit after start,
  the family workers are the gateway's workers. Decision 3: the gateway reads the chain it commits
  to — that remaining half is ADR-0075's gateway gap, on the local `/v1`, not a marketplace.
* **Withdrawn as a PALW-reward goal.** "A node can be a public LLM entrance" (this ADR's pairing
  with ADR-0079's Done-when, SA-1's public-job budget as a product, Decision 9's public mining
  pages as a commercial serving surface). A loopback bind is the default product. A non-loopback
  bind remains ADR-0079 Decision 10's acknowledged operator act; it does not earn PALW for serving
  a stranger's prompt on the operator's GPU. SA-1 still prices exposure if that bind is used; it
  no longer describes what PALW is for.
* **Decision 15** (ADR-0073 Phase ④ activation after one retarget span of measured receipt supply)
  is withheld with Phase ④ itself — see ADR-0073's 2026-09-21 alignment.

## Mission alignment amendment — 2026-10-07

Decision 16および2026-09-06 transport記録のPanelDa/claim_readers_v2は旧規則の履歴である。Panelと既存courtのchallengerしか読めないまま、将来の公開訴追条件を満たしたとは扱わない。court開始前にも外部bondが検査に必要な認証materialを取得できる経路、または別途承認された同等の公開証明方式を必要とする。それまで新しい秘密入力reward profileを閉じる。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md -->

</details>

<a id="source-0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed"></a>

<details>
<summary>0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md — 原文 L1–L685</summary>

[原文を開く](../adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) · SHA-256: `acafd1a31c5c8c36ac8ccafabddc05be0b086dba27ec8597ded90aa5d80206fe`

<!-- ADR_SOURCE_BEGIN:0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md -->
~~~~~~~~markdown
# ADR-0096 — The app you already use is the entrance, and the shape of the answer is committed

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-10. Part A (the entrance) and Part C (distribution, settings, the
  model-request intake) are host-side and land on `feat/adr-0096-everyday-lane` in both
  repositories (misakas and MISAKA-Studio) as they are built — §9 records what has. Part B (the
  shape of the answer) is a ruleset move behind a fence that is `None` on every preset and, as
  ADR-0082 Decision 11 did, is REFUSED at assembly on a build that cannot carry it; nothing in
  Part B changes a shipped job, a shipped claim or the fingerprint.
* Builds on: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (R0: one
  inference, one claim; Decision 2: the answer streams, the commitment does not; Decision 3: the
  gateway reads the chain it commits to; Decision 6: the template speaks the model's own control
  tokens; Decision 13: the 512-token rows; Decision 16: `PanelDa`),
  [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (Decision 2: the DSL is
  the claim's output, canonicalized by a registered grammar; Decision 6: delivery; Decisions 8 and
  9: the kind table is open), [0082](0082-the-close-is-flat-in-the-context.md) (Decision 11: the
  seeded argmax, and the one fence that arms the sampler; §Part D),
  [0084](0084-the-ids-ride-the-capture-stays-home.md) / [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (what is served, and its cap), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)
  (Decision 4: a wider row is a new class at mint, never a raised ceiling),
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) (a line, an owner, a
  version — the thing a model request becomes).
* Amends: ADR-0078 Decision 6's rendering premise, by Decision 6 below, **only past the fence of
  Decision 8** — the shipped rendering does not move.
* Supersedes nothing.

## 0. The sentence this ADR is

Everything a person actually does with a small local model — point the app they already use at
it, keep the conversations they had, ask for JSON, let it call a tool, carry a long thread — is
today either refused by the free-prompt lane or done in a way the chain cannot see. This ADR makes
the **entrance** the person's own app, with their history and their tools, and makes the
**shape** of the answer a field of the job the seat replays and the court can try — while moving
none of the constraints the court needs: one inference is still one claim, the prompt still
reaches the model intact, the context is still the class's, and nothing is ever downgraded
silently.

The operator's word for it (2026-09-10): *the base is finished; what is in the way is the
onboarding, the settings, and above all the constraints that come from the fact that this is
mining.* Those constraints are real and most of them are correct. This ADR sorts them into the
ones an entrance can absorb without lying, and the one that has to become consensus.

## 1. What was measured

Every item below was read on `main` at `e65ccf20` (misakas) and `7096533` (MISAKA-Studio) on
2026-09-10, with the line it lives on.

### 1.1 The entrance refuses what a stock client sends

* The gateway's surface is `POST /v1/chat/completions`, `GET /health`, `GET /v1/artifacts/<id>`
  and nothing else (`misaka/palw/gateway/src/main.rs:1561-1563`). Its request parser takes
  `model`, `messages`, `max_tokens`, `stream`, `derive`, `serve_dsl`, `temperature`, `seed`
  (`main.rs:559-594`). A message is `{role, content: String}`; the roles are `system | user |
  assistant` and any other is refused, not dropped (`misaka/palw/gateway/src/wire.rs:129-137`).
* The Studio's `/v1` (`crates/misaka-studio-runtime/src/api/openai.rs:37-41`) serves `models`,
  `chat/completions`, `completions`, and its `ChatMessage` is likewise `{role, content: String}`
  (`backend/mod.rs:41-45`).
* So a client written against `api.openai.com` fails on four ordinary things: `content` as a list
  of parts (every current SDK's multimodal-shaped default), a `tool` turn or a `tools` list, a
  `response_format`, and a `temperature` — which the gateway refuses BY NAME while
  `Params::palw_fp_decode_rules` is dormant (`main.rs`, `sampling_from_request`; the fence is
  `None` on every preset and its arming is refused by `validate_palw_v2` on this build,
  `consensus/core/src/config/params.rs:1362`, `:1993-2003`). The refusal is right — ADR-0082
  Decision 11's reason stands — and the client still gets a 400 for a field its SDK set by
  default.
* The Studio does not forward sampling knobs to the gateway at all
  (`backend/gateway.rs:195-199`), so through the app the same request works; the app just never
  says what it dropped.

### 1.2 Conversations live in the window

`ui/src/store/studio.ts:3-10`: conversations are the ONLY thing the UI persists, and they persist
in the WebView's `localStorage` through `zustand/persist`. There is no API that lists them, no
file a person can copy, no import. A person moving from another local app arrives with nothing,
and a person who reinstalls leaves with nothing. The runtime's provenance log
(`records.rs`) is the opposite shape — an append-only JSONL with hashes and, opt-in, the text —
and it is written per completion, not per conversation.

### 1.3 Four tables say what to install, and one of them names a binary that no longer exists

* `PalwClassSpec.artifact` (`crates/misaka-studio-core/src/palw.rs:39-70`,
  `PalwArtifactSource::{DerivedFromSeed, Download{sha256,size,hf_repo}, ConvertLocally}`) pins the
  class artifacts; `spawn_default_class_install` (`state.rs:194`) installs the default one.
* `resolve_program` is spelled twice (`backend/llamacpp.rs:63`, `backend/misaka.rs:79`: beside
  the executable, then `engines/`, then `PATH`); `resolve_kaspad` / `resolve_misaka_cli`
  (`node.rs:793`, `:818`) are spelled a third and fourth time with a different search order (no
  `engines/`).
* The desktop bundle stages `resources/*` (`desktop/src-tauri/tauri.conf.json:26`) and nothing
  checks, at build time, that what it stages is what the runtime will look for — the 2026-09-06
  "No UI bundle" release and the v0.1.0 unreachable-sidecar release were both this.
* And `backend/misaka.rs:1-6` drives `misaka-palw-serve`, which the node tree **deleted on
  2026-09-02** (`2f688bc1`: "one runtime — `v3-serve` on both family workers, `misaka-palw-serve`
  retired", ADR-0077 Decision 1). A component that one repository names and the other no longer
  builds, and nothing in either repository's CI knew.

### 1.4 The settings a panel shows are not the settings the engine was built from

`apply_settings` (`state.rs:267-292`) rebuilds the engine when a hand-kept list of fields
changes. On 2026-09-05 that list lacked the gateway URL and the pool token, so "Forget slot →
Join" left the chat mining on the slot the person had just left, with that slot's token, while
every status panel named the new one (Studio `ceee727`). The fix added two fields to the list.
The list is still a list. `/api/v1/runtime` (`state.rs:48-59`) reports the engine's own view of
what it loaded; nothing reports, beside it, what the running engine was constructed FROM, so a
panel cannot say "the file says X, the engine holds Y".

### 1.5 Adding a model has a path and no door

The path exists end to end — the SDK (`docs/palw-model-onboarding-sdk.md`), conversion,
`--palw-register-class` as a producer-start phase (the Studio's Network tab does this),
certification as a consensus object (ADR-0075, `docs/palw-certify-a-new-model.md`), and a line
with an owner and versions (ADR-0088). What does not exist is the place a person who is not going
to do any of that ASKS: `.github/ISSUE_TEMPLATE/` holds a bug report and a feature request. A
model request is neither, and the questions it has to answer (weights, licence, architecture,
quantization, context, which lane, who converts, who bonds the line) are not in either form.

### 1.6 The context is 512 and the answer is what is left of it — and the 256 wall is gone

* The row is 512 (ADR-0077 Decision 13); the budget is `prompt + decode ceiling ≤ n_ctx`
  (`misaka/palw/base0/src/fp_worker.rs:745-749`), so what a person can get back is
  `512 − prompt`. A ChatML frame with a system line costs on the order of ten tokens; a one-shot
  DSL exemplar costs ~130 (the 2026-09-03 drill's `prompt 134`). ADR-0092 Decision 4 says why the
  number does not move on a running chain: the row's history dissection is what eats the court's
  clock, and a wider row is a NEW class at mint, not a raised ceiling.
* The Studio trims history to fit (`backend/mod.rs:216`, `fit_messages_to_budget`: the system
  prompt and the newest turn survive, oldest turns go first) and says how many turns it dropped.
* The 256-token decode wall of 2026-09-05 (the producer hardcoded one retained-trace chunk;
  `PALW_FP_TRACE_CHUNK_EVENTS_V3 = 256`, `palw_freeprompt_v3.rs:151`) is **fixed on `main`**:
  `0001f34c` derives the free-prompt manifest from the trace's own leaves, chunked at 256
  (`misaka/palw/base0/src/backend.rs:406-408`), and the Studio measured decode 257/257 committed
  (`backend/gateway.rs:238-243`). The remaining ceiling is the row and `--max-decode-cap`
  (hard maximum 4,096, `main.rs:114`).

### 1.7 The shape of the answer is nobody's

* Every backend selects with `base0_decode_token_select_v1` — the plain argmax
  (`qwen25_a16_backend.rs:341`, `:355`; `palw_step_refute.rs:3285`). ADR-0082 Decision 11's
  `decode_token_select_v2` (the seeded argmax, `palw_decode_select_v2.rs:185`) exists in
  consensus-core and in no engine, and the fence that would select it is dormant.
* ADR-0078 gives seven kinds (Decision 8) whose grammars canonicalize the answer AFTER the fact
  and, by Decision 2, change nothing about how it was produced: "nothing about the prompt or the
  answer is changed to make it parse". There is no `json` kind.
* So "give me valid JSON" is a request the lane can only hope to honour. On a 1.5 B model with a
  few hundred tokens of room that hope is measured in the derivation notes of 2026-09-04: a
  derivation succeeded only when `max_tokens` happened to equal the answer's exact length, because
  the model keeps generating past its own end-of-generation token to the declared budget
  (ADR-0077: the commitment covers every executed token) and the derivation reads the whole
  rendering (ADR-0078 Decision 6).

The operator's ordering of these is the right one: the everyday use of a small model is
*hand it a simple task → get the result in a shape (JSON, XML) → store it → use it*. Format is
not a nicety on that path; it is the path. And it is the one item on the list that an entrance
cannot absorb honestly, because the seat replays the decode and a mask the seat does not apply is
a token the seat will not reproduce.

## 2. The requirement

R0, F1 and the refusal doctrine are kept whole: one inference is one claim (ADR-0077 R0); the
prompt reaches the model as the person wrote it (ADR-0044 F1); a request the lane cannot honour
is refused BY NAME before the inference, and nothing is downgraded silently (ADR-0082 Decision
11's rationale, kept as a rule of this ADR). Consensus moves only by activation
(ADR-0065/0066's doctrine; regenesis is not on the table).

Under those: a person's existing OpenAI-shaped app must work against the Studio and the gateway
with the base URL changed and nothing else; their conversations must be theirs (a file, an API,
an import); a long thread must be servable as several claims with the seams visible; a request
for a shape must be either committed — replayed by the seat, triable by the court — or clearly
marked advisory; every component a person needs must come from one manifest that both
repositories publish and both check; every settings panel must show what the running process was
built from; and a model that does not exist yet must have a door to knock on.

## 3. Decisions

### Part A — the entrance (host-side; no consensus object moves)

**Decision 1 — one OpenAI surface, spelled once and served twice.** The Studio's `/v1` and the
gateway's `/v1/chat/completions` accept the same request shape and return the same envelope; the
shape is this section, and a conformance corpus (`docs/openai-surface/v1/*.json` in misakas,
mirrored into the Studio's tests by a script that asserts the copies are identical) pins it in
both trees. Accepted, and what happens to each:

| field | Studio `/v1` | gateway `/v1/chat/completions` |
|---|---|---|
| `messages[].content` as a list of `{type:"text"}` parts | flattened to one string, parts joined by `\n` | same |
| `messages[].content` with a non-text part (`image_url`, `input_audio`, …) | refused by name | refused by name |
| `tools`, `tool_choice`, `messages[].tool_calls`, role `tool` | Decision 2 | Decision 2 |
| `response_format` | Decision 3 / Part B | Decision 3 / Part B |
| `temperature`, `top_p`, `top_k`, `min_p`, `repeat_penalty`, `seed`, `stop` | the engine's | Decision 4 |
| `n ≠ 1`, `logprobs`, `functions` / `function_call` (legacy), `stream_options` other than `include_usage` | refused by name | refused by name |
| `Authorization: Bearer …` | the Studio's own key when one is set | ignored; the pool slot token (`x-pool-token`) is the credential |
| `GET /v1/models` | the models on disk | the one class this gateway serves, as `misaka-palw-fp-v3` plus the class name |

The envelope gains nothing OpenAI does not have except the `misaka` object every gateway answer
already carries (job, claim, roots, `output_token_ids`, `job_context`, derivation), which Decisions
2–5 extend. A field this table does not name is refused by name, never dropped — with two
deliberate exceptions, both reported rather than silent: a sampling knob at its IDENTITY value
(`top_p: 1`, `top_k: 0`, `min_p: 0`, `repeat_penalty: 1`, `frequency_penalty: 0`,
`presence_penalty: 0`, `temperature: 0`), which a stock SDK sends by default and which asks for
nothing, is accepted and listed in `misaka.sampling.requested`; and the fields OpenAI defines as
having no effect on the answer (`user`, `metadata`, `store`, `parallel_tool_calls`,
`max_completion_tokens` as an alias of `max_tokens`) are accepted and listed in
`misaka.ignored_fields`. A knob at any other value is refused by name on the gateway (Decision 4).

**Decision 2 — a tool call is a turn of text; the round-trip is the app's; the schema is the
model's own template.** The lane's chat template is the model's (ADR-0077 Decision 6), and the
shipped classes' models (Qwen2.5-Instruct, Qwen3.x) were trained on the Hermes-style tool
convention: the tool list rides the SYSTEM turn as `<tools>…</tools>` JSON, a call is emitted as a
`<tool_call>{…}</tool_call>` block, and a tool's reply is a `user`-role turn wrapping
`<tool_response>…</tool_response>`. So:

* `tools` are rendered into the system segment as the model's own text (a `Text` segment —
  SA-3's subsequence check is unchanged, the ids are the prompt's); a request with `tools` and no
  system turn gets one that holds only the tool block.
* A `tool` message is rendered as the model's tool-response turn; an assistant message carrying
  `tool_calls` is rendered as the `<tool_call>` block it produced. Both are text.
* The committed answer is parsed for `<tool_call>` blocks after the run; each becomes
  `choices[0].message.tool_calls[]` in OpenAI's shape with `finish_reason: "tool_calls"`, and the
  raw text stays in `misaka.answer_untrimmed`. Parsing changes nothing committed.
* The round-trip — the app executes the tool and sends the result back — is one job per leg.
  Every leg is its own inference and its own claim (R0), reported in `misaka.jobs[]` when the
  entrance drove more than one (Decision 5). Nothing on chain knows what a tool is.
* Under Part B a `tool_choice: {type:"function", function:{name}}` or `strict: true` on a tool's
  parameters compiles to a decode constraint (the call's JSON must match the parameters' schema);
  without Part B it is advisory and says so (Decision 3).

Template ids move: `misaka-palw/fp-gateway-template/chat-segments-tools/v1` (and the
think-closed sibling) name the render with a tool block, so a `/health` that advertises a template
id still advertises the executed one, and the golden tests for the plain and ChatML renders are
untouched.

**Decision 3 — `response_format` is served in two enforcement modes, and the answer says which.**
`{"type":"json_object"}` and `{"type":"json_schema","json_schema":{schema}}` are accepted at both
entrances. The mode is the CHAIN's to decide, not the request's:

* **committed** — the network has armed Decision 8's fence: the schema compiles to a decode
  constraint (Decision 7), the job carries its id, the seat replays it, the court can try it.
* **advisory** — every shipped network today: the schema is rendered into the prompt as text
  (the person asked for it; F1 is intact), the run is unconstrained, the answer is validated
  against the schema after the fact, and the `json` kind (Decision 9) is derived when it parses.

Both modes report `misaka.format = { requested, enforcement: "committed" | "advisory",
valid: bool, errors: [...] }`. A request that sets `misaka.require_committed_format: true` on a
network whose fence is dormant is refused by name BEFORE the inference — an integration that
needs the guarantee must never receive a lookalike. `json_object` compiles to the grammar of any
JSON value; `json_schema` to Decision 7's subset, and a schema outside the subset is refused by
name in both modes rather than approximated.

**Decision 4 — sampling at the entrance: refused by the gateway, mapped WITH NOTICE by the
person's own app.** The gateway keeps ADR-0082 Decision 11's refusal exactly: a non-greedy
`temperature` or a non-zero `seed` is a 400 naming the fence, before the inference. The Studio is
the person's own app, and its chat already sends the lane no sampling knobs at all
(`backend/gateway.rs:195-199`). It now SAYS so: every answer that went to the lane carries
`misaka.sampling = { requested: {…}, applied: { temperature: 0, seed: "00…00" }, reason:
"palw_fp_decode_rules is not armed on <network>" }` and the UI shows it under the message. A
setting `node.sampling_policy` chooses `greedy_with_notice` (the default: the request goes
through, the notice is shown) or `refuse` (the app refuses like the gateway). `top_p`, `top_k`,
`min_p` and `repeat_penalty` have no consensus rule and never will under this ADR — a per-lane
key is the only sampler an exact court can carry (ADR-0082 Decision 11) — and are reported as
`not_a_rule_on_this_lane`. The doctrine is honoured: nobody is told a false thing about what ran,
because what ran is printed beside what was asked.

**Decision 5 — a long thread is a chain of jobs, and the chain is reported.** The row does not
move (ADR-0092 Decision 4; a wider class is a mint-time decision this ADR does not take). The
entrance does three things, each visible:

1. **Trim**, as today: oldest turns first, system prompt and newest turn kept, the count of
   dropped turns reported (`misaka.context = { n_ctx, prompt_tokens, dropped_turns }`).
2. **Summarize** when a trim would drop more than `summarize_after_turns` (default 4): the
   dropped turns are sent as their own job — a prompt asking the class for a short summary — and
   the summary rides the next prompt as a system-level "Earlier in this conversation: …" text
   turn. The summary is an inference and a claim like any other (R0). It is never fabricated by
   the app.
3. **Continue** when `finish_reason == "length"` and the request's `max_tokens` exceeded the
   room: a follow-up job whose prompt is the trimmed context plus the answer so far, asking the
   model to continue; at most `continue_max_legs` (default 2) legs. Each leg is a claim.

Every job an entrance drove for one request is listed in `misaka.jobs[] = [{fp_job_id,
fp_claim_id, role: "summary" | "answer" | "continue" | "tool_leg", prompt_tokens,
decode_tokens}]`, and the record store (Decision 12) keeps the same list on the record. A
KV-cache handoff between jobs is deliberately NOT a mechanism here: a checkpoint is court
material (ADR-0082 Decision 4), not a transport, and the seat replays a job from its prompt ids.

### Part B — the shape of the answer (a ruleset move, behind one fence)

**Decision 6 — the answer is cut at the first committed end-of-generation id, by a versioned
rule.** Past Decision 8's fence, `render_answer_v2` renders the ids up to and excluding the first
id in the manifest's `eog_token_ids`; the ids after it stay executed, committed and priced
(ADR-0077: the execution runs to the declared budget and the commitment covers every token) but
are not the answer, and the derivation (ADR-0078) reads the cut rendering. ADR-0078 Decision 6's
premise — "a trimmed answer is one a verifier holding the ids cannot reach" — is true of a display
trim and false of an EOG cut: the manifest declares the ids, so a verifier holding the ids and
the class reaches the same cut. This is what makes a derived JSON the answer rather than the
answer followed by whatever the model said to an imaginary next turn. The shipped rendering does
not move: `render_answer_v1` stays what every dormant network verifies.

**Decision 7 — a job may carry a decode constraint, and the committed token is the admitted
argmax.** The free-prompt job gains one field, `constraint_id: Hash64` (`0` = none), inside
`fp_job_id_v3` by construction and therefore inside the claim id: a constraint cannot be changed
after the fact, and grinding one costs an inference (ADR-0072 kept). `PALW_FP_V3_VERSION`
moves 5 → 6 WITH the fence, so every v5 job stays exactly what it is.

* `constraint_id = H("misaka-palw/constraint/v1" ‖ canonical constraint bytes)`. The bytes are a
  byte-level automaton in one pinned form: the compiled output of `misaka-palw-constraint`'s
  JSON-Schema subset (draft 2020-12: `type` in object/array/string/number/integer/boolean/null,
  `properties`, `required`, `additionalProperties: false`, `items`, `minItems`/`maxItems`,
  `enum`, `const`, `pattern` over a regex subset the crate pins by table, `minLength`/`maxLength`,
  nesting to a pinned depth) or of a GBNF subset. The compiler is a pure function and is
  content-named like a transformer (ADR-0078 Decision 3): `compiler_id` is in the header of the
  bytes. Bytes are bounded (64 KiB) and refused above it.
* The selection rule v3: at position `p` with automaton state `s = A(bytes(prefix ids))`,
  `committed = argmax_j key_v2(j)` over the lanes `j` with `admit(s, bytes(j))` true, where
  `admit` is "feeding the token's bytes from `s` reaches a non-dead state". If no lane is
  admitted, the committed token is the lowest EOG id and the run's stop reason is
  `EndOfGeneration` — which the validators already accept (`decode_tokens_executed < limit`).
  With `constraint_id = 0`, `admit` is identically true and the rule is `decode_token_select_v2`
  byte for byte, which at `T_q = 0` is `base0_decode_token_select_v1` byte for byte
  (ADR-0082 Decision 11's own reduction, kept).
* The refutation keeps its two-disclosure form and gains a state: the pin already carries the
  prefix ids (`PalwTiledDecodePinV1::generated_token_ids`); the court recomputes `s` by running
  the automaton over the rendered prefix — which needs the class's token-to-bytes table
  (Decision 8) — and applies `admit` to the committed lane and the beating lane before comparing
  keys. A third arm is one disclosure: **the committed token at `p` is not admitted at `p`**,
  proven from the ids and the constraint alone, no tile opened. `check_tiled_decode_token_refutation_v3`
  takes the constraint as the v2 form takes the sampler — bound to the claim by the caller, never
  stated by the challenger (a challenger who could state the constraint could state "none" and
  convict an honest masked token).
* The constraint bytes ride the served answer envelope (ADR-0084's `FPA1`, in its next version)
  under the same data-availability obligation as the prompt ids; the commitment carries only the
  id. Privacy is the prompt's: a schema is not the prompt, but it is as public as the prompt's
  ids are on the network the job runs on (PublicDa today; ADR-0077 Decision 16 unchanged), and a
  schema that names what the person is extracting says something about the prompt. The entrance
  says this beside the `PanelDa` sentence it already shows.
* Priced like every other job field. Decode leaves are what they are (ADR-0082 Decision 10);
  a mask does not change a leaf.

**Decision 8 — one fence, refused at assembly until the build carries all three halves.**
`Params::palw_fp_decode_constraint: Option<ForkActivation>`, top level, `None` on every
preset, in `consensus_identity_id` like its siblings. Armed, it selects: job version 6 with
`constraint_id`; selection rule v3 in the transition's replay and in every engine; refutation v3
in the court; `render_answer_v2` (Decision 6); and the class token-to-bytes table as served
material (the table is what the artifact's `tokenizer_commitment` names — v6 classes serve it
beside the artifact, and a seat that does not hold it files `Unavailable`, which is an abstention
and not a verdict, ADR-0065 D4). `validate_palw_v2` refuses a scheduled height on a build that
lacks any of the three halves, in the sentence ADR-0082 Decision 11's refusal uses
(`params.rs:1993-2003`), because a fence that reads as armed while the transition refuses is a
lane that burns inferences. `GetPalwProducerFactsResponse` gains `fp_decode_constraint_armed`
(one wire-version step, fail-closed like `fp_decode_rules_armed`), `ChainFacts` mirrors it, and
Decision 3's mode reads it. Arming does NOT require `palw_fp_decode_rules`: greedy under a
constraint is a complete rule.

**Decision 9 — the `json` kind (ADR-0078 Decision 8, row 8).**

| kind | the DSL | transformer | artifact | determinism basis | not covered |
|---|---|---|---|---|---|
| `json` | the answer's JSON text | `json/canonical/v1`: RFC 8785 (JCS) canonicalization — sorted member names, shortest round-trip number form, UTF-8, no insignificant whitespace — and identity over the result | `.json` | a pure byte function; the artifact IS the canonical bytes | semantic validation; numbers outside JSON's canonical form; comments and trailing commas (refused, not repaired) |

`grammar_id` for the kind is `json/v1`; when the job carried a constraint, the derivation's
`grammar_id` is `H(json/v1 ‖ constraint_id)` so a consumer checks both that the bytes are
canonical JSON and that they were produced under the schema the claim committed (Decision 7). A
parse failure derives nothing and the inference still certifies and mines (ADR-0078 Decision 2).
`xml` is not a row: an XML canonicalization that a 1.5 B model reliably targets in a few hundred
tokens was not measured, and a kind that is not there is not covered.

### Part C — distribution, settings, the door

**Decision 10 — one components manifest, published by both releases, read by every installer.**
`components.json` (schema `misaka/components/v1`):

```text
{ "schema": "misaka/components/v1", "release": "<tag>", "network": "testnet-11",
  "components": [ { "id": "kaspad", "kind": "node", "version": "…",
                    "platform": "aarch64-apple-darwin", "url": "…", "sha256": "…", "size": …,
                    "requires": [] },
                  { "id": "palw-a16-fp-worker", "kind": "worker", … },
                  { "id": "misaka-palw-gateway", "kind": "gateway", … },
                  { "id": "misaka", "kind": "cli", … },
                  { "id": "qwen25-1.5b-a16", "kind": "artifact", "class_id": "…",
                    "artifact_root": "…", "url": "hf://…", "sha256": "…", "size": … },
                  { "id": "qwen25-tokenizer-table", "kind": "tokenizer-table",
                    "tokenizer_commitment": "…", … } ] }
```

* The node repository's deploy workflow writes the manifest for the binaries it builds, per
  platform, beside the release assets. The Studio's release workflow writes its own manifest
  (`misaka-studiod`, the shell, the engines it stages) and NAMES the node manifest it was built
  against by URL and sha256. Two halves, one schema, and the reference is the seam.
* The Studio reads the manifest at `/api/v1/components` — installed version, manifest version,
  sha256 verified or not, where it was found (`beside the executable`, `engines/`, `PATH`,
  `models_dir`) — and installs or updates a component from it through the existing download
  manager (which already verifies sha256 and resumes). `misaka-studiod --check` prints the same
  table. `PalwClassSpec.artifact` becomes the OFFLINE copy of the manifest's artifact rows,
  pinned to the same sha256 by a test.
* One resolution rule, spelled once: `resolve_component(id)` replaces the four spellings in
  §1.3 — configured path, beside the executable, `engines/`, `models_dir` (artifacts), `PATH` —
  and every spawn site says which candidate won, in the `effective` view (Decision 11).
* The cross-repository check that would have caught §1.3's last bullet: the Studio's CI fetches
  the node manifest its own manifest names and asserts that every `engine`, `worker`, `gateway`
  and `node` id the Studio can spawn is a row in it. A binary the node stopped building fails the
  Studio's build, not the person's evening.
* The Studio's local integer engine is **the gateway in `--answer-never-commit` mode over the
  family worker** (ADR-0077 Decision 1: the server is the worker; there is no chat-only server
  and there will not be one). A bond-less identity — the class, the network, an executor key,
  no bond — is accepted by the gateway when and only when `--answer-never-commit` is set, and
  `/health` says `can_submit: false` and why. The Studio's `misaka` backend drives that and
  reports the gateway's `template_id` and `n_ctx` as its descriptor. `misaka-palw-serve` is
  removed from the Studio's settings, with a migration that rewrites `backend.misaka_serve_path`
  into the two paths the gateway needs and says so once.

**Decision 11 — every settings panel shows the effective value, and the rebuild predicate is
a fingerprint.** `GET /api/v1/settings/effective` returns, per subsystem (`backend`, `node`,
`records`, `catalog`, `pool`, `gateway`), `{ configured, effective, source, since }` where
`effective` is read from the RUNNING object — the engine's program path, URL and token
fingerprint, the model it holds and its `n_ctx`; the node's argument list; the record store's
path and enabled flag; the catalog endpoint — and `source` says which file, flag, environment
variable or discovery produced it. The UI's Settings and Network views render `effective` and
mark a field whose `configured` differs. Each backend derives a `RuntimeFingerprint` from
everything it copied at construction; `apply_settings` rebuilds when the fingerprint the new
settings would build differs from the running one, so the predicate cannot forget a field because
it no longer enumerates fields. The `Arc::ptr_eq` test of 2026-09-05 stays and gains the
converse: a change to any constructor input replaces the instance.

**Decision 12 — conversations are the runtime's, and the record remembers the lane.**
`/api/v1/conversations` (list, get, put, delete, `export`, `import`) over a JSON file per
conversation under the data directory (`conversations/<id>.json`, the UI's own `Conversation`
shape with `messages[].mining` kept). The UI persists through the API and keeps `localStorage`
as a cache for the window; on first run after the change the cache is imported once. `import`
accepts the Studio's export, OpenAI's account export (`conversations.json`: the `mapping` tree
walked along `current_node`), and a generic `{title?, messages:[{role, content}]}` list; every
imported message carries `source` and the import reports what it skipped by name (non-text parts,
unknown roles). A conversation may change model — a history made with a GGUF continues on a
class, trimmed by Decision 5 and told so. The inference record (`records.rs`) gains
`misaka.jobs[]`, `misaka.format` and the derived `json` artifact's hash when there is one, so
the everyday pipeline — task → shape → store → use — leaves a record a person can `jq`.

**Decision 13 — a model request has a door, and the door is not chain state.** A GitHub issue
form (`.github/ISSUE_TEMPLATE/model-request.yml`) in the node repository asks the questions §1.5
lists; `docs/model-requests.md` describes the pipeline a request goes through — conversion, a
class id, registration, certification, a line with an owner (ADR-0088), a seeded pair (ADR-0090)
— and who does which step; the Studio's Network tab gets "Request a model", which opens the form
prefilled with what the machine knows (RAM, accelerator, the classes it holds). Requests are
public and the queue is the tracker. A request is not a bond, buys no priority and moves no
consensus object; the day one becomes a line, the line's owner is whoever bonded it (ADR-0088),
not whoever asked.

### Part D — what is deliberately not decided

* The width of the next class row (ADR-0092 Decision 4: mint-time, the operator's).
* The heights at which `palw_fp_decode_constraint` arms on testnet-11 (operator; after §6 step
  4 is measured on devnet).
* The regex subset's exact bounds and the constraint compiler's worst-case size: pinned by
  table when the crate lands, and this ADR is amended with the measurement.
* `xml` as a kind (Decision 9's last sentence).
* Whether Decision 5's summary job should be a canonical-prompt claim rather than a user-prompt
  claim (ADR-0074 Decision 1): it is a user-prompt claim here because the text is the person's.

## 4. What this costs, stated before it is measured

* **Court**: recomputing `s` is one automaton step per byte of the rendered prefix (≤ a few
  thousand bytes at n_ctx 512) plus two `admit` checks of one token each — negligible beside the
  tile openings the v2 refutation already pays. The third arm (not admitted) opens no tile.
* **Producer**: per position, `admit` over the vocabulary — 151,936 lanes × the token's bytes
  (mean ~3) ≈ 4.5 × 10⁵ automaton steps, a few milliseconds on the a16 host against ~1 s/token
  of decode. A state-indexed lane cache (llama.cpp's grammar sampler's trick) brings it under
  1 ms. **Measured 2026-09-10 (§9): about 12 ms per position** in release over a synthetic
  151,936-entry table (685,279 bytes, about 18 ns per byte), about 15 ms through a per-lane
  lookup closure — four times the estimate above, and still about 1 % of a ~1 s decode step.
  The lane cache is the engine's work and is not optional at a larger vocabulary.
* **Data availability**: ≤ 64 KiB per constrained claim in the answer envelope; the tokenizer
  table once per class (≈ 3 MB for the Qwen vocabulary), served like the artifact.
* **Wire**: one `GetPalwProducerFactsResponse` version step; the job's borsh gains 64 bytes (a
  `Hash64`), and only at version 6.
* **Nothing on a shipped network moves**: version 5 jobs, their ids, their claims, the
  fingerprint, the rendering rule.

## 5. Invariants the tests must hold

1. `constraint_id = 0` selects exactly what `decode_token_select_v2` selects on every row, and
   at `T_q = 0` exactly what `base0_decode_token_select_v1` selects (a sweep over random rows,
   ADR-0082's `the_greedy_temperature_is_the_shipped_rule_on_every_row` extended).
2. For every constrained run, every committed token is admitted at its position, and the
   third refutation arm convicts a run whose token at any position is not (a negative control
   that alters one id and is convicted).
3. The two-disclosure refutation under a constraint convicts a producer that committed an
   admitted lane when an admitted lane with a strictly greater key existed, and refuses to
   convict when the beating lane is not admitted (the challenger's lane must be admitted).
4. The automaton's `admit` is a function of `(constraint bytes, prefix ids, class token table)`
   only — a test runs it on two hosts' serializations and pins the state sequence.
5. `render_answer_v2` cuts at the first EOG id and only there; `render_answer_v1` is byte for
   byte what it is today; the derivation under v2 of an answer with a trailing tail equals the
   derivation under v1 of the same answer with `max_tokens` at the answer's exact length (the
   2026-09-04 measurement, now a fixture).
6. A `response_format` request on a dormant network returns `enforcement: "advisory"`, and with
   `require_committed_format: true` is refused before any worker is spawned (a mock worker that
   panics on spawn proves "before").
7. The OpenAI conformance corpus passes on the Studio's `/v1` and on the gateway's, from the
   same files, and the copies in both trees are identical (a test in each tree hashes its copy;
   the ADR pins the hash in §9).
8. A `tool` turn renders as the model's tool-response turn and SA-3's subsequence check still
   holds; a `<tool_call>` block in the committed answer parses to `tool_calls[]` and the
   committed ids are unchanged by parsing (the roots of a run with and without parsing are
   equal — it is the same run).
9. `apply_settings` replaces the engine on any change to any constructor input and keeps it on
   a change to none (a table over every settings field, generated from the struct, not written).
10. The Studio's manifest test fetches (or, offline, reads the pinned copy of) the node manifest
    it names and finds every spawnable component id in it; deleting a row fails the test.
11. The four resolution spellings are gone: `grep` for a second `fn resolve_` over the
    runtime is a test.
12. An import of OpenAI's `conversations.json` fixture yields the conversations along
    `current_node`, with non-text parts skipped by name and counted.

## 6. Order of work

1. **Part A on both trees** (this branch pair): Decision 1's parser and corpus; Decision 2's
   template ids, render and parse; Decision 3's advisory mode with `misaka.format`; Decision 4's
   notice; Decision 5's jobs list, summary and continue legs. No consensus object moves.
2. **Part C**: Decision 10's manifest schema and the node workflow's writer; the Studio's
   `/api/v1/components`, `resolve_component`, the cross-repository check, and the `misaka`
   backend over the gateway; Decision 11's `effective` view and fingerprint; Decision 12's
   conversations API and import; Decision 13's issue form, page and button.
3. **Part B, consensus-core first**: `misaka-palw-constraint` (the compiler, pure, content-named)
   and the automaton in consensus-core; selection rule v3; refutation v3 with the third arm;
   `render_answer_v2`; job v6 behind the fence; `validate_palw_v2`'s refusal; the RPC flag.
   Invariants 1–5 green with negative controls.
4. **Part B, engines**: the a16 and qwen36 backends mask under a constraint; the tokenizer table
   as served material; the devnet drill (`--palw-model-devnet` and a scheduled
   `palw_fp_decode_constraint`) commits a constrained claim, replays it on six nodes, and the
   court convicts the two negative controls. §4's costs measured and written into §9.
5. **testnet-11**: heights are the operator's (Part D); the fence is scheduled by activation.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0077 R0 / Decision 2 / Decision 6 | kept; Decision 2 renders tool turns as text under the same template discipline |
| ADR-0078 Decision 2 (nothing about the answer is changed to make it parse) | kept for the advisory mode and for every dormant network; under Decision 8's fence the CONSTRAINT is part of the job, so the answer was never unconstrained — nothing is changed after the fact |
| ADR-0078 Decision 6 (delivery reads the full committed rendering) | amended past the fence by Decision 6 (EOG cut, versioned); untouched before it |
| ADR-0078 Decisions 8/9 (kind table, open) | row 8 `json` added (Decision 9) |
| ADR-0082 Decision 11 (seeded argmax; refuse by name while dormant) | kept whole; Decision 7 composes with it (mask, then key) and reduces to it at `constraint_id = 0` |
| ADR-0084 / 0086 (served material, its cap) | the constraint bytes and the tokenizer table ride the same obligation; the cap is not raised |
| ADR-0092 Decision 4 (width is a mint-time class) | honoured; Decision 5 lives inside the row |
| ADR-0088 / 0090 (lines, seeds) | Decision 13 routes a request toward them and creates no object |

## 8. Number hygiene

0094 (`feat/adr-0094-accumulating-seed`) and 0095 (`docs/position-is-not-a-share`) are resident
on their branches at the time of writing; 0096 is the next free number in `main`'s index and this
file claims it. A concurrent claimant renumbers the later writer; the index is the only place the
mapping is written down.

## 9. Implementation record

* **2026-09-10** — ADR written against misakas `e65ccf20` and MISAKA-Studio `7096533`. Branch
  `feat/adr-0096-everyday-lane` opened in both repositories. Step 1 of §6 begins on the gateway
  (Decisions 1–5's node half) and on the Studio (Decisions 1, 3, 4, 5, 11, 12); what has landed
  is appended here as it lands.
* **2026-09-10, Decision 8's fence declared** (`feat/adr-0096-partb-fence`, `b62e89c2`):
  `Params::palw_fp_decode_constraint`, `None` on every preset, Some-only in both fingerprints and
  in the schedule id (a shipped network is byte-identical to a build without the field), `never()`
  collapses to absence, and `validate_palw_v2` refuses a scheduled height in the sentence
  ADR-0082 Decision 11's refusal uses — the three halves are absent on this build. The wire says
  which side of it a node is on: `GetPalwProducerFactsResponse` version 7 adds
  `fp_decode_constraint_armed` (borsh suffix, fail-closed on an older writer; gRPC field 29; the
  service answers it at the candidate's score; `misaka palw facts` prints it). Tests: dormant on
  every preset and visible the moment it is not; refused at assembly by name; the borsh and gRPC
  round trips carry it and a version-6 writer reads as dormant (15 + 4 + 1 green). Nothing of
  Decisions 6, 7 or 9 exists yet: no v6 job, no automaton, no v3 refutation, no `render_answer_v2`.
* **2026-09-10, Decision 13's door and Decision 10's node half** (`0de50b0f`):
  `.github/ISSUE_TEMPLATE/model-request.yml`, `docs/model-requests.md` (the eight hands a request
  passes through, each with its ADR), `docs/components-manifest.md` (schema `misaka/components/v1`;
  a row points at the platform archive the release publishes and names the member — a loose
  binary URL would point at nothing), `scripts/release/misaka-components-manifest.py` (write / `--check` /
  `--validate` / `--self-test`, 39 checks) and one deploy step per platform writing
  `components-<platform>.json` beside the archive for the two components the job builds today
  (`kaspad`, `misaka`); the workers, gateway and rail become rows the day the job builds them,
  and the writer refuses any other name rather than guess a kind.
* **2026-09-10, Part A's node half** (`339ed0ad`, wired to the fence in `cc8b47f7`'s merge and
  after): `misaka/palw/gateway/src/surface.rs` (the request read once; `admit_request` before the
  queue and before the worker), `wire.rs` (tools as Qwen's own Hermes-style text under the
  `-tools/v1` template ids; `parse_tool_calls` touching no committed byte), the new crate
  `misaka-palw-constraint` (Decision 7's JSON-Schema subset, RFC 8785 — whose Appendix-B vectors
  found two real defects on the way: Rust's `{:e}` does not round half-even on a shortest tie, and
  serde_json's default float parser is not correctly rounded, so the crate enables
  `float_roundtrip` — and `constraint_id` under `misaka-palw/constraint/v1`), `misaka.format`
  served advisory and said so, `require_committed_format` refused before the inference,
  `GET /v1/models`, and the conformance corpus `docs/openai-surface/v1/` (22 cases; directory
  digest `5cdf928859dccdc7e841b6f4f9b01dc75a1080f3705e515ef1793cf6d08d72b9`, which the Studio's
  mirror pins). `ChainFacts::fp_decode_constraint_armed` reads the node's producer facts (wire
  version 7) and nothing else. Two choices to know: the system line created to hold a tool block
  is "You are a helpful assistant." (Qwen2.5's vendor line names Alibaba; Qwen3's writes nothing),
  and JSON rendered into the prompt is RFC 8785 so the job id is not a function of the client's
  key order. Not done: the `json` derived kind (Decision 9), and a live-worker run on this host.
* **2026-09-10, the gateway admits a bond-less identity exactly when it never commits**
  (`8d3d671c`, `a273d617`): Decision 10's last bullet on the node side — `identity.json` may omit
  the bond and the key under `--answer-never-commit`, `/health` says `bond: null`; refused by name
  otherwise, before any inference.
* **2026-09-10, the Studio (MISAKA-Studio `feat/adr-0096-everyday-lane`)**: `6a7c5d1` + `a3d9975`
  (UI: the sampling notice, the format badge, the context and job lines, tool calls shown as the
  JSON the app must run; conversations through the runtime with `localStorage` as the cache and a
  one-time migration; Export/Import; the Lane settings; "Request a model" — and, measured on the
  way, the desktop window has no opener, filesystem or dialog capability, so a link is shown with
  a Copy button and an export goes to the clipboard), `aaac111` (runtime: Decision 1's surface
  read from one table, tools/tool_choice/response_format forwarded to a child engine and to the
  lane, `node.sampling_policy` with `misaka.sampling`, `require_committed_format` refused before
  anything is sent, the summary job and the continue legs with `misaka.jobs[]` and
  `misaka.context`, the record keeps `misaka`; conversations as one JSON file each under the data
  directory with import from the Studio's export, the window's cache, OpenAI's
  `conversations.json` along `current_node`, or a generic list; the model-request URL),
  `7a5a31f` (README), `bf98e7a` (invariant 7: `testdata/openai-surface/v1` mirrors this tree's
  corpus byte for byte and pins the same digest), `01dfc05` (Decisions 10 and 11: every backend's
  `RuntimeFingerprint` from what its constructor stored, the rebuild predicate over routing
  inputs and fingerprints with a 61-leaf × 5-kind table test, `GET /api/v1/settings/effective`;
  `components.rs` as the one search order with the four spellings reduced to wrappers and pinned
  by a source test, the `misaka/components/v1` reader mirrored rule for rule, `GET
  /api/v1/components`, `POST …/install` through the verifying download manager (archive members
  refused by name for now), `misaka-studiod --check` / `--strict`, and
  `contrib/components/testnet-11.json` as the offline copy pinned to the class table). Tests at
  that point: core 49, runtime 192, integration 10; clippy 0.
* **2026-09-10, Decision 9's `json` kind** (`a582ba20`): grammar `json/v1` (one JSON value under
  RFC 8259 strictly, refused by name and never repaired, emitted as RFC 8785 bytes through the
  constraint crate's canonicalizer — where the RFC's IEEE-754 number rendering lives; the derive
  crate spells no float) and transformer `json/canonical/v1` (identity over canonical bytes).
  Two facts the implementation settled: the kind id is **28**, not 8 — `TEXT` already holds 8 in
  ADR-0078 D9's candidate table and an id is never reused, and the acceptance rule reads
  `kind != 0` only, so the row is consensus-inert; and the transformer-id pin was re-pinned on
  purpose, since every byte of `derive/src` is in every transformer id (ADR-0078 D3) — the ids
  of every shipped kind move with that commit, before any network relies on them. The gateway
  derives the json kind for a request that set `response_format` and named no `derive`. The
  schema-bound grammar id `H(json/v1 ‖ constraint_id)` does not exist yet (Part B).
* **2026-09-10, Decision 7 as pure consensus functions** (`28adec54`; nothing armed):
  `consensus/core/src/palw_decode_constraint_v1.rs` — the automaton form (a pushdown automaton of
  sorted byte-range frames with a 16-bit seen mask, so object keys are any order and at most once;
  64 KiB, 65,535 nodes, depth 16, an epsilon-hop cap), `constraint_id_v1`, `admit`,
  `decode_token_select_v3` (every lane admitted → v2 on every row; `T_q = 0` → v1), and
  `check_tiled_decode_token_refutation_v3` appended to `palw_step_refute.rs` (no constraint →
  the v2 verdict byte for byte; the one-disclosure "not admitted" arm; the two-disclosure arm
  with a forbidden beating lane not a fault). One thing the text above did not say and the court
  needs: **the EOG ids**, beside the token-to-bytes table — without them it cannot try Decision
  7's stop rule, and either convicts an honest stop or leaves the stop position untriable. The
  compiler (`misaka/palw/constraint/src/compile.rs`) admits RFC 8785's canonical form with key
  order free; `pattern` is refused by name, `minimum`/`maximum` stay post-hoc, and a single
  string's `maxLength` is practically about 250 under the 64 KiB cap. §4's producer cost is
  measured and corrected above. Still absent: job version 6, `render_answer_v2`, the engines'
  mask, the served token table, and the assembly refusal's removal.
* **2026-09-10, Decision 10's engine, and the artifact it could not run** (misakas `70d56519`,
  `11b293a9`; MISAKA-Studio `2d9e5e8`, `af9c4da`). Measured before building it: **the a16 worker
  refuses the published dense artifact at boot** (`qwen25-1.5b-a16.palwart`, sha256
  `a8c4e53e…`: "this artifact declares no tokenizer"), so an engine on the family worker could not
  serve the file every Studio downloads. The node tree gains `bind_tokenizer_file_v1` and
  `palw-class bind-tokenizer`, which sets that one field and MEASURES each row's registered root
  before and after: the tiled-map rows (`graph-v2`, `graph-v3`, `graph-v5@512`) keep the inventory
  root `1a7457f1…`, the one-byte-map rows move with the digest. The output's sha256 is
  `3f8fc5066bafae28…` — the file the testnet-11 fleet runs for free prompts, reproduced from the
  public artifact and the public `tokenizer.json`. The gateway admits the identity `{}` under
  `--answer-never-commit` with `--anchor` and adopts its worker's class. The Studio's `misaka`
  engine now supervises that gateway over the family worker, speaks to it through the pool
  path's own request code, empties its answer-only outbox at each load, and — for an unbound
  artifact on a machine that still has the retired `misaka-palw-serve` — falls back to it by name
  rather than taking the chat away. Two defects the smoke run through `misaka-studiod` found and
  fixed: the sampling notice applied only to an engine named `gateway`, and the `misaka` merge
  replaced the Studio's `ignored_fields` with the gateway's empty list. The Studio's Settings show
  the effective values and a Components page holds every binary to the manifest (`af9c4da`).

  **One decision is the operator's and is not taken here: publish the bound artifact.** The
  Studio's class table pins the unbound file's sha256, so every new install downloads a file the
  family worker refuses and either binds it or falls back to a retired binary. Uploading
  `3f8fc5066bafae28…` to `Misakachain/Qwen2.5-1.5B-PALW-A16-runtime` and moving the pin is one
  upload and one line, after which the fallback has no user; it is an outward-facing act, so it
  waits for the operator.

## Mission alignment amendment — 2026-10-07

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md -->

</details>

