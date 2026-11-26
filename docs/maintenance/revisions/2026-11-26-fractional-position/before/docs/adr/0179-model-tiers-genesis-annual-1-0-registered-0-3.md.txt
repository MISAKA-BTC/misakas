# ADR-0179 — モデル二階層と追加モデル専用MisakaOption

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
