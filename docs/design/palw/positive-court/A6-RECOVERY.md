# A6 — Positive Court の保存・復旧

現行実装は下記R2。最初のprivate journalの記録は [VALIDATION-A6.md](VALIDATION-A6.md) に保持し、今回の追加検証は [VALIDATION-A6-R2.md](VALIDATION-A6-R2.md) へ分ける。

2026-10-31。branch `codex/rfc42-a6-recovery`。共通の基準は既存試作を固定した `c4749c861072bd118a1238276a2ab17e383dd2e1`。A0共通契約 `a85152edc`（v2/r1）をmergeした。その前のbaseは `a5800d2dd5d334e89f7a1ddb6e956aedcbf4adf5`。

## 実装範囲

`consensus/core/src/palw_positive_court_v2/coordinator_recovery.rs` は既存のprivate coordinator全体を入力履歴から復旧する。既存の `journal.rs` は一つの検査候補のパーツだけを扱うため、そのcodecを変更せず別schemaとした。private archive schema 2 はconsensus wire番号ではない。旧schema 1の既存eventは再生でき、旧schemaへ新transport eventを混ぜることは拒否する。

記録対象は署名付き資料要求、資料応答とopening、署名付き候補、検査パーツのbatch、finish、expiry、revokeに加え、A4のtransport設定・部分fragment・失敗packet/再送の消費・buffer破棄。要求dedupe、送信bytes/work、試行回数、失敗した候補の署名責任、検査cursor、候補履歴、資源予約、提供済み資料の料金と返金を同じ履歴から再計算する。失敗を返した要求も、検証workや試行回数を消費し得るため記録する。料金は既存試作の私設ledgerであり、native送金を発生させない。

`JournaledCoordinatorV2` はmutableな内部stateを公開しない。入力を永続化してからその副作用を適用する。永続化のErrやpanic/unwindでcommit成否が不明なら、state/head/encodedの読取りと追加処理を停止する。新しいchecked prepare経路は旧runtimeを所有権ごと消費してから再検査し、native関連行の準備前に次stateを確定する。再起動時は選択されたheadを解決してから新しいadmissionへ全履歴を再実行する。入力quota満杯でも、認証済み時計によるexpiryは独立に適用できる。

`persist_generation` は一意のtempへ書込み・file sync・immutable generationへのhard link・directory syncを行う。既存世代を上書きしない。同じbytesの再送は同じ世代を再利用する。**この世代の作成だけではnative case受付・資金hold・resource予約をcommitしたことにはならない。**

## 復元と同期の境界

`restore` はfresh admissionを所有権付きで受け取り、失敗すると途中のstateごと破棄する。登録class、miner key、Panel roster、result、policy、prompt形式、累積上限、admission時刻をbindingへ含める。モデル事前保有は不要だが、各関係を再検査するための資料は必要。

呼出し側には二つの独立した責務がある。

1. `RecoveryHeadV2` を選択履歴の信頼できる保存取引から解決する。peerがarchiveと一緒に渡したdigestをそのまま権威にしない。短い正常prefixでも選択headと違えば拒否する。
2. 全 `EventOriginV2` のcarrier所属・accepted DAA・event内容と正規順序を検証する。expiry/revokeも認証済み履歴からのみ再生する。ハッシュ一致は履歴の同一性であり、計算検証の代用ではない。

historical eventは認証したaccepted DAAで再生し、最後に受信Nodeの現在DAAでexpiryを適用する。これにより過去に完了した正規検査を期限後のIBDでも再検査でき、pending義務の期限を再起動で延ばさない。future DAA、DAA逆行、別branch/ruleset/input/dependency、別上限・別rosterへの差替えは拒否する。

`read_generation` はディスクとIBD双方に使える有界reader。encoded bytes、record数、nested collectionのheapをallocation-free scannerで検査してからdecodeする。truncated frame、余分なsuffix、未知schema/eventは拒否する。保存cursor・meter・合格flagを復元するAPIはない。

reorg時は選択されたprefixのarchiveとheadを使いfresh admissionへ再生し、古いstateを置換する。要求・料金・責任・検査・資源を別々に巻き戻さない。別claimは別のjournalなので、native側もcase keyを分離すること。

`RetentionV2` はpending、evidence、liability、fee、reorg horizonとarchiveの保持条件を確認するための補助predicate。これらはrooted stateと実際の取得可能性から呼出し側が解決する。pruning-point trustを自己検証と呼び替えない。資料喪失、flagだけのsnapshot、未認証archiveは `restore` の代わりにならず、未検証のままとする。

## A0・A4・A5 への接続

- A0の `misaka-palw-positive-contract::ports::Recovery` を `RecoveryAdapterV2<C>` で実装した。`RecoveryContextV2` がtrusted anchorからfresh admission/headを解決し、全eventのoriginを認証する。実際に共通port経由でreplayする試験を追加した。新contractのresult/witness/domainと既存private試作の型をaliasしない。このadapterの復元先はprivate coordinatorであり、新contractを使うnative ingress/DB adapterは未接続。
- A4所有の `coordinator.rs` への差分は専用recovery moduleの宣言3行だけ。統合時にA0/A4のmodule登録と合わせる。
- native `AtomicCourtStore` の一つのDB transactionがcase・資源予約・funding/obligation・journal headを選択する必要がある。生成ファイルは事前prepareできるが、headの選択だけ別取引にしない。旧headのまま孤立したgenerationは信用を持たない。
- A5のnative料金・返金・credit利用には既存engineの一回限り精算とundoを接続する。ここで再計算した私設meterをnative入金と解釈しない。
- `database::palw_positive_recovery` と `misaka-palw-positive-recovery` を追加。実RocksDBでarchive/head/observed clock/関連行差分/undoを同期取引に保存し、CAS・再送・rollback・同履歴再適用・保持境界を実装した。`kaspad::palw_positive_recovery` がNode向けAPIを登録する。nativeの具体的なUTXO/funding authority・wire dispatcherは提供しない。
- `RecoveryChunkImportV2` は固定scope/headへの有界chunk搬送と実SpillAreaを使い、全履歴の再検査後だけ `CheckedRecoveryImportV2` を返す。Node向け `publish_import` がtokenと現在時計を照合して実DBへ公開する。独立した二つのDB間で確認するが、既存P2P106/107への登録・native state rootは未接続。
- `restore_and_commit` は現在時計をpinして再検査し、公開前にも現在時計を照合する。expiryによる料金/資源の変化を同期取引へ含めてからruntimeを返す。実process exit後の復元を試験した。native selected-chain dispatcherのrollback登録はA0/A4側との接続が必要。
- `prune_retired` は同じscope/head/observed clockの再検査済み置換tokenと全horizonを要求する。DB内部でhead/clockをCASし、旧undo/receiptのみ削除する。現在archive・関連行・別caseを保持する。nativeが同じviewからhorizonを解決することが前提。
- whole-history再実行・encodeの累積費用は二次になり得る。`quote_update` / `quote_admission` がその費用を明示する。decoded heap、関連行、RocksDB/allocator、全保持世代を別途予約する。実LLMの時間/RSS測定、native profileの有効化は行っていない。
- A0へ必要な具体的anchor/行差分/排他guard/quote/IBD接続は [A6-NATIVE-CONNECTION.md](A6-NATIVE-CONNECTION.md) に固定した。自己申告origin・私設fee meter・保存成功をnative実原資や信用と数えない。

## 検証

実行結果は同じディレクトリの [VALIDATION-A6.md](VALIDATION-A6.md)へ保存する。復旧試験では実際のML-DSA署名、Merkle opening、既存TIR courtを使う。模擬の永続化失敗はcommit acknowledgement喪失のfail-closed動作を確認するためであり、実native資金取引の障害試験ではない。

有効化設定は変更なし。新native wire、public preset、Gate Cの許可経路は追加していない。
