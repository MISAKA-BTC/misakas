# ADR-0182 — 画像・動画経路とモデル改善を mainnet 後へ延期し、当面無効にする

- **Decision:** Accepted — 追加のユーザー指示。文書化日 2026-10-17。
- **Scope:** RFC-0003 の画像・動画経路、および RFC-0004 のモデル改善の開発優先度・初期提供・有効化時期。
- **Policy status:** DEFERRED_POST_MAINNET / DISABLED。
- **Implementation:** 方針を文書へ反映。コードの実際の無効化は未確認。
- **Verification:** 無効化・回帰試験は full checkout で実施が必要。今回 node 試験なし。
- **Activation:** 当面なし。mainnet 開始時にも自動有効化なし。
- **Related:** [RFC-0022](../rfc/0022-post-mainnet-media-and-model-improvement-scope.md)、[RFC-0003](../rfc/0003-palw-generative-model-classes.md)、[RFC-0004](../rfc/0004-palw-model-improvement.md)、[PRINCIPLES §12](../PRINCIPLES.md#post-mainnet-scope)。
- **Amends:** 上記機能を初期 mainnet / 次回の全面有効化の対象として扱う旧予定・完成条件。この範囲外の技術設計は置換しない。

## 決定

画像・動画の経路とモデル改善は mainnet 稼働開始後に取り組むものとして延期し、当面無効とする。初期 mainnet の必須実装・提供・有効化対象に含めない。mainnet 後も機能別レビューと明示的承認まで開かない。

## 採用理由

ユーザーが当面の提供対象を絞り、画像・動画・改善を後段に置く方針を指定したため。初期リリースの対象と責任を明確にし、過去の「全 RFC 完了」や共有 fence からの誤った同時有効化を避けるため。重大な欠陥が見つかったと仮定した決定ではない。

## 比較した扱い

初期 mainnet で同時に完成・有効化する扱いは今回の指示と一致しない。設計と証拠を削除する扱いは再開に必要な根拠を失う。mainnet 日付で自動的に開く扱いはレビュー・試験・承認を省略する。このため、資料を保持した明示的延期・無効化を採る。

## 帰結・留保

共通のテキスト・TIR・登録の不変性・Gate C・court・bond 会計を維持する。共有部分の欠陥が初期 mainnet に影響するなら release blocker のままであり、延期機能が閉じていることの確認も必要。

旧履歴・旧 claim の責任を遡及的に消さない。一般のオフチェーン研究、通常の固定モデル登録、既定の年次カタログ、RFC-0020 の較正・メモリ改善を RFC-0004 の protocol と同一視しない。

正本は [延期・無効化方針](../governance/post-mainnet-scope.md)。再開時の構造変更は ADR-0181 の凍結規程に従う。この番号は整理版内で新規採番したもので、統合先の予約は別途確認する。
