#!/usr/bin/env bash
# deploy-t12/build-release-local.sh — build the fleet's release binaries ON THE OPERATOR'S MAC: a cross
# build (cargo-zigbuild) for x86_64 Linux with a glibc 2.39 floor — ibm, .113 and 5.104 are AMD EPYC,
# Ubuntu 24.04, glibc 2.39 — from a CLEAN checkout of one commit, into $KIT/.cache/<rev12>/, the
# directory `distribute-from-mac.sh binaries` ships to all three hosts. This is the PREFERRED path
# (PLAN.md §4): it keeps a 20-minute, multi-GiB build off 5.104, which hosts public nodes and has an OOM
# history. build-release-5104.sh stays as the fallback. LOCAL ONLY: it contacts no host, pushes
# nothing, pulls no image; cargo fetches only what Cargo.lock names (OFFLINE=1: not even that).
#
#   ./build-release-local.sh [<commit>]        default: HEAD of the repo this kit sits in (committed state)
#
# What it does, in order (each step aborts the run on failure; nothing is half-written into .cache):
#   1. <commit> → a fresh detached `git worktree` of exactly that commit at $WORK/src (clean by
#      construction, and checked).
#   2. cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu.$GLIBC_FLOOR \
#          --bin kaspad --bin misaka --bin palw-class --bin misaka-dnsseeder
#      — the same four binaries in ONE invocation as build-release-5104.sh (one invocation = the same
#      feature unification), the toolchain rust-toolchain.toml pins, kaspad's default features (incl.
#      `evm`: t12 activates the EVM lane at DAA 0; a build without it refuses to start). RUSTFLAGS and
#      friends from the caller's shell are dropped, so the bytes depend on the commit, not the shell.
#   3. every binary: an x86-64 ELF, interpreter /lib64/ld-linux-x86-64.so.2, NEEDED only glibc's own
#      libraries (the C++ runtime and libgcc are linked in), and no GLIBC_x.y symbol version above the
#      floor → SHA256SUMS, REV, BUILD-INFO.
#   4. the identity: probe-identity-local.sh (this kit's) against THE RELEASE kaspad itself —
#      in a linux/amd64 container on a Mac (below), or directly on an x86_64 Linux build machine —
#      → IDENTITY (EXPECT_FP / EXPECT_GENESIS / PREMINE_TXID). NATIVE_PROBE=1 also builds the commit's
#      kaspad for this machine (dev profile) and probes it: the three values must be equal (they are
#      functions of the params and the genesis, not of the binary or the CPU).
#   5. prints the fleet.env lines to paste.
#
# The container probe (a Mac cannot exec an x86_64 Linux binary): `docker run --pull never --network
# none --platform linux/amd64 $PROBE_IMAGE` — an image ALREADY on this machine whose linux/amd64
# variant has glibc ≥ the floor, bash and python3 (e.g. rust:latest = Debian 13, glibc 2.41). Nothing
# is pulled; `--network none` leaves the probe node loopback only, on top of the probe's own isolation
# (127.0.0.1 listeners, no peers, no DNS seed, throwaway appdir and HOME, `env -i`). colima's docker
# runs amd64 under qemu (colima.yaml `binfmt: true`), so the probe is slow (PROBE_WAIT_S) but it runs
# the exact bytes the fleet will run.
#
# Env:
#   CARGO_TARGET_DIR  default $WORK/target (re-using it makes a re-run incremental)
#   JOBS              cargo -j (default 4 — other work shares this machine; the thin-LTO link of kaspad
#                     alone peaks at several GiB)
#   GLIBC_FLOOR       default 2.39 (the fleet's glibc; zig links against that version's symbol set)
#   WORK              default $HOME/.cache/misaka-t12-rel (the clean worktree and the default target dir)
#   PROBE             auto (default: direct on x86_64 Linux, docker on a Mac when PROBE_IMAGE is set,
#                     else skip) | docker | direct | skip
#   PROBE_IMAGE       the local image for the container probe (required for PROBE=docker)
#   PROBE_WAIT_S      how long the probe waits for the node's identity over RPC (default 3600 in the
#                     container — kaspad's start-up court self-test under qemu took 22 min on 2026-09-25
#                     with the Mac busy, ~1 min natively — and 180 direct)
#   NATIVE_PROBE=1    also probe a native dev build of the same commit and compare (a second build)
#   OFFLINE=1         cargo --offline
#   KEEP_SRC=1        keep $WORK/src after the run
#   FORCE=1           overwrite an existing $KIT/.cache/<rev12>/ (default: refuse — a published
#                     sha256 must not be silently replaced)
set -euo pipefail

KIT=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$KIT/../.." && pwd)
COMMIT_ARG=${1:-HEAD}
GLIBC_FLOOR=${GLIBC_FLOOR:-2.39}
JOBS=${JOBS:-4}
WORK=${WORK:-$HOME/.cache/misaka-t12-rel}
SRC=$WORK/src
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$WORK/target}
TRIPLE=x86_64-unknown-linux-gnu
BINS=(kaspad misaka palw-class misaka-dnsseeder)
CACHE=${CACHE:-$KIT/.cache}
say() { printf '[build-local %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
die() { say "ABORT: $*"; exit 1; }
lsha() { (command -v sha256sum >/dev/null && sha256sum "$1" || shasum -a 256 "$1") | cut -d' ' -f1; }

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) HOST_KIND=linux-x86_64 ;;
    Darwin-*)     HOST_KIND=mac ;;
    *)            HOST_KIND=other ;;
esac
PROBE=${PROBE:-auto}
if [ "$PROBE" = auto ]; then
    if [ "$HOST_KIND" = linux-x86_64 ]; then PROBE=direct
    elif [ -n "${PROBE_IMAGE:-}" ]; then PROBE=docker
    else PROBE=skip; fi
fi
case "$PROBE" in docker|direct|skip) ;; *) die "PROBE must be auto|docker|direct|skip" ;; esac
[ "$PROBE" != direct ] || [ "$HOST_KIND" = linux-x86_64 ] || die "PROBE=direct needs an x86_64 Linux machine"

# ---- 0. tools (nothing is installed here: a missing piece is the operator's decision) ----
for t in git cargo cargo-zigbuild zig python3 file; do command -v "$t" >/dev/null || die "$t is not installed"; done
READELF=""
for r in readelf llvm-readelf /opt/homebrew/opt/llvm/bin/llvm-readelf /usr/local/opt/llvm/bin/llvm-readelf; do
    if command -v "$r" >/dev/null 2>&1; then READELF=$(command -v "$r"); break; fi
done
[ -n "$READELF" ] || die "no readelf / llvm-readelf (brew's llvm has one) — the glibc floor cannot be checked"
[ "$PROBE" != docker ] || { command -v docker >/dev/null && docker info >/dev/null 2>&1; } \
    || die "PROBE=docker but no running docker (colima start?)"

# ---- 1. the commit, and a clean checkout of exactly it ----
COMMIT=$(git -C "$REPO" rev-parse --verify "$COMMIT_ARG^{commit}") || die "no commit $COMMIT_ARG"
REV12=${COMMIT:0:12}
# the compiler the commit pins (rust-toolchain.toml) must have the Linux std already — nothing is installed here
PIN=$(git -C "$REPO" show "$COMMIT:rust-toolchain.toml" 2>/dev/null | sed -n 's/^channel *= *"\(.*\)"/\1/p' | head -1)
[ -n "$PIN" ] || die "$REV12 has no toolchain channel in rust-toolchain.toml"
if command -v rustup >/dev/null; then
    rustup target list --installed --toolchain "$PIN" 2>/dev/null | grep -qx "$TRIPLE" \
        || die "toolchain $PIN has no $TRIPLE std (or is not installed) — the operator runs: rustup target add --toolchain $PIN $TRIPLE"
fi
if [ "$COMMIT_ARG" = HEAD ] && [ -n "$(git -C "$REPO" status --porcelain --untracked-files=no)" ]; then
    say "NOTE: $REPO has uncommitted changes — they are NOT in this build (it builds the commit $REV12)"
fi
if [ -z "$(git -C "$REPO" branch -r --contains "$COMMIT" 2>/dev/null | head -1)" ]; then
    say "NOTE: no remote-tracking branch here contains $REV12 (as of the last fetch) — push it before the fleet runs it"
fi
OUT="$CACHE/$REV12"
if [ -e "$OUT" ]; then
    [ "${FORCE:-0}" = 1 ] || die "$OUT exists (FORCE=1 to rebuild into it; its SHA256SUMS may already be in fleet.env)"
    rm -rf "$OUT"
fi
mkdir -p "$WORK"
if git -C "$REPO" worktree list --porcelain | grep -qx "worktree $SRC"; then
    git -C "$REPO" worktree remove --force "$SRC"
fi
[ ! -e "$SRC" ] || die "$SRC exists but is not a worktree of $REPO — move it away"
git -C "$REPO" worktree add --detach "$SRC" "$COMMIT" >/dev/null 2>&1 || die "git worktree add $SRC $COMMIT failed"
cleanup_src() { [ "${KEEP_SRC:-0}" = 1 ] || git -C "$REPO" worktree remove --force "$SRC" >/dev/null 2>&1 || true; }
trap cleanup_src EXIT
[ "$(git -C "$SRC" rev-parse HEAD)" = "$COMMIT" ] || die "$SRC is not at $COMMIT"
[ -z "$(git -C "$SRC" status --porcelain --untracked-files=all)" ] || die "$SRC is not clean"

# ---- 2. the build ----
STAGE="$CACHE/.$REV12.partial"
rm -rf "$STAGE"; mkdir -p "$STAGE"
cargo_flags=(--release --locked -j "$JOBS" --target "$TRIPLE.$GLIBC_FLOOR")
[ "${OFFLINE:-0}" = 1 ] && cargo_flags+=(--offline)
for b in "${BINS[@]}"; do cargo_flags+=(--bin "$b"); done
say "building $REV12 ($(git -C "$SRC" log -1 --format=%s | cut -c1-80)) for $TRIPLE, glibc floor $GLIBC_FLOOR, -j$JOBS"
say "  source $SRC, target $CARGO_TARGET_DIR, log $STAGE/build.log"
t0=$(date +%s)
set +e
( cd "$SRC" && env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_RUSTFLAGS \
      -u "CARGO_TARGET_$(tr 'a-z-' 'A-Z_' <<<"$TRIPLE")_RUSTFLAGS" -u RUSTC_WRAPPER -u CARGO_BUILD_TARGET \
      CARGO_INCREMENTAL=0 nice -n 10 cargo zigbuild "${cargo_flags[@]}" ) > "$STAGE/build.log" 2>&1
rc=$?
set -e
BUILD_S=$(( $(date +%s) - t0 ))
grep -E '^error|Finished' "$STAGE/build.log" | tail -5 >&2 || true
[ "$rc" -eq 0 ] || die "cargo zigbuild failed (rc=$rc) — see $STAGE/build.log; nothing published to $OUT"
for b in "${BINS[@]}"; do
    [ -x "$CARGO_TARGET_DIR/$TRIPLE/release/$b" ] || die "$b was not built — see $STAGE/build.log"
    install -m 0755 "$CARGO_TARGET_DIR/$TRIPLE/release/$b" "$STAGE/$b"
done

# ---- 3. what the fleet's loader will see ----
MAXV=""
for b in "${BINS[@]}"; do
    f="$STAGE/$b"
    file -b "$f" | grep -q '^ELF 64-bit LSB .*x86-64' || die "$b is not an x86-64 ELF: $(file -b "$f")"
    interp=$("$READELF" -l "$f" | sed -n 's/.*Requesting program interpreter: \(.*\)\]/\1/p')
    [ "$interp" = /lib64/ld-linux-x86-64.so.2 ] || die "$b: program interpreter '$interp'"
    needed=$("$READELF" -d "$f" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | sort | tr '\n' ' ')
    for n in $needed; do
        case $n in libc.so.6|libm.so.6|libpthread.so.0|libdl.so.2|librt.so.1|ld-linux-x86-64.so.2) ;;
            *) die "$b needs $n — not part of glibc; the fleet may not have it (NEEDED: $needed)" ;; esac
    done
    vers=$("$READELF" -V "$f" | grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' | sed 's/GLIBC_//' | sort -u -t. -k1,1n -k2,2n -k3,3n)
    top=$(tail -1 <<<"$vers")
    [ -n "$top" ] || die "$b: no GLIBC_ symbol versions read — is it static? ($READELF -V)"
    [ "$(printf '%s\n%s\n' "$top" "$GLIBC_FLOOR" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)" = "$GLIBC_FLOOR" ] \
        || die "$b needs GLIBC_$top > the floor $GLIBC_FLOOR"
    printf '%-17s %s  interp %s  NEEDED %s max GLIBC_%s\n' "$b" "$(file -b "$f" | cut -d, -f1-2)" "$interp" "$needed" "$top" >> "$STAGE/ELF-CHECK"
    MAXV="$MAXV $top"
done
MAXV=$(tr ' ' '\n' <<<"$MAXV" | grep . | sort -u -t. -k1,1n -k2,2n | tail -1)
(cd "$STAGE" && for b in "${BINS[@]}"; do printf '%s  %s\n' "$(lsha "$b")" "$b"; done > SHA256SUMS)
echo "$COMMIT" > "$STAGE/REV"
{
    echo "builder=build-release-local.sh"
    echo "commit=$COMMIT"
    echo "rev12=$REV12"
    echo "built_utc=$(date -u +%FT%TZ)"
    echo "build_seconds=$BUILD_S"
    echo "host=$(uname -srm)"
    echo "target=$TRIPLE glibc_floor=$GLIBC_FLOOR max_glibc_needed=$MAXV"
    echo "toolchain_pin=$PIN"
    echo "rustc=$(cd "$SRC" && rustc -V)"
    echo "cargo=$(cd "$SRC" && cargo -V)"
    echo "zig=$(zig version)"
    echo "cargo_zigbuild=$(sed -n 's/.*"cargo-zigbuild \([^ ]*\) .*/\1/p' "$HOME/.cargo/.crates2.json" 2>/dev/null | head -1)"
    echo "command=cargo zigbuild ${cargo_flags[*]}"
    echo "cargo_target_dir=$CARGO_TARGET_DIR"
} > "$STAGE/BUILD-INFO"
cat "$STAGE/SHA256SUMS" >&2
cat "$STAGE/ELF-CHECK" >&2

# ---- 4. the identity, read from the release kaspad itself ----
# The probe, t12check.py and the kit constants it compares (fleet.env.example CLASS_8K / PREMINE_TXID,
# lib.sh CLASS_2M_PREFIX) are THIS checkout's kit — run this script from a checkout of the commit it builds.
PKIT="$KIT"
if ! git -C "$REPO" diff --quiet "$COMMIT" -- contrib/t12-deploy-kit 2>/dev/null; then
    say "NOTE: this checkout's contrib/t12-deploy-kit differs from $REV12's — the probe compares the kit constants of THIS checkout"
fi
probe_vals() { sed -n -E 's/^(EXPECT_FP|EXPECT_GENESIS|PREMINE_TXID)=(.*)$/\1=\2/p' "$1"; }
FP=""; GEN=""; PREMINE=""; PROBED=""
case "$PROBE" in
direct)
    say "identity probe: the release kaspad, directly"
    PROBE_OUT="$STAGE/probe" PROBE_WAIT_S=${PROBE_WAIT_S:-180} "$PKIT/probe-identity-local.sh" --kaspad "$STAGE/kaspad" \
        > "$STAGE/probe-release.txt" || die "the probe failed (rc=$?) — see $STAGE/probe-release.txt and $STAGE/probe/"
    PROBED="release kaspad, direct on $(uname -srm)" ;;
docker)
    img_arch=$(docker image inspect --platform linux/amd64 --format '{{.Architecture}}' "$PROBE_IMAGE" 2>/dev/null || true)
    [ "$img_arch" = amd64 ] || die "PROBE_IMAGE $PROBE_IMAGE has no linux/amd64 variant on this machine (nothing is pulled)"
    mkdir -p "$STAGE/probe"
    say "identity probe: the release kaspad in $PROBE_IMAGE (linux/amd64, --network none, --pull never; emulated — minutes)"
    docker run --rm --pull never --network none --platform linux/amd64 --user "$(id -u):$(id -g)" \
        -v "$PKIT:/kit:ro" -v "$STAGE:/rel" -e PROBE_OUT=/rel/probe -e PROBE_WAIT_S="${PROBE_WAIT_S:-3600}" -e TMPDIR=/tmp \
        --entrypoint bash "$PROBE_IMAGE" -c 'ldd --version | head -1 >&2; cd /tmp && /kit/probe-identity-local.sh --kaspad /rel/kaspad' \
        > "$STAGE/probe-release.txt" 2> "$STAGE/probe-release.err" \
        || { tail -20 "$STAGE/probe-release.err" >&2; die "the container probe failed — see $STAGE/probe-release.err and $STAGE/probe/"; }
    PROBED="release kaspad, $PROBE_IMAGE linux/amd64 ($(head -1 "$STAGE/probe-release.err" | sed 's/^ldd //'))" ;;
skip)
    say "identity probe SKIPPED (PROBE=skip, or a Mac without PROBE_IMAGE): IDENTITY is not written;"
    say "  fill EXPECT_FP / EXPECT_GENESIS / PREMINE_TXID from probe-identity-local.sh (Mac) — install-*.sh switch re-checks each node" ;;
esac
if [ "$PROBE" != skip ]; then
    eval "$(probe_vals "$STAGE/probe-release.txt" | sed 's/^EXPECT_FP=/FP=/; s/^EXPECT_GENESIS=/GEN=/; s/^PREMINE_TXID=/PREMINE=/')"
    [[ "$FP" =~ ^[0-9a-f]{64}$ ]] && [[ "$GEN" =~ ^[0-9a-f]{128}$ ]] || die "the probe printed no identity — see $STAGE/probe-release.txt"
fi
if [ "${NATIVE_PROBE:-0}" = 1 ]; then
    say "native probe: building $REV12's kaspad for this machine (dev profile) — a second build"
    ( cd "$SRC" && env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_RUSTFLAGS CARGO_INCREMENTAL=0 \
          nice -n 10 cargo build --locked -j "$JOBS" -p kaspad --bin kaspad ) > "$STAGE/build-native.log" 2>&1 \
        || die "the native build failed — see $STAGE/build-native.log"
    PROBE_OUT="$STAGE/probe" "$PKIT/probe-identity-local.sh" --kaspad "$CARGO_TARGET_DIR/debug/kaspad" > "$STAGE/probe-native.txt" \
        || die "the native probe failed (rc=$?) — see $STAGE/probe-native.txt"
    if [ "$PROBE" = skip ]; then
        eval "$(probe_vals "$STAGE/probe-native.txt" | sed 's/^EXPECT_FP=/FP=/; s/^EXPECT_GENESIS=/GEN=/; s/^PREMINE_TXID=/PREMINE=/')"
        PROBED="NATIVE dev build of $REV12 on $(uname -srm) — the release binary itself was not run"
    else
        [ "$(probe_vals "$STAGE/probe-native.txt")" = "$(probe_vals "$STAGE/probe-release.txt")" ] \
            || die "the release kaspad and the native build of the same commit announce different identities — see $STAGE/probe-*.txt"
        PROBED="$PROBED; equal to the native dev build's probe"
    fi
fi
if [ -n "$FP" ]; then
    printf 'EXPECT_FP=%s\nEXPECT_GENESIS=%s\nPREMINE_TXID=%s\n# probed: %s\n' "$FP" "$GEN" "${PREMINE:-__FILL_ME__}" "$PROBED" > "$STAGE/IDENTITY"
    # the operator's forbidden lists (fleet.env, else the template) — install-*.sh refuses these too
    ( if [ -f "$KIT/fleet.env" ]; then . "$KIT/fleet.env"; else . "$KIT/fleet.env.example"; fi
      for g in $FORBIDDEN_GENESIS ${DRILL_GENESES:-}; do [ "$GEN" = "$g" ] && { say "WARNING: genesis ${GEN:0:16}… is FORBIDDEN (fleet.env) — install-*.sh will refuse it"; exit 3; }; done; exit 0 ) \
        || true
fi

mv "$STAGE" "$OUT"
sha_of_bin() { grep " $1\$" "$OUT/SHA256SUMS" | cut -d' ' -f1; }
cat <<EOF

# ---- build-release-local.sh: $REV12 → $OUT (${BUILD_S}s build; max GLIBC_$MAXV ≤ $GLIBC_FLOOR) ----
# Paste into fleet.env on this Mac, then: ./distribute-from-mac.sh binaries  (ships $OUT to 5.104, .113, ibm)
REV=$REV12
KASPAD_SHA256=$(sha_of_bin kaspad)
MISAKA_SHA256=$(sha_of_bin misaka)
PALW_CLASS_SHA256=$(sha_of_bin palw-class)
SEEDER_SHA256=KEEP   # or $(sha_of_bin misaka-dnsseeder) to swap seeders (SEEDERS.md)
EXPECT_FP=${FP:-__FILL_ME__}
EXPECT_GENESIS=${GEN:-__FILL_ME__}
PREMINE_TXID=${PREMINE:-__FILL_ME__}
# identity probed: ${PROBED:-NOT PROBED}
EOF
