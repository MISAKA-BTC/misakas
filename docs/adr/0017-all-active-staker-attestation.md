# ADR-0017: All-Active-Staker Attestation (Remove Sortition Committee)

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

全 active staker による DNS attestation は将来設計に採用しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で attestation quorum を廃止し、PALW を唯一の live consensus authority にするため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
