# Native source read connection candidate — 2026-11-04

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
