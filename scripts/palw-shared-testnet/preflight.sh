#!/usr/bin/env bash
# =============================================================================
# preflight.sh — Phase-0 CLOSED two-node PALW testnet: environment + toolchain
#                gate. Runs BEFORE any node or miner is started.
#
# Audit-finding coverage (see PHASE0-status.md §2):
#   * STN-001  no hardcoded paths — everything is env-driven and realpath'd by
#              load_env; binaries identified only through $KASPAD/$VAL/$MINER.
#   * STN-002  data dirs created (load_env + an explicit idempotent re-create).
#   * STN-003  preflight part — host/port layout validated, two-host aware; a
#              single machine still cannot prove real partition/NAT (documented,
#              not pretended).
#   * STN-004  partial — asserts a clean, self-consistent STARTING state and
#              fails closed on any ambiguity (stray node on a foreign network,
#              divergent node_network between A and B).
#   * STN-10   release mode — the recorded attestation is EVIDENCE, never this
#              script's output: with PALW_RELEASE_MODE=1 a hash difference is
#              FATAL instead of a rewrite (see §0 and §3b).
#
# WHAT IT DOES (read-only except for the derived hash record):
#   1. load_env, then validate REPO_ROOT / PALW_DATA_ROOT / NETWORK and the six
#      ports (numeric, in range, disjoint on a single host).
#   2. Assert the three release binaries exist and are executable.
#   3. sha256 each -> artifacts/binary-hashes.txt (idempotent; if the recorded
#      hashes differ from the current binaries it says so LOUDLY, never silently
#      overwrites — and in RELEASE MODE it dies instead of rewriting).
#   4. If PEER_BINARY_HASHES is set, compare per-binary and DIE on any mismatch
#      (every node in a closed net must run byte-identical binaries).
#   5. If either node is ALREADY up, assert both report the same node_network
#      and warn LOUDLY that --palw-enable-algo4 must be identical on every node
#      (it is a start-time override and CANNOT be introspected over RPC).
#
# RELEASE MODE (PALW_RELEASE_MODE=1, STN-10) — fail-closed, zero evidence
# mutation. On top of everything above:
#   (a) binary-hashes.txt is COMPARED and never rewritten; a difference dies.
#   (b) no artifacts file is written at all (not even a first-time record).
#   (c) the REPO_ROOT git worktree must be clean (`git status --porcelain` empty).
#   (d) the signed network manifest must exist AND verify — it forces the §11.4
#       gate below (`./network-manifest.sh verify`), which also compares both
#       LIVE nodes' identity, so a release preflight runs against the running net.
#   (e) the release-bundle provenance record artifacts/SOURCE_COMMIT must be
#       present and carry source_commit / source_tag / cargo_lock_sha256 /
#       rust_toolchain; the commit, tag and Cargo.lock digest are re-checked
#       against this checkout.
# Unset / 0 keeps today's dev-loop behaviour unchanged (it still rewrites, but
# only after saying so LOUDLY).
#
# WHAT IT DOES NOT DO: it starts no process, mines nothing, writes no keys, and
# never touches the seeded test-only palw_demo path. The only file it may write
# is the derived artifacts/binary-hashes.txt record — and in release mode it
# writes nothing whatsoever.
#
# Idempotent + fail-closed + portable (bash 3.2 / BSD + GNU coreutils). It
# SOURCES common.sh and calls its helpers — it reimplements none of them.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="preflight"
export PALW_LOG_TAG

# -----------------------------------------------------------------------------
# Cleanup trap, armed up front (LIFO). preflight starts no long-lived process;
# the only teardown is removing a half-written temp hash file on an early
# die/INT/TERM. _TMP_HASH is expanded at trap time (see _run_cleanup's eval).
# -----------------------------------------------------------------------------
_TMP_HASH=""
register_cleanup 'if [ -n "$_TMP_HASH" ]; then rm -f "$_TMP_HASH"; fi'

# load_env: sources config, realpaths REPO_ROOT/PALW_DATA_ROOT, creates the 0700
# data dirs, overlays state.env, validates required vars, binds+verifies the
# three binaries. Fail-closed and re-runnable.
load_env

# External tools this script uses directly (helpers rely on more, all present).
require_cmd awk mktemp install

# =============================================================================
# 0. Release mode (STN-10). PALW_RELEASE_MODE=1 turns this stage from a dev
#    convenience into an EVIDENCE gate: artifacts are verified, never produced,
#    and every ambiguity is fatal. Read AFTER load_env so env.local can pin it on
#    a release host. The knob is spelled like negative-tests.sh's NEG_RELEASE
#    gate: set to 1, or leave it unset for the normal dev loop.
# =============================================================================
case "${PALW_RELEASE_MODE:-0}" in
    1)    RELEASE_MODE=1 ;;
    0|"") RELEASE_MODE=0 ;;
    *)    die "PALW_RELEASE_MODE must be 1 (release gate) or 0/unset (dev loop); got '${PALW_RELEASE_MODE:-}' — a fail-closed gate does not guess" ;;
esac
if [ "$RELEASE_MODE" -eq 1 ]; then
    log "RELEASE MODE (PALW_RELEASE_MODE=1): attestation + provenance are VERIFIED, never written; every difference is fatal"
    require_cmd git
    # Release mode implies the §11.4 signed-manifest gate even on one box: a
    # release identity is required whether or not a remote node is configured.
    PALW_REQUIRE_MANIFEST=1
fi

# =============================================================================
# Local helpers (thin; never duplicate common.sh — these only add checks that
# common.sh does not provide: port validation, portable sha256, and a STRICT
# reader for the release provenance record).
# =============================================================================

# _valid_port <n> — 0 iff <n> is an integer TCP port in 1..65535.
_valid_port() {
    case "${1:-}" in ''|*[!0-9]*) return 1 ;; esac
    [ "$1" -ge 1 ] && [ "$1" -le 65535 ]
}

# _check_ports_distinct LABEL=PORT ... — die on the first colliding pair.
_check_ports_distinct() {
    local a b la lb pa pb
    for a in "$@"; do
        la="${a%%=*}"; pa="${a##*=}"
        for b in "$@"; do
            lb="${b%%=*}"; pb="${b##*=}"
            [ "$la" = "$lb" ] && continue
            if [ "$pa" = "$pb" ]; then
                die "port collision: $la and $lb both use $pa (remap disjoint ports in env.local)"
            fi
        done
    done
}

# _sha256 <file> — echo the lowercase 64-hex sha256 digest, via whichever tool
#   is present (sha256sum | shasum -a 256 | openssl). Fail-closed if none.
_sha256() {
    local f="${1:?file}"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$f" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$f" | awk '{print $1}'
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$f" | awk '{print $NF}'
    else
        die "no sha256 tool found (need one of: sha256sum, shasum, openssl)"
    fi
}

# _hash_line <file> — echo "<64hex>  <basename>" for the manifest. Validates the
#   digest shape (fail-closed) so a truncated/garbled hash can never be recorded.
_hash_line() {
    local f="${1:?file}" h
    h="$(_sha256 "$f")"
    h="$(printf '%s' "$h" | tr 'A-F' 'a-f')"
    case "$h" in *[!0-9a-f]*) die "sha256 of $f is not hex: '$h'" ;; esac
    [ "${#h}" -eq 64 ] || die "sha256 of $f has wrong length (${#h} != 64): '$h'"
    printf '%s  %s\n' "$h" "$(basename "$f")"
}

# _prov_field <file> <key> — echo the value of a `<key>: <value>` line from the
#   release provenance record; empty when the key is absent. Deliberately STRICT
#   where common.sh's _kv/_line are tolerant: the key must start the line, `#`
#   comment lines are skipped (so a commented example can never satisfy a release
#   check), and the whole rest of the line is the value (a rustc version string
#   contains spaces).
_prov_field() {
    local f="${1:?file}" k="${2:?key}"
    awk -v k="$k" '
        /^[[:space:]]*#/ { next }
        {
            p = index($0, ":")
            if (p == 0) next
            key = substr($0, 1, p - 1)
            val = substr($0, p + 1)
            gsub(/^[ \t]+|[ \t]+$/, "", key)
            gsub(/^[ \t]+|[ \t]+$/, "", val)
            if (key == k && val != "") { print val; exit }
        }
    ' "$f"
}

# _write_hash_file — atomically (temp+mv) persist $FRESH_HASHES to $HASH_FILE.
#   STN-10: refuses to run at all under PALW_RELEASE_MODE=1. A release preflight
#   VERIFIES the recorded attestation; it must never become it. Guarded here as
#   well as at the call sites so no future path can write behind release mode.
_write_hash_file() {
    if [ "${RELEASE_MODE:-0}" -eq 1 ]; then
        die "refusing to write $HASH_FILE under PALW_RELEASE_MODE=1 — the binary attestation is evidence, not this script's output (STN-10)"
    fi
    _TMP_HASH="$(mktemp "${HASH_FILE}.XXXXXX")" || die "mktemp failed near $HASH_FILE"
    printf '%s\n' "$FRESH_HASHES" > "$_TMP_HASH"
    chmod 0644 "$_TMP_HASH" 2>/dev/null || true
    mv "$_TMP_HASH" "$HASH_FILE"
    _TMP_HASH=""
    log "recorded binary hashes -> $HASH_FILE"
    while IFS= read -r _hl || [ -n "$_hl" ]; do
        [ -n "$_hl" ] && log "  $_hl"
    done <<PALW_HASH_MANIFEST
$FRESH_HASHES
PALW_HASH_MANIFEST
}

# =============================================================================
# 1. Validate environment (REPO_ROOT, PALW_DATA_ROOT, NETWORK, ports).
#    load_env already fail-closed on empty required vars and realpath'd the two
#    roots; here we re-affirm the load-bearing ones and add the port checks that
#    load_env does not perform.
# =============================================================================
for _v in REPO_ROOT PALW_DATA_ROOT NETWORK; do
    [ -n "${!_v:-}" ] || die "$_v is empty after load_env (define it in env.local / PALW_ENV_FILE)"
done
[ -d "$REPO_ROOT" ] || die "REPO_ROOT is not a directory: $REPO_ROOT"

for _pv in A_P2P_PORT A_GRPC_PORT A_WRPC_PORT B_P2P_PORT B_GRPC_PORT B_WRPC_PORT; do
    _valid_port "${!_pv}" || die "$_pv='${!_pv}' is not a valid TCP port (1-65535); fix env.local"
done

# Within a node the P2P / gRPC / wRPC ports must differ. On a SINGLE host
# (NODE_A_HOST == NODE_B_HOST — the devnet default) all six must be disjoint or
# the two kaspad processes collide. On TWO hosts each host owns its own port
# space, so only the per-node trio must be disjoint.
if [ "$NODE_A_HOST" = "$NODE_B_HOST" ]; then
    log "single-host layout (NODE_A_HOST == NODE_B_HOST == $NODE_A_HOST): all six ports must be disjoint"
    _check_ports_distinct \
        A_P2P="$A_P2P_PORT"  A_GRPC="$A_GRPC_PORT"  A_WRPC="$A_WRPC_PORT" \
        B_P2P="$B_P2P_PORT"  B_GRPC="$B_GRPC_PORT"  B_WRPC="$B_WRPC_PORT"
else
    log "two-host layout (A=$NODE_A_HOST B=$NODE_B_HOST): validating each node's port trio"
    _check_ports_distinct A_P2P="$A_P2P_PORT" A_GRPC="$A_GRPC_PORT" A_WRPC="$A_WRPC_PORT"
    _check_ports_distinct B_P2P="$B_P2P_PORT" B_GRPC="$B_GRPC_PORT" B_WRPC="$B_WRPC_PORT"
fi
log "ports OK (A p2p/grpc/wrpc=$A_P2P_PORT/$A_GRPC_PORT/$A_WRPC_PORT  B=$B_P2P_PORT/$B_GRPC_PORT/$B_WRPC_PORT)"

# Data dirs (STN-002). load_env already created these; install -d is idempotent
# and re-asserting here makes preflight own the invariant explicitly.
for _d in node-a node-b logs keys artifacts; do
    install -d -m 0700 "$PALW_DATA_ROOT/$_d" || die "cannot create $PALW_DATA_ROOT/$_d"
done
log "data dirs ready under $PALW_DATA_ROOT (node-a node-b logs keys artifacts, 0700)"

# =============================================================================
# 2. Assert the three release binaries exist and are executable.
#    load_env already bound + verified KASPAD/VAL/MINER; re-assert explicitly so
#    preflight owns STN-001 with an actionable message and a stable hash order.
# =============================================================================
_bins=("$KASPAD" "$VAL" "$MINER")
for _b in "${_bins[@]}"; do
    [ -e "$_b" ] || die "release binary missing: $_b (build it: ./build-and-hash.sh  or  cargo build --release)"
    [ -f "$_b" ] || die "release binary is not a regular file: $_b"
    [ -x "$_b" ] || die "release binary not executable: $_b (rebuild with cargo build --release)"
    [ -r "$_b" ] || die "release binary not readable: $_b"
done
log "release binaries present + executable: kaspad, kaspa-pq-validator, misaminer"

# TICKET_MODE=mock also needs the controller-only mock-ticket helper (a workspace
# member built by build-and-hash.sh). Local existence/exec only — it is NOT a node
# binary and is NEVER part of the cross-host binary attestation below.
if [ "${TICKET_MODE:-skip}" = mock ]; then
    _mock_bin="${MOCK_TICKET_BIN:-$REPO_ROOT/target/release/mock-ticket}"
    [ -x "$_mock_bin" ] || die "TICKET_MODE=mock requires the mock-ticket helper at $_mock_bin — build it with ./build-and-hash.sh (it now builds -p mock-ticket), or use TICKET_MODE=skip."
    log "mock-ticket helper present + executable: $_mock_bin (controller-only; not cross-host compared)"
elif [ "${TICKET_MODE:-skip}" = real ]; then
    _real_bin="${REAL_PROVIDER_BIN:-$REPO_ROOT/target/release/palw-real-provider}"
    [ -x "$_real_bin" ] || die "TICKET_MODE=real requires $_real_bin — build it with cargo build --release -p palw-real-provider."
    for _real_input in REAL_RECEIPT_A REAL_RECEIPT_B REAL_RESULT_A REAL_RESULT_B; do
        [ -s "${!_real_input:-}" ] || die "TICKET_MODE=real requires a nonempty $_real_input file."
    done
    log "real-provider helper and Qwen k=2 evidence inputs present."
fi

# =============================================================================
# 3. Hash the three binaries -> artifacts/binary-hashes.txt (idempotent).
#    A mismatch against an existing record means the binaries changed since the
#    last preflight (rebuilt/replaced) — reported LOUDLY, never silently.
#    STN-10: outside release mode the record is then rewritten to match the disk
#    (dev convenience, and it says so). Under PALW_RELEASE_MODE=1 the record is
#    the release's evidence: a difference is FATAL and nothing is written.
# =============================================================================
HASH_FILE="$PALW_DATA_ROOT/artifacts/binary-hashes.txt"
install -d -m 0700 "$(dirname "$HASH_FILE")" || die "cannot create artifacts dir for $HASH_FILE"

# Stable order (kaspad, kaspa-pq-validator, misaminer). A die inside this
# command substitution propagates out and aborts the script (fail-closed).
FRESH_HASHES="$(
    for _b in "${_bins[@]}"; do
        _hash_line "$_b"
    done
)"

if [ -f "$HASH_FILE" ]; then
    # Compare only the attestation lines: build-and-hash.sh writes the same file
    # with leading `# ...` metadata/comment lines, so a raw `cat` would never match
    # FRESH_HASHES (bare "<hash>  <name>" lines) and would raise a false
    # "binaries changed" alarm + a non-idempotent rewrite. Strip comments/blank lines.
    EXISTING_HASHES="$(grep -Ev '^[[:space:]]*(#|$)' "$HASH_FILE")"
    if [ "$EXISTING_HASHES" = "$FRESH_HASHES" ]; then
        log "binary-hashes.txt already matches the current binaries (idempotent, unchanged): $HASH_FILE"
    elif [ "$RELEASE_MODE" -eq 1 ]; then
        # STN-10: rewriting here would silently turn the attestation into
        # "whatever is on disk". In release mode the recorded set IS the claim.
        die "recorded binary hashes DIFFER from the current binaries, and PALW_RELEASE_MODE=1 forbids rewriting the attestation (STN-10).
$HASH_FILE is this release's evidence — preflight verifies it, it never becomes it.
Either restore the binaries this release attests to (check out the release commit and
re-run ./build-and-hash.sh), or, if the CURRENT binaries are the release, re-record them
deliberately OUTSIDE release mode (HASH_FORCE=1 ./build-and-hash.sh) and re-sign the
network manifest that pins them (./network-manifest.sh generate).
recorded (sha256  name):
$EXISTING_HASHES
current (sha256  name):
$FRESH_HASHES"
    else
        warn "recorded binary hashes DIFFER from the current binaries."
        warn "the release binaries changed since the last preflight (rebuilt or replaced)."
        warn "updating $HASH_FILE to reflect the CURRENT binaries (reported, not silent)."
        warn "this REWRITES an attestation file — dev convenience ONLY. Under PALW_RELEASE_MODE=1 it is FATAL instead (STN-10)."
        warn "recorded (sha256  name):
$EXISTING_HASHES
current (sha256  name):
$FRESH_HASHES"
        _write_hash_file
    fi
else
    if [ "$RELEASE_MODE" -eq 1 ]; then
        die "$HASH_FILE is absent and PALW_RELEASE_MODE=1 forbids creating it here (STN-10) — the binary attestation must come from the BUILD step, not from preflight. Run ./build-and-hash.sh on the build host (outside release mode) and ship artifacts/binary-hashes.txt with the release bundle."
    fi
    _write_hash_file
fi

# =============================================================================
# 3b. RELEASE MODE source provenance (STN-10) — runs only under
#     PALW_RELEASE_MODE=1. A release has to name the source state it was cut
#     from, so a dirty/unknown worktree or a missing provenance record is FATAL
#     here, never a warning.
#
#     SCOPE: these assert that the source facts are recorded and that this
#     checkout still matches them (HEAD, tag, Cargo.lock digest). They do NOT
#     prove the binaries hashed in §3 were built from that source — nothing in
#     this harness attests a build. That claim still needs an independent rebuild
#     plus the STN-001 peer hash compare in §4. No tag SIGNATURE is checked
#     either; only that the tag resolves and points at HEAD.
# =============================================================================
if [ "$RELEASE_MODE" -eq 1 ]; then
    # (c) clean worktree. Untracked files count: a release cut from a tree with
    #     unversioned sources cannot be reproduced by anyone else.
    git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1 \
        || die "release mode: $REPO_ROOT is not a git worktree — a release must name the commit it was built from; build from a real checkout (or drop PALW_RELEASE_MODE for a dev run)"
    _worktree_dirty="$(git -C "$REPO_ROOT" status --porcelain)" \
        || die "release mode: 'git -C $REPO_ROOT status --porcelain' failed — cannot establish that the source tree is clean"
    if [ -n "$_worktree_dirty" ]; then
        die "release mode: the git worktree at $REPO_ROOT is DIRTY — commit, stash or remove every change (untracked files count) and REBUILD before cutting a release:
$_worktree_dirty"
    fi
    _head_commit="$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || true)"
    _head_commit="$(printf '%s' "$_head_commit" | tr 'A-F' 'a-f')"
    [ -n "$_head_commit" ] || die "release mode: cannot resolve HEAD in $REPO_ROOT (unborn branch or broken checkout)"
    log "release mode: git worktree clean at $REPO_ROOT (HEAD=$_head_commit)"

    # (e) the release-bundle provenance record. release-bundle.sh (the release
    #     packaging step — NOT one of this harness's stage scripts) writes it on
    #     the BUILD host and ships it with the bundle. preflight requires it and
    #     the four facts a third party needs to rebuild; it will not synthesise
    #     one here, because inventing the evidence it was asked to check is
    #     exactly the STN-10 failure mode.
    SOURCE_COMMIT_FILE="$PALW_DATA_ROOT/artifacts/SOURCE_COMMIT"
    if [ ! -s "$SOURCE_COMMIT_FILE" ]; then
        die "release mode: source-provenance record missing or empty: $SOURCE_COMMIT_FILE
It is written by the release packaging step (release-bundle.sh) on the build host. It is a
plain 'key: value' text file ('#' lines are comments) and MUST carry all four facts:
  source_commit:     <full git commit the release was built from>
  source_tag:        <git tag naming this release>
  cargo_lock_sha256: <sha256 of $REPO_ROOT/Cargo.lock at that commit>
  rust_toolchain:    <exact rustc version string used for the build, e.g. 'rustc X.Y.Z (hash date)'>
Produce it on the build host and copy it into $PALW_DATA_ROOT/artifacts/ — preflight will
not write it for you (that is the whole point of release mode)."
    fi

    _prov_commit="$(_prov_field "$SOURCE_COMMIT_FILE" source_commit)"
    _prov_tag="$(_prov_field "$SOURCE_COMMIT_FILE" source_tag)"
    _prov_lock="$(_prov_field "$SOURCE_COMMIT_FILE" cargo_lock_sha256)"
    _prov_toolchain="$(_prov_field "$SOURCE_COMMIT_FILE" rust_toolchain)"
    # bash 3.2 safe: LABEL=VALUE words, value quoted so spaces never re-split.
    for _pf in source_commit="$_prov_commit" source_tag="$_prov_tag" \
               cargo_lock_sha256="$_prov_lock" rust_toolchain="$_prov_toolchain"; do
        [ -n "${_pf#*=}" ] || die "release mode: $SOURCE_COMMIT_FILE has no '${_pf%%=*}:' line — the release-bundle record is incomplete; regenerate it on the build host (all four of source_commit / source_tag / cargo_lock_sha256 / rust_toolchain are required)"
    done

    # source_commit must be THIS checkout: the worktree is clean, so HEAD is the
    # exact source state, and a record naming another commit is not this release.
    _prov_commit="$(printf '%s' "$_prov_commit" | tr 'A-F' 'a-f')"
    case "$_prov_commit" in *[!0-9a-f]*) die "release mode: source_commit in $SOURCE_COMMIT_FILE is not a hex commit id: '$_prov_commit'" ;; esac
    [ "$_prov_commit" = "$_head_commit" ] \
        || die "release mode: source_commit MISMATCH — $SOURCE_COMMIT_FILE records '$_prov_commit' but $REPO_ROOT is at HEAD '$_head_commit' (record the FULL commit id, abbreviations do not compare). Check out the recorded commit and rebuild (./build-and-hash.sh), or cut the bundle again from this commit."

    # source_tag: must resolve in this checkout and point at HEAD. That is
    # EXISTENCE + placement only — it does NOT verify a tag SIGNATURE (git tag -v
    # needs a trusted keyring this harness does not ship, and signing is the
    # operator's step, never this script's).
    _tag_commit="$(git -C "$REPO_ROOT" rev-parse -q --verify "refs/tags/$_prov_tag^{commit}" 2>/dev/null || true)"
    _tag_commit="$(printf '%s' "$_tag_commit" | tr 'A-F' 'a-f')"
    [ -n "$_tag_commit" ] \
        || die "release mode: source_tag '$_prov_tag' does not resolve in $REPO_ROOT — fetch the release tags (git fetch --tags) so it can be checked, or record the tag this checkout actually carries"
    [ "$_tag_commit" = "$_head_commit" ] \
        || die "release mode: source_tag '$_prov_tag' points at $_tag_commit but HEAD is $_head_commit — check out the tag and rebuild before cutting the release"

    # cargo_lock_sha256: the exact dependency graph is half of "rebuildable".
    _lock_file="$REPO_ROOT/Cargo.lock"
    [ -r "$_lock_file" ] || die "release mode: $_lock_file is missing or unreadable — the recorded cargo_lock_sha256 cannot be checked"
    _lock_now="$(_sha256 "$_lock_file" | tr 'A-F' 'a-f')"
    case "$_lock_now" in ''|*[!0-9a-f]*) die "release mode: could not compute a hex sha256 for $_lock_file (got '$_lock_now')" ;; esac
    _prov_lock="$(printf '%s' "$_prov_lock" | tr 'A-F' 'a-f')"
    _prov_lock="${_prov_lock#sha256:}"
    [ "$_lock_now" = "$_prov_lock" ] \
        || die "release mode: Cargo.lock MISMATCH — $SOURCE_COMMIT_FILE records cargo_lock_sha256=$_prov_lock but $_lock_file hashes to $_lock_now; this is not the dependency graph the release was built from"
    log "release mode: source provenance OK (commit=$_prov_commit tag=$_prov_tag cargo_lock_sha256=$_lock_now)"

    # rust_toolchain is RECORDED ONLY. preflight can compare it to THIS host's
    # rustc, but nothing here can prove which toolchain produced the binaries
    # hashed in §3 (no build attestation exists), so a difference is a loud WARN,
    # not a PASS and not a die — release binaries are normally built elsewhere.
    if command -v rustc >/dev/null 2>&1; then
        _rustc_here="$(rustc -V 2>/dev/null || true)"
        if [ -n "$_rustc_here" ] && [ "$_rustc_here" != "$_prov_toolchain" ]; then
            warn "recorded rust_toolchain '$_prov_toolchain' != this host's rustc '$_rustc_here' — expected when the release was built on another host, but preflight CANNOT verify which toolchain produced the binaries. To turn this into a real check, rebuild on the recorded toolchain and compare hashes (§4 / PEER_BINARY_HASHES)."
        fi
    else
        warn "rustc is not on PATH — the recorded rust_toolchain '$_prov_toolchain' is taken as DECLARED, not verified"
    fi
    log "release mode: rust_toolchain declared by the build host as '$_prov_toolchain' (recorded, NOT verified against the binaries)"
fi

# =============================================================================
# 4. Peer binary-hash agreement (optional). PEER_BINARY_HASHES may be a path to
#    a peer's binary-hashes.txt OR the inline hash lines themselves. Every node
#    in a closed net MUST run byte-identical binaries; a mismatch means the two
#    hosts built different code -> fail closed.
# =============================================================================
if [ -n "${PEER_BINARY_HASHES:-}" ]; then
    if [ -f "$PEER_BINARY_HASHES" ] && [ -r "$PEER_BINARY_HASHES" ]; then
        PEER_HASHES="$(cat "$PEER_BINARY_HASHES")"
        PEER_SRC="file:$PEER_BINARY_HASHES"
    else
        PEER_HASHES="$PEER_BINARY_HASHES"
        PEER_SRC="inline"
    fi
    log "comparing local binaries against peer manifest ($PEER_SRC)"
    _mismatch=0
    for _b in "${_bins[@]}"; do
        _name="$(basename "$_b")"
        # our manifest is exactly "<hash>  <basename>"
        _ours="$(printf '%s\n' "$FRESH_HASHES" | awk -v b="$_name" '$2==b{print $1; exit}')"
        # peer manifest is tolerant: any line carrying a 64-hex token AND a field
        # whose basename == this binary (handles "hash name", "hash /path", and
        # "name hash" orderings; no awk interval expressions for portability).
        _theirs="$(printf '%s\n' "$PEER_HASHES" | awk -v b="$_name" '
            {
                h=""; n=""
                for (i=1;i<=NF;i++) if ($i ~ /^[0-9a-fA-F]+$/ && length($i)==64) h=$i
                for (i=1;i<=NF;i++) { p=$i; sub(/.*\//,"",p); if (p==b) n=p }
                if (h!="" && n!="") { print tolower(h); exit }
            }')"
        if [ -z "$_theirs" ]; then
            warn "peer manifest ($PEER_SRC) has NO entry for $_name"
            _mismatch=1
        elif [ "$_ours" != "$_theirs" ]; then
            warn "binary MISMATCH for $_name:"
            warn "  local: $_ours"
            warn "  peer : $_theirs"
            _mismatch=1
        else
            log "peer match: $_name $_ours"
        fi
    done
    if [ "$_mismatch" -ne 0 ]; then
        die "peer binary-hash mismatch (see WARN lines above): all nodes MUST run byte-identical binaries — rebuild every host from the same commit (if PEER_BINARY_HASHES was meant to be a file path, ensure it exists and is readable)"
    fi
    log "peer binary-hash agreement OK (all three match)"
else
    log "PEER_BINARY_HASHES not set — skipping cross-host binary agreement check"
fi

# =============================================================================
# 5. Already-running nodes: verify network identity and effective algo-4 acceptance.
#    The consensus-identity RPC exposes both values for cross-node comparison.
# =============================================================================
_status_a="$(node_status a 2>/dev/null || true)"
_status_b="$(node_status b 2>/dev/null || true)"
_net_a="$(printf '%s\n' "$_status_a" | _kv node_network)"
_net_b="$(printf '%s\n' "$_status_b" | _kv node_network)"
# STN-003/§9: node_genesis_hash. CRITICAL: `kaspa-pq-validator status` prints EITHER
#   node_genesis_hash: <h> (server-reported)                    <- the NODE's own value
#   node_genesis_hash: <h> (CLI-derived from network id; ...)   <- THIS CONTROLLER's value
# and it falls back to the second form whenever getConsensusIdentity fails. `_kv` stops
# at the first space and would DISCARD that marker, so a CLI-derived value would be
# compared against another CLI-derived value and the gate would "pass" without the node
# ever being asked. Read the WHOLE value with `_line` and accept ONLY the server-reported
# form; anything else leaves the observed genesis EMPTY so the gates below treat it as
# "not reported" rather than as proof.
_genesis_observed() {   # <a|b> <status-text> -> echo the NODE-reported genesis, or ""
    local n="$1" line
    line="$(printf '%s\n' "$2" | _line node_genesis_hash)"
    case "$line" in
        '') return 0 ;;
        *'(server-reported)'*) printf '%s' "${line%% *}" | tr 'A-F' 'a-f' ;;
        *)
            warn "node-$n genesis is CLI-DERIVED (this controller computed it from the network id; the node did NOT report it) — the genesis gate did NOT run against node-$n; rebuild/restart it on a binary that serves getConsensusIdentity"
            if [ "${RELEASE_MODE:-0}" -eq 1 ]; then
                die "release mode: node-$n genesis is CLI-derived, so its genesis identity is UNVERIFIED — refusing to attest a release against it"
            fi
            return 0 ;;
    esac
}
_gen_a="$(_genesis_observed a "$_status_a")"
_gen_b="$(_genesis_observed b "$_status_b")"

_a_up=0; [ -n "$_net_a" ] && _a_up=1
_b_up=0; [ -n "$_net_b" ] && _b_up=1
_a_state=down; [ "$_a_up" -eq 1 ] && _a_state=up
_b_state=down; [ "$_b_up" -eq 1 ] && _b_state=up
log "already-running check: node-a=$_a_state (network='${_net_a:-}')  node-b=$_b_state (network='${_net_b:-}')"

# Fail-closed: if BOTH nodes are up they MUST agree on node_network.
if [ "$_a_up" -eq 1 ] && [ "$_b_up" -eq 1 ]; then
    if [ "$_net_a" != "$_net_b" ]; then
        die "both nodes are up but report DIFFERENT node_network (A='$_net_a' B='$_net_b') — stop the stray node(s) before running the harness"
    fi
    log "both nodes up and agree on node_network=$_net_a"
fi

# =============================================================================
# 5b. Genesis-hash identity (STN-003/§9). The genesis hash is the explicit
#     network/config-identity pin that the binary-hash check alone only IMPLIES:
#     byte-identical binaries produce an identical genesis, but this asserts it and
#     lets an operator pin the expected value across independently-built hosts.
#     There is no RPC that returns a node's genesis, so the value comes from the
#     validator's status (Params::from(network_id).genesis.hash) — the same
#     derivation consensus trusts for the unbond replay guard.
# =============================================================================
# Fail-closed: if BOTH nodes are up they MUST report the same genesis.
if [ "$_a_up" -eq 1 ] && [ "$_b_up" -eq 1 ]; then
    if [ -n "$_gen_a" ] && [ -n "$_gen_b" ]; then
        if [ "$_gen_a" != "$_gen_b" ]; then
            die "both nodes are up but report DIFFERENT node_genesis_hash (A='$_gen_a' B='$_gen_b') — the hosts are on different genesis/consensus params; rebuild every host from the same commit and use the same NETWORK/NETSUFFIX"
        fi
        log "both nodes agree on node_genesis_hash=$_gen_a"
    else
        warn "node_genesis_hash not reported by one/both nodes (A='${_gen_a:-}' B='${_gen_b:-}') — an older validator binary predates this status field; rebuild via build-and-hash.sh to enable the genesis parity gate"
    fi
fi

# Optional operator pin: every up node's genesis MUST equal EXPECTED_GENESIS_HASH.
if [ -n "${EXPECTED_GENESIS_HASH:-}" ]; then
    _exp="$(printf '%s' "$EXPECTED_GENESIS_HASH" | tr 'A-F' 'a-f')"
    case "$_exp" in
        *[!0-9a-f]* | "") die "EXPECTED_GENESIS_HASH must be hex: '$EXPECTED_GENESIS_HASH'" ;;
    esac
    [ "${#_exp}" -eq 64 ] || die "EXPECTED_GENESIS_HASH must be 64 hex chars (a 32-byte block hash); got ${#_exp}: '$EXPECTED_GENESIS_HASH'"
    # <label> <observed-genesis> <up>
    _assert_expected_genesis() {
        [ "$3" -eq 1 ] || return 0
        if [ -z "$2" ]; then
            die "EXPECTED_GENESIS_HASH is set but node-$1 did not report node_genesis_hash (older validator binary?) — cannot verify the required genesis pin; rebuild via build-and-hash.sh"
        elif [ "$2" != "$_exp" ]; then
            die "node-$1 genesis MISMATCH: node_genesis_hash='$2' != EXPECTED_GENESIS_HASH='$_exp' — this node is NOT on the expected network/config"
        fi
        log "node-$1 genesis matches EXPECTED_GENESIS_HASH ($_exp)"
    }
    _assert_expected_genesis a "$_gen_a" "$_a_up"
    _assert_expected_genesis b "$_gen_b" "$_b_up"
fi

# Soft, loud heads-up if a running node's network does not match this config —
# usually a stray node from another network bound to our ports (node start would
# then fail on bind; surface it now). WARN, not die: node_network's exact string
# can vary across builds, so we do not hard-gate on equality with $NETWORK.
if [ "$_a_up" -eq 1 ] && [ "$_net_a" != "$NETWORK" ]; then
    warn "node-a reports node_network='$_net_a' but this harness is configured NETWORK='$NETWORK' — is a stray node bound to node-a's ports?"
fi
if [ "$_b_up" -eq 1 ] && [ "$_net_b" != "$NETWORK" ]; then
    warn "node-b reports node_network='$_net_b' but this harness is configured NETWORK='$NETWORK' — is a stray node bound to node-b's ports?"
fi

# --palw-enable-algo4 consistency. Since getConsensusIdentity (review §11.2) a
# RUNNING node reports its EFFECTIVE flag server-side — verify it directly and
# fail-closed on a split. Only an older binary (no identity in `status`) falls
# back to the loud unverifiable warning.
_algo4_of() {   # <a|b> -> "true"/"false"/"" (unknown: node down or old binary)
    local n="$1" blob
    _endpoint_open "$(node_wrpc "$n")" || { printf ''; return 0; }
    blob="$("$VAL" status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" 2>/dev/null || true)"
    printf '%s\n' "$blob" | awk -F': ' '$1=="node_palw_algo4_accept" {print $2; exit}' | awk '{print $1}'
}
if [ "$_a_up" -eq 1 ] || [ "$_b_up" -eq 1 ]; then
    _a4a=""; _a4b=""
    [ "$_a_up" -eq 1 ] && _a4a="$(_algo4_of a)"
    [ "$_b_up" -eq 1 ] && _a4b="$(_algo4_of b)"
    if [ -n "$_a4a" ] || [ -n "$_a4b" ]; then
        log "effective palw_algo4_accept (server-reported): node-a='${_a4a:-<down/old>}' node-b='${_a4b:-<down/old>}'"
        if [ -n "$_a4a" ] && [ -n "$_a4b" ] && [ "$_a4a" != "$_a4b" ]; then
            die "SPLIT algo-4 acceptance: node A reports palw_algo4_accept=$_a4a but node B reports $_a4b — one side would accept blocks the other rejects. Stop the set and restart every node with the SAME --palw-enable-algo4 setting."
        fi
    else
        warn "--palw-enable-algo4 consistency could not be verified over RPC (nodes run an older binary without getConsensusIdentity). It MUST be identical on EVERY node; this host is configured PALW_ENABLE_ALGO4=${PALW_ENABLE_ALGO4:-unset}. Rebuild + restart to make it verifiable."
    fi
else
    log "no node currently up on the configured RPC endpoints — clean start"
    log "reminder: start EVERY node with the same --palw-enable-algo4 setting (PALW_ENABLE_ALGO4=${PALW_ENABLE_ALGO4:-unset}); once up, preflight verifies it via getConsensusIdentity"
fi

# -----------------------------------------------------------------------------
# §11.4 signed-network-manifest gate. SHARED mode (any remote node configured, or
# PALW_REQUIRE_MANIFEST=1) REQUIRES a verified signed manifest — no manifest, no
# shared start. Single-host default stays ungated (the closed one-box dev loop).
# STN-10 (d): release mode set PALW_REQUIRE_MANIFEST=1 in §0, so this gate is
# unconditional there. network-manifest.sh owns the checks and already fail-closes
# with the exact missing path (manifest / .sig / allowed-signers) — this stage
# calls it rather than re-implementing any of it.
# -----------------------------------------------------------------------------
# shellcheck source=remote.sh
. "$SCRIPT_DIR/remote.sh"
if [ "${PALW_REQUIRE_MANIFEST:-0}" = "1" ] || node_is_remote a || node_is_remote b; then
    if [ "$RELEASE_MODE" -eq 1 ]; then
        log "release mode: a verified signed network manifest is REQUIRED (PALW_RELEASE_MODE=1 implies PALW_REQUIRE_MANIFEST=1). Note the verify path also compares BOTH LIVE nodes' identity, so a release preflight must run against the running net."
    else
        log "shared mode detected (remote node configured or PALW_REQUIRE_MANIFEST=1) — a verified signed network manifest is REQUIRED"
    fi
    bash "$SCRIPT_DIR/network-manifest.sh" verify \
        || die "signed network-manifest verification failed — this net will not start without a verified release identity (generate on the coordinator: ./network-manifest.sh generate ; verify anywhere: ./network-manifest.sh verify $PALW_DATA_ROOT/artifacts/network-manifest.json). Signing is the operator's step: preflight never touches a key."
fi

# -----------------------------------------------------------------------------
# Summary.
# -----------------------------------------------------------------------------
_peer_note=""
if [ -n "${PEER_BINARY_HASHES:-}" ]; then _peer_note=" + peer-agreed"; fi
# Release note states only what was actually gated — the binaries' BUILD is still
# unattested (see §3b scope), so this must not read as "release verified".
_release_note=""
if [ "$RELEASE_MODE" -eq 1 ]; then
    _release_note=" [RELEASE MODE: recorded hashes matched (nothing rewritten), worktree clean, SOURCE_COMMIT/tag/Cargo.lock matched, signed manifest verified]"
fi
log "preflight OK: env validated, binaries hashed$_peer_note, data dirs ready under $PALW_DATA_ROOT (NETWORK=$NETWORK, TICKET_MODE=$TICKET_MODE)$_release_note"
