# ADR-0013: Validator Reward Distribution

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 報酬配分の新規実装・拡張は行わない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で将来の validator 割当を廃止するため。既存の報酬債務・bond は同 RFC の移行で清算する。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
