# RFC index

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。この索引のすべての文書はこの前提の下にあり、衝突する場合は前提が優先する(2026-10-09)。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**トークン名は Misaka、ticker は BILI。** アドレスの `misaka` 系プレフィックスと既存のチェーンID・コマンド・wire/API識別子は維持する。過去の実測・出力のMSKは旧表記として保存する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

## MISAKAの中核目標と設計の優先順位（2026-10-07）

[ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を優先する。**普通の非Panel public bondが、producerの秘密状態を使わず、public authenticated materialだけから不正をlocalizeしobjective convictionまで完結できること**を中核目標にする。[RFC14](0014-panel-independent-fraud-prosecution.md)が主実装設計、[RFC15](0015-panel-free-permissionless-verification.md)はその全gate成立後も別途受入・activationを必要とするPanel=0設計である。全RFC/ADRの確認・改定範囲は[監査記録](../adr/evidence/0173-mission-alignment-audit-2026-10-07.md)に記載する。文書の追加・改定でruntimeを更新したとは扱わない。

An RFC is a proposal under discussion. [INDEX.md](../INDEX.md) §3 explains how an RFC becomes an ADR
and a Spec change. Take the next number from this table, not from `ls`: an RFC can live on a branch
before it reaches `main`.

**2026-10-10追加前提:** [ADR176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) / RFC15 §8は、確率的検証・公開prosecutionとbond/DAA採掘予算をANDで要求する。claim容量を増やしてもblock/reward/Final weight総量は増やさない。[全RFC/ADR改定・文書検証記録](../adr/evidence/0176-bond-budget-policy-alignment-2026-10-10.md)。

| RFC | Title | Status | Where |
| --- | --- | --- | --- |
| 0001 | PALW inference surface gaps: deterministic decode controls, serving and input extensions (FP Job V4) | §A (the FP Job V4 release) Implementation Frozen, 2026-09-27 (G0). §0–§7 (P1–P3) Draft, for a later release | [0001-palw-inference-surface-gaps.md](0001-palw-inference-surface-gaps.md). The former branch reservation is historical; the file is included in this reviewed document set. |
| 0002 | PALW Canonical Tensor IR v1 (PALW-TIR): a bounded, deterministic integer tensor program as the consensus meaning of a class, with a reference evaluator and optional fused kernels; v1 frozen on an architecture corpus; Pre-beacon immutable class/plan/challenge-policy binding added in §II.13; actual beacon/transcript remains evidence | Revised 2026-10-08 — existing implementation/test records keep their scope; new policy binding unactivated | [0002-palw-tensor-ir.md](0002-palw-tensor-ir.md) (branch `rfc/0002-tensor-ir`) |
| 0003 | PALW Generative Model Classes: one job, determinism and output layer (deterministic randomness R, canonical tensors and why no execution profile, canonical outputs), and the class profiles on top of it (image generation in detail; text, embedding, multimodal input, audio, video); Generation R strictly separated from RFC07 post-commit verification randomness | Revised 2026-10-08 — new challenge-policy boundary; historical profile/tests unchanged | [0003-palw-generative-model-classes.md](0003-palw-generative-model-classes.md) (branch `rfc/0003-image-generation`) |
| 0004 | モデル改善：学習・評価を継続し、改善版は独立した不変モデルとして登録。既存head/version・Position/AMMの置換は禁止 | Revised 2026-10-09 — 独立immutable fenceとcandidate選択をpreに実装、全preset休眠。memory materialの担保条件・kind別MEASは未決/未測定 | [0004-palw-model-improvement.md](0004-palw-model-improvement.md), [ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md) |
| 0005 | Versioned model/verification kernels, not BVM/GVM: declarative plans within active families, coordinated SegWit-class extensions for missing semantics, encoded probabilistic checks and bounded exact court; Immutable approved checker/challenge/soundness tuple; Kernel differential conformance only after completeness | Revised Draft, 2026-10-08 — §§K.2/K.4 use RFC07; no VM fallback or activation | [0005-palw-ml-vm.md](0005-palw-ml-vm.md) (stable filename) |
| 0006 | Layer-sharded verification: authenticated boundaries, scope-bound receipts and conditional public-bond court; model acquisition outside consensus | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0006-palw-layer-sharded-panels.md](0006-palw-layer-sharded-panels.md) (branch `rfc6/gpu-shard`) |
| 0007 | PALW constraint verification: Part VI is the sole post-commit PALW-work beacon/challenge protocol; Part V remains scoped Freivalds/GKR verification. Parts I–IV preserve their historical/prototype evidence | Revised Draft, 2026-10-08 — source/codec/security/implementation gates open; no beacon or activation proof | [0007-palw-verification-certificates-and-algebraic-checks.md](0007-palw-verification-certificates-and-algebraic-checks.md) |
| 0008 | Unified EXEC_TX / EXEC_SLICE: one root settlement, unique work, zero EXEC weight/DAA, model-bond allocation and Rule E | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0008-palw-claim-backed-consensus-blocks.md](0008-palw-claim-backed-consensus-blocks.md) |
| 0009 | Node-less registration / signed remote claims: explicit GAS/bond, optional off-chain supply and finite claim-evidence duties | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0009-palw-remote-miner.md](0009-palw-remote-miner.md) |
| 0010 | Permissionless Panel binding: sealed claims, snapshots, shared challenge protocol and completion; no model-availability gate | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0010-permissionless-palw-panel-and-claim-completion.md](0010-permissionless-palw-panel-and-claim-completion.md), [implementation](0010-dormant-implementation.md) |
| 0011 | Permissionless full-task onboarding: immutable identity, conditional G14 and model-bond allocation; 9B/2M/HF historical scope retained | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0011-permissionless-model-and-long-context-onboarding.md](0011-permissionless-model-and-long-context-onboarding.md) |
| 0012 | PALW-only / native EVM: legacy DNS wind-down, model-bond allocation, Rule E and zero extra issuance | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0012-palw-only-consensus-and-native-evm-settlement.md](0012-palw-only-consensus-and-native-evm-settlement.md) |
| 0013 | Resource-bounded tools: conformance/resume, identity, distinct model capital and allocation evidence; fetch telemetry stays off-chain | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) |
| 0014 | Conditional public-verifier prosecution; §16 model-acquisition non-interference, distinct miner capital, coinbase allocation; MISAKA Torrent / Seeder rewards / FPR abolished | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0014-panel-independent-fraud-prosecution.md](0014-panel-independent-fraud-prosecution.md) |
| 0015 | Deferred Panel=0: bond/time Q/B/R/F budgets, capacity subdivision and §8.5 model-bond coinbase allocation; no acquisition consensus | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no activation | [0015-panel-free-permissionless-verification.md](0015-panel-free-permissionless-verification.md) |

**Next free number: RFC-0016.**

## Model acquisition non-interference and model-bond allocation — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) is the governing direction.
**MISAKA Torrent adoption/integration, dedicated Bonded Seeders and Seeder rewards are abolished.**
PoR, Full Fetch, availability leases, TRDC/FPR, whole-model disclosure funding and availability-driven
qualification/weight suspension are withdrawn. General off-chain model sharing is optional.
The chain neither manages nor guarantees acquisition; non-publication alone triggers no reward loss or Slash.

Preserve immutable Model ID/root/weights/spec and finite claim-specific evidence/court responsibility.
G14 prosecution is conditional on acquiring the registered model; the all-owner-refusal retrieval guarantee is withdrawn.
Compute distinct `S_m=sum_b C_{b,m}` with `sum_m C_{b,m}<=C_b_effective_locked`.
Allocate `R_m=R_PALW*f(S_m)/sum_j f(S_j)` inside the existing total coinbase budget, with defined zero/rounding/unused rules.
Valid computational work is required for payment; retain ADR176's individual Q/B/R/F ceilings and common DAA hold.
Coinbase allocation alone changes neither block cadence nor DAA/difficulty/fork choice.

The goal is an overwhelming economic advantage for publication that attracts external capital.
Equal capital gives equal allocation irrespective of ownership; wallet count is not independence evidence.
The multiplier curve, self-funding/Sybil/concentration, incumbent caps, competition and closed-model detection
must be evaluated before claiming that goal achieved. Allocation and economic implementation remain pending.
See [RFC14 §16](0014-panel-independent-fraud-prosecution.md), [RFC15 §8.5](0015-panel-free-permissionless-verification.md)
and the [current document validation](../adr/evidence/0177-model-distribution-policy-alignment-2026-10-10.md).

The earlier [Seeder](evidence/0014-independent-bonded-seeders-audit-2026-10-10.md) and
[FPR](evidence/0014-forced-public-retrieval-alignment-2026-10-10.md) checks are historical withdrawn-policy records.
Their PASS does not prove this economic goal, new allocation, G14 under closure or activation.

## Shared post-commit challenge protocol (2026-10-08)

[RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) is the
sole proposed source/seed/sampling/transcript protocol for Kernel/model conformance, claims, EXEC slices and public
checks. Future independently valid canonical PALW useful work supplies the proposed beacon; no committee/DNS/BFT
signature or heartbeat/BASE-0/EXEC-hash fallback. Source unpredictability/bias/bootstrap and all implementation gates
remain open. [ADR171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md),
[ADR172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) and
[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) bind this direction.

RFC02/05 bind immutable class/Kernel/policy identity; RFC11 §17 separates Static Admission, Beacon Conformance and
Active Eligibility; RFC13 §9 records commitments/evidence/resume; RFC14 §3.4 and RFC15 §1.1 extend G14 challenge replay
and exact escalation. RFC03/04/06/09/10/12 keep generation/hold-out/Panel/ticket/settlement authority distinct; RFC08
inherits the shared policy while keeping EXEC weight/DAA zero and main heartbeat/BASE-0/liveness unchanged.
See the [alignment and validation scope](evidence/0007-post-commit-randomness-alignment-2026-10-08.md).
This is a documentation revision, not a secure-beacon proof, conformance run, runtime release or activation.

## Model-extension precedence (2026-10-06)

[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) governs
RFC01–12's model extension: active versioned kernels and bounded declarative plans, never a model
VM fallback. [RFC11 §15.9](0011-permissionless-model-and-long-context-onboarding.md) defines the
MatMul-first probabilistic-check research/benchmark basis; TEE validity trust and BFT orchestrator
authority are excluded. The [consistency audit](evidence/0012-kernel-only-design-audit.md) lists
all twelve RFCs and relevant ADR dispositions. Older replay rules and RFC05's withdrawn VM appendix
are historical/conformance records, not new implementation instructions. No activation is implied.

## ADRs that read as proposals

These ADRs decide nothing, or were forward-looking designs with nothing built. They keep their ADR
numbers. Reopening one means filing a new RFC that cites it.

| ADR | Why it reads as an RFC |
| --- | --- |
| [0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | "Decides nothing and changes no rule". It asks whether an inference can be the ticket without a hash lottery |
| [0023](../adr/0023-base-three-lane-execution.md) | "Proposed — design freeze … Nothing is implemented". The Base three-lane execution design |
