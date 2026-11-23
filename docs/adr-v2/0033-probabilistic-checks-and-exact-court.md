# ADR-v2-0033 — 確率的constraint検査とexact courtの分担

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — 二経路モデル採用 MAD2

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](../adr/0235-model-admission-kernel-auto-and-bft-decision.md)／[適用・移行](../governance/model-admission-dual-route-transition.md)を、MAD2対象のモデル採用について優先する。KERNEL_AUTOは対応kernelへの自動分解と採択済み審査、BFT_DECISIONは証明提出を要求しない所定quorumのYES/NOである。

G-COLLの定理・G-COST/G-USEの証拠が未完成でも、共通の技術互換とq YESがあればBFT承認を記録できる。元のUNKNOWN/FAILを数学的PASSへ改変しない。未知kernel、壊れた実行仕様、個別claimの不正はYESで免除しない。Panel/Court、Final、独立Gate C、Ticket、一回会計、TB1、MOE2は維持する。

以下の旧「UNKNOWNは一律採掘不可」「BFTは停止専用」「全モデルに定理・費用審査を必須」とする条項は、旧profileの履歴及びMAD2のKERNEL_AUTO側として読む。BFT_DECISIONへ再適用しない。実投票・native実装・公開activationの完了を意味せず、既存QF1のREVIEW_HOLDを文書だけで解除しない。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

<!-- MAD2-ADOPTED-20261123-END -->


<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../governance/qwen-fow1-profile-transition.md)。QF1のReadout連鎖と算術反証を区別し、一人の有効反証と資料gateを優先する。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用定理・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。局所Courtの正当性検査とG-COSTを区別する。未知の安全性をBFT賛成で承認しない。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 1本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0006](../rfc-v2/0006-panels-and-probabilistic-verification.md) / [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

大規模modelの通常検査と、異議が出たときのexact courtを異なる役割として固定する。

## 2. 統合した判断の読み方

### 2.1

通常時は承認されたconstraint検査方式を使い、異議を局所化した後の裁定は決定論的なexact計算で行うという方向を記録する。[旧ADR-0171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md)

### 2.2

誤差確率・乱数・commit順序・対象constraintの条件を省略して、少数の抜き取りや署名だけを完全検証と呼ばない。[旧ADR-0171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md)

### 2.3

新しい正常検査方式の方向承認は、protocol仕様・実装・測定・network有効化をまとめて承認したことではない。[旧ADR-0171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md)

## 3. 維持する境界・後続改定

原資料のStatusはdesign direction selectedであり、当該記録時点では仕様化／実装／有効化が別。G14がモデル未取得を解消するという追加保証は入れない。

## 4. 隣接する判断との分担

Panel抽選は[ADR-v2-0032](0032-sampling-beacon-and-receipts.md)、TIR算術は[ADR-v2-0013](0013-tir-arithmetic-and-determinism.md)、裁定の前提は[ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0171-probabilistic-constraint-checks-and-court-on-dispute.md](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md) | L1–L127 | L15–L15 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0171-probabilistic-constraint-checks-and-court-on-dispute"></a>

<details>
<summary>0171-probabilistic-constraint-checks-and-court-on-dispute.md — 原文 L1–L127</summary>

[原文を開く](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md) · SHA-256: `e2bdc16e7574aa39365f9e326ca3e61ba3694c109adbb261e12f216b50bfe462`

<!-- ADR_SOURCE_BEGIN:0171-probabilistic-constraint-checks-and-court-on-dispute.md -->
~~~~~~~~markdown
# ADR-0171 — Probabilistic constraint checks are the normal large-model verifier; the court resolves disputes exactly

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


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

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

PALWの前提を確率的constraint検査・公開exact courtとbond/DAAによる共通採掘上限のANDに拡張する。モデル計算は経済枠を使うための正当な仕事であり、計算省略は追加枠を生まない。
一claimのcreditを小さくしてclaim容量を増やしても、全constraint coverage・post-commit binding・soundness compositionを弱めない。検出確率、実監視、徴収可能net損失と節約費用を別々に評価する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0171-probabilistic-constraint-checks-and-court-on-dispute.md -->

</details>

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

局所spot checkと全体承認を分離する。LLM validity proofを導入せず、コア関数の定理を別に要求する。

[TB1の正本](../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](../governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->
