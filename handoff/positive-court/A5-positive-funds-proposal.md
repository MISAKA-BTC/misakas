# A5: 新Positive資金・source due接続の採用案

2026-10-31。A0のsource fee境界の確定を受けた実装提案。
旧FundingTerms・旧escrow row・旧relay payloadのID意味は維持する。
新global key、wire tag、hash/signature domain、台帳は追加しない。
採用済みaux151 `FundingRow` / `ServiceReceiptRow` / `CaseRow` を使う。

## 提案コードの具体的な入口

node crate `misaka-palw-positive-accounting-node` の専用moduleへ実装する。
core→nodeの依存は追加しない。VP側の採用時に必要なcrate登録・shared callerはA0が確定する。

| API | 用途と保証の限界 |
| --- | --- |
| `dues::prepare_source_due` → `SourceDueProposal` | first receiptのsource fee delta、usage/receiptの期待値と更新案。stateを変更しない |
| `dues::check_source_quote` | whole source scheduleとCourt全feeの上限算術。request前のactual予約そのものではない |
| `native_funds::FundingData` | Result/Case/Fundingの同じ読取候補を渡すDATA参照。rooted authorityではない |
| `native_funds::PositiveLockViewData::new/classify_spend` | old/new/provisionalの共通分類。`NeedsPositiveQualification` / `NeedsLegacyQualification`は許可ではない |
| `native_funds::propose_positive_spend` → `PositiveSpendProposalData` | actual Transactionと提示UTXO entryに対する三勘定・出力・burnの検査案。UTXO entryの出所と入力署名は別に必須 |
| proposalのsnapshot確認 / `require_native_commit` | 期待before値の比較と、actual owner未接続のtyped refusal。snapshot一致はimport世代やABA検査を代替しない |

`CourtWholeFeeData` / `RefundData` / `OtherBurnData` もnode-localのDATA入力である。
正規rate、全体の一回払いmarker、未精算due、返還可能な責任、他burnの額を確定する権限は持たない。

## A0へ採用を求める境界

source料金は **due発生と実支払を分ける**。
`Case.usage.fee` は原資料サービスの累積dueだけで、first unique receiptで一度だけ増える。
一回払いidentityは既存 `NativeRowKey::ServiceReceipt { case, kind, source, unit }` と一致する。
recipient、Panel、relay、attempt、nonceをidentityへ足さない。
control/carrier/Panel完成料はCourt全体feeとして別に処理する。
`Receipt.settlement == None` はsource due未発生という意味ではない。

request前には、正規source scheduleから導出したworst-case feeと資源を予約する。
requestの受付時点で `Case.usage.fee` を支払済みやdueとして増やさない。
dueは最初の正規response受理時に計上し、再送・別Panel・引継ぎで再計上しない。
予約残額/寿命は既存reservationの正規preimageとnative資源ownerが管理し、A5の別ledgerは作らない。

A0 `c2e93ec49` 採用の共通due intentはreceipt key/Result/FundingScope/schedule/provider/amount/accepted-responseを持つ。
A5は同じ型を再公開し、node-local proposalが期待するbefore/after usageを保持する。wireやnative権限ではない。
A4 one-writerは実accepted-prefix、first receiptの有無、正規rate/recipient、old/new行を再解決する。
ゼロ料金もfirst receiptのidentityを保存し、実spendなしでdue 0を記録できる。
そのreceiptを後の料金変更で再課金しない。

## 新FundingRowのlockとspend

lock残額は `fee.remaining + capital.remaining + surplus.remaining` のchecked sumとし、
`FundingRow.current_outpoint` のactual UTXOと金額・refund scriptを照合する。
三勘定間の移動、capitalをfeeへ充当、surplusから料金を引くことは許さない。
実outputの余剰はsurplusとして保存する。

未精算の同一outpointを複数case、旧escrow、新Positiveで重複登録してはならない。
現在のlock indexを構築する際も、writerがinstall/continuationを適用する際も拒否する。
新しいcarrier出力を同mergeset内で受理した直後はprovisional lockへ入り、そのmergesetではspendを拒否する。
lock viewのrooted側はselected-parent由来、provisional側はその後の同prefixで受理したfunding/continuationである。
同じpointを双方へ二重投入せず、native ownerが実acceptance順で管理する。
`consent.admission_expiry` をlock解除・返金・責任終了の期限へ流用しない。

新Positive spend検査は、新FundingRowの期待before行、正規due、実Transaction/UTXO entry、
全種類のlockとの競合、正規refund/continuationを照合する。
funding inputは一つでinput 0に固定し、旧+新の二本を相互にother-input扱いしない。
recipient outputは額・順序・scriptを照合し、その他出力とrelay費用はその他入力から賄う。
burnはactual transaction feeの一回だけの割当で、bond/旧escrow/新Positiveのburn合計以下にならないことを確認する。
新relay payloadの採用前はempty payloadのみを検査対象とする。

spendの結果は三勘定のbefore/after、paid/burn/refund/continuationとactual outpointのDATA提案である。
正常な計算結果からnative admission/UTXO lock/spend許可を生成しない。
実支払時にはcurrent rowとのCAS、receipt settlement、一回払い、正規scheduleの再検査が必須である。
特に返金時は、未払いの成立済みsource料金と予約済みの将来費用を消してはならない。
native ownerが全未精算due・予約・責任を再解決して返金可能額を計算する。
`RefundData.resolution` と保存済みliability参照が一致するだけで、その責任が解消したとは扱わない。

既存worker成功支払の計算信用用途は `WorkerPayment`、primary issuanceは `Reward` と区別する。
この区別は提供済みsource料金・安全返金をPositive/P信用gateへ入れる理由にはならない。
それらはactual receipt/liability/funding/once-money条件で処理し、P不成立だけで返還可能な資金を永久lockしない。
未解決の責任や未払due/予約を消すことも認めない。
`PrivateAccountingStore` のPanel完成サービス料をworker成功報酬へ再解釈せず、
worker payoutは既存の実契約と資金ownerが専用purposeを確認して処理する。

## actual node接続を行う箇所

既存旧escrowの接続順序を参照するが、その旧型・deltaを新意味で使わない。

| native ownerの場所 | 新Positiveで必要な接続 |
| --- | --- |
| `virtual_processor/utxo_validation.rs` のblock/mempool共通lock判定 | 新row由来lockと同mergeset provisional、旧/新outpoint重複拒否、全burnの単一配賦 |
| `virtual_processor/processor.rs` / `processor/palw.rs` の現stateからのlock取得 | actual accepted-prefix/import世代/lineageからaux151 FundingRowを取り、欠落・破損・不明を拒否 |
| A4 `FundingPreflightCoordinatorV2` / `AtomicAdmissionCoordinatorV2` | 同じactual carrier/quote/資源/UTXOを使うnative `AtomicCourtStore`へ接続。private store成功をnative成功へ変換しない |
| core `TransitionBuilder` のA4 Positive one-writer | Funding/Case/Receipt/SourceUnion/責任/資源のold-new CASとjournalを同じcheckpointへ記録 |
| `processor.rs::commit_utxo_state` の単一`WriteBatch` | 上記delta、lease、actual UTXO diff、before-imageを同batch確定。部分成功を残さない |
| A6 undo/replay/import | 同じbefore-imageでlock/due/支払/資源/責任を復元。local replayなしの保存Passedを信用にしない |

これらnative共通ファイルの登録・writer変更・private context型の確定はA0/native backend ownerの採用対象。
node-local calculatorに公開setter、任意callback、`ValidatedTransaction`という型名だけでnative権限を与えない。
実際、この既存型は公開field/constructorを持ち、外部から作成できるためauthority tokenではない。

受理済み署名packetのsemantic失敗は、既に消費したmeter/attempt/accepted receiptと結果を同じbatchに残す。
単純な `Err` でcase全体を巻き戻さない。料金は正規契約と成立したサービスから別に決める。
native batch自体が失敗した場合はそのbatchの全要素を取消す。

## 完成の判定

local proposal試験はnative接続の完成を示さない。
actual nodeでmempool/block/templateの同一判定、same-mergeset lock、三勘定spend、due一回、
old/new CAS、source受理と精算の同batch、reorg/IBDまで接続・検証してからnative完了とする。
本番presetは変更しない。

## A0 checkpoint `f6c7d9f4d` とのjournal接続

新しい正本はA0 `docs/design/palw/positive-court/NATIVE-JOURNAL-R1.md`。
A5はjournal codec・domainを再定義せず、kind1の完全canonical native Transactionをspend再検査の資料とする。
TxID、case journal head、`JournalHeadRow.history.digest` は別identityでありaliasしない。
aux preimageの正規参照一覧・origin・完全性はnative ownerが照合する。
保存済みproposal/paid flag/receiptやhashだけで適格spendを再生成しない。

Case状態または費用を変える精算とsource dueはjournal対象、実認証後のlogical NoOpは対象外である。
異なるauthored failure/retryで生じた消費は残す。ordinary TX受理・fee/historyとCase journalを混同しない。
aux151のold/new、actual UTXO、acceptance、追加material-storeを既存caller-owned per-block batchで確定し、
commit前にcache/terminalを公開しない。別WriteBatchやA2 local I/O storeをchain undoへ流用しない。
restart getterは再受付を避けるread-only portであり、raw eventの独立再生・current root/history/funding/lease確認を代替しない。

この照合は文書上の接続確認で、新しいruntime実行・native journal codec・actual VP統合の成功記録ではない。

検証は [phase5記録](../../misaka/palw/positive-accounting/evidence/phase5-positive-funds/README.md) を参照。
root通常Cargoのno-default node全lib 44件と同候補のall-targets Clippyが成功した。
独立レビューで見つかったCase/Result/receipt snapshotと時刻逆行、局所重複請求のguardも反映した。
これはactual VPでlockを導入した証拠ではなく、採用するcalculator/predicateの候補検証である。
