# Public conviction implementation, 2026-10-13

Base: `origin/pre` = `64699e960bc2782efba2550e247efdfdf5f325a2`.
Workspace branch: `codex/g14-public-conviction`. The original checkout's existing
changes are preserved in its original branch. Full G14 is **not complete**.
The current status estimate is about 50% of end-to-end acceptance and about
75-80% of source implementation, up from the earlier 70% source baseline.
These are informal weighted estimates, not a measured acceptance
score. Source-only work, failed runs and nonterminal trials do not count as done.

## Source-First Workflow

On 2026-10-14 JST the user requested periodic push again. A thread heartbeat
named `G14 workspace checkpoint push` is ACTIVE at an hourly interval, targeting
only `origin/codex/g14-public-conviction`. WIP commits preserve source work, not
proof of compilation or end-to-end completion. Before each checkpoint, shared
editing is coordinated; raw evidence logs, target binaries, captures and secrets
are excluded. No force push, direct `pre` push or history rewrite is authorized.
The first source checkpoint is `49c5c735f`, confirmed on the remote workspace
branch. Subsequent source integration is still incomplete and unqualified.
Functional verification remains deferred until source integration is complete,
as requested.

The latest user instruction is to complete the remaining source implementation
and integration first, then run the verification suite together. All agents are
following that order. Adding test sources and inspecting code are allowed in
this phase; no new builds or tests are being run. The owned integration retry
(session 83605) was deliberately interrupted after that instruction, exit 130.
It is not a PASS or a new functional failure. The preceding build (51720) failed
on the new false-valid test's certificate import; that source import is repaired
but the repair has not been compiled. No current verification process is owned
and left running.

New source-only work includes bounded seed-shared record caches and replay
namespaces, bounded filesystem recovery/retirement, native accepted cache-lease
metadata and irreversible-pruning-point GC grants, keyless service retirement,
global public-history DB read permits, and explicit TickService shutdown in
network fixtures. These changes do not qualify an earlier socket failure.

New source connections include required-family Bundle fixed acquisition/semantic
windows, retained original seed/context queries, checked SUM resource terms,
tokenizer EOF inventory, and Tiled Constraint scheduling. Native archives project
Bundle's active prefix checkpoint and persist recovered original records in the
same accepted batch, without changing original signatures/clocks or recursively
archiving recovery. All remain uncompiled and unrun at the current source tip.

The compact original execution binding is now mandatory in the dormant HFP1
envelope and survives native carry, per-object rehearsal and final candidate
extras. Original public job, execution root, CU rules, authenticated EOF size,
family SUM commitment and prospective producer pledge are checked at admission.
The raw inventory hash includes the original u64 actual-length prefix; false
declared tokenizer lengths are not acquisition size authority. Its node actor
and canonical producer readiness/publication remain under integration.

Shared Flat public material now has separate native claim lease/expiry indices,
created at claim admission before any seed and unioned across all accepted owner
seeds/forks. Per-seed GC ACK cannot retire shared metadata. Native pages visit at
most64 expiry rows, use only strict irreversible-pruning-point expiry, and ACK
the exact current union. Metadata bytes are included in the resource quote.
Accepted archive/lease writers now retain native store exclusion through the
actual atomic DB commit, not merely until batch preparation; GC ACK cannot
interleave between preparing an extended union and persisting its expiry index.
Actual file unlink still needs the complete admission-clock/finality proof that
no later accepted lease can invalidate a prior shared deletion authorization;
rechecking a page and rejecting a stale ACK alone cannot undo a file deletion.
History raw carriers now authenticate their bounded bytes against the rooted
original head before nested Borsh vector decoding. These are source additions,
not new acceptance evidence.

The new history custody module defines a prospective producer-signed pledge,
a separately signed literal Valid-seat grant, backwards rooted recovery with
protected multipart answer slots, and an exact DA-only default. The quote
includes all native admitted custody slots, an independent owner answer slot,
mutable native journals and archive storage. It is not hard-coded to five seats.
The explicit `HFP1` outer envelope keeps original FP bytes intact. Native
ordinary/evaluation/tensor extraction now uses transient original-payload views,
then verifies the transaction-local pledge against the exact extracted claim,
producer, execution root and job pin. Isolation/header doors are configured by
the dormant stage fence; real fee, mass and transaction identity remain outer
transaction facts. The companion survives per-object rehearsal and the final
candidate fold through explicit extras. Expected ruleset, authenticated inventory,
resource commitment and absolute horizon are now connected to the common
admission check in source, not yet qualified by execution.
The prospective default helper avoids re-running a sibling's producer charge
and does not invent a live claim phase after terminal conviction. Native
carriage/admission and Bundle history dispatch are connected in source;
node recovery actors and automatic collection still need integration.
Existing semantic-source receipts are not retroactive history
grants. Unit test sources for recovery and lease storage have been added, not run.

Additional all-format gaps are explicit: original Flat Constraint has a bounded
two-pass stream court and original read-at backend in source, but the shared
native duties/filer/actor integration remains incomplete. New selected-event
acquisition preserves u32 tiles, but old arithmetic paths still need their full
format/size audit. Qualified TIR has a genuine typed hydration/semantic bridge
gap, and a maximum-size token table still needs bounded file/index-backed holding
and streaming mask work. None is permission to exclude supported profiles.
Native fork-union archive retention and global population/RAM capacity are not
yet proven within their quoted storage envelopes.

Raw tokenizer format currently has no authenticated actual-file-size cap before
EOF. A false declared length cannot justify finite claim-wide acquisition terms.
Owner intake EOF inventory alone does not prove original claim-wide bounds.
The new native preclaim inventory authenticates the actual raw operand length
at EOF before computing claim-wide family terms; this is not an arbitrary
raw-file cap. Producer publication/readiness is still being connected, so the
new admission cannot yet be considered a usable end-to-end route.
No shipping fence is armed and no
maximum, deadline, fee or mass gate is weakened to manufacture completion.

The 50% whole / 75-80% source figures are informal estimates, not measured
coverage or proof of completion. Full G14 remains unqualified until
all source connections and the subsequent terminal verification are complete.

## Implementation

The public filer does not evict a retained mismatched capture merely to re-probe
an already missing FP job. Binding/job DA remains reachable without a replica;
acquiring the authenticated descriptor permits fair rotation again. Replaying an
already pursued deferred capture preserves the current paginated history walk.
Only newly pursued cases require replay-triggered backfill; unit selection and
acceptance/branch changes retain their own invalidation rules.

Once a constrained job and binding are known, generic arithmetic DA cannot
preempt the selector's required public rows. A complete authenticated Flat
selection check, or a successful independent tiled replay with no nominee,
permits arithmetic fallback. Missing replay/table/material is still pending,
not evidence of honesty or an objective conviction.

A prospective `PublicPrefix` seed appends to the dormant staged-court seed
selection. It authenticates the accepted job, registered family relation, rule
ladder, independent reserver and literal signed custody. Its fee metadata quotes
the full original-source acquisition budget, including hostile custodian slots,
and must equal the actual saved participant count. Only current credited Valid
signers join the owned custody snapshot. Unsigned seats cannot veto acquisition
or become liable by membership alone.

The fold retains the public prefix court, signed demand, tentative protected
answer slots and exact original-leaf authentication. An authenticated complete
answer advances the paid semantic continuation. A pending demand is indexed at
its exclusive response deadline; an answer at that deadline is refused. Its due default enters the
existing producer/S4/DA reporter collection funnel once. Owned mutable journals,
apply/revert and rebuilt indices retain or remove that pending deadline exactly.
The original direct-prefix row remains available separately. These are dormant
consensus additions; no shipping activation guard or active profile is changed.

The tokenizer test descriptor now includes the required prospective parser key
event bound and explicitly rejects a descriptor missing that qualification.
The socket fixture retries TCP ConnectionRefused only while its asynchronously
started real P2P listener binds, within the original 15-second total deadline.
Protocol/handshake failures are still terminal errors.
The clocked fixture waits for the immutable block's ordinary wall-clock lead
cap instead of trying to advance elapsed network slots faster than real time.
The incorrect preimage is a small direct object; the correct large descriptor
uses ordinary multipart carriers. Neither receives a mass-limit bypass.

## Evidence In Progress

Build environment: Rust 1.93, default EVM features, locked dependencies,
`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=3 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0`. Actual-service futures require
`RUST_MIN_STACK=67108864` on this host. The initial default-stack attempts
aborted and are retained; they are not PASS evidence.

The first node binary was `kaspad_lib-5c485cb7922bbe61`. Its filer filter first
matched zero tests (excluded), then the correct filter ran 47 tests: 45 PASS,
2 tokenizer fixture FAIL. Descriptor-only constrained recovery ran 1 PASS.
The source has subsequently changed; these results do not verify later edits.

The first registered tiled service run completed: **0 PASS / 4 FAIL**, 2591.99s.
All four acquired public material but failed the service deadline; capture
rotation kept restarting the shared public-history walk. No registered tiled
terminal has been counted as PASS. The first sole socket-receiver run completed:
**0 PASS / 2 FAIL**, 2122.90s, both TCP ConnectionRefused before handshake.
These are failures before the subsequent walker and listener repairs.

The first public-prefix fold regression compiled and failed because its synthetic
claim named a different producer than the pre-admitted public row. The fixture
now uses the row's actual producer and funds the claim reservation. A subsequent
compile caught the incorrect test field `committed`, repaired to the existing
`reserved_exposure` map. The repaired core binary ran **2 PASS**, 2.62s: demand
and response deadline indexing, complete original-leaf EOF before advancing,
literal producer/all five qualified signer collection, no owner debit, once-only
collection, and each mutable delta's apply/revert state root and index.

The same core binary also ran **20 prefix + 7 staged-state + 1 staged-isolation
+ 4 post-launch arming PASS**. Together with the two new fold tests these are
34 unique core PASS. The arming regression's expected caught panics are retained
in its raw log; the test process exits successfully. These checks do not constitute
a genuine registered public-prefix service.

The ordinary final node filer run passed **48/48** and descriptor-only recovery
passed **1/1**. After selector ordering changes the optimized node filer again
passed **48/48** (4.25s); these duplicates are not counted as additional cases.
The optimized command only changes `misaka-palw-base0` and `misaka-palw-sdk`
local compiler optimization to level 2, never native deadlines or gates.

The read-only shipping harvest passed: **361 pins unchanged**, 29 history checks,
3 labels and both held-forfeit gates. `t12_repin.py` was using the pre-reorganization
source path for those two gates; it now locates the existing tests in `tests.rs`.
The script's separate decision-rule selftest is **17/22 PASS**, with five stale
expectation/alias scenarios failing. No pin has been rewritten to suppress this.
The selftest's explicit expected copy lists were subsequently updated to include
the already registered cap/floor2/resilience and release/document aliases.
Its repaired run is **22/22 PASS**; the mutation and decision rules are unchanged.

The first optimized registered rerun was terminated after two terminal failures
and while two further cases were still preparing (exit 143). It exposed a generic
arithmetic DA session blocking the selected public row, repaired afterward.
The optimized socket rerun reached a real handshake but aborted on
`ClockLeadTooFarAhead` and panic cleanup (exit 134). The large terminal rerun
failed ordinary admission at transient mass 518320 / 480000 (1 FAIL, 1066.92s).
These three runs are **not PASS**. Their raw logs remain unchanged. Latest
registered/terminal and network reruns are pending and cannot yet be qualified.
In `registered-terminal-final.log`, the first three cases failed; bootstrap's
parallel input request still combined `PromptIds` with arithmetic `StepNode`s.
The remaining two cases were still preparing. In `network-verifier-final.log`
both real handshakes and remote reservation/demand transaction relay succeeded,
but neither endpoint completed. The old fixture binary could wait for a future
native slot beyond the test's wall deadline; its owned process was terminated
(exit 143), not counted as a completed run. The fixture now checks the original
wall deadline during lead-cap waits as well. It does not weaken the native
120-second recovery cadence or authorize future timestamps.

The adjacent core court/resource/32-part transport filter passed **24/24** and
the node accepted-progress answer planner passed **2/2**. These are independent
unit/codec/resource checks, not maximum-model service qualification.

At the user's explicit request, three implementation agents now work in the same
workspace on the remaining replay/reset/bootstrap ordering, the public-prefix
node requester/responder and owner-targeted default collection/admission tests.
Their new source must be integrated, compiled and reverified; prior binaries do
not qualify these changes.

Core binary SHA-256:
`017058f4be5cd291cc75c671d2abaf33c93724ab156b3b373af781f85d199b30`.
Latest optimized node test binary SHA-256:
`ee35111b02486cdde8d6257e558f3ac5858142deefe5f0f08c4efda5dc0685f8`.

An intermediate ordinary node `cargo check` passed. Final core/node builds,
regressions, source hashes and service reruns must be recorded before qualification.
Raw output, including failures, is retained under [evidence](evidence/).

## Parallel Implementation Status

The node integration build completed successfully in 7m 10s. Its test binary is
`kaspad_lib-0e97fa9f0b66b49b`, SHA-256
`db23a21e9dea48cd639af29bc12ae99d9361a1824ac1e792902251c8a5454717`.
The filer filter passed **51/51** and the new public-prefix focused filter passed
**3/3**. Raw output is in `parallel-node-filer-status.log` and
`parallel-node-prefix-status.log`. These are not native service terminals.
The build began before the final core COW/resource edits and does not qualify
the complete latest source tree.

The parallel source connects signed public-prefix seed/Demand/Collect/certificate,
automatic original-leaf replies, Q36 owned pages, accepted-history cache recovery,
bounded signer duty reads and deadline-prioritized submission. The core adds
owner-targeted collection, closed/expired journals, prospective original-job
retention, full acquisition acceptance bounds, derived responder indices and
transparent owned-row Arc/COW sharing. The latest RAM bound is eight times
base plus eight times pending plus carrier scratch. These latest core edits
remain source-only: the current 11 fold tests and one maximum-prefix resource
test have not been rebuilt or run. The intermediate snapshot passed 6/7 fold
tests; its failing collateral fixture was corrected afterward, not counted PASS.

The old registered/large-terminal final run completed **0 PASS / 5 FAIL** in
2530.64s. Subsequent source repairs do not retroactively qualify that run.
The socket material-reader trial reached acquisition, authenticated public replay
and reservation release but aborted during RPC shutdown (exit 134). Its channel
closure repair is not yet reverified, so the trial remains FAIL. The other two
network cases were terminated (exit 143), not terminal PASS. Two new native
initial/inherited public-prefix service tests exist but have not been executed.

## Continuation Evidence

The next integration build completed in 11m 35s, before the archive/API/SDK phase.
Its node binary SHA-256 was
`d07ade03d5619137931beb469111efa9fd4fb381e87d7a36377606335184a65c`,
now preserved at `target/g14-registered-qualified-tests`. Its filer filter passed
**52/52**, including the branch-rollback semantic completion regression
(`filer-reorg-regressions.log`). This does not qualify later tokenizer eligibility,
archive, P2P or SDK edits.

The same phase core binary SHA-256 was
`82ec507e9c2d5ba2391e3ed65cc0927242ca948f9bd91518e046915b16f145c6`.
Its 11 public-prefix fold tests completed **7 PASS / 4 FAIL**
(`parallel-core-eleven-before-history.log`). The fixtures incorrectly included the
producer among their five Panel custodians; assertions expecting seven distinct
attempt slots and seed qualification exposed the error. Fixtures now use five
distinct nonproducer custodians. Production qualification was not relaxed.
Those corrections and the subsequent 13-test archive snapshot remain unverified.

The actual service run `native-prefix-registered-first.log` completed in
1416.27s: **1 PASS / 2 FAIL**, overall exit 101. The registered tiled
forbidden-token case reached exact objective conviction, producer collateral
collection, all five literal Valid signer sanctions and reporter reveals,
reservation closure, and normal service exit. This is one registered-model
service terminal, not all-family or maximum-profile completion.
The two native public-prefix cases failed before commitment admission because
the public wallet builder rejected literal job versions 11 and 12.

The socket reader rerun `socket-reader-shutdown-repair.log` acquired the
authenticated multipart job, independently replayed it, released the reservation,
verified actual socket RPC evidence and joined the notification services. It
still aborted during DB teardown with four remaining strong references (exit
134). Explicit async adaptor/flow/address-manager teardown is now implemented
but not reverified; that rerun is still FAIL.

Latest source adds a court-rooted rolling hash/count for full signed accepted
public-prefix movements, a content-addressed DB archive independent of body/delta
pruning, bounded expiry GC using the irreversible pruning-point DAA, targeted
snapshot/current native reads that remain available until the paid material clock,
and authenticated cache recovery through a dedicated protocol-107 public pull
lane (oneof 80/81). The broker holds at most 16 outstanding records, globally
paces requests, rejects unsolicited/forged/oversized replies, and never treats
peer nonresponse as a conviction. Raw reads are capped at 90,224 bytes.
Original acceptance clocks and seed identities are not reset.

Latest source also qualifies registered tokenizer statements independently of
the FP job version, preserves the constraint job's extra selected-root check,
and supplies a Params/DAA-qualified public prefix builder for both local and
external signers. The producer and actual-service fixture use the new API;
the old builder's legacy behavior remains covered by a new regression.
These source additions need current build/runtime evidence.

An intermediate `cargo check -p kaspad --lib --locked` succeeded in 1m 19s.
The first archive no-run build used stale in-flight core metadata after API
edits and failed with missing current-reader/helper symbols. The fresh no-run
attempt then stopped on **No space left on device** (exit 101;
`public-archive-integrated-build.log`). Only this worktree's generated core/node
artifacts were cleaned (176 files, 1.7 GiB); sources, raw logs and the qualified
registered-service binary were preserved. The repaired build is in progress in
`public-archive-integrated-build-disk-repair.log`, not PASS yet.

Full G14 remains unproven. The same owner cannot yet perform every required
semantic court sequentially: required-family bundles, checked SUM budgets and
fixed per-stage acquisition/stream/inclusion clocks are only designed, not
implemented. The generic Constraint node actor is still absent. Local page
recovery duplicates records per cursor, retains uncollected CAS branches and
performs whole-history replay in one blocking pass; worst-case host disk,
fairness and deadlines are not bounded or measured. Public records missing from
all peers still leave recovery pending. No maximum-profile or activation claim
follows from the one registered forbidden-token service terminal.

## Latest Verification And Paid Continuation Audit

The six-package no-run build completed successfully in 8m 41s after repairing
the missing test-only execution-backend trait import. Raw output:
`public-archive-integrated-build-import-repair.log`. This is a build result, not
a terminal service result; later node fixture repairs and continuation edits
require another build.

Focused results from that snapshot:

- SDK public builder: **4/4 PASS**, `prefix-builder-qualified-regressions.log`.
- Native archive store: **1/1 PASS**, `public-archive-store-regression.log`.
- P2P broker: **2/2 PASS**, `public-history-broker-regressions.log`.
- P2P wire/router: **2/2 PASS**, `public-history-wire-regressions.log`.
- Prefix source/resource quote: **5/5 PASS**, including the maximum active
  positions quote, `public-prefix-resource-before-continuation.log`. A checked
  quote is not a measured maximum-profile service completion.
- Accepted-history codec: **1/1 PASS**, `public-prefix-history-codec.log`.
- Existing staged state: **7/7 PASS**, `staged-seven-before-continuation.log`.
- Public-prefix fold: **2 PASS / 11 FAIL**,
  `public-prefix-thirteen-before-continuation.log`. Its actual producer is bond3;
  the revised synthetic Panel2..6 still included it. A shared fixture assertion
  exposed this before the failing cases reached their folds. Fixture repair is
  in progress; production qualification is not relaxed.

A P0 continuation gap is confirmed in source: `da_release_all_v1` closes every
legacy reservation on a claim, while staged movement/progress and native
prevalidation require that live reservation. PublicPrefix additionally closes
its semantic answer/batch lane with `claim_closed`. A sibling or self-inflicted
DA default therefore blocks an incomplete, already paid semantic prosecution
even though its material remains held. Existing completed certificates are a
different case and do not prove this continuation requirement.

The next implementation phase separates exact already-open paid court authority
from ordinary new-seed reservation admission. Original owner, seed, reservation
DAA, stream/collection/material clocks and collateral holds must remain binding;
there is no extra collection, clock extension or admission bypass. Node candidate
tracking and reorg/restart recovery must use the same authenticated authority.
Until these changes and sibling-default terminal regressions pass, the no-erasure
requirement remains open.

The same node snapshot's filer filter passed **54/54**,
`filer-qualified-before-paid-continuation.log`, and its public-prefix filter
passed **4/4**, `node-prefix-qualified-before-paid-continuation.log`, including
the capture-retention guard's expected caught negative panics. These do not
qualify the new paid-continuation or cache implementation. Baseline binary
hashes are core `bc583194549ac56ecf038e7207485e71d118a15d17deb671eb92eae4a1ab8338`
and node `3723120912dc1fcafa79ecd6d3b4885957e76451627896d13ec05478786f2866`;
the node binary is preserved at `target/g14-before-paid-continuation-tests`.

The socket reader's async-release rerun terminated **FAIL, exit 134** in
`socket-reader-async-release-current.log`. It completed public multipart
acquisition, independent honest replay/release at DAA 1244 and actual socket RPC
authentication, but the 10-second owner-drain wait expired with four DB
references. `SendPingsFlow` retains its context during the normal 120-second
TickService wait. The fixture now owns and shuts down both source/receiver tick
services after panel services stop and before strict DB destruction. No ping
interval, native clock or strict lifetime check is relaxed. This repair still
requires an actual rerun.

Source now implements exact retained paid authority, all four typed core
continuations and native Leaf/Move prevalidation after legacy release, plus a
four-family partial-A/sibling-default/completion/once-only-consumption fold
regression. Node discovery and its dedicated typed-only path use the same
authority; missing Tokenizer/Constraint restart context remains Pending.
The source does not authorize a fresh seed or ordinary legacy demand after
release, and does not extend the original terms.

Cache source uses a seed-shared signed-record CAS, one bounded authenticated
Merkle sequence index per current recovery branch, and process-local replay in
at most 64 links or semantic records per poll. Every index reference is verified
before semantic use; no disk parser checkpoint is authoritative. Raw CAS keeps
atomic durable writes; the disposable index no longer fsyncs every tree node.
Pages are seed-scoped and have a checked plan-derived cap. Expiry GC helpers need
authenticated original terms and irreversible DAA; service-wide GC is not wired.
Repeated reorg page-cap exhaustion and restart deadline qualification remain
open. The P2P flow also has a node-wide two-read budget with cancellation-safe
permits owned by the actual blocking DB work. These latest changes are being
built in `paid-continuation-integrated-build.log`, not yet verified.

## Remaining Acceptance (Current)

The automatic public-source requester/responder now exists in source and has
focused unit coverage, but genuine registered seed/class admission through the
service terminal remains unqualified. The 64-row indexed duty bound does not
establish a maximum-population inclusion guarantee. Missing/pruned historical
accepted carriers leave cache recovery pending; no DA reacquisition route is
qualified. Full population state clones, many signed moves per block and total
queue RAM still need quantitative bounds. Sequential court-family use by the
same owner also needs an end-to-end audit. Complete capital, custody retention,
collection and processing capacity must be shown together.

Finish registered tiled terminals, largest terminal evidence and all covered
family/job/input/output/state/DA endpoints; verify the public prefix route from
normal admission through outside-verifier service and collection; close native
and socket restart/reorg/pruned-IBD paths; measure maximum-profile time, RAM,
capital and collusion load; rerun shipping isolation/pins and the final G14 audit.
Do not label source-only, failed, running or zero-test results complete.

## Source Continuation, 2026-10-15 (unverified source only)

Continued from the Codex snapshot `01ae08063`. Nothing below has been built,
compiled or run; no new evidence log exists and nothing is labelled PASS. Builds
remain deferred to the consolidated validation phase.

Audit of the in-progress items from the Codex snapshot:

- (a) Producer pre-submission quote: already wired in source. The canonical
  producer asks `palw_staged_preclaim_quote_v1` (same `staged_preclaim_quote_v1`
  planner, `staged_maximum_custodians_v1` and class ladder as admission), signs
  the pledge and builds the HFP1 envelope. External producers (gateway rail,
  remote signer) still build plain FP carriers; under the dormant fence their
  required-family claims are refused at admission (not DA-charged). See remaining.
- (b) Tiled original row-zero IDs are the first of three counted selected
  requests in the quote (moves, carrier bytes, two acquisition spans). Present.
- (c) Late-Demand defaults: selected sessions carry an immutable minimum response
  span; Flat open checks the first-pass window; PublicPrefix and history demands
  derive a fixed response span bounded by the original horizon. Present.
- (d) Implemented now: store writers take the irreversible pruning-point DAA and
  freeze any lease (per-seed and claim-wide) whose horizon is below it: never
  extended, never recreated. Commit archives public material only for a block on
  the pruning point's own selected chain (descendant or chain ancestor), not by
  DAA comparison. New store regression source added (not run).
- (e) Pruned-import restore of live claim/owner holds from the verified state in
  the tip batch was present; it now passes the imported irreversible clock.
- (f) Float32 Constraint claims are refused at preclaim admission and Flat
  sessions require the Int32 lane, so honest producers are not DA-charged for a
  format they cannot answer. The Float32 canonical source/adjudicator is absent
  (the base Float32 lane has no decode-token adjudicator either).

New source connections:

- History recovery actor is now a panel module. Custodians answer from the
  native archive, falling back to the bounded authenticated peer pull when a
  pruned-synced custodian lacks its own copy. The outside owner opens the native
  backwards recovery only after page replay found no local/archived/peer copy,
  and always continues an accepted session (Demand/Collect at accepted clocks).
- Court-queue round collision fixed: history used `MAX-17` (Flat responder) and
  the kernel verifier used `MAX-13` (LG14-A release). Now `MAX-18` / `MAX-19`.
- `palw_v2_daa_score` (used by the tokenizer inventory and history actors) did
  not exist; added as the accepted-snapshot DAA through ConsensusApi/session.
- Keyless shared claim material retirement (Flat public projection sidecar)
  wired into both the keyless and seat loops using the native claim-wide union.
- File-backed public token tables: `--palw-token-table` files above the resident
  cap are imported with bounded memory and registered by derived root/tokenizer;
  outside bundle stages use the streaming mask over either holding.

Still not code-complete (not validation): Float32 Constraint source and
adjudicator; qualified TIR projection for required families; external producer
HFP1 envelope (needs a preclaim-quote RPC); Valid-seat history custody grant
issuance (policy decision: opting a seat into history liability); orphan Flat
sidecars of never-accepted claims; quantitative population/per-block move and
queue RAM bounds (analysis, possibly a fenced consensus cap needing a decision).

### Static audit fixes, 2026-10-15 (unverified source only)

A read-only static audit of the outside-verifier route against the latest source
found compile-level and route-breaking defects; these are now repaired in source
(still not built or run):

- Compile-level: the HFP1 builder's staged ML-DSA verifier is exported; the Flat
  responder wraps its capture range in `Arc`; the history actor logs through
  `kaspa_core`; a base0 test imports `PalwStepCoordinateV1` from `palw_step`.
- PublicPrefix demand: the signed deadline is checked at the accepting clock
  (at least one full response span from acceptance, at most the derivation's
  span-aligned two-span bound) instead of exact equality with the owner's
  snapshot clock, which no carrier could ever meet. No remaining horizon is "no
  demand", not an error that blocks the owner's other readers.
- Staged source defaults (selected/Flat/history): the producer's claim-level
  leg and DA record run only when the producer is itself in the literal unpaid
  set; signers who stayed silent beside an answering producer bear only their
  covering charge. Previously total producer withholding could never be charged
  (role check error) and an answering producer could be charged.
- After the staged fence, plain FP carriers (no required family, evaluation,
  tensor, external producers) stay in the accepted set; only an HFP1 carrier is
  replaced by its original view and must authenticate its pledge. The fold still
  refuses a required-family claim without its pledge.
- Required-family claims carry their commitment job pin even before
  `palw_offence_attribution` stamps it (refused if a different pin is present).
- Outside owner: paid-authority comparison uses the active stage's seed/terms;
  the legacy queue sweep leaves staged-round items alone; the FP input projection
  opens HFP1; stage reads tolerate a newer accepted tip.
- Queue rounds: public prefix answers `MAX-20` (was shared with the replay
  filer's `MAX-6`); history grants `MAX-21`.
- History custody grants: a native grant-duty reader and a seat tick that signs
  a grant for its own already credited Valid receipt on a required-family claim.
  This makes every such Valid seat a literal history custodian (liable to the
  once-only history DA default); flagged for the user as a policy point.

Left as found (not route-breaking): an alternative uncalled FP extraction helper
(`palw_fp_objects_from_accepted_txs_by_class_and_history_v1`) and an uncalled
`recover_pages_v1` wrapper.

### Lead defaults pending user review, 2026-10-15 (unverified source only)

Recorded as "lead default pending user review" (reversible):

1. History custody grants stay automatic for Valid Panel seats on required-family
   claims (signing Valid attests holding the material; 10-12 ruling).
2. External producers: read-only preclaim quote RPC (op 244, same planner as
   admission) and a custody delegation carried in an HFP1 version-2 envelope. Both
   the producer and its node custodian sign a standing window; admission refuses an
   invalid delegation; the custodian may sign the producer's literal answer slot and
   the producer stays liable. Node custodians run delegated responder passes on
   separate queue rounds. The gateway rail refuses a required-family claim without
   `--custody-delegation`.
3. Float32 Constraint stays refused at admission; recorded as an explicit uncovered
   lane in `docs/design/palw/g14-completion-matrix.md` §2i/§4.
4. TIR required families: a full projection is not feasible in this phase; the
   design and the six missing items are in the matrix §2i; TIR required-family
   claims stay refused at admission (uncovered, unrewarded).
5. No per-block consensus cap; the fee/prepaid-plan/per-owner bound and its
   derivation are in §2i and `palw_staged_load_bound_v1` (measured inputs are named
   placeholders read by no consensus rule).

Also added: a fold-level regression source for the new default-charging roles
(producer withholds and is charged; producer answers and only the silent seat is
charged; the winner can never be in its own unpaid set). Removed the two uncalled
helpers (`palw_fp_objects_from_accepted_txs_by_class_and_history_v1`,
`recover_pages_v1`). Delegation envelope/signature test sources added. None run.

## Validation 2026-10-15 (paused)

Paused 10-15 by user order; partial results apply only to the recorded source
commits and will be rerun in the consolidated phase. Raw logs:
`evidence/validation-1015/`.

Check ladder (`cargo check --lib --tests --locked -j 4`, own `./target`):

- kaspa-consensus-core: first run 9 errors (private field, borrow, a test using a
  nonexistent `StreamLeafOpened` action, test extras initializers); fixed in
  `7f055eee0`; rerun exit 0.
- kaspa-consensus: exit 0 (one pre-existing dead-code warning in
  `native_settlement.rs`, untouched).
- consensusmanager, rpc-core, grpc core/client/server, wrpc client/server,
  rpc-service, p2p-flows: exit 0.
- kaspad: 2 test-only errors and 7 dead-code/unused warnings; fixed; rerun exit 0,
  no warnings.
- misaka-palw-sdk, -gateway, -base0, kaspa-pq-validator-core (lib, bins, tests):
  exit 0 (pre-existing gateway warnings in untouched files).
- kaspa-testing-integration (tests): exit 0.

Focused tests (partial; not the final pass):

- kaspa-consensus-core, 14 module filters: first run 65 pass / 2 FAIL. Fixed a
  production defect (pledge refused a zero CU ruleset, which the BASE0/RC canonical
  context commits) and a regression with a wrong move order; rerun **PASS 67/67**
  (`test-02-consensus-core-focused.log`), on source commit after `7f055eee0`.
- kaspa-consensus store `prefix_history_v1::tests` (archive, lease freeze, shared
  GC, verified-state restore, recovered history): **PASS 9/9**
  (`test-04-consensus-store.log`).
- kaspad focused unit filters: 84 ok / 2 FAILED before the run was stopped (exit 143;
  it had reached a long service E2E). Failures: a new token-table test expected the
  wrong vocabulary count (fixed in source, not rerun); the generic Constraint
  scheduler regression fails with "constraint accepted seed changed" (not yet
  diagnosed). Not PASS.

Not started: g14_kernel_route_e2e, service E2E, affected suites, clippy.

### Code resumed after the validation pause, 2026-10-15 (unverified source only)

- Float32 Constraint adjudicator core (`palw_float_decode_v1`): greedy first-index
  f32 order key matching the reference worker, bounded streaming authentication of
  one committed row against its event hash/opening/trace root, decode verdict and
  constrained beat comparison. Lane stays refused (rooted row court, Float32 source
  sessions, seed and node actors missing).
- TIR: decode view (`palw_decode_view_v1`) from the admitted TIR header and
  registered record; view-based tiled authentication, selected demand/answer core
  and Constraint court seed constructor; TIR preclaim quote projection
  (Constraint over tiled logits + raw tokenizer). Admission still refuses TIR
  required families via `PALW_STAGED_TIR_ROUTE_ADMITTED_V1 = false`; the remaining
  wiring (a)-(f) is designed in g14-completion-matrix §2i.
- The other GAP rows of the matrix (GAP-00..82) belong to other lanes (kernel
  route, pipelines, typed roots, EXEC slices, legacy Panel decision) and were not
  touched here.

### Validation-phase item: "constraint accepted seed changed" (static diagnosis, 2026-10-15)

`generic_constraint_streams_large_rendering_to_exact_rooted_fault_and_restarts`
failed (before the decode-view refactor) with "constraint accepted seed changed".
Static cause: the node scheduler took the seed id to be the signed carrier digest,
but the native row's seed id is its court's identity, which is that digest only
when the seed retains its profile and otherwise the court's canonical context. A
non-retained seed therefore never matched the accepted cursor, and the locally
streamed EOG/rendering context would have differed from native too. The scheduler
now rebuilds its court through `court_with_resolved_binding_v1` exactly as native
admission does and uses that court's id (open and archived reopen). Source fix
only; the test must be rerun in the consolidated validation phase.

### TIR and greedy Float32 lanes wired behind two new dormant fences, 2026-10-15 (unverified source only)

Commits `fe2a26101` (fences), `7cfeffc8d` and `49eb9a9a3` (Float32), `b94021a1d` and
`8bcd816ea` (TIR), on `codex/g14-public-conviction`. Nothing below was built, tested or
run; it waits for the consolidated validation phase.

**Fences.** `palw_staged_tir_route_v1` and `palw_staged_float_constraint_v1`: `Params`
fields that are `None` on every preset, require ConsensusV2 and the staged public court at
or below them, and appear in the identity hash, fork id, validation and transition
extras. They are lifecycle owning fences (`StagedTirRouteV1`, `StagedFloatConstraintV1`),
in force only where the staged court is too. The forms each route appends inside the
staged court's kinds 165/167 (FloatSource* moves, the FloatConstraint auxiliary seed,
NativeSelected* moves) are owned by their route's fence through `palw_staged_route_forms_v1`.
Below the fence they are the live build's undecodable payload: they ride unjudged (A-2
uniform), validation skips them, and the fold refuses them as defence in depth. TIR's
native seed is a new kind, tag 168 `PublicCourtStreamOpenedNativeV1`, owned by the TIR
fence. The preclaim quote admits TIR required families only with `gates.tir`, and a
Float32 Constraint job only with `gates.float32` and a greedy job.

**Greedy Float32 Constraint.**
- The row stream is a stored keyed-BLAKE2b continuation with a byte-position check, so
  the court can restart.
- The per-owner source session authenticates in this order: the ids against the v2 trace
  root; the owner's nomination over those authenticated ids; the nominated f32 row in
  64 KiB parts; and a finish carrying the event opening.
- Each slot is isolated (producer, held Panel, owner). The clocks follow Flat. Unpaid
  custodians go to the native sanction fold.
- The Constraint court has a Float32 seed: `new_from_float_pin_with_original_stop_context_v1`.
  The owned row admits it, and the FloatConstraint seed is admitted in the fold.
- Node side:
  - a custodian responder that reads a generic public Float32 sidecar or the backend's
    projection;
  - producer-side sidecar retention through a backend seam (default `false`);
  - an owner stage in the fraud filer;
  - the constraint scheduler generalised over Flat|Float pins.
- **Seeded Float32 stays refused at every height**, because there is no canonical float
  sampler. The quote, the session and the court all refuse it.

**TIR required families**, items (a)-(f) of matrix §2i:
- (a) Carriers name no binding: NativeSelectedDemand/Answer, and the tag-168 seed, which
  signs the admitted header's identity and the view vocabulary.
- (b) The bundle retains `selected_native` (the header and the job). Opened stage contexts
  are now `Legacy | Native`.
- (c) The fold derives the view from the claim's admitted header and the `tir_classes`
  record at every move. Constraint is admitted through `new_from_tiled_view_v1`, with the
  header bytes as seed identity.
- (d) The selected responder answers TIR duties through `disclose_tir_selected_event_v1`.
- (e) The fraud filer dispatches TIR bundles with the native view and has Constraint and
  tokenizer stages that emit native seeds. Its beat nominee comes from
  `replay_tir_generated_ids_v1`.
- (f) The tokenizer statement and court run over the view.

**Not adjudicable on this branch.**
- **TIR prefix-state stays refused by name**: there is no TIR state relation for the
  public prefix court. That would be a new family court.
- **No in-node backend executes TIR FP jobs or retains Float32 rows.** Both seams
  (`disclose_tir_selected_event_v1` / `replay_tir_generated_ids_v1`,
  `open/write_float_public_source_v1`) default to `Err`/`false`. The TIR FP execution
  backend belongs to the RFC-0002 lane; a Float32 producer can write the generic sidecar.
- Until those exist, the node actors are wired but cannot answer or replay. Separately,
  the filer's candidate discovery for TIR claims (case creation, reservation) still goes
  through the legacy pursuit pipeline.

**Test sources (not run).**
- Float32:
  - the restart of a stored row continuation;
  - a source session that authenticates ids and then the nominated row per isolated slot;
  - the G14 collusion test: total withholding is collected, a substituted row never
    authenticates, the committed row shows `beats = true` in the Constraint court, and a
    seeded job is refused.
- TIR: `g14_tir_outside_verifier_convicts_from_the_native_view_under_full_collusion`. It
  uses a registered tiled record and an admitted header with its TIR-domain execution
  root, and no binding. Total withholding is collected, a forged tile is refused, and the
  authenticated event seeds a court that sees the beat.
- A-2: a route form rides unjudged until its own fence, even where the staged court is
  armed.

**Validation items added.**
- The lifecycle fence ownership test now also accepts the forms ledger
  (`PALW_STAGED_ROUTE_FORMS_V1`).
- The A2U harness arms both new fences.
- Object tag 168 must not collide with another lane's tag at merge.

### Round 5, 2026-10-15 (unverified source only)

Commits `5a00bbaff` and `ea58a730c`. Nothing was built, tested or run.

**1. Float32 row-holding producer and custodian (base0).**
- In `misaka/palw/base0/src/float_public_source.rs`, a Float32 run's retained material IS the
  generic public sidecar. `float_public_material_v1` builds it from the run's rows and ids and
  returns the v2 trace root the binding must commit.
- The Base0, A16 and Qwen36 backends implement `write_float_public_source_v1`, which copies a
  Float32 run's material to the claim's sidecar and returns `false` for integer material.
- They also implement `open_float_public_source_v1`, which reads the producer's sidecar or a
  custodian's FP payload range and is refused unless it is this binding's Float32 run.
- The files ride the existing claim-scoped lease, the shared GC and the unpublished-projection
  cleanup.
- The core sidecar reader now reads in 16 KiB chunks, which is the node file reader's bound.
- Caveat: no in-node executor runs a Float32 class. The integer families are the only FP
  families, by ADR-0053/0077. A Float32 executor plugs in by retaining `float_public_material_v1`
  as its run material.

**2. TIR prefix-state: relation core implemented; family wiring specified; still refused.**
- The core is `palw_tir_prefix_state_v1` plus `PalwTirStepSpaceV1::prefix_state_leaf_{count,}_v1`.
- **Alignment rule.** `0 < k < prefill` and `k ≡ 0 (mod checkpoint_interval, h_tile)`, so the
  state after `k` positions is exactly committed by step leaves: the `Fixed` checkpoints at
  `k − 1` and the `Hist` chunks `0..k`. An unaligned `k` would need a recurrence replay, a kernel
  court, so it stays refused.
- **Canonical digest.** The digest is job-independent: it hashes the boundary leaves' lanes with a
  kind/state/layer/first/position/count prelude, never the job-bound leaf hashes. The state root
  is `palw_fp_prefix_state_root_v1(class, k, digest)`.
- **Stream court.** It is restartable and authenticates each leaf against the claim's step root.
  It returns `Honest` or `StateRootMismatch`. A producer-side reference digest is included.
- Not wired, in this order (each item is code):
  1. a native seed family `PrefixState { prefix_tokens, layout }`, where the layout is checked
     against the record's `layout_digest` (the step space needs it);
  2. a custodian source session with isolated slots over `PalwTirPrefixStateStreamV1`, with Flat
     clocks and unpaid custodians collected;
  3. a new `PalwStagedCourtRowV1` family row (about 52 match sites);
  4. a TIR prefix family quote: moves = slots × boundary leaves + replay; bytes = leaves × lanes +
     openings;
  5. fold admission and moves;
  6. a node custodian hook, `disclose_tir_state_leaf_v1(material, header, leaf_index) ->
     (values_le, opening)` (RFC-0002), and an owner stage;
  7. the TIR executor declaring prefix states with `palw_tir_prefix_kv_digest_v1`.
- The quote still refuses TIR prefix-state by name.

**3. Fraud filer TIR entry is native.**
- A free-prompt candidate whose claim has an admitted TIR header is handled in
  `fraud_filer_start_replay_v1` by `fraud_filer_tir_replay_v1`, which:
  - reads the accepted FP job and its authenticated public input;
  - runs the verifier's own replay through `replay_tir_generated_ids_v1`, which now returns
    `PalwTirReplayV1 { generated_ids, full_logits_trace_root }`;
  - settles the claim as honest when the tiled trace roots are equal (TIR is integer and
    deterministic);
  - otherwise records a mismatch whose own ids are the Constraint nominee.
- The case carries its native context (`case.tir`). It skips the legacy public-context
  bootstrap, reserves through the engine's prelude (`DisputeReservedV1`, which is claim-generic),
  and after that runs only the staged TIR bundle: no legacy binding read, probe or descent.

**4. adv/c4r4: not merged.**
- The branch is at `162df6b16`. Its fork point is `f7ed9642c`, which is already an ancestor of
  this branch.
- Its patch-unique commits are:
  - the three C4R4 fixes (F-03/-05/-10), already carried here as `8e400f014`, `10d7d8079` and
    `660fa4fbc`;
  - test-only PoCs: `misaka/palw/kernel/tests/c4r4.rs`, `challenge/tests/c4r4_grinding.rs` and the
    `g14_kernel_route_e2e` c4r4 modules, all present here at their post-reorg paths, plus
    `consensus/core/tests/c4r4_reporter_share_49.rs`, which covers ADR-0032's 49% reporter-share
    economics;
  - the round-4/4b adversarial-record doc sections.
- None of it is code for the active plan (the staged legacy/TIR/Float32 route).
- Its merges predate the 10-12 repository reorg, so merging the branch would replay
  pre-reorg paths across hundreds of files for no active-plan gain.
- Left for the coordinator's integration branch if wanted: the reporter-share test and the
  record sections.

**5. TIR execution backend hooks: interface contracts.** These are what the integration branch
wires to RFC-0002's TIR executor. Full text in `consensus/core/src/palw_backend.rs`.

- `disclose_tir_selected_event_v1(material, header, row, tile, maximum_leaves) ->
  PalwSelectedTiledEventV1`:
  - `material` is one retained payload of the claim. Parse only your own format and return
    `Err` otherwise.
  - Check that `material` belongs to `header` (same job context and trace root).
  - Row `r` is the logits row that selected generated id `r`. Tiles are 4,096 lanes. Rows are the
    `post` node's `i32` logits, tiled under the context hash.
  - The result must equal `PalwSelectedTiledEventV1::of_public_rows_view_v1(view, rows, ids,
    (r, t), maximum_leaves)`.
  - Read-only. `Err` means "cannot answer", never a verdict.
- `replay_tir_generated_ids_v1(header, job, prompt_ids) -> PalwTirReplayV1`:
  - Run the registered TIR program from the verifier's own artifact on the job's authenticated
    prompt ids, after checking the ids against the job and context.
  - Select exactly `exact_decode_tokens` ids with the job's rule (greedy, constraint-masked,
    first-index ties).
  - Return the ids and `tiled_logits_trace_root_v1(context, own_rows, own_ids)`. Equal roots mean
    honest. The ids are only a nominee.

**Object tag 168** (`PublicCourtStreamOpenedNativeV1`) belongs to this branch.

### Round 6, 2026-10-15: TIR prefix-state family wired end to end (unverified source only)

Dormant under `palw_staged_tir_route_v1` (A-2: every new form and kind is owned by it).
Nothing was built, tested or run.

- **Layout on chain (tag 169, `TirClassLayoutPublishedV1`).** A registered TIR class's layout is
  public class data. It is authenticated by the record's `layout_digest` and program and checked
  to build a step space. It is stored once as staged row `TirLayout` under key `TirLayout(class)`,
  which never expires; publishing it again is a no-op. The quote needs it, because the record has
  only the digest. A producer publishes it before claiming. Without it, the TIR prefix quote
  refuses by name.
- **Seed and moves.** The native seed family is `PrefixState { prefix_tokens }`. The moves are
  `NativeTirPrefixAnswer { answerer, seed_id, ordinal, values_le, opening }`, whose authority is
  the typed answerer, and `NativeTirPrefixCollect` (owner only). Both are route forms inside tag
  167.
- **Family row `TirPrefix`** (`palw_tir_prefix_owned_v1`). It has isolated slots: the producer,
  the held Panel and the owner. Every custodian owes its complete boundary stream by
  `stream_until`. The first completed slot decides: `Honest`, or `StateRootMismatch`, which yields
  a certificate that resolves as a `Prefix` staged fault through the existing conviction paths.
  The stage is honest-complete only after collection. Unpaid custodians go to the native default
  charge.
- **Exhaustive matches updated:**
  - row codec (discriminant 9) and checkpoint codec (5);
  - `active_row`, cache leases, owner accounting, checkpoint and restore, key check;
  - family mapping (PublicPrefix), honest-complete, bundle stage check;
  - certificate resolution;
  - paid authority, owned progress and the new `TirPrefix` owned cursor;
  - fold `index_rows` and the streamed-duty index;
  - bundle scheduler honest/family, filer paid-stage arm.
- **Quote.** `staged_preclaim_tir_quote_v1` now prices the prefix family instead of refusing it,
  above the fence only:
  - boundary leaves from the published layout;
  - largest leaf ≤ 80,000 bytes, so one leaf fits one carrier;
  - opening depth;
  - slots = custody + owner, plus reorg replay.
- **Fold.** `open_native` admits `PrefixState`: it checks qualified custody and an owner outside it,
  and that the actual plan fits the frozen stage plan. `tir_prefix_control_v1` rebuilds the step
  space natively at every move from the admitted header, registered program and published layout.
  `publish_tir_layout` handles tag 169.
- **Fix found on the way.** The shared streamed-duty page validated entries against Flat sources
  only, which would have refused round 4's Float32 entries and these. It now checks the full
  `flat_response_index_rows_v1` union.
- **Node.**
  - Custodian responder `palw_tir_prefix_responder.rs` (own and delegated slots, lane `MAX − 23`):
    the slot cursor goes through the reader `palw_staged_tir_prefix_leaf_index_v1` to a leaf index,
    then to `disclose_tir_step_leaf_v1` over the retained capture.
  - The fraud filer's TIR bundle has a prefix stage: open the native seed, collect after the
    custody clock, file a staged conviction on mismatch.
  - Also fixed: the filer's queue checks now accept `NativeSelectedDemand` as a bundle-control
    move (round 4 omitted it).
- **Executor contracts for the integration branch** (in `palw_backend.rs`):
  - `disclose_tir_step_leaf_v1(material, header, leaf_index, maximum_leaves) -> (values_le,
    opening)`;
  - `declare_tir_prefix_state_v1(material, header, prefix_tokens) -> state_root`, where
    `palw_fp_prefix_state_root_v1(class, k, palw_tir_prefix_kv_digest_v1(...))` over the run's own
    boundary lanes; aligned `k` only; the layout must be published first.
- **Test source.** `g14_tir_prefix_state_full_collusion_is_collected_or_convicted_from_the_claims_own_leaves`
  runs over the real tiny TIR execution (`tiny_execution`):
  - unaligned and out-of-prompt boundaries are refused;
  - total withholding collects both custodians;
  - a forged leaf is refused;
  - authentic leaves convict a lying declared root and clear an honest one;
  - checkpoint and restore round-trip;
  - honest completion waits for collection.
- **Object tags 168 and 169 belong to this branch.**
