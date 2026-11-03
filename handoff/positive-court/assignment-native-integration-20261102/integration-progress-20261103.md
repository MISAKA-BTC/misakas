# 2026-11-03 integration progress — RFC-0045 remains incomplete

The active checkout is `codex/positive-court-a0`. The190-test phase1 checkpoint
is committed as `556ff5c29` (based on `c9c93e18b`).
The user requested multiple agents to finish RFC-0045. Three agents handed off
candidates for the native writer, funds/Case and original materials/P/recovery.
All three subsequently stopped with a usage-limit error. Root continues their
work. An uncompiled candidate is not native authority or a completed feature.

The user separately adopted BOTH relay drafts on 2026-11-03:
「両案を採択し、限定実装を進める」. They are now RFC-0046 / ADR-0214.
This authorizes the limited relay implementation, with conditional partial
settlement, and excludes public activation and migration of old funds.

## Integrated behavior and verified boundaries

- Original fixed Assignment/Positive/native preimages, canonical job and source
  quote can be retained and independently compared after a later policy change.
  The Open preimage has its own `PC2AOPEN` format. Actual native control records
  additionally require `PC2ACTRL`, exact pre-event origin/root/delta watermark,
  ordered populated-input digest/count and actual fee. Those hashes authenticate
  data, not model computation or native acceptance by themselves.
- Raw actions are rehearsed and rerun at their actual accepted-object position;
  unpublished A/Case material has rollback watermarks and is staged in the same
  existing RocksDB batch as state/delta/UTXO changes. Old v7 refuses nonempty new
  output rather than dropping it. Actual journal fixtures remain DATA tests.
- Reserved Assignment derivation owns actual typed allocations. Source/Case
  prices and the deterministic work-unit mapping have finite, explicit
  preimages; quotes do not grant future service capacity. The A retention quote
  is stable across eligible inclusion clocks and is not an earned-service fee.
- Native operator identity and work-key Principal remain distinct. The fixture
  uses two actual registration keys; it does not establish work eligibility.
- New P data, draw/vote binding and collateral holds are separate from old
  S/K/V/PublicCheck certification. A purpose-specific WorkerPayment approval
  still needs a fresh genuine private Case Session. Original m/k and clocks are
  not shortened. Refutation/revocation charging is not replaced by a release.
- RFC-0046's exact77-byte op12 /0x82 signing template is implemented. Actual
  transaction crypto tests pass; the strengthened ordinary Full-UTXO regression
  passed on the prior frozen source. This does not create R, a future UTXO,
  custody, an earned invoice, a P decision or an accepted Assignment.

## Completed executions in this snapshot series

| record | result | scope |
| --- | --- | --- |
| `integrated-native-typecheck-tenth.json` | exit0 | contract/Core/Consensus lib and ALL test targets typecheck |
| `integrated-native-testcheck-ninth.json` | exit0 | Consensus lib/all tests, including independent fee-input Full script fixture |
| `integrated-shared-clippy-fourth.json` | exit0 | strict library Clippy for contract/Core/Consensus |
| `integrated-contract-all-second.json` |73 passed | all contract lib/test targets |
| `integrated-core-focused-fourth.json` |80 passed |10 external Core targets: body, fence, funds, clocks, original material, principals, finite resources, P codec/crypto, relay crypto and42 source regressions |

Each controlled run preserves its source-before/source-after maps, raw log
hash, exact command/environment and resource-monitor observations. In each
completed run the source maps are equal. The latest seven passing runs share exactly the same source map. Earlier
failures include a test-only zero attempt-share parameter and a fixed-boundary
width error; both are corrected without relaxing their validation rules.
The failed command target and failed fixture are retained as failures, not
counted as passing suites. Previous baseline141 is not added to these counts.

Native DB/G14/Full execution `integrated-native-db-legacy-relay-first.json`
passed14 tests on its retained source. The first private Core owner execution
passed14 of15 and exposed a fixed-boundary width error: Borsh is496 bytes, not596.
Both A and Case boundary constants/encoders are corrected; the rerun
`integrated-core-owners-second.json` passed16 of16, including the new missing-history
regression. A new external extent/roundtrip regression guards the A envelope.
The corrected-source contract73 + Core external80 + internal16 + Exec read-only replay21 =190 passing
executions. The prior Native14 and old baseline141 are not added. The artifact
audit is `corrected-integration-artifact-audit-20261103.json`. The redundant
Native second run was deliberately SIGTERM-canceled during rebuild, before any
test executed, to begin the next source stage; its execution JSON/log and separate
operator cancellation record are retained and counted as zero passes. Runs use
the unchanged default optimized test profile and normal stack. A process-group RSS
monitor is not an OS hard quota or a measurement of native request soundness.

## Corrected safety finding

Root found that the new raw-action interleave and its rehearsal propagated
`NativeResource` but swallowed `NativeContextUnavailable` as a semantic refusal.
Missing local history must instead abort/defer the transition, or differing
retention could yield different native states. An exact two-callsite fix and a
raw-dispatch regression were prepared under `candidates/root/local-context-fault`.
They are now integrated and the regression passed in the16-test owner run. The completed runs above retain their original frozen source. Public presets
remain None; the actual A/C path still has missing owners and accepts no job.

## Remaining owners; no completion claim

Complete prospective all-branch reservation/service/retention ownership,
genuine archived-history/Remaining/Case factory and op1–7 writer, full P native
flow and objective charge, A/C/all-prover money/due/refund owners, actual R
registration/fee-burn/custody/F/R2 binding and terminal payment, old-profile
behavior across forks, consumer/B=F=R=0 audit, cold restart/reorg/pruned IBD and
separate real-model/process cost measurements remain. The partial A1-memory
and P-index candidates are NOT applied or runtime-verified. In particular the
future global verifier-pool work bound is not solved by an at-A pool count.

`candidate-handoff-20261103.tar.gz` preserves all currently handed-off candidate
files, bases, patches and partial root follow-ups; its JSON manifest pins every
file and the archive hash. It is recovery material, not an implementation or
success artifact. The live candidate directory remains available for the next
integration phase. No RFC-0045, RFC-0046, overall goal or public activation is
marked complete.


## Next source stage (not applied at this checkpoint)

Root candidates add fixed R rows19/20, structural pair/material consistency,
exhaustive NativeHeap/Wire inventories, a separate Some-only relay fence and
fixed-recipe identity, a body14 real allocation owner, explicit signed fee-only
source mode/release rule, and a pure fixed-anchor registration cutoff. None of
these candidates is covered by the190 passing executions yet. Their actual
native fee/custody/F/R2/payment owners remain missing. The namespace/domain
collision audits cover1,450 visible refs; no conflicting implementation match
was found in the recorded source scopes. See `native-owner-order-audit-20261103.md`
for the pre-coinbase fee ordering, original contract routing and accounting-session
requirements that must be connected before native admission can succeed.

`integrated-exec-replay-clippy-first.json` passes strict Clippy for the actual
node-feature integration target. `integrated-exec-replay-test-first.json` passes
21 A6 read-only replay cases, including real op9 literal decoding, absent assigned
environment refusal and actual independent A1 execution. No native A/Case/funds/P
acceptance is asserted by those local fixtures. The source map matches the other
six passing runs exactly.

## Latest relay/source/cutoff stage

The next root candidates are now applied and checked: see
[relay-stage-progress-20261103.md](relay-stage-progress-20261103.md) for the actual
source scope,76 contract +90 Core unique cases, typecheck/strict Clippy, exact
source differences and remaining owners. The historical190 above belongs to
phase1 and is not added to166 from the later stage. Native R/A/C acceptance and
actual payment remain closed.


## Original profile and control-carriage followups

Original relay selection is now fixed in retained Assignment material, with no
later Params reselection. This is committed as `3a2bf5bed`; see
[the scoped record](original-profile-progress-20261103.md) and source/binary
audit. All 93 selected Core cases passed on that frozen source. This count is
not added to earlier stage totals.

The next change adds a concrete Assignment control-history entrypoint to the
private native archive loader, sharing actual original Full validation and
executed-prefix reconstruction with Case history. Its borrowed framing/input
checks have explicit DATA fixtures and do not grant native sessions. See
[the current carriage record](control-carriage-progress-20261103.md). Native all-test-target typechecking and strict Clippy passed. All six cases
in the focused Core material target passed. RFC-0045 remains incomplete.


## Original rules at the continuation entrypoint

The block walk now passes its selected-parent state to the op9–13 transport.
A fixed prefix scan supplies the Assignment lookup key. For a rooted
Assignment, the loader reads its ordinal-zero frame from the actual journal,
checks the original material, and scans the candidate against those retained
limits. It then checks the original accepted carriage and reconstructs the
pre-event state. The raw fold compares the same material reference against the
current Assignment row. Missing history for a rooted Assignment defers the
local transition. Malformed continuation bodies fail the original codec check.

This change uses the existing Assignment transport and ordering fence. It
covers original rule selection when the relay configuration changes while that
lane is active. Continuing a contract after removal of the enclosing lane still
requires transport and ordering integration. Actual funding, Case admission,
retention and P decisions still require their native owners.

Core and Consensus library Clippy and focused test Clippy passed. The first
library check found an incorrect relative module import in the UTXO child. The
second check passed with the corrected module path. Both focused Core targets
passed all ten cases on the same source as the passing Clippy checks. The
artifact audit records both executable hashes and the original failed check.
The native reader still requires an actual accepted Assignment for end-to-end
coverage.


## Native preflight for Assignment and Case funding

Op8 now checks the actual carrier output against the composed UTXO and
poster key. It checks the signed worker amount and total caps before deriving
resources and rejects an outpoint that already backs a native lock. The full
Assignment funding calculator uses the same output check.

For op9 the accepted original Open carriage retains its reconstructed pre-event
state through rehearsal and the final fold. The fold checks the current Open
row and original material reference, authenticates the Result and C payer,
checks the C output and original signed caps, and independently derives the
original job, source catalog, and required relations from the registered
program and original layout. Missing original carriage defers the transition.
A same-block Open would require a separate in-memory history path.

Op8 also runs the existing horizon calculation using the actual selected
Gate C policy. Op9 derives the same horizon at the original admission DAA and
checks its Case funding and source budget minima. Actual global service
reservations, native capacity custody, complete price and liability ownership,
and the atomic A/C acceptance writes remain necessary.

The first library Clippy run refused an unnecessary lazy error construction.
The corrected run and focused test Clippy passed. The funds, original material,
and resource quote targets passed all 16 Core cases on that source. The scoped
artifact audit records the three executable hashes. The native A/C path still
needs end-to-end execution with actual accepted contracts.


## Gate C verifier capacity at Assignment intake

The native carrier resolver reads the signed ancestor's DAA from the accepted
header store after its selected-chain reachability check. Op8 binds that clock
to the signed ancestor and derives the latest possible mature registration from
the original Gate C policy and the exclusive Result deadline.

The census uses the actual intake prefix. It includes every registration that
can mature before the cutoff, including inactive and depleted bonds, and checks
the registry length against the signed work budget before iterating. The cutoff
must strictly precede the ancestor DAA. Normal bond registration uses its actual
accepting DAA, and the selected-chain clock adds only nonnegative advances.
Later registrations, including reuse of a removed outpoint, therefore cannot
enter this date range. Existing bond update writers preserve registration DAA.
This counts remaining potential candidates at intake and does not reconstruct
removed rows from the ancestor snapshot.

Op8 checks the draw workspace against the original physical limits and obtains
a real local allocation lease during preflight. The draw uses the same memory
calculation for its currently eligible population, including the weighted
sort scratch space. Active status, operator exclusion, stake and the existing
weighted draw continue to come from the draw's actual state and seed.

The allocation lease is temporary. Persisted future capacity, a bounded later
registration query, complete snapshot provenance verification, and the pending
fraud/service owners are still required before accepting A. The current draw
still traverses the current bond registry when collecting eligible members.

The first library check passed. Its dependent test check was stopped before
runtime to add the shared weighted-sort scratch space to the workspace bound.
The final Core and Consensus library Clippy and focused test Clippy passed.
All 11 Gate C and P cases passed on the same source. The census case verifies
the bound against the existing live eligibility predicate and checks that the
length guard refuses before scanning. The artifact audit records both binaries.

## Registration candidates retained for the later Gate C draw

The actual op8 preflight constructs the candidate keys from its accepted
prefix and the signed ancestor's resolved DAA. The strict cutoff from the
previous stage excludes registrations after intake, including blocks that
share the anchor DAA. The scan includes inactive bonds and bonds with low
collateral. A bounds two registry passes before scanning and obtains a
logical allocation lease for the exact byte extent. The preflight checks
the cohort bytes and the P workspace together against the applicable
heap ceilings.

The internal reservation custody preimage starts with `PC2ACOH1`, a
little-endian u32 count, and 68-byte native outpoints in strict native key
order. The remainder belongs to the reservation owner. This uses the
existing ReservationCustody slot of `PC2AOPEN`. The private A checkpoint
compares the candidate prefix against the independently derived bytes
and checks the hash of the complete preimage. The private C checkpoint
starts P in the same transaction as Result/Case/Funding/journal writes.

The staged P sweep requires the original material and reservation preimage
for the exact Assignment. P checks the rooted Assignment material,
Case/Result binding, policy, original byte budget and preimage digest
before reading the keys. It looks up one current bond per candidate,
using the same active, commit maturity, collateral and operator
predicates. Removal and retirement therefore affect eligibility, and a
new registration of a reused outpoint has to satisfy the original
commit-time maturity requirement. The roster sort, seed and weighted
draw stay on the existing P path. The cost of these lookups also depends
on native map depth. The shared N sweep still needs its own block-wide
work accounting.

The native dispatch still needs to publish the reservation preimage,
reserve its lifetime storage and replay, and supply it to the P sweep
from the accepted branch. The original cohort also needs to be rebuilt
and compared during historical recovery. The combined replay budget
for the cohort and the remaining owners is still open. These staged
boundaries do not establish native A/C/P success or payment.

Core strict library and all-test-target Clippy passed. The three scoped
Core unit cases passed under the repository opt2 profile and normal stack.
They exercise key framing and native order, preallocation work and byte
bounds, and live eligibility for retained keys. The first Clippy run
failed with a test closure borrow error before executing tests. The
corrected Clippy and runtime runs share one unchanged source map.
The documentation check recorded zero new issues against 43 existing
missing references. See the verifier cohort artifact audit and log manifest.

## Original Gate C candidates on Assignment continuation replay

The continuation loader now resolves the signed anchor against the
original accepting block's actual selected parent. It reads the ancestor
DAA from the native header store after the ancestry check. It then checks
the original reservation custody candidate keys against the real pre-op8
state already reconstructed from the accepted ordinal and retained delta.
This is an exact equality check in native bond key order. It rejects
missing, extra, reordered, duplicate or foreign keys and does not allocate
another cohort. Current-branch registrations are not inputs to this check.

The loader deducts the existing carriage work quote before allowing the
registry pass, with the original per-control rate applied to each scanned
row. The scan bound is checked before inspecting the registry. Missing
original ancestry or headers and divergent retained data abort continuation
loading through the local archive error boundary. The caller must retry
from available canonical data.

This change is on the Node continuation read path. It does not complete
the write-side reservation owner, the independently scheduled P sweep
provider, pruning custody, or full accounting of every historical replay
operation. No native Assigned Result or funds movement is enabled here.

Core and Consensus library Clippy passed with warnings denied. All four
cohort unit tests passed on the same unchanged source map with opt2 and
the normal stack. The replay test exercises different registry states as
fixtures. Actual native A acceptance, reorg, and closed-DB replay of that
new row were outside this test. The documentation check found zero new
issues against the 43 existing missing references.

## Assignment Open material at the write checkpoint

The private op8 install checkpoint now takes the actual Open bytes as its
single source of retained material. It checks the active carrier and original
delta boundary, matches the fixed bundle, consents and signatures to that
carrier, and decodes the same wrapper that will be emitted. It compares the
retained policy and Gate C cohort with the fired intake profile and exact
registry prefix. The A account's initial balances and lock are checked
against the payer's actual composed UTXO and the retained money quote.

The checkpoint constructs the control envelope and journal frame from
those bytes, writes the nonce, Assignment, roles, funding, frame and head,
then emits the record through the existing native material outcome. Failure
rolls back the shared checkpoint. The original frame cap, history cap and
retention/replay quotes cover the control envelope and journal header as
well as the inner Open payload. The Node archive loader applies the same
extent check to the stored record.

The actual dispatch still needs the all-branch service and funds reservation
owners to create and call this install plan. The complete aggregate replay
work quote, actual Open acceptance, BindResult publication and new UTXO
settlement are still open.

The final Core and Consensus library Clippy run passed with warnings
denied. Seven Core material tests passed, including the new full-record
extent checks. Both successful runs share the same source map. The prior
Clippy runs stopped at a borrow error and a boolean-style lint. Neither
executed tests. The documentation check found zero new issues against the
43 existing missing references.

## BindResult and the initial Case journal

The private op9 install checkpoint now checks the current original
Assignment and actual event boundary. The literal op9 fields must match
the Result, service, payer consent and signed Case funding rows. The
separate C outpoint, amount and initial legs are qualified against the
actual composed UTXO and the original A account. The initial Case starts
with empty witness and source-use state.

The checkpoint derives the BindResult frame from the actual carrier and
current Assignment journal head. It writes AssignmentUse using that
frame's identity. The first Case journal record contains the literal
native transaction, the exact pre-op9 boundary and the BindResult header.
Both journals, Case and funding rows and the initial P subject are
published through one native checkpoint. The Case reader checks the
BindResult header against the current rooted AssignmentUse and journal
frame as well as the original Case carrier boundary.

The dispatch still requires a concrete all-branch capacity owner,
fresh reservation and relay/funds custody. The aggregate Assignment
history-byte and replay-work quotes, native acceptance and multi-node
recovery of these new rows remain open. This record linkage does not
establish a Positive pass or a P approval.

Core and Consensus library Clippy passed with warnings denied. Three
Case material unit tests passed on the same source at opt2. The new
case checks the reference and exact BindResult header, including missing,
tampered and other-Assignment material. The docs check found zero new
issues against the 43 known missing references.

## Exact cumulative Assignment journal extents

The native BindResult checkpoint prices the proposed control envelope and
journal header together with all existing rooted Assignment frames before
allocating or mutating. The walk is bounded by the original frame count,
uses exact ordinal keys, and checks the chain and original Open origin.
The total must fit the history and retained-byte caps, plus both reads
under the original replay-byte quote. Open also requires enough replay
bytes for both passes of its complete record.

The RocksDB visitor uses the same rooted extents to price the actual
two-pass walk before reading literal bytes. The longest real record bounds
the per-read workspace. The visitor still independently checks the literal
frame identity and raw extents on each pass. A missing frame is local
unavailability. A malformed or disconnected frame cannot be replayed.

This is a journal bound. It covers the new BindResult writer and the
two-pass reader, while total carrier validation and Case replay work,
the all-branch reservation owner, and the other continuation writers
remain separate unfinished work.

Core and Consensus library Clippy passed with warnings denied. The first
runtime run passed the rooted extent unit test and rejected the Open
fixture's insufficient replay budget. The corrected test explicitly
exercises that rejection and accepts the exact two-read boundary. Seven
external material cases and one rooted extent unit case passed on the
final source. Only the external fixture changed after Clippy. No new
documentation issues were reported against 43 existing missing links.

## Native cancel and expiry events

The raw fold now dispatches op10 and op11 for an existing Open
Assignment. It uses the original material and its cumulative journal
limits. Cancel verifies the actual poster and worker keys and both
literal purpose signatures under the original preserve-earnings policy.
Expiry compares the actual accepting DAA with the original exclusive
Result cutoff. Both operations verify the three original roles and all
joined role/funding pairs, including retired roles. The role and funding
walks use bounded Assignment-specific key ranges.

The raw checkpoint writes the terminal Assignment state, journal frame
and head and retains the literal accepted carrier record together. The
change leaves existing funds, duties, roles, reservations, liabilities
and nonce intact. Cancel and expiry do not authorize payments or release
a lock. Native admission of the original Open, liability/receipt replay
for terminal settlement, funding release, and the end-to-end reorg and
recovery exercises remain open.

Core and Consensus library Clippy passed with warnings denied. The 76
contract cases passed on the same source map, including cancellation
signature contexts, joined accounts, and journal links. The new native
close dispatch was typechecked. No accepted op8-to-op10/op11 block
history was executed. The documentation check found zero new issues
against 43 existing missing references.

## Assignment concurrency at the native intake boundary

The op8 preflight and final install checkpoint now price available
global, poster, worker, prover and class slots against the current
rooted Assignment keys. The census reads only the Assignment key range
and performs at most one exact additional lookup for a matching joined
prover per existing Assignment. It stops at the original finite work
bound or the first cap that leaves no room for a new row. Independent
terminal and retired states continue to occupy their retained slots.
A clock or close event does not itself release capacity.

This uses the existing canonical rows, so a later accepted Open takes
its slot with its own state writes, and native rollback undoes the same
occupancy. The authenticated slot-release and retention owner must
still remove a fully released contract safely. This stage covers count
caps. The all-branch service schedule, actual Gate C draw capacity,
lifetime RAM/DB reservations, funds custody and native op8 execution
remain open.

Core and Consensus library Clippy passed with warnings denied. The 13
contract policy/row regressions passed on the same source. They cover
finite cap configuration and the joined-role/funding schema. The new
native census was typechecked. No accepted native op8 history or
resource release was exercised. No new documentation issues appeared
against the 43 existing missing references.

## Joined prover qualification at the actual op13 prefix

Op13 now checks the original rooted profile and requires either an
unexpired Open Assignment with no Case reference or an active Case
with the exact consumed Case/Result pair. It compares the original
shared attempt cap, cumulative frame/retention extent and the current
role and join counts. Original and retired joined accounts stay in the
counts. The cross-Assignment census now counts a prospective joined
prover using the same native role keys as op8. It does not allocate a
second Assignment, poster, worker or class slot.

The accepted-carrier path verifies the literal join signature against the
current registered prover key, reads that prover's own composed funding
output, checks the signed amount envelope and uses the common purpose
lock check. Both the preflight and the priced funding helper share
these coin and payer checks. No role or lock is written by op13 yet.
Actual cumulative fee/capital and service reservation, relay custody,
journal publication and join recovery remain open.

Core and Consensus library Clippy passed with warnings denied. Thirteen
contract cases passed on the same source map, including join consent and
separate account invariants. The new native preflight was typechecked.
No native op13 acceptance, lock or journal mutation was executed.
The documentation check found zero new issues.

## Initial A relay at the actual accepted prefix

The height-aware lifecycle scanner now routes op14 through its distinct
relay fence and exact fixed-body limits. The existing op0–13 decoders
keep their own codecs. The Full-validated carrier handoff retains the
initial-A target, actual signed ancestor clock and the native accepted
fee remaining after the existing mandatory bond/escrow destruction.
That residual is transported by the real carrier identity.

At the canonical interleave, the initial-A relay qualification checks
the current fired contract and relay scopes, original policy window,
current eligible poster key, refund script and current poster nonce.
It reserves the signature template workspace and verifies the literal
purpose signature and ordinary relay transaction signature. The
independently calculable minimum fee is checked against the residual
and the signed fee ceiling before signature work. No fee is deducted
at this stage.

Case, join and continuation targets still require their separate
original-account owners. The full metadata fee, exact-once allocation
before fee consumers, actual bounded custody and retention, raw DB
publication and binding the real funding output remain unfinished.

Core and Consensus libraries and the focused fence test target passed
Clippy with warnings denied. Seven contract relay tests passed. The
first Clippy run failed because the template workspace quote was moved
twice. The corrected runs used a single frozen source map. The added
fence-boundary case was typechecked. Native relay registration and a
new metadata burn were not exercised.

## Initial A relay outpoint and registration capacity

The current-prefix op14 check reads both authorization and custody keys
for all three purposes of the same funding outpoint. A different signed
package cannot replace an existing pair or use the outpoint under another
purpose. A half pair, ambiguous purpose, or inconsistent retained material
is a local history fault. An identical package also stops at the local
history boundary until native custody and accepted origin can be replayed.
The literal comparison covers terms and both signatures.

For new initial A packages, the canonical authorization range and exact
custody lookups count stored pairs, all pending registrations, the
poster's pending registrations and its initial candidates at the same
nonce. A bound pair still uses storage. An unbound pair occupies pending
capacity until the retirement writer actually removes it. The storage cap
bounds the traversal and leaves room for both new rows. A bounded reverse
custody count detects unmatched rows. Both checks reserve their temporary
workspace against the native heap owner alongside the retained literal
body. The initial A scope uses neither prover nor continuation slots.

This is qualification at the accepted native prefix. Successful replay,
aggregate work and metadata pricing, fee allocation before coinbase and
round fee readers, and the custody/retirement writer remain open. The
round path currently reserves its collectible fee before accepting a
mergeset body, so withholding a charge solely in the later PALW fold
would arrive after both that reservation and UTXO reward validation.
The virtual/template UTXO walk also calls the common fee calculation
without the block-validation carrier context. Native allocation must
reconcile both entry points before any registration can deduct a fee.

Core library Clippy passed with warnings denied. All seven existing
contract relay cases passed on the same source map. The tests exercise
the signed package and row shapes. The newly wired prefix lookup and
census were typechecked; no real op14 acceptance, replay or cap boundary
was exercised. The two combined Core/Consensus Clippy attempts were
stopped when the dependency rebuild reached RocksDB on the low-space
volume. Scoped Core verification completed independently. The
documentation check reported zero new issues against the existing 43.

## Initial relay storage floor and fee qualification

New op14 pricing checks the minimum retained images of the literal
lifecycle payload and the exact pending authorization and custody keys
and values. The database/batch floor includes the 66-byte material
key. It checks the signed frame, retention, DB and batch capacities and
applies the original fee schedule and immutable retention interval.
For an initial A registration the prefix checks this minimum against
both the signed fee ceiling and the actual carrier residual before
scanning registration occupancy. A valid existing package still takes
the original-custody replay boundary before new-registration pricing.

Full metadata quotes and restored authorization rows must also cover
the same extents. Actual writer overhead and replay work can raise the
quote. The contract cases serialize all four targets and both pending
rows, check that the fee matches their exact sizes, and refuse a quote
that falls one byte short in any of the four dimensions.

One body digest can occur in different histories after reorg. The
literal payload under that digest has stable bytes, while its exact
accepted origin is scoped to the canonical registration rows. The
actual retained framing and reader still need to authenticate that
association against accepted carrier and branch history. This stage
does not allocate or burn a fee, write RocksDB material, or admit R.

Final-source Core library Clippy passed with warnings denied and all
eight contract cases passed. The Core integration-test target was
stopped when it began rebuilding RocksDB on the low-space volume; those
fixtures were not run. The verification archive records both source
candidates, all five results and their raw log digests. Documentation
checking found zero new issues against the existing 43.

## Relay raw material in the native UTXO WriteBatch

The consensus commit now carries the already admitted raw action
schedule to the DB writer when the final PALW delta contains a new
relay registration. The writer checks adjacent fresh authorization
and custody mutations, original accepted point and ordinal, actual
lifecycle carrier, and the literal terms and both signatures. It uses
the reserved 0x4c key and stages the exact carrier payload in the
existing batch containing the PALW delta, UTXO diff and acceptance
record. Material staging has no independent commit.

An identical existing payload is reused. Different bytes at the same
digest, missing material, a half pair, or a changed accepting point
refuse the local commit. All checks and write reservations precede the
first relay put. The writer keeps the actual archive lock and allocation
tickets through the enclosing commit and checks actual database quota
for new material. Original branch origin is retained in the canonical
rows and acceptance record rather than in the shared content key.

The native op14 dispatcher still refuses registrations while the
pre-coinbase fee allocator and canonical custody reservation owner are
unconnected. There is no live fee burn or successful RocksDB
registration in this stage. Cold native replay, binding and retirement
still require their actual owners.

Core and Consensus library Clippy passed with warnings denied using
the existing static RocksDB/Snappy archives. Eight relay contract
cases passed on the same source map. These cases exercise the signed
package and storage floor. The newly connected WriteBatch path was
typechecked and did not execute a live op14 commit. The documentation
check reported zero new issues against the existing 43.

## Relay registration charge linked to the native fee sink

The validated carrier now retains an observed fee partition from the
UTXO fee sink. It covers gross fee, mandatory burns, collectible fee
after existing rent and the tip actually credited to the block. The
initial A price check reads the pre-relay collectible amount. Checked
subtraction prevents allocating money already used by an earlier
burn. Refused attempts keep their ordinary fee treatment.

The same-batch RocksDB writer requires the observed debit to equal
the complete quoted registration charge. Its reference binds the
accepted literal material, original quote and fee partition at the
actual accepting origin. This applies whether the literal bytes are
new or already present. A zero debit, mismatched amount or reference
cannot create a stored registration. The added contract regression
covers no debit, underpayment, overpayment, arithmetic conservation
and charge reference separation across accepting branches.

The current fee sink credits the ordinary pre-relay amount. Actual
debit scheduling ahead of Round admission and ordinary coinbase
calculation remains open, along with the canonical custody reservation
and restore owners. No real op14 payment or registration ran.

Core and Consensus library Clippy passed with warnings denied and all
nine relay contract cases passed on the same source. The document
check found zero new issues against the existing 43.

## Bounded relay occupancy reads and full registration storage extents

Initial Assignment relay qualification shares one signed physical
budget across the six outpoint/purpose probes, each authorization
range entry, its exact custody lookup and the reverse custody-key
count. Missing point lookups consume a read. Work in this scan is
the encoded key and value bytes inspected, charged before decode.
Exclusive kind endpoints limit both range walks. No new
authorization gets a slot if either physical limit is exhausted.
Signature work and historical ancestry/replay need their own
capacity owners.

The RocksDB staging check measures both complete encoded canonical
delta entries, including their framing, alongside the literal
material key/value. The registration quote must cover this DB
extent plus the additional batch framing bound. The check runs
even when a different branch left an identical payload on disk,
so local material reuse cannot relax the canonical reservation.
The lower-bound quote alone still cannot stage an actual package.

These checks do not install a fee allocation or create a live
relay registration. The canonical pre-reward allocator and
end-to-end custody recovery are still open.

Core and Consensus library Clippy passed with warnings denied. All ten
contract relay cases passed on the same source map, including atomic
read/work boundary checks. The native occupancy and RocksDB staging
paths were typechecked. Documentation checking found zero new issues
against the existing 43.

## Shared relay preflight and persistence footprint

The minimum charge and full quote now use the same storage extent as
the native WriteBatch gate. It covers the literal payload, both
pending row images, both Borsh delta entry envelopes, the raw
material key and its Put framing, and a four-byte bound for growth of
the existing block delta's value-length field. The RocksDB staging
path measures the actual encoded entries and requires their extent
to equal the preflight calculation. A local identical payload still
uses the same canonical quote.

The four target variants are checked against independently encoded
row images and the fresh delta field layout. Each extent rejects a
quote one byte below its boundary. This measures the marginal
registration footprint. The ordinary whole-block write owner still
reserves its full batch and lifetime. Replay work, actual debit
scheduling and native custody remain separate prerequisites.

All ten contract relay cases and Core/Consensus library Clippy passed
on one frozen source map. The RocksDB path was typechecked. No live
registration or debit was executed. Documentation checking found
zero new issues against the existing 43.

## Archived relay fee partition

The native staging path now persists the actual four-field UTXO fee
partition alongside the literal lifecycle payload, canonical
registration delta and acceptance record. The fixed 32-byte charge
preimage uses the existing 0x4c material namespace under its
independent, origin-bound charge digest. This permits payload reuse
across branches while retaining the observed fee split separately for
each accepting origin. The charge reader rejects malformed partitions
and references that do not match the rooted authorization.

The shared price, storage and batch extent now include the charge
record. Allocation and hard-quota checks include both actual puts.
An identical local record can avoid its put while the canonical
extent and registration charge still use the original bounds. The
regression checks the charge preimage against canonical Borsh and
rejects truncated or arithmetically impossible fee partitions.

This supplies data needed to reconstruct the charge on restart. The
branch-authenticated custody reader, actual pre-reward fee debit, and
live op14 registration remain unverified.

Final-source relay contract tests passed 10/10 and the Core and
Consensus library Clippy checks passed with warnings denied. The
first Clippy run found an unnecessary reference in the fixed-buffer
encoder and was rerun after correction. The native archive path
was typechecked. Documentation checking reported zero new issues
against the existing 43.

## Rooted relay archive check at the accepted carrier handoff

An exact op14 repeat now checks the two rows selected by the actual
selected-parent state. It rechecks the original accepting block's
selected-chain ancestry, header clocks, non-coinbase carrier identity
and exact acceptance ordinal. The archive must contain the identical
literal payload and a valid fee partition matching the original quote
and charge reference. A different signed package stays on the existing
Core refusal path.

The acceptance index reader pins the existing bincode record, applies
the original replay byte bound and walks its fixed-width entries
without decoding its nested vectors. The standalone Rust regression
uses the original store module and repository bincode encoding to
check cross-block ordinals, empty blocks, truncation and invalid
transaction hash framing.

The archive check is a local integrity and lineage check. It still
requires a native custody decision in the final fold before an exact
repeat can become an idempotent success. The current implementation
does not restore the original populated transaction and signature
execution or create a live registration/fee debit.

Final-source Core and Consensus library Clippy passed with warnings
denied. The isolated acceptance ordinal regression passed using the
actual store module and matching cached dependencies. Documentation
checking reported zero new issues against the existing 43.

## Original relay rules and signature replay

The exact-repeat archive path rederives the original Assignment and
relay scopes at the recorded acceptance DAA using native Params. It
compares the historical network, rule identities, policy, fees and
physical profile, and checks the signed anchor against the accepting
block's own selected parent. The two additional lineage reads are
included in the replay bound.

Fresh qualification and archival replay share one bounded verifier for
the refund script, registration-purpose signature and standard op12
settlement-template signature. Fresh qualification uses the current
registered poster key. Archive replay uses the rooted authorization's
original key and keeps its row workspace plus the template workspace
within the stored limit. This authenticates the stored signed package
and original rules. Historical bond qualification, the original outer
op14 transaction's populated inputs, native custody and the actual
pre-reward debit are separate pending owners.

Final-source Core and Consensus Clippy passed with warnings denied,
including the focused Core test target. All four native relay tests
passed. The exercised signature case verifies the original signed
package and refuses independent purpose- and relay-signature
mutations and an insufficient workspace. This is a direct Core
verifier test. The selected-chain archive path was typechecked and
did not run as a full node restart. The documentation check found
zero new issues against the existing 43.

## Historical outer relay carrier and gross fee replay

Exact-repeat validation now opens the original accepted carrier block
at the indexed transaction position and checks its native identity
and literal op14 bytes. It reconstructs the populated inputs with the
same helper as Assignment and Case replay, using the accepted UTXO
diff and earlier accepted creators for intra-acceptance spends. It
reruns ordinary Full transaction validation at the original DAA and
compares the resulting gross fee with the archived partition.

The path charges the stored record extents and read count before
opening the historical body, UTXO diff and any same-acceptance creator
body through their ordinary store owners. It also bounds the two
prefix walks and duplicate-input checks and leases the populated input
copies. These are replay-specific bounds. The existing store owners
retain their whole-record decode and cache behavior.

This replay authenticates the outer carrier and gross fee. The
historical bond and native custody decision, actual debit scheduling,
and end-to-end native relay acceptance remain open.

Final-source Core and Consensus library Clippy passed with warnings
denied. A focused acceptance-store regression closed and reopened a
real RocksDB database, checked the exact byte boundary, and recovered
the stored carrier ordinal using the production store method. The Full
relay replay path was typechecked without a native relay history
execution. Initial verification attempts and their setup failures are
retained. Documentation checking found zero new issues against 43.

## Assignment and Case archive parent commitment

The shared archive reader now checks the reconstructed original selected-parent
state against the accepting header's parent-state commitment, using the same
root recipe as ordinary native block validation. The retained delta record's
first field is the resulting state root. Comparing it with the selected-parent
block hash previously rejected valid history before replaying the event prefix.
The reader still checks the actual accepting delta's block and clock and
requires the exact recorded prefix root after replay.

The isolated production RocksDB store suite passed 36 tests, including a new
test that stores distinct parent-block, parent-state and resulting-state
values, recovers the expected prefix and rejects a substituted prefix root.
Core and Consensus library Clippy passed with warnings denied. This is store
and prefix verification. A live Assignment/Case continuation through the
native VirtualStateProcessor still needs an end-to-end run. The test runner
records, source maps and logs are retained as `assignment-parent-root-*`.

## Initial relay payer at the original acceptance position

Exact-repeat archive verification now walks native selected-parent deltas
backward from the rooted view. It rewinds the payer's eligibility and
Assignment nonce through later writes, then stops at the exact original
authorization and adjacent fresh custody rows in the accepting delta. It
checks the historical payer key and collateral against the normal admission
predicate and checks the historical nonce without consuming it. The original
custody row must be unbound and agree with the immutable fields of the
current row. A later binding requires its own native verification.

Each step charges a bounded store read and wire work before decoding. The
store checks the pinned extent and typed decode heap before allocating.
Missing or pruned deltas and exhausted original replay limits refuse archive
verification. This path depends on retained history. The separate durable
current-account package and custody owner needed for terminal spends after
pruning remain open, as do the actual debit and native registration.

Core and Consensus library Clippy passed with warnings denied. The isolated
production RocksDB store suite passed 36 tests, including exact wire and
heap thresholds and rejection one byte below each. The reverse payer walk
was typechecked. No live native relay history ran through it. The failed
initial typecheck and final execution records are retained.

## Canonical full registration price

Fresh op14 preflight now checks the complete registration price against the
actual available native fee. The quote uses the terms-derived frame and storage
extents and reserves the full original finite profile's replay reads and work,
plus its decoded and heap ceilings. Only the priced resources affect the
amount. The local allocator, the later branch, and a peer usage declaration
cannot vary this quote. The same method is called for the actual RocksDB
staging and when authenticating the archived quote under the original rules.
The staging path continues to measure and check the actual encoded rows and
requires an observed fee partition with the matching debit.

The replay reservation is conservatively priced at the signed profile ceiling
per registration. Contract tests cover all four targets, exact Borsh extents,
equal prices across eligible inclusion clocks, exact signed fee caps and
one-sompi-short rejection. Ten relay contract tests passed.

This still needs the actual fee and custody owners. The shared UTXO walk
reserves the Round budget before applying a merged body and computes the
coinbase from that acceptance. The native Assignment rehearsal currently
occurs after coinbase validation. Charging after this rehearsal would leave
an inconsistent reward or reserve, while charging merely for a decoded
op14 would burn fees for refused registrations. The accepted-prefix decision
and reward accounting must meet before the shared fee sink publishes a
debit, on both the candidate and virtual/template paths.

## Initial relay row writer conditional on the observed debit

The native raw Assignment executor now has the initial op14 row-writing
branch. It uses the current-prefix payer bond/key, nonce and occupancy checks,
verifies both signatures, and requires the original full quote to match the
observed debit. Under the raw checkpoint it writes the fresh authorization
and unbound custody rows consecutively. The custody reference commits to the
literal package, quote, actual charge and original retention clock. These
rows occupy the same pending slots checked on later registrations. The
existing UTXO commit must stage their material and measured quota in the
same batch as the delta and acceptance record.

The row construction is still conditional. The native fee sink currently
records the pre-relay collectible amount as the entire credited tip, so no
positive relay charge or live registration is produced. The shared fee
allocator, exact-repeat/no-effect outcome, F and R2 binding, retirement,
and durable current-slot recovery remain open. The contract test exercises
the row pair with an exact fee partition and rejects zero, a one-sompi
shortfall, an overcharge, and substituted origin/reservation material.

The 11 contract tests passed. Contract, Core and Consensus library Clippy
passed with warnings denied. The Core relay integration target passed strict
Clippy. The native fold and the combined RocksDB/UTXO commit were not run
with an actually debited carrier. The exact commands and log hashes are
retained in the relay-pending-writer verification archive.

## Reconcile observed relay debits with canonical writes

The UTXO fee sink now distinguishes the pre-relay collectible fee from the
tip that feeds rewards. A debit observation requires a matching sealed carrier
and cannot replace an earlier observation. The raw-action handoff rejects a
charged carrier that would otherwise be filtered out. Candidate and virtual
UTXO processing propagate the fee-provenance fault before publication.

The UTXO commit retains the fee evidence even when the PALW fold writes no
relay rows. Before appending any relay material to the RocksDB batch, staging
matches charged carriers one for one, in accepted ordinal order, to fresh
authorization/custody pairs. It rejects an orphan debit, an uncharged pair,
or a repeated/reordered ordinal. The check also runs when the literal records
already exist under their content addresses. Failed staging leaves the native
UTXO/acceptance/coinbase transition unpublished.

The UTXO sink still credits the full pre-relay collectible fee. This change
protects the write boundary for the upcoming allocator and does not create an
actual relay debit. The allocator and native end-to-end funded test remain.

All 11 relay contract tests passed. Contract, Core and Consensus library
Clippy passed with warnings denied. All Consensus test targets typechecked,
including the synthetic missing-seal and duplicate-observation test, which
was not executed. The commands, source map and logs are retained in the
relay-debit-conservation verification archive.

## Exact retry of a pending initial relay

The selected-parent reader passes the original pending reservation and fee
partition to the raw native action only after replaying the original accepted
carrier, scope, payer and both signatures. The writer compares that evidence
against the current-prefix pair and its original charge. A live exact pending
retry with zero new registration debit then has no state or custody effect.
The carrier's ordinary transaction fee is separate. The retry uses the original
physical profile, while a fresh registration still uses the current profile.

An exact duplicate within the accepting block can use an earlier registration
already present in the canonical prefix. The original row must have that
block's exact clock and a strictly earlier accepted ordinal. The final DB
staging still reconciles the first debit with the first fresh pair. A changed
package, stale pending slot, additional debit, or a later-prefix slot change
cannot become an exact retry. Bound packages still require the account-slot
custody owner. Archive replay still requires the retained history and its
original limits.

This connects the conditional pending retry. The native UTXO allocation of
the first registration charge, live new-registration/retry runs, binding,
retirement and recovery after historical pruning remain open.

Strict Contract/Core/Consensus library Clippy and Core test-target checking
passed. The new selected-parent/duplicate-attachment regression typechecked
and was not executed. The initial type error and the stopped library-only
check are retained alongside the final verification.

## Preview an initial relay charge on a disposable native prefix

The new Core fee preview checks the same op14 carrier and accepting point,
current-prefix bond/key and nonce, pending capacity, terms, signatures, and
write resources as final execution. It calls the same row writer on a
disposable native state. That state can be used for the next carrier in
an ordered preview. The preview returns the proposed fee partition. A fresh
registration deducts the full quote from the unallocated native fee, and an
exact pending retry returns the original zero-debit partition. The fee
arithmetic refuses a zero registration charge, a charge above the available
fee, or applying a second charge to a partition.

The final fold still requires the actual observed partition and validates
the exact charge before writing. The native acceptance path has no call
to this preview yet. The next connection must supply the real ordered
accepted prefix and route the result through Round fee admission and reward
calculation before publication, then reconcile it with the final fold.

All 11 contract tests passed, including exact tip, insufficient fee and
second-charge cases. Strict Contract/Core/Consensus library Clippy passed.
The native preview typechecked and did not run in a native block. The
commands, source map and logs are retained in the preview archive.

## Replay each observed debit at its actual accepted prefix

The native accepted-object rehearsal now uses the Core preview for every
op14 carrying a nonzero observed debit. It recovers the fee partition before
the registration charge, reruns the registration against the same folded
prefix used by earlier ordinary and Assignment operations, and compares the
complete proposed partition with the observed one. A matched preview supplies
the next disposable state, including the new pending pair. A mismatch or a
semantic refusal for an already debited carrier aborts the enclosing local
transition. The previous semantic-skip behavior still applies to uncharged
carriers. Final execution retains the original observed action and independently
replays the actual write.

This connects verification to the real ordered rehearsal. The UTXO sink
still needs the fee allocator before Round and coinbase decisions, so no live
charge is generated by this step.

All 11 contract tests and strict Contract/Core/Consensus library Clippy
passed. The contract runtime test also confirms that recovering the input
partition preserves the prior burns and reconstructs the exact observed fee.
The ordered native replay typechecked and was not exercised with a live debit.

## Ordered planning output for the fee owner

The native accepted-object traversal now has an explicit relay planning mode.
It previews unallocated op14 actions at their actual positions among legacy
and other Assignment objects, advancing the disposable state after each valid
registration. This exposes same-prefix occupancy to later carriers. Successful
fresh debits retain their full proposed fee partition, selected parent,
accepting point, carrier identity and ordinal. Refused and exact-repeat
carriers contribute no debit. The output uses a bounded retained vector and
refuses a pre-debited input, mixed branch or repeated/decreasing ordinal.

Planning skips the rehearsal's vertex observation and class-carriage index
write. It cannot supply the final observed action schedule. The normal
candidate path still requests observed execution and checks that output kind.
The planning result is ready for the pre-coinbase fee consumer. The UTXO and
Round path does not consume it yet, and no live debit has run.

Strict Contract/Core/Consensus library Clippy and all Consensus test-target
typechecks passed. The isolated collector runtime test compiled the exact
collector and test source in a small harness. It passed the zero-charge,
wrong-prefix, repeated-ordinal and capacity cases. The complete native
planning traversal did not execute in a block.

## UTXO consumption of a supplied relay fee plan

The shared UTXO walk can now take one native fee plan. Both Round fee
admission and the final fee sink verify each matching accepted ordinal
against the accepting point, selected parent, carrier block, exact txid,
op14 kind, gross fee, mandatory burn and pre-relay collectible amount. The
Round reserve and coinbase reward entry use the same verified net tip.
Finality fees also exclude the relay debit, including under the older
gross-fee classification rule. Unused entries or an inconsistent Round
reserve abort the local transition before publication. The fee sink still
forwards the observed partition to the final native PALW fold.

The current candidate and virtual callers still start without a plan. They
need the pre-coinbase orchestration that generates and hands off the plan,
handles changes to the Round acceptance prefix, and supplies the matching
template context. No live relay debit is claimed in this step.

Strict Contract/Core/Consensus library Clippy and all Consensus test-target
typechecks passed. The isolated exact-source collector and UTXO carrier
matcher ran two tests. They covered matched and mismatched prefixes,
ordinal, and gross, mandatory and pre-relay fee mismatches. Full native
UTXO, Round and coinbase execution with a plan was not run.

## Candidate pre-coinbase relay debit replay

Candidate validation now runs the ordered planner when it captures op14
actions, the accepting header commits the actual parent state, and no active
Round fee budget can change the accepted set. For a nonempty debit plan it
discards the preliminary raw actions, resets the disposable UTXO acceptance
outputs to the selected parent, and reruns the shared UTXO walk with the plan.
The second pass must match every accepted block, transaction, carrier index
and position before continuing. It rebuilds the observed native actions from
that pass. The planned debit reaches the candidate's reward verification
through the existing net-fee sink and observed-registration checks.

This is the stable-acceptance candidate path. A Round reward budget that can
change acceptance still needs ordered prefix replanning, and virtual/template
construction still needs an equivalent fee preview using its own point.
Published activation gates stay closed. No actual funded candidate or template
was executed in this step.

Strict Contract/Core/Consensus library Clippy and all Consensus test-target
typechecks passed. The exact-source acceptance comparison ran one isolated
test with changed carrier indices, transaction order, block identity and
missing merged groups. No full candidate with a relay fee plan ran.

## Scoped virtual relay fee preview

The virtual UTXO walk now supplies the template's mergeset rewards with the
same planned relay debit and the same acceptance replay checks used for a
candidate. It uses the virtual DAA, blue score and difficulty bits, and keeps
the reserved scratch origin separate from actual accepted origins. Its output
is the net reward projection. The candidate independently derives its own
plan and checks the observed registration and custody writes.

This preview covers the initial relay path with no interleaved legacy PALW
objects or other Positive actions, outside the timestamp-dependent execution
lane and the own-attempt-dependent DNS-retirement overlay carve. Ordinary
payments can share the mergeset. Mixed acceptance, the Round monetary budget,
and the overlay carve still need construction parity. The public activation
fences remain closed.

Strict Contract/Core/Consensus library Clippy and all Consensus test targets
typechecked. Three isolated tests exercised the exact-source fee-plan
matcher, including rejection across template and actual origins, and the
acceptance replay comparator. An actual funded template and its acceptance
on another node have yet to run. The compressed verification record pins
the commands, source map and logs.

## Candidate relay replay alongside Round fee admission

Candidate validation can apply a relay fee plan when Round bodies are present
and every planned debit is outside bodies subject to the Round fee budget.
The replay rebuilds the disposable Round budget from the same selected-parent
state and candidate inputs. It checks exact equality of both the Round income
records and the accepted blocks, transaction IDs, carrier positions and order
before forwarding any observed relay actions to the native fold.

A debit within a fee-gated Round body can free budget for a later body and
change the accepted prefix. Those plans remain outside this replay path,
including when the carrier's own body was already accepted. The general
Round path also needs to distinguish an inactive budget from a local error
constructing an active budget. The virtual template's timestamp and overlay
limitations described above still apply.

Strict Contract/Core/Consensus library Clippy and all Consensus test targets
typechecked. Three isolated tests ran from the source, covering the plan
matcher and Round gate intersection as well as exact acceptance comparison.
No funded Round candidate was executed in this step.

## Active Round budget preparation failures

The Core admission preview now returns a preparation error separately from
a dormant budget. Candidate and virtual acceptance propagate native resource
or context failures as local deferrals. The relay replay and existing
execution-lane template consumers use the same failure mapping. A node that
cannot construct the active book cannot proceed with an unbudgeted Round body.

The regression holds the local demand limit at capacity, checks the dormant
and active outcomes, releases the lease, and retries from the same parent.
The virtual selected-parent PALW tip loader still needs to propagate its own
preparation failures. Fee changes within budget-gated Round bodies still
require prefix replanning.

Strict Core/Consensus library Clippy and native and EVM-enabled test-target
typechecks passed. The regression's standalone runtime command was building
the default optimized Core library when this static checkpoint was recorded.

## Virtual PALW parent availability

Virtual acceptance now loads one authenticated PALW tip and checks that it
names the selected parent. EXEC anchoring, the UTXO locks and fee planning
use that same snapshot. When Assignment or the monetary Round budget is
active, a missing or different tip defers processing as unavailable history.
Capacity errors and unreadable state keep separate local error outcomes.

The initial Round budget retry test failed after releasing its 16 MiB test
lease. A second run with a 4 GiB fixture limit failed at the same point.
The existing writer quotes the widest u16 assignment row per journal write
when no allocation policy is configured. The corrected fixture uses a
128 GiB logical counter and allocates no buffer of that size. The runtime
then confirmed dormant behavior, active refusal under contention, and
successful retry from the same parent after release. Production limits
are untouched.

An isolated exact-source regression exercised the matching tip, missing
tip, mismatched tip, capacity error and unreadable state outcomes. A native
funded template and a full cold-node recovery run remain outstanding.

Strict Core/Consensus library Clippy and Consensus test-target checks with
and without the EVM feature passed on the same source as both successful
runtime checks. The compressed verification record retains both failed
fixture runs separately.

## Fixed-row Round budget journal quote

The budget writer now charges its fixed-row operations for the pinned
1,024-byte Borsh encoding buffers. The two capital-allocation operations
that can write assignment rows still use the variable-length estimate.
The journal checks the fixed-row bound and rejects unexpected assignment
writes before adding the journal to the native delta.

This removes the u16-wide capital-assignment quote from the Round fee
preview with no allocation policy. The regression restores its 16 MiB
logical cap and holds that entire cap before retrying. Strict Core and
Consensus library Clippy passed. The guarded runtime command was compiling
the default optimized Core library at this checkpoint.

The optimized Core run passed the Round budget regression with the 16 MiB
logical cap. It exercised the inactive case, the typed capacity refusal
with the entire cap held, and successful retry after release from the same
parent. The runtime archive pins its command and source.

## Funded relay carrier through the native node

The private G14 harness now has a relay regression using actual signed
P2PKH fee inputs, heartbeat templates and the ordinary validating node
entrypoint. It checks a one-sompi shortfall, full registration and an exact
retry on a different carrier. It asserts the virtual net fee, actual
acceptance position, rooted authorization and custody rows, and archived
charge. A second database receives the same blocks and checks the tip root
and charge independently.

The carrier fees are real UTXO spends in the fixture. The future funding
outpoint is the declared target of the relay, before an Assignment exists.
This test covers registration and pending custody. Assignment admission,
worker payment, and the public activation fences are outside its scope.

All Consensus test targets and the existing Core Assignment fence fixture
typechecked. The guarded optimized native test command was building at
this checkpoint. No registration or replay success is recorded yet.

The first optimized runtime command exited during the `kaspa-wrpc-server`
link. The externally supplied RocksDB archive had unresolved libc++ symbols,
and the Rust link invocation omitted `-lc++`. The test binary did not run.
The [first verification archive](funded-native-relay-first-verification-20261103.json.gz)
pins the two successful static checks and this separate link failure.

A repeat used the same source, test command, optimized profile and normal
stack. A private linker wrapper supplied libc++ for Rust's
`-nodefaultlibs` links. The test reached the real heartbeat validator, which
refused an active Assignment prefix because the fixture had yet to arm its
local capacity ledger. The carrier assertions did not execute.
The [runtime archive](funded-native-relay-libcxx-verification-20261103.json.gz)
pins the failed result, test binary and linker wrapper.

The corrected fixture registers the class below the fence and arms the
isolated ledger before its first active heartbeat. The repeat is in progress.
An initial F preflight candidate also passed standalone Core library and test
typechecking. It rechecks selected-parent relay material against the actual
funding output and requires the original custody to cover the derived minimum
retention horizon. It awaits integrated validation and grants no Assignment
admission or bound-account custody.

The follow-up candidate compares the signed canonical R body directly against
both the borrowed archive and the historical transaction. It avoids keeping a
second payload buffer during bounded original-payer replay. It also checks
that a restored initial Assignment binding has its matching Assignment and
original funding account, and rejects R and F recorded in the same accepting
block. The standalone Contract suite and streaming comparison passed 12
tests. Standalone Core and both focused targets typechecked, as did the
Consensus transport against the original Core with the two pure matchers
copied into the isolated build. The candidate awaits integrated validation.

The capacity-armed runtime reached an ordinary pre-carrier heartbeat. The
fold refused the fresh delta because the fixed output owner and final journal
entries exceeded the construction reservation. A direct execution of the
same binary localized the refusal to `beat_to`, before any relay carrier.
The [capacity runtime record](funded-native-relay-capacity-verification-20261103.json.gz)
preserves the failed execution and the backtrace.

Commit `00c7f4195` reserves the fixed output owners before the fold and the
actual weight/frontier/point journal slots before writing them. The latter
quote includes the difference between the retained vector capacity and the
pinned Borsh growth bound. The guarded
[Core and Consensus Clippy run](funded-native-relay-journal-static-verification-20261103.json.gz)
passed on this source. The optimized macOS native-node run advanced to the
database-write check at the pre-carrier heartbeat. It refused the transition
because macOS has no audited enforced quota backend. No registration or
fee assertion ran. The same binary passed eight legacy G14 certification,
fraud, restart, reorg/replay and payment cases. The
[runtime record](funded-native-relay-journal-runtime-verification-20261103.json.gz)
pins both commands, the binary and unchanged sources. The
[relay refusal](funded-native-relay-runtime-journal.log.gz) and
[legacy results](post-journal-legacy-regressions.log.gz) are separate logs.

The next recovery fixture closes the producer's RocksDB connection after the
first successful registration, opens the existing directory again, and
compares the accepted state root, both R rows, the archived fee partition and
the real spent/change UTXOs. It then submits the exact registration again
on a different carrier. Its isolated test target typechecked. The candidate
also subtracts the original R rollback margin before qualifying the minimum
F retention horizon and propagates a local ledger projection resource
failure through assigned Court replay. Neither change supplies the
prospective all-branch or bound-account owner.

The follow-up recovery fixture builds a competing native branch with real
conflicting spends of the three relay carrier inputs. Its assertions compare
the reorged node against the independently built branch, require the old
relay rows and change outputs to disappear, and allow the old content-addressed
archive to remain available for rollback. The isolated Consensus test target
typechecked. The branch has not yet been exercised at runtime.

An exact native-wire preflight of a fixture-shaped delta with only the two
pending relay records and a final point measured 160,630 bytes of combined
replay workspace under the prior quote. The fixture's original cap is 128,000
bytes. Borrowed canonical row comparison and a fixed nonce projection reduce
the same minimum to 120,574 bytes. A larger real delta still requires its
actual preflight. Thirteen standalone Contract and streaming cases passed
and the isolated Consensus test target typechecked. The changes were staged as candidates while the heartbeat runtime pinned
its source.

The [empty-heartbeat differential](native-journal-empty-heartbeat-differential.json)
links the same driver against the old and repaired Core archives. The old
archive refuses the first metered transition. The repaired archive matches
eight unmetered roots and deltas under a 1 MiB logical cap, and applying and
reverting each of those deltas reproduces its child and parent roots.

The exact recovery candidate is preserved as commit `7eaf2ba68`. It also
refuses a new registration before its debit when its heap cap cannot cover
even the two inserted rows and the mandatory final point. The extracted
floor matched the actual fixture-shaped native wire preflight at 120,190
bytes. It was integrated as `45927210f` after the macOS run finished.
On the integrated source, warnings-denied Core and Consensus library Clippy,
all Consensus test targets, and the two focused Core test targets passed
their static checks. Eleven relay Contract tests passed. All four commands
share one unchanged source map. The
[run records](initial-f-integrated-verification-20261103.json.gz)
and [raw logs](initial-f-integrated-logs-20261103.tar.gz) preserve the
commands, results and resource observations. The cold reopen, independent
replay and competing-branch assertions still require the full node test
on Linux.

## Enforced XFS quota prerequisite

A disposable Linux container in the existing Colima guest mounted a loop
device with XFS project-quota enforcement. Project 4197 was capped at 16 MiB.
An 8 MiB allocation succeeded in that project and an over-limit allocation
returned ENOSPC. A same-sized control allocation outside the project
succeeded. The production Rust quota reader also accepted a database after
a write, close and reopen, observed reduced headroom, and rejected the
unscoped parent. This probe used system RocksDB and exercised the quota
reader independently of the consensus node. The [probe archive](native-relay-xfs-prerequisite-probes-20261103.tar.gz)
contains the executed scripts and their output.

The [Linux node runner](native-relay-linux-xfs-runner-20261103.tar.gz)
starts an isolated container with a 128 MiB XFS project for node test databases.
It builds commit `45927210f` from a Git archive, uses the existing optimized
test profile with debug symbols disabled and EVM off, and monitors host
free space. At this checkpoint Cargo was compiling the node test dependencies.
The runner has no native registration, fee or recovery result yet.

## Initial Case relay preflight candidate

The [staged candidate](initial-case-relay-preflight-candidate-20261103.tar.gz)
extends the actual-output and selected-parent pending relay checks to C.
It compares the exact ResultId, funding-scope and signed consent IDs, the
original Assignment's payer/anchor/cap and the actual output index/amount.
The extracted matcher passed a substitution, expiry and over-cap regression.
The candidate Core library and focused Core test target passed isolated
metadata checks. The Consensus transport has not been validated.

The candidate remains outside the native branch source. A C registration
uses the already accepted Assignment's original contract scope, while its R
registration fee and physical policy use the rules at R's own acceptance.
The current historical R reader handles initial A and requires its two scopes
to come from the same intake. Before C can be integrated, its historical
reader needs a bounded projection of the original accepted Assignment
and payer at the exact R event. A preflight alone creates neither C custody
nor a Case, and the replay cannot substitute a later Assignment rule scope.

### Case relay historical contract scope

The [follow-up candidate](initial-case-relay-historical-scope-candidate-20261103.tar.gz)
projects the exact original Assignment row from the real R event. It compares
each newer row image before reversing it, keeps one codec-bounded buffer
and meters the original row decode. A uses its existing nonce projection.
R's policy, fee and custody stay tied to R's actual rules. C checks the
contract identity, original payer and caps against the restored Open
Assignment, including its chronology relative to R. Substituting R's
ruleset for A's contract ruleset or using a closed Assignment fails.

Eight isolated funds cases passed with the extracted matchers. Candidate
Core and test metadata passed. The Consensus test target passed metadata
against the prior compiled Core with exact copies of the new pure functions.
This does not establish native replay or registration. The candidate stays
staged while the initial A runtime is pinned. C op14 still needs the actual
original Assignment material/profile owner at registration and an end-to-end
test covering a ruleset change, actual fees, custody, retry and recovery.

### Integrated Core runtime

The unchanged `45927210f` source passed seven actual Core Assignment funds
and four relay tests. The guarded optimized test command exited zero, with
equal source maps and no remaining process-group members. The
[execution record](initial-f-core-runtime-20261103.json.gz) and
[raw log](initial-f-core-runtime-20261103.log.gz) preserve this result.
The Linux Consensus node test is a separate run.

### Native Case relay registration

Commit `9b607f689` uses the rooted original Open record or an earlier accepted
Open event in the same block to qualify the original relay profile. It keeps
R's actual fee policy separate from the accepted Assignment contract, and
charges pending Case registration against the poster and original account
caps. Admission checks the minimum future Open and carrier replay resources.
Core and Consensus library Clippy and all Consensus test-target checks passed
on one unchanged source map. The focused Core typecheck was stopped while
waiting for the shared build lock. The
[verification record](case-relay-registration-verification-20261103.json.gz)
and [logs](case-relay-registration-logs-20261103.tar.gz) preserve that distinction.

Bound Case restore now has a candidate that requires the canonical
AssignmentUse, Result, original funding output and signed funding consent.
Its isolated orphan regression passes. The additional Case capital quote
covers the full initial bound pair and the bounded successor history,
including both original archive key/value records. An isolated encoding
check matched the initial and successor quote to the complete literal bytes.
These follow-ups are applied for integrated checks. Actual F binding still
requires the live native storage owner and atomic native account writer.

### Case relay custody transfer candidate

The [checkpoint candidate](case-relay-atomic-binding-candidate-20261103.tar.gz)
keeps the authenticated pending package under its native workspace ticket
through the Case writer. It rechecks the actual C carrier, original capital
and custody horizon and stages the bound custody update with Case, funding,
AssignmentUse and journal rows. Restore additionally matches the binding to
the Case reservation. An error restores the enclosing canonical checkpoint.

The isolated Contract and Core metadata and focused Core relay test target
typechecked. The candidate remains separate from the worktree undergoing
integrated checks. It still requires a real all-branch Case reservation
factory and real C binding, DB recovery and settlement runtime coverage.

### Bounded staged alias lookup

The [ordered lookup candidate](case-ordered-legacy-alias-candidate-20261103.tar.gz)
replaces the staged-court key walk in the actual Case closure check with
point and inclusive subject-prefix lookups. Kernel seal checks use tuple
prefixes. The order-only isolated test exercises all 26 previously checked
staged namespaces, their upper tail values, neighboring subjects and the
four previously excluded namespaces. Candidate Core metadata passed.

The candidate is separate from the source pinned by the bound-slot checks.
The general claim/job and funded-escrow reverse lookups, plus proof of the
future closed ingress and all-branch native reservation, remain open. The
lookup result cannot authorize a Gate C decision by itself.

### Funded escrow lookup by claim

The [funded escrow index candidate](case-funded-escrow-claim-index-candidate-20261103.tar.gz)
changes the actual Case closure check to an ordered lookup by claim. The
derived index has one member per retained contract, including settled
contracts awaiting retirement. The native row writer and forward/reverse
delta apply share the index update. Carriage restore derives it from the
existing rows. The candidate includes native write and restore heap charges
for the new index. Its patch leaves the rooted escrow map and carriage
encoding in place.

Core library and focused test-target metadata passed against a copy with
the bound-slot follow-ups. A standalone regression using the extracted
lookup and update methods passed for two contracts on one claim, settlement,
removal, re-insertion and a neighboring claim. The existing Core escrow
regressions now also assert lookup results after replay, undo and carriage
restore, including the original serialized bytes. Those Core regressions
have yet to execute. The candidate remains outside the source pinned by the
bound-slot verification.

The full Core unit-test source typechecked with the candidate applied to
the committed base and its matching cached dependencies. The first unit
typecheck caught an extra field in a carriage-only test pattern, which
was removed. The archive records both outcomes. Runtime coverage remains
limited to the standalone lookup regression.

### Integrated bound Case restore and retention quote

Warnings-denied Contract, Core and Consensus library Clippy and the two
focused Core test-target checks passed on one unchanged native source map.
The [guarded logs and source maps](case-relay-bound-slot-integrated-20261103.tar.gz)
record both results. This integrates the bound restore crosslinks and exact
slot quote. The guarded commands typechecked the regressions. The isolated
crosslink and literal-byte checks described above are the runtime evidence
for this change.

### Integrated alias lookups and atomic relay custody

Commit `89c636618` applies the bounded staged/seal lookups and the funded
escrow claim index. Commit `288e9b01d` applies the pending Case relay custody
transfer inside the Case bind checkpoint. Restore requires that the custody
binding name the Case reservation. Actual op9 still refuses without a
native all-branch reservation owner, so the transfer has no accepted-chain
runtime coverage yet.

A guarded run on the combined source queues strict library Clippy, all Core
test-target checks, and the optimized funded escrow runtime regressions.
At the checkpoint the run was waiting for the shared target build lock.
The separate Linux/XFS node runner was still building RocksDB.

The remaining kernel alias lookups cannot use the existing job holder as a
substitute for the reverse relation. The legacy Panel path can retain an
uncovered claim without any holder entry. A bounded closure proof must
account for these rows and the independent native claim job identities.

### Gate C Case links and row lookup

The [Gate C candidate](gate-c-case-links-candidate-20261103.tar.gz) checks
CaseId, ResultId, coverage scope and liability together before treating a
terminal Case as a withdrawal. The same check applies before closing new
source requests. A CaseId intentionally stays the same when only the
worker's output changes. A foreign Result or branch is now an identity
error in the candidate. The Gate C point readers also use an exact key
lookup with a reserved temporary key in the shared S/K/V/N/P table.

The Core library and focused Assignment funds test-target metadata passed
on the formatted candidate. The isolated runtime used the extracted
candidate guard and the existing signed bundle fixture with the linked
Contract crate. It accepted the exact live Case, recognized each of the
three terminal statuses, and refused substituted Result, scope and
liability for each status. It did not exercise native sweep or source
authorization end to end. The candidate remains separate from the A0
source pinned by the active custody/index checks.

The independent Case archive branch's initial op9 journal store change
(`b44ed654b`) applies cleanly to A0. Its two preceding archive replay
changes (`3d65fa756` and `9c61c087b`) need reconciliation with A0's
native resource and historical Case relay readers. A0 has not applied
that series at this checkpoint.

### Assigned Case archive reconciliation

The [reconciled archive candidate](assigned-case-archive-a0-candidate-20261103.tar.gz)
brings the three Case archive branch changes onto the exact A0 source. It
persists sequence-one op9 transactions in the existing Case journal,
checks the complete literal carrier and revalidates original populated
inputs. It reconstructs the original selected parent by undoing actual
ancestor deltas, then replays the accepted prefix to its pinned root.
The A0 adaptation uses its typed retained-size computation for the new
history bound even when the global checker is dormant. It preserves
A0's original Case relay profile reader and receipt replay.

The merged Core library and focused Assignment funds target typechecked
in isolation. The candidate includes the journal DB reopen/append
regression and the one-byte-under heap-cap checks, but they have yet to
run on A0. Full Consensus verification, cold archive I/O accounting and
connection of the Case reader to accepted op9 remain open. The candidate
is separate from the source pinned by the custody/index run.
