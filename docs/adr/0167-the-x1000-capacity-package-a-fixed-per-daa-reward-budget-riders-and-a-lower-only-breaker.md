# ADR-0167 — The ×1000 capacity package: a fixed per-DAA PALW reward budget, riders, a lower-only breaker, and the ρ = 250 / ρ = 1000 steps

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: implemented on `rcore/cap-1000` (testnet-12 only, int-11 release, H = 5,300). Supersedes nothing; completes stages 5, 6 and 7 of
ADR-0160 v3 (§8, §9, §11.4–11.6). Directive: the user, 2026-10-03 10:50 JST ("the claim ×1000 capacity change ships in the DAA-5,300
flag day release").

## 0. Summary (日本語)

claim 数が増えても 1 DAA あたりの発行量が増えない仕組み(F-EM: 1 DAA の PALW 報酬予算 = 16 ブロック分の carve)、1 ブロックの carve を
リード+最大 64 本のライダーで分け合う多重 claim(F-M1、object tag 95)、エポック単位で発行層を下げる一方向ブレーカー(F-K)、および
ρ = 250(H+190 = 5,490、`k_aud` = 2)と ρ = 1000(H+285 = 5,585)の段。**ネットワーク全体の処理量は ρ に比例して増えない**: 律速は検証供給
(`L_ver`)で、RFC-0006 のレイヤーシャーディングが per-claim の seat 作業を約 5 回の full replay から約 2 相当に減らす分(×2.5)だけ
ρ ≥ 250 の段で `L_ver` を 435 → 1,087 に上げる。それ以上は計測された供給が出るまでブレーカーが門番(§7)。

## 1. Fences, heights, wire allocations

| entry | field | prerequisites (at or below) | height on testnet-12 |
|---|---|---|---|
| F-EM | `palw_capacity_emission_budget` | F-L, F-E | H = 5,300 |
| F-M1 | `palw_capacity_multi_claim` | F-EM, F-B, F-N, F-S, F-W, F-Q | H |
| F-K | `palw_capacity_rho_breaker` | F-L, F-S, F-Q | H |
| ρ = 250 | F-L step 4 (`…_step_4`) | F-EM, F-M1, F-K | H + 190 = 5,490 |
| ρ = 1000 | F-L step 5 (`…_step_5`) | F-EM, F-M1, F-K | H + 285 = 5,585 |

Each fence is written in the four places (`Option` field, `for_each_fence`, Some-only id writes, the `never()` collapse), mirrored into
`PalwStateParamsV2` (`sync_palw_capacity_s567`), validated by `validate_palw_capacity_s567_v1` (a ρ ≥ 250 step without all three at or
below its height is refused), and has a drill mover (`--palw-drill-capacity-emission-at`, `-multi-claim-at`, `-rho-breaker-at`,
`-step4-at`, `-step5-at`; `--palw-drill-int11-at` moves the whole list, ρ = 100 / 250 / 1000 at +95 / +190 / +285). Every other preset
is dormant. Wire: object tag **95** (`AttemptRidersV1`), delta entry (`CapacityLedger`, the last variant: 101 on this branch, 105 in INT's merged order),
carriage tail **0xBB**, root block `capacity_s567/v1` (Some-only).

## 2. Stage 5 — emission

A claim's escrow is a carve (a permille) of the subsidy of the block that carried it; the coinbase withholds exactly the claims' recorded
escrows. So emission per DAA is the carves of that DAA's claim-bearing blocks, and a lane that adds claims by adding blocks adds emission.

**D1 (budget).** Past F-EM, an attempt claim's acceptance charges its block's whole carve (1,000 milli-carves) to a rooted ledger row
keyed by the accepting block's DAA; the DAA's budget is `PALW_EMISSION_BLOCKS_PER_DAA_V1` = 16 carves (live t12 measured 5.3 claims a
DAA, so 3×). A claim that would pass it is refused (`EmissionBudgetExhausted`: non-fatal, the block's own attempt skipped, its carve
withheld and burned as every skipped attempt's). Admission, the fold and the producer's readiness read the one ledger. The ledger holds
one row (earlier days leave at the next charge).

**D2 (riders).** A lead may take up to 64 riders (same bond) in one `AttemptRidersV1` object. The fold queues the object at step 3 and
takes it at step 4b′ (after the block's work, so a merged lead exists): the lead must be `Provisional`, accepted within 8 DAA, not
already split; each rider is a full admission (`check_palw_attempt_admission_v2`, then `apply_attempt`) **at a share**: its escrow is
`⌊E_lead/(1+n)⌋` — the rider's subsidy is the least that carves exactly that — and exactly that amount leaves the lead's escrow (the
lead's commitment is released and re-reserved at what it keeps, first, so the riders meet the ceilings with the room freed). The whole
batch is atomic (one refusal restores the builder). Σ escrow of a block's claims is the lead's carve to the sompi; a rider charges the
budget nothing. A rider has no header: its challenge is `H(lead ‖ index)`, its job anchor `H("anchor" ‖ lead ‖ index)` (on-chain data, so
the court re-derives its job), its DA pins its lead's, its price the collateral every claim costs (slot, bucket, J-1, room) — the
per-bond fair share (F-N) and slots (F-S) bound it exactly as they bound any claim.

**Proof obligations (tests, `palw_capacity_stage567.rs`):** the budget refuses the 17th claim-bearing block of a DAA at every ρ and holds
one ledger row; a lead and n ∈ {1, 2, 7, 31, 63} riders hold exactly the lead's carve; a block's carve total is unchanged by riders on
every lead of a full DAA; a refused batch (wrong bond, foreign challenge, a bad second rider, a missing/bound/late lead, a second
batch, dormant fence) leaves the lead and the commitments as they were.

## 3. Stage 6 — the lower-only breaker (F-K)

One rooted row of per-epoch counters and a level ℓ ∈ 0..7 into Λ = [1, 10, 25, 50, 100, 250, 500, 1000]; the issuance tier is
`min(schedule ρ, Λ[ℓ])`, so it never exceeds the flag-day schedule. Epochs are the aligned 1,000-DAA spans; at the end of the first
chain block at or past a boundary (after its own work, admitted at the parent's tier) the epoch is judged and its counters reset.

Counters (written where the event is written, from on-chain facts only): `void_claim` (receipt voids, panel-backlog voids), the first
licence (the receipt denominator; credited licences), the panel bind (the backlog denominator), `close_conviction_v1` (nominal vs
collected, false audits). K6 (overdue audits) is counted over the claims at the boundary.

| metric | trips when | verdict |
|---|---|---|
| K1′ false audit | ≥ 1 conviction of a claim holding audit receipts | severe: level 0 |
| K2 slash shortfall | ≥ 10% of Σ nominal and ≥ 3 distinct bonds (bonds already forfeited whole excluded) | one rung |
| K3 receipt-deadline overrun | ≥ 5% of ≥ 20 claims reaching their receipt deadline | one rung |
| K4 panel backlog | ≥ 5% of ≥ 20 claims reaching their anchor | one rung |
| K6 audit service | ≥ 20 credited licences past 60 DAA unaudited and ≥ 5% of the epoch's credited licences | one rung; ≥ 50%: level 0 |
| B1 (per bond) | ≥ 20% of ≥ 10 of the bond's claims reaching a deadline ended by a producer-attributable outcome (a data-availability default); never an expiry | the bond's own level one rung |
| B2 (per bond) | any conviction of the bond | the bond's own level one rung |

A trip lowers one rung below the tier in force; a level rises one rung after two clean epochs (every metric under half its threshold; an
empty epoch is clean); never above the ladder, never above the schedule. K3/K4 read the NETWORK's expiry and backlog rates (a receipt-deadline expiry is the verification supply's, and lane PL's rule in the same
release charges no producer for it), and lower the network tier only; a bond's own row (B1, B2) counts only proven producer outcomes —
convictions and data-availability defaults — never expiries.

**Scope of the effect — why the issuance tier alone suffices for pricing.** The breaker lowers the tier read by the issuance slots
(`N_out`, refill, burst: `u·ρ_eff`), the per-bond rows and — through §7 — the verification supply. It does not re-price claims already
accepted (there is no per-claim price record; a claim keeps its price, ADR-0160 §5) and does not change the credit's discount for new
claims. The aggregate exposure bound is what matters: a bond's uncovered reward across its live claims is at most
`N_out × (E − m_c)`, with `m_c = ⌈E/ρ_step⌉`; lowering `N_out` to `u·ρ_eff` multiplies that bound by `ρ_eff/ρ_step ≤ 1`, so the breaker's
output is never riskier than the schedule's, and at level 0 a 13,000 BILI bond holds at most u = 2 claims whatever the discount. A credited
claim reaches `Final` only through `k_aud` audit receipts (D-23), so the discount is not what a fraud walks through. Re-pricing new
claims at the lowered ρ would need a per-claim price record (a claim-record schema change, i.e. a state-version fence); it is named as
the follow-up if the breaker ever trips for a reason the issuance bound does not answer.

## 4. Stage 7 — ρ = 250 and ρ = 1000

Ready entries (`PALW_T12_CAPACITY_RHO250_STEP_4_V1`, `…RHO1000_STEP_5_V1`) at 5,490 and 5,585. `k_aud` = 2 from ρ = 250 (already in the
audit door). `N_out` for a 13,000 BILI bond: 500 at ρ = 250, 2,000 at ρ = 1000 (ADR-0160 §9.1's last row: 2,030 with the 6,500 BILI unit
rounding). The per-bond ceilings and the per-bond burst scale linearly in collateral at every ρ of Λ (property test over 2,000 random
splits per ρ), so splitting a bond never gains slots, refill or burst; 13k × 10 bonds never admits more than 130k × 1 (fold-level test at
both tiers). 2M keeps its C7 cap of 1 (ADR-0153).

## 5. The six invariants at ρ = 250 and ρ = 1000

HONEST-NO-LOSS, J1-CAP (with the budget and a rewind), LIABILITY-SURVIVES, NO-FREE-VOID, REORG-DETERMINISM (a tape with riders and an epoch
boundary, replayed and rewound) and SPLIT-NEUTRAL are `palw_capacity_stage567.rs`, folded through testnet-12's own transition.

## 6. What binds the network, with arithmetic

Per-bond capacity is ×1000; the network's is whatever its limiters pass. Each from code:

* **`L_ver`** — unlicensed claims the seats can verify inside half the receipt window: `⌊μ_floor · W_safe⌋ = ⌊1.5 · 290⌋ = 435`.
* **`L_seat`** (the floor's J-6 pipeline plus its bound claims) ≈ 1,200 at the live mix; **`L_carry`** = 21 · 64 · max(1, ā_op) ≈ 3,500;
  **`L_anchor`** = 100 · ā_op · 20 ≈ 5,200. `L_net` = their minimum, then capped by `L_ver`.
* the **audit backlog** `A_max` = 4,800 credited licences awaiting audit (a DoS cap; it binds before 1,000 claims a DAA at any audit latency).
* **emission** — now invariant to the claim count (§2).
* **not built** (named, not reachable by a fence of this release): M-0a (the fold clones the state per merged attempt) and M-0b (the root
  re-hashes every collection each block, O(state)); at tens of claims a DAA they are the next wall. The riders object adds one
  checkpoint per batch.

## 7. Network throughput must rise with ρ: the verification supply, stepped with the tier

`L_ver` is a verification-supply term, so it steps with the verification supply this release adds, not with ρ:

* **RFC-0006 (layer sharding)** cuts a claim's seat work from about five full replays (k = 2 panel + audit + court margin) to about two
  replay-equivalents: the same seats verify 5 / 2 = **2.5×** the claims in the receipt window → `μ_floor` 1.5 → 3.75 licences a DAA,
  `L_ver` 435 → **1,087**.
* **RFC-0007 (tally licensing)** cuts per-claim signature carriage from 14.4 KB to 66–330 B (44×–218×). That relieves **`L_carry`** (and
  block mass), not `L_ver`; `L_carry` already exceeds `L_net`, so it is not stepped.

`palw_verify_supply_milli_v1(tier)` is 1,000 below ρ = 250 and 2,500 from it; `network_level_v1` multiplies `L_ver` by it using the
**issuance tier** (`min(step ρ, breaker level)`), so the supply counts only while the tier is ≥ 250 and **the breaker takes it back
the epoch a backlog, a receipt-deadline overrun or a false audit trips** (lower-only, epoch granularity). The ρ steps are the ready
entries; no separate fence is added, so a node cannot hold the step and not the supply. If the fleet's measured verification supply is
below the stepped `L_ver` the schedule still names it and the breaker is the gate: a queue that outruns verification shows as K3/K4/K6
and lowers the tier, `L_ver` and the slots together. **Consequence, stated plainly:** the network-wide queue rises 435 → 1,087 (×2.5) from
ρ = 250, not ×250 or ×1000; claims a DAA ≈ queue / (bind + wait ≈ 24 DAA) ≈ 45 against ≈ 18. ×1000 per bond is real (the ceilings), ×1000
for the network needs a verification supply two to three orders larger than RFC-0006 and RFC-0007 deliver, which a later step of this
table adds once it is measured.

## 8. Node side

`--palw-riders=N` (0–64, default 0): after each lead block the producer executes N further jobs of its bond under the riders' derived
anchors (the same backend, memory ledger and retention as a lead), signs them, and hands one `AttemptRidersV1` to the panel's carrier lane
through `palw_rider_outbox`. Producer readiness refuses to mine into a spent DAA budget (`PALW_NOT_READY_EMISSION_SPENT_V1`). The audit
duty is unchanged and reads `k_aud` from the claim's own ρ.

## 9. Open risks

* No drill has crossed 5,300 / 5,490 / 5,585 with riders on a live fleet; the lead's one combined drill is the first.
* A rider carrier landing before its lead is accepted is a skip (the lead keeps its carve); the producer does not retry.
* The first step at ρ = 250 is a flag day for the audit pool: `k_aud` = 2 doubles the operator replays per credited claim.
* K3/K4 count every expiry as the network's, including a single producer's own silence; with lane PL's rule that producer is not charged,
  and a lone silent bond cannot reach the 20-claim floor of a ratio metric.

## Mission alignment amendment — 2026-10-07

capacity倍率、rider数、per-DAA報酬budgetの増加は、外部public prosecutionのbytes/work/deadline/aggregate exposureが収まることも受入対象にする。Panel supplyやbreakerの数字だけでこのgateを代替しない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

本ADRの実装記録はρによるslot/予約調整、全体emission budget、rider分配とbreakerであり、全producer経路の報酬・block・Final weightを一律1/1000にした保証ではない。
将来の容量×mは一claimの権利を細分化/固定予算から配分し、同額bond・同期間のblock/reward/Final weight総上限を維持する。共通最早回復時計と全reader/undoを追加し、旧packageのtest PASSを新保証の達成に流用しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
