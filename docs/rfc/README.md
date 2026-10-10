# RFC index

**トークン名は Misaka、ticker は BILI。** アドレスの `misaka` 系プレフィックスと既存のチェーンID・コマンド・wire/API識別子は維持する。過去の実測・出力のMSKは旧表記として保存する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

## MISAKAの中核目標と設計の優先順位（2026-10-07）

[ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を優先する。**普通の非Panel public bondが、producerの秘密状態を使わず、public authenticated materialだけから不正をlocalizeしobjective convictionまで完結できること**を中核目標にする。[RFC14](0014-panel-independent-fraud-prosecution.md)が主実装設計、[RFC15](0015-panel-free-permissionless-verification.md)はその全gate成立後も別途受入・activationを必要とするPanel=0設計である。全RFC/ADRの確認・改定範囲は[監査記録](../adr/evidence/0173-mission-alignment-audit-2026-10-07.md)に記載する。文書の追加・改定でruntimeを更新したとは扱わない。

An RFC is a proposal under discussion. [INDEX.md](../INDEX.md) §3 explains how an RFC becomes an ADR
and a Spec change. Take the next number from this table, not from `ls`: an RFC can live on a branch
before it reaches `main`.

| RFC | Title | Status | Where |
| --- | --- | --- | --- |
| 0001 | PALW inference surface gaps: deterministic decode controls, serving and input extensions (FP Job V4) | §A (the FP Job V4 release) Implementation Frozen, 2026-09-27 (G0). §0–§7 (P1–P3) Draft, for a later release | [0001-palw-inference-surface-gaps.md](0001-palw-inference-surface-gaps.md). The former branch reservation is historical; the file is included in this reviewed document set. |
| 0002 | PALW Canonical Tensor IR v1 (PALW-TIR): a bounded, deterministic integer tensor program as the consensus meaning of a class, with a reference evaluator and optional fused kernels; v1 frozen on an architecture corpus; Pre-beacon immutable class/plan/challenge-policy binding added in §II.13; actual beacon/transcript remains evidence | Revised 2026-10-08 — existing implementation/test records keep their scope; new policy binding unactivated | [0002-palw-tensor-ir.md](0002-palw-tensor-ir.md) (branch `rfc/0002-tensor-ir`) |
| 0003 | PALW Generative Model Classes: one job, determinism and output layer (deterministic randomness R, canonical tensors and why no execution profile, canonical outputs), and the class profiles on top of it (image generation in detail; text, embedding, multimodal input, audio, video); Generation R strictly separated from RFC07 post-commit verification randomness | Revised 2026-10-08 — new challenge-policy boundary; historical profile/tests unchanged | [0003-palw-generative-model-classes.md](0003-palw-generative-model-classes.md) (branch `rfc/0003-image-generation`) |
| 0004 | PALW Model Improvement Protocol: off-chain training, kernel-bound candidate admission, probabilistic constraint-verified evaluation and exact promotion arithmetic, with explicit assurance labels | Revised Draft, 2026-10-06 — no VM dependency or later VM programme; kernel extension follows ADR-0172; new verification not activated | [0004-palw-model-improvement.md](0004-palw-model-improvement.md) |
| 0005 | PALW VM withdrawal reason | Withdrawn — BVM/GVM will not be implemented | [0005-palw-ml-vm.md](0005-palw-ml-vm.md) |
| 0006 | PALW layer-sharded panels: bounded cell checks with authenticated boundaries, scope-aware receipts and independent public-bond localization/court gates; first-divergence localization is conditional on acquired material and actual checks | Draft, 2026-10-01 | [0006-palw-layer-sharded-panels.md](0006-palw-layer-sharded-panels.md) (branch `rfc6/gpu-shard`) |
| 0007 | PALW constraint verification: Part VI is the sole post-commit PALW-work beacon/challenge protocol; Part V remains scoped Freivalds/GKR verification. Parts I–IV preserve their historical/prototype evidence | Revised Draft, 2026-10-08 — source/codec/security/implementation gates open; no beacon or activation proof | [0007-palw-verification-certificates-and-algebraic-checks.md](0007-palw-verification-certificates-and-algebraic-checks.md) |
| 0008 | Unified PALW EXEC lane: EXEC_TX from existing Final-credit permits and EXEC_SLICE for authenticated non-overlapping work of an active REAL-root claim; separate accounting, one root Final/settlement, zero EXEC fork-choice/DAA; main heartbeat/BASE-0/liveness preserved; Inherits RFC07 policy per committed root/slice statement; zero EXEC weight/DAA and unchanged main liveness | Revised Draft, 2026-10-08 — design only; former algo-11 design deleted, historical test record retained | [0008-palw-claim-backed-consensus-blocks.md](0008-palw-claim-backed-consensus-blocks.md) |
| 0009 | PALW Remote Client: node-less model registration with user-paid GAS/bond, signed remote claims, independent evidence delivery, public receipt redemption with bond-bound payout, and a verifiable client | Draft, upstream updated 2026-10-06 retained; mission amendment 2026-10-07 — design only; no activation | [0009-palw-remote-miner.md](0009-palw-remote-miner.md) |
| 0010 | Permissionless PALW Panel binding and claim completion: remove the eight-genesis anchor privilege through sealed claims, frozen public seat snapshots, binder-independent Panel randomness, and open end-to-end claim paths; Panel draw randomness and verification challenges remain separate versioned contracts | Revised Draft, 2026-10-08 — RFC07 separation added; dormant reference engine and guarded policy implemented on pre; production beacon/handoff and release gates pending; no live seed migration or activation | [0010-permissionless-palw-panel-and-claim-completion.md](0010-permissionless-palw-panel-and-claim-completion.md), [implementation](0010-dormant-implementation.md) |
| 0011 | Permissionless model onboarding with probabilistic encoded-constraint checks and exact court on dispute; active-kernel plan admission, explicit kernel-extension gaps (no VM fallback), 9B/2M barriers and ≥90% full-task HF evidence; §17 Static Admission → Beacon Conformance → Active Eligibility; RegisteredDormant can wait asynchronously | Revised Draft, 2026-10-08 — RFC07 policy; source/soundness/G14 gates open, 9B/2M/HF evidence remains scoped | [0011-permissionless-model-and-long-context-onboarding.md](0011-permissionless-model-and-long-context-onboarding.md) |
| 0012 | PALW-only consensus and native EVM settlement: retire DNS validators, attestations, precommits, DNS-final and stake reorg veto; PALW-derived EVM heads, legacy bond/reward wind-down and coordinated migration; Verification beacon is derived PALW data, never DNS/BFT or a new reorg/finality authority | Revised Draft, 2026-10-08 — authority separation added; dormant implementation on pre; native settlement/migration gates pending; no activation | [0012-palw-only-consensus-and-native-evm-settlement.md](0012-palw-only-consensus-and-native-evm-settlement.md) |
| 0013 | Reproducible exact layouts and resource-bounded onboarding tools: historical/live gate separation, bit-exact calibration interchange, independent streamed conformance and honest registration evidence; §9 ConformanceCommitmentV1, BeaconConformanceEvidenceV1 and reproducible asynchronous resume/retry | Revised Draft, 2026-10-08 — proposed tooling/evidence fields unimplemented; previous test scope preserved | [0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) |
| 0014 | Public-verifier dispute completeness; public challenge/source/transcript reconstruction and exact escalation to G14 | Revised Draft — prosecution/randomness gates pending; MISAKA Torrent integration withdrawn 2026-10-10; no activation | [0014-panel-independent-fraud-prosecution.md](0014-panel-independent-fraud-prosecution.md) |
| 0015 | Deferred Panel=0: permissionless verifiers and objective fraud proofs replace fixed-Panel verification; G14 adds post-commit challenge completeness; same-seed watchers remain non-independent | Deferred Revised Draft, 2026-10-08 — all RFC14 and RFC15 gates plus explicit activation still mandatory | [0015-panel-free-permissionless-verification.md](0015-panel-free-permissionless-verification.md) |

**Next free number: RFC-0016.**

## Shared post-commit challenge protocol (2026-10-08)

[RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) is the
sole proposed source/seed/sampling/transcript protocol for Kernel/model conformance, claims, EXEC slices and public
checks. Future independently valid canonical PALW useful work supplies the proposed beacon; no committee/DNS/BFT
signature or heartbeat/BASE-0/EXEC-hash fallback. Source unpredictability/bias/bootstrap and all implementation gates
remain open. [ADR171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md),
[ADR172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) and
[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) bind this direction.

RFC02 and the [Kernel design](../design/palw/versioned-kernels.md) bind immutable class/Kernel/policy identity; RFC11 §17 separates Static Admission, Beacon Conformance and
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
all twelve RFCs and relevant ADR dispositions. RFC05 retains only the VM withdrawal reason;
selected Kernel schemas and gates are in the [Kernel design](../design/palw/versioned-kernels.md). No activation is implied.

## Withdrawn designs

不採用・撤回済みの案は理由と後継文書へのリンクだけを残し、仕様・実装手順・見積もりは削除する。旧版は Git 履歴で参照できる。
RFC05 の VM、RFC14 §16 の MISAKA Torrent、ADR0023 の三 lane 案は実装対象外。DNS validator の旧設計は [RFC12](0012-palw-only-consensus-and-native-evm-settlement.md) の移行方針に従い、移行前の規則と将来の採用方針を区別する。

## ADRs that read as proposals

These ADRs decide nothing, or were forward-looking designs with nothing built. They keep their ADR
numbers. Reopening one means filing a new RFC that cites it.

| ADR | Why it reads as an RFC |
| --- | --- |
| [0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | "Decides nothing and changes no rule". It asks whether an inference can be the ticket without a hash lottery |
