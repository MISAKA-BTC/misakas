# ADR-0166 — Verifier unavailability is not producer fraud

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Model / claim-evidence boundary, 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md) withdraws model-wide FPR/TRDC/Seeder service liability and availability-based reward/weight gates. This historical court concerns named, bounded claim evidence. Future requests must exclude model acquisition/full-weight/iterated-range disclosure and fix permitted units/cumulative scope. Claim-specific default and authenticated computation fraud retain distinct evidence and responsibility. Existing implementations, tests and old fences keep their historical scope; no new allocation or court restriction is activated here.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-10-03 at the operator's request; IMPLEMENTED the same day on `rcore/panel-liveness`, behind three
  dormant fences (`palw_panel_unavailable_expiry`, `palw_panel_fast_switch`, `palw_seat_availability`) — `None` on every preset,
  so no network's fingerprint, schedule or fold moves. Lane INT arms them in the DAA-5,300 list.
* Amends: ADR-0064 (silence is not observable — kept), ADR-0152 §S0′ (the second `ReceiptTimeout` / `NotReplayBacked` charge),
  ADR-0124 (seat draw weight), ADR-0160 E-4 (the obligation hold has no `PanelUnavailable` row).

## 0. The sentence this ADR is

An honest producer of a real model is never slashed because panel seats stayed silent: two panels that fail to conclude end the
claim with no reward and no charge, a silent first panel is replaced after `T_fast` instead of after the whole receipt window, and
seats that answer are drawn more often than seats that are new or gone.

## 1. The problem

Until now the second receipt timeout (and an S2 licence a second panel did not back by replay) charged the PRODUCER the whole
escrow-inclusive reservation, whoever caused the silence: the chain cannot observe silence (ADR-0064), seats are never charged, and
a permissionless seat Sybil pays nothing to cause it. Heavier models draw panels that are silent more often, so running them was
the more dangerous play and miners were pushed to the cheap base floor.

## 2. Part C — expiry is not fraud (`palw_panel_unavailable_expiry`)

Past the fence the second timeout, and the second panel's `NotReplayBacked`, end the claim `Voided { PanelUnavailable }`
(void reason, Borsh 10, appended): no reward, the reservation and bond lock return AT ONCE (no E-4 hold — nobody abandoned
anything), nothing is slashed, no strike is written, the seats get nothing, the fee is not refunded. The slash funnel
(`void_and_slash_at`) returns 0 for the reason whatever route reaches it. Every exhaustive match over the void reasons names it
(`palw_void_reason_keeps_obligation_v1` and its twin: false; the shadow's attribution: `Undetected`; the ledger name
`panel_unavailable`).

**Still slashable (producer fault, not touched):** a court loss (`CourtFraud`, `CourtDefault`, `CourtHeldVerdict`), an invalid
commitment, and an unanswered data-availability demand (`ProducerWithholding` through the DA court and its three-strike action
tier) — so withholding trace data to make seats silent is not free: a seat that cannot be served files a DA accusation, and the
producer's silence there convicts. AG-2's whole-bond forfeiture and the reputation strikes are triggered by convictions, never by
a void reason, so an expiry cannot reach them.

## 3. Part D — a fast switch (`palw_panel_fast_switch`)

**Deviation from the brief, recommended and implemented:** the brief asks for 5 primary + 2 standby seats whose receipts join the
quorum after `T_fast`. In this code base a seat's duty is a segment mask drawn from `seats.len()` (V2 verification: a full seat and
`K = seats − 1` partials, two attestations per segment), and `panel.seats` is read by ~60 sites (licence arms: plain, coverage,
optimistic, batch, supplementary V3, shard parts; counted-mask recount; false-valid and offence attribution; DA; node
producers). Seating standby seats inside the panel changes the assignment of every primary seat, and keeping the primaries'
assignment while admitting full-mask standby receipts needs every one of those sites changed and re-proved before the freeze.
The goal — a silent panel does not hold a claim for the whole window — is delivered with the machinery that already exists and
is already proved: the REDRAW.

Past the fence the first panel of a claim (`rebound_daa == None`) is swept at `bound + T_fast`, `T_fast = clamp(W_r(c)/20, 30,
W_r(c)/2)` — 30 DAA at the shipped 600-DAA window, 699 at the 2M class's 13,995; healthy quorums are 2–6 DAA at the floor, so
`T_fast` is 5x that and scales with the class's own verification window. The sweep revives the claim (the existing first-timeout
redraw: nobody charged, a fresh panel anchored on the sweep, SW-8's one-block bind) and the redraw gets the WHOLE window
(`T_hard = W_r(c)`), after which part C ends the claim. The sweep deadline is one function
(`PalwStateParamsV2::panel_bound_sweep_window_v1`) read by the bind's arm, DL-1, the load check and the DA re-arm. A claim's
receipt deadline for validity (the supplementary doors, Q-5's gate) is unchanged.

Cost and risk: a healthy-but-slow first panel loses its partial work at `T_fast` (not its claim); the node's backup-seat promotion
(120 DAA, `palw_seat_schedule`) is unchanged and simply becomes moot for a first panel that is replaced earlier. Standby seats are
phase 2 (§6).

## 4. Part E — positive liveness in assignment only (`palw_seat_availability`)

A per-bond row (`PalwSeatAvailabilityV1`: first credit, and per-epoch `assigned` / `credited` counters inside a rolling window)
in rooted, journalled state (delta variant `SeatAvailability`, number 105; carriage tail `0xE6`), written at exactly two places:
the panel bind (`reserve_seat_duties`: one assignment per seat) and the duty row's `0 → credited` edge
(`credit_seat_receipts`: one success). Constants: epoch 2,000 DAA, window 6 epochs (12,000 DAA); factor 0.5x with fewer than 3
credits in the window (new or inactive, including no row), 1.2x with at least 30 credits, at least 900‰ of the window's
assignments credited and a first credit two windows old, else 1.0x; integer permille. The factor scales the CLASS SEATS' race
weight (`max(1, ⌊w·f/1000⌋)`) in `palw_panel_stake_entries_availability_v1`; the SW-10 floor, the ADR-0147 outsider's race, the
security weight, slashing, fork weight and pay stay stake-only. Absence is never counted against a seat: it can only keep a seat
from 1.2x, or — by letting its credits leave the window — return it to the new-seat 0.5x.

## 5. Fences and prerequisites

| fence | prerequisites (at or below) |
|---|---|
| `palw_panel_unavailable_expiry` | `palw_audit_2026_09_23` |
| `palw_panel_fast_switch` | `palw_panel_unavailable_expiry`, `palw_rcore_plus` |
| `palw_seat_availability` | `palw_rcore_plus` |

Each is a bare height with a borsh-skipped mirror in `PalwStateParamsV2`, Some-only in the params and schedule ids with the
`never()` collapse, on the fork-id list, in `PALW_T12_PANEL_LIVENESS_FENCES_V1` (the flag-day entry), and moved together by
`--palw-drill-panel-liveness-at`.

## 6. Open

* Standby seats (the brief's D) as a phase 2: needs the seat-view helper at the ~10 mask/membership sites and a decision on the
  pay denominator (`max(primaries, credited)`), plus capital for 7 seats at bind.
* `T_fast` and the availability thresholds are constants chosen from the 10-02 healthy figures; the adaptive form is stage 6.
* An S2 licence's second `NotReplayBacked` now also expires uncharged: a producer colluding with partial seats to license a false
  claim loses only the reward unless a court convicts (the colluding seats' `Valid` locks stay exposed to `PanelFalseValid`).

## Mission alignment amendment — 2026-10-07

verifier unavailabilityをproducer fraudとしない決定を維持する。licenseした偽claimは、不在timeoutではなくpublic objective courtで責任を問う。Panel redrawや期限切れで、外部proofの独立した受理・証拠保持を消さない。

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
