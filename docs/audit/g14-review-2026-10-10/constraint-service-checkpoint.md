# Constraint service checkpoint — 2026-10-11

This checkpoint adds three real public-bond service test cases for flat constrained
decode: a forbidden token, premature EOG, and an admitted but beaten token.
Each case requires the outside verifier to reacquire the original job and reach
an included `ConstrainedDecodeFlat` proof at position 2, with producer and all
FullValid Panel sanctions. The producer capture is cleared from the fixture.

Validation at this checkpoint:

- `cargo test -p kaspad --lib --locked --no-run` passed (24.57 s).
- The test binary lists all three added tests.
- `git diff --check` passed.
- The three service tests have **not yet been executed**. Their assertions are
  intended acceptance criteria, not demonstrated guarantees.

The production implementation remains the previously pushed `55f1ed9d7`.
This checkpoint changes test fixtures only and does not complete full G14.
