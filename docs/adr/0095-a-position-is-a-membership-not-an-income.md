# ADR-0095 — 譲渡可能な追加モデルPositionと現在保有に結び付くmembership

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
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0095-a-position-is-a-membership-not-an-income.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

## 1. 保有資格と経済的権利

MOE2では追加登録と一意市場の生成を同じnative遷移に含める。外部seed又は後続openを登録済み市場の存在条件にしない。core登録は市場を生成せず、追加モデルの市場欠落は不正状態である。

MisakaOptionはADDITIONALモデルだけの譲渡可能な残高資産である。GENESIS/FORK_DESIGNATEDには市場、Position、Positionに基づくmembershipを開かない。BILI報酬、claim認証、bond、登録owner権とは別であり、holderであることだけによる配当・マイニング報酬・合意票を与えない。

元の「非譲渡だからmembershipである」という説明を廃止する。**membershipは現在の認証残高に従い、Position移転後は新しいholderがその保有量の条件を満たす。** 外部サービスの提供そのもの、モデルの品質・取得性をchainが保証する契約ではない。

モデルとartifactは不変である。新しい重みや派生モデルへ、既存Positionの権利を自動移植しない。mainnet後へ延期されたモデル改善・preview/promotion機能をこの改訂で有効化しない。各grantはその機能が別途有効な範囲でのみ提供できる。

永久LP receiptは拠出履歴であり、holder残高、membership資格、配当権又はtier ageを持たない。LPへ入れたellはcurve在庫に一度だけ計上し、プロトコルを架空holderにしてその量をmembershipへ数えない。将来の通常買いでPositionを取得したholderは、その時点から通常の残高・保有期間規則に従う。余剰burn済み単位は資格を持たない。

## 2. 宣言するgrantとtier

ownerが認証された宣言へ署名し、model ID、artifact root、benefit policy、tier、期限、noticeを結合する。最大8tierという既存の有限上限を維持し、`min_units`は正規順、未知grant bitや未対応機能は拒否する。

| grant | MOE2での扱い |
| --- | --- |
| EARLY_VERSION、PRIVATE_BETA | 対応するversion機能が別途有効な場合だけ。登録モデルの不変性・延期規則を破らない |
| EXPERIMENTAL、DEVELOPER_ACCESS、SUPPORT | 宣言された非金銭サービス。chainが外部提供の完遂を自動証明するものではない |
| HOLDER_VOICE | 投稿時の認証残高とtierを記録。合意投票権ではない |
| PRIORITY_INFERENCE、INFERENCE_QUOTA | 旧撤回状態を維持し、この改訂で復活させない |
| 配当、BILI rebate、reserve請求権、採掘weight | grantとして認めない |

contributorへの対価は採用された作業への支払であって、保有への配当ではない。owner/contributorの通常売買対価の合計は3%のowner総leg内であり、holderへ直接分配しない。

## 3. 転送後の残高と保有期間

tierは指定したcanonical state anchorにおける残高と宣言から求める。未受理の送金、未決済AMM購入、第三者の未認証cacheを現在残高に加えない。allowanceを受けたspenderは、そのallowanceだけではholderにならない。送信者のclaim、bond、過去署名、責任又はowner権は移転しない。

旧の単一`holding_since_daa`をそのまま送信先へコピーしない。新profileでは最大8tierごとに連続して`min_units`以上を保有している開始時点を管理する。

- あるtierの閾値を下回ると、そのtierの連続保有開始を解除する。
- 閾値未満から以上になれば、送受・購入等の**確定した状態遷移時点**を開始とする。
- 閾値以上を維持しているtierの時刻は、少額の受取や同じtier内の増減で巻き戻さない。
- 受取によって初めて達成した上位tierは、送信者又は受信者の下位tierの古い期間を継承しない。
- zero/self-transfer及びapproveのみでは残高・tier時刻を変えない。

これにより、少額を長期保有してから大量に借りて上位tierを直ちに熟成済みとする扱いと、dust送金で既存tierの保有期間を強制resetする扱いを禁止する。これは外部借入の不存在や人間の実質所有者を証明する機構ではない。`min_hold_daa=0`の宣言は即時の現在残高資格を明示的に許す。

宣言更新で閾値・grant・期間が変わる場合は、noticeと有効化時点を固定する。新たな閾値の過去保有を認証stateから証明できなければ、宣言有効化時点より前へ開始を遡らせない。旧versionの既に確定した権利や履歴markは、新方式で再採点しない。このtier別状態と同一順序のnative/EVM更新はactivation前の実装条件である。

## 4. Notice・有効期限・証明

既存のgrant弱化に対するnotice、明示expiry、対応するcadence失効を維持する。期限切れを有効なmembershipとして表示せず、未実装のversion/promotion経路を有効と仮定してcadenceを評価しない。強化と弱化の分類・適用時点は、署名済みbenefit policyで全consumerが同じ規則を使う。

holderはnonce・model・benefit policy・state anchor・有効期限・認証domainへ結合したchallengeへ署名する。受け手は最新の要求scopeに必要な状態と残高を検査する。古いanchorの正しい証拠は当時の保有だけを示し、送金後の現在保有を示さない。長寿命のアクセスを与える場合は、その期間を明示した別サービス契約として扱い、古いmembership証拠の無制限再利用で与えない。

nativeとEVMの異なるholder IDを「同じ人らしい」という理由だけで合算しない。共通ownerとして集約する別機能がある場合は、双方の認証・二重計数防止・scopeを別途固定するまで無効とする。

## 5. 移行と確認

GENESIS/FORK_DESIGNATEDの旧Position資格を新profileへ持ち越さず、永久LP未記録モデルの分類切替には旧サービス契約・残高・未決済権利のwinddownを要求する。実際に永久LPを記録したmodelはcore指定不可とし、LPを解除・移植してwinddown条件を満たしたことにしない。同rootの別IDでこの制約を迂回しない。自動没収や別modelへの資格移植も行わない。

受入には送金後のsender/recipient tier、lock、allowance、閾値cross、dust、zero/self、宣言更新notice、期限切れ、古い証拠、別domain、native/EVM順序、reorg/IBDを含める。旧「no transfer」試験を新方式の適合証拠として使わない。
