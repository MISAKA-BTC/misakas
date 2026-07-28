#!/usr/bin/env bash
# =============================================================================
# release-bundle.sh — build a SECRET-FREE, provenance-carrying release bundle
#                     for the PALW closed shared testnet (audit STN-02/STN-03).
#
#   usage:  ./release-bundle.sh <git-tag-or-ref> [outdir]
#           ./release-bundle.sh --help
#
# WHY THIS EXISTS (both findings are the same root cause: the ZIP was made by
# hand from the WORKING TREE):
#   STN-02  the submitted release ZIP shipped scripts/palw-shared-testnet/env.local
#           — node IPs, ssh users, remote dirs, the controller SSH private-key
#           PATH and the port topology — even though .gitignore already ignores
#           that file. Zipping a directory ignores .gitignore; `git archive` does
#           not have to be told, it CANNOT see an ignored or untracked file.
#   STN-03  the release carried no provenance: no signed source tag, no
#           SOURCE_COMMIT, no Cargo.lock hash, no exact rustc version, no
#           --locked, no dependency inventory. Nothing tied the bytes to a source.
#
# WHAT IT DOES
#   1. refuses to run on a dirty worktree and requires an explicit ref;
#   2. builds the payload with `git archive <ref>` (never a tree copy), so an
#      ignored/untracked file cannot enter the bundle by construction;
#   3. extracts that exact tarball to a staging dir and sweeps it with a
#      filename DENYLIST + a content scan (key material, public IP literals) as
#      defense-in-depth — belt AND braces, because (2) is only as good as the
#      .gitignore that fed it;
#   4. emits PROVENANCE.txt (commit, tag + tag-signature status, Cargo.lock
#      SHA-256, rustc -Vv / cargo -V, workspace rust-version, release binary
#      SHA-256s when present) and RELEASE-MANIFEST.txt listing every emitted
#      artifact with its SHA-256;
#   5. PRINTS the commands the operator runs to sign the manifest.
#
# SIGNING IS THE OPERATOR'S STEP. This script never generates, requests, reads,
# prints or otherwise handles a private key, and never runs a signing command.
# It stops at "here is the exact command to run"; a human with the release key
# does the rest. (network-manifest.sh signs a DIFFERENT object — the live
# network identity — and manages its own dedicated key; this script does not
# reuse or touch that key.)
#
# SCOPE: this proves the payload came from a named git ref and records
# the toolchain that was on THIS host. It does not prove the recorded binaries
# were built from that ref (no reproducible-build attestation), and the secret
# sweep is a heuristic, not a proof of absence. See the "does NOT prove" block
# it writes into README-RELEASE.md.
#
# Design rules (shared with the rest of the harness):
#   * set -euo pipefail; sources common.sh and uses ONLY its helpers.
#   * IDEMPOTENT   — never overwrites an existing output dir (die instead).
#   * FAIL-CLOSED  — every ambiguity is a die(); a check that cannot run WARNs
#                    loudly and, under PALW_RELEASE_MODE=1, is fatal.
#   * PORTABLE     — bash 3.2 (stock macOS) + Linux; BSD + GNU coreutils.
#   * HONEST       — an absent artifact is reported as absent, never implied.
#
# Env knobs (all optional):
#   PALW_RELEASE_MODE=1  release gate: an unsigned/unverified tag, a missing
#                        Cargo.lock hash, an absent rustc/cargo, an absent
#                        dependency inventory and any public IP literal in the
#                        payload each become FATAL instead of a loud WARN.
#   PALW_BUNDLE_PATHS    space-separated pathspecs to archive, relative to the
#                        repo root. Default: scripts/palw-shared-testnet (the
#                        harness — the artifact the audit was about). See the
#                        note on the denylist below before widening this.
#   PALW_SBOM_FILE       path to a pre-generated dependency inventory / SBOM to
#                        include and hash. This script does NOT run cargo, so it
#                        cannot produce one itself; absent -> WARN (fatal in
#                        release mode) with the exact command printed.
#
# NOTE ON PALW_BUNDLE_PATHS: the denylist below is deliberately blunt and has NO
# bypass. Widening the pathspec to the whole workspace ('.') WILL trip it on
# legitimate Rust source — wallet/keys/** matches `**/keys/**` and
# wallet/keys/src/secret.rs matches `**/*secret*`. That is a known limitation,
# not a bug to be flag-ed around: a release whose payload needs those paths must
# be assembled by a reviewed, narrowed pathspec, not by weakening the sweep.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd -P)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="${PALW_LOG_TAG:-release-bundle}"; export PALW_LOG_TAG

usage() {
    cat >&2 <<EOF
usage: ${0##*/} <git-tag-or-ref> [outdir]

  <git-tag-or-ref>  REQUIRED. The source ref to publish (e.g. a signed tag
                    v0.3.0-testnet). The payload is produced with
                    \`git archive\` from THIS ref — never from the worktree.
  [outdir]          Output directory. Must NOT already exist (idempotent:
                    this script never overwrites a previous bundle).
                    Default: <repo>/target/palw-release/<ref>

env: PALW_RELEASE_MODE=1  unsigned tag / missing lock hash / missing toolchain /
                          missing dependency inventory / public IP literals are FATAL
     PALW_BUNDLE_PATHS    pathspecs to archive (default: scripts/palw-shared-testnet)
     PALW_SBOM_FILE       pre-generated dependency inventory to include + hash

Signing the manifest is the OPERATOR's step; this script only prints the command.
EOF
}

case "${1:-}" in
    -h|--help|help) usage; exit 0 ;;
    "") usage; die "a git tag or ref is REQUIRED — a release with no named source is exactly finding STN-03" ;;
esac

REF="$1"
OUTDIR_ARG="${2:-}"
# VALIDATED, not merely read: `gate()` below only dies on the exact string "1", so an
# unvalidated PALW_RELEASE_MODE=true (or yes, or "1 ") would silently downgrade EVERY
# release check — including the unsigned-tag refusal — back to a warning. A fail-closed
# gate does not guess. (Same guard as preflight.sh / network-manifest.sh.)
case "${PALW_RELEASE_MODE:-0}" in
    1)    RELEASE_MODE=1 ;;
    0|"") RELEASE_MODE=0 ;;
    *)    die "PALW_RELEASE_MODE must be 1 (release gate) or 0/unset (dev loop); got '${PALW_RELEASE_MODE:-}' — a fail-closed gate does not guess" ;;
esac

require_cmd git tar find grep awk sed

# gate <msg...> — a check that did not pass: WARN loudly, and in release mode
# die. Used for everything that is "provenance we wanted but do not have"; it is
# never used to downgrade a secret-leak finding (those are unconditionally fatal).
gate() {
    if [ "$RELEASE_MODE" = "1" ]; then
        die "$* — PALW_RELEASE_MODE=1, fail-closed (NO-GO)"
    fi
    warn "$* — NOT fatal here, but PALW_RELEASE_MODE=1 makes it FATAL (do not publish as-is)"
}

# Pick the available SHA-256 tool (same detection as build-and-hash.sh):
# sha256sum (GNU coreutils) or `shasum -a 256` (BSD / stock macOS).
if command -v sha256sum >/dev/null 2>&1; then
    SHA256_TOOL="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SHA256_TOOL="shasum -a 256"
else
    die "need 'sha256sum' or 'shasum' on PATH — a bundle with no hashes is not a release"
fi

# sha256_file <path> — 64-hex digest, or die.
sha256_file() {
    local f="$1" out
    [ -r "$f" ] || die "cannot read for hashing: $f"
    out="$($SHA256_TOOL "$f" 2>/dev/null | awk 'NR==1{print $1}')" || true
    case "$out" in ''|*[!0-9a-f]*) die "failed to compute sha256 of $f (tool: $SHA256_TOOL, got '$out')" ;; esac
    [ "${#out}" -eq 64 ] || die "sha256 of $f has unexpected length ${#out}: '$out'"
    printf '%s\n' "$out"
}
# sha256_stdin — digest of stdin (used for `git show <ref>:Cargo.lock`).
sha256_stdin() {
    local out
    out="$($SHA256_TOOL 2>/dev/null | awk 'NR==1{print $1}')" || true
    case "$out" in ''|*[!0-9a-f]*) return 1 ;; esac
    [ "${#out}" -eq 64 ] || return 1
    printf '%s\n' "$out"
}

# -----------------------------------------------------------------------------
# REPO_ROOT — resolved WITHOUT load_env, deliberately.
#
# load_env() sources env.local and then requires target/release/{kaspad,
# kaspa-pq-validator,misaminer} to exist. Neither is acceptable here:
#   * env.local is THE file finding STN-02 is about. A release bundler must
#     never read it — it has no business holding node IPs or key paths in its
#     environment, and a clean checkout of a public repo does not have one.
#   * a release must be buildable from a checkout with nothing built yet; the
#     binaries are OPTIONAL provenance (hashed if present, reported absent if
#     not), never a precondition.
# So we resolve just REPO_ROOT (env override, else two levels up from the
# harness dir) and then pin it to the actual git top-level. This is the same
# minimal bootstrap build-and-hash.sh documents, for the same kind of reason —
# it re-implements no common.sh helper.
# -----------------------------------------------------------------------------
: "${REPO_ROOT:=$(cd "$COMMON_SH_DIR/../.." && pwd -P)}"
[ -d "$REPO_ROOT" ] || die "REPO_ROOT does not exist: $REPO_ROOT"
REPO_ROOT="$(realpath_p "$REPO_ROOT")"
_top="$(git -C "$REPO_ROOT" rev-parse --show-toplevel 2>/dev/null || true)"
[ -n "$_top" ] || die "$REPO_ROOT is not inside a git repository — this script can only bundle tracked, committed source"
REPO_ROOT="$(realpath_p "$_top")"; export REPO_ROOT
log "repo root: $REPO_ROOT"

# -----------------------------------------------------------------------------
# Gate 1 — clean worktree. A dirty tree means the operator has changes that are
# NOT in the ref being published; the resulting bundle would misrepresent its
# own source. (This does not list ignored files, which is the point: the bundle
# is built from the ref, so ignored files are irrelevant to it.)
# -----------------------------------------------------------------------------
DIRTY="$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null || true)"
if [ -n "$DIRTY" ]; then
    die "worktree is DIRTY — commit, stash or discard before cutting a release. A bundle must be reproducible from '$REF' alone.
$DIRTY"
fi
log "worktree clean"

# -----------------------------------------------------------------------------
# Gate 2 — the ref must resolve to a commit.
# -----------------------------------------------------------------------------
SOURCE_COMMIT="$(git -C "$REPO_ROOT" rev-parse --verify -q "${REF}^{commit}" 2>/dev/null || true)"
[ -n "$SOURCE_COMMIT" ] || die "ref '$REF' does not resolve to a commit in $REPO_ROOT"
log "source commit: $SOURCE_COMMIT ($REF)"

SOURCE_TAG=""
if git -C "$REPO_ROOT" rev-parse --verify -q "refs/tags/$REF" >/dev/null 2>&1; then
    SOURCE_TAG="$REF"
fi

# Tag signature status — captured verbatim, never summarised into a PASS.
# `git verify-tag` covers annotated+signed tags; a lightweight tag or a bare
# commit ref has no tag signature at all, which is reported as such.
SIG_STATUS="unsigned"
SIG_OUT=""
if [ -n "$SOURCE_TAG" ]; then
    if SIG_OUT="$(git -C "$REPO_ROOT" verify-tag "$SOURCE_TAG" 2>&1)"; then
        SIG_STATUS="tag-signature-VERIFIED"
    else
        SIG_STATUS="tag-signature-NOT-VERIFIED"
    fi
else
    if SIG_OUT="$(git -C "$REPO_ROOT" verify-commit "$SOURCE_COMMIT" 2>&1)"; then
        SIG_STATUS="commit-signature-VERIFIED (ref is not a tag)"
    else
        SIG_STATUS="unsigned (ref is not a tag, and the commit carries no verified signature)"
    fi
fi
# Only a tag whose signature actually VERIFIES clears this gate. A verified
# commit signature on a non-tag ref is captured as evidence but does NOT clear
# it: a release names a signed tag (STN-03), and "close enough" is how an
# unsigned release ships.
if [ "$SIG_STATUS" = "tag-signature-VERIFIED" ]; then
    log "source signature: $SOURCE_TAG verified by git verify-tag"
else
    gate "source is NOT a verified signed tag (status: $SIG_STATUS; git said: ${SIG_OUT:-<no output>})"
fi

# -----------------------------------------------------------------------------
# Output dir — must not exist (idempotent: never clobber a previous bundle).
# -----------------------------------------------------------------------------
REF_SLUG="$(printf '%s' "$REF" | tr -c 'A-Za-z0-9._-' '-')"
OUTDIR="${OUTDIR_ARG:-$REPO_ROOT/target/palw-release/$REF_SLUG}"
OUTDIR="$(realpath_p "$OUTDIR")"
if [ -e "$OUTDIR" ]; then
    die "output dir already exists: $OUTDIR — refusing to overwrite a previous bundle. Remove it, or pass a different [outdir]."
fi
install -d -m 0755 "$OUTDIR" || die "cannot create output dir: $OUTDIR"

STAGE="$OUTDIR/.stage"
# Cleanup, LIFO. The OUTDIR removal is registered LAST so it runs FIRST: if any
# gate below fails, the tarball we already wrote may contain an unswept payload,
# and an unswept payload must never survive a failed run for someone to pick up
# and ship. On success _BUNDLE_OK=1 keeps the bundle and only the staging dir goes.
_BUNDLE_OK=0
register_cleanup "rm -rf \"$STAGE\""
register_cleanup "[ \"\${_BUNDLE_OK:-0}\" = \"1\" ] || { rm -rf \"$OUTDIR\"; warn 'removed the half-built bundle dir — an unswept payload must not survive a failed run'; }"

# -----------------------------------------------------------------------------
# Payload — `git archive <ref>`, NOT a tree copy (STN-02).
# git archive reads the ref's TREE OBJECT: an ignored or untracked file is not
# in the tree, so it cannot be in the tarball. This is the actual fix; the sweep
# further down is only defense-in-depth.
# -----------------------------------------------------------------------------
BUNDLE_PATHS="${PALW_BUNDLE_PATHS:-scripts/palw-shared-testnet}"
# The pathspec list is word-split when handed to git archive, so every element
# must be a plain path: no quoting, globbing or metacharacters to reason about.
for _p in $BUNDLE_PATHS; do
    case "$_p" in
        *[!A-Za-z0-9._/-]*) die "PALW_BUNDLE_PATHS element '$_p' has characters this script will not word-split safely (allowed: A-Z a-z 0-9 . _ / - , space-separated)" ;;
    esac
done

BUNDLE_NAME="palw-shared-testnet-$REF_SLUG"
TARBALL="$OUTDIR/$BUNDLE_NAME.tar.gz"
log "git archive $REF -- $BUNDLE_PATHS  ->  $TARBALL"
# shellcheck disable=SC2086  # intentional word-split of the validated pathspec list
git -C "$REPO_ROOT" archive --format=tar.gz --prefix="$BUNDLE_NAME/" "$SOURCE_COMMIT" -- $BUNDLE_PATHS > "$TARBALL" \
    || die "git archive failed for ref '$REF' with pathspec '$BUNDLE_PATHS' (does every path exist in that ref?)"
[ -s "$TARBALL" ] || die "git archive produced an empty tarball — nothing matched '$BUNDLE_PATHS' in $REF"

# Extract THE SAME tarball we will ship, so the sweep inspects the shipped bytes
# and not a second, differently-produced copy of them.
install -d -m 0700 "$STAGE" || die "cannot create staging dir: $STAGE"
tar -xf "$TARBALL" -C "$STAGE" || die "could not extract $TARBALL for the secret sweep"
log "payload staged for sweep: $STAGE"

# -----------------------------------------------------------------------------
# Sweep 1 — filename DENYLIST (defense-in-depth).
# Patterns, per the audit: **/env.local **/*.seed **/keys/** **/state.env
# **/*.log **/known_hosts **/*secret* **/*.pid **/id_rsa* **/*.key
# There is no override knob. A hit is fatal, full stop.
# -----------------------------------------------------------------------------
# Scanned from INSIDE the staging dir (subshell cd, as build-and-hash.sh does for
# cargo) so the patterns only ever see payload-relative paths — an output dir that
# happened to sit under a directory called keys/ must not fake a hit.
DENY_HITS="$(
    ( cd "$STAGE" && find . \( \
            -name 'env.local'   -o \
            -name '*.seed'      -o \
            -path '*/keys/*'    -o \
            \( -type d -name 'keys' \) -o \
            -name 'state.env'   -o \
            -name '*.log'       -o \
            -name 'known_hosts' -o \
            -name '*secret*'    -o \
            -name '*.pid'       -o \
            -name 'id_rsa*'     -o \
            -name '*.key'       \
        \) -print ) 2>/dev/null | sed 's|^\./||' | sort || true
)"
if [ -n "$DENY_HITS" ]; then
    die "DENYLIST hit in the staged payload — refusing to publish (this is finding STN-02):
$DENY_HITS
These paths are TRACKED in git at $REF (git archive only emits tracked content),
so the fix is in the repository, not here: remove/redact them at the source, or
narrow PALW_BUNDLE_PATHS to a reviewed subtree. This script has no bypass."
fi
log "denylist sweep clean (env.local / *.seed / keys/** / state.env / *.log / known_hosts / *secret* / *.pid / id_rsa* / *.key)"

# -----------------------------------------------------------------------------
# Sweep 2 — file CONTENTS.
#
# (a) key material: PEM headers. ALWAYS fatal — no mode, no knob. This script
#     never prints the matching bytes, only the paths.
#     The needles are assembled from fragments on purpose: this script is itself
#     inside the payload it scans, and a literal needle here would make every
#     run flag release-bundle.sh. Prose in this file stays lowercase for the
#     same reason (the scan is case-sensitive; PEM headers are uppercase).
# -----------------------------------------------------------------------------
_PEM1="BEGIN OPENSSH PRIVATE ""KEY"
_PEM2="BEGIN RSA PRIVATE ""KEY"
_PEM3="PRIVATE ""KEY-----"
KEY_HITS="$( ( cd "$STAGE" && grep -rlIF -e "$_PEM1" -e "$_PEM2" -e "$_PEM3" . ) 2>/dev/null | sed 's|^\./||' | sort || true )"
if [ -n "$KEY_HITS" ]; then
    die "KEY MATERIAL detected in the staged payload — refusing to publish, and treat these as COMPROMISED (they are committed to git):
$KEY_HITS
Rotate whatever those files hold, purge them from the ref, and cut a new tag."
fi
log "content sweep clean: no pem key-material headers in the payload"

# (b) IP literals. Reported LOUDLY: a routable address in a shipped script or
#     doc is the STN-02 topology leak in another costume (loopback, RFC1918 and
#     the RFC5737 documentation ranges are excluded; RFC6598 shared space is NOT
#     excluded — an overlay address is still a real node address). Non-fatal by
#     default because a literal can be legitimate; FATAL in release mode.
IP_HITS="$(
    ( cd "$STAGE" && grep -rEoI '(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])(\.(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])){3}' . ) 2>/dev/null \
    | sed -E 's|^(.*):([0-9.]+)$|\2 \1|' \
    | grep -Ev '^(0\.|10\.|127\.|169\.254\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.|224\.0\.0\.|255\.255\.255\.255[[:space:]])' \
    | sed 's| \./| |' | sort -u || true
)"
IP_COUNT=0
if [ -n "$IP_HITS" ]; then
    IP_COUNT="$(printf '%s\n' "$IP_HITS" | grep -c . || true)"
    warn "payload contains $IP_COUNT non-private IP literal(s) — each line is '<address> <file>':"
    printf '%s\n' "$IP_HITS" >&2
    gate "non-private IP literals in the payload (see the list above) — redact them at the source before publishing"
else
    log "content sweep clean: no non-private IP literals in the payload"
fi

# =============================================================================
# Provenance (STN-03). Everything below is recorded as found — an absent input
# is written down as absent, never inferred and never quietly skipped.
# =============================================================================
PROVENANCE="$OUTDIR/PROVENANCE.txt"
README="$OUTDIR/README-RELEASE.md"
MANIFEST="$OUTDIR/RELEASE-MANIFEST.txt"

# Cargo.lock — hashed FROM THE REF (git show), not from the worktree, so the
# recorded digest belongs to the published source and not to whatever happens to
# be checked out.
LOCK_SHA=""
if git -C "$REPO_ROOT" cat-file -e "$SOURCE_COMMIT:Cargo.lock" 2>/dev/null; then
    LOCK_SHA="$(git -C "$REPO_ROOT" show "$SOURCE_COMMIT:Cargo.lock" | sha256_stdin || true)"
fi
if [ -n "$LOCK_SHA" ]; then
    log "Cargo.lock sha256: $LOCK_SHA (from $REF)"
else
    gate "no Cargo.lock at $REF (or it could not be hashed) — the dependency set of this release is unpinned"
    LOCK_SHA="<absent at $REF>"
fi

# Workspace MSRV declared in the ref's Cargo.toml ([workspace.package] rust-version).
RUST_VERSION_DECL="$(
    git -C "$REPO_ROOT" show "$SOURCE_COMMIT:Cargo.toml" 2>/dev/null | awk '
        /^[[:space:]]*\[/ { sect=$0 }
        sect ~ /\[workspace\.package\]/ && /^[[:space:]]*rust-version[[:space:]]*=/ {
            sub(/.*=[[:space:]]*"?/, ""); sub(/".*$/, ""); print; exit
        }' || true
)"
[ -n "$RUST_VERSION_DECL" ] || RUST_VERSION_DECL="<not declared in [workspace.package] at $REF>"

# Toolchain actually present on THIS host. This is the toolchain that WOULD
# build the release here; it is not proof of what built any existing binary.
RUSTC_VV="<rustc not on PATH on the bundling host>"
CARGO_V="<cargo not on PATH on the bundling host>"
if command -v rustc >/dev/null 2>&1; then
    RUSTC_VV="$(rustc -Vv 2>&1 || true)"
else
    gate "rustc is not on PATH — the exact compiler version cannot be recorded (STN-03 wants it pinned)"
fi
if command -v cargo >/dev/null 2>&1; then
    CARGO_V="$(cargo -V 2>&1 || true)"
else
    gate "cargo is not on PATH — the exact cargo version cannot be recorded"
fi

# Release binaries — hashed if they exist. They are NOT built here and NOT proven
# to come from $REF; that caveat is written into the file next to the hashes.
# Each hash lands in its OWN assignment first: a die() inside a command
# substitution only kills the subshell, so a nested $(sha256_file ...) would
# silently yield an empty digest. As a plain assignment the failure propagates
# under set -e and the run stops (fail-closed).
BIN_LINES=""
for _b in kaspad kaspa-pq-validator misaminer; do
    _p="$REPO_ROOT/target/release/$_b"
    if [ -f "$_p" ]; then
        _h="$(sha256_file "$_p")"
        BIN_LINES="$BIN_LINES  $_h  $_b"$'\n'
    else
        BIN_LINES="$BIN_LINES  <absent: not built on this host>  $_b"$'\n'
    fi
done

# Dependency inventory / SBOM. This script does not run cargo, so it cannot
# generate one; it includes a pre-generated file if the operator points at it.
SBOM_STATUS="ABSENT — none supplied via PALW_SBOM_FILE"
SBOM_DEST=""
if [ -n "${PALW_SBOM_FILE:-}" ]; then
    [ -f "$PALW_SBOM_FILE" ] || die "PALW_SBOM_FILE is set but not a readable file: $PALW_SBOM_FILE"
    SBOM_DEST="$OUTDIR/$(basename "$PALW_SBOM_FILE")"
    cp "$PALW_SBOM_FILE" "$SBOM_DEST" || die "could not copy the dependency inventory into $OUTDIR"
    SBOM_STATUS="included: $(basename "$SBOM_DEST") (supplied by the operator; this script did not generate or validate it)"
    log "dependency inventory included: $SBOM_DEST"
else
    gate "no dependency inventory / SBOM supplied (PALW_SBOM_FILE unset). This script runs no cargo, so it cannot make one; produce it on the build host and re-run, e.g.
    (cd $REPO_ROOT && cargo tree --locked --workspace --edges normal) > sbom-cargo-tree.txt   # dependency inventory (NOT a signed SBOM)
    (cd $REPO_ROOT && cargo cyclonedx --all --format json)                                    # if cargo-cyclonedx is installed
  then: PALW_SBOM_FILE=<that file> ${0##*/} $REF"
fi

BUILD_CMD="cargo build --release --locked -p kaspad -p kaspa-pq-validator -p misaminer"

log "writing $PROVENANCE"
{
    printf '%s\n' "PALW closed shared testnet — release provenance (schema: palw-release-provenance-v1)"
    printf '%s\n' "Answers audit finding STN-03. Every field is recorded as found on the bundling host."
    printf '\n'
    printf 'source_ref:            %s\n' "$REF"
    printf 'source_commit:         %s\n' "$SOURCE_COMMIT"
    printf 'source_tag:            %s\n' "${SOURCE_TAG:-<ref is not a tag>}"
    printf 'source_signature:      %s\n' "$SIG_STATUS"
    printf 'bundle_pathspec:       %s\n' "$BUNDLE_PATHS"
    printf 'payload_built_by:      git archive (tracked tree objects only — ignored/untracked files cannot be present)\n'
    printf '\n'
    printf '%s\n' "--- source signature output (git verify-tag / git verify-commit, verbatim) ---"
    printf '%s\n' "${SIG_OUT:-<no output>}"
    printf '\n'
    printf 'cargo_lock_sha256:     %s\n' "$LOCK_SHA"
    printf 'workspace_rust_version: %s\n' "$RUST_VERSION_DECL"
    printf 'declared_build_cmd:    %s\n' "$BUILD_CMD"
    printf '\n'
    printf '%s\n' "--- rustc -Vv (bundling host) ---"
    printf '%s\n' "$RUSTC_VV"
    printf '%s\n' "--- cargo -V (bundling host) ---"
    printf '%s\n' "$CARGO_V"
    printf '\n'
    printf '%s\n' "--- release binary SHA-256 ($REPO_ROOT/target/release) ---"
    printf '%s' "$BIN_LINES"
    printf '%s\n' "  NOTE: these are hashes of whatever is in the local target/release tree. Nothing"
    printf '%s\n' "  here proves they were built from $SOURCE_COMMIT, nor with --locked: build-and-hash.sh"
    printf '%s\n' "  does not currently pass --locked, so unless the operator built with declared_build_cmd"
    printf '%s\n' "  above, lockfile pinning is NOT proven for these bytes."
    printf '\n'
    printf 'dependency_inventory:  %s\n' "$SBOM_STATUS"
    printf 'ip_literals_reported:  %s\n' "$IP_COUNT"
    printf 'release_mode:          %s\n' "$( [ "$RELEASE_MODE" = "1" ] && printf 'PALW_RELEASE_MODE=1 (fail-closed)' || printf 'off (warnings were NOT fatal)' )"
    printf 'manifest_signature:    NOT SIGNED BY THIS SCRIPT — signing is the operator step (see README-RELEASE.md)\n'
} > "$PROVENANCE" || die "could not write $PROVENANCE"
chmod 0644 "$PROVENANCE" 2>/dev/null || true

log "writing $README"
{
    printf '%s\n' "# PALW closed shared testnet — release bundle \`$REF\`"
    printf '\n'
    printf '%s\n' "Built by \`scripts/palw-shared-testnet/release-bundle.sh\` from git ref \`$REF\`"
    printf '%s\n' "(commit \`$SOURCE_COMMIT\`)."
    printf '\n'
    printf '%s\n' "## Contents"
    printf '\n'
    printf '%s\n' "* \`$BUNDLE_NAME.tar.gz\` — the payload, produced with \`git archive\`. It contains"
    printf '%s\n' "  only files tracked in git at that commit under \`$BUNDLE_PATHS\`."
    printf '%s\n' "* \`PROVENANCE.txt\` — source commit/tag, tag-signature status, Cargo.lock SHA-256,"
    printf '%s\n' "  toolchain versions, declared build command, release binary hashes."
    printf '%s\n' "* \`RELEASE-MANIFEST.txt\` — SHA-256 of every file above. This is the file to sign."
    printf '\n'
    printf '%s\n' "## What this bundle proves"
    printf '\n'
    printf '%s\n' "* The payload is byte-for-byte the tracked content of \`$SOURCE_COMMIT\`. Files that"
    printf '%s\n' "  are ignored or untracked — \`env.local\`, seeds, keys, logs, pid files — cannot be"
    printf '%s\n' "  present, because \`git archive\` reads tree objects and never the working tree."
    printf '%s\n' "  (Audit STN-02: the previous ZIP was made by zipping the working tree.)"
    printf '%s\n' "* The staged payload passed a filename denylist and a content scan for key material."
    printf '%s\n' "* The source, toolchain and lockfile identities in \`PROVENANCE.txt\` were read at"
    printf '%s\n' "  build time on the bundling host, not typed in by hand."
    printf '\n'
    printf '%s\n' "## What this bundle does NOT prove"
    printf '\n'
    printf '%s\n' "* **Not an absence-of-secrets proof.** The denylist and content scan are heuristics."
    printf '%s\n' "  They catch the known shapes; they cannot prove nothing sensitive is in the payload."
    printf '%s\n' "  Result for this bundle: denylist clean, no key-material headers, $IP_COUNT non-private"
    printf '%s\n' "  IP literal(s) reported (listed in the build log; count in \`PROVENANCE.txt\`)."
    printf '%s\n' "* **Not a reproducible build.** Any binary hashes in \`PROVENANCE.txt\` are of the local"
    printf '%s\n' "  \`target/release\` tree. Nothing ties them to this commit or to \`--locked\`."
    printf '%s\n' "* **Not a dependency audit.** No \`cargo audit\`/\`cargo deny\` ran; a supplied inventory"
    printf '%s\n' "  is copied and hashed as-is, not validated."
    printf '%s\n' "* **Not authenticated until signed.** This script does not sign anything. Until a"
    printf '%s\n' "  detached signature over \`RELEASE-MANIFEST.txt\` exists and verifies against a key"
    printf '%s\n' "  you already trust, these bytes are unattributed."
    printf '%s\n' "* **Not the network identity.** Genesis hash, consensus params and the node roster"
    printf '%s\n' "  are a different signed object — see \`network-manifest.sh\`."
    printf '%s\n' "* Source signature status for this release: \`$SIG_STATUS\`."
    printf '\n'
    printf '%s\n' "## Verifying (recipient)"
    printf '\n'
    printf '%s\n' '```'
    printf '%s\n' "shasum -a 256 -c RELEASE-MANIFEST.txt      # or: sha256sum -c RELEASE-MANIFEST.txt"
    printf '%s\n' "gpg --verify RELEASE-MANIFEST.txt.asc RELEASE-MANIFEST.txt"
    printf '%s\n' '```'
    printf '\n'
    printf '%s\n' "## Signing (operator step — NOT done by the script)"
    printf '\n'
    printf '%s\n' "\`release-bundle.sh\` never generates, requests, reads or prints a private key, and"
    printf '%s\n' "never runs a signing command. A human with the testnet release key runs one of:"
    printf '\n'
    printf '%s\n' '```'
    printf '%s\n' "cd $OUTDIR"
    printf '%s\n' "gpg --detach-sign --armor RELEASE-MANIFEST.txt        # -> RELEASE-MANIFEST.txt.asc"
    printf '%s\n' "minisign -Sm RELEASE-MANIFEST.txt                     # -> RELEASE-MANIFEST.txt.minisig"
    printf '%s\n' '```'
    printf '\n'
    printf '%s\n' "Publish the signature next to the manifest, and distribute the public half"
    printf '%s\n' "out-of-band (as the harness already does for the network manifest's signers pin)."
} > "$README" || die "could not write $README"
chmod 0644 "$README" 2>/dev/null || true

# -----------------------------------------------------------------------------
# RELEASE-MANIFEST.txt — the signable root. Hash lines are in `shasum -a 256 -c`
# format (two spaces, basename) so a recipient can check the whole bundle with
# one command from inside the output dir. The manifest cannot hash itself; the
# signature over it is what covers the manifest.
# -----------------------------------------------------------------------------
log "writing $MANIFEST"
# Hash every artifact BEFORE opening the manifest, one assignment at a time —
# same fail-closed reason as BIN_LINES above.
MANIFEST_LINES=""
for _f in "$TARBALL" "$PROVENANCE" "$README" ${SBOM_DEST:+"$SBOM_DEST"}; do
    _h="$(sha256_file "$_f")"
    MANIFEST_LINES="$MANIFEST_LINES$_h  $(basename "$_f")"$'\n'
done
{
    printf '%s\n' "# PALW closed shared testnet — RELEASE MANIFEST (schema: palw-release-manifest-v1)"
    printf '%s\n' "# source_ref:        $REF"
    printf '%s\n' "# source_commit:     $SOURCE_COMMIT"
    printf '%s\n' "# source_signature:  $SIG_STATUS"
    printf '%s\n' "# cargo_lock_sha256: $LOCK_SHA"
    printf '%s\n' "# UNSIGNED until an operator detach-signs THIS file (see README-RELEASE.md)."
    printf '%s\n' "# Check with: shasum -a 256 -c RELEASE-MANIFEST.txt   (or sha256sum -c)"
    printf '%s' "$MANIFEST_LINES"
} > "$MANIFEST" || die "could not write $MANIFEST"
chmod 0644 "$MANIFEST" 2>/dev/null || true

# Sweep done, artifacts written: the bundle may survive this exit.
_BUNDLE_OK=1
rm -rf "$STAGE"

log "release bundle complete: $OUTDIR"
log "  payload:    $(basename "$TARBALL")  (git archive of $SOURCE_COMMIT -- $BUNDLE_PATHS)"
log "  provenance: $(basename "$PROVENANCE")"
log "  manifest:   $(basename "$MANIFEST")  [UNSIGNED]"

# The script stops here on purpose. It prints the signing command; it does not
# run it, and it never touches key material. Signing is the operator's step.
cat <<EOF

NEXT STEP — OPERATOR SIGNS THE MANIFEST (this script does not, and holds no key):

    cd $OUTDIR
    gpg --detach-sign --armor RELEASE-MANIFEST.txt     # -> RELEASE-MANIFEST.txt.asc
    # or, with minisign:
    minisign -Sm RELEASE-MANIFEST.txt                  # -> RELEASE-MANIFEST.txt.minisig

Then publish the manifest + signature alongside the payload. Until that signature
exists and verifies against a key the recipient already trusts, this bundle is
unattributed bytes. Recipients verify with:

    shasum -a 256 -c RELEASE-MANIFEST.txt
    gpg --verify RELEASE-MANIFEST.txt.asc RELEASE-MANIFEST.txt

EOF
exit 0
