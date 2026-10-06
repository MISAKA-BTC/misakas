# ADR-0171 — Probabilistic constraint checks are the normal large-model verifier; the court resolves disputes exactly

**Status:** Design direction selected at the operator's request, 2026-10-06. Protocol specification/implementation pending; **not active on any network**. No activation height, implementation success or fingerprint change is implied by this document.

> **2026-10-06 extension-mechanism amendment:** [ADR-0172](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) selects versioned kernels and withdraws the VM implementation/fallback programme. The former primitive/VM-template option is withdrawn: new plans use approved kernel-primitive templates only. This changes the extensibility mechanism, not this ADR's probabilistic verification, error accounting or exact-dispute policy.

**Specifies:** [RFC-0011 §§0, 13 and 15](../rfc/0011-permissionless-model-and-long-context-onboarding.md). Builds on [RFC-0007](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)'s algebraic checker and exact escalation, RFC-0006's bounded partitions and RFC-0008's unique work accounting. Existing consensus and old claims keep their existing rules until a separate fence activates a complete implementation.

## Decision in Japanese

MISAKAは、Kimi K3級の巨大モデルまで対象にする通常検証の設計方針として、**確率的なconstraint検査を採用する**。minerが計算と証拠生成を行い、結果・state・weight・constraintをcommitした後にchallengeを決める。Panelはsegmentを丸ごと再実行する代わりに、Freivalds、GKR/sum-check、range/lookupなどで計算の制約を検査する。必要な検査と正のreceiptが揃い、challenge windowが閉じたら、明示した見逃し確率を受け入れてFinalとする。不一致は局所化し、既存の互換courtでterminal stepを正確に裁定する。

これはMISAKAが採る拡張方針であり、「Kimi K3はsampling以外では数学的に検証できない」という主張ではない。生のtraceから数点を選ぶだけでは単一点の不正をほぼ見逃す。**全constraintを検査対象の関係式・集約に含めること**と、**全計算を再実行すること**を区別する。計算全体の正しさは確率的に判定し、検査器・メモリ・裁定の資源上限は決定的に守る。courtがあるだけで未検出の不正が消えるとは扱わない。

## 1. Why this decision

RFC11 originally preferred a model-specific deterministic admission certificate covering every reachable commit point. It already allowed offloaded work and did not require every validator to replay inference. The revision therefore changes the preferred admission representation and normal claim-verification policy; it is not evidence that the prior RFC necessarily replayed every job on chain.

The official [Kimi K3 card](https://huggingface.co/moonshotai/Kimi-K3), consulted 2026-10-06, describes a 2.8T-parameter multimodal model with a million-token context. Supporting this target requires separating producer-scale computation/storage from verifier-scale work. Requiring each verifier to reproduce all inference is not the selected scaling strategy. Small exact replay and other sound future protocols remain possible, but the large-model route must implement and benchmark probabilistic constraint checks before claiming Kimi K3 support.

RFC7 already proposes Freivalds-style checks. Its §II.9 reports bandwidth-sensitive estimates and cases where replay is faster, not a Kimi K3 benchmark. The new choice needs measured total costs, including proof construction and cold preprocessing, rather than an assumed universal speedup.

## 2. Rules selected

1. **Register a verification plan.** `VerificationPlanV1` binds model/program/task/context and arithmetic, all constraint families, approved checker versions, soundness parameters, authenticated material and bounded dispute procedures. Reuse approved kernel-primitive templates and compositional envelopes, never a VM fallback; no exhaustive unrolling of future executions at registration. Dynamic routing/memory must be covered by an applicable verifier rule. Missing support requires the ADR0172 kernel-extension path.
2. **Use randomized algebraic checks in ordinary verification.** Freivalds handles appropriate matrix relations; GKR/sum-check can aggregate compatible circuits. Quantization, nonlinear operations, routing, memory and segment boundaries need their own exact or probabilistic constraints. Their costs and combined soundness count toward the same claim. Ordinary Panel verification does not require complete segment replay.
3. **Commit before each relevant challenge.** Fix outputs and evidence before deriving unpredictable challenges. Interactive proof messages must also precede their round's challenge. Domain separation, bias, grinding, aborts, reorg and any Fiat–Shamir assumptions require an explicit construction. A fixed public vector before execution is prohibited.
4. **Accept quantified risk.** A false whole claim may pass with the reviewed nonzero error bound. RFC11 proposes a conditional `ε_check ≤ 2^-128` target; field size or sample count alone does not establish it. Panel compromise, challenge bias, DA/censorship and repeated attempts are separately budgeted. A signature quorum is not an arithmetic proof and shared challenges are not independent repetitions.
5. **Require positive evidence for Final.** Complete required checks/receipts, DA obligations, elapsed challenge window and no active dispute precede Final and reward release. Missing evidence and silent seats are not success. Node consensus replays deterministic signatures, commitments and lifecycle; it does not rely on nodes making fresh random choices. A future node-verified compact proof has its own metered acceptance rule.
6. **Escalate failures to exact court.** A mismatch with served evidence is a reason to investigate, not sufficient slashing evidence. The plan must localize it to committed inputs and a bounded terminal step; unsupported operations require an explicit court extension. Missing material follows the availability rules. Neither a full giant-model replay nor an unbounded number of simultaneous disputes may be a hidden fallback.
7. **Keep work and safety accounting.** Preserve initial/final state binding, full context, chronological memory, disjoint slice credit, exposure limits, resource caps and receipt maturity. Any pre-Final fork-choice influence must have a tested aggregate bound. Registration, mineability and market creation remain distinct.

## 3. What sampling means here

For one bad segment out of `N`, inspecting `s` uniformly selected distinct segments detects it with probability `s/N`. At `N=10^8`, `s=100`, almost every such lie is missed. A perfect checker inside those segments cannot improve the chance of selecting the bad one. This observation from the existing security analysis continues to hold.

This ADR instead selects randomized checks that cover the committed relation, including sparse errors. For example, Freivalds projects a fixed matrix-product error onto a fresh random vector; GKR/sum-check reduces a circuit statement through randomized algebra. An encoded-query route must bind constraints and boundaries to the encoding with its own soundness theorem. FRI by itself checks proximity; DA sampling by itself checks availability. Neither proves a raw execution trace correct.

The court handles disputes that are actually opened. It leaves the fast path's undetected-error probability in place. Accepting that residual risk is part of this decision, not a claim of deterministic whole-execution correctness.

## 4. Compatibility and supersession

| Existing design | Effect of this ADR after a future fence |
| --- | --- |
| RFC11's previous §13.1 exhaustive model-specific static proof plan | Replaced as the preferred route by verification-plan admission and reusable bounded verifier/court templates. Existing exact admission remains usable. |
| RFC7 Part II private sketches and exact failure escalation | Remain a candidate implementation with their own secrecy/refresh assumptions. Public post-commit checks and GKR need separate reviewed bindings; no automatic security or preprocessing-cost equivalence. |
| RFC7 Parts III/IV and ADR-0098 warnings about raw interval sampling | Retained. Cheap raw spot checks are not a full-security work-verification policy. Constraint coverage may be algebraic instead of full execution replay. |
| Existing terminal court, DA and unavailable-seat rules | Reused where compatible. New primitive/proof-localization routes are specified, tested and fenced; a failed randomized test is not itself an exact terminal conviction. |
| Existing licensing, finality and fork-choice rules | Unchanged today. New probabilistic receipts and any maturity/weight changes require explicit versioned transitions and replay tests. |

The new proposed fence is `palw_probabilistic_constraints_v1`; the name is a proposal, not a field added by this documentation. No existing DAA-5,300 fence activates it. Old bound claims complete under old semantics. Plan/suite identity, transcript and signature domain bindings, evidence retention, IBD and reorg behavior are prerequisites to choosing an activation height.

## 5. Required evidence before activation

* Reviewed whole-claim soundness composition, including single-point faults, integer/field equivalence, adaptive challenges and compromised/idle Panel assumptions; production security is not established by a finite fault-injection test.
* Independent reference comparison and adversarial tests for output, wrong-expert routing, rounding, memory, state continuity, forged projection openings, omitted constraints and reused proof challenges.
* Valid claims finalize with the new checks and no routine segment replay; disputed claims localize within bounded total work/bytes/deadlines and receive the correct exact terminal result. DA failure, no quorum and reorg cases cannot bypass the requirements.
* Real 9B-8k and validated 2M cases, followed by a source-pinned Kimi K3 feasibility report covering its claimed modality/context. Report producer execution, proof/witness generation, preprocessing, verifier computation, material transfer and worst-case dispute load separately. Kimi support stays unproven until that report passes; HF 90% stays a separate empirical target.
* A dormant implementation, shadow comparison, independent review and an explicit network activation decision. This documentation authorizes no deployment or model-registration transaction.

## 6. Primary basis

The operator's clarified implementation direction is [RFC11 §15.9](../rfc/0011-permissionless-model-and-long-context-onboarding.md):
compare PoSP/spML economics and opML disputes first; implement and benchmark a Freivalds/Slalom-inspired
matrix checker before broader aggregation. Use DAS for availability, and GKR/SafetyNets/FRI only as
reviewed components of a fully bound computation relation. This borrows neither a new orchestrator/
DNS-validator set, Slalom's TEE trust nor opML's VM. Economic audit probability is distinct from
algebraic challenge randomness: optional audits do not waive mandatory positive checks before Final.
TEE/enclave trust for validity, model/fraud-proof VMs and BFT orchestrator/PKI/committee-beacon
authority are explicitly **excluded**, not merely architectures to avoid copying unchanged.
Source-pinned prefill, decode and MoE measurements, full-statement soundness and bounded kernel court
are required; expected-loss estimates do not replace a protocol-specific incentive proof.

[Slalom](https://arxiv.org/abs/1806.03287) motivates cheaper checks for linear layers; [GKR](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/) and [SafetyNets](https://arxiv.org/abs/1706.10268) motivate probabilistic circuit verification. [FRI](https://eccc.weizmann.ac.il/report/2017/134/) supplies a possible proximity component, and [Celestia DAS](https://docs.celestia.org/learn/celestia-101/data-availability/) concerns availability. [PoSP](https://arxiv.org/html/2405.00295v3) studies economic sampling/recomputation; [opML](https://arxiv.org/abs/2401.17555) supplies interactive ML-dispute precedent. These are component-level precedents; none is an implementation or performance proof for this combined MISAKA design. The detailed obligations and cost limits are in RFC11 §15.
