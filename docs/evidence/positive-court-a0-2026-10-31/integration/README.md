# A0 incremental integration evidence

2026-10-31。全体完成の単一証拠ではなく、各fileのcommand・source snapshot・exitを確認する。

| 証拠prefix | scope / 結果 |
| --- | --- |
| `a1-court-20261031T114816` | A1 10＋旧Court7 PASS。manifest driftを別fileへ保存、最新全体成功としない |
| `contract-accounting-20261031T122701` | 当時のcommon/accounting testsとClippy。候補hashはJSON参照 |
| `native-rows-20261031T123452` | row初版tests成功、test内useless_vecでClippy失敗。原log保持 |
| `native-rows-final` | 上記Clippy修正後成功。次のreviewでcursor/状態矛盾を補強 |
| `native-rows-review-r2` | row共通29 PASS・Clippy成功、source同一 |
| `p-attestation-common-20261031T124350` | P所見後35 PASS・Clippy成功、source同一 |
| `primitive-record-common-20261031T125130` | record後41 PASS・Clippy成功、source同一 |
| `core-rows-fence-check-20261031` | core library typecheck成功、lifecycle sourceのrun中変動あり |
| `core-rows-fence-tests-typecheck-20261031` | core library/test sources typecheck成功、対象source同一。runtime未実行 |
| `consensus-positive-dispatch-typecheck-20261031` | consensus library typecheck成功、対象dispatcher source同一 |
| `positive-fence-eight-tests-20261031` | 専用target初回はimport/private visitor呼出のcompile失敗 |
| `positive-fence-eight-tests-r2-20261031` | public fence8 PASS、source同一。private visitor直接試験はunit側へ残す |
| `native-source-read-port-typecheck-20261031` | source read API core typecheck成功、source同一。production native duty成功ではない |
| `a2-protected-tail-readonly-review` | 修正fd9の独立コード/証拠hash照合。再buildではない |
| `receipt-semantics-common` | fa1のreceipt byte/work範囲後41 PASS・Clippy成功。後続patchとは別 |
| `post-restore-common-20261031` | A6 preflight追加後44 PASS・Clippy成功、対象source不変 |
| `core-court-restore-typecheck-20261031` | A4選択統合直後、opaque identity不足と未統合wide参照4件でexit101。原log保持 |
| `core-court-restore-typecheck-r2-20261031` | 上記修正後のcore --lib型検査exit0、対象source不変。後続patchやruntime成功を含まない |
| `private-recovery-accounting-20261031` | private readonly復旧receipt追加後のleaf会計44 PASS・Clippy成功、source不変。native復旧権限ではない |

公開有効化、実UTXO lock、Pの実抽選、全consumer、native restart/reorg/IBD、実LLM資格は未確認。fixtureのlocal duty/storeや署名DTOをnative権限へ変換しない。debug/stack等の実行条件も各JSONを参照し、本番性能・stack/RSS保証に使わない。
