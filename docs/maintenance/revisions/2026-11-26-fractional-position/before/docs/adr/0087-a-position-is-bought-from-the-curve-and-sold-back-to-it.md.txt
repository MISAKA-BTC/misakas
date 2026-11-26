# ADR-0087 — 追加モデルのMisakaOption市場・送金・保存則

- 改訂日: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。MOE2の設計改訂であり、native/EVM実装、測定、移行又は公開有効化の完了ではない。
- 共通正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md) / [ADR0232](0232-reward-buy-and-permanent-liquidity.md) / [RFC-v2-0011](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md)。
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

<a id="0-a-position-is-not-a-share--it-is-how-a-models-usefulness-is-held"></a>

## 1. 対象と権利

MisakaOption / ModelPositionは、固定model IDとartifact rootに結合した**追加モデル専用の残高資産**である。nativeに認証されたEconomicsSnapshotの分類が`ADDITIONAL`であるモデルだけが対象となる。`GENESIS`及び`FORK_DESIGNATED`（有効なnative年次fork指定。softforkを含む）は、市場、Position発行・取得・送受信、承認、LP入口、自動買上げを有効にしない。モデル名・自己申告・類似weights・別APIから分類を選べない。

PositionはBILI、native UTXO、miner bond、Panel seat、EXEC creditとは別型である。保有だけによるマイニング報酬の受領権、配当、合意weight、登録owner権を与えない。登録済みのmembership機能は[ADR0095](0095-a-position-is-a-membership-not-an-income.md)に従う。市場が存在することはモデルの正しさや入手性の証明ではなく、価格・収益を保証しない。

モデル登録は不変である。[ADR0175](0175-registered-models-are-permanently-immutable.md)に従い、同じ市場を別のartifact、改善版又は後継modelへ付け替えない。モデル改善・画像動画等の延期された経路をこの改訂で有効にしない。

## 2. 決定

### Decision 1 — 登録と市場生成を一つのnative遷移にする

MOE2有効下の追加登録は、model登録、一意のpair identity、初期virtual reserve・実reserve・Position在庫・owner参照を同じnative遷移で生成する。`ModelRegistered(m, ADDITIONAL) ⇒ ∃! MarketFor(m)`を不変条件とする。部分失敗なら全体をrollbackし、reorg/IBDでも片方だけを残さない。実reserveの初期値は実入金に限り、外部seedがなければ0でよい。別市場作成、後続open、seed/deepen又はmarket-ready待機を必要条件にしない。seed/deepenは生成済み市場への任意増強である。GENESIS/FORK_DESIGNATEDでは市場を生成しない。

市場、holder残高、lock、allowance、retired units、永久流動性のreceipt、`hasPermanentLP`、既処理operation、決済・支払権利を同じnative認証状態へ結合する。EVM facadeは別の発行残高を持たない。holderは認証domain付きidentityであり、旧payloadとkey IDを混同しない。送信権限とBILI受取先は、それぞれ認証された規則から決める。

供給単位・上限・virtual reserve・locked seedは、その市場が使う登録済みcurve profileへ固定する。今回、旧の100,000分割単位と後続の500,000 whole-unitを混在させず、[ADR0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)及び[ADR0162](0162-the-pair-opens-on-a-virtual-reserve.md)の適用版を明示する。新wire数値をこの文書番号から推定しない。

### Decision 2 — AMMと直接転送を区別する

通常AMMのcurve profile、整数丸め、real reserveの支払可能範囲、旧`seed_locked`は維持する。virtual reserveは価格計算用で、引き出せるBILIを増やさない。報酬由来の永久LPは同じ市場の実reserveとPosition在庫を増やすnative操作として追加する。外部流通LP token、利用者の公開add/remove-liquidity入口又は任意pool生成権を追加する変更ではない。

永久LP成功時は`LockedLiquidityReceipt(PERMANENT_PROTOCOL_LOCK)`を同じ原子的遷移で記録し、withdraw、transfer、償還、fee請求の権利を作らず、owner/admin鍵も持たせない。receiptは拠出履歴であり、新しい流動資産残高でもPosition供給の第四項でもない。拠出したBILIとPositionは売買可能なreserve/在庫へ一度だけ計上する。通常swapはそれらを使用できるため、「永久」は個々のBILI又はPositionが売買でも固定される意味ではない。20%BILIを旧seedの引出し下限へ加算せず、別口座で二重計上しない。

直接転送はholder Aからholder Bへ同じPositionを動かす。AMMの売却・再購入を合成せず、curve reserve、curve在庫、retired units、総供給を変えない。外部でBILI等の対価を払って直接譲渡する取引を、単なる送金と自動判別することはできない。通常AMM手数料はprotocol AMMにのみ適用する。

### Decision 3 — 許可される操作

通常のbuy/sellに加えて、native transfer、直接受取、譲渡、allowanceの設定・読出し、権限に基づくtransferFromを許可する。native/EVMの入口は同じ適格性・available balance・操作順・再送防止を使用する。EVMの具体的な同期契約は[ADR0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)が所有する。

正のtransfer量は送信者の`available = balance − locked − reserved`以下でなければならない。allowanceは残高をlockせず、残高を増やさず、別spenderへ再委任する権利を自動で作らない。transferFromは実行時に権限、available balance、allowanceを検査し、成功した移転と同じ原子的遷移でallowanceを消費する。既に拘束されたPositionを別入口へ移して売却可能にしない。

同一network内のtyped recipientへ移す。namespaceを黙って同一人物として合算したり、別chainへ残高を複製したりしない。受信者がcontractであることだけを理由に拒否しない。新しいaddress/profileの対応が未実装なら明示的に拒否し、別namespaceへfallbackしない。

### Decision 4 — 通常AMMの5%・3%・残差

AMMで扱うgross BILI額を`G`（最小整数単位）とする。

\[
B=\lfloor 50G/1000\rfloor,\quad
O=\lfloor 30G/1000\rfloor,\quad
N=G-B-O.
\]

buyは`N`をreserveへ入れる。sellはcurveから得たgrossを同じ式で分け、`N`を売却者へ支払う。burnは5%、owner総legは3%、netは名目92%であり、整数残差はnetへ帰属する。ネットワーク取引手数料・gasは別会計である。owner内の許可されたcontributor分割はこの3%の内部に限り、追加徴収しない。

G=1,000 BILIならburn=50、owner総leg=30、net=920。buy/sellの`minOut`はそれぞれPosition受取量／手数料控除後のBILI受取額に適用し、不足なら全体を拒否する。掛算・reserve・allowanceには十分な幅とchecked arithmeticを使い、overflowをwrapしない。

### Decision 5 — 直接送金の手数料と責任

直接transfer、受取、譲渡、approve、transferFromにAMMの5% burn／3% owner legを課さない。通常のネットワーク手数料のみを適用する。ゼロ又はself-transferで供給・reserve・membership tenureを変えない。EVMイベントとゼロ操作はADR0089の規則を使う。

移転するのはPositionの残高と、その現在保有に結び付くmembership資格である。送信者の過去claim責任、bond、署名、slash対象、モデル登録owner権は移らない。旧holderの過去責任を送金によって消去しない。

### Decision 6 — 追加モデルの30%購入・20%永久LP・50%残余

対象worker報酬`R`を`B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R−B−A`へ分ける。Bで同じpairからPositionを購入し、購入量qのうち規範kernelが求めたellだけをA BILIと組み合わせて同じ市場へ永久LP化する。`q−ell`だけをburnしてretiredへ入れる。qを先に全量retireしてからellを復活させない。BとAはともに追加BILI burn・owner直接leg0%で全額を実reserveへ一度ずつ計上する。Dをその版のworker reward planで配分する。詳細と整数kernelは[ADR0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)。

購入・LP・余剰burn・receipt・Dの権利更新は一つの原子的決済とし、途中に別取引を挟まない。B>0でq=0又はA>0でell=0なら`LiquidityDust`とし、市場・receipt・burn・報酬支払を部分commitしない。元の有限期限内だけ保留し、解消しなければ既定の未成功終端とする。A=0はLP/receiptなし、B=A=0は市場操作のみno-opでD=Rの配分は存続する。

GENESIS/FORK_DESIGNATEDには購入・永久LP・報酬由来Position burnがなく、Minerの報酬権利は`R`全額である。Panel等の役務予算をRから控除しない。これは認証、maturity、hold、export上限、失効条件の免除ではない。

### Decision 7 — 市場状態・分類変更・旧残高

登録済みADDITIONALのpair欠落はstate invariant違反であり、正常な未準備状態・待機分岐として受理しない。認証状態又は移行不整合を明示的に解消し、100%Miner fallbackに変換しない。新規buyを停止しても、既受理claimの購入・永久LP・余剰burn義務を消してMinerへ戻すことはできない。追加モデルの新規報酬claimは、同時生成済みpairへのbinding、決済資源・役務原資・原期限の予約を満たす。これは市場の後付け作成・外部seedを待つ別gateではない。

実際にA>0かつell>0の永久LPが記録されたmodelは`hasPermanentLP`を保持し、GENESIS/FORK_DESIGNATEDへの指定を認めない。withdrawをwinddown例外にしたり、後継modelへLPを移植したり、同じartifact rootを別model IDへ登録して制約を迂回したりしない。市場の残高減少、閉鎖、receiptのpruningで属性を消さない。

永久LP未記録モデルのfork指定又は既存core市場の切替には、旧claim・市場・残高・allowance・未決済権利の明示winddown完了を要求する。core sell-only市場、自動没収、無原資補償を新profileへ持ち込まない。未解決又は永久LP記録があれば切替条件を満たさない。

## 3. 保存則と受入条件

- 追加モデル登録と一意市場の生成は同時に成功又は失敗し、core登録は市場を生成しない。
- `curve_units + sum(holder_units) + retired_units = registered_supply`。
- 正常transferは送受の同額増減のみ。AMM reserve、retired、BILI burn、owner支払は不変。
- 通常AMMは`G = burn_fee + owner_fee + net`。追加モデル報酬は`R = B + A + D`、coreは`R = Miner`。
- `initial_real + seeds/deepen + net_user_buys + reward_B + reward_A = real_reserve + gross_user_sells`を、当該curve版の資金台帳で維持する。virtual reserveやreceiptを資金源へ数えない。
- 成功時の在庫は`U2=U1+ell`、retired増加は`q−ell`だけ。receiptを追加供給として数えない。
- 失敗、署名不正、allowance不足、locked残高、無効model分類、replay、overflowで部分的な残高・手数料・eventを残さない。
- native/EVM二重支払、reorg/restart/IBD、同一operation再送、boundary直前直後を検査する。

旧実装のM3「transferが存在しない」と旧genesis fee burnはMOE2の受入条件ではない。現在は正当な転送の成功と不正な二重支出の拒否を要求する。上記のruntime試験、codec、資源上限、activation条件が完了するまで新操作を有効化しない。
