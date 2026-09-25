#!/bin/bash
# deploy-t12/distribute-from-mac.sh — run on the operator's Mac (it has ssh to all four hosts; the
# hosts have no keys to each other, and none is added). Every transfer is verified by sha256 on the
# Mac, and again by `install-<host>.sh stage` on the host.
#
#   ./distribute-from-mac.sh kit        push this kit to /root/t12-rel/kit on 5.104, .113, ibm
#   ./distribute-from-mac.sh binaries   the release binaries → $INCOMING/$REV on the hosts that need them:
#                                       - the Mac's own build (build-release-local.sh → .cache/$REV, marked by
#                                         its BUILD-INFO) → 5.104, .113, ibm   [preferred, PLAN.md §4]
#                                       - else 5.104:/root/t12-rel/incoming/$REV (build-release-5104.sh) → Mac
#                                         (cache) → ibm, .113                   [fallback]
#                                       BIN_SOURCE=local|5104 forces one. Either way the release's
#                                       misaka-dnsseeder is in the Mac cache, for seeders/20-stage.sh if the
#                                       operator chooses to swap seeders.
#   ./distribute-from-mac.sh artifact   5.104's 8k artifact + sidecar → Mac → ibm, .113 as *.incoming
#                                       (the existing *.palwart.part files there are left alone)
set -euo pipefail
KIT=$(cd "$(dirname "$0")" && pwd)
. "$KIT/fleet.env"
CACHE=${CACHE:-$KIT/.cache}
KEYOPT="ssh -o IdentitiesOnly=yes -i $HOME/.ssh/claude_key"
say() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
die() { say "ABORT: $*"; exit 1; }

# host → rsync -e command and ssh target
rsh_of() { case $1 in ibm) echo "ssh";; *) echo "$KEYOPT";; esac; }
tgt_of() { case $1 in 5104) echo root@5.104.81.23;; 113) echo root@169.58.232.113;; ibm) echo misaka-ibm;; 95111) echo root@95.111.236.186;; esac; }
on()   { local h=$1; shift; $(rsh_of "$h") "$(tgt_of "$h")" "$@"; }
push() { local h=$1 src=$2 dst=$3; rsync -a --partial -e "$(rsh_of "$h")" "$src" "$(tgt_of "$h"):$dst"; }
pull() { local h=$1 src=$2 dst=$3; rsync -a --partial -e "$(rsh_of "$h")" "$(tgt_of "$h"):$src" "$dst"; }
lsha() { shasum -a 256 "$1" | cut -d' ' -f1; }
need_rev() { [ "$REV" != __FILL_ME__ ] || die "fill REV and the sha256s in fleet.env first"; }

case "${1:-}" in
kit)
    for h in 5104 113 ibm; do
        on "$h" "mkdir -p $REL_ROOT/kit $INCOMING $STATE_DIR"
        push "$h" "$KIT/fleet.env" "$REL_ROOT/kit/"
        push "$h" "$KIT/lib.sh" "$REL_ROOT/kit/"
        push "$h" "$KIT/t12check.py" "$REL_ROOT/kit/"
        case $h in
            5104)  push "$h" "$KIT/install-5104.sh" "$REL_ROOT/kit/"; push "$h" "$KIT/build-release-5104.sh" "$REL_ROOT/kit/" ;;
            113)   push "$h" "$KIT/install-113.sh" "$REL_ROOT/kit/" ;;
            ibm)   push "$h" "$KIT/install-ibm.sh" "$REL_ROOT/kit/" ;;
        esac
        on "$h" "chmod 0755 $REL_ROOT/kit/*.sh $REL_ROOT/kit/t12check.py; ls -l $REL_ROOT/kit"
        say "kit → $h"
    done ;;
binaries)
    need_rev
    local_build=0
    grep -qx 'builder=build-release-local.sh' "$CACHE/$REV/BUILD-INFO" 2>/dev/null && local_build=1
    src=${BIN_SOURCE:-auto}
    [ "$src" != auto ] || { [ "$local_build" = 1 ] && src=local || src=5104; }
    case $src in
    local)
        [ "$local_build" = 1 ] || die "no build-release-local.sh output in $CACHE/$REV — run ./build-release-local.sh <commit> first"
        grep -qx "rev12=$REV" "$CACHE/$REV/BUILD-INFO" || die "$CACHE/$REV/BUILD-INFO is not rev $REV"
        targets=(5104 113 ibm)
        say "binaries: the Mac's own release build $CACHE/$REV ($(sed -n 's/^built_utc=//p' "$CACHE/$REV/BUILD-INFO")) → 5.104, .113, ibm" ;;
    5104)
        [ "$local_build" = 0 ] || die "$CACHE/$REV holds a build-release-local.sh build — pulling 5.104's over it would mix two builds (move it away, or BIN_SOURCE=local)"
        mkdir -p "$CACHE/$REV"
        pull 5104 "$INCOMING/$REV/" "$CACHE/$REV/"
        targets=(113 ibm) ;;
    *) die "BIN_SOURCE must be local or 5104" ;;
    esac
    for b in kaspad:$KASPAD_SHA256 misaka:$MISAKA_SHA256 palw-class:$PALW_CLASS_SHA256; do
        [ "$(lsha "$CACHE/$REV/${b%%:*}")" = "${b#*:}" ] || die "${b%%:*} sha256 on the Mac differs from fleet.env"
    done
    list=(kaspad misaka palw-class)
    if [ "$SEEDER_SHA256" != KEEP ]; then
        [ "$(lsha "$CACHE/$REV/misaka-dnsseeder")" = "$SEEDER_SHA256" ] || die "misaka-dnsseeder sha256 differs"
        say "release seeder is at $CACHE/$REV/misaka-dnsseeder — stage it with seeders/20-stage.sh"
    fi
    for h in "${targets[@]}"; do
        on "$h" "mkdir -p $INCOMING/$REV"
        for b in "${list[@]}"; do push "$h" "$CACHE/$REV/$b" "$INCOMING/$REV/"; done
        # the local build's provenance travels with it (install-*.sh reads only the binaries)
        if [ "$src" = local ]; then
            for f in BUILD-INFO SHA256SUMS IDENTITY; do
                if [ -f "$CACHE/$REV/$f" ]; then push "$h" "$CACHE/$REV/$f" "$INCOMING/$REV/"; fi
            done
        fi
        on "$h" "cd $INCOMING/$REV && sha256sum ${list[*]}"
        say "binaries → $h"
    done
    ;;
artifact)
    mkdir -p "$CACHE/art"
    pull 5104 "$ART_8K" "$CACHE/art/"
    pull 5104 "$ART_8K.palwmanifest" "$CACHE/art/"
    a="$CACHE/art/$(basename "$ART_8K")"
    [ "$(lsha "$a")" = "$ART_8K_SHA256" ] || die "8k artifact sha256 on the Mac differs"
    [ "$(lsha "$a.palwmanifest")" = "$MANIFEST_8K_SHA256" ] || die "8k sidecar sha256 differs"
    for h in 113 ibm; do
        if on "$h" "test -f $ART_8K && [ \$(stat -c %s $ART_8K) = $ART_8K_BYTES ]"; then say "$h already has $ART_8K (stage verifies its sha)"; continue; fi
        on "$h" "df -BG --output=avail /root/palw-class | tail -1"
        push "$h" "$a" "$ART_8K.incoming"
        push "$h" "$a.palwmanifest" "$ART_8K.palwmanifest.incoming"
        say "8k artifact → $h (as .incoming; stage promotes it)"
    done ;;
*) sed -n '2,16p' "$0"; exit 2 ;;
esac
