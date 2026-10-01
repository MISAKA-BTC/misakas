#!/bin/bash
# deploy-t12/move-nodes.sh — WHERE a moved seat runs. Sourced by move-host.sh (on a host) and move-seat.sh
# (on the Mac); never executed. The 2026-10-01 migration: docs/design/palw/t12-panel-backlog-1001.md §7.
#
# The five seats on 5.104 (8 cores, 24 GiB: load 35-40, swap 13-17 GiB) gate 65 % of all panels and replayed 100x
# slower than the same code on ibm/.113. Two of them move: b7 to .113 (b6's host: 83 % idle, no swap) and b3 to ibm
# (79 % idle) — 3 seats on 5.104, 3 on ibm, 2 on .113.
#   * b7 is the seat the kit created as its own unit (mode `new`) — nothing of the route-matrix session's is under it;
#   * b3 is the heaviest seat on 5.104 (RES 6.4 GB, anon 4-5 GiB): the host loses the most by it.
# Ports are 127.0.0.1-only, like every seat on 5.104 (inbound is dropped there, nothing dials it); the target host's
# `move-host.sh preflight add` checks that none is held.

# <bond id>|<from host>|<to host>   (host = 5104 | 113 | ibm)
MOVES=( "7|5104|113" "3|5104|ibm" )

# The node spec the TARGET runs the seat as — lib.sh's field order:
#   id|unit|mode|listen|borsh|json|grpc|evm|share_mib|memmax_gib|produce|heartbeat|seat8k|peers
# share 3,584 MiB is the 8k full seat (3,456) + 128: the minimum that runs one 8k replay (PLAN.md §2) — never lower it.
# memmax 9 GiB: share + the 8k artifact (1,716) + the IR artifact (1,780) + 1 GiB = 8,104 MiB, as on 5.104.
PUB_113=169.58.232.113:26311
PUB_IBM=169.58.39.220:26311,169.58.39.220:26321
move_target_spec() { # <id> <host>
    case "$1@$2" in
        7@113) echo "7|misaka-t12-seat7|new|127.0.0.1:26351|26353|26354|-|-|3584|9|none|0|1|127.0.0.1:26311,$PUB_IBM" ;;
        3@ibm) echo "3|misaka-t12-seat3|new|127.0.0.1:26331|26333|26334|-|-|3584|9|none|0|1|127.0.0.1:26311,127.0.0.1:26321,$PUB_113" ;;
        *) return 1 ;;
    esac
}

# The spec the SOURCE (5.104) runs it as now — copied from install-5104.sh's NODES (move-host.sh checks the unit's
# ExecStart names b<id>.sh before it stops anything, so a drift is refused rather than followed).
PUB_5104="169.58.232.113:26311,169.58.39.220:26311,169.58.39.220:26321"
move_source_spec() { # <id> <host>
    case "$1@$2" in
        7@5104) echo "7|misaka-t12-seat7|new|127.0.0.1:26351|26353|26354|-|-|3584|9|none|0|1|127.0.0.1:26311,127.0.0.1:26321,127.0.0.1:26331,127.0.0.1:26341,$PUB_5104" ;;
        3@5104) echo "3|misaka-t12-seat3|dropin|127.0.0.1:26321|26323|26324|-|-|3584|9|none|0|1|127.0.0.1:26311,127.0.0.1:26331,127.0.0.1:26341,127.0.0.1:26351,$PUB_5104" ;;
        *) return 1 ;;
    esac
}

# MiB a host keeps for everything that is not a t12 kaspad (install-<host>.sh RESERVE_MIB). ibm's 2,048 is 1,536 here:
# b0 10,496 + b1 8,192 + seat3 3,584 + 2,048 would be 24,320 MiB against MemTotal 24,031; the non-kaspad RSS measured
# there is 0.3 GiB (ollama, journald, seeder, faucet, tunnel), so 1,536 leaves 1.2 GiB of margin and the sum 23,808.
move_reserve_mib() { case $1 in 113) echo 4096 ;; ibm) echo 1536 ;; 5104) echo 4096 ;; *) return 1 ;; esac; }

# Every public node a moved seat needs up while it is away (the 8k class keeps a margin of one ready seat):
MOVE_REQUIRE_UP=(169.58.232.113:26311 169.58.39.220:26311 169.58.39.220:26321)
