# 文書番号の台帳

既存主文書は再採番していません。このセットの次番号は **ADR-0183 / RFC-0023**。新規0181/0021は設計凍結、0182/0022は画像・動画・改善の延期です。他ブランチの予約は未確認です。[機械可読版](number-registry.json)。

## 既存の衝突

### ADR-0026

- [0026-current-palw-testnet11-architecture.md](../adr/0026-current-palw-testnet11-architecture.md)
- [0026-palw-v2-runtime-separated-verification.md](../adr/0026-palw-v2-runtime-separated-verification.md)

### ADR-0173

- [0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md)
- [0173-public-verifier-dispute-completeness-is-misaka-purpose.md](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)

## 欠番

**ADR:** 0048, 0104, 0113, 0156, 0157, 0158, 0159, 0164。内容を捏造して補いません。

**RFC:** なし。内容を捏造して補いません。

## 補助文書・仕様章

RFC-0010/0012のdormant implementationは主RFCではなくdesignへ移しました。旧新パスは [対応表](path-map.md)。Specの18番は以下の別章が共有するため、完全パスで区別します。

- [18-inference-surface.md](../spec/palw/18-inference-surface.md)
- [18-layer-sharded-panels.md](../spec/palw/18-layer-sharded-panels.md)
- [18-verification-certificates.md](../spec/palw/18-verification-certificates.md)

## 2026-10-20 の別 branch 予約

ADR-0192 は本 branch の Proposed。ADR-0193 (`docs/adr/0193-sequential-witness-filing-clock.md`) は `codex/rfc14-x3-reorg-clock` の Proposed で、本 branch へ本文を統合していない。0183–0191 の別 branch 割当と併せて再使用せず、ADR の次ローカル番号は 0194 とする。冒頭の ZIP 時点の次番号は履歴であり、現在の全 branch の未使用を示さない。

## Account Bond package addition (2026-10-20)

[RFC-0027](../rfc/0027-miner-account-bond-and-panel-incentives.md) and [Account Bond ADR-0193](../adr/0193-miner-account-bond-for-panel-incentives.md) are imported as Proposed. Separate X3 branch also has a filing-clock ADR-0193 under a different filename; histories and paths are preserved and must be distinguished. Next local numbers: ADR-0194 / RFC-0028. Package shared indexes/registry were not blindly applied.

## ADR-0194 reservation (2026-10-20)

[ADR-0194](../adr/0194-dormant-g14-integration-and-service-schema.md) reserves the dormant integration/service namespace proposal. No integration is approved by this allocation. Next local ADR is 0195; RFC remains 0028.
