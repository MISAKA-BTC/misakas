# RFC-0007 native bootstrap source failures stay failures — 2026-10-24

## Production repair

The native paid Lock previously called `paid_beacon_material_v5(...).ok().flatten()`.
An invalid/missing frozen source row, contradictory funding, or local projection capacity failure
therefore looked like an absent source seed and could choose the pinned bootstrap seed instead.
The approved-Final latch also discarded malformed Final rows with `filter_map(...ok())`, potentially
reopening bootstrap after its only historical Final became unreadable.

The native Lock now retains one checked authority and checked ledger and uses a dedicated checked
source getter. `None` means exactly: authority sponsor absent, aux108/109/110 all absent, and no
same-context funding/source/promise in the checked ledger. Any partial presence, malformed bytes,
key/binding mismatch or capacity refusal remains an error. Funded targets use the same fee/cost,
source-event and collector semantics as before. A valid funded but pending source inventory may
still produce no seal seed; the existing private bootstrap rule then applies until the first
positive approved Final. Errors do not take that branch.

The native path constructs source events from the same reserved ledger and the existing RFC07
selected-approval event helper. Its Borsh compatibility reader wraps that same pure helper. There
is no nested String-valued ledger read or serialized-event round trip in the native getter. Source
and promise vectors and concrete collector scratch allocations are reserved before construction;
row decoding retains its preflight reservation. Capacity errors remain `NativeResource` through
the actual fold rather than a verdict against a block/peer.

The checked historical Final latch decodes all encountered Final rows and checks their keys;
a malformed row is an error even if another row already closed bootstrap. Existing decoded zero
receipt/round records remain non-approved observations. The legacy boolean latch conservatively
closes bootstrap on errors; native selection uses the typed result. Historical closure is not
redefined by a later claim's live state, and the latch grants no approval or credit.

## Verification

Actual core runtime verification passed on 2026-10-25. The new regression reuses the real tiny Program and
native authority fixture from the paid-claim reader tests and invokes atomic `apply_paid_claim_v2`
Lock. It checks genuine unfunded success; partial/malformed aux108/109/110, missing funding and
contradictory sponsor refusal with an unchanged root and no lock; malformed Final refusal; and
real `NativeDemandCheckerV1` capacity refusal with an unchanged root, followed by a successful
retry when capacity is available. Funding projections here are private input/decoder controls,
not a native funding campaign or a measurement of source economics.

The coordinated run includes existing normal Lock/Receipt/tick/Final and FS projection controls.
A separate existing legacy latch test preserves the valid-zero/positive historical behavior.

Coordinated actual-core command:

```sh
TMPDIR=/private/tmp RUST_MIN_STACK=67108864 \
CARGO_TARGET_DIR=/tmp/misaka-rfc06-12-build-20261024 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  test -p kaspa-consensus-core --lib paid_claim_read -- --nocapture
```

The actual unit binary was
`/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_consensus_core-210c32867c0e9198`,
SHA-256 `76a62f2b44259277d527bd9a06ecb5e67f6dee3101adb729286f8774bc2a587f`.
The `paid_claim_read` filter passed **5/5** in 0.22 seconds, including both new bootstrap
failure/retry regressions. The same binary passed `selected_approval_tests` **8/8**, then the
explicit `bootstrap_window_closes_at_the_first_approved_paid_final` filter **1/1** (that test is
also included in the eight-test filter; these are not fourteen distinct tests).

Logs are `/tmp/misaka-rfc-parallel-paid_claim_read-20261025.log`,
`/tmp/misaka-rfc-parallel-selected_approval_tests-20261025.log`, and
`/tmp/misaka-rfc-parallel-bootstrap_window_closes_at_the_first_approved_paid_final-20261025.log`.
These are scoped runtime results, not a whole-core-suite pass: the parallel finance suite still
had two fixture setup/expectation failures under investigation when these results were recorded.
The earlier document check retained 43 known issues with zero new issues.

## Boundaries

No public activation, selected-source or V6 activation, interactive bootstrap acceptance, source
independence claim, K3 artifact, campaign gate, model tier or credit rule is changed. The normal
activation refusal and `None` activation height remain. Funded FS StageMessage and native V6
source campaigns remain unexecuted; small fixtures are not their acceptance evidence.
