# RFC-0022 — 初期 mainnet から画像・動画・モデル改善を外す適用範囲

- **Decision:** Accepted — 追加のユーザー指示の具体化。文書化日 2026-10-17。
- **Scope:** RFC-0003 の画像・動画経路と RFC-0004 のモデル改善の延期。
- **Implementation:** 文書方針。runtime の無効化は未確認。
- **Verification:** node の拒否・回帰試験は未実施。
- **Activation:** 当面無効、mainnet 開始で自動有効化しない。
- **Related:** [ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md)。

## 1. 要求

対象機能は mainnet 後へ延期し、初期 mainnet の必須実装・提供・有効化から除外する。詳細と境界は [正式方針](../governance/post-mainnet-scope.md) に集約する。旧予定表・実装完了記録は今回の有効化承認ではない。

## 2. 共通経路との分離

RFC-0003 全体を無条件に停止せず、必要なテキスト・TIR・検証の共通部分を残す。共有 fence や別の CLI / RPC / admission / fold から画像・動画が開かないことを確認する。改善を止めても ADR-0175 の不変登録を止めない。

## 3. 検査

通常入口・別入口の対象 job 拒否、改善 epoch / 評価 / 候補選択 / 固有報酬の不進行、共通経路からの迂回不能、再起動・再同期の一貫性、テキスト・認証・予算の回帰を確認する。延期機能の肯定的 E2E 完成は初期 mainnet の条件から外すが、無効化と共通コードの安全性は免除しない。

## 4. 再開

mainnet 後に対象 release と実装・測定・試験・レビュー・互換性・移行を明示し、機能ごとの提供・有効化承認を得る。自動有効化する日付は設定しない。技術構造を変える場合は [設計凍結規程](../governance/design-freeze.md) の手続きを要する。

## 5. 非変更事項

今回 node コード、定数、wire ID、fingerprint、既存 claim の責任を変更しない。音声・embedding 等の採用を新たに承認しない。以前の試験結果を現在の無効化証拠に流用しない。
