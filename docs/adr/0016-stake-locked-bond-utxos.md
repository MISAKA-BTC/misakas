# ADR-0016: Stake-locked bond UTXOs

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS stake bond の参加・投票用設計は拡張しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で stake 投票を廃止するため。PALW の共有 bond・担保規則は [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) に従う。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
