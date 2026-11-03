# A7 — 8用途entitlement fixture source-ready（2026-11-01）

状態: **SOURCE_READY / NOT_TYPECHECKED_OR_EXECUTED / DEPENDENCIES_NOT_MERGED**。

A5 ready source `30f3fd23e4db6d494d4f511a2addad5e736df851`（parent `dd8efe1ab1034555e30b828d9471b7e2503e1dcd`）へAPIを再照合し、A7の `tests/pipeline.rs`、`tests/pipeline_support/mod.rs` とREADMEを更新した。旧pending案とpatchはA7 branchの `4b68974b5` に準備時点の履歴として保持し、現在の適用状態は本報告を正本とする。

依存2 commitはこのA7 branchへmergeしていない。更新はA0が受理した8用途APIへ接続するためのA7専用source差分であり、現在の旧production baseでのビルド成功を主張しない。A0の全candidate指定後に、依存・lock・専用fence targetを再照合し、その新しい同一候補で型検査とruntimeを実行する。

| 用途 | 明示既存primary oracle | 明示paid-worker oracle | entitlement port未実装 |
|---|---|---|---|
| Reward | 許可 | 拒否 | 拒否 |
| WorkReceipt | 許可 | 拒否 | 拒否 |
| DagWeight | 許可 | 拒否 | 拒否 |
| RuleE | 許可 | 拒否 | 拒否 |
| Settlement | 許可 | 許可 | 拒否 |
| PrefixReuse | 許可 | 拒否 | 拒否 |
| Dependency | 許可 | 拒否 | 拒否 |
| WorkerPayment | 拒否 | 許可 | 拒否 |

許可欄にも実A1のchecked terminal、Positive、P、fraud・資金・満期等の共通条件が必要。これらの成功から権利は生成しない。二つのimmutable oracleは用途ごとに既存権利を宣言し、完全な `CreditBinding` と照合する。fee private bookをprimary源にしない。資料の既提供サービス料・安全な返金は成功worker gateと別のreceipt・liability・資金・一回性条件を維持する。

実bound A2 → A3 → A1 → `TirCreditView` の6試験を更新し、paid限定権利とport未実装拒否の2試験を追加した。登録したpipeline試験は計8件で、まだ実行していない。期待用途はliteralな8集合として固定し、exportされた集合の欠落・追加を検知する。paidのworker支払と決済は、両方許可される場合にも戻り値の `usage()` と完全な `binding()` を個別照合する。bindingのscope/claim/case/result/output/dependencies差替えも拒否する。scope変異はbranchを代表として使い、oracle自体はscope全体を比較する。

確認: ready APIの3 source SHAはpending準備時の観測値と一致。別担当がdefault拒否・TirCreditView転送・primary/paid分離・用途とbinding照合を独立レビューした。repo設定によるrustfmt check、`git diff --check`、patch reverse-checkは成功。Cargo・追加target作成・production変更は行っていない。

[API/source照合記録](../../testing/positive-court-a7/evidence/entitlement-source-ready-2026-11-01/review.json)。歴史的243件のraw/digestと対象 `bd9623914` は変更せず、A5 owner側の成功件数も今回のruntime結果へ合算しない。native権限・公開activation・model tierは変更しない。
