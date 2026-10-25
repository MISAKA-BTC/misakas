# ADR-0010: Validator Node Architecture (operational supplement to ADR-0009)

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 専用ノードの新規実装・再構築は行わない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) でその役割を廃止するため。PALW の共有鍵処理・bond 処理は廃止対象に含めない。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
