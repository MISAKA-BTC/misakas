# ADR-0014: Coordinated-Failover Protocol for Validator Hosts

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator の takeover token・HA failover は実装しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で専用 validator 役を廃止するため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
