# 設計状態の読み方

このページは実装進捗を列挙しない。採択された設計、提案、実装、検証、公開有効化を別に扱う。

| 区分 | 意味 |
| --- | --- |
| DESIGN_SPECIFICATION | 採択済みの責務・不変条件を統合。未採択の詳細は本文で明示 |
| ADOPTED_DESIGN_NOT_ACTIVATED | 設計採択。公開ネットワークでのactivationを意味しない |
| PROPOSED_DESIGN_NOT_ACTIVATED | 検討対象の設計案。既定で有効としない |
| DECISION_RATIONALE | ADRの背景・理由・代替案・帰結。細部の採否は対応RFCへ結合 |
| PROPOSED_DECISION_NOT_ACTIVATED | 判断案。対応仕様を含め未採択 |
| 証跡 | 記載されたcommit・設定・入力の結果。その範囲を超えた完成保証にしない |

[RFC-0013](rfc/0013-model-work-accounting.md)のMWA1、[RFC-0018](rfc/0018-storage-pruning-and-ibd.md)の新認証state mapは提案。画像・動画採掘とモデル改善はmainnet後へ延期・当面無効。[適用範囲](governance/post-mainnet-scope.md)。

モデル追加は[RFC-0007](rfc/0007-bft-model-admission.md)のBFTのみ。G-COLL/G-COST/G-USEがUNKNOWNのまま政策判断で採用されることと、数学的安全性が証明されることを分ける。採用票だけで個別claimの成功を認めない。

旧71RFC・235ADRの本文は保持しない。RFCは設計、ADRは判断理由とする。残る補助spec・実測記録を、現行採用や完成の根拠へ自動昇格させない。
