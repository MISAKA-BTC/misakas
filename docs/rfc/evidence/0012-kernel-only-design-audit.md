# RFC01–12 / ADR consistency audit — kernel-only model extension

Date: 2026-10-06. Source baseline: `40dbbc889d4503ad1644bd98f98f38ce864692dc`
on `misakas/main`; audit edits are documentation only. No registration, implementation,
consensus activation, benchmark or security proof is asserted here.

## Governing direction

[ADR0172](../../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
governs extensibility; [ADR0171](../../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md)
and [RFC11 §§15–16](../0011-permissionless-model-and-long-context-onboarding.md) govern the new
large-model verifier and onboarding route.

* Active generic kernels + bounded typed plans admit new models/compositions without model-name
  allowlists. Missing semantics, checker, memory or court relations require a reusable versioned
  kernel extension and coordinated activation. No BVM/GVM/Universal VM or uploaded verifier.
* “SegWit-class” means a protocol extension comparable in scope, not automatic soft-fork safety.
  Unknown rewarded kernels fail closed; class meanings/identities and legacy replay stay immutable.
* Normal verification binds the computation's full constraint relation, then uses small reviewed
  probabilistic checks. Require positive receipts, DA and the challenge window before Final;
  exact bounded court handles detected/filed disputes, not routine giant-segment replay.
* TEE/enclave validity trust, model/fraud-proof VMs and spML-style BFT orchestrator/PKI/committee
  beacon authority are excluded, including as future dependencies. Native EVM is not a model VM.
* References to historical exact replay and withdrawn VM proposals are retained with explicit
  precedence; this audit does not rewrite already-active protocol semantics.

## RFC disposition

| RFC | Finding and disposition |
| --- | --- |
| [01](../0001-palw-inference-surface-gaps.md) | Frozen decode semantics retained. Added future-profile boundary: all decode constraints covered, generation RNG distinct from verifier challenges, no model VM or routine segment replay in the new route. |
| [02](../0002-palw-tensor-ir.md) | **Active contradiction fixed:** verdicts, alternatives, R8/A8, future-family routes and §II.12 still directed residual models to BVM/GVM. Replaced with active kernels / explicit extension gaps; coverage denominators retained and unimplemented residuals counted as failures. Legacy static-admission/reference evaluator scoped explicitly. |
| [03](../0003-palw-generative-model-classes.md) | Removed future VM `Tests` dependency. Pipelines use approved kernel relations, cover all stages/edges/output and compose whole-task soundness; legacy replay and generation randomness are not the new checker policy. |
| [04](../0004-palw-model-improvement.md) | Already aligned after `40dbbc889`: off-chain training; kernel-bound candidates; probabilistic evaluation claims; exact promotion arithmetic. VM phases/EXEC promises withdrawn. Improvement statistics remain distinct from checker soundness. |
| [05](../0005-palw-ml-vm.md) | §§K.0–K.8 already replace VM implementation with kernel descriptors/plans and coordinated upgrades. Added explicit exclusion of TEE trust and BFT orchestration. Old Parts I/II remain a labeled withdrawn historical appendix, not implementation authority. |
| [06](../0006-palw-layer-sharded-panels.md) | Legacy cell replay/first-divergence lemma preserved as baseline. New-profile boundary uses constraint checks, complete cross-cell coverage and composed soundness; the replay lemma alone does not prove a small-query checker secure. |
| [07](../0007-palw-verification-certificates-and-algebraic-checks.md) | Part V already selects small checks, but §V.7 still allowed a TIR/VM terminal. Replaced it with TIR/kernel-only court. Added approved-kernel/plan binding and prohibition of arbitrary circuit/VM fallback. Parts I–IV remain historical/baseline, not a substitute for whole-claim evidence. |
| [08](../0008-palw-claim-backed-consensus-blocks.md) | Added kernel/suite binding to work slices; checks cover boundary/predecessor state, and session errors compose. Reusing evidence/challenges does not create independent repetitions or duplicate work credit. BLUE/pre-Final safety gates unchanged. |
| [09](../0009-palw-remote-miner.md) | Remote execution/DA cannot evade the kernel boundary. New evidence binds kernel/plan/suite, serves small checks normally and exact court on dispute; legacy delivery remains supported. |
| [10](../0010-permissionless-palw-panel-and-claim-completion.md) | Open Panel assignment retained. Added suite-aware capability/resources and separation of Panel seed, generation RNG and round-specific proof challenges. No imported BFT/DNS validator authority. |
| [11](../0011-permissionless-model-and-long-context-onboarding.md) | §§15–16 already aligned; added §15.9 research comparison, MatMul-first measurement sequence, explicit excluded architectures and economic-audit versus algebraic-soundness boundary. Real 9B/2M barriers and all-HF census remain. |
| [12](../0012-palw-only-consensus-and-native-evm-settlement.md) | Already consistent: removes DNS finality authority, not node validation/Panel/court. Native EVM settlement remains; it is neither a model-verification VM nor a fallback for unsupported kernels. No DNS/BFT committee imported from spML. |

## ADR review and historical boundaries

| ADR / group | Disposition |
| --- | --- |
| [0067](../../adr/0067-classes-are-chain-data-kernels-are-the-build.md) | Model data versus consensus kernel code is retained. Added current typed-plan/new probabilistic-profile boundary; original replay decisions remain historical. |
| [0078](../../adr/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md), [0108](../../adr/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) | Content identity/material derivation is not a license to upload executable semantics. Added an explicit clarification to 0108: manifest recomputation is admission/identity, not compulsory whole-inference replay. |
| [0133](../../adr/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md), [0135](../../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) | Historical registry/window/seat formulas remain baseline. Added 0135 amendment: “canonical ML VM” meant the then-existing kernel evaluator, not a future ISA VM. New checkers need separately derived resource schedules and readiness tests. |
| [0098](../../adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md), [0099](../../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md), [0100](../../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) | Coverage/availability/shard assumptions are not superseded by relabeling spot checks “algebraic.” RFC06/07/11's new-profile coverage and error composition take precedence only at a future explicit fence. |
| [0126](../../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md), [0128](../../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) | Historical DNS authority remains until RFC12's separately tested migration. Not a future verification dependency. |
| [0171](../../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md) | Removed normative primitive/VM-template wording; approved kernel templates only. Added research/benchmark basis and explicit excluded trust architectures. |
| [0172](../../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) | Governing no-VM decision retained; updated cross-RFC supersession, this audit link and TEE/BFT exclusions. |

## What this does not prove

This consistency audit is not evidence that a checker is implemented/sound, a model registered,
or a measured coverage percentage achieved. Freivalds does not cover unbound/nonlinear/memory
relations alone. Sparse raw sampling, DAS, economic deterrence and an exact dispute court cannot
each independently remove an undetected computation fault. Review and benchmark the combined
kernel/proof/Panel/DA/Final protocol before choosing an activation height. RFC11's conditional
`ε_check ≤ 2^-128` is a proposed bound, not demonstrated total network security.
