#!/usr/bin/env bash
# Build the regression binaries inside a resource-capped scope.
#
# These hosts run production testnet-10. An unconstrained `cargo build --release` saturates every
# core, and the production node and miner are on the same CPUs — during an earlier build the load
# average on an 8-core host reached 8.5, which is a real risk of dropped blocks measured against a
# real chain. Building at all on a production host is a compromise; building without a cap is not
# one worth making.
#
# `systemd-run --scope` gives the build its own cgroup with hard CPU and memory ceilings, so the
# kernel enforces the limit rather than `nice` merely suggesting it. `nice` still helps for I/O
# ordering, so both are used.
#
# The better answer is to build elsewhere and copy the binaries in. Until the toolchain for that
# exists, this is the containment.
set -euo pipefail

BASE=${BASE:-/var/lib/misaka-regression}
# Leave at least two cores for whatever else the host is doing — including, on these hosts,
# accepting blocks and mining them.
CPUS=${CPUS:-$(( $(nproc) > 3 ? $(nproc) - 2 : 1 ))}
MEM=${MEM:-6G}
LOG=${LOG:-$BASE/build.$(date -u +%Y%m%dT%H%M%SZ).log}

export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || { echo "cargo not on PATH" >&2; exit 1; }

build() {
  nice -n 19 cargo build --release -j "$CPUS" \
    --manifest-path "$BASE/src/Cargo.toml" \
    -p kaspad -p kaspa-testing-integration \
    --bin kaspad --bin regress-rpc
}

# `-j` is the cap that always works: N compile jobs cannot occupy more than N cores, and it needs no
# privileges. The cgroup is better where it is available — it bounds memory too, and it constrains
# the linker and build scripts that `-j` does not — but creating a system scope needs polkit, which
# an unprivileged operator account over ssh does not have. Probe rather than assume: a build that
# refuses to start because of an authentication prompt is not safer than one that runs capped.
if systemd-run --scope --quiet --collect true >/dev/null 2>&1; then
  echo "building in $BASE/src: cgroup CPUQuota=$((CPUS * 100))% MemoryMax=$MEM, cargo -j $CPUS -> $LOG"
  systemd-run --scope --quiet --collect \
    -p "CPUQuota=$((CPUS * 100))%" -p "MemoryMax=$MEM" -p "CPUWeight=20" -p "IOWeight=20" \
    bash -c "$(declare -f build); CPUS=$CPUS BASE=$BASE build" > "$LOG" 2>&1
else
  echo "building in $BASE/src: no usable cgroup scope, falling back to cargo -j $CPUS + nice -> $LOG"
  build > "$LOG" 2>&1
fi

tail -1 "$LOG"
for b in kaspad regress-rpc; do
  printf '%s  %s\n' "$(sha256sum "$BASE/src/target/release/$b" | cut -d' ' -f1)" "$b"
done
