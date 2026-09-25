#!/usr/bin/env bash
# deploy-t12/seeders/10-build-seeder.sh — OPTIONAL. Cross-build misaka-dnsseeder for the Linux
# hosts on THIS Mac. Not needed for the regenesis (see ../SEEDERS.md §1): the running 1174b965 has
# no genesis/params dependency. Build only if you want the seeder's provenance to equal the release.
#
# Cheaper alternative (no extra build at all): if the node release build on 5.104.81.23 already
# produces misaka-dnsseeder (`-p misaka-dnsseeder` in its cargo line), fetch THAT binary to the Mac
# and hand it to 20-stage.sh — one build, one provenance.
#
#   WT=/path/to/wt-t12 ./10-build-seeder.sh
#
# Output: out/misaka-dnsseeder-t12-<rev12>  (+ .sha256, and out/REV)
source "$(dirname "$0")/lib-seeders.sh"

SCRATCH=/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/24499bec-23cb-41cf-b06f-867a448df39b/scratchpad
WT="${WT:-$SCRATCH/wt-t12}"
TARGET_TRIPLE=x86_64-unknown-linux-gnu.2.35   # hosts are Ubuntu 24.04 / glibc 2.39, x86_64
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$SCRATCH/target-seeder-linux}"   # deletable as a unit
OUT_DIR="$SEED_DIR/out"

avail_gb=$(df -g /private/tmp | awk 'NR==2{print $4}')
if [ "$avail_gb" -lt 15 ]; then
  echo "only ${avail_gb} GB free under /private/tmp (< 15 GB): NOT building." >&2
  echo "command, for when there is room:" >&2
  echo "  cd $WT && CARGO_TARGET_DIR=$CARGO_TARGET_DIR cargo zigbuild --release --locked -p misaka-dnsseeder --target $TARGET_TRIPLE" >&2
  exit 4
fi

rev=$(git -C "$WT" rev-parse --short=12 HEAD)
# Only the seeder's own inputs have to be clean; the worktree may carry unrelated WIP.
dirty=$(git -C "$WT" status --porcelain -- misaka-dnsseeder misaka-endpoints consensus/core/src/network.rs \
          rpc/core rpc/wrpc/client Cargo.lock | wc -l | tr -d ' ')
if [ "$dirty" != 0 ]; then
  echo "seeder inputs are dirty in $WT at $rev — refusing (git status above paths)" >&2
  git -C "$WT" status --short -- misaka-dnsseeder misaka-endpoints consensus/core/src/network.rs rpc/core rpc/wrpc/client Cargo.lock >&2
  exit 5
fi

command -v cargo-zigbuild >/dev/null || { echo "cargo-zigbuild not installed" >&2; exit 6; }
echo "building misaka-dnsseeder @ $rev for $TARGET_TRIPLE into $CARGO_TARGET_DIR ($avail_gb GB free)"
( cd "$WT" && nice -n 10 cargo zigbuild --release --locked -p misaka-dnsseeder --target "$TARGET_TRIPLE" )

bin="$CARGO_TARGET_DIR/x86_64-unknown-linux-gnu/release/misaka-dnsseeder"
file "$bin" | grep -q 'ELF 64-bit LSB.*x86-64' || { echo "not an x86-64 ELF: $(file "$bin")" >&2; exit 7; }
mkdir -p "$OUT_DIR"
out="$OUT_DIR/misaka-dnsseeder-t12-$rev"
cp "$bin" "$out"
shasum -a 256 "$out" | tee "$out.sha256"
echo "$rev" >"$OUT_DIR/REV"
echo
echo "next: CONFIRM=yes ./20-stage.sh c5104 $out     (smoke-tests there), then the other three."
echo "      put the sha256 above into fleet.env SEEDER_SHA256 only if you intend to SWAP (default KEEP)."
