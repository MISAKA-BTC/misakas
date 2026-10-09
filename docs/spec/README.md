# Specification chapters

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。

[新方針・廃止範囲](../adr/0177-model-bond-allocation-without-availability-consensus.md) / [文書チェックと未完了事項](../adr/evidence/0177-model-distribution-policy-alignment-2026-10-10.md)。


Normative PALW chapters live in [`palw/`](palw/):

| file | what it specifies |
|---|---|
| [`04b-tensor-ir.md`](palw/04b-tensor-ir.md) | PALW-TIR (RFC-0002): the IR, class registration, class seating |
| [`17-model-improvement.md`](palw/17-model-improvement.md) | RFC-0003/0004 (generative classes, model improvement), and §17.0 — the object-tag, delta and tail tables |
| [`18-inference-surface.md`](palw/18-inference-surface.md) | RFC-0001: the inference-surface rules |
| [`18-layer-sharded-panels.md`](palw/18-layer-sharded-panels.md) | RFC-0006: layer-sharded panels |
| [`18-verification-certificates.md`](palw/18-verification-certificates.md) | RFC-0007: verification vertices, the audit mesh, capped onboarding |

**Numbering note (2026-10-03).** The three files that begin `18-` are three distinct chapters that were written in parallel and
share the number by accident. They will be renumbered **18 (inference surface), 19 (layer-sharded panels) and 20 (verification
certificates)** — with every "spec 18" reference — after the DAA-5,300 flag day is deployed; until then "spec 18" in a comment or
a document means the chapter of the RFC it sits beside (RFC-0001, RFC-0006 or RFC-0007 respectively).
