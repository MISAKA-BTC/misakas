# audit-tir/lib-df.sh — sourced by the D-F drill runner (RFC-0002 Phase F: D-F1 the Qwen2.5-A16 IR class
# end to end, D-F2 its court battery; D-F3/D-F4 are Phase F's step scripts, run on the same chain).
#
# ONE salted testnet-12 drill chain on this Mac, loopback only: its own salt ($WORK_DIR/SALT, 0600, never
# printed — or SALT from the environment), its own keyring (written by the shipping kaspad itself), its own
# ports, its own app dirs under $WORK_DIR. Never a fleet host, never a public node's app dir or port.
#
# The shared environment (Phase F's names, read by its df3/df4 step scripts too):
#   SALT, WORK_DIR, KASPAD_BIN (the release under test), CLI_BIN, OLD_KASPAD_BIN (the fleet's release),
#   TIR_AT, FENCE_AT, FENCE2_AT, FENCE3_AT, NEW0_P2P/NEW0_RPC/NEW0_GRPC/NEW0_LOG, OLD_P2P/OLD_RPC/OLD_LOG,
#   $WORK_DIR/ir-class.id, $WORK_DIR/ir-artifact.root, $WORK_DIR/ir-registration.obj.
set -euo pipefail
A=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)          # audit-tir/
WT=$(cd "$A/.." && pwd)
WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-tir-drill}
# BIN_DIR: the release under test (kaspad, misaka, palw-class, palw-a16-to-tir, palw-tir-equiv, redteam) —
# the one parameter a run points at a build; each binary can still be named on its own.
BIN_DIR=${BIN_DIR:-}
KASPAD_BIN=${KASPAD_BIN:-${BIN_DIR:+$BIN_DIR/kaspad}}
CLI_BIN=${CLI_BIN:-$(dirname "${KASPAD_BIN:-/nonexistent/kaspad}")/misaka}
# The fleet's release (the arm64 int-7 build, the exact commit the fleet runs), per the coordinator 2026-09-28.
OLD_KASPAD_BIN=${OLD_KASPAD_BIN:-/Users/wata/Downloads/MISAKA-wt-b/lifecycle-run/bin/7aba8dd57/kaspad}
# The offline tools (palw-class, palw-a16-to-tir, palw-tir-equiv): beside kaspad unless named.
TOOLS_BIN=${TOOLS_BIN:-$(dirname "${KASPAD_BIN:-/nonexistent/kaspad}")}
KR=$WORK_DIR/keyring
UHOME=$WORK_DIR/userhome          # HOME for every CLI call (never the real ~/.misaka)
IR_DIR=$WORK_DIR/ir

# The flag days, moved below the IR fence so the drill runs the rules the live chain has when the fence arms
# (validate_palw_v2: all four heights distinct, fence1 <= fence3).
FENCE_AT=${FENCE_AT:-6}; FENCE2_AT=${FENCE2_AT:-10}; FENCE3_AT=${FENCE3_AT:-14}; TIR_AT=${TIR_AT:-20}
# The second IR fence (palw_tir_fence2: the IR DA units of evidence transport C), for the B/D/C piece: set at
# `up` or never — a stored drill chain is only reopened under the same height (the datadir marker's tir2_at).
TIR2_AT=${TIR2_AT:-}
# DF1=0: the B/D/C piece's lighter chain — no node loads or produces D-F1 (the 1.5B class); the seven holders
# load the small class alone and new0 is a plain seat.
DF1=${DF1:-1}

# The IR class: the Qwen2.5-1.5B A16 artifact converted to PALW-TIR (its mirror program, unwindowed: F7
# dissects its history cones), declared at IR_CONTEXT positions with its logits tiled at IR_LOGITS_TILE
# lanes (1,024: at 2,048 its terminal close is ~5.9 MB as carried, over testnet-12's 3.2 MB — PALW-TIR-38).
# A16_ARTIFACT is the legacy .palwart it comes from (and what its logits are compared against);
# IR_ARTIFACT the declared class (built by `df.sh class`).
A16_ARTIFACT=${A16_ARTIFACT:-}
IR_CONTEXT=${IR_CONTEXT:-512}
IR_LOGITS_TILE=${IR_LOGITS_TILE:-1024}
IR_MODEL_ID=${IR_MODEL_ID:-Qwen/Qwen2.5-1.5B/a16-ir}
IR_ARTIFACT=${IR_ARTIFACT:-$IR_DIR/qwen25-a16.class.palwtir}
IR_LOWERED=$IR_DIR/qwen25-a16.lowered.palwtir
EQUIV_PROMPTS=${EQUIV_PROMPTS:-8}

# The small IR class, D-F2's live court battery (the coordinator, 2026-09-28): a 1.5B class's capture does not
# fit the 16 MiB material cap (an honest fold carries ~39.5 MB of logits rows), so no seat holds the accused
# capture and no court can convict there until the IR evidence transport lands
# (docs/design/palw/tir/evidence-transport-scope.md). The tiny HF llama fixture — lowered unwindowed (its
# history cone dissected: F7 live), declared at SMALL_CONTEXT positions — has ~38 KB captures. new4 registers
# and produces it; the same seven IR holders load both artifacts (no artifact node is added).
SMALL_FIXTURE=${SMALL_FIXTURE:-$WT/misaka-palw-tir-lower/tests/fixtures/hf/llama}
SMALL_CONTEXT=${SMALL_CONTEXT:-32}
SMALL_MODEL_ID=${SMALL_MODEL_ID:-test/llama-tiny-ir}
SMALL_LOWERED=$IR_DIR/small.lowered.palwtir
SMALL_ARTIFACT=${SMALL_ARTIFACT:-$IR_DIR/small.class.palwtir}

# Ports: disjoint from drill A (46100+), drill D (51100+), the lifecycle audit (36100+) and public nodes.
P2P_BASE=${P2P_BASE:-56100}; BORSH_BASE=${BORSH_BASE:-57100}; JSON_BASE=${JSON_BASE:-58100}
EVM_BASE=${EVM_BASE:-59100}; GRPC_BASE=${GRPC_BASE:-55100}
RAM_SCALE=${RAM_SCALE:-0.3}
SHARE_MIB=${SHARE_MIB:-3072}      # each node's replay share: the IR artifact (~1.6 GiB mapped) + scratch

# The node table:  name k seat role hb ir grpc
#   seat  the genesis bond (keyring seats[n]); '-' = no keys
#   role  ir     D-F1's registrant and producer (--palw-register-class, --palw-producer-class)
#         ir2    the small class's registrant and producer (D-F2's live battery lies here, one kind at a time)
#         floor  a floor producer (the chain's clock between heartbeats, and the admission jury's anchor)
#         seat   a seat only;   old  the fleet's release, keyless, a relay peered to new0 (D-F4)
#   hb    1 = a heartbeat clock;  ir  1 = loads the IR artifacts (a ready seat: t12 needs 5 + 2 of them)
#   grpc  1 = gRPC on (D-F3's red-team speaks gRPC to new0)
# Seven IR holders — the fewest t12's registry admits (seat_count 5 + spare 2 ready seats) and under this
# Mac's eight-artifact-node limit; new7 and the old relay hold none.
NODES="new0 0 3 ir 0 1 1
new1 1 0 seat 1 1 0
new2 2 1 seat 1 1 0
new3 3 2 floor 0 1 0
new4 4 4 ir2 0 1 0
new5 5 5 seat 0 1 0
new6 6 6 seat 0 1 0
new7 7 7 seat 0 0 0
old 8 - old 0 0 0"

say() { printf '[tir-df %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { say "REFUSED: $*"; exit 1; }
row() { echo "$NODES" | awk -v n="$1" '$1==n'; }
field() { local r; r=$(row "$1"); [ -n "$r" ] || die "no node $1"; echo "$r" | awk -v i="$2" '{print $i}'; }
kof() { field "$1" 2; }
p2p() { echo $((P2P_BASE + $(kof "$1"))); }
borsh() { echo $((BORSH_BASE + $(kof "$1"))); }
jport() { echo $((JSON_BASE + $(kof "$1"))); }
gport() { echo $((GRPC_BASE + $(kof "$1"))); }
all_nodes() { echo "$NODES" | awk '{print $1}'; }
new_nodes() { echo "$NODES" | awk '$4!="old" {print $1}'; }
ir_holders() { echo "$NODES" | awk '$6==1 {print $1}'; }
running() { local d=$WORK_DIR/$1; [ -f "$d/kaspad.pid" ] && ps -p "$(cat "$d/kaspad.pid")" -o command= 2>/dev/null | grep -q -- "--appdir=$d/app"; }

# The salt: SALT from the environment, else $WORK_DIR/SALT (0600, created by `up`, never printed).
salt() {
    local s=${SALT:-}
    [ -n "$s" ] || s=$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)
    [[ "$s" =~ ^[0-9a-f]{64}$ ]] || die "no drill salt (SALT, or $WORK_DIR/SALT)"
    echo "$s"
}
manifest() { python3 -c "import json,sys; m=json.load(open(sys.argv[1])); print($1)" "$KR/manifest.json"; }
ir_class_id() { tr -d ' \n' < "$WORK_DIR/ir-class.id"; }
small_class_id() { tr -d ' \n' < "$WORK_DIR/small-class.id"; }
# Is the class already on the drill chain (getPalwModelRegistry through new3)? Then its registrant is restarted
# WITHOUT --palw-register-class: the int-8 release (tir/node up to 7a9ab18da) builds nothing for a class the
# chain holds, never marks its registration done, and skips every duty after it in the panel's tick — readiness
# proofs included — for as long as the process lives (fixed on tir/node after 7a9ab18da). Not reachable (the
# chain is not up yet): the flag stays, as the first registration needs it.
class_on_chain() {
    local cid=$1
    [ -n "$cid" ] || return 1
    python3 "$A/rpc.py" call --port "$(jport new3)" getPalwModelRegistry '{}' 2>/dev/null | grep -q "$cid"
}

# The shared ports and logs, exported for Phase F's step scripts.
export_layout() {
    export SALT WORK_DIR KASPAD_BIN CLI_BIN OLD_KASPAD_BIN TIR_AT TIR2_AT FENCE_AT FENCE2_AT FENCE3_AT
    NEW0_P2P=$(p2p new0); NEW0_RPC=$(borsh new0); NEW0_GRPC=$(gport new0); NEW0_LOG=$WORK_DIR/new0/kaspad.out
    OLD_P2P=$(p2p old); OLD_RPC=$(borsh old); OLD_LOG=$WORK_DIR/old/kaspad.out
    export NEW0_P2P NEW0_RPC NEW0_GRPC NEW0_LOG OLD_P2P OLD_RPC OLD_LOG
}

# node_args <name> [extra…] — the kaspad argv, one per line.
node_args() {
    local n=$1; shift
    local k seat role hb ir grpc d
    k=$(kof "$n"); seat=$(field "$n" 3); role=$(field "$n" 4); hb=$(field "$n" 5); ir=$(field "$n" 6); grpc=$(field "$n" 7)
    d=$WORK_DIR/$n
    local a=(--testnet --netsuffix=12 "--palw-drill-genesis-salt=$(salt)" "--appdir=$d/app" --yes
             --nodnsseed --disable-upnp
             "--listen=127.0.0.1:$((P2P_BASE + k))" "--rpclisten-borsh=127.0.0.1:$((BORSH_BASE + k))"
             "--rpclisten-json=127.0.0.1:$((JSON_BASE + k))" "--evm-rpc-listen=127.0.0.1:$((EVM_BASE + k))"
             --utxoindex --unsaferpc)
    if [ "$grpc" = 1 ]; then a+=("--rpclisten=127.0.0.1:$((GRPC_BASE + k))"); else a+=(--nogrpc); fi
    # The flag days every node runs, the old release included: below the IR fence the two binaries agree.
    a+=("--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT")
    if [ "$role" = old ]; then
        # The fleet's release: keyless, peered to new0 only, the same salt and flag days, and no IR fence
        # (it has no --palw-drill-tir-at): past TIR_AT its fork id is not the new nodes' (D-F4).
        a+=("--addpeer=127.0.0.1:$(p2p new0)")
        printf '%s\n' "${a[@]}" "$@"
        return
    fi
    a+=("--ram-scale=$RAM_SCALE" "--palw-host-memory-share=$(( $(cat "$d/share-mib" 2>/dev/null || echo "$SHARE_MIB") * 1048576))"
        "--palw-drill-tir-at=$TIR_AT")
    [ -n "$TIR2_AT" ] && a+=("--palw-drill-tir2-at=$TIR2_AT")
    if [ "$seat" != - ]; then
        a+=("--palw-producer-key=$KR/bond-$seat.seed"
            "--palw-producer-bond=$(manifest "m['seats'][$seat]['bond_outpoint']")"
            "--palw-fee-outpoint=$(manifest "m['seats'][$seat]['fee_float_outpoint']")")
    fi
    [ "$ir" = 1 ] && [ "$DF1" = 1 ] && a+=("--palw-class-artifact=$IR_ARTIFACT")
    [ "$ir" = 1 ] && [ -s "$SMALL_ARTIFACT" ] && a+=("--palw-class-artifact=$SMALL_ARTIFACT")
    case $role in
        floor) a+=(--palw-produce) ;;
        ir) if [ "$DF1" = 1 ]; then
                a+=(--palw-produce "--palw-producer-class=$(ir_class_id)")
                class_on_chain "$(ir_class_id)" || a+=("--palw-register-class=$IR_MODEL_ID")
            fi ;;
        ir2) if [ -s "$WORK_DIR/small-class.id" ]; then
                a+=(--palw-produce "--palw-producer-class=$(small_class_id)")
                class_on_chain "$(small_class_id)" || a+=("--palw-register-class=$SMALL_MODEL_ID")
            fi ;;
    esac
    [ "$hb" = 1 ] && a+=("--palw-heartbeat-miner-address=$(manifest "m['heartbeat'][$k]['address']")" --enable-unsynced-mining)
    local m; for m in $(new_nodes); do [ "$m" = "$n" ] || a+=("--addpeer=127.0.0.1:$(p2p "$m")"); done
    # Per-node additions (D-F2's tamper leaf, a challenger's --palw-challenge), one per line.
    local extra="$d/extra-args"; if [ -s "$extra" ]; then while read -r x; do [ -n "$x" ] && a+=("$x"); done < "$extra"; fi
    printf '%s\n' "${a[@]}" "$@"
}

# The CLI against a node, salted, with its own HOME.
cli() { local n=$1; shift; HOME=$UHOME "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$(borsh "$n")" --palw-drill-genesis-salt="$(salt)" "$@"; }
rpc() { python3 "$A/rpc.py" "$@"; }
tip() { python3 "$A/rpc.py" call --port "$(jport "${1:-new3}")" getBlockDagInfo '{}' 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("virtualDaaScore","?"))'; }
