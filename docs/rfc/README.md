# RFC — 提案・技術設計・未決事項

[トピック別](TOPICS.md) / [番号順の完全一覧](INDEX.md) / [番号台帳](../maintenance/numbering.md) / [状態](../STATUS.md)。

採用済み設計は [原則凍結](../governance/design-freeze.md)。重大問題時は新規RFCと新規ADRの両方を明示的に採択します。Draftの存在やmergeは採用・実装完了・有効化ではありません。

**RFC-0003の画像・動画経路とRFC-0004のモデル改善は、mainnet後へ延期・当面無効。** 初期mainnetに含めず、自動有効化しません。[延期方針](../governance/post-mainnet-scope.md)。

## まず読む文書

| 番号 | 内容 |
| --- | --- |
| 0025 | [モデルが増え court が大量発生しても処理を継続できる客観的 court(Draft)](0025-scalable-objective-court-many-models-mass-disputes.md) |
| 0024 | [ctx 2M・大きいモデルでも door ≤ 3,200 DAA に収める court 設計(Draft)](0024-court-door-3200-to-ctx-2m.md) |
| 0023 | [未解決 claim の責任を保持し、依存しない claim を進める](0023-unresolved-claim-liability-and-independent-progress.md) |
| 0022 | [初期 mainnet から画像・動画・モデル改善を外す適用範囲](0022-post-mainnet-media-and-model-improvement-scope.md) |
| 0021 | [設計凍結・例外変更手続きと文書の役割分離](0021-design-freeze-and-documentation-governance.md) |
| 0020 | [長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない](0020-long-context-calibration-memory.md) |
| 0019 | [年次モデル追加フォーク: 段階的ソフトウェア展開・年次モデルカタログ・合意は不変](0019-annual-model-upgrade.md) |
| 0018 | [DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束](0018-dag-parallel-multi-model-consensus.md) |
| 0017 | [モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)](0017-model-eligibility-examination-and-renewal.md) |
| 0016 | [bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| 0004 | [PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR](0004-palw-model-improvement.md) |
| 0003 | [PALW Generative Model Classes — one job, determinism and output layer, and the class profiles on top of it](0003-palw-generative-model-classes.md) |

この整理版の次番号は **RFC-0026**。添付ZIPの台帳に基づき追加した番号で、他ブランチの予約は統合前に照合します。既存番号は再利用・再採番していません。

詳細な旧索引・過去の置換/適用表は [整理前の索引](../history/documentation/2026-10-17/rfc/README.md) に保持しています。旧表のcurrent / liveは現在の保証ではありません。
