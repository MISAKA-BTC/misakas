#!/bin/zsh
# t12-fleet-identity.sh — compare this checkout's testnet-12 shipped ids with the public fleet's source (lane FLEET, 2026-10-26).
#
#   scripts/verify/t12-fleet-identity.sh [FLEET_REV]      (default 247ade1f9, the rev the public fleet runs)
#
# Puts FLEET_REV in a detached scratch worktree, copies consensus/core/tests/t12_fleet_identity_247ade1f9.rs into it, runs the
# dump test in both trees (through buildslot), diffs the FLEETID lines (params / identity / schedule id, fork id at DAAs below
# 17,000, every armed fence below 17,000), and removes the scratch worktree and both targets. Exit 0 = identical.
set -u
FLEET_REV=${1:-247ade1f9}
HERE=$(git rev-parse --show-toplevel)
SCRATCH=$HERE/.fleet-$FLEET_REV
OUT=${OUT:-$HERE/.fleet-identity-out}
BUILDSLOT=${BUILDSLOT:-$HOME/Downloads/MISAKA-wt-b/buildslot.sh}
TEST=t12_fleet_identity_247ade1f9
export CARGO_INCREMENTAL=0 TMPDIR=/private/tmp
mkdir -p "$OUT"
[[ -d $SCRATCH ]] || git -C "$HERE" worktree add --detach "$SCRATCH" "$FLEET_REV" || exit 2
sed '/^\/\/ ---- branch-only below/,$d' "$HERE/consensus/core/tests/$TEST.rs" >"$SCRATCH/consensus/core/tests/$TEST.rs"
run() { # $1 = tree, $2 = log, $3 = extra cargo test args (this checkout only)
  CARGO_TARGET_DIR=$1/target "$BUILDSLOT" env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
    cargo test --offline --manifest-path "$1/Cargo.toml" -p kaspa-consensus-core --test $TEST ${=3} -- --nocapture >"$2" 2>&1
  echo "rc=$?" >>"$2"
}
run "$SCRATCH" "$OUT/fleet.log" ""
run "$HERE" "$OUT/branch.log" "${EXTRA_TESTS:-}"
grep '^FLEETID ' "$OUT/fleet.log" >"$OUT/fleet.ids"
grep '^FLEETID ' "$OUT/branch.log" >"$OUT/branch.ids"
if [[ ${KEEP:-0} != 1 ]]; then
  rm -rf "$SCRATCH/target" "$HERE/target"
  git -C "$HERE" worktree remove --force "$SCRATCH"
fi
if [[ ! -s $OUT/fleet.ids ]]; then echo "no ids from $FLEET_REV (see $OUT/fleet.log)"; exit 3; fi
if diff -u "$OUT/fleet.ids" "$OUT/branch.ids"; then echo "IDENTICAL to $FLEET_REV"; else echo "DIFFERENT from $FLEET_REV"; exit 1; fi
