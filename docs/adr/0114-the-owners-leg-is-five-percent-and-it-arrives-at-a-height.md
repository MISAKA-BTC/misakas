# ADR-0114 — 通常AMMのburn 5%・owner総leg 3%と版付き適用

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTのモデル恒久廃止で、当該永久LP内actual MISAKA・登録者責任bond・MINING用途bond・未確定mining報酬を100%burn/失効させる。**全Panel/Gate C検証用途bondは、不正者も含め一律にBFT廃止burnから保護**し、個人不正はCourtの個別slashへ分離する。正当に成立した検証報酬の支払を維持し、他モデル資産へ影響させない。兼任は用途別lot（MINING6,000だけburn、PANEL4,000は保護）とし、重複担保・責任中の用途変更は禁止する。

通常のLP撤去禁止を維持し、canonical廃止のactual reserve burnだけを限定例外にする。BFTの没収受取・廃止bountyは0。継続5%報酬を没収額に連動させない。可逆Pause/Resumeと区別し、旧停止専用・LP絶対不処分の記述をこのscopeだけ置換する。旧profile/native/履歴を保持し、BMR1実装・移行・有効化は別工程とする。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — 全モデル統一・小数POSITION・50/20/20/5/5

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](../governance/fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

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
