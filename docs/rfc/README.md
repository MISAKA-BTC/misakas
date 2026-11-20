# RFC — 旧番号の互換入口・履歴

<!-- MOR1-DRAFT-20261120-BEGIN -->
## 2026-11-20 PALW-MOR1 — モデル登録時の試行別認証出力

**DRAFT_NOT_ACTIVATED**。対象profileの登録時に、別の採掘試行の認証出力が天文学的に小さい確率でしか重複しない構成を要求する。正式出力は回答y・試行ID・256bit実行識別値。完全context・実beacon・三Rootへ結合し、同一試行の再現性と別試行の非重複性を定義する。AN1の実入力・Ticket・Panel＋Court・一回会計を維持し、通常LLM費用の下界を成立済みとしない。

[RFC-0060](../rfc/0060-model-registration-and-attempt-output-binding.md)／[ADR-0228](../adr/0228-model-registration-and-attempt-output-binding.md)／[変更・検証](../maintenance/revisions/2026-11-20-model-output-binding/README.md)。新しい不変profileへの適用草案であり、旧登録・過去claim・稼働ルールへ遡及適用しない。現行20 RFC／50 ADRの統合構造を維持する。
<!-- MOR1-DRAFT-20261120-END -->

<!-- AN1-DRAFT-20261118-BEGIN -->
## 2026-11-18 PALW-AN1 — Panel＋Courtを維持する草案

**DRAFT_NOT_ACTIVATED**。アドレス・nonceを実入力tokenへ結合し、登録と元claimの二段階固定、完全再実行、局所反証、一回性を具体化した。計算validity proofは導入せず、通常LLM実費の下界を成立済みとしない。[RFC-0059](../rfc/0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md) / [変更・検証](../maintenance/revisions/2026-11-18-an1/README.md)。0056〜0058の追補と現行20冊の担当案内を追加し、旧採択・過去証拠・50 ADRの統合構成を保持する。
<!-- AN1-DRAFT-20261118-END -->


> **2026-11-08追加採択:** EXEC creditは[RFC07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](../rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](../maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

現行の閲覧・更新先は **[RFC 20本（RFC-v2-0001〜0020）](../rfc-v2/README.md)** です。旧文書との[完全対応表](../rfc-v2/MAPPING.md)から統合先を探せます。

旧番号の本文・既存パスは、コード・仕様・証拠からの参照と判断履歴のために保持します。新しい参照には `RFC-v2-xxxx` を使い、旧番号の意味を変更しません。

2026-11-08 採択の新しい claim 版は [RFC-v2-0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) から0016〜0019へ進んでください。[ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) に採択を記録し、状態は `ADOPTED_DESIGN_NOT_ACTIVATED` とします。旧15本への統合記録と、ここにある旧番号の本文を今回の稼働仕様へ読み替えません。

[旧番号順索引](INDEX.md) / [旧トピック別索引](TOPICS.md) / [文書の状態](../STATUS.md) / [原則凍結](../governance/design-freeze.md)。未採択案の採用・有効化を意味しません。
