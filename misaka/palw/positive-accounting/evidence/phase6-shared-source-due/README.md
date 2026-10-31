# Phase6: A0 shared SourceFeeDeltaIntent

2026-10-31。A0 `c2e93ec49` を正式取込み（A5 `3d191fbef`）、A5のローカル7-field型を
`pub use misaka_palw_positive_contract::funding::SourceFeeDeltaIntent` へ置き換えた。
既存 `dues::SourceFeeDeltaIntent` のimport pathと全field型を維持する。
shared型はCopy/Clone/Debug/EqのDATAであり、wire/domain/authorityを追加しない。

## 最初の依存不一致

`common-native-rows.log` / `common-native-rows-result.json` は最初の4件成功・1件失敗（exit 101）を保存する。
A0 `c2e93ec49` 同梱のPacketReceipt追加assertは、先行のA0 `fa1c73f18` を前提としていた。
A5はその下限修正が未取込みで、inner `EXACT_PART_HEADER_BYTES` を含まないpacketを受け入れていた。
テストや下限を弱めず、A0 `fa1c73f18` を正式取込み（A5 `cc45ad4ae`）で修正した。
最初の失敗ログ・JSONは編集・置換しない。

## 修正後の検証記録

`*-rev2-result.json` と同名raw logにcommand、環境、base、両A0 commit、source SHA、exit codeを保存する。
共通native_rows、node no-default全lib、node no-default全target Clippyを順に実行する。
試験中sourceを変更せず、終了後に各結果を確認する。

| 修正後の記録 | 結果 |
| --- | --- |
| `common-native-rows-rev2-result.json` | 5件成功・exit 0 |
| `node-all-rev2-result.json` | 44件成功・exit 0 |
| `node-clippy-rev2-result.json` | no-default全target、`-D warnings`、exit 0・警告なし |

全recordのsource SHAは検証後のsourceと一致し、実行中変更もない。
この検証はshared DATA型の接続と既存検査の回帰であり、actual VP/native writer/UTXO batchを接続しない。
phase5以前の候補の試験数と合算してnative全体の成功数にしない。

独立read-only sourceレビューでは7field一致、同じ旧import path、authorityの追加なし、
PacketReceiptの下限強化を確認した。結果の詳細は後続の独立review JSONに記録する。
