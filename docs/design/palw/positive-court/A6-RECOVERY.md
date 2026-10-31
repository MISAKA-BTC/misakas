# A6 — Positive Court の保存・復旧

2026-10-31。branch `codex/rfc42-a6-recovery`。共通の基準は既存試作を固定した `c4749c861072bd118a1238276a2ab17e383dd2e1`。その前のbaseは `a5800d2dd5d334e89f7a1ddb6e956aedcbf4adf5`。

## 実装範囲

`consensus/core/src/palw_positive_court_v2/coordinator_recovery.rs` は既存のprivate coordinator全体を入力履歴から復旧する。既存の `journal.rs` は一つの検査候補のパーツだけを扱うため、そのcodecを変更せず別schemaとした。private archive schema 1 はconsensus wire番号ではない。

記録対象は署名付き資料要求、資料応答とopening、署名付き候補、検査パーツのbatch、finish、expiry、revoke。要求dedupe、送信bytes/work、試行回数、失敗した候補の署名責任、検査cursor、候補履歴、資源予約、提供済み資料の料金と返金を同じ履歴から再計算する。失敗を返した要求も、検証workや試行回数を消費し得るため記録する。料金は既存試作の私設ledgerであり、native送金を発生させない。

`JournaledCoordinatorV2` はmutableな内部stateを公開しない。入力を永続化してからその副作用を適用する。永続化が失敗した場合、diskへのcommitの成否が不明になり得るので、state/head/encodedの読取りと追加処理を停止する。再起動時は選択されたheadを解決してから新しいadmissionへ全履歴を再実行する。

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

- A0の `misaka-palw-positive-contract::ports::Recovery` は調査した。新contractのresult/witness/domainと既存private試作の型をaliasしない。本commitは旧private coordinatorに対する復旧実装。新contractを使うnative adapterは未接続。
- A4所有の `coordinator.rs` への差分は専用recovery moduleの宣言3行だけ。統合時にA0/A4のmodule登録と合わせる。
- native `AtomicCourtStore` の一つのDB transactionがcase・資源予約・funding/obligation・journal headを選択する必要がある。生成ファイルは事前prepareできるが、headの選択だけ別取引にしない。旧headのまま孤立したgenerationは信用を持たない。
- A5のnative料金・返金・credit利用には既存engineの一回限り精算とundoを接続する。ここで再計算した私設meterをnative入金と解釈しない。
- native row stream、P2P IBD、selected-chain reorg、pruning store、multi-process crashへの接続は未完了。本成果物の単体試験をそれらの統合完了と数えない。
- 全世代が履歴を含むため、保存・encodeの累積費用は履歴長に対して二次になる。明示的なprivate上限内の基準実装であり、本番のincremental DB journal/retentionを代替しない。実LLMの性能値は未測定。

## 検証

実行結果は同じディレクトリのA6 validation記録へ保存する。復旧試験では実際のML-DSA署名、Merkle opening、既存TIR courtを使う。模擬の永続化失敗はcommit acknowledgement喪失のfail-closed動作を確認するためであり、実native資金取引の障害試験ではない。

有効化設定は変更なし。新native wire、public preset、Gate Cの許可経路は追加していない。
