# ADR-0091 — 追加モデル報酬の30%購入・20%永久LP・余剰burn

- 改訂日: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。MOE2の設計改訂であり、native/EVM実装、測定、移行又は公開有効化の完了ではない。
- 共通正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md) / [ADR0232](0232-reward-buy-and-permanent-liquidity.md) / [RFC-v2-0011](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md)。
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。


## 1. 報酬の基準とsnapshot

Rは価格付けと既存発行枠を適用した、購入・LP・Panel配分前の認証済み対象worker報酬である。ネットワーク全体の報酬、別役務fee、通常AMM取引額、返還金とは区別する。

EconomicsSnapshotにはnativeのmodel ID/artifact root、catalog、GENESIS/FORK_DESIGNATED/ADDITIONAL分類、MOE2 policy、価格付け、reward plan、kernel版、pair、原資・原期限・予約を結合する。ADDITIONALのpairは登録時に同時生成した一意市場でNone不可、coreはNone。旧claimのMOE1 snapshotを新配分へ読み替えない。TB1 EXEC前の固定と整合させる。

## 2. 三つの原資とcoreの例外

ADDITIONALは最小BILI整数単位で次に分割する。

\[
B=\lfloor 300R/1000\rfloor,\qquad
A=\lfloor 200R/1000\rfloor,\qquad
D=R-B-A.
\]

BはPosition購入用、Aは購入Positionの必要分と組み合わせる永久LP用、Dは残余worker報酬である。端数はDへ入り、Dを別途floorして未帰属原資を作らない。AとBを追加の支払としてRの外側へ増発しない。Dは版付きreward planのMiner/Panel等へ配分する。

GENESIS/FORK_DESIGNATEDではB=A=0で、対象Rの100%をMinerへ帰属させる。Rからowner、Panel、Verifier、prefix worker、TB1役務等を控除しない。必要な全検証・資料・Court役務は、受理前に同意と資金を確保した別の有限escrowを使う。資金不足の新claimを受理しない。認証・maturity・hold・export上限・責任は維持する。

## 3. 同じ市場で購入してから永久LPへ入れる整数kernel

決済直前の実reserveをr、固定virtual reserveをV、curve在庫をUとする。`X=V+r>0`、`U>=1`を必要条件とし、十分な整数幅とchecked演算を使う。

\[
\begin{aligned}
X_1 &= X+B,\\
U_1 &= \left\lceil\frac{XU}{X_1}\right\rceil,\\
q &= U-U_1=\left\lfloor\frac{BU}{X_1}\right\rfloor,\\
\ell &= \left\lfloor\frac{A U_1}{X_1}\right\rfloor,\\
\operatorname{burn} &= q-\ell,\\
r_2 &= r+B+A,\\
U_2 &= U_1+\ell,\\
\operatorname{retired}_2 &= \operatorname{retired}+\operatorname{burn}.
\end{aligned}
\]

qはBで購入したPosition、ellは**購入後の**比率を用いてAと組み合わせる量、burnはLPに使用しなかった余剰である。A<=Bのためell<=qとなり、既存holderのPositionを補充に徴収せず、mintもretired復活もしない。整数丸めではell/qが連続近似の2/3を上回り得るため、「必ず購入量の1/3以上をburnする」とは主張しない。

Vと旧`seed_locked`は変えない。BとAはともに追加BILI burn・owner/contributor leg0%で全額を実reserveへ入れる。qを先に全量retireしない。ellはpool在庫へ戻し、q−ellだけをretiredへ入れる。BILIをburnする処理ではない。

同じnative遷移で購入→LP→余剰burnを計算し、途中に別swapを挟まない。受理時quoteではなく、規範順序の決済直前状態と署名済みkernel版を用いる。別model、別pool、別rootへの付替えは認めない。

## 4. 永久LPの意味と記録

A>0かつell>0の成功時に、同じ原子決済で`LockedLiquidityReceipt(PERMANENT_PROTOCOL_LOCK)`を記録し、当該modelの`hasPermanentLP`を立てる。receiptはclaim、model/artifact、pair、policy/kernel、拠出A/ell、購入q、余剰burn、元の認証状態と決済operationへ結合する。前状態参照と決済後market値又は`post_market_values_hash`を使い、receipt自身を含む`post_state_root`をreceipt内へ入れる循環を作らない。receipt全体は外側のnative state rootで認証する。wireのfield/tagは別の実装gateで固定する。

receiptは**拠出履歴**であり、新しい流動資産残高、引出せるLP token又は供給保存式の第四項ではない。withdraw、償還、transfer、fee請求、owner/admin鍵を設けず、公開LP入口や外部流通LP tokenも増やさない。

Aとellは、同じ市場の売買可能な実reserveと在庫へ一度だけ入る。後の通常swapでそれらが使用されることは、LP持分の引出しではない。特定のBILI又はPositionを売買でも永久固定する意味ではなく、Aを旧seed_lockedへ加算するものでもない。pool在庫やreceiptにmembership、holder配当、bond又は合意weightを与えない。

## 5. 小額・LiquidityDust・未成功

- B>0かつq=0、又はA>0かつell=0なら`LiquidityDust`とし、購入・reserve・LP・receipt・retired・Dの支払を部分commitしない。署名済みの元有限期限内だけ保留し、解消しなければ既定の未成功終端と原資精算へ進む。
- A=0ではLP操作、receipt生成、`hasPermanentLP`の新規設定を行わない。B>0でq>0なら購入qを全量burnする。
- B=A=0は市場の二つのlegだけno-opであり、元の最小仕事・成功条件を満たす限りD=Rの報酬配分は存続する。receiptを作らない。
- dustを理由にmint、retired復活、donation、owner/Minerへの原資振替、旧50%全burn、期限reset又は無期限待ちへfallbackしない。

MOE2の追加登録はmodelと一意市場を同じnative遷移で作る。初期実reserve0を許し、外部seed・後続open・市場準備待ちを必要としない。登録済み追加モデルの市場欠落はstate invariant違反として拒否し、LiquidityDustや正常なclaim終端の一種と混ぜない。

新規user buy停止は既受理の購入・LP・余剰burn義務を消さない。元の決済資源・役務予算・原期限を保持し、成功条件を満たせなければ既定の未成功終端へ進む。成功R/creditだけの先行発行は認めない。

## 6. 一回決済と保存

全実行検証・異議・資料・成熟・hold/export条件を満たしてから、B/A、q/ell/burn、reserve、在庫、receipt、sticky属性、Dのpayee、claim消費と支払権利を原子的に確定する。void/未検証Abortに成功Rを発行しない。提供済み役務feeは元の別契約で精算する。

BとAの両方をRの外部exportへ合算し、Miner payoutと同じ総上限・maturity・hold・取消条件へ接続する。LPだからcap外としない。後日の不正責任は元のbond等の契約に従い、永久LPからの特例引出しを制裁・補填手段にしない。noncanonical reorgでは市場・receipt・sticky属性・claim配分・消費を含む原子遷移全体を選択履歴へ戻す。これは正規履歴上のLP解約とは異なり、restart/IBDも同じ選択状態を復元する。

\[
R=B+A+D,\qquad
U+\sum\operatorname{holder}+\operatorname{retired}
=\operatorname{registered\_supply}.
\]

LP receiptを加えて供給を二重計上せず、burn済み単位を別wrapperで復活させない。通常holder間transferはreserve、receipt、retiredを変えない。receiptの詳細をpruneする場合も、永久LP既記録という認証された属性を削除しない。

## 7. 金額とwhole-unitの例

| 分類・plan | R | 購入B | LP用BILI A | 残余D | Panel pool | Miner |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ADDITIONAL、他worker legなし・残余20%poolの版 | 1,000 | 300 | 200 | 500 | 100 | 400 |
| GENESIS | 1,000 | 0 | 0 | 1,000 | Rから0 | 1,000 |
| FORK_DESIGNATED | 1,000 | 0 | 0 | 1,000 | Rから0 | 1,000 |

**Rの20%を使うLPと、Dの20%を使うPanel poolは別の項目**である。20%poolはその版の例だけであり、全profileへ強制しない。

whole-unit例は`V=0,r=X=1000,U=10000,R=1000`とする。B=300、A=200、U1=7693、q=2307、ell=1183、余剰burn=1124、r2=1500、U2=8876となる。実際の通貨表示と整数最小単位はprofileに従う。

連続量の2307.69購入・1183.43LP・1124.26burnは比較用近似にすぎず、合意値ではない。また`X=100,U=60,R=10`ではB=3、A=2、q=1、ell=1、burn=0が正常に成立する。LPを必ず購入量の2/3以下とする整数規則は採用しない。

## 8. 移行・表示・受入

実際に永久LPを記録したmodel IDはGENESIS/FORK_DESIGNATEDへ指定不可とする。winddown解除、owner/admin回収、後継移植、同rootの別ID再登録、receipt削除を例外にしない。永久LP未記録モデル又は既存core市場の移行は、元の権利・claim・残高・allowanceを明示winddownしてから行う。旧core sell-only継続、自動没収、無原資補償は認めない。

RPC/CLI/explorerはB、A、D、q、ell、余剰burn、retired、receipt、hasPermanentLP、LiquidityDustと元期限、Panel別原資を区別する。A/ellの累積は拠出履歴であり、現在reserve残高又は担保価値ではないと表示する。「50%を全額買ってburn」「LP receiptは売れる資産」「価格上昇保証」と表示しない。

受入には丸め、overflow、極小値、zero/no-op、dust期限、model/pair不一致、swap介在拒否、原子rollback、B+Aのexport合算、旧claim境界、永久LPの権限不存在、core指定・同root迂回拒否、reorg/IBDを含める。先回り・循環売買・価格操作が新配分だけで解消されたとはしない。実装・測定・公開activationは未完了である。
