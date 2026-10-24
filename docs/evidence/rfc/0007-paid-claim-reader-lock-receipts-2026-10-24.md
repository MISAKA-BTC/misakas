# RFC-0007 paid claim RPC lock and receipt projection — 2026-10-24

## Production connection

The node's `getPalwPaidClaim` RPC (op 247) calls `palw_paid_claim_read_v1`.
The RFC-0012 acceptance checker launcher requires that response's selected seats and positive quorum.
The reader previously populated those fields exclusively from an interactive round row (aux 123).
Noninteractive Program/Pipeline/Segmented/TypedSpec enrollments instead admit their ordinary lock
in aux 116 and complete seat receipts in aux 117; explicit V6 bindings have their own effective lock.
Thus an admitted noninteractive lock was reported with no seats and quorum zero, preventing this
production caller from starting its selected checkers.

The reader now projects the noninteractive effective lock and accepted receipts for round zero,
using the existing V6 effective-lock selector when an explicit binding exists. Other requested rounds
do not inherit round-zero seats or receipts. Interactive round validation remains separate.
Ordinary operator selection is checked with the native fold's rank, operator deduplication,
tie-break and ordering. Enrollment/context/assignment, seat identity, positive quorum and lock time
remain bound to the prospective authority. Every reported ordinary receipt binds the exact enrollment,
class, signer, round zero, empty message root, assignment, seed, source context, evidence/transcript
roots, and signed receipt window, including the lock's lower time bound.

Base-index, authority, round, ordinary lock, receipt and Final decoding distinguish absence from a
present malformed row. Base and enrollment indexes must identify the same primary authority.
Conflicting ordinary/V6 namespaces, an orphan V6 lock, StagedBeacon/initial-lock mixing and receipts
without a lock refuse the read. FS keeps its existing initial aux-116 source lock; it never substitutes
for the requested round's assignment or accepted receipts. Ordinary receipts for an interactive
class refuse, matching retained-state routing invariants. Explicit V6 bindings are rebuilt using
the same existing authority helper as restore, before the effective lock is selected.

Before displaying Final, a targeted check follows the existing retained-state invariants: complete
selected-verifier inputs, canonical claim Final state, identity/class/lifetime/clock, last effective
lock seed/context/quorum, completed round count and transcript. The last interactive round is chosen
independently of the requested display round. This check does not run a full-route consistency scan
or make the RPC a Final authority. Initial lock, round and receipt times cannot exceed the rooted
ledger clock. The ordinary operator-ranking scratch map/vector is reserved before allocation.

## Verification

The regression constructs a real tiny Program registration, posted job and salted claim commit
through the kernel API. It then installs a validated prospective private-mechanics authority and
executes the production atomic `apply_paid_claim_v2` Lock and Receipt folds. The RPC helper must
observe three assigned seats, positive quorum and, after three accepted receipts, round-complete.
Negative controls substitute enrollment, signer, round or pre-lock time; replace the same claim's
lock seed/assignment while retaining stale receipts; and corrupt the base index, Final, lock or receipt.

The actual core binary passed the initial two runtime tests (2/2, 0.17 seconds) before the Final and
clock extension; the build took 10m37s including the shared-target lock wait. The extended actual core run passed **3/3** tests (0.18 seconds runtime; 9m03s build including
shared-target lock waits). It adds actual ordinary native tick/Final projection checks, ten isolated Final
metadata substitutions and a two-round FS projection control where querying the first round must
still bind Final to the last round. Future last-round accepted, lock and receipt times also refuse. FS row projections are synthetic validated inputs, distinct from
the actual ordinary native Lock/Receipt path.

```sh
TMPDIR=/private/tmp RUST_MIN_STACK=67108864 \
CARGO_TARGET_DIR=/tmp/misaka-rfc06-12-build-20261024 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  test -p kaspa-consensus-core --lib paid_claim_read -- --nocapture
```

Initial runtime/build output is `/tmp/misaka-rfc07-paid-reader-test.log`; extended output is
`/tmp/misaka-rfc07-paid-reader-final-test.log`.
Document checks retain 43 known reference issues with zero new issues; owned-file whitespace checks pass.

## Boundaries

This is one production observation connection and a tiny native-fold regression, not a public-chain
campaign or evidence of source independence. The prospective authority setup is not an end-to-end
signed enrollment campaign. The FS projection test does not execute funded V5 source collection or native StageMessage. Existing
`verify_bootstrap_v2` rejects interactive classes, and that refusal is preserved; synthetic round rows
are not substitute acceptance evidence for that missing vertical path. No actual V6 source campaign,
four-adapter campaign, K3 run, whole-model
cost bound or all-stage verification completion is claimed. The reader grants no approval or credit.
Activation remains `None`; ordinary selected-source/V6 bootstrap activation refusal, model tiers and
claim-level selected approval are unchanged.
