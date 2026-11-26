# ADR-0114 — 通常AMMのburn 5%・owner総leg 3%と版付き適用

- 改訂日: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。MOE2の設計改訂であり、native/EVM実装、測定、移行又は公開有効化の完了ではない。
- 共通正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md) / [ADR0232](0232-reward-buy-and-permanent-liquidity.md) / [RFC-v2-0011](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md)。
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

## 1. 対象と正本

MOE2の追加モデル登録は、そのmodelの一意市場と初期reserve・在庫・owner参照を原子的に同時生成する。別の市場準備・外部seedを必要とせず、core登録では市場を生成しない。登録済み追加モデルの市場欠落はstate invariant違反として拒否する。

MOE2の通常protocol AMM売買は、ADDITIONALモデルだけで有効である。GENESIS/FORK_DESIGNATEDに市場や売買feeを設定しない。coreにownerが存在しないからowner legを追加burnする、という旧処理を新profileへ残さない。

手数料は売買の実行時に適用される認証済み経済policyとmodel分類から決める。RPC quote、CLI、EVM view、native実行は同じpolicy ID・state anchor・計算器を使う。未確定又は異なるpolicyによるquoteを約定保証として扱わない。

## 2. 決定

### Decision 1 — 50/30/残差

gross BILI額Gに対し、最小整数単位で次を計算する。

\[
\mathrm{burn}=\lfloor 50G/1000\rfloor,\qquad
\mathrm{owner\_total}=\lfloor 30G/1000\rfloor,\qquad
\mathrm{net}=G-\mathrm{burn}-\mathrm{owner\_total}.
\]

`burnPermille=50`、`ownerPermille=30`、名目`netPermille=920`である。整数残差はnetへ帰属するため、netを別途`floor(920G/1000)`として差分を失わない。wide checked multiplication、amount上限、負値・overflow・未知asset拒否を必要とする。

buyではnetをreserveへ投入し、sellではcurveのgrossから控除後のnetを売却者へ払う。正常売買は同一原子的遷移でreserve、burn、owner総leg、Position、支払権利を更新する。失敗したquote/minOut/署名/資金不足からfeeだけを残さない。

### Decision 2 — Ownerとcontributorの合計は3%

owner ruleが認めるcontributor分割はowner_totalの内部で行う。owner残額とcontributor支払の合計はowner_totalに等しく、3%を超えて請求しない。所有・contributor関係は認証された固定modelと実行時に有効なruleから解決する。

追加モデルの正規owner/payee規則が欠落・無効な場合は売買を拒否する。欠落を理由にowner分をさらにburn、Minerへ送付、別model ownerへ送付するfallbackを設けない。支払責任とowner変更の通知・版境界も認証する。

### Decision 3 — 適用しない操作

| 操作 | AMM burn | owner総leg |
| --- | ---: | ---: |
| 通常のprotocol AMM buy/sell | 5% | 3% |
| protocolによる報酬購入B（Rの30%） | 0% | 0% |
| protocolによる永久LP拠出A（Rの20%）＋ell | 0% | 0% |
| 直接transfer/受取/譲渡 | 0% | 0% |
| approve/transferFromの転送処理 | 0% | 0% |
| GENESIS/FORK_DESIGNATED | 市場自体を無効 | 市場自体を無効 |

network fee/gas、明示的なモデル停止・解除burn、その他署名済み役務契約の費用は別の原資と規則である。それらを通常AMM手数料へ合算したり、無断で無効化したりしない。

MOE2ではB全額とA全額を実reserveへ一度ずつ入れ、購入qのうちellをLPへ戻し、余剰q−ellだけをPosition burnとする。Positionの余剰burnをBILIの追加5% burnと混ぜない。LP receiptにowner/contributorのfee請求権を設けない。`LiquidityDust`では購入・LP・burn・報酬支払を部分commitせず、原期限の有限保留を使う。

### Decision 4 — 旧率を遡及変更しない

旧V1=50/10、旧V2=50/50は、その版で受理・実行した取引の再生用に保持する。新率はMOE2の新policy/activationへ割り当て、既存の`leg_v2`定数を置換してgenesisから再計算しない。旧receipt、旧nonce、旧署名の意味を同じ名前で変更しない。

新旧境界でどの実行がどのpolicyを使うか、pending AMM requestのdrain、minOut・返金・owner snapshot、undo/restart/IBDを全consumerで同一にする。wire値・activation高さは、このADRが実装済みとして割り当てるものではない。

## 3. 金額例と端数

| gross G | burn | owner_total | net |
| ---: | ---: | ---: | ---: |
| 1,000 BILI | 50 BILI | 30 BILI | 920 BILI |
| 1最小単位 | 0 | 0 | 1 |
| 999最小単位 | 49 | 29 | 921 |

曲線価格・Positionの整数丸め・slippage・gasは別である。往復取引を単純に「16%」と断言せず、各取引時のgross・netと実際のcurveを計算する。

## 4. 受入と完成条件

buyとsellの両方、owner/contributor分割、極小値、最大値、minOut、invalid payee、直接送金の無課税、報酬購入/永久LP両legの追加税なし、receiptのfee請求権不存在、dust無副作用、core全入口拒否、旧新boundary、queue/settlement、reorgと二重適用を検査する。旧の5% owner実装・実測・テストは保存原本の履歴であり、新率対応の証拠ではない。
