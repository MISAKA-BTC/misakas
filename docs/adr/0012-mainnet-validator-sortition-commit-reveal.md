# ADR-0012: Mainnet Validator Sortition via On-Chain Commit-Reveal

**Status:** Superseded — commit-reveal 案は 2026-05-30 に ADR-0017 で撤回。

commit-reveal による validator 委員会選出は実装しない。委員会をなくして全 active bond が参加する [ADR-0017](0017-all-active-staker-attestation.md) に置き換えられ、その DNS validator 自体も [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で廃止するため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
