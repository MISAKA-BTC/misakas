# ADR-0170 — The seed anchor is a window, not a span: a merged attempt anchors, the anchor survives span boundaries, and the admission jury and the schedule seeding read the latest anchor of `S − 24 … S − 1`

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED 2026-10-03 on `anchor/window` (lane P2), for the DAA-5,300 flag day, the user's decision of the same evening
(variant "M1 + M2 + M3, W = 24"). Consensus change behind ONE fence, **dormant on every shipped preset**:
`Params::palw_anchor_window_v1`, a bare height whose companion values (`W`, `M1`) are hashed with it. Amends **ADR-0130** (what a seed
anchor is and how old a reader may take it) and **ADR-0147** (the jury's randomness and its population cut). Touches nothing else:
**SW-8's binding rule, ADR-0125's permit ledger, the audit period and its stagger, and every window of the panel are as they were.**

**Builds on:** ADR-0125/0130/0151 (the execution lane, its seed anchor, the economic-safety rotation), ADR-0147 (the admission jury),
ADR-0152-adjacent R2 (the staggered audit), ADR-0165 (the reserve: the reason this is needed now), ADR-0058 (merged work, blue or red).

## 0. The sentence this ADR is

**A chain block's own attempt used to be the only thing that could anchor a span, and under ADR-0165's reserve it almost never has an
attempt; the anchor is now the latest admitted attempt the fold took — its own or one it merged — kept until a reader that wants it
within 24 spans has used it.**

## 1. The finding (P2, 2026-10-03; evidence `lanes/evidence/head-admission-slip-1003/p2-anchor/`)

`round_seed_anchor` is recorded by exactly one thing — a chain block whose OWN header carries an admitted attempt (step 4's `Ok` arm) —
cleared at every span boundary, and read by exactly two:

* **ADR-0147's admission jury** (`admission_jury_v1`): an audit needs the anchor of span `S − 1`; without it no jury sits and the audit is
  SKIPPED, not deferred, until the class's next period (100 DAA on testnet-12);
* **the schedule seeding** (`rotate_round_lane`): one due snapshot a span, seeded only by the anchor of the span before; a snapshot not
  seeded within `PALW_EXEC_PENDING_GRACE_SPANS_V1` (64) spans of its target is dropped, and its Finals' execution rights with it.

While floors flooded the chain this was true of 98.7 % of testnet-12's spans (297 of 301 in the last 300 DAA). The int-11 drill's head class
was admitted one audit period late for the plain reason that span 90 held two heartbeats and no attempt (19.5 % of that drill's spans had
none). ADR-0165's reserve refuses a floor attempt while real work flows (`FloorNotIdle`: skipped by the fold, never a claim, never an
anchor), and a REAL attempt is almost never a chain block (it is merged, blue or red, by the heartbeat that took another selected parent:
0.5 % of the replay's chain blocks). Replaying testnet-12's last 300 DAA under the reserve (K = 20, all producers compliant):

| | today's rule | M1 | M1 + M2 + M3, W = 8 | **W = 24** |
|---|---|---|---|---|
| spans that carry an anchor | 2–3 % | 22–24 % | — | — |
| an audit seats (per period) | **2.7 %** | 24 % | 84 % | **100 %** |
| snapshots seeded (exec lane) | **34 %** (credits 41 %) | 99 % | 99 % | 99 % |

(Idle/Probe/Normal 20/8: 2.0 % / 24 % / 83 % / 100 %, exec 25 % → 99 %.) Every number carries replay #2's stated weak assumptions: one DAG,
exogenous REAL cadence, 72 REAL attempts. The measurement of the carried anchor's age at an audit: p50 3, p95 15, max 20 spans.

## 2. The rule

Past `palw_anchor_window_v1` (resolved at the folding block's DAA, the mirror `PalwStateParamsV2::anchor_window_from_daa`):

* **M1 — a merged admitted attempt records the seed anchor.** In step 4b, after a merged attempt (blue or red) is admitted, the fold
  records `PalwExecSeedAnchorV1 { span: the folding block's span, block: the attempt's own block, execution_key: the attempt's own }`.
  Own work first (step 4), then the mergeset in consensus order; the last admitted attempt wins, as the last chain block of a span always did.
  A refused attempt records nothing. The flag is `PALW_ANCHOR_WINDOW_MERGED_V1` and is hashed with the fence: `false` is the conservative
  variant (own anchors only), whose window is then `PALW_ANCHOR_WINDOW_SPANS_V1 = 64` (P2's replay: 83 % of audits seat on 64 spans of own
  anchors where 24 seat 41 %) — one line each, and a new network id.
* **M2 — the anchor is not cleared at a span boundary.** It keeps the span it was recorded in; every reader checks its age. No state field,
  delta or carriage tail is added: `round_seed_anchor` is reused (it is simply `Some` for longer), so the root and the carriage differ from a
  chain without the fence only in WHEN the anchor is `Some`, from the fence's height on.
* **M3 — the readers take the latest anchor of the window `S − W … S − 1`** (`W = 24`, `palw_anchor_window_admits_v1`):
  * the **admission jury** draws from `palw_admission_jury_seed_{v1,v2}(class, S, anchor)` as before, with the population cut at the ANCHOR's
    span (`palw_anchor_window_jury_cut_span_v1`: bonds registered before the span the randomness was recorded in began), so a bond registered
    after the seed existed is never on the jury it seeds. The audit's own span — once a period, at the class's stagger — is untouched, so there
    is no deferral and no re-roll, and P3's proof-timing adapter (`palw_candidate_audit_due_v1`) is unchanged. **The Activation Pool's (a)
    keeps its meaning**: it pays a drawn juror whose readiness proof LANDED before the seed existed — "two spans before the audit" for the
    anchor of the span before it, as always (review M6) — and so, past the fence, `min(S − 2, anchor.span − 1)`
    (`palw_anchor_window_prep_landed_by_v1`; the jury carries its anchor's span): a juror that can see the seed knows whether it is drawn,
    and a proof sent after that is no preparation. P3's first send (`S − 22` or `S − 21`, landing about `S − 20`) precedes every anchor
    of the window but the oldest;
  * the **schedule seeding** seeds a due snapshot with the anchor only if it was recorded at or after the span the snapshot was taken in
    (`target − 1 − maturity_spans`, `palw_anchor_window_seeds_snapshot_v1`): ADR-0130's "participants first, randomness after". On testnet-12
    the 120-DAA maturity already implies it for `W ≤ 120`; it is asserted rather than assumed. One snapshot a span, oldest first, the 64-span
    grace: as before.

## 3. What it costs, stated (the judgments this ADR asks for)

1. **ADR-0130 — what moves an anchor.** Moving it costs an admitted attempt: while the reserve is closed a REAL inference of an Active class,
   while it is open a bonded floor. That is the property the lane was built on and it holds. **The new freedom** is WHICH of the attempts a
   merging block takes is the last: a choice among attempts that are already public, by a block producer who already chooses its parents —
   not a free re-roll of a value. A producer that can make a cheap admitted attempt (a tiny Active class: ADR-0165's idle ledger has the same
   weakness, a cheap REAL attempt keeps the floor closed) moves the anchor at that cost; the fence adds no cheaper way than the floor's was.
2. **ADR-0147 — the jury's lead time.** The jury's seed is known `S − anchor.span` spans before the audit instead of one: p50 3, p95 15,
   max 20 spans (about an hour at the worst, W = 24). Its population is fixed before the seed (the cut above), so the registrant gains only
   time to make drawn operators hold the class — which they can do only by holding it, a proof signed with their own bond key.
3. **ADR-0125/0130 — the schedule's privacy.** "A span's producers are public for one span, not before" weakens to "the seed is public up
   to W spans before"; the schedule also needs the frontier at the span's opening, which moves when Finals mature. Participants are still
   fixed by the snapshot, which is taken at least `maturity_spans` before.
4. **SW-8 is untouched.** A claim still binds only in its own anchor block (the first chain block at or past its slot that is or merges an
   operator's attempt); a claim whose anchor block does not bind it still voids there; the anti-grinding argument is as ADR-0152 wrote it. Under
   the reserve a claim WAITS for the next operator attempt (P2: 6 / 16 / 21 DAA past the slot at p50 / p95 / max on the replay; no void, REAL
   or floor, operator or external); the hole that remains — external REAL keeping the state Normal while no operator attempt appears — is
   lane RS's R1 (an operator floor producer that holds only for the idle policy mines one binder floor when a claim has waited 30 slots).

## 4. The fence

`Params::palw_anchor_window_v1: Option<ForkActivation>`. Written in the four places of ADR-0165's fences: the field; `for_each_fence`;
Some-only in `consensus_params_id` and `consensus_schedule_id` with `palw_anchor_window_value_v1()` = `[24, 1]`; the `never()` collapse. The
bundle's mirror `PalwStateParamsV2::anchor_window_from_daa` is written by `Params::sync_palw_anchor_window_v1` (called from
`palw_v2_params_on_base` and by the entry's `set`). `validate_palw_v2` refuses it, by name, off ConsensusV2, with the mirror unsynced, and
without the execution lane (`palw_execution_lane`), the economic-safety bundle (`palw_economic_safety`), ADR-0147's jury
(`palw_admission_independence`) or the model registry (`palw_model_registry`) at or below it — all in force from genesis on testnet-12. Its
check runs LAST on both paths of `validate_palw_v2`, after every other fence's own, so a prerequisite it shares with another fence is named
by that fence's refusal first (the other fences' prerequisite tests need no edit).
Its entry `PALW_T12_ANCHOR_WINDOW_ENTRY` is the 24th of the 29 entries of `PALW_T12_INT11_FENCES_V1`, right after `palw_real_clock_tick_v1` and before
`palw_capacity_network_verify` (the capacity entries stay the tail): the DAA-5,300 flag day, and `--palw-drill-int11-at` moves it with the
list, so there is no drill flag of its own. Every shipped preset and `palw_t12_release_v5_params` (the DAA-3,600 release) leave it `None`
and fingerprint byte for byte as before.

## 5. Tests

* `consensus/core/src/palw_anchor_window_v1.rs` — the window's arithmetic, the population cut, the snapshot rule, the pool's landing bound,
  the companion values;
* `consensus/core/tests/palw_anchor_window_fence.rs` — dormant on every ruleset, armed moves params and schedule ids and never the identity, the
  fork id gates it, refused without its prerequisites or with the mirror unsynced;
* `consensus/core/tests/palw_t12_flag_day_int11.rs` — the list is 29 entries, in order, with the window's prerequisites refused by name;
* `palw_state_v2/tests/anchor_window_v1.rs` — M1 (merged anchors; the last admitted wins; a refused one records nothing; below the fence none),
  M2 (survives span boundaries), M3 on the schedule rotation (seeds a snapshot the old rule would have left waiting; older than the snapshot
  seeds nothing; W's far edge), and **byte identity before the fence** (the same chains, a fence at a height never reached: the same root at
  every block);
* `palw_state_v2/tests/adr0135/admission_independence/anchor_window_v1.rs` — the audit seats when the only admitted attempts of the window are
  merged (and not below the fence), the anchor survives to the audit, the window's 24-span edge, the population cut at the anchor's span,
  byte identity before the fence;
* `consensus/src/pipeline/virtual_processor/tests/t12_anchor_window.rs` — testnet-12's own GHOSTDAG (the real pipeline: template, GHOSTDAG
  colouring, fold, state store) with the reserve armed from genesis, floors held (none is mined) and the 8k class's REAL attempts flowing
  (two a period, each templated 340 s before the heartbeat that merges it, a side block and never a chain block): with the fence the head
  class (planted `Candidate`, fresh readiness on all eight operators) is admitted AT ITS FIRST AUDIT (span 130: the REAL attempts merged at
  spans 113 and 124 anchored, the anchor of span 124 seeded the jury, the class left `Candidate` for `Prefetching`); under the reserve alone
  the same traffic leaves no seed anchor at either audit (spans 130 and 230) and the class stays `Candidate` through both — the int-11
  drill's skipped audit, reproduced and closed. (The harness runs no panel, so a REAL claim never resolves and the panel's replay room, five
  claims of the class, bounds the traffic; the live chain's supply of anchors is far denser.)

## Mission alignment amendment — 2026-10-07

seed windowはassignment/admission randomnessの規則であり、public proofの提出資格や算術真実の承認ではない。anchorがない場合の通常処理と、独立した外部courtの受理を混同しない。

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
