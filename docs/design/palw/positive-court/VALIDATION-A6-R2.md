# A6 R2 — 並列実装と検証記録

2026-10-31。branch `codex/rfc42-a6-recovery`。最初の記録は [VALIDATION-A6.md](VALIDATION-A6.md) に保持する。担当はDB保存・同期/保持・独立監査の3 agent、親はA4/A5取込み、checked prepare、Node向けadapter、統合検証を担当した。

## 同じ候補での検証

対象実装commit: `66bef38cc`。**184 passed / 0 failed**（core 84、database 30、production結合11、共通contract 13、A5基盤29、既存court/security回帰17）。

結果とsource hashは [R2 evidence](../../../evidence/positive-court-a6-r2-2026-10-31/) に保存する。前のcandidateや別branchの結果を最終成功件数へ合算しない。

- Positive Court全回帰: [core-tests.log](../../../evidence/positive-court-a6-r2-2026-10-31/core-tests.log)。8,193語彙caseをskipせず実行し、最終候補で217.48秒、全84件成功。
- production adapterと全database試験: [storage-tests.log](../../../evidence/positive-court-a6-r2-2026-10-31/storage-tests.log)。database全30件、production結合11件が成功。実RocksDB、実TIR、実process exit、二つのDBへの再検査importを含む。
- 共通contractとA5会計基盤: [contract-accounting-tests.log](../../../evidence/positive-court-a6-r2-2026-10-31/contract-accounting-tests.log)。
- 既存TIR courtとsecurity certification: [legacy-tests.log](../../../evidence/positive-court-a6-r2-2026-10-31/legacy-tests.log)。最終候補の同じCargo test executableで実行。
- format、`git diff --check`、workspace metadata登録を確認。Nodeは新しいcrateを依存として登録し、そのAPIをreexportする。full kaspad link/node network drillの成功とは数えない。
- 文書検査: [docs-check.log](../../../evidence/positive-court-a6-r2-2026-10-31/docs-check.log)。既知baselineは更新しない。

```sh
CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-testnet/target CARGO_INCREMENTAL=0 \
  cargo test -p kaspa-consensus-core --lib palw_positive_court_v2 \
  --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.dev.package.kaspa-consensus-core.debug=0' -- --test-threads=2

CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-testnet/target CARGO_INCREMENTAL=0 \
  cargo test -p misaka-palw-positive-recovery -p kaspa-database \
  --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.dev.package.kaspa-consensus-core.debug=0' -- --test-threads=2
```

source hashは [source-snapshot.json](../../../evidence/positive-court-a6-r2-2026-10-31/source-snapshot.json)、workspace/Node依存登録は [metadata-summary.json](../../../evidence/positive-court-a6-r2-2026-10-31/metadata-summary.json) を参照。

ignoredにした2つのcrash workerは親testが別processとして実行し、sync commit直後のexit code 86と再起動後の状態を検査する。既存database doc例のignoredは実行成功に数えない。

## 独立監査で閉じた反例

- journalが満杯になるとexpiry/返金もBudgetで停止する → quotaと独立したtrusted clock projection、clock-only DB transaction。
- transport設定済みstateをfresh admissionへ混入 → pristineで拒否。設定/packet/無効packet消費/一時bufferをjournalから再生。
- 永続化callbackのpanic後、imageだけ進んだruntimeを継続 → persist/applyの前にpoisonし、通常return時だけ解除。
- current clockを2回読むことで検査とreplayが逆行 → read-only restoreは一回のsnapshot、公開時は現在clockを別途再照合。
- startupのexpiryだけ進み料金行が旧clockのまま → `restore_and_commit` が一つの同期取引でprojectionを確定してから返す。
- prepare/tokenをDAA6で保持しDAA100で公開 → native current clock一致を必須化し、project前に拒否。
- `a`と`a\x01`のnamespace prefix衝突で他caseのactive行をpurge → 長さ付きnamespace、正規transaction key長、別namespace保持の独立負例。

重い語彙fixtureの算術・checkerは変更していない。test fixtureが保持するinventoryからcanonical multiproofを直接構築し、不変のtreeをopeningごとに繰り返し再構築する費用を減らした。既存のsingle/run/none opening byte一致回帰を同じ候補で実行する。

## 達成範囲とnative結合

A6のnode-local保存・復旧・同期adapterと実DB/実Court結合を検証した。nativeの具体的なTrustedAnchor、実UTXO/署名consent authority、同一DAA reorgも識別するstate generation、native row/state-root/P2P dispatcherは [A6-NATIVE-CONNECTION.md](A6-NATIVE-CONNECTION.md) の接続条件に従ってA0/A4/A5が供給する必要がある。現在時計の一致だけを現在branch/generationの一致と扱わない。

料金行の試験は実RocksDB上のprivate projectionであり、native UTXO支払の実績ではない。複数DB試験は実Node間P2P試験ではない。未接続native creditの拒否、public preset/activation変更なし、暗号学的なモデル計算証明の不採用、モデル二階層の凍結を維持する。
