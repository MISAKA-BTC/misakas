#!/bin/bash
# deploy-t12/install-5104.sh — 5.104.81.23 (hostname vmi3272359): bonds 2, 3, 4, 5, 7. Not public
# (ufw drops inbound; every node listens on 127.0.0.1 and dials the public nodes out).
#
#   b2  misaka-t12-seat2 (drop-in)  127.0.0.1:26311  borsh 26313  json 26314   8k producer + 8k seat
#   b3  misaka-t12-seat3 (drop-in)  127.0.0.1:26321  borsh 26323  json 26324   8k seat
#   b4  misaka-t12-seat4 (drop-in)  127.0.0.1:26331  borsh 26333  json 26334   8k seat
#   b5  misaka-t12-seat5 (drop-in)  127.0.0.1:26341  borsh 26343  json 26344   8k seat
#   b7  misaka-t12-seat7 (NEW unit) 127.0.0.1:26351  borsh 26353  json 26354   8k seat (card 7, re-keyed here 09-23)
# borsh 26313 is what misaka-dnsseeder-t12 here already asks for peers.
#
# The route-matrix session (5bd46c14) runs the OLD live chain's floor seats (misaka-t12f-b2..b5,
# /root/t12-live) and a PRIVATE t12 (misaka-t12p-0..7, -scan, -scan-tunnel, /root/t12-private,
# plus a daa-obs node) here. `switch` refuses while any of it runs: it is ~25 GB of RSS on a 23 GiB
# host, and the private chain shares this chain's keys.
#
# Run as root on 5.104 from $REL_ROOT/kit:  ./install-5104.sh preflight | stage | switch | check | rollback
. "$(dirname "$0")/lib.sh"

HOST_NAME_EXPECTED=vmi3272359
RESERVE_MIB=1536
START_GAP=30                 # each node re-derives the 8k manifest root (--palw-verify-class-manifest) and maps 1.7 GB
BINARIES=(kaspad misaka palw-class)
PUB="169.58.232.113:26311,169.58.39.220:26311,169.58.39.220:26321"
NODES=(
  "2|misaka-t12-seat2|dropin|127.0.0.1:26311|26313|26314|-|-|7168|10|8k|0|1|127.0.0.1:26321,127.0.0.1:26331,127.0.0.1:26341,127.0.0.1:26351,$PUB"
  "3|misaka-t12-seat3|dropin|127.0.0.1:26321|26323|26324|-|-|3584|6|none|0|1|127.0.0.1:26311,127.0.0.1:26331,127.0.0.1:26341,127.0.0.1:26351,$PUB"
  "4|misaka-t12-seat4|dropin|127.0.0.1:26331|26333|26334|-|-|3584|6|none|0|1|127.0.0.1:26311,127.0.0.1:26321,127.0.0.1:26341,127.0.0.1:26351,$PUB"
  "5|misaka-t12-seat5|dropin|127.0.0.1:26341|26343|26344|-|-|3584|6|none|0|1|127.0.0.1:26311,127.0.0.1:26321,127.0.0.1:26331,127.0.0.1:26351,$PUB"
  "7|misaka-t12-seat7|new|127.0.0.1:26351|26353|26354|-|-|3584|6|none|0|1|127.0.0.1:26311,127.0.0.1:26321,127.0.0.1:26331,127.0.0.1:26341,$PUB"
)
# the first regenesis deploy's seat appdirs (a few MB each). NOT the route-matrix session's
# /root/.t12f-b* or /root/t12-private/run/* — those are its to clear.
OLD_APPDIRS=(/root/.t12 /root/.t12b /root/.t12c /root/.t12e)

other_session_running() { # prints what is still up
    systemctl list-units --no-legend --state=active 'misaka-t12f-*' 'misaka-t12p-*' 2>/dev/null | awk '{print "  unit " $1}'
    pgrep -af '/root/t12-private/|/root/t12-live/|/root/perm-drill/' 2>/dev/null | grep -v pgrep | cut -c1-140 | sed 's/^/  proc /' || true
}

host_preflight() {
    local o; o=$(other_session_running)
    if [ -n "$o" ]; then warn "the route-matrix session's nodes are still running here (switch will refuse):"; echo "$o" >&2; fi
    say "  old seat units: $(for n in 2 3 4 5; do printf 'seat%s=%s/%s ' $n "$(systemctl show -p ActiveState --value misaka-t12-seat$n)" "$(systemctl is-enabled misaka-t12-seat$n 2>/dev/null)"; done)"
    say "  t11 fixture node (/root/misakas-stale-consensus-diagnosis, not ours, left alone): $(pgrep -f misakas-stale-consensus-diagnosis >/dev/null && echo running || echo stopped)"
    ls -l /root/palw-class/qwen25-1.5b-a16-8k.palwart* 2>/dev/null | sed 's/^/  /' || true
}

host_guard_switch() {
    local o; o=$(other_session_running)
    [ -z "$o" ] || { echo "$o" >&2; die "stop the route-matrix session's t12f/t12p nodes first (that session owns them) — PLAN.md §5 step 0"; }
    local avail_mib; avail_mib=$(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo)
    [ "$avail_mib" -ge 20000 ] || die "MemAvailable ${avail_mib} MiB < 20000 — something else is holding memory; the five nodes declare 21 GiB"
}

host_usage() { :; }
host_cmd() { usage_common; die "unknown command $1"; }

main_dispatch "$@"
