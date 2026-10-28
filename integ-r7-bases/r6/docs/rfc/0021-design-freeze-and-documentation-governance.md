# RFC-0021 — 設計凍結・例外変更手続きと文書の役割分離

- **Decision:** Accepted — 今回のユーザー指示を手続き化。文書化日 2026-10-17。
- **Scope:** 文書・設計の変更管理。
- **Implementation:** 文書と検査ツール。
- **Verification:** [検査結果](../maintenance/validation-report.md)。
- **Activation:** 合意有効化なし。
- **Related:** [ADR-0181](../adr/0181-design-freeze-and-controlled-change.md)。

## 方針

採用済み設計は原則凍結する。変数・パラメーターは設計を保つ範囲で証拠・影響・試験・承認を残して調整できる。重大問題で設計を変える場合は新規 RFC と新規 ADR の両方をレビュー・明示的に採択する。
[正式手続き](../governance/design-freeze.md) / [台帳](../governance/parameter-register.md)。

## 文書の役割

README は目的別入口、STATUS は適用範囲、PRINCIPLES は原則、Spec は条件付きの規範、ADR は採用理由、RFC は技術検討、design は詳細設計、testing は方法、audit/evidence は観測、history は旧版とする。分類だけで採用・撤回・有効化しない。

## 移行・検査

既存主文書の番号・パスを保持し、補助資料を分ける。移動パスと前後の hash、原文を保持する。番号重複は完全パスで台帳管理し、新しい衝突を禁止する。既存の欠落資料を推測で作らず、新しい不備を検査する。

この整理版で添付索引の次番号 0021 を使用した。他ブランチ予約を統合時に照合する。
