# ADR-v2-0010 — Genesis発行上限・bondゼロ起動

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0013](../rfc-v2/0013-permissionless-bootstrap-and-release.md)

## 1. この文書の役割

genesis発行の上限と、初期の特権bondを置かない公開bootstrapをまとめる。

## 2. 統合した判断の読み方

### 2.1

genesisのpremine総量と内訳を区別し、複数のcarveを足して総量を増やさない判断を保存する。[旧ADR-0059](../adr/0059-the-10b-premine-cap.md)

### 2.2

旧zero-seat genesisの選択肢を経て、新genesisの事前登録bondゼロを採択した経緯を残す。genesis UTXOそのものの廃止ではない。[旧ADR-0061](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) [旧ADR-0209](../adr/0209-genesis-bond-free-bootstrap.md)

### 2.3

BASE回復で得た資金またはgenesis UTXOを通常transactionでbondへ変え、通常の成熟・担保・検証条件を経てPALWへ参加する。運営者にも自動的な特権席を付けない。[旧ADR-0209](../adr/0209-genesis-bond-free-bootstrap.md)

## 3. 維持する境界・後続改定

約30分は元資料にある起動方針の記述で、実ネットワークの成功を示す測定値ではない。旧chainの履歴を書き換える決定とは区別する。

## 4. 隣接する判断との分担

liveness laneは[ADR-v2-0009](0009-heartbeat-floor-and-liveness.md)、署名とUTXO支出は[ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md)・[ADR-v2-0003](0003-utxo-accumulator-and-commitment.md)、配備は[ADR-v2-0011](0011-release-activation-and-flag-days.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0059-the-10b-premine-cap.md](../adr/0059-the-10b-premine-cap.md) | L1–L101 | L14–L19 |
| [0061-zero-seat-genesis-and-right-sized-collateral.md](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) | L1–L128 | L17–L23 |
| [0209-genesis-bond-free-bootstrap.md](../adr/0209-genesis-bond-free-bootstrap.md) | L1–L17 | L3–L3 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0059-the-10b-premine-cap"></a>

<details>
<summary>0059-the-10b-premine-cap.md — 原文 L1–L101</summary>

[原文を開く](../adr/0059-the-10b-premine-cap.md) · SHA-256: `f56190bd0770fbfde28134fdc8bee5e21884cf311750eabee4e9500053fe05dc`

<!-- ADR_SOURCE_BEGIN:0059-the-10b-premine-cap.md -->
~~~~~~~~markdown
# ADR-0059: The 10B premine cap — genesis mints one number, everything else is a carve

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

- Status: Accepted
- Date: 2026-08-30
- Depends on: ADR-0042 (the RC atomic bundle and its genesis registry), audit C-08 (a bond's
  collateral is money the genesis UTXO set really holds)
- Supersedes: the per-network premine split of 2026-08-26 (mainnet 13B / re-minted test nets
  10B) and the 2026-06-17 vault layout (40 × 0.1B custody UTXOs + one main wallet)

## The operator's decision, verbatim

Recorded 2026-08-30, and this ADR is its normative form:

1. **Every network's genesis premine is exactly 10,000,000,000 MSK.** Mainnet 10B, every
   testnet 10B. The constant is `config::premine::MISAKA_PREMINE_CAP_SOMPI`, and it is one
   number for every network by design — the 2026-08-26 split (13B mainnet / 10B testnets)
   existed only because mainnet's identity was not supposed to move; the operator has now
   moved it, and the split is gone.
2. **The vault block is deleted.** The 40 mainnet-custody vault UTXOs (4B MSK, indices 0–39)
   are removed from every network's genesis. A genesis is one main-wallet UTXO, plus — on the
   network that runs a `ConsensusV2` registry — the carve-outs below.
3. **Community allocations are carved out of the main wallet, never minted beside it.** Adding
   an entrant to `TESTNET11_COMMUNITY_ALLOCATIONS` reduces the main wallet by the same amount;
   the genesis total does not move. The same rule binds every other genesis extra: bond
   collateral and fee floats are carves too.
4. **The genesis bonds bond the main wallet.** Each genesis bond's collateral output (0.1B,
   at the bond's own outpoint index `0..cards`) is owned by the main wallet's key — the
   operator stakes the main wallet's money. There is no separate custody block to stake from.

## Why the defect this closes was invisible

The 2026-08-26 "10B" decision reduced the test networks' main wallet to 6B — and testnet-11
still minted 13.547B. The float-carve function inserted the main wallet a second time at
`9B − floats`, and `UtxoCollection` is a `HashMap`, so the second insert silently replaced the
first. Worse, the tests PINNED the wrong total ("the RC's premine is the 13B split plus the
community allocation"), so the suite was green while the decision was not implemented. Two
lessons are structural now:

* **One construction path.** `testnet11_genesis_utxos` builds the whole set in one pass; the
  main wallet is written exactly once, as `cap − Σcarves`.
* **The cap is enforced arithmetically, not by review.** The builder computes the main wallet
  with `checked_sub` against the cap — a carve list that outgrows 10B fails the build loudly —
  and `every_network_genesis_mints_exactly_the_10b_cap` asserts the sum on every network, so
  a future "grow the table" change cannot pass without the main wallet paying for it.

## What deliberately does not move

* **Outpoint indices.** Collateral at `0..cards`, the main wallet at index 40
  (`MAIN_PREMINE_INDEX`), floats at `41..41+cards`, the community set on its own sentinel
  txid. A bond's identity IS its collateral outpoint (`PalwBondKeyV2(premine_outpoint(i))`,
  inside `palw_ruleset_id`), and fleet units name float outpoints in their configs — indices
  are names, not positions, and the gap at 6..39 is deliberate.
* **The collateral value.** 0.1B per seat at this ADR's cut — *superseded the same day by
  ADR-0061*, which re-sizes the outputs to 10,000 BILI per seat (the declared collateral stays
  the derived figure, so `palw_ruleset_id` still does not move).
* **The emission schedule.** 15B over 20 years, untouched. Final supply follows the cap:
  10B + 15B = **25B** (`MAX_SOMPI` 28B → 25B, the same follow-the-premine move as the
  30B → 28B re-derivation of 2026-06-17).
* **The community table.** All 11 entrants, 547M BILI, same addresses, same order — now paid
  for by the main wallet (t11 main = 10B − 0.6B collateral − 600 floats − 547M community
  = 8,852,999,400 BILI).

## Safety of "the main wallet bonds"

Collateral at the operator's spending address is safe on both layers: consensus refuses any
transaction spending a locked bond outpoint (`palw_bond_collateral_is_locked_v2`, enforced on
the block path via `palw_v2_locked_bonds`), and the wallet's input selector excludes locked
collateral via the `palw_locked_bond_outpoints_v2` RPC — the same pair that closed the
fee-outpoint variant of this trap (audit3 H12).

## Consequences

* **Genesis identity moves on every network.** For testnet-11 this is Relaunch 3 (coinbase
  marker `11, 3`): a full re-mint — stop the whole fleet FIRST, wipe every datadir, redeploy
  (a peer wiped in-place while others run will be re-fed the old chain via IBD). Un-wiped
  peers are refused at the handshake. Mainnet has not launched, so its move is free.
* **This is a deliberate exception to the genesis-fixation policy** (2026-08-27: consensus
  changes ship by activation, never re-genesis). A supply-shape change is the one thing only
  a genesis can express, and the operator ordered it explicitly. The policy stands for
  everything else.
* The 40 custody addresses are out of consensus entirely. If mainnet wants custody splitting
  at launch, that is an operational wallet decision (spend the main wallet into N outputs),
  not a genesis structure.

## The rule for the next person who edits the genesis

Add community entrants by APPENDING to `TESTNET11_COMMUNITY_ALLOCATIONS` (the outpoint index
is the table position). Do not touch the cap. The main wallet pays; the build fails if it
cannot. If someone asks for a bigger genesis, the answer is this ADR and the operator, in
that order.
~~~~~~~~
<!-- ADR_SOURCE_END:0059-the-10b-premine-cap.md -->

</details>

<a id="source-0061-zero-seat-genesis-and-right-sized-collateral"></a>

<details>
<summary>0061-zero-seat-genesis-and-right-sized-collateral.md — 原文 L1–L128</summary>

[原文を開く](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) · SHA-256: `8a4c90ffd9f2cba1e4076a8794a65598cc50d098cbb3de1b13bc4c5d482b35b9`

<!-- ADR_SOURCE_BEGIN:0061-zero-seat-genesis-and-right-sized-collateral.md -->
~~~~~~~~markdown
# ADR-0061: Zero-seat genesis, and collateral sized by arithmetic instead of history

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


- Status: Accepted; implemented 2026-08-30 (`palw-genesis-10b-cap` branch, same re-mint train
  as ADR-0059/0060)
- Depends on: ADR-0060 (the heartbeat lane is what makes both decisions safe), the
  post-genesis bond carrier ("a stranger can register their own bond"), ADR-0059 (the 10B cap
  the collateral re-size returns money under)
- Supersedes: the born-licensable rule (`PanelCannotBeSeated`) of the RC genesis gate, and the
  0.1B-per-seat genesis collateral both ADR-0059 and ADR-0060 §8 carried forward

> **Amended (index reconciliation, 2026-09-02).** The audit amendment below ("the capability is
> real and its bootstrap is not yet") was answered by [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
> and [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md): the heartbeat lane is back
> as `algo_id = 8`, armed from genesis on testnet-11 and devnet, and the Phase 1 drill proved the
> bootstrap this ADR draws — a zero-bond devnet born over heartbeats, a bond registered on it, the
> bonded lane starting after. "testnet-11 still ships its six seats" is Relaunch 3: since Relaunch 4
> the RC registry holds eight cards so that [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)
> Decision 1 can be armed (`seat_count + 3`); devnet ships six public-seed bonds
> ([ADR-0075](0075-certification-is-a-consensus-object.md) §7's rehearsal chain). Decision 2's
> 10,000 BILI per seat stands. Map: [`README.md`](README.md).

## The two decisions

**1. A genesis may seat zero bonds.** `verify_palw_genesis_v2` no longer refuses a registry
smaller than `seat_count + 1` distinct operators — down to and including the empty registry.
The refusal's stated premise was "`BondRegistered` may not ride a transaction … a registry too
small has no later repair", and both halves are dead: bonds register on the running chain as
ordinary transactions, and the heartbeat lane (ADR-0060 D1) produces the blocks such a
registration rides even when no bonded producer exists. The bootstrap of a zero-seat network
is therefore fully permissionless:

```
genesis (0 bonds) → heartbeat blocks (bondless, fee-only)
                  → bond registrations ride them
                  → bonded producers light the PALW lanes
                  → sixth distinct operator arrives → licensing begins
```

**Audit amendment (2026-08-30, same day).** The bootstrap sentence above depends on ADR-0060
Decision 1, which the mainnet audit shipped OFF (see ADR-0060 §12). The GATE change stands — a
zero-seat registry is a valid genesis, and `an_empty_bond_registry_still_yields_a_consensus_v2_network`
pins that it stays a ConsensusV2 network rather than silently degrading to a hash chain, which is
a defect this ADR introduced and the audit caught. What waits is the first BLOCK: until the
heartbeat lane is redesigned, a zero-seat network has no permissionless producer, so the
capability is real and its bootstrap is not yet.

Until the sixth operator, claims void at `BindTimeout` and their escrow burns — LOUDLY (the
runtime warns per voided block), which is what distinguishes today's bootstrap phase from the
silent-forever failure the old gate was written against. The gate keeps every other check:
collateral coverage (C-08), the bind-window sustain rule, the catalog root, the class list.

**2. Genesis collateral is 10,000 BILI per seat** (was 0.1B — the old vault denomination, kept
"because it was there"). The binding constraint is the DERIVED requirement —
`palw_v2_collateral_for_claim_lifetime_v1` over the dearest registered class, measured at
**3,223.07 BILI** on the shipped three-class card — and the C-08 gate only demands the output
COVER the declaration. 10,000 is a ~3.1× margin over that structural minimum; the margin
absorbs DAA advancing slower than one per block (parallel production against one bond), while
the derivation itself already covers the whole claim-lifetime exposure horizon, `+1` included.

Consequences of the re-size, under the ADR-0059 cap arithmetic (the main wallet pays for every
carve, so the cap never moves):

| | before | after |
|---|---:|---:|
| collateral per seat | 100,000,000 BILI | 10,000 BILI |
| locked across 6 seats | 600,000,000 BILI | 60,000 BILI |
| t11 main wallet (spendable) | 8,852,999,400 BILI | 9,452,939,400 BILI |
| genesis total | 10B exactly | 10B exactly |

A slash of one seat now burns at most 10,000 BILI of operator money instead of 100M — the
penalty finally matches the protocol's own accounting instead of a historical denomination.

## What deliberately does not move

* **The declared collateral** in every `BondRegistered` (the derived 3,223.07 BILI) — so
  `palw_ruleset_id` is byte-identical. What moves is the genesis UTXO set, its commitment, the
  t11 genesis hash and the t11 fingerprint (`17bdff18…`), all riding the re-mint already in
  progress. No other preset's fingerprint moves.
* **The seating arithmetic.** `derive_panel_v2` still needs `seat_count + 1` distinct
  operators to license; zero-seat changes when they arrive, not how many are needed.
* **The shipped t11 card.** testnet-11 still ships its six seats — zero-seat is a capability
  (the mint tool now assembles any registry size, including empty), exercised by the next
  network that wants it, mainnet included.
* **No version bump.** The gate change moves no consensus bytes on any running network: two
  builds on a seated network behave identically, and an old build meeting a zero-seat genesis
  refuses to assemble it at boot — loud, fail-safe, and impossible to mistake for a fork.

## Why this closes ADR-0060 §8

Both items were listed there as "deliberately not decided", pending exactly the operator
decision this ADR records. With them decided, a genesis is at last only what a genesis must
be: the supply (one 10B main wallet under ADR-0059), the community's allocations, and — where
the operator wants a running start — a registry it could equally have grown on-chain.


## Audit note on the collateral figure

The 10,000 BILI carve did not, as first written, buy a ≈3× runtime margin: every exposure ceiling
reads the DECLARED collateral, and that was pinned to the derived structural minimum, so the
surplus in the outpoint bought exactly one extra concurrent claim. The declaration is now
`max(derived, held)` and the margin is real. Two consequences follow and are deliberate: the
derived minimum itself rose (the redraw's extra bind+receipt pair belongs in
`MAX_CLAIM_EXPOSURE_DAA`), and the carve is now a CEILING on the dearest class a genesis may
register — 10,000 BILI admits `pwu_per_inference ≤ ~8.3M` against Qwen3.6's 2.69M, a 3.1× headroom,
and exceeding it aborts every binary inside `Params::from`. Raising the carve is a supply decision
under ADR-0059's cap.

## Mission alignment amendment — 2026-10-07

zero-seat genesisはpreset genesis seatsを置かない意味であり、固定Panelの検証を廃止するRFC15のPanel=0ではない。後者はRFC14の全gateとRFC15固有gateが成立するまで有効化しない。

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0061-zero-seat-genesis-and-right-sized-collateral.md -->

</details>

<a id="source-0209-genesis-bond-free-bootstrap"></a>

<details>
<summary>0209-genesis-bond-free-bootstrap.md — 原文 L1–L17</summary>

[原文を開く](../adr/0209-genesis-bond-free-bootstrap.md) · SHA-256: `17d64fc16ff93a4bb80336c6ef3274a78b403ee06c7d79b5cca0df0c93def867`

<!-- ADR_SOURCE_BEGIN:0209-genesis-bond-free-bootstrap.md -->
~~~~~~~~markdown
# ADR-0209 — genesis bond を廃止し、BASE 回復と通常 bond 登録で起動する

Status: **Accepted policy / 実装・検証未完**（2026-10-28、ユーザー決定）。仕様は [RFC-0041](../rfc/0041-genesis-bond-free-bootstrap.md)。

## 判断

新 genesis は事前登録 bond をゼロにする。後から取得できない genesis bond という地位を廃止し、運営者を含む全員が通常の公開 bond 登録・成熟・担保条件を使う。

初期は PALW ブロックがない状態で約30分待ち、既存の BASE 回復規則を利用する。その後、BASE で得た資金 **または genesis に含まれる UTXO** を通常のトランザクションで bond にし、必要な資格を満たして PALW を開始する。genesis UTXO 自体は廃止しない。所有者を自動で bond 保有者・検証席・特権 anchor にする仕組みも置かない。

[ADR-0061](0061-zero-seat-genesis-and-right-sized-collateral.md) の任意の zero-seat genesis を、新しい起動方針では必須とする。既存の報酬総量、通常の支出・成熟条件、claim 認証・court の省略を認める決定ではない。

## 根拠と状態

ユーザーは、後から参加できない役割を残すことは不平等で permissionless ではないと判断し、この方針を明示した。再採択の質問は不要である。

約30分の待機起点、無 bond BASE の実受付と報酬、UTXO 支出から bond・PALW までの接続は実装で確認する。既存 heartbeat と bonded BASE の違いを無視して完了と扱わない。稼働済み chain の履歴を変更せず、新 genesis と既存ネットワークの特権除去を区別する。
~~~~~~~~
<!-- ADR_SOURCE_END:0209-genesis-bond-free-bootstrap.md -->

</details>

