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
# The int-11 combined chain's other fences (the flag set is NOT frozen until the integration order is given: a flag the build under test
# does not list is reported by `dm.sh dry` and left out, never assumed). Heights, all distinct from each other and from the flag days:
# gen and decode-rules stay below the improvement fence (validate_palw_v2: RFC-0004's fence needs tir, tir2, gen, kary court and the decode
# rules at or below it, and it is pinned at DAA 40: line R's epoch 1 opens at the 255 boundary, so the head's first Final claim, ~252, must
# be after it); the model court at 36; lane D's tensor-claim carriage (fp-v5) at 104 and held-chunk closes (tag 90) at 140; lane F's class
# seating at 240 (F's SEAT slots S-2..S+55 = 238..295, before the first evaluation claims at 323); the capacity steps at 560 (rho 25) and 655 (rho 100) — they need the rho-10 flag day (--palw-drill-fence3-at, 14) below them.
MODEL_COURT_AT=${MODEL_COURT_AT:-36}; FPV5_AT=${FPV5_AT:-104}; HELD_AT=${HELD_AT:-140}; SEAT_AT=${SEAT_AT:-240}
LATE_AT=${LATE_AT:-103}           # lane D's DG-2: the embedding class is registered late, at this DAA (one below the fp-v5 fence)
GEN_DIR=${GEN_DIR:-$WORK_DIR/gen} # lane D's Plan B class artifacts (drill-classes.json and the .class.palwtir2 files)
CAP2_AT=${CAP2_AT:-560}; CAP3_AT=${CAP3_AT:-655}
# INT11=1 (the default since the release layer): the int-11 flag day as the release arms it — ONE flag, --palw-drill-int11-at=H', moves the whole
# list (decode rules, gen, FP Job V5, held leaf challenge, improvement, L_ver, rho 25) to H' and rho 100 to H' + 95; the per-fence flags of
# rows 8-17 are refused with it, and the model-court window is armed nowhere on the release, so the drill does not arm it either. Every height
# the scenarios name for those fences is therefore H' (GEN_AT = DECODE_AT = IMPROVE_AT = FPV5_AT = HELD_AT = CAP2_AT = H'; LATE_AT = H' + 1;
# CAP3_AT = H' + 95). H' sits before the first evaluation grid (255) and after the head's registration, and ahead of the capacity windows
# (H' + 10 .. + 90 at rho 25, H' + 105 .. + 185 at rho 100). INT11=0 keeps the pre-release per-fence layout. Lane F's class seating has
# no flag in this tree: the `seat` scenarios are skipped.
INT11=${INT11:-1}
if [ "$INT11" = 1 ]; then
    INT11_AT=${INT11_AT:-150}
    GEN_AT=$INT11_AT; DECODE_AT=$INT11_AT; IMPROVE_AT=$INT11_AT; FPV5_AT=$INT11_AT; HELD_AT=$INT11_AT
    CAP2_AT=$INT11_AT; CAP3_AT=$((INT11_AT + 95)); LATE_AT=$((INT11_AT + 1))   # LATE_AT: the embedding class is registered right after the gen fence (a registration below it is dropped) and a claim follows at once: not yet ready
    XB_FROM_DAA=${XB_FROM_DAA:-$((INT11_AT + 4))}
fi
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
# asset_of <win|lose|head> — the model asset (`$MODEL_DIR/<asset>.class.palwtir`, ids/<asset>.class) a plan name stands for ON LINE W1, the
# composite line of D-M1 (the producer of W1's promoted head is configured by it): composite: win=winc, lose=losec (the adapters as composite
# classes); full: win=win, lose=lose (full-weight). The other lines' forms are drill.json's (`form` per line; the driver reads them): W2 and L
# are full-weight, T is composite.
asset_of() {
    case "$CAND_FORM:$1" in
        composite:win) echo winc ;; composite:lose) echo losec ;;
        *) echo "$1" ;;
    esac
}

# Ports: the coordinator's range for this drill — disjoint from the D-F drill (55100+), lane D's (40100+) and public nodes.
P2P_BASE=${P2P_BASE:-61100}; BORSH_BASE=${BORSH_BASE:-62100}; JSON_BASE=${JSON_BASE:-63100}
EVM_BASE=${EVM_BASE:-64100}; GRPC_BASE=${GRPC_BASE:-60100}
# LOWMEM=1: the ram-scale 0.25 variant (the Mac's memory while other lanes' nodes run). The replay share stays: the panel's capacity numbers the
# capacity line measures are not to move with the memory profile. Measured RSS at 0.3 is ~2.1 GB a node; 0.25 is an estimate (~1.9 GB: the 1 GiB share does
# not scale) until a node is measured — the driver's sampler writes $WORK_DIR/memory.tsv from the first tick and `dm.sh status` prints the first hour's figure.
if [ "${LOWMEM:-0}" = 1 ]; then RAM_SCALE=${RAM_SCALE:-0.25}; else RAM_SCALE=${RAM_SCALE:-0.3}; fi
SHARE_MIB=${SHARE_MIB:-1024}      # each node's replay share: the artifacts are tens of KiB each, the scratch is the cost

# The node table:  name k seat role hb ir
#   seat  the genesis bond (keyring seats[n]); '-' = no keys
#   role  floor  a floor producer (the chain's clock between heartbeats, and the admission jury's anchor)
#         head   the head class H's registrant and producer (its claims are the line's usage), and its line's owner
#         eval   an evaluation executor (--palw-improve-evaluate): runs the epoch's jobs and carries their claims
#         evalw  an executor that also produces claims of the winner W (the usage of the line once W heads it: D-M4's second epoch)
#         seat   a seat only;   old  the release before the fence, keyless, a relay peered to new0 (D-M5)
#         liar   a SACRIFICIAL evaluator (D-M3: new7 lies a step leaf, new8 a first id): its bond is a post-genesis one (bonds 8 and 9 of the
#                keyring, registered by the driver), and the drill's capacity package arms F-L from DAA 14, so one CourtFraud conviction (AG-2)
#                voids the bond's live claims and freezes it for good — which is why it is not one of the seven seats. Started just-in-time
#                (the `jit` mark, field 7: `up` does not start it; the driver starts it before T's window, it syncs from its peers, lies, is
#                convicted and is stopped), so the steady state is seven nodes.
#         reg    the registrar: one run per post-genesis bond (the liars' 8 and 9, lane D's 10..13): `kaspad --palw-register-bond` for the key
#                and pay address the driver writes to its extra-args, which prints the bond outpoint and stops
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
new7 7 8 liar 0 1 jit
new8 8 9 liar 0 1 jit
reg 9 - reg 0 0 jit
old 10 - old 0 0"

# OUTSIDER=1 (the combined drill of the DAA-5,300 candidate): one more seat, an OUTSIDER — a post-genesis bond (bond 14, registered FIRST by the driver from DAA
# XB_FROM_DAA) on its own node new9, started by the driver once its bond is registered and never stopped: the panels then seat an operator that is not
# a genesis one. Ten processes would be too many for this Mac, so the driver also stops the old relay as soon as D-M5's crossing is done.
if [ "${OUTSIDER:-0}" = 1 ]; then NODES="$NODES
new9 11 14 outsider 0 1 jit
extfloor 12 15 extfloor 0 0 jit
joiner 13 - joiner 0 0 jit"; fi
# RIDERS=N: --palw-riders=N on every producing node (ADR-0164 F-M1: N further jobs of the producer's own bond per lead attempt).

# NO_OLD=1: no old relay (D-M5 is then not run). The just-in-time nodes (liars, registrar) never run in the steady state: seven nodes + the relay.
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
new_nodes() { echo "$NODES" | awk '$4!="old" && $4!="reg" {print $1}'; }
# The nodes that are up from the start (the ones a new node peers with): the just-in-time nodes are not.
peer_nodes() { echo "$NODES" | awk '$4!="old" && $4!="reg" && $7!="jit" {print $1}'; }
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
    if [ "${INT11:-1}" = 1 ]; then
        cat <<EOF
fence1|--palw-drill-fence-at|$FENCE_AT
fence2|--palw-drill-fence2-at|$FENCE2_AT
fence3|--palw-drill-fence3-at|$FENCE3_AT
tir|--palw-drill-tir-at|$TIR_AT
tir2|--palw-drill-tir2-at|$TIR2_AT
int11|--palw-drill-int11-at|$INT11_AT
EOF
        [ -n "${USEFUL_WORK_AT:-}" ] && echo "useful_work|--palw-drill-useful-work-at|$USEFUL_WORK_AT"
        return 0
    fi
    cat <<EOF
fence1|--palw-drill-fence-at|$FENCE_AT
fence2|--palw-drill-fence2-at|$FENCE2_AT
fence3|--palw-drill-fence3-at|$FENCE3_AT
tir|--palw-drill-tir-at|$TIR_AT
tir2|--palw-drill-tir2-at|$TIR2_AT
gen|--palw-drill-gen-at|$GEN_AT
decode|${DECODE_FLAG:-@detect}|$DECODE_AT
improve|--palw-drill-improve-at|$IMPROVE_AT
model_court|--palw-drill-model-court-at|$MODEL_COURT_AT
fp_v5|--palw-drill-fp-v5-at|$FPV5_AT
held_chunks|--palw-drill-held-chunks-at|$HELD_AT
capacity_step2|--palw-drill-capacity-step2-at|$CAP2_AT
capacity_step3|--palw-drill-capacity-step3-at|$CAP3_AT
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
             "--listen=127.0.0.1:$((P2P_BASE + k + $([ "${ISOLATE_NODE:-}" = "$n" ] && echo "${ISO_PORT_SHIFT:-800}" || echo 0))))" "--rpclisten-borsh=127.0.0.1:$((BORSH_BASE + k))"
             "--rpclisten-json=127.0.0.1:$((JSON_BASE + k))" "--evm-rpc-listen=127.0.0.1:$((EVM_BASE + k))"
             --utxoindex --unsaferpc --nogrpc)
    local bin=$KASPAD_BIN
    [ "$role" = old ] && bin=$OLD_KASPAD_BIN
    local f; while read -r f; do [ -n "$f" ] && a+=("$f"); done < <(fence_args "$(bin_help "$bin")" has)
    if [ "$role" = old ]; then
        # The release before the fence: keyless, peered to new0 only, the same salt and every flag day it lists. Past the first
        # fence it lacks its fork id is not the new nodes' (D-M5).
        a+=("--addpeer=127.0.0.1:$(p2p new0)")
        # The release before the fence: the same memory profile as the others where it has the flag (LOWMEM's point is every node's footprint).
        grep -q -- "--ram-scale" <<<"$(bin_help "$bin")" && a+=("--ram-scale=$RAM_SCALE")
        printf '%s\n' "${a[@]}" "$@"
        return
    fi
    a+=("--ram-scale=$RAM_SCALE" "--palw-host-memory-share=$(( $(cat "$d/share-mib" 2>/dev/null || echo "$SHARE_MIB") * 1048576 ))")
    # INT12=1 (the combined drill): p2p flow_context and the heartbeat relay at DEBUG, so H2's 'kept, not announced' line (a second beat for a slot is validated but not announced: a 1-slot sink split)
    # is captured directly. Few debug lines in those modules; LOGLEVEL overrides.
    [ "${INT12:-0}" = 1 ] && a+=("--loglevel=${LOGLEVEL:-info,kaspa_p2p_flows::flow_context=debug,kaspa_p2p_flows::palw_heartbeat_relay=debug}")
    if [ "$seat" != - ] && [ "$seat" -lt 8 ]; then
        a+=("--palw-producer-key=$KR/bond-$seat.seed"
            "--palw-producer-bond=$(manifest "m['seats'][$seat]['bond_outpoint']")"
            "--palw-fee-outpoint=$(manifest "m['seats'][$seat]['fee_float_outpoint']")")
    elif [ "$seat" != - ]; then
        # A post-genesis bond (the liars'): the driver's extra-bond step wrote its outpoint and a fee float to $WORK_DIR/liars/bond-<n>.json.
        local bj=$WORK_DIR/liars/bond-$seat.json
        [ -s "$bj" ] || die "bond $seat is not registered yet ($bj): the driver registers it first"
        a+=("--palw-producer-key=$KR/bond-$seat.seed"
            "--palw-producer-bond=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['bond_outpoint'])" "$bj")"
            "--palw-fee-outpoint=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['fee_outpoint'])" "$bj")")
    fi
    if [ "$ir" = 1 ]; then
        # The head first (a composite section opens over it), then the full-weight candidates every seat must hold to be a ready seat
        # for them (the extra lines W2 and L hold full-weight candidates in every form). The composite candidates' sections are NOT
        # loaded: they sit in the drop directory, and the seats that see a candidate fetch its section (adapter prefetch, RFC-0004
        # §6.7) and then prove possession of it.
        a+=("--palw-class-artifact=$MODEL_DIR/head.class.palwtir")
        [ -s "$MODEL_DIR/win.class.palwtir" ] && a+=("--palw-class-artifact=$MODEL_DIR/win.class.palwtir")
        [ -s "$MODEL_DIR/lose.class.palwtir" ] && a+=("--palw-class-artifact=$MODEL_DIR/lose.class.palwtir")
        a+=("--palw-improve-artifact-dir=$MODEL_DIR/drop")
        # Lane D's RFC-0003 Plan B classes (toy-image, toy-embed, wide-embed; `gen_drill_classes --ignored` writes them to $GEN_DIR): every IR holder loads all of
        # them from the start, the wide class included, as it loads the head before the head is registered (a class the chain does not list yet is not an error).
        # GEN_PARTIAL_CLASS=<name>: that gen class is held only by GEN_PARTIAL_HOLDERS (fewer than five: the readiness gate, DG-2's GenClassNotReady); dg2 starts the
        # others with the class once it has seen the refusal.
        local g; for g in "$GEN_DIR"/*.class.palwtir2; do
            if [ -n "${GEN_PARTIAL_CLASS:-}" ] && [ "$(basename "$g")" = "$GEN_PARTIAL_CLASS.class.palwtir2" ] && ! grep -qw "$n" <<<"${GEN_PARTIAL_HOLDERS:-}"; then continue; fi
            [ -s "$g" ] && a+=("--palw-class-artifact=$g")
        done
    fi
    local rid=(); [ "${RIDERS:-0}" -gt 0 ] && rid=("--palw-riders=$RIDERS")
    case $role in
        floor) a+=(--palw-produce)
               # ANCHOR_DUTY_AFTER_SLOTS=N (RS's --palw-drill-anchor-duty-after-slots; the release's value is 30): how long a claim waits for an operator attempt, with the floor held as the idle-only
               # fallback, before this OPERATOR floor producer fires its anchor-duty binder (one floor the fold refuses); added only where the binary lists the flag
               if [ -n "${ANCHOR_DUTY_AFTER_SLOTS:-}" ] && grep -q -- "--palw-drill-anchor-duty-after-slots" <<<"$(bin_help "$bin")"; then a+=("--palw-drill-anchor-duty-after-slots=$ANCHOR_DUTY_AFTER_SLOTS"); fi ;;
        # OUTSIDER_PRODUCE=1: the outsider (a NON-operator bond) also makes REAL attempts of class win, riders on (G-A3 splits bind waits by operator / non-operator bond)
        outsider) if [ "${OUTSIDER_PRODUCE:-0}" = 1 ]; then a+=(--palw-produce "${rid[@]}" "--palw-producer-class=$(model_id win)"); fi ;;
        # extfloor (OUTSIDER=1): the EXTERNAL floor producer of the combined drill (post-genesis bond 15): a floor producer that does not honour the idle rule where the build has
        # a drill flag for that (EXT_FLOOR_FLAG, else the first `--palw-drill-*floor*` flag the binary lists besides the fence movers), so its blocks must be REJECTED in
        # Normal / Probe by the honest nodes' header rule. Without such a flag it is an ordinary floor producer (the gate then reports it could not test rejection).
        extfloor) a+=(--palw-produce)
                  local ef=${EXT_FLOOR_FLAG:-$(bin_help "$bin" | grep -oE -- '--palw-drill-[a-z0-9-]*floor[a-z0-9-]*' | grep -vE -- '-at$|reserve' | head -1)}
                  [ -n "$ef" ] && grep -q -- "$ef" <<<"$(bin_help "$bin")" && a+=("$ef") ;;
        # HEAD_PRODUCE=0: new4 is a SEAT only (it still registers the class, and holds every artifact): the combined drill's first leg lets only the delayed producer
        # (new6) make REAL attempts, all of them stale; new4 produces from leg B on.
        head) if [ "${HEAD_PRODUCE:-1}" = 0 ]; then a+=("--palw-register-class=$HEAD_MODEL_ID")
              else a+=(--palw-produce "${rid[@]}" "--palw-register-class=$HEAD_MODEL_ID" "--palw-producer-class=$(model_id head)"); fi ;;
        eval|liar) a+=(--palw-improve-evaluate) ;;
        # evalw produces the FULL-WEIGHT winner's claims: the usage of line R once `win` heads it (D-M4's second epoch). W1's composite winner
        # has no second epoch any more, so nothing produces winc.
        evalw) a+=(--palw-improve-evaluate --palw-produce "${rid[@]}" "--palw-producer-class=$(model_id win)")
               # REAL_SUBMIT_DELAY_S=N (lane RS's --palw-drill-real-submit-delay-s): this ONE REAL producer holds each attempt N s before it submits it, the
               # way an 8k model infers (the live failure: fast floor attempts fill its anticone meanwhile and it turns RED). new4 stays fast. A binary
               # without the flag runs without it and `dc.sh dry` says so.
               if [ -n "${REAL_SUBMIT_DELAY_S:-}" ] && grep -q -- "--palw-drill-real-submit-delay-s" <<<"$(bin_help "$bin")"; then
                   a+=("--palw-drill-real-submit-delay-s=$REAL_SUBMIT_DELAY_S"); fi ;;
    esac
    [ "$hb" = 1 ] && a+=("--palw-heartbeat-miner-address=$(manifest "m['heartbeat'][$k]['address']")" --enable-unsynced-mining)
    # ISOLATE_NODE=<n>: that node is partitioned — it listens on a shifted P2P port (nobody dials the old one) and dials nobody (FORK-c: a reorg across the fence)
    local m; if [ "${ISOLATE_NODE:-}" != "$n" ]; then for m in $(peer_nodes); do [ "$m" = "$n" ] || a+=("--addpeer=127.0.0.1:$(p2p "$m")"); done; fi
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
    export LOGLEVEL INT12 USEFUL_WORK_AT ANCHOR_DUTY_AFTER_SLOTS OUTSIDER_PRODUCE HEAD_PRODUCE ISOLATE_NODE ISO_PORT_SHIFT EXT_FLOOR_FLAG REAL_SUBMIT_DELAY_S GEN_PARTIAL_CLASS GEN_PARTIAL_HOLDERS OUTSIDER RIDERS INT12 INT11 INT11_AT XB_FROM_DAA FENCE_AT FENCE2_AT FENCE3_AT TIR_AT TIR2_AT GEN_AT DECODE_AT IMPROVE_AT MODEL_COURT_AT FPV5_AT HELD_AT SEAT_AT CAP2_AT CAP3_AT LATE_AT GEN_DIR
    export P2P_BASE BORSH_BASE JSON_BASE EVM_BASE GRPC_BASE
}
