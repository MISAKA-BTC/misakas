# RFC07 implementation checkpoint: direct detection and objective court

Updated 2026-10-15 after the permanent user decision in [PRINCIPLES §8](../../PRINCIPLES.md#8-モデル計算の暗号学的証明を使用しない2026-10-15ユーザー恒久決定). This file records source implementation and remaining consumer work. These changes have **not been compiled or executed**. Existing test/node artifacts do not validate this source revision. All implementation precedes the consolidated validation phase.

## Permanent boundary

MISAKA does not use cryptographic proofs of LLM/model computation correctness. ZK, SNARK, STARK, zkVM, zkML, PCS-based succinct validity arguments, GKR, producer-supplied sum-check validity transcripts, recursive validity proofs and proof aggregation are prohibited in implementation, design and future research plans. Earlier proposals in this checkpoint are withdrawn; their commits are history, not an adoption roadmap. The circuit/R1CS development is removed rather than carried as a remaining implementation task.

Permitted checking consists of direct randomized algebraic checks over authenticated operands, sampled reexecution, deterministic native reexecution, public localization and deterministic court. Hashes, Merkle paths, signatures and beacons authenticate identity/material/approvers and derive randomness. They do not prove the calculation is correct. A court filing supplies concrete authenticated operands for deterministic adjudication; a failed random check or a quorum is not itself a conviction.

Consensus credit uses approval by selected verifiers under explicit honest-verifier assumptions. Report the assumed honest fraction, stake/weight, operator correlation, actual participation, available model/material, deadlines and collection conditions. A nominal 128-bit conditional arithmetic bound does not establish whole-model cryptographic soundness or the existence of an honest verifier. Unknown observations remain UNKNOWN; production numbers are decided after validation, not invented by a private fixture.

## Paid ClaimVerification consumers

The new signed class/job/producer enrollment is prospective. A sponsor cannot retrofit a seed or expiry onto an already existing ordinary claim. Canonical native class/job/work/producer commitments, future paid source locks, actual checker calls, receipts and Final have their own versioned contract. Old V1 wire bytes, seeds, receipts and policy identities are not reinterpreted as the new path.

Native targets are Program, Pipeline, Segmented and Spec. Spec includes memory, retrieval and composite consumers. Compiled scope is derived from the actual accepted native class and plan, including pipeline stages, memory steps and composite components. Class/job/producer authority, finite stream bounds, signed lifetime admission and cumulative counters belong to the enrollment/round lifecycle. Retry, pause, restart and reorg cannot silently reset the declared opportunity budget. Existing source opt-ins cannot be attached retrospectively to the same paid work.

The full-material V3 checker reads and recomputes the native relations. Names containing `proof`, `opening` or `transcript` are retained in frozen historical wire grammar; they do not authorize a computation-validity argument. Repeated audit rounds bind the previous material/check record and a paid future source. The legacy `TranscriptBoundFiatShamir` mode name describes audit-history/seed binding only. No producer polynomial message is accepted as proof of computation.

The V4 contract has its own dormant fence and Params identity/mirror, separate from V3. The earlier V3 private profile cannot silently admit a different V4 semantic contract. Public presets remain disabled. A versioned private fixture is mechanics evidence only, never independent review or economic approval.

## Direct relation inventory and retired tags

`misaka/palw/kernel/src/claim_compact_v1.rs` now emits only direct detection checks or `ExactRetained` native execution:

| Relation family / operation | Execution |
| --- | --- |
| DenseMatrix / MatMul | Direct seeded Freivalds over actual authenticated operands, including native CRT/integer-span constraints |
| ExactArithmetic / Add, Sub | Direct random-point equality check when explicitly selected; otherwise exact |
| ExactArithmetic / Mul, ReduceSum, Cast | Deterministic native recomputation |
| Structure | Exact native index/copy relations |
| QuantRange | Exact native rounding, saturation and range semantics |
| Nonlinear | Exact registered fixed-iteration native semantics |
| Selection | Exact comparison, TopK, gather and routing |
| RecurrentState | Exact state/history continuity and native court |
| MediaPipeline | Exact stage edges, external/random inputs and boundary semantics |

`SumCheckHadamardV1 = 2` and `SumCheckReduceSumV1 = 3` remain frozen Borsh identities for historical decoding/rejection. Their prover, Fiat–Shamir round generator and sum-check verifier implementation are deleted. New compilation keeps Mul/ReduceSum exact. A manually injected historical tag is rejected by generation, verification, contract coverage and native V4 record validation; it receives zero conditional detection bits even in scalar/zero-round cases. Native V4 accepts only permitted direct-check tags with empty producer payloads. Neither an old tag nor a review tuple can override the prohibition.

Native target coverage is rederived from the registered class. It includes all component executions and exact boundaries; an exhaustive inventory does not imply every relation is probabilistic or inexpensive. Full operand reads, exact nonlinear/routing work and localization costs remain explicit. The conditional error union counts all direct randomized checks, positions, stages, steps, retries, grinding and lifetime opportunities. Exact checks contribute no random-check error only when actually executed. Source independence, honest participation and data access are distinct assumptions.

## Paid source and economic authority

The funded source path uses native bond collateral and the shared R budget, strict fee debit, distinct reserve/tick/settlement/refund transitions, retained material and reorg journals. The source operator/key roster and canonical target/context are fixed before entropy. One deposit cannot be reused as authority for arbitrarily many attempts. Counters and enrollment identity must survive cold restart and rollback with their dependent native facts.

Distinct native operator IDs enforce a declared native separation rule, not independent beneficial ownership. Independence/censorship evidence admission binds the observed interval, reviewer and actual source workload. Unknown control, cost or market observations cannot be promoted to a safe default. The external source-independence/censorship measurements and actual profitable service remain unverified observations, not facts generated by code.

Actual income evidence requires the authenticated native payment observation, original entitlement, material receipt, queue, paid block/DAA, exact fee portion and full coalesced coinbase queue output. A `FeePaid` event, promised amount, queue row or opaque digest alone is insufficient. Transaction/block/output verification must come from actual own-node authority. Recovery/retention fees and an already funded escrow are not charged twice. External receipts or market data are never synthesized by private mechanics fixtures.

## Complete provider material codec and prospective handoff

`PalwMaterialCodecV1::TypedKernelV4` is a newly appended signed codec; legacy NativeHeldComplete/KernelPositions/SegmentedParts tags and meanings remain unchanged. The new complete codec covers all four native targets. Required unit membership is rederived from actual admitted class/job/body, not from a provider's claimed inventory:

- Ordinary native node positions and actual stage inputs.
- Memory entry state and subsequent native positions.
- Every retrieval snapshot slice and each selected returned entry, including composite retrieval stages.
- Segmented positions' exact native greedy parts, flattened into the new stage 254 ordinal namespace.
- All immutable artifact parameter instances in canonical program/parameter/layer order, flattened into stage 255. Mutable memory-slot overlays instead use authenticated entry state/write continuity.

Parameter units use their actual native commitment suite: V2 authenticated complete rows or V3 bounded row tiles. These are material-identity openings, not correctness proofs. An oversized V2 row is refused before enrollment/funding; it is never silently retiled under a different root. Segmented parts authenticate the actual segment roots. Full weights are not claimed retained merely because trace positions or snapshot rows exist.

Core `palw_prospective_kernel_material_v4` accepts a producer-signed canonical intent over the pinned checkpoint, actual prior ledger/aux roots, native SaltedCommit, explicit providers, finite commit/expiry window and complete manifest. The material consent is excluded so a future consent hash cannot create a claim/hash cycle. Actual kernel admission derives a local candidate through the dedicated fallible salted replay owner; no arbitrary candidate ClassRow or claim is accepted from a peer. Providers must be Active and have distinct native operator IDs from both each other and the producer. Registered native public keys are captured for transport authentication.

The local checkpoint freshness bound is provider ingress policy, not a consensus Final/lifetime/retention clock. An opaque candidate is preflight material only: it is not native acceptance, a source payment, receipt, settlement or Final. The provider durably publishes the complete bytes before signing the exact funded ordinary-240/recovery-242 contract. The real producer must wait for its own coherent positive native enrollment before sending a dependent claim.

SDK `claim_paid_v2/provider_handoff_v4` converts actual `PaidClaimMaterialWireV2` from all-target producers into those complete units and the exact producer-signed intent. It uses the actual candidate class/claim, authenticates the complete bundle, checks registered signer identity, derives tensor/snapshot construction workspace before allocation, and retains output/workspace/candidate leases through transport. It refuses missing snapshots and mismatched segmented dimensions before allocation/indexing. The private `kernel_paid_producer_v4` caller now connects own candidate API, SDK construction, authenticated provider ingress, positive own accepted enrollment, paid-71 signing, explicit UTXO funding and relay. It persists the signed object and exact transaction before relay and resends the same spend after restart. It does not wait for useful-work Finality before the first useful work: native folds require an accepted prior covenant, and imposing a stronger first-work Finality gate would create a bootstrap cycle. This is source implementation, not an executed end-to-end acceptance result.

## Actual checker acquisition and selected receipt caller

SDK `provider_reader_v4` consumes the authenticated query callback, not a producer-asserted key. Each native position/input, retrieval snapshot slice, immutable parameter row/tile and segmented part is classified against the caller's authenticated actual ledger. Parameters are reconstructed from all canonical rows/tiles and their complete native suite-2/suite-3 commitment is recomputed: individual authenticated rows cannot stand in for agreement of both tensor trees. Native segmented assembly retains withheld placeholders, explicit commitments and hidden leaf hashes; it never invents the producer's withheld values. All four targets use the existing checker, including typed Memory/Retrieval/Composite. A callback/codec/authentication/capacity failure overrides a tentative finding and cannot issue Clean or a receipt.

The old V3/V4 round message binds the original complete `PaidClaimMaterialWireV2` encoding. The reader therefore still requires that explicit canonical opening (and V4's rooted empty-payload direct-check audit record). The new provider manifest is **not** silently substituted for that old signed root. Provider-acquired nodes, inputs, weights, snapshots and segmented assembly feed the check; token identities remain bound to the old opening/job. Removing the redundant legacy opening would require a separately signed versioned round contract and remains a migration decision, not an unsigned optimization.

The private `kernel_paid_checker_v4` caller uses its own explicit checker bond/key, provider endpoint and separate fee payer/UTXOs. Every query obtains fresh own-node generation/scope/provider-key authority. After the actual check, the existing receipt builder requires the randomly assigned live seat and exact enrollment/seed/assignment. Native admission is rehearsed before the receipt is persisted and every relay. A query signature, matching local replay, upload or optimistic lifecycle Final cannot substitute for selected approval. The checker writes a no-replace/fsynced receipt and exact transaction WAL before broadcast; cold retry preserves the same spend and rechecks the current assignment. Its success log reports acceptance of **one selected receipt**, not aggregate quorum or Final. Negative findings currently stop this worker; automatic multi-demand/prosecution carrier submission remains separate from the already available objective court APIs.

Resource reservations distinguish actual decoded unit shape, authentication scratch, complete parameter and tensor-tree construction, segmented assembly and retained returned values. Acquisition reservations remain owned through the checker result's lifetime. This does not establish whole-checker/court/RSS/DB closure: legacy reexecution/fault construction, callback transport replay cost, metadata allocator behavior and real model costs still require the separately audited owner paths and consolidated measurement. Repeated uncached reads count toward explicit total-request/byte/workspace caps; no native security or market value is chosen by these local inputs.

## Optimistic history is not selected approval

`finals_read_v1` preserves historical OPV receipts and settlement labels, but labels them `OptimisticUnapproved` and exports no beacon event. The former OPV-to-`PanelIndependent` conversion is removed. Authenticated payer/salt metadata remains discoverable, yet even distinct producers/payers cannot lock a beacon from optimistic timeouts alone. The prior private bootstrap/conformance tests that relied on OPV sources need a genuine selected-approval source adapter; their old success artifacts do not validate the new policy. EXEC slice/credit consumers must likewise require selected approval before new verification credit; a settlement label alone is insufficient.

## Durable context and allocation ownership

The first durable intrinsic context retains the exact prior canonical route, producer signed intent/signature, captured native key/collateral and request identity. It has explicit separate local encoded-metadata and heap limits; the provider additionally needs an aggregate metadata RAM/DB cap across all contracts. Native owner reservations precede cloning, decode, serialization and atomic sidecar publication. A per-contract cap is not aggregate closure. Full-route metadata is deliberately conservative; it is not advertised as an efficient large-population final layout.

Cold restore first requires a current positive own-native covenant: exact request-derived nonce (network/request/policy/recovery), root, manifest and consent generation, provider's own stored signatures and registered native key. It then re-hashes prior ledger/aux roots, checks producer signature/request identity, replays the actual native salted admission and verifies the complete held bundle under fresh reservations. The prior checkpoint is intrinsic verification context; permanent ancestry is not required. A missing current claim/row never authorizes release or answers. An unaccepted signed offer can remain retention-only until a positive finalized expiry fact. Terminal native receipts drive a separate positive garbage-collection path.

Native derived-class/layout reservation, inner wire decode, tensor/Merkle authentication, snapshot expansion, persistent candidate growth, output buffers and durable metadata are separate allocation terms. No encoded-byte multiplier or spare salted-replay ticket implicitly covers later work. Local capacity refusal must remain distinct from consensus invalidity. The current source ownership interfaces are still being integrated; no compilation, measured memory closure or real provider reopen acceptance has been performed for this revision.

## Remaining implementation and consolidated validation

Validate and finish integration of actual producer → own native API → provider durable ingress/240+242 signatures → own positive native enrollment → claim/round/checker/receipt/Final, including automatic negative-finding court carriers and replacement of OPV-only bootstrap entropy with a genuine selected-approval source adapter. Include Program, Pipeline, Segmented, Memory, Retrieval and Composite, actual parameter retrieval for court, provider restart before enrollment, pruning after claim removal, contract replacement and finalized release. Preserve original work/material ownership across each handoff and never treat upload success as native adoption.

Consolidated validation must cover legacy codecs/default identity, all target/subkind consumers, independent source opt-in collisions and per-attempt funding, same-R reserve/refund, all signed lifetime/counter boundaries, sparse DAA/delayed inclusion, fee/queue/coinbase provenance, crash/reopen, reorg and replay, court material acquisition and resource limits, actual node multiprocess/IBD/pruning operation, and permitted checker costs. Source tests for retired-tag rejection, direct checks and cumulative unions are written but unexecuted. Old test binaries do not contain these changes.

Public activation, external reviews of permitted detection checks, source independence/censorship evidence, real costs/income and model sharing/bond participation remain unverified. Removing prohibited proof mechanisms is compliance with the permanent design decision, not evidence that these operational requirements pass.
