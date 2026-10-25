# ADR-0126 — The validator carve drops to a fifth, and the stake reorg gate stays

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

validator に報酬の 20% を残し、stake reorg gate を維持する方針は今後採用しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で validator 割当と veto を廃止し、PALW に consensus authority を一本化するため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
