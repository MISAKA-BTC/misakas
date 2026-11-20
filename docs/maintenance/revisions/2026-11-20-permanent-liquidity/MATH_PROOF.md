# MOE2の30%購入・20%永久流動性・残余配分：整数算術の独立検証

- 対象: 2026-11-20に指定されたMOE2の算術と参照状態遷移。
- 状態: **設計に対する数学的検証。native実装・公開activationの完了ではない。**
- 再現コード: [check_liquidity_math.py](check_liquidity_math.py)。結果: [liquidity-math-check.json](liquidity-math-check.json)。Python標準ライブラリのみを使用する。
- 継承元: [ADR0162のvirtual reserve・whole Position・ceil売買](../../../adr/0162-the-pair-opens-on-a-virtual-reserve.md)、[共通会計と原資・責任の境界](../../../rfc-v2/0011-bond-budgets-fees-and-payments.md)。本資料は旧資料・旧試験結果を書き換えない。

## 1. 結論と証明範囲

指定された式では、購入したPositionだけからLPへ戻す必要量を常に確保できる。整数丸めを含めて`0 ≤ ell ≤ q`が成立し、追加mint又は別市場での買い足しは不要である。余剰だけをretireすると、BILIとPositionの保存則、既存seed floor、非減少のcurve productを維持できる。

ただし、**正の20%原資から、常に正のwhole PositionをLPへ戻せるわけではない。** この場合は指定された`LiquidityDust`として市場・支払・receiptへの副作用を残さず、元の有限期限内だけ再決済の機会を持つ。期限までに成立しなければ未成功終端とし、成功worker報酬・creditを先行発行しない。

証明対象はこの算術と状態遷移であり、全状態での正常完了、将来の市場価格、外部価格、MEV耐性、採掘採算、LLMの計算費用下界、DAG全体の安全性は証明しない。

## 2. 記号と前提

全金額は最小通貨単位の非負整数、Positionはwhole単位とする。

| 記号 | 意味 |
| --- | --- |
| `V` | 当該市場に固定されたvirtual reserve。実BILIではない |
| `r` | 現在の実BILI reserve |
| `s` | 既存の`seed_locked`。通常sellでも下回れない旧seed floor |
| `X = V + r > 0` | 曲線計算に使うBILI側の量 |
| `U ≥ 1` | 曲線に残るPosition在庫 |
| `H ≥ 0` | 全holderのPosition合計 |
| `Z ≥ 0` | 累積retired Position |
| `S` | 登録済みの固定会計供給量。`U + H + Z = S` |
| `T = S − Z = U + H` | 未retireの総量。**retired量そのものではない** |
| `R ≥ 0` | 価格付け・共通上限処理後、配分前の対象worker報酬 |

開始時に`r ≥ s ≥ 0`、`U ≥ 1`、供給保存則が成立しているものとする。これらを満たさないstateを計算式へ代入して正当化しない。

次の配分を一度だけ計算する。

\[
B=\left\lfloor\frac{300R}{1000}\right\rfloor,\qquad
A=\left\lfloor\frac{200R}{1000}\right\rfloor,\qquad
D=R-B-A.
\]

`B`がPosition購入、`A`が永久流動性へのBILI拠出、`D`が版付きworker planへ渡る残余である。名目比率は30%・20%・50%で、整数残差はDへ入る。`D`をさらに`floor(500R/1000)`と計算して未帰属金を作らない。

同一のnative原子遷移内で、現在のcanonical市場状態から次を計算する。

\[
\begin{aligned}
X_1&=X+B,\\
U_1&=\left\lceil\frac{XU}{X_1}\right\rceil,\\
q&=U-U_1,\\
\ell&=\left\lfloor\frac{A U_1}{X_1}\right\rfloor,\\
\beta&=q-\ell,\\
r_2&=r+B+A,\\
U_2&=U_1+\ell,\\
Z_2&=Z+\beta,\qquad T_2=T-\beta,\\
K_2&=(X_1+A)(U_1+\ell).
\end{aligned}
\]

`q`は内部購入で一時的に取り出す量、`ell`は同じ曲線のswap可能在庫へ戻す量、`beta`だけが永久retireとなる。途中のqをいったん全量retireしてから復活させる処理ではない。

## 3. 定理1：購入分でLPへのPosition返却を常に賄える

**主張。** `X > 0`、整数`U ≥ 1`、整数`R ≥ 0`の全てについて、

\[
0\le\ell\le q\le U-1.
\]

**証明。** floorの単調性から`0 ≤ A ≤ B`である。また、`X/(X+B) ≤ 1`かつ`X > 0`なので、`1 ≤ U1 ≤ U`。

整数Uについて、`U − ceil(z) = floor(U − z)`だから、

\[
q
=U-\left\lceil\frac{XU}{X+B}\right\rceil
=\left\lfloor\frac{BU}{X+B}\right\rfloor.
\]

従って、

\[
\ell
=\left\lfloor\frac{A U_1}{X+B}\right\rfloor
\le\left\lfloor\frac{B U}{X+B}\right\rfloor
=q.
\]

非負性と`q ≤ U−1`は`1 ≤ U1 ≤ U`から従う。よって`beta ≥ 0`かつ`1 ≤ U2 = U−beta ≤ U`。□

この証明はA/Bを厳密な2/3とは仮定しない。小額の整数丸めでは、例えばR=5最小単位に対しB=A=1となる。それでも`A ≤ B`があるため成立する。

## 4. 購入量の単純な2/3を戻す規則ではない

丸めを除いた連続式では、

\[
q=\frac{BU}{X+B},\qquad
U_1=\frac{XU}{X+B},\qquad
\frac{\ell}{q}=\frac AB\frac{X}{X+B}.
\]

名目A/B=2/3なら、`ell/q = (2/3) X/(X+B)`となる。購入によって価格が変わるため、必要なPosition量は購入qの単純な2/3ではない。

整数反例は`X=200, U=50, R=100`である。B=30、A=20、X1=230、U1=44、q=6。規定式は`ell=floor(20×44/230)=3`だが、`floor(2q/3)=4`となる。後者を使うとLP前の価格230/44よりLP後の250/48が低くなり、次節の丸め条件を満たさない。

## 5. 定理2：LP部分の価格変化は丸め誤差で評価できる

LP投入直前の価格を`p1=X1/U1`、投入後を`p2=(X1+A)/(U1+ell)`とする。floorの余りを

\[
e=A U_1-\ell X_1
\]

と置けば、`0 ≤ e < X1`。従って相対変化は厳密に、

\[
\delta=\frac{p_2}{p_1}-1
=\frac{A U_1-\ell X_1}{X_1(U_1+\ell)},
\qquad
0\le\delta<\frac{1}{U_1+\ell}.
\]

LP部分は同じ比率での連続投入に対応し、whole丸めはBILI側への小さな偏りを残す。これは**LP投入部分の直前・直後だけ**の関係である。30%購入による価格変化や、その後のuser売買までゼロ変化とする定理ではない。

## 6. 定理3：product、供給、旧floorは維持される

### 6.1 Product

購入後はceilの定義から、

\[
K_1=X_1U_1\ge XU=K.
\]

Aとellが非負なので、

\[
K_2=(X_1+A)(U_1+\ell)\ge X_1U_1=K_1\ge K.
\]

LP投入後も古いKを固定値として持ち続けてはいけない。次のswapは新しい行の`K2=X2 U2`から計算する。

### 6.2 Position保存則

holder残高Hは内部処理で変わらない。従って、

\[
U_2+H+Z_2
=(U-q+\ell)+H+(Z+q-\ell)
=U+H+Z=S.
\]

LPへ戻したellを、curve在庫に加えながら別のfrozen Position残高にも加算してはいけない。`LockedLiquidityReceipt`は拠出履歴であり、上式に第四の資産項を追加しない。

### 6.3 未retire総量とfloor

`T2 = T−beta`、`beta ≥ 0`、`T2 = U2+H ≥ 1`である。旧定義と同じ関数

\[
f(K,T)=\frac{\lceil K/T\rceil}{T}
\]

を考えると、`K2 ≥ K`かつ`T2 ≤ T`から、`ceil(K2/T2) ≥ ceil(K/T)`。分母も正で小さくなるので、

\[
f(K_2,T_2)\ge f(K,T).
\]

よって規定の全原子遷移ではこのfloorが下がらない。これは固定virtual reserveを含む**曲線の算術上のfloor**であり、外部市場価格、全保有者への固定価格償還、ドル価値、将来収益を保証しない。

## 7. BILI保存と旧seed_locked

報酬に関する一回の保存則は、

\[
R=B+A+D,\qquad r_2-r=B+A.
\]

購入・LP両legに通常AMMの5%/3%を追加徴収しない。Dのworker内部配分、成功資格、発行上限、成熟、hold/exportは元の契約で別に確認する。外部へ流れる市場原資はBだけでなく**B+A**である。

旧seed `s`は変えない。成功時は`r2 ≥ r ≥ s`。将来の通常sellには引き続き`gross ≤ r−s`を要求する。virtual reserveを現金と扱わず、seedを払出原資にしない。

**永久LPロックはwithdrawal請求権の永久廃止であり、Aと同額のBILIが永遠にpool内に残るというcash floorではない。** Aを`seed_locked`へ加えると、通常swapにも別の制約を加えることになる。

### 誤ったseed加算の反例

| 段階 | 状態又は計算 |
| --- | --- |
| 初期状態 | V=100、旧seed=10、S=U=1000、H=0 |
| userがnet 2090を買い投入 | X=2200、r=2100、U=50、H=950 |
| R=1000の規定処理 | B=300、A=200、U1=44、q=6、ell=3、burn=3 |
| 処理後 | r2=2600、X2=2700、U2=47、K2=126900 |
| holderが950を通常売却 | X3=ceil(126900/997)=128、gross=2572、残る実reserve=28 |

残る28は旧seed10以上なので有効である。一方、seedを10+200へ増やす誤実装だけが、この同じsellを拒否する。LP receiptに記録したA=200は拠出時の金額であり、現在のreserve残高が200以上であるという意味ではない。

## 8. LiquidityDustと有限な終端

次のどちらかなら、計算結果を市場へcommitしない。

\[
(B>0\land q=0)\quad\lor\quad(A>0\land\ell=0).
\]

この`LiquidityDust`で、Bの購入だけ、Aの寄付だけ、先行burn、先行D支払、LP receipt発行、成功R/credit発行を残さない。30%と20%を含む処理全体が同じ原子的成功又は失敗に属する。途中の状態へ他のswapを挿入できない。

元の有限決済期限と必要な予約を維持し、その範囲内で市場状態が変われば、同じ受理契約について現在のcanonical状態から再計算できる。再試行で期限をresetせず、成功を二回計上しない。期限まで成立しなければ、元規則の未成功終端・原資精算へ進む。正当に発生した別契約役務費の扱いは元の契約に従い、dustだけを計算不正の証拠にしない。

**B=A=0の場合は市場二legがno-opである。** R=1〜3最小単位ならD=Rが残るため、worker配分まで消えるわけではない。R=0ならDも0である。既存の最小仕事条件や成功資格をこの端数規則で免除しない。

### 全状態でLPが成立するという定理はない

- `X=100,U=10,R=10`ではB=3、A=2だがq=ell=0。
- `X=100,U=10,R=40`ではq=1だがell=0。
- **U=1又は2なら、どれほど大きな有限Rでもell>0は不可能である。** U=1ではq=0。U=2ではqが0ならell≤q=0、qが1ならU1=1なので`ell=floor(A/(X+B))=0`である。

最後の反例は報酬をまとめてもA≤Bのまま成立する。従って「batchすれば必ず解決する」とは主張できない。本profileはこの場合の無期限滞留を許さず、有限な未成功終端を持つ。正しいLLM実行であっても市場決済条件を満たせない可能性は残り、LP決済の活性を別途評価する必要がある。

## 9. 永久receiptと分類変更

成功しA>0なら、Aとellをswap可能poolへ一度だけ入れ、非償還のnative `LockedLiquidityReceipt`を同じ原子遷移で永久sinkへ記録する。流通LP token、owner、transfer、withdrawal、fee請求権を作らない。拠出履歴を現在残高又はPosition供給として数えない。

`hasPermanentLP`が成立した登録identityを、後からcoreへ指定する経路は認めない。同じrootを別identityで登録して、この制約を迂回する経路も認めない。これは通常の追加モデルからcoreへのwinddown条件に、永久LPが存在する場合の明示的な制限を加えるものであり、旧seedの返還権やreceiptの償還権を新設しない。

参照fixtureはこのID/root拘束とreceiptの非償還を検査するが、分散台帳全体の認証・重複root検出・IBDの実装完了を意味しない。

## 10. 再現検査と限界

次で実行する。

```bash
python docs/maintenance/revisions/2026-11-20-permanent-liquidity/check_liquidity_math.py
```

今回の結果は次のとおりである。

| 検査 | 件数・結果 |
| --- | --- |
| X=1〜40、U=1〜40、R=0〜250の全組合せ | 401,600件、PASS |
| 1〜256bitを含む決定論的な広幅整数入力 | 10,000件、PASS |
| buy・sell・reward・transfer・期限切れの混合sequence | 64系列×256操作＝16,384操作、PASS |
| dust、元期限、再試行、二重決済、非償還、alias、overflow、undo/replay | 参照状態でPASS |
| seed_locked加算と単純2/3返却の反例 | 反例を再現 |

各成功操作でBILI保存、Position保存、旧seed、product、receipt非供給を確認し、拒否操作は全経済状態が不変であることを確認した。局所undo/replayは同じnative原子状態を一括復元する参照検査であり、DAGの分断・合意確定・外部支払まで含む証明ではない。

有限probeは全入力についての証明を置き換えない。全整数に対する主張は上記代数式から導いている。またPythonは任意精度整数なので、nativeでは`300R`、`200R`、`XU`、`A U1`、`X2 U2`、ceil計算、残高・供給の全中間値が指定幅へ収まるかを別途証明・検査し、overflowをwrapせず原子的に拒否する必要がある。

本算術は、認証済みの正しいclaim・必要な原資・役務・成熟・export枠が別途そろう前提で呼ばれる。これらのgate、runtime/EVM、登録・分類変更、永久receipt保存、容量、MEV、経済的実用性の完成を、算術PASSだけから宣言しない。
