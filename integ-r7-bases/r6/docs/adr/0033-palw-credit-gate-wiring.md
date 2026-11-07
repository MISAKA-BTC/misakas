# ADR-0033: PALW V1 credit overlay（不採用）

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Withdrawn — [ADR-0037](0037-palw-async-job-state-machine.md) / [ADR-0038](0038-palw-is-the-consensus-work.md) により旧 credit overlay 方針を撤回。

DNS validator の投票・bond を使って hash PoW に credit を追加する方式は実装しない。有用計算そのものを consensus work にする現行 PALW V2 と役割が異なり、専用 DNS validator も [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の廃止対象であるため。

旧判定・processor の接続コードは削除した。Params の互換用 encoding だけを維持し、`palw_credit = Some` は設定検証で拒否する。
