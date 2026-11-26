# ADR-0232 — 30%買付・20%永久流動性・余剰Position burnを採択する

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

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — MAD2の採用経路とBFT管理範囲

[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)をモデル採用・管理の後続正本とする。追加モデルはkernel自動審査又は証明提出なしのBFT YES/NOで採用する。旧一律UNKNOWN禁止・BFT停止専用規則はAUTO側と旧profileの履歴に限定する。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

経済比率・永久LP・過去claimは保持する。状態はADOPTED_DESIGN_NOT_ACTIVATEDであり、実装・投票・公開activationの完了ではない。
<!-- MAD2-ADOPTED-20261123-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)
- 会計の統合時点の記録: [RFC-v2-0011](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md)
- 共通移行: [経済profileの適用・移行](../governance/model-option-economics-transition.md)
- 前版: [RFC0063](../rfc/0063-additional-model-option-economics.md)／[ADR0231](0231-additional-model-option-economics.md)

## 背景

MOE1は追加モデルの対象worker報酬Rの50%でPositionを取得し、全量retireする規則だった。新しい方針では、Rの30%でPositionを買い、その一部とRの20%のBILIを永久流動性へ入れ、余剰Positionだけをburnする。残余は元の版付きworker配分へ進める。

既存市場はvirtual reserveを含む整数curveであり、旧seedは通常売却でも払い出せない下限だった。この20%を旧seedへ追加するだけでは、swapに使える流動性という新方針にならない。また、全取得Positionを最初にretireするとLPへ戻せない。金額・Position・永久権限制約を一つの遷移として改める。

## 決定

1. **MOE2を採択する。** wire、codec、ABI、fence、activation位置は未割当。旧claimと過去実装を新規則へ遡及変更しない。
2. **RをB/A/Dへ保存分割する。** `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A`。DのPanel等の配分は受理時planへ従い、全profileへ20%を追加しない。coreはB=A=0、Miner=Rを維持する。
3. **一つの整数kernelに固定する。** `X=V+r>0, U>=1`に対し、`X1=X+B`、`U1=ceil(XU/X1)`、`q=U-U1`、`ell=floor(AU1/X1)`、`burn=q-ell`。結果は`r2=r+B+A`、`U2=U1+ell`、`retired2=retired+burn`。Vと旧seed_lockedは不変。qは一時数量であり、先に全量retireしない。
4. **整数境界をそのまま適用する。** A<=Bからell<=q。余剰burn=0も正常であり、連続3:2から整数でもell/q<2/3と主張しない。例X100/U60/R10ではq=ell=1、burn=0。
5. **dustを成功へ変換しない。** B>0かつq=0、又はA>0かつell=0はLiquidityDustで全操作未commit。原有限期限内だけhold・再評価し、解消しなければ既定未成功終端。成功R/creditの先行発行、期限reset、donation、mint、手数料版変更、全額Minerへのfallbackを設けない。B=A=0は市場二legだけno-opでD=Rの元配分を維持する。A=0の成功買付はq全量burn、LP操作・receiptなし。
6. **永久LPを非償還native履歴として記録する。** A>0かつell>0を成功commitした時だけ、`LockedLiquidityReceipt`を`PERMANENT_PROTOCOL_LOCK`へ結合する。撤去、償還、transfer、fee請求、owner/admin鍵、期限解除を設けない。外部流通LP token又は公開LP入口を追加しない。
7. **永久lockとswapを区別する。** Aとellは通常swap用reserve/在庫へ一度だけ入り、receiptは追加assetでも第四の供給項でもない。旧seed_lockedとVを増やさず、通常swapを禁止する「同じcoinが永久に動かない」という意味にしない。
8. **永久LP付きモデルをcoreへ指定しない。** 成功receiptがあるmodel IDとcanonical artifact rootの履歴を認証する。同rootの再登録、owner変更、pruning、後継移植でcore指定禁止を迂回させない。永久LPをwinddownで解除しない。永久LPのない旧cohortだけを元条件でdrainし、core旧市場の解消条件へ接続する。
9. **入口と通常経済は維持する。** 追加登録と一意市場の生成は原子的。genesis/正式fork指定にはOptionを作らずR全額をMinerへ帰属させ、Panel等は別の事前原資で賄う。通常AMMはburn5%・owner側総額3%・net差分、直接移転はAMM手数料なし。内部買付・LPの追加BILI burn/owner支払は0。
10. **全工程を原子的に一回だけ決済する。** B/A流出、q/ell/burn、D、receipt、型制約、消費キーを同じnative遷移へ結合する。市場順序、quote・amount上限、整数qに対するminOut、価格影響、slippage、原期限を経済snapshotに固定する。B+Aも共通export capへ合算し、途中取引やreplay/IBDで増幅させない。

receiptのsettlement IDは固定claim文脈から事前決定する。前market参照とreceipt自身を除いた後market値を記録し、global post-state rootの自己参照を作らない。累積A/ellは拠出履歴であり、現在残高や担保価値ではない。成熟後の責任精算もLPの特別withdrawで行わず、元のbond/holdを維持する。noncanonical reorgはreceiptを含む原子遷移全体の巻戻しであり、私的な解除権とは区別する。

## 優先関係

本採択は、RFC0063・ADR0231・ADR0091の50%買上げと全量retire、ADR0090のLP記録不在、ADR0162の旧kernel・seedとの関係、永久LP保有市場のcore移行をRFC0064の範囲で置換する。pre-trade seedの元契約、通常売買、移転、core R100%、資本・発行・成熟・hold/exportの上限は継承する。

TB1/FOW1、全実行検証、一人の有効な反証、Court、旧claimの元期限と責任は変更しない。旧MOE1の原文・codec・fence・測定は版固有の履歴として保全する。

## 効果と限界

整数kernelは購入量の範囲でLPへ戻すPositionを決め、余剰burnと資金保存を一意にする。永久receiptに私的引出し権を与えず、市場内の流動性として利用する。

この算術は、価格上昇、holder利益、任意売却数量へのreal reserve十分性、先回り取引の不採算、LLM仕事費用の下界を証明しない。整数dustは活性制約として残る。最小仕事・quote・amount/slippage上限と有限終端を測定・実装し、既存市場への移行、native/EVM、一回性、reorg/restart/IBDを確認するまで未有効化とする。

## 2026-11-23 MAD2の分類と管理権限

[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)に従い、元genesisはBFTの追加・変更・停止・解除の対象外とする。BFT YESでfork指定モデルを追加でき、Optionなし・対象worker報酬R100% Minerを継承するがBFT停止対象である。追加モデルも自動/BFTの採用経路にかかわらずBFT管理対象とする。永久LP付きmodel ID・同rootのcore指定禁止、既存市場と債務のdrain、既存停止はYESで免除しない。
