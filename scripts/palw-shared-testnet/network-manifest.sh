#!/usr/bin/env bash
# =============================================================================
# network-manifest.sh — §11.3 signed network manifest for the PALW closed testnet.
#
#   usage:  ./network-manifest.sh generate            # build + sign the manifest
#           ./network-manifest.sh verify [FILE]       # verify signature + LIVE node identity
#           ./network-manifest.sh show [FILE]         # print the manifest
#           ./network-manifest.sh --help
#
# WHAT IT IS: one signed JSON document (misaka-palw-network-manifest-v1) pinning
# the release identity a shared net must agree on — network id, the ACTUAL
# genesis hash + consensus-params hash + header version + effective algo4 flag
# (all read from the RUNNING nodes' getConsensusIdentity RPC, never re-derived
# client-side), the release binary SHA-256s (STN-001), and the node roster.
# `verify` fail-closes when the signature is bad OR any LIVE node's identity
# differs from the pinned values (review §11.4: binary-hash mismatch, algo4-flag
# mismatch, params-hash mismatch are each fatal).
#
# STN-01 (audit, HIGH): `verify` is fail-CLOSED, never fail-open.
#   * the binary comparison is a STRICT SET compare — a name PINNED by the signed
#     manifest but absent from the local set is a FAILURE (deleting a line from
#     binary-hashes.txt used to make verification "PASS"), as are unpinned local
#     binaries and digest mismatches;
#   * a missing/empty binary-hashes.txt is a loud INCOMPLETE warning (the PASS
#     line says so) and is FATAL under PALW_RELEASE_MODE=1;
#   * under PALW_RELEASE_MODE=1 the recorded hash file is NOT trusted as the
#     source of truth — the on-disk release binaries are re-hashed and THOSE are
#     compared to the pin (the file is cross-checked too, so a stale attestation
#     is an incident, not a shrug);
#   * the pinned `commit` is compared to each node's LIVE node_git_commit.
#   * the PASS line enumerates exactly WHAT was verified, so it can never
#     overstate (which binary mode, and the commit-binding status).
#
# SIGNATURE: OpenSSH `ssh-keygen -Y sign` (available on stock macOS + Linux) with
# the release coordinator's SSH key (PALW_MANIFEST_KEY, default ~/.ssh/id_ed25519)
# under the namespace `palw-manifest`. Verification uses an allowed-signers file
# (PALW_MANIFEST_SIGNERS, default <manifest>.signers) that pins WHO may sign —
# distribute that file out-of-band with the harness, like the SSH known_hosts.
#
# HONEST SCOPE: this signs and checks the CONFIGURED release identity; it cannot
# prove a node runs the hashed binary (no remote attestation). Combined with
# preflight's binary-hash comparison and the server-side identity RPC it closes
# the review's §11 "release identity is optional" gap for a closed testnet.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="${PALW_LOG_TAG:-net-manifest}"; export PALW_LOG_TAG

usage() {
    cat >&2 <<EOF
usage: ${0##*/} {generate|verify [FILE]|show [FILE]|--help}

  generate      Read BOTH live nodes' getConsensusIdentity, require them to agree,
                bundle the STN-001 binary hashes + node roster, write
                artifacts/network-manifest.json and SIGN it (ssh-keygen -Y sign,
                key: \$PALW_MANIFEST_KEY, default ~/.ssh/id_ed25519).
  verify [FILE] Verify the detached signature against the allowed-signers pin
                (\$PALW_MANIFEST_SIGNERS, default FILE.signers) AND compare each
                LIVE node's identity RPC (incl. node_git_commit) to the pinned
                values, AND strict-set-compare the release binaries. Any
                mismatch — including a PINNED binary missing locally — dies.
  show [FILE]   Print the manifest.

Default FILE: \$PALW_DATA_ROOT/artifacts/network-manifest.json

env: PALW_RELEASE_MODE=1 — release gate for \`verify\`: re-hash the ON-DISK
     binaries instead of trusting artifacts/binary-hashes.txt, and turn every
     check that cannot actually be performed (absent hash file, unverifiable
     commit binding) from a warning into a fatal error.
EOF
}

ACTION="${1:-}"
case "$ACTION" in
    -h|--help|help|"") usage; [ -z "$ACTION" ] && exit 2 || exit 0 ;;
    generate|verify|show) : ;;
    *) usage; die "unknown action '$ACTION'" ;;
esac

require_cmd ssh-keygen python3
load_env

MANIFEST="${2:-$PALW_DATA_ROOT/artifacts/network-manifest.json}"
SIG="$MANIFEST.sig"
SIGNERS="${PALW_MANIFEST_SIGNERS:-$MANIFEST.signers}"
# The release-signing key is a DEDICATED key, never the operator's personal SSH
# identity: it is generated passphrase-less under keys/ on first `generate` (the
# coordinator machine), and only its PUBLIC half travels (in the .signers pin).
SIGNKEY="${PALW_MANIFEST_KEY:-$PALW_DATA_ROOT/keys/manifest-signing.key}"
NAMESPACE="palw-manifest"
# Release gate (STN-01). PALW_RELEASE_MODE=1 = "this verify is the gate that lets
# a host join the shared net": every check `verify` cannot ACTUALLY perform stops
# being a warning and becomes fatal, and the recorded hash file stops being the
# source of truth. Same spirit as negative-tests.sh's NEG_RELEASE=1.
# VALIDATED, not merely read: comparing an unvalidated value against the literal "1"
# means PALW_RELEASE_MODE=true (or yes, or "1 ") silently DOWNGRADES every release
# check back to a warning. A fail-closed gate does not guess. (Same guard as
# preflight.sh and negative-tests.sh's NEG_REQUIRE_MINT.)
case "${PALW_RELEASE_MODE:-0}" in
    1)    RELEASE_MODE=1 ;;
    0|"") RELEASE_MODE=0 ;;
    *)    die "PALW_RELEASE_MODE must be 1 (release gate) or 0/unset (dev loop); got '${PALW_RELEASE_MODE:-}' — a fail-closed gate does not guess" ;;
esac

# _identity <a|b> — the node's server-side consensus identity as `key: value` lines
#   (kaspa-pq-validator status prints node_genesis_hash/node_params_hash/...).
_identity() {
    local n="$1"
    _endpoint_open "$(node_wrpc "$n")" || die "node-$n wRPC $(node_wrpc "$n") is not answering — both nodes must be up"
    "$VAL" status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" 2>/dev/null \
        || die "node-$n identity query failed"
}

# _ident_field <blob> <key> — extract one identity value.
_ident_field() { printf '%s\n' "$1" | awk -F': ' -v k="$2" '$1==k {print $2; exit}' | awk '{print $1}'; }

# _binary_set_match <label> <"<64hex>  <name>" lines> — STRICT set comparison of a
#   local binary set against the manifest's pinned `binaries` map. Fail-closed on
#   all THREE difference classes, each named separately in the diagnostic:
#     missing  — PINNED by the signed manifest but ABSENT locally. STN-01(a): the
#                old comparison skipped these (`if name in local`), so deleting a
#                line from binary-hashes.txt made verification print PASS.
#     extra    — present locally but NOT pinned (an unattested binary).
#     mismatch — present on both sides with a different digest.
#   The python exits non-zero on any difference and the diagnostic it printed
#   becomes this function's die() message, so the shell always fail-closes.
#   EDITING NOTE: this heredoc lives inside a command substitution, and bash 3.2
#   (stock macOS) still scans quoting while skipping such a heredoc — so the
#   python body below must keep apostrophes and parentheses balanced or the
#   whole script fails to parse on a Mac. Same constraint the pre-existing
#   `generate`/`verify` heredocs obey.
_binary_set_match() {
    local label="${1:?label}" text="${2-}" diag
    if diag="$(LOCAL_TEXT="$text" MANIFEST="$MANIFEST" python3 - <<'PYEOF'
import json, os, sys
pinned = json.load(open(os.environ["MANIFEST"])).get("binaries", {})
if not pinned:
    print("the signed manifest pins NO binaries at all — it attests nothing")
    sys.exit(1)
local = {}
for line in os.environ["LOCAL_TEXT"].splitlines():
    parts = line.split()
    # shasum/sha256sum format: "<64-hex>  <path>" — key by the basename of <path>.
    # (comment/metadata lines never have a 64-hex first field, so they drop out)
    if len(parts) >= 2 and len(parts[0]) == 64 and all(c in "0123456789abcdef" for c in parts[0]):
        local[os.path.basename(parts[-1])] = "sha256:" + parts[0]
missing  = sorted(set(pinned) - set(local))
extra    = sorted(set(local) - set(pinned))
mismatch = sorted(n for n in (set(pinned) & set(local)) if pinned[n] != local[n])
diag = []
if missing:
    diag.append("PINNED-BUT-MISSING locally: " + ", ".join("%s (pinned %s)" % (n, pinned[n]) for n in missing))
if extra:
    diag.append("LOCAL-BUT-UNPINNED: " + ", ".join("%s (local %s)" % (n, local[n]) for n in extra))
if mismatch:
    diag.append("DIGEST MISMATCH: " + "; ".join("%s: local=%s pinned=%s" % (n, local[n], pinned[n]) for n in mismatch))
if diag:
    print(" | ".join(diag))
    sys.exit(1)
PYEOF
)"; then
        return 0
    fi
    [ -n "$diag" ] || die "binary set comparison [$label] could not run (python3 failed; see its error above) — refusing to report a PASS on a check that did not execute"
    die "binary set does NOT match the signed manifest [$label]: $diag — this host does not run the pinned release; do not proceed"
}

case "$ACTION" in
generate)
    log "reading LIVE consensus identity from both nodes (server-side getConsensusIdentity)"
    ID_A="$(_identity a)"
    ID_B="$(_identity b)"
    GEN_A="$(_ident_field "$ID_A" node_genesis_hash)";  GEN_B="$(_ident_field "$ID_B" node_genesis_hash)"
    PAR_A="$(_ident_field "$ID_A" node_params_hash)";   PAR_B="$(_ident_field "$ID_B" node_params_hash)"
    HV_A="$(_ident_field "$ID_A" node_header_version_effective)"; HV_B="$(_ident_field "$ID_B" node_header_version_effective)"
    A4_A="$(_ident_field "$ID_A" node_palw_algo4_accept)"; A4_B="$(_ident_field "$ID_B" node_palw_algo4_accept)"
    GIT_A="$(_ident_field "$ID_A" node_git_commit)"

    [ -n "$GEN_A" ] || die "node A did not report node_genesis_hash — its binary predates getConsensusIdentity; rebuild + restart both nodes first"
    [ -n "$PAR_A" ] || die "node A did not report node_params_hash — rebuild + restart both nodes first"
    # §11.4: the manifest pins ONE identity — refusing to sign a disagreeing pair
    # is the point (a mismatch here IS the incident, not an inconvenience).
    [ "$GEN_A" = "$GEN_B" ] || die "genesis hash disagrees between live nodes (A=$GEN_A B=$GEN_B) — refusing to sign a split-identity net"
    [ "$PAR_A" = "$PAR_B" ] || die "consensus params hash disagrees between live nodes (A=$PAR_A B=$PAR_B) — refusing to sign"
    [ "$HV_A" = "$HV_B" ]   || die "effective header version disagrees (A=$HV_A B=$HV_B) — refusing to sign"
    [ "$A4_A" = "$A4_B" ]   || die "effective palw_algo4_accept disagrees (A=$A4_A B=$A4_B) — refusing to sign"

    HASHES_FILE="$PALW_DATA_ROOT/artifacts/binary-hashes.txt"
    [ -s "$HASHES_FILE" ] || die "STN-001 binary hashes not found at $HASHES_FILE — run ./build-and-hash.sh first (the manifest pins the release binaries)"

    log "building $MANIFEST"
    MANIFEST_TMP="$(mktemp "$MANIFEST.XXXXXX")"
    GEN_A="$GEN_A" PAR_A="$PAR_A" HV_A="$HV_A" A4_A="$A4_A" GIT_A="$GIT_A" \
    NETWORK="$NETWORK" NETSUFFIX="$NETSUFFIX" HASHES_FILE="$HASHES_FILE" \
    NODE_A_HOST="$NODE_A_HOST" NODE_B_HOST="$NODE_B_HOST" A_P2P_PORT="$A_P2P_PORT" B_P2P_PORT="$B_P2P_PORT" \
    python3 - > "$MANIFEST_TMP" <<'PYEOF'
import json, os, sys
binaries = {}
with open(os.environ["HASHES_FILE"]) as f:
    for line in f:
        parts = line.split()
        # shasum/sha256sum format: "<64-hex>  <path>" — key by the path's basename.
        if len(parts) >= 2 and len(parts[0]) == 64 and all(c in "0123456789abcdef" for c in parts[0]):
            binaries[os.path.basename(parts[-1])] = "sha256:" + parts[0]
doc = {
    "schema": "misaka-palw-network-manifest-v1",
    "network_id": os.environ["NETWORK"],
    "netsuffix": int(os.environ["NETSUFFIX"]),
    "genesis_hash": os.environ["GEN_A"],
    "consensus_params_hash": os.environ["PAR_A"],
    "header_version": int(os.environ["HV_A"] or 0),
    "palw_algo4_accept": os.environ["A4_A"] == "true",
    "commit": os.environ.get("GIT_A", ""),
    "binaries": binaries,
    "nodes": [
        {"id": "node-a", "p2p": f'{os.environ["NODE_A_HOST"]}:{os.environ["A_P2P_PORT"]}', "role": ["archive", "validator"]},
        {"id": "node-b", "p2p": f'{os.environ["NODE_B_HOST"]}:{os.environ["B_P2P_PORT"]}', "role": ["archive"]},
    ],
}
json.dump(doc, sys.stdout, indent=2, sort_keys=True)
sys.stdout.write("\n")
PYEOF
    mv "$MANIFEST_TMP" "$MANIFEST"

    if [ ! -f "$SIGNKEY" ]; then
        log "generating a DEDICATED release-signing key -> $SIGNKEY (ed25519, key stays on the coordinator; only the public half travels in the .signers pin)"
        install -d -m 0700 "$(dirname "$SIGNKEY")"
        ssh-keygen -t ed25519 -N '' -C 'palw-release-manifest' -f "$SIGNKEY" -q \
            || die "could not generate the release-signing key at $SIGNKEY"
    fi
    log "signing with $SIGNKEY (namespace $NAMESPACE)"
    # Remove any prior signature FIRST: ssh-keygen -Y sign PROMPTS interactively on
    # an existing .sig (hanging headless runs), and a stale signature for an older
    # manifest body would fail verification confusingly.
    rm -f "$SIG"
    ssh-keygen -Y sign -f "$SIGNKEY" -n "$NAMESPACE" "$MANIFEST" \
        || die "ssh-keygen -Y sign failed (the signature is REQUIRED — an unsigned manifest is not a release identity)"
    # ssh-keygen writes MANIFEST.sig next to the file.
    [ -s "$SIG" ] || die "expected signature at $SIG was not produced"

    # Emit a starter allowed-signers pin for the verifier side if none exists yet.
    if [ ! -f "$SIGNERS" ]; then
        printf 'palw-release %s\n' "$(awk '{print $1" "$2}' "$SIGNKEY.pub")" > "$SIGNERS"
        log "wrote allowed-signers pin -> $SIGNERS (distribute out-of-band with the harness)"
    fi
    log "manifest signed: $MANIFEST (+ .sig). Verify anywhere with: ./network-manifest.sh verify $MANIFEST"
    ;;

verify)
    [ -s "$MANIFEST" ] || die "manifest not found: $MANIFEST — shared mode requires a signed network manifest (generate it on the coordinator: ./network-manifest.sh generate)"
    [ -s "$SIG" ]      || die "manifest signature not found: $SIG — an unsigned manifest is not a release identity (fail-closed)"
    [ -s "$SIGNERS" ]  || die "allowed-signers pin not found: $SIGNERS — distribute it out-of-band (PALW_MANIFEST_SIGNERS)"

    log "verifying signature (allowed-signers: $SIGNERS)"
    ssh-keygen -Y verify -f "$SIGNERS" -I palw-release -n "$NAMESPACE" -s "$SIG" < "$MANIFEST" >/dev/null \
        || die "manifest SIGNATURE verification FAILED — do not join this net"
    log "signature OK"

    # Parse the pinned identity.
    PIN_GEN="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["genesis_hash"])' "$MANIFEST")"
    PIN_PAR="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["consensus_params_hash"])' "$MANIFEST")"
    PIN_HV="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["header_version"])' "$MANIFEST")"
    PIN_A4="$(python3 -c 'import json,sys;print(str(json.load(open(sys.argv[1]))["palw_algo4_accept"]).lower())' "$MANIFEST")"
    PIN_NET="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["network_id"])' "$MANIFEST")"
    # `commit` is .get()-read, not indexed: a manifest written before this field
    # existed simply carries no source-revision binding — handled explicitly
    # below (warn / release-fatal), never silently treated as verified.
    PIN_COMMIT="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("commit",""))' "$MANIFEST")"

    [ "$PIN_NET" = "$NETWORK" ] || die "manifest is for network '$PIN_NET' but this host is configured for '$NETWORK'"
    if [ "$RELEASE_MODE" = "1" ]; then
        log "verify mode: RELEASE (PALW_RELEASE_MODE=1) — the on-disk binaries are re-hashed and every check that cannot be performed is fatal"
    else
        log "verify mode: normal (set PALW_RELEASE_MODE=1 for the release gate) — the PASS line below states exactly what was checked"
    fi

    # STN-01(c) — LIVE commit binding. WHICH VARIANT IS IMPLEMENTED: the real
    # comparison, because the running node DOES already expose its source
    # revision — getConsensusIdentity returns git_commit (rpc/service/src/service.rs
    # -> GetConsensusIdentityResponse.git_commit), which `kaspa-pq-validator status`
    # prints as `node_git_commit` and which `generate` above pins verbatim. No new
    # RPC is invented; the identity blob already read for genesis/params/header-
    # version/algo4 carries it. A live/pinned mismatch is fatal for every node.
    # A node built OUTSIDE a git checkout reports the literal "unknown" — that is
    # not a binding, so it degrades to a loud warning here and dies in release mode.
    COMMIT_UNBOUND=""   # accumulates the node labels whose commit could not be read

    # Compare each LIVE node against the pin (review §11.4 — each mismatch fatal).
    for n in a b; do
        ID="$(_identity "$n")"
        GEN="$(_ident_field "$ID" node_genesis_hash)"
        PAR="$(_ident_field "$ID" node_params_hash)"
        HV="$(_ident_field "$ID" node_header_version_effective)"
        A4="$(_ident_field "$ID" node_palw_algo4_accept)"
        GIT="$(_ident_field "$ID" node_git_commit)"
        [ -n "$GEN" ] || die "node-$n does not serve getConsensusIdentity (older binary) — every node in a shared net must serve its identity"
        [ "$GEN" = "$PIN_GEN" ] || die "node-$n GENESIS mismatch: live=$GEN pinned=$PIN_GEN — different chain; do not proceed"
        [ "$PAR" = "$PIN_PAR" ] || die "node-$n PARAMS-HASH mismatch: live=$PAR pinned=$PIN_PAR — different consensus rules; do not proceed"
        [ "$HV" = "$PIN_HV" ]   || die "node-$n header-version mismatch: live=$HV pinned=$PIN_HV"
        [ "$A4" = "$PIN_A4" ]   || die "node-$n palw_algo4_accept mismatch: live=$A4 pinned=$PIN_A4 — one side would accept blocks the other rejects"
        if [ -n "$PIN_COMMIT" ] && [ "$PIN_COMMIT" != "unknown" ]; then
            if [ -z "$GIT" ] || [ "$GIT" = "unknown" ]; then
                warn "node-$n reports no git commit (node_git_commit='${GIT:-<absent>}') — the manifest's commit binding CANNOT be checked against this node"
                COMMIT_UNBOUND="$COMMIT_UNBOUND node-$n"
            elif [ "$GIT" != "$PIN_COMMIT" ]; then
                die "node-$n COMMIT mismatch: live=$GIT pinned=$PIN_COMMIT — this node runs a DIFFERENT source revision than the signed release; do not proceed"
            fi
        fi
        log "node-$n identity matches the signed manifest (genesis/params/header-version/algo4)"
    done

    # Resolve the commit-binding verdict once, for the PASS line (and release gate).
    if [ -z "$PIN_COMMIT" ] || [ "$PIN_COMMIT" = "unknown" ]; then
        COMMIT_STATUS="ABSENT (the manifest pins no usable commit: '${PIN_COMMIT:-<empty>}') — source revision NOT bound"
        warn "the signed manifest carries no usable 'commit' — this verification does NOT bind the release source revision"
        if [ "$RELEASE_MODE" = "1" ]; then
            die "PALW_RELEASE_MODE=1: a release manifest MUST pin the node's git commit. Rebuild the nodes inside a git checkout (so getConsensusIdentity reports node_git_commit), restart them, and re-run ./network-manifest.sh generate."
        fi
    elif [ -n "$COMMIT_UNBOUND" ]; then
        COMMIT_STATUS="UNVERIFIABLE (pinned $PIN_COMMIT; node(s)$COMMIT_UNBOUND report no git commit)"
        if [ "$RELEASE_MODE" = "1" ]; then
            die "PALW_RELEASE_MODE=1: node(s)$COMMIT_UNBOUND do not expose their source revision (node_git_commit is empty/'unknown'). A node in a release net MUST expose its commit — rebuild those nodes inside a git checkout, restart them, and re-verify."
        fi
    else
        COMMIT_STATUS="VERIFIED (both nodes live at $PIN_COMMIT)"
    fi

    # -------------------------------------------------------------------------
    # Binary identity (STN-01 a/b). The PASS line always states WHICH mode ran.
    #   RELEASE (PALW_RELEASE_MODE=1): artifacts/binary-hashes.txt is a plain text
    #     file anyone can edit, so it is NOT trusted as the source of truth —
    #     re-hash the ACTUAL on-disk release binaries load_env bound (KASPAD/VAL/
    #     MINER: kaspad, kaspa-pq-validator, misaminer — exactly build-and-hash.sh's
    #     attested set; mock-ticket is a controller-only helper and is deliberately
    #     NOT part of the node attestation) and compare THOSE to the pin. The
    #     recorded file must also be present and agree: a stale attestation means
    #     someone rebuilt without re-running build-and-hash.sh, which is an
    #     incident in a release net.
    #   NON-RELEASE: compare the recorded file (and say the binaries were not
    #     re-hashed). If the file is absent the binary check did NOT run — loud
    #     warning, and the PASS line reports the verification as INCOMPLETE.
    # -------------------------------------------------------------------------
    HASHES_FILE="$PALW_DATA_ROOT/artifacts/binary-hashes.txt"
    if [ "$RELEASE_MODE" = "1" ]; then
        # sha256 tool: sha256sum (GNU coreutils) or `shasum -a 256` (BSD / stock
        # macOS) — same detection build-and-hash.sh uses; require_cmd supplies the
        # harness's standard fail-closed message when neither spelling exists.
        if command -v sha256sum >/dev/null 2>&1; then
            SHA256_TOOL="sha256sum"
        else
            require_cmd shasum
            SHA256_TOOL="shasum -a 256"
        fi
        [ -s "$HASHES_FILE" ] || die "PALW_RELEASE_MODE=1 but the STN-001 attestation is missing/empty: $HASHES_FILE — run ./build-and-hash.sh; a release verify may not skip the binary check"
        ONDISK=""
        for _bin in "$KASPAD" "$VAL" "$MINER"; do
            [ -r "$_bin" ] || die "release binary not readable: $_bin — cannot re-hash the on-disk build"
            _bh="$($SHA256_TOOL "$_bin" 2>/dev/null | awk 'NR==1{print $1}' | tr 'A-F' 'a-f')"
            case "$_bh" in ''|*[!0-9a-f]*) die "sha256 of $_bin is not hex (tool: $SHA256_TOOL, got: '$_bh')" ;; esac
            [ "${#_bh}" -eq 64 ] || die "sha256 of $_bin has wrong length (${#_bh} != 64): '$_bh'"
            ONDISK="$ONDISK$_bh  $(basename "$_bin")"$'\n'
        done
        _binary_set_match "on-disk binaries under $REPO_ROOT/target/release" "$ONDISK"
        _binary_set_match "recorded $HASHES_FILE" "$(cat "$HASHES_FILE")"
        BIN_MODE="RE-HASHED on-disk binaries (kaspad/kaspa-pq-validator/misaminer) + recorded binary-hashes.txt cross-checked"
        log "on-disk release binaries re-hashed and strict-set-matched against the signed manifest (release mode)"
    elif [ -s "$HASHES_FILE" ]; then
        _binary_set_match "recorded $HASHES_FILE" "$(cat "$HASHES_FILE")"
        BIN_MODE="recorded binary-hashes.txt ONLY (binaries not re-hashed; set PALW_RELEASE_MODE=1 to hash the real files)"
        log "recorded binary hashes strict-set-match the signed manifest (file compare — the binaries themselves were NOT re-hashed)"
    else
        warn "no binary-hashes.txt at $HASHES_FILE — the binary check DID NOT RUN, so this verification is INCOMPLETE (run ./build-and-hash.sh; under PALW_RELEASE_MODE=1 this is fatal and the on-disk binaries are hashed directly)"
        BIN_MODE="NOT CHECKED — INCOMPLETE (no binary-hashes.txt)"
    fi

    # The PASS line enumerates exactly what was verified; it never says more than
    # was actually done (STN-01), and it keeps naming what this tool cannot do.
    # Even the verdict WORD is qualified when a check could not run — outside
    # release mode that is allowed to happen, but it may never read as a clean PASS.
    VERDICT="PASS"
    case "$BIN_MODE" in *"NOT CHECKED"*) VERDICT="PASS (INCOMPLETE)" ;; esac
    case "$COMMIT_STATUS" in ABSENT*|UNVERIFIABLE*) VERDICT="PASS (INCOMPLETE)" ;; esac
    log "manifest verification $VERDICT — verified: signature (allowed-signers $SIGNERS) + node-a/node-b LIVE identity (genesis/params-hash/header-version/algo4) + binaries [$BIN_MODE] + commit-binding [$COMMIT_STATUS]. NOT verified: remote attestation — nothing here proves a node EXECUTES the hashed binary."
    ;;

show)
    [ -s "$MANIFEST" ] || die "manifest not found: $MANIFEST"
    cat "$MANIFEST"
    ;;
esac
exit 0
