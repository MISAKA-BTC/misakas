# ADR-0095 — 譲渡可能な追加モデルPositionと現在保有に結び付くmembership

- 改訂日: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。MOE2の設計改訂であり、native/EVM実装、測定、移行又は公開有効化の完了ではない。
- 共通正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md) / [ADR0232](0232-reward-buy-and-permanent-liquidity.md) / [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)。
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
