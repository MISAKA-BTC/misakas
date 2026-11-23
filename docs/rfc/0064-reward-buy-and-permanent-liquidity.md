# RFC-0064 — 報酬30%買付・20%永久流動性・余剰Position burn

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — MAD2の採用経路とBFT管理範囲

[RFC0067](0067-model-admission-kernel-auto-and-bft-decision.md)をモデル採用・管理の後続正本とする。追加モデルはkernel自動審査又は証明提出なしのBFT YES/NOで採用する。旧一律UNKNOWN禁止・BFT停止専用規則はAUTO側と旧profileの履歴に限定する。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

経済比率・永久LP・過去claimは保持する。状態はADOPTED_DESIGN_NOT_ACTIVATEDであり、実装・投票・公開activationの完了ではない。
<!-- MAD2-ADOPTED-20261123-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。設計採択であり、native実装、経済的安全性の実測、公開有効化の完了ではない。
- 論理profile: **MOE2**。wire値、native object tag、codec、ABI、fence、activation位置は未割当。
- 判断記録: [ADR0232](../adr/0232-reward-buy-and-permanent-liquidity.md)
- 現行会計: [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)
- 適用・移行: [共通経済移行](../governance/model-option-economics-transition.md)
- 先行規範: [MOE1/RFC0063](0063-additional-model-option-economics.md)、[TB1](0061-palw-internal-threshold-beacon.md)、[FOW1](0062-palw-function-output-and-work-admissibility.md)
- 数学的範囲: [整数kernelの証明・検査](../maintenance/revisions/2026-11-20-permanent-liquidity/MATH_PROOF.md)

## 0. 採択する構成と継承範囲

ADDITIONALの対象worker報酬Rを、30%のPosition買付、20%のBILI流動性追加、残余のworker配分へ分ける。買付で得たPositionのうち、追加BILIと規範比率で組み合わせる整数数量を同じ市場へ戻し、残りだけをburnする。買付・流動性追加・burn・報酬配分を、一つのnative原子的決済として処理する。

**永久LPとは、その流動性に対する撤去・償還権を誰にも与えないことである。** LPへ入ったBILIとPositionは市場の通常swapに使える。BILIそのものを売却不能な旧seedとして積み上げる規則ではない。

| 項目 | ADDITIONAL | GENESIS / FORK_DESIGNATED |
| --- | --- | --- |
| 市場 | 追加登録時に一意市場を原子生成 | 作成・有効化しない |
| RからのPosition買付 | `B=floor(300R/1000)` | 0 |
| RからのLP用BILI | `A=floor(200R/1000)` | 0 |
| 残余worker報酬 | `D=R-B-A`。版付きplanで配分 | `Miner=R` |
| 内部買付・LP追加のBILI手数料 | 追加burn 0、owner直接支払0 | 対象なし |
| 取得Position | LP用数量をcurveへ戻し、余剰をburn | 対象なし |
| LPの保有・管理 | 非償還native receipt、永久protocol lock | 対象なし |

R、分類、事前資金、旧claim、通常AMMのburn5%・owner側総額3%・net残差、holder間の直接移転はRFC0063から継承する。ModelPositionの直接移転にAMM手数料を課さず、通常network feeだけを適用する。GENESIS/FORK_DESIGNATEDのRからPanel、Verifier、prefix worker、ownerその他の役務費を控除せず、必要な検証役務は別の事前確保原資で賄う。

二階層係数、資本配分、発行・export上限、成熟、liability hold、完全検証・反証優先、TB1/FOW1の条件を変更しない。経済的な30/20配分は、LLMの反復費用検査やDAG安全性の代わりにはならない。

## 1. 登録identity、初期状態、適用時点

モデル分類はnative model ID、canonical artifact root、有効catalogから解決する。表示名、caller申告、単独のtier係数から分類を選ばせない。ADDITIONALの登録recordと一意市場を、同じnative遷移で生成する。

\[
\operatorname{Registered}(m,\mathrm{ADDITIONAL})
\Longrightarrow \exists!p:\operatorname{MarketFor}(m,p).
\]

登録と市場の一部だけを成功させない。pair identity、curve版、virtual/実reserve、旧seed、Position在庫、retired、owner参照、永久LP履歴の初期状態を同時に確定する。初期値は登録時の採択済みcurve profileに従い、本RFCの計算例から変更しない。実入金がなければreal reserveは0でよく、virtual reserveを実入金と見なさない。

市場の存在に外部seed、別の市場作成、後続open又はmarket-ready待機を要求しない。市場欠落は不正state又は移行不整合であり、正常なclaim分岐やMiner全額fallbackにしない。市場の存在とFOW1採掘適格性は別条件である。GENESIS/FORK_DESIGNATEDの登録・指定は市場を生成しない。

旧pre-trade seedはその版の条件で維持する。MOE2の報酬LP追加はseed操作ではなく、売買開始後にも適用する別の内部遷移である。20%のAを旧`seed_locked`へ加算しない。

## 2. 金額とSnapshot

### 2.1 報酬の整数分割

Rは一つの認証済みcanonical claimについて、仕事価格付け、tier、資本配分、発行上限を適用した**市場配分・Panel分配前のworker報酬**である。network総報酬、取引fee、provider/Court fee、返還金を混ぜない。最小通貨単位の非負整数で表す。

\[
B=\left\lfloor\frac{300R}{1000}\right\rfloor,\quad
A=\left\lfloor\frac{200R}{1000}\right\rfloor,\quad
D=R-B-A.
\]

30%、20%、50%は名目比率であり、端数はDへ残す。BとAを求めた後にDを独立したfloorで計算しない。常に`R=B+A+D`、`0≤A≤B`が成り立つ。総50%を先に切り捨ててから差額をAへ振り替える別式を使わない。

Dの役割配分は受理時の`reward_plan_id`に従う。他のworker legを持たない20% Panel pool版なら`P=floor(200D/1000)`、`Miner=D-P`。全profileに20%を自動適用せず、pool残差・資格・支払条件を元planへ結合する。R=1,000ではB=300、A=200、D=500であり、このplanのPanel=100、Miner=400となる。

### 2.2 経済Snapshotと数量・quoteの制約

MOE2の`EconomicsSnapshot`はRFC0063のnetwork、policy、model/root、catalog/class、pricing、reward plan、pair、owner規則、原資・原期限・予約に加え、次の規則と必要な値を固定する。

- curve profileと整数精度、`ceilDiv`の規則、reserve/units/productの上限。
- R・B・Aの上限、買付・LPの数量上限、許容するquote参照anchorとその有効期間。
- 買付数量の下限`minOut`、価格影響上限、slippage許容幅、その計算基準と認証方法。
- 正規の市場処理位置・順序、再評価の条件、最大処理回数、元の有限決済期限。
- 永久LP receiptの版、永久LP履歴の分類制約、二重決済防止キー。

これらは論理要件であり、未割当のfieldを既存署名codecへ暗黙追加しない。未知の版、未設定の上限・quote規則は新規受理に使わない。実行時には同じpairの認証canonical状態からq、ell、burnを再計算する。`minOut`の単位は**今回購入する整数Position量q**であり、burn量やBILI額ではない。

FOW1の文脈とsnapshot参照をTB1 EXEC公開前に固定する。抽選結果を見て経済版、型、pair、上限又はquote条件を有利に付け替えない。古いquoteを無条件採用せず、原規則の範囲内の再評価だけを許す。期限・slippage・amount制約を満たせない場合も§6の有限な処理に従う。

## 3. 一つに固定する整数kernel

### 3.1 入力状態

同じ市場について、Vをvirtual BILI、rをreal BILI reserve、Uをcurve内の整数Position在庫、sを旧`seed_locked`、Zをretired/burned unitsとする。

\[
X=V+r>0,\quad U\ge1,\quad r\ge s\ge0,\quad K=XU.
\]

Vは入金・払出可能資産ではない。登録済みVとsはこのkernelで変更しない。`issued_supply=U+sum(holder_units)+Z`を満たす認証状態だけを入力にする。未決済注文等の有効なholder lockは既存規則に従う。

### 3.2 Positionを30%で購入する

\[
X_1=X+B,\qquad
U_1=\left\lceil\frac{XU}{X_1}\right\rceil,\qquad
q=U-U_1=\left\lfloor\frac{BU}{X_1}\right\rfloor.
\]

B全額を同じpairのreal reserveへ入れる内部買付であり、通常AMMの5%/3%を控除しない。qは原子的処理内の一時数量で、Miner・owner・holderへ渡さず、まだretireもしない。第三者がこの中間状態へ売買を挟める別transactionにしない。

### 3.3 20%のBILIとPositionをLPへ入れる

\[
\ell=\left\lfloor\frac{AU_1}{X_1}\right\rfloor,\qquad
d=q-\ell.
\]

A全額とell unitsを同じ市場へ投入する。ellは**買付後のvirtual曲線価格X1/U1**に対してfloorで決める。Vを実資産としてLPへ入れる意味ではない。整数のため正確な比率とは限らず、余剰BILIを別口へ返金したり、比率を合わせるPositionをmintしたりしない。§6のdustに該当する場合は全操作をcommitしない。

ellをcurve在庫へ戻し、dだけをburn/retireする。結果は、

\[
r_2=r+B+A,\qquad U_2=U_1+\ell,\qquad
Z_2=Z+d,\qquad K_2=(X_1+A)U_2.
\]

BもAも実reserveへ一度だけ入る。20%を更に別のseed残高へ二重計上しない。内部LP操作にも追加BILI burn・owner/contributor支払を課さない。公開user取引が内部operationと名乗って通常手数料を免れる入口は設けない。

### 3.4 整数演算と不正状態

全乗算、加算、差分、比較にprofileのchecked整数を使い、十分な中間幅を確保する。`ceilDiv(n,d)=n//d+(n%d!=0)`等、上方向丸めをoverflowなしに定義する。浮動小数点quote、独立rounding、負数、未知版、上限超過を受理しない。

ell>q、U1=0、U2>U、保存則違反が生じたなら、許された別の経済結果ではなく不正な計算又はstateである。式を変更して帳尻を合わせず、未commitのまま拒否する。

## 4. 保証する算術と、その限界

`0≤A≤B`、`1≤U1≤U`から、

\[
0\le\ell=\left\lfloor\frac{AU_1}{X_1}\right\rfloor
\le\left\lfloor\frac{BU}{X_1}\right\rfloor=q.
\]

したがってqを超えるPositionはLPへ要求せず、`q=ell+d`、`1≤U2≤U`である。`X1 U1≥XU`かつ`(X1+A)(U1+ell)≥X1 U1`からproductは減少しない。また`X1 ell≤A U1`により、LP追加直後の曲線価格は買付直後より低下しない。

この主張は**一回の内部複合遷移の算術**である。通常の売却後の市場価格、holderの利益、将来価格上昇、LPの現金価値、MEV不採算を保証しない。旧seed払出上限とvirtual reserveを実資金としない制約は継承する。旧price-floor式を使うconsumerは、増加後Kと減少後live supplyの双方を更新し、本kernelを含む証明・検査に従う。旧retire-allの数値や測定値をMOE2の検証結果として引用しない。

連続量での3:2から、整数LP量が常に購入量の2/3未満になるとは結論しない。例えば`X=100, U=60, R=10`ではB=3、A=2、U1=59、q=1、ell=1、d=0となる。**余剰が0ならburnは0**であり、追加burnを作るためにLP量やBILI配分を変えない。

詳細な前提、整数証明、検査結果は[数学記録](../maintenance/revisions/2026-11-20-permanent-liquidity/MATH_PROOF.md)へ分離する。数学kernelの検査がnative/EVM実装、経済均衡、流動性十分性を実証したことにはならない。

## 5. 1,000 BILIの数値例

比較用の初期状態を`V=0, r=1,000 BILI, U=10,000`、対象報酬をR=1,000 BILIとする。これは説明用状態であり、実networkのV、Position供給、最低報酬、通貨精度を変更する値ではない。

| 処理 | 連続量の参考 | 規範の整数Position |
| --- | ---: | ---: |
| 300 BILIで購入するq | 約2,307.6923 | 2,307 |
| 200 BILIと組み合わせるell | 約1,183.43195 | 1,183 |
| 余剰のburn d | 約1,124.26035 | 1,124 |
| 最終curve在庫 | 約8,875.73965 | 8,876 |

最終real reserveは1,500 BILIである。整数計算ではU1=7,693、K2=1,500×8,876。整数ellは連続量のellを丸めるのではなく、**整数U1を使って**`floor(200×7,693/1,300)=1,183`と求める。

残余D=500 BILIは同じclaimの版付きworker配分へ進む。例示の20% Panel pool版ならPanel=100、Miner=400で、300+200+100+400=1,000である。coreでは市場の二legはなく、Miner=1,000、RからのPanel控除=0となる。

## 6. 小額・保留・有限終端

以下は同一のprestateから全てを試算し、実際のreserve・Position・報酬を更新する前に判定する。

| 条件 | 処理 |
| --- | --- |
| B=A=0 | 市場二legはno-op。receiptを作らず、永久LP属性を設定しない。D=Rの元worker配分は残る |
| B>0かつq=0 | `LiquidityDust`。買付を含む全経済操作を未commitにする |
| A>0かつell=0 | `LiquidityDust`。買付・LP・burn・D支払の全てを未commitにする |
| A=0、B>0、q>0 | LP操作・receiptなし。q全量をburnし、Dを元planで配分する |
| A>0、ell>0、他の全条件成立 | 整数kernelを原子的にcommitし、永久LP receiptを記録する |

B=A=0を新しい無償採掘機会や最小仕事条件の免除にしない。元の成功条件、仕事量、資格と一回性を維持する。

LiquidityDust、許容quote外、slippage超過、予約資源不足等は、**元の有限決済期限内だけ**所定のhold・再評価に進める。再評価回数・順序は事前profileに従い、nonce、quote、別claimとして自由に引き直さない。原期限までに全条件を満たせなければ、そのclaimに適用される既定の未成功終端と元原資精算へ進む。未知のwire enumを本書だけで新設しない。

保留中にR、Miner/Panel成功報酬、EXEC credit、B、A又はburnを先にcommitしない。無期限Pending、期限reset、LP未成立のAをdonation扱いにする経路、Position mint、別fee版、Miner全額fallback、他claimとの無断poolingは認めない。既に提供された正当な検証・供給役務のfeeは、成功報酬とは別の元契約で精算する。

小額・高価格でこの条件を満たせない仕事があることは活性上の制約である。登録時に市場が存在することだけで、全てのRが成功決済できると主張しない。最小仕事量、上限、quote規則と期限の実装・測定が有効化に必要である。

特にU=1又は2では、A≤Bのまま報酬を増やしてもell>0にできない。まとめ払いだけで全状態のLP成立を保証できず、在庫を変える正規取引等が元期限内に生じなければ未成功終端となる。LiquidityDustそのものをMinerの計算不正又はslashの証拠にしない。これらの境界は[数学記録§8](../maintenance/revisions/2026-11-20-permanent-liquidity/MATH_PROOF.md)を参照する。

## 7. 永久LPの記録と権限

A>0かつell>0のLPを成功commitした場合に限り、nativeの`LockedLiquidityReceipt`を`PERMANENT_PROTOCOL_LOCK`へ記録する。これは非償還の認証履歴であり、通常addressへ送る流通LP tokenではない。

receiptはmodel ID、canonical artifact root、pair、MOE2、claimの一回決済キー、処理順序、B/A/q/ell/dを結合する。`settlement_id`はclaim/policy/model/pair等の固定文脈から事前決定し、処理後rootから循環的に導出しない。同じキーから重複receiptを発生させない。

前状態は`pre_market_state_ref`で参照し、後状態は明示した市場値又は`post_market_values_hash`で記録する。後者のhash入力はreceipt集合と自身のhashを除く規範market値に固定する。receipt自身を含むglobal post-state rootをreceipt内部へ格納しない。完成したreceiptは通常のnative state rootで認証する。wire field、ID生成codec、状態樹の配置は別途明示有効化する。

**withdraw、redeem、transfer、approve、fee請求、解除期限、owner/admin鍵、emergency unlockを設けない。** 公開LP追加・LP売買・LP rewardという別機能も追加しない。第三者へ裏付けとなる権利を渡すwrapperや、別市場への流動性移植でこの禁止を回避できない。

Aとellは既にreal reserveとcurve在庫へ入っている。receipt記載額を追加asset、別vault残高又は第四のPosition供給項へ数えない。`issued_supply=curve_units+holder_units+retired_units`を維持する。receiptは新しいholder、membership、model owner、consensus bond又は投票権を生まない。

receiptごとのA/ellと累積値は拠出履歴であり、現時点のBILI/Position残高、償還価値又は担保価値ではない。native/RPC/EVM/explorerも現在の市場残高と履歴累計を区別して表示する。

永久lockは撤去権の不存在であり、pool内の同じ数量のBILI・Positionが永久に静止する意味ではない。通常の適法なswapは動き、旧seed_lockedとVは別の意味を保つ。実LPのないA=0経路ではreceiptも永久LP属性も作らない。

canonical履歴上のLP成功により`hasPermanentLP(model_id)`と、そのcanonical artifact rootについての対応履歴を設定する。通常操作、owner変更、終了、pruningで消さない。reorgではreceipt、原state、属性を一つのcanonical遷移として復元し、孤立forkの履歴を正規stateへ混入させない。noncanonical遷移の全巻戻しは原子性の処理であり、canonical LPのwithdraw権ではない。pruning後も分類検査に必要な認証summaryを保持する。

成熟後の不正claimに責任精算が必要になっても、このreceiptを特別に引き出す権限を作らない。元のbond、liability hold、徴収可能責任を維持し、永久投入済みLPをslash原資として二重担保化しない。

## 8. core指定と旧cohortの移行

MAD2ではBFT YESでFORK_DESIGNATEDを追加できる。新forkはOption市場なし・Miner=Rを継承するが、BFT安全停止・正規解除の対象である。GENESIS_ORIGINALだけがBFT管理対象外であり、後付けGENESIS指定は認めない。BFT YESでも以下の永久LP保護・旧市場drain・既存停止の継承を免除しない。

**永久LPを一度でも成功記録したmodel IDを、GENESIS/FORK_DESIGNATEDへ変更してはならない。** MOE1の「旧市場をwinddownしてcoreへ指定する」経路は、永久LPを持たない旧cohortだけに限定する。

同一canonical artifact rootを別ID、別owner、別名、別catalog entryで再登録してcore指定を通すことも禁止する。model IDとrootの認証履歴の両方を照合する。LPをburn済みと表示する、receiptをpruneする、poolを停止する、後継へ移す等で属性を解除しない。異なる正規モデルを将来coreへ採用する場合も、旧市場・資産・receiptを自動移動しない。

永久LPのない旧cohortは、その署名・資金・配分・原期限でdrainする。core旧市場の残高、allowance、支払権利、役務義務を適法に処理し終えなければ該当移行を有効化しない。無同意の没収、未知価格の強制償還、新規発行による穴埋め、恒久的なcore sell-only市場を認めない。

旧MOE1で既にretireしたPositionは復活させない。旧50%買上げclaimを新30/20へ遡及変更せず、旧snapshotの義務を元条件で終える。MOE2の最初の有効化は、native/ABI/資金/旧cohort/receipt履歴が整合する明示境界で行う。

## 9. 決済原子性・順序・市場リスク

claim成功条件、成熟、hold、元予約、export枠を満たした一つのnative遷移で、B入金、q取得、A入金、ell返却、d burn、永久receipt、型制約、Dの支払権利、claim消費をcommitする。何か一つ失敗すれば全て戻す。EVM経由でも同じ正本・transaction journalを使い、内部の買付だけを外部注文へ分離しない。

市場操作順序とslotはcanonical profileで固定する。Miner又はownerが有利な位置へ動かしたり、二leg間に取引を入れたり、結果を見て片方を捨てたりできる実行経路を作らない。再送、別carrier/API、import、repair、reorg/restart/IBDでreceiptと市場入金を増幅させない。

B+Aのreserve流出も同じRのexport capへ合算する。Dの分配と別枠の発行にせず、元の発行・資本・hold条件を維持する。単にFinalというラベルが付いたことだけで、将来取消され得る報酬を先に永久市場投入してよいとはしない。

内部二leg間の介入を排除しても、前後の先回り売買、holderによる循環取引、quote操作、売却集中の経済問題は残る。minOut、amount上限、slippage、価格影響、元期限を実装・測定し、少額・最大値・敵対的順序を評価する。未測定の値を安全な本番定数へ昇格させず、MEVや全攻撃の不採算を解決済みとはしない。

## 10. 旧規則の置換と有効化条件

| 旧規則 | MOE2での置換 |
| --- | --- |
| RFC0063 §3/5、ADR0231 D3/5、ADR0091の50%取得・全量retire | 30%買付、20%BILI＋ellの永久LP、余剰dだけburn、Dは保存差分 |
| ADR0090のno-LP記述 | holderへ流通LP権利を与えない点は維持し、非償還native receiptを追加 |
| ADR0162のpre-trade seed限定 | 旧seedには継承。MOE2報酬LPは別操作として売買後も適用し、seed_lockedへ加算しない |
| 旧retire-allのfloor/product/供給試験 | 新kernelの再計算・証明・実装検査を要求。旧測定を新方式の証拠にしない |
| core移行前のwinddown | 永久LPのない旧cohortのみ。永久LP付きID及び同rootの再登録によるcore指定を禁止 |
| 通常AMM5%/3%、直接移転、core R100%、原資分離 | 継承。内部二legの追加BILI feeは0 |

有効化前に、分類・登録原子性、B/A/D端数、q/ell/d、LiquidityDust、minOut単位、上限・overflow、LP0の境界、receipt権限制限、同root迂回、旧cohort、全保存則、hold/export、元期限、二重決済、EVM revert、reorg/restart/IBD、quote・価格操作を確認する。既存doc checkerの成功又は小規模算術検査だけを、このgateの完了としない。

状態は**ADOPTED_DESIGN_NOT_ACTIVATED**のままである。TB1/FOW1、全実行検証、Court、反復費用検査、共通consensusの未完了条件を本経済改訂から免除しない。
