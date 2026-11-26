# ADR-0235 — モデル採用をkernel自動追加とBFT YES/NOへ分ける

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTによるモデル廃止を可逆Pauseと区別した恒久終端にする。モデル廃止と個人の有罪判定を分け、次のscopeだけを処理する。

| 対象 | BMR1のモデル廃止時 | 個人不正との関係 |
| --- | --- | --- |
| 当該モデルの永久LP内MISAKA | 認証済みactual reserveを100%burn | V・receipt累計・別pool残高を実資産に数えない |
| 当該モデルの登録者責任bond | 対象の責任lotを100%burn | 無関係なwallet/別model資産へ拡張しない |
| 当該モデルのmining bond | MINING用途lotを100%burn | 同じ人がPanelを兼任しても用途を分ける |
| 当該モデルの未確定mining報酬 | 100%失効/burn | 未発行予約失効と実資産burnを二重計上しない |
| 全Panel/Gate C verification bond | **一律にBFT廃止burnから保護** | 不正者も同じ保護。個人不正はCourtの個別slashで処理 |
| 正当に成立した検証報酬 | MISAKAで支払を維持 | 廃止を理由に成立済み役務対価を横取りしない |
| 他モデルの資産・bond・報酬 | 影響なし | 同一address・同一参加者という理由で混ぜない |

兼任者は用途別lotで管理する。MINING6,000/PANEL4,000なら前者6,000だけをburn、検証用途4,000は保護する。同じ元本の重複担保と責任中の用途変更を禁止し、廃止直前の付替えでburn又は保護を迂回しない。

通常の永久LP withdraw/redeem・管理者引出し禁止を維持し、BMR1 canonical廃止だけにactual MISAKA reserveの宛先なし消却を認める。**BFTは没収を受け取らず、廃止bountyは0**。継続5%を廃止件数や没収額に連動させない。RETIREDを通常Resume/BurnResumeで戻さず、台帳finality・fork choice・任意残高処分の権限を追加しない。

旧「BFTは停止だけ」「永久LPを一切burnできない」等は、この明示的な廃止scopeだけで置換する。旧claim/旧profileの義務・履歴・wire・実装/過去試験を保全し、BMR1 native接続・移行実行・公開activationを今回完了扱いしない。

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

- Date: 2026-11-23（Asia/Tokyo）
- Status: **ADOPTED_DESIGN_NOT_ACTIVATED**
- Authority: 今回のユーザー指示「モデル追加経路でkernelの自動追加経路とBFTによる承認の追加経路で2種類」「BFTは…YES NOだけで証明不要」。
- RFC: [RFC-0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)
- Transition: [MAD2移行契約](../governance/model-admission-dual-route-transition.md)

## 背景

MAS1は自動判定のUNKNOWNを保留し、BFTを停止・証拠付き解除に限定した。これでは、通常のLLMに対する非衝突定理や費用審査が揃わない場合、人の採否判断で実用モデルを採用する経路がない。ユーザーは二つ目の入口を明示的に追加した。

また、LLMの出力が計算前に予測困難であることと、モデル非保有Nodeが提出出力を安く検証できることは別である。モデル採用の簡素化を、個別claimの検証廃止へ拡大しない。

## 決定

1. 採用basisはKERNEL_AUTO又はBFT_DECISIONの二種類。同じ不変instance、admission reducer、停止集合を共有する。
2. 自動経路は既存kernelへのloweringと現行自動条件を使う。新しい未知primitiveの実行意味を自動ダウンロードしない。
3. BFT経路は所定rosterのq YESで採用、q NOで不採用。理由・数学的証明・費用/品質の証拠提出は票の成立条件ではない。
4. 共通技術条件は両経路で必要。未知演算・非決定的意味・codec/commitment不正をYESで免除しない。
5. G-COLL/G-COST/G-USEのUNKNOWN又は採用判断FAILを保持したままBFT_POLICY_APPROVALとして採用できる。全PASSを共通ANDへ戻さない。
6. n=3f+1、q=2f+1の権限・署名認証と、同一proposalへの非二重署名を要求する。実人数・署名codec・activationは別途固定する。
7. BFT承認は数学的安全性の証明ではない。未知の高速算法や品質低下は採用判断の残余リスクとして引き受ける。
8. claimのPanel、Court、Final、独立Gate C、Ticket、権利と会計は維持する。新しい乱数・台帳finality・別の受賞経路を追加しない。
9. 停止は承認より優先し、解除は現在の停止集合へ明示的に束縛する。正しい高速化や誤った採用判断だけで過去資産を没収しない。
10. 旧claim・旧評価結果・既存市場・過去ログは保持する。本決定からQF1合格、native実装又は公開完了を推定しない。

## 追加指示：genesisだけをBFT管理対象外とする

genesis manifestでchain開始時に固定されたGENESIS_ORIGINALは、BFTによる採用・変更・削除・停止・解除の対象外とする。genesis追加や分類の後付けを許さない。個別claimの正当性・当選・会計検証は維持する。

追加モデルは自動採用又はBFT YESで登録し、どちらの経路でもBFTが停止・再審・正規解除を管理する。BFTはFORK_DESIGNATEDもYES/NOで追加でき、そのモデルはOption対象外・対象worker報酬R100% Minerというgenesis同様の経済扱いを受ける。しかしBFT停止免除は持たない。**停止管理の対象外は元genesisだけ**である。

投票本文に経済分類と不変originを束縛する。fork候補の事前ADDITIONAL市場登録は不要とし、YESの正規包含でfork登録・catalog指定・採用を原子的に行う。既存追加モデルの変換では永久LP・旧市場・責任・停止を消去できない。旧「coreも全てBFT停止対象」はgenesisについて明示的に置換する。

## 明示的な置換

| 旧規則 | MAD2対象での後続規則 |
| RFC0065のgenesis/fork一律のBFT安全管理 | 元genesisだけ除外。追加モデルとforkモデルはBFT管理対象 |
| forkは既存catalog指定でのみ追加 | MAD2ではBFT YESによるfork指定を追加。停止権限を保持 |
| --- | --- |
| RFC0062の全候補G-COLL定理・G-COST/G-USE合格必須 | KERNEL_AUTOの要件。BFT_DECISIONでは採否投票へ置換する |
| RFC0065/ADR0233のBFT停止専用・UNKNOWN承認禁止 | 採用権限を用途別に追加。q YESによるBFT承認を許す |
| RFC0066/ADR0234の未完成G-COLLによる一律採掘保留 | 自動経路は保留。技術互換なQF1 instanceはBFT経路を選べる |
| BFT安全停止解除時の全自動gate PASS | 自動経路は維持。BFT経路は技術互換・現YES・正規解除へ分岐する |
| ADR0180の台帳BFT禁止 | 維持。採用と停止以外の権限を与えない |

本ADRは旧文書番号・wire ID・署名済みpayloadを再利用しない。更新前の規範本文は[改訂記録](../maintenance/revisions/2026-11-23-dual-route-admission/README.md)に保存する。

## 影響と実装方針

採用を人の判断で進められる一方、数学的なモデル安全性の保証は増えない。誤採用、審査の集中、検閲、quorum不成立、停止遅延を明記する。自動経路は投票なしの入口として残す。

実装は新しいadmission basis、票のcarrier、通常状態へのfold、共通gate、UIの保証表示に限定して接続し、既存の停止・署名・kernel・claim処理を再利用する。Gate C等の削除は別の設計変更として扱う。
