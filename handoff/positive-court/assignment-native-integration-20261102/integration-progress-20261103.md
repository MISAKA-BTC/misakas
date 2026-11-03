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
