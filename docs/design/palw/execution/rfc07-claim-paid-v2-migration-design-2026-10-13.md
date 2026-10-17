# RFC07 ClaimVerification paid consumer migration design — 2026-10-13

This is a read-only code audit and implementation proposal. It adds no production module, wire allocation, activation, approval registry entry or acceptance claim. All existing class/job/claim/receipt codecs and accepted behavior remain unchanged. The first implementation slice below is not completion of every ClaimVerification adapter.

## Actual current consumer

| Boundary | Current code | Migration implication |
| --- | --- | --- |
| Committed subject | `kernel/src/claim_subject.rs::claim_challenge_subject` derives Program, Pipeline, Segmented and Spec subjects. Search finds only test callers. | A declared subject is not a production seeded verifier. V2 must invoke it or an explicitly versioned equivalent in the actual consumer. |
| Public claim data | `kernel/src/ledger.rs::commit_claim`, `public_record`, pipeline equivalent publish `beacon = [0;64]`. | The current route does not use the legacy block-hash beacon. Do not reinterpret that stored field as a funded seed. |
| Kernel randomness | `kernel/src/public.rs::FreshVerifierV1::ctx/check_salted` builds `ChallengeBindingV1`; `verify.rs::verify_scope_v1` reads `binding.seed()`. Outsiders replace its beacon with their own salt. | Bind a new seeded verifier API and proof/receipt contract. Preserve the own-salt exact public prosecution path. |
| Native seat/receipt challenge | `core/src/palw_kernel_route_fold_v1.rs` creates an assignment immediately on Program `ClaimCommitted`; `palw_kernel_route_v1.rs::palw_kernel_interim_challenge_v1` and lottery are hashes of claim ID alone. | Existing tag 111 receipts attest an INTERIM, grindable challenge. They cannot authorize a V2 pass, even when another object prepays sources. |
| Native licence | `apply_kernel_receipt_v1` admits signed V1 receipts, counts per-segment coverage and calls `ledger.apply_panel_tally`; OPV rejects any Panel tally. | V2 requires its own signed receipt facts, tally and native pass gate. A signature is an admitted assertion, not proof that the node reran the relations. |
| Lifecycle | `ledger.admit` starts Checking at commitment DAA (or OPV Challengeable), marks retention met, and `tick` finalizes/pays from job escrow. | Waiting for future funded entropy needs a versioned committed/waiting/checking state, not the existing immediate-check clock. An unfunded/unchecked V2 claim must never reach Final. |
| Verification identity | `descriptor.rs` binds challenge policy, checker suite, semantics, field/repetition and resource IDs; `mode.rs` separates class identity and OPV mode. | New seed/receipt semantics require a versioned checker-policy/class contract. Do not silently change a registered old descriptor or convert OPV to Panel verification. |
| Useful-work identity | `opv.rs::final_receipt` derives canonical work from `(class_binding_id, job_id)`. | New verification wrappers must not create another useful-work issuance right. Define one canonical execution identity and shared job-holder/reuse authority before emitting V2 Finals. |

## Three separate signed authorities

1. **Class registrant:** `PaidClaimClassContractV2` fixes the implemented compound checker policy, complete program/plan/artifact binding, underlying execution identity, proof format, fields/repetitions/whole relation coverage and admission/court/material resources. Its class ID uses a new domain and policy root. The compound policy names the existing mathematical suite and the new challenge/stream/receipt/lifecycle contracts separately. It is not an old descriptor with a changed seed field.
2. **Job payer:** `PaidVerificationJobV2` binds the new class/policy, complete job facts, nonce, immutable budget and absolute verification/material deadlines. It is a new signed job, not supplemental authority to spend or reinterpret an existing job/root escrow. New job identity and its relation to the canonical execution identity are explicit. Old `PostJob` objects do not enroll a job.
3. **Claim producer:** `EnrolledClaimCommitmentV2` binds the exact new job/class/policy and all pre-draw input/output/state/trace/manifest commitments, its producer, salted seal and the same finite deadline/resource contract. Its ID and seal domain are V2. It cannot adopt an already committed V1 claim or reseal after observing entropy.

A fourth actor, the **source sponsor**, can sign only its own fixed source fee/capacity/context contract before S. It cannot change any of the above policies, seed, deadlines, claim status or job budget. If payer and sponsor coincide, both authorities must still be present. No implicit pooling or automatic use of the old job/root payer's funds is allowed.

Initial policy registration must reject a mismatch between the class, job and producer enrollments. It must also reject every unsupported checker/field/proof mode; a registrant's signature is not a security approval. New supported-policy identity must enter a Some-only dormant params/ruleset binding or equivalent explicit versioned capability contract, agreed with the Params owner. Adding a descriptor to the historical default `known` set would change `config_root`; do not do that silently.

## Versioned types and independent modules

Proposed new modules, initially independent and unregistered until the integration boundary is authorized:

| Module | Types / responsibility |
| --- | --- |
| `misaka/palw/kernel/src/claim_challenge_v2.rs` | `ClaimCheckerPolicyV2`, `PaidClaimClassContractV2`, `PaidVerificationJobV2`, `EnrolledClaimCommitmentV2`, `ClaimChallengeContractV2`; canonical IDs, supported-shape validation, finite checked clocks and admission bounds. |
| `misaka/palw/kernel/src/claim_verify_v2.rs` | `VerifiedClaimChallengeV2` with private fields, `ClaimPublicRecordV2`, `VerifiedPostCommitClaimEvidenceV2`, V2 bounded sampler and actual relation verifier entry. No public constructor from a caller seed or a signed receipt alone. |
| `misaka/palw/kernel/src/claim_receipt_v2.rs` | `ClaimConstraintReceiptV2`, `AdmittedClaimReceiptV2`, per-scope coverage and once-only duty/round rules. New signing domain; admitted signature/assertion kept distinct from locally verified evidence. |
| `misaka/palw/kernel/src/claim_lifecycle_v2.rs` | Versioned waiting/funded-lock/checking/pass/window/default/dispute/Final/expiry transitions, absolute deadlines and journaled retry/admission counters. |
| `consensus/core/src/palw_claim_source_fee_v2.rs` | Rooted class/job/producer enrollment and source authority, snapshot, cost tuple, canonical work identity and native projection/reader. |
| `consensus/core/src/palw_claim_source_fee_fold_v2.rs` | Native signatures/actors already admitted, atomic sponsor funding, lock/receipt/lifecycle admission and money/budget journal integration. |
| `misaka/palw/sdk/src/claim_source_fee_v2.rs` | Full-root fresh reader; class/job/producer/sponsor builders; proof/receipt producer using the same verified seed/contract and supported math suite. |

The minimum contract fields are version; chain/ruleset; compound checker/class/job/enrollment IDs; canonical execution identity; complete subject/manifest/plan/scope roots; actual accepted block/DAA; paid source policy/context; frozen eligibility and excluded dependency/work IDs; proof and stream/sampler IDs; stage/round ordinal; maximum draw words/retries/subjects; fixed source/court/material/inclusion budgets; every absolute cutoff; and the relevant review-scope ID. Commitments use typed absent roots, never wildcard zero. A scope field is not an assertion of coverage.

For the initial CRT suite, distinguish the paid beacon policy's stream/field identifiers from the mathematical checker's M89/M107/M127 policy. Bind the exact implemented CRT/repetition contract; do not infer 128-bit approval from the source policy's M127 field label. V2 bounded streams must fail exhaustion rather than loop indefinitely or fall back to an unreviewed sampler. V1 stream domains/vectors stay byte-exact.

## Transaction sequence and real consumers

1. New class and job registration validate the prospective signed policy and resources. Reject old jobs/claims and unsupported variants. Before accepting a producer's enrollment, reserve computation Q/B/R/F, relevant proof/court/inclusion and material capacity through the same native accounting path. Record the canonical execution/reuse subject and a persistent admission counter before any entropy.
2. Accept the salted producer commitment after authenticating structure. Freeze the exact ClaimVerification subject, complete eligibility/exclusion snapshot, actual accepted block/DAA and fixed deadlines. The claim waits; it has no interim V1 assignment, pass or Final eligibility. A V1 receipt/tally must explicitly refuse this enrollment.
3. Native source sponsor funding derives one exact target from that authority, checks pre-S position, immutable costs/caps and exact named sponsor, writes source/fee/cost/authority and strictly debits native collateral in one checkpointed transition. Reuse the existing prepaid source escrow, opt-in, recipient R, source collateral, refund, settlement and payout queue contracts. No target-only half transition survives failure.
4. Only the authenticated paid v5 inventory can produce a locked challenge. `challenge_seed_v1(context, V2_subject, verified_beacon)` supplies the shared seed, then a **new** V2 seed domain binds the compound checker contract, enrollment, stage/round and exact scope. The actual V2 checker reads that seed directly. Do not store it in `ChallengeBindingV1.beacon` and accidentally apply the old hash again.
5. A V2 public record contains immutable pre-draw manifest data plus the authenticated contract/lock. V1 manifest data may be reused explicitly as data; serializing a V1 public record or receipt does not create V2 verified evidence. The checker verifies every declared relation/opening for its assigned complete scope. Diagnostic samples never constitute coverage.
6. Native V2 receipt admission verifies its own signature domain, canonical assignment/duty, contract, locked beacon, seed, scope, whole relation/check count and absolute deadline. Its tally controls the V2 pass/Final gate. It must not call the old boolean `apply_panel_tally` as an unchecked shortcut. A node accepting signatures does not label that as a locally rerun mathematical proof.
7. Exact public fraud/binding/decode/default proofs remain directly admissible within the new signed, class-minimum liability contract and are processed before any expiry outcome. No new source fee, ticket, challenge lock or accepting Panel receipt is a prerequisite to prosecute. Expiry must not neutralize a timely open demand or positive proof. Default and arithmetic conviction remain distinct.
8. Final requires the V2 contract's pass/coverage, closed absolute window, material/retention authority and no unresolved demand/dispute. The source H covers all allowed dependent verification/court/material positions. Logical decision clocks and actual payout clocks are separate: sparse ticks cannot extend the signed consumer scope or shorten recipient R retention. Late first processing cannot fabricate an on-time pass/Final. Source fees and job rewards conserve funds once; actual queued/paid R lasts through actual payment + W.
9. All class/job/enrollment, source/cost, lock, round/counter, receipt/tally, material/provider, claim/reuse, budget and payout mutations share the native journal. Restart/reorg/replay restore one coherent generation. Canonical counters reorg; abandoned-fork/offline strategy opportunities require separately reviewed G, not a node-local nonrollback counter or a fresh review-digest reset.

## Finite lifecycle requirement

Do not reuse the Panel implementation's provisional H formula. Define absolute latest source lock, last checker/receipt/draw, challenge window, last demand/proof inclusion, court/default decision, material retention and source liability positions at enrollment, with checked arithmetic and class-derived lower bounds. Any permitted pause/retry consumes the fixed allocation and cannot move those cutoffs. A timely public case takes precedence over a neutral expiry; when its bounded resolution is missing, no favourable Final is allowed. Source material/provider authority must actually cover the promised horizon.

This requires a V2 lifecycle/native final consumer, not just another auxiliary expiry field. Existing `ClaimLifecycleV1` starts checking at inclusion and derives some liability from the actual Final/default tick; blindly reusing it would reproduce the sparse-tick/late-liability gap. Preserve V1 behavior for old claims. Resource and inclusion bounds must be proved for the supported shape; when unknown they refuse or remain a closed approval gate.

## Files requiring coordinated integration

Common changes must wait for the root's production-stable boundary:

- Kernel: `lib.rs`, `route.rs` (new signed inner kinds and size limits), `descriptor.rs` / `plan.rs` (explicit compound checker policy, no old ID mutation), `verify.rs` and `public.rs` (private reusable math/structural helpers plus V2 seeded API), `ledger.rs` / `rows.rs` / `state.rs` (authenticated V2 projection/lifecycle and common money/claims; old bytes retained), `claim_subject.rs`, `opv.rs` (canonical work and assurance projection), `job.rs` / `mode.rs` only if an explicit V2 bridge is chosen.
- Native: `palw_kernel_route_v1.rs` (rooted V2 reader/projection), `palw_kernel_route_fold_v1.rs` (skip interim assignment/old tally, native budgets, shared settlements), source/fee inputs (Claim authority branch), lifecycle ownership table/A2, `palw_state_v2.rs` exports/child module only, coordinated Some-only policy/ruleset identity in Params owner files, signature/acceptance/rehearsal in `consensus/src/pipeline/virtual_processor/processor/{palw_validate.rs,palw.rs}`.
- SDK/transport: SDK new module and `lib.rs`, actual proof/material producer (`kernel_execution.rs` and public-record readers), generic signing/submission path/CLI. Existing rooted row RPC can carry optional aux rows, but pruning/IBD must preserve the complete new authority/counter/failure/material inventory and authenticate it to chain history.

Ledger row numbers 0–31 are occupied; aux tables begin at 32. Do not allocate a new ledger row at 32. Suggested aux 113+ and kernel inner71 (renumbered from 64)+ are **proposals only**, requiring owner/inventory collision checks and allocation. New aux records are optional, absent on old state; a mandatory field appended to `ClaimRowV1`, header or old receipt would break historical bytes. Separate V2 data can preserve old codecs, but the native/SDK projection must validate it; synthetic aliases that bypass class/job identity checks are forbidden. Old nodes must reject the new signed kinds. Existing outer receipt tag 111 cannot be reused with different bytes/signature semantics; a new receipt can ride a newly allocated inner kind of the already signed opaque kernel route, or a separately allocated outer tag agreed with the owner.

## Implementation slices and final completion scope

Initial slice: newly signed PanelLicensed **Program / K2-TIR-v2 / WholeClaim / noninteractive** contract, class/job/producer enrollment, sponsor funding, future paid lock, actual seeded whole-scope checker, V2 receipts/native Final, common money/material/reorg and SDK. This is useful only when those consumers are connected together. Reject unsupported inputs; no new public fence or approved tuple.

The adapter's final completion still includes:

| Target/path | Required migration, not satisfied by Program slice |
| --- | --- |
| Pipeline / K2-TIR-v3 | Every stage program/artifact, edge/R facts and state boundary; stage-domain seeds, material availability, whole-pipeline coverage, native licence/Final and exact edge courts. |
| Segmented / K2-TIR-v4/v5 | Prompt tiles, row/shard/bridge checkpoints, segment roots and range coverage; V2 bounded positions/relations, non-overlapping work/reward accounting, commitment-only courts and provider retention. |
| Spec / K2-TR-v1 | Typed constraint/execution roots, memory-chain/segment continuity, complete supported relation/checker policy, exact typed courts and scoped resource/material bounds. |
| OPV | Retain explicit optimistic assurance/no Panel semantics unless a separately registered new mode/class/job/producer policy enrolls the new route. No supplementation may turn an old unchallenged Final into verified coverage or grant another reward. |
| Interactive / staged / Fiat-Shamir | New per-round messages/contracts and post-message challenges or reviewed transcript-bound transform; all message/round/stage bindings, abort/retry counters and inclusion/material/court budgets. A seed published once cannot generate all future interactive rounds. |
| All supported suites/models | Exact complete/sampled behavior and field/repetition/coverage conditions, total resource/fees, all rejection paths and a full union across current/historical subjects/suites. |

Required acceptance: codec/old-seed KAT and unarmed fingerprint tests; cross-policy/class/job/proof replay refusal; no paid lock before funding/commitment; actual V2 seed caller mutation tests; full relation/coverage refusal tests; same R and strict native debits/refund/repeat; pending proof versus expiry/default precedence; sparse ticks, exhaustion, restart/reorg/migration; authenticated SDK/pruned reads; signed real-process useful-work source/proof/Final/payment acceptance. Independent source/censorship/economic bounds, live lifetime scope enforcement and full-horizon external 128-bit review remain separate release requirements. Empty registries and closed public activation remain unchanged throughout.
