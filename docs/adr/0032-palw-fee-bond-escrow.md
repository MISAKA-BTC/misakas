# ADR-0032: PALW fee-bond escrow — pricing calls and paying challengers without new covenants

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Accepted (design; activates nothing).** This decides HOW opening-call fees and
challenger bounties work — the piece ADR-0029 §7 deferred "needs the bond-UTXO covenant
discipline". The decision is that Stage 1 needs **no new covenant machinery at all**, and
Stage 2's escrow reuses the one covenant discipline the chain already enforces: the
consensus-recognized bond-UTXO spend gate.
Date: 2026-08-16
Relates to: ADR-0028 §4/§5 (the economics being carried: `(1+q·ρ_v)·base` issuance split,
fee-bonded audit calls, no-show slash floor = fee×100, challenger economics rivalrous by
construction), ADR-0027 §4 (slash allocation, challenger bounty ≤ 49 % under the 2026-10-10 amendment, `slash_id`
idempotence), ADR-0016/0017 (stake-locked bond UTXOs — the existing spend-gate discipline),
ADR-0029 §2 (the no-outputs rule reserving the `(tx_id, 0)` reporter slot).

## Premises

* **No general covenants exist and none are being invented.** This chain's only
  outcome-conditioned spending is the bond spend gate (consensus rules that refuse to spend
  a recognized bond UTXO outside its lifecycle). Any escrow that needs "spendable only on
  outcome X" must be THAT mechanism, not a new script capability.
* **A fee's job is to price denial-of-service, not to fund the system.** ADR-0028 already
  decided verification funding is an issuance split; the call fee only has to make spamming
  opening calls cost more than answering them costs the answerer.
* **A bounty's job is promptness, not security.** Security is the permissionless window
  (ADR-0028 §4); the bounty rewards whoever moved first, capped so slashing never becomes a
  profit center that invites manufactured offenses (≤ 49 % under the 2026-10-10 amendment,
  additionally bounded by `B_cap` in this credit-overlay flow).

## Decision

### Phase E1 (Stage 1) — fees are fees, bounties are consensus credits

1. **Opening-call fee = the transaction fee itself, made mandatory.** A Stage-1 opening-call
   transaction must carry `tx_fee ≥ F_call` (a registered network fact, placeholder until the
   economic simulation gate — ADR-0028's discipline for every such number). It is not
   refundable and not escrowed: the DoS pricing is achieved the moment the caller burned it
   to ordinary fee processing, with zero new state. An answered call costs the answerer one
   replay; an unanswered call within `W_answer` is the miner's `DATA_WITHHOLDING` — the fee
   does not need to move for either outcome to hold.
2. **Challenger bounty = a consensus credit at slash execution.** When a refutation's slash
   executes (Stage 2+), the slash transaction credits `min(49 % · slashed, B_cap)` to the
   `(tx_id, 0)` slot of the refutation-carrying transaction — the slot ADR-0029 §2's
   no-outputs rule reserved so this would never be a retrofit. The remainder burns. Dedup is
   `slash_id` idempotence (ADR-0027 §4): one offense, one bounty, first-accepted refutation
   wins — rivalrous by construction, exactly ADR-0028 §4's challenger economics.
3. **No-show slash floor** stays fee-denominated (`≥ 100 × F_call`, ADR-0028 §4's placeholder)
   so griefing-by-silence has negative ROI at any fee level.

Phase E1 requires: the fee minimum in the Stage-1 opening-call admission validator, and
nothing else. No escrow store, no covenant, no refund path.

### Phase E2 (Stage 2+) — the audit-call bond, as a bond

ADR-0028 §5's fee-BONDED audit call (the DA heartbeat) needs more than a burned fee: the
caller must stake something forfeitable if the audit is abusive, refundable if the answer is
late. That is a **bond lifecycle**, so it uses the bond machinery:

* An **audit-call bond UTXO** — the ADR-0016 pattern with a new recognized bond class
  (`AUDIT_CALL`), minimum value `F_audit`, and a spend gate keyed to the call's outcome
  window: spendable by the caller after `W_answer + settlement` if the answer never came
  (the miner's offense stands and the bond returns), spendable INTO the slash flow (burn +
  answerer compensation) if the call was answered and the caller abandoned it, unspendable
  before resolution. The gate reads the same Stage-1 carriage store the duty logic reads —
  the store is the oracle, the spend gate is the covenant, both already exist as disciplines.
* **What is deliberately refused**: escrow inside payloads (value must be UTXO-visible or
  the mass/UTXO accounting lies), third-party escrow agents (a trusted party in a BFT-free
  design), and per-outcome script predicates (a new covenant language for one use case).

### Numbers

`F_call`, `F_audit`, `B_cap`, and the no-show multiplier are **economic-simulation-gated**
(ADR-0028's rule: shipping placeholders as measured is a §15-class violation). This ADR
fixes the mechanisms and the flow of value; B15's simulation fixes the values.

## Consequences

* Stage-1 carriage needs exactly one new admission rule (the fee minimum); the reporter slot
  and dedup discipline it depends on already exist. B10's "未設計" is closed as design.
* The Stage-2 audit bond adds a bond class, not a covenant system — implementation rides the
  same rails as every bond change (ADR-0016 lineage), with its own drills.
* Bounty value flows are auditable on-chain by construction: burned remainder, credited slot,
  `slash_id` — no off-chain settlement anywhere.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## PALW challenger share amendment — 2026-10-10

PALWの摘発報酬率を**10%から49%（4,900 bps）**へ改定する。Phase E1の式は
`min(⌊49 × slashed / 100⌋, B_cap)`、残額はburnとする。`slashed`は実際の回収額であり、
名目Slash額・未回収額を支払財源にしない。`B_cap`、返却条件、`slash_id`による重複排除は維持する。
[2026-08-16の経済記録](../palw-economic-parameters-2026-08-16.md)の`B_cap = 2,000` coin候補も据え置く。
率の変更を理由に上限を9,800 coinへ引き上げない。金額は整数sompiで切り捨てる。

現行V2のR-core報酬は、上記の旧credit-overlayとは別の会計である。
[`PALW_RCORE_REPORTER_REWARD_BPS_V1`](../../consensus/core/src/palw_state_v2.rs)を4,900とし、
`R = ⌊4,900 × max(0, collected − X) / 10,000⌋`を用いる。
`collected`はconvictionのtierが実際に徴収した額、`X`は既に抽出された利益を補填する控除である。
aggregate-liabilityによる追加forfeitureは報酬基準に含めない。R-coreに旧`B_cap`は存在せず、
この改定で旧credit-overlayのcapや支払先をR-coreへ移植しない。commit–reveal、named reporter、
vesting、forgone reward、one-offence/one-rewardの規則も維持する。

R-coreのDA-6 exposureは既存の同率規則に従い
`min(⌈4,900 × S_P(stage) / 10,000⌉, min_collateral_sompi)`へ連動する。
これは報酬の切捨てと異なり切上げであり、既存の担保上限を維持する。
DNS/PoSのreporter rewardと4-way splitは別会計であり、この改定のPALW率には連動しない。

自己摘発の還流を考慮しても、旧capped flowの純損失は少なくともSlash額の51%である。
R-coreでは`ΣS > G_res`の条件下で、Final後の純収支は
`−0.51 × (ΣS − G_res) < 0`、Final前は`−0.51 × ΣS < 0`となる
（整数丸めは受領額をさらに減らす）。既存の担保・期限・回収可能性の前提は維持する。
これは限定した自己摘発の会計条件であり、49%の経済安全性全体の測定や監査を代替しない。

この改定はソースのPALW率を変更するが、ネットワークのfork heightやdeploymentを有効化しない。
R-coreが設定されたrulesetの`consensus_params_id`にはPALW率をcommitし、
10%と49%のbuildが同一rulesetとしてhandshakeしないようにする。
既存chainへ適用するには過去の10%会計を再解釈しないversioned移行が必要であり、
以前の10%による測定・activation記録を49%の結果として扱ってはならない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

producer bondは不正発覚後の徴収だけでなく、共通DAA期間内のclaim・block・reward・Final weight機会を制限する。受理時の権利予約、d+Wまでの早期回復禁止と残存責任担保を分離する。
escrow予約額の引下げは公開検証・court/DA費用・実徴収可能担保・共通拘束と一体で評価し、未払rewardの実資金を架空の新しい発行枠にしない。既存49%改定、旧claim会計と過去の実測は保持する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
