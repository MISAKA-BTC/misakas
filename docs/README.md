# Documentation map

> **最初に読む — PALW を成り立たせる前提([PRINCIPLES §10](PRINCIPLES.md)、2026-10-16):**
> MISAKA は、確率的検証と経済によって PALW を実現する。**Bond が採掘速度と報酬総額の上限を決め、モデル計算はその枠を使うために必要な正当な仕事になる。不正をしても枠は増えず、反証されれば担保を失う。**
> 不正をして高速に claim しても、実際に計算して claim しても、bond で時間内に出せる claim の量は変わらない。時間内の claim を bond で制限することで、正しい計算を促す。
> - モデルを公開しない自作自演には、2 つで対処する。モデル別配分を Panel と miner の claim bond の総額の 1.5 乗にすること(利用者と bond が集まらないモデルは不利になる)と、genesis・フォークを正規モデル、追加モデルを非正規モデル(報酬とブロック生成が 0.3 倍程度)にすることである。
> - G14 の定義:正しい Panel が 1 つでもいて、計算との不一致を確認できれば、それを客観的に証明して、slash・ブロック生成の拒否・報酬の拒否まで進められる。この定義が満たされていれば、正規モデルで「計算しないで提出する」不正を防げる。
> - **設計の前提となる条件:** (1) 不正や DoS の claim では、bond の slash による損失が、想定される利益を上回ること。(2) 客観的な court を悪用して、正常に計算している miner・Panel を slash させる不正ができないこと。


> **設計前提(全文書の上位)— [MISAKAの不可侵原則](PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。すべての ADR・RFC・Spec・設計文書はこの前提の下にあり、衝突する場合は前提が優先する(2026-10-09)。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

The code, the current CLI `--help` and the ADR decisions are authoritative. This map is a short, purpose-based entry point: every long list lives in the index it points to. When a historical report conflicts with a current operator document, use the current operator document and verify against `--help` and chain RPC.

**A decision being accepted is not the same as it being implemented, verified, armable or activated.** [STATUS.md](STATUS.md) keeps those five states apart, one row per feature.

## By purpose

| I want to … | Go to |
| --- | --- |
| **Build** | [getting-started/build.md](getting-started/build.md) |
| **Run a node** | [getting-started/run-node.md](getting-started/run-node.md) |
| **Mine** (PALW producer) | [getting-started/mining.md](getting-started/mining.md) → [testnet12-join-mining.md](operations/testnet-12/testnet12-join-mining.md) |
| **Run a validator** | [getting-started/validator.md](getting-started/validator.md) → [validator-runbook.md](operations/validator/validator-runbook.md) |
| **Test and benchmark** | [getting-started/testing.md](getting-started/testing.md), [testing/](testing/) |
| **Operate** (models, EVM backend, PQ verification, release, mainnet readiness, liveness probe) | [operations/README.md](operations/README.md) |
| **Understand the architecture** | [architecture/overview.md](architecture/overview.md) · [consensus](architecture/consensus.md) · [PALW](architecture/palw.md) · [post-quantum](architecture/pq.md) · [EVM](architecture/evm.md) · [what is different from Kaspa](architecture/differences-from-kaspa.md) |
| **Read the specification** | [spec/README.md](spec/README.md) (PALW chapters in [spec/palw/](spec/palw/)), [kaspa-pq-spec.md](spec/kaspa-pq-spec.md) |
| **Find a decision (ADR)** | [adr/README.md](adr/README.md) (number order) · [adr/INDEX-by-topic.md](adr/INDEX-by-topic.md) (**read first**: ADR-0175, 0176, 0177, 0179, 0180) · [adr/HISTORY.md](adr/HISTORY.md) (long per-ADR history) · [adr/NUMBERING.md](adr/NUMBERING.md) (duplicate numbers) |
| **Read a proposal (RFC)** | [rfc/README.md](rfc/README.md) (RFC 0001–0020, number order) · [rfc/INDEX-by-topic.md](rfc/INDEX-by-topic.md) (**read first**: RFC-0016, 0017, 0018) · [rfc/HISTORY.md](rfc/HISTORY.md) |
| **See what is done** (decision → implemented → verified → armable → activated) | [STATUS.md](STATUS.md), then [activation readiness matrix](design/palw/activation-readiness-matrix.md) and [remaining work (2026-10-16)](design/palw/remaining-work-2026-10-16.md) |
| **Look up a registry, manifest or index** | [reference/README.md](reference/README.md) |
| **Read a design record** | [design/README.md](design/README.md) — [design/palw/](design/palw/) (PALW lanes, matrices, user decisions), [design/evm/](design/evm/), [design/consensus/](design/consensus/) |
| **Check an audit or its evidence** | [audit/](audit/README.md) · [evidence/](evidence/README.md) · [security/](security/README.md) · [testing/](testing/) — engineering evidence, not current defaults |
| **Read history** (testnet-10 / testnet-11 runbooks, regenesis records, the pre-2026-10-12 root README) and **find a moved file** | [history/README.md](history/README.md), [history/MOVED.md](history/MOVED.md) |
| **Read the design premise** | [PRINCIPLES.md](PRINCIPLES.md) (§8 no cryptographic computation proofs · §9 two model tiers · §10 the bond bounds mining speed and total reward) |
| **Read the wiki mirror** | [wiki/README.md](wiki/README.md) |

## Current network — testnet-12

- [testnet-12 launch note (2026-09-25)](operations/testnet-12/t12-launch-2026-09-25.md) — the release, its known issues and when a payment is final
- [Join testnet-12 as a PALW producer](operations/testnet-12/testnet12-join-mining.md)
- [testnet-12 regenesis record](history/testnet-12/testnet-12-regenesis-2026-09-23.md)
- [Mainnet readiness](operations/mainnet/mainnet-readiness.md), [Release process](operations/release-process.md)

Invariants to check on testnet-12: consensus params fingerprint `5ee7fd8ee019968cf52929b844cf9ddfb1aad500842a89cf04bced8ba4edefb6`,
genesis `a27f8f44fe4d91a5…`, schedule id `1678e07359f6727e…` (`kaspad --testnet --netsuffix=12`, or `misaka --network testnet-12`).
Never count blocks, blue-score depth or DAA as finality on testnet-12; use finality depth or a `Final` PALW anchor.
What testnet-12 actually runs, and what is still dormant, is in [STATUS.md §1](STATUS.md).

## Other design records worth knowing

- [2026-10-12 RFC/ADR review](design/palw/rfc-adr-review-resolution-2026-10-12.md) and the [Panel=1 / Bond-time / model-capital proposal](design/palw/panel1-bond-budget-model-capital-amendments-2026-10-12.md)
- [Round / EXEC additional acceptance conditions (2026-10-10)](design/palw/palw-round-exec-additional-acceptance-2026-10-10.md)
- [Registry of numbers across the remaining RFCs](design/palw/remaining-rfc-integration-matrix.md)
