# ADR-0172 — Model extensibility uses versioned kernels, not a universal VM

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** Design direction selected by the operator, 2026-10-06. Documentation decision only;
implementation, cryptographic review and network activation are pending. No fence height assigned.

**Changes:** [RFC04](../rfc/0004-palw-model-improvement.md)'s future automation roadmap,
[RFC05](../rfc/0005-palw-ml-vm.md)'s BVM/GVM implementation direction, and
[RFC11](../rfc/0011-permissionless-model-and-long-context-onboarding.md)'s VM fallback for registration.
**Preserves:** [ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md) and
[RFC07](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md): probabilistic constraint
verification, positive receipts, bounded exact court on dispute and explicit residual error.

## 日本語での決定

モデル対応のための **PALW-BVM・PALW-GVM・Universal VMは実装しない**。ISA、任意の分岐・ループ、
syscall、guestメモリ、gas、one-step VM court、toolchainを合意規則として維持する実装・監査・履歴再生の
コストを避ける。既存コードや実験用ブランチをこの文書で削除・無効化したとは扱わない。

代わりに、**少数の汎用的なversioned Kernel + 宣言的VerificationPlan + 合意更新によるKernel追加**を採用する。
既存演算の新しい組合せならfrontendとplanで対応し、本当に新しい演算・メモリ・検証原理だけをKernel更新にする。
モデル名ごとにQwen用・Kimi用Kernelを増やさない。未知演算を既存Kernelで表現できなければ、対応Kernelが
有効になるまで登録は明示的に待つ。CPUを増やすことや未検証コードのuploadでこの条件を迂回しない。

通常検証の方針は変更しない。**不正があれば極めて高い確率で検出されるよう計算をconstraintとして符号化・
集約し、小さい検査と必要なreceipt・DA・challenge windowを満たしてFinalする。** Panelが毎回全推論や
選ばれたsegment全部を再実行する設計へ戻さない。例外時だけ、拘束された証拠から有界のterminal courtへ落とす。
単なるtraceの抜取りやデータのerasure codingだけでは、単一点不正に対するこの保証は成立しない。

**追加の明示的禁止:** 参照論文にあるTEE/enclaveを正当性の信頼根拠にする方式、モデル検証・fraud-proof VM、
spML型BFT運営者委員会・trusted PKI・committee beaconは、この経路では採用しない。将来の代替依存としても
残さない。公開のidentity/bond規則、別途安全性を定義するpost-commit乱数、承認済みKernel検査器と有界PALW courtを使う。
論文から利用する検査・誘因・局所化の考え方と、利用しない信頼アーキテクチャを区別する。

## 1. Why remove the VM implementation programme

An existing production ISA still needs MISAKA-specific deterministic arithmetic, syscall bindings,
state commitments, metering, proof localization, replay and adversarial tests. Borrowing a VM does
not remove those costs. The previous RFC05 estimates are historical planning estimates, not measured
savings achieved by this decision. Kernel extensions also require reference implementations,
independent review, court/resource tests and safe upgrades; their costs are not zero.

The deliberate trade-off is a narrower accepted language today in exchange for a smaller consensus
surface. A novel operation may wait for a protocol release. There is no promise that every future
finite program can be admitted under today's kernels or that upgrades are needed only once in a
particular number of years. Such cadence and coverage need evidence.

## 2. Selected architecture

| Conceptual layer (not allocated wire ids) | Responsibility |
| --- | --- |
| K0 — base consensus | Canonical commitments, activation/fingerprint rules, bounded extension envelope, state transitions, accounting and resource limits. No uploaded verifier execution. |
| K1 — existing tensor semantics | Versioned deterministic tensor operators, bounded recurrence/state, canonical input/output, material openings and exact terminal courts. Existing classes keep their existing identities/rules. |
| K2 — probabilistic constraint verification | Approved matrix, range/lookup, nonlinear, routing, authenticated-memory and boundary relations; Freivalds/GKR or another explicitly reviewed suite; soundness composition and bounded localization. This is a proposed extension, not a currently active kernel. |
| Later kernels | Reusable new semantic/verification families that cannot safely be expressed by the active set. Old semantics are not silently reinterpreted. |

`VerificationPlan` is **data in a bounded, typed grammar**, not native code, WASM, Python, an ISA
image or a programmable verifier VM. It names implemented primitive/constraint/checker ids and
dimensions, graph/state connections, commitments and budgets. The checker validates the plan's
coverage and composition; it never trusts a registrant-supplied assertion that a custom relation is
sound. An arbitrary bytecode interpreter disguised as `CustomOp` or a universal CPU circuit is not
an approved fallback. Abstract GKR circuit generality does not authorize any uploaded computation:
the accepted circuit construction must be tied to the kernel's defined tensor/state relations.

Producers may use any off-chain accelerator or orchestration tool that yields the same committed
semantics. Existing EVM contracts and the native EVM lane are not removed. Neither EVM nor a new
EVM job precompile may be used to smuggle a general model-verification VM back into this roadmap.

## 3. “SegWit-class” means a versioned protocol extension, not automatic soft-fork compatibility

The relevant precedents are versioned validation rules and committed witness data in
[BIP141](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki), and explicit future extension
points in [BIP342](https://github.com/bitcoin/bips/blob/master/bip-0342.mediawiki). They do not prove
that adding a MISAKA kernel is a soft fork. In particular, an old node skipping unknown work checks
cannot safely price, reward or order that work.

Default to a coordinated, fingerprinted protocol upgrade. A future proposal may call itself a soft
fork only after proving valid-history inclusion, state/reward/fork-choice compatibility and old/new
node behavior for that concrete change. Never adopt unknown-version/unknown-op success for rewarded
PALW claims. Unsupported versions are explicitly unsupported; an old node must stop participating
in post-activation validation/production until upgraded, not continue “validating” by ignoring them.

The extension sequence is proposal → reference + independent implementation → vectors/resource and
soundness review → shadow validation → coordinated LOCKED_IN schedule → ACTIVE. These are release
states, not a newly invented validator ballot or a current implementation. Readiness signalling alone
is not validation, and there is no selected activation DAA in this ADR.

## 4. Immutable meanings and registration

New-format class identity binds a kernel descriptor/version, semantic/constraint/court suite ids,
the immutable challenge policy and approved checker/challenge/soundness tuple, plan root and all existing
program/material/task/context commitments. Actual future beacon/transcript is later evidence, never an identity
input. A hash of code is not its
correctness proof and never authorizes the node to download/execute a module. Legacy class ids and
in-flight claim semantics stay unchanged; an existing class is not rehashed or upgraded in place.

For a new model:

1. Resolve source, rights, task/context and deterministic lowering independently of deployment.
2. If the active kernel expresses the **whole** operation, submit a bounded plan with coverage,
   integer/field rules, DA and worst-dispute envelopes. Registration needs no whole inference replay
   by the registrar/node. Source fidelity and operator readiness remain distinct checks.
3. If only a frontend/format adapter is missing, implement that off-chain adapter and conformance
   vectors. It changes no consensus semantics.
4. If a semantic, verifier, memory or court capability is absent, return `KERNEL_EXTENSION_REQUIRED`
   with the exact relation and limits. Propose a reusable extension, implement and activate it, then
   resubmit. No VM fallback and no pretend successful registration during the wait.

No user may select a weaker confidence level, omit a constraint or raise a node budget through plan
metadata. The full-check error target remains RFC11's **conditional proposed** `ε_check ≤ 2^-128`,
not a demonstrated network security level. Sparse errors must affect a covered algebraic/encoded
relation. Bind evidence before challenges; include multi-round adaptivity, grinding, DA and Panel
compromise in their separate analyses. Exact terminal court remains a backstop for **detected** or
challenged faults, not a mechanism that retroactively removes undetected-error probability.

## 5. Supersession and implementation boundaries

| Document / component | Decision |
| --- | --- |
| RFC05 old Parts I/II, ladder TIR → BVM/GVM, Linux/ISA fallback and EXEC VM programme | Withdrawn implementation directions; retained only as historical rationale. RFC05's new current section defines kernel-only extension. Do not implement or arm the old VM proposals. |
| RFC04 improvement, candidate/evaluation/promotion rules | Keep. Off-chain automation is unrestricted. Code-by-tests/EXEC is not promised via a future VM; a distinct reviewed non-VM checker extension would need its own specification. |
| RFC11 model registration and broad coverage | Replace all proposed VM fallback with active-kernel plan composition or an explicitly required future kernel. Preserve the real 9B/2M blockers and the all-HF denominator. |
| ADR0171 / RFC07 probabilistic verification | Unchanged security direction; reusable kernel-primitive templates replace references to VM templates. Full replay remains a conformance baseline or exact small-class option, not the required ordinary large-model path. |
| RFC02's residual-GVM route and RFC03's VM Tests reference | Withdrawn; RFC02 §§II.11–II.12 now specify active-kernel registration and an explicit extension queue. RFC03's future test relation needs a bounded non-VM kernel/task profile. Missing support stays uncovered until the full route passes. |
| Existing live EVM and any historical VM-coded objects | No change by documentation. Inventory actual activation/code before cleanup; retain required historical replay. Dormant prototypes need no new implementation or activation under this roadmap. |

The [RFC01–12 consistency audit](../rfc/evidence/0012-kernel-only-design-audit.md) records the
2026-10-06 cross-document dispositions. RFC01/03/06 distinguish legacy replay from the new checker;
RFC07/08/09/10 bind kernel/checker identities without reviving an interpreter. RFC12 removes DNS
authority, not the existing native EVM lane. RFC11 §15.9 specifies the research/benchmark basis
for probabilistic checks and the boundary between economic audits and cryptographic soundness.

## 6. Acceptance evidence and honest coverage

* Independent reference/optimized executor agreement, plan coverage checks and falsified-constraint
  tests across dense, MoE, recurrent, lookup, quantization and authenticated-state cases.
* A valid large claim reaches Final with the proposed small-check suite and no routine segment
  replay; a single bad scalar/routing choice/memory write is covered by the **reviewed** error bound.
  Missing evidence, weak suite ids, forged circuit bindings and absent families cannot finalize.
* A failure localizes within the specified worst-case court bytes/work/time. Compiler and verifier
  bugs and Panel collusion are not excused by probabilistic soundness of an ideal relation.
* Mixed old/new kernel classes, long-running claims across activation, reorg/IBD/pruning and unsupported
  peers have tested behavior. Cross-kernel pipelines require an explicit bounded composition profile.
* Report actual 9B-8k, validated long-context and full-modality large-model registration/Final results,
  producer/proof/verifier/DA/court cost separately. No benchmark or registration is performed by this ADR.
* Preserve RFC11's pinned all-HF `D_all` denominator, rights ceiling and one-sided confidence requirement.
  The attachment's 90–95% / 95–99% projections are **not measured coverage** and are not adopted as a
  guarantee. Without a VM, residual models may require an upgrade; count them as failures until their
  complete route is implemented, active and supported by the required registration evidence.

**Outcome:** no VM implementation programme for model extensibility; permissionless plans within
implemented kernels, explicit protocol upgrades outside them, and unchanged probabilistic-normal /
exact-dispute verification. This decision neither implements nor activates any of those extensions.

## 7. Beacon-backed Kernel/model conformance, after completeness (2026-10-08)

`KernelDescriptorV1.challenge_policy_id` resolves [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
PostCommitChallengePolicyV1 digest. RFC07 alone specifies sources, seed/sampling, transcript transforms and recovery;
other workstreams must not implement independent formulas. Approve checker/challenge/soundness policies together.

Before Kernel differential challenges, establish semantics completeness, full constraint coverage, exact court,
public-material/prosecution completeness and bounded resources. Then fix reference/checker/court/independent/
optimized implementation revisions and test scope, commit, wait for independently valid future PALW work, and run
canonical randomized vectors alongside required deterministic/adversarial tests. Agreement reduces fixture bias;
it proves neither Kernel semantics nor soundness. New unknown operations still require coordinated implementation,
review/shadow evidence and explicit Kernel activation. A beacon or Panel vote cannot authorize them.

Model onboarding follows RFC11 §17: Static Admission / RegisteredDormant, Beacon Conformance, then G14/public
availability/resource/actual chain Active Eligibility. RFC13 carries commitments, source/lock/transcript/results,
scoped error and resumable state. Changing implementations/artifact/layout/plan invalidates dependent evidence and
requires a fresh pre-beacon commitment with counted retries; actual beacon/query data does not rehash the class.

Use RFC07's candidate-independent qualifying Final useful work. Committee/DNS/BFT signatures, heartbeat/BASE-0,
round/EXEC headers and self-testing candidates supply no substitute entropy. Source absence waits without changing
main liveness. Bootstrap, source cost/unpredictability, last-contributor/withholding bias, staged GKR/Fiat–Shamir
security, retries/reorg and fresh-public-verifier exact escalation remain unpassed gates. This documentation adds
no runtime, source approval privilege, wire id, fingerprint or activation height.

## Mission alignment amendment — 2026-10-07

kernel-extension acceptanceとregistrationにpublic-verifier dispute completenessを追加する。approved bounded kernel courtが存在するだけでnew profileをmineableにしない。VM/TEE/BFT authorityなしの選択は維持し、missing public localization/openingをmissing kernel/profile capabilityとして閉じる。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
