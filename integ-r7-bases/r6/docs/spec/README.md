# 仕様 — 規則と適用条件から読む

採用済み部分は [原則凍結](../governance/design-freeze.md)。この整理は仕様の実装・検証・本番有効化を宣言しません。

## PALW の読解用統合版

| 論点 | 文書 |
| --- | --- |
| 固定 identity・登録・二階層 | [model-tiers.md](palw/model-tiers.md) |
| `α`・30日平均・係数・総枠 | [model-allocation.md](palw/model-allocation.md) |
| Q/B/R/F・拘束・mode・責任 | [bond-budgets.md](palw/bond-budgets.md) |
| V+ の物理 block 機会 | [block-opportunity.md](palw/block-opportunity.md) |
| Final と SecurityCertified | [claim-certification.md](palw/claim-certification.md) |

これらは既存の採択済み条項の案内で、独自の新規規則ではありません。詳細の整数算術・境界条件・適用対象は明示した出典に従います。未決事項を推測で補いません。

## 既存の仕様章

[Tensor IR](palw/04b-tensor-ir.md)、[モデル改善を含む章](palw/17-model-improvement.md)、[推論サーフェス](palw/18-inference-surface.md)、[layer-sharded panels](palw/18-layer-sharded-panels.md)、[verification certificates](palw/18-verification-certificates.md)、[PQ](pq/kaspa-pq-spec.md)。

RFC-0003 の画像・動画と RFC-0004 のモデル改善は **mainnet 後へ延期・当面無効**です。これらの仕様や過去の試験が存在しても有効化しません。共有のTIR・テキスト・不変登録・claim認証は別です。[正式方針](../governance/post-mainnet-scope.md)

「18」で始まる三章は別文書です。[番号台帳](../maintenance/numbering.md)を参照し、完全パスで識別します。番号が大きい文書が無条件に優先するのではなく、PRINCIPLESと明示的な採択・置換関係に従います。
