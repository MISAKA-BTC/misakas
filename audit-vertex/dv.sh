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
#   BIN_DIR=<dir with kaspad misaka> bash audit-vertex/dv.sh <command>
#   [WORK_DIR=~/.misaka-palw-vertex-drill] [VERTEX_AT=120]
# commands: dry | up | status | census | carriage | equivocate | bond | evidence | down
#   evidence copies the run's proofs (census, carriage, the nodes' vertex / equivocation / slash log lines, the fence banners) into
#   ~/Downloads/MISAKA-wt-b/lanes/evidence/rfc7-vertex/drill/.
set -euo pipefail
V=$(cd "$(dirname "$0")" && pwd)           # audit-vertex/
WT=$(cd "$V/.." && pwd)
A=$WT/audit-tir
export WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-vertex-drill}
export DF1=0 VERTEX_AT=${VERTEX_AT:-120}
export P2P_BASE=${P2P_BASE:-56300} BORSH_BASE=${BORSH_BASE:-57300} JSON_BASE=${JSON_BASE:-58300} EVM_BASE=${EVM_BASE:-59300} GRPC_BASE=${GRPC_BASE:-55300}
EVIDENCE=${EVIDENCE:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/rfc7-vertex/drill}
cmd=${1:-dry}
shift || true

# shellcheck disable=SC1091
. "$A/lib-df.sh"


rpcport() { echo $((JSON_BASE + $(kof "$1"))); }

# new6: the seat that equivocates; new5: another seat that files; the floor producer is new3.
EQUIVOCATOR=${EQUIVOCATOR:-new6}

# The eight nodes of the D-F layout, minus the old relay: the genesis registers eight bonds, a panel draws five of the seven that are not
# its executor's, and a claim licenses only when every one of its five seats answers — so every bond runs. new3 is the floor producer,
# new1 / new2 the chain's heartbeat clocks, the rest plain seats. The small IR class and the 1.5B class play no part: RFC-0007's rules
# read the floor class's own claims.
VNODES=${VNODES:-"new1 new2 new3 new0 new4 new5 new6 new7"}
FAILED=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; FAILED=1; }

preflight() {
    echo "== vertex drill preflight ($(date '+%F %T')): flag days $FENCE_AT/$FENCE2_AT/$FENCE3_AT, IR fence $TIR_AT, the verification vertex at $VERTEX_AT, work dir $WORK_DIR, ports ${P2P_BASE}+/${BORSH_BASE}+/${JSON_BASE}+"
    local b
    for b in "$KASPAD_BIN" "$CLI_BIN"; do
        [ -n "$b" ] && [ -x "$b" ] && ok "$b ($(shasum -a 256 "$b" | cut -c1-16))" || bad "binary missing: '${b}' (--bin-dir / KASPAD_BIN, CLI_BIN)"
    done
    if [ -x "${KASPAD_BIN:-/nonexistent}" ]; then
        local H; H=$("$KASPAD_BIN" --help 2>/dev/null || true)
        for f in --palw-drill-vertex-at --palw-drill-vertex-equivocate-at --palw-vertex-full-refs --palw-drill-fence-at --palw-drill-tir2-at --palw-drill-write-keyring; do
            grep -q -- "$f" <<<"$H" && ok "kaspad lists $f" || bad "kaspad lacks $f"
        done
    fi
    [ "$VERTEX_AT" -gt "$FENCE3_AT" ] 2>/dev/null && [ "$VERTEX_AT" -ne "$TIR_AT" ] && ok "the vertex fence at $VERTEX_AT is a height of its own" || bad "VERTEX_AT=$VERTEX_AT must be past $FENCE3_AT and not $TIR_AT"
    local busy="" k base n
    for n in $VNODES; do k=$(kof "$n"); for base in $P2P_BASE $BORSH_BASE $JSON_BASE $EVM_BASE $GRPC_BASE; do
        lsof -nP -iTCP:$((base + k)) -sTCP:LISTEN >/dev/null 2>&1 && busy="$busy $((base + k))"; done; done
    [ -z "$busy" ] && ok "ports free" || bad "ports in use:$busy"
    local others; others=$( { pgrep -f -- "--palw-drill-genesis-salt" 2>/dev/null || true; } | while read -r p; do
        ps -p "$p" -o command= 2>/dev/null | grep -q -- "--appdir=$WORK_DIR/" || echo "$p"; done | wc -l | tr -d ' ')
    [ "${others:-0}" = 0 ] && ok "no other drill running" || bad "another drill is running ($others salted kaspad processes outside $WORK_DIR)"
    local free; free=$( { memory_pressure 2>/dev/null || true; } | awk -F': ' '/System-wide memory free percentage/ {gsub("%","",$2); print $2}')
    [ "${free:-0}" -ge 40 ] && ok "memory free ${free}%" || bad "memory free ${free}% < 40%"
    local disk; disk=$(df -g "$HOME" | awk 'NR==2 {print $4}')
    [ "${disk:-0}" -ge 15 ] && ok "disk free ${disk} GiB" || bad "disk free ${disk} GiB < 15"
    for n in $VNODES; do [ -d "$WORK_DIR/$n/app" ] && bad "$WORK_DIR/$n/app exists (a drill already created here)"; done
    return 0
}

case $cmd in
    dry) preflight; [ "$FAILED" = 0 ] && echo "dry: ok" || { echo "dry: FAILED"; exit 1; } ;;
    up)
        preflight; [ "$FAILED" = 0 ] || die "preflight failed — not starting"
        mkdir -p "$WORK_DIR" "$UHOME"; chmod 700 "$WORK_DIR"
        if [ -z "${SALT:-}" ] && [ ! -s "$WORK_DIR/SALT" ]; then ( umask 077; openssl rand -hex 32 > "$WORK_DIR/SALT" ); fi
        if [ ! -e "$KR/manifest.json" ]; then
            mkdir -p "$KR" "$WORK_DIR/keyring-app"; chmod 700 "$KR"
            "$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$WORK_DIR/keyring-app" --palw-drill-genesis-salt="$(salt)" \
                "--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT" \
                "--palw-drill-tir-at=$TIR_AT" ${TIR2_AT:+"--palw-drill-tir2-at=$TIR2_AT"} "--palw-drill-vertex-at=$VERTEX_AT" \
                --palw-drill-write-keyring="$KR" 2>&1 | tail -2 | sed -E "s/[0-9a-f]{64}/<64hex>/g"
            chmod 600 "$KR"/*
        fi
        manifest "'genesis', m['genesis_hash'][:16], 'params', m['consensus_params_id'][:16], 'salt_id', m['salt_id'], 'vertex_at', m.get('vertex_at')"
        touch "$WORK_DIR/.up"
        for n in $VNODES; do bash "$A/nodes.sh" start "$n"; sleep 3; done
        say "up: tip $(tip new3)" ;;
    down) bash "$A/nodes.sh" stop $VNODES ;;
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
