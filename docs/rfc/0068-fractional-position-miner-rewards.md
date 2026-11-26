# RFC-0068 — MOE3：小数POSITION・全モデル統一・50/20/20/5/5配分

- 日付: 2026-11-26（Asia/Tokyo、同日の追加訂正を反映）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。設計・参照計算の改訂であり、MOE3/BMR1のnative接続・移行実行・公開有効化ではない。
- 判断: [ADR0236](../adr/0236-fractional-position-miner-rewards.md)
- 適用・移行: [MOE3移行契約](../governance/fractional-position-economics-transition.md)
- 方針: [moe3-economic-policy.json](../design/palw/economics/moe3-economic-policy.json)
- 恒久廃止・用途別bond: [RFC0069](0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../adr/0237-bft-model-retirement-and-role-bonds.md)
- 検査: [改訂記録](../maintenance/revisions/2026-11-26-fractional-position/README.md)

## 0. 最新の採択内容

ユーザーの最終指定を、同じ対象coinbase原資Cに対する次の配分として採択する。

| 用途 | Cに対する名目比率 | 最終asset・行先 |
| --- | ---: | --- |
| Miner等への採掘報酬 | 50% | この原資で購入したPOSITIONを配布 |
| 流動性用POSITION購入 | 20% | 購入POSITIONを永久LPへ投入 |
| MISAKA側の永久LP | 20% | MISAKAのまま同じpoolへ投入 |
| Panel・Gate C | 合計5% | MISAKAで配布 |
| BFTの継続報酬 | 5% | MISAKAで配布。廃止成功報酬とは無関係 |

**50%分と20%分の合計70%でPOSITIONを購入する。** 端数がない場合は購入数量の5/7をMiner等、2/7を永久LPへ分ける。20%のMISAKAと20%購入原資相当のPOSITIONがLPの両脚となる。整数端数がある場合は§4の実購入予算比を用いる。

同日の途中案「80%購入・POSITION20%相当LP・60%相当Miner」「BFT10%」、直前の40/20/20/15/5及び可変のPanel総枠案は、**今回の50/20/20/5/5へ置換**する。旧MOE2の30/20/残余を新規経路へ混ぜない。

先行指示の全モデル共通化を維持する。genesisもPOSITIONを有効にし、全モデルの係数は1.0、forkモデルの独立区分は廃止。genesisに登録者がいないことが差であり、市場・採掘係数・BFT管理の免除を残さない。新しくモデルを追加する経路はMAD2のKERNEL_AUTO又はBFT_DECISIONである。

旧資料のBILIはMISAKAのtickerであり、通貨の単位やaddress prefixは変更しない。POSITIONの数量を小数化し、model IDやNFT/token IDを小数にしない。

## 1. 置換と継承

MOE3は[RFC0063](0063-additional-model-option-economics.md)／[RFC0064](0064-reward-buy-and-permanent-liquidity.md)の追加モデル限定、core市場なし・直接MISAKA報酬、whole Position、30%買付・20%LP・残余MISAKA、reserve比率からのLP数量、余剰POSITION burn、通常owner3%を置換する。二階層1.0/0.3、追加モデル集合だけの発行上限、[RFC0067](0067-model-admission-kernel-auto-and-bft-decision.md)のgenesis専用BFT免除・fork特別指定も新profileでは適用しない。

不変model ID/root、一意市場、共通発行予算、bond配分・責任上限、7日相当DAA平均・指数1.5、成熟、liability hold、export capを維持する。TB1/FOW1、Panel/Court、Final、独立Gate C、Ticket、個別claimの検証、一回会計と原期限を免除しない。暗号学的なモデル計算validity proofは導入しない。

holderの直接POSITION移転、認証、allowance、旧責任の非移転、非金銭membershipを継承する。モデル改善等の延期範囲は変更しない。恒久LPは通常操作による引出し不可を維持し、正規のBFT恒久廃止による**burn専用例外**だけをRFC0069で追加する。

旧claim、署名、codec、receipt、測定・試験を遡及変更しない。既存private MOE2のnative実装は保存する。本RFCは新wire番号、署名domain bytes、ABI selector、fence・activation高さを割り当てない。

## 2. 全モデル共通の制度

### 2.1 係数と市場

新規MOE3は全モデル `c_m=1` とし、配分重みは既存の平均bondに対する `A_m=mean_bond_m^(3/2)` を用いる。係数を仕事価格や市場報酬へ重複適用しない。全モデルを一つの配分集合として正規化し、追加モデル専用 `registered_tier_cap` を適用しない。共通の総発行・model/bond/claimの上限は維持する。

係数1.0はモデル間の同額報酬、無条件採掘、過去0.3報酬の増額、共通供給上限の引上げを意味しない。originを市場なし・BFT免除・直接MISAKA払いの代理条件にしない。

新規登録はmodel recordと一意市場を原子的に生成する。既存genesis・旧forkの市場がなければ認証移行で生成する。既存市場はresetしない。市場の存在やPOSITION購入自体は採掘採用を意味しない。

### 2.2 採用・管理とfork廃止

genesis由来・後日登録・旧fork由来は認証された来歴として保持し、全モデルへ同じBFT管理・停止・正規解除・恒久廃止の範囲を適用する。新しいFORK_DESIGNATED分類とfork昇格を設けない。旧origin・停止・採用根拠は移行するが、旧policyで権限外だった票を遡及有効化しない。

廃止するforkはモデルの特別区分であり、network genesis hashの認証domain、chain fork/reorg、protocol upgradeではない。不変rootをBFT票で上書きせず、変更モデルは別instanceとして登録する。共通技術互換、未知kernel拒否、個別claim検証は全モデル必須。

### 2.3 登録者の有無

元genesisは認証された `creator_entitlement=None`。通常取引の登録者feeは0%とし、BFT、管理者又はzero addressを架空の登録者にしない。登録者ありは正規のowner/contributor受取規則へ総額1%を配る。登録者側総額の内訳を上乗せしない。

callerの申告ではなく登録・移行stateから有無を読む。登録者があるのに宛先不明なら適法に解決するまで新規取引を受理しない。旧forkに真の登録者がいる場合、分類廃止を理由に消さない。

### 2.4 可逆burn-pause

[RFC0053](0053-additional-model-burn-pause.md)のburn-pauseも全モデルに共通化する。Pauseは正規の支払署名、期待ACTIVE/sequence/直前操作、規定停止burnを必要とする。ResumeはPAUSED状態に対して `native登録者の正規認証 OR 専用BFT BurnResume承認` と、どちらの経路も同じ規定解除burnを必要とする。

専用BFT経路を登録者有無にかかわらず利用可能にし、登録者不在genesisの解除不能を避ける。登録者はfee受取addressやPOSITION holderと同義ではない。専用承認はnetwork/genesis、profile、model/root、期待state/sequence/前操作、roster/epoch、purpose、期限、burn額/上限を束縛する。通常の採用YESや安全停止解除票を流用しない。

実支払者署名とburn会計を原子的に検査する。旧確定PAUSEDはsequenceを保持して移行し、自動ACTIVEにしない。未決旧操作は元termsでdrainする。具体burn額は元の版付き正のパラメーターであり、通常取引5%とは別である。BurnResumeは独立safety stop、Court責任、原期限、**BMR1のRETIRED状態**を解除しない。

## 3. 数量と初期状態

```text
POSITION_DECIMALS = 8
POSITION_SCALE = 100000000
1 POSITION = 100000000 position_atoms
minimum nonzero quantity = 1 atom = 0.00000001 POSITION
```

残高、在庫、購入、LP、Miner受取、allowance、lock、retired、membership閾値を同じatomsで表す。格納はchecked u128、二因子積はchecked u256。価格bps等の三因子以上の比較はchecked u512又は同等の厳密有理比較を用いる。float、wraparound、飽和、未知版、負数を拒否する。

RPC/JSON atomsは正規10進文字列。UIは8桁以下の小数文字列を厳密parseし、指数表記・9桁以上を丸めて受理しない。`decimals=8`とquantity profileを応答へ結合する。旧fieldの単位を同じ版で変えない。zero-transfer等の既存意味は保持する。

新市場の供給は従来どおり500,000 POSITION、初期在庫50,000,000,000,000 atoms。virtual reserveは現行curveと同じ10,000,000 MISAKA。実入金がなければreal reserve=0、seed_locked=0、holder=retired=0。virtual reserveを現金化せず、無償holder配布や新たなseed待機を設けない。既存市場のreserve・holder・永久LP履歴は初期値へresetしない。

## 4. Coinbaseを一回分割する

### 4.1 共通の分母

Cは、共通価格付け・発行上限により認証され、同じcanonical sourceへ結合された**通常coinbaseの配分対象新規発行予算**である。取引fee、slash、廃止burn、返金を含めない。将来の市場失敗・未発行取消を見て過去のBFT/検証役務配分を循環的に再計算しない。

Cのsource key・配分単位は事前profileへ固定し、利用者が任意分割してfloor手数料や配分を変えない。同じCをnetworkと各modelの両方で二重に処理しない。

### 4.2 整数予算

MISAKA最小単位の整数で次を一度だけ計算する。

\[
F=\lfloor50C/1000\rfloor,\quad P=\lfloor50C/1000\rfloor,
\]
\[
A=\lfloor200C/1000\rfloor,\quad J=\lfloor200C/1000\rfloor,
\]
\[
D=C-F-P-A-J,\qquad B=D+J.
\]

FはBFT5%、PはPanel/Gate C合計5%、AはMISAKA側LP20%、JはLP用POSITION購入20%、DはMiner等のPOSITION購入50%と通貨端数。Bは合計購入予算であり、名目70%。常に `C=F+P+A+J+D=F+P+A+B`。Dを独立にfloorして端数を失わない。DそのものをMISAKAでMinerへ渡さない。

これは割合をCへ一度適用する方式である。5%控除後の残額へ再び5%等を掛けたり、Cとは別に追加の50%・20%を徴収したりしない。

### 4.3 複数モデルと役務配分

一つのCが複数モデルを含む場合も上の5legを一度だけ固定する。認証済みmodel配分重みに対し、厳密有理比のfloor後、余剰を最大剰余順で相異なるmodelへ1 atomずつ配る `apportion` を用い、同率ならcanonical model ID順とする。

追加のMiner用予算を `E=D-2J=C-F-P-4J` とするとEは非負で、名目10%Cと通貨端数を含む。`J_i=apportion(J)`、`A_i=J_i`、`E_i=apportion(E)`、`D_i=2J_i+E_i` と結合して配る。これにより全legの総和を保ち、`J_i>0`なら`D_i>=2J_i`となる。DをJと独立に丸めてLP購入だけが正・Miner原資0となる分配を避ける。D_iの理想的な重み按分との差は最大約3 atomsになり得るため、全legで1 atom未満とは主張しない。

各市場は `B_i=D_i+J_i` で購入し、その実予算比 `J_i/B_i` でLP数量を決める。個々のmodelで5%/5%をもう一度引かない。単一モデルの場合は添字を省略できる。global Cが正でも、あるmodelの `B_i=A_i=0` は起こり得る。このmodelはquote・除算より前にno-opとして扱い、receiptを作らない。0/0を評価せず、正の配分だけをdust検査する。

P内部のPanelとGate Cの比率・資格・成熟・受取先・端数は、新profileへ結合した認証済み検証reward planで決める。合計5%であり、それぞれ5%ではない。新たな一律半々を仮定しない。旧planの20%等をそのまま総枠へ重ねない。D内の採掘役割配分も明示planに従い、全員POSITIONで払い、Panel/Gate C/BFTをD側へ重複計上しない。

Fは[RFC0069](0069-bft-model-retirement-and-role-bonds.md)のBFT継続役務planでMISAKA配布する。未設定の受取資格・planをcallerが選べず、元の有限予約・期限・責任を満たしてから支払う。

### 4.4 検証報酬の分離

canonical予算割当時にP、F、市場原資B+Aを用途別に予約し、元原資の消費は一回だけ記録する。正当に成立した検証報酬は、採掘報酬のPOSITION決済失敗又はモデル廃止によって取り消さない。裁定待ちの検証役務資金も専用予約へ残し、Minerの未確定報酬へ混ぜない。

市場の成功はP/Fの正当性の代用ではなく、市場失敗も既に成立したP/Fを消す理由ではない。未成立分は元の役務契約・有限期限で精算し、追加の発行や他modelからの補填を作らない。

## 5. 購入・LP・採掘POSITION配布

入力は同じmodel/pairのr=real MISAKA、V=virtual、s=旧seed下限、U=curve POSITION atoms、H=holder合計、Z=retired。`X=V+r>0`、`r>=s>=0`、`U>=1`、`U+H+Z=S`を要求する。

\[
X_1=X+B,\quad U_1=\lceil XU/X_1\rceil,\quad q=U-U_1=\lfloor BU/X_1\rfloor.
\]
\[
L=\lfloor qJ/B\rfloor,\qquad M=q-L.
\]

LはLPへ戻すPOSITION、MはMiner等へのPOSITION。Cが20で割り切れる単一モデル例ではJ/B=2/7になる。**端数を含む実予算比を規範とし、全入力にq*2//7を適用しない。** 旧q/4、購入後reserve比の `floor(AU1/X1)`、余剰POSITION burnは使わない。POSITION端数はMへ残す。

\[
r_2=r+B+A,\quad U_2=U_1+L=U-M,\quad H_2=H+M,\quad Z_2=Z.
\]

PとFをreal reserveへ加えない。内部購入・LP・採掘報酬配布のAMM feeは0。Vとsは不変。Dをさらに買う別取引を重ねず、B一回の購入でqを求める。

購入、LP両脚、Mの各採掘受取先・hold、receipt、市場原資の予約消費を同じnative journalでcommitする。中間状態への外部取引・callback、後段失敗後の部分commitを認めない。独立に成立したP/Fの役務予約・支払権は市場journalの失敗で失わない。

元の成熟・liability hold前にMをavailableへ出さない。`miner_position_atoms` と検証/BFTのMISAKA額をasset型で分離し、旧MISAKA `miner` fieldのbytesをPOSITIONへ読み替えない。

## 6. 通常取引の5% burn・追加者1%

通常AMMのgross MISAKA額Gについて、

\[
burn=\lfloor50G/1000\rfloor,
\quad creator=\begin{cases}\lfloor10G/1000\rfloor&登録者あり\\0&登録者なし\end{cases},
\quad net=G-burn-creator.
\]

登録者ありの名目netは94%、元genesisの登録者なしは95%。追加者不在の1%をburnへ足さない。この5%は**通常売買のburn**であり、coinbaseのBFT5%又は廃止時の100%burnとは別である。

購入はnetをcurveへ、売却はcurve算出grossをreal reserveから一度出して三項へ分ける。実reserve・旧seed制約を守り、virtualを現金化しない。直接POSITION移転にAMM feeを課さず、通常network fee・署名・nonce・allowance・lockを適用する。送金で登録者権、bond、claim責任、EXEC creditは移動しない。

Courtによる個別不正の確定slashは、実回収MISAKAの49%を正規の不正発見・立証Panel/Gate Cへ配り、残り51%をburnする。これはcoinbaseの検証報酬5%に加える別原資であり、BFT恒久廃止100%burnからの報酬ではない。受取資格と一回性はRFC0069に従う。

## 7. 永久LPと恒久廃止の限定例外

LPはAとLの固定量拠出であり、比例LP持分を発行しない。通常withdraw、redeem、transfer、approve、owner/admin引出し、LP fee請求を持たない。AとLは通常swapに利用でき、Aをseed_lockedへ加えない。LP累積拠出額は現残高・償還額ではない。

非償還receiptはmodel/pair/profile、単位版、source/settlement ID、D/J/B/A/q/L/M、受取先、前参照・後市場値を結合する。自身を含むglobal post-state rootを内部へ置かず、外側native rootで認証する。receiptは供給又は別vaultの追加残高ではない。

RFC0069の正規恒久廃止だけは、対象poolの**現存real MISAKA全額のburn**を許す。歴史的な20%累計又はvirtual額を焼却額にしない。これは誰かへの引出し権ではなく、BFTも没収資産を受領しない。既確定holder POSITIONの数量は保存されるが、市場閉鎖・実reserve消失により同poolで換金できなくなる。

## 8. 数量保存・例・価格

\[
C=F+P+A+B,\quad q=L+M,\quad U_2+H_2+Z_2=S.
\]

`1<=U1<=U2<=U`、`K2=(X+B+A)U2>=K1=(X+B)U1>=XU`。これは一回の算術であり将来価格、換金額、MEV安全性の保証ではない。固定予算比でLPを入れるため、購入直後との価格一致・価格不低下を保証しない。

説明用にV=0、r=1,000 MISAKA、U=10,000 POSITION、C=1,000 MISAKAとする。

| 項目 | 結果 |
| --- | ---: |
| Miner等の購入原資D | 500 MISAKA |
| LP用POSITIONの購入原資J | 200 MISAKA |
| 合計購入B | 700 MISAKA |
| LPのMISAKA A | 200 MISAKA |
| Panel・Gate C P | 50 MISAKA |
| BFT F | 50 MISAKA |
| 購入q | 4,117.64705882 POSITION |
| LPへL | 1,176.47058823 POSITION |
| Miner等へM | 2,941.17647059 POSITION |
| 終了real reserve | 1,900 MISAKA |
| 終了curve在庫 | 7,058.82352941 POSITION |

50%と20%は購入原資の配分であり、POSITIONの時価又は売却額が500/200 MISAKAになる保証ではない。開始価格0.1、購入直後約0.289、LP後約0.26916667 MISAKA/POSITIONとなる。実際の新市場のVや供給をこの例で変更しない。

## 9. quote・小額・有限終端

SnapshotはCのsource、profile/quantity版、model/root/pair、D/J/A/P/Fの配分record、受取plan、quote anchor・鮮度、金額上限、価格影響/slippage、処理順序、原期限、最大再評価回数、一回キーを固定する。抽選後に版・受取先を付け替えない。

`min_miner_position_atoms` はLP控除後Mへ、別の `min_purchase_position_atoms` はqへ適用する。実行時canonical stateから再計算する。未知又は未設定のpolicyを受理せず、多因子比較は§3の幅を守る。

C=0は市場・配布・receiptのno-opであり、元の仕事条件は免除しない。正の市場予算でB/A/J/D/q/L/Mの必要量が0なら市場決済を全体未commitの `LiquidityDust` とする。P/Fは整数floorで0になり得るが、そのために正当な別役務支払を作ったり没収したりしない。

dust、quote逸脱、slippage又は資源不足は元の有限期限内だけ保留・再評価し、未成功終端で未使用市場原資を元契約どおり精算する。既に成立した検証/BFT役務のMISAKA権利は独立に保持する。無期限pending、期限reset、LP省略、直接MISAKA fallback、追加mint、無断poolingを認めない。dustはMiner不正の証拠ではない。

## 10. 移行と検証範囲

旧cohortの支払asset・率・原期限は保持する。旧whole市場は数量だけ一回10^8倍し、通貨額・旧signature bytes・過去receiptを変えない。旧市場操作をdrainしてから切り替える。市場なしのgenesis等は認証移行で一意生成する。

受入は50/20/20/5/5保存、全モデル1.0、8桁codec、fee、quote、役務資金分離、二重消費、失敗原子性、old replay拒否、移行一回性、BMR1用途別bond保護を含む。Python参照は算術と抽象journalの検査であり、native/RPC/ABI、実署名・BFT、役務資格、DAG reorg/IBD、有効化の完了ではない。
