# ADR-v2-0039 — DA court・claim資料請求・PanelDa期限付き開示

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

claim固有の資料不提出を客観的に扱い、計算不正との帰責の差を残す。

## 2. 統合した判断の読み方

### 2.1

DA accusationは不足資料を特定し、開示または期限に基づく手続きで解決する。単なる票だけでproducerのbondを取る仕組みにしない。[旧ADR-0062](../adr/0062-data-availability-court.md)

### 2.2

leaf証拠の開示と一手の裁定を同じ認証・計算境界へつなぎ、資料の欠落と正しい値の自己申告を区別する。[旧ADR-0111](../adr/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)

### 2.3

PanelDaは期限までの非公開に縮小し、その後の必要なprompt IDs公開と外部者の追及窓を持つ採択を記録する。未開示defaultと算術有罪の制裁を同一にしない。[旧ADR-0206](../adr/0206-panel-da-mandatory-disclosure-g14-route.md)

## 3. 維持する境界・後続改定

PanelDaの対象はclaim資料であり、モデル全weightsの強制公開を復活させる決定ではない。文書中の目安値を本番測定値として承認しない。

## 4. 隣接する判断との分担

G14のモデル取得前提は[ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md)、新material入口は[ADR-v2-0045](0045-terminal-material-and-readiness.md)、廃止されたモデル配布制度は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0062-data-availability-court.md](../adr/0062-data-availability-court.md) | L1–L632 | L25–L25 |
| [0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md](../adr/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) | L1–L310 | L15–L33 |
| [0206-panel-da-mandatory-disclosure-g14-route.md](../adr/0206-panel-da-mandatory-disclosure-g14-route.md) | L1–L68 | L3–L7 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0062-data-availability-court"></a>

<details>
<summary>0062-data-availability-court.md — 原文 L1–L632</summary>

[原文を開く](../adr/0062-data-availability-court.md) · SHA-256: `4067f7cce556165185743de72ece25d9638f0bc01e44c1e6f70fb9107dc94913`

<!-- ADR_SOURCE_BEGIN:0062-data-availability-court.md -->
~~~~~~~~markdown
# ADR-0062 — The data-availability court: stop a vote from taking a bond

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Model / claim-evidence boundary, 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md) withdraws model-wide FPR/TRDC/Seeder service liability and availability-based reward/weight gates. This historical court concerns named, bounded claim evidence. Future requests must exclude model acquisition/full-weight/iterated-range disclosure and fix permitted units/cumulative scope. Claim-specific default and authenticated computation fraud retain distinct evidence and responsibility. Existing implementations, tests and old fences keep their historical scope; no new allocation or court restriction is activated here.

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


> Renumbered 0059 → 0062 at the 2026-08-30 branch merge: 0059 was taken the same day by the
> 10B premine cap on the parallel line (0060 the liveness doctrine, 0061 zero-seat genesis),
> and this document was the cheaper of the two to move — one file, no code references. The
> original branch commit (`554ca77c`) carries the old number in its message; this file is the
> authority.

Status: **Implemented behind `Params::palw_da_court`, dormant** (the amended form landed 2026-09-02; SA-7 widened the same fence 2026-09-03 — see the implementation sections below). The "Proposed, not landed" reading that stood in this line and in the README until 2026-09-05 was stale. Supersedes nothing; completes ADR-0042 Decision 7.

> **Standing (index reconciliation, 2026-09-02).** Still Proposed, not landed. The harm it was
> written to stop — an `Unavailable` quorum voiding a claim and slashing the producer's bond with no
> proof — is removed on armed presets by [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)
> Decision 4 (`palw_unavailable_abstains`, armed from genesis on testnet-11 since Relaunch 5:
> `ProducerDefaulted` is unlicensable; the claim falls to a redraw and a receipt timeout, which voids
> without slashing). What this ADR still adds is the positive half — a provable default that CAN
> take a bond. The DA fields it lists as "already committed" are pinned by equality since
> [ADR-0072](0072-the-ticket-is-the-execution.md) Decision 8. The carriage ADR cited as ADR-0046 is
> [0046](0046-palw-v2-consensus-object-carriage.md). Map: [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: the accusation becomes a bonded, singular object; the disclosure is hash-checked and bounded; "silence" is a fold fact with a majority-proof window; abstaining seats pay nothing (ADR-0065 D4); the lattice is re-derived.

## The defect

Every other judgement in this lattice is arithmetic. A claim's execution can be disputed by
anyone holding a bond, the dispute bisects to a single step, and the verdict is derived from
operand openings against the class's registered artifact root — a captured panel cannot make a
wrong execution right, because `slash_dissenting_seats` charges a seat for contradicting the
quorum and the *court* charges the quorum for contradicting arithmetic.

**One judgement is decided by a vote: data availability.**

```rust
// palw_state_v2.rs, the ProducerDefaulted arm
let verdicts = palw_seat_verdicts_of_v2(receipts);
builder.slash_dissenting_seats(&claim, &verdicts, false)?;
builder.slash_silent_seats(claim_id, &claim, &verdicts)?;
builder.void_and_slash(*claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
```

`void_and_slash` is terminal and immediate. A quorum that signs `Unavailable` destroys the claim
and takes `claim.reserved` from the producer's bond, and there is no window, no session, and no
evidence anyone must produce. The accusation does not even have to say *what* was withheld — the
object carries receipts and a claim id, nothing more.

The minority case is already handled and handled well: a seat that signs `Valid` against a
defaulting quorum is charged, and one that signs `Unavailable` against a licensing quorum is
charged. **The hole is the majority.** Take `k` of `n` seats — 3 of 5 on the shipped panel — and
every producer on the network is slashable at will, for the price of holding three bonds.

This is also why the bond floor cannot fix it. The 2026-08-30 economics pass priced quorum
capture and found that making it cost real money requires per-seat collateral that prices out
every honest participant (~17.7M BILI/seat to make one capture cost $1,000 at the traded rate).
A cheap seat is only safe if capturing the panel *buys* little, and today it buys a slash.

## What the chain already has

Three facts make the remedy cheap, and all three are already committed:

1. **`claim.trace_root`** — the Merkle root over the ordered trace events, bound to the job
   context by `full_logits_trace_root_v2`. Fixed at claim creation; the producer cannot restate it.
2. **`trace_event_opening_v2` / `trace_event_opening_root_v2`** — a complete membership proof and
   verifier, index-bound at the leaf, odd nodes promoted unchanged (no CVE-2012-2459 ambiguity),
   with the event count taken from the commitment rather than the opening.
3. **`claim.trace_chunk_count`** and **`claim.trace_retention_daa`** — how many pieces the
   obligation covers and how long it lasts.

So "the producer can still show the material" is already a checkable arithmetic statement. Nothing
new needs to be committed; what is missing is a transition that asks the question.

## Decision

`ProducerDefaulted` stops being terminal. It opens a **data-availability session** that resolves
the way the arithmetic court does — by a proof, or by a deadline nobody met.

### D1. An accusation must name what is missing

`ProducerDefaulted` gains `missing_event_index: u32`, validated `< claim.trace_chunk_count`.

An accusation that names nothing cannot be refuted, and one that names everything is not an
accusation. This is the same move ADR-0042 made for court closes: a verdict that carries no proof
is an assertion, and the acceptance layer refuses it.

### D2. Accepting the accusation opens a session, it does not void

The claim moves to a new phase `DefaultDisputed { opened_daa, missing_event_index }` and arms a
deadline at `opened_daa + window_challenge`. The reservation is held (the claim is still live) and
the escrow is neither paid nor destroyed.

`slash_dissenting_seats` still runs at this point — a seat that signed `Valid` against the
accusing quorum has contradicted its own panel, which is true whatever the session decides.
`void_and_slash` does **not**.

### D3. The producer answers by publishing the event

A new object:

```rust
MaterialDisclosed {
    claim: Hash64,
    opening: PalwTraceEventOpeningV2,
    /// What `logits_event_hash_v2` hashes: (phase, phase_step, event_index, n_vocab, logits).
    /// For a class on `tiled_logits_scheme_id_v1` this is the TILED form — see the costs below,
    /// where the flat one is measured and does not fit.
    event: PalwDisclosedEventV1,
    signature: Vec<u8>,           // under the claim's bond key
}
```

Accepted only while the claim is `DefaultDisputed`, and only when all three hold:

* `opening.event_index == the session's missing_event_index` — answering a different index is not
  an answer;
* the event re-hashes to `opening.event_hash` under `logits_event_hash_v2` — the preimage is the
  one the opening names, so a prover cannot open a hash it never has to produce;
* `trace_event_opening_root_v2(claim.trace_chunk_count, &opening) == claim.trace_root` — the event
  is the one this claim committed to, at that index.

The signature is checked for the same reason `CourtDisclosed`'s is: an unsigned disclosure would
let a third party bind the producer to material it never published.

### D4. A verified disclosure refutes the accusation

The claim returns to `ReceiptLicensed { licensed_daa: <the disclosure's DAA> }` and resumes its
path to `Final`. Every seat whose receipt said `Unavailable` is charged `claim.reserved`, capped at
`min_collateral_sompi` exactly as `slash_seat` already caps a dissent — they signed that this data
could not be obtained, and it is now on the chain.

**This is the whole point.** The accusing quorum is not charged for losing a vote; it is charged
for signing a statement the chain can now see is false. That is the same standard the arithmetic
court holds an executor to.

### D5. Silence past the window confirms the default

The sweep finds a `DefaultDisputed` claim whose deadline passed with no accepted disclosure and
does exactly what today's arm does immediately: `void_and_slash(ProducerWithholding)`. A producer
that cannot open one committed event of its own trace has not served the material, and the panel
was right.

## Costs, measured

| | |
|---|---|
| opening, worst case | `4 + 64 + 13x64` = **900 bytes** (`PALW_V2_MAX_TRACE_EVENTS` = 4096, so at most 13 siblings) |
| flat event preimage | `n_vocab x 4` bytes — `logits_event_hash_v2` hashes the whole logits vector |
| ...at Qwen's vocab (151,936) | **607,744 bytes** |
| court close ceiling | `DEFAULT_MAX_CLOSE_BYTES` = **80 KiB**, frozen for testnet-11 |
| added lattice length | `window_challenge`, once per disputed claim |

The opening fits with three orders of magnitude to spare. **The preimage does not, and that is a
measured fact rather than a caveat**: a flat disclosure at Qwen's vocabulary is 7.4x the entire
close budget, so for any class on `tiled_logits_scheme_id_v1` the tiled form is not an optimisation
but the only representable answer — the same conclusion ADR-0046 reached for court closes, where a
flat pin was found to be inadmissible at *any* (tile, context) and the two-tile disclosure is what
made those classes prosecutable at all.

This ADR therefore does not introduce a second disclosure format. `MaterialDisclosed` carries
whatever form the class's `logits_scheme_id` names, bounded by the class's own registered
`max_close_bytes` exactly as a close is, and `verify_class_admission_v2` must refuse a class whose
event cannot be disclosed within that ceiling. A class whose data obligation cannot be adjudicated
is unprosecutable in the DA dimension for the same reason ADR-0049 Decision C refuses one that is
unprosecutable in the arithmetic dimension — and the check belongs in the same place, so a class
cannot pass one gate and fail the other silently.

The lattice bound in `Params::validate` becomes
`2 x (bind + receipt) + challenge + court + challenge`, the last term being this session. It is
already `2 x (bind + receipt) + challenge + court` after the 2026-08-30 redraw.

## What this does not fix

**A producer that publishes late still profits from having withheld.** D4 restores the claim in
full, so withholding until accused, then disclosing, costs the producer only the disclosure. The
panel's wasted verification is not compensated — this ADR deliberately does not invent a payment
rule, for the reason `slash_silent_seats` gives: seats are never paid anywhere in this lattice, and
fixing that is one decision, made once, not a side effect of a remediation. Until it is made, the
honest reading is that D4 refutes an *accusation* and not a *delay*.

**One disclosure refutes one index.** A panel that genuinely could not obtain most of the trace
must accuse at one index and can be refuted at that index. Sampling — accuse at `m` indices, refute
all `m` — is the natural extension and is left out on purpose: `m = 1` is the shape whose costs are
measured above, and a sampling rule needs its own analysis of what `m` buys against `max_close_bytes`.

## Why not the alternatives

**Charge the accusing quorum on any refuted default, without a disclosure.** There is nothing to
refute with — that is the defect.

**Use `PalwMaterialRequest` (protocol 104) re-serving as the evidence.** A seat re-serving material
it heard is hearsay: it proves some node has bytes, not that those bytes are this claim's, and it
puts the verdict back on testimony. The opening puts it on the commitment.

**Raise the panel bond until capture is unaffordable.** Priced at the traded rate this costs the
network its independent seats and buys, per the 2026-08-30 pass, a capture cost still under a
thousand dollars. Cheap seats plus arithmetic beats expensive seats plus votes.

## Consensus impact

`PALW_STATE_V2_VERSION` 13 → 14. New claim phase, new consensus object, and `ProducerDefaulted`
gains a field — the claim schema and the object schema both move, so an old build cannot decode
the state rather than merely disagreeing about it. Deploying is a re-mint, and the fingerprint
moves for testnet-11 (mainnet carries no PALW bundle).

## Security amendment (2026-09-02) — hardening before implementation

Written against the shipped tree of Relaunch 5e, where ADR-0065 Decision 4 is armed from genesis on
testnet-11 (`Unavailable` abstains; the `ProducerDefaulted` quorum is unreachable) and ADR-0072
Decision 8 pins `trace_manifest_root`, `trace_chunk_count` and `trace_retention_daa` by equality.
Six things the Decision above must gain before it is coded, each with the attack it closes.

**SA-1 — The accusation is its own bonded object, singular per claim, inside the retention
window.** Decision 1 rides on `ProducerDefaulted { missing_event_index }`, a quorum verdict that a
D4-armed preset never reaches. Replace it with `DefaultAccused { claim, missing_event_index,
accuser: bond, signature }`: signed by an Active bond that is a seat of that claim's panel or a
bonded challenger; refused if `missing_event_index ≥ trace_chunk_count`, if the claim is outside
`[accepted_daa, trace_retention_daa]`, or if an accusation is already open on the claim. The
accuser's bond reserves `ACCUSATION_EXPOSURE = claim.reserved / seat_count` on the ledger the
claim's own exposure lives on (ADR-0056 Decision 3's shape); a valid disclosure charges it to the
accuser and refunds the producer's disclosure fee out of it. Griefing an honest producer then costs
the griefer more than the producer per attempt, and ten accusations cannot drain one fee float.

**SA-2 — The disclosure is checked by hash arithmetic, never by execution, and it is bounded.**
`MaterialDisclosed` carries the event preimage at `missing_event_index`, its Merkle opening against
the claim's pinned `trace_manifest_root`, and the producer's bond signature. Every validator checks
the opening and the index; none runs a model. Its size is bounded by `max_close_bytes` (80 KiB,
the tiled form), and `derive_court_cost_v1` must include the DA event's worst opening in the
class's priced close, so a class whose event cannot be disclosed inside the budget is refused at
admission (ADR-0049 Decision C) rather than being undefendable afterwards.

**SA-3 — "Silence" is a fold fact on this chain, and the window is wide enough that suppressing
the disclosure needs a majority.** Decision 5 confirms default when no disclosure lands in the
window. That is checkable — unlike ADR-0064 Fact A's "the network was silent" — only because it is
*absence of an object on this chain within `W_disclose` DAA of the accusation's acceptance*,
recomputed from the fold on every branch. Two rules make it safe: the disclosure may ride ANY
block (permissionless carriage, fee-priced, any producer), so excluding it needs every producer for
the whole window; and `W_disclose ≥ 2 ×` the finality window, so a reorg across the deadline cannot
flip the verdict without a finality violation. No node-local timer participates.

**SA-4 — Abstaining seats pay nothing.** Decisions 2 and 4 charge "dissenting" and `Unavailable`
seats. Under ADR-0065 Decision 4 an `Unavailable` vote is an abstention, not a finding, and
charging it re-creates the transport-loss slashing D4 removed (a third of remote seats' verdicts
were transport). Only the accuser — who made a positive, falsifiable claim — pays on refutation.

**SA-5 — Poverty is not default, and the producer's loss is bounded.** A producer that cannot fund
the disclosure's carriage fee draws it from the claim's escrow (the escrow exists to pay the claim's
obligations); the fee is mass-priced like every lifecycle object. On confirmed default the slash
stays `claim.reserved`, never the bond.

**SA-6 — The lattice is re-derived with the new phase.** `DefaultDisputed` adds `W_accuse +
W_disclose` to a claim's life: `2·(bind + receipt) + challenge + court + accuse + disclose ≤
pruning_depth` and `≤ MAX_CLAIM_EXPOSURE_DAA` must hold at every preset, pinned by the existing
lattice test. A phase that outlives the trace's retention is a phase in which the honest producer
was already allowed to delete what it is asked to open.

Invariants: **DA-1** an accusation outside the claim's retention window is refused; **DA-2** a
second open accusation on one claim is refused; **DA-3** a valid disclosure charges the accuser and
nobody else; **DA-4** a disclosure carried by a block the producer did not mine is accepted;
**DA-5** two nodes that saw the disclosure at different wall-clock times reach the same verdict.

## Second security amendment (2026-09-03) — SA-7: what an accusation costs, what it buys, and what it may not suspend

An adversarial read of the landed SA-1…SA-6 code found the mechanism exploitable in **both**
directions at once, which SA-1…SA-6 do not address because they are about the accusation's *form*
and this is about its *price*. All four are one question, so they are one decision.

**SA-7(a) — A data-availability accusation does not suspend the arithmetic fraud court.** DA and
computation are orthogonal: a producer can serve every byte it committed to and still have computed
the wrong answer. `validate_court_opened_v2` read `claim.phase` directly, so an open
`DefaultDisputed` was not `ReceiptLicensed` and therefore not challengeable — for the *whole
remaining challenge window*, since `W_disclose = window_challenge`. A producer whose arithmetic is
fraudulent therefore had a purchase available to it: have any bond accuse it (its own second bond
will do — nothing in the ruleset makes two bonds strangers), answer the single named index honestly
out of a complete trace, and let the challenge window lapse underneath the session. Price
`min(⌈reserved/seat_count⌉, min_collateral_sompi)`; avoided, a `CourtFraud` conviction worth
`claim.reserved` *and* the escrow, with the fraudulent `pwu` entering `safe_weight` permanently.
Strictly profitable whenever `seat_count ≥ 2`.

So the challenge surface is read **through** the disputed phase, at the phase the accusation found
(`resumed`), and the part of the window the open session has already consumed is added back. A
claim that is not `Final` is challengeable; a claim under accusation cannot be `Final`; therefore a
claim under accusation is challengeable. A court that opens on a disputed claim leaves the
accusation's own deadline armed — disarming it would make opening a court the way to cancel an
accusation.

**SA-7(b) — The claim's clock is paused, which means it is given back.** `W_disclose =
window_challenge` is strictly longer than both `window_receipt` and `window_bind` on every
ConsensusV2 preset (shipped reference bundle: bind 600, receipt 600, challenge 1200). A claim
accused while `PanelBound` and restored to the deadline it held *before* the session therefore came
back to a receipt window that had already lapsed — guaranteed by the inequality, not by timing — and
the next sweep redrew it. The redraw leaves the old panel record in place, so a second accusation
was admissible immediately, and the second honest answer came back to an expired *bind* deadline:
`Voided { BindTimeout }`. Two accusations at the registry floor, and an honest producer that
answered both correctly and on time lost its escrow. A disclosure responder does not help, because
*answering* is what triggers it.

On refutation the resumed phase's anchor therefore advances by exactly the elapsed session —
`bound_daa` for `PanelBound`, `rebound_daa` for `Provisional` (never `accepted_daa`, which anchors
the retention obligation and must not grow because someone accused). `ReceiptLicensed` resumes
**unmoved**: that phase names no obligation the session prevented the producer from meeting — it has
finished and is only waiting — so extending it would only delay an honest `Final` further. The rule
is "return the time the session took *from* the producer", not "extend every window".

**SA-7(c) — One bond's total data-availability liability is its exposure ceiling.** Being `Active`
and at or above the registry floor is a check on the accuser's *state*, and reserving does not
change that state: `write_exposure` moves the exposure ledger and leaves `collateral` alone. So a
single bond holding exactly `min_collateral_sompi` passed the same two checks once per live claim on
the network and froze all *K* of them — in one block if it liked, since each accusation is on a
distinct claim and the per-claim singularity never fires. And because `slash_bond` debits
`min(amount, collateral)` and returns early at zero, the *first* refutation emptied the bond and the
remaining `K−1` refutations cost nothing: **K frozen claims for one registry floor.** The exposure
ceiling the `CourtOpened` arm says "does the counting" is enforced only in
`check_palw_attempt_admission_v2`, i.e. only against a bond that wants to *produce*.

The accusation therefore meets the same ceiling — `reserved_exposure + registration_exposure +
this accusation ≤ collateral × max_exposure_ratio_permille / 1000` — and, with SA-7(d), the figure
counted is one the fold can actually collect, so the K-th accusation is funded exactly like the
first.

**SA-7(c) applies to *both* arms that accuse, and the first pass fixed only one of them.** The
paragraph above quotes the `CourtOpened` arm's claim that "the ceiling it already lives under does
the counting" in order to refute it — and then left that arm exactly as it was. The two arms are one
mechanism: a bonded party freezes somebody else's claim by *reserving* against its own collateral,
and reserving does not touch collateral, so `Active` plus the registry floor is a pair of checks the
same bond passes once per live claim on the network. Opening a court is in fact the *cheaper* of the
two, because the challenger-side close charges `min(claim.reserved, min_collateral_sompi)` and
`slash_bond` clamps at collateral and returns `Ok` early at a zero debit: one bond at the floor could
open a court on every licensed claim in a single block, freeze each of them until its `window_court`
backstop, and pay for at most the first close. Both arms now reserve through one helper
(`reserve_accuser_exposure_v2`) against one ledger and one ceiling, so a bond's total liability for
*accusing* — by either object — is its exposure ceiling. Spelling it once is the fix; two spellings
is how the arms drifted apart in the first place, and one test walks both.

Widening the *same* fence, `palw_da_court`, is what makes this a rule change a network opts into
rather than a silent one: with the fence dormant the `CourtOpened` arm reserves exactly what it
always reserved. The consequence, stated rather than left for a reader to find: **a `ConsensusV2`
network that does not arm `palw_da_court` still has the unbounded court-opening gap**, and closing
it there is a fence decision somebody has to make knowingly.

**SA-7(c) also needs `max_exposure_ratio_permille ≤ 1000`, and now says so in code.** The ceiling is
`collateral × ratio / 1000` while `slash_bond` can never debit more than `collateral` in total. Above
unity a bond holds more concurrent exposure than it can ever pay: the first refutations empty it and
the rest are free — precisely the behaviour this clause removes, restored by a genesis-time constant.
The bound was prose in the fold's margin; `PalwAdmissionParamsV2::new` refused only zero, and
`palw_mode_v2` required the admission and state copies to be *equal*, not bounded. Both constructors
refuse above 1000 now, and `validate_ruleset_shape` refuses it again for a bundle that arrives
deserialized rather than constructed — which is how a ruleset actually reaches a node.

**SA-7(d) — An accusation reserves exactly what the fold can take from it.** `slash_seat` caps the
charge at `min_collateral_sompi`, for the reason SA-4 gives: both factors of `claim.reserved` are
chosen by whoever registered the class, so an uncapped charge would let a registrant make accusing
its own class ruinous and buy itself immunity. The *reservation*, however, was the uncapped
fraction, so for every claim with `reserved / seat_count > min_collateral_sompi` the ledger recorded
a liability that could never be collected and SA-7(c)'s ceiling would have counted money that does
not exist. Reservation and charge are now the same number.

**Not decided here, and the exposure until it is — restated, because the first statement of it
rested on a false fact.** A *correct* accuser is still paid nothing. The reason given here was that
"the only payment path is the coinbase payout queue keyed by claim id, which a voided claim never
enters, and a bond has collateral but no payee script", citing a comment in
`virtual_processor/processor.rs` that said exactly that. **The comment was stale and the second half
of that sentence is wrong**: `PalwBondStateV2` carries `payout_payload`, registration refuses an
empty one, `finalize_claim` writes `PalwPayoutV2 { payload: bond.payout_payload, amount }`, and
nothing on the paying side — neither `pending_payouts_iter` nor the drain — asks which phase enqueued
an entry. The queue is generic; there *is* a payee to resolve, for any bond. (The comment has been
corrected in the same commit as this paragraph.)

So the residual is narrower than it was written. What is missing is not a payment mechanism but a
*supply decision* and the transition-side write that spends it: at a confirmed DA default the claim
is voided and its escrow — carved from the accepted block's own subsidy — is simply never paid, so
funding an accuser out of that escrow moves no new coins and does not touch `slashed`, which
`palw_bond_burn_obligation_v2` holds as a burn obligation. That is one `write_payout` at the void,
keyed by the claim id the void frees, and a rule about how much of the escrow it may take. It is an
ADR-sized decision — who is paid, how much, and what stops a producer collecting it through a second
bond of its own — but "the ruleset has no payment path" was not the reason, and this ADR should not
be read as claiming that any longer.

What is *unchanged* is the incentive statement, and it is the part that carries the deferral:
crediting an accuser's `collateral` still would not move coins (it would raise that bond's exposure
ceiling against nothing), no payout to an accuser exists today, and therefore policing data
availability costs transaction fees
plus a window's use of collateral and returns zero, and the expected value of accusing is negative
for an honest participant. **SA-7 makes the accusation safe to leave armed; it does not make it
attractive to use.** Paying an accuser is a new payment *rule* — the one decision this ADR says is
made deliberately and not as a side effect — and it needs its own ADR alongside SA-5's escrow-funded
carriage fee, which is unimplemented for the same reason; what it does **not** need is new
machinery. Until then the DA
court is a deterrent that depends on someone accusing for a non-monetary reason (a panel seat whose
own material never arrived, or a competitor), and the accuse-window arithmetic bounds what it can
ever be: a claim can absorb at most `retention / W_disclose` sessions (shipped: `(600 + 600 + 1200 +
3000) / 1200 = 4`), each opening one accuser-chosen index, so a producer that deleted a fraction *f*
of its trace is caught with probability about `4f`. It is a bounded spot check, not an availability
guarantee, and this ADR should not be read as claiming otherwise.

Invariants added: **DA-6** an open accusation leaves the claim challengeable at every DAA of the
session; **DA-7** a claim that answers every accusation correctly and on time reaches its own
terminal phase unslashed; **DA-8** a bond cannot hold more exposure than its ceiling for *accusing*,
counting a `DefaultAccused` and a `CourtOpened` against the same ledger and the same figure, and the
ceiling is at most the collateral because the ratio is at most unity;
**DA-9** the reservation an accusation takes equals the charge a refutation collects.

## Implementation, 2026-09-02 — the amended form, behind `palw_da_court`

Landed dormant. The fence is a top-level `Params` field, `None` on every shipped preset, so every
fingerprint, identity and state root is byte-identical to the tree before it; `PALW_STATE_V2_VERSION`
stays at 16 for the same reason, and moving it is the *arming* release's job, not this one.

What the code does, against each amendment clause:

* **SA-1** — `PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index, accuser,
  signature }`, riding an ordinary lifecycle transaction. Refused if the index is at or past
  `trace_chunk_count`, if the whole disclose window does not fit inside the claim's own
  `trace_retention_daa`, if the claim is terminal or has no bound panel, if the accuser is the
  producer, is not Active, or is under the registry floor — and if an accusation is already open,
  which is structural: a claim has one phase. The accuser reserves
  `⌈claim.reserved / seat_count⌉` on the claim's own exposure ledger.
* **SA-2** — `MaterialDisclosed { claim, event_index, preimage, opening, signature }`, checked by
  hash arithmetic only: the preimage re-keys to the opening's event hash, and the opening
  reconstructs the claim's pinned `trace_root` at `trace_chunk_count` leaves. Bounded by the ride
  list at `DEFAULT_MAX_CLOSE_BYTES` and at acceptance by `PalwCourtParamsV2::max_close_bytes` —
  the RULESET's one bundle-wide court ceiling, fixed by `palw_ruleset_id_v2`. There is no per-class
  disclosure ceiling on this path and none is read; an earlier draft of this paragraph said "the
  class's own `max_close_bytes`", which is a field this code never consults.
* **SA-3** — the phase `DefaultDisputed { accused_daa, … }`, whose deadline is
  `accused_daa + W_disclose` with `W_disclose = window_challenge`, re-derived from the record by
  `assert_deadline_consistency` and `rebuild_deadline_index_v2` on every branch. `validate_palw_v2`
  proves `W_disclose ≥ 2 × finality_depth` past the fence. Carriage is permissionless.
* **SA-4** — only the accuser is charged, capped at `min_collateral_sompi`. No seat is touched.
* **SA-5** — the confirmed default slashes `claim.reserved`, never the bond. **The escrow-funded
  carriage fee is NOT implemented**: this ruleset has no credit primitive (slashed value is burned,
  and the only payment path is the coinbase payout queue keyed by claim id), so a refund would be a
  new payment rule — the one decision this ADR says is made once and not as a side effect. What
  stands in for it is SA-3's permissionless carriage: a producer with no fee can have anyone carry
  its signed disclosure.
* **SA-6** — the DA term enters both lattice bounds (`with_palw_v2_depths` and `validate_palw_v2`)
  and is zero while dormant. Per claim the session is *contained* in the retention obligation, so it
  adds nothing to `MAX_CLAIM_EXPOSURE_DAA`; the existing lattice walk in `palw_fp_devnet_v3` now
  asserts that containment rather than assuming it.

`DefaultDisputed` is the first phase in this ruleset that holds a **second bond's** reservation,
and that is the whole of its accounting risk. A claim under accusation is not terminal, so every arm
that voids a live claim can land on one — the panel's own `ProducerDefaulted`, and a `CourtClosed
{ ExecutorGuilty }` on a session that was already open when the accusation arrived — and none of
them knows what an accusation cost: `release_for_claim` gives back what the *producer* put up. The
release therefore lives in **one place, `write_claim`**, beside the unresolved and work-id indices
and for the reason `write_court` states about the challenger's stake: a release each new arm has to
remember is a release one of them eventually will not. Left stranded it is not merely wrong
accounting — `assert_internal_consistency` rebuilds that ledger from the claims, so it would be
state the fold writes and cannot read back, and the next `load_tip` would refuse the snapshot the
chain had just written, on every node at once.

The same reasoning polices `resumed`, the one field of this state nothing else re-derives. The
accusation arm refuses a terminal claim and refuses an already-disputed one, so the phase it
snapshots is always live and undisputed; the loader asserts that rather than assuming it, because
the alternative is meeting a bad record at a disclosure a window later with nothing left to say
where it came from.

Deliberately not restored from Decision 4: the refuted claim resumes **the phase it held**, not a
fresh `ReceiptLicensed` dated at the disclosure. Restarting the challenge clock would punish a
producer the chain has just proven honest and would let serial accusations hold a claim — and its
reservation — open a window at a time. Under SA-7(b) that phase's *anchor* advances by exactly the
elapsed session, which is a pause and not a restart: the producer gets back the window it had left
and not one DAA more.

### SA-7, 2026-09-03 — the same fence, widened

No new fence and no new state: `palw_da_court` already gates every path SA-7 touches, and
`DefaultDisputed` is unconstructible while it is dormant, so the challenge-surface lookthrough, the
pause credit and the resume rebase are all unreachable on every shipped preset. `PALW_STATE_V2_VERSION`
still stays at 16, and still must move in the release that arms the fence.

* SA-7(a) — `palw_challenge_surface_phase_v2` and `palw_da_paused_daa_v2` (both the identity off the
  fence) in `validate_court_opened_v2`; the `CourtOpened` fold arm's `disarm_deadline` deliberately
  excludes `DefaultDisputed`. Test `an_open_accusation_does_not_close_the_arithmetic_court`.
* SA-7(b) — `resume_claim_after_da_session_v2`. Test
  `two_answered_accusations_do_not_destroy_an_honest_producers_claim`, which walks the exact
  two-session sequence that voided an honest claim before it.
* SA-7(c) — `TransitionBuilder::reserve_accuser_exposure_v2` is the ceiling, and **both** arms that
  accuse call it: the `DefaultAccused` arm always, the `CourtOpened` arm behind `builder.da_court`
  (the same fence, widened — dormant, that arm reserves exactly what it always did). It refuses with
  `AccusationExposureCeiling`, whose `edge` names which object was refused. Tests
  `one_bond_at_the_floor_can_freeze_exactly_one_claim` and — one test walking both arms, so they
  cannot drift apart again — `one_exposure_ceiling_binds_both_arms_that_accuse`, which spends the
  ceiling through the court, tops it up through the accusation, watches both arms refuse the next
  object against the same figure, and then refutes both and asserts every debit landed.
* SA-7(c), the ratio — `PalwAdmissionParamsV2::new` and `PalwConsensusParamsV2::validate_ruleset_shape`
  refuse `max_exposure_ratio_permille > 1000`. Tests
  `an_exposure_ratio_above_unity_is_refused_where_the_value_is_admitted` and
  `an_exposure_ratio_above_unity_does_not_boot` (which smuggles the value past the constructor by
  deserializing it, because that is how a bundle reaches a node).
* SA-7(d) — `palw_da_accusation_exposure_v2`. Test
  `an_accusation_reserves_exactly_what_the_fold_can_take`.

**Not shipped, and the reason the fence must stay dormant — this is a hard precondition, not a
scheduling note.** Nothing in this tree constructs a `MaterialDisclosed`. `palw_panel.rs` answers
`CourtDisclosed` for the arithmetic ladder and **nothing answers `DefaultAccused`**: there is no
producer-side responder that opens the accused index out of the capture the producer already holds.
Armed without one, every accusation wins on silence, and SA-7's ceiling does not bound that case —
a *successful* accuser is never charged (the DA sweep voids and slashes the producer while
`write_claim` releases the accuser's reservation intact), so its collateral is immediately free to
accuse the next claim. The ceiling bounds *concurrent* accusations; sequential winning accusations
are free and unbounded. One floor bond would therefore destroy every live claim on a responder-less
network, in sequence, at the price of postage.

**`palw_da_court` may not be scheduled from this ADR until that responder exists and ships.** Whoever
sets the fence must be able to name the binary that answers an accusation. What building it takes,
concretely: the responder belongs in the node's panel service beside the existing
`CourtDisclosed` path (`kaspad/src/palw_panel.rs`, which already builds and submits that object from
a duty it polls); the material is the producer's own trace capture, the same
one `trace_event_merkle_root_v2` was computed over at attempt time, so answering is opening event
`missing_event_index` — preimage plus Merkle path — and nothing needs to be recomputed or re-executed;
the object reaches the chain as an ordinary lifecycle transaction, exactly like the `CourtDisclosed`
the same service already submits. The two open questions are retention on the producer side (the
capture must still be on disk for the whole `trace_retention_daa`, which is a node-operations
property nothing currently verifies) and the fee float the responder spends to answer. Neither is in
this batch, and `PALW_STATE_V2_VERSION` must move in the same release that arms the fence.

## Implementation, 2026-09-06 — the disclosure the shipped commitments allow, and a responder (`01e034c4`, `f7363db9`)

The 2026-09-02 implementation verified a disclosed event against a Merkle root over event hashes.
No shipped class commits such a root — every claim's `trace_root` is the flat
`base0_logits_trace_root_v1` or the tiled `tiled_logits_trace_root_v1` — so, armed, every
accusation would have won by silence: the precondition this ADR names ("a responder") had a second,
unnamed half, the arithmetic. SA-2's own words are what is implemented now: the disclosure is
"the event's own bytes in whatever form the class's `logits_scheme_id` names" —
`PalwTraceEventDisclosureV1::{Flat, Tiled, OutOfRange}`, each pinned to the claim's `trace_root` and
`execution_root` through the binding, bounded by the ruleset's close ceiling, signed over a keyed
digest. The event index is `(row << 8) | tile`, bounded by `trace_chunk_count × 256`.

The responder is the panel's `palw_da_duties_v2` → `PalwBackend::disclose_trace_event` (the floor
and both model tiers), filed as a lifecycle carrier inside the disclose window. The accuser is an
operator: `misaka palw da-accuse` builds and signs `DefaultAccused`; `palw submit-object` files it.
No node accuses on its own — ADR-0065 D4 already answers "I never received it".

Armed on a carded mainnet from block one (`mainnet_card_base_v1`), with the withdrawal delay derived
past `liability + accuse + disclose` (`palw_v2_bond_outlasting_da_court`; `validate_palw_v2` refuses
a delay inside that sum wherever the court is armed). testnet-11 and devnet stay dormant; arming
them requires the state-version move this ADR asks for, and a test now says so. Design record:
`docs/palw-private-prompts-design-2026-09-05.md`.

## Arming on testnet-11, 2026-09-06 — a scheduled fence, and what the state version really means

Scheduled at **DAA 1,900** on testnet-11 (`PALW_RC_DA_COURT_FENCE_DAA`), together with ADR-0077
Decision 16's `palw_panel_da` and ADR-0087/0088/0089's three model fences. The chain is live and
carries value, so this is ADR-0083's path (a) — a height — and not the re-mint this ADR's
Consensus-impact section assumed.

**The state version does not move, and the sentence that said it must was written for a re-mint.**
`PALW_STATE_V2_VERSION` is hashed into `state_root`, and every header commits `palw_state_root`, so
moving it invalidates the chain from block one: on a live network the bump IS the re-mint rather
than a step towards one. What the bump was for — an old build must not fold under new rules and
call the result the same state — is bought here by the fence itself: below 1,900 the fold is
byte-identical (`the_da_court_fence_off_is_byte_identical`), and past it an un-upgraded node forks
VISIBLY, because `palw_da_court` now arms the fork-id gate (`fork_id_gate_fences_v1`), which
refuses a peer whose schedule is not this node's. The rule as it now stands, pinned by
`a_scheduled_da_court_needs_no_state_version_move_and_a_genesis_one_would`: a court armed at
genesis on a preset that carries history needs the version move (it is a re-mint); a scheduled one
does not, and must arm the gate instead.

**Two values ride with the fence, and neither is in the ruleset.**

* *The withdrawal delay.* A defaulted claim adds `accuse + disclose` to the path from acceptance to
  a verdict, so past the fence a retiring bond stays locked for the bundle's delay plus that
  lattice (`palw_v2_bond_withdrawal_delay_at_v1`, resolved at each block by the fold). Rewriting the
  bundle's own `withdrawal_delay` would have moved `palw_ruleset_id_v2` — every fingerprint and
  identity with it — and a live fleet cannot upgrade one host at a time through an identity change.
  This closes the mainnet audit's "should fix" at `palw_mode_v2.rs:938`.
* *The pruning horizon.* SA-6's lattice grows by the same term, so the depth goes 6,602 → 12,002.
  The depth is not a fence and is hashed into `consensus_params_id` directly, so
  `consensus_identity_id` normalises it back with the fence: two builds that differ only about a
  future height announce one identity and stay peers, which is the whole rollout. Proven by
  `scheduling_the_da_court_moves_the_fingerprint_and_leaves_the_identity_alone`, which reconstructs
  the running fleet's ruleset and pins its fingerprint to the one testnet-11's nodes print.

The horizon change is invisible in substance until blue score 6,602 — a depth this chain has never
reached (tip 1,619 at 6.2 DAA/h) — so no header's pruning point differs under either build.

testnet-11 keeps the FLAT prompt commitment: `palw_prompt_ids_merkle` is genesis-only
(`validate_palw_v2`), because no reader of a `prompt_token_ids_hash` holds a job's anchor height.

### Rollout record, 2026-09-06 05:33–05:47Z

Built on ibm from `9f2aad95` (`--features evm`, 14m19s), kaspad sha256 `7181cc07eb57a2f8`; the
previous binary (`b6cf47ea3a66480b`) is kept on each host as `/root/t11/kaspad.pre-flagday-b6cf47ea`.
Restarted in order — ibm node0, ibm node1, .113 node, .113 pool slots 02/04/05/06, 5.104 seat2 —
each confirming `Consensus params fingerprint: b511dd1e…` before the next. After the roll: node0
7 peers, .113 9 peers, both at DAA 1,627, no panics, no refusals inside the fleet.

**What the roll measured, and it is not what the fork-id module's doc promises.** Adding `1900` to
the schedule partitions old from new IMMEDIATELY. The new build keeps an old peer ("keeping the
peer"); the old build does not keep the new one — its gate is already armed by ADR-0083's fence, the
new node announces `next = 1900`, and 1900 is not on the old schedule, so the old side rejects and
closes. Six third-party seats were cut off within four minutes of the first restart
(164.68.119.212, 217.178.131.170, 113.155.23.105, 60.114.127.4, 207.180.230.3, 121.81.248.189).
The height's lead time buys the FLEET an ordering; it does not buy the network a grace period, and
anyone running a testnet-11 seat has to upgrade now rather than before DAA 1,900. Recorded at
`fork_id_gate_fences_v1`, where the next operator will read it.

One node on the old build is deliberately untouched: `/root/misakas-user/target/release/kaspad`
on .113 (`--appdir=/root/palw-user/appdir`, `--palw-register-class=Qwen/Qwen3.8-27B/`), the manual
node from the Qwen3.8-27B add-model walk. It is the operator's experiment, and restarting it is
theirs to time.

## Mission alignment amendment — 2026-10-07

客観的demand/disclosure/deadlineによるDA courtを維持する。materialが開示された自己整合的な偽traceはDAだけで算術有罪にならないため、公開localizationとexact courtへの次経路を必要とする。非開示有罪とComputationMismatchを同一視しない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0062-data-availability-court.md -->

</details>

<a id="source-0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge"></a>

<details>
<summary>0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md — 原文 L1–L310</summary>

[原文を開く](../adr/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) · SHA-256: `73d849b7a6b384deff4965843b803047f5b8fba1dbf28b589e3aa1d217cf2fb2`

<!-- ADR_SOURCE_BEGIN:0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md -->
~~~~~~~~markdown
# ADR-0111 — A seat may demand the committed leaf it needs to judge

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context` (continuing ADR-0103 at `36f115c4`),
  written from the operator's decision on ADR-0103 §10.5; **IMPLEMENTED the same day (§8), both
  paths drilled to a slash on a live devnet**. Nothing is armed: the new unit rides the held regime
  (`Params::palw_held_context`, `None` on every shipped preset) and ADR-0062's court, so no shipped
  fingerprint, identity, schedule or fork id moves. Testnet-11 does not move.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (Decision 1: the one-move court is the court; §10.3 item 11: the prompt tile rides the accusation;
  §10.5: the gap this ADR closes), [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the one-move court and its false-accusation charge), [0062](0062-data-availability-court.md)
  (an accusation names what is missing; SA-2 an answer is hash arithmetic; SA-4 a refuted accuser
  pays; Decision 5 silence is the default), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)
  Decision 8 and SA-2 (the seat's draw; the authenticated interval lane),
  [0086](0086-the-opening-carries-the-fold-not-the-leaves.md) Decision 6 (a block's leaves name the
  leaf), 0085 §6 item 4 (the close annex: the committed tiles of a disputed leaf),
  [0081](0081-long-context-the-input-is-a-state-chain.md) Decision 3 (the prompt's tiled root).
* Amends: ADR-0103 Decision 1 (who holds a refutation's inputs) and §10.3 item 8 (the
  data-availability court is required at the fence after all — §2 Decision 5); ADR-0062 Decision 1
  (what an accusation may name: a leaf's committed evidence); ADR-0085 §6 item 4 (the annex is
  served for a leaf a seat names, not only for a leaf an open session names). Supersedes nothing.

## 0. The sentence this ADR is

**A seat need not retain the whole capture. A committed leaf needed by a valid court challenge
must be obtainable from the executor. The fast path is a signed off-chain leaf-evidence request.
Failure of that path opens a bounded on-chain availability request. Failure to open the committed
leaf before the deadline voids the claim. Opening it feeds the existing one-move adjudicator; it
does not create a second execution court.**

## 1. What was found

ADR-0103's live drill convicted a tampered free-prompt claim under the held fence (§10.4, run 2),
and every seat that convicted held the claim's whole capture. At the widths ADR-0103 is for, no
seat does. What a seat holds there, and what the court needs, do not meet:

* **A refutation reveals committed tiles.** The one-move court recomputes a leaf from its committed
  inputs and compares the committed output, so the accusation carries the output tile and every
  input row, each opened against the step root, plus the artifact rows and the prompt tile
  (ADR-0100, ADR-0103 §10.3 item 11).
* **A seat holds hashes.** An interval opening carries the interval's committed leaves as hashes
  (block roots where the capture is retained sparse), and the block-leaves lane carries leaf hashes
  (ADR-0086 Decisions 1 and 6). A seat can NAME the leaf off the chain and holds none of its tiles.
* **The executor serves tiles only to a session.** The close annex carries a disputed leaf's tiles
  for a leaf an open court session names (ADR-0085 §6 item 4, "read off the chain, never off the
  request"), and under the held fence no session opens (ADR-0103 Decision 1).
* **The held DA court cannot compel them.** Its units are a prompt tile, a state chunk and a range
  of leaf hashes (ADR-0103 Decision 4) — none is a step tile.

So a liar that commits one wrong leaf is named and not convicted. Serving the tiles on request is
not enough by itself: an honest executor answers, and a liar simply does not.

## 2. Decisions

**Decision 1 — the unit is a leaf's evidence, and it is the one-move court's object.**
`PalwLeafEvidenceV1` is a refutation's committed half — the output tile and its opening, the
canonical input rows and their run siblings, the KV anchor where the class checkpoints, the decode
pin where the leaf is a decode gather — plus the artifact rows its recomputation reads (opened
against the class root) and the prompt tile its gather reads (ADR-0103 §10.3 item 11's carriage).
It is exactly a `ShardCourtAccused` accusation's content without the accuser. One builder produces
it and one function adjudicates it (`palw_one_move_verdict_v1`), whether it arrives inside an
accusation or inside a disclosure; there is no second court. It carries no prompt list and no
weights beyond the rows the recomputation reads, and it is bounded by the court's close ceiling.

**Decision 2 — the fast path: a signed leaf request on the interval lane.** The seat asks the
executor for the evidence of the leaf it named, on the transport that already carries its interval
requests (ADR-0077 Decision 8): the request index sets bit 29 above the leaf's interval, and the
leaf itself rides a new field that the request's signature binds. The executor answers with the
evidence built from its retention by ADR-0085's path — the leaf's interval replayed from its
anchor, the annex for that one leaf, the refutation assembled from it — so an honest executor's
answer costs one interval's replay, never the job's. (Where that path cannot serve the leaf — a
family that commits its logits flat, or an interval whose committed leaves are not the executor's
own replay, which is a liar's — the builder takes the whole-capture prover, ADR-0085 X1's same
object, at the cost of the executor's own retention.) It is served only to a key the lane already
authorises for the claim, charged to that bond's request rate and throttled per `(peer, claim,
request)` exactly as every interval request is. The seat runs Decision 1's verdict at its own artifact root:
guilty, it files `ShardCourtAccused` (ADR-0100); not guilty, it files nothing and says that its
replay and the court disagree, which is a determinism fault and not a claim's.

**Decision 3 — the slow path: a demand on chain, bounded by chain facts.** When the fast path
does not answer, the seat files `DefaultAccusedHeld` naming `PalwHeldMissingV1::StepLeaf { leaf }`.
It is admitted only when:

* the accuser is a seat of the claim's bound panel;
* the leaf lies in one of the intervals that seat's draw assigned it —
  `palw_fp_interval_draw_v1` over the claim's panel anchor, the seat's index and the claim's own
  interval count, derived from the binding the demand carries — so a seat can demand only a leaf the
  chain told it to check, never one it chose;
* the leaf is not a fused-attention site (that leaf's terminal is ADR-0103 Decision 5's
  dissection, whose responder is already clocked);
* the seat has not demanded on this claim before, for the claim's whole life;
* and every gate of ADR-0062's court holds: an Active bond at or above the floor, not the claim's
  own, a bound panel, the whole disclose window inside the claim's retention, one session at a time.

**Decision 4 — the answer is an adjudication.** The executor answers with `MaterialDisclosedHeld`
carrying `PalwHeldDisclosureV1::StepLeaf { evidence }`, signed by the claim's bond. The fold runs
Decision 1's verdict on it at the class's artifact root and the ruleset's ladder:

* `ExecutorGuilty` — the claim voids `CourtFraud` and its executor is slashed exactly as a
  `ShardCourtAccused` conviction does it; the accuser's stake comes back.
* `FalseAccusation` — the leaf recomputes: the session closes refuted, the accuser pays what it
  reserved (ADR-0062 SA-4), and the claim resumes.
* anything else — the evidence does not adjudicate, so it is not an answer; the session runs on.

No answer inside `W_disclose` is ADR-0062 Decision 5's default: `ProducerWithholding`, void and
slash. **Hiding the evidence becomes the conviction.**

**Decision 5 — the held regime needs the data-availability court.** ADR-0103 §10.3 item 8 left the
court optional at the fence. Without it Decision 3 does not exist and a withheld leaf is
unprosecutable, so `palw_held_context` now refuses to arm unless `palw_da_court` and
`palw_fp_da_pins` are armed at or below its height (the pins are what make a free-prompt claim's
retention obligation a chain fact rather than the producer's number), and `palw_held_context_mint_v1`
arms both from genesis.

**Decision 6 — the node carries both halves, and they ship together.** A seat that names a leaf and
holds no capture it could accuse from asks the fast path; if no answer arrives within
`PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1`, it files the demand. An executor answers leaf requests, and
answers every held demand its retention can answer — a prompt tile, a state chunk, a range, a
leaf's evidence — because an executor that cannot answer is one the court slashes. The answering
half is part of the court, not an optional extra.

**Decision 7 — the bound on an executor's burden is the chain's, not the requester's.** At most one
demand per seat per claim, each for a leaf the chain assigned that seat, each answered by one object
inside the close ceiling: per claim, at most the panel's size in disclosures. A requester pays the
carrier of its demand and stakes what an accuser stakes; an executor that is honest and answers is
paid back by the charge on the requester.

## 3. What this costs

* **Chain, honest path:** nothing. The fast path is off chain.
* **Chain, withheld leaf:** one demand (a binding and a leaf index) and one disclosure (at most the
  close ceiling), or one default.
* **Executor:** one interval replay per leaf request, and the same to build a disclosure.
* **Seat:** one request and one one-move verdict, at its own artifact root.

## 4. Invariants the tests must hold

```
1  One verdict: a leaf's evidence adjudicates identically inside ShardCourtAccused and inside a
   disclosure, and the fast and slow paths carry the same evidence bytes for one (claim, leaf).
2  A demand is refused by name from a bond that is not a seat of the claim, for a leaf outside the
   seat's drawn intervals, for a fused-attention leaf, a second time by the same seat, and while
   the held fence or the DA court is dormant.
3  A disclosure of guilty evidence voids the claim CourtFraud; of honest evidence resumes it and
   charges the accuser; of evidence that does not adjudicate is no answer; silence past W_disclose
   voids it ProducerWithholding.
4  palw_held_context refuses to arm without palw_da_court and palw_fp_da_pins at or below it; the
   mint arms them; both shipped fingerprints are byte-identical.
5  Live: a seat that holds no capture convicts a tampered claim through the fast path, and through
   the slow path when the executor refuses the fast one.
```

## 5. Order of work

1. The evidence and the one verdict (core); the unit, its admission and its adjudication (fold and
   acceptance); the fence rule and the mint.
2. The duty the executor answers (the held unit on `PalwDaDutyV2`); the executor's answers.
3. The leaf request on the interval lane; the seat's fast path, then its demand.
4. The drill: a seat without the capture, then an executor that refuses the fast path.

## 6. What is deliberately not decided

* **Activating a wider context.** How a context limit is armed from reproducible public evidence
  rather than a maintainer's workstation is the next ADR's (the verification vectors; §7).
* **A fused leaf's evidence.** Its terminal is the dissection (ADR-0103 Decision 5); the responder
  is clocked there, and silence already convicts.
* **Who pays a seat's fetch bandwidth** (ADR-0103 §8), unchanged.

## 7. Number hygiene

0104 (the close cut, `fix/panel-pays-consensus-rent`), 0105 (the heartbeat, on
`docs/renumber-heartbeat-adr-0105`), 0106 (the streaming inventory, `feat/adr-0099-sharded-seat`)
and 0107 (share growth, `fix/share-growth-counts-final-work`) are resident on other branches as of
2026-09-11. The operator's draft called this ADR and the verification-vector one 0104 and 0105.

This ADR has been renumbered twice on 2026-09-11, each time by the rule it states:

1. It was first committed as 0108 (`e251c751`, 15:19:45 +0900), 102 seconds after `f608a066`
   (15:18:03) committed ADR-0108, "an extension is a manifest the verifier recomputes", on a
   concurrent session's `feat/adr-0108-extension-envelope`. That made this ADR the later writer,
   and it moved to 0109 (`8856efc0`, 16:45:14).
2. At 17:02:16 the bridge-liveness ADR, "a lock is its own claim, and finality is a label, not a
   pause", was committed as 0109 on `feat/adr-0109-bridge-liveness`. At 17:37 it and ADR-0108 were
   merged to `origin/main`. This ADR was the earlier writer of 0109 by seventeen minutes, but the
   other one is published and this one was not. An unpublished document moves for nothing, and a
   published one moves for every reader, so this one moved again. The number it took is 0111,
   because 0110 was already this branch's verification-vector ADR, whose vector names embed its
   number.

Every reference on this branch moved with each renumbering: code comments, log lines and the drill's
patterns. The commit subjects of the day still say `adr-0108` and `adr-0109`.

## 8. Implementation record (2026-09-11)

Built on `feat/adr-0103-held-context`, continuing ADR-0103 at `36f115c4`: commits `e251c751` …
the branch head. **Nothing on a shipped preset moves**: the unit, its admission and its
adjudication ride the held regime and ADR-0062's court, both `None` on every shipped preset, so
both shipped fingerprints are byte-identical (`shipped_presets_have_pinned_fingerprints` green) and
testnet-11 does not move. The one rule that changed is the held fence's own (Decision 5), which no
shipped preset arms.

### 8.1 What each Decision became

| Decision | where it lives | what pins it |
|---|---|---|
| **1** one unit, one verdict | `PalwLeafEvidenceV1` and `palw_one_move_verdict_v1` (`palw_shard_court_v1.rs`); `palw_shard_court_verdict_v1` is the shape check plus the same function; `palw_leaf_evidence_from_capture_v1` (`palw_leaf_evidence_v1.rs`) the one builder — the annex route, the whole-capture prover where the family refuses it, the artifact rows, the prompt carriage | `the_executors_leaf_evidence_is_the_one_move_object_by_either_route` (guilty convicts, honest clears, X1 equality, a decode leaf, one devnet carrier) |
| **2** the fast path | request index bit 29 (`palw_leaf_evidence_request_index_v1`), `leafIndex = 6` on `PalwIntervalOpeningRequestMessage`, the signed message tag 3 (`palw_fp_leaf_request_message_v1`); the resolver answers a leaf request before any interval arithmetic; the seat's `pursue_named_leaf_v1` | `the_panel_pursues_a_named_leaf_serves_its_evidence_and_answers_the_held_court`; the leaf-request signature round trip |
| **3** the demand, bounded | `PalwHeldMissingV1::StepLeaf` (appended); the binding half in `palw_held_da_check_accusation_v1` (in the step space, not fused); the chain half at acceptance (`palw_leaf_demand_is_the_seats_v1`: a seat of the bound panel, a leaf in its drawn interval — the seat's own geometry, `PalwSeatIntervalGeometryV1`, in both units) and in the fold (once per seat per claim: `held_leaf_demands`, delta tag 40, carriage tail `0xA4`, root block `held_leaf_demands`) | the two fold tests; `the_chains_interval_geometry_is_the_seats_on_both_units`; the delta tag pin |
| **4** the answer adjudicated | `MaterialDisclosedHeld` with `PalwHeldDisclosureV1::StepLeaf` — `ExecutorGuilty` voids `CourtFraud`, `FalseAccusation` closes refuted, anything else is `HeldDaRefused` | the fold tests |
| **5** the fence needs the court | `Params::validate_palw_v2`: `palw_held_context` refuses without `palw_da_court` and `palw_fp_da_pins` at or below it; `palw_held_context_mint_v1` arms both from genesis | the fence test's refusal cases |
| **6** both halves ship | the executor answers all four held units (`held_da_answer_v1` has no catch-all arm; the state chunk and the step range through `held_state_chunk_answer_v1` / `held_step_range_answer_v1`, every family); `PalwDaDutyV2.held_missing` tells it which | `the_executor_answers_every_held_unit_the_court_can_name` (held fold, every checkpoint, ranges across a block edge and at the widest, the court's own check); `the_floor_answers_a_held_chunk_and_range_from_its_dense_retention`; the no-catch-all pin |
| **7** the burden is the chain's | Decision 3's bound; the lane's rate and throttle for the fast path | as above |

### 8.2 What the live drill found (both fixed, both pinned)

`scripts/drills/misaka-palw-shard-court-devnet-drill.sh` with `HELD=1 LEAF=fast` runs the executor with
`--palw-drill-answer-only` — it serves its claims' answer envelope and never the capture — so a
seat judges by intervals alone. Its first two runs found two defects older than this ADR, on the
path from a sampled fault to a named leaf:

1. **The first fault gate stopped a fault still addressed at a block.** The interval arm notes a
   `FaultInRange` as a block, asks for the block's leaves, and names the leaf from them on a later
   round (ADR-0086 Decision 6) — which the gate, stopping the verdict block the moment any fault was
   held, never let run. `de6fb0da`: a free-prompt claim whose fault is a block runs the interval arm
   again; the ledger never replaces a named leaf with a block, so the fall-through ends the round
   the leaf is named.
2. **The naming could never name a leaf in a lying interval.** It ran the close's replay, which on
   a fold puts the seat's own leaves under the served frontier and requires them to walk to the
   committed root — exactly what a lie inside the interval prevents ("this party's replay does not
   reproduce the step leg root under the served frontier", every round). `6dffa805`: the naming
   replays without the walk; the executor's leaves are bound to its commitment through the served
   block's digest, and what is named is adjudicated on the executor's own evidence, so a ghost can
   only clear. Pinned by `a_served_block_names_the_leaf_a_liar_committed`; restoring the walk
   reproduces the drill's error verbatim. **The same walk was on ADR-0085 Decision 3's close from
   served intervals**, which therefore could not close against a lying interval either. It was not
   on this ADR's path (the evidence is the executor's), and it was fixed the same day (§8.5).

### 8.3 The drill runs

Three nodes on one host, the floor-only devnet minted with `--palw-held-context-devnet`, node-0
the liar (step leaf 0 of its canonical free-prompt claims tampered and re-committed) and running
`--palw-drill-answer-only`, so no seat holds a capture:

| run | binary | what happened |
|---|---|---|
| 1 | `be3c5101` | the seat verified interval 0 as a fault in block 0, asked for the block, held it — and nothing followed: 8.2 item 1 |
| 2 | `de6fb0da` | the block was read every round and named no leaf ("does not reproduce the step leg root under the served frontier"): 8.2 item 2 |
| 3, `LEAF=fast` | `6dffa805` | **PASS.** Claim `14b3ed4c…` committed 16:47:08; at 16:53:09 node-1 found the fault in block 0 of interval 0 and asked for the block (`0x80000000`); at 16:53:11 the block named leaf 0; at 16:53:13 it asked for the leaf's evidence (`0x20000000`) and node-0 served 25,029 bytes in 29 ms; at 16:53:15 it filed `ShardCourtAccused` on the executor's own evidence, and node-2 did the same ten seconds later. Block `a7591092…` carried it at 16:55:29; the claim read `voided court_fraud` on node-1 and node-2; bond 0's collateral fell 1,110,106,160 → 1,110,067,640 sompi |
| 4, `LEAF=slow` | `8856efc0` | **PASS.** node-0 also runs `--palw-drill-refuse-leaf-evidence`. Claim `c31aa3bd…` committed 17:00:55; at 17:07:02–04 both seats named leaf 0 and asked, and node-0 refused; at 17:12:24, `PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1` later, both demanded it on chain; block `0d6679a1…` carried one `DefaultAccusedHeld` at 17:13:17 (the other seat's demand was dropped with its block standing — one session a claim); at 17:13:18 node-0 answered "the held accusation of `StepLeaf { leaf: 0 }` from the retained capture — deadline DAA 124"; block `a9cfa992…` carried the `MaterialDisclosedHeld` at 17:15:34 and the fold's one-move verdict on it voided the claim `court_fraud`, on node-1 and node-2; bond 0's collateral fell by the same 38,520 sompi |

Invariant 5 holds on a live devnet: a seat that holds no capture convicts a tampered claim through
the fast path, and through the slow path when the executor refuses the fast one — and a refusal is
worth nothing to it, since the demand's answer is its own evidence and silence would have been
`ProducerWithholding`.

### 8.4 What remains before a network arms the held regime

* The verification-vector ADR (§6) — the evidence a context limit is armed on (ADR-0110).

### 8.5 The close from served intervals, against a liar

ADR-0085 Decision 3's close (a challenger holding no capture assembles the close from the served
intervals and its own replay) replayed each served V4 interval and required its own leaves to walk
to the committed step root under the served frontier. A lie inside the interval rules that out, so
the close refused every lying interval, and its one test ran an honest executor.

* **The close holds the executor's leaves for the one block its own leaves differ in.** They are
  ADR-0086 Decision 6's `Base0FpBlockLeavesV1`, carried in `held` under the interval lane's
  block-leaves index. `base0_fp_committed_range_with_served_block_v1` substitutes them only once
  they fold to the digest the opening carries, and the result walks to the committed root or the
  close is refused. The refusals are by name: no served block, a block that is not the one that
  differs, a block that does not fold, and more than one differing block (a commitment that
  differs past the block a lie is confined to). An honest interval walks on its own leaves and
  needs no block. The replay's question is now an enum (`Base0FpServedLeavesV1`): the naming asks
  for this party's own leaves, and the close asks for the committed range.
* **The node's close gathers the block.** `close_source_from_served_intervals_v1` names the
  disputed leaf's block when the opening covers it whole, asks for it on the lane like a missing
  interval, and hands it to the close.

Pinned by `a_close_from_served_intervals_convicts_a_lying_interval_once_its_block_is_served` (the
graph-v5 dense row, tiled logits, the drill's one-tile lie). Without the block the close is refused
by a name that says so. With it, the close is the capture path's object for the same leaf
(ADR-0085 X1 against a liar too), and the one-move verdict on it is `ExecutorGuilty`. The honest
capture's close at the same leaf needs no block and is `FalseAccusation`. Also pinned:
`a_close_takes_the_one_served_block_a_lie_is_in_and_refuses_the_rest_by_name` (every refusal) and
`the_close_from_served_intervals_holds_and_asks_for_the_disputed_leafs_block` (the node). The honest
X1 pin (`a_close_from_served_intervals_is_the_close_from_the_capture_and_the_annex_changes_no_verdict`)
is unchanged and green.

## Mission alignment amendment — 2026-10-07

将来のleaf demandは、bound seatだけでなく公開bond条件を満たす非seatにも提供する。committed fused leafとそのdissection/close materialまで認証し、単にleaf番号を特定できたことを訴追完了としない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md -->

</details>

<a id="source-0206-panel-da-mandatory-disclosure-g14-route"></a>

<details>
<summary>0206-panel-da-mandatory-disclosure-g14-route.md — 原文 L1–L68</summary>

[原文を開く](../adr/0206-panel-da-mandatory-disclosure-g14-route.md) · SHA-256: `44aa854f9a7f654c1264742a192632b8827966a60452d17be5934ec51427a889`

<!-- ADR_SOURCE_BEGIN:0206-panel-da-mandatory-disclosure-g14-route.md -->
~~~~~~~~markdown
# ADR-0206: PanelDa の G14 経路 — 期限付きの必須公開

- Status: **accepted**(2026-10-26 ユーザー決定: 期限付き必須公開と「期限まで非公開」への縮小を採用。値は目安のまま。fence は全 preset で `None`)
- Date: 2026-10-26
- Lane: g14-fam-panelda(branch `claude/g14-fam-panelda`)
- 関連: ADR-0077 D16 / SA-5、ADR-0119 D7、ADR-0192(claim 共通 frontier)、ADR-0198(notice a+3,120 / hard a+10,000)、2026-10-17 決定(court 保持 ≥ 4W+開示窓)、PRINCIPLES §8・§9
- ユーザー決定 2026-10-26:「PanelDa の実装は行なって 画像は実装しないけど」— FP V5(画像入力)には経路を作らない。

## 問題

PanelDa(privacy mode 2)はプロンプトの ids を commitment tx に載せず、抽選 seat だけが material で受け取り、紛争時にだけ公開される。G14 の不変条件(producer と Panel 全員が共謀しても、Panel 外の public bonded verifier 1 人が公開認証 material だけで客観的に conviction し、期限内に slash まで届く)に対し、全 owner が共謀すると誰も紛争を起こさず、外部者は ids を得られない。よって現行表では V5 と同じ `NonPublicEvidence`(経路なし)で、G-01 (b) により報酬・weight・credit なし。

## 比較した候補

| 案 | 内容 | 判定 |
|---|---|---|
| A. 期限付き必須公開 | commit 時に `prompt_token_ids_hash` で束縛、service 中は非公開、受理 a から a+1,040 までに ids を公開 material として開示。未開示は default(deposit 没収・claim void、計算の有罪にしない)。開示後は PublicDa と同じ ③〜⑥ | **採用** |
| B. 外部者による開示請求(請求があれば開示) | 外部者は請求すべき claim を知るのに material が要らないが、請求の bond・期限・応答遅延の窓が増え、全 claim に請求を出す Sybil 費用を外部者が負う | 不採用(外部者 1 人で足りる保証が費用依存になる) |
| C. 抽選された外部 seat への秘匿共有 | 「外部」seat も抽選された保持者で、共謀集合に入りうる。結局公開しない限り外部者 1 人の保証にならない | 不採用 |
| D. 暗号化して公開し後で鍵公開 | A と同じ期限問題に帰着し、鍵の不開示=A の default。構造が増えるだけ | 不採用(A に含まれる) |
| E. 計算の暗号学的証明で非公開のまま検証 | PRINCIPLES §8 で恒久禁止 | 対象外 |

A を選ぶ理由: 外部者が必要とするのは「hash に束縛された ids」だけで、期限で公開を強制すれば共謀の有無に関係なく material が公開側に出る。取得不能は default として経済的に処理し(計算誤りの有罪にしない)、開示後の告発経路は既存の公開 FP family をそのまま使うので新しい裁定規則が要らない。

## 決定(案)

1. **束縛**: job の `prompt_token_ids_hash`(class の prompt-ids form、Flat / MerkleV1)が ids を固定する。同一性のための hash commitment であり、計算の正しさの証明ではない。
2. **非公開の期間**: 受理 `a` から最大 `a + 1,040` DAA(目安 `PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1`)。その間 ids を見るのは抽選 seat だけ。**confidentiality は約束しない**(seat は見る、期限で必ず公開される)。
3. **開示義務**: 期限(含む)までに ids を公開 material として載せる。誰が載せてもよい(hash が中身を固定するので、filer の身元は関係ない)。責任者は producer。上限 32,768 ids(目安)。
4. **外部者の窓**: 期限後も notice 窓 `3,120 − 1,040 = 2,080` DAA が残る。武装条件として `DISCLOSE_BY + OUTSIDER_MIN(2,000) ≤ 3,120` を validation で検査。hard deadline・延長は `palw_court_clock_v2`(a+3,120 / a+10,000)に従う。
5. **保留**: 開示が fold されるまで報酬・weight・settlement credit を出さない(CERT の保留と同じ扱い:未開示の claim は Final 決済を進めない)。
6. **default**: 期限までに開示がなければ claim void、報酬なし、開示 deposit(目安 13,107,200 sompi = 最大開示の運搬費の 2 倍)を没収(burn)。**計算の有罪・計算 bond の slash にはしない**(取得不能を計算誤りの有罪にしない)。期限後の開示で復活しない。
7. **開示後**: claim は PublicDa と同形の公開 FP family(FpSelection・FpConstraint・FpPrefix・FpTokenizer、③〜⑥)で外部者が告発でき、有罪なら既存経路の slash。
8. **G14 表**: 新 family `FpPanelDaDisclosure`(enum に追加、既存番号不変、serialize/hash されない)。fence `palw_panel_da_disclosure_v1` 以降に受理された PanelDa の**テキスト** job は `[FpPanelDaDisclosure, FpSelection, (FpConstraint), (FpPrefix), FpTokenizer]`。fence 未満と V5 は従来どおり `NonPublicEvidence`(byte 同一)。
9. **武装に要る fence**: `palw_panel_da`、`palw_court_clock_v2`、`palw_panel_da_disclosure_v1`、`palw_g14_route_required_v1`(+開示後の公開 FP family 自身の fence)。record は空(pin は統括)。
10. モデル二階層・claim 単位の抽選承認(Gate C)は変えない。

## 利用者への開示の文面(fence が governs する network で、初回利用前に gateway が表示)

> Private only for a while: the verifying seat sees this prompt during the work, and the prompt is published on chain within about 1,040 DAA of acceptance whether or not anyone disputes it. This is not confidentiality. If it is not published in time the claim is void and its disclosure deposit is forfeited.

(`PALW_FP_PANEL_DA_MANDATORY_DISCLOSURE_V1`。fence 未満では従来の `PALW_FP_PANEL_DA_DISCLOSURE_V1`。)

## 費用・保持

- 開示の運搬: 最大 32,768 × 4 B = 131,072 B、t12 料率 50 sompi/B で 6,553,600 sompi。deposit はその 2 倍。
- 保持: 開示 material は公開 material として court class の保持(≥ 4W+開示窓、2026-10-17 決定)と clock v2 の `a + 10,000 + W_disclose` に従う。新しい保持規則は足さない。

## 結果

- 正: PanelDa テキストが G14 経路を持ち、全員共謀でも期限で material が公開される。
- 負: プロンプトは「期限まで非公開」に縮む(紛争がなくても公開)。利用者に明示する。
- 未了(本 ADR の武装前提): 開示 carrier(tx object・version 付き)、fold row、deposit lock、Final 保留の fold 配線、実ノード e2e。これらが入るまで validation は武装を拒否する。

## 値(目安、`palw_g14_values_v1`、台帳「2026-10-26 lane PANELDA」)

| 値 | 目安 | 根拠 | 見直し条件 |
|---|---|---|---|
| DISCLOSE_BY | 1,040 DAA | notice 窓の 1/3 | notice 窓・turn 期限・外部者 replay 実測 |
| OUTSIDER_MIN | 2,000 DAA | 包含遅延+最大 class replay+turn の 2 倍超 | 外部者 replay 実測 |
| MAX_DISCLOSED_IDS | 32,768 | t12 最大 8K class に余裕 | 長 context class |
| DEPOSIT | 13,107,200 sompi | 最大開示の運搬費の 2 倍 | 料率・最大 ids |

## 実装記録(2026-10-27、lane g14-fam-panelda)

- carrier tag 233(割当は統括の確認要)、row delta 236 / tail 0xE8 / root `panel_da_disclosures/v1`、void reason 140。
- 価格: derived-work 下で governed PanelDa は prompt 全体を既払い扱い(prefill 0)。過少払いのみで二重払いはない。answer accounting 武装時は従来どおり拒否。**ユーザー確認待ち**。
- validation は実ノード E2E が済むまで武装を拒否する。
~~~~~~~~
<!-- ADR_SOURCE_END:0206-panel-da-mandatory-disclosure-g14-route.md -->

</details>

