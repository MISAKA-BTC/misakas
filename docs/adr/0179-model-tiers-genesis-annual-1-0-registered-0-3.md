# ADR-0179 — モデル二階層と追加モデル専用MisakaOption

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
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

## 1. 経済tierと分類

genesis及びnativeの年次fork catalogが指定したモデルは1.0、追加登録モデルは0.3という既存tier方針を維持する。係数はmodel配分の重みに一度だけ適用し、確定したRへ再度掛けない。κと縮小分・共通期間の適用版は現在のparameter台帳に従い、未承認値をこの改訂で確定しない。

MOE2の分類は次とする。

| 分類 | 判定元 | MisakaOption | 対象worker報酬R |
| --- | --- | --- | --- |
| GENESIS | native genesis catalogの固定model ID＋artifact root | 無効 | Minerが100% |
| FORK_DESIGNATED | 有効化されたnative年次fork catalogの指定。softforkを含む | 無効 | Minerが100% |
| ADDITIONAL | 正規追加登録であり、上の二分類に入らないmodel | MOE2下の追加登録と同時に一意市場を生成 | 30%購入・20%永久LP・余剰burn、残余をreward planで分配 |

「softforkモデル」はnative catalogによる指定を意味し、モデルの派生版・forkしたrepository・所有者の名称変更を意味しない。catalog/edition、model ID、root、分類、経済policy、受理anchorをEconomicsSnapshotへ固定する。判定不能な状態を都合のよいtierへfallbackしない。

## 2. MisakaOptionの適用範囲

Position化、protocol AMM、buy/sell、任意seed/deepen、native/EVM transfer・受取・approve/transferFrom、報酬購入・永久LP・余剰burn、PositionベースmembershipはADDITIONALだけに認める。永久LPはnative protocol操作であり、公開LP入口又は外部流通LP tokenを増やさない。genesis又はfork指定モデルについて、別のline名、proxy、facade、APIから市場を作ることはできない。

coreのworker報酬Rは、購入0、永久LP0、owner0、報酬由来burn0、RからのPanel pool0でMinerへ全額帰属する。必要なPanel・資料・Court・TB1の役務資金は、Rから隠れて控除せず別の署名済み前払契約で確保する。資金不足なら新claimを受理しない。

追加モデルの通常AMM売買はburn5%・owner総leg3%・残差net、直接送金はAMM税なし。追加モデルはB=floor(300R/1000)でPositionを購入し、A=floor(200R/1000)と必要Positionを同じ市場へ永久LP化し、余剰だけをburnする。B/Aとも追加BILI burn/owner0%、残余D=R−B−Aはworker planへ配分する。MOE2下の追加登録成功は一意市場の生成を含む。ただし、市場が存在することと、FOW1採掘適格性・個々のclaimの正当性が成立することは別である。

## 3. 登録と実行の検証

追加model登録、pair identity、初期virtual reserve・実reserve・Position在庫・owner参照を同じnative遷移で生成する。失敗時は全体をrollbackし、外部seedがなければ実reserve初期値0を許す。別の市場作成、後続open、seed/deepen又はmarket-ready待機を前提にしない。seed/deepenは登録後の任意増強である。GENESIS/FORK_DESIGNATED登録では市場を生成しない。登録済み追加モデルの市場欠落はstate invariant違反として拒否し、正常な未準備状態へ分類しない。

一般登録はモデルの人気・品質・保有者数・取得ACKによる審査へ戻さない。固定artifactの不変性を保持する。新FOW1に対する数学的・構造的な適格性は[RFC0062](../rfc/0062-palw-function-output-and-work-admissibility.md)の別の共通ruleで確認する。tier 1.0 のモデルにも同じG-COST検査を適用する。

全分類のclaimは同じ元実行binding、選出Panelの完全再実行、資料・異議機会、認証された局所Court、反証優先、成功条件を満たす。genesis・fork指定への100%Miner配分は、これらの免除ではない。Position移転でclaim責任・bond・owner権が移ることもない。

## 4. Fork指定と旧市場の移行

model/rootを上書きせず、catalogの版付き指定を使う。A>0かつell>0の永久LPが実際に記録されたmodelにはstickyな`hasPermanentLP`を保持し、GENESIS/FORK_DESIGNATEDへの指定を拒否する。LPを解除してwinddownしたことにせず、owner/admin回収、後継移植、同rootの別ID再登録、receiptのpruningで制約を迂回しない。A=0でreceiptを作らない処理だけから、この属性を新規設定しない。

永久LP未記録の追加モデルをFORK_DESIGNATEDへ指定する場合だけ、切替前に旧profileの新規受付を閉じ、受理済みclaim、購入/LP義務、AMM request、allowance、Position残高、membershipを明示した移行手順で処理する。drain中に永久LPが記録されれば、core指定条件を再検査して拒否する。

MOE2でcore市場をsell-onlyのまま継続しない。旧Positionを自動没収したり、後継modelへ移し替えたり、無根拠にmintして補償したりしない。旧残高等のwinddownが完了しなければ、そのcore経済分類へのactivation条件を満たさない。新規claimと旧snapshot決済を混同し、後付け分類で30%購入・20%永久LP又は100%Minerを選ばせない。

既存core市場を持つnetworkでMOE2を初めて有効化する場合も同じwinddown gateを適用する。単にmarket flagをfalseにしてreserveやholder権利を消す手順は認めない。

## 5. 完成条件

分類の正本とsnapshot、全native/EVM入口、core禁止、追加登録と一意市場の原子生成、購入/永久LP決済資源予約、永久LP記録済みmodelのcore指定拒否、R全額Miner、別Panel原資、tierの二重乗算拒否、fork境界、再編・IBD・再起動、winddown完了を同じ実装・試験で確認する。過去のtier実装・未build試験source・旧fenceは保存原本の記録であり、新経済条件の実装完了ではない。
