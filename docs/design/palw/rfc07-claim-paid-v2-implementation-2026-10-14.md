# RFC07 prospective paid ClaimVerification V2 implementation checkpoint

## Scope and status

This checkpoint adds production code for Program, Pipeline, Segmented and typed Spec (memory, retrieval, composite) consumers under a new dormant, explicitly signed contract. It is **not acceptance evidence**, public activation, external full128 approval or measured source economics. The user's implementation-first order postpones compilation/tests to the consolidated phase. Rustfmt parsing and whitespace checks alone have run.

The initial mathematical contract is NonInteractive. Every statement/step/stage is committed before its paid beacon. Existing interactive proof semantics do not exist in these compiled checkers: choosing StagedBeacon/Fiat–Shamir in a source-policy enum cannot manufacture them. Class registration and StageMessage therefore refuse those choices explicitly. A separately versioned interactive proof compiler/validator plus per-round future-lock/receipt migration remains implementation work; a type or the existing challenge library's generic transcript transform is not its completion.

## Native authority and compatibility

Inner64 `PaidClaimV2` carries RegisterClass, EnrollJob, Commit, Fund, Lock, Receipt and evidence/review admission subobjects. New verification class/job/enrollment identities bind the mathematical base class/job, the registrant, actual payer, producer, policy and absolute deadlines. Base V1 class/job/claim codecs, seed domains and canonical payment identity remain unchanged. An actual payer may prospectively enroll an unused base job; old claims cannot be enrolled. Thereafter an ordinary V1 reveal of that job is refused. Old receipt/tally paths cannot cover its new verification identity.

Aux113–119 and122–124 hold class/job/producer consent, lock/candidate authority, receipts, counters, evidence, reviews and Final projections. Aux120 is RFC08 monetary material; aux121 and inner65/66 are the root agent's signed Panel lifetime contract. The native gate and A2 inventory name the separately dormant fields. Default presets are None, Some-only params identity and mirror additions are handled by root. Public validation keeps activation refused pending independent evidence.

Producer consent fixes a signed not-before anchor and finite accepted-until interval. Actual inclusion inside that interval never extends checking/challenge/dispute/liability endpoints. The source window is derived from **actual native accepted DAA**, including sparse DAA. The complete source window, native source Final floor, depth and original checking rights must fit the signed checking deadline. OPV prospective enrollment retains at least its old challenge window and changes neither existing OPV claims nor their payment rights.

Class admission is bounded, job consent precedes every reveal, live/total per-job enrollment is bounded, and every native object spends the common block budget. Class resource counts have hard caps. Timely exact fraud/default precedes the closing tick. Court/grace requests must fit signed H. A delayed daemon cannot rebase H. Physical material/payment obligations remain the separately signed RFC08 event-relative covenant, preserving actual P+W and earned cash.

## Seed, money and receipts

Eligible source profiles and native checker bond/operator/weight candidates freeze at producer commitment, before future entropy. The checker snapshot is committed by the V2 subject. Native seats are selected from that snapshot with the verified future seed; joining after entropy cannot introduce a candidate. Distinct operators are enforced in native seat selection, and not represented as proof of independent economic control.

Fund atomically writes the frozen target/fee/cost/sponsor authority and uses the existing strict native prepaid debit (same checkpoint). Existing paid opt-in62, source R, settlement/refund, restart/cold-row projection and reorg journal remain the common financial path. The consumer's original income R is extended to signed H through the same native income identity with no new credit/reservation. A failed nested debit rolls back every authority/row write.

`challenge_seed_v1` takes a re-derived verified paid beacon and the full new subject. The seed is passed directly to the compiled checker; it is never inserted into V1's beacon slot for accidental rehashing. The V2 rejection stream has a finite block budget shared across all scopes/stages and fields, distinct stage/scope/instance/repetition labels, and no biased fallback. Exhaustion is unavailable, not success/fraud.

Program and Pipeline use the seeded mathematical verifier, including decode, authenticated material and exact edges. Typed memory/composite use separately labeled seeded checks per committed step/stage; retrieval remains exact. Segmented uses its existing exact full replay and authenticated root comparisons; diagnostic subsets cannot produce complete coverage. A locally verified success has a private type and cannot be deserialized from an asserted receipt. Native signed receipts have a new identity and bind enrollment/class/assignment/source context/seed/evidence/transcript/signer/deadline. Quorum is distinct native checker operators. A receipt is still seat assurance, not a native recomputation of all tensors.

Standing source failure before consumer decision invalidates the pending lock, removes cached complete coverage and causes neutral timeout; it is not producer fraud. Decided income is not erased by this hook. V2 Final projection preserves the one canonical base job settlement. A paid prospective OPV Final is labeled PaidClaimV2 and excluded from the legacy PanelIndependent source pool.

## Approval/evidence admission

LifetimeReviewScopeV3 commits chain/ruleset/start/end, complete historical suite/policy registry, Q/G/beacon caps and independent review/source/economic roots. A chain-wide rooted counter, admitted event identities and historical suite/policy pairs survive cold restart. Changing policy, ruleset or review id cannot reset Q or silently omit historical authorized pairs. Reorg undoes canonical writes; offline fork/grinding choices remain reviewed explicit external bounds, not a fictional local observation.

Before granting a scoped admission, native code compares the review's raw algebraic bound with the actual registered plan/positions/repetitions and typed step/stage union. The source policy's M127 label/repetition declaration cannot overstate a CRT/checker result. Unknown binding/source/coverage assumptions fail existing full composition. Shipped registries stay empty.

EconomicEvidence requires the exact precompiled release observation identity and reviewer, measured interval/root, actual RFC08 native payment observation, exact original source entitlement/material receipt, prepaid fee terms and source operator. It rehashes a canonical native header, reconstructs the committed transaction Merkle root, matches the actual coinbase transaction and exact aggregate queue output script/value, and identifies the source fee's portion through its authenticated entitlement. A fee promise, FeePaid ledger event, cached transaction id, queue, coalesced recipient amount, missing/historical entitlement or opaque report digest alone does not pass. Costs remain external observations authenticated by the configured review scope; code generates none. IndependenceEvidence similarly requires an exact release identity/external measurement and disjoint observed control identities; distinct keys alone are insufficient.

## SDK/CLI and outstanding acceptance

The SDK exposes rooted all-target class/job/derived-commit/fund builders, native role-checked ML-DSA signing, future source readers, portable public material, all-target verification and private-success receipt generation, and a Final reader usable after the checking interval. `palw-class claim-v2 class|job|commit|fund|lock|verify|sign|inspect|final` uses these typed paths. Drafts/signatures are offline; the existing native submit-object carrier performs actual inclusion. Roots/anchors must be authenticated against the client's selected chain. Root's complete-bootstrap generator must invoke these SDK paths for real signed new claims; that actual CLI/node acceptance is not established by merely adding builders.

Consolidated verification must cover V1 fixed vectors/codec/default identity; all four target forms and typed subkinds; OPV prospective rights; delayed inclusion/sparse DAA; source failure/replay/cold restart/reorg; native UTXO debit/refund and same R; source/economic proof substitution and missing provenance; stream exhaustion/CRT/scope/stage separation; full historical review counter exhaustion/no reset; signed CLI through actual node receipt/Final/material history; and resource/cost/100k/P2P acceptance. External full128 review, source independence/censorship measurements and actual profitable source participation remain evidence gates. No code checkpoint or old test executable can substitute for these.

## Interactive full-material opening contract (source checkpoint, not acceptance)

The new unshipped `PaidClaimClassV2` carries an optional, producer-visible
`CompiledClaimProofPlanV3`. The implemented compiler is explicitly **full-material
opening**, for Program, Pipeline, Segmented, and TypedSpec, including prospective
OPV classes. It checks each complete immutable statement using the actual seeded
math checker (or exact segmented replay). It does not represent a compact sumcheck
compiler, trained-model provenance, random-oracle proof, or independent review.
The compiled plan commits target, interactive mode, finite rounds (1–64), stream
budget and complete opening byte cap (at most 64 MiB). Plan identity, rather than
only the underlying math checker identity, is required in lifetime review scope.

Each `StageMessage` is signed by the original enrolled producer. It fixes the
canonical material root, round index and prior completed transcript. The signed
absolute lifetime remains unchanged. Round zero has no prior transcript; later
rounds require complete previous quorum. Staged mode derives a new source context
from this exact message at its native acceptance DAA, opens its seal window
strictly later, and requires a new sponsor-owned native fee reserve through the
same strict inner61 funding path. Source terms, native collateral reserve, source
fees, common ticks/refunds, material holds and journal/reorg use existing V5 source
machinery; there is no second offchain budget. The before-S input filter now
includes authenticated ClaimVerification targets, so a mandatory before-S sponsor
fund can reach that native adapter.

Fiat–Shamir mode requires the initial paid source lock. Every round recomputes the
existing `FiatShamirTranscriptV1` transform from the plan-bound statement, source
anchor/output and **all** earlier messages and challenges; the initial source seed
is not independently reused as every later challenge. Source and query admission
counts are separately charged to the persistent lifetime counter. Neither mode
can reset signed H, choose a new checker snapshot after learning entropy, or retry
by re-enrolling an existing base claim.

Aux123 holds authenticated round rows, lock, transcript and distinct-seat receipt
vector bounded by the existing committee capacity. SDK root readers rederive the
actual paid beacon and round challenge. `bind_opening_v3` authenticates canonical
material against the producer message before any all-target actual verifier can
produce its private complete-result capability. Native `RoundReceipt` validates
assignment, signer, seed, context, entire statement, message, transcript, round
and fixed deadline; partial coverage cannot close a round. Native cold/cache
ledger projection requires **all** signed rounds complete before tally/Final.
Pending round locks lose eligibility if source standing Final fails. Already
terminal monetary rights are not erased. The new Final projection additionally
binds completed round count and last transcript root.

CLI commands include MESSAGE, ROUND-FUND, ROUND-LOCK and ROUND-VERIFY; the latter
executes actual math and opens the exact committed public material. Original V1
wire/codecs remain unchanged; the new, unshipped inner71 (renumbered from 64) sub-enum and class/receipt
formats are versioned integration work. Public presets and approval registries
remain disabled. Compact proof compiler families beyond full-material opening
require separate explicit contracts, implementations and review before they can
be claimed or admitted.

## Private signed Program work generator

`claim_paid_v2/private_bootstrap.rs` starts from caller-pinned native class/job
rows after conformance, authenticates every parameter tensor against registered
commitments, performs real Greedy TIR execution for each generated token and
exports the final trace/evidence/material and salted commit. Its signed enrollment
and fresh-reader receipt builders call the same actual SDK path as user consumers.
The independent kaspad `private_integration/claim_paid_bootstrap.rs` accepts a
bounded request with separately pinned LR/AR and an explicit private producer
card, writes fresh 0600 files, and leaves immutable complete-v1 artifacts intact.
It does not invent inclusion, a paid source lock, quorum, native Final or payment.
The dedicated feature-gated private fixture binary is integrated by the RFC12
owner. Actual inclusion, assigned receipts, new Final and source payment acceptance
remain consolidated verification work after all source changes finish.

No new compiler, SDK or native tests were run for this source checkpoint, following
the requested implementation-first order. Required acceptance coverage includes
both modes over all four target forms and prospective OPV; before-S strict native
funding; zero/late/duplicate/wrong-actor/partial-message refusal; source/fee/material
substitution; all-round rather than first-round closure; missing/altered openings;
source failure; absolute-expiry handling; cold restart; and journal/reorg replay.

The private producer preflight uses `producer_resources.rs`, an exhaustive table
of all 25 current `Prim` variants, before artifact decode or TIR execution. It
counts retained whole-trace i128 buffers, explicitly cloned operands and Hist
prior rows, each primitive's broadcast/index/TopK/matmul/concat scratch, sequential
row/column Merkle levels and copied leaves, commitments, retained encoded artifact,
opening material and simultaneous serialized buffer capacity. `trace_stage_v1`
restores Fixed/Hist from already retained node values, with no hidden producer
state cache. A new primitive forces the exhaustive allocation match to be updated.
Private workspace/material/execution work hard caps are 256 MiB/32 MiB/100M, and
actual job positions are used to rederive execution bounds before repeated Greedy
work. Source-bound numbers are exported as private preflight bounds, never measured
host consumption, network resource approval or source profitability.

## Authenticated lookups and private factory

New unshipped aux115 has 64-byte context authority keys and 65-byte `0x49` plus
enrollment-ID / `0x42` plus base-claim-ID index keys. Aux123 has 64-byte round keys
and 65-byte `0x49` plus staged-context keys. Fixed `PalwPaidClaimIndexV3` rows bind
version, kind, owner and primary target. Native journal creates pointers atomically;
restore rejects missing or substituted pointers. These lookups no longer scan all
retained enrollments/rounds. Cold receipt projection reads only assigned committee
seat keys, avoiding a full receipt scan per claim. Restore counts admissions once
in bounded temporary per-job scratch (at most 65,536 fixed entries), and validates
primary rows against one loaded ledger without cloning that ledger per pointer.
All new index bytes count toward the native inventory; the typed wire preflight
must dispatch the new 65-byte key variants before parsing their bodies.

`private_integration/paid_claim_factory.rs` creates a distinct, exact paid-v3
private configuration from complete-v1, with source/fee/ClaimV2/typed-root/material
and mandatory native budget/reporter/provider dependencies, mirrored from DAA 0.
Whole reward-block B-quantum and one-KAS capital units are explicit mechanics-only
caps, not market parameters or release approval. Every proposal is value checked;
actual source/court timing and prerequisite gates are retained. Only an ephemeral
validation copy makes these specifically unreleased activation fields dormant.
Public startup/presets keep their existing refusal, and the private node compares
its full consensus fingerprint with this exact factory before serving loopback.

`paid_bootstrap.rs` preserves the old complete-v1 exporter and generates a separate
base-material subtree. It verifies identical genesis and actual paid-v3 native
registration scope/signature, re-signs a new fired-policy envelope, derives the
actual artifact, and commits the new paid ruleset in an explicitly versioned
conformance statement. Its manifest never promotes old control/receipt evidence
and reports new native inclusion, conformance, paid work, source lock and Final as
unobserved. Actual acceptance remains pending consolidated verification.

## Compact relation compilers (2026-10-15, round 3)

**Status: source checkpoint.** Not compiled and not tested, following the
implementation-first order. Not reviewed, not wired into `PaidClaimClassV2` rounds,
and not admitted: registration needs the dormant ClaimV2 fence active plus an
approved review tuple, and the shipped registry is empty. This is no PASS.

**What exists.** `misaka/palw/kernel/src/claim_compact_v1.rs` compiles a checked
`VerificationPlanV1` relation by relation (`compile_plan_v1`) into a
`CompactPlanCoverageV1`. Every relation is either compiled to a compact relation
proof or kept on its descriptor's exact checker. A (family, checker) pair outside
the inventory refuses the whole plan, and so does a typed-spec target or an
undeclared grinding budget.

### Relation classes in the shipped plans

K2-TIR-v1…v5 use these (family, checker) pairs:

- DenseMatrix with `FreivaldsM127` / `FreivaldsCrtV2`
- ExactArithmetic, Structure, QuantRange, Nonlinear and Selection with `ExactRecompute`
- RecurrentState with `StateContinuity`
- MediaPipeline with `EdgeRecompute`

A test pins that each of these pairs appears in `COMPACT_RELATION_INVENTORY_V1`.

### Implemented compilers

Each compiler has a distinct identity (`compact_relation_compiler_id_v1`).

**`FreivaldsSeededV1` (`MatMul`)**
- **Moduli:** the plan's `relation_moduli`, so 2^127−1 alone or the CRT prefix.
- **Randomness:** one vector `r` per (repetition, modulus), drawn from the paid round seed's `Freivalds` stream. The weight projection is shared across batch slices.
- **Proof:** a header only.
- **Bound:** `ε ≤ 1/p` per slice and repetition. Instance bits are `t·b₀`.

**`LinearEvaluationV1` (`Add`, `Sub`, numpy broadcast)**
- **Check:** at one random point `ρ` from the `Vector` stream, `ỹ(ρ) = ã(ρ) ± b̃(ρ)` over the zero-padded flattened output.
- **Bound:** `ε ≤ ν/p` by Schwartz–Zippel on the multilinear difference.
- **Scalar case:** with `ν = 0` the identity is exact.

**`SumCheckHadamardV1` (`Mul`, broadcast)**
- **Check:** a sum-check of `Σ_x eq(ρ,x)·a(x)·b(x)` with degree 3 and `ν` rounds. The final claim is checked against `eq(ρ,κ)·ã(κ)·b̃(κ)`, which the verifier evaluates from the operands.
- **Per-repetition bound:** `ε ≤ ν/p + 3ν/p`.
- **Grinding:** round challenges are Fiat–Shamir over the transcript, keyed by the seed. The instance bound is therefore `b₀ − ⌈log₂(4ν + 3Q)⌉`, and repetitions are not credited.

**`SumCheckReduceSumV1` (`ReduceSum{axis}`)**
- **Layout:** the operand is laid out as `[output index][j]`, with `j` padded to `2^ν_j`.
- **Check:** a degree-1 sum-check over `j` of `Ã'(ρ, j)`.
- **Bound:** `ε ≤ ν_o/p + ν_j/p`. The instance bound is `b₀ − ⌈log₂(ν_o + ν_j + Q)⌉`.

### Common rules

- **Moduli and alias argument:** the moduli are the fewest Mersenne primes whose product exceeds the relation's integer error span. The verifier checks every operand and the output against its dtype range, so a nonzero integer error is nonzero modulo one used modulus. The bounds use the smallest such modulus.
- **Proof rejection:** a proof is refused if it carries
  - a non-canonical field element,
  - a wrong round count,
  - another relation or repetition count, or
  - a size above its compiled byte bound.
- **Stream exhaustion:** this yields `Unavailable`, never success or fraud.
- **Fault localization:** `exact_relation_fault_v1` localizes a rejected relation to the first wrong output element for the existing exact court.
- **Commitment binding:**
  - `verify_relation_committed_v1` authenticates operands against the claim's commitments.
  - `compact_proof_root_v1` / `bind_compact_round_message_v1` bind the proofs to a signed round message.
- **No compact proof larger than its material:** an arithmetic relation is compiled only when its proof bound is below its operand bytes. Otherwise it stays exact.

### Accounting

- **Composed bound:** `compact_check_families_v1` gives one `CheckFamilyV1` per compact relation. Each carries its instance bits, with `instances = instances_per_position × max_positions`. `compact_composed_bound_v1` applies the union bound and the binding term.
- **Lifetime term:** `compact_lifetime_failure_term_v1` turns the per-attempt bound into a `CumulativeFailureTermV1` over lifetime admissions. It refuses a budget that already charges `Q`, so `Q` is not counted twice.
- **Contract binding:** `CompactRelationContractV1` binds the coverage root, the claimed bits, the binding bits and per-position proof/work bounds. `check` refuses an unbound coverage, a claimed bit count above what is derived, and any exceeded bound. `suite_id` is the identity a review tuple must name.

### Refused, with reasons (`COMPACT_REFUSED_FAMILIES_V1`)

- **GKR-style layered chain and PCS-backed evaluation:** there is no polynomial commitment. A hash-Merkle tensor commitment cannot open a multilinear evaluation.
- **Lookup arguments (QuantRange, Nonlinear) and permutation/lookup arguments (Selection, routing):** these stay on exact recompute.
- **Compact memory argument:** state continuity stays on wiring plus exact recompute.
- **Staged-beacon sum-check:** one beacon per round is not compiled.
- **Typed-spec targets:** there is no plan relation map.

`CompiledClaimProofPlanV3::validate` refuses a compact relation compiler id by name (`COMPACT_RELATION_NOT_A_V3_PLAN_V1`). It never reads such an id as a full-material plan. The family-level stub contract keeps refusing.

### Honest limits

- **Reads:** verifier reads are not reduced below the operands' size.
- **Message size:** the gain is the round message, with an explicit per-relation error.
- **Elementwise error:** for elementwise relations the compact check adds an error term where exact recompute had none. Compiling them is the class's opt-in (`compact_exact_arithmetic`).
- **Fiat–Shamir review:** the transform's grinding bound is a declared `Q`, which needs external review.

### Required next

1. Compile all the new code.
2. Run the module tests:
   - completeness on honest products and arithmetic, including the CRT case and broadcasting;
   - tampered outputs caught;
   - forged and non-canonical transcripts refused;
   - bound arithmetic at fixed points;
   - the composition and lifetime terms;
   - refusal of unknown pairs and unimplemented families;
   - a shipped fixture plan fully covered, with registration refused as unreviewed.
3. External review of the bounds and the transform.
4. A PCS/GKR suite.
5. Wiring into paid rounds and receipts.


## All-target private producer/exporter (2026-10-15 integration workspace)

Source only; no build, test or node run has been performed for this checkpoint.
`claim_paid_v2/all_target_bootstrap.rs` adds real generation for Pipeline,
Segmented v4 decoder and v5 encoder, and every TypedSpec form (Memory, Retrieval,
Composite). Program continues through its existing implementation. Artifact
parameters must match the registered native commitments (v3 tensor commitments
for segmented classes); tiled prompts are authenticated against the job root;
retrieval snapshots are rebuilt against the registered item count, dimensions,
slice size and Merkle root. Memory uses the chain-authenticated current line head
and actual step StateWrite outputs. Composite uses actual preceding generated
ids, payloads and last logits to construct later stages. Each result carries the
same native salted commit, public material, producer-signed enrollment and rooted
receipt builder as Program. This extends generation and adapters, and does not
assert that these paths have been accepted by a live node.

Every stage is bounded before its TIR execution. Bounds include retained traces,
external/random input copies, aggregate composite/memory steps, snapshot sort and
Merkle data, encoded artifact, public material and serialization. The private
hard caps remain 256 MiB workspace, 32 MiB material and 100M execution work;
these are fixture refusal bounds, not measured network policy values. The
exporter adds the entire restored request/route and file buffer to the workspace
bound before running a producer. An oversized registered class is refused by this
finite private generator even when a smaller input might fit.

The feature-only private exporter retains the old Program request codec V3 and
accepts `PrivatePaidNativeRequestV4` for all targets. V4 has the same independently
pinned LR/AR, native class/job/enrollment terms and private signing card, plus
stage-indexed parameters, a tiled prompt and public snapshot items. The existing
`--request` invocation dispatches by the leading version. The manifest names the
request version and native commit kind; the work file name stays
`program-work-v3.borsh` for existing consumers, and its salted commit discriminant
identifies the actual target. No file is an inclusion, source payment or Final
receipt. Before Borsh allocates collections, the exact request schema scanner
checks the header, every ledger/aux row using the native row codec scanner,
canonical sorted map keys, tensor rank/product/dtype byte width, bounded snapshot
item/key/payload counts, exact EOF and the combined decode heap. A 64 MiB input
file bound is checked independently. Program V3 requests retain their layout and
are normalized to the same bounded all-target input contract.

Deferred acceptance source must cover honest work and an altered opening for
Program, Pipeline, segmented decoder/encoder, Memory, Retrieval and Composite;
both interactive modes; actual source funding/lock and assigned receipts; native
Final/payment; restart and journal/reorg. Pure schema/resource tests added here do
not substitute for those end-to-end cases. Compact family compilation and its
native consensus contract remain a separate open implementation boundary; the
all-target producer does not turn a full-material V3 plan into a compact proof.


The native source opt-in adapter now resolves Program, Pipeline, Tiled/Segmented
and TypedSpec jobs through one `beacon_source_job_profile_v1` function at both
admission and cold restore. The old two-form lookup rejected otherwise valid
salted typed/segmented work, despite the shared native claim admission and Final
adapter already supporting those forms. The resolver grants no eligibility:
the frozen target, candidate exclusion, OPV eligibility, exact live ordinary seal,
registered payer, distinct per-target reservation and concurrency/horizon rules
still apply. No ordinary seal acquires a source deposit implicitly. The added
unrun typed-retrieval source test exercises real registration/job/retrieval,
two target reservations against one ordinary seal, duplicate refusal, cold row
rebuild, actual salted reveal, native Final and post-horizon refunds. Separate
funded v5 fee collection and native-process acceptance remain required validation.
