# 仕様 — 規則と適用条件から読む

> **2026-11-08追加採択:** EXEC creditは[RFC07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](../rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](../maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

採用済み部分は [原則凍結](../governance/design-freeze.md)。この整理は仕様の実装・検証・本番有効化を宣言しません。

## 2026-11-08 採択の新しい claim 版

現行の設計入口は [RFC 20本](../rfc-v2/README.md)。新規0016〜0020の採択は [ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) に記録し、**ADOPTED_DESIGN_NOT_ACTIVATED** とします。新しい claim / profile の規範は [RFC-v2-0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) と [公開裁定の方針 v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md) から参照してください。

新版では Node にモデル本体・全推論・GPU を要求せず、モデル未共有とモデル内 Miner／Panel の全結託を想定します。Miner の資料供給、global pool から独立選出した Panel の非対話 positive、不一致時の対話 negative を組み合わせます。結果固定後に検証を始め、必須局所 positive と独立肯定承認の成立で `CertificationReady`、公開異議・依存条件等も満たした claim だけを成功 Final に進めます。資料不足は成功不可とし、global な正直者の前提、局所証拠と全体承認の区別を維持します。

以下のトピック別文書・仕様章は各出典の版と共通制約の入口です。新版の変更対象は上記の採択文書に従い、従来の `Final` 後認証などを新版へ混在させません。旧署名済み claim、未採択・延期機能の状態、過去の実装記録は変更しません。新しい wire・fence・有効化高さは未割当です。[適用状態](../STATUS.md)。

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
