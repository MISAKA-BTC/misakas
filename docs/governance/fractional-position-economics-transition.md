# MOE3：小数POSITIONと全モデル統一の適用・移行

- 日付: 2026-11-26（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../adr/0236-fractional-position-miner-rewards.md)
- 前版: [MOE2移行契約](model-option-economics-transition.md)／[MAD2移行契約](model-admission-dual-route-transition.md)

## 1. 境界と承認の範囲

今回のユーザー指示は設計変更を承認する。新しいnative状態・codec・署名domain・fence・activation高さ・本番構成をこの文書だけで割り当てない。

新規MOE3は全モデルに係数1.0、POSITION市場、coinbaseの50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKA永久LP、5%をPanel/Gate CのMISAKA、5%をBFTのMISAKAへ配分する規則を適用する。新fork分類、genesis専用BFT免除、追加モデル専用発行枠を廃止する。originは過去の採用・creatorの有無を確認する履歴であり、稼働時の経済分岐に用いない。

旧受理claimは受理時の価格、係数、fee、reward plan、経済版、元期限、責任と支払assetを維持する。旧0.3 claimを1.0へ増額せず、既に約束したMISAKAをPOSITIONへ強制変更しない。新規MOE3へ旧receiptや旧署名を再投入することも認めない。

## 2. 有効化前に確定するもの

最低限、networkと認証activation anchor、新quantity/state/receipt/quote/ABI/RPC版、署名domain、移行集合root、旧受付停止点、旧cohortのdrain手順、有限の移行期限、共通配分epoch境界を一つの移行計画へ固定する。

wire fieldの論理名は本RFCに記すが、実codecは別の実装契約である。新profile IDに旧bytesを割り当てず、未知版を旧MOE2として読むfallbackも作らない。旧history readerと新writerを分け、activation前に新単位の取引を受理しない。

今回の提出物はこの契約と参照計算であり、移行manifestの実network値やapproval票は含まない。

## 3. モデル集合を共通化する

| 入力の既存モデル | 新規MOE3の扱い |
| --- | --- |
| 元genesis・市場なし | 同じ不変model/rootに一意市場を生成。係数1.0、共通BFT管理、creatorなし |
| 追加登録・既存市場あり | 元model/root/pairを保ち数量を一回変換。係数0.3から新規配分のみ1.0へ。creator権を保全 |
| 旧fork・市場なし | 通常モデルへ統合し一意市場を生成。係数1.0、共通BFT管理、正規登録者を移行 |
| 旧fork又は旧genesisで市場が既存 | 市場を二重生成しない。既存資産を保全した数量移行 |

canonical artifact root／model IDの重複・aliasを検査し、一つの市場を複製して供給を増やさない。今回の統一はモデル内容・rootの変更、別モデルへの資産移植を認可しない。

既存の有効採用recordは、どのoriginにも共通の「認証された旧採用の移行」という根拠で保持する。既存停止、再開条件、世代、期限を保持し、新規BFTモデル管理ではgenesisも対象にする。停止・NO・保留によってPOSITION市場、holder、永久LPを没収・削除しない。

旧BFT proposalにあるrequested_model_class、旧genesis免除、fork指定は元domainの履歴として読む。未決proposalを新共通モデルの承認へ自動転用しない。新domainと共通制度の対象を束縛した正規の認可を要求する。

旧genesis向けに権限外だった管理・停止票は、MOE3後にも旧版の有効票にならない。新policyに従って新規に認可する。廃止対象はforkモデル分類であり、network genesis hash、chain fork/reorg、protocol upgradeの認証・処理は保持する。

### burn-pauseの移行

旧確定PAUSED状態はsequence、直前canonical操作、burn履歴、元claimの責任を保持して移行する。ACTIVEにする移行ではなく、登録者の解除完了を市場移行の必須条件にもしない。旧登録者鍵の喪失で状態移行自体が永遠に不可能になることを避ける。

旧未決Pause/Resume操作は元の署名・権限・burn条件でdrainする。移行後の新Resumeは、全モデル共通の `登録者認証 OR 専用BFT BurnResume承認` と規定解除burn、実支払署名を必要とする。追加者不在のgenesisも同じBFT経路を使う。旧署名・採用YES・SAFETY_RESOLVEを新しいBurnResumeへ転用しない。

burn理由の解除はBFT safety stop、未採用、他の責任・原期限を変更しない。停止・解除burnの値、操作量上限、署名domainの具体割当は別途有効化契約で確定し、今回の通常AMM burn5%と混同しない。

## 4. 配分epochと旧責任

新規共通配分の最初のepochで全モデル `c_m=1`、単一集合の正規化を適用する。`registered_tier_cap` は同epochから不適用。以前の枠を全モデルへ一律30%として適用する代替も採用しない。

共通発行予算、既消費のcap、未解放のhold、bond平均の観測窓、modelごとの予約と原責任を移行する。観測履歴をリセットして倍の発行能力を作らない。係数1.0への変更で新規の相対配分が変わることは許容するが、過去の確定報酬又はcreditを再計算しない。

旧claimは元の経済planでdrainする。旧市場に影響する買付・売却・quote・予約を新単位の市場と同時に実行しない。市場の移行前に該当旧cohortを元期限・元精算規則で終端させる。長期の不正責任・bondは市場移行後も履歴参照とともに保持できる。

未解決claimを無断に取消して移行を通さず、待機中のモデルで他profileの直接MISAKA fallbackを開始しない。activationはdrainを満たす認証集合に対して原子的に行い、不成立の移行を部分commitしない。

## 5. whole数量を一回だけ変換する

旧数量qについて `q_atoms = checked_mul(q, 100000000)` とする。経済上の保有量と供給を変えず、量の表現だけを拡張する。

| 変換するもの | 変換しないもの |
| --- | --- |
| curve POSITION在庫、registered supply、retired | real MISAKA reserve、virtual reserve、seed_locked |
| holderの総残高とavailable/locked内訳 | 通貨の既存最小単位、MISAKA額の役務予約 |
| 正規に持越せるPOSITION lock・予約 | 不変model ID、artifact root、pair identity |
| 認証済みの確定allowance、membership min_units | 旧署名bytes、nonce、過去receipt bytes、古いstate root |
| 新viewで正規化して表示する数量 | 旧決済結果、原期限、bond、責任、採用・停止履歴 |

旧allowanceを持越す場合はowner/spender/domain/nonce/lockと同じ認証snapshotにある**確定stateだけ**をscaleする。未受理approve署名・市場注文・minOut・旧SDK requestを新数量へ変換して受け付けない。古い署名domainは新操作を認可できず、移行後の新署名が必要である。

allowance最大値等のsentinelは旧版に従って明示識別する。金額との区別ができない値やu128範囲に入らない値があれば、勝手に飽和／無制限へ変えず移行を拒否する。

変換後 `supply_atoms = curve_atoms + holder_atoms + retired_atoms` を検査する。holder lockを第四の供給項にしない。旧membership閾値が1 POSITIONなら新閾値は100,000,000 atomsであり、1 atomへ下げない。単位変換だけでは継続保有時刻を変更しない。新受取で達成する閾値には既存の期間規則を適用する。

quantity migration recordは前後profile、認証元state、model/pair、係数、数量root、元原資を結合し、一回だけcommitする。移行済みrowへの再変換、部分残高のみの変換、旧新viewの同時消費は拒否する。

## 6. 市場がないモデルの初期化

既存genesis・旧forkのmarket absentは、旧規則では有効な状態だった。MOE3 activationに限り、登録済み不変modelと一意市場を認証移行で結合する。新規受付で正常なmarket absent状態を残さない。

新市場は供給500,000 POSITION、8桁atoms、全供給をcurve在庫へ置く。既存の現行curve初期値を用い、V=10,000,000 MISAKA、実入金がなければr=0、seed_locked=0、holder=retired=0とする。virtual reserveの額を現金又は新しい通貨発行として計上しない。

genesisのcreatorはNone。旧fork／追加モデルのcreatorは元の認証された登録者権へ対応させる。元資料に登録者の根拠がなければ、管理者を仮受取人にせず移行の未解決項目とする。追加者の有無だけを理由に係数・BFT管理・POSITION報酬・市場生成を変えない。

## 7. fee、報酬asset、LP、外部表示

新規通常売買はburn5%、creatorあり1%／なし0%。genesisに請求しなかった1%をburn・treasury・LP feeへ付け替えない。旧取引の3%その他の確定feeは再計算しない。

Cを一度だけ分割し、F=P=floor(50C/1000)、A=J=floor(200C/1000)、D=C-F-P-A-J、B=D+Jとする。Bで取得したqのL=floor(qJ/B)をLP、M=q-Lを採掘受取先へ渡す。市場増分はB+Aだけであり、Panel/Gate CのPとBFTのFはMISAKAの別予約・支払にする。旧native awardのMISAKA `miner` fieldを上書きせず、assetとprofileを明示する。市場決済がdust・廃止になっても正当に成立した検証/BFTの役務権利を取り消さない。

旧永久LP receiptは元profile/元単位のbytesを保持する。現在市場の数量をscaleしても、receiptの履歴値を生stateへ再入金しない。表示上の単位換算にはreceiptのquantity版を付ける。永久lock属性・累積履歴の認証をpruning後も保持する。

旧「永久LP後はcore昇格不可」は、MOE3ではcore昇格自体が廃止される。通常のLP撤去・償還禁止を全モデルへ引き継ぐ。[BMR1移行](model-retirement-transition.md)の正規恒久廃止だけは現存real MISAKAのburn専用例外とする。通常BFT停止・解除や新登録は償還権を生まない。

native/RPC/ABI/CLI/SDK/explorer/walletはprofile、8桁、atoms文字列、Miner POSITION額、LP両脚、creatorの有無を一貫表示する。旧Webは旧稼働profileの表示として保存し、MOE3が稼働したようにラベルだけ変えない。新APIは旧単位のrequestを黙って互換扱いしない。

Courtの個別slashは新offence/policyに限り実回収MISAKAの49%をPanel/Gate C合計へ配り、残り51%をburnする。BFT恒久廃止100%burnとは原資・purposeを分ける。旧offenceの分配は元契約でdrainし、同一回収の再請求を認めない。詳細はBMR1移行に従う。

## 8. 原子性、reorg、失敗時処理

登録市場生成、数量移行、報酬決済のそれぞれに一回キーと前状態参照を持たせる。未commitの失敗は元状態を保持する。canonical reorgは関連state・holder・receipt・fee・原資予約・移行markerを同じjournalで戻す。undo時に任意のdivideによって旧wholeへ切り捨てず、保存された厳密な前状態へ戻す。

成功したcanonical移行後に新小数残高が存在する状態で、旧schemaへ単純な逆scaleを行わない。将来のupgrade中止やrollbackは認証された移行計画・全依存履歴の原子的復元を必要とする。永久LPを任意に引き出す権限とchain reorgを混同しない。

初期化不能、残高不一致、未知creator、overflow、未drainの旧市場操作、旧版の重複署名がある場合は移行を未commitとして拒否する。値を丸める、債務を消す、genesisを例外にする方法で成功を作らない。

## 9. 検証・有効化の判定

今回確認できるのは設計のリンク整合、構造化policy、Pythonの固定小数・算術・抽象状態検査である。入力ZIPに元からある欠落リンクは基準との差分で報告し、検査baselineを書き換えて隠さない。

有効化前には新native市場・reward award・サービス予約・BFT共通管理・配分epoch・移行journal・署名domain・各consumerを接続し、旧版とのcross-replay拒否、全体供給保存、実際のreorg/IBD、限界値、quoteの妥当性、停止・再開、未決旧権利を試験する。今回の参照検査だけでこの段階を完了したとしない。
