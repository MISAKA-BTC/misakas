# ADR-0015: Remote-Signer / HSM Protocol for Validator Signing

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 向け HSM・PKCS#11・HA 拡張は今後実装しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で対象の validator 役を廃止するため。

既存 software signer（`kaspa-pq-signer`）と共有鍵処理の削除を意味しない。DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
