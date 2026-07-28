#!/usr/bin/env bash
# =============================================================================
# collect-artifacts.sh — STN-013: bundle the closed two-node PALW testnet's
#                        evidence into one portable, REDACTED directory.
#
#   usage:  ./collect-artifacts.sh [LABEL]      # LABEL overrides BUNDLE_LABEL
#           ./collect-artifacts.sh --check-signatures [LABEL]   # read-only re-check
#           ./collect-artifacts.sh --help
#
# WHAT THIS PRODUCES (into artifacts/bundle-<LABEL>/):
#   binary-hashes.txt              copy of the STN-001 release-binary SHA-256s
#   network-manifest.json[.sig]    the §11.3 SIGNED release identity (+ its PUBLIC
#   [.signers]                     allowed-signers pin) — copied verbatim when
#                                  present; the signing KEY is never touched
#   SOURCE_COMMIT                  the source revision this evidence came from
#   negative-tests.json            the G7 failure/recovery report (negative-tests.sh)
#   verify-consensus.txt           both-node consensus parity report, when present
#   verify-coinbase.txt            algo-4 settlement / payout report, when present
#   node-a-status.txt              live `VAL status` dump from node A (over wRPC)
#   node-b-status.txt              live `VAL status` dump from node B
#   identity/node-{a,b}.txt        per-node consensus identity (genesis / params /
#                                  header version / algo4 flag / node git commit),
#                                  EXTRACTED from the live status dumps above
#   palw-status/                   `VAL palw-status` dumps, per identity, per node:
#       provider-A.node-{a,b}.txt  provider A provider-bond view
#       provider-B.node-{a,b}.txt  provider B provider-bond view
#       provider-C.node-{a,b}.txt  independent auditor C provider-bond view
#       batch.node-{a,b}.txt       batch view for PALW_BATCH_ID
#   outpoints-and-ids.txt          every captured tx/outpoint id + PALW_BATCH_ID
#                                  (+ any recorded algo-4 block hash / verdict /
#                                  observed coinbase settlement slots)
#   logs/<name>.log.tail.txt       the tail of EVERY log under logs/
#   env.redacted                   PUBLIC config only (network/ports/commitment
#                                  ids/funding addresses/outpoints). Secrets are
#                                  REDACTED and *.seed / key material is NEVER
#                                  copied or referenced by value.
#   REDACTIONS.txt                 exactly WHAT was withheld/redacted — and, just
#                                  as importantly, what was NOT redacted.
#   run-result.json                MACHINE-READABLE run summary (schema
#                                  "palw-run-result-v1"): network, both nodes'
#                                  sink + identity, batch id, algo-4 block hash and
#                                  per-node accept verdicts (or explicit nulls), the
#                                  negative-test summary, and evidence_complete.
#   SIGNATURES.txt                 which signable artifact had a detached .sig at
#                                  collection time + the EXACT command the human
#                                  operator runs to sign MANIFEST.txt.
#   MANIFEST.txt                   listing of every bundled file: sha256, size, path
#   MANIFEST.txt.sig               NOT produced here — the operator detach-signs
#                                  MANIFEST.txt after this script exits (see below).
#
# SCOPE:
#   * It bundles REAL evidence: the status/palw-status dumps are read LIVE from
#     the two validators over independent wRPC; the ids come from the discovered
#     artifacts/state.env; the log tails are the real daemons' logs; the hashes
#     are the real just-built binaries. It NEVER invokes the seeded, test-only
#     `palw_demo` path and it mints nothing — there is no demo evidence here.
#   * The two status dumps prove what the two configured RPC endpoints saw. On a
#     SINGLE host that is two processes agreeing, NOT a network-partition proof
#     (STN-003) — the manifest states this plainly rather than overclaiming.
#   * NO seed or secret material is bundled. artifacts/state.env itself is NOT
#     copied (it can hold seed *paths*); only a redacted, allow-listed env is
#     emitted. keys/*.seed and the ticket secret store are never read or copied.
#     Log tails may contain seed FILE PATHS that daemons logged in their argv
#     (e.g. --validator-key <path>) but never seed CONTENTS — common.sh
#     guarantees "NO SECRETS TO ARGV / LOG". REDACTIONS.txt states this in-bundle.
#   * SIGNING IS A HUMAN STEP (STN-05). This script NEVER generates, reads,
#     handles or prints a private key. It hashes the bundle into MANIFEST.txt and
#     prints the exact `ssh-keygen -Y sign` command for the operator to run on the
#     coordinator; MANIFEST.txt.sig therefore never exists at collection time and
#     is deliberately absent from MANIFEST.txt's own listing (a detached signature
#     cannot be inside the listing it signs). Re-check after signing with
#     `--check-signatures <LABEL>`, which mutates nothing.
#   * The signatures this stage DOES check, it checks for real: an already-signed
#     network-manifest.json is verified with `ssh-keygen -Y verify` against the
#     allowed-signers pin. When it cannot check (no .sig, no pin, no ssh-keygen)
#     it says "UNVERIFIED" — never "OK".
#   * evidence_complete in run-result.json is TRUE only when EVERY required item
#     was actually found. A TICKET_MODE=skip run cannot mint, so it legitimately
#     has no algo-4 block evidence and its evidence_complete is FALSE — that is
#     the expected interpretation. missing_required[] names what is absent.
#
# Design rules (shared with the whole harness):
#   * IDEMPOTENT   — this stage creates NO pids / keys / outpoints; it only READS
#                    them. The single thing it writes is the bundle directory,
#                    which it builds in a temp staging dir and moves into place
#                    atomically. It NEVER silently overwrites an existing bundle:
#                    an already-present artifacts/bundle-<LABEL>/ is fail-closed
#                    (pick a new label, or BUNDLE_FORCE=1 to replace — logged).
#   * FAIL-CLOSED  — any missing evidence (nodes down, unset outpoints/batch id,
#                    absent binary-hashes.txt, no logs) is a die() with an
#                    actionable message. BUNDLE_ALLOW_PARTIAL=1 downgrades those
#                    to recorded gaps (an explicitly labeled post-mortem bundle).
#   * TRAP-SAFE    — a register_cleanup trap removes the temp staging dir on any
#                    early EXIT/INT/TERM, so a failed run leaves no half-bundle.
#   * PORTABLE     — bash 3.2 (stock macOS) + Linux; BSD + GNU coreutils.
#
# Env knobs (all optional):
#   BUNDLE_LABEL=<s> / positional LABEL — bundle dir suffix (default "unlabeled";
#                    this script never synthesises one from date()). The label is
#                    validated as a safe single path component.
#   BUNDLE_FORCE=1 — replace an existing artifacts/bundle-<LABEL>/ (logged).
#   BUNDLE_ALLOW_PARTIAL=1 — still produce a bundle when some evidence is missing;
#                    each gap is recorded in-place instead of aborting. REFUSED
#                    under PALW_RELEASE_MODE=1 (a release bundle is never partial).
#   PALW_RELEASE_MODE=1 — release gate, fail-closed, no evidence mutation: gaps
#                    cannot be downgraded, a FAILED negative-test report is fatal,
#                    and a MISSING detached signature for MANIFEST.txt or
#                    network-manifest.json is FATAL (the bundle is still written —
#                    the operator needs it in order to sign it — but the stage
#                    exits non-zero with the signing command). Standardised knob;
#                    negative-tests.sh spells its own gate NEG_RELEASE=1.
#   BUNDLE_TAIL_LINES=<n> — lines per log tail (default 200).
#   BUNDLE_RPC_PROBE_SECS=<n> — per-node wRPC probe timeout, seconds (default 10).
#   PALW_MANIFEST_KEY / PALW_MANIFEST_SIGNERS — as network-manifest.sh. Used ONLY
#                    to print the operator's signing command and to locate the
#                    PUBLIC allowed-signers pin. The key itself is never read.
#   PALW_ENV_FILE / env.local / env.example — config source (as load_env).
#
# It SOURCES common.sh and uses ONLY its helpers — it reimplements none of them.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"
# shellcheck source=remote.sh
. "$SCRIPT_DIR/remote.sh"   # node_is_remote / node_dispatch / remote host bundle pull (§5.4 cond 4)

# Per-stage log tag (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-collect-artifacts}"; export PALW_LOG_TAG

usage() {
    cat >&2 <<EOF
usage: ${0##*/} [LABEL|--check-signatures [LABEL]|--help]

  Bundle the closed two-node PALW testnet's evidence into a portable, REDACTED
  directory  artifacts/bundle-<LABEL>/  (STN-013): the STN-001 binary hashes, the
  §11.3 signed network-manifest.json (+ .sig/.signers), SOURCE_COMMIT, the G7
  negative-tests.json, the consensus/coinbase reports, live node A/B status +
  identity dumps, provider (A/B/auditor-C) + batch palw-status dumps, every
  captured outpoint/tx id + PALW_BATCH_ID, the tail of every log, a PUBLIC-only
  redacted env, a machine-readable run-result.json, and a sha256 MANIFEST. It
  never copies *.seed and never bundles artifacts/state.env (seed paths).

  LABEL              Bundle directory suffix (overrides \$BUNDLE_LABEL; default
                     "unlabeled"). Must be a single safe path component.
  --check-signatures [LABEL]
                     READ-ONLY re-check of an already-published bundle: reports
                     (and where possible ssh-keygen -Y verifies) the detached
                     signatures over MANIFEST.txt and network-manifest.json.
                     Mutates nothing, so it is safe under PALW_RELEASE_MODE=1.
  --help             Show this help and exit.

  Idempotent: creates no pids/keys/outpoints (reads only). Refuses to overwrite
  an existing bundle-<LABEL>/ — choose a new label or set BUNDLE_FORCE=1.
  Fail-closed on missing evidence unless BUNDLE_ALLOW_PARTIAL=1 (then each gap
  is recorded explicitly in the bundle instead of aborting).

  SIGNING IS YOURS: this stage never handles a private key. It prints the exact
  \`ssh-keygen -Y sign\` command for MANIFEST.txt; run it on the coordinator, then
  re-check with  ${0##*/} --check-signatures <LABEL>.
EOF
}

# ---------------------------------------------------------------------------
# Dispatch / arg validation BEFORE load_env so --help works unconfigured.
# A single optional positional is the label; anything more is fail-closed.
# ---------------------------------------------------------------------------
BUNDLE_LABEL_ARG=""
CHECK_SIGS_ONLY=0
case "${1:-}" in
    -h|--help|help)     usage; exit 0 ;;
    --check-signatures) CHECK_SIGS_ONLY=1; BUNDLE_LABEL_ARG="${2:-}" ;;
    "")                 : ;;
    -*)                 usage; die "unknown option '$1' (this stage takes an optional LABEL, --check-signatures [LABEL], or --help)." ;;
    *)                  BUNDLE_LABEL_ARG="$1" ;;
esac
if [ "$CHECK_SIGS_ONLY" = "1" ]; then
    if [ "$#" -gt 2 ]; then usage; die "unexpected extra argument(s): ${*:3} (--check-signatures takes at most a single LABEL)."; fi
elif [ "$#" -gt 1 ]; then
    usage; die "unexpected extra argument(s): ${*:2} (expected at most a single LABEL)."
fi

# External tools this script invokes directly (helpers rely on more, all present).
require_cmd awk grep mktemp install date tail find sort cp wc chmod

# Pick the available SHA-256 tool (used for the manifest integrity column):
# sha256sum (GNU coreutils) or `shasum -a 256` (BSD / stock macOS). Fail fast.
if command -v sha256sum >/dev/null 2>&1; then
    SHA256_TOOL="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SHA256_TOOL="shasum -a 256"
else
    die "need 'sha256sum' or 'shasum' on PATH to hash bundle files (install coreutils, or use macOS's shasum)."
fi

# load_env: sources config, realpaths REPO_ROOT/PALW_DATA_ROOT, creates the 0700
# data dirs, overlays state.env, validates required vars, binds+verifies the
# three binaries. Fail-closed and re-runnable. We need PALW_DATA_ROOT (where the
# logs / artifacts / state live) and the node addressing/status helpers.
load_env

# ---------------------------------------------------------------------------
# Resolve + validate the bundle label (it becomes a directory name).
# ---------------------------------------------------------------------------
BUNDLE_LABEL="${BUNDLE_LABEL_ARG:-${BUNDLE_LABEL:-unlabeled}}"
case "$BUNDLE_LABEL" in
    ''|*[!A-Za-z0-9._-]*) die "invalid bundle label '$BUNDLE_LABEL' — use only letters, digits, '.', '-', '_' (it becomes a single directory name)." ;;
esac
case "$BUNDLE_LABEL" in
    .|..|-*) die "bundle label must not be '.', '..', or start with '-' (got '$BUNDLE_LABEL')." ;;
esac

ARTIFACTS_DIR="$PALW_DATA_ROOT/artifacts"
LOGS_DIR="$PALW_DATA_ROOT/logs"
HASHES_SRC="$ARTIFACTS_DIR/binary-hashes.txt"
BUNDLE_DIR="$ARTIFACTS_DIR/bundle-$BUNDLE_LABEL"

TAIL_LINES="${BUNDLE_TAIL_LINES:-200}"
case "$TAIL_LINES" in ''|*[!0-9]*) die "BUNDLE_TAIL_LINES must be a non-negative integer, got '$TAIL_LINES'." ;; esac
RPC_PROBE_SECS="${BUNDLE_RPC_PROBE_SECS:-10}"
case "$RPC_PROBE_SECS" in ''|*[!0-9]*) die "BUNDLE_RPC_PROBE_SECS must be a non-negative integer, got '$RPC_PROBE_SECS'." ;; esac

RELEASE_MODE="${PALW_RELEASE_MODE:-0}"
case "$RELEASE_MODE" in 0|1) : ;; *) die "PALW_RELEASE_MODE must be 0 or 1, got '$RELEASE_MODE'." ;; esac
# A release bundle is never a partial one: the two knobs contradict each other, so
# refuse the combination up front rather than letting release mode silently inherit
# a downgraded gap() (that would be exactly the "PASS overstates it" failure mode).
if [ "$RELEASE_MODE" = "1" ] && [ "${BUNDLE_ALLOW_PARTIAL:-}" = "1" ]; then
    die "PALW_RELEASE_MODE=1 and BUNDLE_ALLOW_PARTIAL=1 are mutually exclusive — a release bundle cannot be a partial post-mortem bundle. Drop BUNDLE_ALLOW_PARTIAL=1, fix the missing evidence, and re-run."
fi

# ---------------------------------------------------------------------------
# Signing (STN-05). SIGNING IS THE HUMAN OPERATOR'S STEP — this script never
# generates, reads, handles or prints a private key. It only:
#   * prints the exact `ssh-keygen -Y sign` command to run, and
#   * records / verifies the PRESENCE of detached .sig files.
# Namespaces are separated on purpose so a signature over one document can never
# be replayed as a signature over the other:
#   palw-manifest  — network-manifest.json  (MUST match network-manifest.sh)
#   palw-evidence  — this bundle's MANIFEST.txt (new document, new namespace)
# The signer identity is `palw-release`, the identity network-manifest.sh writes
# into the allowed-signers pin.
# ---------------------------------------------------------------------------
NS_MANIFEST="palw-manifest"
NS_EVIDENCE="palw-evidence"
SIGN_IDENTITY="palw-release"
# Only the PATH of the release key is ever referenced (same default as
# network-manifest.sh: a DEDICATED key under keys/, never the operator's personal
# identity). The file is NEVER opened by this script.
SIGNKEY_PATH="${PALW_MANIFEST_KEY:-$PALW_DATA_ROOT/keys/manifest-signing.key}"
NETMAN_SRC="$ARTIFACTS_DIR/network-manifest.json"
NETMAN_SIGNERS_SRC="${PALW_MANIFEST_SIGNERS:-$NETMAN_SRC.signers}"

# _sign_cmd <file> <namespace>  — the exact command the OPERATOR runs. Printed to
#   the operator's terminal only (it names a host-local key PATH, never a key).
_sign_cmd() {
    printf 'ssh-keygen -Y sign -f %s -n %s %s' "$SIGNKEY_PATH" "$2" "$1"
}
# _verify_cmd <file> <namespace> <signers>  — the exact verification command.
_verify_cmd() {
    printf 'ssh-keygen -Y verify -f %s -I %s -n %s -s %s.sig < %s' "$3" "$SIGN_IDENTITY" "$2" "$1" "$1"
}
# _verify_sig <file> <namespace> <signers>  — really verify <file>.sig.
#   0 = signature VERIFIED, 1 = signature BAD, 2 = CANNOT CHECK (no .sig, no pin,
#   or no ssh-keygen). "Cannot check" is deliberately distinct from "verified":
#   an unverifiable signature is never reported as a pass.
_verify_sig() {
    local f="$1" ns="$2" signers="$3"
    [ -s "$f" ] || return 2
    [ -s "$f.sig" ] || return 2
    [ -s "$signers" ] || return 2
    command -v ssh-keygen >/dev/null 2>&1 || return 2
    ssh-keygen -Y verify -f "$signers" -I "$SIGN_IDENTITY" -n "$ns" -s "$f.sig" < "$f" >/dev/null 2>&1 || return 1
    return 0
}

# ---------------------------------------------------------------------------
# --check-signatures: READ-ONLY re-check of an ALREADY-PUBLISHED bundle.
#
# It exists because MANIFEST.txt is produced BY a collect run, so its detached
# signature can only appear AFTERWARDS — and simply re-running collect would
# rewrite MANIFEST.txt and invalidate the signature the operator just made. This
# path therefore creates nothing, moves nothing and rewrites nothing (which is
# what PALW_RELEASE_MODE=1's "no evidence mutation" requires); it reports and,
# where the allowed-signers pin is available, actually verifies.
# ---------------------------------------------------------------------------
if [ "$CHECK_SIGS_ONLY" = "1" ]; then
    [ -d "$BUNDLE_DIR" ]             || die "no bundle at $BUNDLE_DIR — pass the LABEL of a bundle this stage already produced."
    [ -s "$BUNDLE_DIR/MANIFEST.txt" ] || die "$BUNDLE_DIR has no MANIFEST.txt — that is not a bundle produced by this stage."
    _cs_pin="$BUNDLE_DIR/network-manifest.json.signers"
    [ -s "$_cs_pin" ] || _cs_pin="$NETMAN_SIGNERS_SRC"
    _cs_missing=""
    for _cs_pair in "MANIFEST.txt:$NS_EVIDENCE" "network-manifest.json:$NS_MANIFEST"; do
        _cs_name="${_cs_pair%%:*}"; _cs_ns="${_cs_pair##*:}"
        _cs_file="$BUNDLE_DIR/$_cs_name"
        if [ ! -s "$_cs_file" ]; then
            warn "$_cs_name is NOT IN THIS BUNDLE — nothing to check for it."
            _cs_missing="$_cs_missing $_cs_name(absent)"
            continue
        fi
        if [ ! -s "$_cs_file.sig" ]; then
            warn "signature MISSING: $_cs_name.sig"
            _cs_missing="$_cs_missing $_cs_name"
            continue
        fi
        _cs_rc=0; _verify_sig "$_cs_file" "$_cs_ns" "$_cs_pin" || _cs_rc=$?
        case "$_cs_rc" in
            0) log "signature VERIFIED: $_cs_name.sig (namespace $_cs_ns, signer $SIGN_IDENTITY, pin $_cs_pin)" ;;
            1) _cs_missing="$_cs_missing $_cs_name(BAD)"
               warn "signature BAD: $_cs_name.sig does NOT verify against $_cs_pin — do not trust this bundle." ;;
            2) _cs_missing="$_cs_missing $_cs_name(UNVERIFIED)"
               warn "signature PRESENT but UNVERIFIED for $_cs_name: no allowed-signers pin at $_cs_pin (or ssh-keygen absent). Presence is NOT verification — obtain the pin out-of-band and run: $(_verify_cmd "$_cs_file" "$_cs_ns" "$_cs_pin")" ;;
        esac
    done
    if [ -n "$_cs_missing" ]; then
        die "signature check NOT clean:$_cs_missing
This bundle is NOT release-complete. Sign it on the coordinator — this harness
never handles a private key:
    $(_sign_cmd "$BUNDLE_DIR/MANIFEST.txt" "$NS_EVIDENCE")
and, if network-manifest.json is unsigned, re-sign it WHERE IT LIVES (the bundle
copy is evidence; the live file is the release identity):
    ./network-manifest.sh generate
Then re-run: ${0##*/} --check-signatures $BUNDLE_LABEL"
    fi
    log "signature check PASS: every signable artifact in $BUNDLE_DIR carries a detached signature that VERIFIES against $_cs_pin. (This checks signatures only — it does not re-read the network or re-hash the bundle.)"
    exit 0
fi

# ---------------------------------------------------------------------------
# Staging dir + cleanup trap. We build the whole bundle under a temp dir and
# only mv it into place at the very end, so a crash never leaves a half-bundle
# and never clobbers a prior-good one. $STAGE is expanded at trap time (see
# _run_cleanup's eval); it is blanked the instant the bundle is committed, and
# the guard makes an empty $STAGE a no-op so the committed dir is never removed.
# ---------------------------------------------------------------------------
install -d -m 0700 "$ARTIFACTS_DIR" || die "cannot create artifacts dir: $ARTIFACTS_DIR"
STAGE="$(mktemp -d "${BUNDLE_DIR}.partial.XXXXXX")" || die "mktemp -d failed near $BUNDLE_DIR"
register_cleanup '[ -n "${STAGE:-}" ] && rm -rf "$STAGE"'
install -d -m 0755 "$STAGE/palw-status" "$STAGE/logs" "$STAGE/identity" \
    || die "cannot create staging subdirs under $STAGE"

# ---------------------------------------------------------------------------
# gap <message> — record missing evidence. Fatal (fail-closed) by default; with
#   BUNDLE_ALLOW_PARTIAL=1 it warns, bumps the gap counter, and returns 0 so the
#   caller can write an honest placeholder and continue.
# ---------------------------------------------------------------------------
GAP_COUNT=0
gap() {
    GAP_COUNT=$(( GAP_COUNT + 1 ))
    if [ "${BUNDLE_ALLOW_PARTIAL:-}" = "1" ]; then
        warn "partial bundle: $*"
        return 0
    fi
    die "missing evidence: $*
Fix it and re-run, or set BUNDLE_ALLOW_PARTIAL=1 to bundle only what IS available
(an explicitly labeled post-mortem bundle)."
}

# ---------------------------------------------------------------------------
# Required-item ledger (STN-05). run-result.json's `evidence_complete` is TRUE
# only when EVERY item recorded here was ACTUALLY FOUND — so each item is booked
# exactly once, at the point where its presence is decided, and never inferred.
# Two space-separated token lists (bash 3.2: no associative arrays).
# ---------------------------------------------------------------------------
REQ_FOUND=""
REQ_MISSING=""
req_record() {   # <token-name> <1 found | 0 missing>
    if [ "$2" = "1" ]; then REQ_FOUND="$REQ_FOUND $1"; else REQ_MISSING="$REQ_MISSING $1"; fi
}

# Names of env keys whose VALUE was redacted in env.redacted (reported in-bundle
# by REDACTIONS.txt — a redaction nobody is told about is not a disclosure).
REDACTED_KEYS=""

# run_capture <outfile> <rpc_ok 0|1> <desc> <cmd> [args...]
#   Capture a live status command's stdout+stderr into <outfile>. If the node's
#   RPC is not up (<rpc_ok> != 1) it writes an UNAVAILABLE marker instead of a
#   fabricated dump. A non-zero command still records its output plus an error
#   marker (never a false-clean dump).
run_capture() {
    local out="$1" ok="$2" desc="$3"; shift 3
    if [ "$ok" != "1" ]; then
        printf '<UNAVAILABLE: %s — node wRPC not answering at collection time>\n' "$desc" > "$out"
        warn "recorded UNAVAILABLE: $desc (node RPC down)"
        return 0
    fi
    if "$@" > "$out" 2>&1; then
        [ -s "$out" ] || printf '<empty response: %s>\n' "$desc" > "$out"
        log "captured: $desc -> ${out#$STAGE/}"
    else
        printf '\n<ERROR: %s — command exited non-zero; any partial output is above>\n' "$desc" >> "$out"
        warn "capture returned non-zero: $desc (recorded with an error marker)"
    fi
}

# _looks_secret <name> <value> — defence-in-depth guard for the redacted env.
#   The env allow-list below already excludes secrets by construction; this also
#   redacts any value that turns out to reference key material.
_looks_secret() {
    local name="$1" val="$2"
    case "$name" in
        *SEED*|*SECRET*|*PRIVATE*|*_KEY) return 0 ;;
    esac
    case "$val" in
        *.seed|*/keys/*) return 0 ;;
    esac
    if [ -n "$val" ] && [ -e "$val" ]; then
        case "$(realpath_p "$val")" in
            "$PALW_DATA_ROOT"/keys/*) return 0 ;;
        esac
    fi
    return 1
}

log "collecting evidence into staging dir for bundle-$BUNDLE_LABEL (network=$NETWORK, data=$PALW_DATA_ROOT)"

# ===========================================================================
# [1] binary-hashes.txt — copy the STN-001 release-binary attestation.
# ===========================================================================
if [ -s "$HASHES_SRC" ]; then
    cp "$HASHES_SRC" "$STAGE/binary-hashes.txt" || die "failed to copy $HASHES_SRC into the bundle."
    req_record binary-hashes.txt 1
    log "bundled binary-hashes.txt (STN-001)"
else
    gap "binary-hashes.txt not found at $HASHES_SRC — run ./build-and-hash.sh first (STN-001)."
    req_record binary-hashes.txt 0
    printf '<MISSING: %s — run ./build-and-hash.sh (STN-001) before collecting>\n' "$HASHES_SRC" \
        > "$STAGE/binary-hashes.txt"
fi

# ===========================================================================
# [1b] network-manifest.json (+ detached .sig, + the PUBLIC allowed-signers pin).
#      This is the §11.3 release identity a third party needs in order to know
#      WHICH net the rest of this bundle is evidence for. Copied verbatim; the
#      signing KEY is never read or copied (only the public half travels, exactly
#      as network-manifest.sh intends). An already-present signature is really
#      verified here — presence alone is never reported as a pass.
# ===========================================================================
NETMAN_SIG_STATE="absent"          # absent | present-unverified | verified | BAD
if [ -s "$NETMAN_SRC" ]; then
    cp "$NETMAN_SRC" "$STAGE/network-manifest.json" || die "failed to copy $NETMAN_SRC into the bundle."
    req_record network-manifest.json 1
    if [ -s "$NETMAN_SRC.sig" ]; then
        cp "$NETMAN_SRC.sig" "$STAGE/network-manifest.json.sig" || die "failed to copy $NETMAN_SRC.sig into the bundle."
        req_record network-manifest.json.sig 1
        NETMAN_SIG_STATE="present-unverified"
    else
        req_record network-manifest.json.sig 0
        gap "network-manifest.json has NO detached signature at $NETMAN_SRC.sig — an unsigned manifest is not a release identity (network-manifest.sh verify fail-closes on it). Sign it on the coordinator: $(_sign_cmd "$NETMAN_SRC" "$NS_MANIFEST")"
        printf '<MISSING: no detached signature accompanied %s at collection time. An UNSIGNED network manifest is NOT a release identity.>\n' "network-manifest.json" \
            > "$STAGE/network-manifest.json.sig.MISSING"
    fi
    # The allowed-signers pin is the PUBLIC half only — it is precisely what a
    # third-party verifier needs, and it carries no private key material.
    if [ -s "$NETMAN_SIGNERS_SRC" ]; then
        cp "$NETMAN_SIGNERS_SRC" "$STAGE/network-manifest.json.signers" || die "failed to copy the allowed-signers pin into the bundle."
        log "bundled network-manifest.json.signers (PUBLIC allowed-signers pin — no private key material)"
    else
        warn "no allowed-signers pin at $NETMAN_SIGNERS_SRC — without it a recipient cannot check WHO signed this net's manifest. Distribute it out-of-band (PALW_MANIFEST_SIGNERS)."
    fi
    # Real verification (read-only), not a presence check.
    if [ "$NETMAN_SIG_STATE" != "absent" ]; then
        _nm_rc=0; _verify_sig "$STAGE/network-manifest.json" "$NS_MANIFEST" "$STAGE/network-manifest.json.signers" || _nm_rc=$?
        case "$_nm_rc" in
            0) NETMAN_SIG_STATE="verified"
               log "network-manifest.json signature VERIFIED against the bundled allowed-signers pin (namespace $NS_MANIFEST, signer $SIGN_IDENTITY)" ;;
            1) NETMAN_SIG_STATE="BAD"
               gap "network-manifest.json's detached signature does NOT verify against the allowed-signers pin — the release identity in this bundle is untrustworthy. Investigate before publishing anything from this net." ;;
            2) NETMAN_SIG_STATE="present-unverified"
               warn "network-manifest.json has a signature but it could NOT be checked here (no allowed-signers pin in the bundle, or ssh-keygen unavailable). Recorded as PRESENT-BUT-UNVERIFIED — presence is not verification." ;;
        esac
    fi
else
    gap "network-manifest.json not found at $NETMAN_SRC — generate the §11.3 signed release identity first: ./network-manifest.sh generate"
    req_record network-manifest.json 0
    req_record network-manifest.json.sig 0
    printf '<MISSING: %s — run ./network-manifest.sh generate. Without it this bundle cannot tell a third party which genesis / consensus params / header version / algo4 flag it is evidence for.>\n' "$NETMAN_SRC" \
        > "$STAGE/network-manifest.json.MISSING"
fi

# ===========================================================================
# [1c] negative-tests.json — the G7 failure/recovery report (negative-tests.sh).
#      Copied verbatim; this stage never re-runs, re-scores or re-interprets it.
# ===========================================================================
NEG_SRC="$ARTIFACTS_DIR/negative-tests.json"
if [ -s "$NEG_SRC" ]; then
    cp "$NEG_SRC" "$STAGE/negative-tests.json" || die "failed to copy $NEG_SRC into the bundle."
    req_record negative-tests.json 1
    log "bundled negative-tests.json (G7 failure/recovery report)"
else
    gap "negative-tests.json not found at $NEG_SRC — run ./negative-tests.sh all (release gate: NEG_RELEASE=1) so the bundle carries a machine-readable failure/recovery result."
    req_record negative-tests.json 0
    printf '<MISSING: %s — run ./negative-tests.sh all (NEG_RELEASE=1 for the release gate)>\n' "$NEG_SRC" \
        > "$STAGE/negative-tests.json.MISSING"
fi

# ===========================================================================
# [1d] The two verification reports. verify-consensus.txt is the both-node parity
#      evidence; verify-coinbase.txt is the algo-4 settlement/payout evidence
#      (including the find-reward-settlement output when the mint was merged).
#      Both are produced by their own stages — copied verbatim, never re-derived.
# ===========================================================================
for _vr in verify-consensus.txt verify-coinbase.txt; do
    if [ -s "$ARTIFACTS_DIR/$_vr" ]; then
        cp "$ARTIFACTS_DIR/$_vr" "$STAGE/$_vr" || die "failed to copy $ARTIFACTS_DIR/$_vr into the bundle."
        req_record "$_vr" 1
        log "bundled $_vr"
    else
        gap "$_vr not found at $ARTIFACTS_DIR/$_vr — run ./${_vr%.txt}.sh so the bundle carries its verdict."
        req_record "$_vr" 0
        printf '<MISSING: %s — run ./%s.sh before collecting>\n' "$ARTIFACTS_DIR/$_vr" "${_vr%.txt}" \
            > "$STAGE/$_vr.MISSING"
    fi
done

# ===========================================================================
# [2] Probe each node's wRPC once (short timeout). Live status/palw-status dumps
#     need the nodes up. Per-node flags drive UNAVAILABLE markers under partial.
# ===========================================================================
RPC_OK_A=0; RPC_OK_B=0
if wait_rpc_up a "$RPC_PROBE_SECS"; then RPC_OK_A=1; fi
if wait_rpc_up b "$RPC_PROBE_SECS"; then RPC_OK_B=1; fi
node_rpc_ok() { case "$(_node_label "$1")" in a) printf '%s' "$RPC_OK_A" ;; b) printf '%s' "$RPC_OK_B" ;; esac; }
req_record node-a-status "$RPC_OK_A"
req_record node-b-status "$RPC_OK_B"

if [ "$RPC_OK_A" != "1" ] || [ "$RPC_OK_B" != "1" ]; then
    gap "one or both node wRPC endpoints are not answering (A up=$RPC_OK_A [$(node_wrpc a)], B up=$RPC_OK_B [$(node_wrpc b)]) — live status/palw-status dumps require both nodes running. Start ./node-a.sh and ./node-b.sh."
fi

# ===========================================================================
# [3] Discovered ids from artifacts/state.env (public on-chain values). The
#     provider bonds and the batch id are required for a COMPLETE bundle.
# ===========================================================================
DNS_BOND="$(state_get DNS_BOND)"          # validator stake-bond outpoint (optional)
PROV_A_BOND="$(state_get PROV_A_BOND)"    # provider A provider-bond outpoint
PROV_B_BOND="$(state_get PROV_B_BOND)"    # provider B provider-bond outpoint
AUD_C_BOND="$(state_get AUD_C_BOND)"      # independent auditor C provider-bond outpoint
PALW_BATCH_ID="$(state_get PALW_BATCH_ID)" # batch id (128hex)

[ -n "$PROV_A_BOND" ]   || gap "PROV_A_BOND not recorded in $(state_file) — run ./register-providers.sh first."
[ -n "$PROV_B_BOND" ]   || gap "PROV_B_BOND not recorded in $(state_file) — run ./register-providers.sh first."
[ -n "$AUD_C_BOND" ]    || gap "AUD_C_BOND not recorded in $(state_file) — run ./register-providers.sh first."
[ -n "$PALW_BATCH_ID" ] || gap "PALW_BATCH_ID not recorded in $(state_file) — run the batch-manifest/lifecycle stage first."

if [ -n "$PROV_A_BOND" ] && [ -n "$PROV_B_BOND" ] && [ -n "$AUD_C_BOND" ]; then
    req_record provider-bonds 1
else
    req_record provider-bonds 0
fi
if [ -n "$PALW_BATCH_ID" ]; then req_record batch-id 1; else req_record batch-id 0; fi

# ---- accepted algo-4 block + observed coinbase settlement (state.env slots) ----
# The binaries expose no "give me the last algo-4 block" query and an algo-4 block
# has fork-choice weight 0 (PALW-014), so the MINT stage is the component that
# knows these; verify-consensus.sh / verify-coinbase.sh read the very same slots.
# This stage READS them and never invents one: an unset slot stays unset.
A4_HASH_A="$(state_get PALW_ALGO4_BLOCK_HASH_A || true)"
A4_HASH_B="$(state_get PALW_ALGO4_BLOCK_HASH_B || true)"
A4_ACCEPT_A="$(state_get PALW_ALGO4_ACCEPT_A || true)"
A4_ACCEPT_B="$(state_get PALW_ALGO4_ACCEPT_B || true)"
A4_SUBSIDY="$(state_get PALW_ALGO4_SUBSIDY_SOMPI || true)"
A4_PI_BPS="$(state_get PALW_ALGO4_PREMIUM_PI_BPS || true)"
A4_SRC_CLASS="$(state_get PALW_ALGO4_SOURCE_CLASS || true)"
A4_CB_A="$(state_get PALW_ALGO4_CB_PROVIDER_A_SOMPI || true)"
A4_CB_B="$(state_get PALW_ALGO4_CB_PROVIDER_B_SOMPI || true)"
A4_CB_INCL="$(state_get PALW_ALGO4_CB_INCLUSION_SOMPI || true)"
A4_CB_VAL="$(state_get PALW_ALGO4_CB_VALIDATOR_SOMPI || true)"
A4_CB_A_SPK="$(state_get PALW_ALGO4_CB_PROVIDER_A_SPK || true)"
A4_CB_B_SPK="$(state_get PALW_ALGO4_CB_PROVIDER_B_SPK || true)"
A4_CB_VERDICT="$(state_get PALW_COINBASE_VERDICT || true)"

# "Accepted algo-4 block evidence" means: the SAME full 128-hex hash recorded on
# BOTH nodes. One-sided or short hashes are NOT accepted-block evidence.
if [ -n "$A4_HASH_A" ] && [ "$A4_HASH_A" = "$A4_HASH_B" ] && [ "${#A4_HASH_A}" -eq 128 ]; then
    req_record algo4-block-hash 1
else
    req_record algo4-block-hash 0
    if [ -n "$A4_HASH_A" ] || [ -n "$A4_HASH_B" ]; then
        warn "algo-4 block hash is not both-node identical 128-hex (A='${A4_HASH_A:-<unset>}' B='${A4_HASH_B:-<unset>}') — bundling as-is; this is NOT accepted-block evidence."
    fi
fi
if [ -n "$A4_ACCEPT_A" ] && [ -n "$A4_ACCEPT_B" ]; then
    req_record algo4-accept-verdicts 1
else
    req_record algo4-accept-verdicts 0
fi

# Non-fatal shape sanity for the batch id (record it as-is either way; it is
# evidence). A malformed id means an upstream stage is broken.
if [ -n "$PALW_BATCH_ID" ]; then
    case "$PALW_BATCH_ID" in *[!0-9a-fA-F]*) warn "PALW_BATCH_ID is not hex: '$PALW_BATCH_ID' (bundling as-is)." ;; esac
    [ "${#PALW_BATCH_ID}" -eq 128 ] || warn "PALW_BATCH_ID length ${#PALW_BATCH_ID} != 128 (bundling as-is)."
    [ "$PALW_BATCH_ID" != "$(zero128)" ] || warn "PALW_BATCH_ID is the all-zero unbound sentinel (bundling as-is)."
fi

# ---- outpoints-and-ids.txt: every captured id, public values only ----------
IDS_OUT="$STAGE/outpoints-and-ids.txt"
{
    printf '# PALW closed-testnet — captured tx/outpoint ids + batch id (STN-013)\n'
    printf '# generated: %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')"
    printf '# network:   %s (base=%s suffix=%s)\n' "$NETWORK" "$NETWORK_BASE" "$NETSUFFIX"
    printf '# These are PUBLIC on-chain identifiers. No secret material appears here.\n'
    printf '\n'
    printf 'DNS_BOND               (validator stake-bond outpoint): %s\n' "${DNS_BOND:-<unset>}"
    printf 'PROV_A_BOND            (provider A provider-bond)      : %s\n' "${PROV_A_BOND:-<unset>}"
    printf 'PROV_B_BOND            (provider B provider-bond)      : %s\n' "${PROV_B_BOND:-<unset>}"
    printf 'AUD_C_BOND             (auditor  C provider-bond)      : %s\n' "${AUD_C_BOND:-<unset>}"
    printf 'PALW_BATCH_ID          (batch id, 128hex)             : %s\n' "${PALW_BATCH_ID:-<unset>}"
    printf '\n'
    printf '# Minted algo-4 block evidence (present only when TICKET_MODE=mock minted one;\n'
    printf '# algo-4 blocks carry fork-choice weight 0 (PALW-014) so they never become the sink):\n'
    printf 'PALW_ALGO4_BLOCK_HASH_A: %s\n' "${A4_HASH_A:-<unset>}"
    printf 'PALW_ALGO4_BLOCK_HASH_B: %s\n' "${A4_HASH_B:-<unset>}"
    printf 'PALW_ALGO4_ACCEPT_A    : %s\n' "${A4_ACCEPT_A:-<unset>}"
    printf 'PALW_ALGO4_ACCEPT_B    : %s\n' "${A4_ACCEPT_B:-<unset>}"
    printf '\n'
    printf '# Observed coinbase settlement / payout slots, as recorded by the mint stage and\n'
    printf '# asserted by verify-coinbase.sh. An algo-4 block pays its providers only on a\n'
    printf '# LATER block that blue-merges it, so these stay <unset> until that merge — an\n'
    printf '# unset payout is a DEFERRED payout, never a verified one (see verify-coinbase.txt):\n'
    printf 'PALW_ALGO4_SUBSIDY_SOMPI        (S)                    : %s\n' "${A4_SUBSIDY:-<unset>}"
    printf 'PALW_ALGO4_PREMIUM_PI_BPS       (pi, 10000 = neutral)  : %s\n' "${A4_PI_BPS:-<unset>}"
    printf 'PALW_ALGO4_SOURCE_CLASS                                : %s\n' "${A4_SRC_CLASS:-<unset>}"
    printf 'PALW_ALGO4_CB_PROVIDER_A_SOMPI  (observed provider A)  : %s\n' "${A4_CB_A:-<unset>}"
    printf 'PALW_ALGO4_CB_PROVIDER_B_SOMPI  (observed provider B)  : %s\n' "${A4_CB_B:-<unset>}"
    printf 'PALW_ALGO4_CB_INCLUSION_SOMPI   (observed sec-D pool)  : %s\n' "${A4_CB_INCL:-<unset>}"
    printf 'PALW_ALGO4_CB_VALIDATOR_SOMPI   (observed sec-E pool)  : %s\n' "${A4_CB_VAL:-<unset>}"
    printf 'PALW_ALGO4_CB_PROVIDER_A_SPK    (observed A output SPK): %s\n' "${A4_CB_A_SPK:-<unset>}"
    printf 'PALW_ALGO4_CB_PROVIDER_B_SPK    (observed B output SPK): %s\n' "${A4_CB_B_SPK:-<unset>}"
    printf 'PALW_COINBASE_VERDICT           (verify-coinbase.sh)   : %s\n' "${A4_CB_VERDICT:-<unset>}"
} > "$IDS_OUT"
log "bundled outpoints-and-ids.txt"

# ===========================================================================
# [4] Node A / B status dumps (live, over independent wRPC). If DNS_BOND is
#     recorded, pass it so the dump also carries stake_depth / bond_status.
# ===========================================================================
if [ -n "$DNS_BOND" ]; then
    run_capture "$STAGE/node-a-status.txt" "$RPC_OK_A" "node A status (stake-bond $DNS_BOND)" node_status a "$DNS_BOND"
    run_capture "$STAGE/node-b-status.txt" "$RPC_OK_B" "node B status (stake-bond $DNS_BOND)" node_status b "$DNS_BOND"
else
    run_capture "$STAGE/node-a-status.txt" "$RPC_OK_A" "node A status" node_status a
    run_capture "$STAGE/node-b-status.txt" "$RPC_OK_B" "node B status" node_status b
fi

# ===========================================================================
# [4a] Per-node consensus IDENTITY dumps. The binaries serve the identity as
#      fields of `VAL status` (node_genesis_hash / node_params_hash /
#      node_header_version_effective / node_palw_algo4_accept / node_git_commit —
#      exactly the fields network-manifest.sh pins), so this EXTRACTS them from
#      the live dumps captured in [4] instead of inventing a second RPC. A field
#      the node did not report is written as <unavailable>, never as a value.
# ===========================================================================
ID_FIELDS="node_network node_genesis_hash node_params_hash node_header_version_effective node_palw_algo4_accept node_git_commit"
dump_identity() {   # <a|b>
    local n="$1" src out f v gen="" par=""
    src="$STAGE/node-$n-status.txt"
    out="$STAGE/identity/node-$n.txt"
    {
        printf '# node-%s consensus identity — EXTRACTED from the LIVE status dump\n' "$n"
        printf '# captured in this same run (node-%s-status.txt). Nothing here is\n' "$n"
        printf '# re-derived client-side; these are the fields the node itself served.\n'
        printf '# <unavailable> = the node did not report that field (RPC down at\n'
        printf '# collection time, or a binary predating getConsensusIdentity).\n'
    } > "$out"
    for f in $ID_FIELDS; do
        v=""
        if [ -s "$src" ]; then v="$(_kv "$f" < "$src" || true)"; fi
        printf '%s: %s\n' "$f" "${v:-<unavailable>}" >> "$out"
    done
    if [ -s "$src" ]; then
        gen="$(_kv node_genesis_hash < "$src" || true)"
        par="$(_kv node_params_hash  < "$src" || true)"
    fi
    # An identity is only USABLE as a chain pin when both pinning fields are there.
    if [ -n "$gen" ] && [ -n "$par" ]; then
        req_record "node-$n-identity" 1
        log "bundled identity/node-$n.txt (genesis + params reported)"
    else
        req_record "node-$n-identity" 0
        warn "node-$n identity is INCOMPLETE (no node_genesis_hash/node_params_hash in its status dump) — a third party cannot pin which chain this evidence is from using node-$n."
    fi
}
dump_identity a
dump_identity b

# ===========================================================================
# [4b] SOURCE_COMMIT — the source revision this evidence was produced from.
#      Preferred source: an operator/CI-written artifacts/SOURCE_COMMIT, copied
#      verbatim. Otherwise DERIVED from two independent, separately-labelled
#      readings: what the RUNNING binaries report (node_git_commit, from [4a] —
#      the authoritative one, since it describes the code that produced the
#      chain) and the collecting host's git worktree (which may differ from, or
#      be dirtier than, what the nodes actually run). Never fabricated: with
#      neither reading available the file says so and the item counts as missing.
# ===========================================================================
SRC_COMMIT_OUT="$STAGE/SOURCE_COMMIT"
SRC_COMMIT_NODE_A="$(_kv node_git_commit < "$STAGE/identity/node-a.txt" || true)"
SRC_COMMIT_NODE_B="$(_kv node_git_commit < "$STAGE/identity/node-b.txt" || true)"
case "$SRC_COMMIT_NODE_A" in '<unavailable>') SRC_COMMIT_NODE_A="" ;; esac
case "$SRC_COMMIT_NODE_B" in '<unavailable>') SRC_COMMIT_NODE_B="" ;; esac
SRC_COMMIT_GIT=""; SRC_COMMIT_DIRTY="unknown"
if command -v git >/dev/null 2>&1 && [ -e "$REPO_ROOT/.git" ]; then   # -e: .git is a FILE in a linked worktree
    SRC_COMMIT_GIT="$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || true)"
    if [ -n "$SRC_COMMIT_GIT" ]; then
        if [ -n "$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null || true)" ]; then
            SRC_COMMIT_DIRTY="yes"
        else
            SRC_COMMIT_DIRTY="no"
        fi
    fi
fi

if [ -s "$ARTIFACTS_DIR/SOURCE_COMMIT" ]; then
    cp "$ARTIFACTS_DIR/SOURCE_COMMIT" "$SRC_COMMIT_OUT" || die "failed to copy $ARTIFACTS_DIR/SOURCE_COMMIT into the bundle."
    req_record SOURCE_COMMIT 1
    log "bundled SOURCE_COMMIT (copied verbatim from artifacts/SOURCE_COMMIT)"
else
    {
        printf '# PALW closed-testnet — SOURCE_COMMIT (STN-05)\n'
        printf '# generated: %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')"
        printf '# DERIVED at collection time (no artifacts/SOURCE_COMMIT was present).\n'
        printf '# Each line states HOW it was obtained. <unavailable> is never a guess.\n'
        printf '\n'
        printf '# What the RUNNING node binaries report (authoritative for the chain):\n'
        printf 'node_a_git_commit:   %s\n' "${SRC_COMMIT_NODE_A:-<unavailable>}"
        printf 'node_b_git_commit:   %s\n' "${SRC_COMMIT_NODE_B:-<unavailable>}"
        printf '\n'
        printf '# The COLLECTING host'"'"'s git worktree (may differ from what the nodes run):\n'
        printf 'collector_git_head:  %s\n' "${SRC_COMMIT_GIT:-<unavailable>}"
        printf 'collector_worktree_dirty: %s\n' "$SRC_COMMIT_DIRTY"
        printf '\n'
        printf '# A dirty worktree means the collecting checkout does NOT correspond to any\n'
        printf '# single commit. The node-reported commit is the one that produced the chain.\n'
    } > "$SRC_COMMIT_OUT"
    if [ -n "$SRC_COMMIT_NODE_A" ] || [ -n "$SRC_COMMIT_NODE_B" ] || [ -n "$SRC_COMMIT_GIT" ]; then
        req_record SOURCE_COMMIT 1
        log "bundled SOURCE_COMMIT (derived: node-reported commit and/or collector git HEAD)"
    else
        gap "SOURCE_COMMIT could not be established — no artifacts/SOURCE_COMMIT, no node_git_commit from either node, and no readable git worktree at REPO_ROOT. Record the release commit in $ARTIFACTS_DIR/SOURCE_COMMIT before collecting."
        req_record SOURCE_COMMIT 0
    fi
    if [ -n "$SRC_COMMIT_NODE_A" ] && [ -n "$SRC_COMMIT_NODE_B" ] && [ "$SRC_COMMIT_NODE_A" != "$SRC_COMMIT_NODE_B" ]; then
        warn "the two nodes report DIFFERENT source commits (A=$SRC_COMMIT_NODE_A B=$SRC_COMMIT_NODE_B) — this net is not running one build. Recorded as-is."
    fi
    if [ "$SRC_COMMIT_DIRTY" = "yes" ]; then
        warn "the collecting host's git worktree is DIRTY — collector_git_head does not identify the collected source exactly. Recorded as-is."
    fi
fi

# ===========================================================================
# [5] Provider + batch palw-status dumps, per identity, per node (A and B) so
#     the bundle carries both-node parity evidence, mirroring verify-consensus.
# ===========================================================================
dump_provider() {   # <label A|B|C> <bond outpoint>
    local label="$1" bond="$2" n out
    for n in a b; do
        out="$STAGE/palw-status/provider-$label.node-$n.txt"
        if [ -z "$bond" ]; then
            printf '<skipped: provider-%s bond outpoint not recorded in state.env>\n' "$label" > "$out"
        else
            run_capture "$out" "$(node_rpc_ok "$n")" "palw-status provider-$label ($bond) on node-$n" \
                palw_provider_status "$n" "$bond"
        fi
    done
}
dump_provider A "$PROV_A_BOND"
dump_provider B "$PROV_B_BOND"
dump_provider C "$AUD_C_BOND"

for n in a b; do
    out="$STAGE/palw-status/batch.node-$n.txt"
    if [ -z "$PALW_BATCH_ID" ]; then
        printf '<skipped: PALW_BATCH_ID not recorded in state.env>\n' > "$out"
    else
        run_capture "$out" "$(node_rpc_ok "$n")" "palw-status batch ($PALW_BATCH_ID) on node-$n" \
            palw_batch_status "$n" "$PALW_BATCH_ID"
    fi
done

# ===========================================================================
# [6] Tail of EVERY log under logs/ (node-a.log, node-b.log, miner-supporting.log,
#     and any rotated *.log.<ts>). Public daemon output; no seed CONTENTS appear.
# ===========================================================================
LOG_LIST="$(find "$LOGS_DIR" -type f -name '*.log*' 2>/dev/null | LC_ALL=C sort || true)"
if [ -z "$LOG_LIST" ]; then
    gap "no log files found under $LOGS_DIR — start the net (node-a.sh / node-b.sh / supporting-miner.sh) so there are logs to bundle."
    printf '<no log files present under %s at collection time>\n' "$LOGS_DIR" > "$STAGE/logs/NO-LOGS.txt"
else
    printf '%s\n' "$LOG_LIST" | while IFS= read -r lf; do
        [ -n "$lf" ] || continue
        base="$(basename "$lf")"
        out="$STAGE/logs/$base.tail.txt"
        {
            printf '# tail -n %s of %s (path on the collecting host)\n' "$TAIL_LINES" "$lf"
            tail -n "$TAIL_LINES" "$lf" 2>/dev/null || printf '<could not read %s>\n' "$lf"
        } > "$out"
    done
    log "bundled tails of $(printf '%s\n' "$LOG_LIST" | grep -c .) log file(s) (last $TAIL_LINES lines each)"
fi

# ===========================================================================
# [6b] REMOTE host bundles (§5.4 condition 4). Section [6] tails logs on the
#      COLLECTING host only. For a node whose host is REMOTE, its logs / pid
#      records / effective argv / disk metrics live on THAT host — so ask its
#      agent to bundle them host-local (`collect`, secrets excluded there) and
#      pull the archive back over one SSH hop (`collect-tar` streams a clean tar
#      on stdout; agent log lines go to stderr). Local nodes are already covered
#      by [6], so this loop no-ops on a single host.
# ===========================================================================
pull_remote_host_bundle() {   # <a|b>
    local n="$1" rname localdst host
    rname="agent-collect-$BUNDLE_LABEL"
    localdst="$STAGE/logs/remote-node-$n"
    install -d -m 0755 "$localdst" || { gap "cannot create $localdst"; return 0; }
    # 1. bundle host-local evidence on the node's own host (agent, secrets excluded).
    if ! node_dispatch "$n" collect "$rname" >/dev/null 2>"$localdst/agent-collect.log"; then
        gap "remote 'collect' on node-$n host failed (see $localdst/agent-collect.log)"; return 0
    fi
    # 2. stream the bundle back as a tar. stdout is the clean archive; the agent's
    #    log/warn lines go to stderr (captured separately, never into the tar).
    if node_dispatch "$n" collect-tar "$rname" >"$localdst/bundle.tar" 2>>"$localdst/agent-collect.log"; then
        if ( cd "$localdst" && tar -xf bundle.tar ) 2>/dev/null; then
            rm -f "$localdst/bundle.tar"
            log "pulled remote host bundle for node-$n -> ${localdst#$STAGE/}"
        else
            warn "could not extract remote bundle for node-$n (kept $localdst/bundle.tar for inspection)"
        fi
    else
        gap "could not pull remote bundle from node-$n host (collect-tar failed; see $localdst/agent-collect.log)"
    fi
    # defence-in-depth: a pulled bundle must never carry key material.
    find "$localdst" -type f -name '*.seed' -delete 2>/dev/null || true
}
for n in a b; do
    if node_is_remote "$n"; then
        log "collecting remote host bundle for node-$n ($(node_ssh_host "$n")) via its agent"
        pull_remote_host_bundle "$n"
    fi
done

# ===========================================================================
# [7] Redacted env — PUBLIC config only. Allow-list of names known to hold
#     network/topology config, public commitment ids, public funding addresses
#     and public on-chain outpoints. Secrets are excluded by name AND re-checked
#     per value (_looks_secret). *.seed and key material are NEVER emitted, and
#     artifacts/state.env is NOT copied (it can hold seed paths).
# ===========================================================================
PUBLIC_ENV_KEYS="
NETWORK NETWORK_BASE NETSUFFIX ADDR_PREFIX PALW_ENABLE_ALGO4
NODE_A_HOST NODE_B_HOST RPC_BIND
A_P2P_PORT A_GRPC_PORT A_WRPC_PORT B_P2P_PORT B_GRPC_PORT B_WRPC_PORT
MINER_INTERVAL_MS MINER_WORKER
LEAF_COUNT SHAPE_ID CAPACITY_COUNT TICKET_MODE
DNS_BOND_AMOUNT PROVIDER_A_AMOUNT PROVIDER_B_AMOUNT AUDITOR_AMOUNT
UNBONDING_PERIOD_BLOCKS UNBOND_DELAY_EPOCHS MIN_EPOCH_HEADROOM_DAA
OPERATOR_GROUP_A OPERATOR_GROUP_B OPERATOR_GROUP_AUD
RUNTIME_CLASS_ID MODEL_PROFILE_ID REWARD_KEY_ROOT_A REWARD_KEY_ROOT_B
AUDIT_POLICY_ID DESCRIPTOR_ROOT PROV_A_REWARD_PK_BYTE PROV_B_REWARD_PK_BYTE
DNS_BOND PROV_A_BOND PROV_B_BOND AUD_C_BOND PALW_BATCH_ID
DNS_ADDR PROV_A_ADDR PROV_B_ADDR AUD_C_ADDR PALW_MINE_ADDR SUPPORTING_ADDR
PALW_MINE_ADDRESS
PALW_CHUNK_COUNT PALW_LEAF_COUNT PALW_REG_EPOCH PALW_DA_REAL
PROV_A_REWARD_SPK PROV_B_REWARD_SPK
PALW_ALGO4_BLOCK_HASH_A PALW_ALGO4_BLOCK_HASH_B PALW_ALGO4_ACCEPT_A PALW_ALGO4_ACCEPT_B
PALW_ALGO4_SUBSIDY_SOMPI PALW_ALGO4_PREMIUM_PI_BPS PALW_ALGO4_SOURCE_CLASS
PALW_ALGO4_CB_PROVIDER_A_SOMPI PALW_ALGO4_CB_PROVIDER_B_SOMPI
PALW_ALGO4_CB_INCLUSION_SOMPI PALW_ALGO4_CB_VALIDATOR_SOMPI
PALW_ALGO4_CB_PROVIDER_A_SPK PALW_ALGO4_CB_PROVIDER_B_SPK PALW_COINBASE_VERDICT
"
ENV_OUT="$STAGE/env.redacted"
{
    printf '# PALW closed-testnet — REDACTED public env (STN-013)\n'
    printf '# generated: %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')"
    printf '# PUBLIC config / commitment ids / funding addresses / on-chain outpoints ONLY.\n'
    printf '# Secrets are omitted: NO *.seed, NO ticket secret store, NO key material, and\n'
    printf '# artifacts/state.env itself is NOT bundled (it can hold seed file paths).\n'
    printf '# REPO_ROOT / PALW_DATA_ROOT are host-local paths, deliberately not exported here.\n'
    printf '\n'
} > "$ENV_OUT"
for k in $PUBLIC_ENV_KEYS; do
    v="$(state_get "$k" || true)"
    if _looks_secret "$k" "$v"; then
        printf '# %s=<REDACTED: references key material — never exported>\n' "$k" >> "$ENV_OUT"
        REDACTED_KEYS="$REDACTED_KEYS $k"
        warn "redacted $k in env.redacted (value looked like key material)"
    elif [ -z "$v" ]; then
        printf '# %s=<unset>\n' "$k" >> "$ENV_OUT"
    else
        printf 'export %s=%q\n' "$k" "$v" >> "$ENV_OUT"
    fi
done
log "bundled env.redacted (public config only; secrets redacted)"

# ===========================================================================
# [7b] REDACTIONS.txt — say IN THE BUNDLE what was withheld, and (just as
#      important, and the part evidence bundles usually get wrong) what was NOT.
#      A recipient who is told only "secrets are redacted" cannot judge whether
#      it is safe to republish; this file lets them.
# ===========================================================================
RED_OUT="$STAGE/REDACTIONS.txt"
{
    printf 'PALW closed-testnet evidence bundle — REDACTIONS (STN-05)\n'
    printf 'bundle label: %s\n' "$BUNDLE_LABEL"
    printf 'generated:    %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')"
    printf '\n'
    printf 'WITHHELD BY CONSTRUCTION (never read, never copied by this stage):\n'
    printf '  * <data-root>/keys/**       all key material, including every *.seed\n'
    printf '  * the ticket secret store   never opened\n'
    printf '  * the release-signing key   never opened; signing is the operator step\n'
    printf '  * artifacts/state.env       NOT copied (it can hold seed FILE PATHS);\n'
    printf '                              only the allow-listed public values in\n'
    printf '                              env.redacted are exported\n'
    printf '  * REPO_ROOT / PALW_DATA_ROOT  host-local absolute paths, not exported\n'
    printf '  * SSH host / user config    excluded from env.redacted by allow-list.\n'
    printf '                              Only the PUBLIC P2P NODE_A_HOST/NODE_B_HOST\n'
    printf '                              are exported — peers need them, and the\n'
    printf '                              signed network manifest pins them anyway.\n'
    printf '\n'
    printf 'REDACTED BY VALUE CHECK (_looks_secret) in env.redacted:\n'
    if [ -n "$REDACTED_KEYS" ]; then
        for k in $REDACTED_KEYS; do printf '  * %s  (value referenced key material)\n' "$k"; done
    else
        printf '  none — no allow-listed value looked like key material this run.\n'
    fi
    printf '\n'
    printf 'NOT REDACTED — read this before republishing:\n'
    printf '  * logs/*.tail.txt are the REAL daemon logs, tailed VERBATIM. This stage\n'
    printf '    does not rewrite log text (rewriting it would make the evidence\n'
    printf '    uncheckable). They therefore contain peer IP addresses and P2P\n'
    printf '    endpoints — the same public addresses the network manifest pins — and\n'
    printf '    they may contain seed FILE PATHS a daemon logged in its own argv\n'
    printf '    (e.g. --validator-key <path>). They never contain seed CONTENTS:\n'
    printf '    common.sh guarantees "NO SECRETS TO ARGV / LOG".\n'
    printf '  * the first line of each log tail names the log path on the collecting\n'
    printf '    host, which may embed a username.\n'
    printf '  * remote host bundles pulled in [6b] are produced by each node own\n'
    printf '    agent (which excludes secrets there) and are only *.seed-swept on\n'
    printf '    arrival here — this stage did not author their contents.\n'
    printf '  If your threat model forbids publishing host paths or peer IPs, review\n'
    printf '  logs/ before sharing this bundle. This stage will not pretend it did.\n'
} > "$RED_OUT"
log "bundled REDACTIONS.txt (states what was withheld AND what was not)"

# ===========================================================================
# [8] Defence-in-depth: refuse to publish a bundle that somehow contains a
#     .seed file. By construction we never copy from keys/, so this must be 0.
# ===========================================================================
# MATERIALISE the result — never gate on a pipeline whose writer can be SIGPIPEd.
# With `set -o pipefail` (in force above), `find ... | grep -q .` returns 141
# precisely WHEN a match exists: grep -q exits on the first line, find dies of
# SIGPIPE, and pipefail propagates 141, so the `if` is FALSE and this guard would
# be skipped in exactly the case it exists to catch (reproduced on bash 3.2).
# Do NOT reintroduce `grep -q` or `head -n1` here.
_seed_hits="$(find "$STAGE" -type f -name '*.seed' -print 2>/dev/null || true)"
if [ -n "$_seed_hits" ]; then
    die "internal error: a *.seed file ended up in the staged bundle — refusing to publish evidence containing key material. This is a bug; report it."
fi
# Same guard, one level deeper: no PEM / OpenSSH private-key ARMOUR anywhere in
# the staged tree. A pulled remote bundle or a log tail is the realistic carrier.
# -l prints file NAMES only and the output is discarded, so a matched secret is
# never echoed anywhere.
if grep -rlE 'BEGIN (OPENSSH|RSA|EC|DSA|PGP) PRIVATE KEY' "$STAGE" >/dev/null 2>&1; then
    die "internal error: a staged file contains a PRIVATE KEY block — refusing to publish evidence containing key material. Inspect the likely carrier (a pulled remote host bundle under logs/remote-node-*, or a log tail) and report it."
fi

# ===========================================================================
# [8b] run-result.json — the MACHINE-READABLE summary a third party needs
#      (schema "palw-run-result-v1"). Every field is either a value we actually
#      read or an explicit null; nothing is inferred and nothing is defaulted to
#      a value that could be mistaken for a reading. `evidence_complete` is TRUE
#      only when the required-item ledger is empty of misses AND no gap fired.
#
#      The negative-test counts are read out of the BUNDLED negative-tests.json
#      (palw-negative-tests-v1) — this stage never re-runs or re-scores G7.
#      NOTE (honest limitation): palw-negative-tests-v1 records pass/fail/skip/
#      release_mode/cases but NOT the unjustified-skip count, so `unjustified` is
#      reported as null with a note. The authoritative unjustified-skip gate is
#      negative-tests.sh's own exit code under NEG_RELEASE=1; it cannot be
#      reconstructed from the report file, so we say so rather than guess 0.
#
#      python3 is the established JSON tool in this harness (network-manifest.sh).
#      If it is absent we cannot emit the summary — that is a gap(), not a silent
#      skip. The heredoc writes the file and prints one `key=value` line per
#      derived verdict on stdout, which the shell reads back with common.sh's _kv.
# ===========================================================================
RUNRESULT="$STAGE/run-result.json"
# The derived verdicts come back through a scratch file rather than a command
# substitution: stock macOS bash 3.2 mis-parses a here-document nested inside
# $( ... ), so the heredoc is run as a plain command. The scratch file lives
# OUTSIDE the stage (it is plumbing, not evidence) and the cleanup trap removes
# it on any exit path.
RR_SUMMARY_FILE="$(mktemp "${BUNDLE_DIR}.summary.XXXXXX")" || die "mktemp failed near $BUNDLE_DIR"
register_cleanup "rm -f \"$RR_SUMMARY_FILE\""
# Live sink identities for the summary (only where the node actually answered —
# node_sink talks to the node, so a down node must not be asked).
SINK_A=""; SINK_B=""; SINK_DAA_A=""; SINK_DAA_B=""
if [ "$RPC_OK_A" = "1" ]; then
    SINK_A="$(node_sink a 2>/dev/null || true)"
    SINK_DAA_A="$(node_sink_daa a 2>/dev/null || true)"
fi
if [ "$RPC_OK_B" = "1" ]; then
    SINK_B="$(node_sink b 2>/dev/null || true)"
    SINK_DAA_B="$(node_sink_daa b 2>/dev/null || true)"
fi

if command -v python3 >/dev/null 2>&1; then
    if RR_OUT="$RUNRESULT" RR_SUMMARY_OUT="$RR_SUMMARY_FILE" \
        RR_GENERATED="$(date '+%Y-%m-%dT%H:%M:%S%z')" \
        RR_LABEL="$BUNDLE_LABEL" \
        RR_NETWORK="$NETWORK" RR_NETWORK_BASE="$NETWORK_BASE" RR_NETSUFFIX="$NETSUFFIX" \
        RR_TICKET_MODE="$TICKET_MODE" \
        RR_RPC_OK_A="$RPC_OK_A" RR_RPC_OK_B="$RPC_OK_B" \
        RR_P2P_A="$(node_p2p_addr a)" RR_P2P_B="$(node_p2p_addr b)" \
        RR_SINK_A="$SINK_A" RR_SINK_B="$SINK_B" \
        RR_SINK_DAA_A="$SINK_DAA_A" RR_SINK_DAA_B="$SINK_DAA_B" \
        RR_ID_A="$STAGE/identity/node-a.txt" RR_ID_B="$STAGE/identity/node-b.txt" \
        RR_NEG_FILE="$STAGE/negative-tests.json" \
        RR_DNS_BOND="$DNS_BOND" RR_PROV_A_BOND="$PROV_A_BOND" \
        RR_PROV_B_BOND="$PROV_B_BOND" RR_AUD_C_BOND="$AUD_C_BOND" \
        RR_BATCH_ID="$PALW_BATCH_ID" \
        RR_A4_HASH_A="$A4_HASH_A" RR_A4_HASH_B="$A4_HASH_B" \
        RR_A4_ACCEPT_A="$A4_ACCEPT_A" RR_A4_ACCEPT_B="$A4_ACCEPT_B" \
        RR_A4_SUBSIDY="$A4_SUBSIDY" RR_A4_PI_BPS="$A4_PI_BPS" RR_A4_SRC_CLASS="$A4_SRC_CLASS" \
        RR_A4_CB_A="$A4_CB_A" RR_A4_CB_B="$A4_CB_B" \
        RR_A4_CB_INCL="$A4_CB_INCL" RR_A4_CB_VAL="$A4_CB_VAL" \
        RR_A4_CB_A_SPK="$A4_CB_A_SPK" RR_A4_CB_B_SPK="$A4_CB_B_SPK" \
        RR_A4_CB_VERDICT="$A4_CB_VERDICT" \
        RR_SRC_NODE_A="$SRC_COMMIT_NODE_A" RR_SRC_NODE_B="$SRC_COMMIT_NODE_B" \
        RR_SRC_GIT="$SRC_COMMIT_GIT" RR_SRC_DIRTY="$SRC_COMMIT_DIRTY" \
        RR_NETMAN_SIG="$NETMAN_SIG_STATE" \
        RR_REQ_FOUND="$REQ_FOUND" RR_REQ_MISSING="$REQ_MISSING" \
        RR_REDACTED_KEYS="$REDACTED_KEYS" \
        RR_GAPS="$GAP_COUNT" RR_PARTIAL="${BUNDLE_ALLOW_PARTIAL:-0}" RR_RELEASE="$RELEASE_MODE" \
        python3 - <<'PYEOF'
import json, os

def s(k):
    """Env value or None — never an empty string masquerading as a reading."""
    v = os.environ.get(k, "").strip()
    return v if v else None

def i(k):
    v = s(k)
    try:
        return int(v)
    except (TypeError, ValueError):
        return None

def b(k):
    """A recorded 'true'/'false' verdict, or None when it was never recorded."""
    v = s(k)
    if v is None:
        return None
    lv = v.lower()
    if lv in ("true", "1", "yes"):
        return True
    if lv in ("false", "0", "no"):
        return False
    return v          # keep an unexpected verdict verbatim rather than coercing it

def identity(path):
    """Parse an identity/node-X.txt dump. <unavailable> stays None."""
    out = {}
    try:
        with open(path) as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#") or ":" not in line:
                    continue
                k, v = line.split(":", 1)
                v = v.strip()
                out[k.strip()] = None if v == "<unavailable>" else v
    except OSError:
        return None
    return out or None

id_a = identity(os.environ.get("RR_ID_A", ""))
id_b = identity(os.environ.get("RR_ID_B", ""))

# ---- negative tests: read the bundled report, never re-score it -------------
neg = {
    "present": False,
    "pass": None, "fail": None, "skip": None,
    "unjustified": None,
    "unjustified_note": (
        "palw-negative-tests-v1 does not record the unjustified-skip count, so it "
        "cannot be reconstructed here. The authoritative gate is negative-tests.sh's "
        "own exit code under NEG_RELEASE=1."
    ),
    "require_mint": {},
    "release_mode": None,
    "cases": {},
}
negp = os.environ.get("RR_NEG_FILE", "")
if negp and os.path.exists(negp):
    try:
        d = json.load(open(negp))
        neg["present"] = True
        neg["schema"] = d.get("schema")
        neg["pass"] = d.get("pass")
        neg["fail"] = d.get("fail")
        neg["skip"] = d.get("skip")
        neg["release_mode"] = d.get("release_mode")
        neg["cases"] = d.get("cases", {}) or {}
        # Mirrors MINT_CASES in negative-tests.sh: these cases are structurally
        # unreachable without recorded mint evidence, so their result is called
        # out separately instead of being averaged into the skip count.
        for c in ("wrong-authority", "duplicate-submit", "reorg-parity"):
            if c in neg["cases"]:
                neg["require_mint"][c] = neg["cases"][c]
    except (ValueError, OSError) as e:
        neg["parse_error"] = str(e)

found = [t for t in os.environ.get("RR_REQ_FOUND", "").split() if t]
missing = [t for t in os.environ.get("RR_REQ_MISSING", "").split() if t]
gaps = i("RR_GAPS") or 0
# TRUE only when every required item was ACTUALLY found and nothing was recorded
# as a gap. A TICKET_MODE=skip run has no algo-4 evidence, so it lands FALSE —
# that is the expected interpretation of this bundle.
evidence_complete = (len(missing) == 0 and gaps == 0)

h_a, h_b = s("RR_A4_HASH_A"), s("RR_A4_HASH_B")
doc = {
    "schema": "palw-run-result-v1",
    "generated": s("RR_GENERATED"),
    "bundle_label": s("RR_LABEL"),
    "release_mode": os.environ.get("RR_RELEASE") == "1",
    "allow_partial": os.environ.get("RR_PARTIAL") == "1",
    "network": {
        "network_id": s("RR_NETWORK"),
        "network_base": s("RR_NETWORK_BASE"),
        "netsuffix": i("RR_NETSUFFIX"),
        "ticket_mode": s("RR_TICKET_MODE"),
    },
    "source_commit": {
        "node_a_reported": s("RR_SRC_NODE_A"),
        "node_b_reported": s("RR_SRC_NODE_B"),
        "collector_git_head": s("RR_SRC_GIT"),
        "collector_worktree_dirty": s("RR_SRC_DIRTY"),
    },
    "nodes": {
        "a": {
            "wrpc_up_at_collection": os.environ.get("RR_RPC_OK_A") == "1",
            "p2p": s("RR_P2P_A"),
            "sink": s("RR_SINK_A"),
            "sink_daa_score": i("RR_SINK_DAA_A"),
            "identity": id_a,
        },
        "b": {
            "wrpc_up_at_collection": os.environ.get("RR_RPC_OK_B") == "1",
            "p2p": s("RR_P2P_B"),
            "sink": s("RR_SINK_B"),
            "sink_daa_score": i("RR_SINK_DAA_B"),
            "identity": id_b,
        },
    },
    "sinks_identical": (
        None if (s("RR_SINK_A") is None or s("RR_SINK_B") is None)
        else s("RR_SINK_A") == s("RR_SINK_B")
    ),
    "bonds": {
        "dns": s("RR_DNS_BOND"),
        "provider_a": s("RR_PROV_A_BOND"),
        "provider_b": s("RR_PROV_B_BOND"),
        "auditor_c": s("RR_AUD_C_BOND"),
    },
    "batch_id": s("RR_BATCH_ID"),
    "algo4_block": {
        "block_hash_node_a": h_a,
        "block_hash_node_b": h_b,
        "hashes_identical": (None if (h_a is None or h_b is None) else h_a == h_b),
        "accept_node_a": b("RR_A4_ACCEPT_A"),
        "accept_node_b": b("RR_A4_ACCEPT_B"),
        "note": (
            "Recorded by the mint stage; an algo-4 block has fork-choice weight 0 "
            "(PALW-014) and never becomes the sink. Nulls mean 'never recorded', "
            "not 'false'."
        ),
    },
    "settlement": {
        "subsidy_sompi": i("RR_A4_SUBSIDY"),
        "premium_pi_bps": i("RR_A4_PI_BPS"),
        "source_class": s("RR_A4_SRC_CLASS"),
        "observed_provider_a_sompi": i("RR_A4_CB_A"),
        "observed_provider_b_sompi": i("RR_A4_CB_B"),
        "observed_inclusion_sompi": i("RR_A4_CB_INCL"),
        "observed_validator_sompi": i("RR_A4_CB_VAL"),
        "observed_provider_a_spk": s("RR_A4_CB_A_SPK"),
        "observed_provider_b_spk": s("RR_A4_CB_B_SPK"),
        "verify_coinbase_verdict": s("RR_A4_CB_VERDICT"),
        "note": (
            "Providers are paid on a LATER block that blue-merges the algo-4 block; "
            "null observed_* values mean DEFERRED (not yet observed), never zero-paid. "
            "See verify-coinbase.txt for the asserted split."
        ),
    },
    "negative_tests": neg,
    "signatures": {
        "network-manifest.json": s("RR_NETMAN_SIG"),
        "MANIFEST.txt": "absent-at-collection",
        "note": (
            "MANIFEST.txt is produced by this run, so its detached signature can only "
            "exist after it: signing is the operator's step (see SIGNATURES.txt). "
            "'present-unverified' means a .sig exists but could not be checked here; "
            "only 'verified' means ssh-keygen -Y verify succeeded."
        ),
    },
    "redaction": {
        "env_keys_redacted": [k for k in os.environ.get("RR_REDACTED_KEYS", "").split() if k],
        "state_env_bundled": False,
        "seed_or_key_material_bundled": False,
        "log_tails_rewritten": False,
    },
    "required_evidence": {"found": found, "missing": missing},
    "recorded_gaps": gaps,
    "evidence_complete": evidence_complete,
}

with open(os.environ["RR_OUT"], "w") as f:
    json.dump(doc, f, indent=2, sort_keys=True)
    f.write("\n")

# Machine-readable handback for the shell (read with common.sh's _kv).
with open(os.environ["RR_SUMMARY_OUT"], "w") as f:
    f.write("evidence_complete=%d\n" % (1 if evidence_complete else 0))
    f.write("neg_present=%d\n" % (1 if neg["present"] else 0))
    f.write("neg_fail=%s\n" % ("unknown" if neg["fail"] is None else neg["fail"]))
    f.write("neg_release_mode=%s\n" % ("unknown" if neg["release_mode"] is None else str(neg["release_mode"]).lower()))
PYEOF
    then
        :
    else
        warn "python3 failed while building run-result.json (see its error above)."
    fi
    if [ -s "$RUNRESULT" ]; then
        log "bundled run-result.json (palw-run-result-v1)"
    else
        gap "run-result.json could not be written — the bundle would carry no machine-readable summary. See the python3 error above."
    fi
else
    gap "python3 is not on PATH — cannot emit run-result.json (palw-run-result-v1), the machine-readable summary a third party needs. Install python3 (the harness already requires it for network-manifest.sh) and re-run."
    printf '<MISSING: run-result.json — python3 was unavailable on the collecting host at collection time, so no machine-readable run summary was produced.>\n' \
        > "$STAGE/run-result.json.MISSING"
fi

# Read the derived verdicts back with common.sh's _kv, then drop the scratch file
# (the trap would too, but evidence plumbing should not outlive its use).
EVIDENCE_COMPLETE=""; NEG_FAIL=""; NEG_RELEASE_MODE=""
if [ -s "$RR_SUMMARY_FILE" ]; then
    EVIDENCE_COMPLETE="$(_kv evidence_complete < "$RR_SUMMARY_FILE" || true)"
    NEG_FAIL="$(_kv neg_fail            < "$RR_SUMMARY_FILE" || true)"
    NEG_RELEASE_MODE="$(_kv neg_release_mode < "$RR_SUMMARY_FILE" || true)"
fi
rm -f "$RR_SUMMARY_FILE"
# Absent/unreadable verdicts are treated as NOT complete — never as a pass.
[ -n "$EVIDENCE_COMPLETE" ] || EVIDENCE_COMPLETE=0
if [ "$EVIDENCE_COMPLETE" = "1" ]; then
    log "evidence_complete=true — every required artifact was found:$REQ_FOUND"
else
    warn "evidence_complete=FALSE — missing required artifact(s):${REQ_MISSING:- <none recorded; see recorded gaps>}. This bundle is honest but INCOMPLETE for third-party verification."
fi

# ===========================================================================
# [8c] SIGNATURES.txt — record, in the bundle, whether each signable artifact
#      had a detached .sig at collection time, and print the EXACT command the
#      human operator runs to sign MANIFEST.txt. This script NEVER generates,
#      reads or prints a private key; the in-bundle text names the key only as
#      $PALW_MANIFEST_KEY so no host-local path leaks into published evidence
#      (the fully-resolved command goes to the operator's terminal below).
# ===========================================================================
SIG_OUT="$STAGE/SIGNATURES.txt"
{
    printf 'PALW closed-testnet evidence bundle — SIGNATURES (STN-05)\n'
    printf 'bundle label: %s\n' "$BUNDLE_LABEL"
    printf 'generated:    %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')"
    printf '\n'
    printf 'signable artifact        state of its detached signature at collection time\n'
    printf '-----------------------  ---------------------------------------------------\n'
    printf 'network-manifest.json    %s\n' "$NETMAN_SIG_STATE"
    printf 'MANIFEST.txt             not-yet-signed (this run PRODUCED it; signing is the\n'
    printf '                         operator step below, performed after this script exits)\n'
    printf '\n'
    printf '  verified            = ssh-keygen -Y verify succeeded against the allowed-signers pin\n'
    printf '  present-unverified  = a .sig exists but could NOT be checked here (no pin, or no\n'
    printf '                        ssh-keygen). Presence is NOT verification.\n'
    printf '  BAD                 = a .sig exists and does NOT verify. Do not trust this bundle.\n'
    printf '  absent              = no detached signature at all.\n'
    printf '\n'
    printf 'THIS HARNESS NEVER HANDLES A PRIVATE KEY. Signing is a human step on the\n'
    printf 'coordinator machine, with the DEDICATED release key network-manifest.sh\n'
    printf 'creates under <data-root>/keys/ (override: $PALW_MANIFEST_KEY). The key never\n'
    printf 'leaves the coordinator and is never copied into a bundle; only its PUBLIC half\n'
    printf 'travels, in network-manifest.json.signers.\n'
    printf '\n'
    printf '1) Detach-sign this bundle manifest — run FROM THIS DIRECTORY:\n'
    printf '       ssh-keygen -Y sign -f "$PALW_MANIFEST_KEY" -n %s MANIFEST.txt\n' "$NS_EVIDENCE"
    printf '   That writes MANIFEST.txt.sig next to MANIFEST.txt.\n'
    printf '\n'
    printf '2) Anyone can then verify, from this directory:\n'
    printf '       ssh-keygen -Y verify -f network-manifest.json.signers -I %s \\\n' "$SIGN_IDENTITY"
    printf '           -n %s -s MANIFEST.txt.sig < MANIFEST.txt\n' "$NS_EVIDENCE"
    printf '       ssh-keygen -Y verify -f network-manifest.json.signers -I %s \\\n' "$SIGN_IDENTITY"
    printf '           -n %s -s network-manifest.json.sig < network-manifest.json\n' "$NS_MANIFEST"
    printf '   ...and re-hash the listed files to confirm they match MANIFEST.txt.\n'
    printf '\n'
    if [ "$NETMAN_SIG_STATE" = "absent" ]; then
        printf '3) network-manifest.json in this bundle is UNSIGNED. Sign it WHERE IT LIVES —\n'
        printf '   the bundle copy is evidence, the live file is the release identity:\n'
        printf '       ./network-manifest.sh generate        # regenerates AND signs\n'
        printf '   then re-collect. An unsigned manifest is not a release identity.\n'
        printf '\n'
    fi
    printf 'Namespaces are separated so a signature over one document can never be replayed\n'
    printf 'as a signature over the other: %s for the network manifest, %s for this\n' "$NS_MANIFEST" "$NS_EVIDENCE"
    printf 'evidence manifest.\n'
    printf '\n'
    printf 'MANIFEST.txt.sig is deliberately ABSENT from MANIFEST.txt own listing: a\n'
    printf 'detached signature can never be inside the listing it signs.\n'
    printf '\n'
    printf 'After signing, re-check without rewriting any evidence:\n'
    printf '       ./collect-artifacts.sh --check-signatures %s\n' "$BUNDLE_LABEL"
} > "$SIG_OUT"
log "bundled SIGNATURES.txt (network-manifest.json signature: $NETMAN_SIG_STATE)"

# ===========================================================================
# [9] MANIFEST.txt — listing of every bundled file: sha256, byte size, path.
#     Written last and excluded from its own listing.
# ===========================================================================
MAN="$STAGE/MANIFEST.txt"
{
    printf 'PALW closed-testnet evidence bundle — MANIFEST (STN-013)\n'
    printf 'bundle label:        %s\n' "$BUNDLE_LABEL"
    printf 'generated:           %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')"
    printf 'network:             %s (base=%s suffix=%s)\n' "$NETWORK" "$NETWORK_BASE" "$NETSUFFIX"
    printf 'ticket mode:         %s\n' "$TICKET_MODE"
    printf 'node A wRPC:         %s   (up at collection: %s)\n' "$(node_wrpc a)" "$( [ "$RPC_OK_A" = 1 ] && echo yes || echo no)"
    printf 'node B wRPC:         %s   (up at collection: %s)\n' "$(node_wrpc b)" "$( [ "$RPC_OK_B" = 1 ] && echo yes || echo no)"
    printf 'recorded gaps:       %s%s\n' "$GAP_COUNT" "$( [ "${BUNDLE_ALLOW_PARTIAL:-}" = 1 ] && printf ' (BUNDLE_ALLOW_PARTIAL=1: partial post-mortem bundle)' )"
    printf 'evidence complete:   %s\n' "$( [ "$EVIDENCE_COMPLETE" = 1 ] && printf 'yes' || printf 'no — missing:%s' "${REQ_MISSING:- <see recorded gaps>}" )"
    printf 'release mode:        %s\n' "$( [ "$RELEASE_MODE" = 1 ] && printf 'PALW_RELEASE_MODE=1 (fail-closed)' || printf 'no' )"
    printf 'netman signature:    %s\n' "$NETMAN_SIG_STATE"
    printf '\n'
    printf 'SCOPE: real evidence read LIVE from the two validators over independent\n'
    printf 'wRPC + real logs + real binary hashes. NOT the seeded test-only palw_demo path;\n'
    printf 'nothing was minted here. On a single host the two status dumps prove two\n'
    printf 'processes agree, NOT network-partition survival (STN-003). NO seed or secret\n'
    printf 'material is bundled; artifacts/state.env is not copied. See REDACTIONS.txt for\n'
    printf 'what was withheld AND what was not, and run-result.json for the machine-readable\n'
    printf 'summary (evidence_complete tells you whether this bundle is verifiable in full).\n'
    printf '\n'
    printf 'THIS LISTING IS NOT SIGNED BY THIS SCRIPT. A sha256 list proves nothing about\n'
    printf 'WHO produced it. The operator detach-signs MANIFEST.txt with the release key\n'
    printf 'after collection — see SIGNATURES.txt for the exact command, and for whether\n'
    printf 'each signable artifact had a signature when this bundle was built. MANIFEST.txt\n'
    printf 'and MANIFEST.txt.sig are both excluded from the listing below (a file cannot\n'
    printf 'list its own hash, and a detached signature cannot be inside what it signs).\n'
    printf '\n'
    printf 'files (sha256  bytes  path):\n'
} > "$MAN"
# Compute the listing from within the stage so paths are bundle-relative. The
# subshell cd never changes this script's own working directory.
(
    cd "$STAGE" || exit 1
    find . -type f ! -name 'MANIFEST.txt' ! -name 'MANIFEST.txt.sig' | LC_ALL=C sort | while IFS= read -r f; do
        rel="${f#./}"
        h="$($SHA256_TOOL "$f" 2>/dev/null | awk 'NR==1{print $1}')"
        [ -n "$h" ] || h="<hash-failed>"
        sz="$(wc -c < "$f" 2>/dev/null | tr -d ' ')"
        [ -n "$sz" ] || sz="?"
        printf '%s  %s  %s\n' "$h" "$sz" "$rel"
    done
) >> "$MAN" || die "failed to build the manifest listing."
log "bundled MANIFEST.txt"

# ===========================================================================
# Normalise permissions, then commit atomically (idempotent, never a silent
# overwrite). A prior bundle of the same label is replaced only with
# BUNDLE_FORCE=1, and that replacement is LOGGED.
# ===========================================================================
find "$STAGE" -type d -exec chmod 0755 {} + 2>/dev/null || true
find "$STAGE" -type f -exec chmod 0644 {} + 2>/dev/null || true

if [ -e "$BUNDLE_DIR" ]; then
    if [ "${BUNDLE_FORCE:-}" = "1" ]; then
        warn "bundle already exists at $BUNDLE_DIR; BUNDLE_FORCE=1 -> replacing it"
        rm -rf "$BUNDLE_DIR" || die "cannot remove existing bundle $BUNDLE_DIR for replacement."
        mv "$STAGE" "$BUNDLE_DIR" || die "failed to move staged bundle into place at $BUNDLE_DIR."
    else
        die "a bundle already exists at $BUNDLE_DIR — this harness will not silently overwrite evidence. Choose a new label (BUNDLE_LABEL=... or pass a LABEL arg), or set BUNDLE_FORCE=1 to replace it."
    fi
else
    mv "$STAGE" "$BUNDLE_DIR" || die "failed to move staged bundle into place at $BUNDLE_DIR."
fi
STAGE=""   # committed: the cleanup trap must not touch the published bundle

if [ "$GAP_COUNT" -gt 0 ]; then
    # Reachable only under BUNDLE_ALLOW_PARTIAL=1 (otherwise gap() already died).
    warn "STN-013 bundle written with $GAP_COUNT recorded gap(s) (BUNDLE_ALLOW_PARTIAL=1) -> $BUNDLE_DIR. See MANIFEST.txt and the in-place markers; this is an incomplete post-mortem bundle."
else
    log "STN-013 evidence bundle complete -> $BUNDLE_DIR (see MANIFEST.txt). No secrets bundled; no *.seed copied."
fi

# ===========================================================================
# [10] The OPERATOR SIGNING STEP (STN-05). A sha256 listing proves the bundle is
#      internally consistent; it proves nothing about WHO produced it. That gap
#      is closed by a detached signature from the release key — made by a human,
#      never by this script, which has not read and will not read any key.
#      Printed with the fully-resolved paths here (operator's terminal only; the
#      in-bundle SIGNATURES.txt keeps the $PALW_MANIFEST_KEY placeholder form).
# ===========================================================================
warn "NEXT STEP — SIGN THE BUNDLE (this harness never handles a private key):"
warn "    ( cd $BUNDLE_DIR && $(_sign_cmd MANIFEST.txt "$NS_EVIDENCE") )"
warn "  then re-check WITHOUT rewriting any evidence:"
warn "    ${0##*/} --check-signatures $BUNDLE_LABEL"
if [ "$NETMAN_SIG_STATE" = "absent" ]; then
    warn "  network-manifest.json is UNSIGNED. Sign it where it LIVES (not in the bundle):"
    warn "    ./network-manifest.sh generate      # regenerates AND signs, then re-collect"
    warn "    (equivalently: $(_sign_cmd "$NETMAN_SRC" "$NS_MANIFEST"))"
fi

# ---------------------------------------------------------------------------
# Release gate (PALW_RELEASE_MODE=1). Deliberately AFTER the commit: the operator
# needs the bundle in hand in order to sign it, so a non-zero exit here means
# "not release-ready yet", never "the evidence was thrown away".
# ---------------------------------------------------------------------------
if [ "$RELEASE_MODE" = "1" ]; then
    # A release bundle must carry a G7 report that actually PASSED, and that was
    # produced under the release gate. We read only what we bundled; we never
    # re-run or re-score negative-tests.sh.
    if [ "$NEG_FAIL" != "0" ]; then
        die "PALW_RELEASE_MODE=1: the bundled negative-tests.json reports fail=${NEG_FAIL:-<unreadable>} — a release bundle must carry a PASSING G7 report. Fix the failing case(s) and re-run: NEG_RELEASE=1 ./negative-tests.sh all
The bundle IS written (nothing was discarded) -> $BUNDLE_DIR"
    fi
    if [ "$NEG_RELEASE_MODE" != "true" ]; then
        die "PALW_RELEASE_MODE=1: the bundled negative-tests.json records release_mode=${NEG_RELEASE_MODE:-<unreadable>} — the G7 suite was not run under its own release gate, so unjustified skips were never fatal. Re-run it as: NEG_RELEASE=1 ./negative-tests.sh all
(palw-negative-tests-v1 does not record the unjustified-skip COUNT, so that gate can only be enforced by negative-tests.sh itself — this stage cannot reconstruct it.)
The bundle IS written (nothing was discarded) -> $BUNDLE_DIR"
    fi
    if [ "$EVIDENCE_COMPLETE" != "1" ]; then
        die "PALW_RELEASE_MODE=1: evidence_complete=false — missing required artifact(s):${REQ_MISSING:- <see recorded gaps>}. A release bundle must carry every required item. Produce the missing evidence and re-collect.
The bundle IS written (nothing was discarded) -> $BUNDLE_DIR"
    fi
    _rel_missing=""
    [ "$NETMAN_SIG_STATE" = "verified" ] || _rel_missing="$_rel_missing network-manifest.json($NETMAN_SIG_STATE)"
    [ -s "$BUNDLE_DIR/MANIFEST.txt.sig" ] || _rel_missing="$_rel_missing MANIFEST.txt(absent)"
    if [ -n "$_rel_missing" ]; then
        die "PALW_RELEASE_MODE=1: MISSING or UNVERIFIED detached signature(s):$_rel_missing
An unsigned evidence bundle is not a release artifact. The bundle IS written
(nothing was discarded) -> $BUNDLE_DIR
Sign it yourself — this harness never generates, reads or handles a private key:
    ( cd $BUNDLE_DIR && $(_sign_cmd MANIFEST.txt "$NS_EVIDENCE") )
and, if network-manifest.json is unsigned or unverifiable, fix it where it lives:
    ./network-manifest.sh generate      # regenerates AND signs, then re-collect
Then close the gate WITHOUT rewriting the evidence:
    ${0##*/} --check-signatures $BUNDLE_LABEL"
    fi
    log "PALW_RELEASE_MODE=1: release gate PASS — evidence_complete, G7 passing under its own release gate, and every signable artifact carries a VERIFIED detached signature."
fi
exit 0
