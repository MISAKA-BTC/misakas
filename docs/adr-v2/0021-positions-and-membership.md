# ADR-v2-0021 — Position・curve・membership・非譲渡性

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)

## 1. この文書の役割

ModelPositionを報酬請求権や譲渡可能tokenと混同せず、その権利と検証をまとめる。

## 2. 統合した判断の読み方

### 2.1

Positionはstate fold上の残高としてcurveから購入・売却する設計で、native UTXOと同じものではない。[旧ADR-0087](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)

### 2.2

Positionはmembershipであり、自動的な配当や採掘報酬の受領権ではないという変更を反映する。[旧ADR-0095](../adr/0095-a-position-is-a-membership-not-an-income.md)

### 2.3

membershipのchain認証と第三者による証拠配布を区別し、Positionをholder間で移転させない条件を保持する。[旧ADR-0101](../adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md)

## 3. 維持する境界・後続改定

後続ADR-0179のPosition対象範囲（追加モデルのみ）と、genesis／年次指定モデルに対する適用を必ず照合する。市場が存在することをモデルの正当性や取得可能性の証明にしない。

## 4. 隣接する判断との分担

seedとreserveは[ADR-v2-0022](0022-liquidity-seeding-and-virtual-reserves.md)、ownerへの手数料は[ADR-v2-0023](0023-model-reward-routing-and-owner-leg.md)、EVMの窓は[ADR-v2-0007](0007-evm-exec-lane-and-anchors.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) | L1–L394 | L18–L25 |
| [0095-a-position-is-a-membership-not-an-income.md](../adr/0095-a-position-is-a-membership-not-an-income.md) | L1–L334 | L20–L40 |
| [0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md](../adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) | L1–L207 | L12–L34 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0087-a-position-is-bought-from-the-curve-and-sold-back-to-it"></a>

<details>
<summary>0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md — 原文 L1–L394</summary>

[原文を開く](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) · SHA-256: `42c96943def3b953216e34becdf18fffa054357372bedb3cd3eee9ebb508e55d`

<!-- ADR_SOURCE_BEGIN:0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md -->
~~~~~~~~markdown
# ADR-0087 — a position is bought from the curve and sold back to it

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 改定 — Positionと市場の対象を固定する

[ADR-0175](0175-registered-models-are-permanently-immutable.md)を適用する。市場の`line_id`は固定されたmodel registration IDであり、Position、reserve、seed、reward buybackとEVMの参照先を改善版へ付け替えない。改善版は自身の登録・検証責任・報酬資格・独立AMMを持つ。既存市場の残高、価格、virtual reserveや実reserveを新登録へ継承・自動移動させない。新規登録が他モデルの市場を変更してはならない。以下に残るversion移行への期待は旧設計の記録であり、新規則では権利を与えない。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

**Status:** PROPOSED 2026-09-05, design only (no implementation yet). Requested by the operator
on 2026-09-05: "Model Positions" — a per-model, fixed-supply position whose price is set by
market participants' beliefs about the model's future (its usage, its PALW work, its evaluation,
its migration to new versions, the scarcity of its capacity), traded against BILI on an AMM, with
1 % of every trade to the model's registrant and 5 % of every trade burned, and with **no
transfer between holders**, so that a position is never something one person hands another.
The operator's word is *position* (ポジション), not *share*: it is bought from the protocol's
curve and sold back to it, and that is the whole of what it is.

> **Amended a fifth time (2026-10-01, implemented the same day behind the dormant fence `palw_model_virtual_v1`).** [0162](0162-the-pair-opens-on-a-virtual-reserve.md): past the fence Decision 2's virtual reserve returns, recorded in each row — the curve is over `X = V + reserve` with `V` = 10,000,000 MSK that no object ever pays out (a sell is refused beyond the real reserve above the locked seed) — and a line's market exists from the line's creation, trading only while its class is `Active` (approved). A market a seed opened before the fence keeps `V = 0` and quotes as it always did. §0 below holds unchanged (ADR-0162 §7).

> **Amended a fourth time (2026-09-07, design).** [0095](0095-a-position-is-a-membership-not-an-income.md): a position stops granting nothing. It still buys no income, no weight and no vote — ADR-0091 settled that — but it now carries whatever its LINE has declared for its holders: a new version's artifact before the promotion, the private beta, experimental modes, the developer's room. The grant set is closed and contains nothing that pays; the chain proves the holding and publishes the promise, and the serving stays with whoever serves. **§0 below is the canonical statement of what a position is and is not, and is the section to quote when anyone reads this market as shares.**

> **Amended a third time (2026-09-06, design first).** [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) adds the chain's own move to Decision 3: at a claim's `Final`, five percent of its escrowed worker reward buys from the pair of the line the claim ran, the positions the curve gives up are retired (the chain's, for good — M1 counts them), no leg is taken (Decision 4), and the miner is named the other ninety-five percent; nothing is ever distributed to a holder. Decision 8's row gains `buyback_sompi` and `retired_units`.
>
> **Amended again (2026-09-05, implemented the same day).** [ADR-0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) retires the virtual reserve and the lazy opening: a market opens only by a **seed** of at least 100,000 MSK that becomes the reserve, fee-free and locked for good (the reserve never falls under it); a position is **whole** and there are **500,000** a line; a third move (`ModelSeed`) rides beside the buy and the sell. Decisions 1, 2, 3, 4 and 8 below read with that in mind.
>
> **Amended (design, 2026-09-05, revised the same day).** [ADR-0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) keys this market by a
> *line* — a class, an owner, a name — of which the class's own line is the first, with the class id
> as its key, so every value here is unchanged for a class that has only its own; Decision 4's
> registrant leg becomes the line's *owner's*, shared with an adopted contributor when the owner
> says so; Decision 7's "a new version is a new class with a new market" is narrowed to a new
> *graph*: new weights are a new *version* of the line, published by its developer, and the
> position stays where it is. ADR-0088 is design only. [ADR-0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) (design, 2026-09-05) adds the EVM's
> two doors to Decision 3's two moves and three read precompiles plus a per-class MRC-20 facade to
> Decision 8; §1's "an optional, non-default feature" is stale — the lane is a default build since
> 2026-08-21. Map: [`README.md`](README.md).

> **2026-10-04 (the operator):** no served inference exists — every job is self-ordered (ADR-0144 P1);
> "priority in the queue" and "a served quota" were removed from the benefit list (ADR-0095's amendment).

## 0. A position is not a share — it is how a model's usefulness is held

**A Model Position is not stock, not equity, not a share of an enterprise, not a security by
intent, and no surface of this project may present it as one.** There is no company, no issuer,
no capital raised, no profit, and no person who owes a holder anything. What a position *is*:
**a fixed, non-transferable place in one model line's pair, the only thing the protocol itself
ever does to its price being to buy the pair with the reward the model's own use earned, and
which buys its holder a service from that model's developer** — the
new version's artifact before it may be promoted, the private beta, experimental modes, the developer's room ([0095](0095-a-position-is-a-membership-not-an-income.md)).
It is the way to hold *the added value of an LLM*: not a claim on someone's earnings, but a
position in the usefulness of a specific model, priced by how much that model is actually asked
to do.

This is not a disclaimer bolted on after the design. It is what the rules below already do, and
every row names the rule, so a reader checks it instead of believing it.

| what a share does | what a position does — and the rule that makes it so |
|---|---|
| is issued by a company that owes its holders | **no issuer exists.** A line is a `(class, owner, name)` row and its owner is a *publisher of weights*, not a debtor ([0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) Decision 1) |
| pays a dividend out of profit | **nothing is ever paid to a holder.** The fold has no move that pays one; the only BILI a holder ever receives is what the curve pays for a position they themselves sell back ([0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) §0) |
| is a claim on the enterprise's assets | **a holder has no claim on anything** — not the seed, not the reserve, not the weights, not the owner. The seed is locked for good and is paid back to nobody, not even the one who locked it ([0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) Decision 2) |
| carries a vote, a seat, governance | no weight, no vote, no seat, no quorum, no bond (§2, Decision 5); the grant set a line may declare is **closed at the fold** and has no bit for any of them ([0095](0095-a-position-is-a-membership-not-an-income.md) §4.2) |
| is transferred, lent, pledged, wrapped, scalped | **no transfer object exists**, on either lane. The only way in is to pay the curve and the only way out is to sell back to it; the MRC-20 facade is ERC-20's read half with the curve where its transfer half would be, and `supportsInterface(ERC-20) == false` (Decision 5; [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)) |
| returns the fruits of an issuer's and other people's labour | **nothing is owed to a holder and no work is directed at one.** Miners mine for their own ninety-five percent and users ask the model for their own reasons; the five percent that reaches the pair is a rule of the chain, not somebody's effort on a holder's behalf, and the protocol never supports the price — the operator's own constraint ([0095](0095-a-position-is-a-membership-not-an-income.md) §1) |
| matures, is redeemed, accrues interest | there is no maturity, no redemption value, no interest, and no promise of any price at any block. A seller is paid exactly what the curve's arithmetic gives at that block, and never more |

### 0.1 How the LLM's added value reaches a position, and there is no second path

Someone asks the model to do something. PALW prices that work and a block is produced with it.
The block's worker reward is *escrowed, never minted* ([0042](0042-palw-mainnet-candidate-ruleset.md)
Decision 10). When the claim reaches `Final`, **five percent of that reward buys positions out of
that line's own pair and the chain retires what it buys, for good**; the miner is named the other
ninety-five ([0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)). The reserve is
deeper and the positions are fewer, so the curve pays more for each one that is left.

    a person uses the model  →  PALW prices the work  →  the block escrows the worker reward
                             →  at Final, 5 % buys this line's pair and the units are retired
                             →  the curve's price per position rises for everyone equally

Nothing was distributed, nobody was handed anything, and the value arrived **because the model
was used**. A line nobody uses buys nothing back, however highly anyone speaks of it. That is
the sense in which a position receives an LLM's added value, and it is the only sense this design
supports: usage is the input, the pair is the meter, and the price is the reading.

### 0.2 What a holder actually receives is a service, not a return

[ADR-0095](0095-a-position-is-a-membership-not-an-income.md) makes the position a **membership in
the line**: the artifact of a new version ahead of its promotion — a window the fold *enforces*,
refusing an early promotion rather than trusting a promise — the private beta, experimental modes, the developer's own room, a voice in what ships next,
and support. The set is closed at the fold and contains **nothing that pays**: no share, no
rebate, no discount in BILI, no claim on the reserve, and an unknown grant bit is *refused*, not
stored and ignored. **A grant is a service or it is not a grant.** What is bought is access to
what the model can do; what is not bought is anyone else's income.

The one place a holder may legitimately be paid BILI by a line is the contributor share of an
adopted **proposal** ([0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
Decision 8) — pay for work that was adopted, open to holders and non-holders alike, not scaled to
units and not owed to anyone for holding.

### 0.3 The words, because the words are where the misreading starts

In this repository, in the CLI, in the RPC, in the explorer and on misakaoptions.com these are
**positions**, held by **holders**, in a **line**, conferring a **membership**. They are never
*shares*, *stock*, *equity*, *securities*, *dividends*, *yield*, *investors*, *株*, *株式*,
*配当*, *出資* or *利回り*, and no page, wallet label or release note should render them so. The
operator's word has been *position* (ポジション) since the first line of this ADR; §0.1 is why
that word is the accurate one and not a euphemism.

**What this section does not decide.** It states properties the design has and refuses, each one
checkable against the fold. Whether those properties satisfy a particular jurisdiction's
definition of a security or of an exchange business is a question for counsel — Decision 5 has
said so since the first draft, and stating the intent more strongly does not turn it into a
verdict.

## 1. What exists, and what a market can therefore see

* **A model is a class.** `PalwClassRowV2 { class_id, status, share_permille, budget_blocks,
  canonical_leaves, is_base_class }` (palw_state_v2.rs); a post-genesis class carries
  `registrant_bond: Option<PalwBondKeyV2>`, `None` exactly for the classes the assembly
  registered at genesis (ADR-0056). A bond's `payout_payload: Hash64` is where consensus pays
  its holder. The registrant is therefore an on-chain identity with a pay address — the
  "model's adder" this ADR pays.
* **Consensus already pays and burns.** A claim's `escrowed_reward` becomes
  `PalwPayoutV2 { payload, amount }` in `pending_payouts`, honoured by the coinbase; bonds lock
  a collateral outpoint by rule (`palw_bond_collateral_is_locked_v2`) and a slashed bond has a
  burn obligation (`palw_bond_burn_obligation_v2`). "Value held by the protocol and paid out by
  the fold" is a shape the chain has, on the UTXO model it has, without covenants.
* **Objects are how the chain is told things.** `PalwConsensusObjectV2` variants ride carrier
  transactions (`misaka-cli palw submit-object`, chunk groups under the 81,920-byte carrier —
  ADR-0080), are applied in the state fold (`apply_object`), and every new rule is armed by an
  `Option<ForkActivation>` on the params (twelve exist; `palw_rc_arm_phase1` sets the shipped
  ones). The state is at `PALW_STATE_V2_VERSION = 20`.
* **The premine is 10 B and the rule is carve-not-mint** (ADR-0059): nothing here may mint.
  Everything a position pays out must have been paid in, and a burn is the only way supply moves.
* **Prior art in this repository:** the Token Program of 2026-08-10/11 (an SPL-style,
  consensus-store account ledger with transfer and burn, on the retired VLT lineage) — an
  account ledger inside consensus is a shape that was built once and reviewed. The EVM lane
  (ADR-0020) is an optional, non-default feature and is not the substrate here.
* **What the chain knows about a model, per class:** claims accepted on each lane, receipts
  licensed, the work priced on the free-prompt lane (`work_leaves`, ADR-0083), the budget granted
  and used per epoch (`budget_blocks`, `share_permille`), certification (ADR-0075), the registrant,
  the class's status. **What it does not know:** any human preference score, any "inference per
  day" other than the count of claims — those are the explorer's derivations (misakascan) and
  never a consensus fact. A market prices from the first list and from whatever its participants
  believe; this ADR gives it the first list and nothing else.

## 2. The requirement

A per-class position with a fixed supply, bought from a protocol-owned curve in BILI and sold
back to it, never moved between holders; every trade burns 5 % of its BILI leg and pays 1 % to
the class's registrant; the whole is supply-neutral except for the burn; every balance and every
price is a function of the chain alone. As first written, a position granted nothing but the
right to sell it back: no weight, no vote, no seat, no fee discount, no bond — so its price is
exactly the market's belief about the model and nothing the protocol adds. That last clause is
the requirement's spine and survives every amendment: **the protocol never supports the price,
never pays a holder and never promises one anything.** What [0095](0095-a-position-is-a-membership-not-an-income.md)
adds on top of it is a *service* — the line's own membership grants, a closed set with nothing
that pays in it — and what [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) adds
underneath it is the model's **use**, buying the pair with the reward the use earned. Neither is
a return on an enterprise, and neither makes a position a share of one; §0 is the statement of
that, at length, with the rule behind each clause.

## 3. Decisions

**Decision 1 — a position is a balance in the state fold, not a coin and not a UTXO.**
Per class: `PalwModelMarketV1 { class_id, opened_daa, msk_reserve, position_units, sold_units,
burned_sompi, registrant_paid_sompi, closed_to_buys }`. Per holder: `PalwModelPositionV1 {
class_id, holder: Hash64 (the holder's payout payload — the same identity a bond pays),
units: u64 }`. One position is `10^6` units; every class opens with
`PALW_MODEL_POSITION_SUPPLY_V1 = 100_000` positions, a network constant so that no model is
issued more room than another — the operator's example, and the number this ADR carries into
the tests. The state root covers markets and positions (state v21).

**Decision 2 — the curve is constant-product over the reserve plus a virtual reserve, and the
curve is the only counterparty.** A market opens with the whole supply in the curve and no BILI:
`(msk_reserve + V) × position_units = K`, `K` fixed at opening as `V × supply`, where
`PALW_MODEL_MARKET_VIRTUAL_SOMPI_V1 = V` is a network constant that sets the first position's
price (`V / supply`) and the curve's steepness. No liquidity provider, no pool token, no pair
other than BILI↔class, no market a user can create: the market of a class is opened by the fold
when the class is registered (post-genesis) or when this rule activates (genesis classes).
The price at any moment is `(msk_reserve + V) / position_units`; there is no other price.

**Decision 3 — two moves, and only two.** `PalwModelBuyV1 { class_id, holder, msk_in,
min_units_out }` and `PalwModelSellV1 { class_id, holder, units_in, min_msk_out }`, both
`PalwConsensusObjectV2` variants in a carrier transaction. A buy's carrier pays `msk_in` to the
class market's sink — a consensus-recognised, provably unspendable output the fold credits to
`msk_reserve`; the fold computes `units_out` from the curve over the NET leg and credits the
holder, refusing the object when `units_out < min_units_out`. A sell is signed by the holder's
key; the fold debits `units_in`, computes the gross BILI leg from the curve, and writes a
`PalwPayoutV2 { payload: holder, amount: net }` the coinbase honours, refusing the object when
`net < min_msk_out`. The reserve never sits in a spendable output: it is an accounting entry
funded by sinks and drained by coinbase payouts, exactly as escrowed rewards are today.

**Decision 4 — the fee is on the BILI leg of every move, split three ways, and the split is
the operator's.** Of a gross BILI leg `m`: `burn = 5 % of m`, never paid to anyone and subtracted
from supply; `registrant = 1 % of m`, a `PalwPayoutV2` to the class's `registrant_bond`'s
`payout_payload`, or burned as well when the class has no registrant (a genesis class); the
remaining `94 %` is the net leg — on a buy it enters the reserve, on a sell it is paid out.
A round trip therefore costs 12 % plus the curve's own slippage; the record below carries the
arithmetic so nobody discovers it from a wallet.

**Decision 5 — no transfer exists.** There is no object that moves units from one holder to
another, and none that lends, locks, wraps, delegates or pledges them: a position is not a bond,
not collateral, not a fee, not a seat, not weight. The only way a position changes hands is
through the curve, and then it is not the same position but a new balance bought at the
curve's price. This is the design's answer to the operator's constraint that a position must
not be something exchanged between persons.

Read with §0, this decision is the load-bearing half of "a position is not a share": a thing that
cannot be handed to another person cannot be placed with investors, cannot be lent or scalped, and
has no holder register anyone could take over. It is a membership one buys from the protocol and
returns to the protocol — **a position in a model's usefulness, priced by that model's use** —
and the closed grant set of [0095](0095-a-position-is-a-membership-not-an-income.md) §4.2 keeps
it that way as the design grows: a line may declare a service, never a payment. Whether the
design meets a legal definition of a security or of an exchange business is a question for
counsel, and this ADR records the intent, not the verdict.

**Decision 6 — the market is a consensus rule, armed by activation, never by regenesis.**
`palw_model_market: Option<ForkActivation>` on the params; below the activation the objects are
refused and no market exists; the fingerprint moves only where the flag is set. Consensus
changes go by activation on this chain (the standing rule); this one is no exception.

**Decision 7 — a new version is a new class with a new market; nothing migrates by itself.**
Positions in class A are positions in class A. When a class leaves `Active` (retired,
superseded, frozen) its market `closed_to_buys` becomes true; sells continue at the curve's
price until the reserve is drained; nothing is redistributed, minted or moved to the successor.
A registrant who wants holders to follow a version sells the story, not a migration.

**Decision 8 — what a participant reads.** RPC `getPalwModelMarket(class_id)` (reserve, units,
price, supply sold, burned, paid, status), `getPalwModelPositions(holder)`; per-class chain
counters as they already exist in the class facts; the explorer's Model Market page derives
everything else. CLI: `misaka palw model buy|sell|show`.

## 4. What this costs, stated before it is measured

* **Trading:** 6 % of each leg leaves the trade (5 % burned, 1 % to the registrant), so a round
  trip is 12 % before slippage. Worked with `V = 1,000 MSK`, supply `100,000`
  (`K = 10^8 MSK·positions`), from an empty market:

  | move | gross MSK | burn (5 %) | registrant (1 %) | net (94 %) | positions out / in | price after (MSK) |
  |---|---|---|---|---|---|---|
  | buy | 1,000 | 50 | 10 | 940 → reserve | 48,453 out | 0.0376 |
  | buy | 1,000 | 50 | 10 | 940 → reserve | 16,824 out | 0.0829 |
  | sell all 65,277 | 1,880 from reserve | 94 | 18.8 | 1,767.2 paid | 65,277 in | 0.0100 |

  (Corrected twice, and the second time is why the first is written down. The design draft read
  `12,846 out`, `0.0742`, `3,014 from the reserve` and `2,833 paid` — arithmetic the curve cannot
  produce, since a reserve holding 1,880 pays at most 1,880, and 3,014 would have been 1,134 MSK
  nobody paid in, against M2 and against ADR-0059's carve-not-mint rule. The first correction, made
  with the implementation, fixed the positions and both MSK columns but left the second row's
  *price* at `0.0576`, a number belonging to neither reading; reported as
  [#98](https://github.com/MISAKA-BTC/misakas/issues/98), which reproduced row 1 exactly and so
  established that the curve is being read as written. `(1,880 + 1,000) / 34,722.22 = 0.082944`.)

  **This table is the pre-[0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  curve and nothing computes it.** It is kept because it is the record of what this ADR decided;
  the virtual reserve `V` it turns on was retired the same day, and the arithmetic the tree
  actually runs is ADR-0090 §4's — pinned by `the_adr_table_is_the_curves_arithmetic` in
  `consensus/core/src/palw_model_market_v1.rs`, which is the golden table M4 asks for. A worked
  example with no test behind it is a number that drifts, and this one drifted twice.

  Two buys of 1,000 MSK and one sell of everything bought return 1,767.2 MSK of the 2,000 paid in
  — a round trip of `0.94² = 0.8836`, exactly what M4 states, and 11.6 % less than went in, which
  is the 12 % this section's own first sentence quotes. When every unit comes back the curve's `x`
  returns to `V`, so the gross leg is exactly the reserve: there is no last-out loss beyond the
  fees, and no first-in gain beyond the curve. `V` itself is never paid to anyone — it sets the
  opening price and the curve's steepness and nothing more, which is why the gross leg tops out at
  the reserve rather than at `reserve + V`. (ADR-0090 replaces it with a real, locked seed, so the
  question of what stands behind the opening price stops being rhetorical.) A partial sell is paid
  on the curve's slope at that point, which is the only place slippage lives.
* **State:** one market row per class, one position row per (class, holder); ≈ 150 bytes each.
* **Fold:** O(1) per move; a carrier per move (≈ 300 bytes plus the sink output).
* **Consensus:** an activation; a state version; two objects; one sink script; one payout kind.

## 5. Invariants the tests must hold

* **M1 (supply).** For every class, `position_units + Σ holders' units = supply`, always.
* **M2 (value).** For every class, `Σ msk_in = msk_reserve + Σ payouts + burned + registrant_paid`,
  always; nothing is minted.
* **M3 (no transfer).** The object set has no variant whose effect is a change of two holders'
  units in one class; a property test over every object kind.
* **M4 (the curve).** The price is `(msk_reserve + V) / position_units`; buying raises it,
  selling lowers it; a buy and an immediate sell of what it bought returns `0.94² ×` the gross
  less slippage, and the fee arithmetic is fixed in a golden table.
* **M5 (protection).** `min_units_out` and `min_msk_out` refuse, never partially fill.
* **M6 (lifecycle).** A class that leaves Active refuses buys and honours sells until the
  reserve is empty; a genesis class's registrant fee is burned.
* **M7 (determinism).** The fold over a recorded sequence of moves reaches one state root on
  every node; the fingerprint is unchanged where the flag is `None`.
* **M8 (the address).** A holder is its payout payload; a sell is signed by the key that
  payload names; no other key can sell it. (Amended 2026-09-23, §10: the holder is the key's id,
  which is not a payout payload, and a sell pays the key's own address.)

## 6. Order of work

1. State: markets, positions, v21, the two objects, the sink script, the payout kind, M1–M8.
2. Params: the flag; the fingerprint pin test for `None`.
3. RPC, CLI, the explorer page.
4. A devnet drill: register a class, buy, sell, retire, drain; then testnet-11 by activation.

## 7. Implementation record (2026-09-05, `palw-adr0084-served-answer`)

**Landed — §6 items 1–3 (state, params, RPC, CLI); the explorer page and the devnet drill are
not.**

| where | what |
|---|---|
| `consensus/core/src/palw_model_market_v1.rs` | The constants (`10^6` units a position, `100,000` positions a class, `V = 1,000 MSK`, 50 ‰ burn, 10 ‰ registrant), `PalwModelMarketV1`, the fee split (exact, remainder on the net leg), `palw_model_buy_quote_v1` / `palw_model_sell_quote_v1` over `(x + V) × u = K` with the rounding that keeps the product at or above `K` and the sell's gross capped by the reserve, the sink script `OP_RETURN "MSKMDL01" <class id>` and its reader, the holder as `BLAKE2b-512(pubkey)` — the same identity a bond pays — and the sell's signed message under its own ML-DSA-87 context. |
| `palw_state_v2.rs` | `model_markets` and `model_positions` on the state; **in the state root only when non-empty** — the root is committed in headers, so a chain on which the rule is dormant commits the roots a build without the fields computes; the carriage (persisted and served over IBD) encodes the two collections as a tagged tail only once a move was folded, so the legacy layout is byte-identical until then (`PalwStateCarriageV2Legacy` pins it); two delta entries with replay and revert; `ModelBuy { class_id, holder, msk_in, min_units_out, sink_index }` and `ModelSell { class_id, holder, units_in, min_msk_out, pubkey, signature }` appended to the object enum; the two fold arms (a buy opens the market lazily, requires `Active`, refuses under the floor; a sell debits, pays the net leg and the registrant's through `pending_payouts` keyed by the move; a class with no registrant burns that leg); eight error kinds. |
| `palw_lifecycle_objects_v2.rs` | A buy rides only when its carrier's output `sink_index` holds exactly `msk_in` under the class's sink script; a sell rides only signed. |
| `config/params.rs` | `palw_model_market: Option<ForkActivation>` with the `palw_da_court` contract; `palw_model_market_fence` / `palw_model_market_active_at`. |
| `virtual_processor/processor.rs` | Both objects refused by name below the fence at the block's DAA; a sell's signature verified at acceptance against the payload its key derives to (M8). |
| `mining/.../check_transaction_standard.rs` | The sink is the one unspendable output that is not dust — recognised by its exact script. **Amended 2026-09-05 (the devnet drill):** the same script is also carved out of the mempool's *output-class* rule, which refused the form before the dust rule was ever reached; and out of consensus's `check_transaction_pq_output_classes` (`tx_validation_in_isolation.rs`), gated by `TransactionValidator::model_sink_outputs_allowed` = "the network declares `palw_model_market`" — a dormant network keeps the PQ-only rule byte-for-byte. Without both, no carrier buy was ever relayed or consensus-valid on a PQ-only network; the fold tests folded the object but never validated its carrier. Test: `consensus_mode_admits_the_model_sink_only_where_the_market_is_declared`. |
| RPC | `getPalwModelMarket(classId)` (an unregistered class is `found: false`; an unopened market reads as the whole supply in the curve) and `getPalwModelPositions(holder)`, through core, service, gRPC (proto ids 1144–1147) and wRPC. |
| CLI | `misaka palw model-show [--quote-msk]`, `model-positions`, `model-buy --class --msk --min-positions --key --yes`, `model-sell --class --positions --min-msk --key --yes`; every move prints the quote the chain's own arithmetic gives against the tip and sends nothing without `--yes`. |

**Tests.** `palw_model_market_v1` (6): the corrected §4 table, the product never below `K` at
every size tried, a round trip returns at most `0.94²` of the gross, a closed market refuses buys
and honours sells, the sink names its class and nothing else does, the sell message binds its
fields. `palw_state_v2::tests::model_market` (5): M1 and M2 at every stop of a two-buyer run; M5's
floors refuse whole (`ModelBuyBelowFloor`, `ModelSellBelowFloor`, `ModelSellExceedsPosition`) and
fill at the floor exactly; M3 as "one move changes one holder's row"; M6 as a not-yet-Active
class refusing buys, a registrant paid 1 % to its payload, a registrant-less class burning it;
M7 as the deltas replaying and reverting to the same root and the carriage staying legacy until
the first move. The params test pins M7's fingerprint half. M4's monotonicity is asserted in both
suites; M8's signature check is at acceptance and is covered by reading, not by a test — a
processor-level fixture for a signed lifecycle object does not exist yet.

**What the implementation taught.** (1) The design draft's worked table was wrong twice (above);
the curve's arithmetic is now the test's golden and the table follows it. (2) "State v21" cannot
be a root-version bump: the state root is in every header, so the version and the collections
enter the root only where a move has been folded, and the carriage grows a tail rather than a
field. (3) "The market opens when the class is registered" became "on its first buy": the same
observable market at a fraction of the fold, and no edge at activation for genesis classes.
(4) The mempool's dust rule treats every unspendable output as dust; the sink needs the one
exception, matched on its exact script. (5, learnt on 2026-09-05 when `scripts/drills/misaka-palw-model-market-devnet-e2e.sh`
first ran a carrier buy through a live mempool) The dust exception was never reached: both the
mempool and consensus refuse a non-PQ output *class* first, and an `OP_RETURN` sink is one. The
carve-out now sits in both places, gated on the network declaring the market, so a dormant network
is unchanged. A rule tested only at the fold is a rule tested for the half that cannot refuse.

**Not landed.** Retire and drain under the devnet drill (§6 item 4 — buy and sell ran on both lanes
on 2026-09-05, ADR-0089 §9, once the sink's output class was carved out); arming on testnet-11.
The market's page is no longer another repository's: `web/misaka-options/` (served at
misakaoptions.com) reads the market over wRPC and the EVM windows, and offers only the two moves.

## 8. What is deliberately not decided

* The virtual reserve `V` and the supply constant: numbers the operator sets when the flag is
  armed; the tests carry the operator's example.
* Whether a registrant may seed its market with BILI at opening (a curve with a non-zero
  starting reserve); not needed for the curve to work.
* Any on-chain human-preference or usage metric beyond what the fold already counts.
* Any incentive to migrate holders to a successor class.
* Whether burned BILI is reported against the premine cap in the explorer (the cap binds
  genesis; burns only lower supply).

## 9. Number hygiene

This is ADR-0087. The README's next free number was 0087 after 0086's row; it becomes 0088 with
this row.

## 10. Amendment 2026-09-23: the holder id is not a payout payload (P-B4)

The 2026-09-23 Position route matrix found that no one could spend the proceeds of a sell (P-B4).
The holder was implemented as `mldsa87_key_id(pubkey)`, the **unkeyed** BLAKE2b-512 of the key, and
Decision 3's `PalwPayoutV2 { payload: holder }` put that id in the payout. The coinbase locks a
payout with `p2pkh_mldsa87_spk(payload)`. Its `OP_BLAKE2B_512` recomputes the **keyed** address
payload (`blake2b_512_address_payload`, the value in the wallet's address) at spend time. The two
hashes never match, so each sell's net leg was locked to an output no key can spend. A bond's
`payout_payload` is the keyed value, which is why the owner and contributor legs were not affected.

What changed is under `Params::palw_audit_2026_09_23`. Testnet-11 armed the market before that
fence existed, so its fold is unchanged byte for byte.

* The holder stays the key's id (`palw_model_holder_of_pubkey_v1`). Positions, the sell message,
  the RPC and the CLI name holders exactly as before.
* Past the fence, a carrier sell's net leg is paid to `palw_model_sell_net_payload_v1(pubkey)`. That
  is the keyed address payload of the key that signed the sell. The payout row's key still hashes
  the holder, so only the payee changes.
* Past the fence, the fold checks that the sell's key is the holder's
  (`ModelSellKeyIsNotTheHolders`), because that key is now the payee. Acceptance already refused a
  mismatch, so this is a second check.
* The EVM lane pays its net leg through the settlement, not through a payout row, so it has no payee
  to fix. No other market, line, benefit or buyback path pays a holder id. The owner, contributor
  and panel legs pay a bond's `payout_payload`.
~~~~~~~~
<!-- ADR_SOURCE_END:0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md -->

</details>

<a id="source-0095-a-position-is-a-membership-not-an-income"></a>

<details>
<summary>0095-a-position-is-a-membership-not-an-income.md — 原文 L1–L334</summary>

[原文を開く](../adr/0095-a-position-is-a-membership-not-an-income.md) · SHA-256: `f77645fe7798f075076a8fdf54e9f8c9c865c243805ba4ad85ef223385aa408c`

<!-- ADR_SOURCE_BEGIN:0095-a-position-is-a-membership-not-an-income.md -->
~~~~~~~~markdown
# ADR-0095 — A position is a membership, not an income

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 改定 — 固定モデルに対するmembership

[ADR-0175](0175-registered-models-are-permanently-immutable.md)を適用する。Positionは買った登録モデルから動かない。`EARLY_VERSION`のwire bitは履歴に保持するが、新しい不変規則下のbenefit宣言では拒否する。同一登録へのpreview発行や将来versionの先行取得を約束できない。改善版は別登録であり、既存Positionから自動的に権利を継承しない。

`PRIVATE_BETA`は当該登録の固定artifact・同じ実行仕様のprivateサービスへのアクセスとして用いる。priority、quota、support、developer roomもそのモデルの運用サービスである。異なるquantization・weights・実行仕様をexperimental serviceの名で既存登録に付け替えない。旧lead/promotion/cadence規則は履歴再現用に保持し、新規則でversion操作を再開する根拠にはしない。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-09-07
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 1's
  "grants nothing but the right to sell it back" — it now also grants what its LINE declares;
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) Decision 2 (a version's
  entry and promotion are constrained by the line's declaration) and Decision 12 (rows to read).
* **Amended 2026-10-04 (the operator): no served inference — every job is self-ordered.** MISAKA has no
  hosted or third-party inference service: a user submits their OWN job, the network's seats verify it, and
  nobody rents GPUs to run other people's jobs or offers an API on the chain's behalf
  ([ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P1: third-party serving is a
  non-goal). So the grants that describe a serving queue — bit 2 `PRIORITY_INFERENCE` ("the line's gateways
  serve holders' jobs ahead of others'") and bit 5 `INFERENCE_QUOTA` ("served capacity") — promise something
  that does not exist and are **retired**: no surface may present them as a benefit and a line should not
  declare them. (The consensus-inert code in `palw_model_benefits_v1` / `palw_service_descriptor_v1` still
  accepts the bits; refusing them at the fold is a later fence.) Likewise "gateway" below means the holder's
  OWN entrance (ADR-0096) or a service the line's owner runs off-chain — not a protocol service. What a Position
  can protocol-natively unlock in future is **holder-only job classes** (job admission reading the holding —
  e.g. a class whose jobs require `PositionRequired`); today no job admission path reads a Position, so a
  Position is an on-chain membership card whose other benefits are the owner's to provide.
* Builds on: 0087 (no transfer, the curve), 0088 (lines, versions, previews, evaluations,
  proposals, roles), 0090 (the seeded pair), 0091 (the reward buys the pair — income was already
  refused).

## 0. The sentence this ADR is

A position buys no income and no vote; it buys **what the line's developer owes its holders** — the
new version first, the private beta, the experimental modes, a voice in what
ships next — and the chain's job is to make the holding provable, the promise readable, **the
exclusivity window a rule the fold enforces**, and the abandoned promise lapse on its own.

## 1. What the operator asked, in the operator's words

> Model Position を「投機対象」から「AI サービスの会員権・アクセス権」に近づけられます。単なる
> 「保有していると報酬が貰える」ではなく、**Position を持っていることでモデル利用上の明確な優位性
> が得られる**ようにする。Early Access／Private Beta／Priority Inference／新機能の先行提供。Tier は
> 保有量で分けるが、**保有量に比例して利益を受け取るのではなく、あくまでサービス上の権利にする**。
> **Developer が、この Model Line の Position Holder には何を提供するかを設定できる**ようにする。
> 重要なのは、**MISAKA プロトコルが価格を支えないこと。**

And, on the first draft of this ADR:

> **より Holder が モデル提供者からの恩恵を受けられる設計に**強化修正して。

The first draft declared everything and enforced nothing: every benefit sat behind "the developer
promised, and the market punishes a liar". That is too little. This revision keeps the boundary
honest — the chain still cannot serve an inference — but takes every part of the promise that IS
chain-visible and makes it a rule instead of a hope. Sections 4.4, 4.6 and 4.7 are that change.

## 2. Why a membership, and why this market was already shaped for one

[ADR-0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) removed the last thing that
looked like income: five percent of a model's mining reward buys from its pair and the chain retires
what it buys, so **no holder is ever handed anything**. What was left was a position that grants
nothing at all. A thing that grants nothing and can only be sold is the definition of a speculative
object, and that is the wrong shape for a network whose point is models people use.

Three properties this market already has make a membership the natural next thing, and they are not
properties a token normally has:

* **A position cannot be transferred** (ADR-0087 Decision 5). A membership that cannot be
  transferred cannot be scalped or lent; the only way in is to pay the curve, which RAISES the price
  for the next entrant, and the only way out is to sell back, which lowers it.
* **The supply is fixed and whole** (ADR-0090): five hundred thousand memberships a line, forever,
  and a holder's stake is a count anyone can read.
* **Usage already buys the pair** (ADR-0091), so a model that is actually mined buys its own
  memberships. Service demand and production push the same number, and **nothing here promises a
  price or pays a holder** — the operator's constraint that the protocol must not support the price
  is kept by construction.

## 3. The boundary, and what is on each side of it after this revision

**The chain cannot serve an inference, cannot keep a published root secret, and cannot see a
download.** Any design that pretends otherwise is lying to holders. What it CAN do is more than the
first draft admitted:

| the chain ENFORCES (a rule, refused at the fold) | the chain RECORDS (a fact, read by anyone) | the chain CANNOT (and does not claim) |
|---|---|---|
| a version may not be promoted before the declared lead has passed (§4.4) | who holds how many, at any height | serve the artifact, or run a queue |
| while an early-access lead is in effect, a version must ENTER as a preview (§4.4) | what the line declared, and when | verify a download happened |
| a weakening of the declaration takes effect only after notice (§4.7) | how long a holder has held without selling | make a developer answer an email |
| a declaration that outlives its cadence or expiry stops granting anything (§4.6) | every evaluation and proposal, and whether a holder wrote it | judge whether a model got better |

So the exclusivity window — the thing a holder is actually buying when a line declares Early Access
— **is not a promise in this design. It is arithmetic the fold performs.** Everything a developer
does off chain is still theirs to do or fail to do, and §4.6 makes failing to ship visible without
anyone having to file a complaint.

The most common misreading, again: **early access is not secrecy.** A published root is public. What
a holder gets first is the **artifact** — one to thirty-four gigabytes of weights that have never
been on chain — and a place in whatever queue the developer runs. The root is what lets a holder
check that the bytes they were handed are the bytes the line published.

## 4. Decisions

### 4.1 A line declares what its positions grant, and the declaration is on chain

`ModelLineBenefitsDeclared { line_id, tiers, cadence_daa, expires_daa, signature }`, signed by the
line's **owner** — the cold role of ADR-0088 Decision 6, not the developer, because this is what the
line promises rather than what it ships. `tiers` is an ordered list of at most eight
`{ min_units, grants (a bitset), lead_daa, min_hold_daa, note ≤64 bytes }`, strictly increasing in
`min_units`. A declaration replaces the previous one wholly, subject to §4.7's notice; an empty list
withdraws it, also subject to notice. Priced like every registry object (rent, ADR-0088 Decision 11).

### 4.2 The grants are a small closed set, and the set belongs to the chain

A bitset the whole network reads one way, so a wallet renders any line's card without knowing the
line:

| bit | grant | what the holder gets |
|---|---|---|
| 0 | `EARLY_VERSION` | the artifact of a new version, `lead_daa` before the line may make it current — **and §4.4 makes that window a rule** |
| 1 | `PRIVATE_BETA` | versions published as previews are served to holders and to nobody else |
| 2 | `PRIORITY_INFERENCE` | **RETIRED 2026-10-04** — there is no serving queue: every job is self-ordered (see the amendment above) |
| 3 | `EXPERIMENTAL` | modes the line runs but has not made default: longer context, a thinking mode, tools, a new quantisation |
| 4 | `DEVELOPER_ACCESS` | the line's own room — proposals, research previews, where the next version is argued about |
| 5 | `INFERENCE_QUOTA` | **RETIRED 2026-10-04** — no one serves capacity to holders; every job is self-ordered |
| 6 | `HOLDER_VOICE` | evaluations and proposals from holders carry the holder mark and the tier they held at (§4.9) |
| 7 | `SUPPORT` | the line answers holders' reports first |

What a line may NOT declare is anything that pays. There is no bit for a share, a rebate, a discount
in BILI, or a claim on the reserve, and an unknown bit is **refused at the fold**, not stored and
ignored (§6 N5): a promise no reader can render is not a promise. **A grant is a service or it is
not a grant.**

### 4.3 The tier is a pure function of chain state, and both lanes count

`palw_model_benefit_tier_v1(state, line, holder, daa) -> Option<TierIndex>`: the highest tier whose
`min_units` the holder meets **and** whose `min_hold_daa` their tenure satisfies (§4.5). A holder's
balance is the sum of the carrier lane and the EVM lane — ADR-0089 keeps these in separate
namespaces, and a person is not two people. The function lives in `kaspa-consensus-core` so a
gateway, a wallet and the explorer compute the same answer, and it takes the height it is asked
about, so "were they a holder when they asked?" has exactly one meaning.

### 4.4 The lead is a consensus rule, not a promise — **this is the revision's core**

Let `L` be the largest `lead_daa` over the tiers that declare `EARLY_VERSION` in the declaration in
effect (§4.6, §4.7). While `L > 0`:

* **`ModelVersionPromoted` is refused** before `published_daa + L`. The fold returns
  `ModelBenefitLeadNotElapsed { line, version, promotable_at }`. A developer who wants to promote
  sooner must first lower their own declaration, and §4.7 makes that take notice.
* **A version may not be published straight to current.** `ModelVersionPublished` with a
  non-preview status is refused while `L > 0`; the version must ENTER as a preview and be promoted
  later. Without this clause the whole mechanism is bypassable in one field, and the first draft was.

The two clauses together are what a holder is buying: **the window exists whether or not the
developer feels like honouring it.** What the chain still cannot do is put the artifact in the
holder's hands; that stays the developer's act, and §4.6 is what happens when they stop performing
it.

### 4.5 Tenure: a membership rewards not selling

Each `(line, holder)` carries `holding_since_daa`. It is set when a holder's balance rises from
zero, **preserved when they buy more**, and **reset to the current height whenever their balance
falls** — selling any part of a holding restarts the clock. A tier may require
`min_hold_daa`, and the tier function refuses a holder who has not held that long.

This is the anti-flip lever, and it is the honest description of what it measures: not "how long you
have held this many", but **how long since you last sold**. A holder who buys more just before a
release keeps their old clock; that is a deliberate choice, because punishing a holder for
increasing their stake would be perverse.

### 4.6 A promise that stops being kept lapses by itself

A declaration names `cadence_daa` (how often the line undertakes to publish a version) and
`expires_daa`. `palw_model_benefits_in_effect_v1(row, line, daa)` returns nothing — no tiers, no
grants, **and therefore no lead for §4.4 to enforce** — when either

* `daa ≥ expires_daa`, or
* `cadence_daa > 0` and `daa > last_version_published_daa + cadence_daa`.

Nobody submits anything: a line that stops shipping stops granting, on the block that crosses the
line, and the explorer shows `LAPSED` with the height it lapsed at. This is the answer to "what
makes the developer keep delivering" that the chain can actually give — it cannot verify a download,
but it can see that nothing has shipped for three months, and it can stop the line from advertising
a membership it is no longer servicing. A lapsed declaration is revived by declaring again, which
starts §4.7's notice afresh for anything it strengthens.

### 4.7 Taking a benefit away takes notice; giving one takes effect at once

A new declaration is compared with the one in effect. It is **strengthening** if every tier's grants
are a superset, no `min_units` rose, no `min_hold_daa` rose, and `expires_daa` did not fall;
otherwise it is a **weakening**. Strengthening applies at the block it lands in. A weakening — a
removed grant, a raised threshold, a shortened expiry, a withdrawal — is stored as
`pending { tiers, effective_daa = daa + PALW_MODEL_BENEFIT_NOTICE_DAA }` and the OLD declaration
stays in effect until then.

Without this, everything above is theatre: a developer would declare rich benefits, take a holder's
money at the curve, and withdraw the declaration in the next block. The notice is the holder's
protection, and it is the reason a benefit card can be read as something more durable than the
current block.

### 4.8 A holder proves the holding without spending anything

A verifier — the owner's own off-chain service today, a holder-only job class should one exist — needs "this caller holds ≥ N of line L, and has held since H".
`palw_model_benefit_challenge_v1(line, holder, nonce, daa)` is a message the holder signs with the
key their position is held under — the ML-DSA-87 payout key on the carrier lane, the secp256k1
account on the EVM lane. The gateway verifies the signature and reads the tier at `daa`. Nothing is
submitted, nothing is spent, and a holder proving membership does not cost the network a byte.

### 4.9 Holders get a voice, and a path to be paid for work rather than for holding

With `HOLDER_VOICE`, an evaluation (ADR-0088 Decision 5) or a proposal (Decision 7) records
`by_holder_tier: Option<u8>` — the tier its author held at the height it landed. The explorer ranks
holder evaluations first, and a developer looking for what to fix next reads the people who paid to
be there.

This is also the one place a holder can legitimately receive BILI from the line, and it is worth
being precise about why it is not income: ADR-0088 Decision 8 pays the **contributor share** of the
owner's leg to the author of an **adopted proposal** while that version is current. That is payment
for work that was adopted, available to holders and non-holders alike; it is not a return on
holding, it does not scale with units, and no rule here routes anything to a holder because they
hold. The membership buys standing and attention. It never buys a share.

### 4.10 What a participant reads

`getPalwModelLine` gains `benefits` (the tiers in effect, any pending weakening with its height,
the cadence, the expiry, and whether it has lapsed); `getPalwModelBenefitTier(line, holder)` answers
the gateway's question. `misaka palw line-benefits --line --tier …` declares them; `misaka palw
benefits --line [--key-file]` reads them and says which tier a key is in and what the next tier
costs at today's price. The site shows the card on the line page and the trade page — what this
position gets you, what the next tier needs, when the promise expires — because the reason to buy
belongs before the buy.

### 4.11 A fence of its own — the first draft was wrong about this

The draft said "no fence of its own; under `palw_model_lines`". **That is wrong on any chain where
the registry is already live**, which testnet-11 is: this ADR writes two collections that enter the
state root — the declarations (§4.1) and the tenure clocks (§4.5) — so switching it on with the
registry's fence would move the root under a running network the moment the code shipped, and old
and new nodes would disagree about the next block. A consensus change gets an activation.

So: `Params::palw_model_benefits`, `Some` only where `palw_model_lines` is also armed (a membership
over a registry that does not exist is meaningless), scheduled on testnet-11 at
`PALW_RC_MODEL_BENEFITS_FENCE_DAA` and `None` on every other preset and on every mainnet card.
Below the fence a declaration is refused, no tenure clock is written, and §4.4's two refusals never
fire — so a chain that has not armed it keeps exactly the state root it had. The carriage gains a
tail of its own (`0x95`) for the same reason, and both new collections enter the root only when
non-empty.

**A card does not arm it, deliberately.** A membership is a promise a LINE makes, and arming the
mechanism from genesis on a network whose lines do not exist yet would be a default rather than a
decision. The test that compares a card's fences with testnet-11's names this exemption, so it stays
a decision someone took.

## 5. Security — the four principles, checked

* **Nothing is minted, nothing is paid.** No decision here moves a sompi. The closed grant set of
  §4.2 is what stops "just a small rebate" from arriving later as a `tiers` field.
* **The credential cannot be forged.** A tier is a function of state plus a signature over a nonce.
* **The credential cannot be lent.** Positions do not transfer, so a membership cannot be rented;
  a holder may proxy for a friend, which costs them their own place in the queue, and no rule can or
  should prevent that.
* **A declaration is a statement, and the chain never endorses more than it can check.** §4.4 is
  enforced, §4.6 lapses on arithmetic, and everything else is labelled as declared.

| | threat | why it is not one |
|---|---|---|
| A1 | declare rich benefits, sell into the demand, withdraw next block | §4.7: a withdrawal takes notice, and the old declaration governs until then |
| A2 | declare a 30-day lead and promote the next day anyway | §4.4: the fold refuses the promotion |
| A3 | bypass the window by publishing straight to current | §4.4's second clause: while a lead is in effect a version must enter as a preview |
| A4 | declare a lead, then never ship again, keeping the card on the site | §4.6: the declaration lapses at the cadence and grants nothing |
| A5 | buy in the block before a release, take the tier, sell after | §4.5: `min_hold_daa` is the line's own answer, and the tenure clock resets on any sale |
| A6 | a whale buys the top tier and resells access | positions do not transfer; proxying costs them their own slot |
| A7 | tiers used to sell weight, votes or fee shares | the grant set is closed and contains none of them |
| A8 | a gateway is handed a stale balance | the challenge names a height and the gateway reads the balance at it |
| A9 | lower the lead to promote today, using §4.7 as the escape | lowering a lead is a weakening, so it takes notice too |

## 6. Invariants the tests must hold

* **N1 (no payment).** No path added here writes a payout, a position, or a reserve.
* **N2 (the tier is the balance and the clock).** `benefit_tier` is the highest tier whose
  `min_units` and `min_hold_daa` the holder meets at the height asked, and `None` below the first.
* **N3 (both lanes, one person).** 60 units on the carrier lane and 60 on the EVM lane is the
  100-unit tier.
* **N4 (the owner declares).** A declaration signed by the developer or the maintainer is refused.
* **N5 (the set is closed).** An unknown grant bit is refused at the fold.
* **N6 (ordering).** Tiers strictly increase in `min_units`, at most eight, or the object is refused.
* **N7 (the lead is a rule).** Promotion before `published_daa + L` is refused; at exactly
  `published_daa + L` it is accepted — the boundary is pinned on both sides.
* **N8 (no straight-to-current).** While `L > 0`, a non-preview publish is refused.
* **N9 (notice).** A weakening does not change what `in_effect` returns until `effective_daa`; a
  strengthening changes it in its own block.
* **N10 (lapse).** Past the cadence or the expiry, `in_effect` is empty AND §4.4 stops refusing —
  a lapsed promise constrains nobody.
* **N11 (tenure resets on a sale, not on a buy).** Buying more preserves `holding_since_daa`;
  selling one unit resets it to the current height.
* **N12 (the lapse is not a state write).** `in_effect` is a read-time function of the row and the
  line's last publication, so two nodes at the same height agree without anyone submitting anything.

## 7. Order of work

1. This text; the README row; the banner on ADR-0087. **(done)**
2. `palw_model_benefits_v1`: the grant set, the tier row, `tier_for_units`, `in_effect`, the
   strengthening comparison, the challenge message, and their goldens. **(done — 9 tests)**
3. State: the row, the tenure map, the fold arm, the refusals (N4–N6, N9), and §4.4's two
   refusals in the version paths (N7, N8). **(done — 6 tests, and the fence of §4.11)**
4. RPC (`benefits` on the line), CLI (`line-benefits`, and the card on `line-show`). **(done)**
5. The site: the benefits card on the line and trade pages, the next tier's distance, the lapse.
6. A reference membership check — verify the signature, read the tier — for an owner's off-chain service.
   (There is no serving queue to choose: every job is self-ordered, 2026-10-04.)

## 8. What is deliberately not decided

* **A bond behind the promise.** Considered and rejected for now: the only thing a bond could
  secure that the chain can adjudicate is shipping cadence, and §4.6 gets that without escrowing
  anything or inventing a payout path. Anything richer needs the chain to see a download.
* Whether a line may declare benefits before its class is `Active`. Probably yes; nothing turns on
  it until a gateway exists.
* Whether the chain should carry a gateway endpoint for a line. It is a URL, it rots, and the
  registry is not a directory.
* ~~The units of `INFERENCE_QUOTA`.~~ Moot: the grant is retired (2026-10-04) — nothing serves a quota.

## 9. Number hygiene

0095 was the README's next free number on 2026-09-07. The next is 0096.
~~~~~~~~
<!-- ADR_SOURCE_END:0095-a-position-is-a-membership-not-an-income.md -->

</details>

<a id="source-0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves"></a>

<details>
<summary>0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md — 原文 L1–L207</summary>

[原文を開く](../adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) · SHA-256: `c910224a5c686aee35e9856691ab9609266a555325802331f591938274da067a`

<!-- ADR_SOURCE_BEGIN:0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md -->
~~~~~~~~markdown
# ADR-0101 — A membership is proven by the chain and served by anyone, and a Position never moves between holders

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


* Status: PROPOSED 2026-09-10. **Decisions 2, 3 and 5 IMPLEMENTED the same day, consensus-inert**
  (§9): which grant needs whom, the signed service descriptor and the check a client runs
  against chain facts, and three pins that a Position moves only between a holder and the
  curve. Decisions 4 and 7 are stated, not built. Nothing moves on any preset: no rule verifies a
  descriptor, no fold reads one, no fence exists.
* **ADR-0144 alignment (2026-09-21).** Unimplemented D4 / D6 / D7 steps 2–4 (anyone-serves
  discovery and public serving) are withdrawn as PALW-reward work. See the end section.
* **2026-10-04 (the operator):** `PRIORITY_INFERENCE` and `INFERENCE_QUOTA` are retired (ADR-0095's amendment):
  every job is self-ordered and nobody serves inference to holders, so Decision 2's "any provider" row no longer
  includes them; a descriptor offering them describes nothing MISAKA has.
* Builds on: [0095](0095-a-position-is-a-membership-not-an-income.md) (a Position is a
  membership: the line declares, the chain computes the tier, a gateway reads it; §8's "a bond
  behind the promise" rejected; the reference check of §7 step 6),
  [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) (bought from the curve,
  sold back to it, no transfer), [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
  (a line's owner, developer and maintainer; anyone founds a competing line),
  [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) (the EVM is the
  fold's window and hand, never a second market), [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  (use reaches the curve; no holder is paid), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  Decision 6 (the chain carries kilobytes and no URL), [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the boundary "permissionless" means, of which this is the membership half).
* Amends: nothing. Supersedes nothing. Records, as a decision, what the operator settled on
  2026-09-10: **a Position has no payment or settlement role, and therefore no transfer.**

## 0. The sentence this ADR is

**The line controls the PRODUCT, providers control the SERVING, and the chain proves the
MEMBERSHIP.** Anyone may serve a line's holders; a client checks a provider's signed statement
against what the chain says about the line — the grants it declares, the roots it names, the keys
its own bonds hold — and needs nobody's word; and a Position, bought from the curve and sold back
to it, never moves from one holder to another and never pays for anything, here or anywhere a
later decision adds.

## 1. What was read

### 1.1 The permissionless map, and its one gap

A review of `main` and testnet-11's fences (2026-09-10) mapped every path into the PALW economy.
Adopted, with the review's verdicts: a node syncs and verifies without a bond, production needs
an Active bond and nothing else (ADR-0061); a class registers with no allowlist, vote or identity
(ADR-0056); anyone certifies, and the court grades (ADR-0075); the chain draws the attempt
(ADR-0074); a bonded party challenges (ADR-0027) and accuses (ADR-0062, armed on testnet-11 at
DAA 1,900); anyone founds a line, posts a proposal or an evaluation, and a line's versions are
its developer's and its roles its owner's — ownership of a line is not permission over the chain
(ADR-0088); anyone seeds, anyone buys, a holder sells (ADR-0087, ADR-0094); a claim's Final
reward buys the line's pair with nobody's hand on it (ADR-0091); the tier is the chain's and the
early-access window is the fold's (ADR-0095).

Two corrections. **The reference gateway check exists** — `misaka/palw/gateway/src/membership.rs`
(`773f6e7c`) on `feat/adr-0095-membership-serving`, ADR-0095 §7 step 6 "done 2026-09-10" — and
is not on `main` (`a60f78e7` is not an ancestor of `main`'s tip), which is why a review of `main`
saw it missing. And **it is already provider-neutral**: its configuration is a line id and a
network domain, it reads the chain's tier, and it asks the line's owner nothing — any operator
can run it for any line today. What is missing is the rest of "anyone serves": a statement of
WHICH grant a stranger can serve, a provider's statement a client can CHECK, and a way to FIND
providers that does not put a URL on the chain.

### 1.2 A Position's movement, read off the fold

| path | what it does to a holding | where |
|---|---|---|
| buy | credits the buyer, from the curve | `model_buy_v1`, the ONE of two writers |
| sell | debits the seller, to the curve | `model_sell_v1`, the other |
| the reward's buyback (ADR-0091) | retires units into the market row's `retired_units`; credits no holder | `model_buyback_at_final` |
| a line's owner transfer (ADR-0088) | moves the LINE's ownership; writes no holding | `ModelLineOwnerTransferred` |
| the EVM facade (ADR-0089) | `transfer`, `transferFrom`, `approve`, `allowance` answer `NonTransferable()` | `misaka/evm/core/src/model_market.rs` |
| the CLI | `palw model-buy`, `model-sell`, their EVM twins, `model-seed`; no transfer command | `misaka-cli` |

No object names two holders. That is the settled rule, and §5 pins it three ways.

## 2. The requirement

> **R-serve — anyone may serve a line's holders, and a client can check it.** Which grant a
> provider may serve is a function of the grant, not of anyone's approval; a provider's offer is
> a signed statement checked against the chain; the chain holds no provider and no URL; and no
> path of the serving side moves, escrows or is paid in a Position.

## 3. Decisions

**Decision 1 — the product, the serving and the membership are three parties' business.** The
line declares what each tier grants (ADR-0095 §4); the chain computes a holder's tier at the tip
and enforces what it can see (the early-access window); a provider serves. A membership whose only
server is the line's own machine dies with that machine; one any provider can serve does not.

**Decision 2 — which grant needs whom** (`palw_benefit_server_v1`, one spelling for every client
and provider):

| grant | needs | why |
|---|---|---|
| `HOLDER_VOICE` | the chain | the fold writes the mark (ADR-0095 §4.9) |
| `PRIORITY_INFERENCE`, `INFERENCE_QUOTA`, `EXPERIMENTAL`, `PRIVATE_BETA`, `EARLY_VERSION` | any provider holding bytes under a root the chain names for the line | the class's artifact or a version's; a client checks the root, and nobody's permission is involved |
| `DEVELOPER_ACCESS`, `SUPPORT` | the line's origin — a key of its owner, developer or maintainer bond | a relation with the line's own people, which a stranger's server cannot provide |

A bit this build does not know needs nobody it can name, and a descriptor offering it is refused.

**Decision 3 — the signed service descriptor, and the check a client runs.**
`PalwServiceDescriptorV1 { version, line_id, grants, roots, endpoints, valid_from_daa,
expires_daa, provider_pubkey, signature }`, its id keyed over the network domain and every field
but the signature, signed under `misaka-palw/service-descriptor/mldsa87/v1` — a context in no
committed set, on purpose: no consensus rule verifies it. `palw_service_descriptor_check_v1`
takes what the chain says about the line (`PalwLineServiceFactsV1`: the grants declared IN EFFECT
now — a lapsed promise offers nothing to serve — the class's and the versions' roots, the line's
bond keys, the tip's DAA) and refuses by name: another line, a window that does not hold, no
grant, an unknown bit, a grant the line does not declare, the chain's own grant, a served-by-root
grant with no root, a root the chain does not name for the line, an origin grant under a
stranger's key, a signature that does not verify. The verdict says `Origin` or `Open`.

**Decision 4 — discovery is not chain state (withdrawn).** 公開 descriptor・mirror・directory の発見基盤は PALW work として実装しない。第三者 inference marketplace は [ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P1 の対象外だからである。

**Decision 5 — a Position is a membership and never money** (settled by the operator,
2026-09-10). A Position is bought from the curve and sold back to it; it does not move from one
holder to another; it is never a means of payment or settlement — on the carrier lane, in the
EVM facade, and in any serving layer this ADR admits. A provider READS a holding through the
proof the holder signs (ADR-0095 §4.8's challenge) and receives nothing from it; a descriptor has
no price in Positions, no escrow, no holder to credit; no benefit may be priced, paid or
collateralised in Positions. The three pins of §5 fail on a third writer of a holding, on an
object that moves a Position between holders, and on a facade that answers `transfer`. A proposal
to the contrary is a new ADR that supersedes this decision by name, never an edit to a pin.

**Decision 6 — serving stays unadjudicated (withdrawn).** marketplace の serving・執行モデルは実装しない。ADR-0144 P1 のローカル推論という目的から外れるため。

**Decision 7 — order of work.** 公開 serving 用 gateway・site・directory（旧 steps 2–4）は実装しない。Decision 4/6 と同じ理由による。実装済み membership check と holder mark は ADR-0095 に従う。

## 4. What this costs

Nothing on chain. One module of pure functions and types in `consensus-core`; three pins.

## 5. Invariants the tests hold

```
1  Every known grant names who serves it; an unknown bit names nobody.
2  A stranger serves by root, the origin serves its own, and every refusal of Decision 3 is by
   name.
3  A real ML-DSA-87 signature verifies; a field changed after signing, or another network, does
   not; a stranger's key cannot claim an origin grant, signed or not.
4  A descriptor survives its JSON transport with its id.
5  A holding has exactly two writers — the buy and the sell — and no object moves a Position
   between holders (the one "Transferred" object moves a line's ownership).
6  The EVM facade answers transfer, transferFrom, approve and allowance with NonTransferable(),
   whatever the arguments.
```

1–2 are `palw_service_descriptor_v1`'s; 3 `misaka/palw/sdk/tests/service_descriptor_signed.rs`;
4–5 `consensus/core/tests/palw_adr0101_membership.rs`; 6 `kaspa-evm`'s
`a_position_has_no_transfer_door_on_the_evm`.

## 6. Order of work

Decisions 2, 3 and 5's pins — **done** (§9); the rest is Decision 7's list.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0095 §7 step 6 (a reference gateway check) | kept; it is the membership half a provider runs, and it is provider-neutral already |
| ADR-0095 §8 (no chain URL; no bond behind the promise) | kept; Decisions 4 and 6 |
| ADR-0087 (no transfer) / ADR-0089 (the facade refuses `transfer`) | kept; named as Decision 5 and pinned |
| ADR-0100 Decision 6 (the permissionless boundary) | this is its membership half |

## 8. What is deliberately not decided

* **The directory's transport** (Decision 4).
* **The units of `INFERENCE_QUOTA`** — ADR-0095 §8: a serving decision is not a consensus one.
* **Whether an origin may delegate its grants to a named provider.** A line that wants that
  declares the provider's key as a maintainer (ADR-0088 roles) — the mechanism exists; a
  dedicated delegation object would be a new ADR.

## 9. Number hygiene and implementation record

0101 is the next free number after ADR-0100. Claimed on `feat/adr-0099-sharded-seat` on
2026-09-10. **The next free number is 0102.**

* **2026-09-10** — written and implemented the same day:
  * `consensus/core/src/palw_service_descriptor_v1.rs` — Decisions 2 and 3, with their tests;
    registered in `lib.rs`, its domains in `palw_derived_v1`'s uniqueness sweep.
  * `consensus/core/tests/palw_adr0101_membership.rs` — Invariants 4 and 5.
  * `misaka/palw/sdk/tests/service_descriptor_signed.rs` — Invariant 3, with real keys.
  * `misaka/evm/core/src/model_market.rs` — Invariant 6.

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P1: third-party
inference serving is a non-goal. R-serve ("anyone may serve a line's holders") is a GPU marketplace
in all but name.

* **Kept.** Decisions 2, 3 and 5 as already implemented: which grant needs whom, the signed
  descriptor a client can check against chain facts, and a Position that never moves and never
  pays. Those pins do not require anyone to serve inference.
* **Withdrawn as unimplemented PALW work.** Decision 4 (discovery is not chain state — well-known
  path, mirrors, a directory). Decision 6 (serving stays unadjudicated — the marketplace's
  enforcement model). Decision 7 steps 2–4 (the gateway serves a public descriptor, the site
  renders checked descriptors, a directory of any transport). Step 1 (merge the 0095 membership
  check) and step 5 (the holder mark at its own activation) are membership, not serving, and stay
  with ADR-0095.
* A later product that wants a marketplace is a new ADR that supersedes this alignment by name,
  and it is not paid in PALW.
~~~~~~~~
<!-- ADR_SOURCE_END:0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md -->

</details>

