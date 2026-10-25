# ADR-0128 — DNS validators vote BFT by bonded stake, and that vote decides the stake reorg gate

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

bond-weighted BFT・precommit・inactivity leak による DNS reorg veto は今後採用しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で validator 投票が PALW の chain selection・settlement を止める依存を除くため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
