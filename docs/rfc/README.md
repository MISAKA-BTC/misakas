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
| 0002 | PALW Canonical Tensor IR v1 (PALW-TIR): a bounded, deterministic integer tensor program as the consensus meaning of a class, with a reference evaluator and optional fused kernels; v1 frozen on an architecture corpus | Draft, 2026-09-28 | [0002-palw-tensor-ir.md](0002-palw-tensor-ir.md) (branch `rfc/0002-tensor-ir`) |
| 0003 | PALW Generative Model Classes: one job, determinism and output layer (deterministic randomness R, canonical tensors and why no execution profile, canonical outputs), and the class profiles on top of it (image generation in detail; text, embedding, multimodal input, audio, video) | Draft, 2026-09-28 | [0003-palw-generative-model-classes.md](0003-palw-generative-model-classes.md) (branch `rfc/0003-image-generation`) |
| 0004 | PALW Model Improvement Protocol: off-chain training, kernel-bound candidate admission, probabilistic constraint-verified evaluation and exact promotion arithmetic, with explicit assurance labels | Revised Draft, 2026-10-06 — no VM dependency or later VM programme; kernel extension follows ADR-0172; new verification not activated | [0004-palw-model-improvement.md](0004-palw-model-improvement.md) |
| 0005 | Versioned model/verification kernels, not BVM/GVM: declarative plans within active families, coordinated SegWit-class extensions for missing semantics, encoded probabilistic checks and bounded exact court | Revised Draft, 2026-10-06 — ADR-0172 withdraws VM implementation; §§K.0–K.8 current, former VM proposal historical only; no activation | [0005-palw-ml-vm.md](0005-palw-ml-vm.md) (stable filename) |
| 0006 | PALW layer-sharded panels: bounded cell checks with authenticated boundaries, scope-aware receipts and independent public-bond localization/court gates; first-divergence localization is conditional on acquired material and actual checks | Draft, 2026-10-01 | [0006-palw-layer-sharded-panels.md](0006-palw-layer-sharded-panels.md) (branch `rfc6/gpu-shard`) |
| 0007 | PALW constraint verification: **Part V** replaces normal segment replay with batched Freivalds/GKR and complete constraint checks; scheme/scope/evidence-bound Panel receipts, coverage-aware tally, whole-claim error accounting and bounded exact court on dispute. Parts I–IV preserve the vertex, private-sketch and audit baselines | Revised 2026-10-06 — Part V selected under RFC11/ADR-0171, implementation/activation pending; existing components governed by spec 18 | [0007-palw-verification-certificates-and-algebraic-checks.md](0007-palw-verification-certificates-and-algebraic-checks.md) |
| 0008 | Unified PALW EXEC lane: EXEC_TX from existing Final-credit permits and EXEC_SLICE for authenticated non-overlapping work of an active REAL-root claim; separate accounting, one root Final/settlement, zero EXEC fork-choice/DAA; main heartbeat/BASE-0/liveness preserved | Revised Draft, 2026-10-08 — design only; no runtime change or activation; former algo-11 design deleted, test record retained | [0008-palw-claim-backed-consensus-blocks.md](0008-palw-claim-backed-consensus-blocks.md) |
| 0009 | PALW Remote Client: node-less model registration with user-paid GAS/bond, signed remote claims, independent evidence delivery, public receipt redemption with bond-bound payout, and a verifiable client | Draft, upstream updated 2026-10-06 retained; mission amendment 2026-10-07 — design only; no activation | [0009-palw-remote-miner.md](0009-palw-remote-miner.md) |
| 0010 | Permissionless PALW Panel binding and claim completion: remove the eight-genesis anchor privilege through sealed claims, frozen public seat snapshots, binder-independent Panel randomness, and open end-to-end claim paths | Draft; dormant reference engine and guarded policy implemented 2026-10-08; production beacon/handoff and release gates pending; no activation | [0010-permissionless-palw-panel-and-claim-completion.md](0010-permissionless-palw-panel-and-claim-completion.md), [implementation](0010-dormant-implementation.md) |
| 0011 | Permissionless model onboarding with probabilistic encoded-constraint checks and exact court on dispute; active-kernel plan admission, explicit kernel-extension gaps (no VM fallback), 9B/2M barriers and ≥90% full-task HF evidence | Revised Draft, 2026-10-06 — ADR-0171/0172; no activation, Kimi feasibility and 90% coverage unproven; §16 kernel-only route | [0011-permissionless-model-and-long-context-onboarding.md](0011-permissionless-model-and-long-context-onboarding.md) |
| 0012 | PALW-only consensus and native EVM settlement: retire DNS validators, attestations, precommits, DNS-final and stake reorg veto; PALW-derived EVM heads, legacy bond/reward wind-down and coordinated migration | Draft — dormant implementation; no activation, settlement/security parameters require evidence | [0012-palw-only-consensus-and-native-evm-settlement.md](0012-palw-only-consensus-and-native-evm-settlement.md) |
| 0013 | Reproducible exact layouts and resource-bounded onboarding tools: historical/live gate separation, bit-exact calibration interchange, independent streamed conformance and honest registration evidence | Draft, 2026-10-06 — tool fixes and release criteria; no consensus activation | [0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) |
| 0014 | Public-verifier dispute completeness; §16 integrates MISAKA Model Transport into misakas and makes class-bound model download availability a consensus reward/weight condition, with bonded service obligations and cold-verifier prosecution | Draft / implementation design, 2026-10-07 — ADR173 core objective; transport/availability/reward integration gates pending; no activation | [0014-panel-independent-fraud-prosecution.md](0014-panel-independent-fraud-prosecution.md) |
| 0015 | Deferred Panel=0: permissionless verifiers and objective fraud proofs replace fixed-Panel verification | Deferred Draft, 2026-10-07 — prohibited until all RFC14 completion gates and RFC15-specific gates pass; no activation | [0015-panel-free-permissionless-verification.md](0015-panel-free-permissionless-verification.md) |

**Next free number: RFC-0016.**

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
