# Phase 3: 正式workspace登録後の候補

実装は `1341730c7`。検証開始時baseは `31831d48c`、未commitの実装sourceをSHA-256で固定し、
終了後も全対象sourceの一致を確認した。正本は `tested-source-manifest.json` と `validated-checks.json`。

- 通常workspaceのnode: **22成功**（実A1/authpacket 7、新signed funding/既存calculator 15）。
- A1/既存Court: **35成功**（13 + 2 + 13 + 7）。
- 共通契約: **24成功**。
- leaf: **40成功**（単体35 + 独立oracle/consumer 5）。
- node/leaf Clippy: `--all-targets --no-deps -- -D warnings` 成功。

共通/leafと一部lintの原記録は、同じcomponent source hashを照合した別候補の成功記録を参照する。
nodeとA1/Courtは正式登録後の通常 `cargo test --locked` をこのcheckoutで実行した。
別branchの異なる実装の成功を合算したものではない。各コマンド・環境・終了code・参照logは集約JSONに記録。

直前の記録は上書きしていない。

- `../independent-phase3/`: intent期限の追加反例前。node21/common24/leaf40/lint成功後、
  A1/Courtの再buildがディスク不足でexit101。これを回帰成功には数えない。
- `../independent-phase3-rev2/`: 狭いintent期限の修正と反例を追加し、node22とnode lint成功。
- このdirectory: A0の正式workspace/lock登録後にnode22・A1/Court35を成功させた候補。

native live decoder、実UTXO/coin lock、Pの実drawn Certified、全native consumerへの物理接続、
DB reopen/reorg/IBDの合同試験の完成を示すものではない。新funding preflightはactual outputの検査までで、
policy/schedule resolverやnative受付権限へ変換しない。
