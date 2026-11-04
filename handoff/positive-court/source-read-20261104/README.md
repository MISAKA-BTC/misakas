# Native source read connection candidate — 2026-11-04

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
`/tmp/misaka-positive-f9a4a9c80-batch.{log,exit}`. Runtime success is pending,
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
