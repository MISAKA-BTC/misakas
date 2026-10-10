# Documentation map

> **設計前提(全文書の上位)— [MISAKAの不可侵原則](PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。すべての ADR・RFC・Spec・設計文書はこの前提の下にあり、衝突する場合は前提が優先する(2026-10-09)。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

The code, current `main` CLI `--help` and ADR decisions are authoritative. This index separates live operator instructions from dated engineering evidence.

## Current Testnet-12 documents

- [Architecture overview](architecture/overview.md) — the protocol as it is now, topic by topic, with the ADRs that govern each part and where its code lives
- [Mainnet readiness](mainnet-readiness.md) — what is done, partly done and not done before a mainnet genesis, with the evidence for each
- [Release process](release-process.md) — how a release is cut, signed and verified
- [testnet-12 launch note (2026-09-25)](t12-launch-2026-09-25.md) — the release, its known issues and when a payment is final
- [Join testnet-12 as a PALW producer](testnet12-join-mining.md)
- [testnet-12 regenesis record](testnet-12-regenesis-2026-09-23.md)

## Testnet-11 documents (previous network; build `1f98d3bf4` to run it)

- [testnet-11 history](history/testnet-11.md) — the flag days, fingerprints and rollout notes that used to head the README

- [Join as a PALW producer](testnet11-join-mining.md)
- [Run a full node](testnet11-node-operator.md)
- [The DAA 7,101 PALW upgrade and Verification V2 at 7,200](testnet11-7101-upgrade-announcement.md) — what fires at 7,100, 7,101, 7,200, 7,300, 7,301 and 6,900, and how to diagnose the execution lane
- [Pre-arming security audit of the DAA 7,101 bundle (2026-09-18)](palw-audit-2026-09-18-6001.md) — two Critical and six High findings, the seven release blockers and their fixes, and what is left as residual risk
- [The DAA clock under the 7,101 bundle (2026-09-18)](palw-daa-clock-audit-2026-09-18.md) — every lane's effect on the DAA score, `bits` and blue work; the window arithmetic at 120 s / measured / max lane load; the blocker and its fix (ADR-0138)
- [Run a DNS-finality validator](validator-runbook.md)
- [Operate model classes](palw-public-testnet-classes-runbook.md)
- [Add a model through the SDK](palw-model-onboarding-sdk.md)
- [Free-prompt mining](testnet11-free-prompt-mining.md)
- [EVM flat backend migration](misaka-evm-flat-backend-runbook-v0.1.md)
- [Node liveness probe](node-liveness-probe.md)

## Governing design

- [モデル入手への不介入・model-bond coinbase配分 — ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md) — MISAKA Torrent・専用Seeder/Seeder報酬を廃止。固定identityと個別bond上限を維持し、公開参加の経済優位を評価する。配分式/実装/activationは未完了。
- [今回のdocs検証](adr/evidence/0177-model-distribution-policy-alignment-2026-10-10.md) — 旧可用性規範の撤回と新方針への整合。経済・runtime試験のPASSではない。

- [PALW bond/time production and reward premise — ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) — claim capacity may grow while bond-attributed blocks, rewards and Final weight stay bounded; probabilistic checks, public prosecution and collectible liability remain necessary. Design accepted; implementation/activation pending.
- [RFC-0015 §8](rfc/0015-panel-free-permissionless-verification.md) — shared reservation, common DAA hold, claim-right subdivision and Final-weight accounting for the new design.
- [ADR index](adr/README.md)
- [PQ specification](kaspa-pq-spec.md)
- [ML-DSA-87 design](kaspa-pq-design-mldsa87.md)
- [PALW versioned Kernel design](design/palw/versioned-kernels.md)
- [PALW registry map](palw-registry-map.md)
- [PALW extension envelope](palw-extension-envelope.md)

## Historical evidence

Files whose names contain a date, audit, launch record, old relaunch, testnet-10, testnet-21, shadow drill or transition are retained as engineering evidence. Their measured hashes, peers, DAA scores, class IDs and commands describe that historical run and are not current operator defaults.

In particular, do not derive a current command from:

- `testnet10-*.md`
- `testnet11-relaunch2-*.md`, `relaunch5*.md` or old genesis cards
- dated `palw-*-2026-*.md` audits and measurements
- archived mainnet-readiness or drill reports (not [mainnet-readiness.md](mainnet-readiness.md), which is kept current)

When a historical report conflicts with a current operator document, use the current operator document and verify against `--help` and chain RPC.

## Current invariants worth checking

- network: `testnet-11`
- fingerprint: `400403b8431082c9464d7326c3c11f77425ef3dbc41110f85a0dd28cb6f5f2d8` (the current main build; DAA 7,100 is the held/deep-audit boundary, 7,101 is the `6001`-named PALW upgrade bundle including ADR-0125's 1-BPS execution lane, 7,200 arms ADR-0133 Verification V2/readiness multiproofs, 7,300 shortens the execution span 5 DAA → 1 DAA, and 7,301 retires the compute overlay)
- PALW cadence: 120 seconds per block
- class shares at Relaunch 5f genesis: Floor 22‰, A16 489‰, QWEN36 489‰
- DNS Testnet-11 minimum stake Bond: 10 BILI
- ADR-0123 epoch-budget release: implemented, dormant on shipped presets
