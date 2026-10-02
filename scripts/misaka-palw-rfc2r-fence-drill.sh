#!/usr/bin/env bash
# misaka-palw-rfc2r-fence-drill.sh — lane R2's chain drill (RFC-0002 Part II §II.7.5 Proposal A and Phase 0(a)'s exit drill (f)): a SALTED
# testnet-12 chain on this Mac, on the SHIPPING kaspad of rfc2/rest, with FOUR nodes (the lane cap), crossing in one run
#
#   the first three post-launch flag days (--palw-drill-fence-at / -fence2-at / -fence3-at, as the release crosses them),
#   palw_gdn_key_heads, testnet-12's fourth (--palw-drill-fence4-at: P0a, Phase 0(a)),
#   palw_tir_v1 (--palw-drill-tir-at) and palw_gen_v1 (--palw-drill-gen-at), the seating fence's prerequisites, and
#   palw_class_seating (--palw-drill-class-seating-at: RFC-0002 Part II Proposal A, dormant on every preset).
#
# Four nodes: g0 and g1 (genesis seats 0 and 1, the heartbeat clocks), g2 (seat 2, a floor producer: stopped below the seating fence and
# restarted past it, so it crosses the fence down), and x3 (a fresh key-less joiner started past every fence, so it syncs across all of them).
# Eight genesis seats exist in the keyring; four nodes run, so no panel of five forms and no claim is attempted: what this drill shows is
# that the shipping binary crosses the heights with ONE consensus state on every node (palwStateRoot, utxoCommitment, claims, vesting,
# registry, panel seats ALL-EQUAL at checkpoints around each flag day), that a node down across the seating fence and a fresh node syncing
# across it agree, and that the clock does not stall. The semantics of the rule itself (SEAT-1..11) are the processor tests of
# `consensus/core/tests/palw_class_seating_fence.rs` and the consensus processor tests: a class's claims cannot be attempted here without
# the eight seats' nodes (the Mac holds four).
#
#   BIN=<dir holding this branch's release kaspad and misaka> [RUN=~/Downloads/MISAKA-wt-b/rfc2r-fence-run] \
#   [FENCE_AT=20 FENCE2_AT=30 FENCE3_AT=40 FENCE4_AT=60 TIR_AT=70 GEN_AT=80 SEAT_AT=100] \
#     bash scripts/misaka-palw-rfc2r-fence-drill.sh preflight|start|status|verdict|stop
#
# Same machinery and rules as scripts/misaka-palw-p0a-fence4-drill.sh (which it is derived from): `stop` sends SIGINT only to PIDs recorded under
# RUN; never SIGKILL; nothing this script did not start. The drill takes the lane lock (~/Downloads/MISAKA-wt-b/lanes/DRILL.lock) itself in
# `preflight`'s refusal if it is held by another lane, and never creates or removes it: the lane takes it and releases it.
#
# WHAT THE VERDICT CHECKS
#   V1 identity   every node prints the keyring manifest's consensus params fingerprint, a "PALW DRILL FLAG DAY" line naming
#                 palw_gdn_key_heads at FENCE4_AT and palw_class_seating at SEAT_AT, and a fence schedule holding both heights.
#   V2 crossing   the sampler's checkpoints around every flag day are ALL-EQUAL (sink, palwStateRoot, utxoCommitment and the digests of
#                 getPalwClaims, getPalwVesting, getPalwModelRegistry, getPalwPanelSeats on every running node).
#   V3 clock      the chain's DAA passes SEAT_AT+40.
#   V4 down       g2, stopped below the seating fence and started past it, is in the ALL-EQUAL rows from SEAT_AT+20 on.
#   V5 joiner     x3, a fresh key-less node started past every fence, is in the ALL-EQUAL rows from SEAT_AT+20 on.
#   V6 seating    getPalwModelRegistry on g0 serves the class rows' seating after the fence (MANUAL when the registry holds no class row).
#   V7 disagree   no synced node disagreed with the majority sink for more than two samples in a row.
set -euo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
RUN=${RUN:-$HOME/Downloads/MISAKA-wt-b/rfc2r-fence-run}
SEC=$RUN/secrets
KR=$SEC/keyring
BIN=${BIN:-}
FENCE_AT=${FENCE_AT:-20}; FENCE2_AT=${FENCE2_AT:-30}; FENCE3_AT=${FENCE3_AT:-40}; FENCE4_AT=${FENCE4_AT:-60}
TIR_AT=${TIR_AT:-70}; GEN_AT=${GEN_AT:-80}; SEAT_AT=${SEAT_AT:-100}
# Disjoint from the other drills (36100+, 46100+/48100+, 51100+, 61100+).
P2P_BASE=55100; BORSH_BASE=55200; JSON_BASE=55300; EVM_BASE=55400
RAM_SCALE=${RAM_SCALE:-0.3}
SHARE_MIB=${SHARE_MIB:-1536}
MIN_FREE_PCT=${MIN_FREE_PCT:-40}
MIN_DISK_GIB=${MIN_DISK_GIB:-25}

# name k seat produce hb build   (seat '-' = no genesis bond; build new|old)
NODES="g0 0 0 none 1 new
g1 1 1 none 1 new
g2 2 2 floor 0 new
x3 3 - none 0 new"

say() { printf '[rfc2r-drill %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { say "REFUSED: $*"; exit 1; }
row() { echo "$NODES" | awk -v n="$1" '$1==n'; }
field() { local r; r=$(row "$1"); [ -n "$r" ] || die "no node $1"; echo "$r" | awk -v i="$2" '{print $i}'; }
kof() { field "$1" 2; }
all_nodes() { echo "$NODES" | awk '{print $1}'; }
genesis_seats() { echo "$NODES" | awk '$3 != "-" {print $1}'; }
bin_of() { echo "$BIN"; }
salt() { local s; s=$(tr -d ' \n' < "$SEC/SALT"); [[ "$s" =~ ^[0-9a-f]{64}$ ]] || die "bad $SEC/SALT"; echo "$s"; }
manifest() { python3 -c "import json,sys; m=json.load(open(sys.argv[1])); print($1)" "$KR/manifest.json"; }
free_pct() { memory_pressure 2>/dev/null | awk -F': ' '/System-wide memory free percentage/ {gsub("%","",$2); print $2}'; }
running() { local d=$RUN/$1; [ -f "$d/kaspad.pid" ] && ps -p "$(cat "$d/kaspad.pid")" -o command= 2>/dev/null | grep -q -- "--appdir=$d/app"; }
fence_flags() {   # every fence the drill crosses, as flags
    printf '%s\n' "--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT" \
        "--palw-drill-fence4-at=$FENCE4_AT" "--palw-drill-tir-at=$TIR_AT" "--palw-drill-gen-at=$GEN_AT" "--palw-drill-class-seating-at=$SEAT_AT"
}
rpc() {   # rpc <port> <method> [json params] — one wRPC-JSON call through the repo's stdlib client
    python3 - "$REPO/scripts" "$@" <<'PY'
import json, sys
sys.path.insert(0, sys.argv[1])
from misaka_wrpc_json import WsRpc
port, method = int(sys.argv[2]), sys.argv[3]
params = json.loads(sys.argv[4]) if len(sys.argv) > 4 else {}
c = WsRpc(port=port, timeout=10)
try:
    print(json.dumps(c.call(method, params)))
finally:
    c.close()
PY
}

# node_args <name> — the kaspad argv, one per line.
node_args() {
    local n=$1 k seat produce hb d build; k=$(kof "$n"); seat=$(field "$n" 3); produce=$(field "$n" 4); hb=$(field "$n" 5); build=$(field "$n" 6); d=$RUN/$n
    local a=(--testnet --netsuffix=12 "--palw-drill-genesis-salt=$(salt)" "--appdir=$d/app" --yes
             --nodnsseed --disable-upnp --nogrpc
             "--listen=127.0.0.1:$((P2P_BASE + k))" "--rpclisten-borsh=127.0.0.1:$((BORSH_BASE + k))"
             "--rpclisten-json=127.0.0.1:$((JSON_BASE + k))" "--evm-rpc-listen=127.0.0.1:$((EVM_BASE + k))"
             --utxoindex --unsaferpc "--ram-scale=$RAM_SCALE" "--palw-host-memory-share=$((SHARE_MIB * 1048576))")
    if [ "$seat" != - ]; then
        a+=("--palw-producer-key=$KR/$(manifest "m['seats'][$seat]['seed_file']")"
            "--palw-producer-bond=$(manifest "m['seats'][$seat]['bond_outpoint']")"
            "--palw-fee-outpoint=$(manifest "m['seats'][$seat]['fee_float_outpoint']")")
    fi
    [ "$produce" = floor ] && a+=(--palw-produce)
    [ "$hb" = 1 ] && a+=("--palw-heartbeat-miner-address=$(manifest "m['heartbeat'][$k]['address']")" --enable-unsynced-mining)
    local m; for m in $(all_nodes); do [ "$m" = "$n" ] || a+=("--addpeer=127.0.0.1:$((P2P_BASE + $(kof "$m")))"); done
    local x; while IFS= read -r x; do a+=("$x"); done < <(fence_flags "$build")
    printf '%s\n' "${a[@]}"
}

start_node() {
    local n=$1 d=$RUN/$1 b; b=$(bin_of "$1")
    running "$n" && { say "$n: already running (pid $(cat "$d/kaspad.pid"))"; return 0; }
    local free; free=$(free_pct); [ "${free:-0}" -ge "$MIN_FREE_PCT" ] || die "memory free ${free:-?}% < ${MIN_FREE_PCT}% — not starting $n"
    mkdir -p "$d/home"
    local A=() x; while IFS= read -r x; do A+=("$x"); done < <(node_args "$n")
    printf '%s\n' "${A[@]}" | sed -E 's/salt=[0-9a-f]{64}/salt=<SALT>/' > "$d/args.redacted"
    local c; c=$(stat -f %z "$d/kaspad.out" 2>/dev/null || echo 0); echo "$c" > "$d/start.cursor"
    ( cd "$d"; ulimit -n 10240; HOME="$d/home" nohup nice -n 10 "$b/kaspad" "${A[@]}" >> "$d/kaspad.out" 2>&1 & echo $! > "$d/kaspad.pid" )
    echo "$(date '+%F %T') START pid $(cat "$d/kaspad.pid") cursor $c build $(field "$n" 6)" >> "$d/events.log"
    local g i; g=$(manifest "m['genesis_hash']")
    for i in $(seq 1 60); do
        if tail -c +$((c + 1)) "$d/kaspad.out" 2>/dev/null | grep -E "PALW DRILL CHAIN .*genesis $g" > /dev/null; then
            say "$n: up (pid $(cat "$d/kaspad.pid"))"; return 0
        fi
        ps -p "$(cat "$d/kaspad.pid")" > /dev/null 2>&1 || die "$n DIED at start: $(tail -c +$((c + 1)) "$d/kaspad.out" | tail -3 | sed -E 's/[0-9a-f]{64}/<64hex>/g')"
        sleep 2
    done
    die "$n: no drill genesis line in 120 s"
}

stop_node() {
    local n=$1 d=$RUN/$1 pid i
    running "$n" || { say "$n: not running"; return 0; }
    pid=$(cat "$d/kaspad.pid"); kill -INT "$pid"; echo "$(date '+%F %T') STOP(INT) pid $pid" >> "$d/events.log"
    for i in $(seq 1 90); do ps -p "$pid" > /dev/null 2>&1 || { say "$n: stopped"; return 0; }; sleep 1; done
    say "$n: still alive after 90 s (pid $pid) — left alone"; return 1
}

tip() { rpc "$((JSON_BASE + $(kof "${1:-g0}")))" getBlockDagInfo 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("virtualDaaScore", 0))' 2>/dev/null || echo 0; }

cmd_preflight() {
    local failed=0 b H n k base busy="" other free disk
    ok() { echo "  ok   $*"; }; bad() { echo "  FAIL $*"; failed=1; }
    echo "== R2 chain drill preflight ($(date '+%F %T')): fences $FENCE_AT/$FENCE2_AT/$FENCE3_AT/$FENCE4_AT tir $TIR_AT gen $GEN_AT seating $SEAT_AT, run dir $RUN"
    [ "$(uname -s)" = Darwin ] || bad "a Mac drill (loopback only); on a fleet host use scripts/misaka-palw-t12-rcore-drill.sh"
    [ "$FENCE_AT" -lt "$FENCE2_AT" ] && [ "$FENCE2_AT" -lt "$FENCE3_AT" ] && [ "$FENCE3_AT" -lt "$FENCE4_AT" ] && [ "$FENCE4_AT" -lt "$TIR_AT" ] \
        && [ "$TIR_AT" -lt "$GEN_AT" ] && [ "$GEN_AT" -lt "$SEAT_AT" ] && [ "$FENCE_AT" -ge 10 ] \
        && ok "heights ordered as the prerequisites need them (tir < gen < seating)" || bad "heights must be 10 <= FENCE_AT < FENCE2_AT < FENCE3_AT < FENCE4_AT < TIR_AT < GEN_AT < SEAT_AT"
    [ -n "$BIN" ] || bad "BIN is required (the release build of rfc2/rest — the binary that ships)"
    for b in kaspad misaka; do
        [ -n "$BIN" ] && [ -x "$BIN/$b" ] && ok "$BIN/$b ($(shasum -a 256 "$BIN/$b" | cut -c1-16))" || bad "$BIN/$b missing"
    done
    if [ -n "$BIN" ] && [ -x "$BIN/kaspad" ]; then
        H=$("$BIN/kaspad" --help 2>/dev/null || true)
        for f in --palw-drill-genesis-salt --palw-drill-write-keyring --palw-drill-fence-at --palw-drill-fence2-at --palw-drill-fence3-at --palw-drill-fence4-at --palw-drill-tir-at --palw-drill-gen-at --palw-drill-class-seating-at; do
            grep -q -- "$f" <<< "$H" && ok "kaspad lists $f" || bad "kaspad lacks $f (not this branch's build)"
        done
    fi
    [ -e "$RUN/g0/app" ] && bad "$RUN/g0/app exists — refusing to reuse a drill's chain (a new RUN, or remove it)" || ok "no drill datadir at $RUN"
    for n in $(all_nodes); do k=$(kof "$n"); for base in $P2P_BASE $BORSH_BASE $JSON_BASE $EVM_BASE; do
        lsof -nP -iTCP:$((base + k)) -sTCP:LISTEN > /dev/null 2>&1 && busy="$busy $((base + k))"; done; done
    [ -z "$busy" ] && ok "ports free (${P2P_BASE}+ … ${EVM_BASE}+)" || bad "ports in use:$busy"
    [ -d "$HOME/Downloads/MISAKA-wt-b/lanes/DRILL.lock" ] && [ "$(cat "$HOME/Downloads/MISAKA-wt-b/lanes/DRILL.lock/owner" 2>/dev/null)" != "${LANE:-R2}" ] \
        && bad "the lane lock DRILL.lock is held by $(cat "$HOME/Downloads/MISAKA-wt-b/lanes/DRILL.lock/owner" 2>/dev/null): one drill at a time" || ok "the lane lock is ours or free"
    other=$(pgrep -f -- "--palw-drill-genesis-salt" 2>/dev/null | wc -l | tr -d ' ')
    [ "${other:-0}" = 0 ] && ok "no other drill kaspad runs" \
        || { [ "${ALLOW_CONCURRENT:-0}" = 1 ] && ok "$other other drill kaspad(s) run — ALLOW_CONCURRENT=1" || bad "$other other drill kaspad(s) run: two drills at once have rebooted this Mac (ALLOW_CONCURRENT=1 overrides)"; }
    free=$(free_pct); [ "${free:-0}" -ge "$MIN_FREE_PCT" ] && ok "memory free ${free}%" || bad "memory free ${free:-?}% < ${MIN_FREE_PCT}%"
    disk=$(df -g "$HOME" | awk 'NR==2 {print $4}'); [ "${disk:-0}" -ge "$MIN_DISK_GIB" ] && ok "disk free ${disk} GiB" || bad "disk free ${disk:-?} GiB < $MIN_DISK_GIB"
    [ -r "$REPO/scripts/misaka_wrpc_json.py" ] && ok "wRPC client $REPO/scripts/misaka_wrpc_json.py" || bad "missing scripts/misaka_wrpc_json.py"
    [ "$failed" = 0 ] || { echo "== preflight FAILED"; return 1; }
    echo "== preflight ok"
}

write_sampler() {
    cat > "$RUN/sampler.py" <<'PY'
#!/usr/bin/env python3
"""The P0a drill's sampler — read-only wRPC-JSON to this drill's loopback nodes (the fence drills' fencewatch.py, in one
file). Every 10 s each node's (virtualDaaScore, sink) -> sinks.tsv. A synced node (DAA >= max-1) whose sink differs from
the majority's for > 2 samples in a row -> disagree.tsv. At each target DAA, once every running node is synced on one
sink, each node's view is digested -> checkpoints.tsv (ALL-EQUAL or DIFFER)."""
import hashlib, json, os, sys, time
sys.path.insert(0, os.environ["P0A_SCRIPTS"])
from misaka_wrpc_json import WsRpc
RUN = os.environ["P0A_RUN"]
JSON_BASE = int(os.environ["P0A_JSON_BASE"])
NODES = json.loads(os.environ["P0A_NODES"])            # {name: k}
TARGETS = [int(x) for x in os.environ["P0A_TARGETS"].split(",")]
BONDS = json.loads(os.environ["P0A_BONDS"])            # {"g2": outpoint, "g5": ..., "g3": ...}
# A node the drill EXPECTS to be partitioned from a height on (the pre-P0a build at the fence): once the chain's tip is
# there it is left out of the majority, the disagreement streaks and the checkpoints — it is behind by design.
PARTITIONED = json.loads(os.environ.get("P0A_PARTITIONED", "{}"))   # {name: height}
VOLATILE = {"tipDaa", "measuredMsPerDaa", "nextDaa", "readySeatsNow", "tip_daa"}

def call(port, method, params=None, timeout=10):
    try:
        c = WsRpc(port=port, timeout=timeout)
        try:
            return c.call(method, params or {}) or {}
        finally:
            c.close()
    except Exception as e:  # noqa: BLE001 — a node down or mid-restart is a missing sample, not a crash
        return {"error": str(e)}

def strip(o):
    if isinstance(o, dict): return {k: strip(v) for k, v in o.items() if k not in VOLATILE}
    if isinstance(o, list): return [strip(x) for x in o]
    return o

def md5(o): return hashlib.md5(json.dumps(strip(o), sort_keys=True).encode()).hexdigest()[:12]

def digest(port):
    dag = call(port, "getBlockDagInfo")
    sink = str(dag.get("sink", ""))
    hd = call(port, "getBlock", {"hash": sink, "includeTransactions": False}).get("block", {}).get("header", {})
    parts = [md5(call(port, "getPalwClaims", {"bond": BONDS["g2"], "role": "executor", "includeTerminal": True, "limit": 0})),
             md5(call(port, "getPalwClaims", {"bond": BONDS["g5"], "role": "executor", "includeTerminal": True, "limit": 0})),
             md5(call(port, "getPalwClaims", {"bond": BONDS["g3"], "role": "seat", "includeTerminal": True, "limit": 0})),
             md5(call(port, "getPalwVesting", {"bond": "", "payoutAddress": "", "claimId": "", "limit": 0, "after": ""})),
             md5(call(port, "getPalwModelRegistry")), md5(call(port, "getPalwPanelSeats", {"classId": ""}))]
    return dag.get("virtualDaaScore"), sink[:16], str(hd.get("palwStateRoot", ""))[:16], str(hd.get("utxoCommitment", ""))[:16], "/".join(parts)

def main():
    streak, done = {}, set()
    if os.path.exists(f"{RUN}/checkpoints.tsv"):
        done = {int(l.split("\t")[1]) for l in open(f"{RUN}/checkpoints.tsv") if l.startswith("CP")}
    while True:
        now, view = time.strftime("%F %T"), {}
        for n, k in NODES.items():
            d = call(JSON_BASE + k, "getBlockDagInfo", timeout=5)
            if "error" not in d and d.get("sink"):
                view[n] = (int(d.get("virtualDaaScore") or 0), str(d.get("sink", ""))[:16])
        with open(f"{RUN}/sinks.tsv", "a") as f:
            f.write(now + "\t" + " ".join(f"{n}={v[0]}:{v[1][:8]}" for n, v in view.items()) + "\n")
        if view:
            top = max(v[0] for v in view.values())
            view = {n: v for n, v in view.items() if not (n in PARTITIONED and top >= PARTITIONED[n])}
            if not view:
                time.sleep(10)
                continue
            synced = {n: v for n, v in view.items() if v[0] >= top - 1}
            count = {}
            for v in synced.values(): count[v[1]] = count.get(v[1], 0) + 1
            majority = max(count, key=count.get)
            for n, v in synced.items():
                streak[n] = streak.get(n, 0) + 1 if v[1] != majority else 0
                if streak[n] > 2:
                    with open(f"{RUN}/disagree.tsv", "a") as f:
                        f.write(f"{now}\t{n}\t{v[0]}:{v[1]}\tmajority {majority} ({count[majority]}/{len(synced)})\tstreak {streak[n]}\n")
            for t in TARGETS:
                if t in done: continue
                if len(synced) == len(view) and min(v[0] for v in view.values()) >= t and len(count) == 1:
                    for _ in range(6):   # a block landing mid-capture moves one node's sink: capture again
                        rows = {n: digest(JSON_BASE + NODES[n]) for n in view}
                        if len({r[1] for r in rows.values()}) == 1: break
                        time.sleep(3)
                    same = len({r[1:] for r in rows.values()}) == 1
                    with open(f"{RUN}/checkpoints.tsv", "a") as f:
                        f.write(f"CP\t{t}\t{now}\t{'ALL-EQUAL' if same else 'DIFFER'}\t{len(rows)} nodes\t{' '.join(sorted(rows))}\n")
                        for n, r in sorted(rows.items()):
                            f.write(f"  \t{t}\t{n}\tdaa={r[0]} sink={r[1]} palwStateRoot={r[2]} utxoCommitment={r[3]} rpc={r[4]}\n")
                    done.add(t)
        time.sleep(10)

if __name__ == "__main__":
    main()
PY
}

targets() { echo "$((FENCE_AT + 1)),$((FENCE2_AT + 1)),$((FENCE3_AT + 1)),$((FENCE4_AT - 1)),$FENCE4_AT,$((FENCE4_AT + 5)),$((TIR_AT + 1)),$((GEN_AT + 1)),$((SEAT_AT - 5)),$((SEAT_AT - 1)),$SEAT_AT,$((SEAT_AT + 1)),$((SEAT_AT + 5)),$((SEAT_AT + 20)),$((SEAT_AT + 40))"; }

cmd_start() {
    cmd_preflight || exit 1
    mkdir -p "$SEC"; chmod 700 "$SEC"
    echo "== salt and keyring"
    [ -s "$SEC/SALT" ] || ( umask 077; openssl rand -hex 32 > "$SEC/SALT" )
    chmod 600 "$SEC/SALT"
    if [ ! -e "$KR/manifest.json" ]; then
        mkdir -p "$KR" "$SEC/keyring-app"; chmod 700 "$KR"
        local F=() x; while IFS= read -r x; do F+=("$x"); done < <(fence_flags new)
        "$BIN/kaspad" --testnet --netsuffix=12 --appdir="$SEC/keyring-app" --palw-drill-genesis-salt="$(salt)" "${F[@]}" \
            --palw-drill-write-keyring="$KR" 2>&1 | tail -2 | sed -E 's/[0-9a-f]{64}/<64hex>/g'
        chmod 600 "$KR"/*
    fi
    [ "$(manifest "m['format']")" = misaka-palw-drill-keyring/v1 ] || die "unexpected keyring format"
    [ "$(manifest "m.get('fence4_at')")" = "$FENCE4_AT" ] || die "the keyring was written for fence4_at=$(manifest "m.get('fence4_at')"), not $FENCE4_AT"
    [ "$(manifest "m.get('class_seating_at')")" = "$SEAT_AT" ] || die "the keyring was written for class_seating_at=$(manifest "m.get('class_seating_at')"), not $SEAT_AT"
    manifest "'genesis', m['genesis_hash'][:16], 'params', m['consensus_params_id'][:16], 'salt_id', m['salt_id'], 'seats', len(m['seats']), 'fences', m['fence_at'], m['fence2_at'], m['fence3_at'], m['fence4_at'], 'seating', m.get('class_seating_at')"
    echo "== nodes"
    local n
    for n in $(genesis_seats); do start_node "$n"; sleep 3; done
    echo "== sampler and scheduled actions (background)"
    write_sampler
    local nodes_json bonds_json
    nodes_json=$(python3 -c "import json,sys; print(json.dumps({l.split()[0]: int(l.split()[1]) for l in sys.argv[1].splitlines()}))" "$NODES")
    bonds_json=$(python3 -c "import json; m=json.load(open('$KR/manifest.json')); print(json.dumps({'g2': m['seats'][2]['bond_outpoint'], 'g5': m['seats'][5]['bond_outpoint'], 'g3': m['seats'][3]['bond_outpoint']}))")
    ( P0A_SCRIPTS="$REPO/scripts" P0A_RUN="$RUN" P0A_JSON_BASE="$JSON_BASE" P0A_NODES="$nodes_json" P0A_TARGETS="$(targets)" P0A_BONDS="$bonds_json" \
        nohup python3 "$RUN/sampler.py" >> "$RUN/sampler.out" 2>&1 & echo $! > "$RUN/sampler.pid" )
    ( at() { until [ "$(tip g0)" -ge "$1" ] 2>/dev/null; do sleep 10; done; }
      act() { echo "$(date '+%F %T') $* (tip $(tip g0))" >> "$RUN/actions.log"; }
      at $((SEAT_AT - 4)); act "stop g2 (below the seating fence)"; stop_node g2 >> "$RUN/actions.log" 2>&1 || true
      at $((SEAT_AT + 4)); act "start g2 (past the seating fence)"; start_node g2 >> "$RUN/actions.log" 2>&1 || true
      at $((SEAT_AT + 6)); act "start x3 (fresh key-less joiner, IBD across every flag day)"; start_node x3 >> "$RUN/actions.log" 2>&1 || true
      act "actions done" ) > /dev/null 2>&1 &
    echo $! > "$RUN/actions.pid"
    say "started: sampler $(cat "$RUN/sampler.pid"), actions $(cat "$RUN/actions.pid"); \`$0 verdict\` once the tip passes $((FENCE4_AT + 45))"
}

cmd_status() {
    local n pid rss st peers
    for n in $(all_nodes); do
        if running "$n"; then
            pid=$(cat "$RUN/$n/kaspad.pid"); rss=$(ps -o rss= -p "$pid" | awk '{printf "%.0f", $1/1024}')
            st=$(rpc "$((JSON_BASE + $(kof "$n")))" getBlockDagInfo 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("virtualDaaScore","?"), str(d.get("sink",""))[:12])' 2>/dev/null || echo "? ?")
            peers=$(rpc "$((JSON_BASE + $(kof "$n")))" getConnectedPeerInfo 2>/dev/null | python3 -c 'import json,sys; print(len(json.load(sys.stdin).get("peerInfo",[])))' 2>/dev/null || echo "?")
            printf '%-3s %-3s pid %-6s rss %5s MiB  daa/sink %s  peers %s\n' "$n" "$(field "$n" 6)" "$pid" "$rss" "$st" "$peers"
        else
            printf '%-3s %-3s down\n' "$n" "$(field "$n" 6)"
        fi
    done
    echo "mac free $(free_pct)%"
}

cmd_verdict() {
    local fails=0 manual=0 n want got t line
    pass() { echo "  PASS $*"; }; fail() { echo "  FAIL $*"; fails=1; }; man() { echo "  MANUAL $*"; manual=1; }
    [ -f "$KR/manifest.json" ] || die "no keyring under $RUN — was the drill started?"
    want=$(manifest "m['consensus_params_id']")
    echo "== V1 identity"
    for n in $(all_nodes); do
        [ -f "$RUN/$n/kaspad.out" ] || continue
        got=$(grep -m1 -o -E 'Consensus params fingerprint: [0-9a-f]+' "$RUN/$n/kaspad.out" | awk '{print $4}')
        [ "$got" = "$want" ] && pass "$n prints the manifest's fingerprint ${want:0:16}" || fail "$n prints '${got:0:16}', the manifest ${want:0:16}"
        grep -q -E "PALW DRILL FLAG DAY: .*palw_gdn_key_heads.*$FENCE4_AT" "$RUN/$n/kaspad.out" && pass "$n names palw_gdn_key_heads at $FENCE4_AT" \
            || fail "$n has no PALW DRILL FLAG DAY line naming palw_gdn_key_heads at $FENCE4_AT"
        grep -q -E "PALW DRILL FLAG DAY: .*palw_class_seating.*$SEAT_AT" "$RUN/$n/kaspad.out" && pass "$n names palw_class_seating at $SEAT_AT" \
            || fail "$n has no PALW DRILL FLAG DAY line naming palw_class_seating at $SEAT_AT"
        grep -q -E "Consensus fence schedule: (.*[, ])?$SEAT_AT(,| |$)" "$RUN/$n/kaspad.out" && pass "$n schedules a fence at $SEAT_AT" \
            || fail "$n's fence schedule has no fence at $SEAT_AT"
    done
    echo "== V2 crossing"
    [ -f "$RUN/checkpoints.tsv" ] || fail "no checkpoints.tsv (the sampler never saw every node synced at a target)"
    for t in $(targets | tr , ' '); do
        line=$(grep -E "^CP	$t	" "$RUN/checkpoints.tsv" 2>/dev/null | tail -1 || true)
        case "$line" in
            *ALL-EQUAL*) pass "DAA $t ALL-EQUAL ($(echo "$line" | cut -f5-))" ;;
            *DIFFER*) fail "DAA $t DIFFER — see checkpoints.tsv" ;;
            *) fail "DAA $t not reached with every node synced" ;;
        esac
    done
    echo "== V3 clock"
    t=$(awk -F'\t' '{n = split($2, a, " "); for (i = 1; i <= n; i++) { split(a[i], b, "="); split(b[2], c, ":"); if (c[1] + 0 > m) m = c[1] + 0 } } END {print m + 0}' "$RUN/sinks.tsv" 2>/dev/null || echo 0)
    [ "${t:-0}" -gt 0 ] || t=$(tip g0)
    [ "${t:-0}" -ge $((SEAT_AT + 40)) ] && pass "the chain reached DAA $t, past $((SEAT_AT + 40))" || fail "the chain reached DAA ${t:-?}, not past $((SEAT_AT + 40))"
    echo "== V4 down across the seating fence / V5 fresh joiner"
    for n in g2 x3; do
        for t in $((SEAT_AT + 20)) $((SEAT_AT + 40)); do
            line=$(grep -E "^CP	$t	" "$RUN/checkpoints.tsv" 2>/dev/null | tail -1 || true)
            if echo "$line" | grep -q ALL-EQUAL && echo "$line" | cut -f6 | tr ' ' '\n' | grep -qx "$n"; then pass "$n in the ALL-EQUAL row at $t"
            else fail "$n not in an ALL-EQUAL row at $t"; fi
        done
    done
    echo "== V6 seating on the registry read"
    got=$(rpc "$((JSON_BASE + $(kof g0)))" getPalwModelRegistry 2>/dev/null || true)
    if echo "$got" | grep -q '"seating"'; then pass "getPalwModelRegistry serves the classes' seating past the fence"
    else man "getPalwModelRegistry holds no class row with a seating read on this chain (no class was registered here)"; fi
    echo "== V7 disagreement"
    [ ! -s "$RUN/disagree.tsv" ] && pass "no disagreement streak" || fail "$(wc -l < "$RUN/disagree.tsv" | tr -d ' ') disagreement row(s) in disagree.tsv"
    echo "== not checked here (see the header): the seating rule's semantics on claims — the processor and core tests (SEAT-1..11); the Mac holds four nodes, a panel needs five"
    if [ "$fails" = 0 ]; then echo "== VERDICT: PASS$([ "$manual" = 1 ] && echo " (with MANUAL items)")"; else echo "== VERDICT: FAIL"; return 1; fi
}

cmd_stop() {
    local f pid n
    for f in actions sampler; do
        if [ -f "$RUN/$f.pid" ]; then
            pid=$(cat "$RUN/$f.pid")
            if ps -p "$pid" -o command= 2>/dev/null | grep -q -E "sampler.py|bash"; then kill -INT "$pid" 2>/dev/null || true; say "$f ($pid) stopped"; fi
        fi
    done
    for n in $(all_nodes); do stop_node "$n" & done
    wait
}

case "${1:-}" in
    preflight) cmd_preflight ;;
    start) cmd_start ;;
    status) cmd_status ;;
    verdict) cmd_verdict ;;
    stop) cmd_stop ;;
    *) sed -n '2,50p' "$0"; exit 2 ;;
esac
