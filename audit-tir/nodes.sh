#!/usr/bin/env bash
# audit-tir/nodes.sh start|stop|kill|status|args <node…> — the D-F drill's nodes (lib-df.sh NODES), after
# drill D's audit-lifecycle/nodes.sh.
#   start   nohup nice kaspad (KASPAD_BIN; the `old` relay OLD_KASPAD_BIN); waits for the salted genesis line
#           in log bytes written after the start
#   stop    SIGINT, wait <= 90 s (then report, never SIGKILL on its own)
#   kill    SIGKILL (the "process killed" fault)
#   status  pid, RSS, DAA, peers per node + memory_pressure
#   args    the node's argv (salt redacted)
. "$(cd "$(dirname "$0")" && pwd)/lib-df.sh"
cmd=${1:-status}; shift || true
[ $# -gt 0 ] || set -- $(all_nodes)

bin_of() { if [ "$(field "$1" 4)" = old ]; then echo "$OLD_KASPAD_BIN"; else echo "$KASPAD_BIN"; fi; }

start1() {
    local n=$1 d=$WORK_DIR/$1; mkdir -p "$d/home"
    if running "$n"; then say "$n: already running (pid $(cat "$d/kaspad.pid"))"; return 0; fi
    local free; free=$(memory_pressure 2>/dev/null | awk -F': ' '/System-wide memory free percentage/ {gsub("%","",$2); print $2}')
    [ "${free:-0}" -ge 40 ] || die "memory free ${free}% < 40% — not starting $n"
    local A2=() x; while IFS= read -r x; do A2+=("$x"); done < <(node_args "$n")
    printf '%s\n' "${A2[@]}" | sed 's/salt=[0-9a-f]*/salt=<SALT>/' > "$d/args.redacted"
    local c; c=$(stat -f %z "$d/kaspad.out" 2>/dev/null || echo 0); echo "$c" > "$d/start.cursor"
    local b; b=$(bin_of "$n")
    ( cd "$d"; ulimit -n 10240; HOME="$d/home" nohup nice -n 10 "$b" "${A2[@]}" >> "$d/kaspad.out" 2>&1 & echo $! > "$d/kaspad.pid" )
    echo "$(date '+%F %T') START pid $(cat "$d/kaspad.pid") cursor $c bin $b" >> "$d/events.log"
    local g; g=$(manifest "m['genesis_hash']")
    for i in $(seq 1 60); do
        if tail -c +$((c + 1)) "$d/kaspad.out" 2>/dev/null | grep -E "PALW DRILL CHAIN .*genesis $g" >/dev/null; then   # no -q under pipefail
            say "$n: up (pid $(cat "$d/kaspad.pid"))"; return 0; fi
        ps -p "$(cat "$d/kaspad.pid")" >/dev/null 2>&1 || { say "$n: DIED — $(tail -c +$((c + 1)) "$d/kaspad.out" | tail -5)"; return 1; }
        sleep 2
    done
    say "$n: no genesis line in 120 s"; return 1
}

stop1() {
    local n=$1 d=$WORK_DIR/$1 sig=${2:-INT}
    running "$n" || { say "$n: not running"; return 0; }
    local pid; pid=$(cat "$d/kaspad.pid")
    kill -"$sig" "$pid"; echo "$(date '+%F %T') STOP($sig) pid $pid" >> "$d/events.log"
    [ "$sig" = KILL ] && { sleep 1; say "$n: SIGKILLed $pid"; return 0; }
    for i in $(seq 1 90); do ps -p "$pid" >/dev/null 2>&1 || { say "$n: stopped"; return 0; }; sleep 1; done
    say "$n: still alive after 90 s (pid $pid)"; return 1
}

case $cmd in
    start) for n in "$@"; do start1 "$n"; done ;;
    stop) for n in "$@"; do stop1 "$n" INT & done; wait ;;
    kill) for n in "$@"; do stop1 "$n" KILL; done ;;
    args) node_args "$1" | sed 's/salt=[0-9a-f]*/salt=<SALT>/' ;;
    status)
        for n in "$@"; do
            if running "$n"; then pid=$(cat "$WORK_DIR/$n/kaspad.pid"); rss=$(ps -o rss= -p "$pid" | awk '{printf "%.0f", $1/1024}')
                st=$(python3 "$A/rpc.py" call --port "$(jport "$n")" getBlockDagInfo '{}' 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("virtualDaaScore","?"), str(d.get("sink",""))[:12])' 2>/dev/null || echo "? ?")
                pe=$(python3 "$A/rpc.py" call --port "$(jport "$n")" getConnectedPeerInfo '{}' 2>/dev/null | python3 -c 'import json,sys; print(len(json.load(sys.stdin).get("peerInfo",[])))' 2>/dev/null || echo "?")
                printf '%-4s pid %-6s rss %5s MiB  daa/sink %s  peers %s\n' "$n" "$pid" "$rss" "$st" "$pe"
            else printf '%-4s down\n' "$n"; fi
        done
        memory_pressure | awk -F': ' '/free percentage/ {print "mac free", $2}' ;;
    *) sed -n '2,8p' "$0"; exit 2 ;;
esac
