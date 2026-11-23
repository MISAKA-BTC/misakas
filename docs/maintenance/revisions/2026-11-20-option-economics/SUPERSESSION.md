# MOE1の改訂対象と旧記載の扱い

経済規範は[RFC0063](../../../rfc/0063-additional-model-option-economics.md)と[経済移行契約](../../../governance/model-option-economics-transition.md)に従う。以下は旧テキストをすべて削除した記録ではなく、現行規範を置き換えつつ、過去原本・証拠を保全した範囲である。

## 主要条項

| 文書・箇所 | 現行profileでの処理 |
| --- | --- |
| RFC-v2-0011 §§3、7–9 | モデル分類別のR配分、原子登録、5%/3%/残額、別役務原資、直接移転、activationを本文へ反映 |
| ADR0087、0089、0095、0101 / ADR-v2-0021 | 追加モデルだけの送受信・allowance・同期native/EVM・membership。旧非譲渡を置換 |
| ADR0091、0114 / ADR-v2-0023 | 50%買上げ、通常5%/3%/残額、内部buyback追加fee0、Position retire |
| ADR0124、0177、0179 | coreはMiner=R、additionalは買上げ後の版別plan、階層と資本会計を維持 |
| ADR0162 / ADR-v2-0022 | 追加登録時に市場同時生成。外部seedは任意。旧core sell-onlyは新profileへ継承しない |
| モデル登録・registry・運用手順 | ADDITIONALの登録済み市場欠落を正常状態にしない。GENESIS/FORK指定から市場を作らない |
| Panel・Court・予算・Final・EVMの入口 | 検証、原期限、成熟・hold・export上限を維持し、core役務費は別原資へ接続 |
| 旧署名claim・過去測定・統合原条文 | 元版・元snapshotを保全。新率を遡及適用しない |

旧Decisionの全面改訂前原本は `before/` の同じ相対パスに `.txt` を付けたファイルへ保存した。元ファイルのSHA-256と改訂後のSHA-256は[変更一覧](changes.json)で確認できる。

## 旧記載インベントリ

直前TB1/FOW1版のMarkdownを経済・移転関連のパターンで検索し、**79ファイル・333箇所**を列挙した。キーワード一致には履歴引用、Ticket/ExecCreditの非譲渡、過去の実装記録も含む。すべてをModelPositionの現行規範と見なして置換していない。[元位置・文脈](economic-legacy-inventory.json)と[追加入口の範囲監査](economic-scope-audit.json)を併用する。

| ファイル | 該当数 | 今回の扱い |
| --- | ---: | --- |
| [PRINCIPLES.md](../../../PRINCIPLES.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0038-palw-is-the-consensus-work.md](../../../adr/0038-palw-is-the-consensus-work.md) | 2 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md](../../../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) | 9 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md](../../../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) | 3 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md](../../../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) | 15 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md](../../../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) | 2 | 旧seed/fence原決定を保存。新規市場は登録と同時生成、外部seedは任意 |
| [adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md](../../../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) | 21 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0095-a-position-is-a-membership-not-an-income.md](../../../adr/0095-a-position-is-a-membership-not-an-income.md) | 3 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md](../../../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) | 1 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md](../../../adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) | 4 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md](../../../adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) | 8 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md](../../../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) | 1 | 旧seed/fence原決定を保存。新規市場は登録と同時生成、外部seedは任意 |
| [adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md](../../../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) | 5 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md](../../../adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) | 1 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [adr/0143-an-artifact-root-has-one-owner-on-the-chain.md](../../../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md) | 4 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [adr/0162-the-pair-opens-on-a-virtual-reserve.md](../../../adr/0162-the-pair-opens-on-a-virtual-reserve.md) | 8 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/0177-model-bond-allocation-without-availability-consensus.md](../../../adr/0177-model-bond-allocation-without-availability-consensus.md) | 2 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/INDEX.md](../../../adr/INDEX.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr/TOPICS.md](../../../adr/TOPICS.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0005-palw-consensus-architecture.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0005-palw-consensus-architecture.md) | 2 | 統合された旧原条文をbyte保全。適用は新しい経済正本で制限 |
| [adr-v2/0007-evm-exec-lane-and-anchors.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0007-evm-exec-lane-and-anchors.md) | 15 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0015-permissionless-model-registration.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0015-permissionless-model-registration.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0016-artifact-ownership-and-model-lineage.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0016-artifact-ownership-and-model-lineage.md) | 7 | 統合された旧原条文をbyte保全。適用は新しい経済正本で制限 |
| [adr-v2/0019-model-tiers-and-annual-catalog.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0019-model-tiers-and-annual-catalog.md) | 2 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0021-positions-and-membership.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0021-positions-and-membership.md) | 19 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0022-liquidity-seeding-and-virtual-reserves.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0022-liquidity-seeding-and-virtual-reserves.md) | 12 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0023-model-reward-routing-and-owner-leg.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0023-model-reward-routing-and-owner-leg.md) | 40 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0026-canonical-work-and-answer-accounting.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0026-canonical-work-and-answer-accounting.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0037-fee-escrow-and-role-consent.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0037-fee-escrow-and-role-consent.md) | 5 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/0050-freeze-deferred-rejected-and-governance.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0050-freeze-deferred-rejected-and-governance.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/MAPPING.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/MAPPING.md) | 3 | 統合時の対応・Status履歴を保全。新規範は経済移行契約で指定 |
| [adr-v2/PRECEDENCE.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/PRECEDENCE.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/README.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/README.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [adr-v2/STATUS-LEDGER.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/STATUS-LEDGER.md) | 8 | 統合時の対応・Status履歴を保全。新規範は経済移行契約で指定 |
| [architecture/palw-specification.md](../../../architecture/palw-specification.md) | 14 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [audit/adr/STATUS-AUDIT-2026-09-18.md](../../../audit/adr/STATUS-AUDIT-2026-09-18.md) | 3 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [audit/palw/design-reviews/emission-audit-2026-10-04.md](../../../audit/palw/design-reviews/emission-audit-2026-10-04.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [audit/palw/design-reviews/rfc-completion-audit-2026-10-17-status.md](../../../audit/palw/design-reviews/rfc-completion-audit-2026-10-17-status.md) | 2 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [audit/palw/rfc18-dag-audit-2026-10-17/source_evidence.md](../../../audit/palw/rfc18-dag-audit-2026-10-17/source_evidence.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [design/palw/economics/bond-budget-and-model-allocation.md](../../../design/palw/economics/bond-budget-and-model-allocation.md) | 7 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [design/palw/economics/econ-verifier-incentive-and-allocation.md](../../../design/palw/economics/econ-verifier-incentive-and-allocation.md) | 2 | 対象外の制約又は既存資料を保全。ModelPositionの現行条件はMOE1へ接続 |
| [design/palw/economics/palw-position-benefits-consumer-p-b2-2026-09-24-ja.md](../../../design/palw/economics/palw-position-benefits-consumer-p-b2-2026-09-24-ja.md) | 13 | 日付付き設計・実装調査を保全。MOE1有効化の実績にはしない |
| [design/palw/execution/rfc-0008-implementation-spec.md](../../../design/palw/execution/rfc-0008-implementation-spec.md) | 1 | 対象外の制約又は既存資料を保全。ModelPositionの現行条件はMOE1へ接続 |
| [design/palw/integration/activation-readiness-matrix.md](../../../design/palw/integration/activation-readiness-matrix.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [design/palw/integration/adr-0162-0178-implementation-status-2026-10-15.md](../../../design/palw/integration/adr-0162-0178-implementation-status-2026-10-15.md) | 2 | 日付付き設計・実装調査を保全。MOE1有効化の実績にはしない |
| [design/palw/integration/rfc-0018-implementation-status-2026-10-15.md](../../../design/palw/integration/rfc-0018-implementation-status-2026-10-15.md) | 1 | 日付付き設計・実装調査を保全。MOE1有効化の実績にはしない |
| [design/palw/integration/unmerged-inventory-2026-10-15.md](../../../design/palw/integration/unmerged-inventory-2026-10-15.md) | 3 | 日付付き設計・実装調査を保全。MOE1有効化の実績にはしない |
| [design/palw/positive-court/SOURCE-AUDIT.md](../../../design/palw/positive-court/SOURCE-AUDIT.md) | 1 | 日付付き設計・実装調査を保全。MOE1有効化の実績にはしない |
| [design/palw/rfc-completion-audit-2026-10-17/report_ja.md](../../../design/palw/rfc-completion-audit-2026-10-17/report_ja.md) | 1 | 日付付き設計・実装調査を保全。MOE1有効化の実績にはしない |
| [design/palw/rfc-completion-audit-2026-10-17/source_evidence.md](../../../design/palw/rfc-completion-audit-2026-10-17/source_evidence.md) | 1 | 日付付き設計・実装調査を保全。MOE1有効化の実績にはしない |
| [history/README-2026-10-12.md](../../../history/README-2026-10-12.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/decisions/2026-10/adr-0162-0178-user-decisions-2026-10-15.md](../../../history/decisions/2026-10/adr-0162-0178-user-decisions-2026-10-15.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/decisions/2026-10/rfc-0006-0012-user-decisions-2026-10-15.md](../../../history/decisions/2026-10/rfc-0006-0012-user-decisions-2026-10-15.md) | 2 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/00-header-and-changelogs.md](../../../history/design/palw/0152/00-header-and-changelogs.md) | 5 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/01-operator-summary-ja.md](../../../history/design/palw/0152/01-operator-summary-ja.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/02-motivation-and-overview.md](../../../history/design/palw/0152/02-motivation-and-overview.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/03-rules-b-sr-v-l-a.md](../../../history/design/palw/0152/03-rules-b-sr-v-l-a.md) | 9 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/04-rules-s-t-w-p0-10.md](../../../history/design/palw/0152/04-rules-s-t-w-p0-10.md) | 3 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/07-invariant-and-comparison.md](../../../history/design/palw/0152/07-invariant-and-comparison.md) | 2 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/08-fences-schema-phasing.md](../../../history/design/palw/0152/08-fences-schema-phasing.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/09-tests-and-launch-gate.md](../../../history/design/palw/0152/09-tests-and-launch-gate.md) | 2 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/design/palw/0152/10-questions-traceability-reviews.md](../../../history/design/palw/0152/10-questions-traceability-reviews.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/documentation/2026-10-17/adr/README.md](../../../history/documentation/2026-10-17/adr/README.md) | 9 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/documentation/2026-10-17/architecture/overview.md](../../../history/documentation/2026-10-17/architecture/overview.md) | 6 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/networks/testnet-11/testnet-11.md](../../../history/networks/testnet-11/testnet-11.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/networks/testnet-11/testnet11-relaunch-5f-genesis-card.md](../../../history/networks/testnet-11/testnet11-relaunch-5f-genesis-card.md) | 2 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [history/networks/testnet-12/t12-rcore-launch-checklist.md](../../../history/networks/testnet-12/t12-rcore-launch-checklist.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [maintenance/catalog.md](../../../maintenance/catalog.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md](../../../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md) | 1 | 過去の監査・証拠・履歴をbyte保全。新規範の実装実績にはしない |
| [operations/releases/mainnet-readiness.md](../../../operations/releases/mainnet-readiness.md) | 1 | 対象外の制約又は既存資料を保全。ModelPositionの現行条件はMOE1へ接続 |
| [reference/explorer/user-dictionary.md](../../../reference/explorer/user-dictionary.md) | 2 | 対象外の制約又は既存資料を保全。ModelPositionの現行条件はMOE1へ接続 |
| [rfc/0004-palw-model-improvement.md](../../../rfc/0004-palw-model-improvement.md) | 1 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [rfc/0007-palw-verification-certificates-and-algebraic-checks.md](../../../rfc/0007-palw-verification-certificates-and-algebraic-checks.md) | 2 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [rfc/0015-panel-free-permissionless-verification.md](../../../rfc/0015-panel-free-permissionless-verification.md) | 4 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [rfc/0018-dag-parallel-multi-model-consensus.md](../../../rfc/0018-dag-parallel-multi-model-consensus.md) | 1 | 旧採択を保全。経済条項はMOE1の追加モデル限定・比率・移転規則が優先 |
| [rfc-v2/0006-panels-and-probabilistic-verification.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0006-panels-and-probabilistic-verification.md) | 2 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [rfc-v2/0010-dag-consensus-and-native-settlement.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0010-dag-consensus-and-native-settlement.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [rfc-v2/0011-bond-budgets-fees-and-payments.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md) | 4 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |
| [rfc-v2/0015-freeze-deferred-rejected-and-governance.md](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0015-freeze-deferred-rejected-and-governance.md) | 1 | 現行本文、適用節又は索引を改訂。旧原本を別保存 |

## 取り違えない境界

- ModelPositionを移転可能にしても、ExecCredit、claim帰属、bond、署名責任、登録owner権を移転可能にしない。
- 追加市場の存在を、採掘適格性、資料取得、成功Final、任意額の売却支払能力と同一視しない。
- GENESIS/FORKの100%は価格付け後のRの帰属率であり、検証免除、即時払、総発行枠拡大ではない。
- owner3%はcontributor内部分配を含む総leg。通常売買、内部buyback、直接移転を別の資金経路として定義する。
- 旧core市場がある場合、旧権利・既受理義務のwinddownが完了する前に新profileを有効化しない。
- 保存された旧率や非譲渡の語句が検索に残ることと、現行profileへその規則を適用することは区別する。
