# MisakaOption経済仕様MOE2の適用・移行

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../adr/0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

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

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../adr/0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — MAD2の採用経路とBFT管理範囲

[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)をモデル採用・管理の後続正本とする。追加モデルはkernel自動審査又は証明提出なしのBFT YES/NOで採用する。旧一律UNKNOWN禁止・BFT停止専用規則はAUTO側と旧profileの履歴に限定する。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

経済比率・永久LP・過去claimは保持する。状態はADOPTED_DESIGN_NOT_ACTIVATEDであり、実装・投票・公開activationの完了ではない。
<!-- MAD2-ADOPTED-20261123-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。市場は登録時に作成する。MAS1の審査待ちや停止を市場未準備・市場閉鎖へ読み替えず、MOE2の率・永久LP・確定残高を変更しない。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未検査の費用経路の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。native/ABI/state版、activation位置は未割当。
- 正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)、[ADR0232](../adr/0232-reward-buy-and-permanent-liquidity.md)
- 会計の統合時点の記録: [RFC-v2-0011](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md)
- 前版: [MOE1 RFC0063](../rfc/0063-additional-model-option-economics.md)／[ADR0231](../adr/0231-additional-model-option-economics.md)。旧claimは元版の意味を保持する。
- 継承: [TB1/FOW1](palw-tb1-fow1-transition.md)の正当性、乱数、採掘適格性、元claim、Panel・Court・資料・合意条件。

## 1. 対象と市場生成

RFC0067により、BFT YESはFORK_DESIGNATEDの追加も認可できる。forkはgenesisと同じOptionなし・Miner=Rだが、BFT停止・解除の対象である。BFT管理対象外はchain開始時に固定されたGENESIS_ORIGINALだけとし、分類・alias・rule停止でこの境界を迂回しない。追加モデルは自動採用かBFT採用かにかかわらずBFT管理対象とする。

MisakaOptionはADDITIONALだけに適用する。GENESISと正式softfork・年次fork指定モデルにはOption市場を生成せず、対象worker報酬Rの100%をMinerへ帰属させる。native identity、artifact root、有効catalogから分類し、自己申告や係数表示だけで選ばせない。

**追加モデル登録と一意市場生成は同じ原子遷移。** model/pair、curve profile、virtual/real reserve、Position在庫、owner参照を同時に確定し、初期の永久LP記録集合は空とする。片方だけ成功させず、登録済み・市場未作成、後続open、外部seed待ちを通常状態にしない。市場欠落を買上げ免除・Miner全額払いへ変換しない。

外部seedは既存市場への別の任意操作であり、報酬LPとは区別する。市場の存在、FOW1採掘適格性、資料取得、成功Final、任意額の売却支払能力は別条件。virtual reserveは実BILIではない。

## 2. 30%・20%・残余

Rは当該モデルの認証済みclaimに帰属し、tier・価格付け・発行予算・上限を適用した後、買上げやPanel配分前の非負整数worker報酬である。ADDITIONALは、

\[
B=\lfloor300R/1000\rfloor,\quad A=\lfloor200R/1000\rfloor,\quad D=R-B-A.
\]

| leg | 目的 |
| --- | --- |
| B、名目30% | 同じモデル市場でPosition購入 |
| A、名目20% | 購入Positionの必要量と組み合わせ、同じ市場へ永久流動性追加 |
| D、名目50% | 当該版のreward planでMiner・Panel等へ分配 |

Dを独立にfloor(500R/1000)と計算して端数を失わせない。Positionを買うための追加20%や50%を別徴収しない。R=1,000なら300/200/500、旧残余20%Panel planの例ではPanel100・Miner400。**RのLP20%と残余のPanel20%は別のleg**である。

coreはB=A=0、Miner=R、RからのPanel・owner・LP等控除0。検証役務は別の事前資金・有限予約・同意による。100%帰属は検証免除、即時払、発行増、成熟・hold・export上限の免除ではない。

## 3. 固定整数kernel

X=V+r>0、r=実BILI reserve、V=市場固定virtual reserve、U>=1=poolのwhole Position在庫とし、

\[
X_1=X+B,\quad U_1=\lceil XU/X_1\rceil,\quad q=U-U_1,
\]
\[
\ell=\lfloor AU_1/X_1\rfloor,\quad z=q-\ell,
\]
\[
r_2=r+B+A,\quad U_2=U_1+\ell,\quad retired_2=retired+z.
\]

qが購入量、ellがLPへ戻す量、zが余剰burn。qを先に全retireしてellを復活させない。A<=B、q=floor(BU/X1)、U1<=Uにより**0<=ell<=q**が整数でも成立する。連続値で厳密3:2ならell/q=(2/3)X/(X+B)<2/3だが、整数ではその割合や最低burn割合を保証しない。

次のswapは新rowのK2=(V+r2)U2を使う。Vと旧seed_lockedは不変。LPのための追加Position発行、retired復活、仮想BILIの現金化は禁止。[証明と反例](../maintenance/revisions/2026-11-20-permanent-liquidity/MATH_PROOF.md)を参照する。

## 4. 永久LPと保存則

nativeの**LockedLiquidityReceipt**をPERMANENT_PROTOCOL_LOCKへ記録する。withdraw、移転、allowance、LP fee請求、owner/管理者引出し、bridge/wrapperによる取出しを持たない。外部流通LP tokenや一般公開の任意LP入口は新設しない。

永久に禁止するのはLP撤去・償還であり、**Aとellは通常swapに使えるpool資産**。同額BILIが永遠にpoolへ残る保証ではない。Aを旧seed_lockedへ加え、売却に使えない現金floorへ変えてはならない。

保存則は `U + sum(holder) + retired = registered_supply`、`q=ell+z`、`R=B+A+D`。ellはpoolへ一度だけ戻し、receiptを第四の供給項や別残高として加えない。累積LP拠出量は履歴であり、現在reserveや担保価値ではない。

receiptはclaim/model/pair/policy/settlement ID、B/A/q/ell/z、事前参照と事後市場値を認証する。自身を含むglobal post_state_rootを内部へ格納する循環を作らず、完成receiptは通常の外側native rootで認証する。

## 5. 原子性・dust・価格条件

購入→LP→余剰burn→receipt→D配分→claim経済消費を一つのnative journalでcommitする。途中の外部取引・callback・部分支払を認めない。export capにはBとAを両方加算し、Dと合わせたR全体に元の成熟・hold・発行制約を適用する。

B>0でq=0、又はA>0でell=0なら**LiquidityDust**として全体未commit。受理時に固定した元の有限決済期限までだけ保留し、解消せず期限を越えれば既定の未成功終端・元原資精算へ進む。成功R/creditの先行発行、無期限Pending、期限reset、追加mint、片側donationへの無断変更、Miner全額fallbackは禁止。

A=0ではLP操作・receipt・永久LP flagを新設しない。B>0かつq>0ならq全量burn。B=A=0は市場2legだけno-opであり、D=Rのworker配分と元の最小仕事条件は残る。

quote・金額上限・最小受取量・許容価格影響・deadlineをversioned経済snapshotへ固定し、同じ実行時pool状態からnativeが再計算する。未指定policy、unchecked overflowを拒否する。MEV、quote freshness、事前の外部売買、dustによる決済活性は別評価であり、ell<=qの証明から解決済みとしない。

## 6. 手数料と送金

通常AMMはgross Gにburn=floor(50G/1000)、owner側総額=floor(30G/1000)、net=G−burn−owner。owner総額にはcontributor内部配分を含み、実reserveの売却減少はgrossに一致させる。

**内部購入BとLP拠出Aの両方で、追加BILI burn・owner直接支払は0。** Positionの余剰burnから別配当を作らない。holder Positionの直接送受信・approve/transferFromにはAMM税を課さず、native/EVMの共通残高・同期journal/revertを維持する。pool在庫・receipt・retiredをholderへ移す例外を作らず、bond・旧責任・model owner・ExecCreditを移転させない。

## 7. 旧claimと分類変更

既受理claimは元MOE1/旧版snapshotの率と原期限でdrainする。Final時に30/20へ読み替えず、文書採択をnetwork activationと同一視しない。

**永久LP receiptを正規に記録したmodel IDは、本profileではcore指定不可。** permanent flagを消す、winddownでLP撤去、後継移植、同artifact/rootの別名再登録で迂回しない。永久LPのない旧市場だけ、旧権利・残高・allowance・義務の適法な清算後に元のcore移行条件を適用できる。

後日の不正claim責任精算、価格変動、モデル停止、owner変更はLP撤去権を作らず、既存bond・hold・責任契約で扱う。noncanonical reorgは市場・receipt・claim配分を含む原子遷移全体を元に戻すことで、通常のLP償還とは区別する。

## 8. 置換とactivation

MOE1の50%全購入・全retire、旧no-LP条項、旧seedとLPの混同、core指定前の一般winddown、単一buyback表示を、新しい30/20/残余・非償還receipt・整数kernel・分類制約へ置き換える。旧原条文や試験は履歴として保持する。

native状態、receipt認証、新curve操作、fee、EVM/RPC表示、finite hold/終端、snapshot、分類拒否、replay/reorg/IBD、quote・量上限の値を実装・検証してから明示activationする。TB1/FOW1やCourtの未完了条件を免除しない。

[今回の改訂記録](../maintenance/revisions/2026-11-20-permanent-liquidity/README.md)／[MOE1の履歴](../maintenance/revisions/2026-11-20-option-economics/README.md)。
