# 文書番号の台帳

既存主文書は再採番していません。このセットの次番号は **ADR-0185 / RFC-0023**。新規0181/0021は設計凍結、0182/0022は画像・動画・改善の延期です。他ブランチの予約は未確認です。[機械可読版](number-registry.json)。

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

## 2026-10-20 X3 worktree の追加予約

[ADR-0193](../adr/0193-sequential-witness-filing-clock.md) を Proposed として予約。ADR の次ローカル番号は 0194。0189–0192 は別 branch の割当で、この branch に主文書を捏造・複製しない。上の ZIP 時点の次番号は履歴であり、全 branch の未使用番号を保証しない。RFC の別 branch 予約も引き続き区別する。


## 2026-10-21 終了効果候補の追加予約

[ADR-0197](../adr/0197-court-end-effects-and-pending-liability-archive.md) / [RFC-0030](../rfc/0030-court-end-effects-and-pending-liability-archive.md)をProposed/Draftとして予約した。利用者はAの詳細仕様化・隔離実装を指示し、本番採択・有効化とは区別する。ローカルの822 unique ref headとactive worktreeを調査し、同番号の別文書は見つからなかった。[調査範囲](../evidence/palw/g14-integration-candidate-2026-10-20/court-end-effects-number-audit.json)。未fetchのremoteや将来branchの未使用保証ではない。次のローカル番号はADR-0198 / RFC-0031。上の過去時点の次番号は履歴である。

## 2026-10-22 認証付き state map の追加予約

[ADR-0203](../adr/0203-authenticated-palw-state-map-and-row-access.md) / [RFC-0036](../rfc/0036-authenticated-palw-state-map-and-row-access.md)をProposed / Review Requiredとして予約した（30 GB pruned node 方針の state lane）。基準 b1eb33960 の tree には ADR-0202 / RFC-0035 までの主文書があり、0198–0202 / 0031–0035 は他 lane の割当で本台帳には未記録。storage 作業の他 lane（IBD）が consensus 可視の形式を起案する場合は ADR-0204 / RFC-0037 以降を使う。次のローカル番号は ADR-0204 / RFC-0037。未 fetch の remote や将来 branch の未使用保証ではない。
