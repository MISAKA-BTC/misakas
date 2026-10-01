#!/bin/bash
# deploy-t12/move-host.sh — run as root ON A HOST, from $REL_ROOT/kit, to move one seat between hosts (2026-10-01, the
# panel backlog: docs/design/palw/t12-panel-backlog-1001.md §7). move-seat.sh (the Mac) drives it; every command here is
# also safe to run by hand. Uses the kit's lib.sh (the live one: RID, Phase H) — it changes none of its files.
#
#   ./move-host.sh preflight retire <id>   read-only, on the SOURCE: unit, release, other hosts up, the 8k class's margin
#   ./move-host.sh preflight add <id>      read-only, on the TARGET: release staged, artifacts, ports, memory, disk, key
#   ./move-host.sh retire <id>             SOURCE: stop the seat, make its unit UNSTARTABLE, disable it (CONFIRM_MOVE_SEAT=yes)
#   ./move-host.sh add <id>                TARGET: stage + install the unit, start on the copied appdir, gate it
#                                          (CONFIRM_SOURCE_STOPPED=yes; MOVE_READY_WANT=<n> from retire's output;
#                                          MOVE_FRESH=1: only palw-panel/ was copied, the node syncs from its peers)
#   ./move-host.sh unadd <id>              TARGET rollback: stop, remove the unit, move the appdir aside
#   ./move-host.sh unretire <id>           SOURCE rollback: remove the marker, start the old unit (CONFIRM_TARGET_STOPPED=yes)
#   ./move-host.sh check <id>              read-only: the node's own check
#
# THE RULE: one bond is never in two processes (a second process signs its round permits and answers its seat duties
# beside the first — lib.sh prints the same warning at start). So `retire` refuses to finish while any process on the host
# still holds the bond, writes a marker the unit's ConditionPathExists=! honours (a `systemctl start`, a reboot or an
# `upgrade` of the old host cannot start it), and `add` needs the operator's word that the old unit is stopped.
# Keys are never read here: the 64-byte key is copied by move-seat.sh over a pipe, and `add` only `test -f`s it.
. "$(dirname "$0")/lib.sh"
. "$KIT_DIR/move-nodes.sh"
DRY_RUN=${DRY_RUN:-0}

case "$(hostname)" in vmi3272359) HOST_KEY=5104 ;; vmi3527497) HOST_KEY=113 ;; vmi3450148) HOST_KEY=ibm ;; *) HOST_KEY="" ;; esac
[ -n "$HOST_KEY" ] || die "unknown host $(hostname) — not one of the three t12 hosts"
[ "$(id -u)" = 0 ] || die "run as root"

moved_mark()    { echo "$STATE_DIR/moved-b$1"; }
moved_in_mark() { echo "$STATE_DIR/moved-in-b$1"; }
moved_dropin()  { echo "/etc/systemd/system/$N_UNIT.service.d/zzz-moved-b$N_ID.conf"; }
move_log()      { mkdir -p "$STATE_DIR"; [ "$DRY_RUN" = 1 ] && { say "  DRY-RUN: log: $*"; return 0; }; echo "$(date -u +%FT%TZ) $*" >> "$STATE_DIR/move-b$N_ID.log"; }

# every process on this host that holds bond <id>, one per line (never two, never one on the old host after retire)
bond_procs() {
    { pgrep -af -- "--palw-producer-bond=$PREMINE_TXID:$1( |\$)" 2>/dev/null || true; } | grep -v 'pgrep' | cut -c1-140
}

# the 8k class's ready seats, now/required, from the node's own state line
ready_of() { st_get "$(node_state "${EXPECT_ID_ARGS[@]}")" ready; }

require_public_nodes_up() {
    local hp
    for hp in "${MOVE_REQUIRE_UP[@]}"; do
        tcp_open "$hp" || die "$1: the public node $hp does not take a connection — move one seat at a time and only while the three public nodes (b6, b0, b1) are up (the 8k class keeps a margin of one ready seat)"
    done
}

# ---------------------------------------------------------------------------------------------
# SOURCE
# ---------------------------------------------------------------------------------------------
retire_checks() { # <id> — read-only; parses the source spec
    local id=$1 spec eff rd
    spec=$(move_source_spec "$id" "$HOST_KEY") || die "b$id is not a seat this host ($HOST_KEY) moves away (move-nodes.sh)"
    parse_node "$spec"; require_release
    systemctl cat "$N_UNIT" >/dev/null 2>&1 || die "no unit $N_UNIT here"
    eff=$(systemctl show -p ExecStart --value "$N_UNIT" 2>/dev/null || true)
    case "$eff" in *"/launch/b$id.sh"*) ;; *) die "$N_UNIT's ExecStart does not name a kit launch script b$id.sh (${eff:0:160}) — move-nodes.sh does not describe this node; nothing was stopped" ;; esac
    [ ! -e "$(moved_mark "$id")" ] || die "b$id was already retired here: $(cat "$(moved_mark "$id")")"
    require_public_nodes_up "b$id"
    t12check_expect
    N_ST=$(node_state "${EXPECT_ID_ARGS[@]}")
    say "b$id ($N_UNIT) now: ${N_ST#STATE }"
    rd=$(st_get "$N_ST" ready)
    if [ "$N_SEAT8K" = 1 ] && [[ "$rd" =~ ^([0-9]+)/([0-9]+)$ ]] && [ "${BASH_REMATCH[1]}" -le "${BASH_REMATCH[2]}" ]; then
        [ "${MOVE_READY_OK:-0}" = 1 ] || die "the 8k class has $rd ready seats: with b$id away for the whole move it is below its requirement. Wait until it shows one more than required, or MOVE_READY_OK=1"
    fi
    N_READY=$rd
}

cmd_retire() {
    local id=$1 res
    [ "${CONFIRM_MOVE_SEAT:-}" = yes ] || die "retire stops seat b$id and makes its unit unstartable here (its keys and data go to another host). CONFIRM_MOVE_SEAT=yes"
    retire_checks "$id"
    local mark; mark=$(moved_mark "$id")
    say "retiring b$id ($N_UNIT): stop, marker $mark, drop-in $(moved_dropin), disable"
    run mkdir -p "$STATE_DIR"
    if [ "$DRY_RUN" != 1 ]; then printf '%s\n' "$N_ST" > "$STATE_DIR/move-b$id.before"; fi
    move_log "RETIRE-START $N_UNIT before=${N_ST#STATE }"
    if [ "$DRY_RUN" = 1 ]; then say "  DRY-RUN: systemctl stop $N_UNIT   (SIGINT, up to 180 s)"; else
        stop_unit "$N_UNIT"
        res=$(systemctl show -p Result --value "$N_UNIT" 2>/dev/null || true)
        [ "$res" != timeout ] || warn "$N_UNIT was SIGKILLed after TimeoutStopSec (Result=timeout): its databases recover on the next start — the copy is of that state"
    fi
    local was_enabled; was_enabled=$(systemctl is-enabled "$N_UNIT" 2>/dev/null || echo none)
    if [ "$DRY_RUN" = 1 ]; then say "  DRY-RUN: write marker + drop-in; daemon-reload; disable"; else
        printf 'retired b%s (%s) on %s at %s; enabled=%s; moved to: %s\n' "$id" "$N_UNIT" "$(hostname)" "$TS" "$was_enabled" "${MOVE_TO:-?}" > "$mark"
        mkdir -p "/etc/systemd/system/$N_UNIT.service.d"
        atomic_write "$(moved_dropin)" 0644 <<EOF
# deploy-t12 MOVED: seat b$id ($N_UNIT) was moved to another host on $TS (move-host.sh retire).
# This drop-in keeps the unit from starting while the marker exists: one bond is never in two processes.
# Undo (only with the new host's unit stopped): move-host.sh unretire $id.
[Unit]
ConditionPathExists=!$mark
EOF
        systemctl daemon-reload
        systemctl disable "$N_UNIT" >/dev/null 2>&1 || true
    fi
    if [ "$DRY_RUN" != 1 ]; then
        systemctl is-active --quiet "$N_UNIT" && die "$N_UNIT is still active after retire"
        [ -z "$(bond_procs "$id")" ] || { bond_procs "$id" >&2; die "a process on this host still holds bond $id (above) — it must not run beside the copy; stop it, then re-run"; }
    fi
    move_log "RETIRE-DONE $N_UNIT enabled_before=$was_enabled"
    say "b$id retired: stopped, unit unstartable (marker), appdir $N_APPDIR untouched (the rollback)"
    echo "MOVE_READY_WANT=${N_READY%%/*}"
}

cmd_unretire() {
    local id=$1 mark since
    [ "${CONFIRM_TARGET_STOPPED:-}" = yes ] || die "unretire starts bond $id HERE again: the copy on the new host must be stopped first (move-host.sh unadd $id there). CONFIRM_TARGET_STOPPED=yes"
    local spec; spec=$(move_source_spec "$id" "$HOST_KEY") || die "b$id is not a seat this host moves away"
    parse_node "$spec"; require_release
    mark=$(moved_mark "$id")
    [ -e "$mark" ] || die "no marker $mark — b$id was not retired here"
    [ -z "$(bond_procs "$id")" ] || die "a process on this host already holds bond $id"
    run rm -f "$(moved_dropin)" "$mark"
    run systemctl daemon-reload
    if grep -q 'enabled=enabled' "$mark" 2>/dev/null; then run systemctl enable "$N_UNIT" >/dev/null 2>&1 || true; fi
    since=$(date +%s)
    run systemctl reset-failed "$N_UNIT" 2>/dev/null || true
    run systemctl start "$N_UNIT"
    move_log "UNRETIRE $N_UNIT"
    [ "$DRY_RUN" = 1 ] && return 0
    wait_fingerprint "$N_UNIT" "$since" || die "b$id did not come back on the release's fingerprint after unretire"
    upgrade_wait_synced "" warn || true
    say "b$id runs on $(hostname) again"
}

# ---------------------------------------------------------------------------------------------
# TARGET
# ---------------------------------------------------------------------------------------------
shares_running_mib() { # the --palw-host-memory-share of every running t12 kaspad here, MiB
    local u pid total=0 s
    for u in $(systemctl list-units --no-legend --state=active 'misaka-t12*' 2>/dev/null | awk '{print $1}'); do
        pid=$(systemctl show -p MainPID --value "$u" 2>/dev/null || echo 0)
        [ "${pid:-0}" != 0 ] || continue
        s=$(tr '\0' '\n' < "/proc/$pid/cmdline" 2>/dev/null | sed -n 's/^--palw-host-memory-share=//p' | head -1)
        [[ "$s" =~ ^[0-9]+$ ]] && total=$((total + s / 1048576))
    done
    echo "$total"
}

pinned_once_mib() { # int-10.2 A1: the class artifacts this host's seats pin, each ONCE for the host, MiB (0 before int-10.2)
    # A release whose kaspad pins (its --help knows --palw-no-artifact-pin) locks each artifact a replay reads in place:
    # one page-cache copy for the host however many seats lock it, outside every seat's --palw-host-memory-share. The
    # declared arithmetic therefore adds each distinct file once -- the 8k .palwart and, with Phase H, the IR container.
    "$REL/bin/kaspad" --help 2>/dev/null | grep -q -- '--palw-no-artifact-pin' || { echo 0; return; }
    local mib=$(( (ART_8K_BYTES + 1048575) / 1048576 ))
    [ -n "${ART_PH:-}" ] && mib=$((mib + (ART_PH_BYTES + 1048575) / 1048576))
    echo "$mib"
}

add_checks() { # <id> <hard: 1 = die on what add needs, 0 = warn (the key is copied after the preflight)>
    local id=$1 hard=$2 ok=1 port holder shares memtotal free_gb need
    local spec; spec=$(move_target_spec "$id" "$HOST_KEY") || die "b$id does not move to this host ($HOST_KEY) (move-nodes.sh)"
    parse_node "$spec"; require_release
    [ "$N_MODE" = new ] || die "b$id: a moved seat arrives as a NEW unit (mode new), not a drop-in"
    # int-10.2 D1: this host's pinner / host ledger configuration for the moved seat's launch script and unit — only
    # under a staged kaspad that knows the flags (an int-10.1 kaspad would refuse --palw-host-ledger-dir at its launch
    # check) and a kit whose lib.sh carries pinner-lib.sh (patch p4)
    if "$REL/bin/kaspad" --help 2>/dev/null | grep -q -- '--palw-host-pinner' && declare -F pinner_unit_deps >/dev/null; then
        move_host_memory_env "$HOST_KEY"
        say "  host memory configuration here: HOST_PINNER=${HOST_PINNER:-0} HOST_LEDGER_DIR=${HOST_LEDGER_DIR:-<off>}$(pinner_on && echo "; pinner unit $(systemctl is-active "$PINNER_UNIT" 2>/dev/null || true)")"
    fi
    flag() { if [ "$hard" = 1 ]; then die "$*"; else warn "$*"; ok=0; fi; }
    [ -x "$REL/bin/kaspad" ] && [ "$(sha_of "$REL/bin/kaspad")" = "$KASPAD_SHA256" ] || die "$REL/bin/kaspad is not the release's (sha $KASPAD_SHA256) — this host's release is not staged: \`./install-<host>.sh stage\` first (a moved seat runs the release the fleet runs)"
    if [ -f "$ART_8K" ] && [ "$(stat -c %s "$ART_8K")" = "$ART_8K_BYTES" ]; then say "  8k artifact present"; else flag "8k artifact $ART_8K missing or the wrong size (distribute-from-mac.sh artifact)"; fi
    if [ -n "${ART_PH:-}" ]; then [ -f "$ART_PH" ] && [ "$(stat -c %s "$ART_PH")" = "$ART_PH_BYTES" ] && say "  IR artifact present" || flag "IR artifact $ART_PH missing or the wrong size (distribute-from-mac.sh artifact-ph)"; fi
    if [ -f "$N_KEY" ]; then say "  key present: $(ls -l "$N_KEY" | awk '{print $1, $5"B", $9}')"; else flag "bond key $N_KEY is not here yet (move-seat.sh copies it after the data, over a pipe)"; fi
    if [ "${MOVE_FRESH:-0}" = 1 ]; then
        # fresh: only palw-panel/ (the rolling fee outpoint) was copied; the node syncs the chain from its peers
        if [ -d "$N_APPDIR/misaka-testnet-12/palw-panel" ] && [ ! -e "$N_APPDIR/misaka-testnet-12/datadir" ]; then say "  fresh appdir: palw-panel/ present, no datadir (the node syncs from its peers)"
        elif [ -e "$N_APPDIR/misaka-testnet-12/datadir" ]; then flag "MOVE_FRESH=1 but $N_APPDIR already holds a datadir — a fresh join starts without one"
        else flag "fresh appdir $N_APPDIR/misaka-testnet-12/palw-panel is not here yet (move-seat.sh copies it)"; fi
    elif [ -d "$N_APPDIR/misaka-testnet-12/datadir" ]; then say "  appdir present: $(du -sh "$N_APPDIR" | cut -f1) $N_APPDIR"
    elif [ -e "$N_APPDIR" ]; then flag "$N_APPDIR exists but holds no misaka-testnet-12/datadir — not a copy of the seat's appdir"
    else flag "appdir $N_APPDIR is not here yet (move-seat.sh copies it from the old host)"; fi
    [ ! -e "$N_APPDIR/misaka-testnet-12/palw-drill-genesis" ] || flag "$N_APPDIR carries a DRILL marker"
    if systemctl cat "$N_UNIT" >/dev/null 2>&1; then flag "unit $N_UNIT already exists on this host"; fi
    for port in "${N_LISTEN##*:}" "$N_BORSH" "$N_JSON"; do
        holder=$(ss -ltnpH "sport = :$port" 2>/dev/null | head -1 || true)
        [ -z "$holder" ] || flag "port $port is held (${holder:0:100})"
    done
    [ -z "$(bond_procs "$id")" ] || flag "a process on THIS host already holds bond $id"
    local pinned; pinned=$(pinned_once_mib)
    shares=$(shares_running_mib); memtotal=$(awk '/MemTotal/{print int($2/1024)}' /proc/meminfo); need=$((shares + N_SHARE + pinned + $(move_reserve_mib "$HOST_KEY")))
    say "  memory: running shares ${shares} MiB + b$id ${N_SHARE} + pinned artifacts (once for the host) ${pinned} + reserve $(move_reserve_mib "$HOST_KEY") = ${need} MiB of MemTotal ${memtotal} MiB"
    [ "$need" -le "$memtotal" ] || flag "shares + pinned artifacts + reserve ${need} MiB exceed MemTotal ${memtotal} MiB"
    free_gb=$(df -BG --output=avail "$(dirname "$N_APPDIR")" | tail -1 | tr -dc 0-9)
    [ "$free_gb" -ge 12 ] || flag "only ${free_gb} GB free next to $N_APPDIR (a seat appdir is ~5 GB; 12 GB keeps the rest of the host breathing)"
    require_public_nodes_up "b$id"
    [ "$ok" = 1 ] && say "preflight add b$id on $HOST_KEY: OK" || warn "preflight add b$id on $HOST_KEY: NOT OK (see warnings)"
}

cmd_add() {
    local id=$1 since want rd deadline
    [ "${CONFIRM_SOURCE_STOPPED:-}" = yes ] || die "add starts bond $id HERE. The unit on the old host must be stopped and retired (move-host.sh retire $id there; its marker makes it unstartable): one bond is never in two processes. CONFIRM_SOURCE_STOPPED=yes"
    add_checks "$id" 1
    [ ! -e "$(moved_in_mark "$id")" ] || die "b$id was already added here: $(cat "$(moved_in_mark "$id")")"
    say "adding b$id ($N_UNIT) on $HOST_KEY: stage launch script + unit, install, start on the copied appdir"
    if [ "$DRY_RUN" = 1 ]; then say "  DRY-RUN: write_launch; write_unit; $N_LAUNCH --check"; else
        write_launch; write_unit
        "$N_LAUNCH" --check || die "b$id launch check failed — nothing was installed"
    fi
    run install -m 0644 "$REL/units/$N_UNIT.service" "$(unit_file_path)"
    run mkdir -p "$STATE_DIR"
    if [ "$DRY_RUN" != 1 ]; then printf 'added b%s (%s) on %s at %s from %s\n' "$id" "$N_UNIT" "$(hostname)" "$TS" "$REL" > "$(moved_in_mark "$id")"; fi
    move_log "ADD $N_UNIT launch=$N_LAUNCH"
    run systemctl daemon-reload
    run systemctl enable "$N_UNIT" >/dev/null 2>&1 || true
    since=$(date +%s)
    run systemctl start "$N_UNIT"
    [ "$DRY_RUN" = 1 ] && { say "  DRY-RUN: then gate: fingerprint, genesis, duties, synced + a peer, the 8k ready seats back to MOVE_READY_WANT"; return 0; }
    wait_fingerprint "$N_UNIT" "$since" || die "b$id did not come up on EXPECT_FP (it was stopped) — \`move-host.sh unadd $id\` here, then \`unretire $id\` on the old host"
    wait_genesis "$N_UNIT" || die "b$id does not hold EXPECT_GENESIS (it was stopped) — unadd, then unretire"
    wait_duties "$N_UNIT" "$since"
    UPGRADE_SYNC_TIMEOUT=${UPGRADE_SYNC_TIMEOUT:-900}
    upgrade_wait_synced "" die
    # the seat proves its readiness again (its possession proof is carried on chain by its own node): the 8k class's
    # ready seats come back to what they were before the retire
    want=${MOVE_READY_WANT:-}
    if [[ "$want" =~ ^[0-9]+$ ]]; then
        deadline=$(( $(date +%s) + ${MOVE_READY_TIMEOUT:-3600} ))
        say "  waiting ≤ ${MOVE_READY_TIMEOUT:-3600}s for the 8k class's ready seats to be back at $want"
        while :; do
            rd=$(ready_of)
            if [[ "$rd" =~ ^([0-9]+)/([0-9]+)$ ]] && [ "${BASH_REMATCH[1]}" -ge "$want" ]; then say "  ready seats $rd — b$id's readiness proof is back"; break; fi
            if [ "$(date +%s)" -ge "$deadline" ]; then warn "ready seats are still ${rd:-?} (want $want) after ${MOVE_READY_TIMEOUT:-3600}s — b$id runs and is synced; read \`check\` and the journal ('readiness'); do not move the next seat until it is back"; break; fi
            systemctl is-active --quiet "$N_UNIT" || die "b$id exited while waiting for its readiness proof"
            sleep 20
        done
    else
        warn "no MOVE_READY_WANT: the readiness gate is skipped"
    fi
    move_log "ADD-DONE $N_UNIT"
    say "b$id runs on $HOST_KEY. NEXT: move its spec in the kit's node tables (install-5104.sh NODES → install-$HOST_KEY.sh NODES), re-push the kit; the old host's appdir stays until the seat has run a day"
}

cmd_unadd() {
    local id=$1 spec
    spec=$(move_target_spec "$id" "$HOST_KEY") || die "b$id does not move to this host"
    parse_node "$spec"; require_release
    say "undoing the add of b$id here: stop, remove the unit, move the appdir aside (never deleted)"
    if systemctl cat "$N_UNIT" >/dev/null 2>&1; then
        if [ "$DRY_RUN" = 1 ]; then say "  DRY-RUN: stop $N_UNIT"; else stop_unit "$N_UNIT"; fi
        run systemctl disable "$N_UNIT" >/dev/null 2>&1 || true
        if grep -q '^# deploy-t12 unit' "$(unit_file_path)" 2>/dev/null; then run rm -f "$(unit_file_path)"; fi
        run systemctl daemon-reload
    fi
    [ -z "$(bond_procs "$id")" ] || die "a process on this host still holds bond $id"
    if [ -e "$N_APPDIR" ]; then run mv "$N_APPDIR" "$N_APPDIR.moved-in-undone-$TS"; say "  appdir kept as $N_APPDIR.moved-in-undone-$TS"; fi
    run rm -f "$(moved_in_mark "$id")"
    move_log "UNADD $N_UNIT"
    say "b$id is not on $HOST_KEY any more; start it again on the old host: move-host.sh unretire $id (CONFIRM_TARGET_STOPPED=yes)"
}

cmd_check() {
    local id=$1 spec
    spec=$(move_target_spec "$id" "$HOST_KEY" 2>/dev/null || move_source_spec "$id" "$HOST_KEY") || die "b$id: no spec for host $HOST_KEY"
    parse_node "$spec"; require_release; t12check_expect
    say "== b$N_ID $N_UNIT: $(systemctl show -p ActiveState,SubState,NRestarts --value "$N_UNIT" | tr '\n' ' ')"
    python3 "$KIT_DIR/t12check.py" --port "$N_JSON" "${EXPECT_ARGS[@]}" --expect-panel ${CHECK_REGISTRY:+--registry} || true
    cgroup_memory "$N_UNIT"
}

main() {
    local cmd=${1:-help} id
    case "$cmd" in
        preflight)
            case "${2:-}" in
                retire) retire_checks "${3:?bond id}"; say "preflight retire b$3: OK" ;;
                add)    add_checks "${3:?bond id}" 0 ;;
                *) die "preflight retire|add <id>" ;;
            esac ;;
        retire)   cmd_retire "${2:?bond id}" ;;
        unretire) cmd_unretire "${2:?bond id}" ;;
        add)      cmd_add "${2:?bond id}" ;;
        unadd)    cmd_unadd "${2:?bond id}" ;;
        check)    cmd_check "${2:?bond id}" ;;
        help|-h|--help) sed -n '2,22p' "$0" ;;
        *) die "unknown command $cmd (preflight|retire|unretire|add|unadd|check)" ;;
    esac
}
main "$@"
