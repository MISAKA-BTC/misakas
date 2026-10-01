# audit-improve/lib-dm.sh — sourced by the D-M drills' runner (RFC-0004 A13: D-M1 the whole epoch, D-M2 a candidate that
# must not win, D-M3 the court battery on evaluation claims, D-M4 rollback, D-M5 the fence crossing, D-M6 copying,
# leakage, grinding and fee DoS). After audit-tir/lib-df.sh (the D-F drills of RFC-0002).
#
# ONE salted testnet-12 drill chain on this Mac, loopback only: its own salt ($WORK_DIR/SALT, 0600, never printed — or SALT
# from the environment), its own keyring (written by the shipping kaspad itself), its own ports, its own app dirs under
# $WORK_DIR. Never a fleet host, never a public node's app dir or port, never another drill's.
#
# The model side (`dm.sh model`, offline) builds, from the tiny HF llama fixture, the head class H (byte-identical to D-F's
# small class), a candidate that WINS a synthetic exact-match pool, one that LOSES (it breaks what the parent passes), and a
# composite (LoRA adapter section) of the winner — see audit-improve/modeltool.py.
set -euo pipefail
A=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)          # audit-improve/
WT=$(cd "$A/.." && pwd)
WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-improve-drill}
# BIN_DIR: the release under test (kaspad, misaka, palw-class, palw-tir-fidelity) — the one parameter a run points at a build;
# each binary can still be named on its own.
BIN_DIR=${BIN_DIR:-}
KASPAD_BIN=${KASPAD_BIN:-${BIN_DIR:+$BIN_DIR/kaspad}}
CLI_BIN=${CLI_BIN:-$(dirname "${KASPAD_BIN:-/nonexistent/kaspad}")/misaka}
# D-M5's other side: the release BEFORE the improvement fence (the fleet's release, per the coordinator). No default: the
# fleet's release is the coordinator's to name (OLD_KASPAD_BIN), and `dm.sh dry` says which fence it first lacks.
OLD_KASPAD_BIN=${OLD_KASPAD_BIN:-}
TOOLS_BIN=${TOOLS_BIN:-$(dirname "${KASPAD_BIN:-/nonexistent/kaspad}")}
VENV_PY=${VENV_PY:-$HOME/Downloads/MISAKA-wt-b/tir-venv/bin/python}
SALT=${SALT:-}
KR=$WORK_DIR/keyring
UHOME=$WORK_DIR/userhome          # HOME for every CLI call (never the real ~/.misaka)
MODEL_DIR=$WORK_DIR/model
VERDICT_DIR=$WORK_DIR/verdict

# The flag days, low, so the drill runs the rules the live chain has when the fences arm. The first four are D-F's (distinct
# heights, fence1 <= fence3); then the second IR fence, RFC-0003's generative fence, the decode rules an evaluation claim needs
# (the flag — if the build has one — is detected, never assumed), and RFC-0004's improvement fence last: validate_palw_v2
# refuses the improvement fence unless palw_tir_v1, palw_tir_fence2, palw_gen_v1 and palw_kary_court are in force at or below it.
FENCE_AT=${FENCE_AT:-6}; FENCE2_AT=${FENCE2_AT:-10}; FENCE3_AT=${FENCE3_AT:-14}; TIR_AT=${TIR_AT:-20}
TIR2_AT=${TIR2_AT:-24}; GEN_AT=${GEN_AT:-28}; DECODE_AT=${DECODE_AT:-32}; IMPROVE_AT=${IMPROVE_AT:-40}
DECODE_FLAG=${DECODE_FLAG:-}     # e.g. --palw-drill-decode-at; empty = look for one in kaspad --help

# The head: the tiny HF llama at 32 positions — D-F's small class (class eaabaff9…) — and the model ids of the candidates.
FIXTURE=${FIXTURE:-$WT/misaka-palw-tir-lower/tests/fixtures/hf/llama}
HEAD_MODEL_ID=${HEAD_MODEL_ID:-test/llama-tiny-ir}
HEAD_CONTEXT=${HEAD_CONTEXT:-32}
# What a candidate IS: `composite` (parent + PALWTIRS adapter section: RFC-0004's own form, the default — a seat fetches only the
# adapter, proves possession of it under `adapter_root` and of the parent as its own class, spec 17 §17.7.1) or `full` (the adapter
# merged into a full-weight class: an ordinary IR class; the form of the drills before the core lane's composite readiness, kept as
# the extra line). The plan names its candidates `win` and `lose` and one `extra`; asset_of() says which model asset each is.
CAND_FORM=${CAND_FORM:-composite}
# asset_of <win|lose|extra|head> — the model asset (`$MODEL_DIR/<asset>.class.palwtir`, ids/<asset>.class) a plan name stands for.
# composite: win=winc, lose=losec (the adapters as composite classes), extra=win (the full-weight winner, riding along on line L);
# full: win=win, lose=lose (full-weight), extra=winc (the composite winner).
asset_of() {
    case "$CAND_FORM:$1" in
        composite:win) echo winc ;; composite:lose) echo losec ;; composite:extra) echo win ;;
        full:extra) echo winc ;;
        *) echo "$1" ;;
    esac
}

# Ports: the coordinator's range for this drill — disjoint from the D-F drill (55100+), lane D's (40100+) and public nodes.
P2P_BASE=${P2P_BASE:-61100}; BORSH_BASE=${BORSH_BASE:-62100}; JSON_BASE=${JSON_BASE:-63100}
EVM_BASE=${EVM_BASE:-64100}; GRPC_BASE=${GRPC_BASE:-60100}
RAM_SCALE=${RAM_SCALE:-0.3}
SHARE_MIB=${SHARE_MIB:-1024}      # each node's replay share: the artifacts are tens of KiB each, the scratch is the cost

# The node table:  name k seat role hb ir
#   seat  the genesis bond (keyring seats[n]); '-' = no keys
#   role  floor  a floor producer (the chain's clock between heartbeats, and the admission jury's anchor)
#         head   the head class H's registrant and producer (its claims are the line's usage), and its line's owner
#         eval   an evaluation executor (--palw-improve-evaluate): runs the epoch's jobs and carries their claims
#         evalw  an executor that also produces claims of the winner W (the usage of the line once W heads it: D-M4's second epoch)
#         seat   a seat only;   old  the release before the fence, keyless, a relay peered to new0 (D-M5)
#   hb    1 = a heartbeat clock;  ir  1 = loads the head and the candidates' artifacts (a ready seat: t12 needs 5 + 2 of them)
# Seven IR holders — the fewest t12's registry admits — and the old relay: eight nodes. Seat 7 has no node: it is the drills'
# own bond (the lines it founds, the candidates it submits and registers, the setters and the material), so its carriers are
# funded by the keyring's main wallet and never compete with a panel's fee float.
NODES="new0 0 3 floor 0 1
new1 1 0 seat 1 1
new2 2 1 seat 1 1
new3 3 2 seat 0 1
new4 4 4 head 0 1
new5 5 5 eval 0 1
new6 6 6 evalw 0 1
old 7 - old 0 0"

# NO_OLD=1: no old relay (D-M5 is then not run) — eight nodes become seven.
[ "${NO_OLD:-0}" = 1 ] && NODES=$(echo "$NODES" | grep -v '^old ')

say() { printf '[improve-dm %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { say "REFUSED: $*"; exit 1; }
row() { echo "$NODES" | awk -v n="$1" '$1==n'; }
field() { local r; r=$(row "$1"); [ -n "$r" ] || die "no node $1"; echo "$r" | awk -v i="$2" '{print $i}'; }
kof() { field "$1" 2; }
p2p() { echo $((P2P_BASE + $(kof "$1"))); }
borsh() { echo $((BORSH_BASE + $(kof "$1"))); }
jport() { echo $((JSON_BASE + $(kof "$1"))); }
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
# The ids `dm.sh model` writes: head, win, lose, winc, losec (a class id, hex) and each one's artifact root.
model_id() { tr -d ' \n' < "$MODEL_DIR/ids/$1.${2:-class}" 2>/dev/null || true; }

# The fences a drill arms, `name|flag|height`, in the order the node applies them.
fence_rows() {
    cat <<EOF
fence1|--palw-drill-fence-at|$FENCE_AT
fence2|--palw-drill-fence2-at|$FENCE2_AT
fence3|--palw-drill-fence3-at|$FENCE3_AT
tir|--palw-drill-tir-at|$TIR_AT
tir2|--palw-drill-tir2-at|$TIR2_AT
gen|--palw-drill-gen-at|$GEN_AT
decode|${DECODE_FLAG:-@detect}|$DECODE_AT
improve|--palw-drill-improve-at|$IMPROVE_AT
EOF
}
# A decode-rules flag, if the build has one: --palw-drill-<…decode…>-at.
detect_decode_flag() { { grep -oE -- '--palw-drill-[a-z0-9-]*decode[a-z0-9-]*-at' <<<"$1" || true; } | head -1; }
# The `flag=height` list of the fences a binary (its --help text in $1) lists; the rest are the ones it lacks. Argument 2 = lacks.
fence_args() {
    local help=$1 want=${2:-has} name flag at dflag
    dflag=${DECODE_FLAG:-$(detect_decode_flag "$help")}
    while IFS='|' read -r name flag at; do
        [ "$flag" = "@detect" ] && flag=$dflag
        if [ -n "$flag" ] && grep -q -- "$flag" <<<"$help"; then
            [ "$want" = has ] && echo "$flag=$at"
        else
            [ "$want" = lacks ] && echo "$name@$at"
        fi
    done < <(fence_rows)
    return 0
}
bin_help() { "$1" --help 2>/dev/null || true; }

# node_args <name> [extra…] — the kaspad argv, one per line.
node_args() {
    local n=$1; shift
    local k seat role hb ir d
    k=$(kof "$n"); seat=$(field "$n" 3); role=$(field "$n" 4); hb=$(field "$n" 5); ir=$(field "$n" 6)
    d=$WORK_DIR/$n
    local a=(--testnet --netsuffix=12 "--palw-drill-genesis-salt=$(salt)" "--appdir=$d/app" --yes
             --nodnsseed --disable-upnp
             "--listen=127.0.0.1:$((P2P_BASE + k))" "--rpclisten-borsh=127.0.0.1:$((BORSH_BASE + k))"
             "--rpclisten-json=127.0.0.1:$((JSON_BASE + k))" "--evm-rpc-listen=127.0.0.1:$((EVM_BASE + k))"
             --utxoindex --unsaferpc --nogrpc)
    local bin=$KASPAD_BIN
    [ "$role" = old ] && bin=$OLD_KASPAD_BIN
    local f; while read -r f; do [ -n "$f" ] && a+=("$f"); done < <(fence_args "$(bin_help "$bin")" has)
    if [ "$role" = old ]; then
        # The release before the fence: keyless, peered to new0 only, the same salt and every flag day it lists. Past the first
        # fence it lacks its fork id is not the new nodes' (D-M5).
        a+=("--addpeer=127.0.0.1:$(p2p new0)")
        printf '%s\n' "${a[@]}" "$@"
        return
    fi
    a+=("--ram-scale=$RAM_SCALE" "--palw-host-memory-share=$(( $(cat "$d/share-mib" 2>/dev/null || echo "$SHARE_MIB") * 1048576 ))")
    if [ "$seat" != - ]; then
        a+=("--palw-producer-key=$KR/bond-$seat.seed"
            "--palw-producer-bond=$(manifest "m['seats'][$seat]['bond_outpoint']")"
            "--palw-fee-outpoint=$(manifest "m['seats'][$seat]['fee_float_outpoint']")")
    fi
    if [ "$ir" = 1 ]; then
        # The head first (a composite section opens over it), then the full-weight candidates every seat must hold to be a ready
        # seat for them: both in the full form, the winner alone in the composite form (the extra line's candidate). The composite
        # candidates' sections are NOT loaded: they sit in the drop directory, and the seats that see a candidate fetch its section
        # (adapter prefetch, RFC-0004 §6.7) and then prove possession of it.
        a+=("--palw-class-artifact=$MODEL_DIR/head.class.palwtir")
        [ -s "$MODEL_DIR/win.class.palwtir" ] && a+=("--palw-class-artifact=$MODEL_DIR/win.class.palwtir")
        [ "$CAND_FORM" = full ] && [ -s "$MODEL_DIR/lose.class.palwtir" ] && a+=("--palw-class-artifact=$MODEL_DIR/lose.class.palwtir")
        a+=("--palw-improve-artifact-dir=$MODEL_DIR/drop")
    fi
    case $role in
        floor) a+=(--palw-produce) ;;
        head) a+=(--palw-produce "--palw-register-class=$HEAD_MODEL_ID" "--palw-producer-class=$(model_id head)") ;;
        eval) a+=(--palw-improve-evaluate) ;;
        evalw) a+=(--palw-improve-evaluate --palw-produce "--palw-producer-class=$(model_id "$(asset_of win)")") ;;
    esac
    [ "$hb" = 1 ] && a+=("--palw-heartbeat-miner-address=$(manifest "m['heartbeat'][$k]['address']")" --enable-unsynced-mining)
    local m; for m in $(new_nodes); do [ "$m" = "$n" ] || a+=("--addpeer=127.0.0.1:$(p2p "$m")"); done
    # Per-node additions (a tamper flag, a challenger), one per line.
    local extra="$d/extra-args"; if [ -s "$extra" ]; then while read -r x; do [ -n "$x" ] && a+=("$x"); done < "$extra"; fi
    printf '%s\n' "${a[@]}" "$@"
}

# The CLI against a node, salted, with its own HOME.
cli() { local n=$1; shift; HOME=$UHOME "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$(borsh "$n")" --palw-drill-genesis-salt="$(salt)" "$@"; }
rpc() { python3 "$A/rpc.py" "$@"; }
tip() { python3 "$A/rpc.py" call --port "$(jport "${1:-new0}")" getBlockDagInfo '{}' 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("virtualDaaScore","?"))'; }

# The environment the Python driver reads (everything it needs to find a node, a key, a tool or a model file).
export_env() {
    export SALT WORK_DIR KASPAD_BIN CLI_BIN OLD_KASPAD_BIN TOOLS_BIN VENV_PY KR UHOME MODEL_DIR VERDICT_DIR CAND_FORM NODES
    export FENCE_AT FENCE2_AT FENCE3_AT TIR_AT TIR2_AT GEN_AT DECODE_AT IMPROVE_AT
    export P2P_BASE BORSH_BASE JSON_BASE EVM_BASE GRPC_BASE
}
