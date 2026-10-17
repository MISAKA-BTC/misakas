# ADR — 決定と採用理由

[トピック別](TOPICS.md) / [番号順の完全一覧](INDEX.md) / [番号台帳](../maintenance/numbering.md) / [状態](../STATUS.md)。

採用済み設計は [原則凍結](../governance/design-freeze.md)。重大問題時は新規RFCと新規ADRの両方を明示的に採択します。Draftの存在やmergeは採用・実装完了・有効化ではありません。

**RFC-0003の画像・動画経路とRFC-0004のモデル改善は、mainnet後へ延期・当面無効。** 初期mainnetに含めず、自動有効化しません。[延期方針](../governance/post-mainnet-scope.md)。

## まず読む文書

| 番号 | 内容 |
| --- | --- |
| 0182 | [画像・動画経路とモデル改善を mainnet 後へ延期し、当面無効にする](0182-defer-media-and-model-improvement-until-post-mainnet.md) |
| 0181 | [採用済み設計を原則凍結し、重大問題時のみ新規 RFC・ADR で変更する](0181-design-freeze-and-controlled-change.md) |
| 0180 | [PALW のみの合意:BFT の validator を使わない](0180-palw-only-consensus-no-bft-validators.md) |
| 0179 | [モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3](0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| 0178 | [The fork choice heals a partition on bonded participation: one search over every tip, economic keys counted over each branch's exclusive past, and participation first past `W_p`](0178-fork-choice-heals-a-partition-on-bonded-participation.md) |
| 0177 | [Model-bond coinbase allocation without model-availability consensus](0177-model-bond-allocation-without-availability-consensus.md) |
| 0176 | [Bond bounds PALW claim capacity, blocks, rewards and Final weight](0176-bond-bounds-palw-production-rewards-and-final-weight.md) |
| 0175 | [新規モデル登録は自由、登録モデルは永久不変](0175-registered-models-are-permanently-immutable.md) |

この整理版の次番号は **ADR-0185**。添付ZIPの台帳に基づき追加した番号で、他ブランチの予約は統合前に照合します。既存番号は再利用・再採番していません。

詳細な旧索引・過去の置換/適用表は [整理前の索引](../history/documentation/2026-10-17/adr/README.md) に保持しています。旧表のcurrent / liveは現在の保証ではありません。
