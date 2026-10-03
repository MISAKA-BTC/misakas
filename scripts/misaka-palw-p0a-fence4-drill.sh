#!/usr/bin/env bash
# misaka-palw-p0a-fence4-drill.sh — P0a's exit drill (f) (docs/design/palw/tir/phase-f-integration.md §3.3 on
# tir/phase-f): a SALTED testnet-12 chain on this Mac, on the SHIPPING kaspad of rcore/p0a-gdn-key-heads, crossing
# testnet-12's fourth post-launch flag day — `palw_gdn_key_heads`, the list PALW_T12_POST_LAUNCH_FENCES_V4 — at a low
# height (--palw-drill-fence4-at), after the first three flag days moved lower still, as the release will cross them.
#
# WRITTEN, NOT RUN (2026-09-28): the coordinator runs it when the Mac is free. Two drills at once have rebooted this Mac
# (memory: eight artifact nodes, four dense nodes): `preflight` refuses while any other salted drill kaspad runs.
#
#   BIN=<dir holding this branch's release kaspad and misaka> [OLD_BIN=<dir holding a pre-P0a kaspad>] \
#   [FENCE_AT=20] [FENCE2_AT=30] [FENCE3_AT=40] [FENCE4_AT=60] [RUN=~/Downloads/MISAKA-wt-b/p0a-fence4-run] \
#     bash scripts/misaka-palw-p0a-fence4-drill.sh preflight|start|status|verdict|stop
#
#   preflight  the checks `start` runs first, and nothing else: no salt, keyring, datadir or process is created.
#   start      salt (RUN/secrets/SALT, 0600, never printed) → keyring (this kaspad's --palw-drill-write-keyring with all
#              four fence flags) → g0..g7 (the eight genesis seats: g0, g1 heartbeat clocks; g2, g5 floor producers; every
#              seat bonded, so every node runs its seat duties) → with OLD_BIN, o8 (the pre-P0a build, a key-less relay,
#              fences 1–3 only) → in the background: the sampler (RUN/sampler.py) and the scheduled actions (stop g6 at
#              FENCE4_AT-4 and start it at FENCE4_AT+4, so it crosses the fence while down; start x9, a fresh key-less
#              joiner, at FENCE4_AT+6, so it syncs across all four flag days).
#   status     per node: pid, RSS, DAA, sink, peers; the Mac's free memory.
#   verdict    PASS/FAIL per check (below), from RUN's files only. Exit 0 = every automated check PASS.
#   stop       SIGINT to this drill's nodes and background jobs (only PIDs recorded under RUN whose command line
#              names RUN), then waits. Never SIGKILL; never a process this script did not start.
#
# WHAT THE VERDICT CHECKS
#   V1 identity   every node of the new build prints the keyring manifest's consensus params fingerprint, a
#                 "PALW DRILL FLAG DAY" line naming palw_gdn_key_heads ARMED at FENCE4_AT, and a fence schedule holding
#                 that height (the schedule line prints heights, not names).
#   V2 crossing   the sampler's checkpoints around every flag day — FENCE4_AT-5, -1, 0, +1, +5, +20, +40 and one past each
#                 of the first three — are ALL-EQUAL: every running node reports the same sink and, through that sink,
#                 the same header palwStateRoot and utxoCommitment and the same digests of getPalwClaims (g2, g5
#                 executors; g3 seat), getPalwVesting, getPalwModelRegistry and getPalwPanelSeats.
#   V3 clock      the chain's DAA passes FENCE4_AT+40: the fence stalls nothing.
#   V4 down       g6, stopped below the fence and started past it, is in the ALL-EQUAL rows from FENCE4_AT+20 on.
#   V5 joiner     x9, a fresh key-less node started past every fence, is in the ALL-EQUAL rows from FENCE4_AT+20 on.
#   V6 old build  (OLD_BIN only; e.g. a Mac build of rcore/int-6 b1a1e8736, which differs from this branch's base by
#                 tests alone) the pre-P0a relay o8 peers and agrees BELOW the fence — a scheduled fence is normalised out
#                 of consensus_identity_id, so a rollout does not partition — and PAST it a new node refuses it by the
#                 fork id ("Fork-id mismatch … crossed fence FENCE4_AT"); the sampler leaves o8 out from that height.
#                 Without OLD_BIN: MANUAL.
#   V7 disagree   no synced node disagreed with the majority sink for more than two samples in a row (disagree.tsv).
#
# WHAT IT CANNOT CHECK, AND WHY (read before calling the drill complete)
#   The class the fence exists for cannot register on testnet-12 — on either side of the fence. testnet-12 arms
#   palw_offence_attribution at DAA 0, and past it ADR-0152 §4-ter C5 refuses every HELD class with a recurrent layer
#   (HeldClassUnanswerable::Recurrent); graph-v8 is held by construction (profile version 3 on the held map v5). So a
#   graph-v8 registration on this drill is refused below the fence by the fence (GdnKeyHeadsNeedsItsFence) and past it
#   by C5 — and no node carries one anyway: this build's catalog has no graph-v8 row (a registration needs a catalog
#   row or a manifest over the 33.5 GiB artifact). The admission half is the processor test's (exit test (e),
#   consensus/src/pipeline/virtual_processor/tests/t47_p0a_gdn_key_heads.rs): below the fence refused by name with the
#   block standing, past it admitted on the attribution-off twin, C5's refusal on testnet-12 as shipped, and a seeded
#   graph-v8 class produced, disputed and closed. What this drill adds is the part a test cannot: the shipping binary
#   crossing the height with the other three flag days, a node down across it, a fresh node syncing across it, and the
#   old build partitioned by the fork id rather than by accident.
set -euo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
RUN=${RUN:-$HOME/Downloads/MISAKA-wt-b/p0a-fence4-run}
SEC=$RUN/secrets
KR=$SEC/keyring
BIN=${BIN:-}
OLD_BIN=${OLD_BIN:-}
FENCE_AT=${FENCE_AT:-20}; FENCE2_AT=${FENCE2_AT:-30}; FENCE3_AT=${FENCE3_AT:-40}; FENCE4_AT=${FENCE4_AT:-60}
# Disjoint from drill A (36100+), the fence drills (46100+/48100+) and drill C (51100+).
P2P_BASE=61100; BORSH_BASE=62100; JSON_BASE=63100; EVM_BASE=64100
RAM_SCALE=${RAM_SCALE:-0.3}
SHARE_MIB=${SHARE_MIB:-1536}
MIN_FREE_PCT=${MIN_FREE_PCT:-40}
MIN_DISK_GIB=${MIN_DISK_GIB:-25}

# name k seat produce hb build   (seat '-' = no genesis bond; build new|old)
NODES="g0 0 0 none 1 new
g1 1 1 none 1 new
g2 2 2 floor 0 new
g3 3 3 none 0 new
g4 4 4 none 0 new
g5 5 5 floor 0 new
g6 6 6 none 0 new
g7 7 7 none 0 new
o8 8 - none 0 old
x9 10 - none 0 new"

say() { printf '[p0a-drill %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { say "REFUSED: $*"; exit 1; }
row() { echo "$NODES" | awk -v n="$1" '$1==n'; }
field() { local r; r=$(row "$1"); [ -n "$r" ] || die "no node $1"; echo "$r" | awk -v i="$2" '{print $i}'; }
kof() { field "$1" 2; }
all_nodes() { echo "$NODES" | awk '{print $1}'; }
genesis_seats() { echo "$NODES" | awk '$3 != "-" {print $1}'; }
bin_of() { if [ "$(field "$1" 6)" = old ]; then echo "$OLD_BIN"; else echo "$BIN"; fi; }
salt() { local s; s=$(tr -d ' \n' < "$SEC/SALT"); [[ "$s" =~ ^[0-9a-f]{64}$ ]] || die "bad $SEC/SALT"; echo "$s"; }
manifest() { python3 -c "import json,sys; m=json.load(open(sys.argv[1])); print($1)" "$KR/manifest.json"; }
free_pct() { memory_pressure 2>/dev/null | awk -F': ' '/System-wide memory free percentage/ {gsub("%","",$2); print $2}'; }
running() { local d=$RUN/$1; [ -f "$d/kaspad.pid" ] && ps -p "$(cat "$d/kaspad.pid")" -o command= 2>/dev/null | grep -q -- "--appdir=$d/app"; }
fence_flags() {   # the four flags for the new build, the first three for the old one
    printf '%s\n' "--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT"
    [ "${1:-new}" = old ] || printf '%s\n' "--palw-drill-fence4-at=$FENCE4_AT"
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
    [ -n "$b" ] || die "$n needs OLD_BIN"
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

tip() { rpc "$((JSON_BASE + $(kof "${1:-g3}")))" getBlockDagInfo 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("virtualDaaScore", 0))' 2>/dev/null || echo 0; }

cmd_preflight() {
    local failed=0 b H n k base busy="" other free disk
    ok() { echo "  ok   $*"; }; bad() { echo "  FAIL $*"; failed=1; }
    echo "== P0a drill preflight ($(date '+%F %T')): fences $FENCE_AT/$FENCE2_AT/$FENCE3_AT/$FENCE4_AT, run dir $RUN"
    [ "$(uname -s)" = Darwin ] || bad "a Mac drill (loopback only); on a fleet host use scripts/misaka-palw-t12-rcore-drill.sh"
    [ "$FENCE_AT" -lt "$FENCE2_AT" ] && [ "$FENCE2_AT" -lt "$FENCE3_AT" ] && [ "$FENCE3_AT" -lt "$FENCE4_AT" ] && [ "$FENCE_AT" -ge 10 ] \
        && ok "heights ordered as the release crosses them" || bad "heights must be 10 <= FENCE_AT < FENCE2_AT < FENCE3_AT < FENCE4_AT"
    [ -n "$BIN" ] || bad "BIN is required (the release build of rcore/p0a-gdn-key-heads — the binary that ships)"
    for b in kaspad misaka; do
        [ -n "$BIN" ] && [ -x "$BIN/$b" ] && ok "$BIN/$b ($(shasum -a 256 "$BIN/$b" | cut -c1-16))" || bad "$BIN/$b missing"
    done
    if [ -n "$BIN" ] && [ -x "$BIN/kaspad" ]; then
        H=$("$BIN/kaspad" --help 2>/dev/null || true)
        for f in --palw-drill-genesis-salt --palw-drill-write-keyring --palw-drill-fence-at --palw-drill-fence2-at --palw-drill-fence3-at --palw-drill-fence4-at; do
            grep -q -- "$f" <<< "$H" && ok "kaspad lists $f" || bad "kaspad lacks $f (not this branch's build)"
        done
    fi
    if [ -n "$OLD_BIN" ]; then
        [ -x "$OLD_BIN/kaspad" ] && ok "OLD_BIN $OLD_BIN/kaspad ($(shasum -a 256 "$OLD_BIN/kaspad" | cut -c1-16))" || bad "$OLD_BIN/kaspad missing"
        H=$("$OLD_BIN/kaspad" --help 2>/dev/null || true)
        grep -q -- --palw-drill-fence3-at <<< "$H" && ok "the old build moves the first three flag days" || bad "OLD_BIN lacks --palw-drill-fence3-at (older than int-6)"
        grep -q -- --palw-drill-fence4-at <<< "$H" && bad "OLD_BIN lists --palw-drill-fence4-at: it is not a pre-P0a build" || ok "the old build predates P0a"
    else
        ok "no OLD_BIN: V6 (the old build partitioned by the fork id) will be MANUAL"
    fi
    [ -e "$RUN/g0/app" ] && bad "$RUN/g0/app exists — refusing to reuse a drill's chain (a new RUN, or remove it)" || ok "no drill datadir at $RUN"
    for n in $(all_nodes); do k=$(kof "$n"); for base in $P2P_BASE $BORSH_BASE $JSON_BASE $EVM_BASE; do
        lsof -nP -iTCP:$((base + k)) -sTCP:LISTEN > /dev/null 2>&1 && busy="$busy $((base + k))"; done; done
    [ -z "$busy" ] && ok "ports free (${P2P_BASE}+ … ${EVM_BASE}+)" || bad "ports in use:$busy"
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

targets() { echo "$((FENCE_AT + 1)),$((FENCE2_AT + 1)),$((FENCE3_AT + 1)),$((FENCE4_AT - 5)),$((FENCE4_AT - 1)),$FENCE4_AT,$((FENCE4_AT + 1)),$((FENCE4_AT + 5)),$((FENCE4_AT + 20)),$((FENCE4_AT + 40))"; }

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
    manifest "'genesis', m['genesis_hash'][:16], 'params', m['consensus_params_id'][:16], 'salt_id', m['salt_id'], 'seats', len(m['seats']), 'fences', m['fence_at'], m['fence2_at'], m['fence3_at'], m['fence4_at']"
    echo "== nodes"
    local n
    for n in $(genesis_seats); do start_node "$n"; sleep 3; done
    [ -n "$OLD_BIN" ] && { start_node o8; sleep 3; }
    echo "== sampler and scheduled actions (background)"
    write_sampler
    local nodes_json bonds_json
    nodes_json=$(python3 -c "import json,sys; print(json.dumps({l.split()[0]: int(l.split()[1]) for l in sys.argv[1].splitlines()}))" "$NODES")
    bonds_json=$(python3 -c "import json; m=json.load(open('$KR/manifest.json')); print(json.dumps({'g2': m['seats'][2]['bond_outpoint'], 'g5': m['seats'][5]['bond_outpoint'], 'g3': m['seats'][3]['bond_outpoint']}))")
    ( P0A_SCRIPTS="$REPO/scripts" P0A_RUN="$RUN" P0A_JSON_BASE="$JSON_BASE" P0A_NODES="$nodes_json" P0A_TARGETS="$(targets)" P0A_BONDS="$bonds_json" \
        P0A_PARTITIONED="{\"o8\": $FENCE4_AT}" nohup python3 "$RUN/sampler.py" >> "$RUN/sampler.out" 2>&1 & echo $! > "$RUN/sampler.pid" )
    ( at() { until [ "$(tip g3)" -ge "$1" ] 2>/dev/null; do sleep 10; done; }
      act() { echo "$(date '+%F %T') $* (tip $(tip g3))" >> "$RUN/actions.log"; }
      at $((FENCE4_AT - 4)); act "stop g6 (below the P0a fence)"; stop_node g6 >> "$RUN/actions.log" 2>&1 || true
      at $((FENCE4_AT + 4)); act "start g6 (past the P0a fence)"; start_node g6 >> "$RUN/actions.log" 2>&1 || true
      at $((FENCE4_AT + 6)); act "start x9 (fresh key-less joiner, IBD across all four flag days)"; start_node x9 >> "$RUN/actions.log" 2>&1 || true
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
        [ "$(field "$n" 6)" = new ] && [ -f "$RUN/$n/kaspad.out" ] || continue
        got=$(grep -m1 -o -E 'Consensus params fingerprint: [0-9a-f]+' "$RUN/$n/kaspad.out" | awk '{print $4}')
        [ "$got" = "$want" ] && pass "$n prints the manifest's fingerprint ${want:0:16}" || fail "$n prints '${got:0:16}', the manifest ${want:0:16}"
        grep -q -E "PALW DRILL FLAG DAY: .*palw_gdn_key_heads.*$FENCE4_AT" "$RUN/$n/kaspad.out" && pass "$n names palw_gdn_key_heads at $FENCE4_AT" \
            || fail "$n has no PALW DRILL FLAG DAY line naming palw_gdn_key_heads at $FENCE4_AT"
        grep -q -E "Consensus fence schedule: (.*, )?$FENCE4_AT(,| )" "$RUN/$n/kaspad.out" && pass "$n schedules a fence at $FENCE4_AT" \
            || fail "$n's fence schedule has no fence at $FENCE4_AT"
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
    # The highest DAA the sampler saw on any node (the drill may already be stopped), else g3's live tip.
    t=$(awk -F'\t' '{n = split($2, a, " "); for (i = 1; i <= n; i++) { split(a[i], b, "="); split(b[2], c, ":"); if (c[1] + 0 > m) m = c[1] + 0 } } END {print m + 0}' "$RUN/sinks.tsv" 2>/dev/null || echo 0)
    [ "${t:-0}" -gt 0 ] || t=$(tip g3)
    [ "${t:-0}" -ge $((FENCE4_AT + 40)) ] && pass "the chain reached DAA $t, past $((FENCE4_AT + 40))" || fail "the chain reached DAA ${t:-?}, not past $((FENCE4_AT + 40))"
    echo "== V4 down across the fence / V5 fresh joiner"
    for n in g6 x9; do
        for t in $((FENCE4_AT + 20)) $((FENCE4_AT + 40)); do
            line=$(grep -E "^CP	$t	" "$RUN/checkpoints.tsv" 2>/dev/null | tail -1 || true)
            if echo "$line" | grep -q ALL-EQUAL && echo "$line" | cut -f6 | tr ' ' '\n' | grep -qx "$n"; then pass "$n in the ALL-EQUAL row at $t"
            else fail "$n not in an ALL-EQUAL row at $t"; fi
        done
    done
    echo "== V6 old build"
    if [ -z "$OLD_BIN" ]; then
        man "run with OLD_BIN=<a pre-P0a kaspad dir> to show the rollout: o8 peers below the fence, and past it a new node refuses it by the fork id"
    else
        # A checkpoint is taken at the first sample with every node synced at or past its target, so the one for
        # FENCE4_AT-1 may land past the fence (o8 already left out): either pre-fence row shows the agreement.
        got=""
        for t in $((FENCE4_AT - 5)) $((FENCE4_AT - 1)); do
            line=$(grep -E "^CP	$t	" "$RUN/checkpoints.tsv" 2>/dev/null | tail -1 || true)
            if echo "$line" | grep -q ALL-EQUAL && echo "$line" | cut -f6 | tr ' ' '\n' | grep -qx o8; then got=$t; fi
        done
        [ -n "$got" ] && pass "o8 (pre-P0a) agreed below the fence (ALL-EQUAL row at $got): a scheduled fence does not partition" \
            || fail "o8 is in no ALL-EQUAL row below the fence ($((FENCE4_AT - 5)), $((FENCE4_AT - 1)))"
        got=$(cat "$RUN"/g*/kaspad.out "$RUN"/x9/kaspad.out 2>/dev/null | grep -m1 -E "Fork-id mismatch .*crossed fence $FENCE4_AT;" || true)
        [ -n "$got" ] && pass "refused past the fence: $(echo "$got" | cut -c1-200)" || fail "no new node refused o8 by the fork id at fence $FENCE4_AT"
    fi
    echo "== V7 disagreement"
    [ ! -s "$RUN/disagree.tsv" ] && pass "no disagreement streak" || fail "$(wc -l < "$RUN/disagree.tsv" | tr -d ' ') disagreement row(s) in disagree.tsv"
    echo "== not checked here (see the header): a graph-v8 registration — refused on testnet-12 on both sides of the fence (C5); the processor test (e) holds the admission half"
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
