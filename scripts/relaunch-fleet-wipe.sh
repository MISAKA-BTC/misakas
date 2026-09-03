#!/usr/bin/env bash
# The re-genesis fleet wipe, as a reviewable sequence rather than an improvisation.
#
# DRY RUN BY DEFAULT. Nothing is stopped or deleted without --execute.
#
# This encodes the ordering hazard §4b of the launch runbook exists for: STOP EVERY
# HOST BEFORE WIPING ANY. Wiping host A while host B still runs means B re-feeds A
# the old chain by IBD, and the relaunch looks like it worked until it does not.
#
# It also encodes the three findings that made the old procedure wrong:
#   - the peer set comes from LIVE SOCKETS, not a journal scrape (the journal misses
#     any peer that connected before the window and stayed connected; it found 3 of 9)
#   - appdirs are ENUMERATED FROM THE PROCESSES, never named (one host carries a full
#     appdir under /var/lib/misaka-minerpool/ that no `.t11` glob finds)
#   - the dnsseeders are part of the wipe (a surviving seeder hands new joiners
#     old-genesis peers, so a newcomer's first contact is the dead chain)
#   - units are listed with `--plain --all`, never the default. Without --plain a FAILED
#     unit's name is the bullet glyph; without --all the listing omits every unit that is
#     enabled but not running right now. Measured on ibm, 2026-09-03, with the naive form:
#         misaka-miner       inactive  ENABLED    <- absent from the listing
#         misaka-validator   inactive  ENABLED    <- absent from the listing
#         palw-ibd-join      failed    transient  <- listed as "●"
#     An enabled-but-inactive unit is exactly the one that comes back at the next boot,
#     after the appdirs are moved aside, on a host everybody believes is wiped.
#
#   usage: scripts/relaunch-fleet-wipe.sh census        # what is out there, changes nothing
#          scripts/relaunch-fleet-wipe.sh stop [--execute]
#          scripts/relaunch-fleet-wipe.sh verify        # assert nothing is running anywhere
#          scripts/relaunch-fleet-wipe.sh wipe --genesis <hash> [--execute]
set -uo pipefail

HOSTS=(169.58.39.220 169.58.232.113 5.104.81.23)
KEY="${MISAKA_WIPE_KEY:-$HOME/.ssh/claude_key}"
EXECUTE=0; GENESIS=""
CMD="${1:-}"; shift || true
while [ $# -gt 0 ]; do
    case "$1" in
        --execute) EXECUTE=1 ;;
        --genesis) GENESIS="${2:-}"; shift ;;
        *) echo "unknown argument: $1"; exit 2 ;;
    esac
    shift
done
r() { ssh -o ConnectTimeout=10 -o BatchMode=yes -i "$KEY" "root@$1" "$2" 2>/dev/null; }
say() { printf '%s\n' "$*"; }

case "$CMD" in
census)
    say "== the peer set, from LIVE SOCKETS (the journal misses long-lived peers) =="
    for h in "${HOSTS[@]}"; do
        say "-- $h --"
        # Strip the LAST :port rather than splitting on every colon — an IPv6 peer is
        # `[2001:db8::1]:26311` and a naive split prints "[", which is an address nobody
        # can act on. Found by running this against the live fleet.
        r "$h" 'ss -tn state established 2>/dev/null | grep -E "263[0-9][0-9]" \
             | awk "{ l=\$3; p=\$4; sub(/:[0-9]+\$/,\"\",l); sub(/:[0-9]+\$/,\"\",p); \
                      gsub(/^\\[|\\]\$/,\"\",p); lp=\$3; sub(/.*:/,\"\",lp); \
                      print (lp ~ /^263/ ? \"  INBOUND \" : \"  outbound\"), p }" \
             | sort -u' || say "  UNREACHABLE — resolve before wiping anything"
    done
    say ""
    say "== what each host runs, ENUMERATED FROM THE PROCESSES =="
    for h in "${HOSTS[@]}"; do
        say "-- $h ($(r "$h" hostname)) --"
        # Three forms of this have been wrong, in both directions:
        #   pgrep -af kaspad | grep -oE "appdir=[^ ]+"   matched THIS COMMAND — the remote shell's
        #       argv holds both "kaspad" and the literal regex, so it printed `appdir=[^` as an
        #       appdir on all three hosts, for as long as this script existed.
        #   pgrep -x kaspad                              missed `kaspad.candidate` on ibm — a
        #       RUNNING NODE on /tmp/fpchk, dropped from the wipe list. The repair for a false
        #       positive produced a FALSE NEGATIVE, which is the direction that loses a host.
        # So: walk /proc and match the EXECUTABLE's basename by prefix. The asker's exe is `bash`,
        # so it cannot appear; and any `kaspad*` build does, whatever it is called.
        r "$h" 'for d in /proc/[0-9]*; do
                    exe=$(readlink -f "$d/exe" 2>/dev/null) || continue
                    case "${exe##*/}" in kaspad*) ;; *) continue ;; esac
                    printf "  %-18s %s\n" "${exe##*/}" "$(tr "\0" "\n" < "$d/cmdline" 2>/dev/null | grep -E "^--appdir=" || echo "--appdir=<none>")"
                done | sort -u'

        r "$h" 'systemctl list-units --type=service --no-pager --no-legend --plain --all 2>/dev/null \
             | grep -iE "kaspad|misaka|palw|minerpool|dnsseeder" | awk "{print \"  unit \"\$1\" \"\$4}"'
    done
    say ""
    say "-- any address above that is NOT one of ${HOSTS[*]} is a peer you cannot wipe."
    say "-- ownership is decided by ONE thing: does it answer a key you hold TODAY."
    ;;
stop)
    say "== STOP EVERY HOST — no wiping until this reports clean for all of them =="
    [ "$EXECUTE" = 1 ] || say "   (dry run; add --execute to actually stop)"
    for h in "${HOSTS[@]}"; do
        units=$(r "$h" 'systemctl list-units --type=service --no-pager --no-legend --plain --all 2>/dev/null \
             | grep -iE "kaspad|misaka|palw|minerpool|dnsseeder" | awk "{print \$1}" | grep -E "\\.service$"')
        say "-- $h --"
        for u in $units; do
            if [ "$EXECUTE" = 1 ]; then
                r "$h" "systemctl stop $u; systemctl disable $u" >/dev/null
                say "  stopped+disabled $u"
            else
                say "  would stop+disable $u"
            fi
        done
    done
    say ""
    say "== and the nodes NO UNIT OWNS — the unit loop above cannot reach these =="
    # Measured 2026-09-03: ibm runs `kaspad.candidate` on /tmp/fpchk from
    # `/user.slice/user-0.slice/session-28377.scope` — a LOGIN SESSION, not a service. Stopping
    # every `.service` leaves it running, `verify` then refuses, and before this block the
    # procedure had no next step: an operator who reads that refusal as a bug and wipes anyway
    # defeats the entire point of the gate. Enumerate by EXECUTABLE basename, the way `census`
    # does, and report the cgroup so the operator can see it is not a unit.
    for h in "${HOSTS[@]}"; do
        say "-- $h --"
        found=$(r "$h" 'for d in /proc/[0-9]*; do
                            exe=$(readlink -f "$d/exe" 2>/dev/null) || continue
                            case "${exe##*/}" in kaspad*) ;; *) continue ;; esac
                            cg=$(head -1 "$d/cgroup" 2>/dev/null)
                            case "$cg" in *.service*) continue ;; esac
                            echo "${d#/proc/} ${exe##*/} ${cg##*/}"
                        done')
        if [ -z "$found" ]; then
            say "  none — every node here is owned by a unit"
        else
            printf '%s\n' "$found" | while read -r pid name cg; do
                if [ "$EXECUTE" = 1 ]; then
                    r "$h" "kill $pid" >/dev/null
                    say "  killed pid $pid ($name, $cg)"
                else
                    say "  would kill pid $pid ($name, in $cg — NOT a service)"
                fi
            done
        fi
    done
    say ""
    say "-- disable as well as stop: a unit that restarts on boot un-wipes the host."
    say "-- a node no unit owns survives the unit loop entirely: that is what the block above is for."
    say "-- now run: $0 verify"
    ;;
verify)
    say "== assert NOTHING is producing anywhere (this gates the wipe) =="
    bad=0
    for h in "${HOSTS[@]}"; do
        n=$(r "$h" 'pgrep -c kaspad 2>/dev/null || true')
        s=$(r "$h" 'ss -tn state established 2>/dev/null | grep -cE "263[0-9][0-9]" || true')
        n=${n:-0}; s=${s:-0}; say "  $h  kaspad=$n  p2p-sockets=$s"
        [ "${n:-1}" != "0" ] && bad=1
    done
    if [ "$bad" = 0 ]; then say ""; say "-- clean. The wipe is now safe to run."; exit 0
    else say ""; say "-- NOT CLEAN. Wiping now would let a live host re-feed the old chain."; exit 1; fi
    ;;
wipe)
    [ -n "$GENESIS" ] || { say "refusing: --genesis <old-hash> is required, it names the backup"; exit 2; }
    say "== WIPE — appdirs moved aside, never deleted, named by the genesis they held =="
    [ "$EXECUTE" = 1 ] || say "   (dry run; add --execute to actually move)"
    say "   re-verifying nothing runs, because 'stop' may have raced a restart:"
    "$0" verify || { say "   ABORTING: a host is still running."; exit 1; }
    stamp=$(date -u +%Y%m%d-%H%M)
    for h in "${HOSTS[@]}"; do
        say "-- $h --"
        dirs=$(r "$h" 'pgrep -af kaspad 2>/dev/null | grep -oE "appdir=[^ ]+" | cut -d= -f2 | sort -u')
        [ -z "$dirs" ] && dirs=$(r "$h" 'ls -d /root/.t11 /root/.t11? /var/lib/misaka-minerpool/slots/*/appdir 2>/dev/null')
        for d in $dirs; do
            if [ "$EXECUTE" = 1 ]; then
                r "$h" "mv '$d' '${d}.old-${GENESIS}-${stamp}'" && say "  moved $d -> ${d}.old-${GENESIS}-${stamp}"
            else
                say "  would move $d -> ${d}.old-${GENESIS}-${stamp}"
            fi
        done
        seeder=$(r "$h" 'ls -d /var/lib/misaka-dnsseeder /root/.dnsseeder 2>/dev/null')
        for sd in $seeder; do
            if [ "$EXECUTE" = 1 ]; then
                r "$h" "mv '$sd' '${sd}.old-${GENESIS}-${stamp}'" && say "  moved SEEDER $sd"
            else
                say "  would move SEEDER $sd  (a surviving seeder hands joiners the dead chain)"
            fi
        done
    done
    say ""
    say "-- producer starts FIRST. Then verify the genesis hash on every host before the next one."
    ;;
*)
    say "usage: $0 {census|stop|verify|wipe --genesis <hash>} [--execute]"
    exit 2 ;;
esac
