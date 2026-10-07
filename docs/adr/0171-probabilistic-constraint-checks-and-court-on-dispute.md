# ADR-0171 — Probabilistic constraint checks are the normal large-model verifier; the court resolves disputes exactly

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


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
3. **Commit before each relevant challenge.** [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) is the sole proposed construction for Kernel/model/claim/slice/public checks. Bind the immutable challenge policy and complete statement before its qualifying future PALW work; every interactive message precedes its own policy-pinned staged/Fiat–Shamir challenge. No local seed formula, free-header entropy, committee beacon, timeout fallback or fixed public vector before execution. Source independence, bias, grinding, abort/retry and reorg remain explicit activation gates.
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

## 7. Common challenge protocol and onboarding stages (2026-10-08)

RFC07 Part VI owns source eligibility, canonical collection/lock, seed derivation, domain-separated sampling,
per-message interactive timing and deterministic recovery. Kernel/model registrants, Panels, outsiders and full
nodes replay those same bindings. Final useful-work correctness and k-item aggregation do not by themselves prove
source unpredictability/unbiasedness; independent validity, bootstrap/non-circularity, precomputation, withholding,
adaptive last-contributor and grinding/reorg analysis are mandatory. Source scarcity is pending, not permission to
use heartbeat/BASE-0/EXEC hashes, local randomness or validator/committee authority; chain liveness is unchanged.

RFC11 §17 separates Static Admission, Beacon Conformance and Active Eligibility. Kernel testing fixes semantics,
constraint/court/public-material/resource completeness first, then commits implementation revisions and uses RFC07
differential challenges. Model conformance has a reviewed scope/fault-model epsilon and does not authorize unknown
ops, replace claim verification or satisfy G14 alone. RegisteredDormant may return before beacon availability.
RFC13 records reproducible commitment/source/transcript/results and resume/abort status; RFC14 adds fresh-outsider
challenge replay and exact escalation to G14. A failed randomized test remains distinct from an objective conviction.

Checker/challenge/soundness ids are an immutable approved tuple, bound by the new Kernel/class/plan. Actual source
anchors and transcripts are later evidence, not class-identity inputs. Existing ids, tickets, Panel draws and live
claims keep their old rules. Revised RFC8 EXEC slices add zero fork-choice/DAA before and after Final, settle once
at root Final and never obtain extra weight/source count from more carriers. All new protocol/activation evidence
is pending; this amendment changes no runtime/fingerprint/height and does not enable Panel=0.

## Mission alignment amendment — 2026-10-07

新しいVerificationPlanのnormal checksとexact escalationを、普通のpublic bondが公開materialだけで実行できるよう拘束する。private sketchやselected Panelの検査は公開訴追経路の代替ではない。ε_checkは条件付き目標のままであり、courtが未検出確率をゼロにするという保証を加えない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
