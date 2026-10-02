#!/usr/bin/env bash
# audit-vertex/dv.sh — RFC-0007 Part I's drill, ONE salted testnet-12 drill chain on this Mac (loopback only), on the binary this branch
# would ship. It runs audit-tir/df.sh's chain (the D-F kit's eight nodes, DF1=0: the tiny class, no 1.5B artifact) with ONE more fence:
# `palw_verification_vertex_v1` at VERTEX_AT (`--palw-drill-vertex-at`), and reads the chain through RPC and the nodes' logs.
#
# What it shows (RFC-0007's activation table, row 1):
#   licences   claims whose panel bound BELOW the fence license by receipts (receipt-licence objects in the blocks); claims bound at or
#              above it license by TALLY (no licence object past the fence; vertex objects instead)
#   cross      a claim bound below the fence and licensed at or after it licensed on the OLD path (`census`: cross_claims)
#   equivocate one seat (new6) restarted with `--palw-drill-vertex-equivocate-at` signs a second vertex for a round; another node files
#              the evidence; the fold slashes 100 per mille, forfeits the locks, ejects the bond (`bond` before and after)
#   carriage   the bytes the chain carries per licence, below the fence (receipts) and above it (vertices) — `carriage`
#
#   bash audit-vertex/dv.sh <command> --bin-dir <dir with kaspad misaka palw-class palw-tir-fidelity>
#   [WORK_DIR=~/.misaka-palw-vertex-drill] [VERTEX_AT=120] [TIR2_AT=30]
# commands: dry | small | up | status | census | carriage | equivocate | bond | evidence | down
#   evidence copies the run's proofs (census, carriage, the nodes' vertex / equivocation / slash log lines, the fence banners) into
#   ~/Downloads/MISAKA-wt-b/lanes/evidence/rfc7-vertex/drill/.
set -euo pipefail
V=$(cd "$(dirname "$0")" && pwd)           # audit-vertex/
WT=$(cd "$V/.." && pwd)
A=$WT/audit-tir
export WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-vertex-drill}
export DF1=0 TIR2_AT=${TIR2_AT:-30} VERTEX_AT=${VERTEX_AT:-120}
export P2P_BASE=${P2P_BASE:-56300} BORSH_BASE=${BORSH_BASE:-57300} JSON_BASE=${JSON_BASE:-58300} EVM_BASE=${EVM_BASE:-59300} GRPC_BASE=${GRPC_BASE:-55300}
EVIDENCE=${EVIDENCE:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/rfc7-vertex/drill}
cmd=${1:-dry}
shift || true

# shellcheck disable=SC1091
. "$A/lib-df.sh"

df() { bash "$A/df.sh" "$@"; }
rpcport() { echo $((JSON_BASE + $(kof "$1"))); }

# new6: the seat that equivocates; new5: another seat that files; the floor producer is new3.
EQUIVOCATOR=${EQUIVOCATOR:-new6}

case $cmd in
    dry|small|up|down)
        df "$cmd" "$@" ;;
    status)
        df status "$@" 2>/dev/null || true
        python3 "$V/dvmeasure.py" status --ports "$(for n in $(new_nodes); do printf '%s,' "$(jport "$n")"; done | sed 's/,$//')" ;;
    census)
        python3 "$V/dvmeasure.py" census --port "$(jport new3)" --fence "$VERTEX_AT" --keyring "$KR" ;;
    carriage)
        python3 "$V/dvmeasure.py" carriage --port "$(jport new3)" --fence "$VERTEX_AT" "$@" ;;
    equivocate)
        tipnow=$(tip new3)
        [[ "$tipnow" =~ ^[0-9]+$ ]] || die "no tip"
        [ "$tipnow" -gt "$VERTEX_AT" ] || die "the tip $tipnow is not past the fence $VERTEX_AT"
        at=$((tipnow + ${EQUIVOCATE_LEAD:-15}))
        say "equivocate: $EQUIVOCATOR is restarted with --palw-drill-vertex-equivocate-at=$at (tip $tipnow)"
        mkdir -p "$WORK_DIR/$EQUIVOCATOR"
        echo "--palw-drill-vertex-equivocate-at=$at" > "$WORK_DIR/$EQUIVOCATOR/extra-args"
        bash "$A/nodes.sh" stop "$EQUIVOCATOR"
        bash "$A/nodes.sh" start "$EQUIVOCATOR"
        echo "$at" > "$WORK_DIR/equivocate.at" ;;
    bond)
        # The equivocator's bond: collateral, slashed, status, before / after (rpc.py snap).
        seat=$(field "$EQUIVOCATOR" 3)
        python3 "$A/rpc.py" snap --port "$(jport new3)" --bond "$(manifest "m['seats'][$seat]['bond_outpoint']")" --label "${1:-bond}" ;;
    evidence)
        mkdir -p "$EVIDENCE"
        { echo "# census at $(date '+%F %T') (fence $VERTEX_AT)"; python3 "$V/dvmeasure.py" census --port "$(jport new3)" --fence "$VERTEX_AT" --keyring "$KR"; } > "$EVIDENCE/census.json" 2>&1 || true
        { echo "# carriage at $(date '+%F %T') (fence $VERTEX_AT)"; python3 "$V/dvmeasure.py" carriage --port "$(jport new3)" --fence "$VERTEX_AT"; } > "$EVIDENCE/carriage.json" 2>&1 || true
        { for n in $(new_nodes); do echo "## $n"; grep -hE "palw_verification_vertex_v1|vertex|Vertex|equivocation|PALW DRILL" "$WORK_DIR/$n/kaspad.out" 2>/dev/null | sed -E 's/[0-9a-f]{64}/<64hex>/g' | tail -60; done; } > "$EVIDENCE/node-vertex-lines.txt"
        { for n in $(new_nodes); do echo "## $n"; grep -hE "Consensus (params|fence)|fence schedule|PALW DRILL CHAIN|moved" "$WORK_DIR/$n/kaspad.out" 2>/dev/null | head -8 | sed -E 's/salt=[0-9a-f]+/salt=<SALT>/'; done; } > "$EVIDENCE/banners.txt"
        python3 "$V/dvmeasure.py" status --ports "$(for n in $(new_nodes); do printf '%s,' "$(jport "$n")"; done | sed 's/,$//')" > "$EVIDENCE/status.txt" 2>&1 || true
        say "evidence written to $EVIDENCE" ;;
    *) sed -n '2,24p' "$0"; exit 2 ;;
esac
