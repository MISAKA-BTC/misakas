# ADR-0101 — chain認証membershipとPositionの直接送受信

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
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

## 1. 責務の分離

MOE2の追加モデル登録は一意のMisakaOption市場を原子的に同時生成する。別のmarket-ready待機や外部seedを要求せず、core登録からは市場を生成しない。登録済み追加モデルの市場欠落を正常状態へ扱わない。

chainは固定モデル、現在のPosition残高、適用中のbenefit宣言、tier、署名・状態anchorを認証する。外部サービスは必要なmembershipを検査して提供する。Position transferはnative/EVMの資産遷移であり、外部サービスの支払や履行確認を推定する処理ではない。

MOE2対象はADDITIONALのみ。GENESIS/FORK_DESIGNATEDのMisakaOption・Position・市場・転送・承認を無効とする。モデル自体のPALW実行・Panel検証・Courtは維持し、coreの対象worker報酬RはMinerへ100%帰属する。

## 2. 現行決定

### Decision 1 — Membershipは現在保有に結合する

Positionを取得したrecipientは、新しいcanonical残高と[ADR0095](0095-a-position-is-a-membership-not-an-income.md)のtier・保有期間条件で資格を得る。senderは減少した残高で再判定される。過去のholder mark、署名責任、claim liability、bond、モデルowner権は移さない。

### Decision 2 — 認証済みの実行・サービスcontextを確認する

署名済みdescriptorを使う機能では、model ID/artifact root、grant、provider identity、認証domain、state anchor、期限を照合する。unknown/expired/別モデル/未宣言grantを拒否する。現在保有を必要とする操作へ古い保有証拠をそのまま再利用しない。

外部descriptorやmirrorの発見をPALW work、model登録審査、claim Finalの新しい投票へしない。従来撤回されたinference marketplaceやpriority/quotaの機能は復活させない。

### Decision 3 — 直接transfer・受取・譲渡を許可する

旧Decision 5の「Positionはholder間を移動しない」を、MOE2について明示的に置換する。正しい送信者署名又は承認されたspender権限により、同じモデルのavailable Positionを直接送受信・譲渡できる。AMMでの売り直しを条件にしない。

直接送金にAMM burn 5%・owner 3%を課さず、通常のnetwork fee/gasのみを適用する。第三者が別途対価を支払う相対取引と無償譲渡を合意が自動的に区別するとはしない。Position移転によってBILIの法定又はprotocol決済単位を変更するわけではない。

### Decision 4 — EVMのtransfer・承認を実装する

旧`NonTransferable()`の一律拒否と「holdingのwriterはbuy/sellだけ」という受入条件を廃止する。`transfer`、`approve`、`allowance`、`transferFrom`及び対応eventは[ADR0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)の同一tx journal契約を満たす。`ActionQueued`を転送成功と表示しない。

### Decision 5 — 資産を動かす権限だけを移す

allowanceはrecipient、model登録者、第三者serverへの包括権限ではない。spender/model/network/ownerごとにscopeを固定し、実行時の残高・lock・予約・allowanceを検査する。retired units、locked units、予約済みの単位を移さない。永久LP receiptは転送可能なPosition又はLP tokenではなく、allowanceを設定できず、withdraw/transfer/fee請求・owner/admin権限も持たない。LPへ戻したellはpool在庫だけへ計上し、receiptとholder残高へ重複加算しない。transferに成功してもsenderの過去のslash責任は残る。

### Decision 6 — サービス提供の未完成部分を有効化しない

membership機能と原資・資産転送の正確性を分ける。chainはoff-chainの実提供を自動裁定しない。今回の送金可能化から、撤回・延期されていたpublic marketplace、モデル改善、強制資料取得監査等の採用を導かない。

## 3. 置換表

| 旧条件 | MOE2での条件 |
| --- | --- |
| holder間転送が存在しない | 認証されたavailable残高の直接転送・受取・譲渡を許可 |
| EVM transfer/approve等は必ずNonTransferable | 新profileの原子的なtransfer/allowance経路を実装 |
| holderの唯一の出口はcurveへのsell | sellと直接転送を別操作として許可 |
| 保有期間は売却だけで変化する | tier別に、確定残高が閾値を継続して満たした期間を扱う |
| 古い署名だけで現在membershipを示す | scope・期限・現在必要なstate anchorと残高を照合 |

## 4. 検査と移行

永久LP記録済みmodelはcore指定不可であり、withdrawをwinddown例外にしない。後継移植、同rootの別ID再登録又はreceipt削除で制約を解除しない。永久LP未記録モデルの旧権利処理は既存の明示移行へ従い、holder資産を自動没収しない。

転送による残高保存、sender/recipient tier、spender権限、locked/retired拒否、zero/self、replay、revert、event、同tx read、cross-domain、reorg/IBDを検査する。旧のno-transferテスト成功は履歴に残し、変更後の成功証拠へ書き換えない。

新分類・policy・ABI・native状態・全consumerを同じactivation境界へ結合する。旧claim・サービス契約は元scopeと原期限でdrainし、coreの旧Position市場をsell-onlyのまま新profileへ残さない。全gate未完の間は設計採択のみである。
