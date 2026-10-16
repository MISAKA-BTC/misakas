# RFC index by topic

> 番号順の一覧は [README.md](README.md)、旧 README の本文は [HISTORY.md](HISTORY.md)、決定が実装・検証・有効化のどこまで進んだかは [../STATUS.md](../STATUS.md)。
> 1 つの RFC は主題を 1 つだけ選んで載せています。† は同じ番号に 2 つのファイルがあることを示します([../adr/NUMBERING.md](../adr/NUMBERING.md))。
> **RFC の Status(Draft / Revised Draft / Accepted design)は設計の状態です。** 実装済み・検証済み・有効化済みとは別です([../STATUS.md](../STATUS.md))。

## まず読む — PALW とモデルの規則

設計の前提は [../PRINCIPLES.md](../PRINCIPLES.md)(§8 暗号学的な計算証明を使わない、§9 モデルの二階層、§10 bond が採掘速度と報酬総額の上限を決める)です。

1. [RFC-0016](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) — bond 加重のブロック生成機会と弾力的 Final 容量(V+ を物理層の設計として採用、値は測定待ち)。 _(Status: Accepted design (V+), values pending measurement, 2026-10-15 — implemented dormant on integ/rfc0016-impl (unbuilt; fences refused when armed until measured values are pinned)…)_
1. [RFC-0017](0017-model-eligibility-examination-and-renewal.md) — モデル階層(genesis・年次フォーク 1.0、登録モデル 0.3)。モデル単位の審査は廃止。採用された規則は ADR-0179。 _(Status: Draft, 2026-10-15 — tier fence palw_model_tier_v1 implemented, dormant, unverified (ADR-0179); the old review code dormant and unreachable under it…)_
1. [RFC-0018](0018-dag-parallel-multi-model-consensus.md) — DAG 並行の多モデル合意、特権なき Panel anchor、Final と SecurityCertified(Gate C)の分離。 _(Status: Draft, 2026-10-15 — implemented, dormant, unverified…)_

これらに先立つ ADR は [../adr/INDEX-by-topic.md](../adr/INDEX-by-topic.md) の「まず読む」(ADR-0175、0176、0177、0179、0180)です。

## Consensus / BlockDAG

- [RFC-0012 † — dormant implementation](0012-dormant-implementation.md) — implemented behind an unassigned Params::palw_dns_retirement fence. All shipped network presets use None. No network, activation height, D/W…
- [RFC-0012 † — PALW-only consensus and native EVM settlement: retire DNS validators and their reorg veto](0012-palw-only-consensus-and-native-evm-settlement.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…
- [RFC-0018 — DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束](0018-dag-parallel-multi-model-consensus.md) — Draft, 2026-10-15 — implemented, dormant, unverified…

## Post-quantum crypto

この主題の RFC はありません。ADR を参照してください([../adr/INDEX-by-topic.md](../adr/INDEX-by-topic.md))。署名方式・UTXO・hash identity は ADR-0002/0003/0004/0005/0008/0019 にあります。

## EVM and execution lanes

- [RFC-0005 — PALW VM（不採用）](0005-palw-ml-vm.md) — Revised Draft, 2026-10-08 — §§K.2/K.4 use RFC07; no VM fallback or activation
- [RFC-0008 — PALW EXEC lane — claim-backed work slices and transaction execution](0008-palw-claim-backed-consensus-blocks.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…

## Network and remote participation

- [RFC-0009 — PALW Remote Client — node を持たないモデル登録・claim・非保管型の報酬回収](0009-palw-remote-miner.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…

## Economics, rewards and block opportunity

- [RFC-0016 — bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) — Accepted design (V+), values pending measurement, 2026-10-15 — implemented dormant on integ/rfc0016-impl (unbuilt; fences refused when armed until measured values are pinned)…

## PALW verification: panels, certificates, prosecution

- [RFC-0006 — PALW layer-sharded panels — bounded cell verification from authenticated boundaries, with…](0006-palw-layer-sharded-panels.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…
- [RFC-0007 — PALW probabilistic detection — direct checks, authenticated receipts, and deterministic court](0007-palw-verification-certificates-and-algebraic-checks.md) — Revised Draft, 2026-10-08 — source/codec/security/implementation gates open; no beacon or activation proof
- [RFC-0010 † — dormant implementation and permissionlessness audit](0010-dormant-implementation.md) — Status 行なし — ファイルを開いて確認
- [RFC-0010 † — Permissionless PALW Panel binding and claim completion](0010-permissionless-palw-panel-and-claim-completion.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…
- [RFC-0014 — Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ](0014-panel-independent-fraud-prosecution.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…
- [RFC-0015 — Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する](0015-panel-free-permissionless-verification.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…

## Model onboarding, classes, tiers and upgrades

- [RFC-0001 — PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計](0001-palw-inference-surface-gaps.md) — §A (the FP Job V4 release) Implementation Frozen, 2026-09-27 (G0). §0–§7 (P1–P3) Draft, for a later release
- [RFC-0002 — PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor program as the…](0002-palw-tensor-ir.md) — Revised 2026-10-08 — existing implementation/test records keep their scope; new policy binding unactivated
- [RFC-0003 — PALW Generative Model Classes — one job, determinism and output layer, and the class profiles on…](0003-palw-generative-model-classes.md) — Revised 2026-10-08 — new challenge-policy boundary; historical profile/tests unchanged
- [RFC-0004 — PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR](0004-palw-model-improvement.md) — Revised 2026-10-09 — 独立immutable fenceとcandidate選択をpreに実装、全preset休眠。memory materialの担保条件・kind別MEASは未決/未測定
- [RFC-0011 — Permissionless model onboarding with probabilistic constraint verification and court on dispute](0011-permissionless-model-and-long-context-onboarding.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…
- [RFC-0013 — Reproducible layouts and resource-bounded model onboarding tools](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) — Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no…
- [RFC-0017 — モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)](0017-model-eligibility-examination-and-renewal.md) — Draft, 2026-10-15 — tier fence palw_model_tier_v1 implemented, dormant, unverified (ADR-0179); the old review code dormant and unreachable under it…
- [RFC-0019 — 年次モデル追加フォーク: 段階的ソフトウェア展開・年次モデルカタログ・合意は不変](0019-annual-model-upgrade.md) — Draft, 2026-10-15 — implemented, dormant, unverified…
- [RFC-0020 — 長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない](0020-long-context-calibration-memory.md) — Draft, 2026-10-16 — tooling only, no consensus surface; implemented, unverified (ram_account.rs, float_ref/stream.rs, palw-tir-convert/palw-tir-fidelity flags)…
