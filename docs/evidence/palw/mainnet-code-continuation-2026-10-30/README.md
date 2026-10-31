# mainnetコード継続作業 — 2026-10-30

状態: **実装修正と同一候補の対象検証を実施。新供給契約・RFC-0037の接続は未完であり、全11項目の完了宣言ではない。**

基点は `7aa60b6fa46a51c065424f7537dc5c6ab2ed0f85`、既存worktreeのbranchは `codex/g14-17400-release`。
現在のmainではなく、ユーザーが挙げた3試験を含む継続候補で作業する。
開始時の未コミットproducer修正と、並行して更新されたrouted-material処理は保存している。
この作業で既存ネットワークへの配備やDAAの有効化は実施していない。

## 実装差分

| 対象 | 今回の実装・確認 | 未完または確認待ち |
| --- | --- | --- |
| RFC-0039 | verifier/providerの物理支払queue満杯・合算overflow時に全額保留し、義務を消費せず再試行。復旧・一回性・rollback試験追加 | 署名・保留・混雑後の支払・返金・一回性・rollbackの対象試験通過 |
| 有料Court・材料徴収 | 既存native funding/escrow経路を確認。parameter/promptの実材料需要を受付とpreclaimで共有し、scratchも予約 | 対象試験通過 |
| Court全分岐予約 | 新供給契約の双方署名・scope/inventory/byte上限/有限期限/profile束縛を休眠コード化 | native供給義務、全分岐install、semantic/tokenizer/生成後stopの供給・default・精算接続は未完。下記提案の採択が必要 |
| RFC-0023 | 保存済み出生に基づくcarrier/prefix依存閉包、退役した有罪源泉のcredit拒否、独立claimの維持 | 対象試験通過。出生証拠不足は独立扱いせずHold |
| RFC-0040 | 既存prefix exportの認証・材料予約と、退役後の依存を確認 | 対象試験通過 |
| RFC-0012 | 既存native39抽出・native settlement変換・選択chainでの失効反映を確認 | 対象試験通過 |
| RFC-0037 | canonical root writerからcollection出力、投影class codec、kernel/panel/budgetの逐次codec、typed native予約、局所capacity帰責、旧versionへの送受信拒否 | 新version未採択。実peerのserve→受信→全体import、wire chunkingは未完。versionはNoneで休眠 |
| RFC-0038 | 壊れたwinning carrier索引を索引欠落として扱う経路を拒否 | 同一候補で6試験通過 |
| RFC-0041 | genesis timestampから30分待機、回復時のartifact/PWU/lottery検査、zero genesis限定の起動検査。coinbase→通常登録→既存成熟期間→PALW/Panel試験追加 | 実coinbase支払・8件の通常登録・既存成熟期間・PALW/Panel成立の統合試験通過 |
| 一括検証 | core/consensus/kaspad/P2P flows/P2P libを同じ差分でビルドする | 5パッケージのlib試験ビルド、通常kaspadバイナリ、選定した179個の一意な試験が成功。未接続機能の完成やworkspace全試験の合格は意味しない |

新しい供給役の具体案は[契約提案](../../../design/palw/integration/g14-17000/family-supplier-contract-proposal-2026-10-30.md)。
旧custody署名を新しい義務へ流用せず、未定の費用をゼロとして受付を通さない。
RFC-0037と新供給契約に必要な採択確認をユーザーへ提出済み。未回答は承認ではない。

## 検証記録

成功したビルド:

```sh
cargo test -p kaspa-consensus-core -p kaspa-consensus -p kaspad -p kaspa-p2p-flows -p kaspa-p2p-lib --lib --no-run
cargo build -p kaspad --bin kaspad
target/debug/kaspad --version
```

作成された同じ試験バイナリで31群を実行し、延べ181件、重複を除く **179件が成功、失敗0件**。
既存のrelease測定専用 `material_intake_measure_wall_time_and_retained_bytes` 1件はignoreのまま未実行。
これは実測値・金融profileの承認ではない。試験環境は `RUST_MIN_STACK=67108864`、test threads=2。
[実行結果](results.json)、[重複除去した一覧](unique-passed-tests.json)、[検証集計](verification-summary.json)、
[実行したスクリプト](run.py)、[一括実行log](runner.log)を保存した。

前回止まっていた3試験も今回の候補で成功:

- `finite_provider_fee_horizon_default_and_original_contract_are_separate`
- `rfc40_export_reservation_survives_source_terminal_and_releases_only_after_material`
- `rfc23s2_checkpoint_admits_before_build_and_retains_until_drop`

[unified build](unified-build.log)、[通常node build](node-build.log)、[バイナリ情報](node-build.json)、
[37ファイルのsource digest](source-verified.json)を保存した。試験前後のsource差分は0件。
同一候補は基点commitと未コミット差分の組であり、mainへのmergeやcommitは行っていない。

編集と重なった古いmetadata/API参照、および新試験の型誤りで失敗したbuildも、
[初回](initial-build-failed.log)、[編集途中](editing-build-failed.log)、
[最初の安定build](first-stable-build-failed.log)として元の出力のまま保存した。
失敗したbuildを合格や実行済み試験として数えていない。

新規供給役の金融profile対応、timely/late default、未使用・未解決時の支払・返金は未決。
旧pledgeの義務を拡張せず、新契約のnative installは行っていない。
RFC-0037は新version未採択であり、P2Pのserve・分割転送・受信・全体importの実peer接続は未完。
`PROTOCOL_VERSION_PALW_ROW_STREAM=None` のままで、既存106〜108へ新方式を送受信しない。
生成codecの384 MiB上限と、既存P2Pの320 MiBメッセージ上限を混同せず、ネットワーク上限は変更していない。

この2範囲は[docsの作業規約](../../../AGENTS.md)が要求する「設計責任者の明示的採択」が必要であり、
採択確認は未回答。コード・統合の完成として閉じていない。
画像・動画、モデル改善は初期mainnetの対象外。監査・長時間稼働・障害復旧・移行・公開配備判断は本記録のコード試験と別である。
