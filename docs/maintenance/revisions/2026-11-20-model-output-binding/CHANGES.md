# 変更範囲

## 新規設計

| 文書 | 追加内容 |
| --- | --- |
| [RFC-0060](../../../rfc/0060-model-registration-and-attempt-output-binding.md) | 登録対象、descriptor、実入力、出力envelope、rho、非循環、登録適合、claim接続、非重複性定理、受入試験、移行条件 |
| [ADR-0228](../../../adr/0228-model-registration-and-attempt-output-binding.md) | この構成を提案する理由、AN1を維持する判断、構造適合と資格審査の境界、代案、未証明事項 |

## 既存20ファイルへの限定変更

| 対象 | 変更方法 |
| --- | --- |
| RFC-v2-0001／0003 | 現行著述部末尾、履歴節より前に、入出力・モデル登録からMOR1へ進む参照を追加。全履歴断片を保持 |
| ADR-v2-0015／0024 | 既存本文と履歴の後に、登録・入出力の担当範囲を追加。現行著述部分および原文ブロックのchecksum対象を保持 |
| RFC-0059 | 既存AN1本文の後にMOR1との依存関係を参照として追加。Ticket式や採択状態は変更しない |
| RFC／ADR／RFC-v2／ADR-v2のREADME | 冒頭に新草案への案内を追加 |
| docs/INDEX.md／docs/STATUS.md | 新草案と適用状態への案内を追加 |
| docs/maintenance/numbering.md | RFC0060／ADR0228のローカル番号割当を追記 |
| RFC-v2／ADR-v2のregistry.json | 統合後追加文書、latest_proposal、proposal_historyを更新。既存documentsとadoptionは維持 |
| RFC／ADRのINDEX.md・TOPICS.md、decision-catalog.json、maintenance/catalog.md | 新文書の番号・トピック・全文書一覧を追加。既存記録を保持 |

## 変更していないもの

原則、freeze方針、旧採択、既存20／50冊の履歴原文、source manifest、公式baseline、checker、過去の検証ログ・実測値、native/runtimeコードは変更していない。追加資料は今回のrevisionディレクトリに収録する。

新しいprofileの入出力規則は実装が必要だが、この文書改訂で本番wire tag、protocol version、activation epoch、採掘費用の下界を割り当てたり成立済みにしたりしていない。
