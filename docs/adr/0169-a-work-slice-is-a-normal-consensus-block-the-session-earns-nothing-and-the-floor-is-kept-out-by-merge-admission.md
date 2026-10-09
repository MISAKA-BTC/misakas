# ADR-0169 — RFC8 uses the existing EXEC lane; the former algo-11 design is retired

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: SUPERSEDED DIRECTION, 2026-10-08. Stable filename retained for existing links.

The former algo-11 design body has been deleted. Its previously reported test results, suite totals and
unrun cases remain in the [v0 test record](../design/palw/rfc-0008-v0-test-record.md). Those results describe the
`rfc8/claim-backed-blocks` branch of 2026-10-03/04; they do not establish current main's implementation or
validate the replacement EXEC design. No old fence is activated by this change.

[Revised RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) and
[EXEC integration spec v1](../design/palw/rfc-0008-implementation-spec.md) govern new RFC8 work:

- REAL anchors an ordinary or long useful-work claim under the current REAL qualification rules.
- EXEC_TX and EXEC_SLICE share the existing chain-independent EXEC class; neither becomes a selected parent,
  contributes raw/PALW fork-choice weight or advances the DAA/clock.
- TxPermit and WorkSlice authorization/accounting are separate. Slice work settles once at root claim Final;
  root-prefix and slice ranges cannot duplicate credit, rewards or execution-schedule credit.
- Current main's heartbeat, BASE-0, 120-second cadence and liveness structure remain the baseline.
- ADR-0173/RFC14 public prosecution, evidence retention and liability apply to every slice/boundary.
  Panel=0 still requires RFC14 and RFC15's separate completion/activation gates.
- New root/slice statements bind [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
  common post-commit policy. Each slice's evidence is fixed before its corresponding future source window;
  a seed exposed at root-open cannot test later freely chosen statements. Carriers never multiply beacon sources.

This is a design/documentation update. The replacement runtime has not been implemented or activated by it.
