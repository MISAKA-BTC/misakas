#!/bin/bash
# deploy-t12/build-release-5104.sh — build the release binaries ON 5.104.81.23 (the only fleet host
# that is not serving the public), into $INCOMING/<rev12>/ with SHA256SUMS.
#
#   usage (as root on 5.104): ./build-release-5104.sh <full commit sha pushed to origin>
#
# All four hosts are x86_64 AMD EPYC, Ubuntu glibc 2.39 (checked 09-23), and .113/ibm already run a
# binary copied from one build, so one build serves the fleet. kaspad's default features include
# `evm` (t12 activates the EVM lane at DAA 0; a build without it refuses to start).
#
# It does NOT touch /root/misakas or its worktrees (other sessions use them): it makes its own
# blobless clone in /root/t12-rel/src and its own target dir /root/t12-rel/target.
set -euo pipefail
. "$(dirname "$0")/fleet.env"
COMMIT=${1:?usage: $0 <commit sha>}
REPO=${REPO:-https://github.com/MISAKA-BTC/misakas.git}
SRC=/root/t12-rel/src
TARGET=/root/t12-rel/target
JOBS=${JOBS:-6}
say() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
die() { say "ABORT: $*"; exit 1; }

[ "$(hostname)" = vmi3272359 ] || die "build on 5.104.81.23 only (never on .113/ibm — public nodes, OOM history)"
avail_mib=$(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo)
[ "$avail_mib" -ge 10240 ] || die "MemAvailable ${avail_mib} MiB < 10 GiB — the kaspad link step alone takes several GiB; stop the private chain first or lower JOBS"
free_gb=$(df -BG --output=avail /root | tail -1 | tr -dc 0-9)
[ "$free_gb" -ge 30 ] || die "only ${free_gb} GB free"
# shellcheck disable=SC1091
. /root/.cargo/env

if [ ! -d "$SRC/.git" ]; then
    say "cloning $REPO (blobless) into $SRC"
    git clone --filter=blob:none --no-checkout "$REPO" "$SRC"
fi
git -C "$SRC" fetch --filter=blob:none origin "$COMMIT"
git -C "$SRC" checkout --detach --force "$COMMIT"
[ -z "$(git -C "$SRC" status --porcelain)" ] || die "$SRC is dirty"
REV12=$(git -C "$SRC" rev-parse --short=12 HEAD)
OUT="$INCOMING/$REV12"
mkdir -p "$OUT"
say "building $REV12 ($(git -C "$SRC" log -1 --format=%s | cut -c1-90)) with -j$JOBS → $OUT"
set +e
( cd "$SRC" && CARGO_TARGET_DIR="$TARGET" nice -n 10 cargo build --release --locked -j "$JOBS" \
      --bin kaspad --bin misaka --bin palw-class --bin misaka-dnsseeder ) > "$OUT/build.log" 2>&1
rc=$?
set -e
grep -E '^error|Finished' "$OUT/build.log" | tail -5 || true
[ "$rc" -eq 0 ] || die "cargo build failed (rc=$rc) — see $OUT/build.log; nothing copied"
for b in kaspad misaka palw-class misaka-dnsseeder; do
    [ -x "$TARGET/release/$b" ] || die "$b was not built — see $OUT/build.log"
    install -m 0755 "$TARGET/release/$b" "$OUT/$b"
done
echo "$COMMIT" > "$OUT/REV"
(cd "$OUT" && sha256sum kaspad misaka palw-class misaka-dnsseeder > SHA256SUMS)
cat "$OUT/SHA256SUMS"

# ---- identity probe: the release's own fingerprint and genesis, read from a node that can reach
# nobody (loopback listener, no DNS seed, no peers, fresh throwaway appdir, no PALW flags). `env -i` and a
# throwaway HOME: kaspad writes ~/.misaka/testnet-12/endpoints.json on start, and root's is the live
# node's registry (the drill kit's 09-24 finding) — the probe must not rewrite it. The same probe runs
# on the Mac as probe-identity-local.sh. ----
for p in 26991 26994; do [ -z "$(ss -ltnH "sport = :$p")" ] || die "probe port $p is in use"; done
PROBE_DIR=$(mktemp -d /root/t12-rel/probe-XXXXXX)
say "identity probe in $PROBE_DIR (isolated: 127.0.0.1:26991, json 26994, no peers)"
mkdir -p "$PROBE_DIR/home"
env -i PATH="$PATH" HOME="$PROBE_DIR/home" "$OUT/kaspad" --testnet --netsuffix=12 --appdir="$PROBE_DIR/app" --yes \
    --listen=127.0.0.1:26991 --rpclisten-json=127.0.0.1:26994 --nogrpc --nodnsseed --disable-upnp --outpeers=0 \
    > "$OUT/probe.log" 2>&1 &
PID=$!
FP=""; GEN=""
for _ in $(seq 1 60); do
    sleep 2
    out=$(python3 "$(dirname "$0")/t12check.py" --port 26994 --probe 2>/dev/null || true)
    FP=$(sed -n 's/^FP=//p' <<<"$out"); GEN=$(sed -n 's/^GENESIS=//p' <<<"$out")
    [ -n "$FP" ] && [ -n "$GEN" ] && break
done
PREMINE=$(python3 "$(dirname "$0")/t12check.py" --port 26994 --premine 2>/dev/null | sed -n 's/^PREMINE_TXID=//p' || true)
kill -INT "$PID" 2>/dev/null || true
for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 2; done
kill -9 "$PID" 2>/dev/null || true
LOGFP=$(grep -oE 'Consensus params fingerprint: [0-9a-f]{64}' "$OUT/probe.log" | head -1 | awk '{print $4}')
rm -rf --one-file-system "$PROBE_DIR"
[ -n "$FP" ] && [ "$FP" = "$LOGFP" ] || die "probe failed or disagrees (rpc '$FP' vs log '$LOGFP') — see $OUT/probe.log"
grep -q 'PALW DRILL' "$OUT/probe.log" && die "the release probe announced a PALW DRILL chain — this is not a public build"
printf 'EXPECT_FP=%s\nEXPECT_GENESIS=%s\nPREMINE_TXID=%s\n' "$FP" "$GEN" "${PREMINE:-__FILL_ME__}" > "$OUT/IDENTITY"
for g in $FORBIDDEN_GENESIS ${DRILL_GENESES:-}; do [ "$GEN" = "$g" ] && say "WARNING: this release's genesis ${GEN:0:16}… is FORBIDDEN (fleet.env FORBIDDEN_GENESIS / DRILL_GENESES) — install-*.sh will refuse it"; done
cat <<EOF

Fill deploy-t12/fleet.env on the Mac:
  REV=$REV12
  KASPAD_SHA256=$(grep ' kaspad$' "$OUT/SHA256SUMS" | cut -d' ' -f1)
  MISAKA_SHA256=$(grep ' misaka$' "$OUT/SHA256SUMS" | cut -d' ' -f1)
  PALW_CLASS_SHA256=$(grep ' palw-class$' "$OUT/SHA256SUMS" | cut -d' ' -f1)
  SEEDER_SHA256=KEEP   # or $(grep ' misaka-dnsseeder$' "$OUT/SHA256SUMS" | cut -d' ' -f1) to swap seeders
  EXPECT_FP=$FP
  EXPECT_GENESIS=$GEN
  PREMINE_TXID=${PREMINE:-__FILL_ME__}
(also in $OUT/IDENTITY; every install-*.sh switch re-checks each node's own fingerprint line and genesis.
 Compare with probe-identity-local.sh on the Mac for the same commit: the three values must be equal.)
EOF
