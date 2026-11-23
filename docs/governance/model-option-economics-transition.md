# MisakaOption経済仕様MOE2の適用・移行

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
