# RFC-0011: Permissionless model onboarding with probabilistic constraint verification and court on dispute

## 2026-10-09 改定 — 登録IDとモデル内容の永久不変binding

[ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)を適用する。permissionlessな新規登録は継続し、weights、graph、tokenizer、実行仕様、canonical artifact root、kernel/verification planを登録IDに固定する。登録後の更新要求、同一登録へのV2/V3、overwrite、head置換を拒否する。改善版・異なる実行仕様は新規IDと独立した検証・報酬資格を持つ。計算graphの識別とmodel registration IDを混同しない。既存class/hash/wire IDは再採番せず、追加modelの登録IDにrootを含める。

新kernel・planの有効化は新登録のadmissionを可能にするもので、既存モデルのbindingを置換しない。実際のbeacon、challenge transcript、conformance証拠、利用量や報酬資格は運用・証拠状態であり、固定された承認bindingを変更しない。登録の自由と配布・rewardabilityの要件は分離する。

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: Revised Draft, 2026-10-08. **Sampling-first / court-on-dispute** remains the ADR-0171 direction; §17 adds Static Admission → Beacon Conformance → Active Eligibility using RFC07's single challenge protocol. Design and acceptance criteria only; no runtime, testnet-12 rule, activation height or fingerprint changes by merging this text.
* Source audit: repository commit `27670cd2b` (includes the DAA 5,300 release). Source-proven limits below describe this commit, not a fresh measurement of every deployed node. Prior 9B run results, current code facts, and proposed fixes are labelled separately; no successful 9B registration or 2M run is claimed.
* Scope: PALW-TIR model conversion, class admission, long-context claims, probabilistic constraint checks, data availability, court, and readiness. The primary coverage target is **at least 90% of all public Hugging Face model repositories at a pinned snapshot, registered as their complete advertised task and context**. Registration must be independent of the registrant's VPS size. Execution semantics and terminal adjudication remain exact; normal verification accepts a quantified, nonzero soundness error.
* Related: [RFC-0002](0002-palw-tensor-ir.md), [RFC-0006](0006-palw-layer-sharded-panels.md), [RFC-0007](0007-palw-verification-certificates-and-algebraic-checks.md), [ADR-0082](../adr/0082-the-close-is-flat-in-the-context.md), [ADR-0103](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md), [ADR-0135](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md).

## 0. Decision requested

**Use probabilistic constraint verification as the normal acceptance path: a miner executes once, commits its result and evidence, then verifiers use Freivalds/GKR-style checks under post-commit challenges. A passing claim may become Final after its challenge window without full execution or segment replay by the Panel. On disagreement, localize the disputed computation and use the existing deterministic terminal court where compatible.** A passing transcript is correctness at the declared error bound, not a proof with zero error. Raw trace spot checks alone do not meet this decision (§15).

Registration checks a bounded `VerificationPlanV1`: semantic bindings, coverage of constraint families, probabilistic soundness parameters, compositional resource limits and a bounded dispute route. It does not enumerate the entire future execution or prove every execution value correct in advance. Heavy conversion, calibration, witness construction and proof generation may be supplied by untrusted workers. A class can be listed before it is mineable; useful-work weight requires the armed verification, DA and capacity rules.

**Revision boundary.** This replaces §13.1's former requirement for a model-specific, exhaustive deterministic admission proof as the preferred route. Small existing classes may retain exact sizing/replay. The old 9B/2M failures and coverage audit remain historical evidence. Probabilistic execution acceptance does not permit probabilistic parser/memory safety: every permitted query and terminal dispute must still fit enforced bounds, established through reusable approved kernel-primitive rules and composition. An unknown dynamic access can use an authenticated-memory constraint and bounded opening; it cannot be omitted from verification. §15 defines the new route; where earlier release gates mention exact bounds they refer to resource safety and terminal semantics, not full normal-path replay.

**Kernel-only revision, 2026-10-06:** [ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) and [RFC05 §§K.0–K.8](0005-palw-ml-vm.md) withdraw the BVM/GVM fallback. Model extensibility uses declarative plans within active kernels or a coordinated, versioned kernel extension when semantics/checkers/courts are missing (§16). The probabilistic, encoded-constraint acceptance policy in §15 is unchanged. This is not a claim that all finite models fit today's kernels or that a SegWit-style extension is automatically a soft fork.

**The coverage claim this RFC is intended to enable is not “90% of decoder fixtures” or “90% of local LLMs”; it is `registered_full_task / D_all ≥ 0.90` for a dated, reproducible census of all public Hugging Face model repositories (§7).** `registered_full_task` requires an accepted on-chain class registration under the target rules, or independently reproduced equivalence to an already registered class (§11.2), not a passing config parser, a draft artifact, a catalogue entry, a short-context substitute or a model-market line. This proposal does not assert that today's implementation meets that threshold; it defines the evidence that must exist before anyone may say it does.

“Any model, any context if resources are supplied” is a *directional interoperability goal*, not an assertion that arbitrary future code or an infinite context can safely run under today's TIR. Every admitted class still needs deterministic TIR semantics (or a separately reviewed, versioned primitive set), a valid lowering, a finite resource envelope and an answerable challenge. Enough independently qualified execution/verification capacity is a further condition for mining, not a requirement to start a Panel before zero-share registration. Unsupported architecture, unbounded work, and unavailable evidence have distinct, machine-readable outcomes. Neither changing `max_context` to 512 nor bypassing a court/DA bound counts as success.

## 1. Evidence: why the Huihui Qwen3.5-9B registration stopped

The attempted source was `huihui-ai/Huihui-Qwen3.5-9B-abliterated`, revision `05b9e7c9b978ba29bdb8f50a49c30e4b91183339`. A fidelity artifact was built and retained locally; no class-registration transaction was signed or broadcast, and no model line was created. The following are separate gates, not one generic “CPU shortage”:

| Stage | Observed or code-proven result | Meaning |
| --- | --- | --- |
| Source → fidelity artifact | The 9B source and 512-position calibration produced a PALW-TIR artifact. Full pack conformance was costly on CPU; an independent `ref2` run was skipped at this artifact size. | Source conversion is possible; pack verification time is **not** evidence that consensus rejected the class. |
| Artifact metadata | The streamed `palw-tir-convert`/pack path checks recurrent calibration length when `--calib` is used, but its written metadata omits `calibrated_context`; `palw-tir-fidelity` writes it. `declare-layout` refuses a recurrent artifact that cannot show context coverage. | A tooling/provenance mismatch, independent of hardware. See [`convert.rs`](../../misaka-palw-tir-lower/src/convert.rs) and [`tir_layout.rs`](../../misaka-palw-sdk/src/tir_layout.rs). |
| First 512-position layout | Default tile produced `COURT_COST_EXCEEDS_CEILING`: 16,842,752 tile MACs against 16,777,216 allowed. | A tile-shape constraint; smaller legal tiles get past this check. It is not an invitation to raise the court ceiling blindly. |
| Reduced-tile layouts | Contexts 512, 256, 128, 32 and 16 still returned `TIR_EXCEEDS_CEILING`: IR close-sizing work **67,108,865** against **67,108,864**. The t12 node's live preflight at DAA 5,381 also refused the tested 256 profile. | This is the decisive observed registration blocker. **67,108,865 is `cap + 1`, a refusal sentinel, not the measured exact work.** The range twin introduced by `palw_tir_fence2` is already available on t12 after DAA 3,600; it must be profiled rather than proposed as an unactivated fix. See [`palw_tir_admission_v1.rs`](../../consensus/core/src/palw_tir_admission_v1.rs) and [`palw_tir_close_size_v1.rs`](../../consensus/core/src/palw_tir_close_size_v1.rs). |
| Seat feasibility | The prior preflight reported a working-set shortage (`SEAT_MEMORY_SHORT`). Its default comparison uses fixed t12 tier estimates of 3.5 and 8 GiB, not an automatic measurement of live seats. | This is a **mine/readiness** blocker, not the registration refusal. Preserve exact needed bytes and tier provenance in the next report; adding genuinely capable seats or safe layer shards addresses it. An arbitrary `--seat-share` does not prove live capacity. See [`preflight/chain.rs`](../../misaka-palw-sdk/src/preflight/chain.rs). |

These measurements are one 9B artifact and particular layouts, not a proof that *all* Qwen3.5-9B variants or all contexts fail. The exact cap-crossing commit point, range-twin step count, terminal close bytes, and candidate layout's other gates remain to be measured. The former 5,102-DAA ladder-clock report is not established as the blocker for this artifact: t12's held clock and its manifest verifier must be checked against the same live ruleset before making that claim.

**Scope of this source matters.** The pinned upstream [config](https://huggingface.co/huihui-ai/Huihui-Qwen3.5-9B-abliterated/blob/05b9e7c9b978ba29bdb8f50a49c30e4b91183339/config.json) declares `Qwen3_5ForConditionalGeneration`, a vision configuration, and `text_config.max_position_embeddings = 262144`. An 8k text-only profile is a useful requested deployment, but is **not** full-task/full-context coverage of this repository for §7. A 2M extension also needs a versioned position/attention policy and fidelity evidence: raising a context integer does not give the upstream model a validated 2M capability. Track the requested 9B-8k case, full-source coverage, and a validated 2M profile as separate acceptance cases.

## 2. Why 2M is more than a CPU problem

1. **Calibration cost.** For a recurrent program with `Fixed` state, [`check_calibration_length`](../../misaka-palw-tir-lower/src/fidelity.rs) requires at least one sequence as long as the declared context. A 2,097,152-position declaration therefore asks for 2,097,152 sequentially dependent positions under today's rule: 256 times the token count of 8,192, or 4,096 times 512. This is *not* a measured 256× elapsed-time prediction; attention, paging, batching and state size can change scaling. GDN/Mamba state writes cannot simply be distributed as independent positions. The short-calibration waiver exists but is not a general safety-preserving solution; its long-context fidelity must be demonstrated.
2. **Three independent format gates block 2M, before hardware is considered.** [`palw_tir_v1.rs`](../../consensus/core/src/palw_tir_v1.rs)'s t12 ceilings set `max_context = 2^18 = 262144`. [`program.rs`](../../misaka-palw-tir/src/program.rs) permits only the small `2^18` and held `2^21` history declarations, but [`verify_class_admission_v10`](../../consensus/core/src/palw_tir_admission_v1.rs) explicitly rejects the held declaration because its accusation/answer route is still legacy-binding-shaped. Independently, the canonical prefill is `floor(max_context / 8) - 1` and must fit J5b's 4,096 inline token IDs; this limits admitted layouts to 32,783 positions. At 2,097,152 positions the canonical prefill would be 262,143 IDs. Fixing **only** prompt attribution, **only** the network context ceiling, or **only** held admission leaves the others in place. A new complete IR context/court route and its armed ruleset are required.
3. **A full-context proof is not a full-context replay.** A root over held state can keep *carried bytes* small, but a producer must still do the work, seats must be able to inspect disputed segments, and the court must have bounded bytes, computation and time. The close-sizing `2^26` step cap is a validator DoS guard, not a parameter estimating the model's inference FLOPs. If the sizing algorithm traverses too much, moving the same traversal to every validator with a larger cap is not a solution. Bound-preserving algebraic/range proofs or succinct certificates need exact differential tests and a court path.
4. **Memory, DA and seats are separate.** Long histories and large weights can exceed a seat's published memory share even after admission is fast. The existing preflight estimates `artifact + state(context) + peak live + widest opened tile`. A class without capable seats can be registered/listed only if the protocol explicitly leaves it non-mineable; it must not claim ready status or earn for unverifiable work.

Therefore “CPU不足” should be reported as one of `LOCAL_COMPUTE_EXHAUSTED`, `CALIBRATION_LENGTH_UNPROVEN`, `ADMISSION_WORK_CAP`, `CANONICAL_PROMPT_UNATTRIBUTABLE`, `COURT_BYTES`, `COURT_WINDOW`, `DA_UNANSWERABLE`, or `SEAT_MEMORY_SHORT`, with stage, actual measured value (or a **lower bound** when censored), limit and active fence. A 2M benchmark has not been run in this investigation; exact runtime and RAM requirements must be measured before capacity claims.

## 3. Proposed interface and trust separation

```text
source checkpoint + tokenizer + frontend/adapter version
       │
       ├── untrusted calibration worker → CalibrationEvidenceV1
       ├── untrusted streamed converter → content-addressed TIR artifact
       └── untrusted conformance worker → independently reproducible results
                                              │
                                              ▼
  bounded VerificationPlan admission → Registered / Dormant / Certified / Mineable
                                              │
  producer execution → committed trace / constraints / material
                                              │
  post-commit challenges → probabilistic checks → challenge window → Final
                                   │ disagreement / missing evidence
                                   └→ localization / DA route → exact terminal court
```

`CalibrationEvidenceV1` MUST bind the source root/revision, tokenizer and frontend/adapter spec digests, deterministic corpus/token-sequence root and length, claimed context, statistics root, quantization policy, math and converter versions. A signed worker statement can locate or attest to evidence but is **not** a substitute for reproducing it. Stats accepted through `--stats-in` MUST carry the same context-coverage record as `--calib`; neither path may silently omit it. Static registration may commit to unverified provenance while clearly withholding source-fidelity certification; the evidence must be verified before counting it as source-equivalent coverage or granting the corresponding certification. Do not impose full calibration replay on every registering client or validator in the name of checking metadata.

The protocol MUST distinguish:

* **Published artifact / listed line:** hash-identified, discoverable, no claim that its model is ready or useful work is being rewarded.
* **Registered TIR class:** deterministic semantics and a validated verification plan for the declared envelope, with coverage, soundness, court and DA budgets; initially zero useful-work weight where current rules allow that. No execution has been certified by registration alone.
* **Certified and mineable:** independent conformance, material availability, sufficient capable bonded seats, deadlines and end-to-end challenge drills passed. The existing `ClassLaneCertified`/activation semantics govern rewards; this RFC does not grant weight to a merely listed class.

Tool eligibility is separate from consensus admission. Today's CLI requires artifact and conformance PASS in its runtime-pack gate **before submission**; a size-based independent-conformance SKIPPED can therefore stop local registration even when live class preflight passes. Consensus itself does not enforce that pack gate. [RFC13](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) specifies exact layout handoff, target-ruleset provenance and large-artifact independent checking, using the separate Llama 3B attempt as evidence.

If the current transaction format already requires full admissibility for `ClassRegisteredTirV1`, a lighter *artifact publication* must be a separate, explicitly non-mineable object or off-chain content address—not an unsafe weakening of `ClassRegisteredTirV1`. A model market/position is also a separate economic object: a line appearing in `#/lines` does not itself prove that the class is mineable.

## 4. Admission and long-context changes

### A. Make today's 9B failure explainable and fixable without raising limits

Instrument the close sizer to return `cap_exceeded_at` (commit point, counted work, exact count only when cheaply obtainable, lower bound otherwise), twin, largest read-set category, and close-byte estimate. Profile the 9B `16, 128, 512, 8192` layouts under the **actual** t12 post-fence parameters. Compare the element and range twins' bounds and work; if both exceed the cap, identify which loop scales with program size rather than context. For the legacy route, improve exact sizing inside its budget; for the new route, use §13.1's compositional verification plan and bounded terminal templates. The new route need not first solve exhaustive legacy enumeration, but must establish its own resource bounds and accepted class transition under an armed fence. Differential-test terminal bounds against uncapped small cases and mutation cases. Do not raise `2^26`, tile MACs, close bytes or court windows until adversarial validator CPU and carrier tests justify a separately fenced rule.

### B. Open a safe path above 32,783 positions

Add a versioned, Merkle-attributed/tiled prompt-ID commitment to **IR canonical jobs**, with domain-separated root, token count, tile index and paths bound into job/claim identity. Define a bounded opening and challenge for every segment; include reorg/DA behavior and a worst-case prompt-ID proof size. This must replace—not merely bypass—the J5b inline-attribution assumption and have a distinct consensus fence and fingerprint. A 2M class may use a bounded canonical work segment plus committed predecessor state, but only after proving that claim splitting cannot duplicate work credit, alter ticket odds, or launder unverified history. RFC-0008's work-slice blocks and RFC-0006's layer/position cells are related designs, not already-shipped solutions. Admission must still account for the longest permitted service context and worst-case dispute, not just a short benchmark prompt.

This phase MUST also replace the `2^18` network context cap and the explicit IR held-history rejection with a fully specified versioned route. It must handle the separate per-position MAC/state/live-memory caps, job leaf ladder and DA reach in §12. A segmented execution retains the full declared history through authenticated boundary state; setting every segment's semantic context to 512 is a different model, not segmentation. Every source configuration change, including positional scaling for a 2M extension, is committed into the profile identity and tested independently.

### C. Keep execution exact; make normal verification probabilistic

Permit independent workers to produce checkpointed, resumable calibration evidence and streamed weight chunks, keyed by all semantic inputs. Separate `convert`, `declare/preflight`, `conformance`, and `certify` commands; `pack build` may remain a convenience wrapper. Use existing `--stats-in`, `--chunk-store`, and `--keep-chunks` as implementation starting points, but fix provenance and resumability. CPU or integer GPU acceleration is an **operator choice**, never a new semantic rule: all outputs and commit points must match the reference TIR; a GPU backend's existence does not mean the pack calibration/conformance path already uses it. Recurrent scans require a validated chunk-summary/parallel-scan algorithm or honest sequential execution; do not promise linear core scaling.

Normal Panel verification follows §15: randomized algebraic checks replace replay of heavy operations; authenticated constraints cover the remaining operations, state and routing. Exhaustive reference replay is a development/conformance baseline and an optional small-class path, not a per-claim requirement for large classes. Applying the same machinery to calibration verifies the declared calibration computation; source-model accuracy still requires its separate fidelity evaluation.

For 2M, a full-sequence calibration requirement may remain for accuracy, but it must be possible for a separate, provisioned worker to compute it once and publish content-addressed evidence. Before making “ordinary VPS registration” a guarantee, specify how a cheap verifier detects forged coverage/statistics without replaying the entire calibration. Options include recomputable challenge windows, a bounded fraud-proof with committed intermediate states, or independently bonded attestations plus slashable availability, each with quantified soundness; a hash and signature alone prove identity, **not correctness**. Calibration fidelity is also not the same as consensus determinism: the chain may validate deterministic TIR semantics while a certification gate judges source-model fidelity.

### D. Resource envelopes, not fixed fleet tiers or architecture names

Admission/preflight should expose separate finite budgets for `registration_validator_steps`, producer inference time/bytes, DA answer bytes/time, worst court bytes/work/window, and seat RAM/VRAM/storage. A registrant can publish a larger honest resource requirement; operators self-declare capacity and prove readiness through reproducible jobs. The chain must not assume the SDK's current 8 GiB tier is every future seat. Layer sharding or other verified partitioning may reduce per-seat footprint only with the coverage and fraud-detection proof in RFC-0006/0007.

An unknown HF architecture is first a **frontend/primitive coverage** question, not a CPU error. Allow permissionless, signed/source-pinned frontend adapters that lower to a frozen, validated TIR primitive set. New semantic primitives require review, a new version/fence, reference/court support and a test corpus; arbitrary Python or custom kernels do not become consensus meaning by being uploaded. The objective is that any finite, representable model can progress by supplying an adapter and sufficient resources, and that a genuinely unsupported model reports the missing primitive or semantic rule precisely.

## 5. Implementation order and release gates

| Phase | Change | Passing evidence |
| --- | --- | --- |
| A — tool-only | Fix `calibrated_context` parity for `--calib`/`--stats-in`; structured refusal report; resumable calibration/conversion; split pack operations. | Same source+stats+policy yields identical artifact root on two machines; forged/short coverage is refused; 9B refusal names the exact gate without allocating unbounded CPU. No consensus fingerprint change. |
| B — admission efficiency | Bounded verification-plan admission for the new route; exact close-sizing improvement remains an optional legacy route. | Real 9B profiles under the explicitly named target ruleset: old refusal reproduced, new route passes **all** registration gates and its terminal bounds hold. A newly exposed blocker is not completion. Differential bound checks and hostile-input CPU ceiling. A consensus change has its own fence and depends on P before use. |
| C — 2M semantics | IR prompt attribution beyond 4,096 IDs, replacement of the `2^18` context cap and held-format refusal, bounded segment/state commitments, DA and court route. | 32,783/32,784 and 262,144/262,145 boundaries; 2M valid/invalid jobs; altered prompt tile or predecessor state convicted; no split-credit inflation; **all** §12 gates pass with a real artifact. New fence and fingerprint. |
| D — capacity/certification | Open worker evidence, capable-seat/shard certification and lifecycle. | Untrusted calibration worker can be replaced; a low-resource registrant can publish/register a *safe* class without doing model inference locally; class remains weightless until independent readiness; a provisioned model is actually mined and challenged; non-ready class cannot earn. |
| P — probabilistic acceptance (required for the new route) | §15 verification plans, commitment-bound Freivalds/GKR checks, complete constraint coverage, post-commit challenge schedule, soundness accounting and bounded court localization. Develop alongside B/C; D cannot enable this route before P passes. | Single-fault and adaptive-grinding tests; reviewed claim-wide error bound; valid claims finalize without routine segment replay; a failed check reaches an exact terminal dispute; missing evidence cannot finalize; model/SDK/IBD verdict parity. A separate dormant fence, no inherited activation at DAA 5,300. |
| E — Hub-wide coverage | Census and close the highest-volume *full-task* blockers across every Hub modality, format and framework (§§7–10). | A pinned all-Hub report proves the one-sided 95% lower bound for `registered_full_task / D_all` is **at least 0.90**, with actual on-chain registration evidence for sampled successes; every inaccessible, partial-task and untested repo remains in the denominator and out of the numerator. |

Release gate: reproduce the prior 9B `2^26 + 1` refusal on the old binary, show the proposed binary's proven sufficient bound **and accepted state transition through every registration gate**, compare node and SDK preflight at the same DAA/fence, and run registration/IBD/reorg/court/DA tests. A named next blocker is a useful development result, **never a release pass**. Then measure 8k and 2M calibration wall time, peak RSS/VRAM, energy and cost on named hardware, with abort/resume and missing-worker cases. No change is “fixed” merely because a UI line appears or a fast happy-path registration passes.

## 6. Explicit non-goals and unresolved questions

This RFC does not register the Huihui model, open a market, start a Panel, claim that 2M or Kimi K3 was benchmarked, or assert an exact hardware requirement. It does not remove court windows, treat trust in a calibration signer as a proof, or promise support for physically unbounded contexts. The selected probabilistic acceptance policy deliberately admits a nonzero false-accept risk under explicit assumptions. Before activation, resolve §15's challenge construction, integer/field equivalence, whole-claim error budget, witness/prover costs and bounded escalation, alongside the existing 9B/2M barriers. Neither court availability nor a successful small-model benchmark resolves those questions by itself.

## 7. The 90% claim: denominator, numerator, and feasibility ceiling

This section makes the user's *all-Hugging-Face* target explicit. It is intentionally stronger than [RFC-0002 §II.10](0002-palw-tensor-ir.md)'s first **majority** milestone and distinct from [RFC-0002 §II.12](0002-palw-tensor-ir.md)'s **90% of local text LLMs** target. The latter cannot satisfy this RFC by changing a denominator.

* `D_all`: every publicly discoverable HF **model repository** in a dated, complete, cursor-paginated Hub snapshot, counted once as `repo_id@commit_sha`. Include gated listings, missing or broken weight repos, unknown architecture/task, custom code, every modality and every framework. Private repos cannot be enumerated and must be reported outside the claim. Report exact count, crawl time, page count, API version, failures and duplicate-ID resolution. Do **not** silently exclude “unusable” repos.
* `D_files`: the subset with complete, readable checkpoint files or a resolvable base-plus-adapter chain at pinned revisions. `D_files` is a secondary diagnostic denominator, not the headline.
* `D_rights`: the subset of `D_files` with evidence of the rights needed for the proposed registration and artifact distribution. Unknown permission is `RIGHTS_UNCONFIRMED`, not an assumed pass. This RFC cannot create rights that a registrant does not have.
* `R_full`: repositories in `D_all` for which an independent party has reproduced source and declared task/context, built the full artifact, passed **real-size** consensus admission, and obtained either (a) an accepted on-chain class registration or (b) reproducible evidence that the exact same artifact/program/tokenizer/layout/task/context is already represented by an accepted registration (§11.2). The evidence must link the source revision to the existing class and its chain-state proof; a similar name, model card, shared weights alone or unverified equivalence assertion does not count. Reusing an identical registered class is an idempotent success, not a second registration or added reward weight. A narrow text-only extraction of a vision/audio model, a 512-context redefinition of an 8k/2M model, header preflight, a published-only artifact, and market creation are not `R_full`. A genuinely admitted zero-share class may count; mining readiness and `Final` are separately reported and do not automatically follow from registration.
* Primary target: `R_full / D_all ≥ 0.90`, **and**, when a probability sample is used, a predeclared one-sided 95% lower confidence bound for this same repo-weighted ratio ≥ 0.90. Publish point estimate, bound, exact raw successes/failures and stratum weights. Also publish `R_full / D_files`, `R_full / D_rights`, the actual on-chain registration count, download-weighted demand, unique artifact roots and feature-family coverage. None substitutes for the primary ratio.

Rights/access evidence is an off-chain feasibility and publication condition, **not a proposed consensus licensing oracle, architecture allowlist or HF-account permission gate**. The chain verifies commitments, signatures and computation rules; it does not decide whether an HF username owns intellectual-property rights. Do not introduce a central approver under the label of permissionless registration.

An immediate feasibility check is `D_rights / D_all ≥ 0.90`: `R_full ≤ D_rights`. If gated, missing, legally unavailable or unresolvable repositories alone exceed 10%, **no converter, extra CPU or RFC wording can make the all-public-listings target true**. Report the actual ceiling and seek access/rights or explicitly revise the objective with the user; do not quietly switch the headline to `D_files`. A repository with uncertain rights remains a failure until evidence exists. The snapshot must disclose whether HF listings, rather than distinct checkpoint weights, are the intended unit; popular mirrors do not prove family breadth, so artifact- and feature-weighted results remain mandatory diagnostics.

## 8. Current evidence, and how much remains unproven

The existing [HF coverage analysis](../design/palw/tir/hf-coverage.md) reports **90/100 representative architecture entries labelled A/B** (§23). This is not uniformly a completed full-weight harness: the underlying report includes route-only entries and `chatglm3` with `fixture.available: false` while still labelled B. Its curated corpus is not a probability sample of HF repositories. The analysis also excludes non-text models from an earlier architecture denominator (§2). Its 2026-09-28 Hub-count-based calculations (§§18–20) estimate about **3.10 million** model repositories and roughly **505,000 / 16.3%** potentially registrable after proposed generic-class work. These are **estimates**, not a pinned, audited `R_full` census; they include model families and fences not proven at real size and therefore are not a current registration percentage. RFC-0002 §II.10 reaches the same conclusion. The real 9B refusal in §1 is a counterexample to treating “lowerable” as “registrable.”

On that historical *illustrative* 3.10-million denominator, 90% is about **2.79 million** repositories. Even taking the optimistic 505,000 estimate at face value leaves about **2.285 million additional full-task registrations** to justify. This arithmetic is not a forecast; it shows why improving only the 9B converter, 2M context, or GGUF import cannot substantiate an all-HF 90% claim. The status at the time of this draft is **UNPROVEN**. No live all-Hub census, rights ceiling, real-checkpoint registration sample, or one-sided lower bound is present in this worktree.

The appropriate empirical sources are the official [HF Hub API](https://huggingface.co/docs/huggingface_hub/en/package_reference/hf_api) (`list_models`, revision-pinned `model_info`, `list_repo_tree`), [model cards](https://huggingface.co/docs/hub/model-cards) and [Diffusers loading metadata](https://huggingface.co/docs/diffusers/main/using-diffusers/loading). A `transformers`-only `config.json` inventory necessarily misses other Hub libraries; these are not “not models” and remain in `D_all`.

### 8.1 Recount of the checked-in evidence, not a projected success rate

The accompanying [audit JSON](evidence/0011-existing-coverage-audit.json) pins SHA-256 hashes of the reports and their harnesses. The [read-only audit script](evidence/0011-audit-coverage.mjs) recomputes these results from repository files; it does not execute or register models, enumerate HF, or replace missing evidence with a prediction.

| Evidence scope | Observed saved result | What it establishes |
| --- | --- | --- |
| Curated architecture report | A/B 90/100 | Expressibility labels under this harness; not 90 accepted registrations. |
| Additional architecture report, misleading if read as a Hub census | A/B 54/91; C 37/91 | More causal-LM-family fixtures, not a repo-weighted probability sample; unsupported tail still substantial in this fixture set. |
| Pinned shape preflight, `convert` | ok 87, blocked 13 | Conversion-stage verdict on light specs. |
| Same preflight, `register` | ok **74**, blocked **4**, unknown **22** | Static stage verdict, not a signed stateful registration. |
| Same rows, `convert` AND `register` | **73/100** | Stage counts are not a sequential funnel: `minicpm` has blocked conversion but ok registration-stage status. |
| A/B AND both preceding preflight stages | **72/100** | Even this intersection is only fixture evidence, not HF registration coverage. |

The preflight [harness](../../misaka-palw-sdk/tests/corpus_preflight.rs) explicitly writes **header-only safetensors** (tensor data absent), defaults to `Depth::Shape` and `max_context = 128`, and compares verdicts against golden JSON. It does not send registration transactions. The architecture [harness](../../misaka-palw-tir-lower/tests/corpus_v2.rs) principally uses tiny random-initialized models. A `heavy_fixture: true` report field therefore does not by itself mean a production-sized upstream checkpoint was tested. These limitations prevent any table entry above from being used as `R_full/D_all`.

The four pinned registration blockers are concrete regression targets: `deepseek_v32` operand-count admission; `deepseek_v4` dissection root-claim bytes/court cost; `gemma3n_text` program-byte limits; `internlm2` tile MACs. The 22 unknowns include encoder–decoder, vision, diffusion and audio entries. Classify and actually exercise those routes instead of promoting `unknown` to supported. The 9B fix alone does not address these independent gates.

Reproduce the audit without modifying source or connecting to a node:

```sh
node docs/rfc/evidence/0011-audit-coverage.mjs --check
node docs/rfc/evidence/0011-audit-coverage.mjs --self-test
```

If report bytes change, `--check` must fail until the evidence and explanation are deliberately regenerated/reviewed. A passing audit checks provenance and arithmetic only; its verdict remains `UNPROVEN` for HF registration coverage. Missing tests, unrun later stages and a permissive label never become evidence through this audit.

## 9. Required coverage work, selected by measured lost repositories

The following are **candidate workstreams**, not unsupported claims about their present share. The census must quantify each bucket first and then attach before/after evidence to a reusable implementation, so the combined *remaining* blocker count is at most 10% of `D_all` with uncertainty included.

| Full-task blocker family | Required generic route | Passing evidence; no shortcut |
| --- | --- | --- |
| Text generation, MoE, recurrent/hybrid, 8k–2M context | RFC-0002 TIR / active versioned kernel relations; new families require the §16 upgrade route, not a VM fallback. Real-size close sizing, prompt attribution, calibration and court fixes in §§2–4. | Full configured context and source-equivalent task pass actual chain admission; 9B and 2M examples are adversarial fixtures, not representative counts. |
| Text encoders, classifiers, rerankers, seq2seq and embeddings | Task-specific canonical input/output and pooling/heads, tokenizer and labels, bidirectional/encoder–decoder execution; no decoder-only inference from a familiar backbone. | Each advertised output and preprocessing path reproduced by independent vectors and the court; real checkpoint registered. |
| Image classification, detection, segmentation, generation/editing, vision-language | Full image preprocessing, backbone/projector or diffusion/flow pipeline, output profile and input commitments through RFC-0003 or an equally exact route. | Complete vision task, not the text subnetwork, passes exact job, artifact, DA and court tests. |
| Speech, audio, video and other temporal or multimodal tasks | Canonical media encoding/preprocessing, stage pipeline, bounded long-sequence/reduction dissection, exact output forms. | End-to-end intended modality and resolution/duration; missing RFC-0003 profile is an explicit blocker. |
| GGUF, GPTQ/AWQ, bitsandbytes, MLX and other quantized or framework-specific formats | Declarative, versioned importers and per-format tensor/scale/rounding semantics; offline sandbox for untrusted code. | Real upstream files and independent decoder cross-check; no implicit dequantize-and-requantize that changes the claimed model silently. |
| LoRA/PEFT, merged variants, custom-code architectures and composite repositories | Resolve every pinned base, adapter, tokenizer and custom component; lower into an active kernel's bounded tensor/state and constraint grammar. | Whole dependency graph and source equivalence, not a generic `model_type` guess; custom source is conversion input, never an uploaded verifier or guest executed by consensus. |
| Gated, missing, ambiguous or legally unusable sources | Access and rights evidence, archive/revision pinning, or upstream repair. | If unresolved, fail `source` and remain in `D_all`; software cannot manufacture absent weights or permission. |

An architecture adapter is sufficient **only** when the complete operation is expressible under armed semantics. Otherwise a missing general primitive, task profile, format semantics, DA route or court path requires a versioned kernel upgrade and an actual fence (§16). RFC05's former GVM residual is withdrawn: unsupported operations remain `KERNEL_EXTENSION_REQUIRED` until the complete new kernel route is implemented and active. Keep model-file size, operator hardware and court envelope visible separately. More capable machines can satisfy finite execution budgets; they cannot validate an unbounded or unrepresentable class by declaration.

## 10. Reproducible census, experiment, and release decision

1. **Freeze the frame.** Save the complete cursor-paginated Hub listing, crawl start/end UTC, immutable repo SHA resolution, task/library/gating/license metadata, file tree identifiers/sizes, and API/client version. A multi-page live crawl is an observation interval, not an atomic historical Hub snapshot; document and reconcile added/deleted/renamed repos, retain unresolved IDs as unknown failures, then freeze the resulting frame. Hash and publish the manifest. The crawl must reach the end cursor; interruptions resume from a recorded cursor without treating uncrawled pages as supported. File errors and `unknown` categories stay in `D_all`.
2. **Predeclare the experiment.** Stratify by advertised `pipeline_tag`/task, library, format, parameter/bytes band, context band, age and rare/unknown features. Record a deterministic random seed, inclusion probabilities, sample size powered so its **one-sided 95% lower bound could exceed 0.90**, and a holdout snapshot not used to select features. Include both a probability sample and a separate stress cohort of top-download, new-family, custom-code, largest-size and 2M-context repos. Convenience samples and the 100-architecture corpus never estimate `D_all` coverage.
3. **Record the full funnel.** For every sampled repo, retain source/rights → full-task frontend → real-weight pack and fidelity → consensus admission → accepted registration → qualified seats → claim/`Final`. Mark `NOT_RUN_AFTER_<gate>` when blocked; it is not a pass. Preserve original task/context and exact source revision. Each failure has stage, code, needed/cap values, active fence, retry provenance and independent reproduction. A censored sizing result states “≥ cap+1,” not an exact work count. Existing RFC-0002 §II.10.3 gate names are the baseline.
4. **Prove each sampled registration.** Where the repository can legally be fetched, use real checkpoint bytes and the actual network rules/binary; produce an accepted class-registration transaction or verified equivalence to an existing registration (§11.2), with block/DAA/class ID, artifact root and node/SDK matching verdict. Verify the resulting class record, not merely transaction inclusion: the processor can drop a lifecycle object while its carrier block stands. If cost requires an isolated testnet, run the same consensus rules with a separately named test chain, then cross-check a subset on the intended public testnet. Offline preflight alone is `registration-ready`, **not** `R_full`. If a sampled model cannot be fully exercised, it is a failure/unknown outside the numerator, not projected success.
5. **Calculate without optimism.** Estimate repo-weighted `R_full/D_all` using the predeclared stratified design and a valid one-sided lower bound (finite-population/stratum weighting disclosed). Include inaccessible/rights-unknown and nonresponding samples as failures; report best-case eligibility ceiling, per-stratum estimates, a family-level holdout and the exact unsupported-repo list. Guard against a large copied architecture masking whole missing tasks. Re-run on a newer snapshot to detect model-distribution drift.
6. **Iterate by lost count.** Prioritize generic workstreams by observed lost **repositories**, not only downloads or a striking example. For each change, publish blocker count before/after, regressions, full-task fixtures, source equality, on-chain sample and the new lower bound. If the all-Hub bound remains below 0.90, RFC11 remains open regardless of decoder-only or local-LLM success.

Acceptance is conjunctive: the crawl is complete and pinned; rights/access ceiling permits 90%; the estimate's one-sided lower bound is ≥90% for **on-chain full-task registration over `D_all`**; no sampled success rests on a partial task, short-context substitution or unarmed rule; the release binary has bounded validator work, exact court and DA tests for every newly armed semantic route; and mining/`Final` rates are reported separately, never implied by registration. If any item lacks evidence, the allowed statement is the measured narrower result and its blocker distribution—not “RFC11 implementation registers 90% of HF models.”

## 11. No hidden registration blocker: exhaustive gate-closure contract

The 9B sequence (`metadata` → `tile MAC` → `close-sizing` → possible later gate) demonstrates why removing the **first** error is not removing the barrier. A successful implementation must expose and close the *whole* path, including its transaction acceptance, for every claimed success. The current authoritative surfaces include [`palw_tir_admission_v1.rs`](../../consensus/core/src/palw_tir_admission_v1.rs), [`PalwClassAdmissionError::code`](../../consensus/core/src/palw_class_admission_v2.rs), the SDK's [`tir_registration_preflight_v1`](../../misaka-palw-sdk/src/tir_registration.rs), [`preflight`](../../misaka-palw-sdk/src/preflight/mod.rs) and the processor's `ClassRegisteredTirV1` acceptance arm. `model add --manifest` also has a verifier path. A frontend PASS that bypasses any one of these is insufficient.

### 11.1 Inventory every gate, not only the first refusal

| Gate family | Current/likely blocker | Closure obligation and proof |
| --- | --- | --- |
| Source, scope, rights | Missing/gated weights, unresolved base/adapters, `CONFIG_INVALID`, partial advertised task, unsafe custom code or `RIGHTS_UNCONFIRMED` | Immutable complete source graph; owner/access evidence; full-task input/output and external-code isolation. A missing or unauthorized source is an **external** failure, not “fixed” by implementation. |
| Frontend and material | `ARCH_REFUSED`, `ARCH_NEEDS_PRIMITIVE`, unknown quant type, unconsumed tensor, tokenizer mismatch, `calibrated_context`/`--stats-in` mismatch, fidelity or reproducibility failure | Versioned adapter over active kernel semantics, or an explicitly required kernel extension; all weight/layout/rounding keys consumed, independent reference comparison, same root on independent workers and full declared context. |
| Protocol availability | `FENCE_NOT_ARMED`, wrong `prim_set_id`, absent task/job/output profile, legacy-only prompt attribution | Required rule actually armed at the target DAA with a pinned fingerprint; direct-TIR remains open, new primitives use a reviewed fence, and >4,096 prompt IDs have a complete IR proof route. |
| Static admission | `TIR_EXCEEDS_CEILING`, tile MAC/elementwise/transcendental budget, close-sizing `2^26`, range/bounds, canonical job, PWU, ladder, credit, context and history bounds | A candidate layout or mathematically equivalent bound passes the *same* chain gate under worst-case real-size inputs; no unmeasured ceiling waiver. All later checks are run after each fix. |
| Court and DA | `TIR_NEEDS_DISSECTION`, `TIR_DISSECTION_REFUSED`, close bytes, unavailable history/tile, court window, unanswerable DA challenge | Every reachable commit point has an exact one-move or bounded dissection route, a feasible signed DA response and deterministic worst-case bytes/work/time; independent false-claim court drill. |
| Registry and transport | SDK duplicate-root filter, class ID mismatch, source/class ambiguity, registrant's active bond and signature, exposure plus burn, zero-share rule, activation horizon, per-block registration cap, carrier assembly, transaction inclusion, reorg/pruning | Deterministic IDs and signed sponsorship path, idempotent reuse of exact classes, stateful preflight and retryable capacity results; end-to-end **class-state** acceptance and recovery after reorg. No genesis/fleet-only permission. |
| Pre-submission tooling | `PACK_NOT_VERIFIED`, artifact mismatch, required conformance SKIPPED, incomplete declared layout | CLI/tool blockers, not chain refusals; exact identity/conformance must pass before the normal CLI submission route. RFC13 defines remediation without overriding verification. |
| Post-registration only | `SEAT_MEMORY_SHORT`, insufficient independent seats, Panel backlog, absent `Final`, market not opened | Report separately as `mine`/`serve`/`market`. These do not retroactively make an accepted class unregistered, but no UI may label it mineable or sell benefits as ready without the corresponding proof. |

This table is **not** a manually maintained claim that no other gate exists. CI must enumerate every `PalwClassAdmissionError` code, every stable SDK preflight blocker and every processor/manifest rejection reachable for a class registration. Add an exhaustiveness test so a new refusal cannot ship without a stage, structured `code`, `needed/cap/fence`, remediation category, adversarial test and coverage-census bucket. Record all blockers per candidate by independent staged checks even when production validation stops at the first refusal; `NOT_RUN_AFTER_<gate>` remains explicit where subsequent checks require unavailable data. Compare the manifest verifier, SDK, mempool and accepted-chain result at the same ruleset/DAA, including genesis/reorg replay. Any disagreement is a **tool/protocol bug**, not a model failure to ignore.

For the 9B 8k target, a release demonstration MUST use the original pinned source and the full 8k artifact, show fixed provenance, the sufficient close-sizing bound, tile/court/DA/window results, a successfully accepted `ClassRegisteredTirV1` (or proven equivalence to the exact existing class) and matching node/SDK verdicts. Then show the seat shortage as a separate readiness state. For 2M, use a real declared 2M model with 2M calibration evidence, >4,096-ID prompt attribution, worst-case history dissection, finite resource/court bounds and an accepted registration. A 512-context substitute, synthetic tiny net or header-only test cannot close either case. If the next refusal appears, the case remains **open** until it too is resolved and retested through acceptance. The separate full-HF task/context requirement still applies before either case contributes to `R_full`.

### 11.2 Exact duplicates must be idempotent, without inventing a new mandatory registry

The SDK's [`tir_registration_candidate_v1`](../../misaka-palw-sdk/src/tir_registration.rs) removes both already registered class IDs and **already registered artifact roots**. This client-side test is broader than the fold's `claim_artifact_root`: its ownership key is `(class_id, artifact_root)`, not a globally unique artifact root. Do not infer that consensus forbids every distinct class sharing weights, or introduce a new mandatory on-chain source-binding transaction solely to work around this SDK filter.

Change the SDK result to `ALREADY_REGISTERED` **only after** checking the complete class identity against accepted chain state. The coverage evidence binds `repo_id@sha`, source-component hashes, frontend/quantization versions, program, tokenizer, layout, full task/context and artifact root to that class. Independent re-conversion supplies the equivalence evidence; a signature or URL alone does not. Exact mirrors then need no new fee, transaction or reward weight. Preserve and report lifecycle status: reuse cannot rehabilitate a `Frozen` class or skip the fresh signed/reserved transition for Dormant reactivation. Different context, tokenizer, task or program is not an alias: run the normal class gate and stateful ownership checks instead of rejecting it merely for sharing a weight root. Test both equal-class reuse and different-class/same-root registration. Preserve existing ownership, work-credit and payout rules; this route confers no right to impersonate an HF publisher or claim its rewards.

The current post-genesis path also requires an **active registrant bond and a signed object** in the virtual processor. Provide a permissionless remote-builder/relayer workflow using those existing signed objects so a registrant need not run inference or a Panel locally. A sponsor may pay only under explicit, signed terms distinguishing the bond signer, publisher attribution and model-line economic owner; being a transport relayer gives no signing or ownership authority. If preserving a distinct publisher's on-chain ownership requires new authority fields, specify and fence that change rather than silently assigning ownership to the sponsor. Exposure, the registration burn and transaction fees remain deliberate anti-spam/economic conditions, not CPU errors. The dry-run must price them against current reserved and slashable balances, not merely test that the bond exists.

### 11.3 A proof obligation for “more hardware solves it”

For every candidate blocked by local compute, attach a *witnessed resource plan*: pinned source/artifact bytes, streaming peak RAM, disk/net transfer, calibration and conversion steps/time on named CPU/GPU, producer latency, seat RAM/VRAM/storage, number and independence of seats, DA fetch bandwidth, worst court work/bytes/window and node admission steps. Provide an actually executed reference run and a second independent verifier where required. A user-supplied `--seat-share` or a declared fast GPU is **not** proof of capacity. The work may be delegated or sponsored, but must finish within the protocol's real deadlines. If any protocol bound still refuses after more resources are supplied, label `PROTOCOL_LIMIT` and design a safe fenced change; never report `ADD_CPU` as its remedy. If no finite reproducible plan is known, the case is unproven, not “guaranteed with enough hardware.”

### 11.4 Zero-surprise acceptance test and permitted language

The release candidate's coverage report MUST have a machine-checkable row for every sampled repository containing source/task/context SHA bindings, all tested gate codes, independent artifact build roots, chosen rule/fence/DAA, accepted class-state proof and transaction (or verified reuse) evidence, and a separate `mine_ready`/`market_open` result. A random independent auditor reruns the entire acceptance path, not just the preflight. Mutation tests include: one missing weight, forged calibration length, wrong quant rounding, too-wide tile, close-sizing sentinel, 4,097-ID and 2M prompts, false Merkle tile, late court move, unavailable DA shard, same-root/different-context selection, unbonded or wrong signer, depleted exposure, stale fence view and a one-block reorg. A valid case reaches acceptance; each invalid case fails at the named gate without node CPU/memory exhaustion. No unexplained `ADMISSION_REFUSED` bucket remains among claimed successes.

**Only after §§7–14 pass may documentation say “the implemented release can register at least nine of ten public HF model repositories in the named snapshot.”** If based on sampling, this is an estimate at the stated confidence, not a measured success for every repo. Quote the snapshot, numerator/denominator, lower bound, ruleset, full-task definition and residual failures. “All models, every context, no barriers” is not a defensible protocol guarantee: inaccessible weights, missing permission, non-finite work and an absent sound court cannot be made valid by a software patch. The implementable guarantee is **no undocumented or arbitrarily hard-coded registration blocker for a source-complete, authorized, finite, deterministically representable model inside an armed, provably adjudicable envelope**; outside that envelope the precise missing capability or external constraint is reported and counted against the 90% target.

For registrations under the new probabilistic route, §15's verification-plan and soundness requirements are additional release conditions for the coverage claim above. A registration percentage cannot substitute for computational soundness.

## 12. Concrete remaining limits: replace aggregate cliffs, retain bounded verification

The following code facts were checked at the audit commit. They are independent checks, not predictions that the 9B artifact exceeds each one. **More CPU on a worker changes none of these consensus comparisons.** The implementation must resolve every relevant row for each target profile; a single new context flag does not suffice.

| Current gate and source | Current bound / behavior | Required design change or retained condition |
| --- | --- | --- |
| TIR shape/encoding, [`program.rs`](../../misaka-palw-tir/src/program.rs) | 256 KiB encoded program; 16 blocks; 512 nodes/block; 1,024 layers; 4,096 parameter declarations; 64 state declarations; 8 inputs/node. These are **declarations**, not a 4,096-weight parameter limit. | Reuse structural templates where semantically exact. For models beyond this grammar, versioned kernel graph composition with typed interfaces and bounded summaries; missing grammar/semantics requires an activated extension, not a VM. Do not raise parser/allocation caps without metering or assume every architecture fits this grammar. |
| Network TIR ceilings, [`PALW_T12_TIR_CEILINGS_V1`](../../consensus/core/src/palw_tir_v1.rs) | 88,000 program bytes; 65,536 unrolled nodes; 262,144 positions; `2^37` MACs/position; 32 GiB state; 4 GiB peak live; 65,536 cone-work units. | Separate **total producer resources** from **maximum validator/terminal work**. A versioned resource envelope may allow larger total work/state when bounded segments, authenticated paging, complete DA and court verification cover it. Those aggregate caps remain effective until that route is armed; available RAM alone is insufficient. |
| IR held/context and canonical input, [`verify_class_admission_v10`](../../consensus/core/src/palw_tir_admission_v1.rs) | Explicit refusal of `HISTORY_BOUND_V1_HELD`; canonical prefill ≤4,096 IDs, thus context ≤32,783 independently of the larger network cap. | Versioned IR held bindings plus attributable prompt tiles, with predecessor-state and input challenges. Implement all three context changes together, not by waiving one check. |
| Per-tile court and recurrence | MAC, elementwise/transcendental work, operands and `(checkpoint_interval − 1) × state-replay work` are bounded independently. Smaller output tiles do not necessarily shrink a whole input/reduction or state replay. | Derive a legal tile/checkpoint plan. If none exists, split the reduction/state transition into committed, independently adjudicable steps preserving rounding and order. Supply a witness for **every** commit point, not just the largest matmul. |
| Close bytes and close sizing | Both the preliminary `program + frame + operands` test and the final carried close/root-claim bounds must pass; the final sizer stops at `2^26` units. Range sizing is already selected after fence2. | Profile actual repeated ranges/requests. Use proof-checked reusable summaries (§13), and reconcile preliminary vs actual-carriage accounting under a fence if it changes acceptance. A small estimate, faster worker or hash of an unchecked bound is not a proof. |
| Job step ladder, PWU and DA | Deepest permitted job must fit the network ladder; canonical counted PWU must agree. [`PALW_TIR_DA_SEAT_REACH_LEAVES_V1`](../../consensus/core/src/palw_tir_court_v1.rs) is `2^30` leaves from three 10-level descents plus a terminal session. | Segment large execution with authenticated global coordinates/state and disjoint credit. Price segment-directory proofs and all DA descents; never reset a leaf counter to evade the reach bound. Every allowed job, including noncanonical jobs on other lanes, needs an explicit reach proof. |
| Court deadline and material | Every root claim, dissection round, terminal close and assembly must fit its carrier and time budget. | Preserve a bounded terminal court; compose segment proofs and derive total worst-case deadline/escrow retention. Benchmark an adversarial last-leaf dispute and missing material, not only an honest receipt. A model may not select its own arbitrarily long timeout. |
| Stateful registration, [`apply_class_registration_v1`](../../consensus/core/src/palw_state_v2.rs) | Duplicate non-Dormant class; network slash price and target; nonzero PWU; activation no more than 4,000 DAA ahead; affordable reserved exposure plus registration burn and existing slashable locks; share and ownership checks. | Shared stateful dry-run with precise retry/remediation, including Dormant reactivation and accepted class lookup. These are security/economic conditions to satisfy, not checks to delete. The SDK's broad duplicate-root filter must not create an extra protocol rule (§11.2). |
| Inclusion capacity and replay | At most **one TIR registration per block**; the processor can drop further objects while keeping the block. Signed object acceptance, class state, IBD and pruning must all agree. | Return `QUEUED_CAPACITY`, `ACCEPTED` or a named refusal; retry idempotently after state refresh. Increase throughput only with measured, bounded verification cost and a versioned admission-work budget. Do not treat a transaction hash or elapsed DAA as registration success. |

The inventory must also traverse nested decoder/layout errors, transaction mass/size, object chunks and rent, public-source requirements where armed, signature verification, state-root persistence and RPC visibility. Matching a list of public error strings alone cannot prove completeness: CI must associate the actual acceptance call graph and each early return with a gate ID, and run boundary/negative cases at each reachable gate. Proposed diagnostic codes in this RFC are not claims that all those strings already exist.

## 13. Implementable route to removing the cliffs

### 13.1 Verification-plan admission; probabilistic execution acceptance

The preferred route is `VerificationPlanV1`, composed from implemented, versioned kernel-primitive verifier templates. Registration checks the plan and resource envelope; later claims instantiate its randomized checks. The plan is bounded typed data, not executable verifier code or a universal circuit/ISA fallback. Existing exact range sizing remains available for compatible classes. This is a proposed protocol, not a certificate system already shipped.

1. Bind the network/ruleset, active kernel descriptor/version, source/program/tokenizer/artifact roots, full task/context, arithmetic and memory semantics, constraint compiler, verifier suite, segmentation and plan version. Bind the challenge schedule, field/modulus choices, repetition policy, maximum proof/witness sizes, resource limits and security target. Neither a registrant nor a seat may lower these below the armed network policy.
2. Use reusable templates for matrix relations, nonlinear/lookup constraints, routing and authenticated state/memory. Verify typed interfaces and composition over a compact graph or metered modules. Do not unroll every token, expert execution and possible dynamic index at registration. Dynamic access must have an authenticated read/write and bounds rule; routing must prove which expert is selected. The plan covers all constraint families and input/output bindings even though checking their instances is probabilistic.
3. Enforce deterministic limits on parser/allocation work, query openings, localization and terminal computation. Approved kernel-primitive courts and composed primitive bounds establish adjudicability without a separate symbolic proof for each reachable model commit point; this is not an arbitrary ISA/microcode court. If a primitive cannot be checked or localized within the current route, return the missing verifier/court capability. A sample cannot establish the worst-case safety of an unchecked operation.
4. Charge each module/rule, arithmetic width and evidence byte before allocation; cache under the full semantic digest. Large plans use metered composition with authenticated accumulated state, expiry and replay/reorg rules. Registration completes only after composition and stateful gates pass. Cold/warm cache and IBD must give the same result.
5. At claim time verify the committed constraints with the suite in §15. Accept at its reviewed error bound after positive evidence and the challenge window; full trace or full segment replay is not the normal-path completion condition. Preserve deterministic reference executors for conformance and exact terminal disputes.

The real 9B/2M acceptance cases and census remain required. A reusable verifier removes the need to analyze a giant execution exhaustively, but it does not automatically supply unsupported operations, field encodings, authenticated openings or a sound composition theorem.

### 13.2 Segments preserve the model; they do not shorten its context

A segment identity binds `class/program`, job and input roots, full context policy, global layer/position interval, entry-state root, exit-state root, material root and predecessor identity. Tiling, module splitting and parallel scheduling must preserve integer rounding, overflow rules and ordered recurrence; even an associative real-valued expression is not automatically associative under the committed arithmetic. The boundary-state protocol must establish continuity from a verified initial state and permit a challenge to any dependency across segments.

Admission checks a covering partition: no missing or overlapping credited execution cells, no dependency cycle outside an explicitly bounded recurrence, and an exact ownership rule for shared boundary work. PWU/reward calculations charge original work once; splitting a claim cannot multiply ticket opportunities, subsidy or counted inference. A segment directory must itself have bounded inclusion/DA proofs and an adversarially tested dispute route. Total producer RAM/storage/time can grow with the model; per-validator verification, each terminal close, and the aggregate open-session exposure stay bounded by armed rules. This requires coordinated work with RFC-0006/0008, not merely accepting longer arrays.

### 13.3 Cheap registration is not cheap correctness certification

Remove local inference from the **registrant client** requirement: clients may submit a prebuilt artifact commitment and proof-checked static plan made by independent workers. Full weights remain available through verified material providers. Registration is distinct from source fidelity and readiness, with those states explicit in both RPC and UI. `--stats-in` provenance can establish which computation is claimed, but proving the statistics/calibration correct still needs reproduction or a sound challenge/proof protocol before they count as verified.

For quantized models, exact agreement between independent TIR executors is mandatory; bitwise equality to the upstream floating-point runtime is **not** generally possible. Pin the transformation and predeclare numerical/task fidelity thresholds, corpus, maximum context and modality coverage. Report measured fidelity separately. An explicit, validated quantization may count under the census policy; silently dropping vision, shortening context, changing quant semantics or skipping the independent reference does not. Extending this particular 262k-configured source to 2M must be labelled an extension and independently validated, not counted as upstream 2M support.

### 13.4 Deterministic eligibility, not a promise of immediate inclusion

Add a shared stateful dry-run which invokes both admission and the same fold guards used by acceptance, on an immutable parent-state snapshot. Report `ELIGIBLE_AT(parent_hash, ruleset, object_hash)` with exact required exposure/fee, activation range and remaining block budget. Before signing/sending, refresh changes in bond state, target, fence and ownership. An earlier result is not a guarantee across a state change or reorg.

The proof obligation is: **for identical parent state, ruleset and object bytes, with valid carriage and available per-block budget, the dry-run's accepted transition equals the node's accepted transition and class-state root contribution**. Differential tests exercise this property against production acceptance, not a mock reimplementation. Network liveness and a fair inclusion opportunity are separate assumptions; under congestion or censorship no RFC can promise that every eligible transaction will be included at a particular wall time. Block-cap deferral must be observable and retryable, not presented as model incompatibility.

## 14. Required evidence bundle and completion checklist

Publish a versioned `registration-evidence/v1` bundle for each accepted case. These are required output fields, not fabricated run results:

* `source`: repo/revision, complete file/component hashes, advertised task/context and any declared transformation/extension; model-card/config evidence.
* `build`: converter and adapter commit, immutable calibration inputs/statistics, source-fidelity results, artifact/program/tokenizer/layout roots, independent rebuild/reference result and hardware/resource measurements.
* `rules`: chain genesis, code commit, consensus/schedule fingerprints, parent hash/state root, DAA, armed fences and all derived limits.
* `gates[]`: stable gate ID and source symbol; `PASS / FAIL / NOT_RUN / DEFERRED`; exact value or censored lower bound, units, cap, proof/witness digest, test result, dependency and remediation. `PASS` cannot be manufactured from an unchecked declaration.
* `acceptance`: exact object/transaction hash, accepted block, resulting class-state proof, or existing-class proof plus reproduced equivalence. Include object-level disposition, RPC observation, confirmation/reorg policy and cold-node IBD replay.
* `post_registration`: independent fields for capable seats, certification, first mined claim/Final, model-line listing and market availability. Missing Panel operation does not invalidate **registration**, and registration does not imply any of these later states.
* `verification`: verification-plan/suite version, constraint-coverage map, committed evidence and challenge/transcript digests, per-claim error derivation and assumptions, Panel rule, challenge-window result, and measured prover/verifier/DA/localization costs. Distinguish `probabilistic` from legacy `replay`; never label a sampling receipt as full recomputation.

Release checks are conjunctive:

| Case | Required result |
| --- | --- |
| Previous 9B failure | Old pinned build reproduces the named refusal; candidate passes every gate using the requested 8k semantics; accepted class state and RPC agree. No context reduction or skipped late gate. |
| Long context | Validated real 2M artifact, all three context barriers removed, worst-case court/DA/resource bounds and accepted state proof; upstream full-task/context coverage evaluated separately. |
| Broad coverage | Complete pinned sampling frame; declared full-task policy; real-source probability sample, missing/untested entries counted as failures; ≥90% lower bound and separate unsupported buckets. A statistical confidence bound is not a deterministic guarantee for every repository. |
| Anti-regression | Boundary tests at cap−1/cap/cap+1, malicious proof and material tests, manifest/SDK/fold parity, no-reference-skip acceptance, independent audit and cold IBD/reorg equivalence. |
| Operations | Offloaded workers replaceable; abort/resume reproduces roots; inclusion-cap retries and depleted-bond errors observable; no false “registered” based on transaction presence alone. |
| Probabilistic route | §15's whole-claim soundness review, single-fault/adaptive adversarial cases, constrained Final and bounded dispute/DA handling; no routine segment replay. Kimi K3 is a separate real-model scaling gate, not implied by the 9B pass. |

Until the bundle exists, the conclusion is **“known barriers identified; proposed closure path; registration/coverage unproven.”** RFC approval is not evidence of implementation correctness. Successful real registrations establish individual cases; a representative experiment establishes a scoped coverage estimate. Neither establishes support for all future architectures or unlimited contexts.

## 15. Sampling-first / court-on-dispute (2026-10-06 decision)

### 15.1 What is accepted probabilistically

Normal verification evaluates randomized **constraints on committed computation**, rather than re-executing whole sampled segments. The miner still performs the intended execution and constructs its evidence. All constraints affecting outputs, state and rewarded work must enter a sound check or a sound aggregation; this is coverage of the verification statement, not deterministic replay of all operations. Small cheap constraints may be checked exactly. The accepted mathematical statement is that a false committed execution passes with probability at most the suite's stated bound, under its stated assumptions. It is not a Bayesian claim that a particular passing execution has that posterior probability of being wrong.

For raw uniform sampling without replacement, with `N` segments, `b` bad segments and `s` queries, the miss probability is `C(N-b,s) / C(N,s)`. For a single bad segment it is `1-s/N`; at `N=100,000,000`, `s=100`, detection is only `0.0001%`. Checking a selected segment perfectly, or using Freivalds only inside it, does not repair that selection loss. The exact court only handles a detected/filed dispute; it cannot retroactively make an undetected lie detectable.

Accordingly, raw 32–128 segment spot checks may be diagnostic audits but are not sufficient for this route's Final or useful-work entitlement. Selection of a small number of encoded queries is permitted when the complete constraint-to-proof reduction has a reviewed error bound, including a single output-changing fault. Erasure-coding a false raw trace does not itself prove its execution correct; FRI proximity must be composed with computation constraints and boundary conditions. A public, non-ZK interactive proof/IOP is allowed. Calling it sampling does not remove its witness/prover cost.

### 15.2 Constraint map and checker choices

[RFC-0007 Part V](0007-palw-verification-certificates-and-algebraic-checks.md#part-v--constraint-verification-is-the-normal-panel-path-2026-10-06) supplies the Panel integration contract: batched token/cell verification, an evidence-bound `PalwConstraintReceiptV1`, explicit whole-claim/segment/audit scopes, coverage-aware tally and a MatMul-first delivery plan. The proposed receipt is distinct from the already existing `PalwSeatReceiptV3`; protocol versions and old claims must not be reinterpreted. That contract implements the direction here without lowering this section's coverage or §15.4's error target.

| Relation in the committed computation | Normal check | Exactness and evidence obligations |
| --- | --- | --- |
| Dense, attention and executed-expert matrix products | Freivalds projection or GKR/sum-check reduction | Bind both inputs, output, dimensions, weights and arithmetic before challenges. Authenticate projected evaluations against those same commitments. |
| Multiple layers/modules and many products | Random aggregation and a reviewed GKR/sum-check composition | Include every constraint and its wiring; derive degree/round/batch error, not a fixed `1/p` for an arbitrary circuit. Final input evaluations must bind the actual input and weight roots. |
| Quantization, rounding, saturation, carries, activation, normalization and comparisons | Bounded exact checks or range/lookup/bit constraints within the proof suite | Prove committed integer semantics, including ties and overflow. Replacing a nonlinear operation by an approximate polynomial defines a different execution unless explicitly part of the model identity. |
| MoE routing/TopK, dynamic indexing, embeddings and memory/history | Routing, index bounds, authenticated read/write and permutation/lookup consistency checks | Prove the chosen expert/address and read-after-write order. A correct multiplication using the wrong expert is invalid. Include all executed branches and prove their selection; unused branches need not be executed. |
| Segment boundaries, recurrence, inputs and outputs | Boundary equality and authenticated state-transition constraints | Bind the initial state, predecessor chain, full context, final output and disjoint work coordinates. Correct isolated segments with fabricated entry states cannot pass as a correct job. |
| Data availability | Committed material, bounded requests and a separate availability policy | Availability is not execution validity. Evidence must remain retrievable through disputes and retention; unavailable evidence cannot be a positive receipt. |

For `X ∈ F^(m×k)`, `W ∈ F^(k×n)` and claimed `Y`, Freivalds checks `X(Wr) = Yr`. For fixed false `Y`, a uniform independent `r ∈ F_p^n` passes with probability at most `1/p`; `t` independent repetitions give at most `p^-t`. A binary challenge vector instead gives the classical bound `2^-t`. These statements require a nonzero error **in that field**, fixed before the challenge. Integer differences that are multiples of the modulus require proven ranges, limb/carry constraints or sufficient independent moduli; mere modular equality is insufficient. See [Slalom, Lemma 2.1](https://arxiv.org/html/1806.03287v2).

Direct Freivalds costs `O(mk + kn + mn)` field work per repetition, including reading the matrices, versus `O(mkn)` classical multiplication. For single-token GEMV this need not save asymptotic work. Preprocessing/sketches can help fixed weights, but their construction, storage, authenticated relation to the artifact and refresh must be counted. Public `r` known before `Y` is fixed is unsafe. RFC-0007's private reusable sketches have different secrecy/reuse assumptions; do not expose them as a public reusable challenge or inherit their amortization for fresh public vectors.

GKR is the preferred candidate for aggregating compatible large constraint graphs; its cost depends on circuit depth, input access, wiring regularity and commitment openings. It is not an automatic constant-time verifier for arbitrary TIR. The [GKR theorem](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/) and [SafetyNets](https://arxiv.org/abs/1706.10268) motivate this route but do not establish performance for MISAKA's nonlinear, recurrent, multimodal graphs. A Merkle root alone cannot authenticate an arbitrary linear/polynomial evaluation cheaply: specify and price the opening protocol, a verified streaming pass, or a reviewed commitment scheme. Preserve MISAKA's post-quantum security requirements when selecting a scheme; pairing-based commitments cannot silently become a new assumption.

### 15.3 Bind first, challenge later, preserve replay determinism

The proposed `VerificationPlanV1` and `VerificationEvidenceV1` are new schemas, not names of existing wire objects. They bind `chain_genesis`, `ruleset`, class/program/artifact roots, job/input and initial/final state roots, trace/constraint/material commitments, full semantic context, segment directory, suite version and security parameters. A claim cannot substitute a different output, expert, witness or plan after learning a challenge.

1. Carry the claim and required initial commitments, reserve its exposure, and bind the immutable `challenge_policy_id` of [RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol).
2. Use only that protocol's qualifying future PALW work, canonical collection/lock, seed derivation and field/query sampler. RFC11 defines no independent seed formula. Source validity must be independent of the consuming claim; a header-valid attempt, heartbeat/BASE-0/EXEC hash or committee signature is not replacement entropy.
3. Each GKR/sum-check round fixes the prover's message **before that round's challenge**, under the policy-pinned staged-beacon or transcript-bound Fiat–Shamir mode in RFC07 VI.6. No all-future-challenges seed, prover-selected mode switch or unbounded transcript retry is permitted; the transform's own security review remains necessary.
4. Make roots/openings/transcripts available before their deadlines. Seats verify assigned relations or the aggregate proof and file positive signed receipts bound to evidence and suite. The baseline retains a versioned, explicitly specified Panel quorum: node replay checks signatures, assignment, roots, timing and lifecycle; it does not secretly replay the LLM. A receipt attests that a seat ran the suite, not that the node checked all its arithmetic. If compact public proof verification is later moved into consensus, meter it separately and define the new acceptance rule.
5. Canonical bytes and fixed transcript challenges yield identical verification/replay decisions. Nodes never use private runtime randomness to decide consensus. Reorg/IBD/pruning restore the same roots, seed derivation and outcome; a new anchor invalidates stale receipts as prescribed by the fence. Bound and account for retries rather than letting producers resample until success.

RFC07 Part VI is the normative construction contract; its source-unpredictability, bias, bootstrap, codec, parameters, transcript transform and replay gates remain open. Commitment opening and Panel tally also need complete implementation evidence. This draft does not prove a deployed beacon secure. §17 extends the same protocol to model onboarding; it does not replace per-claim checks by registration conformance.

### 15.4 Whole-claim error budget and adversary

The plan must provide a reviewed conditional computational-check error `ε_check` for a **false whole claim**, not just one selected product. Include constraint selection, aggregation, polynomial degree, rounds, repetitions, field representation, boundary/memory checks and binding/opening errors. A conservative composition can sum component bounds; no independence is needed for a union bound. A missing constraint has no small error bound. For illustration, at most `2^20` separate matrix relations, each checked three times over a field of size at least `2^61` with fresh independent vectors, give the loose union bound `2^20 × 2^-183 = 2^-163` for that matrix-check component alone. This is neither a complete suite nor its real cost.

**Proposed cryptographic design target:** `ε_check ≤ 2^-128` per committed claim under honest execution of the verifier and the specified challenge/binding assumptions. This is a target to demonstrate and parameterize, not a measured property, a shipped constant, or “128 segments.” If unmet, the suite remains experimental and cannot silently lower the network threshold. The manifest declares the bound; the verifier derives/checks it from the approved suite and dimensions, rather than trusting the declaration.

Separately quantify probability of a compromised accepting Panel, challenge bias/withholding, selective DA and censorship. Quorum signatures do not multiply soundness by the number of seats, especially with shared public challenges; colluding seats can sign without checking. For a stated horizon with `Q` adversarial attempts/reorg retries and bad-assumption event bound `ε_env`, a conservative model is `P(any false acceptance) ≤ min(1, Q·ε_check + ε_env)`. Count off-chain Fiat–Shamir grinding in the applicable proof analysis as well. Do not describe the network as having 128-bit security solely from the algebraic target. Publish assumptions, maximum/estimated exposure horizon, economic incentives and sensitivity to correlated operators.

Full proof security needs analysis, not just successful random fault tests. PoSP-style economic incentives can discourage skipped work but are a separate rational-adversary argument. They do not supply a worst-case single-fault detection theorem.

### 15.5 Final and exact dispute handling

Proposed state flow (wire names and migrations to be specified before implementation):

```text
Committed → ChallengeBound → Checking → ProbabilisticPass → WindowClosed → Final
                                 └─ mismatch / filed challenge → Disputed → exact verdict
                                 └─ missing evidence / deadline → DA or timeout outcome
```

`ProbabilisticPass` requires the complete suite's positive evidence and the specified receipt rule. Silence, missing rounds, absent required constraints or unavailable witnesses never count as pass. Final requires the armed challenge window to close, required DA/retention obligations, and no unresolved accepted dispute. Reward escrow and useful-work maturation follow this lifecycle. Any REAL claim's pre-Final fork-choice weight and aggregate adversarial exposure must remain bounded and drilled under ADR-0069. Revised RFC-0008's EXEC slices contribute zero fork-choice weight before and after Final; their pending reward/collateral/DA/court exposure still needs bounds, with one aggregate root Final and settlement. Sampling receipts must not immediately release an entire session's budget or duplicate slice credit. A single network-versioned policy applies to all claims using this route, not a model-selected confidence level.

A failing random check suspends acceptance and identifies an evidence/relation disagreement. It does **not** by itself convict a miner: the served witness may differ from the committed claim, or a verifier may be faulty. Localize via authenticated matrix tiling or proof/dissection into named TIR/kernel primitive transitions with bounded inputs, weights and entry state. Then the existing compatible terminal court recomputes the exact step; a new primitive requires a versioned court extension, not a generic VM fallback. A GKR mismatch does not automatically identify a false terminal leaf: the plan must supply and test that localization protocol, its total bytes, work and deadline. No emergency end-to-end replay of a multi-terabyte model is an acceptable hidden dependency.

Use the existing distinction between missing material, missed duties and proven false computation. Apply slashing only under its defined evidence/obligation rule; verifier unavailability is not producer arithmetic fraud. Rate-limit bonded disputes and bound simultaneous court exposure so an attacker cannot force unbounded fallback work. Exact fraud proof, dismissal and availability/timeout results must each have deterministic transitions. The protocol accepts the residual chance of undetected invalidity after Final; court does not make it zero, and this proposal does not invent a post-final rollback rule.

### 15.6 Kimi K3 and large-model feasibility gate

The official [Kimi K3 model card](https://huggingface.co/moonshotai/Kimi-K3) describes a 2.8T-parameter multimodal model, KDA/Attention Residuals and a million-token context (consulted 2026-10-06). This makes it a concrete stress target for the selected architecture. Pin its exact source revision, tokenizer, quantization, full modality and context before testing. A text-only or short-context extraction is a separately named partial result. The card is evidence of the target, not evidence of MISAKA compatibility or a successful benchmark.

**Engineering decision:** for MISAKA's intended large-model path, normal verification must use bounded probabilistic/algebraic constraint checks instead of requiring every seat to repeat full inference or every queried segment in full. This is the chosen way to seek affordable Kimi K3-class verification. No theorem establishes that sampling is the only possible architecture, or that parameter count alone makes every replay scheme infeasible. Existing RFC-0007 §II.9 measurements also show that algebra can lose to replay at low bandwidth or on prefill-heavy work; they are not Kimi K3 measurements.

The benchmark report must include producer execution **and** witness/proof generation, cold and amortized weight preprocessing, verifier field work, total proof/opening bytes, network transfer, RAM/VRAM/storage, time to positive receipts and Final, worst localized dispute cost and attack-induced court load. Compare against the same deterministic replay baseline on named hardware for prefill, decode, recurrent state, routing and multimodal paths. Report separate registration, verification-feasibility and mining outcomes. No automatic “all models” claim follows from adding Freivalds/GKR.

### 15.7 Activation tests and implementation boundaries

* **Arithmetic and binding:** independent reference vectors; single incorrect output scalar, modulus-alias error, rounding/carry fault, wrong expert/TopK tie, memory permutation, forged entry state, uncommitted projection, omitted constraint and duplicate work. All relations must be covered; toy-field trials check predicted distributions, while the production error target needs a reviewed proof.
* **Adaptive adversary:** producer fixes outputs after seeing a vector; reused/public sketches; prover learns future sum-check challenges; multiple commitments, abort/retry, beacon withholding/bias, selective availability, correlated seats and reorg reuse. Test rejection or explicitly bound remaining risk.
* **Court and resource bounds:** isolate a late single fault within the largest permitted model/context; prove exact terminal equivalence without whole-model replay; measure total localization bandwidth/work/time and concurrent disputes. Withheld data exercises the DA path rather than an invented arithmetic conviction.
* **Lifecycle and deployment:** old claims finish under old rules; plan/suite enters identities, signatures and fingerprints; independent node/SDK/manifest results and cold IBD agree; Final cannot bypass the window or an active dispute. Introduce a separately named dormant fence (proposed `palw_probabilistic_constraints_v1`), no activation height in this RFC. Compare in shadow mode before allowing reward-bearing acceptance.
* **Acceptance evidence:** retain §§7–14's coverage audit; add actual 9B-8k, validated 2M and Kimi K3 scaling reports for the new route. A conformance replay may be expensive once; normal verification must demonstrate the claimed reduction with all costs included. Unbuilt verifier families and unrun model cases remain open.

### 15.8 Primary sources and what they establish

* [Slalom (2018/2019)](https://arxiv.org/abs/1806.03287): Freivalds-based linear-layer verification, including preprocessing and TEE assumptions. Its throughput results are not MISAKA/Kimi results; MISAKA does not inherit trusted hardware merely by citing it.
* [GKR (2008)](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/) and [SafetyNets (2017)](https://arxiv.org/abs/1706.10268): interactive verification of appropriate circuits. Input access, depth and exact operation encoding remain costs to resolve here.
* [FRI, TR17-134](https://eccc.weizmann.ac.il/report/2017/134/): low-degree proximity checks, a possible ingredient in a computation proof; not standalone evidence that arbitrary trace values follow a model.
* [Celestia DA documentation](https://docs.celestia.org/learn/celestia-101/data-availability/): erasure-coded availability sampling. Available data may still encode an invalid computation.
* [PoSP/spML](https://arxiv.org/html/2405.00295v3): probabilistic challenges, recomputation and economic arbitration assumptions. Its selected validators recompute the function; it does not prove this RFC's sub-execution checker sound.
* [opML (2024)](https://arxiv.org/abs/2401.17555): interactive ML fraud-proof precedent. MISAKA still needs its own binding and bounded localization implementation; a dispute system cannot detect a lie no participant challenges.

### 15.9 Selected research basis: economic challenge, MatMul-first checks and bounded disputes

**Explicit exclusions:** no TEE/enclave/SGX trust assumption for computation validity; no model
execution/fraud-proof VM; no spML-style BFT orchestrator committee, trusted operator PKI or committee
beacon as a protocol authority. These concepts are not alternative implementations or future
dependencies of this route. Use public consensus identity/bond rules, independently specified
post-commit randomness, approved kernel checkers and the existing bounded PALW court. Existing
native EVM and required historical validation are outside this model-VM prohibition.

**2026-10-06 clarification requested by the operator:** “small probabilistic checks” means this
combination of established ideas, not inventing a raw few-segment trust rule. Review comparison
baselines in the following order before changing the acceptance policy; engineering starts with
the matrix checker. These are component precedents, not a theorem for the combined MISAKA protocol.

| Priority / primary comparison | What to use | What does not transfer automatically |
| --- | --- | --- |
| 1. [PoSP/spML v3, 2025](https://arxiv.org/html/2405.00295v3), §§2–3 | Economic challenge probability, verifier selection after commitment, bonded rewards/slashing, arbitration and failure handling | Selected validators recompute `f(x)`; the Nash-equilibrium result has rationality, cost, collusion and accurate-arbitration assumptions. spML also assumes BFT orchestrators/beacon and trusted chain/PKI. This is neither sublinear algebraic-check soundness nor authority to add DNS validators. |
| 2. [opML v2](https://arxiv.org/html/2401.17555v2), §§3, 5–6 | Interactive localization of a disagreement, execution/proving separation and bounded terminal arbitration as a comparison target | Its FPVM is not adopted. MISAKA localizes to an approved tensor/state kernel transition, not an ISA step. A participant must actually discover/file a fault. |
| 3. [Freivalds / Slalom](https://arxiv.org/html/1806.03287v2), Lemma 2.1 / §3 | First implement and measure committed matrix-product checks, batching and fixed-weight preprocessing | No inherited TEE, secret-reuse assumptions, field/quantization choices or reported speedups. Public-challenge openings and MISAKA integer semantics need their own construction. |
| 4. [Celestia DAS](https://docs.celestia.org/learn/celestia-101/data-availability/) | Coded data/commitments make substantial withholding detectable by a few availability queries | DA error amplification concerns missing data, not an incorrect scalar or transition. Merely erasure-coding a wrong trace cannot certify inference. |
| 5. [GKR](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/), [SafetyNets](https://arxiv.org/abs/1706.10268), [FRI](https://eccc.weizmann.ac.il/report/2017/134/) | Reviewed constraint aggregation / interactive proof / proximity components when their complete relation binding and measured cost justify them | Circuit/proof machinery does not authorize a model VM, arbitrary uploaded circuits or omitted nonlinear/state relations. Proximity alone does not prove execution. |

#### Implementation sequence and the measurement that decides it

1. Profile a source-pinned deterministic model: record fractions of execution work **and time**
   for GEMM/GEMV, attention, active-expert products, routing, nonlinear/range and authenticated
   state. Do not assume matrix multiplication dominates every model/device/task just from its brand
   or parameter count. Kimi K3 remains §15.6's measured full-task stress target, not proven support.
2. Implement the §15.2 Freivalds relation as a versioned checker kernel. Fix every matrix and
   material commitment before challenges; authenticate projections. Compare exact multiplication
   with fresh public-vector checks and a separately specified preprocessing variant on identical
   inputs/weights. Measure prefill batched GEMM, single-token GEMV and executed MoE experts separately.
   Include producer witness cost, cold/warm setup, retained sketches, openings, bytes, RAM and
   end-to-end receipt latency. A speedup in multiplication alone is not an end-to-end win.
3. Cover the remaining routing, quantization, nonlinear, memory and boundary relations using
   approved exact small checks or reviewed algebraic/lookup suites. Add GKR-style aggregation only
   with a bound for the **whole committed relation**, including sparse faults and all input openings.
   Add a computation-encoding/FRI route only with a demonstrated constraint-to-encoding reduction.
4. Localize failed checks to compatible bounded exact court with tested worst-case work/bytes/time.
   PoSP-style challenge/reward and timeout analysis must price this new checker/localization cost,
   not copy a full-function replay cost. Differentiate provable fraud, DA failure and verifier faults.
5. Repeat adversarial tests and independent soundness/economic review, then shadow the complete
   registration/Panel/Final path. No checker or Kernel extension is activated by this comparison.

#### Economic deterrence is not the cryptographic error bound

Preserve positive mandatory checks before Final. Optional PoSP-inspired extra audits can measure
cheating incentives and verifier shirking, but cannot replace those checks by “unchallenged = Final”.
If the **only** check is dispatched with probability `p`, then under an ideal honest checker with
conditional miss bound `ε`, a deliberately false claim's miss probability is
`(1-p) + p·ε`; a small dispatch probability cannot yield §15.4's whole-check target. This is distinct
from sampling a random vector on **every** claim, whose algebraic relation has its own conditional
soundness bound. Panel compromise and randomness/DA failures remain separately budgeted.

Publish the actual probability of detection **and successful enforceable conviction**, available
collateral, saved compute/proof cost, maximum external/reorg gain, checker participation costs,
fees/rewards, collusion/Sybil assumptions, and dispute backlog. As a screening requirement, expected
enforceable loss must exceed the bounded gain from fraud; that inequality alone is not a Nash-
equilibrium proof for MISAKA. Include RFC08's REAL-root exposure, pending EXEC work reward/collateral/DA/court exposure,
and RFC12's settlement risk. EXEC slice fork-choice weight is zero under revised RFC08; that does not make its other exposure zero. PoSP's paper “validators” map to computation checkers/Panel
seats here, never a new DNS attestation/precommit/finality authority.

## 16. Model extensibility without a VM (2026-10-06 revision)

### 16.1 Fixed policy, changed extension mechanism

Under [ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md),
the preferred large-model path is:

`pinned model → active kernel + declarative VerificationPlan → committed encoded constraints →`
`small probabilistic checks → positive receipts / DA / closed window → Final`.

Disagreement still invokes bounded exact court; routine full execution/segment replay is not added.
“Encoding” means a sound relation-preserving construction over all relevant computation, boundaries
and memory, not just erasure-coded trace storage. The miner/prover still performs the large work.
Whole-claim sparse-error soundness, post-commit randomness and the conditional proposed `2^-128`
target in §15 stay in force. Static bounds/identity checks remain deterministic.

The removed path is `unsupported model → BVM/GVM/Universal VM`. It is replaced by
`unsupported relation → precise missing kernel capability → reviewed versioned upgrade → retry`.
The full kernel descriptor, class binding and activation design is [RFC05 §§K.0–K.8](0005-palw-ml-vm.md).

### 16.2 Registration outcomes, independently of hardware

| Finding | Outcome / next step |
| --- | --- |
| Whole task fits active kernel relations and all plan/admission bounds | `ELIGIBLE_AT(...)` after stateful admission; actual inclusion remains subject to §13.4 |
| Source/format frontend missing but target semantics exist | `FRONTEND_REQUIRED`; off-chain importer/lowerer and reference tests, no semantic node upgrade |
| Missing semantic primitive, verifier relation, memory rule or terminal court | `KERNEL_EXTENSION_REQUIRED` with family id, offending relation and required/available bounds; no successful registration under unknown semantics |
| Matching descriptor exists in code but is not activated | `KERNEL_NOT_ACTIVE` plus schedule/proposal status; no automatic activation by registration |
| Finite declared producer/seat requirements exceed current operators | Distinct readiness/capacity status; no arbitrary 8 GiB fleet cap on a semantically admissible class |
| Mandatory node/DA/court bounds exceeded, or incomplete constraint coverage | Refuse precisely; more producer CPU does not remove node DoS or missing-proof obligations |
| Missing rights/source material, uncontrolled external input or unbounded computation | Preserve the external/semantic blocker; neither a new kernel nor more CPU manufactures validity |

These are proposed structured outcomes, not already implemented RPC codes. Listing an extension
request is not class admission, and class admission is neither mineability nor market opening.

### 16.3 Extension contract

1. A kernel contains reusable computational families, not per-model-brand allowlists. Plans may
   combine supported families without node updates, but must bind all task/context/quantization and
   input/output semantics. Frontend output and generated constraint circuits are untrusted input.
2. The plan grammar names implemented operators/checkers with finite dimensions, fixed bounded
   state templates and metered composition. It never supplies executable plugins, a guest ISA or
   a custom verifier program. GKR circuit support must be constrained to the approved relation
   constructors and checked against the intended model statement.
3. Every newly permitted family needs a deterministic reference, an independent implementation,
   full-statement error composition, authenticated evidence, bounded localization/terminal court,
   parser/resource limits and adversarial tests. No arbitrary success for unknown kernel/op ids.
4. New-format class/claim ids bind the immutable descriptor and plan. Old class ids and already-bound
   claims keep old rules. New semantics require a new version, not reinterpretation of old hashes.
   Snapshot/IBD and cross-kernel composition follow the same rule as live verification.
5. A SegWit/Taproot-class extension is the design analogy, **not automatic soft-fork compatibility**.
   Use a coordinated fingerprinted schedule unless a concrete valid-history/state/fork-choice
   compatibility proof supports something narrower. Unupgraded nodes cannot count unknown work as
   verified, reward it or continue post-fence full validation by skipping its witness.

### 16.4 Recalculate coverage without fictional VM successes

The existing audit and historical 9B/2M evidence are unchanged by this design choice. No model has
been newly registered by editing these RFCs. Keep the exact pinned all-HF denominator and sampling
method in §§7–10. Assign **one primary current blocker per repository**, with secondary tags:

* `supported_active_kernel`: complete-task on-chain registration evidence under the measured release;
* `frontend_gap`: representable but importer/lowerer missing or untested;
* `kernel_extension_gap`: missing semantics/checker/court or not yet active;
* `resource_or_lifecycle_gap`: semantically covered but required admission/DA/resource/state gate fails;
* `external_gap`: inaccessible/missing/unauthorized source or other external dependency;
* `untested`: no qualifying end-to-end result.

Only the first bucket supplies successes to the existing estimator; the others remain failures.
Models formerly described as “GVM residual” are reclassified by actual kernel/adapter evidence, never
credited because a future extension could theoretically represent them. Thus **there is no new
numeric coverage percentage to report until new trials run**. The reference's 90–95% and 95–99%
figures are not empirical inputs, lower bounds or adopted guarantees. Do not change `D_all` to
“kernel-expressible models” to make the target easier.

Prioritize extensions by measured full-task repository gain, verification/prover/DA costs and audit
effort; publish both before and after results at each activated kernel version. The release gate
remains the all-HF ≥90% one-sided lower bound with source/task/registration evidence, not theoretical
expressiveness, a successful conformance fixture or a model-name list.

### 16.5 Additional completion tests

Test old/new kernel coexistence, absent/not-active/forged ids, cheap but unsound custom checkers,
omitted relation families, cross-kernel replay, activation-boundary claims and rollback/pruning.
Require equivalent node/SDK verdicts without VM dependencies and real 9B-8k/validated long-context
registration plus Final/court/DA results. Report actual proof preparation and small-check costs.
Any reference replay used during development must be labelled conformance work rather than a
hidden prerequisite on every ordinary claim. No arbitrary soft-fork, universal-model support or
performance claim follows from choosing the kernel-only implementation strategy.

## 17. Three-stage model onboarding with post-commit conformance (2026-10-08)

This is a proposed semantic lifecycle for the new route, not a change to current Rust enums, legacy registration,
active classes or network rules. [RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)
alone defines the policy, sources, seed/sampling and interactive transcript rules. RFC05 binds the policy to the
Kernel; RFC02 binds it into new class/plan identity; RFC13 records commitment/evidence and resumable tooling.

```text
Candidate -> StaticAdmitted / RegisteredDormant
          -> ChallengePending -> ConformancePending -> ConformancePassed
          -> G14 + public availability + resources + actual chain eligibility
          -> ActiveRewardable
```

`Registered != Active != Rewardable`. A static registration can finish without waiting for future work; a CLI can
return a durable candidate/commitment id and resume status later. Transaction presence or a local preflight alone
cannot claim chain registration, conformance PASS or reward eligibility. Missing beacon keeps the candidate dormant.

### 17.1 Stage A — Static Admission

Resolve supported active Kernel semantics, all typed relation/constraint coverage, exact-court/localization coverage,
authenticated public-material structure, deterministic class/plan identity and worst-case parse/verification/DA/court
resources. G14 structurally possible is required here; completed public-outside prosecution evidence is required
again at Stage C. Unknown op, uncovered rounding/routing/state relation or missing court returns the precise extension
gap. Beacon sampling cannot authorize it, and a Kernel proposal is not an active descriptor.

Fix artifact, program, tokenizer/input schema, layout, verification-plan, constraint and conformance commitments,
implementation-set root, scope, challenge policy and conditional error/resource profile. New-format class identity
is determined now, before actual beacon evidence, using the versioned RFC02 binding. The canonical accepted
commitment object and its position start the future window; a caller's wall-clock timestamp is insufficient.

### 17.2 Stage B — Beacon Conformance

Apply `MODEL_CONFORMANCE` under the precommitted RFC07 policy. Only independently validated future work from
pre-existing eligible source classes qualifies; candidate X and work relying on X cannot test X. Collect/lock/replay
the source facts exactly as RFC07 specifies. Absence of qualifying sources is `BEACON_UNAVAILABLE`, not success,
and never enables heartbeat/BASE-0/EXEC/hash/committee fallback.

Run the approved independent authenticated checks: tensor/weight ranges, state positions, routing/TopK/boundaries,
pipeline stages, conformance vectors and Freivalds coefficients where the selected relation supports them. Include
all required static/adversarial deterministic tests. Pin reference/independent/optimized implementations before
randomness; retain exact results, openings, scope and transcript. No shared implementation disguised as an
independent oracle and no operator-picked post-beacon fixture list.

PASS is **probabilistic conformance for a declared scope and reviewed fault model**. It does not prove omitted
semantics or replace per-claim whole-statement verification. Derive/check the declared epsilon under the approved
suite rather than trusting manifest assertions. Raw s-of-N tile sampling misses a single bad tile with probability
`1-s/N`; Freivalds inside sampled tiles does not remove that selection term. A missing coverage/soundness argument
is UNKNOWN/ineligible for this route, not an unqualified conformance PASS. Publish measured artifact access,
transfer, verifier/prover work and resource use; a 100GB artifact becoming a few GB of checks is a feasibility target,
not demonstrated by this document. Opening authenticates bytes, not the computation relation by itself.

Changes to artifact/layout/plan/constraints/scope/implementation invalidate evidence and require a new commitment
before a new future window, subject to counted retries. A failed check initiates authenticated localization and the
applicable exact/DA outcome; it does not itself convict or slash. Preserve failed and aborted records.

### 17.3 Stage C — Active Eligibility

ConformancePassed additionally needs complete semantic/constraint/court coverage, RFC14/G14 fresh-public-bond
prosecution, class-bound public download/DA/retention, funded bond/liability, bounded resource profile, active
Kernel/challenge policy, challenge lifecycle and the actual chain carrier/admission path. Local conformance, a Panel
vote, register transaction or successful market opening cannot replace any of those conditions. Source fidelity,
market listing, operator readiness and active reward eligibility remain separately labelled.

```text
ActiveRewardable = SemanticComplete AND ConstraintComplete AND ChallengeComplete
                  AND CourtComplete AND PublicMaterialComplete AND G14Complete
                  AND ResourceBounded AND ConformancePassed AND ActiveChainEligibility
```

Future randomness provides ChallengeComplete only when RFC07's source/security/implementation gates pass. It
does not supply the other predicates. A conformance pass does not activate a Kernel extension, remove Panel rules
or certify a future claim. Whole-claim Final, one-use work/reward and all present exposure caps still apply.
Reorg rolls back dependent challenge/conformance/activation state under RFC07's branch-relative lock policy.

### 17.4 Kernel extension and release evidence

Kernel authors first satisfy semantics, constraint coverage, court, public-material and resource completeness.
Then freeze implementation revisions and differential-vector scope, commit `KERNEL_CONFORMANCE` under RFC07,
and compare reference, independent implementation and optimized backend on canonical selected cases. Tests can
reduce fixture-selection bias; they cannot prove the statement includes every required relation or a Kernel sound.
Unknown semantics await ADR0172's coordinated extension review, shadow validation and explicit activation.

Test dormant asynchronous registration, missing/biased/nonqualifying sources, candidate self-source, changed roots,
policy/suite substitution, honest/invalid conformance, adaptive GKR messages, retries, no-private-state outsider
reconstruction/localization/conviction, reorg/restart/IBD/pruning and existing liveness under useful-work scarcity.
All new protocol gates remain open. Preserve existing 9B/2M/HF evidence and coverage denominators; these added
requirements do not convert untested models or dormant proposals into registered/active successes.

## Mission alignment amendment — 2026-10-07

§13、§15、§16のVerificationPlan/kernel admission、9B-8k/long-context/全taskのcoverageに、fresh non-seat public-bond prosecutionの実測を追加する。kernel-only certificateや正直なFinalだけを成功と数えない。非公開weights/input/state、FOLD prefixやfused preimageが必要なprofileは、公開取得または承認済み公開証明経路が完結するまで新しい報酬対象にしない。HF coverage、ε_check、source fidelityとこのgateは別の指標である。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。
