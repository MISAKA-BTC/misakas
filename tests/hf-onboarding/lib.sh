# tests/hf-onboarding/lib.sh — sourced by devnet.sh and onboard.sh (H1, the HF onboarding closed loop).
#
# ONE private devnet on this Mac, loopback only: a salted testnet-12 drill chain (ADR-0152 §8.2 — its own genesis, its own keyring
# written by the binary under test, its own app dirs and ports), running the shipped testnet-12 rules with the release's flag days
# compressed to low heights (the combined drill's schedule, audit-combined/dc.sh: 6/10/14, IR 16, IR-2 24, int-11 26). It never
# touches a public node, a fleet host, a card key or a public app dir; kaspad itself refuses all of them on a salted chain.
#
# Roles (the H1 brief):
#   A    bootstrap/chain     genesis seat 0, floor producer, heartbeat clock
#   B    validator/RPC       genesis seat 1, heartbeat clock (the RPC the client U uses)
#   C    registry/observer   no keys, no seat (reads only)
#   D1-6 Panel seats         genesis seats 2..7 (the Panel needs seat_count 5 + 1 distinct operators)
#   V    public verifier     a POST-GENESIS bond (keyring bond 9), started once its bond is on the chain; not a genesis operator
#   reg  bond registrar      transient: `kaspad --palw-register-bond` for one post-genesis bond, stopped once it prints the bond
#   Z    fresh node          transient: a clean node that joins later (IBD) and is compared with A/B
# The client U is NOT a node: its own HOME, its own key (keyring bond 8 — see devnet.sh `user`), its own funds (sent by the bootstrap's
# main wallet once), its own bond, and only the CLI over B's RPC. U never reads a genesis seat key or the main wallet key.
set -euo pipefail
H1=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WT=${H1_WT:-$(cd "$H1/../.." && pwd)}
RUN_ROOT=${RUN_ROOT:-/Users/wata/Downloads/MISAKA-wt-b/wh-h1-run}
# **A stage runs from a SNAPSHOT of this directory** — bash reads a script as it runs, so an edit to the runner while a stage of hours
# is in flight would make the running bash execute whatever lines now sit at its old offset (it happened once here). The entry scripts
# copy the runner once and exec the copy; the copy knows the worktree through H1_WT.
h1_snapshot_exec() { # h1_snapshot_exec <script name> "$@"
    [ -n "${H1_SNAPSHOT:-}" ] && return 0
    local snap=$RUN_ROOT/runner-snap/$(date +%Y%m%d-%H%M%S)-$$
    mkdir -p "$snap"; cp "$H1"/*.sh "$H1"/*.py "$H1"/*.json "$snap"/
    H1_SNAPSHOT=$snap H1_WT=$WT exec bash "$snap/$1" "${@:2}"
}
RUN_ID=${RUN_ID:-}
WORK_DIR=${WORK_DIR:-$RUN_ROOT/devnet${RUN_ID:+-$RUN_ID}}
BIN_DIR=${BIN_DIR:-$WT/target/release}
KASPAD_BIN=${KASPAD_BIN:-$BIN_DIR/kaspad}
CLI_BIN=${CLI_BIN:-$BIN_DIR/misaka}
PALW_CLASS_BIN=${PALW_CLASS_BIN:-$BIN_DIR/palw-class}
KR=$WORK_DIR/keyring
OPHOME=$WORK_DIR/ophome          # HOME for the bootstrap operator's CLI calls (main wallet funding only)
UHOME=$WORK_DIR/user             # HOME of the client U (never the real ~/.misaka)
U_BOND_N=${U_BOND_N:-8}          # U's key: keyring bond 8 (a salted kaspad refuses any other key as a bond registrar's)
V_BOND_N=${V_BOND_N:-9}          # V's bond: keyring bond 9
GENESIS_SEATS=8

# The release's flag days, compressed (the combined drill's validated schedule; `validate_palw_v2` holds the order).
FENCE_AT=${FENCE_AT:-6}; FENCE2_AT=${FENCE2_AT:-10}; FENCE3_AT=${FENCE3_AT:-14}
TIR_AT=${TIR_AT:-16}; TIR2_AT=${TIR2_AT:-24}; INT11_AT=${INT11_AT:-26}
FENCE4_AT=${FENCE4_AT:-}         # testnet-12's fourth post-launch list (P0a GDN key heads): set it only if the shipped schedule has it

# Ports: disjoint from every other drill on this Mac (36100, 46100, 50200-54200, 55100-59100) and from public testnet-12's.
P2P_BASE=${P2P_BASE:-61100}; BORSH_BASE=${BORSH_BASE:-62100}; JSON_BASE=${JSON_BASE:-63100}; EVM_BASE=${EVM_BASE:-64100}
RAM_SCALE=${RAM_SCALE:-0.3}
SHARE_MIB=${SHARE_MIB:-2048}
MEM_FLOOR_PCT=${MEM_FLOOR_PCT:-25}   # refuse to start a node below this much free memory

#  name k  seat role     hb jit
NODES="A    0  0    floor    1  0
B    1  1    seat     1  0
C    2  -    observer 0  0
D1   3  2    seat     0  0
D2   4  3    seat     0  0
D3   5  4    seat     0  0
D4   6  5    seat     0  0
D5   7  6    seat     0  0
D6   8  7    seat     0  0
V    9  x9   verifier 0  1
reg  10 -    reg      0  1
Z    11 -    fresh    0  1"

say() { printf '[h1 %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { say "REFUSED: $*"; exit 1; }
row() { echo "$NODES" | awk -v n="$1" '$1==n'; }
field() { local r; r=$(row "$1"); [ -n "$r" ] || die "no node $1"; echo "$r" | awk -v i="$2" '{print $i}'; }
kof() { field "$1" 2; }
p2p() { echo $((P2P_BASE + $(kof "$1"))); }
borsh() { echo $((BORSH_BASE + $(kof "$1"))); }
jport() { echo $((JSON_BASE + $(kof "$1"))); }
steady_nodes() { echo "$NODES" | awk '$6==0 {print $1}'; }
all_nodes() { echo "$NODES" | awk '{print $1}'; }
running() { local d=$WORK_DIR/$1; [ -f "$d/kaspad.pid" ] && ps -p "$(cat "$d/kaspad.pid")" -o command= 2>/dev/null | grep -q -- "--appdir=$d/app"; }
salt() {
    local s=${SALT:-}
    [ -n "$s" ] || s=$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)
    [[ "$s" =~ ^[0-9a-f]{64}$ ]] || die "no devnet salt ($WORK_DIR/SALT)"
    echo "$s"
}
manifest() { python3 -c "import json,sys; m=json.load(open(sys.argv[1])); print($1)" "$KR/manifest.json"; }
mem_free_pct() { memory_pressure 2>/dev/null | awk -F': ' '/System-wide memory free percentage/ {gsub("%","",$2); print $2}'; }

# The CLI as a party: `ucli` is the client U (its own HOME, B's RPC), `ocli` the bootstrap operator (main wallet), `ncli <node>` a read.
ucli() { HOME=$UHOME "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$(borsh "${U_RPC_NODE:-B}")" --palw-drill-genesis-salt="$(salt)" "$@"; }
ocli() { HOME=$OPHOME "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$(borsh A)" --palw-drill-genesis-salt="$(salt)" "$@"; }
ncli() { local n=$1; shift; HOME=$OPHOME "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$(borsh "$n")" --palw-drill-genesis-salt="$(salt)" "$@"; }
rpc() { python3 "$WT/audit-tir/rpc.py" call --port "$(jport "$1")" "$2" "${3:-{\}}"; }
tip() { rpc "${1:-C}" getBlockDagInfo '{}' 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("virtualDaaScore","?"))' 2>/dev/null || echo "?"; }

# node_args <name> — the kaspad argv, one per line.
node_args() {
    local n=$1 k seat role hb d
    k=$(kof "$n"); seat=$(field "$n" 3); role=$(field "$n" 4); hb=$(field "$n" 5); d=$WORK_DIR/$n
    local a=(--testnet --netsuffix=12 "--palw-drill-genesis-salt=$(salt)" "--appdir=$d/app" --yes --nodnsseed --disable-upnp
             "--listen=127.0.0.1:$((P2P_BASE + k))" "--rpclisten-borsh=127.0.0.1:$((BORSH_BASE + k))"
             "--rpclisten-json=127.0.0.1:$((JSON_BASE + k))" "--evm-rpc-listen=127.0.0.1:$((EVM_BASE + k))"
             --utxoindex --nogrpc
             "--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT"
             "--palw-drill-tir-at=$TIR_AT" "--palw-drill-tir2-at=$TIR2_AT" "--palw-drill-int11-at=$INT11_AT"
             "--ram-scale=$RAM_SCALE" "--palw-host-memory-share=$((SHARE_MIB * 1048576))")
    [ -n "$FENCE4_AT" ] && a+=("--palw-drill-fence4-at=$FENCE4_AT")
    case $seat in
        -) ;;
        x*) local xn=${seat#x}; local xb="$WORK_DIR/bonds/bond-$xn.json"
            [ -s "$xb" ] || die "$n: post-genesis bond $xn is not registered yet ($xb)"
            a+=("--palw-producer-key=$KR/bond-$xn.seed"
                "--palw-producer-bond=$(python3 -c "import json;print(json.load(open('$xb'))['bond_outpoint'])")"
                "--palw-fee-outpoint=$(python3 -c "import json;print(json.load(open('$xb'))['fee_outpoint'])")") ;;
        *) a+=("--palw-producer-key=$KR/bond-$seat.seed"
               "--palw-producer-bond=$(manifest "m['seats'][$seat]['bond_outpoint']")"
               "--palw-fee-outpoint=$(manifest "m['seats'][$seat]['fee_float_outpoint']")") ;;
    esac
    [ "$role" = floor ] && a+=(--palw-produce)
    [ "$hb" = 1 ] && a+=("--palw-heartbeat-miner-address=$(manifest "m['heartbeat'][$k]['address']")" --enable-unsynced-mining)
    # ISOLATED (a file in the node's dir): listen on a shifted P2P port and dial nobody — the others' --addpeer names the old port, so
    # the node mines its own branch until it is restarted without the mark (the reorg check).
    if [ -e "$d/ISOLATED" ]; then
        local i; for i in "${!a[@]}"; do [[ "${a[$i]}" == --listen=* ]] && a[$i]="--listen=127.0.0.1:$((P2P_BASE + 500 + k))"; done
    else
        local m; for m in $(steady_nodes); do [ "$m" = "$n" ] || a+=("--addpeer=127.0.0.1:$(p2p "$m")"); done
    fi
    local extra="$d/extra-args"; if [ -s "$extra" ]; then while read -r x; do [ -n "$x" ] && a+=("$x"); done < "$extra"; fi
    printf '%s\n' "${a[@]}"
}

start_node() {
    local n=$1 d=$WORK_DIR/$1; mkdir -p "$d/home"
    if running "$n"; then say "$n: already running (pid $(cat "$d/kaspad.pid"))"; return 0; fi
    local free; free=$(mem_free_pct)
    [ "${free:-0}" -ge "$MEM_FLOOR_PCT" ] || die "memory free ${free}% < ${MEM_FLOOR_PCT}% — not starting $n"
    local A2=() x; while IFS= read -r x; do A2+=("$x"); done < <(node_args "$n")
    printf '%s\n' "${A2[@]}" | sed 's/salt=[0-9a-f]*/salt=<SALT>/' > "$d/args.redacted"
    local c; c=$(stat -f %z "$d/kaspad.out" 2>/dev/null || echo 0); echo "$c" > "$d/start.cursor"
    ( cd "$d"; ulimit -n 10240; HOME="$d/home" nohup nice -n 10 "$KASPAD_BIN" "${A2[@]}" >> "$d/kaspad.out" 2>&1 & echo $! > "$d/kaspad.pid" )
    echo "$(date '+%F %T') START pid $(cat "$d/kaspad.pid") cursor $c bin $(shasum -a 256 "$KASPAD_BIN" | cut -c1-16)" >> "$d/events.log"
    local port; port=$(jport "$n")
    for _ in $(seq 1 90); do
        # Up = its JSON wRPC listens and answers getBlockDagInfo (the genesis is checked by the salt at start-up: a node on another genesis
        # refuses the drill's app dir and exits).
        if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1 && rpc "$n" getBlockDagInfo '{}' >/dev/null 2>&1; then say "$n: up (pid $(cat "$d/kaspad.pid"))"; return 0; fi
        ps -p "$(cat "$d/kaspad.pid")" >/dev/null 2>&1 || { say "$n: DIED — $(tail -c +$((c + 1)) "$d/kaspad.out" | tail -5)"; return 1; }
        sleep 2
    done
    say "$n: its RPC did not answer in 180 s"; return 1
}

stop_node() {
    local n=$1 d=$WORK_DIR/$1
    running "$n" || return 0
    local pid; pid=$(cat "$d/kaspad.pid"); kill -INT "$pid"; echo "$(date '+%F %T') STOP pid $pid" >> "$d/events.log"
    for _ in $(seq 1 90); do ps -p "$pid" >/dev/null 2>&1 || { say "$n: stopped"; return 0; }; sleep 1; done
    say "$n: still alive after 90 s (pid $pid)"; return 1
}
