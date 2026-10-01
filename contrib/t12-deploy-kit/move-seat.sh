#!/bin/bash
# deploy-t12/move-seat.sh — run on the operator's Mac: move ONE seat from one host to another, end to end
# (2026-10-01, the panel backlog: docs/design/palw/t12-panel-backlog-1001.md §7). The hosts have no keys to each
# other, so data and the key travel through the Mac; each host-side step is move-host.sh (which uses the live kit's
# lib.sh and changes none of its files). ONE SEAT AT A TIME, after the previous one is synced and its readiness proof
# is back.
#
#   ./move-seat.sh plan               print the moves and what each step runs; touches nothing
#   ./move-seat.sh run <id>           the seat end to end; asks before every state-changing step (MOVE_YES=1: no asking)
#   ./move-seat.sh rollback <id>      target: unadd (stop, unit removed, appdir aside) → source: unretire (start the old unit)
#   ./move-seat.sh status <id>        read-only: both ends' unit state and the bond's processes
# env: DRY_RUN=1 prints every command without running one; MOVE_MODE=copy|fresh (default: move-nodes.sh MOVE_MODE_OF);
#      MOVE_READY_TIMEOUT=3600; UPGRADE_SYNC_TIMEOUT=900 (3600 for fresh); CACHE=<dir with ≥ 6 GB> for the copy mode.
#
# copy  — the appdir (≈ 4.7 GB: datadir 1.9 GB, retention 2.5 GB, panel state) goes source → Mac → target while the seat
#         is stopped, and is verified by a second `rsync -c` pass; the seat resumes where it stopped.
# fresh — only palw-panel/ (the rolling fee outpoint, 2 KB) is copied; the target syncs the chain from its peers (IBD)
#         and the retention copies of the seat's old materials are not carried over (a seat re-verifies a foreign claim
#         by replaying it; DA answers for claims it covered come from the other covering signers). Used where the link is
#         too slow for the copy: Mac → .113 measured 0.64 MB/s on 2026-10-01 (4.7 GB ≈ 2 h); Mac → ibm 4.7 MB/s (≈ 17 min).
set -euo pipefail
KIT=$(cd "$(dirname "$0")" && pwd)
. "$KIT/fleet.env"
. "$KIT/move-nodes.sh"
CACHE=${CACHE:-$KIT/.cache}
DRY_RUN=${DRY_RUN:-0}
KEYOPT="ssh -o IdentitiesOnly=yes -i $HOME/.ssh/claude_key"
say() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
die() { say "ABORT: $*"; exit 1; }

rsh_of() { case $1 in ibm) echo "ssh" ;; *) echo "$KEYOPT" ;; esac; }
tgt_of() { case $1 in 5104) echo root@5.104.81.23 ;; 113) echo root@169.58.232.113 ;; ibm) echo misaka-ibm ;; esac; }
on()   { local h=$1; shift; if [ "$DRY_RUN" = 1 ]; then say "  DRY-RUN [$h]: $*"; return 0; fi; $(rsh_of "$h") "$(tgt_of "$h")" "$@"; }
rs()   { local h=$1; shift; if [ "$DRY_RUN" = 1 ]; then say "  DRY-RUN: rsync [$h] $*"; return 0; fi; rsync -a --partial -e "$(rsh_of "$h")" "$@"; }
HOSTKIT=$REL_ROOT/kit

# default data mode per move (the link's speed decides): .113 is reached over 0.64 MB/s
MOVE_MODE_OF() { case $1 in 113) echo fresh ;; *) echo copy ;; esac; }

lookup() { # <id> → sets M_ID M_FROM M_TO M_MODE
    local m
    for m in "${MOVES[@]}"; do
        IFS='|' read -r M_ID M_FROM M_TO <<<"$m"
        if [ "$M_ID" = "$1" ]; then M_MODE=${MOVE_MODE:-$(MOVE_MODE_OF "$M_TO")}; return 0; fi
    done
    die "b$1 is not in move-nodes.sh MOVES (${MOVES[*]})"
}
ask() { # <what the next step does>
    say "NEXT: $*"
    [ "${MOVE_YES:-0}" = 1 ] || [ "$DRY_RUN" = 1 ] || { read -r -p "  proceed? [y/N] " a; [ "$a" = y ] || die "stopped by the operator (nothing after this step ran)"; }
}
appdir_of() { echo "${APPDIR_PREFIX}$1"; }
key_of() { echo "$KEY_DIR/t12-bond-$1.key"; }

plan() {
    local m
    say "moves (move-nodes.sh): ${MOVES[*]}"
    for m in "${MOVES[@]}"; do
        IFS='|' read -r M_ID M_FROM M_TO <<<"$m"
        say "  b$M_ID: $M_FROM → $M_TO, data mode ${MOVE_MODE:-$(MOVE_MODE_OF "$M_TO")}, target spec: $(move_target_spec "$M_ID" "$M_TO")"
    done
    cat <<EOF
steps of \`run <id>\` (one seat at a time):
  1  push move-host.sh + move-nodes.sh to both hosts' $HOSTKIT (lib.sh / fleet.env / t12check.py there are the live kit's, untouched)
  2  preflight retire (source) and preflight add (target): release staged, artifacts, ports, memory, disk, 8k margin, public nodes up
  3  retire (source): stop the seat, marker + ConditionPathExists drop-in (cannot be started), disable; prints MOVE_READY_WANT
  4  verify: the source unit is inactive and no process holds the bond
  5  data: copy = rsync source → Mac → target (excluding logs/), then a checksum pass; fresh = palw-panel/ only
  6  key: piped host → host through the Mac (never written to the Mac), sha256 compared on both ends (the digest only)
  7  add (target): write_launch + unit for this node only, install, start on the copied appdir; gates: fingerprint, genesis,
     duties, synced + a peer, the 8k class's ready seats back to MOVE_READY_WANT
  8  check both ends; print the kit-table edit (NODES) to make and re-push
rollback: \`./move-seat.sh rollback <id>\` (target unadd, then source unretire). The source appdir is never touched.
EOF
}

run_seat() {
    local id=$1 want="" appdir key out sk tk mode
    lookup "$id"; mode=$M_MODE; appdir=$(appdir_of "$id"); key=$(key_of "$id")
    say "moving b$id: $M_FROM → $M_TO (data mode: $mode)"
    ask "push move-host.sh and move-nodes.sh to $M_FROM and $M_TO"
    for h in "$M_FROM" "$M_TO"; do
        on "$h" "mkdir -p $HOSTKIT"
        rs "$h" "$KIT/move-host.sh" "$KIT/move-nodes.sh" "$(tgt_of "$h"):$HOSTKIT/"
        on "$h" "chmod 0755 $HOSTKIT/move-host.sh; test -f $HOSTKIT/lib.sh -a -f $HOSTKIT/fleet.env -a -f $HOSTKIT/t12check.py && echo 'kit files present on $h' || echo 'ABORT: the live kit is not on $h'"
    done
    say "preflight (read-only)"
    on "$M_FROM" "cd $HOSTKIT && ./move-host.sh preflight retire $id"
    on "$M_TO" "cd $HOSTKIT && $([ "$mode" = fresh ] && echo 'MOVE_FRESH=1 ')./move-host.sh preflight add $id" || true
    ask "RETIRE b$id on $M_FROM: the seat stops and cannot be started there (its appdir and key stay: the rollback)"
    out=$(on "$M_FROM" "cd $HOSTKIT && CONFIRM_MOVE_SEAT=yes MOVE_TO=$M_TO ./move-host.sh retire $id" | tee /dev/stderr) || die "retire failed — nothing was copied; \`./move-seat.sh rollback $id\` if the unit is stopped"
    want=$(sed -n 's/^MOVE_READY_WANT=//p' <<<"$out" | tail -1)
    say "source stopped; the 8k class had $want ready seats before the retire"
    on "$M_FROM" "systemctl is-active misaka-t12-seat$id || true; pgrep -af -- '--palw-producer-bond=.*:$id( |\$)' | grep -v pgrep || echo 'no process holds bond $id on $M_FROM'"
    ask "COPY the data ($mode) $M_FROM → Mac → $M_TO"
    if [ "$mode" = copy ]; then
        mkdir -p "$CACHE/seat-b$id"
        rs "$M_FROM" --exclude='/misaka-testnet-12/logs/' "$(tgt_of "$M_FROM"):$appdir/" "$CACHE/seat-b$id/"
        on "$M_TO" "mkdir -p $appdir"
        rs "$M_TO" --exclude='/misaka-testnet-12/logs/' "$CACHE/seat-b$id/" "$(tgt_of "$M_TO"):$appdir/"
        say "verify: a checksum pass Mac → $M_TO must list nothing"
        if [ "$DRY_RUN" != 1 ]; then
            diffs=$(rsync -rnc --delete --exclude='/misaka-testnet-12/logs/' --exclude='/misaka-testnet-12/logs' -e "$(rsh_of "$M_TO")" "$CACHE/seat-b$id/" "$(tgt_of "$M_TO"):$appdir/" | grep -v '/$' | head -5 || true)
            [ -z "$diffs" ] || die "the copy on $M_TO differs from the source's (first lines: $diffs) — do not start it. Re-run the rsync, or rollback"
            say "  identical ($(du -sh "$CACHE/seat-b$id" | cut -f1))"
        fi
    else
        mkdir -p "$CACHE/seat-b$id/misaka-testnet-12"
        rs "$M_FROM" "$(tgt_of "$M_FROM"):$appdir/misaka-testnet-12/palw-panel" "$CACHE/seat-b$id/misaka-testnet-12/"
        on "$M_TO" "mkdir -p $appdir/misaka-testnet-12"
        rs "$M_TO" "$CACHE/seat-b$id/misaka-testnet-12/palw-panel" "$(tgt_of "$M_TO"):$appdir/misaka-testnet-12/"
        say "  palw-panel/ copied; $M_TO syncs the chain from its peers"
    fi
    ask "COPY the bond key b$id (a pipe: $M_FROM → $M_TO, never on the Mac's disk) and compare sha256 on both ends"
    if [ "$DRY_RUN" = 1 ]; then say "  DRY-RUN: ssh $M_FROM cat $key | ssh $M_TO 'umask 077; cat > $key.incoming'; sha256 compare; mv"; else
        $(rsh_of "$M_FROM") "$(tgt_of "$M_FROM")" "cat $key" | $(rsh_of "$M_TO") "$(tgt_of "$M_TO")" "umask 077; mkdir -p $KEY_DIR; cat > $key.incoming"
        sk=$(on "$M_FROM" "sha256sum < $key | cut -d' ' -f1"); tk=$(on "$M_TO" "sha256sum < $key.incoming | cut -d' ' -f1")
        [ "$sk" = "$tk" ] && [ -n "$sk" ] || { on "$M_TO" "rm -f $key.incoming"; die "the key's sha256 differs between the hosts — removed the incoming copy; nothing started"; }
        on "$M_TO" "chmod 0600 $key.incoming && mv -f $key.incoming $key && ls -l $key | awk '{print \$1, \$5\"B\", \$9}'"
        say "  key copied, digests equal ($(printf %s "$sk" | cut -c1-8)…)"
    fi
    ask "ADD b$id on $M_TO: stage + install the unit and START it (the old host's unit is stopped and cannot start)"
    on "$M_TO" "cd $HOSTKIT && $([ "$mode" = fresh ] && echo 'MOVE_FRESH=1 ')CONFIRM_SOURCE_STOPPED=yes MOVE_READY_WANT=$want MOVE_READY_TIMEOUT=${MOVE_READY_TIMEOUT:-3600} UPGRADE_SYNC_TIMEOUT=${UPGRADE_SYNC_TIMEOUT:-$([ "$mode" = fresh ] && echo 3600 || echo 900)} ./move-host.sh add $id" \
        || die "add failed on $M_TO — \`./move-seat.sh rollback $id\` (unadd the target, unretire the source)"
    on "$M_TO" "cd $HOSTKIT && ./move-host.sh check $id" || true
    say "DONE b$id on $M_TO. Remaining: edit the kit tables (b$id out of install-$M_FROM.sh NODES, into install-$M_TO.sh NODES with spec: $(move_target_spec "$id" "$M_TO")), re-push the kit; leave $appdir on $M_FROM for a day; THEN the next seat."
}

rollback_seat() {
    local id=$1
    lookup "$id"
    ask "ROLLBACK b$id: stop and remove it on $M_TO, start the old unit on $M_FROM"
    on "$M_TO" "cd $HOSTKIT && ./move-host.sh unadd $id" || say "(nothing to undo on $M_TO)"
    on "$M_FROM" "cd $HOSTKIT && CONFIRM_TARGET_STOPPED=yes ./move-host.sh unretire $id"
}

status_seat() {
    local id=$1 h
    lookup "$id"
    for h in "$M_FROM" "$M_TO"; do
        say "== $h"
        on "$h" "systemctl is-active misaka-t12-seat$id 2>&1 || true; ls $(key_of "$id") $(appdir_of "$id") 2>&1 | head -3; pgrep -af -- '--palw-producer-bond=.*:$id( |\$)' | grep -v pgrep | cut -c1-100 || echo 'no process holds bond $id'; cat $STATE_DIR/moved-b$id $STATE_DIR/moved-in-b$id 2>/dev/null || true"
    done
}

case "${1:-}" in
    plan)     plan ;;
    run)      run_seat "${2:?bond id}" ;;
    rollback) rollback_seat "${2:?bond id}" ;;
    status)   status_seat "${2:?bond id}" ;;
    *) sed -n '2,24p' "$0"; exit 2 ;;
esac
