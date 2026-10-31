# Phase11: strict Positive restoreとA5 row fixtureの整合

2026-11-01 JST。production基点は `79c3f9bcae0fde86940fd65cbcbfa2018a72cc21`。
同じA5 workspaceで `codex/a5-row-restore-fixtures` を作り、旧A5 branch `codex/positive-court-a5` / `30f3fd23e` は保持した。
変更するRust sourceは **`positive-accounting-node/src/rows/tests.rs` だけ**。
Rootの後続wide/A6差分やnative Assignment案を取り込んだ検証ではない。

## 報告された失敗と原因

A7 HEAD `7dd56d8cd1e31b684a97e35c6650eff1b04fab80` のstage05は49成功・6失敗で、run全体は
`FAILED_OR_INCOMPLETE`。その実行をA5が再実行した失敗とは扱わない。
`reported-failure-05.log` はA7原ログの無改変copy、`reported-failure.json` は原path・SHA・結果と、
production79c3/A7 recordのcore/fixture 5 source SHA一致を保持する。A7原本は変更していない。

6件はすべて `dto_state(...).into_state().unwrap()` で失敗し、A5 reader/gateへ未到達だった。
新coreが不完全・破損snapshotのrestoreを拒否した結果であり、productionの検証を緩める修正はしない。
従来の成功fixtureにも次の不整合があった。

- JournalHead欠落。
- 要求unionがないのにMinerDataStatus::Pending。
- witness attempt0なのに使用attempt数0、Result/Witnessが同じevent位置。
- Matched cursor10なのにPacketReceipt0行、unsigned frame bytes1000で必要下限3000未満。

## fixture修正

正常fixtureは署名付き基本6行＋JournalHead1行＋PacketReceipt10行の17行とした。
NotRequested、attempt数1、admission5 < witness6 < packet7..16、journal sequence12/history origin16を揃える。
各packetはunsigned300（header284＋inner16）/signed4927、coverageはunsigned3000/signatures10。
実restore reportの17行・1case・1witness・3署名検査をassertする。
実stateのtyped readerへ渡してもDATAであり、全8信用用途はnative owner欠落として拒否する。
fixtureのorigin・journal参照・packet preimageはnative履歴/実算術/P認証/用途別権利/実資金資格を成立させない。

不完全・破損fixtureは、実restore validatorと公的carriage importの両方による拒否を検査する。
missing/malformed/signature/linkの診断は実codec/core/shared validatorと既存A5分類helperで確認し、
壊れたデータをnative stateへ注入するsetter・unchecked constructor・unsafe・成功stubは追加しない。

## 修正後の実行

| 記録 | 結果 |
| --- | --- |
| `node-full-result.json` / `node-full.log` | production79c3＋fixture修正の通常Cargo package試験、55成功・0失敗、doc0 |
| `node-clippy-result.json` / `node-clippy.log` | 同候補のall-targets、`-D warnings`、exit 0・警告なし |

command/env/production tree/source SHA/実行中source不変を各JSONへ保存する。
A7 stage05と同じcore opt0のdev/test設定を用い、A5既存targetだけを使用した。
容量調整は停止確認後のA5旧core cache成果物だけで、対象を `own-cache-cleanup.json` に記録する。
Root/A7のtargetや元の失敗記録は変更していない。既存native archive指定はread-onlyで、C++再buildを行わない。
独立read-only source/raw/hash監査は `independent-review.json` を参照する。

この結果をA7 pipeline全体やRoot currentの成功へ加算・転記しない。
actual native writer・coin lock・資格・全consumer・復旧/実LLMの完成を示すものではない。
