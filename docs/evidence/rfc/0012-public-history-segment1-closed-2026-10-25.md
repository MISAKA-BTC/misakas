# RFC12 公開履歴 census segment 1 — bounded partial

元の serial read-only job は2400秒の上限で停止した。unifiedexec51660はexit0、SSH PID23593は不存在。実scriptの停止記録は`bounded 2400 second budget exhausted`、elapsed 2401.404627秒、その後の`after_error` stampは成功。このsegment中にnode再起動・設定変更は行っていない。

- 原raw SHA256: `e65938258f04bcc9ae8276ff6ac4e50fe9c74f1e72adb5670b35818bc59eb7c5`
- 原raw: 1,685,695,025 bytes / 376 complete pages
- unique headers: 64,680 / 最大DAA8896
- algo10 round headers: 24,359
- node verdict: UNKNOWN 24,359、granted/refused観測0、exec欠落0、既知判定矛盾0
- 固定anchor: `af1fbb4db37e5e98106cd75e1f9a0a92d5efff0448734036e620b88286d8041a283cb5e01616630a89db45db7eebdea85a3195a2735f669050f219be223480e2` (DAA15477)
- 最後に保存したinclusive page末尾cursor: `337bbb66cabc860746535dba3baa0d0d0740f5c5714a38eb0d1040752220cf9a08e202835d2f7ef2148ba9fb80dd05ff82a792601641f7d718a2775b7b1f49b0`

**Coverage UNKNOWN_PARTIAL、EVM互換性UNKNOWN。** 固定anchorまで未到達であり、全ancestor closure検査はまだ閉じない。ancestor_holes空配列は全履歴に穴がないという意味ではない。取得した初期区間のunknown判定を、許可無し・拒否・accepted permit 0件とは解釈しない。

前後stampは同一genesis/params/schedule/epoch0/legacy-stable/PP(genesis)、stable=true、固定anchor isChainBlock=trueを確認。afterのDAAは15497。sink/DAA前進自体はidentity変更として拒否しない。header conflict、parse欠落、不完全line、stamp不一致は検出0。これは読み取りの整合性確認であり、upgrade replayの互換性確認ではない。

`palwMergeView.roundBlocks`はalgo10の分類で、permit許可ではない。exec.verdictは4096件のnode-local volatile ledgerから取得するため、restart/eviction後にunknownとなる。派生EVM gas headerをnative RPCで取得しておらず、c5a7修正前30Mと修正後33Mの公開履歴一致を推測できない。

新collectorはresume時に全segmentのidentity/stampを確認し、新しいlocal outputを排他的に作る。既存segmentを変更せず、新segmentへ保存する。frame宣言長をpayload read前に制限し、segment出力量のhard limitとafter stamp余地を持つ。未保存pageのcursorは進めない。`.ndjson.gz`のreaderは展開一時ファイルを作らず3pass streamingでCRC/EOFを検証し、展開SHA/byte数を元raw identityとして、storage SHA/byte数を別に記録する。不正CRC/truncationはUNKNOWN/resume拒否。

Offline合成14tests PASS(.050s)、py_compile/diff-check PASS。RPCを使用しない合成テストであり、公開履歴の完了証拠に加算しない。

## Artifacts

- `/tmp/misaka-public-history-segment1-final-summary-20261025.json`
- `/tmp/misaka-public-history-segment1-final-analysis-20261025.json` (全verdict観測を含む)
- `/tmp/misaka-public-history-segment1-terminal-20261025.json`
- `/tmp/misaka-public-history-offline-tests-20261025.log`

原rawの展開SHAを完全検証したgzipを `/tmp/misaka-public-history-census-20261025.ndjson.gz` に保全し、元rawの重複ファイルだけ削除した。gzip 554,675,756 bytes、SHA256 `7f9d1ab499eab6b28e99ccfa77beb897148821c39dd9bc53d1bd884d39c94ece`。展開SHAは元rawと同じである。上記summaryの元raw pathは取得当時の履歴であり、現在の保存先はgzipとなる。

[確定segmentの証拠index](0012-public-history-segment1-closed-2026-10-25.json)に取得終了、集計、圧縮検証、tool source hashを保存した。segment 2は同一固定anchorへのread-only再開として別ファイルへ実行中であり、この確定segmentの件数へ先取り加算しない。
