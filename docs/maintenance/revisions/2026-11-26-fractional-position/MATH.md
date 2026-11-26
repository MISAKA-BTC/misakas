# MOE3参照算術 — 小数POSITIONと50/20/20/5/5

2026-11-26（Asia/Tokyo）。最終指示は総対象coinbase Cの**50%をMiner向けPOSITION購入原資、20%をLP向けPOSITION購入原資、20%をMISAKA側LP、5%をPanel/Gate C合計MISAKA、5%をBFT MISAKA**へ配分する。途中の80/20、40% Miner・15%検証、BFT10%の案は新規profileの正本にしない。

正本は [RFC0068](../../../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../../../adr/0236-fractional-position-miner-rewards.md)。[参照実装](moe3_reference.py)、[検査](test_moe3_reference.py)、[結果JSON](math-check.json)は設計の検査であり、native実装・公開有効化・既存private MOE2の置換ではない。廃止とCourtの49%発見報酬は [別の参照検査](RETIREMENT_MATH.md)を参照する。

## 1. 単位と範囲

POSITIONは小数8桁、`SCALE=100,000,000` atomsで1 POSITIONとする。MISAKAも最小通貨単位の整数で保持する。表示例は1 MISAKA=100,000,000通貨atomsで換算する。NFT/model IDは離散識別子のままで、数量だけを小数化する。

金額・残高はchecked unsigned128、二因子の積はchecked unsigned256相当。`V+r`、`X+B`、reserve増分、供給合計も検査する。ceil除算は商と剰余で計算し、`n+d-1`の中間overflowを避ける。Pythonの任意精度整数を理由にnative上限を省略しない。

外部小数入力とJSON atomsは文字列で扱う。ASCIIの非負10進数、先頭0のない整数部、小数1〜8桁だけを受理する。float、負数、指数表記、空白、桁区切り、NaN/Infinity、9桁以上の小数を拒否し、JSON numberへ残高を変換しない。

全モデルは同じ経済式、係数1.0。genesis、旧fork、自動採用、BFT採用の免除分岐はなく、creator有無は通常取引feeだけに使う。参照登録の既定供給は従来の500,000 POSITION＝50,000,000,000,000 atoms。実際のモデル選択、登録認証、特別cap撤去のnative実装は検査対象外である。

## 2. Cを一回だけ配分する

Cは正規な総対象coinbaseのMISAKA atoms。同じCを複数モデルやcarrierごとに再発行・再分割しない。

```text
F = floor( 50 C / 1000)       BFT MISAKA
P = floor( 50 C / 1000)       Panel/Gate C合計 MISAKA
A = floor(200 C / 1000)       LPのMISAKA
J = floor(200 C / 1000)       LP用POSITIONの購入原資
D = C-F-P-A-J                Miner用POSITIONの購入原資
B = D+J                     一回のPOSITION購入原資
```

D/J/A/P/Fは名目50/20/20/5/5、Bは70%。整数端数は保存差分Dへ入る。`C=F+P+A+J+D=F+P+A+B`が厳密に成立し、BFT分をさらに控除しない。P内部のPanel/Gate C配分は正規planで指定し、5%ずつにも、無断の半分ずつにも分けない。必要な発行・資格・成熟・役務成立・責任条件は元の契約から継承する。

## 3. POSITION購入、LP、Miner

Vをvirtual reserve、rを実reserve、sを旧locked seed、Uを曲線内POSITION atoms、Hをholder合計、Zを過去retiredとする。

```text
X=V+r>0, r>=s>=0, U>=1, issued_supply=U+H+Z
X1 = X+B
U1 = ceil(X*U/X1)
q  = U-U1 = floor(B*U/X1)
L  = floor(q*J/B)            B>0のとき
M  = q-L
r2 = r+B+A
U2 = U1+L = U-M
H2 = H+M
Z2 = Z
```

一回購入したqを**実購入原資J/B**でLPへ配分する。Cが20 atomsの倍数ならJ/B=2/7で、Minerは残り5/7。端数がある状態へ2/7を強制しない。C=19 atomsならF=P=0、A=J=3、D=13、B=16なのでL=`floor(3q/16)`となる。

内部購入、LP、Miner配布は通常AMM fee0。購入数量の余剰burnはなく、過去Zは復活させない。rの増分はB+Aであり、P/Fは市場へ入れず別のMISAKA支払権として保つ。minOutはLP控除後の`min_miner_position_atoms`に適用し、総購入qも期待quote・上限・認証から省略しない。

## 4. 保存則と積

通貨側は`C=F+P+B+A`、POSITION側は`q=L+M`であり、

```text
U2+H2+Z2 = U1+L+H+(q-L)+Z = U+H+Z
```

が成立する。追加mintは不要。receiptは拠出履歴であり、Lを曲線へ戻した後で第四の供給項や別残高に足さない。履歴累計は現在reserve・償還価値ではない。

整数Uについて`U-ceil(XU/(X+B))=floor(BU/(X+B))`が成立する。検査は後者を独立式として用い、実装側のceil式と照合する。ceilから`X1*U1>=X*U`、A/L非負から`(X1+A)*(U1+L)>=X1*U1`。curve productは非減少、Vと旧sは不変である。将来価格、償還、MEV、利益の保証ではない。

`q*J=B*L+rem`なら`0<=rem<B`かつ`B*M=D*q+rem`。除算余りはMinerへ含め、POSITION atomsを未帰属にしない。

## 5. 複数モデルの結合配分

F/Pをglobalに一回確保した後、認証された同一weightsとcanonical model ID順を使う。`apportion(N, weights)`はfloor割当後の残額を剰余降順、同率はcanonical ID昇順で配る最大剰余法。

```text
J_i = apportion(J, weights)_i
A_i = J_i
E   = D-2J
E_i = apportion(E, weights)_i
D_i = 2J_i+E_i
B_i = 3J_i+E_i
```

最終率のEは名目10%Cと端数であり、0〜5 atomsではない。各global legの総和を保ち、`J_i>0`なら`D_i>=2J_i`を保証する。Dの理想比例割当に対する誤差はJの丸め2回分とEの丸め1回分から最大約3 atomsになり得る。すべてのlegが1 atom以内とは主張しない。

C=21 atoms、weights=(1,4,4,4)の場合、JとDを独立に配るとmodel aはJ=1/D=1となり、上の局所下限を満たさない。結合方式はJ=(1,1,1,1)、D=(2,3,3,3)。列挙順を逆にしてもcanonical IDで同じ結果になる。

参照関数はこの配分額と総和を検査する。`ReferenceBook`の市場処理は単一モデル用であり、複数モデル配分を同じnative発行予約・市場順序へ接続する実装は未完。市場予算が全0のmodelはno-opで、C全体の正値だけからそのmodelをdustにしない。正の市場割当があってもPOSITION dust等は残り、全状態の成功を保証しない。

## 6. 予約と市場決済を分ける

`allocate_budget`はCを一回配分し、成立済みP/FのMISAKA権利と市場原資B+Aを別予約する。Pの正規planが未指定なら総額の予約までとし、Panel/Gate C比率を作らない。明示planは許可role、受取先、総和Pを検査する。署名・役務成立の検証は対象外。

`settle_market`は購入、LP、Miner、receipt、claim一回消費、市場原資消費を原子的に処理する。market dust、quote不成立、後段失敗は先行P/F予約を巻き戻さない。廃止でも既得の正当な検証・BFT支払権を市場原資と混ぜてburnしない。

便宜関数`settle`は未公開入力から成功経路をまとめて投影する。既に成立したP/Fがある処理は二段階APIを使う。C=0はno-op。正の市場予算でB/A/J/D/q/L/Mの必要量が0なら市場を未commitにする。元期限内でのみ再評価し、無断pooling、期限reset、mint、Miner直接MISAKA fallbackを使わない。

局所undoは現在状態と一致する最後の遷移だけを巻き戻し、reserve、holder、receipt、消費済みID、市場予約消費、永久LP属性を一括復元する。DAG finality、永続化、RPC/EVM、restart/IBDの試験ではない。

## 7. 固定例と価格の反例

初期X=1,000 MISAKA、U=10,000 POSITION、C=1,000 MISAKA、V=s=0とする。

| 項目 | 数量 |
| --- | ---: |
| F：BFT | 50 MISAKA |
| P：Panel/Gate C合計 | 50 MISAKA |
| A：LPのMISAKA | 200 MISAKA |
| J：LP用購入原資 | 200 MISAKA |
| D：Miner用購入原資 | 500 MISAKA |
| B：総購入原資 | 700 MISAKA |
| q：総購入 | 4,117.64705882 POSITION |
| L：LPへ戻す | 1,176.47058823 POSITION |
| M：Miner | 2,941.17647059 POSITION |
| 最終実reserve | 1,900 MISAKA |
| 最終POSITION在庫 | 7,058.82352941 POSITION |

買付後spotは約0.289 MISAKA/POSITION、LP後は約0.2691667。価格中立にする旧`floor(A*U1/X1)`は692.04152249 POSITIONで、今回のLとは異なる。一般にLP前後spot差の符号は`A*U1-X1*L`で決まり、0に固定されない。

Miner50%相当は購入原資Dの名目であり、時価・換金保証ではない。この例でMを直後に全売却すると仮定すればgross558.82352941 MISAKA、creatorありのfee後525.29411765 MISAKAで、名目原資500とは一致しない。将来の売却価格を予測する例ではない。

## 8. 通常feeと移行

通常gross Gに対し`burn=floor(50G/1000)`、creatorありなら`creator=floor(10G/1000)`、なしなら0、`net=G-burn-creator`。名目5/1/94又は5/0/95。creator不在の1%をburnへ足して6%にしない。contributorはcreator総枠に含める。内部報酬購入・LP・Miner配布はfee0。

旧whole市場の在庫・holder・retired・issuedはcheckedな`old*SCALE`へ一回換算し、MISAKA、V、旧seedは変えない。旧`X/U_whole`と新`X*SCALE/U_atoms`は同じ単位価格。旧pending claim、生存allowance、未知profile、再移行、overflowは拒否し、署名数量を黙って拡大しない。市場のないgenesisは同じ新規市場を生成し、架空の旧残高を換算しない。認証移行・receipt単位・wire/署名互換・資産責任保存はnative統合で必要。

## 9. 検査結果と限界

```sh
PYTHONDONTWRITEBYTECODE=1 python3 docs/maintenance/revisions/2026-11-26-fractional-position/check_moe3.py
```

- 26 unittest：最終率、実原資J/B、端数、P/F独立予約、minOut、creator、codec、u128/u256、原子性、undo、移行、価格反例、結合配分。
- X=1..12、U=1..20 atoms、C=0..32 atomsの7,920組：成功4,325、dust3,355、0 no-op240。
- seed 20261126の600組：virtual/real reserve、seed、holder、retiredを変えて独立式・保存則を確認。
- C=0..1000で結合配分・総和・局所下限・zero-weightを確認。

有限試験を全入力の機械証明、native有効化、全状態の活性、MEV、不正の不採算、価格保証としない。`q*X*10000`等の三因子比較はu128入力だけでu256へ収まるとは限らず、checked u512又は厳密な比較分解が必要。本参照にbps評価器は含めない。
