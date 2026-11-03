# Phase7: committed private case receiptのread-only取得

2026-10-31。`PrivateAccountingStore::recover_committed_case(intent)` を追加した。
共通AtomicCourtStoreとnative storeは変更していない。

| 実行記録 | 結果 |
| --- | --- |
| `leaf-all-result.json` / `leaf-all.log` | leaf全44件成功（unit39、consumer1、独立graph2、独立料金2） |
| `leaf-clippy-result.json` / `leaf-clippy.log` | 同leaf全target、`-D warnings`、exit 0・警告なし |

command/env/base/source SHAと実行中source不変を各JSONへ保存する。
新4試験ではauthorityのRefCellをmutable borrowして呼出不能にしてもgetterが成功すること、
期限後の既存case取得とnew admission拒否、book/epoch/head/supply不変、全9 literal fieldの差替え拒否、
同内容foreign storeへのhandle転用拒否、undoNone、実pay/closeの一回性と元undoでのLIFO rollbackを検査する。

opaque receiptはprivate bookの現caseを指すだけで、native復旧のauthorityではない。
A4のcurrent root/generation/署名Result再検査、actual history/funding/lease、A6の同一batch復元は別に必要である。
独立read-only reviewは `independent-review.json` に保存する。
