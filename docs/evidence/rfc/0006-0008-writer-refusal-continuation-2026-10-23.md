# RFC-0006 / RFC-0008 writer refusal continuation

Base: `6a128eb65775c9cf1a26f975b7028c2d4a8b1187`; branch
`codex/rfc06-12-continuation`.

`3c70b4078` propagates `write_kernel_row` failures from reader/router V2,
selected-source bootstrap, V6 paid bootstrap, and the two material-challenge
writers in `transition_builder.rs`. Previously these five consumers discarded
the `Result`, although the writer can refuse local resource admission before
changing its map or journal.

`b9ba2e757` propagates both the provider receipt-stream checkpoint write and
the material row write from `routed_answer_v2`. It also routes the common
monetary material row writer through `write_kernel_row_encoded_v1`, which
reserves the actual key and canonical row buffers before allocating them.
These changes retain `NativeResource` as a local retryable error rather than
continuing a transition with a missing write. They do not enable a fence.

## Verification boundary

The baseline Rust test started before these writer changes completed:

```text
CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-testnet/target cargo test \
  -p kaspa-consensus-core --test rfc0012_native_evidence_fold -- --nocapture
test result: ok. 6 passed; 0 failed; 0 ignored
```

This verifies the earlier tree's fold evidence extraction, retirement retention,
and post-Final conviction retraction. The fixture drives a floor claim and
explicitly relabels its record for part of the evidence-reader exercise; it
does not establish an actual registered-model useful-work campaign. It is
also not a regression run of the new writer changes.

The campaign driver control suite separately passed 14 tests. After both writer
commits, `cargo check -p kaspa-consensus-core --lib` exited 0 (3m 40s, 385
warnings). Its output is at `/tmp/misaka-rfc06-12-writer-check.log`. No unused
`Result` warning remained in that check. This is compilation evidence, not a
runtime capacity-refusal test. `git diff --check` and the documentation
no-new-regressions check succeed; the documentation baseline still has 43
known issues.

Still required: targeted capacity-refusal controls at the outer consumers,
restart/reorg/prune and physical resource acceptance, the full model/source/
adapter/payment requirements, and the 15-gate real-process campaign. No gate
is promoted from UNKNOWN by these checks. Public presets and the unarmed
full-activation height remain unchanged.

## Runtime continuation at `a2fcee238`

The reader/router consumer's new late-refusal regression passed: it measures
the concrete successful write's admitted request, then supplies one byte less.
The consumer returns `NativeResource`, leaves state and journal unchanged,
and releases all reservations when the builder drops. This checks the outer
consumer that previously discarded the writer's result; other outer consumers
still require their own controls.

The library test build also exposed and repaired two missing closing braces
in pre-existing post-Final duty/material intake tests and two EVM market
fixtures missing the dormant selected-source bootstrap fields.

The subsequent filtered library run passed 41 tests covering native writers,
GDN court, settlement-once and liability. Logs:
`/tmp/misaka-rfc06-router-capacity-test.log` and
`/tmp/misaka-rfc06-08-core-consumers-test.log`.

A further material/router/bootstrap/retention/liability run passed 40 tests
and failed one stale wire-tag assertion. The actual declaration codec and
lifecycle registry assign `ReaderRouterDeclaredV2` tag 248; 244 belongs to
`MemoryRoutingV2`. `fc88a4a38` corrects only the assertion. The combined
recheck passed 81 tests with zero failures, using the actual library test
binary rebuilt after that correction. Output is in
`/tmp/misaka-rfc06-08-consumers-recheck.log`.

Source inspection also identified a remaining production GDN history issue:
`gdn_filer_tick.rs` discards both errors and continuation from its 4096-block
accepted-lifecycle page read. This is not runtime evidence of a false verdict,
but completeness of disclosure lookup across the full eligible history has
not been established. Production GDN acceptance must include paged history,
read failure, restart and reorg cases rather than treating a prefix as complete.

`29b97afcd` now propagates a page-read error out of the blocking collector:
the service logs it and returns before stepping the filer book or queuing a
new court object. This fixes the error path only. Continuation beyond the first
page remains unimplemented. `cargo check -p kaspad --lib` passed for that
change (2m 36s; 60 warnings), with output at
`/tmp/misaka-rfc06-gdn-service-check.log`.

`dbf505cae` extracts the production strict page collector and adds a control
using the accepted-chain mock: a short page reports its continuation; an
unavailable older acceptance record returns an error rather than a partial
success. The accepted-history/service tests are compiling with output at
`/tmp/misaka-rfc06-gdn-history-tests.log`. The new test's outcome is not yet
known. Cross-tick continuation, runtime failure/retry controls and production
acceptance are still required.

## Service/history execution and wider GDN failures

The history/service run subsequently passed all 15 tests, including the new
GDN page error/continuation control. The wider `gdn` filter ran 20 tests:
17 passed and 3 failed (`/tmp/misaka-rfc06-gdn-all-tests.log`). Ordinary signed
registration and host resolution passed. Two held-court E2E cases (answering
forger and silent forger) stopped at claim acceptance with
`ImmutableModelUpdate("V2 claim needs its native canonical profile")`.
These are failed acceptance tests; no conviction/slash success is established
by those two runs. The routing gate reads `fp_work_profile_of`; tracing the
canonical registered-profile source and its consumer connection remains work.
Do not bypass that guard or add model-level review to make the fixture pass.

The third failure expected the fold ladder refusal but attempted to construct
evidence first, producing unrelated unsupported-kernel/coordinate diagnostics.
`757e52a4c` now rejects a served leaf outside the fold's ladder before evidence
allocation/recompute. Its targeted recheck is running with output in
`/tmp/misaka-rfc06-gdn-ladder-recheck.log`. No result is claimed yet.

The targeted ladder recheck subsequently passed (1 test, zero failures,
0.86s). This resolves that observed probe failure only.

Further source tracing of the two E2E failures found that
`stamp_reader_router_claim_v2` reads `fp_work_profile_of`, while the only
production caller of `write_fp_work_profile` is the `ClassLaneCertified`
object arm. Ordinary `ClassRegistered` does not populate that graph index.
The index also feeds free-prompt pricing and class deadline/held-context
calculations, so storing ordinary registration profiles there requires an
audit of those consumers, not merely a fixture insertion. The missing normal
registration-to-routing connection remains open. Model-level verification or
certification must not be added as a workaround (PRINCIPLES section 9).

## Ordinary registration graph retention (`c4201b296`)

Past the existing memory/routing fence, ordinary legacy class registration
retains its carried graph in table 147 under `0xA3 || class_id`. Routing
consumers use that graph, retaining the existing legacy graph fallback without
writing the free-prompt pricing/deadline index. Decode has a native resource
owner; writes use the existing reserved cell writer. Class identity,
immutability, restore consistency and class removal cover the new namespace.
This stores structural registration data and grants neither model approval nor
claim credit. No fence is armed.

`cargo check -p kaspa-consensus-core --lib` passed (15.94s). The targeted
ordinary-registration/profile/delta-reversal test is compiling; output is
`/tmp/misaka-rfc06-registered-profile-test.log`. The two failed GDN E2E paths
must still be rerun after their fixture supplies the native route/declaration
prerequisites; this compilation is not evidence that they now pass.

The concrete aux scanner recognizes the profile row. Its generator source
also has the profile arm, but the base generator still uses the old router
table number 136 while the checked-in scanner uses 147 (and the adjacent
bootstrap namespaces have moved too). Full generator reconciliation and
idempotency remain open; do not overwrite the current scanner with the old
namespace output or claim reproducible generation from this check.

## Generator reconciliation (`48fd931b1`)

An isolated copy reproduced the generator's failure at the old aux134 marker.
The generator now emits material checkpoint namespaces in 137 and routing/
bootstrap namespaces in 147/148/149. Two isolated generations matched. The
three differing generated files were reviewed and installed after the Rust
test process finished; no target types were removed. The inventory now also
includes the selected-source bootstrap fields previously omitted from the
transition-extras visitor.

The official command then exited 0 in the real checkout:

```text
python3 scripts/drills/check_rfc06_native_wire_idempotent.py
idempotent; committed outputs are the generator output
```

Output: `/tmp/misaka-rfc06-generator-idempotent.log`. This closes the observed
generator-numbering/reproducibility gap for this tree.

The first ordinary-registration test run failed before exercising profile
retention because the shared fixture specified slash value 1 whereas its
existing floor specifies 5. The fixture now reads that value from its floor
row rather than changing any network parameter. The runtime recheck is
compiling; output: `/tmp/misaka-rfc06-registered-profile-recheck.log`.

The corrected registration test passed (1 test). A further run of native
writer, reader/router, manual wire and aux registry controls passed 26 tests
(`/tmp/misaka-rfc06-profile-wire-controls.log`). These are local library/fold
controls; they do not establish a model-wide cost measurement or real IBD.

`5452a3186` supplies the existing GDN fold E2E fixture with a native kernel
route and an explicit per-reader declaration authored from the fixture graph.
It does not insert a profile directly into state, enable model certification,
or guess a router in production. The two GDN E2E cases are rebuilding with
output at `/tmp/misaka-rfc06-gdn-e2e-routing-recheck.log`.

`f2cf3663a` additionally checks native carriage encode/decode/restore at the
expected state root in the ordinary-registration test. That expanded test is
queued with output at `/tmp/misaka-rfc06-profile-snapshot-recheck.log`; its
outcome and the GDN E2E outcome are not yet known.

The `5452a3186` GDN E2E recheck subsequently failed both cases at registry
construction with `routing V2 journal requires native kernel route`. The
fixture supplied route policy, but the route's first header had only been
initialized by the later closing tick. `2ffe0ef32` calls the existing reserved
`ensure_route_header` on first profile retention when no route exists, before
writing the graph. Existing routes and pre-fence registration retain their
existing path. The ordinary-registration test now starts with no native route
and supplies route policy through extras, so its delta/restore assertions also
cover this first-use boundary.

This latest test is queued at `/tmp/misaka-rfc06-first-route-profile-recheck.log`.
The earlier snapshot run at `/tmp/misaka-rfc06-profile-snapshot-recheck.log`
was already live and is allowed to finish; its result must not be attributed
to the first-use fix. Neither GDN E2E case has passed yet.

The earlier snapshot run subsequently passed (1 test, zero failures) after a
7m17s library-test build. It verifies retained-profile native carriage restore
for an already initialized route, in addition to delta apply/revert and the
unchanged price/deadline/certification indexes. It does not verify first-use
route initialization.

The revised first-use regression is now compiling under the original handle.
The GDN E2E recheck of `2ffe0ef32` is queued separately at
`/tmp/misaka-rfc06-gdn-first-route-recheck.log`; neither of these later runs
has produced an outcome yet. Earlier failing GDN logs are preserved.

The first-use regression subsequently passed (1 test, zero failures), including
native carriage restore from the route initially created by registration.
Output: `/tmp/misaka-rfc06-first-route-profile-recheck.log`. This verifies the
first-use fix rather than only the earlier already-initialized-route fixture.

The rebuilt core binary also passed 12 filtered selected-event/source-bootstrap/
paid-bootstrap tests (`/tmp/misaka-rfc07-bootstrap-source-controls.log`). These
cover local source/context contracts; they do not measure independent source
availability, participation, cumulative soundness or actual checker income.
Normal startup remains refused for the private bootstrap paths. The GDN E2E
recheck at `/tmp/misaka-rfc06-gdn-first-route-recheck.log` is still compiling.

The GDN first-route E2E run subsequently passed both cases (2 tests, 1.82s
runtime after a 6m27s build): the answering forger reaches the accusation and
collateral reduction, and the silent forger reaches DA default and collateral
reduction. Each fold step is delta-applied and reversed. These are in-process
fold E2E controls, not the real multi-process/100k campaign.

A wider same-binary GDN/history regression ran 31 tests: 30 passed, including
the two E2E cases, but ordinary node registration failed at genesis because
the newly used native route had no active probabilistic-constraints policy.
The log is `/tmp/misaka-rfc06-gdn-history-final-regression.log`.
`b4792a025` gives that genesis-armed test fixture its native route policy at
DAA 0 as well. The normal node registration recheck is compiling with output
at `/tmp/misaka-rfc06-gdn-native-registration-recheck.log`. It is not yet a
pass; no public preset or activation setting was changed.

The normal node registration recheck then passed (1 test, 47.05s runtime),
including ordinary signed registration acceptance and backend resolution.
The latest same-binary combined GDN/history run also passed all 31 tests
(103.99s, zero failures, zero ignored):
`/tmp/misaka-rfc06-gdn-history-recheck-after-policy.log`.
This closes the three observed GDN regression failures and the additional
first-route/genesis-policy failures tracked above for these fixtures.

Still open: cross-tick continuation of the production GDN disclosure page,
real multi-process/restart/prune/100k acceptance, model-wide cost boundaries,
and the remaining approval/source/material/payment/campaign requirements.
The 31-test result is not evidence that the full RFC-0006 or the 15-gate
campaign is complete. Public activation remains unarmed.

## Production GDN history continuation (`5e5258f11`)

The service now resumes a frozen native anchor/cohort across 4096-block pages
instead of discarding continuation. New arrivals wait for the next scan.
Only the current candidate's required authenticated answers are retained in existing cases;
complete page bodies are not accumulated across ticks. Read failures retain the
cursor. A replaced anchor clears the old cases/settled cache and starts again;
branch replacement during a page refuses that page.

Partial scans collect answers without issuing moves or concluding timeout.
Completion carries the frozen anchor's DAA, not the current tip's later clock.
A new demand records the current submission clock, so an older backfill cannot
shorten its window. A claim missing material/kernels or absent from the live
host during a page is held out of that cohort's completion; a new full scan
may retry it. The existing native court remains the authority for default.

The targeted history/service controls passed 4 tests, covering frozen-cohort
progress under new arrivals, unavailable suffix retry, pre-/mid-page reorg,
partial-history withholding, per-claim incomplete coverage and demand clocks.
Output: `/tmp/misaka-rfc06-paged-gdn-clock-recheck.log`. An earlier library
check passed before the final test/coverage additions; the latest test build
compiled those additions successfully.

The full GDN/history regression passed all 33 tests (104.98s, zero failures,
zero ignored), including both fold conviction/default cases and ordinary node
registration. Output: `/tmp/misaka-rfc06-paged-gdn-full-regression.log`.
These controls are not real
multi-process restart/reorg/prune acceptance or physical cost measurements.
Those and the other RFC requirements remain open; no campaign gate is promoted.

## RFC-0008 exact replacement observation

The core material/provider court/recovery/once-settlement controls passed 41
tests (zero failures or ignored tests, 0.35s), using the current core test binary.
Output: `/tmp/misaka-rfc08-material-consumers-tests.log`. A pre-change
`cargo check -p kaspad --lib --features private-integration-config` also passed
(1m32s); output: `/tmp/misaka-rfc08-private-consumers-check.log`.
These are fold and compilation controls, not physical coinbase or network
provider acceptance.

Inspection found that the private `kernel_paid_replace_v4` worker declared
acceptance whenever the current sequence reached or exceeded its saved intent.
A competing replacement could therefore produce a false success report. The
worker now durably publishes its exact native grant consent before provider
exchange, reuses it on restart, and requires the own-native row to match its
root, exact sequence and consent. Missing old consent records, competing or
later generations fail closed with acceptance unconfirmed. Saved intent
identity, provider roster and deadline must also match the explicit work/spec;
retrying may not rebind the durable subject. Later historical acceptance is
not inferred from a current higher sequence.

The observation regression covers an unchanged old generation, a competing
same-sequence consent, missing durable consent, an exact match, a later
sequence and a different root. The remote material tests passed all 7 controls
(zero failures or ignored tests, 0.75s), including that regression and typed
material reopen/GC/history/journal controls. Output:
`/tmp/misaka-rfc08-remote-material-tests.log`. The post-change private consumer
library check passed (2m46s including the build lock wait); output:
`/tmp/misaka-rfc08-replacement-check.log`. Documentation checks pass with no
new issues and the unchanged 43-issue baseline. Public activation and source-economic
parameters are unchanged; this fix does not close the restart/reorg/prune,
real-provider or campaign requirements.

## RFC-0007 provider acquisition controls

The SDK provider reader now has an end-to-end parameter acquisition regression
using the shared in-process dense/MoE execution fixture. It creates a job and
an honest claim, then obtains every immutable parameter through the actual
provider reader callback. The callback builds native row/tile openings from
the producer's tensors; the consumer authenticates canonical ordinals,
reconstructs each complete tensor, and compares its shape and bytes. The test
also asserts the exact number of fetched units and refuses a missing provider,
a substituted row ordinal and insufficient workspace before fetching.

`cargo test -p misaka-palw-sdk --lib provider_ -- --test-threads=1` passed
6 tests (zero failures or ignored tests, 1.09s; build 4m29s). Output:
`/tmp/misaka-rfc07-provider-adapter-tests.log`. These include the existing
mixed row/column root, commitment-suite and objective-demand selection controls.
The fixture is one in-process Program path, not four-target real-node adapter
acceptance, an independent source, model screening or a whole-model cost bound.

The current core binary additionally passed 11 selected-approval consumer
controls (zero failures or ignored tests, 0.28s): event export requires a
positive quorum and canonical Final, bootstrap carries no EXEC credit,
uncertified claims have no frontier/weight/reward, all model tiers retain
SecurityCertified, and native dependency reads require selected OPV approval.
Output: `/tmp/misaka-rfc07-selected-consumer-controls.log`. This is evidence for
these named consumers only; all-consumer coverage and the full campaign remain
open. No activation or UNKNOWN gate changes.

## RFC-0007 immutable checker action history

Inspection found a stale archive branch in the private checker: it would move
accepted demand WAL out of its original action slot and retain a file-count
cursor. The current `scan_outbox` already derives its next action from the
canonical native view and retains immutable subjects/carriers in funding slots.
Moving those files would lose the original funding owner and reorg replay
subject. The obsolete move/count branch and its old test are now removed.

Startup explicitly refuses an outbox containing any legacy `demand-` archive
entry, including partial, gapped or malformed archives. It retains all files
and does not reinterpret a count as an acceptance/funding cursor. Existing
modern `receipt.borsh`/`transaction.borsh` and `action-NN` slots remain in place.
A remote-helper regression passed 2 tests, zero failures or ignored tests;
output: `/tmp/misaka-rfc07-checker-legacy-tests.log`.

The existing immutable-slot funding test now additionally checks native
acceptance followed by rollback: the successor slot is selected without
releasing the original funding, and rollback reselects the exact original
transaction and signed subject. A new actual scanner test rejects legacy
archives both before and after native demand acceptance while preserving bytes.
The guard runs on every action-history scan, including restart.

The first private test build exposed three existing observation calls missing
the new finality argument; they now explicitly pass `false` (unfinalized).
Output: `/tmp/misaka-rfc07-checker-immutable-tests.log`. The next build succeeded,
but 11 of 15 runtime cases stopped at the owned-directory guard because macOS
TMPDIR traversed the `/var` symlink; output:
`/tmp/misaka-rfc07-checker-immutable-recheck.log`. The same compiled binary
`kaspad_lib-9bd7f4026b4887da`, with TMPDIR resolved to its existing canonical
`/private/var/.../T` path, passed all 15 checker controls (zero failures or
ignored tests, 0.46s). The symlink guard was not weakened. Output:
`/tmp/misaka-rfc07-checker-canonical-tmp-recheck.log`. Coverage includes immutable
carrier/funding retention, native demand/served/default observations, material
challenge applied-versus-finalized history, rollback and enrollment/round/actor identity.

An earlier exploratory archive-move helper was discarded after tracing the
current immutable-slot flow; its local SIGKILL tests are not evidence for this
final implementation or the node campaign.

The campaign requires implementation-complete execution and still has
unconnected gate collectors; no executable campaign candidate exists at the
shared debug binary path. The 15-gate campaign remains unrun and UNKNOWN.

## RFC-0008 measurement attribution and fixed windows

Three measurement defects were corrected without supplying any unapproved
economic values. First, all resolved input owners must be the same actual
address before fees/payments are assigned to a payer; mixed or unidentified
owners stay unresolved, and duplicate inputs are rejected. A fee-sharing rule
is not inferred from the first input or from address labels.

Second, capital integration retains every sampled DAA endpoint. A missing
interval-start value makes the integral UNKNOWN rather than bridging or
dropping its interval. The final endpoint's unused value may be missing while
its DAA still closes the preceding interval. Clocks and values reject booleans,
floats, negative values and noncanonical decimal strings. This remains a
sampled left sum, not a physical upper bound between samples.

Third, the cost driver fixes accepted transaction collection to the recorded
after-snapshot sink. Later accepting blocks in the same RPC page are excluded,
pagination continues only to that endpoint, and a final own-node selected-chain
read checks that the endpoint did not reorg. Missing accepted bodies or an
unreachable endpoint block observation. The existing chain/mergeset scan
budgets remain enforced, including chain hashes read by the endpoint recheck.

The cost controls passed 10 tests and the read-helper controls passed 12. The
final complete RFC Python control suite passed 193 tests (0.916s, zero failures),
output: `/tmp/misaka-rfc08-cost-window-final2-python-tests.log`. These are fake
transport/measurement controls, not actual fees, provider replacement, capital
cost measurements or coinbase acceptance. The initial attribution test's
self-referencing synthetic transaction was corrected before the successful
runs; its failed output remains `/tmp/misaka-rfc08-cost-attribution-tests.log`.

The attempted current-source consensus coinbase build did not execute tests.
The first non-EVM build exhausted disk space and was interrupted; output:
`/tmp/misaka-rfc08-consensus-coinbase-tests.log`. A subsequent EVM/private feature
build also failed during compiler file writes with `No space left on device`;
output: `/tmp/misaka-rfc08-evm-consensus-coinbase-tests.log`. This is an environment
capacity failure, not a coinbase verdict. Completed incremental caches positively
identified with this worktree and these failed build sessions were reclaimed;
unrelated shared caches, sources, test binaries and evidence logs were retained.
Approximately 2.3 GiB remained afterwards. No old-branch binary was substituted
for current acceptance. Larger Rust builds await sufficient storage; the K3
asset/manifest and signed campaign input locations also remain unprovided.
All full-scope requirements and UNKNOWN gates stay open.

## RFC-0008 queue-to-coinbase rendering and replay

The existing private binary passed 13 provider/material controls (zero failures
or ignored tests, 0.39s), with canonical TMPDIR as before. They cover frame
bounds, authority-index reopen/substitution, nested request/response preflight,
deadline/stream signature binding, exact producer recovery artifacts, V5
generation/WAL source controls and recovery carrier decoding. Output:
`/tmp/misaka-rfc08-private-material-runtime-tests.log`. They do not drive actual
provider replacement acceptance or independent remote hosts.

The processor's existing payout-queue rendering expression was moved without
changing its order, width, values or scripts into
`processes::coinbase::palw_pending_payout_outputs_v2`. The construction and
validation consumers continue to call the processor method, which retains its
existing queue-lemma assertion and delegates to that renderer.

A separate Cargo integration target, `consensus/tests/coinbase_manager.rs`,
compiles the actual coinbase manager source and its existing unit controls.
Its added regression supplies a controlled ten-row queue in reverse insertion
order, renders the first eight through the actual coinbase manager, applies
the real PALW fold drain, then renders the remaining two and observes no payout
after the next drain. It checks every amount/payee and the conserved total.
Root-checked in-memory carriage restoration and delta apply/revert reproduce
the same queue and coinbase transaction. The fixture supplies the already-decided
queue; it does not establish reward earning, funding or selected approval.

To avoid another whole-crate codegen under the storage constraint, runtime
execution used `rustc --test` on that integration target with compatible
existing rlibs selected from the worktree build's Cargo dependency fingerprints.
The actual manager source and new test were compiled afresh. Exact argv,
dependency paths and source digests are recorded in
`/tmp/misaka-rfc08-coinbase-manager-build4.json`; build output:
`/tmp/misaka-rfc08-coinbase-manager-build4.log`. The first wrapper compile exposed
a private constants re-export and a missing `faster_hex` extern; the target now
includes the actual constants module and the matching extern was supplied.
Earlier build logs remain. These dependency identities are compatibility
inputs, not a source-to-binary attestation or a release approval.

All 11 runtime controls passed (zero failures or ignored tests, 0.08s), output:
`/tmp/misaka-rfc08-coinbase-queue-final-runtime.log`. The ordinary private node
library check also passed (6.77s), validating the new renderer call in its real
consumer: `/tmp/misaka-rfc08-payout-renderer-private-check.log`. The standard
`cargo check -p kaspa-consensus --test coinbase_manager --features private-integration-config,evm`
also passed (1m32s); output: `/tmp/misaka-rfc08-coinbase-target-cargo-check.log`.

This verifies queue rendering/drain and PALW state replay, not UTXO rollback,
whole-block acceptance, full reward-leg provenance/once settlement, material
restart/reorg/pruned IBD, process crash, allocation/RSS bounds or the 15-gate
campaign. Those remain open; no dormant fence or UNKNOWN gate changes.

## RFC-0006 payout rendering buffer reservation

The renderer now returns `Result<PalwPayoutOutputsV2, NativeCoverageV1>` rather
than a bare Vec. Its private output storage owns the logical native ticket;
there is no mutable dereference, implicit deep Clone or consuming extraction.
The payload drops before its ticket. The request is derived from the actual
bounded queue prefix, output struct sizes and the fixed 69-byte P2PKH script
construction, including a two-length retained script envelope and temporary
script Vec. This is a typed request bound for that source buffer, not RSS or a
whole-block bound. Empty queues allocate no render payload and need no ticket.

Admission precedes the output Vec/scripts. Both real callers propagate refusal:
the virtual walk returns its local native transition fault, and the template
returns `LocalNativeCapacityUnavailable`. Neither substitutes an empty payout
list nor converts local capacity into a peer/block semantic verdict. Validation
keeps the owner in its processing context; template construction keeps its owner
through coinbase assembly. The coinbase manager accepts borrowed output
iterators, so the existing validator/reserve sequence and native prefix are
chained in their original order without an intermediate native-script copy.
Coinbase destination allocations remain a separate owner/capacity boundary.

The source-built test target passed 13 harness cases (0.22s, zero failures or
ignored tests), including its inactive child entry and one parent launching six
isolated native-checker cases: zero capacity, one byte short and exact capacity
for one row and a full eight-row prefix with backlog, and an empty queue under
zero capacity. A live output owner exhausts its admitted request; dropping it
returns capacity. Refusal preserves the state root and leaks no reservation.
The previous queue/coinbase/drain/carriage/delta controls still pass. Exact argv,
matching cached dependency paths and source digests:
`/tmp/misaka-rfc06-payout-owner-iterator-build.json`; build/runtime outputs:
`/tmp/misaka-rfc06-payout-owner-iterator-build.log`,
`/tmp/misaka-rfc06-payout-owner-iterator-runtime.log`. The execution uses the same
direct-source test workflow described above, not a full-node binary.

The first iterator integration check found a remaining slice-only `len()` in
the mismatch diagnostic. It now counts outputs while the iterator is consumed,
without another traversal or allocation. Those failed logs remain. The final
private library check passed (4.24s), output:
`/tmp/misaka-rfc06-payout-owner-final-private-check.log`. The full consensus
`cargo check --tests --features private-integration-config,evm` also passed
(8.23s including its lock wait), output:
`/tmp/misaka-rfc06-payout-owner-final-all-tests-check.log`. That compiles test
callers; it is not execution of every consensus test.

Still open: destination coinbase/block-copy ownership and reservations,
outer live-node capacity/retry acceptance, other payout/court/aux consumers,
whole-model/physical costs, all reward provenance and the full campaign. No
activation, consensus bytes, economic policy values or UNKNOWN gate changes.


### Historical-header observations bind the native generation

The old private coverage sidecar named only a DAA and ruleset. An observation
from another sink at the same DAA, or before a local pruning import, could be
matched to the current RPC read. The writer now publishes the explicitly new
`rfc06-exec-origin-coverage-v2.json` envelope with `generation` containing sink,
virtual DAA and local import epoch. It brackets the census and refuses unstable
or active imports. The consensus census API independently brackets its state
and header reads with the import epoch, sink and DAA. Failures publish an error
with no census or generation; no fabricated epoch is used.

The collector accepts only V2 and matches each observation against that role's
own stable RPC import state, sink and DAA. Different local epochs across the
archive and pruned nodes are allowed; each must match its own file. Legacy V1,
missing generation, boolean/inexact counters, changed sink/epoch, active import,
and contradictory census/error observations cannot become coverage evidence.
The underlying census counts remain `PalwExecOriginCoverageV1`; this does not
change historical consensus bytes or give the observation activation authority.

Validation: all 195 RFC Python controls passed (`/tmp/misaka-census-python.log`).
The production generation-bracket module ran directly through a small rustc
module harness: two controls passed, covering stable reads, each stamp dimension
changing, unavailable stamps, active import on the ending read, and census
failure (`/tmp/misaka-census-bracket-runtime.log`). This is helper execution,
not a live-node archive/pruned acceptance run. The initial integration check
found an owned-session API call mismatch; it was corrected to use the existing
`with_consensus_api_v1` accessor. The final `cargo check -p kaspad --lib --tests --features private-integration-config`
passed in 20.65s (`/tmp/misaka-census-final-check.log`); this typechecks the
private worker and tests without executing the full node campaign. The final
Python rerun also passed all 195 controls (`/tmp/misaka-census-python-final.log`).
Documentation checks retained the same 43 baseline issues with zero new issues
(`/tmp/misaka-census-docs.log`).

Still open: historical-header resource reservation ownership, archive coverage
at real campaign scale, migration approval, other consumer requirements and all
unrun campaign gates. The activation pin remains unarmed and UNKNOWN remains
UNKNOWN.


### Historical census result lists retain their reservation

The census previously pushed into two bare Vecs and returned them without a
native capacity lease. Its implementation now lives in
`consensus/core/src/palw_exec_origin_coverage_v1.rs`, re-exported from the prior
work-slice module. Before reading a historical header it computes the maximum
named entries from the actual eligible native claims and requested list cap,
reserves the two concrete tuple layouts plus the result owner, and fallibly
allocates both lists. It returns `PalwExecOriginCoverageOwnedV1`; the consensus
API, session and private writer preserve that owner until publication finishes.
The writer borrows the counts for serde, preserving the V2 envelope and original
V1 census fields. No deep serialized header buffer is introduced.

An empty history or a zero list cap allocates no list storage and needs no list
lease. A zero list cap still counts every eligible header. Both named lists stop
at their cap while totals continue. Capacity refusal precedes header callbacks;
errors and dropped results release their reservations. An outstanding result
holds its full admitted typed-list capacity until drop. This covers typed list
requests only, not allocator RSS, the already owned source state, historical
header store decode/cache allocations, CPU work or disk reads.

Four controls ran from this exact production source through a small direct-source
rustc module harness with compatible cached core/serde dependencies. They cover
empty native history, exact cap/above cap/missing headers, complete totals beyond
list limits, checked request overflow, and five separate-process capacity cases
(zero, one below, exact, empty, totals-only). The exact-cap result prevents a
simultaneous extra reservation until dropped; refusals make zero header reads.
Logs and source digest/argv: `/tmp/misaka-census-owned-build.json`,
`/tmp/misaka-census-owned-build.log`, `/tmp/misaka-census-owned-runtime.log`.
This does not execute full archive/pruned nodes. The initial integration compile
caught an opaque iterator that does not expose Clone; the implementation now
regenerates a borrowed state iterator instead of cloning or collecting it.

The full historical-header consumer and campaign requirements remain open.
No public preset, activation pin, model tier, source economics input or UNKNOWN
gate is changed by the reservation.


Private integration validation passed:
`cargo check -p kaspad --lib --tests --features private-integration-config`
completed in 1m19s (`/tmp/misaka-census-owner-final-check.log`). This compiles the
owner API through the session and writer, not a full live acceptance run. The
RFC Python suite remains 195/195 (`/tmp/misaka-census-owner-python.log`). Docs
checks retain the 43 baseline issues and add none
(`/tmp/misaka-census-owner-docs.log`). Streaming JSON serializer scratch remains
outside the typed census-list reservation, as do header cache/decode owners.


The broader core test typecheck uncovered a pre-existing compile failure in
`palw_t12_full_activation_16000`: its independently named LIST has 37 entries,
but its declaration still said 36. Only that declared array length was corrected
to 37. The ordered names, comparison with the production list, prerequisite
checks, activation pin and schedules remain unchanged. The failed check log is
`/tmp/misaka-census-owner-core-tests-check.log`.


After that declaration fix, the complete core test typecheck passed:
`cargo check -p kaspa-consensus-core --tests --features private-integration-config,evm`
finished in 1m37s (`/tmp/misaka-census-owner-core-tests-final-check.log`). This
includes compilation of the core unit controls and integration test targets; it
is not runtime execution of all those tests. The four direct-source census
controls above are the runtime evidence for this change. The remaining local
filesystem headroom was about 1.3 GiB after this check; no new full codegen or
campaign run was attempted on that space.


### Cost driver preserves failed capital polls and shares the fee endpoints

A remaining measurement error was in the live cost driver rather than the
integrator: `tick` swallowed failed capital polls, so the next successful value
was integrated directly from the last successful value. The earlier control
for a present row with a missing value did not cover a failed RPC that produced
no row. Separately, first/last capital polls were taken after/before the fee
window snapshots, leaving different measurement endpoints.

The driver now records failed polls explicitly, with their error and an
unavailable status, without inventing a sink, DAA or balance. Recovering later
does not erase them. Both fee snapshots read the producer capacity shadow
inside the same native generation bracket as the UTXO/vesting reads and supply
the capital endpoints. Periodic sampling stays active while waiting for the
final snapshot. Capital observation requires explicit successful native rows,
available shadow state, one local import epoch, and exact sink/DAA endpoints
matching the fee window. A failed poll, unavailable import/shadow, changed epoch
or mismatched endpoints keeps both capital integrals, capital cost and total
cost UNKNOWN and records a BLOCKED finding. Missing polls also prevent claiming
a complete cap/window observation or locating the first conviction floor.

The successful integral is still a sampled estimate: its output names the
left-Riemann method and the assumption that balances stay constant between
polls. It does not measure every intermediate transition, actual wall-clock
capital financing, allocator/disk costs or a retention extension. It provides no
new decision input, source economics preset or release approval.

Controls exercise the real driver methods with synthetic bracketed rows and
RPC failures, not live nodes. Six new controls cover: endpoint/fork/import and
availability refusals; each retriable poll failure followed by recovery;
unstable native imports; shared snapshot endpoints; and a failed poll while
waiting for the final snapshot; and missing/malformed budget balances. Missing
budget balances cannot be labelled unsaturated, and boolean claim counters or
inexact/noncanonical amounts are refused. The final-snapshot failure case
preserves the failed row, then
receives a successful final snapshot, but still reports UNKNOWN capital and
total cost with a BLOCKED capital finding. The cost module's 16 controls passed
(`/tmp/misaka-cost-gap-controls-direct.log`). The complete RFC Python suite
passed all 201 controls (`/tmp/misaka-cost-gap-python-final.log`). The CLI help
entry point also starts correctly (`/tmp/misaka-cost-gap-help-final.log`). Docs
checks retain the same 43 baseline issues with none added
(`/tmp/misaka-cost-gap-docs.log`). Real cost measurement and the full RFC12 campaign remain
unrun. Working filesystem headroom was approximately 1.3 GiB; no large Rust
codegen was started for this Python-only change.


### Coinbase validation compares borrowed reward outputs

After the rendering buffer got a retained owner, validation still constructed
an entire expected coinbase just to hash-compare it with the received one. Every
native payout script was cloned again and held by that comparison transaction.
`CoinbaseManager` now has one shared reward renderer with building and comparison
sinks. The building sink retains the existing owning outputs. The comparison
sink consumes each expected `(value, borrowed script)` in canonical order,
compares the corresponding received output, and keeps only count/first-mismatch
scalars. Validator/PALW appended outputs are borrowed rather than cloned. Round
payee aggregation borrows its source scripts; it does not make an intermediate
owning script copy before output rendering.

`verify_coinbase_transaction` uses the comparison sink. It checks every field
previously included by `hashing::tx::hash(FULL)`: version, empty inputs, output
count/order/value/script, lock time, subnetwork, gas, payload and zero storage
mass. The cached transaction ID remains irrelevant, as it was to the hash.
Mismatch logs retain the same shape facts without keeping an expected output
collection. No reward formula, queue prefix, selected approval requirement,
wire field, hash definition, activation pin or public economic input changes.

The current production coinbase source and integration controls were compiled
directly against the compatible cached dependencies described above. All 16
controls passed, including the original manager/queue/reservation tests and
three new controls: mutated hashed fields and output order; a mixed
blue/red/round/receipt/carve reward stream; and allocation requests for 0/1/8
appended ML-DSA payout scripts. The mixed fixture covers own subsidy 0/1000,
selected-parent escrow 0/200, a merged red withholding 100, both red payout modes
and denied/entitled reds. These are controlled values, not a proposed policy.

A second harness also compiles the unchanged builder source from parent commit
`0c698bf21` and compares its complete transaction (including cached ID) with the
new builder for the same fixtures. All 26 controls passed (16 current plus 10
legacy manager controls). It also compares the new predicate with the old full
transaction hash test on changed metadata/output cases. Exact argv, cached
inputs, parent commit and source digests:
`/tmp/misaka-payout-legacy-comparison-build.json`; build/runtime logs:
`/tmp/misaka-payout-legacy-comparison-build.log`,
`/tmp/misaka-payout-legacy-comparison-runtime.log`.

A test-only thread-local System allocator observer counts requested bytes
(including realloc requests), not allocator rounding, live heap or RSS. It
observed 488 bytes for each of 0, 1 and 8 payouts in the small fixed fixture.
That confirms removal of payout-count-dependent copying in this comparison;
it is not a whole-node bound or a consensus cost constant. Payload serialization,
mergeset classification, miner-index and round-aggregation scratch still allocate
and need their own coverage. Template/block/mining-cache owning copies remain
open, as do all other RFC06/07/08/12 requirements not explicitly verified here.

`cargo check -p kaspad --lib --features private-integration-config` passed in
35.78s (`/tmp/misaka-payout-borrowed-validation-check.log`). It compiles the actual
validation consumer and template callers; the direct-source harness is runtime
manager evidence, not a full newly built node or acceptance campaign. No new
full codegen or large campaign was attempted with approximately 1.1 GiB left.


Documentation validation retained 43 known baseline issues with zero new issues
(`/tmp/misaka-payout-borrowed-validation-docs.log`). The final runtime rerun
uses the current production source, including the comparison renderer, and all
26 controls remain successful. The harness digest manifest matches those files.


### Held-GDN filer reaches convolution-window faults

The library already built and judged Q36 convolution-window evidence, but the
service filer's candidate list contained only recurrence transitions. A held
checkpoint with honest recurrence slices and a forged conv window therefore
exhausted its candidates without an accusation. The list now preserves the
existing transition slot/head order and appends Q36 SsmConv window slots/heads.
Each candidate uses the existing binding-derived held DA demand units. Once
answers authenticate, the filer builds the matching transition/window evidence,
runs that exact terminal, and signs the existing tag-244 accusation only on a
fault verdict. A conv-window accusation carries no artifact openings: that
terminal reads authenticated projection rows and state chunks, not operands.
No wire tag, table, commitment domain, registration path or fence is added.

There was a second refusal bug: failed evidence construction or a refused
terminal could be skipped as if the candidate were honest. Now only the explicit
`Evidence(NoFaultFound)` outcome advances. Other evidence/resource refusals stay
retryable. Loading the next candidate constructs its demands/answer slots before
changing the current index or answers; a failed load cannot pair a new candidate
with the old candidate's disclosures. Exhaustion leaves a valid last index.

The production filer module and its real Q36 dev-fixture controls were compiled
directly with compatible cached core/base0/hash/borsh dependencies. Source and
argv manifest: `/tmp/misaka-gdn-conv-filer-build.json`; build output:
`/tmp/misaka-gdn-conv-filer-build.log`. Ten controls exercise the actual module:
existing recurrence faults/honest cases/service-tick decisions plus four new
controls for conv-only faults, conv-window silence, refusal classification and
atomic candidate loading. The conv faults at heads 0, 1 and 3 pass through held
DA answer authentication, the existing memory/routing verdict function and the
conv-window terminal; the verdict is ExecutorGuilty with no operand openings.
The tiny fixture observed 13/14/16 demanded units respectively before accusation,
including its honest recurrence candidates; these are observations, not policy
limits. Silence after the recurrence answers produces the existing DefaultDue
signal at the window candidate. Refused material preserves already authenticated
answers and the current demand clock; restoring it permits loading the next
candidate. The Resource error classification control injects the typed refusal;
it does not measure a physical quota or full-node resource liveness.

The direct-source tests use the established test signing seam and synthetic
accepted-object lists, not a newly signed live-node campaign. They therefore do
not establish actual signature/carrier acceptance, chain conviction/settlement,
restart/reorg/prune behavior or production-scale costs. Candidate/case/evidence
allocation ownership, full model coverage, solo checkpoint-map GDN registration
and the remaining RFC06/07/08/12 requirements remain open. In particular the
larger candidate set is not claimed to fit a measured resource/session budget.
Public presets and the full-activation pin stay dormant.

`cargo check -p kaspad --lib --features private-integration-config` passed in
6.18s (`/tmp/misaka-gdn-conv-filer-check.log`), compiling the actual service host
against the extended case type. Runtime/documentation results are recorded below.


The final current-source run passed all ten controls in 12.50s
(`/tmp/misaka-gdn-conv-filer-current-runtime.log`); source digests in the build
manifest match the files. Docs checks retain 43 baseline issues and add zero
(`/tmp/misaka-gdn-conv-filer-docs.log`). One concrete remaining resource path is
`Qwen36Backend::gdn_inventory_openings_v2`: it currently opens every inventory
operand before the service creates its oracle, although a recurrence terminal
reads only its parameter slice and a conv-window terminal reads none. Omitting
unneeded operands from the final window accusation does not close that earlier
inventory construction/clone path or its lifetime. It remains in scope along
with candidate/case ownership and the real campaign.


### GDN parameter openings stream under the existing complete artifact root

`Qwen36Backend::gdn_inventory_openings_v2` previously materialized every artifact
operand and opened every row before the service built its GDN oracle. The step
and held filer both use this API, even though Q36 recurrence terminals only read
small parameter rows and a convolution-window terminal reads no operands.
The backend now calls `gdn_inventory_openings_v2.rs`, which selects the parameter
tensor names/layers of actual registered Q36 GdnStep descriptors. It does not
reimplement the court's per-head byte offset or parameter-length conventions.
Every canonical inventory row is still visited and hashed under the existing
leaf/node domains. Unselected weights and leaf metadata are discarded.

The helper builds the existing V1 per-leaf opening format directly during that
single stream. A fixed frontier holds at most 33 subtrees (V1 leaf_count is u32).
Merging adjacent subtrees appends the opposite subtree hash only to selected
leaves in that interval; the final fold promotes unmatched odd subtrees without
duplicating leaves or adding fictitious siblings. Selected operands keep at most
32 sibling slots. No vector of every leaf or every artifact operand is created
by this helper. Empty, noncanonical and unrepresentable-count streams return no
result. The original emitter's errors also abort rather than publish a prefix.
This is data identity authentication, not computation correctness certification.
No model review/tier, commitment domain, wire field, fence or approval rule changes.

Four direct-source runtime controls passed, using compatible cached base0/core
libraries for the unchanged emitter/backend and court functions. They compare
roots and complete V1 openings against the original materialized implementation
for trees of size 1..65 and sparse selections at 1023/1024/1025; include dense,
empty and sparse selections/odd promotions; compare a 10,000-row, 8,192-byte
unselected stream against the existing inventory summary; and reject empty,
duplicate and overflowed streams. In the real Q36 dev fixture the helper keeps
12 rows / 816 operand bytes instead of 801 rows / 187,886 bytes. Every selected
opening equals the legacy opening under the same complete root; the oracle
serves every GDN parameter row and refuses unrelated rows. An actual backend
execution/capture, evidence construction and step court query using this reduced
oracle return the expected NoFaultFound for the honest step, rather than a
missing-operand refusal. The quantities are toy-fixture observations, not
production-model memory limits or approved economic measurements.

Exact argv, dependency paths and source digests:
`/tmp/misaka-gdn-stream-build.json`; build/runtime output:
`/tmp/misaka-gdn-stream-build.log`, `/tmp/misaka-gdn-stream-current-runtime.log`
(four controls, 0.19s). `cargo check -p kaspad --lib --features private-integration-config`
passed in 18.92s (`/tmp/misaka-gdn-stream-check.log`), compiling both actual service
consumers with the backend change. The standalone runtime uses the new helper
source directly; it is not a complete newly built node binary or live campaign.

Still open: reservations and lifetime ownership for the selected Vec/paths and
oracle copies; the upstream emitter's plan/read/overlap workspace; case/evidence
ownership and all-model bounds. Selection is derived from the registered
descriptors, so the tiny fixture's retained size is not asserted for every
possible profile. Every weight is still read to compute the root and sibling
paths, so large-model I/O/time/energy/capital costs require real measurement.
These changes neither complete RFC06 nor arm a public preset or UNKNOWN gate.


Documentation checks retained the 43 known baseline issues and added none
(`/tmp/misaka-gdn-stream-docs.log`). The final runtime source digests match the
files in the build manifest. Public full activation remains `None`; the model
tiers, mandatory per-claim approvals and dormant bootstrap guards are unchanged.


### GDN opening and oracle allocations keep their typed reservations

The streamed subset now returns `GdnInventoryOpeningsV2`, rather than a bare Vec.
It exposes borrowed openings and has no Clone, consuming extraction or mutable
Vec access. A ticket is admitted before stream construction, selector insertion,
layout-checker name growth, selected-row copies or opening-buffer growth.
Quotes derive from actual compiler layouts, descriptor counts, actual name and
operand lengths, and V1's maximum 32 sibling slots. The helper's selected-row,
name and path copies use fallible allocations. The existing core oracle
constructor still uses standard Clone/collect after its separate admission;
this does not make every system allocator failure recoverable. Checked request
arithmetic rejects unrepresentable sizes before payload allocation.
The selector is now one borrowed-key BTreeSet, covered by the existing 64-bit
node envelope under strict native admission. Non-strict legacy use on other
pointer widths is preserved rather than claiming a pinned BTree bound there.

When the opening Vec grows, the new slot envelope is admitted while the old
buffer remains live; the old slot reservation is returned only after successful
growth. The final owner keeps the selected payload and a conservative initial
stream/checker envelope until drop. It is a typed request bound, not an exact
heap/RSS measurement. The upstream inventory emitter's plan/read/overlap buffers
and the borrowed artifact/source are separate, still-open ownership scopes.

`into_oracle_v2` separately reserves the actual independent operand clones the
core proven-operand constructor makes and authenticates their openings. Its
`GdnInventoryOracleV2` keeps both the original opening owner and oracle ticket
through the held filer's use; payload fields drop before their tickets. The
actual service host and trait now return that composite owner. The step service
borrows the opening owner through probing. A typed `try_clone_from_v2` adapter
reserves before making an independent copy of pre-existing openings; it does
not retroactively cover the source's construction. No wire/ID, model tier,
claim-approval rule, public fence or economic parameter changes.

Eight isolated-process capacity cases cover zero, one below the stream's
observed typed request peak, exact peak, one below/exact oracle construction,
bad root, and one below/exact independent opening copy. The oracle's requested
bytes include both its cloned operands and wrapper layout. The live result or
composite oracle prevents a reservation beyond the remaining budget; drop and
all refusal paths return the complete ticket. The source sizing fixtures are
created with admission disabled and dropped before arming, or explicitly kept
as pre-existing source inputs outside the new copy's quota. These test logical
admission/ownership, not OS quota enforcement or production-model RSS.

A direct-source harness compiles the new helper and actual held filer together,
re-exporting compatible cached base0 implementation modules as dependencies for
unchanged artifact execution/court routines. All 16 controls pass: the six helper
controls (including the eight isolated capacity cases) and ten actual filer
controls, retaining honest/forged recurrence, conv-window, silence/default,
refusal/retry and service-tick coverage. The local proxy exists only in the
harness, not the product. Exact argv/dependency paths/source digests:
`/tmp/misaka-gdn-owned-filer-build.json`; logs:
`/tmp/misaka-gdn-owned-filer-current-build.log`,
`/tmp/misaka-gdn-owned-filer-current-runtime.log` (16 passed, 12.44s).

The actual private service library typecheck also passes:
`cargo check -p kaspad --lib --features private-integration-config`
(`/tmp/misaka-gdn-owned-filer-current-check.log`, 5.76s). Initial checks caught a
stale test field and missing admission-function imports; both were corrected.
No result from a failed or stale build is used as current runtime evidence.

Still open: copies from these borrowed inputs into accusation objects, the case
book, outbox/carrier ownership and cached/reorg/restart lifetimes; emitter
workspace, whole-model resource/time/economic bounds and the live campaign.
In particular owning the input opening/oracle tickets does not reserve the
separately cloned artifact openings in a queued accusation. The public activation
pin and all unapproved source economics remain dormant.


The final source rerun passes all 16 controls in 12.42s
(`/tmp/misaka-gdn-owned-filer-final-build.log`,
`/tmp/misaka-gdn-owned-filer-final-source-runtime.log`). The manifest digests
match the current helper/filer/test files. Documentation checks retain 43 known
baseline issues with zero new issues (`/tmp/misaka-gdn-owned-filer-docs.log`).
The full campaign and remaining ownership scopes above are not promoted to PASS.


## Q36 inventory reusable read buffer admission — 2026-10-23

The canonical emitter now constructs a scoped `Q36RowReaderV1` with a typed
base reservation. Stored, zero, parameter-wire and rotary rows reserve a
checked byte envelope before growing their reusable read buffer. Growth keeps
the old ticket live until fallible allocation succeeds; refusal preserves the
old bytes, length and ticket. The byte payload drops before its retained
tickets. Only slices escape the buffer. Parameter/rotary serialization writes
into the admitted slice rather than growing an unreserved Vec via extend.
`InventoryBuildError::Resource` distinguishes local admission refusal from
invalid artifact data. This is logical typed admission, not an allocator/RSS
upper bound or an OS crash test.

The prechange production emitter at parent
`eb5c2a73376ac5e7f21c69687ee189e6f679b10f` generated the golden roots.
The current emitter reproduces them for graph v5/v6/v7/v8 at read sizes
1, 7, 17 and 4096 on the actual small Q36 fixture. Five isolated quota cases
cover zero, one below/exact old-plus-new growth peak, refusal/retry, and a
Stored read refused before any sink call. They also check retained reservation,
overflow refusal, unchanged old bytes and full release after drop.

Fresh runtime harness: `/tmp/misaka-inventory-reader-build.json` records argv,
parent commit and source digests. It compiles each inventory source's production
prefix before the first old cfg(test) module; only the new child-module path is
made absolute. Existing inventory test modules are omitted because their private
fixture helpers are not exported by the compatible cached dependency. The new
reader tests are included from their actual source. Cached base0/core/hash/Borsh
rlibs supply unchanged dependencies, not current-node acceptance evidence.
`/tmp/misaka-inventory-reader-final-build.log` and
`/tmp/misaka-inventory-reader-runtime.log`: 4 test entries pass (including the
child entry that returns when unselected, the parent golden-root generator,
the five-process quota driver, and the 16 current root comparisons).
The actual product wiring also passes
`cargo check -p kaspad --lib --features private-integration-config`
(`/tmp/misaka-inventory-read-buffer-check.log`, 19.04s).

Still open: inventory plan/order/distinct-index storage, parameter-view
construction (which precedes parameter wire-buffer admission), overlapping-row
merge, downstream result collections and accusation/case/outbox copies. Small
rows borrow already-planned storage and do not retroactively cover its owner.
Whole-model costs and the 15-gate live campaign remain unexecuted/UNKNOWN.
The activation pin remains None; no model tier, claim approval, wire identity,
public economics or bootstrap permission changes.

The complete base0 test targets also typecheck via
`cargo check -p misaka-palw-base0 --tests`
(`/tmp/misaka-inventory-reader-tests-check.log`, 1m15s); this does not execute
those existing tests. Documentation checks retain 43 baseline issues and zero
new issues (`/tmp/misaka-inventory-reader-docs.log`).


## Inventory parameter owners and mapped extent reads — 2026-10-24

User clarification: K3 formal specification/manifest/weights and signed campaign
inputs are not created. This is missing acceptance input, not permission to
substitute a toy fixture, Kimi-linear implementation or an invented K3 runtime.
The complete RFC06/07/08/12 objective remains open.

With the user's cleanup instruction, this continuation removed 938,675,573 bytes
of reproducible artifacts: nine task-owned temporary test executables and one
completed incremental session whose query cache contains this exact worktree
path. The removal manifest is `/tmp/misaka-cleanup-20261024-removed.json`;
selection evidence is `/tmp/misaka-owned-incremental-cleanup-20261024.json`.
No source, runtime/campaign logs, build manifests, cached dependency rlibs or
unrelated incremental sessions were removed. Filesystem availability increased
from 1.7 GiB to 2.6 GiB at that observation; later builds can consume space.

`InventoryParamRowsV1` now owns the decoded parameter table and its native
reservation. The envelope uses actual table length, A16QuantParams layout and
owner header with checked arithmetic, before fallible Vec reservation. Only an
immutable slice escapes; no Clone or raw Vec extraction can separate data from
the ticket. Every canonical Q36 plan parameter read and each emitted parameter
view now uses this owner; widening remains a logical view of the original rows.

For a mapped, budgeted routed expert, the inventory reader follows the same
source selection as the engine and reads only its immutable parameter extent
through the file descriptor. It reserves encoded scratch before allocation and
keeps that ticket through decoded-row construction; scratch drops before the
returned decoded owner. It does not call the engine's expert-LRU admission and
does not load the expert's unrelated weights. Other stored parameters borrow
existing artifact bytes for decoding. Engine execution and public param_rows
remain unchanged. IO/decode errors retain the existing inventory operand refusal;
local capacity refusal is Resource. These are logical typed envelopes, not RSS
measurements or a claim to handle every process OOM.

Fresh direct-source runtime includes current qwen36 production code, inventory
production code, and both actual new leaf-module test suites. It omits the old
qwen36/inventory test modules and the two old cfg(test)-only qwen36 fixture
wrappers; always-compiled real fixture constructors remain included. Only child
module paths are made absolute. Compatible cached base0/core/hash/Borsh/rayon/
Blake2 dependencies supply unchanged modules, not current-node acceptance.
Argv and all included source digests: `/tmp/misaka-inventory-params-build.json`;
build/runtime logs: `/tmp/misaka-inventory-params-build.log`,
`/tmp/misaka-inventory-params-runtime.log`.

Six top-level entries pass in 0.14s (two are child entries that return unless
selected). The quota drivers actually select five read-buffer cases and six
parameter cases: owned and mapped one-below/exact admission, malformed decode,
and file read error. They check retained decoded ownership, scratch release
before return, and full release on failure/drop. All owned/mapped parameter rows
match the original engine decoder on the real small fixture; mapped misses,
bytes_read and held expert bytes stay unchanged. Full canonical roots match the
prechange pins for graph v5/v6/v7/v8 at four read sizes on both owned and mapped
artifacts (32 comparisons), again without populating the expert LRU.

Still open: plan Vec/Strings/Small rows and temporary derived parameter tables,
order/distinct-index/sort scratch, overlapping-row merge and downstream owning
collections; accusation/case/outbox/carrier lifetimes and whole-model cost
bounds. Artifact loading and engine expert residency have separate ownership
scopes. These controls are not the real multiprocess/100k/pruned/crash campaign.
The public activation pin stays None; normal bootstrap refusal, model tiers,
claim approval rules and dormant source economics remain unchanged.

Product/typecheck results for this change:
`cargo check -p kaspad --lib --features private-integration-config` passes in
31.17s (`/tmp/misaka-inventory-params-private-check.log`), and
`cargo check -p misaka-palw-base0 --tests` passes in 20.38s
(`/tmp/misaka-inventory-params-final-tests-check.log`). These metadata checks
include current production wiring and existing test targets but do not execute
the complete existing test suite. Documentation checks retain 43 baseline
issues and zero new issues (`/tmp/misaka-inventory-params-docs.log`).
