# ADR-0011: Validator Single-Host Deployment + Equivocation-Safety Operating Model

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 専用の配置・二重署名防止設計は拡張しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で validator 投票を consensus authority から外すため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
