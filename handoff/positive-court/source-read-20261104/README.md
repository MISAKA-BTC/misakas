# Native source read connection candidate — 2026-11-04

## Source-preparation work correction: test targets typechecked

The same clean frozen `efb8862fc` passed Core and Consensus test-target
checking with `--features evm --tests`, exit 0 in 16m44s. The
[check result and artifact hashes](results/efb-work/check-result.json)
include the original log and exit. Together with the strict library Clippy
below, this proves those static checks for this source only. It executes no
regression and does not check other features or later source changes.
The queued source-preparation Merkle regression has started building its
actual executable on that unchanged source; its runtime result is pending.
Native Assignment/Case and the original real-model/Gate C/payment/credit
objective remain incomplete.

## Source-preparation work correction: strict library Clippy passed

Frozen `efb8862fc` passed Core and Consensus library Clippy with
`--features evm --lib -- -D warnings`, exit 0 in 5m31s. The
[exact-source result and artifact hashes](results/efb-work/clippy-result.json)
include the original log, exit, source identity and executed queue script.
The same guarded queue has started all enabled Core/Consensus test-target
typechecking, followed by the source-preparation Merkle-work regression.
Those results remain pending. This static check does not validate the later
source adapter/provider/Batch test changes or complete native intake,
real-model execution, independent Gate C, payment or credit.

## Frozen f9 Batch Core regression passed

`f9a4a9c80` built its Core unit-test executable in 69m55s and passed the
single focused `batch_native_dispatch_opens_and_finishes_the_same_rooted_demand`
test in 2.68s (exit 0, 5,900 other tests filtered out). The result now replaces
the earlier pending queue status below. The post-run binary SHA256 is
`ccacfce66004821a4e9ed9bd3bcc5b1cdf5e22f0729a4eaa07e74692b04c5749`.
[Exact result, scope and artifact hashes](results/f9-batch/result.json)
accompany the original build/test log, exit, source identity and queue script.

This is a Core fold test using constructed populated/ValidatedTransaction
inputs, not ordinary consensus Full UTXO/script validation. It opens a rooted
Batch and completes one planned parameter response; full numerical material
is intentionally incomplete and verification refuses. A separately inserted
receipt tests only reading/terminal refusal and is not an accepted computation
verdict. It does not establish successful Positive Assignment/Case intake,
real-model PositivePassed, Gate C, settlement or credit. The guarded queue
has advanced to clean `efb8862fc` and started its strict Core/Consensus Clippy;
those later check and runtime results remain pending.

## Frozen 50d legacy native regression: four passed

The existing `50d5eda4c` consensus test binary passed four additional exact
legacy G14 tests: replay/reorg roots (5.32 s), same-DB node restart (5.71 s),
pruned-import carriage continuation (2.92 s), and poster-escrow reward once
across reorg/replay/redemption (12.03 s). Each ran in a separate process and
reported exactly one pass, exit zero. Binary SHA256 before and after remained
`583efd6d80d36d6de19d4bc9db21d0e96a6700fcf686975fa634253f4289cadd`.
The XFS project 713 quota audit records inherited project assignment and the
original 256 MiB hard limit; the runner unmounted it on completion.

[Raw result](results/50d-legacy/misaka-positive-50d-legacy-results.json) and
[artifact hashes and limits](results/50d-legacy/assessment.json) accompany
all original logs, exits and the executed script. These are existing tiny
legacy fixtures. The restart test retains its DB Arc while restarting node
services, and the pruned-import test installs carriage through the harness;
neither expands to a full cold DB close or complete network IBD claim. The
old reward route is not Positive Assignment WorkerPayment. These passes apply
only to frozen 50d, not the current integrated source, and do not complete
the original real-model/native/Gate C/payment/credit objective.

## Remaining OOM signed Batch fixture integrated

`31d2550ef` ports the remaining three-file uncommitted OOM test change onto
the expanded Family/Prefix/archive fixture. The old patch did not apply and
was not used as a whole-file replacement. All eleven existing test entry
points remain, with one added `g14_native_funded_batch_candidate_v1` entry.
The new path uses the existing material constructor's executor argument to
regenerate the 2/2 Input-history fixture in the signer's own FP job context;
it does not rewrite an already executed job. Existing Family recursive source
history calls explicitly select their previous mode. The source pledge uses
the derived custodian count, and the new Batch mode derives its allocation
upper bound. It cannot overwrite the legacy generator's requested dump.

The all-branch service-book checks run on the new material fixture and remain
on existing production-material Family paths. The legacy synthetic generator
no longer enters that material-dependent block. The explicit 8/64 test FP
quanta apply only to the new Batch path; existing Family clock parameters
remain unchanged. No released Params or production admission code changes.

Rustfmt parsing of both fixture modules and the wrapped include fragment,
whitespace checks, and a comparison preserving all old test entry names
passed. Typechecking and execution have NOT yet passed. The guarded queue
`/tmp/misaka-positive-31d2550ef-verify.sh` waits for the provider-bound tests,
requires their newly added regression to appear as passed (not a zero-test
success), then checks all enabled Core test targets and runs five individual
regressions: legacy generator, signed Batch, ordinary Family financial path,
scoped Prefix checkpoint/reorg, and legacy OAR2 source/archive replay. Each
runtime log must report exactly one passed test. Logs/exits are
`/tmp/misaka-positive-31d2550ef-<stage>.{log,exit}`. This is tiny existing G14
regression coverage; it is not successful RFC-0045 Assignment/Case admission,
real-model execution, Gate C, payment or DAG credit.

## Per-unit bounds at provider retrieval

`719bb0e90` passes the canonical per-unit raw-data and encoded-body bounds
into `StoredProviderV2::unit_bounded` before each charged provider attempt.
The default delegates to the existing borrowed-unit lookup for preloaded,
independently bounded stores. File/network adapters doing I/O during this
call must override it and apply the supplied bounds before retrieval;
transport framing/probes retain separate finite process allowances.
Post-retrieval geometry, authentication, budget, and final native-view checks
remain in place. No transport or production service-loop caller is added.

The regression exercises the real responder with the existing authenticated
ragged prompt fixture. Its provider refuses the old unbounded entry point,
checks each supplied bound including the short tail, and must receive no
further calls after response budget exhaustion. This is local responder
coverage, not native acceptance or a service receipt. Rustfmt and whitespace
checks passed. Runtime remains pending in the guarded IBM queue
`/tmp/misaka-positive-719bb0e90-verify.sh`, after both stages of `5917693dd`
succeed; the queue runs all `miner::positive_v2::unit_bounds_tests` with
`positive-miner` enabled. Logs/results are
`/tmp/misaka-positive-719bb0e90-unit-bounds.{log,exit}`. It checks the exact
preceding clean source before switching; it does not alter the active build.

## Native authorization before protected local storage

`5917693dd` changes the own-node Miner source constructor to resolve a
literal accepted source request before binding its irreversible local Case
namespace. Previously a correctly signed but unadmitted Result could bind
that namespace before `respond` first checked native state. Different outputs
for the same execution share a CaseId, while the protected store freezes the
ResultId, so the earlier order could prevent opening another Result for that
Case even after the process released its writer lock.

The constructor now requires its actual Consensus session and RequestId.
Local signer/service authentication precedes native archive work; native duty
resolution precedes protected-store binding and journal creation/recovery.
It rechecks the native view after recovery before returning. Existing local
I/O history and explicit Create/Resume/DiscardUnanchoredTail modes remain;
there is no automatic reset on failure or reorg. This adapter still has no
production service-loop caller, and no successful native Case is claimed.

The added regression uses signed local test data, a refusing resolver and
real RocksDB. All three recovery modes must leave no journal or immutable
Case binding on native refusal; after a full database close/reopen, another
signed Result with the same CaseId must still be bindable by the lower-level
local store. The resolver is an explicitly negative test double, not an
accepted native session. Formatting and whitespace checks passed; the test
has not executed yet.

The guarded IBM queue `/tmp/misaka-positive-5917693dd-verify.sh` waits for
the preceding `efb8862fc` verification and then checks kaspad's enabled test
targets with `evm,positive-miner-client`, followed by the actual protected-store
regression. Stage logs/results use
`/tmp/misaka-positive-5917693dd-<stage>.{log,exit}`. These results and the full
original objective remain pending; the earlier frozen `50d5eda4c` runtime
passes below do not validate this later constructor change.

## Frozen-candidate native runtime passed

`50d5eda4cd92c506eef20b042a6f85b505fd4877` passed both actual native
publication-generation tests and the private native initial-relay trial.
The test binary built in 66m02s. The relay trial completed in 4.69s, including
its separate child process; the child and parent are one logical relay case,
not two distinct cases. Both Cargo invocations exited 0. See
[the source/binary-pinned runtime record](results/50d-runtime.json),
[generation log](results/50d-generation.log) and [relay log](results/50d-relay.log).

The relay trial covers the one-sompi-short refusal, exact registration charge,
full RocksDB close/reopen with identical root, signed authorization/custody
and actual fee/change UTXOs, distinct-carrier retry without a second relay
charge, independent native block replay, and rollback across the production
shallow economic-tie boundary. The rolled-back canonical rows and UTXOs
follow the selected branch while original archive bytes remain retained.
The future Assignment funding outpoint in this standalone relay test is
declared only; no Assignment escrow or Case is admitted by this trial.

Binary SHA-256:
`583efd6d80d36d6de19d4bc9db21d0e96a6700fcf686975fa634253f4289cadd`.
Strict library Clippy and all-test-target typechecking also passed on this
same frozen source, as recorded below. The later preparation-work correction
is a different candidate and is not credited with these results.

The guarded queue has now selected clean `f9a4a9c80` and started the actual
Core Batch fixture build. Its verified live Cargo PID at collection was
1481807. The `efb8862fc` Clippy/check/source-work test queue still waits for
that result. Live source-service execution, actual Assignment/Case admission,
real-model canonical native completion, Gate C, worker/service/refund UTXOs,
all credit consumers and full recovery/regression verification remain open.

## Source preparation Merkle work correction

`efb8862fc` corrects the native source read projection's conservative
`work_per_read`. The previous projection priced codec/hash/I/O over only
the encoded body. Artifact authentication hashes both 64-byte children
at each Merkle branch; the encoded path supplies only one sibling.
The projection now includes the same maximum 32-level, 128-byte-per-level
opening allowance used by native assigned-source intake, in addition to
the actual body read. Original schedule rates and signed budgets are
unchanged. The projection cannot increase a signed budget, accept a
receipt or grant payment/credit.

The new adversarial test enumerates odd and power-of-two inventory widths
through `u32::MAX`, checks an independent full-path hashing lower bound,
and checks overflow and invalid rates. The exact numeric methods, field
definitions and added test passed in an extracted Rust harness with
`-D warnings`: [scope and raw output](results/source-preparation-work-extracted.json).
The reduced error enum in that harness is explicitly a stand-in. This is
not full Core typechecking or native source-service execution.

The guarded `/tmp/misaka-positive-efb8862fc-verify.sh` on IBM waits for the
preceding Batch verification owner and its successful result before selecting
the clean `efb8862fc` candidate. It then runs strict Core/Consensus library
Clippy, all test-target typechecking and the actual added Core integration
test. Logs/results use `/tmp/misaka-positive-efb8862fc-<stage>.{log,exit}`.
All these integrated results remain pending. The frozen running `50d5eda4c`
source is unchanged, and the full original objective remains open.

## Canonical job input audit

The existing three-process short-job report explicitly records
`canonical_job: false` and `native_authority: false`. Its 16-input/1-output
execution cannot stand in for the RFC-0045 canonical Assignment job.
Running the exact two extracted production numeric helpers against the
retained original-context pins gives 1,023 prefill / 2 decode for both
SmolLM2 registrations (8,192 positions), and 255 / 2 for Pythia-70M
(2,048 positions). See [the source-pinned numeric result](results/canonical-job-shape.json).
The token-budget field uses the existing native positions-plus-one convention;
the registered model layout/context is not shortened or modified.

This extraction checks only the context-to-count calculation. It does not
derive or authenticate an Assignment ID, prompt, accepted registration,
source catalogue, coverage, fee, reservation or role contract, and executes
no model. Pythia's retained program is wide Q24; this numeric result does
not establish support in the initial ExactStream Assignment profile.
The live integration driver must obtain its actual job, prompt and complete
resource plan from the native registered metadata and signed terms rather
than copying the earlier short-job execution or its budgets.

## Batch regression follow-up

`f9a4a9c80dbe64657863a9d22f85b97db1eed198` ports the pending OOM
worktree correction for `batch_native_dispatch_opens_and_finishes_the_same_rooted_demand`.
The fixture selects a parameter opening, so its RPC target and retained
material assertions now compare that exact parameter leaf instead of prompt
IDs. The private fixture also enables the existing TIR conviction rule used
by its numerical verification path. Production rules and the existing rent,
reservation, duplicate, incomplete-material and terminal checks are unchanged.
Rustfmt parsed the changed test file successfully; `git diff --check` passed.
These checks execute no tests.

The frozen `50d5eda4c` runtime build remains untouched. Its Cargo/rustc
processes were confirmed live while preparing this follow-up. A separate
guarded IBM script waits for that owner's exit and requires successful
Clippy, test-target checking, generation tests and relay execution before
selecting the clean `f9a4a9c80` source and running the focused Core Batch test.
The queue is `/tmp/misaka-positive-f9a4a9c80-verify.sh`; its log and result are
`/tmp/misaka-positive-f9a4a9c80-batch.{log,exit}`. At queue creation runtime success was pending (now superseded by the f9 pass above),
and the full Assignment/Case, three-process, certification and settlement
goal is still open.

Implementation candidate `1376b0e9b`, on top of publication generation
`7583923ac` and integration `f5c463f70`. This is not an end-to-end acceptance
or a successful runtime source-service result. These candidates have not
been pushed to `pre`.

The real `ConsensusApi` implementation now delegates to a private native read
session. Construction replays the rooted Assignment/Case archive using the
writer's existing arithmetic/source replay. It selects a literal accepted
op3 request, reconstructs op4 receipts, uses the original geometry/service
cutoff, and checks outstanding service fees against the remaining Case funds.
The unit projection has a typed native allocation lease and an original replay
heap/work bound. Source status is not inferred from a local preparation log
or a Panel acknowledgement.

Every authorize, unit-bound and status operation rechecks native publication
generation, selected parent, state root, accepted Case head, import epoch and
stability, and the actual unspent funding output's amount and payer script.
The snapshot does not implement the live read trait; only the private node
session does. Existing historical obligations use original retained policy,
not the current intake policy. Public activation is unchanged.

The publication generation is process-local and atomically published with its
state. Reusing an earlier selected parent or even the exact same state Arc
cannot reuse its generation. The two tests embedded in the production cache
implementation passed in an extracted harness using arc-swap 1.7.1 and an
opaque substitute for the unrelated VirtualState fields. This verifies the
cache implementation, not the complete Consensus module or source service.
New source files pass rustfmt; git diff whitespace checks pass. Whole-file
format checks of the large existing parent modules report pre-existing
format differences; those files have not been broadly reformatted.

## Pending authoritative verification

IBM source directory: `/dev/shm/misaka-oom-positive-next`.
Shared Cargo target: `/dev/shm/misaka-positive-pre-isolated-target`.
The preceding candidate `f6fd18ea8` is still compiling its native relay trial
at the time of this record. Its log is
`/tmp/misaka-positive-f6fd-relay.log`. Its running source is left unchanged.
Candidate objects were transferred separately through a Git bundle.

A queued command waits for that Cargo process to exit, checks that the source
is clean at the expected preceding commit, then checks out `1376b0e9b` and runs:

```sh
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
CARGO_TARGET_DIR=/dev/shm/misaka-positive-pre-isolated-target \
cargo +1.93.0 check --offline --locked \
  -p kaspa-consensus-core -p kaspa-consensus --features evm --tests
```

Output: `/tmp/misaka-positive-source-1376-check.log`.
Exit code file: `/tmp/misaka-positive-source-1376-check.exit`.
No check success is asserted before inspecting those results.

Still required: full typecheck/Clippy, real accepted op3/op4 source sessions,
state-change/import/UTXO denial through the live API, cold restart and local
spent-I/O preservation, and the complete same-candidate Assignment/Case,
PositivePassed, Gate C, settlement/reward and DAG-credit trial. A native
source-read implementation does not itself create any of those transitions.

## First run and follow-up

The `f6fd18ea8` native relay trial built successfully in 65m04s and then
failed at the competing-branch selection assertion. Both branches were
UTXO-valid; selected DAA/work were 13/28 and competing DAA/work were 15/32.
The preceding registration, cold-close/reopen, distinct-carrier retry and
independent replay assertions were reached, but this is an overall FAILED
trial. Its raw log, binary hash and explicit scope are in
[the result record](results/f6fd-relay.json) and [raw log](results/f6fd-relay.log).

Static inspection found that the original common ancestor was too old for
the production two-DAA economic-tie reorg limit. `c826615a5` instead puts the
underpaid non-registration in the common prefix, before the successful
registration. It explicitly checks the actual shallow depth and preserves
the common underpaid change UTXO. The consensus rule is unchanged. Runtime
verification of the corrected fixture remains pending.

Core typechecking of `1376b0e9b` reported unresolved references to the
source owner's capacity helper. `8aaf6ac4c` makes that existing helper
visible to the sibling read projection and imports it. A queued verification
waits for the preceding check process to finish, selects that exact candidate,
and runs all Core/Consensus test-target checks, then strict library Clippy
if checking succeeds. Logs and exit files use
`/tmp/misaka-positive-source-8aaf-{check,clippy}.{log,exit}` on IBM.
No successful source-service execution or completion of the overall goal is
claimed by these changes.

## Corrected candidate typecheck result

`8aaf6ac4c489325b551a0c0f4cf58019f2e584f1` passed Core and Consensus
all-test-target typechecking with exit code 0 in 15m36s. The source checkout
was clean when collected. See [the exact-source record](results/8aaf-check.json)
and [Cargo log](results/8aaf-check.log). This command executes no tests.
Strict library Clippy is now running. A separate guarded command waits for
both check and Clippy success, then builds/runs the actual native cache's two
`lkg_generation_tests` and the corrected native relay trial, on that same
source candidate. Relay execution uses the existing isolated enforced XFS
project-quota image. Logs and exit files are
`/tmp/misaka-positive-source-8aaf-{generation,relay}.{log,exit}` on IBM;
the native executable hash will be recorded in
`/tmp/misaka-positive-source-8aaf-runtime.sha256` after its build.
No runtime success is asserted yet. The full original goal remains open.

## Strict Clippy result and complete correction candidate

The `8aaf6ac4c` strict library Clippy run exited 101 with ten diagnostics:
four clone-on-Copy uses, four nonminimal boolean expressions, one unnecessary
filter_map and one redundant borrowed comparison. See the
[complete Clippy log](results/8aaf-clippy.log). The earlier successful test-target
typecheck result remains valid for that exact source; it is not Clippy success.

`57b252442` fixes the first six diagnostics and `806a07f13` fixes the remaining
four emitted later in the completed run. Changes use direct copies, equivalent
Option predicates, a filter preserving the original ordinal iterator, and a
borrow-equivalent comparison. No lint suppression or policy changes were added.
The full correction candidate `806a07f13` is queued behind the existing
verification owner. Its guarded script runs strict library Clippy first,
all Core/Consensus test-target checks second, and actual generation and relay
execution only if every preceding stage succeeds. Stage logs and exit files
are `/tmp/misaka-positive-806a07f13-{clippy,check,generation,relay}.{log,exit}`;
the script records its exact source and executable hash separately. These
results remain pending. Candidates after `f5c463f70` are still local and have
not been pushed to `pre`.

## Consensus acceptance argument correction

`806a07f13` cleared the ten Core diagnostics and completed its RocksDB native
dependency build. Strict Clippy then exited 101 with two too-many-arguments
findings in the Consensus acceptance wrappers. See
[the completed log](results/806-clippy.log). No runtime stage ran after that
failure.

`50d5eda4c` groups the existing immutable state, state parameters and block
context into one tuple argument in the two affected wrappers, and updates
all callers. Values, branch/clock selection, original-material transport,
Batch handoffs and acceptance policy remain unchanged. No additional lint
suppression was added. Its guarded Clippy -> all-test-target check -> native
generation tests -> quota-backed relay trial pipeline is now running on IBM.
Artifacts use `/tmp/misaka-positive-50d5eda4c-<stage>.{log,exit}`. This is still
an unverified runtime candidate, not completion of Assignment/Case or the
original whole-chain/three-process goal.

## Strict Clippy passed

`50d5eda4cd92c506eef20b042a6f85b505fd4877` passed strict Core and Consensus
library Clippy with `-D warnings`, exit code 0. Completed identical-source
Core/dependency artifacts were reused; the final invocation finished in
24.04s. The checkout was clean when collected. See
[the exact-source record](results/50d-clippy.json) and [raw log](results/50d-clippy.log).
All-test-target checking is now running on the same candidate. Actual native
generation and corrected relay execution remain pending; the complete
original three-process/chain integration goal remains open.

## Same-candidate static verification complete; runtime build started

`50d5eda4cd92c506eef20b042a6f85b505fd4877` also passed all Core/Consensus
test-target typechecking, exit code 0, in 16m01s. The checkout was clean when
collected. See [the exact-source check record](results/50d-check.json) and
[raw log](results/50d-check.log). Strict library Clippy and all test-target
checking now both pass for this same source.

The guarded native generation/relay stage has started compiling its executable.
No runtime success is asserted. The source-read backend and publication
freshness implementation, corrected shallow-reorg fixture, and integration
lint corrections are ready for the user's requested `pre` integration. Later
commits in this branch contain evidence/documents only; `main` remains outside
this push. Accepted Assignment/Case, live source sessions, PositivePassed,
independent Gate C approval, real worker/service/refund payments, role-process
integration, all purpose-specific credit predicates, and the remaining
restart/reorg/pruning/IBD invariants are still required by the original goal.
