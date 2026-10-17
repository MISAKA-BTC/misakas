# Consensus

> **整理前の保存版。** 当時のcurrent / live / PASS・数値・手順を現在の保証にしません。新しい入口は [README](../../../../README.md)。元バイトはoriginal-markdown.zipに保持しています。

MISAKA keeps rusty-kaspa's blockDAG (GHOSTDAG ordering, DAA difficulty) and adds PALW production, PQ-only authorization and
PALW-based settlement. Start with the [architecture overview](../../../../architecture/overview.md) and [what is different from Kaspa](../../../../architecture/differences-from-kaspa.md).

- **PALW consensus and Final:** [RFC-0008](../../../../rfc/0008-palw-claim-backed-consensus-blocks.md) (claim-backed blocks, EXEC v2), [RFC-0010](../../../../rfc/0010-permissionless-palw-panel-and-claim-completion.md) (permissionless Panel), [RFC-0014](../../../../rfc/0014-panel-independent-fraud-prosecution.md) (public prosecution), [RFC-0015](../../../../rfc/0015-panel-free-permissionless-verification.md) (Panel-free, not activated).
- **Finality and DNS retirement:** [RFC-0012](../../../../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) and its [dormant implementation note](../../../../design/palw/integration/rfc-0012-dormant-implementation.md); fork choice rule E in ADR-0178 (see the [ADR index](../../../../adr/README.md)).
- **Bond/time budgets:** [ADR-0176](../../../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md).
- **Activation status:** [activation-readiness-matrix.md](../../../../design/palw/integration/activation-readiness-matrix.md) — nothing new is armed until the user's single full-activation release.
