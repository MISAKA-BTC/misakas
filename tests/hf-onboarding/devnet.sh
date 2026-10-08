#!/usr/bin/env bash
# tests/hf-onboarding/devnet.sh — H1's private devnet (layout and roles: lib.sh).
#
#   devnet.sh dry                 binaries, flags, ports, memory, disk, the plan and every node's argv (salt redacted); starts nothing
#   devnet.sh up                  salt (0600, never printed), keyring (written by the kaspad under test), the steady nodes A B C D1..D6
#   devnet.sh schedule            the fence schedule the salted chain runs (kaspad's own MOVED/ARMED lines from A's log)
#   devnet.sh bond <n>            register post-genesis bond <n> (keyring bond n): fund its address from the main wallet, run the
#                                 registrar (`kaspad --palw-register-bond`) until it prints the bond, stop it; writes bonds/bond-<n>.json
#   devnet.sh verifier            bond V_BOND_N, then start V (a bonded verifier outside the genesis operators)
#   devnet.sh user                U's home and key (keyring bond U_BOND_N), its funds (one main-wallet send), its own bond (registrar run)
#   devnet.sh fresh               start Z, a clean node with an empty app dir, and wait until it reaches A's sink (IBD)
#   devnet.sh restart <node>      SIGINT, then start again over the same app dir
#   devnet.sh status              pid, RSS, DAA, sink, peers per node; free memory
#   devnet.sh down                SIGINT every node (never SIGKILL)
#   devnet.sh env                 the pinned environment (binary hashes, genesis, params id, schedule) as JSON
set -euo pipefail
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
h1_snapshot_exec devnet.sh "$@"
cmd=${1:-dry}; shift || true

preflight() {
    local fail=0 b
    for b in "$KASPAD_BIN" "$CLI_BIN" "$PALW_CLASS_BIN"; do
        [ -x "$b" ] && echo "  ok   $b ($(shasum -a 256 "$b" | cut -c1-16))" || { echo "  FAIL missing $b"; fail=1; }
    done
    local H; H=$("$KASPAD_BIN" --help 2>/dev/null || true)
    for f in --palw-drill-genesis-salt --palw-drill-write-keyring --palw-drill-fence-at --palw-drill-fence2-at --palw-drill-fence3-at \
             --palw-drill-tir-at --palw-drill-tir2-at --palw-drill-int11-at --palw-register-bond --palw-bond-collateral --ram-scale \
             --palw-host-memory-share --palw-heartbeat-miner-address; do
        grep -q -- "$f" <<<"$H" && echo "  ok   kaspad lists $f" || { echo "  FAIL kaspad lacks $f"; fail=1; }
    done
    local k p
    for n in $(all_nodes); do k=$(kof "$n"); for p in $((P2P_BASE + k)) $((BORSH_BASE + k)) $((JSON_BASE + k)) $((EVM_BASE + k)); do
        if lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1 && ! running "$n"; then echo "  FAIL port $p ($n) is in use"; fail=1; fi
    done; done
    local free; free=$(mem_free_pct); echo "  note memory free ${free}% (floor ${MEM_FLOOR_PCT}%)"
    local disk; disk=$(df -g / | awk 'NR==2 {print $4}'); echo "  note disk free ${disk} GiB"
    [ "${disk:-0}" -ge 15 ] || { echo "  FAIL disk free ${disk} GiB < 15"; fail=1; }
    return $fail
}

write_keyring() {
    [ -e "$KR/manifest.json" ] && return 0
    mkdir -p "$KR" "$WORK_DIR/keyring-app"; chmod 700 "$KR"
    "$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$WORK_DIR/keyring-app" --palw-drill-genesis-salt="$(salt)" \
        "--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT" \
        "--palw-drill-tir-at=$TIR_AT" "--palw-drill-tir2-at=$TIR2_AT" "--palw-drill-int11-at=$INT11_AT" \
        ${FENCE4_AT:+"--palw-drill-fence4-at=$FENCE4_AT"} --palw-drill-write-keyring="$KR" > "$WORK_DIR/keyring.out" 2>&1 || true
    sed -E "s/[0-9a-f]{64}/<64hex>/g" "$WORK_DIR/keyring.out" | tail -3 >&2
    [ -s "$KR/manifest.json" ] || die "the keyring was not written (see $WORK_DIR/keyring.out)"
    chmod 600 "$KR"/*
}

up() {
    preflight || die "preflight failed"
    mkdir -p "$WORK_DIR" "$OPHOME" "$WORK_DIR/bonds"; chmod 700 "$WORK_DIR"
    [ -s "$WORK_DIR/SALT" ] || ( umask 077; openssl rand -hex 32 > "$WORK_DIR/SALT" )
    write_keyring
    say "genesis $(manifest "m['genesis_hash'][:16]") params $(manifest "m['consensus_params_id'][:16]") salt_id $(manifest "m['salt_id']")"
    local n; for n in A B C D1 D2 D3 D4 D5 D6; do start_node "$n"; sleep 2; done
    say "up: tip $(tip C)"
}

wait_daa() { # wait_daa <daa> <max seconds>
    local want=$1 max=${2:-3600} t0=$SECONDS d
    while :; do d=$(tip C); [[ "$d" =~ ^[0-9]+$ ]] && [ "$d" -ge "$want" ] && return 0
        [ $((SECONDS - t0)) -ge "$max" ] && { say "DAA $d did not reach $want in ${max}s"; return 1; }; sleep 15; done
}

utxos_of() { # utxos_of <address> -> "outpoint amount" lines
    rpc C getUtxosByAddresses "{\"addresses\":[\"$1\"]}" | python3 -c '
import json,sys
d=json.load(sys.stdin)
for e in d.get("entries",[]):
    o=e.get("outpoint",{}); u=e.get("utxoEntry",{})
    print("%s:%s %s %s" % (o.get("transactionId"), o.get("index"), u.get("amount"), u.get("isCoinbase")))'
}

bond() { # bond <n> [collateral sompi] — without a collateral, kaspad's own default (`--palw-register-bond`'s sizing)
    local n=$1 coll=${2:-}
    local out="$WORK_DIR/bonds/bond-$n.json"; [ -s "$out" ] && { say "bond $n: already registered ($(python3 -c "import json;print(json.load(open('$out'))['bond_outpoint'][:24])")…)"; return 0; }
    local addr; addr=$(manifest "m['bonds'][$n - $GENESIS_SEATS]['address']")
    local fund=$(( ${FUND_BILI:-2000000} * 100000000 ))
    [ -z "$coll" ] || [ "$coll" -lt "$fund" ] || fund=$((coll + 1000 * 100000000))
    local have; have=$(utxos_of "$addr" | awk '{s+=$2} END {print s+0}')
    if [ "$have" -lt "$fund" ]; then
        local amount; amount=$(python3 -c "print('%.8f' % ($fund / 1e8))")
        say "bond $n: funding $amount BILI to its address from the main wallet (the bootstrap's one send)"
        ocli wallet send --to "$addr" --amount "$amount" --key-file "$KR/main-0.seed" --yes >> "$WORK_DIR/bonds/fund-$n.log" 2>&1 || die "bond $n: wallet send failed (see $WORK_DIR/bonds/fund-$n.log)"
        local t0=$SECONDS; while [ "$(utxos_of "$addr" | awk '{s+=$2} END {print s+0}')" -lt "$fund" ]; do
            [ $((SECONDS - t0)) -lt 1800 ] || die "bond $n: the funding did not confirm in 30 min"; sleep 15; done
    fi
    local d=$WORK_DIR/reg; stop_node reg || true
    [ -d "$d/app" ] && mv "$d" "$d.done-$(date +%s)"
    mkdir -p "$d"; printf '%s\n' --palw-register-bond "--palw-producer-key=$KR/bond-$n.seed" "--palw-producer-pay-address=$addr" \
        ${coll:+"--palw-bond-collateral=$coll"} > "$d/extra-args"
    start_node reg || die "bond $n: the registrar did not start"
    local c; c=$(cat "$d/start.cursor"); local t0=$SECONDS line=""
    while [ -z "$line" ]; do
        line=$(tail -c +$((c + 1)) "$d/kaspad.out" | grep -oE "registered bond [0-9a-f]+:[0-9]+ with [0-9]+ sompi of collateral, in tx [0-9a-f]+|this key already holds bond [0-9a-f]{128}:[0-9]+ on this chain" | head -1 || true)
        [ $((SECONDS - t0)) -lt 3600 ] || { stop_node reg; die "bond $n: the registrar printed no bond in 60 min (see $d/kaspad.out)"; }
        [ -n "$line" ] || sleep 15
    done
    stop_node reg
    local op; op=$(grep -oE "[0-9a-f]{64,}:[0-9]+" <<<"$line" | head -1)
    [ -n "$coll" ] || coll=$(grep -oE "with [0-9]+ sompi" <<<"$line" | grep -oE "[0-9]+" || echo 0)
    # The fee float: the largest spendable output at the address that is not the bond's collateral.
    local t1=$SECONDS fee=""
    while [ -z "$fee" ]; do
        fee=$(utxos_of "$addr" | awk -v b="$op" '$1!=b && $2>=100000000 {print $2, $1}' | sort -n | tail -1 | awk '{print $2}')
        [ -n "$fee" ] || { [ $((SECONDS - t1)) -lt 600 ] || die "bond $n: no fee float at $addr"; sleep 15; }
    done
    python3 - "$out" "$n" "$op" "$fee" "$addr" "$coll" "$(tip C)" "$line" <<'PY'
import json, sys
o, n, op, fee, addr, coll, daa, line = sys.argv[1:]
json.dump({"n": int(n), "bond_outpoint": op, "fee_outpoint": fee, "address": addr, "collateral_sompi": int(coll),
           "registered_by_daa": int(daa) if daa.isdigit() else None, "registrar_line": line}, open(o, "w"), indent=1)
PY
    say "bond $n: $op (collateral $coll sompi), fee float $fee"
}

case $cmd in
    dry) preflight || true; echo "== plan: WORK_DIR=$WORK_DIR, fences $FENCE_AT/$FENCE2_AT/$FENCE3_AT, IR $TIR_AT, IR-2 $TIR2_AT, int-11 $INT11_AT${FENCE4_AT:+, fence4 $FENCE4_AT}"
         echo "$NODES" ;;
    up) up ;;
    schedule) grep -hE "MOVED from DAA|ARMED at DAA|the shipping release's own height" "$WORK_DIR/A/kaspad.out" "$WORK_DIR/keyring.out" 2>/dev/null | sed -E 's/^.*(palw_[a-z0-9_]+)/\1/' | sort -u ;;
    bond) bond "$@" ;;
    verifier) bond "$V_BOND_N" "${1:-}"; start_node V ;;
    user)
        mkdir -p "$UHOME/.misaka"; chmod 700 "$UHOME"
        install -m 600 "$KR/bond-$U_BOND_N.seed" "$UHOME/.misaka/u.seed"
        bond "$U_BOND_N" "${1:-}"
        cp "$WORK_DIR/bonds/bond-$U_BOND_N.json" "$UHOME/.misaka/u-bond.json" ;;
    fresh) start_node Z; t0=$SECONDS
        while :; do a=$(rpc A getBlockDagInfo '{}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["sink"])'); z=$(rpc Z getBlockDagInfo '{}' 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["sink"])' 2>/dev/null || true)
            [ -n "$z" ] && [ "$a" = "$z" ] && { say "Z reached A's sink $a after $((SECONDS - t0)) s"; break; }
            [ $((SECONDS - t0)) -lt 3600 ] || die "Z did not reach A's sink in 60 min"; sleep 10; done ;;
    restart) stop_node "$1"; start_node "$1" ;;
    start) for n in "$@"; do start_node "$n"; done ;;
    stop) for n in "$@"; do stop_node "$n"; done ;;
    status)
        for n in $(all_nodes); do
            if running "$n"; then pid=$(cat "$WORK_DIR/$n/kaspad.pid"); rss=$(ps -o rss= -p "$pid" | awk '{printf "%.0f", $1/1024}')
                st=$(rpc "$n" getBlockDagInfo '{}' 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("virtualDaaScore","?"), str(d.get("sink",""))[:12])' 2>/dev/null || echo "? ?")
                pe=$(rpc "$n" getConnectedPeerInfo '{}' 2>/dev/null | python3 -c 'import json,sys; print(len(json.load(sys.stdin).get("peerInfo",[])))' 2>/dev/null || echo "?")
                printf '%-4s pid %-6s rss %5s MiB  daa/sink %s  peers %s\n' "$n" "$pid" "$rss" "$st" "$pe"
            else printf '%-4s down\n' "$n"; fi
        done
        echo "mac free $(mem_free_pct)%" ;;
    down) for n in $(all_nodes); do stop_node "$n" & done; wait ;;
    env)
        python3 - "$KASPAD_BIN" "$CLI_BIN" "$PALW_CLASS_BIN" "$KR/manifest.json" "$WT" <<'PY'
import hashlib, json, os, platform, subprocess, sys
k, c, p, man, wt = sys.argv[1:]
sha = lambda f: hashlib.sha256(open(f, "rb").read()).hexdigest()
m = json.load(open(man)) if os.path.exists(man) else {}
git = lambda *a: subprocess.run(["git", "-C", wt] + list(a), capture_output=True, text=True).stdout.strip()
mem = int(subprocess.run(["sysctl", "-n", "hw.memsize"], capture_output=True, text=True).stdout.strip() or 0)
print(json.dumps({
  "integration_sha": git("rev-parse", "HEAD"), "branch": git("rev-parse", "--abbrev-ref", "HEAD"), "dirty": bool(git("status", "--porcelain", "--untracked-files=no")),
  "binaries": {os.path.basename(f): {"path": f, "sha256": sha(f)} for f in (k, c, p) if os.path.exists(f)},
  "network": "testnet-12 salted drill (private, loopback)", "genesis_hash": m.get("genesis_hash"), "consensus_params_id": m.get("consensus_params_id"),
  "salt_id": m.get("salt_id"),
  "hardware": {"machine": platform.machine(), "os": platform.platform(), "ram_bytes": mem},
}, indent=1))
PY
        ;;
    *) sed -n '2,16p' "$0"; exit 2 ;;
esac
