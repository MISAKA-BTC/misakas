#!/usr/bin/env bash
# misaka-palw-t12-rcore-drill.sh — ADR-0152 §8.3 item 3's short drill (D-1 … D-10) and the T10 setup of
# docs/handoff/t12-rcore-20260924/phase2-plan.md §4, on the SHIPPING kaspad with --palw-drill-genesis-salt
# (P2-12).
#
# BUILT BEFORE THE LAUNCH, RUN AFTER IT. The operator moved every drill — this one included — to after the
# testnet-12 launch (2026-09-24; ADR-0152 §8.3 item 3 "not a pre-launch gate", §8.4). Nothing in this file
# has been run against any host; `bash -n` is the only thing that has read it.
#
# WHERE. At least four fleet hosts, one drill node per host. NEVER on the operator's Mac and NEVER on a host
# that runs a public testnet-12 node (the 2026-09-23 crash-loop lesson: a script staged beside a public unit
# is a crash loop at its next restart). `check-host` refuses both, and every subcommand that starts a process
# runs it first.
#
# WHAT MAKES IT A DRILL AND NOT testnet-12 (ADR-0152 §8.2; T53 tests each):
#   * SALT (64 hex; `new-salt` prints one) moves the genesis — every genesis outpoint, the network domain and
#     the handshake identity are the drill's own, so public testnet-12 and the drill refuse each other;
#   * every key is drill-only: the keyring kaspad itself writes (`keyring`), never a card key. kaspad refuses
#     a salted node configured with any other producer key, pay address or heartbeat address;
#   * no discovery: --nodnsseed and an explicit PEERS list, and each node in its own app dir under WORK_DIR,
#     which kaspad marks and refuses to share with a public node.
#
# SUBCOMMANDS, in the order a drill uses them:
#   new-salt              print a fresh salt — hand the SAME salt to every drill host, out of band
#   check-host            the host guard alone
#   keyring               kaspad --palw-drill-write-keyring → $WORK_DIR/keyring/manifest.json (+ seed files)
#   node <seat>           start this host's drill node as genesis seat <seat> (0..7 in the manifest)
#   steps                 list the steps, numbered by POSITION in STEPS (never by hope)
#   step <k>              run step k: its action, then its evidence — read only from log bytes written after
#                         the action (a cursor is taken before it) and from the node's RPC
#   snapshot              append one analyzer snapshot (wall clock, DAG info, getPalwVesting) to
#                         $WORK_DIR/snapshots.jsonl; run it from cron every few minutes for the whole drill
#   analyze               scripts/misaka-palw-t12-rcore-analyze.py over $WORK_DIR (the three T10 reports)
#
# ENV: SALT (required; never defaulted), KASPAD_BIN / CLI_BIN (default target/release/{kaspad,misaka}),
#   OLD_KASPAD_BIN (a pre-R-core+ testnet-12 build, for D-1's second half), WORK_DIR (default
#   $HOME/.misaka-palw-t12-rcore-drill), PEERS (space-separated ip:port of the OTHER drill hosts), P2P_PORT
#   (26711), RPC_PORT (27711; the node's wRPC borsh listen, loopback), PUBLIC_PEER (a public testnet-12 node's
#   ip:port — D-1's refusal target; the drill only handshakes with it), PRODUCER_CLASS (the class a seat
#   mines; default the floor), STALL_WAIT (s, 3600: the chain's DAA did not move), STEP_WAIT (s, 172800).
#
# The short drill needs about a day at 200 s/DAA (ADR-0152 §8.3, an estimate): the first Final comes some
# 147 DAA after acceptance. A DA default (1,200 DAA ≈ 2.8 d) and a row's maturity (≈ 7.3 d) are O-7 and O-5 of
# the post-launch observation program (§8.4); steps 11 and 12 below run them when the drill is left that long.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
OLD_KASPAD_BIN="${OLD_KASPAD_BIN:-}"
WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-t12-rcore-drill}"
PEERS="${PEERS:-}"
P2P_PORT="${P2P_PORT:-26711}"
RPC_PORT="${RPC_PORT:-27711}"
PUBLIC_PEER="${PUBLIC_PEER:-}"
PRODUCER_CLASS="${PRODUCER_CLASS:-}"
STALL_WAIT="${STALL_WAIT:-3600}"
STEP_WAIT="${STEP_WAIT:-172800}"
KEYRING="$WORK_DIR/keyring"
MANIFEST="$KEYRING/manifest.json"
NODE_DIR="$WORK_DIR/node"
NODE_LOG="$NODE_DIR/kaspad.out"
ANALYZER="$REPO_ROOT/scripts/misaka-palw-t12-rcore-analyze.py"

log() { printf '[t12-rcore-drill] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

# --------------------------------------------------------------------------------------------------------
# Guards
# --------------------------------------------------------------------------------------------------------

need_salt() {
  [ -n "${SALT:-}" ] || die "SALT is required (64 hex; \`$0 new-salt\` prints one — the same salt on every drill host)"
  [[ "$SALT" =~ ^[0-9a-fA-F]{64}$ ]] || die "SALT must be exactly 64 hex characters"
  [[ "$SALT" =~ ^0+$ ]] && die "SALT is all zero — a salt everyone would type is a chain everyone shares"
  return 0
}

need_bins() {
  for b in "$KASPAD_BIN" "$CLI_BIN"; do [ -x "$b" ] || die "missing binary $b (cargo build --release -p kaspad -p misaka-cli)"; done
  "$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-drill-genesis-salt" \
    || die "$KASPAD_BIN has no --palw-drill-genesis-salt: drill the SHIPPING binary of the R-core+ line (P2-12)"
}

# The host guard. Refuses this Mac (any macOS host), and any host running a public testnet-12 node: a unit
# named misaka-t12-node*, or a kaspad on --netsuffix=12 without a drill salt whose app dir is not ours.
check_host() {
  [ "$(uname -s)" != "Darwin" ] || die "never on the operator's Mac (or any macOS host): the drill runs on fleet hosts only"
  if command -v systemctl >/dev/null 2>&1; then
    local unit
    for unit in $(systemctl list-units --type=service --state=active --no-legend 2>/dev/null | awk '{print $1}' | grep -E '^misaka-t12-node' || true); do
      die "this host runs a public testnet-12 node ($unit) — never drill beside one"
    done
  fi
  local line
  while IFS= read -r line; do
    [ -z "$line" ] && continue
    case "$line" in
      *--palw-drill-genesis-salt*) ;;                      # another drill node — allowed
      *"$WORK_DIR"*) ;;                                     # this drill's own throwaway probes
      *) die "this host runs a public testnet-12 kaspad: $line" ;;
    esac
  done < <(pgrep -af kaspad 2>/dev/null | grep -E -- '--netsuffix(=| )12' || true)
  log "host guard: not macOS, no public testnet-12 node on this host"
}

manifest() {
  # `manifest <python expression over m>` — prints the expression's value from the keyring manifest.
  [ -f "$MANIFEST" ] || die "no keyring at $MANIFEST — run \`$0 keyring\` first"
  python3 -c "import json,sys; m=json.load(open(sys.argv[1])); print($1)" "$MANIFEST"
}

cli() { "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$RPC_PORT" "$@"; }

# The drill's own genesis, read back from the node — never assumed. A node that answers with public
# testnet-12's genesis is not this drill's node, and every step refuses to read evidence from it.
check_node_is_the_drill() {
  local want
  want="$(manifest "m['genesis_hash']")"
  grep -q -E -- "PALW DRILL CHAIN .*genesis $want" "$NODE_LOG" 2>/dev/null \
    || die "the node log at $NODE_LOG does not announce drill genesis $want — is this node running with this drill's salt?"
}

# --------------------------------------------------------------------------------------------------------
# Evidence: a byte cursor before the action, then only what the log gained after it.
# --------------------------------------------------------------------------------------------------------

cursor() { { wc -c < "${1:-$NODE_LOG}"; } 2>/dev/null | tr -d ' ' || echo 0; }

virtual_daa() {
  cli node dag-info --output json 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["virtual_daa"])' 2>/dev/null || true
}

# `wait_after <cursor> <regex> <what> [log]` — the first line matching regex written after cursor, polled
# on the chain's clock: gives up when the virtual DAA stops moving for STALL_WAIT seconds or after STEP_WAIT.
wait_after() {
  local from="$1" pattern="$2" what="$3" file="${4:-$NODE_LOG}" began=$SECONDS last_move=$SECONDS last_daa="" daa line
  while :; do
    line="$(tail -c +"$((from + 1))" "$file" 2>/dev/null | grep -m1 -E -- "$pattern" || true)"
    if [ -n "$line" ]; then echo "$line"; return 0; fi
    daa="$(virtual_daa)"
    if [ -n "$daa" ] && [ "$daa" != "$last_daa" ]; then last_daa="$daa"; last_move=$SECONDS; fi
    [ $((SECONDS - began)) -lt "$STEP_WAIT" ] || die "gave up waiting for: $what ($STEP_WAIT s into the step, DAA ${last_daa:-?})"
    [ $((SECONDS - last_move)) -lt "$STALL_WAIT" ] || die "gave up waiting for: $what (DAA ${last_daa:-?} unmoved for $STALL_WAIT s)"
    sleep 10
  done
}

# `vesting_json` — getPalwVesting through the CLI (P2-10/P2-11), or empty when this build has no op 199.
vesting_json() { cli palw vesting --json 2>/dev/null || true; }

# `wait_vesting <python expression over v> <what>` — polls getPalwVesting until the expression is True.
wait_vesting() {
  local expr="$1" what="$2" began=$SECONDS doc
  [ -n "$(vesting_json)" ] || { log "SKIP evidence \"$what\": this build answers no getPalwVesting (P2-10/P2-11 not in it)"; return 0; }
  while :; do
    doc="$(vesting_json)"
    if [ "$(printf '%s' "$doc" | python3 -c "import json,sys; v=json.load(sys.stdin); print(bool($expr))" 2>/dev/null)" = "True" ]; then
      return 0
    fi
    [ $((SECONDS - began)) -lt "$STEP_WAIT" ] || die "gave up waiting for: $what"
    sleep 30
  done
}

manual() { log "MANUAL: $*"; }

# --------------------------------------------------------------------------------------------------------
# Subcommands
# --------------------------------------------------------------------------------------------------------

cmd_new_salt() {
  command -v openssl >/dev/null 2>&1 || die "openssl is needed to draw a salt"
  openssl rand -hex 32
}

cmd_keyring() {
  need_salt; need_bins
  mkdir -p "$WORK_DIR"
  "$KASPAD_BIN" --testnet --netsuffix=12 --palw-drill-genesis-salt="$SALT" --palw-drill-write-keyring="$KEYRING"
  [ "$(manifest "m['format']")" = "misaka-palw-drill-keyring/v1" ] || die "unexpected keyring format in $MANIFEST"
  log "drill $(manifest "m['salt_id']"): genesis $(manifest "m['genesis_hash'][:16]")… (public testnet-12: $(manifest "m['public_genesis_hash'][:16]")…)"
}

cmd_node() {
  local seat="${1:-}"
  [[ "$seat" =~ ^[0-9]+$ ]] || die "usage: $0 node <seat 0..7>"
  need_salt; need_bins; check_host
  [ -n "$PEERS" ] || die "PEERS is required: the other drill hosts' ip:port (a drill has no discovery)"
  [ -f "$MANIFEST" ] || cmd_keyring
  local n_seats
  n_seats="$(manifest "len(m['seats'])")"
  [ "$seat" -lt "$n_seats" ] || die "seat $seat: the drill registry has $n_seats seats"
  if lsof -nP -iTCP:"$P2P_PORT" -sTCP:LISTEN >/dev/null 2>&1 || lsof -nP -iTCP:"$RPC_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
    die "port $P2P_PORT or $RPC_PORT is in use — set P2P_PORT / RPC_PORT"
  fi
  mkdir -p "$NODE_DIR"
  # The log is appended across restarts: evidence of THIS start is what the log gains from here.
  local from; from="$(cursor)"
  local args=(--testnet --netsuffix=12 --palw-drill-genesis-salt="$SALT" --appdir="$NODE_DIR/app"
              --nodnsseed --disable-upnp --listen="0.0.0.0:$P2P_PORT" --rpclisten-borsh="127.0.0.1:$RPC_PORT"
              --utxoindex --enable-unsynced-mining --yes
              --palw-produce --palw-panel
              --palw-producer-key="$KEYRING/$(manifest "m['seats'][$seat]['seed_file']")"
              --palw-producer-bond="$(manifest "m['seats'][$seat]['bond_outpoint']")"
              --palw-fee-outpoint="$(manifest "m['seats'][$seat]['fee_float_outpoint']")"
              --palw-heartbeat-miner-address="$(manifest "m['heartbeat'][$seat]['address']")")
  [ -n "$PRODUCER_CLASS" ] && args+=(--palw-producer-class="$PRODUCER_CLASS")
  local peer
  for peer in $PEERS; do args+=(--addpeer="$peer"); done
  nohup "$KASPAD_BIN" "${args[@]}" >>"$NODE_LOG" 2>&1 &
  echo $! > "$NODE_DIR/kaspad.pid"
  log "drill node seat $seat started, pid $(cat "$NODE_DIR/kaspad.pid"), log $NODE_LOG"
  log "    $(wait_after "$from" "PALW DRILL CHAIN .*genesis $(manifest "m['genesis_hash']")" "the node to announce this drill's genesis")"
}

# `probe <name> <kaspad args…>` — a throwaway process under WORK_DIR (its own app dir, loopback-only unless
# told otherwise), killed on return. Used by D-1 only.
probe_pid=""
probe_stop() { [ -n "$probe_pid" ] && kill "$probe_pid" 2>/dev/null || true; probe_pid=""; }
trap probe_stop EXIT

# --------------------------------------------------------------------------------------------------------
# The steps. Numbered by POSITION in this array: a step inserted later renumbers everything after it, and
# the printed number is always the one this file runs (memory: "a drill step numbered by hope").
# --------------------------------------------------------------------------------------------------------
STEPS=(
  "d1|D-1 the handshake refuses public testnet-12 (genesis) and a pre-R-core+ binary (params id / fork id)"
  "d2|D-2 floor claims: acceptance → full-service licence (committed drops by E) → Final (a row, no payout)"
  "d3|D-3 Withheld-seen, Sampled and S2 claims hold w+E; the S2 claim upgrades through the V3 door; an 8k licence frees its replay"
  "d4|D-4 staged faults: capture-arm ExecutorRefuted before Final; PanelFalseValidV2 after Final burns a row; commit–reveal pays; 8k relabel (F1-M)"
  "d5|D-5 two accusers on one claim; the drawn units answered by MaterialDisclosedV2 from a Valid signer"
  "d6|D-6 a heartbeat-only stretch in which a conviction and a reveal are carried and folded"
  "d7|D-7 a node restarted mid-session; a reorg across a Final; IBD root agreement"
  "d8|D-8 a staged silent panel sinks both panels and forfeits at RT#2 (S0'); SR-9 fires on 3 Unavailable"
  "d9|D-9 a 130,000 MSK drill-only seat registered before the first anchor; every PanelBound recomputed under the stake draw"
  "d10|D-10 a 13,000 MSK drill producer takes an S0', holds ('holding: top up'), re-registers and resumes"
  "market|(T10 step 9, O-9) market load while rows mature: queue ≤ 1,032, ≥ 1 row moved per block while latched"
  "maturity|(T10 step 12, O-5) the first natural maturity: latch → move → mint; mempool refuses at mint+599, spends at mint+600"
  "report|(T10 step 11) the analyzer's three reports: door/withheld histogram, seconds per DAA, licence cadence"
)

cmd_steps() {
  local i=1 s
  for s in "${STEPS[@]}"; do printf '%2d  %s\n' "$i" "${s#*|}"; i=$((i + 1)); done
}

step_d1() {
  # (a) public testnet-12 refuses the drill on the genesis: a throwaway drill probe dials PUBLIC_PEER only.
  if [ -n "$PUBLIC_PEER" ]; then
    local dir="$WORK_DIR/probe-d1a"; rm -rf "$dir"; mkdir -p "$dir"
    "$KASPAD_BIN" --testnet --netsuffix=12 --palw-drill-genesis-salt="$SALT" --appdir="$dir/app" --nodnsseed --disable-upnp \
      --listen="127.0.0.1:$((P2P_PORT + 50))" --rpclisten-borsh="127.0.0.1:$((RPC_PORT + 50))" --connect="$PUBLIC_PEER" \
      >"$dir/kaspad.out" 2>&1 &
    probe_pid=$!
    log "    $(wait_after 0 "Genesis mismatch on network" "the public peer to be refused on the genesis" "$dir/kaspad.out")"
    probe_stop
  else
    manual "set PUBLIC_PEER=<a public testnet-12 node ip:port> to show the genesis refusal against the live network"
  fi
  # (b) a pre-R-core+ testnet-12 binary is refused: two loopback-only probes on the public genesis, no DNS,
  # connected to each other and to nothing else.
  if [ -n "$OLD_KASPAD_BIN" ]; then
    local dir="$WORK_DIR/probe-d1b"; rm -rf "$dir"; mkdir -p "$dir"
    "$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$dir/new" --nodnsseed --disable-upnp \
      --listen="127.0.0.1:$((P2P_PORT + 60))" --rpclisten-borsh="127.0.0.1:$((RPC_PORT + 60))" >"$dir/new.out" 2>&1 &
    probe_pid=$!
    local old_pid
    "$OLD_KASPAD_BIN" --testnet --netsuffix=12 --appdir="$dir/old" --nodnsseed --disable-upnp \
      --listen="127.0.0.1:$((P2P_PORT + 61))" --rpclisten-borsh="127.0.0.1:$((RPC_PORT + 61))" \
      --connect="127.0.0.1:$((P2P_PORT + 60))" >"$dir/old.out" 2>&1 &
    old_pid=$!
    log "    $(wait_after 0 "Consensus params mismatch|Genesis mismatch|fork id" "the pre-R-core+ binary to be refused" "$dir/new.out")"
    kill "$old_pid" 2>/dev/null || true
    probe_stop
  else
    manual "set OLD_KASPAD_BIN=<a pre-R-core+ testnet-12 build> for D-1's second half"
  fi
}

step_d2() {
  local from; from="$(cursor)"
  log "    waiting for a seat on this host to license a floor claim (after the step began)"
  log "    $(wait_after "$from" "\\[palw-panel\\] claim [0-9a-f]+: licensed" "a licence")"
  log "    waiting for a vesting row (the first Final): getPalwVesting totals.vesting_created > 0"
  wait_vesting "(v.get('totals') or {}).get('vesting_created', 0) > 0" "a vesting row at a Final"
}

step_d3() {
  manual "stage a Withheld-seen claim (a seat with --palw-drill-refuse-leaf-evidence, allowed on a salted drill), a Sampled seat and an S2 licence;"
  manual "then read their rows: w+E held until the upgrade; the S2 claim's door becomes Coverage/Quorum through the V3 supplementary set."
  wait_vesting "len((v.get('licence_histogram') or {})) >= 2" "licences through at least two doors"
}

step_d4() {
  manual "restart one seat's producer with --palw-drill-tamper-leaf=<leaf> (the shard-court drill's tamper path) on this drill only;"
  manual "a P2-8 seat files ExecutorRefuted from its capture arm before Final (S2)."
  local from; from="$(cursor)"
  wait_vesting "(v.get('totals') or {}).get('vesting_burned', 0) > 0" "a row burned by a post-Final conviction (the Burned note)"
  log "    evidence after the action only: $(tail -c +"$((from + 1))" "$NODE_LOG" | grep -c -E 'ExecutorRefuted|PanelFalseValid' || true) conviction line(s) in this node's log"
}

step_d5() {
  manual "open two DefaultAccused on one claim from two seats; a Valid signer answers the drawn units with MaterialDisclosedV2 (P2-7)."
}

step_d6() {
  manual "stop every drill producer (--palw-produce off; heartbeats only), then file a conviction and a reveal from a seat."
  local from; from="$(cursor)"
  log "    $(wait_after "$from" "heartbeat #[0-9]+ [0-9a-f]+ carries [1-9][0-9]* lifecycle carrier" "a heartbeat carrying the conviction (H-1)")"
}

step_d7() {
  manual "restart one node mid-session (SIGINT, same app dir); partition two hosts to force a reorg across a Final; start a fresh node (no app dir) for IBD."
  manual "evidence: the restarted node's state root and the fresh node's equal an archival node's at the same sink (misaka node dag-info)."
}

step_d8() {
  manual "run one claim's panel with every seat silent (--palw-drill-answer-only / refuse, allowed on a salted drill): both panels sink, RT#2 forfeits (S0');"
  manual "three Unavailable receipts fire SR-9."
}

step_d9() {
  manual "before the first anchor: register a 130,000 MSK drill-only seat from the drill main wallet (manifest 'bonds'[0], --palw-register-bond);"
  manual "every node must recompute every PanelBound under the stake draw with no refusal; log that seat's panel count against its stake (O-2)."
}

step_d10() {
  manual "start a 13,000 MSK drill producer (manifest 'bonds'[1]); stage one S0'."
  local from; from="$(cursor)"
  log "    $(wait_after "$from" "holding: top up [0-9]+ sompi to reach the producer floor" "the producer to hold for a top-up")"
  manual "re-register that operator with a NEW drill-only bond key (manifest 'bonds'[2], ≥ 13,000 MSK): production resumes, no block refused."
}

step_market() {
  manual "run a carrier and EVM buy/sell generator at > 8 rows per block while rows mature."
  wait_vesting "(v.get('totals') or {}).get('backlog_legs', 0) <= 1032" "the queue at or under 1,032 legs"
}

step_maturity() {
  wait_vesting "(v.get('totals') or {}).get('vesting_moved', 0) > 0" "the first natural maturity (a row moved)"
  manual "find the minted output (misaka wallet utxo list); submit its spend at mint+599 (refused by the mempool) and at mint+600 (accepted), 0 DNS anchors."
}

step_report() {
  [ -f "$WORK_DIR/snapshots.jsonl" ] || die "no snapshots at $WORK_DIR/snapshots.jsonl — run \`$0 snapshot\` from cron during the drill"
  python3 "$ANALYZER" "$WORK_DIR"
}

cmd_step() {
  local k="${1:-}"
  [[ "$k" =~ ^[0-9]+$ ]] && [ "$k" -ge 1 ] && [ "$k" -le "${#STEPS[@]}" ] || die "usage: $0 step <1..${#STEPS[@]}> (see \`$0 steps\`)"
  need_salt; need_bins; check_host
  local entry="${STEPS[$((k - 1))]}"
  local id="${entry%%|*}"
  log "$k/${#STEPS[@]} ${entry#*|}"
  [ "$id" = "d1" ] || [ "$id" = "report" ] || check_node_is_the_drill
  "step_$id"
  log "$k/${#STEPS[@]} done"
}

cmd_snapshot() {
  need_salt
  local wall dag vesting
  wall="$(python3 -c 'import time; print(int(time.time() * 1000))')"
  dag="$(cli node dag-info --output json 2>/dev/null || echo null)"
  vesting="$(vesting_json)"; [ -n "$vesting" ] || vesting=null
  python3 - "$WORK_DIR/snapshots.jsonl" "$wall" "$(hostname)" "$dag" "$vesting" <<'PY'
import json, sys
path, wall, host, dag, vesting = sys.argv[1:6]
row = {"format": "misaka-palw-drill-snapshot/v1", "wall_ms": int(wall), "host": host,
       "dag": json.loads(dag), "vesting": json.loads(vesting)}
with open(path, "a") as f:
    f.write(json.dumps(row, sort_keys=True) + "\n")
PY
}

cmd_analyze() { python3 "$ANALYZER" "$WORK_DIR" "$@"; }

case "${1:-}" in
  new-salt) cmd_new_salt ;;
  check-host) check_host ;;
  keyring) cmd_keyring ;;
  node) shift; cmd_node "$@" ;;
  steps) cmd_steps ;;
  step) shift; cmd_step "$@" ;;
  snapshot) cmd_snapshot ;;
  analyze) shift; cmd_analyze "$@" ;;
  *) sed -n '2,40p' "$0"; exit 2 ;;
esac
