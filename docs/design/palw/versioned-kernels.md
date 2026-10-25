# PALW versioned model and verification kernels

[ADR-0172](../../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) governs this selected design.
This specification does not establish implementation or activation; the [K2 completion matrix](kernel-k2-tir-v1.md) records existing dormant implementations. [ADR-0173](../../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)
requires independent public-verifier prosecution before new profiles become eligible.

## K.0 Decision and scope

**Implement reusable versioned semantic/verification kernels, not PALW-BVM, PALW-GVM or a universal
VM fallback for model registration.** Ordinary model additions are declarative plans over the
active kernel. A new operation that cannot be represented safely requires a reviewed, coordinated
protocol extension comparable in scope to a SegWit/Taproot-class update.

The normal verifier policy remains [RFC11 §15](../../rfc/0011-permissionless-model-and-long-context-onboarding.md)
and [RFC07 Part V](../../rfc/0007-palw-verification-certificates-and-algebraic-checks.md): encode/aggregate all
relevant execution constraints so small randomized checks detect false claims at the declared error
bound. A successful suite, required receipts, DA and elapsed challenge window permit Final. Do not
replace this with mandatory whole-model or whole-segment replay. Exact bounded court handles disputes.

This is not removal of MISAKA's existing EVM lane or a ban on off-chain software. It cancels this
RFC's new VM workflow/model fallback, guest ISA/syscalls, universal one-step court, Linux profile and
associated VM-dependent EXEC implementation. Existing EVM contracts may continue under their own
rules; an EVM job-precompile roadmap is no longer a prerequisite or selected workaround here.

The research comparators in RFC11 §15.9 do not introduce their original trust architectures:
TEE/enclave validity trust, a model/fraud-proof VM, and a spML-style BFT orchestrator/PKI/committee
beacon are explicitly excluded, not deferred alternatives. Kernel checkers, public bond/identity
rules and separately analysed post-commit randomness carry the selected route.

## K.1 Extension ladder, without an interpreter escape hatch

| Model requirement | Route | Consensus update? |
| --- | --- | --- |
| New weights, dimensions, known graph/state pattern inside active bounds | New artifact and `VerificationPlan` over approved relations | No semantic update; ordinary registration |
| New format/frontend, same exactly representable model semantics | Off-chain pinned adapter + independent conformance | No, if every relation/bound is already supported |
| New combination of implemented operators | Bounded typed composition, dependency coverage and resource/soundness accounting | No, only within the already active grammar |
| Missing operator, memory rule, proof relation, commitment or terminal court | Specify a reusable kernel family/extension and activate it before registration | Yes |
| Missing weights/rights, non-deterministic unavailable oracle, unbounded execution | Report the precise external or semantic blocker | Not solved merely by a kernel release |

A plan cannot upload checker code. No native/WASM/Python plugin, ISA guest, arbitrary syscall,
user-defined gas machine or `UniversalCPU` circuit is accepted as a disguised kernel. The approved
grammar can describe finite tensor graphs, fixed recurrence/selection templates and authenticated
state accesses; its parse/check cost and semantics are specified and metered. A declarative description
does not automatically authorize arbitrary control flow. Novel templates still require review.

## K.2 Kernel descriptor and class binding (proposed schema)

```text
KernelDescriptorV1 {
  kernel_id, version, semantics_digest,
  plan_grammar_id, primitive_set_id, constraint_set_id,
  arithmetic_id, memory_model_id, commitment_suite_id,
  checker_suite_id, challenge_policy_id, court_suite_id,
  resource_schedule_id, soundness_policy_id
}
ModelKernelBindingV1 {
  descriptor_digest, plan_root,
  program_root, artifact_root, tokenizer_or_input_schema_root,
  task_output_schema, context_and_state_policy
}
```

These are specification placeholders, **not allocated ids or implemented Rust types**. The network
schedule authorizes descriptor digests implemented by the binary. Registration selects only an active
descriptor. A binary/code hash identifies an implementation; it neither proves soundness nor makes
downloaded code executable by consensus.

**2026-10-08 challenge-policy binding:** `challenge_policy_id` resolves the immutable digest of
[RFC07 Part VI's PostCommitChallengePolicyV1](../../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol).
It is a required Kernel security parameter, not a local seed preference. Approve `checker_suite_id +
challenge_policy_id + soundness_policy_id` as one tuple: field, sampling, repetitions, source qualification,
commit timing, GKR mode, retry/grinding and reorg assumptions must agree. Reject unknown/mismatched policies;
no per-model weaker confidence setting. Policy changes require a new descriptor/version and coordinated review.
The actual beacon, sampled queries and conformance transcript are later evidence, never part of the pre-beacon Kernel id.

New-format class identity commits to the whole binding using canonical encoding and a new domain.
Legacy class hashes are unchanged and resolve to their historic profile. Claims, receipts, challenges,
proof transcripts, cache keys and DA manifests bind the complete applicable class/kernel identity.
No plan substitution, weakest-suite negotiation or retroactive class upgrade is allowed. A materially
new profile is a new class/version, subject to the existing line/owner and reward rules.

## K.3 Constraint families, not model-brand kernels

The first proposed probabilistic kernel should expose reusable families:

| Family | Required meaning and verification obligations |
| --- | --- |
| Dense/sparse matrix relations | Correct dimensions and integer/field encoding, committed inputs/weights/outputs; batched Freivalds or approved circuit proof with sparse-error coverage |
| Quantization, range, rounding and nonlinear functions | Exact specified integer semantics, carries/overflow and authenticated lookup/range relations; not a linear-only check |
| Attention, MoE and selection | Score/routing/TopK tie rules, correct expert and gather/scatter bindings; one wrong route cannot be outside the statement |
| Recurrent and dynamic state | Bounded address/step domains, authenticated reads/writes, ordering, initialization, entry/exit continuity and full declared context |
| Media and pipelines | Canonical input/output formats, stage/task semantics and cross-stage commitments; text-only extraction is not full multimodal support |
| Encoding/aggregation | Constraint-to-circuit/encoding binding, degree/length limits, committed oracle openings, all required boundary constraints and the whole-claim soundness composition |

Each family needs a reference definition, legal parameter bounds, soundness argument, executable
checker, exact failure-localization route and bounded court. Kernel activation enables only families
whose complete route passed; a row in this table is not implementation evidence. The finite approved
relation grammar is normative even if the underlying GKR math could represent a more general circuit.

Freivalds/GKR are component choices, not magic constant-cost verifiers. Count input/material access,
proof construction, cold preprocessing, verifier computation/bytes and worst dispute load. An erasure
code only protects availability unless a sound computation-relation construction also binds it.
Raw `s`-of-`N` spot checks still miss a unique bad segment with probability `1 - s/N`.

## K.4 Registration, checking and Final

1. Resolve the pinned source/task and lower into an active semantic kernel. Build artifact and plan
   remotely if desired; do not force the registrant's VPS to reproduce the inference.
2. Node/SDK validate plan grammar, kernel activation, typed composition, complete relation coverage,
   integer semantics and worst-case parse/DA/localization/court budgets. Static admission is
   deterministic; it does not assert that future outputs are already proven correct.
3. Before new-format model Active eligibility, follow [RFC11 §17](../../rfc/0011-permissionless-model-and-long-context-onboarding.md#17-three-stage-model-onboarding-with-post-commit-conformance-2026-10-08):
   Static Admission, fixed candidate commitment, RFC07 Beacon Conformance, then G14/availability/resource eligibility.
   RegisteredDormant may wait asynchronously; a conformance pass neither approves unknown semantics nor activates a Kernel.
4. Execute once, commit output/state/material and encoded evidence before the applicable challenge.
   Bind every interactive round or reviewed transcript transform in order; prevent adaptive response
   selection, challenge reuse and unbounded grinding. Retain RFC07's suite/scope-bound receipt tally.
5. Panel checks the approved relations/proof, not entire selected segments. Positive evidence,
   quorum, DA/retention and no outstanding dispute after the challenge window are required for Final.
   Node replay is deterministic: signatures/commitments/lifecycle plus any explicitly metered
   public-proof verifier, never fresh private randomness or an implicit full LLM replay.
6. Mismatch triggers bounded localization to a named **kernel primitive/state transition**, then exact
   terminal adjudication. New primitives need a matching court; there is no generic ISA-step fallback.
   Missing material follows DA rules, not an invented arithmetic conviction.

The conditional proposed error target stays `ε_check ≤ 2^-128` for a false whole claim under the
specified checker/binding assumptions. It is not a posterior probability, an established production
parameter or total network security. Add the separately stated Panel, randomness, DA, retry and
consensus risks. The court cannot remove false claims that escape all checks and challenges.

### K.4.1 Kernel extension conformance, after completeness (2026-10-08)

A new Kernel relation needs semantics, complete constraint coverage, exact court, authenticated public material
and resource bounds before beacon-backed testing. Unknown operations cannot pass admission by being missed in samples.
Freeze reference/checker/court/independent/optimized implementation revisions and vector-generation scope before
an RFC07 `KERNEL_CONFORMANCE` commitment. Differential vectors may cover matrix relations, rounding, routing/TopK,
state/checkpoint boundaries and pipeline stages; all required deterministic adversarial vectors remain mandatory.
Compare authenticated results across reference, independent implementation and optimized backend; publish scope,
fault model, conditional error assumptions and cost. Random test agreement does not prove Kernel soundness or replace
semantic/court review. A changed implementation requires a new commitment and future source window, with retries counted.
After checks, a Kernel still needs the coordinated shadow/release/activation path of ADR0172. Committee votes and
DNS/BFT beacons supply neither verification challenges nor upgrade authority in this route.

## K.5 Versioning, activation and historical coexistence

K0/K1/K2 in ADR0172 are conceptual layers, not numbers already assigned by consensus. Activate a new
descriptor through proposal, reference and independent implementations, conformance/adversarial tests,
reviewed soundness and resource analysis, shadow comparison and a coordinated locked-in schedule.
This introduces no DNS validator vote or new committee approval gate. The network fingerprint binds
the actual authorized versions, feature dependencies, ceilings and activation rules.

Kernel definitions are append-only in meaning: K2 may remain valid for existing classes while K3
admits new ones. Correctness-preserving optimized executors are allowed; changed semantics,
confidence parameters, memory/commitment or court rules require a new version/fence. A vulnerability
can require a coordinated stop-new-claims/deprecation transition; append-only does not mean an unsafe
profile must earn forever. Specify treatment of already-bound claims, locked funds and evidence;
never rewrite their recorded semantics silently.

Default compatibility is a coordinated consensus upgrade. Bitcoin's
[BIP141](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki) and
[BIP342](https://github.com/bitcoin/bips/blob/master/bip-0342.mediawiki) are extension precedents,
not proof that this proposal is a soft fork. Unknown kernel/opcode **must not mean success** for
PALW work, reward or state transitions. Unsupported nodes advertise capabilities and cannot produce
or claim full validation after the incompatible fence. Only a separate valid-history/state/fork-choice
compatibility proof could justify a particular soft-fork deployment.

Class binding and the accepted schedule choose the kernel; a producer cannot select it by claiming
a future DAA. Explicitly test crossing-boundary accepted transactions, old claims finishing after
activation, reorg, pruning and cold IBD. Cross-kernel pipelines are refused unless an active composition
profile binds each stage and covers the complete soundness/resource envelope. New verification work
does not multiply PWU or reward for an existing computation.

## K.6 RFC04, existing software and historical proposals

RFC04's off-chain training, candidate registry, evaluation and promotion stay independent of a VM.
Automation can run outside consensus with ordinary job/candidate submissions. A new test/checker
family would be a separately reviewed kernel relation, not automatic EXEC/CRITIC/PROOF support.
Promotion's statistical test is distinct from probabilistic correctness of its evaluation claims.

Inventory any dormant BVM/GVM prototypes and actual historical activation before cleanup. This
documentation neither deletes them nor proves they were never active elsewhere. No new implementation
or activation of the withdrawn RFC05 VM programme is authorized. Preserve necessary historical replay
and do not reuse allocated program-kind ids, syscall ids, signature domains or old hashes.

## K.7 Coverage and implementation gates

Keep RFC11's real-source, full-task/all-HF census; a source-complete finite model counts as supported
only when its active kernel route actually passes the required registration tests. Missing kernel
support remains a failure bucket, not “covered by future upgrade.” No 90–99% forecast or update
frequency from the reference attachment is adopted as measured evidence.

Implement the descriptor/plan envelope and a measured set of reusable families, not one kernel per
brand. Required release evidence includes mixed-kernel replay, unknown-version fail-closed behavior,
forged/omitted constraints, wrong field encodings, one-point faults, bad routing/memory/boundaries,
adaptive transcripts, unavailable data, bounded exact disputes and duplicate-slice accounting.
Demonstrate valid large claims reaching Final with small checks, without routine segment replay,
and report producer/proof/verifier/DA costs and real 9B/long-context cases separately. An independent
soundness review is necessary; passing a finite fault corpus is not a proof of the security target.

No production fence or implementation-time estimate is selected here. Kernel-only reduces the
planned general interpreter surface; it does not eliminate new-proof-system engineering or guarantee
that every future model becomes expressible without a consensus update.

## K.8 Precedence

ADR0172 and §§K.0–K.7 replace the former RFC05 recommendation to implement BVM/GVM after RFC04.
References from other drafts to RFC05's VM residual must follow this replacement and treat missing
kernel support explicitly. RFC11 §16 applies this design to registration; RFC07/ADR0171 retain
their probabilistic verification direction. Existing network rules remain the executable authority.

---
