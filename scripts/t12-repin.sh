#!/usr/bin/env bash
# scripts/t12-repin.sh — the testnet-12 re-pin, mechanical (docs/t12-rcore-launch-checklist.md §5).
#
#   scripts/t12-repin.sh                                  dry run: every pin, "pinned vs computed"
#   scripts/t12-repin.sh --apply --reason "<one line>"    rewrite the drifted t12 / mainnet / layout pins
#                                                         and the kit/doc copies in place (one reason comment
#                                                         each), then run the affected tests
#   scripts/t12-repin.sh --selftest                       the decision rules on the last harvest logs (no build)
#
# --apply works in rounds: a drifted genesis constant is re-pinned first (the params ids hash it, so the
# fingerprint, the twins and their copies are only right once it is), the tree is rebuilt and read again,
# then the rest is re-pinned, and the last round is a build that shows no drift. Each round is a cargo
# build of kaspa-consensus-core's tests (the first one on a cold target takes ~12 minutes on the Mac Studio).
#
# Other flags: --drift-only (print only rows that are not ok), --allow-verdict (ADR-0150's corpus digest,
# only with a moved rule manifest), --from-log <file> (dry run: parse saved harvest logs instead of
# building), --log-dir <dir>, --no-verify (with --apply: skip the affected-test run).
#
# The computed side is this tree's own build: `cargo test -p kaspa-consensus-core` over
# consensus/core/tests/t12_repin_values.rs (REPIN lines: ids, geneses, txids, classes, kit facts) and
# the pin tests that print their value before asserting it (the "one fence moved it" twins, T41, the t11
# dormant-parity dump, the dormant ring root, the t11 verdict roots, the corpus verdicts), plus the
# committed class manifests read as JSON. Nothing is typed. The registry of pins — file, anchor,
# spelling, scope, the tests that hold them — is scripts/t12_repin.py.
#
# It never moves a testnet-11 / testnet-10 / devnet / simnet pin: a drift there refuses the whole
# --apply and names the test that would fail. A t11 golden that hashes a v22 root moves only with a v22
# layout move and with every non-root value beside it unchanged (see t12_repin.py).
#
# Exit: 0 no drift (dry run) / applied, converged and the affected tests pass; 1 drift (dry run); 2 usage;
#       3 refused or not every pin checked; 4 the harvest build failed; 5 applied but a test failed or the
#       rounds did not converge.
#
# Build environment: the caller's CARGO_TARGET_DIR / CARGO_BUILD_JOBS (default 4) / CARGO_INCREMENTAL
# (default 0). Local only: it contacts no host, pushes nothing, downloads nothing but locked crates.
set -euo pipefail
REPO=$(cd "$(dirname "$0")/.." && pwd)
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-0}
command -v python3 >/dev/null || { echo "t12-repin: python3 is required" >&2; exit 2; }
exec python3 "$REPO/scripts/t12_repin.py" "$@"
