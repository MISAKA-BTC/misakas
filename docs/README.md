# Documentation map

> **設計前提(全文書の上位)— [MISAKAの不可侵原則](PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。すべての ADR・RFC・Spec・設計文書はこの前提の下にあり、衝突する場合は前提が優先する(2026-10-09)。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

The code, the current CLI `--help` and the ADR decisions are authoritative. This map separates the entry points for today's network from design references and retained evidence.

## Start here

| | |
| --- | --- |
| Build | [getting-started/build.md](getting-started/build.md) |
| Run a node | [getting-started/run-node.md](getting-started/run-node.md) |
| Mine (PALW producer) | [getting-started/mining.md](getting-started/mining.md) → [testnet12-join-mining.md](testnet12-join-mining.md) |
| Run a validator | [getting-started/validator.md](getting-started/validator.md) → [validator-runbook.md](validator-runbook.md) |
| Benchmarks and tests | [getting-started/testing.md](getting-started/testing.md) |

## Current network — testnet-12

- [testnet-12 launch note (2026-09-25)](t12-launch-2026-09-25.md) — the release, its known issues and when a payment is final
- [Join testnet-12 as a PALW producer](testnet12-join-mining.md)
- [testnet-12 regenesis record](testnet-12-regenesis-2026-09-23.md)
- [Mainnet readiness](mainnet-readiness.md), [Release process](release-process.md)

Invariants to check on testnet-12: consensus params fingerprint `ef379378f09bf33f29e97854bbee157a8f86fa1dd8248e1a847f08c5c6e7f15e`,
genesis `a27f8f44fe4d91a5…`, schedule id `9ef2b1a795c74d6f…` (`kaspad --testnet --netsuffix=12`, or `misaka --network testnet-12`).
The handshake identity is unchanged. The int-13 list (`palw_audit_1004_v1`, `palw_gen_range_twin_v1`, `palw_model_court_window`,
`palw_receipt_spend_v4`) arms at DAA 15,500.
Never count blocks, blue-score depth or DAA as finality on testnet-12; use finality depth or a `Final` PALW anchor.

## Architecture

- [Overview](architecture/overview.md) — the protocol as it is now, topic by topic, with the ADRs that govern each part
- [What is different from Kaspa](architecture/differences-from-kaspa.md)
- [Consensus](architecture/consensus.md) · [PALW](architecture/palw.md) · [Post-quantum](architecture/pq.md) · [EVM](architecture/evm.md)

## Reference and design

- [Reference index](reference/README.md) — ADRs, RFCs 0001–0015, specs, design records, registries
- [ADR index](adr/README.md) · [RFCs](rfc/) · [Specs](spec/) · [PALW design records](design/palw/)
- [Activation readiness matrix](design/palw/activation-readiness-matrix.md) — what is implemented, verified and armable (the int-13 list arms at DAA 15,500)
- [2026-10-12 RFC/ADR review](design/palw/rfc-adr-review-resolution-2026-10-12.md) and the [Panel=1 / Bond-time / model-capital proposal](design/palw/panel1-bond-budget-model-capital-amendments-2026-10-12.md)
- [Round / EXEC additional acceptance conditions (2026-10-10)](palw-round-exec-additional-acceptance-2026-10-10.md)

## Operations

- [Operations index](operations/README.md) — validator, model classes, model onboarding, liveness probe, EVM backend, PQ verification, release

## Audits and history

- [audit/](audit/) and the dated `palw-*-2026-*.md` files in this directory — engineering evidence, not current defaults
- [history/](history/README.md) — testnet-10 / testnet-11 runbooks and announcements, and the [pre-2026-10-12 root README](history/README-2026-10-12.md)

When a historical report conflicts with a current operator document, use the current operator document and verify against `--help` and chain RPC.
