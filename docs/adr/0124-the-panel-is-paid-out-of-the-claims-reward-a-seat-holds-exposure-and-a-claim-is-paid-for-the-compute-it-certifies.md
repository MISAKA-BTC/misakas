# ADR-0124 — モデル分類別のMiner報酬・Panel原資・責任予約

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTのモデル恒久廃止で、当該永久LP内actual MISAKA・登録者責任bond・MINING用途bond・未確定mining報酬を100%burn/失効させる。**全Panel/Gate C検証用途bondは、不正者も含め一律にBFT廃止burnから保護**し、個人不正はCourtの個別slashへ分離する。正当に成立した検証報酬の支払を維持し、他モデル資産へ影響させない。兼任は用途別lot（MINING6,000だけburn、PANEL4,000は保護）とし、重複担保・責任中の用途変更は禁止する。

通常のLP撤去禁止を維持し、canonical廃止のactual reserve burnだけを限定例外にする。BFTの没収受取・廃止bountyは0。継続5%報酬を没収額に連動させない。可逆Pause/Resumeと区別し、旧停止専用・LP絶対不処分の記述をこのscopeだけ置換する。旧profile/native/履歴を保持し、BMR1実装・移行・有効化は別工程とする。

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

- 改訂日: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。MOE2の設計改訂であり、native/EVM実装、測定、移行又は公開有効化の完了ではない。
- 共通正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md) / [ADR0232](0232-reward-buy-and-permanent-liquidity.md) / [RFC-v2-0011](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md)。
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

## 1. Rの定義と支払の順序

本書のRは**購入・永久LP・Panel配分前**の価格付け済み対象worker報酬である。MOE2ではB=floor(300R/1000)、A=floor(200R/1000)、D=R−B−Aとする。旧文書の「buyback後残額」Wは今回のDに対応し、名前を使い回して控除を二重適用しない。

対象モデル、価格付け版、reward plan、分類、原資、支払先、原期限をEconomicsSnapshotと署名済み契約へ固定する。難易度・receipt数・後から追加したowner署名を理由に元の総発行予算を増やさない。FOW1の正当性・費用条件と、TB1による抽選を引き続き満たす。

## 2. 決定

### Decision 1 — 分類別のworker報酬

| 分類 | 購入B | LP用BILI A | 残余D | RからのPanel控除 | Miner |
| --- | --- | --- | --- | --- | --- |
| ADDITIONAL | floor(300R/1000) | floor(200R/1000) | R−B−A | 当該reward planが定めるD内のpool | Dからplanの適法なworker legを引いた残額 |
| GENESIS | 0 | 0 | R | 0 | R |
| FORK_DESIGNATED | 0 | 0 | R | 0 | R |

coreの100%はMinerへ帰属する対象worker報酬Rの全額である。既存の認証・maturity・hold・export上限・責任・失効規則を免除する意味ではない。

### Decision 2 — Additionalで旧20%pool方式を使う場合

その版を明示し、ほかのworker legを持たないreward planに限り、`pool=floor(200D/1000)`、`Miner=D−pool`とする。R=1,000ならB=300、A=200、D=500、pool=100、Miner=400である。**Rの20%であるLP用Aと、Dの20%であるPanel poolは別**であり、AをPanelへ渡さない。Panel20%をRへ先に課したり、全profileへ追加したりしない。

旧固定seat share方式を継承するplanは、抽選時の担当数Kに対し`per_seat=floor(pool/K)`を固定する。後から返答者数を分母にして、他者の省略で一人当たり報酬を増やさない。適格な担当の履行が独立に包含できる経路と元windowを維持する。

未履行seat分と割算残差は当該planの明示的な未分配reserveへ留め、Minerへ戻さない。各seatの支払＋reserve=poolであり、新規発行ではない。K=0のplanへ暗黙fallbackせず、別の明示採択がない限り受理しない。正当な単独反証者を多数派との不一致だけでslash又は不履行扱いにしない。

### Decision 3 — CoreのPanel・資料・Courtは別原資

GENESIS/FORK_DESIGNATEDのPanel、モデル取得・完全再実行、資料供給、Court、TB1役務に必要な費用は、Rとは別の有限な署名済みfee契約・前払escrow・予約で確保する。誰がいくら負担し、どの役務で解除又は返金するかを受理前に固定する。

Rを先に小さく再定義してPanel分を隠したり、MinerへRを払い同時に必須の同額後払いを請求したりして100%を表示しない。新しい無根拠なtreasury支出・追加mintを作らない。別原資が不足する新claimは受理しない。既受理の反証・資料・終端の予約を新claimへ流用しない。

### Decision 4 — Seat責任と報酬権利を分ける

担保元本、同時claimのreserved exposure、role同意、責任期限、支払期限を認証された状態へ結合する。ModelPosition残高やallowanceをminer/Panel bondへ読み替えない。同じ元本を複数role・model・claimに二重計上せず、必要なheadroomを受理・役務割当前に確認する。

過去の3倍seat exposure、10倍seat floor、無加重抽選等は、その署名済みpolicy版に限定する。今回の比率変更で新しい倍率を採択したとしない。現在のPanel抽選は固定DrawPolicyとTB1-PANEL、裁定はPublicOneHonestと認証局所反証の規範に従う。旧quorumと異なる答えを出したという理由だけの罰則を新profileへ継承しない。

### Decision 5 — 価格付けと総量制限

Rは元の`pricing_policy_id`、モデル/個別bondの予算、共通期間、work/credit会計へ結合する。tier係数1.0/0.3やmodel配分を、確定したRの上から再び掛けない。`expected_attempts`又は実装の上界FLOPsを、最安計算費用の証明や追加発行原資としない。

旧canonical-pwu比による価格付け、未発行縮小分の扱い、κ、共通期間等はそれぞれの適用版と既存の未解決gateを維持する。MOE2は対象Rの分配を変えるが、ネットワーク全体の予算やblock機会を増やさない。

### Decision 6 — 成功・未成功・一回会計

成功した認証済みclaimだけが対象worker報酬とcreditを得る。BのPosition購入、Aとellの永久LP、余剰q−ellのburn、LP receipt/sticky属性、Miner/pool/payee/reserve、経済消費を同じ原子的遷移へ接続し、元export上限をB+Aの両額にも適用する。void/未検証Abortに成功Rを発行しない。実際の役務feeの精算は別契約の条件に従う。

追加モデル登録は一意市場を原子的に同時生成し、別市場準備・外部seed待ちを設けない。pair欠落はstate invariant違反として拒否し、正常な未成功claim分岐へ混ぜない。存在する市場の新規buy停止等で、coreの100%規則へ切り替えることもない。既受理の購入・永久LP・余剰burn義務は予約済み決済と元期限を満たすか、規定の未成功終端へ進む。LiquidityDustではDの支払だけを先行せず全工程未commitで原期限内のみ保留し、解消しなければ既定終端へ進む。B=A=0の市場no-opでは元の成功・最小仕事条件を満たすD=Rの配分を維持する。未受理仕事へ永続的な債務を増やすpool予約を認めない。

## 3. 移行と検査

旧claimは旧受理snapshotで終える。実永久LP記録済みmodelをcore指定へ移すことは不可とし、LP解除・後継移植・同root再登録の迂回をwinddownへ含めない。新classification、fee、reward plan、Panel別原資、core旧市場のwinddown、全consumerの版を同じactivation手順へ結合する。新率を旧fenceの定数だけに上書きしない。

検査には、R=0/奇数/最大値、B/A控除後のDからのpool、全seat/一部seat/ゼロ履行、K固定、reserve残差、coreのR全額、別fee予算不足、原期限、誤分類、購入とLPのexport cap合算、LiquidityDust/zero/no-op、二重Final、返金競合、reorg/restart/IBDを含める。過去のtestnet値や旧試験PASSを本改訂の検証済み状態へ転記しない。
