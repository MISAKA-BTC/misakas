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
#      `evm`: t12 activates the EVM lane at DAA 0; a build without it refuses to start). cargo runs
#      under `env -i` with only PATH, HOME, TMPDIR, CARGO_HOME / RUSTUP_HOME (when set), the target dir,
#      the remaps and CARGO_INCREMENTAL=0 — no RUSTFLAGS, CARGO_PROFILE_*, CC / CFLAGS / CXXFLAGS (read by
#      the cc crate for rocksdb, secp256k1, blst, c-kzg, ring, zstd), *_SYS or ROCKSDB_* switch and no
#      RUSTUP_TOOLCHAIN of the caller's shell reaches the build — and a cargo config file OUTSIDE the
#      commit ($CARGO_HOME/config[.toml], .cargo/config[.toml] in a directory above the checkout) aborts the
#      run (ALLOW_CARGO_CONFIG=1 builds anyway and records each one, with its sha256, in BUILD-INFO), so
#      the bytes depend on the commit and the recorded toolchain, not on the shell or the machine's config.
#      The only rustflags are path remaps: rustc embeds the absolute path of every source file it compiles
#      (panic locations, include!d OUT_DIR files), so without them the sha256 depends on where the
#      checkout, the target dir and ~/.cargo sit, and the binary carries the builder's home directory.
#      Mapped to /misaka, /target and /cargo, the same commit builds to the same bytes in any directory
#      (2026-09-25: two builds of 8270cf03 from different checkouts into different, empty target dirs
#      gave four byte-identical binaries — PLAN.md §13).
#   3. every binary: an x86-64 ELF, interpreter /lib64/ld-linux-x86-64.so.2, NEEDED only glibc's own
#      libraries (the C++ runtime and libgcc are linked in), and no GLIBC_x.y symbol version above
#      FLEET_GLIBC (2.39 — a constant of this script, not an env knob: the fleet's Ubuntu 24.04) nor above
#      the floor zig linked against → SHA256SUMS, REV, BUILD-INFO.
#   4. the identity: probe-identity-local.sh (this kit's) against THE RELEASE kaspad itself —
#      in a linux/amd64 container on a Mac (below), or directly on an x86_64 Linux build machine —
#      → IDENTITY (EXPECT_FP / EXPECT_GENESIS / PREMINE_TXID, and as comments the schedule id, the rule
#      manifest digest and the start-up court_e2e_root). NATIVE_PROBE=1 also builds the commit's kaspad
#      for this machine (dev profile) and probes it: EVERY line of the two probe outputs but their header
#      (binary, sha256, source, log, time) must be equal — the three values, the schedule id, the rule
#      manifest digest, court_e2e_root and each genesis CLASS / BOND line (functions of the params, the
#      genesis and PALW's integer execution, not of the binary or the CPU).
#   5. prints the fleet.env lines to paste.
#
# One run at a time: a run holds $WORK.lock (its checkout $WORK/src), <target dir>/.build-release-local.lock
# and $CACHE/.<rev12>.lock (mkdir; the holder's pid inside) for its whole length, and refuses to start while
# a live run holds any of them — two runs sharing the checkout would build a mix of two commits under one
# rev, sharing the target dir they would copy each other's binaries, and sharing <rev12> they would delete
# each other's staging. A dead run's lock (kill -9, reboot) is taken over. The checkout stays at the fixed
# path $WORK/src (the remap names it, which is what lets a re-run reuse the dependency builds).
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
#   GLIBC_FLOOR       default 2.39 (zig links against that version's symbol set); refused above
#                     FLEET_GLIBC=2.39, the fleet's glibc, which the ELF check holds every binary to
#   WORK              default $HOME/.cache/misaka-t12-rel (the clean worktree and the default target dir;
#                     the remaps name both, so keep WORK and CARGO_TARGET_DIR fixed to reuse dependency builds)
#   CACHE             default $KIT/.cache (where <rev12>/ lands — distribute-from-mac.sh reads $KIT/.cache)
#   PROBE             auto (default: direct on x86_64 Linux, docker on a Mac when PROBE_IMAGE is set,
#                     else skip) | docker | direct | skip
#   PROBE_IMAGE       the local image for the container probe (required for PROBE=docker)
#   PROBE_WAIT_S      how long the probe waits for the node's identity over RPC (default 3600 in the
#                     container — kaspad's start-up court self-test under qemu took 22 min on 2026-09-25
#                     with the Mac busy, ~1 min natively — and 180 direct)
#   NATIVE_PROBE=1    also probe a native dev build of the same commit and compare (a second build)
#   OFFLINE=1         cargo --offline (the release build and NATIVE_PROBE's dev build)
#   ALLOW_CARGO_CONFIG=1  build although a cargo config outside the commit applies (recorded in BUILD-INFO)
#   KEEP_SRC=1        keep $WORK/src after the run
#   FORCE=1           overwrite an existing $KIT/.cache/<rev12>/ (default: refuse — a published
#                     sha256 must not be silently replaced)
set -euo pipefail

KIT=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$KIT/../.." && pwd)
COMMIT_ARG=${1:-HEAD}
FLEET_GLIBC=2.39   # ibm, .113, 5.104: Ubuntu 24.04. Not an env knob — the ELF check's reference must not move with GLIBC_FLOOR
GLIBC_FLOOR=${GLIBC_FLOOR:-2.39}
JOBS=${JOBS:-4}
WORK=${WORK:-$HOME/.cache/misaka-t12-rel}
WORK=${WORK%/}
SRC=$WORK/src
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$WORK/target}
TRIPLE=x86_64-unknown-linux-gnu
BINS=(kaspad misaka palw-class misaka-dnsseeder)
CACHE=${CACHE:-$KIT/.cache}
say() { printf '[build-local %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
die() { say "ABORT: $*"; exit 1; }
lsha() { (command -v sha256sum >/dev/null && sha256sum "$1" || shasum -a 256 "$1") | cut -d' ' -f1; }
ver_le() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)" = "$2" ]; }   # $1 <= $2

[[ "$GLIBC_FLOOR" =~ ^[0-9]+\.[0-9]+$ ]] || die "GLIBC_FLOOR must look like 2.39"
ver_le "$GLIBC_FLOOR" "$FLEET_GLIBC" || die "GLIBC_FLOOR=$GLIBC_FLOOR is above the fleet's glibc $FLEET_GLIBC — the fleet could not load what zig links"

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
# the only environment cargo sees (header): the caller's PATH picks the tools, BUILD-INFO records which
CLEAN_ENV=(env -i PATH="$PATH" HOME="$HOME")
for v in CARGO_HOME RUSTUP_HOME TMPDIR; do [ -z "${!v:-}" ] || CLEAN_ENV+=("$v=${!v}"); done

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
# the probe compares the kit constants of THIS checkout (step 4) — say so now, not after a 30-minute build
if ! git -C "$REPO" diff --quiet "$COMMIT" -- contrib/t12-deploy-kit 2>/dev/null; then
    say "NOTE: this checkout's contrib/t12-deploy-kit differs from $REV12's — the probe will compare the kit constants of THIS checkout"
fi

# ---- one run at a time per checkout, target dir and <rev12> (header) ----
LOCKS=(); SRC_CREATED=0
on_exit() {
    local l
    if [ "$SRC_CREATED" = 1 ] && [ "${KEEP_SRC:-0}" != 1 ]; then git -C "$REPO" worktree remove --force "$SRC" >/dev/null 2>&1 || true; fi
    for l in ${LOCKS[@]+"${LOCKS[@]}"}; do [ "$(cat "$l/pid" 2>/dev/null)" = "$$" ] && rm -rf "$l"; done
    return 0
}
trap on_exit EXIT
trap 'exit 129' HUP; trap 'exit 130' INT; trap 'exit 143' TERM
take_lock() { # <lock dir> <what it guards>
    local l=$1 holder i
    for i in 1 2 3; do
        if mkdir "$l" 2>/dev/null; then echo "$$" > "$l/pid"; LOCKS+=("$l"); return 0; fi
        holder=""
        for _ in 1 2 3 4 5; do holder=$(cat "$l/pid" 2>/dev/null || true); [ -n "$holder" ] && break; sleep 1; done
        if [[ "$holder" =~ ^[0-9]+$ ]] && kill -0 "$holder" 2>/dev/null; then
            die "$2 is in use by another build-release-local.sh (pid $holder; lock $l) — wait for it, or give this run its own WORK / CARGO_TARGET_DIR"
        fi
        say "NOTE: taking over $l from a run that is gone (pid ${holder:-unknown})"
        mv "$l" "$l.stale.$$" 2>/dev/null && rm -rf "$l.stale.$$"
    done
    die "could not take $l"
}
mkdir -p "$WORK" "$CARGO_TARGET_DIR" "$CACHE"
TGT_P=$(cd "$CARGO_TARGET_DIR" && pwd -P)
take_lock "$WORK.lock" "WORK $WORK (the checkout $SRC)"
take_lock "$TGT_P/.build-release-local.lock" "the target dir $TGT_P"
take_lock "$CACHE/.$REV12.lock" "$CACHE/$REV12"

OUT="$CACHE/$REV12"
if [ -e "$OUT" ]; then
    [ "${FORCE:-0}" = 1 ] || die "$OUT exists (FORCE=1 to rebuild into it; its SHA256SUMS may already be in fleet.env)"
    rm -rf "$OUT"
fi
# a worktree at $SRC now is a dead run's (or a KEEP_SRC=1 run's): this run holds $WORK.lock
if git -C "$REPO" worktree list --porcelain | grep -qx "worktree $SRC"; then
    git -C "$REPO" worktree remove --force "$SRC"
fi
[ ! -e "$SRC" ] || die "$SRC exists but is not a worktree of $REPO — move it away"
git -C "$REPO" worktree add --detach "$SRC" "$COMMIT" >/dev/null 2>&1 || die "git worktree add $SRC $COMMIT failed"
SRC_CREATED=1
[ "$(git -C "$SRC" rev-parse HEAD)" = "$COMMIT" ] || die "$SRC is not at $COMMIT"
[ -z "$(git -C "$SRC" status --porcelain --untracked-files=all)" ] || die "$SRC is not clean"

# ---- 2. the build ----
SRC_P=$(cd "$SRC" && pwd -P)
CH_P=$(cd "${CARGO_HOME:-$HOME/.cargo}" && pwd -P)
# cargo config files the commit does not carry (the checkout's own .cargo/config.toml is part of the commit)
CFG_OUT=""
cfgs=("$CH_P/config" "$CH_P/config.toml")
d=$(dirname "$SRC_P")
while :; do cfgs+=("${d%/}/.cargo/config" "${d%/}/.cargo/config.toml"); [ "$d" = / ] && break; d=$(dirname "$d"); done
while IFS= read -r c; do
    [ -z "$c" ] || CFG_OUT+="$c (sha256 $(lsha "$c" | cut -c1-16)); "
done < <(for c in "${cfgs[@]}"; do [ ! -f "$c" ] || (cd "$(dirname "$c")" && echo "$(pwd -P)/$(basename "$c")"); done | sort -u)
if [ -n "$CFG_OUT" ]; then
    [ "${ALLOW_CARGO_CONFIG:-0}" = 1 ] || die "cargo would read config outside the commit: $CFG_OUT— its [build]/[profile]/[env]/[target] entries would change the bytes (move it away, or ALLOW_CARGO_CONFIG=1 to record it in BUILD-INFO)"
    say "NOTE: building with cargo config outside the commit (ALLOW_CARGO_CONFIG=1): $CFG_OUT"
fi
STAGE="$CACHE/.$REV12.partial"
rm -rf "$STAGE"; mkdir -p "$STAGE"
cargo_flags=(--release --locked -j "$JOBS" --target "$TRIPLE.$GLIBC_FLOOR")
# the path remaps (see the header), most specific last — rustc applies the LAST matching one
ENC_RUSTFLAGS=$(printf '%s\n' "${#CH_P} $CH_P=/cargo" "${#TGT_P} $TGT_P=/target" "${#SRC_P} $SRC_P=/misaka" \
    | sort -n | cut -d' ' -f2- | sed 's/^/--remap-path-prefix=/' | paste -sd $'\x1f' -)
[ "${OFFLINE:-0}" = 1 ] && cargo_flags+=(--offline)
for b in "${BINS[@]}"; do cargo_flags+=(--bin "$b"); done
say "building $REV12 ($(git -C "$SRC" log -1 --format=%s | cut -c1-80)) for $TRIPLE, glibc floor $GLIBC_FLOOR, -j$JOBS"
say "  source $SRC, target $CARGO_TARGET_DIR, log $STAGE/build.log"
t0=$(date +%s)
set +e
( cd "$SRC_P" && "${CLEAN_ENV[@]}" CARGO_ENCODED_RUSTFLAGS="$ENC_RUSTFLAGS" CARGO_TARGET_DIR="$TGT_P" \
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
    ver_le "$top" "$FLEET_GLIBC" || die "$b needs GLIBC_$top > the fleet's glibc $FLEET_GLIBC"
    ver_le "$top" "$GLIBC_FLOOR" || die "$b needs GLIBC_$top > the floor $GLIBC_FLOOR zig linked against"
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
    echo "target=$TRIPLE glibc_floor=$GLIBC_FLOOR fleet_glibc=$FLEET_GLIBC max_glibc_needed=$MAXV"
    echo "toolchain_pin=$PIN"
    echo "rustc=$(cd "$SRC" && "${CLEAN_ENV[@]}" rustc -V)"
    echo "cargo=$(cd "$SRC" && "${CLEAN_ENV[@]}" cargo -V)"
    echo "cargo_bin=$(command -v cargo) zig_bin=$(command -v zig)"
    echo "zig=$(zig version)"
    echo "cargo_zigbuild=$(sed -n 's/.*"cargo-zigbuild \([^ ]*\) .*/\1/p' "$CH_P/.crates2.json" 2>/dev/null | head -1)"
    echo "command=cargo zigbuild ${cargo_flags[*]}"
    echo "env=env -i PATH HOME$(for v in CARGO_HOME RUSTUP_HOME TMPDIR; do [ -z "${!v:-}" ] || printf ' %s' "$v"; done) CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR CARGO_INCREMENTAL=0"
    echo "cargo_config_outside_commit=${CFG_OUT:-none}"
    echo "rustflags=$(tr $'\x1f' ' ' <<<"$ENC_RUSTFLAGS")"
    echo "cargo_target_dir=$CARGO_TARGET_DIR"
} > "$STAGE/BUILD-INFO"
cat "$STAGE/SHA256SUMS" >&2
cat "$STAGE/ELF-CHECK" >&2

# ---- 4. the identity, read from the release kaspad itself ----
# The probe, t12check.py and the kit constants it compares with the binary's genesis (fleet.env[.example]
# CLASS_8K, lib.sh CLASS_2M_PREFIX; the bonds at the binary's own premine txid :0..7) are THIS checkout's
# kit — run this script from a checkout of the commit it builds (the NOTE before the build says when not).
PKIT="$KIT"
probe_vals() { sed -n -E 's/^(EXPECT_FP|EXPECT_GENESIS|PREMINE_TXID)=(.*)$/\1=\2/p' "$1"; }
# everything a probe prints but its header (time, binary path, sha256, source tree, log path)
probe_body() { grep -vE '^# (---- probe-identity-local|binary |sha256 |source |log )' "$1"; }
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
    PROBED="release kaspad, $PROBE_IMAGE linux/amd64, $(head -1 "$STAGE/probe-release.err" | sed 's/^ldd //')" ;;
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
    native_flags=(--locked -j "$JOBS" -p kaspad --bin kaspad)
    [ "${OFFLINE:-0}" = 1 ] && native_flags+=(--offline)
    ( cd "$SRC_P" && "${CLEAN_ENV[@]}" CARGO_TARGET_DIR="$TGT_P" CARGO_INCREMENTAL=0 \
          nice -n 10 cargo build "${native_flags[@]}" ) > "$STAGE/build-native.log" 2>&1 \
        || die "the native build failed — see $STAGE/build-native.log"
    PROBE_OUT="$STAGE/probe" "$PKIT/probe-identity-local.sh" --kaspad "$TGT_P/debug/kaspad" > "$STAGE/probe-native.txt" \
        || die "the native probe failed (rc=$?) — see $STAGE/probe-native.txt"
    if [ "$PROBE" = skip ]; then
        eval "$(probe_vals "$STAGE/probe-native.txt" | sed 's/^EXPECT_FP=/FP=/; s/^EXPECT_GENESIS=/GEN=/; s/^PREMINE_TXID=/PREMINE=/')"
        [[ "$FP" =~ ^[0-9a-f]{64}$ ]] && [[ "$GEN" =~ ^[0-9a-f]{128}$ ]] || die "the native probe printed no identity — see $STAGE/probe-native.txt"
        PROBED="NATIVE dev build of $REV12 on $(uname -srm) — the release binary itself was not run"
    else
        # every line but the header: the identity, schedule id, rule manifest, court_e2e_root, CLASS and BOND lines
        diff <(probe_body "$STAGE/probe-native.txt") <(probe_body "$STAGE/probe-release.txt") > "$STAGE/probe-diff.txt" \
            || { cat "$STAGE/probe-diff.txt" >&2; die "the release kaspad and the native build of the same commit announce different identities — see $STAGE/probe-diff.txt"; }
        PROBED="$PROBED; all $(probe_body "$STAGE/probe-release.txt" | grep -c .) lines but the header equal to the native dev build's probe"
    fi
fi
if [ -n "$FP" ]; then
    printf 'EXPECT_FP=%s\nEXPECT_GENESIS=%s\nPREMINE_TXID=%s\n# probed: %s\n' "$FP" "$GEN" "${PREMINE:-__FILL_ME__}" "$PROBED" > "$STAGE/IDENTITY"
    # informational, for comparing builds (distribute-from-mac.sh and install-*.sh stage read only the three values)
    grep -hE '^# (EXPECT_SCHEDULE_ID|RULE_MANIFEST_DIGEST|COURT_E2E_ROOT)=' "$STAGE"/probe-release.txt "$STAGE"/probe-native.txt 2>/dev/null \
        | sort -u >> "$STAGE/IDENTITY" || true
    # the operator's forbidden lists (fleet.env, else the template) — install-*.sh refuses these too
    ( if [ -f "$KIT/fleet.env" ]; then . "$KIT/fleet.env"; else . "$KIT/fleet.env.example"; fi
      for g in $FORBIDDEN_GENESIS ${DRILL_GENESES:-}; do [ "$GEN" = "$g" ] && { say "WARNING: genesis ${GEN:0:16}… is FORBIDDEN (fleet.env) — install-*.sh will refuse it"; exit 3; }; done; exit 0 ) \
        || true
fi

mv "$STAGE" "$OUT"
sha_of_bin() { grep " $1\$" "$OUT/SHA256SUMS" | cut -d' ' -f1; }
cat <<EOF

# ---- build-release-local.sh: $REV12 → $OUT (${BUILD_S}s build; max GLIBC_$MAXV ≤ $GLIBC_FLOOR) ----
# Paste into fleet.env on this Mac, then: ./distribute-from-mac.sh binaries  (ships $OUT to 5.104, .113, ibm;
# it refuses unless fleet.env's EXPECT_FP / EXPECT_GENESIS / PREMINE_TXID equal $OUT/IDENTITY)
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
