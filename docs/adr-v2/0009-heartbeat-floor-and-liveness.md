# ADR-v2-0009 — Heartbeat・BASE floor・停止回復・liveness

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 7本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md) / [RFC-v2-0013](../rfc-v2/0013-permissionless-bootstrap-and-release.md)

## 1. この文書の役割

通常の仕事が止まったときのclock回復を、通常時の報酬・難易度・担保責任から分ける。

## 2. 統合した判断の読み方

### 2.1

livenessを少数の既存producerだけに依存させず、heartbeatとbonded workの役割を分離する。停止回復・非稼働記録は合意上の状態とnode内メモリを区別する。[旧ADR-0060](../adr/0060-the-liveness-doctrine.md) [旧ADR-0064](../adr/0064-trustless-recovery-from-a-total-stop.md) [旧ADR-0066](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)

### 2.2

heartbeatは通常生産中の経済・difficultyを不用意に変えず、bonded blockの評価やclockに対する透過性を適用規則に従って扱う。[旧ADR-0105](../adr/0105-a-heartbeat-never-turns-a-bonded-block-red.md) [旧ADR-0140](../adr/0140-the-heartbeat-is-the-emergency-generator.md)

### 2.3

livenessを構造で支え、collateralは不正責任を担うという分離と、BASEをidle時のreserveにする判断を継承する。[旧ADR-0151](../adr/0151-liveness-is-structural-collateral-covers-fraud.md) [旧ADR-0165](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md)

## 3. 維持する境界・後続改定

旧ADR-0064の回復用即時利用案と、後続の通常bond成熟・genesis bond廃止を同時に一般化しない。新genesisの起動は[ADR-v2-0010](0010-genesis-emission-and-bond-free-bootstrap.md)のADR-0209が対象を明示する。

## 4. 隣接する判断との分担

合意clockは[ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md)。回復laneが動くことだけで新genesisから報酬・bond登録まで通ると判断しない。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0060-the-liveness-doctrine.md](../adr/0060-the-liveness-doctrine.md) | L1–L338 | L17–L35 |
| [0064-trustless-recovery-from-a-total-stop.md](../adr/0064-trustless-recovery-from-a-total-stop.md) | L1–L240 | L14–L19 |
| [0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) | L1–L279 | L12–L15 |
| [0105-a-heartbeat-never-turns-a-bonded-block-red.md](../adr/0105-a-heartbeat-never-turns-a-bonded-block-red.md) | L1–L558 | L12–L32 |
| [0140-the-heartbeat-is-the-emergency-generator.md](../adr/0140-the-heartbeat-is-the-emergency-generator.md) | L1–L248 | L12–L15 |
| [0151-liveness-is-structural-collateral-covers-fraud.md](../adr/0151-liveness-is-structural-collateral-covers-fraud.md) | L1–L383 | L17–L29 |
| [0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) | L1–L438 | L17–L17 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0060-the-liveness-doctrine"></a>

<details>
<summary>0060-the-liveness-doctrine.md — 原文 L1–L338</summary>

[原文を開く](../adr/0060-the-liveness-doctrine.md) · SHA-256: `3a595b4f615c7be2c9b7bacc94d1864ec53a076fbb3051f01084edf99b21e753`

<!-- ADR_SOURCE_BEGIN:0060-the-liveness-doctrine.md -->
~~~~~~~~markdown
# ADR-0060: The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


- Status: Accepted as doctrine; **implemented 2026-08-30, SHIPPED OFF the same day, and
  RE-IMPLEMENTED 2026-08-31 per ADR-0066** — the mainnet audit
  (`docs/palw-mainnet-audit-2026-08-30.md`) found four structural defects in Decisions 1–2 and one
  in Decision 4. ADR-0066 closes F1, F3b and F4 (the lane's price left `header.bits`; the evidence
  walk is deleted) and records F3a as open and F2 as staged. **Both features still ship OFF, but
  they are now `Params::palw_heartbeat` and `Params::palw_inactivity_leak` — top-level fences an
  operator can arm on a running network, not constants that need a rebuild.** The switches this
  status line used to name are gone: `PALW_HEARTBEAT_LANE_ENABLED` no longer exists and
  `DnsParams.inactivity_leak_daa` is retired at `u64::MAX` permanently. §12 records what must
  change before either turns on; ADR-0066's "What landed" section supersedes it for D1/D2. Decisions 3 and 5 are unaffected and live. Originally: **implemented 2026-08-30** (`palw-genesis-10b-cap` branch, same re-mint
  train as ADR-0059) — see §11 for what landed and where the implementation amended this text.
  The one operator-tunable decision is the finality leak's time constant (§6).
- Date: 2026-08-30
- Depends on: ADR-0038 (PALW is consensus work), ADR-0045/0046 (class economy; carriage),
  ADR-0054 (share follows production; permissionless admission), ADR-0056 (share economy),
  ADR-0058 (merged work is counted), ADR-0059 (the 10B premine cap)
- Amends: ADR-0038's pure-PALW production stance and testnet-11's published "there is no hash
  lane to fall back to" doctrine — a *bounded, near-weightless* hash lane re-enters, as the
  chain's clock and nothing else.

> **Arming changed after the Status line above was written (index reconciliation, 2026-09-02).**
> "Both features still ship OFF" was true of the Phase 1 build. [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
> Phase 2 (Relaunch 5, `palw_rc_arm_phase1`) arms `Params::palw_heartbeat` **from genesis** on
> testnet-11, on devnet, and on any carded mainnet; only `Params::palw_inactivity_leak`
> (Decision 4's fence) is still `None` on every preset. Decision 1's lane is therefore live on
> testnet-11 in [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)'s
> form — `algo_id = 8`, fixed target, ε = 1 blue work against 2²⁰ per attempt block
> (`Params::palw_attempt_work`, ADR-0068 F2), width bound "≤ 4 heartbeats or one chain" per
> mergeset (ADR-0068 F5) — and Decision 2's ramp is the two-step form ADR-0066 §"What landed"
> describes. Map: [`README.md`](README.md).

## 1. The failure family, measured

Every liveness failure this project has actually had is one defect wearing four costumes:

* **The exposure wedge** (testnet-12, block 600): the only bonds filled their exposure
  ceilings; releasing a claim requires DAA to advance; DAA advances only when blocks are
  produced; only those bonds could produce. Held forever, by arithmetic.
* **The floor-producer stall** (testnet-11 operations): with no floor producer running, the
  DAA stops and cannot recover on its own — the chain waits for an operator.
* **The quarantine runaway** (chain-participation switch counter): a refusal gate whose
  predicate feeds itself, with no reset path — a synced node becomes permanently unable to
  participate.
* **The virtual wedge** (testnet-10, 81 hours): the DNS reorg gate refused every heavier
  candidate; nothing inside the chain could expire the refusal until a TTL was retrofitted.

And the bootstrap variant: bonds are required to produce, bond registration rides a
transaction, a transaction needs a produced block — so a chain born with zero bonds can never
produce block 1 (this is why ADR-0059 still ships genesis seats — six then, eight since
2026-08-31, so that ADR-0065 D1 can be armed).

The shared root: **the chain's clock — DAA advancement, timeout sweeping, gate expiry — was a
hostage of the very actors that were stuck.** Bonds can die (slashing, exhaustion, exit,
court error); a clock must not.

## 2. The doctrine

> **Time is supplied by a permissionless lane. Weight is supplied by bonds. Finality is
> supplied by validators. None of the three can stop the other two, and every refusal gate
> decays on the clock that always runs.**

Concretely: the accountability apparatus (bonds, panels, courts, quorums) exists to make
*weight and irreversibility* expensive to fake. It must never be what *time* depends on,
because accountability implies killability. A hash proof is the one kind of work that is
self-verifying — no claim, no panel, no court, therefore no bond — so it is the one lane that
may run permissionless, and the only thing it is allowed to sell is time.

## 3. Decision 1 — the heartbeat lane (new)

A bondless hash lane, re-enabling the existing BLAKE2b-512 ∥ SHA3-512 Layer-1 (`algo_id = 3`
— the lane testnet-11 deliberately switched off) under four bounds:

1. **Bondless, claimless.** Admission requires a valid hash proof and nothing else: no bond,
   no PALW claim, no escrow, no lifecycle objects. There is nothing to slash because there is
   nothing to lie about.
2. **Near-weightless.** A heartbeat block contributes a fixed constant `ε` to blue work,
   *independent of its difficulty* — an explicit, named exception to ADR-0045's DerivedV1
   work equality. Consequence: any amount of hash power loses fork choice to a single bonded
   PALW block; among heartbeat-only branches (total collapse), `ε × n` still orders longer
   chains first. An ASIC farm pointed at this lane buys cadence-capped, weightless blocks —
   i.e. nothing.
3. **Cadence-capped.** Nominal 33‰ of cadence (one block per hour at the 120 s cadence,
   ≈ 24/day). *Amended at implementation:* the cap is NOT a share-table entry — the share
   table is class-granted chain state and the heartbeat is deliberately not a class — but a
   **slot rule** (at most one heartbeat per interval, measured against the POV's youngest
   heartbeat in chain order) plus the lane's **own windowed retarget** toward one block per
   interval, floored at ~2²⁴ hashes so sibling spam is never free. The slot rule is the hard
   cap; the retarget is the price.
4. **Fee-only.** No subsidy, no worker/escrow split (there is no claim to escrow against).
   The 25B supply of ADR-0059 is untouched to the sompi. Running a heartbeat miner costs one
   CPU thread; it is a public good in calm weather and self-rewarding in a crisis, when every
   queued transaction's fee rides it.

Heartbeat blocks are otherwise ordinary blocks: they carry transactions — which is the entire
point, because bond registration, unbond, collateral funding and lifecycle transactions are
what ride them when no bonded lane is alive.

### What implementation added to Decision 1

* **A heartbeat chain block commits the parent PALW state root** like the bonded lanes
  (`palw_state_root` hash-visible on algo-3; every historical algo-3 header carries the zero
  root, so no identity moved): the doctrine's own regime is days of heartbeat-only chain,
  which must not be days of uncommitted state. Closing that surfaced a pre-existing gap —
  nothing refused a stuffed `palw_state_root` on any non-committing lane (hash-invisible
  bytes = block-identity malleability, the `NonPalwHeaderCarriesPalwCommitment` class through
  the sibling field) — now refused at the door (`UncommittedPalwStateRoot`).
* **The evidence source is a chain-order walk, not the difficulty window.** The integration
  test caught the slot rule waving a second heartbeat through: the difficulty window is
  SAMPLED, so the newest blocks — exactly what the slot rule is about — can be absent from
  it. The rules read a bounded selected-parent-chain walk (`processes::heartbeat_evidence`),
  which also makes back-dating a heartbeat useless: the youngest heartbeat is the nearest by
  chain distance, whatever timestamp it stamped.
* **The miner is one flag**: `kaspad --palw-heartbeat-miner-address=<ML-DSA-87 addr>` runs
  the bondless in-node miner (template → `heartbeat_adapt_block_template` → slot wait →
  one-thread grind → submit). Fee-only, gated to ConsensusV2 networks.

## 4. Decision 2 — the emergency ramp (new)

24 blocks/day is a heartbeat, not an ambulance. The shipped lifecycle windows sum to 6,000
DAA (600 bind + 600 receipt + 1,200 challenge + 3,000 court + 600 abandon-hold): at full
cadence that horizon sweeps in ≈ 8.3 days, but on the heartbeat alone it would take **250
days** — and the per-class epoch retarget cannot help, because an epoch is 1,000 DAA, which
at 24/day is 41 days away. The ramp therefore keys on **timestamps, not epochs**:

* The heartbeat's *target rate* is a deterministic step function of bonded-lane silence,
  measured as `now − timestamp(last block from any bonded lane)` over chain-observable data:
  nominal **1/hour**; silence > 1 h → **1/10 min**; silence > 6 h → **1/120 s** (full
  cadence). When a bonded lane produces again, the rate steps back down the same ladder.
* Inside the chosen rate, per-block difficulty follows an ASERT-style schedule on parent
  timestamps, so the lane holds its rate through hash-rate swings without waiting for an
  epoch boundary.

With the ramp, a total-collapse recovery sweeps timeouts at near-normal speed: the bind
window in ~20 hours, the full exposure horizon in ~8–9 days — instead of 25 and 250 days.

Timestamp manipulation is bounded by the existing median-time-past and future-drift rules,
and blunted by Decision 1.2: easing this lane's difficulty wins weightless blocks at a capped
rate and nothing else.

## 5. Decision 3 — producer-bond self-healing is now unconditional (mostly landed)

The producer side of the doctrine is largely built; what it lacked was the clock. On record,
as one list:

* Collateral is **derived, not declared** (`palw_v2_collateral_for_claim_lifetime_v1`, the
  "+1" lesson of the block-600 wedge), and re-derived from the dearest registered class.
* Exposure releases on timeout sweeps; **with Decision 1, sweeps run on a clock no bond can
  stop** — this is what turns "the wedged party must produce the releasing block" from a
  deadlock into a delay.
* Share follows production (ADR-0054), merged work is counted (ADR-0058), admission is
  permissionless (ADR-0054/0056), post-genesis bond registration rides an ordinary
  transaction (the carrier), and the post-genesis collateral floor is deliberately thin
  (`min_collateral_sompi` = 400,000 sompi): the economic re-entry door is always open, and
  with Decision 1 there is always a block for the re-entry transaction to ride.

No new mechanism is needed on this layer. (A collateral top-up path — extending headroom
without re-registering — remains a nice-to-have, deferred.)

## 6. Decision 4 — the finality inactivity leak (new; overlay-scoped)

Wherever the finality overlay (VLT) is enabled: a validator that has not attested for
**T_leak** — proposed at the DAA-equivalent of **7 days**, measured on the always-advancing
clock — is **excluded from the quorum denominator**. Its bond is *not* burned; it re-enters
by attesting again under the normal (re)bond rules. The remaining active set therefore
re-forms quorum on its own, and finality self-heals instead of halting until an operator
re-bonds by hand.

**The trade-off, stated plainly:** a partition longer than T_leak can produce two finalized
histories — each side leaks the other side's validators and each re-forms a local quorum.
This is the same price Ethereum's inactivity leak pays, and we accept it for the same
reason: the alternative (finality halts indefinitely on quorum loss) reproduces exactly the
manual-recovery operational pattern this doctrine exists to end. Two mitigations stand:
equivocation slashing still burns any validator that signs both sides, and T_leak is long
enough that a partition must persist for a week before uniqueness is at risk. T_leak is the
one parameter this ADR leaves to the operator.

Two clauses restated as binding, not new: the overlay **never gates production** (a finality
stall leaves Decisions 1–3 untouched), and the minority-cannot-finalize safety rule of the
overlay is unchanged — the leak shrinks the denominator slowly and deterministically; it
never lets a present minority finalize *now*.

VLT is currently dormant on testnet-11, so this decision creates no re-mint pressure; it
binds the overlay whenever and wherever it is next enabled.

*As implemented (superseded by ADR-0066: the grace now comes from the top-level
`Params::palw_inactivity_leak` fence, and the `DnsParams` field is retired at `u64::MAX`):*
`DnsParams.inactivity_leak_daa` (in every preset's fingerprint) drives
`InactivityLeakViewV1`, which filters BOTH quorum denominators
(`total_active_stake_by_epoch`, `total_voting_weight_by_epoch`), wired through every tally
site (live, shadow, precommit duty, branch score). The leak's evidence is the same
signature-verified contribution set the numerator aggregates — "counted as present" and
"counted as voting" are one fact — and the baseline is the later of the last attestation and
each bond's activation, so a freshly bonded validator is in grace, never leaked on arrival.
Shipped values: testnet lineage 5,040 DAA (~7 days at 120 s), mainnet 6,048,000 (~7 days at
10 bps), devnet/simnet `u64::MAX` (off — drills assume a frozen set).

## 7. Decision 5 — refusal gates decay (partially landed)

Two kinds of refusal exist, and the doctrine treats them oppositely:

* **Identity refusals** — wrong genesis, wrong consensus fingerprint, wrong ruleset — are
  *correct forever*. No TTL; these are what re-mints and version bumps are for.
* **State refusals** — quarantines, vetoes, holds, bans, candidate exclusions — every one
  MUST carry a decay measured in wall-clock or heartbeat DAA (a clock the refused party
  cannot be blamed for stalling), plus a logged reason naming the gate. A gate whose refusal
  can feed its own predicate is a wedge with a fuse of unknown length; the TTL is the fuse
  you chose on purpose.

Inventory at drafting time:

| gate | status (verified 2026-08-30) |
|---|---|
| DNS reorg veto | TTL landed (`dns_veto_ttl_daa_score`; the t10 wedge-release retrofit) |
| chain-participation switch counter | **root cause closed** (`e05a8699`, in this branch's ancestry: a refused switch no longer feeds the counter that refused it) and the `--clear-quarantine` operator override works. Residual: no automatic decay — a documented exception, not a gap: post-fix the counter counts only REAL adopted switches, auto-release would re-admit genuine flapping, and the override is the deliberate escape. |
| pruned-IBD panic → quarantine | newcomer-path fixes in ancestry (`fix/newcomer-join-panic`, `fix/newcomer-bond-path` merges) |
| producer hold (sweep cursor) | hold reasons are logged with their diagnosing numbers (throttled, never silent); the holds are economic states that decay on chain progress — which Decision 1 makes unconditional |
| peer/mempool bans | already TTL'd (upstream behaviour) |

New gates inherit the rule at review time: *state refusal ⇒ decay + reason, or it does not
merge.*

## 8. What this deliberately does not decide

* **Removing the genesis bond seats** and **right-sizing the genesis collateral** — *decided
  the same day as ADR-0061*: the gate now admits any registry size down to zero (Decision 1's
  heartbeat carries the first bond registrations), and the collateral outputs are 10,000 BILI
  per seat against the 3,223.07 BILI derived demand.
* **Mainnet finality topology** — the leak's parameters for a mainnet validator set are a
  launch-time decision.

## 9. Implementation staging

1. **Heartbeat + ramp** (Decisions 1–2): consensus change — admission, per-class DAA
   exception, ε-work in GHOSTDAG accounting, fee-only coinbase path, share-table entry.
   Ships with a `PALW_STATE_V2_VERSION` bump and rides a re-mint; the ADR-0059 10B-cap
   re-mint is the natural vehicle if sequenced together.
2. **TTL audit** (Decision 5): mostly node-local; the switch-counter closure is the one
   consensus-adjacent item and is verified, not assumed.
3. **Inactivity leak** (Decision 4): overlay change, dormant until VLT is next enabled;
   carries its own version bump then.

Each stage declares itself by version bump — this project's own law: a rule change not
declared by a version bump forks the network silently.

## 11. Implementation record (2026-08-30)

All three stages landed together on the ADR-0059 re-mint train rather than serially — the
fingerprint was moving anyway, and one coordinated wipe is cheaper than three:

* **D1 + D2**: `palw_heartbeat_v1` (the pure rules), `processes::heartbeat_evidence` (the
  chain-order walk), acceptance via `accepts_algo_id`, the lane's bits + slot in header
  validation, ε in GHOSTDAG at every proof level, zero-subsidy enforcement in the body rule
  AND the expected-coinbase (`own_subsidy` threaded through `expected_coinbase_transaction`),
  `heartbeat_adapt_block_template` (consensus API + session delegate) and the
  `--palw-heartbeat-miner-address` in-node miner. Declared by `PALW_STATE_V2_VERSION`
  12 → 13.
* **D4**: `InactivityLeakViewV1` as described in §6, with its grace supplied by
  `Params::palw_inactivity_leak` since ADR-0066. `DnsParams.inactivity_leak_daa` is retired at
  `u64::MAX` — it could never be armed, because `DnsParams` reaches `consensus_params_id` as one
  raw borsh blob that `for_each_fence` does not visit, so editing it split the network at the
  handshake on deploy day.
* **D5**: inventory verified (§7 table); the `palw_state_root` malleability gap found and
  closed in passing.
* **Tests**: unit (ladder, slot, retarget, floor, ε, leak denominators, evidence builder) and
  a pipeline integration test (`palw_heartbeat_blocks_tick_the_clock_and_weigh_epsilon`) that
  mints, folds, weighs (ε exactly), slot-refuses, ramp-admits and subsidy-refuses heartbeat
  blocks through the real block pipeline — the test that caught the sampled-window defect
  before it shipped.
* All five preset fingerprints re-pinned (t11: `4486d9b1…`); golden state-root vectors moved
  to version 13.

## 10. The degradation ladder

| state | production | finality | recovery path |
|---|---|---|---|
| normal | all bonded PALW lanes + 33‰ heartbeat | overlay (where enabled) | — |
| validators dead | untouched | probabilistic only | leak re-forms quorum (≤ T_leak + margin) |
| producers dead | heartbeat, ramped to full cadence | continues | timeouts sweep ≈ normal speed → permissionless re-bond → PALW relights |
| everything dead | heartbeat alone | probabilistic only | both of the above, in order — **no re-mint, ever, for liveness** |

The honest ledger of what this costs: up to 33‰ of blocks in calm weather (and temporarily
more in a crisis) are hash work rather than useful inference — the minimal concession that
buys the property that the network survives its producers, its validators, and its own
court's mistakes.


## 12. What the audit changed (2026-08-30, same day)

The doctrine in §2 stands. This implementation of it does not, and the failures are worth as much
as the design was:

* **D1/D2 (heartbeat lane) — OFF.** It could price the bonded lane off its own chain permanently
  (heartbeat `bits` poison the global difficulty window: 0 bonded + 263 heartbeat rows demands
  33,554,432 work where the ambient V2 target demands 2); ε = 1 is half a bonded block rather than
  a millionth, because a V2 block's work IS 2; the slot rule bounds the chain but not the DAG, and
  the retarget structurally cannot rise above its floor; and the evidence walk terminated on row
  count rather than depth, so archival and pruned nodes could reject each other's blocks. The
  first of those is the doctrine's own failure mode, reintroduced by the doctrine's own remedy —
  which is the strongest argument in this file for writing the failure down instead of the intent.
* **D4 (inactivity leak) — DORMANT.** The evidence cannot express the rule: the attestation walk
  spans ~150 s on mainnet against a declared 7 days, so absence from a two-minute window read as
  seven days of silence. Wired into the branch comparison it let a candidate branch write its own
  denominator — a 2-of-12 branch scoring full credit where it used to score zero. It is now
  dormant, structurally excluded from the branch comparison, and correctly classified as a
  duration rather than an activation fence (as a fence its value was normalised away, so two
  builds disagreeing about the denominator would have peered).
* **D3, D5 — unaffected.** Producer self-healing and the refusal-decay inventory stand as landed.

**Consequence for ADR-0061.** A zero-seat genesis is still a valid genesis, but its BOOTSTRAP —
"heartbeat blocks carry the first bond registrations" — waits on the redesign above. Until then a
zero-seat network can be minted and cannot make its first block.

**What a correct heartbeat lane needs:** its price out of `header.bits` (the receipt lane's ticket
is the existing pattern), a work basis that is not the shared blue-work scale, DAG-wide slot
evidence, and a depth bound tied to `pruning_depth`.

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0060-the-liveness-doctrine.md -->

</details>

<a id="source-0064-trustless-recovery-from-a-total-stop"></a>

<details>
<summary>0064-trustless-recovery-from-a-total-stop.md — 原文 L1–L240</summary>

[原文を開く](../adr/0064-trustless-recovery-from-a-total-stop.md) · SHA-256: `1de41fd78e06482ef15b11af6b5f335bace231cf036d5f113cfaaed5a78bdf86`

<!-- ADR_SOURCE_BEGIN:0064-trustless-recovery-from-a-total-stop.md -->
~~~~~~~~markdown
# ADR-0064 — Trustless recovery from a total producer stop: the bond becomes usable in the block that registers it

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

Status: **Partially superseded by its own correction** (2026-08-30). It was written to answer the
one item ADR-0060 §12 left open after the audit withdrew the heartbeat lane. **It does not answer
it** — see the correction under §Decision, verified against the code and the reason the title is now
wrong. What it does deliver is landed and dormant: a bond becomes usable one chain block earlier,
and the pipeline stops disagreeing with the state machine about which state answers *is this bond
registered*. Facts A and B stand on their own.

> **Superseded in effect (index reconciliation, 2026-09-02).** Facts A and B stand and are cited
> by later ADRs. The mechanism (one earlier bond lookup) is landed and dormant behind
> `palw_bootstrap_activation`, and by this ADR's own correction does not close the deadlock. The
> open item it left — a lane a chain that has given nothing can still produce — is answered on armed
> presets by the heartbeat lane in [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)'s
> form, armed from genesis on testnet-11 and devnet by [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
> Phase 2, whose drill showed unattended producer re-entry over the clock. "The heartbeat lane is
> `false` since the 2026-08-30 audit" describes the build of that day. Map: [`README.md`](README.md).

## The question

If every bonded producer stops permanently, can the chain restart **without** an operator, a
checkpoint or a privileged key? Producing requires an attempt naming a registered bond; a bond is
registered by an object riding a transaction; a transaction needs a block; a block needs a
producer.

Four independent designs were generated and each red-teamed twice. Two facts decided it, and both
are worth more than the answer.

## Fact A — "the network was silent" is not a checkable predicate. This one is a theorem.

The heartbeat lane, and three of the four designs, gated a recovery right on demonstrated silence.
That cannot be done. The only wall clock in consensus bounds the **future**
(`check_block_timestamp_in_isolation`); the past is bounded by median-time-past, computed from the
branch's own headers. The other clock — DAA — advances only when blocks are produced, so in a total
stop it is frozen, which is the hostage clock ADR-0060 §1 is about.

So "no bonded block for S" evaluates to *"my selected parent's declared timestamp is S old"*, which
is true of **every fork rooted more than S ago on a perfectly healthy chain**. A branch cannot
witness the DAG's future. No depth bound or state read fixes this; it is what silence means.

**The design constraint that follows: the recovery right must be worthless to a fork, because you
cannot stop a fork from claiming it.**

## Fact B — a zero-weight lane hands fork choice to the block hash, for free

`palw_tip_weights_v1` returns `None` unconditionally on a V2 network, and its own comment records
that wiring the PALW order into the tip heap was a measured permanent wedge. Ordering therefore
falls to `SortableBlock` = `(blue_work, hash)`, and the deep-reorg comparator runs only for
candidates that are **not** chain-descendants of the sink — so it is not consulted on forward
progress.

Any lane whose blocks carry zero blue work makes every branch tie on work, and the largest hash
wins, at zero cost, forever. (This also settles the ε-versus-0 question the withdrawn lane left
open: 0 is not available.)

## Decision — no new lane. Move one lookup.

> On a `ConsensusV2` network past `palw_bootstrap_activation`, admission item 1's **bond lookup**,
> for the block's **own** attempt only, resolves against
> `bonds(parent state) ∪ { BondRegistered carried by this block's own mergeset }`.
>
> Every other item — class record, artifact root, class target, the `DerivedV1` pwu equality,
> epoch budget, class ticket, exposure ceiling, duplicate-attempt — keeps reading the parent state.

Equivalently: **a bond becomes usable by the block that accepts its registration, instead of by
that block's child.**

> ### CORRECTION (2026-08-30, against the implemented code) — this does NOT close the deadlock
>
> The sentence that stood here — *"a would-be producer mines one block that both carries their
> `BondRegistered` and makes an attempt under it; no existing producer is needed, so the deadlock is
> gone"* — is **false**, and it was false when written. Three facts, each read off the tree:
>
> 1. **A block's own body is never in its own mergeset.** `calculate_utxo_state` builds
>    `mergeset_acceptance_data` from `once(selected_parent) ++
>    consensus_ordered_mergeset_without_selected_parent` (`utxo_validation.rs:328-331`), which is
>    `mergeset_blues[1..] ++ mergeset_reds`. A block's transactions are accepted by a *later* block,
>    never by itself. So "this block's own mergeset" cannot contain a registration the same block
>    carries — the recovery block described above is unconstructible.
> 2. **A `ConsensusV2` network accepts exactly two lanes, and both require standing already.**
>    `accepts_algo_id` (`palw_mode_v2.rs:729-737`) admits the committed-attempt id — which must
>    decode an envelope naming a registered bond — and the free-prompt receipt id — which must
>    name a claim already certified at this chain point. The heartbeat lane is `false` since the
>    2026-08-30 audit.
> 3. Therefore **a party holding neither a bond nor a certified quantum cannot make any block at
>    all**, so its `BondRegistered` transaction can never reach a block body, so no later block can
>    accept it. Moving the lookup earlier does not help: the input it reads is empty for exactly the
>    party the ADR was written for.
>
> **What the change actually buys, and it is real.** On a chain that still has a producer, a bond
> becomes usable one block sooner — by the block that accepts the registration rather than by that
> block's child — and the pipeline stops disagreeing with the state machine about which state
> answers *is this bond registered* (see the next section, which was always the honest description
> of the change). That defect was real and is now closed. It is a narrowing, not a recovery
> mechanism.
>
> **What a real answer requires, stated so the next attempt does not repeat this one:** a block lane
> whose validity depends on nothing the chain has previously granted. That is what ADR-0060's
> heartbeat lane was, and it was withdrawn by the 2026-08-30 audit for four structural findings —
> so **trustless recovery from a total stop is an OPEN problem**, and this ADR does not solve it.
> Fact A below is unaffected: it is a theorem about what a chain can check, not about this fix.
>
> The staging item that would have caught this is in this document already — *"the bootstrap tool
> has been run on a real fleet, not reasoned about."* The fixture was never written, and the
> reasoning was wrong.

### This is not a new rule; it is the removal of a disagreement

`apply_palw_transition_v4` applies accepted objects at **step 3** and the block's own work at
**step 4**, and `apply_attempt` resolves the bond from the live fold. **The state machine already
accepts a block whose bond was registered in its own mergeset.** The only thing that refuses is the
pipeline pre-check, which resolves against the walk state at the selected parent. Two answers to
one question — the failure shape this project keeps meeting — and here it is the shape that stops
the chain.

The set is already built and already a consensus input: `palw_v2_bonds_declared_in_mergeset` feeds
`ctx.palw_v2_locked_bonds` on every chain block. The graft is to return the full records rather
than the outpoints.

### What the block is

*(Retitled: it is not a "recovery block" — see the correction above.)*

An **ordinary algo-6 block**. It carries a real attempt with a real inference, creates a real claim
with real escrow and exposure, earns the ordinary subsidy, adds `calc_work(bits)` like every other
block, and is refutable by a panel and a court. Nothing is relaxed except *when* its bond becomes
visible — and, corrected, that is one chain block earlier for a NEWCOMER JOINING A LIVE CHAIN: the
block that accepts the registration may attempt under it, instead of that block's child.

**The marginal cost of bonding-then-attempting versus pre-bonding is exactly zero** — same
collateral, same signature, same ticket, same inference, same ceiling. That is why it is not a cheap
permissionless mint, and it is a stronger argument than any rate limit, because there is no rate to
limit.

### Why it does not reproduce the four withdrawn findings

1. **Window poisoning** — no new lane and no new `bits`; there is no second bits population to
   average, so no fixed point can form.
2. **Weight parity** — no new work basis. The block's weight is `calc_work(bits)` in the ordinary
   currency, so Fact B never engages.
3. **Sibling width** — no slot rule is introduced, so there is no chain-versus-DAG gap. Width is
   bounded by what already bounds every producer: the collateral floor, one-key-one-bond, the
   exposure ceiling, the class ticket and the epoch budget.
4. **Horizon divergence** — depth ≤ 1 and no walk: the parent PALW state plus this block's own
   mergeset, both of which any node validating this block necessarily holds.

### The narrowing that must not be dropped

Move the **bond lookup** and nothing else. Inserting the *full* admission after the class retargets
would make admission item 6's strict `DerivedV1` equality compare against a post-retarget target
while the producer computed its pwu from the pre-retarget tip — so on a multi-class chain the first
block across every epoch boundary would be disqualified and the chain would stop every 1000 DAA.
testnet-11 runs three classes. A regression fixture pins this.

## What it costs

Supply, emission, coinbase rules, fork-choice weight, state root, ruleset id: **all unchanged**.
No version bump, no golden vectors, no re-mint. Node cost in normal operation is zero (the mergeset
extraction already runs; the admission call gains one `Option`).

**The real cost, stated plainly:** a restarted chain has a clock, a ledger and block production —
and **no finality** until six distinct operators exist, because `derive_panel_v2` refuses a short
draw. Until then every claim voids at `BindTimeout` and the worker carve of every block is
destroyed by don't-mint. Recovery restores liveness, not the economy. "The deadlock is closed" must
not be read as "the chain is back".

## The objection this exposes — and it is a P0 that exists TODAY

The red team's strongest attack is that the rescue procedure, run six times, manufactures a
`safe_frontier` on a private branch. The mechanism is real, and every link was verified here:

* post-genesis registration gates on `min_collateral_sompi` alone — **400,000 sompi (0.004 BILI)**,
  and the collateral is refundable;
* `write_bond(key, None)` has **no callers**: a bond never leaves the registry, retired or not;
* `registered_daa` is written and **read by no consensus gate anywhere** — there is no bond
  maturity and no soak;
* `PalwBondStatusV2` is `Active | Retiring`, so a bond is Active in the block that registers it;
* seat tickets are `H(anchor ‖ claim ‖ bond)` with both the anchor and the claim id
  attacker-influenced.

**But this proposal does not create it.** An attacker holding one bond registered at any time
before the fork point can already fork, carry sybil `BondRegistered` objects inside fork blocks
(they fold at the accepting block today, with no rule change), seat their own panels, self-license
and grow `safe_frontier`. `palw_fork_choice`'s stated invariant — *a fork nobody could see collects
no receipts, so it has no frontier* — **is already false for anyone who has ever paid 0.004 BILI**,
and the right is permanent and retroactive because bonds never leave.

That is a P0 in its own right and is tracked separately (seat maturity + frontier provenance —
`registered_daa` is precisely the unused field such a rule needs). This ADR must not ship as the
last word on it.

## Staging

* **Behind `Option<ForkActivation> palw_bootstrap_activation` on `Params`, TOP LEVEL** — never
  inside the V2 bundle. `consensus_identity_id` normalises top-level fences, so two builds that
  disagree only about a future height keep one identity and stay peers; a fence inside the bundle
  goes through `palw_ruleset_id_v2`, which is not normalised, and would be a deploy-day partition.
* **Ships dormant.** With the fence unset the pipeline passes `None` and behaviour is byte-identical
  to today: no fingerprint move, no re-mint.
* **Tests. What EXISTS, and what this list used to claim.** This section previously read as though
  four pipeline fixtures were in place. **None of them is.** What exists is:
  * `a_bond_registered_in_this_blocks_own_mergeset_admits_this_blocks_own_attempt` — a pure-function
    unit test of `check_palw_producer_entitlement_v2_with_bootstrap`, both fence positions, plus the
    two narrowings (a mergeset bond for an unknown class still fails on the CLASS; a mismatched key
    still fails item 2);
  * `palw_v2_the_bootstrap_registry_is_the_state_the_transition_will_hold` — a pipeline test that the
    bootstrap record is read from the state the accepted objects fold to, verified to fail when the
    filter returns the parent state instead.

  **Still outstanding, and they are the deploy precondition:** (1) fence ARMED in a real
  `ConfigBuilder` — a bonded-in-mergeset block validates and its successor is ordinary; (2) fence
  off, same DAG — disqualified; (3) an epoch-boundary block on a multi-class chain still validates
  (the §narrowing regression — the existing admission test asserts only `ClassMissing`, which is not
  that); (4) a self-bonded block merged into a *sibling's* chain is skipped and unpaid.

  Until those exist, `palw_bootstrap_activation` has **never been executed in the `Some` position by
  any test at the pipeline level** — nor have D1's or D4's fences. That is precisely the condition
  this section quotes about the heartbeat lane, so it must be read as a blocker on arming, not as a
  nicety.
* **Before switching it on:** the fence is armed at a future, *reachable* score while the network is
  still alive (a recovery rule fenced behind a score a dead chain cannot reach is withdrawn-finding
  1 in new clothes); and the bootstrap tool has been **run on a real fleet**, not reasoned about.

## Two things worth recording that nobody asked for

1. **The pipeline and the transition disagree today about which state answers "is this bond
   registered."** That disagreement is the deadlock, and it is a defect independent of this fix.
2. **`registered_daa` is a consensus field no consensus rule reads.** A recorded fact with no reader
   is either dead weight or a missing rule. Here it is the second.
~~~~~~~~
<!-- ADR_SOURCE_END:0064-trustless-recovery-from-a-total-stop.md -->

</details>

<a id="source-0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table"></a>

<details>
<summary>0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md — 原文 L1–L279</summary>

[原文を開く](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) · SHA-256: `a4dfac430c4265c7697b867b1077619d9aae0aa6c2cb7dcc702e4c771634be53`

<!-- ADR_SOURCE_BEGIN:0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md -->
~~~~~~~~markdown
# ADR-0066 — The heartbeat lane out of `header.bits`, and the inactivity leak out of node memory

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: **Decisions 1, 2 and 4's fence LANDED (2026-08-31), both dormant. Decision 3 LANDED through ADR-0068 Phase 1 (`Params::palw_attempt_work_fence`: an attempt block's blue work is the constant `1 << PALW_ATTEMPT_BLUE_WORK_LOG2`, a heartbeat's is ε — ghostdag/protocol.rs names this ADR's Decision 3 as closed). Decision 4's committed per-validator table is NOT pursued: it belongs to the validator lineage, which is retired (recorded 2026-09-05).** Supersedes the *implementation* of ADR-0060 Decisions 1–2
and Decision 4; the doctrine those decisions state is unaffected. Both features still ship OFF —
but they ship off behind **fences that can be armed**, which is the difference this ADR was written
to make. See "What landed" at the foot of this document for exactly what is and is not in the tree.

**Neither part is a re-mint. Both must be activations, and both must be fenced at TOP LEVEL.**

> **Arming and closures after the Status line (index reconciliation, 2026-09-02).** "Both
> dormant … ship OFF" was the 2026-08-31 state. [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
> arms `Params::palw_heartbeat` from genesis on testnet-11 (Relaunch 5 onward) and devnet, closes
> Decision 3 (attempt blue work leaves `calc_work(bits)` — `Params::palw_attempt_work`, a 2²⁰
> constant against ε = 1) and closes F3a (width bound: at most four heartbeats per mergeset, or any
> number forming one chain — its F5 amendment). Decision 4's fence `Params::palw_inactivity_leak`
> is still `None`; the committed-table expansion is withdrawn under RFC-0012. The sentence "a V2 network's
> doctrine is to re-mint rather than schedule" is the testnet practice, not the doctrine: consensus
> changes ship by activation (mainnet), and [ADR-0072](0072-the-ticket-is-the-execution.md) §3 records
> the activation shape. Map: [`README.md`](README.md).

> **Decision 4 の DNS table 拡張は不採用。** PALW liveness に別の validator 専用 state を追加する必要がないため。移行境界は RFC-0012 に従う。

> **The two-step ramp met a producer slower than its recovery step (2026-09-10) — see
> [ADR-0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md).** "One block deep admits two
> states … the chain is producing, or it is not" assumed a producing chain produces faster than the
> recovery cadence. With ~17-minute draws, a heartbeat selected parent kept the chain on heartbeats
> and every bonded block landed red behind them. The slot rule here is unchanged; ADR-0105 fences a
> coloring rule (a heartbeat never turns a bonded block red) and makes the heartbeat miner step
> aside for a landed draw.

## Why the first implementation failed, sorted by cause

The audit recorded four findings. Sorting them by *mechanism* rather than by symptom is what makes
the redesign tractable, because only two of them are about `bits`:

| # | finding | cause |
|---|---|---|
| F1 | the lane can price the bonded lane off its own chain, permanently | **`bits`** |
| F2 | ε = 1 is not small against a V2 block worth 2 | the shared **blue-work** scale |
| F3a | sibling heartbeats share one POV, so nothing bounds their width | independent |
| F3b | the retarget can never rise above its floor | **`bits`** |
| F4 | the evidence walk terminates on a node-local fact | independent |

F1 is the fatal one and it is purely a consequence of the price living in the field the global
difficulty window averages. A V2 network runs at `MAX_DIFFICULTY_TARGET` because the class lottery,
not the hash target, is its throttle — so a window of heartbeat rows demands work 33,554,432 and no
bonded block can re-enter it. The fixed point is a heartbeat-only chain recoverable only by re-mint:
the self-feeding refusal ADR-0060 was written to abolish, reintroduced by its own remedy.

## Decision 1 — the lane gets its own algorithm id, and its price never touches `bits`

Four carriers were considered. Three are unusable:

* **A new header field** works and moves no genesis hash (the `palw_state_root` precedent shows a
  double-gated field leaves every existing preimage byte-identical), but it changes `Header`, the
  p2p conversion and the RPC model, and old binaries compute a different identity for the new
  headers. Cost without a matching benefit.
* **`header.palw_commitment`** is **disqualifying**: it is excluded from every PoW digest, so a
  cadence datum carried there is post-PoW and one solved header mints unbounded sibling identities.
  A cadence datum must be pre-PoW.
* **The coinbase payload** is body-level, and bodies arrive after headers. A price that cannot gate
  header relay makes header spam free.

**So: a new `POW_ALGO_ID_HEARTBEAT_V1`, whose target is a network constant rather than
`header.bits`.** Heartbeat headers carry the global expected bits like every other lane, so they
enter the difficulty window as ordinary rows and F1 and F3b both disappear — not as a tuning, but
because the quantity that fed back on itself is gone.

The current file argues against a new id ("it would need a new finalizer arm for identical bytes").
That arm is ~8 lines, and it buys three things: every rule stops being triple-gated on
`(id == 3) && ConsensusV2 && CONST`; a hash-network algo-3 solution can never be replayed as a
heartbeat, because the Layer-0 digest binds `pow_algo_id`; and the id becomes the fence's own
observable.

## Decision 2 — the slot rule is one block deep, and the evidence walk is deleted

F4 is not a tuning problem. `heartbeat_evidence`'s walk terminates on `Err(get_header) => break`,
which is a **node-local** fact: an archival node never hits it and a pruned node hits it at its own
pruning point. Two honest nodes then compute different verdicts for the same header — a partition
along the `--archival` flag.

A retarget is what needs ancestor evidence. With Decision 1 there is no retarget, so the slot rule
becomes a function of the selected parent alone: *is this heartbeat far enough after the parent's
timestamp*. `consensus/src/processes/heartbeat_evidence.rs` is deleted, not bounded.

F3a — sibling width — is **not** fixed by this and must be stated: the slot rule bounds the chain,
not the DAG. What bounds width is the price, which is now a fixed target rather than a floor the
retarget cannot leave.

## Decision 3 — ε stops competing with a V2 block's work

F2 is independent of `bits` and survives Decisions 1–2 untouched: ε = 1 against a bonded block worth
2 is parity at `ghostdag_k = 1`. The fix is that a V2 attempt block's blue work must not be
`calc_work(bits)` — the attempt lane's throttle is the class lottery, so its work should reflect the
inference it carries, not the hash target it did not need.

**This is the expensive decision.** It moves `header.blue_work` on every V2 block, so it needs its
own soak, its own golden vectors and a re-derivation of the pruning proof's level argument. It is
separable from Decisions 1–2 and should be staged after them.

## Decision 4 — the inactivity leak needs committed per-validator state

committed per-validator liveness table は実装しない。廃止対象の DNS validator lineage 専用であり、PALW の liveness に不要なため。DNS の移行境界は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) に従う。

## The trap in the constant, and why both fences must be top level

旧 DNS inactivity-leak 専用 fence・table の追加計画は継続しない。DNS validator 拡張を採用しないため。既存 fence の履歴は以下の実装記録に残す。

The same applies to the heartbeat lane. Its current shape — a `const bool` that changes block
validity and moves no fingerprint — is **code-only masquerading as a rule**, which is a silent fork.
It becomes a top-level `Option<PalwHeartbeatParamsV1>`; the new algo id, the work constant and the
slot rule may not go into the V2 bundle, because the bundle's fences live under
`palw_ruleset_id_v2`, and a V2 network's doctrine is to re-mint rather than schedule.

## What this costs, honestly

heartbeat の既存実装・activation は以下の記録に従う。撤回した DNS table の snapshot・apply/revert・IBD 用実装見積もりは継続しない。

## Consequences

* Both features stay OFF until their fences exist and are armed. No preset fingerprint moves while
  they are `None`, so this ADR costs nothing to adopt and everything to arm carelessly.
* F3a (sibling width) is **not** closed by Decisions 1–2 and is recorded as open.
* The heartbeat lane remains the only known answer to trustless recovery from a total producer
  stop — ADR-0064's correction establishes that its own remedy does not close that, so this ADR is
  on the critical path for a property the chain currently does not have.


## What landed, 2026-08-31 — and what did not

**Landed. Decisions 1 and 2, in full, plus a fence for Decision 4.**

*The lane's price left `header.bits`.* `POW_ALGO_ID_HEARTBEAT_V1 = 8` exists, `check_algo_id_known`
admits it, and `StateLayer0::new` substitutes `Uint512::MAX >> PALW_HEARTBEAT_WORK_LOG2` for the
target on that id. A heartbeat header carries the GLOBAL expected bits like every other lane's, so
heartbeat rows enter the difficulty window as ordinary rows. F1 and F3b are gone as arithmetic. The
substitution is in the one place every PoW path shares — ordinary validation, the pruning proof,
trusted import — so no caller can price the lane by forgetting to. The tag arm is algo-3's, shared
rather than copied, and the two lanes cannot borrow each other's solutions because the Layer-0
digest binds `pow_algo_id`.

*The evidence walk is deleted.* `consensus/src/processes/heartbeat_evidence.rs` is gone, both call
sites with it. `check_heartbeat_slot` now takes `(selected_parent_timestamp, selected_parent_algo_id,
header_timestamp)` — one header the caller already holds. F4, the `--archival` partition, cannot
recur because there is nothing left to walk.

*The ramp has two steps, not three.* The middle step asked "how long has the bonded lane been
silent", which is ancestor evidence — F4 in one question. One block deep distinguishes exactly two
states and they are the two that matter: the chain is producing (nominal hour) or it is not
(recovery cadence).

*Both features became fences.* `Params::palw_heartbeat: Option<PalwHeartbeatV1 { activation,
work_log2 }>` replaces the `const bool`; `Params::palw_inactivity_leak: Option<PalwInactivityLeakV1
{ activation, t_leak_daa }>` replaces `DnsParams.inactivity_leak_daa`, which is now retired at
`u64::MAX` permanently and pinned there by a test. Both are wired at all seven identity sites and
are `None` on every preset, so **no shipped fingerprint moved**.

*One defect found while landing it, recorded because the shape recurs.* `algo_id_carries_no_chain_position`
answers two questions at once — block LEVEL and blue WORK — because for the receipt lane both
answers are the same. For the heartbeat they are NOT: level must be zero (a constant target makes a
lucky solve indistinguishable from a hard one, so `calc_level_from_pow_512` would read luck as
hierarchy) but blue work must be ε, not zero (the regime the lane exists for is total collapse,
where every block is a heartbeat and zero-work branches all tie). Folding the heartbeat into the
shared predicate made the ghostdag zero-arm return before the ε-arm was reached, so the lane weighed
nothing and a collapsed chain could not order its own branches. Split into
`algo_id_derives_no_block_level`, and pinned by a test that asserts the two predicates DISAGREE for
this id.

*The integration test runs now.* `palw_heartbeat_blocks_tick_the_clock_and_weigh_epsilon` used to
`return` early because the lane was a `const bool` set to false — a test that could only be run by
rebuilding the binary, which was also the only way to run the feature. It arms the fence and
executes: the adapter, the ε credit, both slot intervals, the fee-only coinbase, and the assertion
that carries this ADR — **the heartbeat's `bits` equal the bits the ordinary template already had**.

**Not landed, and not started.**

*Decision 3 (ε against a V2 block's work).* Unchanged and still correct: on a V2 preset
`calc_work(0x207fffff) = 2`, so a heartbeat is worth half a bonded block and `ghostdag_k = 1` makes
that parity. The fix moves `header.blue_work` on every V2 block and needs its own soak, its own
golden vectors and a re-derivation of the pruning proof's level argument. Deliberately staged.

*Decision 4's committed per-validator table.* The FENCE landed; the TABLE did not. And one
correction to this ADR's own text, made after reading the code rather than the design: it says the
per-validator state "lives in node memory (`last_attestation_daa_by_validator`), which cannot be
right: a value that decides block validity and is not committed is a value two nodes can disagree
about with no way to notice." The premise is wrong. The table is built by
`last_attestation_daa_by_validator(&contributions, &epoch_anchor_daa)` from the contributions
`collect_stake_contributions_v2` gathers on a walk from the tip, bounded by
`dns_params.stake_score_window_blue_score` — a CONSENSUS parameter, identical on every node. So it
is already a pure function of (tip, params), reorg-safe by the same argument `recompute_epoch_tallies`
uses, and two nodes holding the same tip compute the same table. The "node memory" this ADR feared
is not there.

What IS still worth committing is narrower, and it is the half this ADR got right: a node that
starts from a pruning point has no window to walk. `PruningPointOverlaySnapshot` is how the other
overlay components cross that boundary, and the leak table has no equivalent — so a pruned-IBD node
cannot reconstruct it, and if the leak were armed it would judge liveness from an empty table.
Closing that still needs the fourth snapshot component, the capture-time filter for validators with
no surviving bond, and the pruned-IBD import tests; it no longer needs the apply/revert pair this
ADR budgeted for, because there is no incremental state to revert. Because the leak ships dormant,
nothing depends on it today.

*F3a (sibling width).* Still open, as this ADR recorded. The slot rule bounds the chain, not the
DAG. What bounds width is the price, which is now a fixed 2²⁴ per block rather than a floor a
retarget could never leave — better, but not a bound.

**The standing warning is unchanged, and SA-4 below makes it legible rather than removing it.**
Arming either fence is a coordinated change: `work_log2` and `t_leak_daa` reach
`consensus_params_id` but not the fence visitor, so two operators scheduling one height with
different values share an identity, peer, and disagree the moment the fence fires. Since SA-4 they
also announce different `consensus_schedule_id`s, which is what the `flow_context` warning prints —
so the disagreement is visible before it fires instead of inferable after. For the heartbeat price
there is a second lock — the binary refuses to start if the fence names a `work_log2` it does not
implement — because that value has a second source in `StateLayer0`. `t_leak_daa` has no second
source and therefore no such lock.

## Security amendment (2026-09-02) — Decision 4's committed table, before it is built

Decision 4 の新規 snapshot・leak-table 拡張は行わない。対象の DNS validator lineage を廃止するため。旧ネットワークで有効な leak 規則は RFC-0012 の移行まで維持する。

**SA-4 — A value that rides a fence must be VISIBLE to the operator comparing two builds.** Two
builds that peer with different `t_leak_daa` and then diverge on finality is the
`inactivity_leak_daa` accident under a new name.

*As built (2026-09-03), and this is a decision that had to be made rather than a bug that had to be
fixed.* The amendment's original wording — put the value in `consensus_identity_id` raw, past the
fence visitor — was implemented and then withdrawn, because it buys the wrong half of a trade that
cannot be had both ways:

* `consensus_identity_id` is a **gate**: `flow_context` refuses a peer whose identity differs.
* The rolling deploy needs `id(None) == id(Some(H, v))` — a build that schedules a fence must peer
  with a build that has never heard of it. The value-in-the-identity rule needs
  `id(Some(H, v₁)) != id(Some(H, v₂))`. For any equality-compared fingerprint those two are
  contradictory by transitivity. One of them has to go.

The rolling deploy is what stays, for two reasons and both are recorded in `params.rs`:

1. **It is a procedure this project has executed.** The ADR-0068 Phase-1 rollout of 2026-09-01
   scheduled `palw_heartbeat` (work_log2 24, max_per_mergeset 4) and `palw_attempt_work`
   (work_log2 20) at DAA 5,000 with the live testnet-11 tip at 1,746, and rolled the fleet host by
   host *because* "a scheduled fence is normalised out of `consensus_identity_id`". Under a gate on
   the value, the FIRST host to restart is disconnected from every host that has not — a partition
   thousands of DAA before the fence, on a fleet with no way to converge except stopping all of it
   at once.
2. **For the leak specifically it would undo the reason the fence exists.** `palw_inactivity_leak`
   was lifted out of `DnsParams` precisely because that field could only be armed by a flag day, and
   the moment a network needs finality to self-heal after validator loss is the moment it cannot
   coordinate one. A gate on `t_leak_daa` puts the flag day back.

So the value goes in **`consensus_schedule_id`** — reported, never gated — which is the answer
ADR-0065 D1's `window_daa` already gave and which `params.rs` documents at that call site. That is
the id `flow_context` prints in the warning it already emits whenever two peers' params ids differ
and their identities agree, and folding the value in is what makes that warning able to name the
disagreement it is the only defence against; without it, the two builds SA-4 is about printed two
identical schedule ids, which says "these agree" about the one thing they do not.

**The rule, stated so it can be applied to the next such value without re-litigating this:** a value
that rides a fence goes in `consensus_params_id` (it always did) and in `consensus_schedule_id`
(SA-4). It goes in `consensus_identity_id` only through the fence being ACTIVE AT GENESIS, where two
builds disagree about block 1 and the handshake is right to refuse. Nothing scheduled belongs in the
gate — not the height, and not the value beside it. This covers `palw_heartbeat.{work_log2,
max_per_mergeset}`, `palw_attempt_work.{work_log2, ticket_bucket_log2}`,
`palw_inactivity_leak.{t_leak_daa, reentry_final_depth_daa}`, `palw_bond_maturity.window_daa`, and
the beacon fold width `k`, whose placement in `consensus_schedule_id` is therefore correct as
shipped.

What this does NOT buy, said plainly: arming a value-carrying fence is still a **coordinated**
change. Two operators who schedule one height with different values will peer, sync, and diverge
when it fires. The warning is the whole of the defence, and the schedule id is what makes the
warning true.
~~~~~~~~
<!-- ADR_SOURCE_END:0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md -->

</details>

<a id="source-0105-a-heartbeat-never-turns-a-bonded-block-red"></a>

<details>
<summary>0105-a-heartbeat-never-turns-a-bonded-block-red.md — 原文 L1–L558</summary>

[原文を開く](../adr/0105-a-heartbeat-never-turns-a-bonded-block-red.md) · SHA-256: `4e734271e58166ad163788033f8a2f63244479c8f44cab86cd6cc6e0da1c828b`

<!-- ADR_SOURCE_BEGIN:0105-a-heartbeat-never-turns-a-bonded-block-red.md -->
~~~~~~~~markdown
# ADR-0105 — A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has landed

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


* Status: PROPOSED 2026-09-11 on `fix/heartbeat-trap-slow-producers` (from `main` at `a5f1bdf7`).
  **Decision 1 IMPLEMENTED behind `Params::palw_heartbeat_transparent`, `None` on every shipped
  preset** — no fingerprint, identity, schedule or fork id moves on any network, and arming it on
  testnet-11 is a flag day (§7). **Decision 2 IMPLEMENTED in the heartbeat miner** — node policy,
  no rule, safe to roll out host by host today. **Decision 3** is operator guidance, written into
  [`testnet11-node-operator.md`](../history/networks/testnet-11/testnet11-node-operator.md) §7a.
* **Amended 2026-09-25 — F1 (§11), a post-launch flag day**: past
  `Params::palw_heartbeat_transparent_same_chain` transparency stops at the merging block's own
  selected chain; the rule as written (and as testnet-12 launched with it, from genesis) lets a
  private heartbeat branch borrow the public chain's attempts. The fence is `None` on every shipped
  preset, testnet-12 included, until an operator schedules it.
* Builds on: [ADR-0060](0060-the-liveness-doctrine.md) Decisions 1–2 (the heartbeat lane and its
  ramp), [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
  Decisions 1–3 (the lane's own id and constant price; the one-block-deep slot rule; ε against the
  attempt lane's constant), [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
  Phase 1 (`palw_attempt_work` = 2²⁰; the F3a width bound), [ADR-0058](0058-palw-merged-work-is-counted.md)
  (merged work — reds included — is applied to the PALW state), [ADR-0083](0083-the-difficulty-window-counts-only-rows-priced-by-bits.md).
* Amends: ADR-0066's "the ramp has two steps … the chain is producing, or it is not" — one sentence
  of it, the unstated premise that a producing chain produces faster than the recovery cadence.
  `heartbeat_interval_ms` and `check_heartbeat_slot` are unchanged.
* Supersedes nothing.

## 0. The sentence this ADR is

**From 11:41Z to ~13:20Z on 2026-09-10 testnet-11 ran on heartbeats alone, its DNS finality
stopped and its EVM bridge stayed paused, and nothing inside the chain could end it: a bonded block
that takes seventeen minutes to draw lands eight heartbeats behind the tip, and at `ghostdag_k = 1`
those eight heartbeats — one unit of work each — colored a block worth 2²⁰ of them RED.** Past this
ADR's fence a heartbeat never counts against a bonded block's coloring, so the heartbeat that merges
the draw carries its work and the DNS work depth passes the requirement at the first draw; and
today, fence or no fence, a heartbeat miner stands aside when a bonded block is waiting, so the next
draw takes the chain back — bounded, because a clock that waits on producers is the hostage
ADR-0060 was written to free.

## 1. What was measured (testnet-11, genesis `ad30b5cb…`, 2026-09-10)

* **Every block from 11:41Z was an algo-8 heartbeat**, two to four siblings every ~120 s. The
  fleet ran four heartbeat miners, all `--palw-heartbeat-miner-address`.
* **`getDnsConfirmation`**: `pow_confirmed = false`, `workDepth` ≈ 10 against `requiredWorkDepth`
  100, health `DegradedCertificateCensored`, the last DNS-confirmed anchor stuck. Attestations were
  being included — the stall was work, not stake. `dns_finality_fresh_for_bridge` requires
  `dns_confirmed`, so the bridge stayed paused.
* **Bonded blocks kept landing and kept losing.** Qwen3.6 draws on the fleet (8 cores, 23 GB,
  two 34 GiB mappings per host) take ~17 minutes, and a bonded block's timestamp is its TEMPLATE's.
  The draws that landed were merged RED — `59563aa3…` and `1b9225e1…` are two of them — so their
  2²⁰ entered no block's blue work. None became a chain block.
* **How it started**: node restarts at 11:18Z and 11:23Z lost the in-flight draws, the selected
  parent's timestamp plus the nominal hour passed, and a heartbeat became the selected parent.
* **How it ended**: the operator removed the heartbeat flag from all four miners. The next bonded
  block became the sink within minutes, and `workDepth` jumped to **1,048,584** — one attempt
  block's 2²⁰ plus eight ε. DNS confirmed at 13:26Z, a bridge claim was accepted at 13:27Z.

## 2. The mechanism

Five facts, each individually correct:

1. **The slot rule is one block deep** (ADR-0066 Decision 2): one nominal hour after a bonded
   selected parent, one recovery interval (120 s) after a heartbeat. It cannot see a bonded block
   that is not the selected parent.
2. **A bonded block's timestamp and parents are its template's.** The attempt envelope binds both
   (ADR-0042 Decision 3a), so a draw that took seventeen minutes lands carrying seventeen-minute-old
   parents.
3. **A block's blue work excludes its own work.** `mergeset_blues` holds the selected parent, not
   the block. A draw built on tip `T` has `blue_work = bw(T) + work(T)`; the heartbeat tip built in
   the meantime has `bw(T) + 8ε` or more. The draw is never selected.
4. **At `ghostdag_k = 1` a block with two blues in its anticone is red** (ADR-0058 said it: "any
   block whose anticone holds two or more blocks is a red *by construction*"). Eight heartbeats in
   the draw's anticone make it red, and a red's work is in nobody's blue work. ADR-0058 applies a
   red's PALW claim and pays it; it does not give it fork-choice weight.
5. **The DNS work depth is `blue_work(sink) − blue_work(anchor)`**, and the anchor advances with
   the attestation epochs. Inside the episode every chain step adds ε per blue heartbeat, so the
   work any current anchor can have piled on it stays near its distance in heartbeats — ~10 — and
   never reaches 100.

Together: **once a heartbeat is the selected parent, a producer slower than one recovery interval
can never become the selected parent again, and its work is lost while it tries.** The mode
sustains itself for as long as any heartbeat miner runs. `params.rs`'s premise that a draw takes
12–60 s under a 120 s block does not hold on the fleet, and the ramp has no step between "the chain
is producing" and "it is not".

## 3. The requirement

A heartbeat chain at the recovery cadence plus a bonded block submitted 17 minutes after its
template must eventually either become the selected parent or at least keep its work, so that the
DNS work depth keeps growing past `required_work_depth`. Without giving up what the lane is for:
keeping the DAA clock alive when every bonded lane is dead (ADR-0060 §2, §10) — so no rule may let a
producer, least of all a stuck one, stop the clock.

## 4. The candidates

| | breaks the trap? | keeps the draw's work? | one block deep? | total-collapse clock | cost / risk |
|---|---|---|---|---|---|
| (a) recovery cadence ≥ the preset's draw time | only while every draw beats it | only then | yes | slowed for good | the ambulance ADR-0060 §4 built |
| (b) hold the slot while a merged bonded block is recent | only if every heartbeat miner merges it | no — the first draw is red before any heartbeat can be held | yes (the width rule already reads mergeset headers) | **a stuck producer holds it** | fatal, below |
| (c) choose the selected parent by `blue_work + own work` | yes, at once | yes | yes | untouched | every ordering in consensus; circular fence |
| (d) a heartbeat does not count against a bonded block's coloring | keeps the work; the chain stays heartbeat-led | **yes, whatever the heartbeat miners do** | yes | untouched | one header read per peer past the fence; needs a depth bound |
| (e) operator guidance | no | no | — | — | needed anyway |

**(a) A longer recovery cadence** breaks the trap exactly while a draw lands before the next
heartbeat is allowed, so it would have to be tied to the slowest producer a preset will ever have,
and it fails the moment one is slower. It also takes back ADR-0060 §4's arithmetic: at a 25-minute
recovery cadence the 6,000-DAA lifecycle horizon a total collapse must sweep takes ~104 days instead
of ~8. It changes the one thing the lane exists to guarantee in order to accommodate producers the
lane exists to survive. Rejected.

**(b) Hold the slot for a merged bonded block** ("a heartbeat whose mergeset holds an attempt block
younger than the nominal hour is too early"). It is one block deep — the width rule already reads
every mergeset header — and it would end the mode when every miner cooperates: honest heartbeats
stop, the next draw lands on a tip nobody stacked ε on, and becomes the sink. Three reasons it is
not the rule:

* it cannot save the draw that STARTS the recovery: that draw was red in the anticone of heartbeats
  that existed before it did, and no rule about later heartbeats reaches them;
* alone, it is defeated by any heartbeat that does not merge the draw — a propagation race, or one
  miner that ignores bonded tips. The draw is red, so the next draw that merges it gains ε and not
  2²⁰, and the stray heartbeat chain ties or wins;
* **and it lets a producer whose blocks can never take the chain hold the clock.** Whether a bonded
  block may become the sink is chain state — a bond at its exposure ceiling produces blocks that are
  disqualified from the chain (the block-600 wedge ADR-0060 §1 lists first), and a key that pays the
  attempt lottery needs no bond to put a header-valid attempt block into the DAG. One block deep
  cannot tell those from a good draw. Under (b) each of them holds the lane for an hour (plus the
  future-time tolerance: 132 s on testnet-11, 1,620 s on a card), and a stream of them holds it for
  good — the wedge the clock was built to sweep, re-entering through the lane that sweeps it.

(b) survives as node POLICY with a budget, which is Decision 2.

**(c) Count a block's own work when choosing a selected parent** — Bitcoin's chainwork. It breaks the
trap at once: the draw's `bw(T) + ε + 2²⁰` beats the heartbeat tip's `bw(T) + 9ε`. It is also the most
invasive change available. Every ordering in consensus that uses blue work as "the heavier chain"
would have to move together or two of them would pick different chains inside one node:
`find_selected_parent` in GHOSTDAG; the virtual's sink search (`RankedTip`, `SortableBlock`);
`pick_virtual_parents`, which ASSERTS that the selected parent's blue work exceeds every other
candidate's; the pruning point and the pruning proof's level comparison; IBD's heavier-chain
checks; the DNS reorg gate's work dominance. The mergeset's topological order must stay blue-work
order, because the coloring loop relies on processing past-to-future. And it cannot be fenced the
way every other rule here is: it changes the selected parent, the selected parent determines the
mergeset, and the mergeset determines the DAA score a fence would be keyed on — so the fence would
have to be keyed on the parents' DAA scores and every site above re-derived under it. ADR-0058
already rejected its sibling ("weigh the candidate tip's own attempt in fork choice"). Rejected; the
trap does not need it.

**(d) Keep the merged draw blue against a heartbeat-only anticone.** The draw is merged — by the
next heartbeat or the next bonded block, whichever comes first — and under (d) it is merged BLUE, so
its 2²⁰ enters the merging block's blue work and every descendant's. That is the requirement's
second branch, and it holds **whatever the heartbeat miners do**: no cooperation, no timing, no
node-local fact. It is fenced the way ε already is: the rule is decided per merged block from that
block's own header (lane, DAA score), which is fixed before any block that merges it is colored.
What it does not do is make the draw a chain block — the heartbeat that merges it carries its work
and stays the selected parent, so the lane keeps running at the recovery cadence while miners run.
It needs one bound, derived in §5.1. **Chosen as the consensus rule.**

**(e) Operator guidance** — one heartbeat miner per operator, jitter, check the last bonded chain
block's timestamp before restarting a producer. All worth writing down (Decision 3), none a fix: one
miner still stacks eight ε during a seventeen-minute draw, and a restart is not the only way to lose
an hour of bonded production.

## 5. Decisions

### 5.1 Decision 1 (consensus, fenced) — a heartbeat never turns a bonded block red

Past `Params::palw_heartbeat_transparent`, GHOSTDAG colors each mergeset candidate by one of three
rules, chosen by the candidate's own header (`consensus/src/processes/ghostdag/protocol.rs`,
`LaneColoring`):

* **Classic** — below the fence, or on a network that has not armed it: the k-cluster rule over
  every blue, byte for byte what it was.
* **Weighted** — a non-heartbeat block past the fence. **Heartbeats are invisible to it**: a blue
  heartbeat in its anticone is not counted, the heartbeat's own count is not consulted, and neither
  enlarges the other's recorded count. Among non-heartbeat blocks it is the classic rule. The
  classic `|mergeset_blues| = k + 1` shortcut does not apply to it: that bound is a consequence of
  the k-cluster rule (every mergeset block is in the selected parent's anticone) and stops being one
  for a candidate that does not count a heartbeat selected parent — applied anyway, it is exactly the
  rule by which a heartbeat sibling that sorts first takes the only slot. Where the selected parent
  is itself non-heartbeat, its recorded count still caps weighted blues at `k + 1`.
* **Heartbeat** — a heartbeat past the fence: counted against every blue as before, shortcut
  included (it yields to bonded blocks, as it always did), but it never ENLARGES a non-heartbeat
  block's recorded count — that count is what a later bonded candidate is checked against, and a
  heartbeat must not be able to turn a bonded block red through a third block.

**The bound: the exemption stops at the merge-depth window.** A classic blue is inside the
merge-depth window by construction — a block further back has every chain block since in its
anticone — and `check_bounded_merge_depth` relies on that: it checks REDS against the merge-depth
root and never blues. A candidate that ignores heartbeats loses the automatic bound. Unbounded, a
bonded block withheld through a long heartbeat-only stretch would be colored blue arbitrarily deep —
deeper than any red the bounded-merge rule admits, outside the prunality argument's assumptions —
and its coloring walk would run back through every heartbeat to its parent, past a pruned node's
pruning point if it were old enough: a verdict that depends on what a node kept, the F4 shape
ADR-0066 deleted. So once a Weighted walk has passed over a heartbeat, it is red on reaching a chain
block whose blue score is below

    floor = blue_score(selected parent) + (blues already added + candidates not yet colored) − merge_depth

The parenthesis bounds the new block's final mergeset blues, so a candidate whose selected-chain
ancestor is at or above the floor is in the future of the new block's merge-depth root — exactly
where a red must be. At testnet-11's `merge_depth` of 30 blue score this is 13 recovery slots with
two blue heartbeats a slot (four miners give no more than two at `k = 1`), ~26 minutes, and ~56
minutes with one miner. A draw staler than that is red, as it is today, and — as today — cannot be
merged at all (`ViolatingBoundedMergeDepth`). The fleet's seventeen minutes is well inside.

**Keyed on the candidate's own DAA score**, the shape `palw_lane_blue_work_v1` already has: the key
is fixed before any block that merges the candidate is colored, so the rule never depends on the
new block's own GHOSTDAG output — which the new block's own DAA score does — and two builds agree
on every block below the height. The pruning proof's build and validate get the same fence through
`GhostdagManager::with_level`; heartbeats derive no block level, so above level 0 the rule has
nothing to apply to.

**The fence.** `Params::palw_heartbeat_transparent: Option<ForkActivation>`, a bare top-level fence:

| preset | value |
|---|---|
| testnet-11 (`palw_rc_shipped_params`) | `None` |
| devnet (`devnet_shipped_params`) | `None` |
| mainnet, the carded mainnet (`mainnet_card_base_v1`) | `None` |
| testnet-10, simnet | `None` |

Read through `Params::palw_heartbeat_transparent_fence()`, which folds in the mode and the heartbeat
lane (there is nothing for the rule to be about without heartbeats); `validate_palw_v2` refuses it
armed on a network whose lane is not. Wired the way every bare fence is: `palw_fences_v1` (so the
fork-id gate names a scheduled height — `fork_id_gate_fences_v1` is derived from it),
`for_each_fence` (Some-only, at the tail, so the identity normalises a scheduled height and
`fence_schedule_v1` carries it), `consensus_params_id` (Some-only, NAMED and WRITTEN — the step
ADR-0095's `palw_model_benefits` missed, `0448d955`), `consensus_schedule_id` (named, Some-only),
the `never() → None` collapse, and `override_params`. Every pin is unchanged, because a `None`
writes nothing.

### 5.2 Decision 2 (node policy, ships now) — the heartbeat miner steps aside for a landed draw

`palw_heartbeat_v1::heartbeat_yield_hint_v1` answers, from the virtual state (the selected
parent's lane; the lane and timestamp of every other block the virtual merges), one of three
things — three, because the miner has to act on each differently and one `Option` would fold two of
them into `None`:

* `BondedSelectedParent` — the chain is producing; a heartbeat-led episode, if there was one, is over;
* `NothingToYieldTo` — on heartbeats with no attempt block waiting: the regime the lane exists for;
* `YieldUntil(t)` — on heartbeats, and an attempt-lane block is waiting in the virtual's mergeset;
  `t` is the latest such block's `timestamp + HEARTBEAT_NOMINAL_INTERVAL_MS`, the hour the slot rule
  would have granted it had it been selected.

The miner (`kaspad/src/palw_heartbeat_miner.rs`) waits on `YieldUntil` once its slot is open, in
steps of at most a minute, charged to a **budget of one nominal hour per heartbeat-led episode**,
refilled only when a bonded block is the selected parent (observed on any pass, including one the
slot rule holds). Only attempt lanes count as something to yield to: they carry the weight and pay
the class lottery to exist; a receipt-lane block carries no weight a heartbeat could bury.

With the miners yielding, the producer's next draw lands on a tip nobody stacked ε on, becomes the
sink, and the slot rule's nominal hour holds the lane off again — **on the rule testnet-11 runs
today**. Without Decision 1 the first draw's work is still lost (it was red before anyone could
yield); with it, it is kept too.

The budget is what makes this safe to default on: a wedged bond, or a key that pays the lottery with
no bond, can put blocks into the mergeset that never take the chain, and each asks for its hour; the
episode pays for one hour in total and then the lane ticks at cadence until a bonded block really is
the selected parent. In a total collapse there is no attempt block and nothing to yield to. It is
advice: a heartbeat mined anyway — by a miner on an older build, or a stranger's — is exactly as valid
as before, and such a miner can keep the chain heartbeat-led (Decision 1 is what keeps that harmless).

### 5.3 Decision 3 (operators)

[`testnet11-node-operator.md`](../history/networks/testnet-11/testnet11-node-operator.md) §7a: how to recognise the mode (every
block algo-8; `getDnsConfirmation` with `pow_confirmed = false` and `workDepth` below
`requiredWorkDepth`), how to escape it on a build without Decision 2 (stop every heartbeat miner for
one draw), and how not to enter it (check the last bonded chain block's TIMESTAMP before restarting
a producer; at most one heartbeat miner per operator, on the current build). The Relaunch-5 runbook's
"heartbeat miners on every host" is corrected there.

## 6. Safety and liveness, checked against the doctrine

* **The clock in a total collapse is byte-identical.** Decision 1 touches coloring only; the slot
  rule, the lane's price, ε and the width bound are unchanged. Decision 2 yields only to attempt
  blocks that exist.
* **A heartbeat flooder** loses a power it had: at `k = 1` two sibling heartbeats could make any
  bonded block beside them red; past the fence they cannot. Heartbeats still yield to bonded blocks
  and still meet the width bound (F3a), so flooding buys what it bought before — ε and fees — minus
  the ability to erase bonded work.
* **A bonded producer that backdates or forward-dates.** Decision 1 is keyed on DAA scores, which
  are derived from a block's parents rather than written by its producer, and ignores timestamps. Decision 2 reads the timestamp: backdating
  shortens the yield it earns; forward-dating lengthens it by at most the future-time tolerance, and
  the budget caps the episode either way.
* **A withheld bonded block** is blue only inside the merge-depth window — where a red would be
  admitted — and adds its weight to whichever chain merges it. A private chain gains nothing: its
  own bonded blocks are its chain blocks. **(Wrong as written: a private chain that merges the
  PUBLIC chain's attempts gains all of their weight — §11, closed past
  `palw_heartbeat_transparent_same_chain`.)** The honest chain in a heartbeat-led episode is now as
  heavy as its bonded production, where before one private attempt block out-weighed the whole
  episode.
* **Sibling width (ADR-0066 F3a)** — unchanged, and no longer able to redden bonded blocks.
* **Blue score** can grow by more than `k + 1` per block past the fence: heartbeat blues and bonded
  blues no longer compete for the same `k + 1` slots, and under a heartbeat selected parent bonded
  candidates that are not in each other's anticone are all blue. Nothing in consensus assumes the
  bound (the coinbase cap is `mergeset_size_limit + 1 + extras`), and blue ⊆ mergeset, so blue
  score still grows no faster than the mergeset the DAA score counts.

## 7. What arming it takes

**testnet-11 (a flag day):** schedule `palw_heartbeat_transparent = Some(ForkActivation::new(H))` in
`palw_rc_base_params` with lead enough for every seat to rebuild, the way DAA 1150, 1900, 2150 and
2400 were scheduled. The printed fingerprint (`ecbdbc22…` in the release this ADR landed in) moves; the identity does not, so a build
without it peers until it is refused. The fork-id gate names H — and, as the third flag day measured,
an older build whose gate is already armed refuses the upgraded peer the moment it announces
`next = H`, so the notice must say *rebuild now, not by H*. Past H every node must carry the rule:
the blue set, blue work and blue score of every block whose mergeset holds a bonded block beside a
heartbeat anticone differ between the two rules. Pins to move deliberately: testnet-11's row in
`shipped_presets_have_pinned_fingerprints`; `the_shipped_schedules_are_measured_not_assumed` and
`the_gate_is_armed_only_where_a_shipped_preset_schedules_a_gate_fence` (the schedule gains H);
`fork_id_gate_fences_v1`'s invariance list in `the_fork_id_gate_names_every_scheduled_palw_fence`.

**A carded mainnet** can state it from genesis (`always()`), where it is free and the identity moves
by construction. Not done here: the card's armed set is pinned in
`a_carded_mainnet_arms_every_fence_testnet_11_arms` and mirrored in the README's activation table,
and changing what a card arms is a decision to take with that table open — ideally after a devnet
drill with the fence armed, which this change did not run.

## 8. Invariants the tests hold

* `a_slow_bonded_draw_is_red_behind_heartbeats_until_the_fence_keeps_its_work` — the incident in the
  pipeline (V2 at 120 s, heartbeat and attempt-work armed, two heartbeats a slot, draws landing 17
  minutes after their templates). **Fence dormant**: every draw red; each merging heartbeat adds
  **2** (two ε); no draw is ever a chain block; the chain stays at the recovery cadence; the work over
  the whole ~40-minute episode is **36 < 100**. **Fence armed**: every draw blue; each merging
  heartbeat adds **1,048,577** (2²⁰ + ε); the episode's work is **2,097,186**; and — stated — no draw
  is a chain block (Decision 1 keeps work, it does not hand the chain back).
* `the_fence_keeps_a_slow_draw_blue_only_inside_the_merge_depth_window` — blue exactly when the
  draw's chain ancestor is at or above the floor: 8 and 13 slots blue, 14 red, checked against the
  formula from stored blue scores; dormant, 8 slots red.
* `a_yielding_heartbeat_miner_hands_the_chain_back_to_the_next_draw` — both fence positions: the hint
  reads `BondedSelectedParent`, then `NothingToYieldTo` on heartbeats, then
  `YieldUntil(draw.timestamp + 1 h)` when the draw lands; with nobody mining, the next draw is the
  sink, the hint returns to `BondedSelectedParent`, and the next heartbeat's earliest slot is that
  draw's timestamp plus the nominal hour. The first draw is blue exactly when the fence is armed.
* Mutation check, run once while writing this: with `lane_coloring` forced to `Classic`, all three
  tests fail at their armed assertions.
* `palw_heartbeat_v1::the_yield_hint_separates_a_producing_chain_from_a_quiet_clock`; the miner's
  `yield_budget_tests` (a stream of blocks that never take the chain buys exactly one hour; only a
  bonded selected parent refills; minute steps; saturated deadlines).
* `the_heartbeat_transparent_fence_is_dormant_scheduled_or_a_rule_and_never_hides` — `None` on every
  preset; on testnet-11 a scheduled height keeps the identity and moves the fingerprint, the schedule
  id, the fence schedule and the fork-id gate; at genesis it moves the identity; refused without the
  lane.
* `fork_id_v1::every_scheduled_palw_fence_moves_the_fingerprint_and_the_schedule_and_never_the_identity`
  — for EVERY PALW fence, two future heights must print different fingerprints and schedule ids and
  one identity. Written listing `palw_model_benefits` as the one fence whose height the fingerprint
  did not carry (known since `0448d955`); the release this ADR landed in writes it (testnet-11's pin
  060e3597… → ecbdbc22…) and the list is empty, so the next fence forgotten the same way fails by name.

## 9. What is deliberately not decided, and what is not verified

* **Whether the chain should hand itself back without the miners' help.** Decision 1 keeps the work
  and Decision 2 ends the mode when the miners cooperate. A heartbeat miner that does not yield keeps
  the chain heartbeat-led — harmlessly past the fence, since every draw's work is counted. Making the
  bonded lane the selected chain again against an uncooperative miner needs chain state (whether a
  bonded block may become the sink), which is not one block deep; this ADR does not buy it.
* **Not verified on a network.** No multi-node drill has run with the fence armed; the pruning proof
  path with the fence armed is covered only by construction (the same `ghostdag()` with the same
  fence at both proof sites), not by a test that builds and validates a proof across it; and the
  yield has been exercised by unit tests and the pipeline hint, not by a running `kaspad` against a
  live chain.

## 10. Number hygiene and implementation record

Written as **0102**: `docs/adr/README.md` on `main` said the next free number was 0099, and 0099–0101
were resident on other branches. 0102 had already been taken the same morning on another branch
(the embedding lift, first written at 04:22), and a concurrent claimant renumbers the later writer —
this one. The sessions writing 0103 (held context), 0104 (close cut), 0106 (streaming inventory) and
0107 (share growth) held **0105** for it, so it was renumbered on 2026-09-11 after landing on `main`
(`ea2fd48e`); the commits of that day and `fix/heartbeat-trap-slow-producers` still say ADR-0102. The
fence name, the code and every test are unchanged. **The next free number is 0108.**

* **2026-09-11** — written and implemented on `fix/heartbeat-trap-slow-producers`:
  * `consensus/core/src/config/params.rs` — the fence, its accessor, the `validate_palw_v2` refusal,
    the identity/fingerprint/schedule wiring, and its test.
  * `consensus/core/src/fork_id_v1.rs` — the probe arm and the every-fence fingerprint test.
  * `consensus/core/src/palw_heartbeat_v1.rs` — `HeartbeatYieldHintV1`, `heartbeat_yield_hint_v1`.
  * `consensus/src/processes/ghostdag/protocol.rs` — `LaneColoring`, `HeartbeatTransparency`, the
    walk floor; `consensus/src/consensus/services.rs` and `processes/pruning_proof/{mod,build,validate}.rs`
    thread the fence to every GHOSTDAG manager.
  * `consensus/src/pipeline/virtual_processor/processor.rs` — `heartbeat_yield_hint`; the
    `ConsensusApi` method, its `Consensus` impl and the session proxy.
  * `kaspad/src/palw_heartbeat_miner.rs` — the yield and its budget.
  * `consensus/src/pipeline/virtual_processor/tests.rs` — the three pipeline tests above.
  * `docs/history/testnet11-node-operator.md` §7a; `docs/history/testnet11-relaunch5-runbook.md` (correction note);
    a pointer in ADR-0066.

  Verified on that tree with `MISAKA_PALW_POW_FIXTURE=1`: `cargo test -p kaspa-consensus-core --lib`
  1,973 passed / 0 failed / 10 ignored; `cargo test -p kaspa-consensus --lib` 295 / 0 / 7 (the three
  pipeline tests above and every existing heartbeat test among them); `cargo test -p kaspad --lib`
  85 / 0 / 0; `cargo test -p kaspa-testing-integration --lib -- ghostdag_test pruning` 3 / 0 / 3
  (`ghostdag_test`'s golden DAGs run the classic rule, which this change must leave byte-identical);
  `cargo clippy --tests` on the four touched crates adds no finding. `shipped_presets_have_pinned_fingerprints`
  passes unchanged: no pin moved.

## 11. Amendment F1 (2026-09-25): transparency stops at the merging block's own chain — a post-launch flag day

* **The hole.** §5.1 as written makes a non-heartbeat candidate blind to heartbeats in EVERY block
  that merges it. The heartbeat double-spend verifier (workflow `wf_35c50751-12d`, on the eve of
  testnet-12's launch — the one network that arms the fence, from genesis) built what §6's "a private
  chain gains nothing" missed: a bondless heartbeat miner forks with the conflicting spend Y and
  merges the public chain's attempts into its private heartbeat branch. There their anticone holds
  only the attacker's heartbeats, so they are blue there too: every public 2²⁰ counts on both
  branches, and the private one wins by its own ε (g2: +2,097,158 against +2,097,157 — X reorged
  away, Y kept; h1 absorbed the attempts after an X-carrying one; f let one bond out-weigh two; h2
  borrowed eight before merge depth 30 reddened the ninth — on the release, nine in a row, the tenth
  left out of the attacker's next block). Reach: the merge depth, ~30 blue,
  ~5 DAA, ~10 minutes of the public chain. Independent of the beat lead cap and of the 132 s /
  1,620 s tolerance.
* **Why a fence and not a genesis change.** testnet-12 launched on release `0e8ec984e` with §5.1 as
  written. The fix changes which blocks are blue past a height, so it ships as a flag day:
  `Params::palw_heartbeat_transparent_same_chain`, a bare Some-only fence, `None` on every shipped
  preset — testnet-12 included, whose params, identity and schedule ids (`b8564b88…`, `5de80e64…`,
  `93da24cc…`) stay the release's — until an operator schedules it at a height `H`. The operator
  picks one common height for every post-launch fence, and it must be a height no other fence uses
  (the fork id sorts and dedups heights, so a fence at a height already scheduled is invisible to it).
  **Arm it after `palw_t12_base_params`' pass 2**, as `Some(ForkActivation::new(H))`: that pass walks
  every fence height to 0, so a line added before it — or at the builder's `at`, which is `always()` —
  makes this a genesis rule, a new identity, and every launched node refused at once rather than at
  `H` (`on_testnet12_it_is_a_height_never_a_genesis_rule` guards it).
* **The rule.** Past `H`, keyed on the candidate's own DAA score, a non-heartbeat candidate is
  `Weighted` (transparent to heartbeats) only when it **hangs from the merging block's own selected
  chain**: its selected parent is the merging block's selected parent, or one of that block's
  selected-chain ancestors at most `merge_depth` blue score below it
  (`GhostdagManager::hangs_from_the_merging_chain`). Any other non-heartbeat candidate is `Classic` —
  counted against every blue, the other branch's heartbeats included. Heartbeat candidates are
  unchanged. Below `H` the code path is the shipped one, byte for byte: the walk runs only for a
  non-heartbeat candidate at or past `H`, so a block that merges no such candidate reads exactly what
  it read before. The fence rides `HeartbeatTransparency` beside the merge depth, so every site that
  colors — the header path, the virtual, the pruning proof's build, validate and apply — carries it
  with the rule it narrows, and `validate_palw_v2` refuses it unless `palw_heartbeat_transparent` is
  in force at or below it.
* **Why the switch is keyed on the candidate's own DAA score.**
  * *Fixed before any block that merges it is colored*, and it is the key the transparency fence
    already uses — so a candidate is colored by one rule in every block that ever merges it, on every
    node, whatever chain the merging block is on. A block's GHOSTDAG data is a pure function of its
    past, computed once and never recomputed on a reorg, so a reorg whose two sides straddle `H` needs
    nothing: each block keeps the coloring its own past gives it, and the virtual's is recomputed from
    its parents like any block's.
  * *Out of the attacker's hands.* The blocks being absorbed are the PUBLIC chain's attempts, whose DAA
    the public chain sets. Keyed on the merging block instead (its selected parent's DAA score), a
    private branch whose clock lags the public one — every branch steps its own clock at most once a
    slot, and a withholding miner may step it less — could keep merging post-`H` public attempts under
    the old rule for as long as its own tip stayed below `H`.
  * *In the header*, so the pruning proof's build, validate and apply and an IBD node read the same
    number; a computed blue score is not (a proof level computes blue scores from its own root).
  * The consequence to operate by: a block whose own DAA score is below `H` can merge a candidate at
    or past `H` and color it by the new rule, so the launched build and the armed build can disagree
    about a block stamped just below `H`. Every node must run the armed build **before the network
    reaches `H`**; from `H` the fork id refuses a node that does not.
* **The property it keeps.** ADR-0105 is for a draw that lands late on the chain it was drawn on: its
  template's selected parent `T` was the sink, and the heartbeats minted while it computed were minted
  on `T`'s chain, so `T` is on the merging block's chain and the draw is transparent exactly as before.
  §8's three pipeline tests are unchanged, and
  `hb_regression_an_8k_draw_200s_late_stays_blue_on_its_own_chain` measures it on testnet-12 at an 8k
  draw's ~200 s with one and with two heartbeat miners, as launched and past the fence (blue, the
  merger carries 2²⁰; the control with ADR-0105 taken away is red).
* **What a branch can still count.** Only attempts that hang from its own chain — on a branch that
  forked at `F`, attempts drawn at or below `F`. The other branch hangs from those as well, so they
  count on both and decide nothing (`hb_regression_h1_…`). Attempts minted below `H` stay borrowable
  under the old rule until they fall out of the merge depth — the window a scheduled fence leaves is
  the ~10 minutes before `H` (`hb_regression_the_fence_splits_an_attack_at_the_candidates_own_daa`:
  the attempt below `H` is borrowed, the one at `H` is not, and one borrowed attempt no longer beats
  two). The heartbeat-only regime (sibling layers, run-ahead — probes b) and attempt parity (c) are
  untouched and out of scope; the finality depth (d) and a `Final` anchor (e) still stop a reorg.
* **What it gives up.** A draw whose template's selected parent leaves the chain before the draw lands
  — in practice a template taken while only the losing one of two sibling heartbeats had arrived — is
  colored classically, i.e. red, as testnet-11 colors every late draw today
  (`hb_regression_the_same_chain_rule_colors_the_same_through_the_pruning_proof`'s draw 2: blue as
  launched, red past `H`). No DAG-only rule can keep it blue and close the hole: the absorbed public
  attempt of g2 hangs one heartbeat off the attacker's chain exactly as that draw hangs one heartbeat
  off the honest chain, and admitting "one heartbeat off" re-opens f (one bond plus one borrowed
  attempt beats two). The exposure is a template taken inside the propagation window of a sibling
  layer — about the propagation delay times two layers a slot, a percent or so of draws where the
  fleet mines sibling layers; not measured on a network. A node-side mitigation (take a draw's
  template only once a heartbeat layer has settled) is open.
* **Deterministic from the DAG — walked on the GHOSTDAG store, not asked of the reachability tree.**
  The candidate's selected parent is its stored GHOSTDAG output, and the walk follows stored selected
  parents down from the merging block's selected parent. The reachability tree would give the same
  answer on the header path and in the proof's build and validate, which all make the GHOSTDAG
  selected parent the tree parent — but not on a node that applied a pruning proof: `apply_proof`
  fills the tree below the pruning point from the heaviest parent by header blue work, which does not
  know the round lane (ADR-0125: a round block is never a selected parent), so a round block tied with
  a chain block on blue work and winning the hash tie is that node's tree parent and not its selected
  parent. A verdict read off the tree would then differ from the network's in a block right above the
  pruning point and the syncing node would refuse it; the stored selected parent is what every path
  computes (`find_selected_parent`, or the syncer's own for a trusted block). The walk is bounded:
  blue score strictly falls along a selected chain, so it visits at most `merge_depth + 1` blocks, and
  a candidate whose selected parent sits deeper is answered `Classic` without walking, so a block
  cannot make a node walk its chain by merging an old block. (Such a candidate is red under the
  transparent rule too wherever its coloring walk passes a heartbeat — the merge-depth floor comes
  before its own ancestor — and where it passes none the two rules count the same blues.) A candidate
  whose selected parent is ORIGIN hangs from no chain. A read that
  fails (a walk that runs off a proof level's truncated root) answers `Classic` on every node that
  holds the same level. Measured: the victim of every absorb regression colors each released block
  exactly as the attacker that built it did; and on testnet-12 (depths shrunk) a proof whose level 0
  holds a hanging draw and a non-hanging one is built, validated by a node at genesis, applied by a
  staging node that colors every level-0 header as the source did, and built back — as launched and
  past the fence.
* **The fingerprint and the handshake.** Hashed Some-only in `consensus_params_id`,
  `consensus_schedule_id` (named) and the `for_each_fence` walk, collapsed from `Some(never())` in
  `normalize_values_a_scheduled_fence_drags_with_it`, and named in `palw_fences_v1`, the fork-id probe
  and the ruleset-candidate kind. Scheduled at `H` on testnet-12: the params id and the schedule id
  move, the identity does not, so the armed build and the launched build peer through the rollout
  (`handshake_rules_verdict_v1` keeps a peer whose identity agrees). The fork id names `H` — beside
  the one height the launched testnet-12 already schedules, ADR-0065 D1's bond-maturity window at
  **1,000** (armed AT its window), which also means the launched build's fork-id gate is armed:
  * **`H` must not be 1,000.** The fork id carries heights, deduplicated, never names: at 1,000 the
    armed and launched builds announce identical fork ids at every DAA, neither ever refuses the
    other, and past 1,000 they fork silently.
  * **`H` below 1,000** (inside the first ~33 hours): nobody is refused before `H`; from `H` the armed
    build refuses the launched one (`DisagreePastFence { fired_through: H }`), and a connection either
    side made below `H` is re-judged at `H`.
  * **`H` above 1,000**: nobody is refused before `H`, and from `H` the armed build refuses the launched
    one at a handshake — **but a connection an armed node made while the chain was below 1,000 is
    never re-judged**: the launched peer announced `next = 1,000`, which the armed schedule also has
    next, so the snapshot the connection layer stores agrees at every height and the straggler stays
    connected past `H` until the connection is re-made (block validity still refuses what it relays).
    Re-make the armed fleet's connections (restart) once the chain is past 1,000, or prefer `H` below
    it.
  Two consequences the notice must carry: a node syncing a launched peer's chain past `H` stops at the
  first block the two rules color differently; and, as for every scheduled fence, the block-relay
  recovery path ignores IBD-candidate summaries from peers whose params id differs, so during the
  rollout a mixed fleet recovers only from its own build.
  (`palw_hb_transparency_same_chain_fence` holds all of it.)
* **Tests** (`hb_fork_choice_probe`, each attack run as launched — the verifier's measurement kept as
  the control — and fenced): `hb_regression_g2_…`, `_h1_…`, `_h2_the_horizon_is_zero_past_the_fence`,
  `_f_one_bond_cannot_borrow_a_second_attempt` (the double spend lands as launched and does not past
  the fence); `_the_fence_splits_an_attack_at_the_candidates_own_daa`;
  `_below_the_fence_the_armed_build_is_the_launched_build` (g2's attack DAG replayed into a node with
  the fence scheduled above it: every block colored identically and the same double-spent UTXO set;
  into a node with it armed below: the absorbing blocks are refused and X stands);
  `_the_fence_crosses_an_honest_chain_and_the_clock_runs` (fence at DAA 8: every honest slot steps the
  DAA score by exactly one before, at and past the height, heartbeat-only stretches on both sides, late
  draws with one and two heartbeat miners blue on both sides, two draws on one template both blue, and
  a node without the fence colors all of it identically); `_an_8k_draw_200s_late_…`;
  `_the_same_chain_rule_colors_the_same_through_the_pruning_proof`.
* **What a drill across `H` must check** (the rule changes coloring, so the drill must cross the
  height, not reach it, on the lane profile of testnet-12): `H` a few slots above the drill's start;
  (1) the DAA score keeps stepping one per slot across `H` on every node, with heartbeat-only stretches
  before and after it; (2) late attempts landing behind the heartbeat tip on the SAME chain are blue in
  their merging heartbeat before and after `H` (`getBlock` blues/reds of the merger), with the fleet's
  sibling heartbeat miners running; (3) every node reports the same sink, blue score and blue work at
  the same DAA — no split at `H`; (4) a node on the launched build is kept below `H` and disconnected at
  `H` with the named fork-id refusal — including a connection made before the drill's own
  maturity-window height when `H` is above it (the re-judge gap: expect it to survive, and verify the
  restart that re-makes it) — and an armed node restarted from an empty datadir after `H` syncs through
  `H` (IBD and pruning-proof paths) to the same tip; (5) the absorb attack replayed on the drill
  (a withheld heartbeat branch merging the public attempts after `H`, then released) leaves X in place;
  (6) the same attack with its attempts minted below `H` shows the ~10-minute window close as they
  leave the merge depth; (7) evidence from the run's own logs after the action, not from configured
  values.
~~~~~~~~
<!-- ADR_SOURCE_END:0105-a-heartbeat-never-turns-a-bonded-block-red.md -->

</details>

<a id="source-0140-the-heartbeat-is-the-emergency-generator"></a>

<details>
<summary>0140-the-heartbeat-is-the-emergency-generator.md — 原文 L1–L248</summary>

[原文を開く](../adr/0140-the-heartbeat-is-the-emergency-generator.md) · SHA-256: `936520f39e1d6ff3d8b11a6f611119443319013451341a615df81b9467493a88`

<!-- ADR_SOURCE_BEGIN:0140-the-heartbeat-is-the-emergency-generator.md -->
~~~~~~~~markdown
# ADR-0140 — The heartbeat is the emergency generator: it must not touch the economy or the difficulty while the chain is producing

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **Changes no
consensus rule and arms no fence.** It settles what the goal is, keeps the hash puzzle, states the
non-interference invariant as five separate claims, and says which of them the code already
guarantees and which are only believed.

**Builds on:** ADR-0060 (the liveness doctrine), ADR-0064 (trustless recovery from a total stop),
ADR-0066 (the heartbeat out of `bits`), ADR-0083 Decision 1 (only priced rows price the window),
ADR-0105 (what a heartbeat miner is told), ADR-0138 §3c (the heartbeat's interval follows the clock).

## 1. The goal, corrected

An earlier draft of this ADR set out to replace the heartbeat's hash puzzle with something that
prices elapsed time without a hash. **That is the wrong goal**, and stating why is the most useful
thing this document does.

Removing a hash function is not an end in itself. Block ids, Merkle roots and commitments are hashes
and are not going anywhere. What this chain is built to avoid is the *other* thing, the one that made
"proof of work" a synonym for it:

> a large amount of hashing, where whoever holds the most hash power controls economic block
> production.

That is already not how MISAKA works. Economic block production is one deterministic inference, its
execution commitment, and the attempt or receipt that carries it. The heartbeat is a bondless,
fee-only, near-weightless liveness lane beside it, and its hash is not model work and not an
alternative way to mine.

So the goal is not "no hash". The goal is:

> **While the chain is producing normally, the heartbeat lane must be unable to touch the economy,
> the difficulty, the fork choice or the clock — and in an emergency it must be able to carry all
> four on its own.**

The hash puzzle is a fine price for that. It is simple, permissionless, verified in microseconds,
already shipped and already tested. **It stays.**

## 2. The shape this should take in operation

```
NORMAL
    PALW attempt / receipt ──► economic block production
    heartbeat producers    ──► asleep, ~0 hash spent

FAILURE
    PALW silence, observed LOCALLY by each miner
    heartbeat producers    ──► awake
                           ──► bounded hash puzzle
                           ──► clock advances, transactions carried, lifecycle sweeps
    PALW recovers
    heartbeat producers    ──► asleep again
```

This is the right shape and most of it is already built: ADR-0105's yield hint is exactly the "go
back to sleep when a bonded block lands" signal, and it is deliberately **policy**, not a rule.

## 3. The one distinction that must never be lost

**"When to mine a heartbeat" is a miner's local policy. "Whether a heartbeat is valid" is consensus,
and it must not depend on whether PALW was producing.**

A rule of the form

```
if a PALW block was seen recently:  heartbeat invalid
if no PALW block was seen:          heartbeat valid
```

cannot be written. ADR-0064 Fact A is the theorem: **silence is not checkable from a fork.** A branch
sees only what it contains, so two partitions disagree about whether PALW was producing, and
heartbeat eligibility becomes a function of which branch you are on. ADR-0105 §4 reached the same
conclusion from the attacker's side and called it fatal: a bond at its exposure ceiling, or any key
able to emit a header-valid attempt, could stop the clock an hour at a time, and one block of depth
cannot tell that from an honest draw.

The same theorem closes the other tempting shortcut. **A heartbeat signed by a bonded producer** is
forbidden by ADR-0060 §2 — "accountability implies killability" — because a bond can be slashed,
capped, exited or wrongly convicted, and a clock that depends on one dies with it.

So, in consensus, a heartbeat is **always admissible under the same fixed conditions**, and the
safety comes not from restricting *when* it may exist but from making it *worth nothing* while the
chain is producing. That is §4.

## 4. The invariant, as five separate claims

This is the substance of the ADR. Each claim is separate, each has its own mechanism, and four of
the five are already closed by earlier ADRs — which is worth writing down, because "we believe the
heartbeat is harmless" is not the same as "here is the rule that makes it harmless".

### C1 — It must not tighten the difficulty

**Closed, twice.** ADR-0066 took the lane's price out of `header.bits` and into a network constant,
so a heartbeat header carries the global expected bits like every other row. ADR-0083 Decision 1 then
made the retarget count only rows a price paced.

This one is not hypothetical. Measured over the shipped 264-row window before the fix: 255 bonded
plus 9 heartbeat rows still demanded work 2, while **0 bonded plus 263 heartbeat rows demanded
33,554,432**. After a bonded outage longer than the window, a returning producer needed tens of
millions of inferences for one block, so no bonded block could re-enter the window and the average
never re-mixed — the emergency generator locking out the engine it was there to restart.

### C2 — It must not pace the clock while something else is pacing it

**Closed by ADR-0138.** A block advances the DAA score only where `bits` priced it, and a heartbeat
gives back exactly one exemption only in a mergeset that carries no priced block at all. Where the
economic lane produces, beats add nothing to the score. §3c then made the *interval* follow the same
question, so the lane also stops competing for slots while a parent is pacing the clock.

### C3 — It must not take fork-choice weight from the economic lane

**Closed by ADR-0060 Decision 1.2.** A heartbeat weighs ε = 1 against an attempt block's fixed 2²⁰.
A million beats do not outweigh one bonded block, and among heartbeat-only branches ε × n still
orders the longer one first, which zero would not.

### C4 — It must not earn, and must not capture block production

**Closed by the lane's definition**: bondless, claimless, fee-only, no subsidy. There is nothing to
win by minting one while the chain is healthy, which is what makes the permissionlessness of §3 safe
rather than dangerous.

### C5 — and in an emergency it must actually start

The invariant has two halves, and C1–C4 are only the first. The generator also has to turn on.

This half failed on the drill while this ADR was being written, and the failure was in the sleep/wake
policy rather than in any rule. ADR-0105's yield exists so a beat does not bury an attempt block
waiting to be merged. Past the anchor clock an attempt block carries its weight but advances no DAA,
so standing aside for one waits on a block that will not move the clock — on a chain where nothing
else will either. Worse, the economic lane produces continuously, so the yield target kept sliding
forward and the lane stood aside from the very job it exists for, until its hour-long episode budget
drained. Observed directly: the fence crossed, the DAA frozen at 20, the miner running and never
minting.

The fix is §3c's principle applied to the mergeset scan rather than the parent: **yield to whoever is
pacing the clock, and to nobody else.** It is policy, not a rule, which is exactly where D4 says this
decision belongs — and it is why D4 is a decision rather than an observation.

It happened a second time within the hour, and the second one is the more instructive. ADR-0138 §3c
changed the slot rule's interval on the VALIDATING side and left the template adapter calling the
pre-fence form, so a node would have stamped the nominal hour onto a template its own validator
granted the recovery cadence — putting the timestamp an hour into the future, where the drift rule
refuses it outright. ADR-0066 Decision 2 had already written down that construction and validation
must read one answer. The requirement was in the file; nothing enforced it. It does now: a guard
walks the workspace and fails if anything outside the module asks the pre-fence entry points, and it
found the one remaining caller on its first run.

A third thing was wrong and is worth naming separately: **the lane was silent about all of it.** Every
wait in the miner was a `trace!`, so a heartbeat producer that could not mint said nothing at the
level anyone runs. Both failures presented identically from the outside — a running miner, a frozen
score, no line explaining either. The waits now print on a change of reason and then at most every
five minutes, and name the state to watch for.

The lesson generalises past these bugs. C1–C4 are about the generator not running when the mains are
on; C5 is about it starting when they go off, and the two are easy to trade against each other
without noticing. A guard that only checks the first half would have passed all three.

### What is NOT closed

The five claims above are each enforced somewhere, but **no test asserts them as a group**, and
nothing fails if a future change quietly breaks one. Every one has been broken at least once — C1 in
the first heartbeat implementation, C2 by ADR-0132 S leaving the DAA behind, C2 again twice inside
ADR-0138's own drafting, and C5 by the yield policy above. That is the work this ADR leaves behind:
not a new primitive, a standing guard.

## 5. Why not PoSW or a VDF

Both remove the *parallel* advantage: a thousand machines do not shorten `T` sequential steps. Neither
fixes real time — faster single hardware still goes faster, which is why Chia has ASIC timelords —
so the honest claim is "an attacker cannot buy speed by buying width", not "cannot go faster than
wall-clock". Class-group and RSA VDFs are also broken by a quantum adversary, which a chain that
picked ML-DSA-87 should not adopt; the post-quantum option is hash-based sequential work.

Against that, the cost of adopting one is a new cryptographic primitive, a new proof format, new
verification code, a new denial-of-service surface and new performance assumptions — **for a lane
that should be idle almost always.**

**Decision: not now, and the condition is a measurement, not an opinion.** If the heartbeat share of
blocks over a long run is under a few tenths of a percent, the hash puzzle stays and this question
closes. The measurement is D5.

## 6. Where the hash that matters actually is

Three uses survive, and the emergency generator is the smallest.

| # | where | what it is | verdict |
|---|---|---|---|
| 1 | identity, Merkle roots, commitments | a hash function used as a hash function | keep, nothing proposes otherwise |
| 2 | the heartbeat's constant target | the spam price of a lane that should be idle | **keep** — §1, §5 |
| 3 | **the attempt lottery, `ticket digest < target`** | rate control on the economic lane | the real question — [ADR-0141](0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) |

A producer pays for one full inference *before* the third comparison happens, and most draws lose.
ADR-0071 §1 names the same coupling from the inside. If the aim is to reduce hashing that shapes who
produces blocks, that is where it lives — not in the generator that runs when the lights are out.

An LLM inference cannot replace either one as a clock, for a structural reason rather than a
preference: a clock lane must be permissionless, self-verifying and expensive to generate, and a
forward pass fails the second because the verifier pays what the producer paid. Pushing verification
to a court closes a circle, since the court is scheduled by the clock. A *micro-PALW clock class*
would be a proof of work with the hash swapped for a neural network, and it would put the model
runtime on the path that has to work when the network is most broken — the opposite of ADR-0060's
dependency direction. **Rejected as a clock; recorded as a candidate determinism canary**, which
carries no clock role and no weight.

## 7. Decision

**D1. The hash heartbeat stays.** It is the emergency generator's price. Replacing it is not a goal.

**D2. The goal is non-interference while the chain is producing, and reliable starting when it is
not**, stated as C1–C5 and enforced by the mechanisms named there.

**D3. Consensus admissibility stays unconditional.** No bonded signature, and no rule keyed on
whether PALW was producing. Safety comes from the lane being worth nothing, not from restricting when
it may exist.

**D4. When to mine is policy and stays policy.** ADR-0105's hint is the mechanism; a miner sleeps
while a bonded block is pacing the chain and wakes when none is. No consensus rule reads it.

**D5. C1–C5 get one standing guard, and the lane gets counted.** A test that asserts the five claims
together — including the second half, that the lane starts when nothing else is pacing the clock — so a future change that breaks one fails here rather than on a live network; and counters
for what share of blocks the lane actually produces, how often it is the clock, how many transactions
it carries and how long its episodes last. The counters are what close §5's question.

**D6. The attempt lottery is the real hash question**, and it is ADR-0141's.

**D7. No zero-knowledge machinery anywhere in this line of work.** Bounded deterministic verification
plus commitments is the existing doctrine and it suffices.

## 8. What this ADR does not decide

* Whether PoSW or a VDF is ever adopted. D5's measurement is the gate, and the expected answer is no.
* Whether any lifecycle deadline moves from DAA to elapsed time. That was explored while this ADR was
  drafted and is deliberately dropped: median time past is itself derived from timestamps a branch
  declares (ADR-0064 Fact A), so moving deadlines onto it would raise the stake on the clock lane's
  price rather than lower it. If it is ever revisited it needs its own ADR and this one's C1–C5 in
  force first.
* Whether the attempt lottery can go. ADR-0141.
* Anything in the 7,101 bundle. This ADR is scheduled behind that rollout and behind arming ADR-0105:
  **roll out, arm, measure, then decide.** Nothing here requests a height.
~~~~~~~~
<!-- ADR_SOURCE_END:0140-the-heartbeat-is-the-emergency-generator.md -->

</details>

<a id="source-0151-liveness-is-structural-collateral-covers-fraud"></a>

<details>
<summary>0151-liveness-is-structural-collateral-covers-fraud.md — 原文 L1–L383</summary>

[原文を開く](../adr/0151-liveness-is-structural-collateral-covers-fraud.md) · SHA-256: `321165813e67d661c0cbf5c6e7bf130b2942774120f1d358247f880e75816930`

<!-- ADR_SOURCE_BEGIN:0151-liveness-is-structural-collateral-covers-fraud.md -->
~~~~~~~~markdown
# ADR-0151 — Liveness is structural; collateral covers fraud

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **D2, D3, D4 and D5 in force on testnet-12 from genesis; D1's genesis half landed, its
  runtime half open; D6 partially.** D4 and D5 were drafted as OPEN from a description of the defect
  and turned out to be already built — that correction is recorded in their sections rather than
  edited away.
  testnet-12 is the last testnet before mainnet, so the operator's decision (2026-09-22) was to start
  it on the separated design rather than ship the capital buffer and remove it later.
* Date: 2026-09-22
* Retires, on testnet-12: the `window_bind × dearest claim` genesis collateral requirement in
  `palw_genesis_v2` (`BondCannotSustainBindWindow`). It stays in force wherever the structural
  guarantee is not proved.
* Related: ADR-0061, ADR-0066 (the beat is outside `bits`), ADR-0071, ADR-0133 §7/§11.3,
  ADR-0137 (a share is a result), ADR-0138 §3b (the stand-in tick), ADR-0142 (the clock cursor),
  ADR-0144 §9 (the lock ledger), ADR-0145 (work is derived).

## 1. The defect, measured

testnet-12 registers the held Qwen2.5 row at `n_ctx` 2,097,152 — the class the fleet already mines.
Sizing its genesis bonds produced four figures, and the distance between them is the subject:

| figure | what produced it | per seat |
|---|---|---|
| 38,889,673.34 MSK | `palw_v2_collateral_for_claim_lifetime_v1`: `max(per-claim) × (MAX_CLAIM_EXPOSURE_DAA + 1)` = ×7,201 | the derivation as shipped |
| 1,620,178.03 MSK | `verify_palw_genesis_v2`: `dearest per-claim × window_bind` = ×600 | the genesis liveness bound |
| **21,630.05 MSK** | reachable concurrency: floor at 7,201 claims + each model row at its in-flight cap ×4 | **what testnet-12 ships** |
| 5,400.59 MSK | ONE 2M claim's exposure | the unit all of them are built from |

The first is a straightforward over-derivation, caught by the operator: it applies the FLOOR's
concurrency — claims arrive one a block, nothing caps them — to a class the class-local admission gate
caps at one in flight. 7,201× where 4× would do.

The second is not a pricing error, which is why it survived the correction. It defends a cycle:

```
collateral exhausted -> no claim admitted -> no block produced -> DAA frozen
  -> no BindTimeout -> collateral never released
```

DAA advances only when blocks are produced, so a bond that cannot hold `window_bind` claims at once
fills its ceiling and the chain stops — no timeout, no operator action, no message. The bound buys the
way out with capital, and on a test network funded from the operator's premine that is cheap.

**It is the wrong instrument, and mainnet is where that bites.** It makes the initial operators the
only parties who can afford a seat, and it makes a genesis seat and a later seat obey different
economics for the same duty — `1.62M because you are genesis, 2,700 because you arrived later` is
workable operationally and indefensible as a protocol. So it is cut where it closes: at the clock.

## 2. Decision

**Liveness is guaranteed by protocol structure. Collateral covers fraud liability and nothing else.**

### D2 — the genesis-only collateral rule is conditional, then gone — **ARMED**

`verify_palw_genesis_v2_with_clock_v1` takes one fact — can this chain advance its clock without
admitting a claim? — and skips the bind-window requirement when it holds. The old entry point
delegates with `false`, so **the conservative behaviour is the default** and only a caller holding
`Params` can claim otherwise. Nothing else in the gate is relaxed: C-08 (a bond declares only what its
outpoint holds), the panel-seating requirement and the catalog agreement all still run.

### D3 — the clock does not depend on any bond's collateral — **ARMED**

`palw_clock_advances_without_a_claim_v1(&Params)` is two clauses:

* **the heartbeat lane is armed from genesis.** A beat carries no claim and reserves no exposure, so a
  bond whose ceiling is completely full can still mint one;
* **no lane `bits` prices is producible.** On a `ConsensusV2` network every V1 proof-of-work activation
  is `never()`, and `algo_id_is_priced_by_bits_v3` takes the attempt, execution, receipt and round
  lanes out of the pricing.

**The beat is not a priced lane either, and that is the premise rather than a gap.** ADR-0066 took the
heartbeat out of `bits` deliberately, so `palw_lane_advances_daa_v1` answers *no* for it — the beat's
contribution is decided per MERGESET, not per lane. Because the second clause makes `priced == 0` on
every mergeset, ADR-0138 §3b's stand-in fires on every mergeset carrying a beat
(`stand_in = priced == 0 && heartbeats > 0`, in `DifficultyManagerExtension::daa_exempt_count`) and
exactly one beat is counted into the score in the missing anchor's place — at most once per wall-clock
interval under ADR-0142's cursor.

So the DAA advances every heartbeat interval regardless of what any bond can afford. Both clauses rest
on activations that are `never()` and a fence armed at 0, so the answer cannot become false later.

**The operational consequence, stated because it is a real one:** at one tick per 120 s the 600-DAA
bind window is ~20 hours of wall clock and the 7,200-DAA exposure span is ~10 days. Bounded, not
collateral-dependent — but a claim's exposure is held far longer in wall-clock terms than the figures
suggest, and D4 is what stops that from blocking new work.

### D1 — collateral is reachable fraud liability — **GENESIS HALF LANDED**

`palw_v2_collateral_for_class_set_v1` makes two corrections, which pull in opposite directions:

* **concurrency, per class instead of the dearest** — the floor at `MAX_CLAIM_EXPOSURE_DAA + 1`
  (genuinely reachable), each model row at `PALW_MODEL_CLAIM_CONCURRENCY_V1 = 4` (its enforced cap ×4);
* **liability, per claim, is `palw_max_fraud_gain_v1` = `escrowed_reward + fork_weight(pwu, slash)`** —
  the fraud a Valid Final AUTHORIZES, not the compute it counts.

**The second correction RAISES the figure, and that was the finding.** The expectation was a refinement
downward; measured, the dense row's gain is 2,702.96 BILI against 1,350.15 BILI of `pwu × slash`, because
a claim's `pwu` is the expected attempt count times one inference — so the fork weight a Final buys is
twice what the bond reserves against it. And the ESCROW half of a gain does not shrink with a cheap
claim, so the floor's term (7,201 concurrent × 2.66814 BILI) becomes the largest of the three.

| class | concurrent | gain/claim | collateral |
|---|---|---|---|
| BASE-0 floor | 7,201 | 2.66814 BILI | 38,426.56 BILI |
| held Qwen3.6 @512 | 4 | 4.73917 BILI | 37.91 BILI |
| held Qwen2.5 @2M | 4 | 2,702.96409 BILI | 21,623.71 BILI |
| | | | **60,088.18 BILI** |

Eight seats: 480,705.47 BILI, 0.0048 % of the 10B cap.

**The runtime half is NOT done, and this is the one place testnet-12 knowingly ships a gap.** A
producer's live reservation is still `palw_exposure_pwu_v1 × slash_value`, i.e. half the gain on the
dense row. Closing it means splitting a value the design keeps unified on purpose: that same
`DerivedV1` arm feeds ADR-0124's work price and ADR-0125's execution credit, and "paid on the same
number it can be slashed on" is what `work_priced_escrow` rests on. Its own commit, on the running
network.

### D4 — admission capacity and slash liability are separate ledgers — **ALREADY BUILT, armed here**

Drafted as OPEN on the premise that "one ledger does both jobs". **That premise is false**, and the
check is recorded rather than the draft quietly fixed:

* `reserved_exposure` is the CAPACITY ledger. `release_for_claim` runs **on `Final` and on `Voided`
  alike** — "the exposure and the immature contribution both belong only to non-terminal claims" — so
  processing capacity returns the moment a claim resolves.
* `slashable_locks: BTreeMap<(bond, claim), PalwSlashableLockV1>` is the LIABILITY ledger:
  `{ claim, amount, expiry_daa }`, `is_live(now_daa)`, no claim bytes retained. Its own doc is this
  decision's sentence: *"Final does not erase liability … withdraw is refused while any lock on the
  bond is live."*

What testnet-12 adds is that the second is armed from DAA 0 (`palw_objective_offence`, which
testnet-11 schedules at 8,500). Pinned by `t12_adr0151_d4_d5::d4_the_liability_ledger_is_armed_from_genesis`.

### D5 — one derived profile per class, and duration is not weight — **ALREADY BUILT, armed here**

Also drafted as OPEN, also already true:

* **duration is not weight.** `palw_economic_payout_v1` contains no `window`, no `_daa`, no
  `deadline` and no `expiry` — the reward is `min(escrow, C_P × rate)` and the panel's share is
  `clamp(α·C_V / (C_P + α·C_V), S_min, S_max)`. A source-level test pins the absence, because the
  property is "no such term exists" and only the text can state that.
* **the deadline is the class's own.** ADR-0133 §11.3's `max(window_receipt,
  verification_window_spans × span_daa)`, armed from genesis here (`palw_class_receipt_window`), which
  is what lets the 2M row leave `Probation` without weakening the network's deadline for anybody.

### D6 — the gate asks for progress, not for capital — **HALF DONE**

The question has changed: `verify_palw_genesis_v2_with_clock_v1` asks a FACT about the network's clock
rather than sizing capital. What it does not yet do is run §4's reachable-state search against the
card. That search is owed as a drill before deployment (§4) and belongs in the gate before mainnet.

## 3. What this does NOT change

* **ADR-0137 stands.** A share is a result; block rights are `verified work / execution quantum` behind
  the execution lane's one permit a round. This ADR is about collateral, not cadence.
* **Fraud stays unprofitable.** D1 lowers collateral toward the honest liability and never below it:
  `collateral ≥ max_fraud_gain / minimum_colluding_quorum` is a floor, not a target.
* **No other network moves.** testnet-11 satisfies D3's clauses too, so its gate result is unchanged;
  its declared collateral was never gate-derived. `shipped_presets_have_pinned_fingerprints` and
  `every_genesis_commits_to_the_premine_this_build_mints` confirm every other preset is byte-identical.

## 3a. The live evidence, taken 2026-09-23

Run with the binary being shipped (`de857a71`, a release build of this branch), two nodes on a fresh
testnet-12 genesis and no class artifact — the point being that the clock moves without one.

```
[palw-heartbeat-miner] starting — bondless heartbeat lane (ADR-0060), fee-only, one thread
[palw-heartbeat-miner] heartbeat #1 8b557c1e… — the clock ticked
[dns-bft] anchor=a8cabac47b96fe30…          ← the pinned t12 genesis
Consensus rule manifest: … palw_work_target=1 palw_independence=1
PALW court certified end-to-end for: PALW-BASE-0, PALW-QWEN36, PALW-QWEN25-A16, PALW-QWEN25-A16-V5
```

Four heartbeats in the first two minutes, eight blocks relayed to the peer, zero errors. **"Bondless"
is D3 in the node's own words**: the lane that carries this chain's clock takes no bond, so no
exposure ceiling and no collateral figure can stop it. That is the premise the collateral reduction
rests on, now observed rather than argued.

Restart and partition-rejoin were clean (0 errors, the surviving node kept minting, the returning one
rejoined). **One drill defect worth recording rather than hiding:** the first pass used `--connect`
for the peer links and measured an IBD node that never synced. `--connect` puts a node in
outbound-only mode and it does not listen for inbound P2P at all — there is no "P2P Server starting"
line — so the drill had silently built a topology with no listener. The live fleet already encodes
this distinction (`--addpeer` on the two hosts that accept inbound, `--connect` on the local-only
seats) and the switch plan must preserve it exactly.

## 4. The evidence still owed — a DEPLOYMENT blocker for testnet-12

The collateral was lowered on a rule-level proof: `consensus/tests/palw_t12_liveness.rs` shows
`priced == 0` at every height and that the predicate the gate reads answers yes. **That is the premise,
not the behaviour.** The stand-in's firing needs a store, and the property is about reachable states.

Before testnet-12 carries value, a wedge search must show the timeout clock advances from every one of:

* the 2M class locked to its cap and the hybrid locked to its cap, simultaneously;
* one panel seat dropped;
* immediately after a restart;
* immediately after a reorg;
* during IBD;
* one DAA before a receipt deadline;
* several classes at their caps at once.

For each: the chain produces a block, the DAA advances, a timeout fires, liability is released. A green
predicate is not a wedge search, and this ADR does not claim it is.

## 5. Order of the remaining work

1. **testnet-12 genesis** — D2, D3, D4, D5 and D1's genesis half. Done 2026-09-22. **§4's wedge search
   before the network carries value.**
2. **On the running t12** — D1's runtime half: the producer's live reservation raised to the gain its
   claim authorizes, which means separating the reservation from the work price and the execution
   credit. t12 is the network that rehearses it.
3. **Before mainnet** — D6 in full (the gate runs §4's search), and the removal of the
   `window_bind × dearest` code path entirely rather than its conditioning. A mainnet card must not be
   populated while that path can still be reached.

## Addendum 2026-09-23 — what the first testnet-12 fleet and the item 6 acceptance run found

Two of this ADR's numbers were posted in units no runtime path reads, and the acceptance ladder that
followed found three memory paths that would have killed a 2M seat before it produced a claim. All
are fixed on `feat/testnet-12-regenesis`; the genesis block is unchanged and only the params
fingerprint moved (`fb8f378d…` → `c746f07c…` → `30848c6b…` → `bcfbf2a3…`, the last for `palw_unavailable_abstains`, a t11-genesis fence the t12 preset had left dormant; `t12_arms_every_fence_t11_armed` now holds every t11-armed fence to genesis on t12).

### The two defects of the first deployment

1. **Collateral in the wrong unit.** §D1's carve priced each class on its own declared leaves;
   the runtime reserves `palw_exposure_pwu_v3` — the derived MAC-eq draw renormalised by the
   floor's leaves-per-MAC-eq — 44.25× larger on the 2M row. Every producer held at `produced=0`
   from genesis. Collateral 60,088.18 → 516,429.80 MSK a seat (`2bd134ec`). The raw fork weight of
   one 2M Final is 335,728,175.72 MSK, 3.36 % of the supply, so "collateral covers the fraud gain"
   is a statement about the EXPOSURE unit; the weight unit is D1's remaining half.
2. **A flat `artifact_digest` pinned where the operand-inventory root belonged** — the same
   defect testnet-11's card records — over a byte-identical artifact. Every seat: `holds no artifact
   whose registered root form is b5baca63…`. Closed structurally, not by pinning a third value:
   distinct types with no conversion (`palw_class_identity_v1`), a `.palwmanifest` beside the
   artifact written by the same derivation the runtime resolves with, the manifest committed and
   parsed by a `const fn` so the genesis card carries no hand-written hash, and
   `--palw-verify-class-manifest` refusing to start on a disagreement (`ea7ad7df`, `26030bc1`,
   `077d4c7f`, `2f588a58`).

### The three memory paths the acceptance ladder found (item 6)

With the streamed root (`216641a4`) already in place, a lone 2M seat on an isolated chain grew from
5.9 to 23.7 GiB of anonymous memory on nothing but heartbeats and the panel's per-tick pre-checks,
and was OOM-killed twice without producing. Named and fixed:

* `RopeTableV1::digest_bytes` built a 1.07 GiB `Vec` of the rotary table per `artifact_digest()`
  call, and every class resolve, cache key and manifest check calls that, from three threads at
  once. The digest now streams (`digest_into`), byte-identical, so no class id moves (`e40dfcd2`).
* The panel's replay-budget pre-check resolved the class — compiling its plan and hashing the
  artifact — on every tick. Memoized on (holdings, class, root) (`e40dfcd2`).
* The readiness proof materialized the whole inventory (retained twice), copied every leaf hash,
  and re-folded the tree per drawn leaf, and an unsubmittable proof was rebuilt every tick. The
  material is now one streamed walk and the built proof is kept per (class, span) (`427a1b8d`).

### What the ladder also taught about the ladder

A node cannot mine alone (`peers=false` holds regardless of `--enable-unsynced-mining`), so a
one-node rung measures idle and the manifest verify only; blocks appear at the rung where a peer
exists. An isolated chain's first block needs `--enable-unsynced-mining` (in the fleet only `ibm`
carries it). A verification flag that finds no manifest must refuse, not report "0, all agreeing".
Leftover nodes from a previous run hold their ports and ignore `SIGINT`; a ladder starts by killing
them. The three fixed memory phases bracket construction only; a periodic line and an external
sampler are what bracket growth during operation.

### The fourth path, and the gate, measured (item 6, continued)

With the three above fixed, the producer sat flat at 2.3 GiB of anonymous memory until a peer
arrived and blocks flowed — then rose 16.05 GiB in ONE minute and was killed. Not a leak: the K/V
cache of one attempt. `A16Cache` is `Vec<Vec<Vec<i32>>>` twice, and the 2M row's canonical job
prefills 262,143 positions: 28 × 2 × 256 × 4 B = 56 KiB a position, 14.55 GiB with headers. The
producer's path had epoch and exposure gates and no memory gate; the panel's replay estimate was the
artifact's file size plus 512 MiB (3.17 GiB), five times too small.

`a16_attempt_working_set_bytes_v1` names the cost once, beside the cache; the producer asks
`attempt_working_set_bytes(prefill)` right before it spawns and HOLDS; the panel's replay figure
carries the K/V term (`82575797`). Verified on 5.104.81.23 (23 GiB, two nodes, budget 21 GiB / 4):

```
this attempt would allocate 17.23 GiB for a 262143-token prefill (its K/V cache plus scratch)
and the host's replay budget is 13.74 GiB … — holding rather than being OOM-killed
producer alive; 10 heartbeat blocks; no kill; steady anon 3.85 GiB
```

One transient remains unexplained: anon 1.08 → 9.79 GiB about a minute after start, released within
two minutes, with the attempt already refused. It is a working-set term no figure names yet
(candidates: the plan compile on the first resolve, the readiness material walk) and is assigned to
the engine track below.

**This gate is stage one, not the answer.** On 24 GiB hosts the 2M row is now a *named hold*
rather than a crash, which also means it does not produce there. The operator's direction for the
engine (tracked on `feat/kv-codec`): the runtime representation is not `CanonicalWork` — i32/i16/i8,
paged/resident, mmap/RAM, CPU/GPU may all change while `execution_root`, verdict, `CanonicalWork`,
payout and quanta stay byte-identical. Order: working-set canonicalization (producer / full seat /
partial seat / checkpoint / KV-resident / scratch, derived from one canonical profile and separated
from file size) → producer/panel/Court through one derivation → a node-local memory reservation
ledger (`estimate → atomic reserve → execute → release`, because a correct per-attempt figure still
OOMs when three seats each see "available") → S1 partial seats resuming only their segment → i16
then i8 KV as versioned runtime profiles that never change a class's economic identity → RPC
telemetry (`estimatedWorkingSet`, `reservedMemory`, `holdReason`, …) → cross-runtime determinism
tests. Memory requirement is capacity; collateral is `max_fraud_gain`; the two are separate systems.

Post-fix ladder (gate in place, `82575797`, 21 GiB / 4): producer 5.00 GiB PSS alone; + one panel seat
7.20 total; + two more **9.39 GiB total for four nodes** — each added panel seat costs ~1.07 GiB anonymous
and the artifact's file pages are shared once across the four. Sub-linear, as required; no kill; the
producer held on its working set 27 times and produced no model claim, which on that build is correct.

**The engine track landed** (`feat/kv-codec`, merged at `5451aac2`, params fingerprint unchanged):
K/V values are all within ±32,767 by construction (`a16_rope` and `a16_matmul_requant` both end in
`clamp16`), so **A16-KV-i16 is a lossless repack** and ships as the default; i8 would re-quantize 89 % of
the elements and change `execution_root` — a class change, not a runtime profile — and is refused by
name. The 2M attempt's working set falls 14.84 → 7.84 GiB; on the fleet host (+2.67 GiB file, +1.07 GiB
rotary table) ≈ 11.6 GiB, which fits a stated 13 GiB share (`--palw-host-memory-share`, `4e05f85d`)
beside three 2.5 GiB panel seats but not an equal 5.25 GiB split — that needs paged KV, not started.
Also landed: one resource profile for producer / full seat / partial seat, a node-local reservation
ledger (RAII, refusals name the holder, races decided deterministically), S1 partial seats replaying
only their prefix, and RPC/CLI telemetry. The i16 acceptance run (does the 2M producer now START an
attempt?) is appended when it completes.

## Addendum 2026-09-23 (evening) — D1's second half, and the regenesis it forces

**The escrow was priced at a subsidy no block pays.** The economic audit's U2: every ConsensusV2
preset sets `deflationary_phase_daa_score = 0`, so a genesis-era claim carries block one's real
subsidy through the 720‰ worker carve — 444,562,014,000 sompi a block, an escrow of 3,200.84650080 MSK —
not the `pre_deflationary_phase_base_subsidy` figure (370,468,345 sompi) the card used, 1,200× short.
Priced the way §D1's card prices it — every floor claim of the exposure horizon at once,
`MAX_CLAIM_EXPOSURE_DAA + 1` — one seat would have to post 46,627,776.51 MSK, 98.9 % of it the floor
term: a network nobody can join. The operator's decision (option A):

* **The runtime reserves a claim's whole fraud gain, escrow + weight.** The escrow is a separate term
  keyed on the claim's `accepted_daa` through a borsh-skipped state-params copy of
  `palw_audit_2026_09_23` (`escrow_backed_exposure_from_daa`), so the audit's collateral-unit helper
  (`reserved / slash`) keeps its meaning. It is reserved at acceptance, re-derived by the consistency
  check, reverted with its block, and released at Final or void. A CourtFraud void forfeits
  reserved + escrow; a liveness void (withholding, timeouts) forfeits the weight only. The admission
  ceiling and the producer's headroom count it, so a bond holds exactly the claims its collateral
  backs — participation proportional to stake, which is what a fraud-gain collateral is for.
* **A genesis seat is sized for 64 concurrent floor claims** (`PALW_T12_GENESIS_FLOOR_CONCURRENCY_V1`)
  plus each model row's in-flight cap: 939,063.21001040 MSK a seat, eight seats 0.0751 % of the cap.

**The genesis rows changed in the same regenesis.** Floor + dense Qwen2.5-1.5B graph-v7 at 8,192
(`ebf44d0a…`, over its own conversion, root `88096dc1…` from the committed sidecar) and at 2,097,152
(`74c67e63…`). The held hybrid row is gone: the graph-v7 held map gathers the GDN convolution over
`(2·k_dim + v_dim)·heads`, which is the engine's window only when key and value head counts agree,
and Qwen3.6-35B-A3B's are 16 and 32 — every held hybrid attempt fails at the first recurrence
checkpoint (prefill position 15). A hybrid row is a registration transaction once the map is
corrected. Collateral and rows move the premine, so the genesis block moves with them — and so does
genesis card 7, re-keyed for testnet-12 because its key is on no fleet host: testnet-12's genesis is
`f6cc9576…` (utxo commitment `11669bc6…`); the deployment record carries the fingerprint.

## Mission alignment amendment — 2026-10-07

capacity ledgerとfraud liability ledgerの分離を維持する。Finalでcapacityを戻しても、外部public verifierのproof期間と証拠保持が終わる前に回収可能なliabilityを消さない。nominal slashと実際に回収できる額を区別する。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

collateralの役割を、徴収可能な不正責任と共通DAA期間内の経済的発行能力に分ける。同額資本・同期間・同倍率の正直な計算と高速偽造は同じ上限に当たる。
発行枠回復で未解決責任を消さず、同じ資本を新旧claimの損失に二重使用しない。小さい権利・予約へ細分化する方向でも、検証能力と実際に回収できる損失を確認する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0151-liveness-is-structural-collateral-covers-fraud.md -->

</details>

<a id="source-0165-the-floor-is-a-reserve-and-the-work-carries-the-clock"></a>

<details>
<summary>0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md — 原文 L1–L438</summary>

[原文を開く](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) · SHA-256: `922cc3a260cf33f2a7bb927dbd3c7838da3f70a1983b1dc2078731744705ff29`

<!-- ADR_SOURCE_BEGIN:0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md -->
~~~~~~~~markdown
# ADR-0165 - Useful work carries the clock: BASE-0 remains the idle-only bonded floor (A″), REAL attempts can carry slot ticks (B), and heartbeat remains an independent emergency lane

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **RFC8 scope correction — 2026-10-08:** [Revised RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) preserves current main's active REAL/heartbeat/BASE-0/clock/anchor-duty structure. The earlier handoff to RF8 for class-aware floor colouring/merge exclusion (§10.3) is historical and is not part of the EXEC-slice design. EXEC slices never tick the clock or suppress heartbeat/BASE-0. This correction neither activates nor removes this ADR's existing rules or other independent proposals.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** Direction amended **2026-10-07 (revision 5, user decision)**. Retain A″ (the idle-only bonded BASE-0 floor) and B (an attempt can carry a slot tick). Retain the independent bondless heartbeat lane of [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) and [ADR-0140](0140-the-heartbeat-is-the-emergency-generator.md). **The proposed FALLBACK-V1 integration and heartbeat retirement are withdrawn**, including its added weight, bond envelope and per-bond slot credit; it is not the next fence. Sections 5-8 and 10.5 record this change.

Revision 4's 2026-10-03 implementation and measurement record remains: A″ uses `floor_idle_slots = 20`, `probe_cooldown_slots = 20` and `probe_slots = 8`; the two implementation fences are `Params::palw_floor_reserve_v1` and `Params::palw_real_clock_tick_v1`. The historical DAA-5,300 release plan and preset status below are retained as implementation history. **This revision changes design direction only; it does not change activation heights, shipped presets, consensus fingerprints or runtime accounting.** Header-level floor invalidation remains an unbuilt research question, not a scheduled follow-on fence.

**Builds on:** ADR-0060/0064 (the liveness doctrine), ADR-0066 (the heartbeat lane), ADR-0105 (heartbeat
transparency, `LaneColoring`), ADR-0138 (the anchor clock), ADR-0142 (the cursor; §9 the floor), ADR-0045/0135
(classes, registry), ADR-0149 (an attempt's pwu is its weight), ADR-0152/0160 (the share and room gates, J-1's
per-bond weight cap).

## 00. The DAA-5,300 rule: A″ + B

### 00.1 A″ — the floor is the idle-only bonded fallback (`palw_floor_reserve_v1`)

Past the fence a `PALW-BASE-0` attempt — a *floor* attempt — is accepted by the fold **only while the chain's floor state
is Idle**, and is refused by name (`FloorNotIdle`) otherwise. The refusal is in the fold's class gate
(`check_class_admits_claim`), which the own attempt, a merged attempt and the producer's pre-check
(`palw_producer_facts_v2` → `class_admission_refusal` → `ready_to_produce_v3`) all ask. It comes before any write and
joins the pre-write skip arm, so the block stands and **no claim is written**: no reward is escrowed (the worker carve is
withheld and burned, as for every skipped own attempt) and no PALW weight is credited. **When the floor IS accepted
(Idle) it is today's bonded floor attempt, unchanged** — the same bond, producer-floor, exposure, share and room checks, the
same reservation, weight and reward. The rule adds a refusal and relaxes none; claims taken before the fence settle as before.
A **REAL attempt** is any attempt of a registered class other than the base class.

**The floor state** is one small rooted value, `PalwFloorStateV1 { mode, last_probe_end }`, with `mode` one of

* **Idle** — no REAL work is flowing; the bonded floor is the fallback and is accepted;
* **Probe { until }** — a REAL attempt was accepted *RED* after an idle stretch; the floor is refused through slot
  `until − 1`, so the next REAL attempt can land BLUE;
* **Normal { last_blue }** — REAL work is landing BLUE; the floor is refused while `daa − last_blue ≤ floor_idle_slots`;

and `last_probe_end` the DAA a probe last expired with no BLUE REAL attempt in it (`until`; `None` until one has) — the
cooldown's reference. The default (Idle, none) is stored as `None`, so a chain that never saw REAL work roots exactly as one
without the rule.

**One pure function moves it** — `palw_floor_step_v1(state, daa, accepted)`, `daa` the accepting block's DAA score (the slot),
*time first, then the event*:

```text
time   Normal ──(daa − last_blue > floor_idle_slots)──▶ Idle
       Probe  ──(daa ≥ until)──▶ Idle   [last_probe_end = until — the nominal end, never the DAA a block noticed it at]

event  a BLUE REAL attempt, in ANY mode ──▶ Normal { last_blue = daa }        (verified success: no cooldown)
       a RED REAL attempt in Normal or Probe ──▶ nothing                      (a RED one never extends)
       a RED REAL attempt in Idle ──▶ Probe { until = daa + probe_slots }     iff no probe has ended, or
                                                                              daa − last_probe_end ≥ probe_cooldown_slots;
                                                                              otherwise nothing
```

Floors are valid only in Idle. Time is idempotent and path independent (stepping to `t₁` then `t₂` is stepping to `t₂`), so
the result does not depend on block spacing. A BLUE attempt needs no cooldown; the cooldown exists only so that a stream of
RED ones cannot suppress the floor: a RED-only stream keeps it refused for at most `probe_slots` of every
`probe_slots + probe_cooldown_slots` (8 of 28), and never for a longer run than `probe_slots`
(`a_red_only_stream_cannot_keep_the_floor_refused_beyond_probe_slots_per_cooldown`).

**Events, their order and their colour.** An event is one REAL attempt the fold **fully accepted**: it passed every check the
fold makes — the registered, Active class, the bond (key, producer floor, freeze), the budget, the lottery, the room, the
exposure ceiling and share, the network room — **and wrote its claim**. The step runs inside `apply_attempt` just before
`write_claim`, so a merged attempt the fold later refuses is restored with its checkpoint, and an attempt the fold *skipped*
(refused by name, before any write) never moves the machine. **The kind a header claims never moves it; the claim does.**
Within a block the order is the fold's: the block's own attempt first (BLUE: a chain block), then the merged works in
consensus acceptance order, each stepping the state the one before left, all at the accepting block's DAA (Idle + [RED, BLUE]
is Probe then Normal; Idle + [BLUE, RED] is Normal and the RED changes nothing). A merged attempt's colour is its place in the
accepting block's mergeset: the processor hands the fold the classified `genuine_reds()` (`extras.merged_reds`, ADR-0125 semantic amendment), and the fold
reads a carrying block in it as RED, every other attempt as BLUE. The time step runs once at the start of every block's fold
(step 1e, before the sweeps and before any attempt, in the acceptance rehearsal too) so own and merged attempts read one state;
a floor attempt's gate reads the state at its place in the order.

**Rooted and reorg-safe.** The field `floor_state` is in the state root (only when `Some`), the pruning carriage (tail `0xEA`,
only when `Some`) and the delta (variant 104, `RealWork { old, new }`; **one entry per change, exact revert and apply**, so a
reorg restores it and a branch computes its own); time writes once (`time_writes_the_floor_state_once`). A node that joins by
pruned sync gets it in the carriage, checked against the root the witness header commits
(`real_share_a_pruned_join_inside_a_normal_stretch_…`, `…_inside_a_probe_…`: 00.8).

### 00.2 The live finding, what A″ answers — and what it does not

**Evidence** (P2, `lanes/evidence/8k-red-1003/`, the last 300 DAA of testnet-12 before 2026-10-03): 72 attempts of the 8k
class, **71 RED and 1 BLUE; every RED caused by floor attempts** (141 counted peers, all floors; heartbeats counted 0). The 8k
producer infers for 342 s at the median (p95 418 s) while floors — 3.0 a slot, 64 % of them external miners — extend the chain.

**Mechanism.** ADR-0105's `LaneColoring::Weighted` makes heartbeats invisible to a bonded candidate; a floor attempt is
another attempt and is counted classically. Testnet-12's `ghostdag_k` is 1, so two floor-lane blocks in an attempt's anticone
make it RED, and 340 s holds about eight. The colouring is header-stage: it reads the lane, not the class and not the state.

**What A″ does.** While REAL work lands BLUE (Normal) or has just landed RED (Probe) the floor is refused by the fold **and by
the producer's pre-check**, so honest floor producers hold before they spend a draw; only heartbeats extend the chain under a
slow attempt, and it is BLUE. `t12_real_share` runs this on testnet-12's own GHOSTDAG through the real pipeline: before the
fence a slow REAL attempt with two or three floors in its anticone is RED (the finding reproduced) and with the heartbeats of
one, two or three slots it is the selected parent; past the fence, with the floor refused, it is BLUE, accepted and refreshes
Normal; after Normal has run out the first REAL attempt can go RED, opens a Probe, and the next one is BLUE.

**What it does not do — the honest limit, with numbers.** **A″ is a rule about which blocks earn a claim; it is not a rule
about which blocks exist.** A floor-lane block the fold refuses is still a valid block in the DAG, and GHOSTDAG still counts it
against a slow REAL attempt: the header-stage colouring cannot read the class or the state. What protects the REAL attempts is
that **honest floor producers do not mine while the state says Probe or Normal** — the producer's default hold, which asks the
fold's own gate — and that a producer that ignores the hold earns nothing for it (its blocks carry no claim, no weight, no
reward). P2's second replay of the live capture (`lanes/evidence/8k-red-1003/replay2/`, 72 REAL attempts, DAA 3,779–4,219; *REAL
BLUE* = the share of REAL attempts GHOSTDAG colours BLUE) measures how much rests on that compliance:

| who ignores the hold | REAL BLUE at 20 / 8 / 20 (shipped) | at probe 6 |
|---|---|---|
| live, no rule at all | 1.4 % (1 of 72) | 1.4 % |
| **everybody — the fold's refusal alone, nobody holds** | **1.4 % (1 of 72)** | 1.4 % |
| **nobody — every floor producer holds** | **97.2 % (70 of 72)**; REAL attempts are 93.3 % of the BLUE attempts GHOSTDAG counts | 88.9 % (64 of 72) |
| one external producer, 9.5 % of the floors | 72.2 % (52 of 72) | 68.1 % (49 of 72) |
| the largest external producer, 38 % of the floors | 29.2 % | 19.4 % |
| the fleet's own producers, 36 % of the floors | 18.1 % | 13.9 % |
| the small externals together, 26 % of the floors | 38.9 % | 20.8 % |
| every external producer, 64 % of the floors | 6.9 % | 5.6 % |

(The first three rows, the fleet row and the last are P2's `final2.json` sweep; the per-producer rows are P2's `perprod.py`, rerun
here with the shipped probe as well.) **The fold-level refusal without the producer hold is worthless** (1.4 % BLUE — the live figure), and
**one non-compliant floor producer at a tenth of the floors takes REAL BLUE from 97 % to 72 %.** So the hold is **ON by default in
every role that mines floors** — the kaspad PALW producer service, the only one — and has no off switch except the drill flag
`--palw-drill-floor-ignore-policy` (a salted private drill only). Non-compliance is not version skew: a binary without the rule
forks off at the flag day, so a non-compliant floor producer on the chain is a *modified* one, whose only motive is to degrade
REAL work (its floors earn nothing). Possible answers that do not depend on producer compliance remain open research questions (§10); this implemented rule is a
policy the producers must follow, and says so. None authorizes replacing the heartbeat with a bonded block.

### 00.3 The constants, from the measurements

All three are named constants in `palw_real_share_v1`, **hashed with the fence** (`palw_floor_reserve_value_v1` →
`[20, 8, 20]`: changing one is a new network id), each checked against its derivation.

* **`floor_idle_slots` = 20** (40 minutes) — `⌈p95 gap between accepted REAL attempts / slot⌉ + margin` = `⌈17.3⌉ + 2`
  (`palw_floor_idle_slots_for_v1`; P2's gap: p50 3.1 slots, p95 17.3, max 30.6 — the long gaps are panel and backpressure
  holds; the margin is one slot of accepting-block lag and one of rounding and jitter; the inference is *inside* the gap so it
  is not added), capped at 30 (one hour, the merge-depth duration). The sweep (full compliance, probe 8) is why 20 and not
  less: REAL attempts are **47 % of the BLUE attempts GHOSTDAG counts at 12, 69 % at 16, 93 % at 20**, and 100 % at 24 and 30
  (the rest are floors accepted in Idle); more than 20 only lengthens the unbonded stretch after REAL work stops (00.4).
* **`probe_slots` = 8** (16 minutes) — `⌈median draw / slot⌉ + ⌈longest draw / slot⌉ + lag` = `3 + 4 + 1`
  (`palw_floor_probe_slots_for_v1`; P2's second replay: the 8k producer's draws take 325 s at the median and 426 s at the
  longest). A Probe opens when a REAL attempt lands RED (the trigger). The producer's next attempt was templated before it, so
  it collides with the probe's start; the one after it is the first templated under the probe, with no floor under it, and the
  probe must outlast its whole draw. **Full compliance, REAL BLUE: probe 4 or 6 gives 63–89 % (idle window 12–30), probe 8 or
  more gives 93–99 %** — which is why it is 8 and not the 6 of revision 3.
* **`probe_cooldown_slots` = 20** — the least gap from the end of one unanswered probe to the start of the next. At full
  compliance cooldowns of 10, 20 and 40 measured 91.7 %, 88.9 % and 90.3 % REAL BLUE (probe 6): it is not what the result turns on;
  it bounds a RED-only stream to 8 refused slots of every 28.

**What the constants buy, as measured at 20 / 8 / 20 with full compliance:** REAL BLUE 97.2 %, and the chain is Idle — the floor
is the fallback — for 1.2 % of the wall time. The **empty-slot share** (slots with no accepted attempt of either kind, the
heartbeat carrying the clock alone) is 81 % at these constants: the 8k stream is about one attempt in three slots, so the chain is
mostly heartbeats between attempts. A″ makes the attempts that exist land; it does not make more of them.

### 00.4 The price

* **After REAL work stops, up to `floor_idle_slots` = 20 slots (40 minutes) are heartbeat-only** — unbonded, weight ε a block —
  before the bonded floor resumes. A transaction confirmed only inside that stretch is as exposed as inside any heartbeat-only
  stretch today (`hb_probe_b_*`, `hb_probe_d_only_the_finality_depth_stops_a_heartbeat_only_reorg`): a rewriter needs more beats a
  slot than the honest chain and no bond, until the finality depth or a Settlement Anchor stops it. The stretch is bounded by the
  constant (and the constant by its cap), not open-ended. A Probe adds at most 8 refused slots of floor per 28 to a RED-only stream.
* **While REAL work flows, the chain is better secured than by floors, not worse.** An 8k claim weighs 1,388,926,972,416 pwu
  against a floor's 6,042,506,112 (229.8×). Three floors a slot is 18.1e9 a slot; the 8k stream at its median gap is 448e9 a slot
  (24.7×) and even at its p95 gap 80e9 a slot (4.4×).
* **Weight concentrates in the REAL producers' bonds** while it flows, because the floors that used to add to it are refused.
  That is the intent (the floor is a fallback), and it is the cost: the rewrite cost of a busy stretch is the REAL claims' weight,
  bonded and panel-verified, which a rewriter must out-weigh with REAL work of its own.
* **Normal is kept alive by BLUE REAL attempts only.** A producer holding a bond and any Active REAL class can keep the floor
  refused indefinitely by landing one BLUE REAL attempt every `floor_idle_slots` slots; the chain's weight then grows by at least
  that attempt's weight and the stretch is *not* heartbeat-only (each REAL attempt is bonded and panel-verified), but a cheap
  class could make that weight small. A RED stream cannot do it (cooldown). **Open, recommended follow-up (a decision, not done
  here):** step the machine only for a REAL attempt whose pwu is at least a stated multiple of the floor's, so a token class
  cannot suppress the floor. Without it the guard is the class's registration burn, its panel verification and the bond's capacity.
* **Withholding REAL work to force the fallback gains nothing:** a REAL producer that withholds goes idle after
  `floor_idle_slots`, from where the floor — bonded weight, reward as today — is what it could have earned anyway, and it
  forgoes the REAL attempts' weight and reward.
* **A modified floor producer that ignores the hold forgoes everything and can still hurt** (00.2): its blocks are valid and
  colour classically. That is the limit of a fold-level rule and the reason for §10.

### 00.5 Reds count — and never extend

`palw_v2_merged_works` returns the work of the whole mergeset — blues and reds alike, the selected parent excluded (its own
work is applied by its own fold); its comment: "at `ghostdag_k = 1`, any block whose anticone holds two or more blocks is a red by
construction … A blues-only rule would measure nothing". A RED REAL attempt is therefore applied by the merging block's fold,
exactly as a BLUE one — its claim, its weight, its reward — and it is an event of the machine with colour RED: **in Idle it opens
a Probe** (once per cooldown), in Normal and Probe it changes nothing. Tested through the pipeline
(`…a_rogue_floor_still_colours_classically_but_earns_nothing_and_a_red_real_attempt_never_extends_normal`: the attempt is RED, its
claim exists, its weight is credited, Normal still ends `floor_idle_slots` after the BLUE attempt, not after it;
`…a_red_first_attempt_opens_a_probe_…`). A non-DAA block (outside the window or a round block) is the one exclusion, as it is for the
coinbase.

### 00.6 B — a REAL attempt carries the slot's tick (`palw_real_clock_tick_v1`)

**What cannot be done, and what is done instead.** The DAA score is derived at **header** processing from the window and the
mergeset's HEADERS (ADR-0142 §6a: no stored cursor, so a pruned and an archival node agree). A block's class and bond live in its
body and are judged at the virtual stage. A tick conditioned on "an accepted REAL attempt" would make a header-stage value depend
on virtual state — circular, and divergent between a node holding the bodies and one that does not. So the consensus tick source is
the **lane**, which a header states: an attempt-lane block (`is_palw_attempt_algo_id`: algos 6 and 9), beside the heartbeat lane.
The preference for REAL work is the product of A″ (no honest producer mines a floor while a REAL attempt is recent) and of the
miner's policy, not of a clock rule.

**The rule.** In `palw_clock_step_v1`, past the fence: a mergeset with nothing `bits` prices and at least one tick source (a
heartbeat or an attempt-lane block) is `granted` when the NEWEST source is stamped at or past the cursor's slot. `granted` removes
exactly ONE exemption (`palw_clock_tick_source_v1`), so a mergeset advances the DAA by one however many attempts it holds — *never
each attempt +1*. The cursor, the reference (earliest tied step), H3, H5 and the lead cap are unchanged; the lead cap now also
covers an attempt-lane header. **Node policy**: `heartbeat_slot_hint_v1` — the heartbeat miner holds until `slot + 20 s`
(`PALW_REAL_TICK_GRACE_MS_V1`) for an attempt to carry the tick, and then mints as before.

| property | holds because |
|---|---|
| a producer cannot **speed up** the clock | a tick needs a source stamped ≥ the cursor's slot and the step that consumes it is stamped ≥ that slot (H5) and ≤ receiver clock + 132 s (lead cap): references are ≥ one interval apart. Attempts are cheap at the header stage and a heartbeat costs 2^24, but cost never bounded the RATE — the cursor does. A burst is `⌊132/120⌋ + 1 = 2` ticks, once. `a_producer_of_any_mix_cannot_run_the_clock_faster_than_a_heartbeat_miner` simulates adversarial mixes of up to 100 attempts and 3 beats per instant. |
| cannot **stall** it beyond a heartbeat miner | the reference is the earliest tied step; any honest block merging a granted source is a step. |
| one tick per slot however many attempts | `granted` removes one exemption: `a_hundred_attempts_in_one_slot_advance_the_score_by_one`; through the pipeline, `…a_real_attempt_carries_its_slots_tick_and_a_slot_ticks_once` (an attempts-only chain: one tick a slot with no heartbeat anywhere; the unarmed control ticks not at all). |
| DAA-denominated windows keep wall meaning | the DAA still moves at most once per 120 s. |
| reorg / pruned node | `attempt_ticks`, `granted` and the cursor are functions of the block's own parents and window; no new state; headers only. |

### 00.7 Node and RPC

* `getBlock` verboseData carries **`blockKind`** (JSON only; the borsh wire and the gRPC proto are unchanged): `REAL` (a non-floor
  attempt), `FALLBACK` (a floor attempt past the fence), `LEGACY_FLOOR` (before it), `LEGACY_HEARTBEAT` (algo 8), `EXEC` (a round
  block), empty otherwise (`palw_block_kind_v1`, tested against a real `palw_commitment`).
* **`[palw-floor-state]`** — one info line per chain block whose fold moved the floor state, naming the net move:
  `[palw-floor-state] daa=<N> block=<hash> <from>-><to> last_blue=<n|-> until=<n|-> last_probe_end=<n|->`, `from`/`to` one of
  `idle`, `probe`, `normal`, the three fields describing the new state (`palw_floor_state_log_line_v1`, format pinned by a test).
* **The producer's hold** (the default policy, nothing to enable): while the state at the candidate's DAA is not Idle the floor
  producer logs `[palw-producer] holding: the model registry admits no new claim of this class now [class=<id> epoch=<n> …
  registry="the base class <id> is the idle-only fallback: at DAA <n> the chain is <state> (ADR-0165: floors are refused until it
  is idle)"]`, at **info** and never as a starvation error (`palw_hold_level_v1`: it is the design working). The grep marker is
  `idle-only fallback` (`PALW_FLOOR_NOT_IDLE_MARKER_V1`, pinned to the error's message by a test). It resumes by itself when the
  state is Idle. The fold's own skip of a refused floor is logged by the processor as `PALW: merged blue <hash> carried work this
  chain point refused (the accepting block stands): the base class … is the idle-only fallback …`.
* **Anchor duty** (§00.9): `[palw-producer] anchor duty: the floor is held as the idle-only fallback, and a claim has waited <n> slots (since
  DAA <d>) for an operator attempt — mining ONE binder, refused by the fold (no claim, no weight) and still an operator attempt that binds
  the due claims`, at info. Only for an operator's floor producer (`binder_due` is false for any other bond).
* **The lane watch** (`palw_lane_watch`) counts an attempt-lane block a selected-chain block MERGED, blue or red, as PALW work: under A″ the
  REAL attempts land beside the chain (none of the 86 in P2's capture is a chain block), and a watch that read the selected chain alone
  would call a healthy chain "running on its clock alone" and raise its ERROR continuously. A chain of heartbeats with nothing merged
  still alarms.
* **Drill flags** (salted testnet-12 chains only; each refused without the salt at start-up and re-checked against the chain the
  node runs): `--palw-drill-useful-work-at=H` arms both fences at a low height; **`--palw-drill-real-submit-delay-s[=N]`** holds a
  REAL (non-floor) attempt's submission until N seconds after its template (default 340 s, the 8k median; 1..=3,600) to emulate an
  8k inference's minutes on a drill-sized class; **`--palw-drill-floor-ignore-policy`** makes the floor producer ignore *only* the
  idle-only refusal — the drill's policy-ignoring floor miner (00.2), a switch that takes no value; **`--palw-drill-anchor-duty-after-slots=N`**
  (1..=3,600; the release's wait is 30) moves how long a claim waits for an operator attempt before an operator's floor producer fires its
  anchor-duty binder (§00.9), so a drill that runs minutes can see it.

### 00.8 Tests

`consensus/core` — `palw_real_share_v1` (every arc of the machine, time idempotent and path independent, ordering inside a slot,
the RED-only bound, BLUE sustains Normal and Normal ends `floor_idle_slots` after the last, BLUE after an expired probe is Normal,
the constants and their derivations and caps, the canonical rooted form, the log line and the net move, the refusal's words, the
tick-source rule, the clock-speed simulation), `real_work_reserve_v1` (the fold around the machine: Idle accepts, Probe and Normal
refuse by name, an unanswered probe expires and the cooldown holds RED off but not BLUE, RED never extends and BLUE does, several
attempts in one mergeset in the fold's order, **only a fully accepted REAL attempt moves it**, fence straddle, reorg revert/apply and
branches, the carriage, time writes once, an all-floor network is Idle and live), `palw_useful_work_fences` (dormant on every preset,
fingerprint and fork id, prerequisites). Pipeline, on testnet-12's own GHOSTDAG — `t12_real_share` (00.2, 00.5, 00.6: the slow
attempt under floors and under heartbeats, the busy chain, the rogue floor, the RED first attempt and its Probe, an unanswered probe
expiring, B), **the pruned join** (`…a_pruned_join_inside_a_normal_stretch_…`, `…_inside_a_probe_…`: a second node follows the
archival one through the pruning point, is left as a pruned join leaves it, imports the carriage against the witness child's committed
root and then agrees with the archival node on every block — sink, root, floor state and the decision on each floor attempt), and
`t12_post_launch_fences_combined` (the two fences armed with the whole release at one height: the floor reserve is live, an
all-floor chain stays Idle and roots as before, the clock still runs). `kaspad` — the drill flags (refusals, the wait rule, the
policy bypass and its wiring), the hold's wording and level, the anchor duty's rule, the lane watch counting merged attempts. `rpc-service` — `blockKind`.
The anchor finding (00.9) is measured by `real_share_gap_*` and `real_share_control_*` in `t12_real_share`.

### 00.9 What A″ does to anchors (open finding of 2026-10-03; P2, `lanes/evidence/head-admission-slip-1003/`)

**The floor was the chain's anchor factory.** In P2's capture every attempt on the selected chain is a floor (1,415 of 1,415 chain
attempt blocks); **none of the 86 REAL attempts is a chain block** — an 8k attempt is templated slots before it lands, the heartbeat
chain that grew meanwhile (about 2^24 of work a beat) out-weighs it, and the next block merges it beside the chain — and **all 86 are
the fleet's own genesis bond**, an *operator* by lane A's reckoning. Two consumers read the admitted attempts of chain blocks, and A″
holds the producers that made them:

1. **ADR-0130's round seed anchor.** Recorded by a chain block whose OWN attempt is admitted (`record_round_seed_anchor`, step 4's
   `Ok` arm), read by the next span's schedule and by the ADR-0147 admission jury (`round_seed_anchor.span + 1 == span_now`, else no
   audit and the class stays `Candidate`; no schedule and the execution lane is idle for the span). With the floor held a span has an
   anchor only if a REAL attempt was a chain block in it — which none is.
2. **Lane A's panel binding.** A claim binds only in a block that is or merges an OPERATOR's attempt at or past its slot
   (`palw_chain_block_as_anchor_v1`). Today the REAL attempts are the fleet's, so they anchor; where REAL work comes from
   non-operators and the operators' floor producers hold, **no operator attempt exists and the claim waits for its backstop**
   (`bind_base + window_bind`, 580 DAA, then `BindTimeout` without forfeit).

**Measured in-tree** (testnet-12's own pipeline; A″ + B armed; lane A and lane F1 armed with 4 of the 8 genesis cards as operators;
spans are one DAA here; `t12_real_share::real_share_gap_*` and `…_control_*`, which assert the gap as it stands — flip them when a fix
lands):

| | result |
|---|---|
| a non-operator's REAL claim, chain Normal, floor producers holding, non-operator REAL attempts keeping it Normal | slot DAA 30; at DAA 45 still **`Provisional`** |
| the same, then ONE operator floor — refused by the fold (`FloorNotIdle`: no claim, no weight) | **`PanelBound { bound_daa: 45 }`** at once: the anchor needs the operator's *attempt* (its header), not its claim |
| the same claim with the REAL attempts made by operators | `PanelBound { bound_daa: 41 }` |
| seed anchors, the same run | **1 of 36 spans anchored** (the opening REAL attempt, a chain block); the REAL attempts merged beside the chain anchor nothing |
| seed anchors, a denser run (one REAL attempt in flight at a time, four merged beside the chain, none the selected parent of its merger) against the floor producers mining every slot | Normal, floor held: **1 of 16 spans anchored**; honest floors, Idle: **15 of 16** |

**What was decided** (coordinator, 2026-10-03; none of it changes the state machine):

* **Anchor duty — built, in this fence's release, default on** (producer only, no consensus change). An operator's floor producer that
  holds *only* because of the idle-only policy mines ONE binder when a claim has waited 30 slots for an operator attempt (staggered 0–7
  slots by bond so the operators do not all fire in one slot, and the wait starts again after it fires:
  `palw_floor_anchor_duty_v1`). A refused floor is still an operator attempt, so it binds the due claims (measured above); the cost is
  about one floor per 30 slots in the non-operator-REAL regime and none while operators' REAL attempts (a 3-slot gap) anchor by themselves.
  It is the only floor the hold lets through, it replaces the time-triggered "beacon floor" an earlier proposal gave the replay, and it is
  logged (§00.7).
* **The round seed anchor — NOT in this fence.** Merged admitted attempts recording the anchor, the anchor kept across spans, and the jury
  and the schedule seeding reading the latest anchor within the last 24 spans are the separate 5,300 fence `palw_anchor_window_v1`
  (lane P2, branch `anchor/window`, user-approved). This ADR neither implements nor assumes it: without it, A″ leaves the ADR-0130 schedule
  and the ADR-0147 admission jury without an anchor in almost every span (measured above), and the tests that assert that gap are to be
  flipped when it merges.
* **Class-aware colouring — open research** (§10.3), handed to lane RF8 (RFC-0008) as the alternative to its merge-admission design.

## 0. Where the implementation stands

| piece | state |
|---|---|
| **B** — a REAL attempt carries the slot's tick, one tick per slot (`palw_real_clock_tick_v1`) | built, tested (00.6) |
| **A″** — the floor is the idle-only bonded fallback, behind the floor state machine, 20 / 8 / 20 (`palw_floor_reserve_v1`) | built, tested (00.1–00.5); **the producer hold is default-on** |
| **Independent heartbeat** — algo 8, bondless, claimless, fee-only, epsilon weight | retained under ADR-0066/0140; never replaced by FALLBACK-V1 |
| **FALLBACK-V1 integration** — formerly proposed bonded heartbeat and fallback accounting | **WITHDRAWN 2026-10-07 (§5–§8); not built, no planned fence** |
| anchors under Normal (lane A's binding; the round seed and the jury) | **00.9**: measured in-tree; **anchor duty built** (binding); the round seed / jury are `palw_anchor_window_v1`'s (separate fence); class-aware colouring (§10.3) remains an unbuilt research option |
| header-level floor invalidation / class-aware colouring / re-anchored inference | **open problems, §10, not built** |

## 1. The defect

The 600 blocks of testnet-12 at DAA 3,939–4,071: 67 % attempts, 33 % heartbeats, and almost every attempt the
`PALW-BASE-0` floor class. Two distinct mechanisms contribute to liveness — the bonded BASE-0 floor model attempt and the independent heartbeat — and
together they were two thirds of the observed chain. BASE-0 is a model class; heartbeat carries no model work. And the floor was not only idle weight: it buried the
models that did run (00.2).

## 2. The block kinds

| kind | what it is | claim / panel / court | clock | weight | reward |
|---|---|---|---|---|---|
| **REAL** | an attempt of a registered, Active class other than `PALW-BASE-0` | yes (unchanged) | can carry the slot's tick (B) | its pwu (ADR-0149) | unchanged |
| **BASE-0** | the bonded floor model class; A″ accepts its claim only while Idle | yes: floor claim, panel and court | carries the tick like an attempt under B | existing floor useful-work weight | existing floor useful-work reward |
| **HEARTBEAT** | independent emergency clock/liveness lane, algo 8; bondless | no claim, no panel, no useful-work credit | emergency slot tick under its existing rules | epsilon (`ε`), not floor weight | fee-only, zero useful-work subsidy |
| **ROUND / EXEC** | algo-10 execution lane block under ADR-0125; raw GHOSTDAG red | its execution permit; no new floor claim | DAA +0, never a selected parent | no added blue score/work or useful-work claim credit | existing round fee payout |

BASE-0 and heartbeat remain different lanes with different admission, collateral and accounting. An RPC `blockKind=FALLBACK` for a post-fence floor attempt is a retained compatibility spelling for **BASE-0**, not the withdrawn FALLBACK-V1 block kind. `LEGACY_HEARTBEAT` and `LEGACY_FLOOR` are old RPC names, not a decision to retire either mechanism.

## 3. How the rule got here (and A′, superseded)

旧 hysteresis ledger は last accepted REAL attempt の記録で足りるため撤回した。A′ の floor 全廃は、REAL producer が少ない期間を安価に書き換えられる unbonded heartbeat だけにしてしまうため不採用。現在の A″ は BLUE による成功確認と bounded Probe・cooldown で bonded floor の停止と復帰を制御する。

## 4. B — see 00.6

## 5. FALLBACK-V1 integration - WITHDRAWN 2026-10-07

heartbeat と bonded floor を FALLBACK-V1 に統合する案は採用しない。claim・collateral・有用計算報酬を持つ floor と emergency clock を混同し、独立した heartbeat の役割を失わせるため。BASE-0 と bondless・fee-only・epsilon-weight の heartbeat は別 lane として維持する。

## 6. Fallback accounting - WITHDRAWN 2026-10-07

統合専用の fallback credit・fork-choice key・ledger は実装しない。FALLBACK-V1 自体を撤回したため。既存 REAL・BASE-0 の work/reward と heartbeat の会計は維持する。

## 7. Security consequence of keeping the lanes separate

The bonded floor continues to provide its existing useful-work collateral and claim lifecycle in Idle. Heartbeat provides emergency clock/liveness with epsilon weight and no bonded economic security. A heartbeat-only stretch must not be described or accounted as a bonded floor stretch; the existing risks and measurements in 00.4 remain relevant. A″ and B retain their existing limits, including the producer-hold dependence measured in 00.2.

## 8. Implementation boundary

本 ADR が扱う実装は A″ と B の既存検証・release record に限る。撤回した FALLBACK-V1 用の fence・migration は追加しない。

## 9. Prerequisites and the release

`validate_palw_useful_work_v1`: A″ needs ConsensusV2 and `palw_model_registry` at or below it, and the bundle's mirror
(`sync_palw_floor_reserve_v1`); B needs ConsensusV2 and `palw_anchor_clock`, `palw_single_lottery`, `palw_clock_cursor`,
`palw_clock_floor` and `palw_clock_lead_cap` at or below it. Release entries: `PALW_T12_FLOOR_RESERVE_ENTRY`,
`PALW_T12_REAL_CLOCK_TICK_ENTRY` (`PALW_T12_USEFUL_WORK_FENCES_V1`); the fence name `palw_floor_reserve_v1` is kept for A″.
State: delta variant 104, carriage tail `0xEA`. The three constants are in the fingerprint (`palw_floor_reserve_value_v1`).

## 10. Open research beyond A″ + B (no follow-on fence selected)

The fold-level rule leaves one thing open, and 00.2 measured how much it matters: **a floor-lane block the fold refuses still
exists, and still colours.** Three directions, an honest account of each, and the boundary problem that makes all of them hard.

**10.1 The boundary problem.** GHOSTDAG colouring and the DAA are **header-stage**: a node orders headers first (IBD,
pruning-proof sync) and judges bodies and state later at the virtual stage; the class, the bond, the room and the registry live in
state. Anything that must stop a floor block from counting *at colouring* has to be decided from header data alone. A header names
its lane and its claimed class, but **none of it is authenticated at that stage**: the class lottery needs the class target, the
signature needs the bond's key, both in state.

**10.2 Header-level floor invalidation (the direct answer, not built).** Make a floor-lane block invalid at the header stage when
the chain is not Idle, so it never enters the DAG. It needs a floor state at the header stage. Two findings against building
it as a recursive header-level machine:

* **Header-claimed events are forgeable.** The only REAL signal a header carries is its own claim. Cheap forged REAL-claiming
  headers (their bodies are refused later) would drive the machine — holding the floor off at will (a denial of the fallback), or
  flipping it at will — at the price of a header's proof of work, which for an attempt-lane header is small. A state machine
  must not be driven by data nobody has checked.
* **A state-based cooldown has unbounded memory.** The header stage derives its values from a bounded window of ancestors (ADR-0142
  §6a: a pruned node and an archival node agree because the cursor is a function of a window). `last_probe_end` reaches back as far
  as the last unanswered probe — arbitrarily far — so a node that starts at the pruning point cannot rebuild it from headers. The
  fold-level machine has no such problem because its state is *rooted* and travels in the carriage against a committed root.
  A header-level one would have to commit its state in the header (a header-hash change) and validate each header's value as the
  step from its parent's — which fixes the memory, and not the forgery.

**10.3 Class-aware colouring (an unbuilt structural research option, historically handed to lane RF8 as an alternative to merge admission).**
ADR-0105's `LaneColoring::Weighted` already makes heartbeats invisible to a bonded candidate by lane. Extending it so floor-lane peers
are invisible to a REAL candidate needs no state machine, no probe, no producer hold and no anchor duty: the floors keep flowing, so the
anchors (the round seed, the jury, lane A's binding), the clock and the bonded stretches are exactly as they were, and REAL BLUE stops
depending on anyone's compliance — P2's measurement of the dependence is 00.2's table (one non-compliant producer at a tenth of the floors
takes REAL BLUE from 97 % to 72 %). A REAL candidate's class is in its commitment, readable at the header stage; the floor peers would be
told apart the same way. The floor, for its part, would still yield to REAL blues as a heartbeat yields to bonded ones.

**The open question, which decides whether it is safe: what unverified BLUE does it grant a header-claimed REAL attempt once floors no
longer compete with it?** A header can claim a REAL class without anyone having checked the claim at that stage (the class target, the
bond's registration, the room and the lottery are state; the envelope's signature against its embedded key is the one thing the relay path
checks statelessly). Today such a header is coloured classically against the floors around it, and at `ghostdag_k = 1` most of them turn
RED. With floors invisible to it, a forged REAL-claiming header would be BLUE against everything but other REAL-claiming headers — and
BLUE is what lets a block's header-stage weight (the constant every attempt-lane header adds, the fork-choice attack review's verdict 1)
count towards GHOSTDAG's blue work and the selected chain. Three things to settle before it ships: (1) how much blue work an adversary
gains per unit of proof of work by minting REAL-claiming headers with no valid body (bounded by the per-header work, by `k` among the
forgeries themselves and by the merge-depth floor `Weighted` already applies, but not yet measured); (2) whether the immunity should be
conditioned on something a header can carry and the header stage can check — the claimed class being one the carried registry root lists,
or the bond being named in a header-committed bond set — at the price of header bytes; (3) the effect on the selected parent, since a
REAL attempt then out-weighs a floor chain it used to lose to. It also touches every site that computes GHOSTDAG (the pruning proof's
two included, which take the colouring as a constructor parameter), so it needs its own fence, its own drill and a fork-id.

**10.4 Re-anchoring inference work (the cure for the slow producer itself).** A REAL attempt's execution is bound to the template
it was drawn on (ADR-0072: the ticket is the execution), so a chain that moves for three slots under a draw makes its block a late
side block — the slow-producer problem that A″ mitigates by silence. A producer could carry a finished inference to a newer tip
if the job anchor came from a coarser clock — the slot — instead of the exact parent set. That touches ADR-0072's anchor, ADR-0152
J-1's weight cap and the replay rule; it is the structural cure and is not designed here.

**10.5 Shared slot ticks, independent heartbeat.** B lets an attempt carry a slot tick under the existing cursor, lead-cap and one-tick-per-slot rules. The independent heartbeat remains an emergency tick source. Sharing the clock does not combine their lanes, collateral, claims, weight or reward, and does not retire heartbeat. The former FALLBACK-V1 replacement in this subsection is withdrawn.

**Current direction:** retain A″ + B and the independent heartbeat. The historical DAA-5,300 plan retained A″ + B with the producer hold on by default. Questions 10.1-10.4 remain research only; none is selected or activated here, and none revives the withdrawn integration.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md -->

</details>

