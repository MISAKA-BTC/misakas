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
wire/codecs remain unchanged; the new, unshipped inner64 sub-enum and class/receipt
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
