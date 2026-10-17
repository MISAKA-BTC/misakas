# 文書整理・検査結果

**入力:** ユーザー提供 `docs(1).zip`。**整理日:** 2026-10-17。

## 結果

| 項目 | 結果 |
| --- | --- |
| 追跡・保持した元資料 | 2,481 ファイル |
| 移動した元資料 | 578 ファイル |
| docs直下 | 103 → 5 ファイル |
| ADRの入口README | 165,955 → 2,364 bytes。完全索引・履歴は別に保持 |
| 元のMarkdown | 660 件を元バイトでZIP保存・照合 |
| ログ・JSON・バイナリ等 | 1,821 件が移動後もバイト一致 |
| 記録したURL書換え | 1,624 箇所。ほかに既存見出しの互換aliasを2件追加 |
| 新規のリンク不備 | 0 件（限定パーサーによる回帰検査） |
| 元資料から残る欠落参照 | 43 箇所 / 19 参照先 |
| 文書ツールunit tests | 16 tests PASS |
| 安全性・negativeテスト | 6 checks PASS。使い捨てコピーで実施 |
| node・稼働設定 | 変更・build・有効化・拒否試験は行っていない |

## 反映した方針

[設計凍結](../governance/design-freeze.md)をADR-0181 / RFC-0021 / PRINCIPLES §11に記録しました。設計変数・パラメーター以外は通常変更禁止、重大問題時には新規RFCと新規ADRの両方のレビュー・明示的採択を必要とします。

[画像・動画・モデル改善の延期](../governance/post-mainnet-scope.md)はADR-0182 / RFC-0022 / PRINCIPLES §12、RFC-0003 / 0004、Spec、STATUS、索引、readiness/統合/release資料に反映しました。mainnet後へ延期し、当面無効、mainnet開始での自動有効化なしです。テキスト・固定登録・共通の検証・会計は別扱いです。

**無効化の方針と、runtimeで既に無効であることは別です。** 今回の入力はdocsだけのため、後者は未確認です。node側で対象経路・共有fence・別入口の拒否と回帰を実装照合・試験する必要があります。文書のチェックをその証拠にしません。

## 検査の読み方

標準モードは **新しい不備を追加していないこと**を検査します。旧来の不備は原文scanと照合し、[既知課題台帳](known-link-issues.json)に記録しました。全リンク正常・全仕様の意味を検証済みとはしません。`--strict`は既存不備も失敗とし、今回も非ゼロ終了を確認しました。

[機械出力](validation-summary.json) / [unit・反映ツールの検査記録](tool-test-results.json) / [実行手順](README.md)。

外部URL 226 件とdocs外参照 108 件は取得・検証していません。完全なCommonMarkパーサーではなく、GitHub見出しfragmentは近似です。コードの実在・行番号・承認の真正性・安全性・実際のfenceは別途照合します。

## 元資料にある欠落参照先

本文が添付資料にないため推測で再構成していません。番号だけで別の文書へ接続することも避けました。

| 元資料の参照先 | 現在の参照箇所 |
| --- | --- |
| `docs/adr/0067-a-registered-class-is-served-by-any-node-that-holds-its-artifact.md` | 1 |
| `docs/adr/0071-hash-coupling-and-the-remainder.md` | 2 |
| `docs/adr/0072-the-attempt-is-a-function-of-the-execution.md` | 1 |
| `docs/adr/0098-a-panels-coverage-of-one-claim-is-a-number.md` | 2 |
| `docs/adr/0107-share-growth-is-counted-at-final.md` | 1 |
| `docs/adr/0112-the-weight-residency-budget.md` | 1 |
| `docs/adr/INVENTORY.md` | 1 |
| `docs/design/palw/collateral.md` | 5 |
| `docs/design/palw/lineage.md` | 2 |
| `docs/design/palw/verification.md` | 2 |
| `docs/palw-rc-testnet12-launch-runbook.md` | 1 |
| `docs/spec/palw/00-index.md` | 13 |
| `docs/spec/palw/04a-integer-arithmetic.md` | 1 |
| `docs/spec/palw/07-claim-lifecycle.md` | 2 |
| `docs/spec/palw/08-verification.md` | 1 |
| `docs/spec/palw/09-court-and-offences.md` | 1 |
| `docs/spec/palw/10-collateral-and-economics.md` | 2 |
| `docs/spec/palw/16-network-parameters-and-fences.md` | 3 |
| `docs/spec/palw/divergences.md` | 1 |

## 番号・原文・反映

既存ADR-0026 / ADR-0173の衝突とSpec18番の共有を完全パスで識別しました。新規ADR-0181/0182、RFC-0021/0022は添付ZIP内の次番号に基づき、統合先の別予約は未確認です。[番号台帳](numbering.md)。

[移動manifest](migration-manifest.json)は元/新パスと前後SHA-256を記録します。[原文ZIP](../history/documentation/2026-10-17/original-markdown.zip)には全元Markdownを保持しました。既存の数値・コード・ログを推測で書き換えていません。

同梱の反映ツールはdry-runが既定で、元ファイル変更時は停止、明示適用時は全体バックアップと未知の追加ファイル保持を行います。docs外の旧パス参照があればコードを自動変更せず停止します。[反映手順](REORGANIZATION.md)。

GitHubへのcommit/push、リポジトリルートのAGENTS.md / CLAUDE.md、既存CIワークフローの編集は今回行っていません。
