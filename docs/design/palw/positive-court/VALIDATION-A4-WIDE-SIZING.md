# A4 — wide Q24 local sizing の検証記録

2026-11-01。実行対象は `13e1fe21337a81fedfc4353e1066fae3d0583c58`
（tree `9bc69b389b4d453800c8aaf0ccd54df13689c09d`）。
[実行・hash・範囲の記録](../../../evidence/positive-court-a4-wide-sizing-2026-11-01/owner-13e/audit.json) に、
source、環境、RocksDB archive、raw log、6本の実行binaryのhashを保存した。

| 同じ source の実行範囲 | 結果 |
| --- | --- |
| 新しい TIR V2 sizing と旧入口拒否 | 13 PASS |
| scalar overflow / wire 長境界 | 2 PASS |
| V2 bound を旧 admission に渡せない compile-fail doctest | 1 PASS |
| 既存 TIR close size | 1 PASS |
| 既存 TIR close range | 3 PASS |
| 既存 GEN close | 5 PASS |
| GEN close price 全14件、default stack | 最初の既存 Params 関連テストで stack overflow、SIGABRT |
| GEN close price の非 Params subset | 12 PASS、Params 関連2件を明示 skip |
| Core lib と新targetの strict Clippy | `collapsible_if` 1件で exit 101 |

default stack の全14件実行は未完了である。
[初回 raw log](../../../evidence/positive-court-a4-wide-sizing-2026-11-01/owner-13e/first-default/07-legacy-gen-close-price.log) と
[subset / Clippy の記録](../../../evidence/positive-court-a4-wide-sizing-2026-11-01/owner-13e/nonparams-clippy/validation.json) を分け、
subset を全14件の成功へ昇格させない。`RUST_MIN_STACK` は設定していない。

Clippy の診断は `guard_allocation` の入れ子 if だけだった。lib 段階で停止し、新targetの lint 完了も未確認。
owner は `1a23ce8f25f5eb522e56c5cb7433915ad1fd1721` で同意味の let-chain に修正し、
整形・差分検査を行った。A4の重い build 枠を release したため、この修正後の runtime / Clippy は未再実行であり、
Root の統合候補で別途検証する。公開 API、価格、上限、試験入力の変更はない。

owner 修正後の軽量 Docs 検査は exit 0、既知43件・新規0件。
[検査ログ](../../../evidence/positive-court-a4-wide-sizing-2026-11-01/owner-13e/docs-check-after-style-fix.log) を保存した。
これは runtime / Clippy の再検証ではない。

実cacheは既存 `target/a4-fresh`。jobs 2、debug 0、incremental 無効、Core opt-level 0。
指定された既存 RocksDB archive の SHA256 は実行前後とも
`6eae541967b2cb5345ace55eadb34b2c490e91acfd8a4e2da5822a60f91af3a2`。
旧 final-r5 の25本は gzip 保全・復元byte比較済みで、
[保全記録](../../../evidence/positive-court-a4-wide-sizing-2026-11-01/owner-13e/historical-preservation.json) を保持する。
過去の `c1292bc08` の結果は、この候補へ合算していない。

対象は TIR-only の local sizing と mutable V1 DATA 入口の幅拒否。
GEN/composite/native の V2 受付、全wide Courtの実carriage、実lease・料金・決済・公開有効化の完了を示す記録ではない。
