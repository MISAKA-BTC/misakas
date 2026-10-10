# ADR-0009: DNS Probabilistic Finality Overlay

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator の署名・stake による finality は今後採用しない。PALW の chain selection と settlement に別の finality 権限を重ねる依存を除くため。後継は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md)。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
