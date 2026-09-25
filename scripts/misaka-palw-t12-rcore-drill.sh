#!/usr/bin/env bash
# misaka-palw-t12-rcore-drill.sh — ADR-0152 §8.3 item 3's short drill (D-1 … D-10) and the T10 setup of
# docs/handoff/t12-rcore-20260924/phase2-plan.md §4, on the SHIPPING kaspad with --palw-drill-genesis-salt
# (P2-12).
#
# BUILT BEFORE THE LAUNCH, RUN AFTER IT. The operator moved every drill — this one included — to after the
# testnet-12 launch (2026-09-24; ADR-0152 §8.3 item 3 "not a pre-launch gate", §8.4). Nothing in this file
# has been run against any host; `bash -n` is the only thing that has read it.
#
# WHERE. Fleet hosts only — NEVER the operator's Mac and NEVER a host that runs a public testnet-12 node (the
# 2026-09-23 crash-loop lesson: a script staged beside a public unit is a crash loop at its next restart).
# `check-host` refuses both, and every subcommand that starts a process runs it first. The 8k row needs 7
# ready seats of distinct operators (the regenesis doc), and there are 8 genesis seats: run several seats per
# host (`node <seat>` once per seat, each with its own app dir and ports — P2P_BASE+seat, RPC_BASE+seat,
# EVM_RPC_BASE+seat — and no gRPC listener at all), or one per host on at least 7 hosts.
#
# WHAT MAKES IT A DRILL AND NOT testnet-12 (ADR-0152 §8.2; T53 tests each):
#   * SALT (64 hex; `new-salt` prints one) moves the genesis — every genesis outpoint, the network domain and
#     the handshake identity are the drill's own, so public testnet-12 and the drill refuse each other;
#   * every key is drill-only: the keyring kaspad itself writes (`keyring`), never a card key. kaspad refuses
#     a salted node configured with any other producer key, validator key, pay or heartbeat address, or EVM
#     fee recipient; its getBlockTemplate pays only keyring addresses; its EVM ingress admits only keyring
#     EVM accounts;
#   * THE EVM LANE IS NOT SEPARATED BY THE SALT: an EVM transaction binds EVM_CHAIN_ID (one constant on every
#     network) and a nonce, never the genesis. Only the manifest's `evm` accounts may sign on a drill — an
#     account that holds value on public testnet-12 would sign transactions valid there;
#   * off-node signers (the misaka CLI, the gateway rail) take the salt too (`--palw-drill-genesis-salt`) and
#     refuse to sign for a node whose genesis is not the salt's — `cli` below always passes it;
#   * no discovery: --nodnsseed and an explicit PEERS list, and each node in its own app dir under WORK_DIR,
#     which kaspad marks and refuses to share with a public node.
#
# SUBCOMMANDS, in the order a drill uses them:
#   new-salt              print a fresh salt — hand the SAME salt to every drill host, out of band
#   check-host            the host guard alone
#   keyring               kaspad --palw-drill-write-keyring → $WORK_DIR/keyring/manifest.json (+ seed files)
#   node <seat>           start this host's drill node for genesis seat <seat> (0..7 in the manifest)
#   steps                 list the steps, numbered by POSITION in STEPS (never by hope)
#   step <k>              run step k against seat $SEAT's node: its action, then its evidence — read only from
#                         log bytes written after the action (a cursor is taken before it) and from the RPC.
#                         Exit 0 PASS, 1 FAIL, 3 INCOMPLETE (evidence this build cannot produce), 4 MANUAL
#                         (automated evidence passed; the printed manual part needs `attest`). Every result is
#                         recorded in $WORK_DIR/steps.status.
#   attest <k> <text>     record the operator's evidence for step k's manual part (only after a MANUAL result)
#   snapshot              append one analyzer snapshot (wall clock, DAG info, getPalwVesting) to
#                         $WORK_DIR/snapshots.jsonl; run it from cron every few minutes for the whole drill
#   analyze               scripts/misaka-palw-t12-rcore-analyze.py over $WORK_DIR (the three T10 reports)
#   self-test             the step machinery on a scratch log (no node, no host; safe anywhere)
#
# ENV: SALT (required; never defaulted), KASPAD_BIN / CLI_BIN (default target/release/{kaspad,misaka}),
#   OLD_KASPAD_BIN (a pre-R-core+ testnet-12 build, for D-1's second half), WORK_DIR (default
#   $HOME/.misaka-palw-t12-rcore-drill), SEAT (the local seat a step, snapshot or CLI call reads; default 0),
#   PEERS (space-separated ip:port of the OTHER drill nodes), P2P_BASE (26711), RPC_BASE (27711; wRPC borsh,
#   loopback), EVM_RPC_BASE (18711; EVM HTTP, loopback), PUBLIC_PEER (a public testnet-12 node's ip:port —
#   D-1's refusal target; the drill only handshakes with it), PRODUCER_CLASS (the class a seat mines; default
#   the floor), STALL_WAIT (s, 3600: the chain's DAA did not move), STEP_WAIT (s, 172800), PROBE_WAIT (s, 300:
#   a D-1 probe's wall-clock limit — a probe is not on the drill's chain, so its wait is not on its clock).
#
# The short drill needs about a day at 200 s/DAA (ADR-0152 §8.3, an estimate): the first Final comes some
# 147 DAA after acceptance. A DA default (1,200 DAA ≈ 2.8 d) and a row's maturity (≈ 7.3 d) are O-7 and O-5 of
# the post-launch observation program (§8.4); the `maturity` step runs them when the drill is left that long.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
OLD_KASPAD_BIN="${OLD_KASPAD_BIN:-}"
WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-t12-rcore-drill}"
SEAT="${SEAT:-0}"
PEERS="${PEERS:-}"
P2P_BASE="${P2P_BASE:-26711}"
RPC_BASE="${RPC_BASE:-27711}"
EVM_RPC_BASE="${EVM_RPC_BASE:-18711}"
PUBLIC_PEER="${PUBLIC_PEER:-}"
PRODUCER_CLASS="${PRODUCER_CLASS:-}"
STALL_WAIT="${STALL_WAIT:-3600}"
STEP_WAIT="${STEP_WAIT:-172800}"
PROBE_WAIT="${PROBE_WAIT:-300}"
KEYRING="$WORK_DIR/keyring"
MANIFEST="$KEYRING/manifest.json"
STATUS_FILE="$WORK_DIR/steps.status"
ANALYZER="$REPO_ROOT/scripts/misaka-palw-t12-rcore-analyze.py"

# Public testnet-12's own listeners (P2P shared with testnet-11 by the 2026-09-22 decision; testnet gRPC,
# wRPC borsh, wRPC JSON; the EVM HTTP default), and the ports contrib/t12-deploy-kit gives the fleet's
# public nodes (install-*.sh: gRPC 26312, wRPC 26313/26314 and the second..fifth node's 263x1/263x3/263x4).
# A drill never binds one, and a host where one listens runs a public node, however it was started.
PUBLIC_T12_PORTS="26311 26210 27210 28210 8545 26312 26313 26314 26321 26323 26324 26331 26333 26334 26341 26343 26344 26351 26353 26354"

# Exit codes of a step (see SUBCOMMANDS).
RC_FAIL=1
RC_INCOMPLETE=3
RC_MANUAL=4

log() { printf '[t12-rcore-drill] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit "$RC_FAIL"; }
incomplete() { log "INCOMPLETE: $*"; exit "$RC_INCOMPLETE"; }

# `evidence <command…>` — run a command that prints one evidence line and dies on failure, and log the
# line. The command runs in a command substitution, where `die`'s exit leaves only the subshell and `set -e`
# does not fire for an assignment's argument; the `|| exit` carries its status out, so a wait that gave up
# is the step's failure and never a line followed by "done" (P2-12 review finding 3).
evidence() {
  local line
  line="$("$@")" || exit $?
  log "    $line"
}

seat_ports() {
  # `seat_ports <seat>` sets P2P_PORT RPC_PORT EVM_RPC_PORT NODE_DIR NODE_LOG for that seat.
  local s="$1"
  [[ "$s" =~ ^[0-9]+$ ]] || die "seat must be a number, got '$s'"
  P2P_PORT=$((P2P_BASE + s)); RPC_PORT=$((RPC_BASE + s)); EVM_RPC_PORT=$((EVM_RPC_BASE + s))
  NODE_DIR="$WORK_DIR/seat-$s"; NODE_LOG="$NODE_DIR/kaspad.out"
}
seat_ports "$SEAT"

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
  "$CLI_BIN" --help 2>/dev/null | grep -q -- "--palw-drill-genesis-salt" \
    || die "$CLI_BIN has no --palw-drill-genesis-salt: it would sign under public testnet-12's domain (P2-12 review finding 1)"
}

port_listening() { lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1 || { command -v ss >/dev/null 2>&1 && ss -ltn "sport = :$1" 2>/dev/null | grep -q LISTEN; }; }

# The host guard. Refuses this Mac (any macOS host), and any host running a public testnet-12 node, whichever
# way it was started: an active unit named misaka-t12-node* or whose unit file runs testnet-12 without a
# drill salt; a kaspad process on --netsuffix=12 without a drill salt whose app dir is not ours; a listener
# on one of public testnet-12's default ports (a node started from a --configfile names its network in TOML,
# not on its command line, so only its sockets give it away); or a default testnet-12 app dir without a drill
# marker under any user's home.
check_host() {
  [ "$(uname -s)" != "Darwin" ] || die "never on the operator's Mac (or any macOS host): the drill runs on fleet hosts only"
  if command -v systemctl >/dev/null 2>&1; then
    local unit file
    for unit in $(systemctl list-units --type=service --state=active --no-legend 2>/dev/null | awk '{print $1}' || true); do
      # the deploy kit's public units: misaka-t12-node*, misaka-t12-seat2..7 (drop-ins over older units),
      # and the retired live chain's floor seats misaka-t12f-*
      case "$unit" in misaka-t12-node*|misaka-t12-seat*|misaka-t12f-*) die "this host runs a public testnet-12 node ($unit) — never drill beside one" ;; esac
      # the unit file AND its drop-ins: the deploy kit puts a public node's ExecStart in a drop-in
      for file in $(systemctl show -p FragmentPath,DropInPaths --value "$unit" 2>/dev/null | tr ' ' '\n' || true); do
        [ -n "$file" ] && [ -r "$file" ] || continue
        if grep -q -E -- '--netsuffix(=| )12|testnet-12|misaka-testnet-12|/t12-rel/' "$file" && ! grep -q -- '--palw-drill-genesis-salt' "$file"; then
          die "this host runs a public testnet-12 node (unit $unit, $file) — never drill beside one"
        fi
      done
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
  done < <(pgrep -af kaspad 2>/dev/null | grep -E -- '--netsuffix(=| )12|--configfile|-C ' || true)
  local port
  for port in $PUBLIC_T12_PORTS; do
    port_listening "$port" && die "port $port (a public testnet-12 default) is listening — a public node may run here from a config file"
  done
  local home dir
  for home in /root /home/*; do
    dir="$home/.rusty-kaspa/misaka-testnet-12"
    [ -d "$dir" ] || continue
    [ -f "$dir/palw-drill-genesis" ] || die "$dir holds a testnet-12 app dir with no drill marker — a public node's; never drill beside it"
  done
  log "host guard: not macOS, no public testnet-12 unit, process, listener or app dir on this host"
}

manifest() {
  # `manifest <python expression over m>` — prints the expression's value from the keyring manifest.
  [ -f "$MANIFEST" ] || die "no keyring at $MANIFEST — run \`$0 keyring\` first"
  python3 -c "import json,sys; m=json.load(open(sys.argv[1])); print($1)" "$MANIFEST"
}

# Every CLI call carries the salt: the CLI signs under the node's genesis or refuses (review finding 1).
cli() { "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$RPC_PORT" --palw-drill-genesis-salt="$SALT" "$@"; }

# The drill's own genesis, read back from the node — never assumed. A node that answers with public
# testnet-12's genesis is not this drill's node, and every step refuses to read evidence from it.
check_node_is_the_drill() {
  local want
  want="$(manifest "m['genesis_hash']")"
  grep -q -E -- "PALW DRILL CHAIN .*genesis $want" "$NODE_LOG" 2>/dev/null \
    || die "the node log at $NODE_LOG does not announce drill genesis $want — is seat $SEAT's node running with this drill's salt?"
}

# --------------------------------------------------------------------------------------------------------
# Evidence: a byte cursor before the action, then only what the log gained after it.
# --------------------------------------------------------------------------------------------------------

cursor() { { wc -c < "${1:-$NODE_LOG}"; } 2>/dev/null | tr -d ' ' || echo 0; }

virtual_daa() {
  cli node dag-info --output json 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["virtual_daa"])' 2>/dev/null || true
}

# `wait_after <cursor> <regex> <what> [log] [max_seconds]` — prints the first line matching regex written
# after cursor. Without max_seconds it polls on the chain's clock: gives up when the drill's virtual DAA
# stops moving for STALL_WAIT seconds or after STEP_WAIT. With max_seconds (a probe that is not on the
# drill's chain) it gives up after that many wall-clock seconds. Giving up is `die`: call it through
# `evidence` (or `x="$(wait_after …)" || exit`), never inside a bare `$( )`.
wait_after() {
  local from="$1" pattern="$2" what="$3" file="${4:-$NODE_LOG}" max="${5:-}" began=$SECONDS last_move=$SECONDS last_daa="" daa line
  while :; do
    line="$(tail -c +"$((from + 1))" "$file" 2>/dev/null | grep -m1 -E -- "$pattern" || true)"
    if [ -n "$line" ]; then echo "$line"; return 0; fi
    if [ -n "$max" ]; then
      [ $((SECONDS - began)) -lt "$max" ] || die "gave up waiting for: $what ($max s of wall clock)"
    else
      daa="$(virtual_daa)"
      if [ -n "$daa" ] && [ "$daa" != "$last_daa" ]; then last_daa="$daa"; last_move=$SECONDS; fi
      [ $((SECONDS - began)) -lt "$STEP_WAIT" ] || die "gave up waiting for: $what ($STEP_WAIT s into the step, DAA ${last_daa:-?})"
      [ $((SECONDS - last_move)) -lt "$STALL_WAIT" ] || die "gave up waiting for: $what (DAA ${last_daa:-?} unmoved for $STALL_WAIT s)"
    fi
    sleep 10
  done
}

# getPalwVesting (P2-10) through the CLI (P2-11). A CLI without the command cannot produce this evidence:
# that is INCOMPLETE — a distinct exit that `report` refuses to pass — never a SKIP that reads as a pass.
need_vesting_command() {
  "$CLI_BIN" palw vesting --help >/dev/null 2>&1 \
    || incomplete "this CLI has no \`palw vesting\` (getPalwVesting, P2-10/P2-11): the vesting evidence of this step cannot be read"
}

# `wait_vesting <python expression over v> <what>` — polls getPalwVesting until the expression is True. A
# failed poll (the RPC, the CLI, the JSON) is retried on the next round, never taken as an answer; the wait
# gives up only on the deadlines, as `die`.
wait_vesting() {
  local expr="$1" what="$2" began=$SECONDS last_move=$SECONDS last_daa="" daa doc verdict
  need_vesting_command
  while :; do
    doc="$(cli palw vesting --json 2>/dev/null || true)"
    if [ -n "$doc" ]; then
      verdict="$(printf '%s' "$doc" | python3 -c "import json,sys; v=json.load(sys.stdin); print(bool($expr))" 2>/dev/null || true)"
      [ "$verdict" = "True" ] && { echo "getPalwVesting: $what"; return 0; }
    fi
    daa="$(virtual_daa)"
    if [ -n "$daa" ] && [ "$daa" != "$last_daa" ]; then last_daa="$daa"; last_move=$SECONDS; fi
    [ $((SECONDS - began)) -lt "$STEP_WAIT" ] || die "gave up waiting for: $what ($STEP_WAIT s, DAA ${last_daa:-?})"
    [ $((SECONDS - last_move)) -lt "$STALL_WAIT" ] || die "gave up waiting for: $what (DAA ${last_daa:-?} unmoved for $STALL_WAIT s)"
    sleep 30
  done
}

# A step's manual part: printed, and the step ends MANUAL (exit 4) rather than PASS until `attest`.
MANUAL_NEEDED=0
manual() { log "MANUAL: $*"; MANUAL_NEEDED=1; }

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
  [ "$(manifest "len(m.get('evm') or [])")" -gt 0 ] || die "the keyring has no EVM accounts — this kaspad was built without the EVM lane"
  log "drill $(manifest "m['salt_id']"): genesis $(manifest "m['genesis_hash'][:16]")… (public testnet-12: $(manifest "m['public_genesis_hash'][:16]")…)"
}

# Every port a node or probe binds must be free and must not be one of public testnet-12's.
need_ports_free() {
  local port public
  for port in "$@"; do
    for public in $PUBLIC_T12_PORTS; do
      [ "$port" != "$public" ] || die "port $port is a public testnet-12 default — set P2P_BASE / RPC_BASE / EVM_RPC_BASE elsewhere"
    done
    port_listening "$port" && die "port $port is in use — set P2P_BASE / RPC_BASE / EVM_RPC_BASE"
  done
  return 0
}

cmd_node() {
  local seat="${1:-}"
  [[ "$seat" =~ ^[0-9]+$ ]] || die "usage: $0 node <seat 0..7>"
  seat_ports "$seat"
  need_salt; need_bins; check_host
  [ -n "$PEERS" ] || die "PEERS is required: the other drill nodes' ip:port (a drill has no discovery)"
  [ -f "$MANIFEST" ] || cmd_keyring
  local n_seats
  n_seats="$(manifest "len(m['seats'])")"
  [ "$seat" -lt "$n_seats" ] || die "seat $seat: the drill registry has $n_seats seats"
  need_ports_free "$P2P_PORT" "$RPC_PORT" "$EVM_RPC_PORT"
  mkdir -p "$NODE_DIR"
  # The log is appended across restarts: evidence of THIS start is what the log gains from here.
  local from; from="$(cursor)"
  # Listeners: P2P (public, for the other drill nodes), wRPC borsh and EVM HTTP (loopback), and NO gRPC —
  # its default 127.0.0.1:26210 is public testnet-12's and one per host, and a bind failure exits the node.
  local args=(--testnet --netsuffix=12 --palw-drill-genesis-salt="$SALT" --appdir="$NODE_DIR/app"
              --nodnsseed --disable-upnp --nogrpc --listen="0.0.0.0:$P2P_PORT" --rpclisten-borsh="127.0.0.1:$RPC_PORT"
              --evm-rpc-listen="127.0.0.1:$EVM_RPC_PORT"
              --utxoindex --enable-unsynced-mining --yes
              --palw-produce --palw-panel
              --palw-producer-key="$KEYRING/$(manifest "m['seats'][$seat]['seed_file']")"
              --palw-producer-bond="$(manifest "m['seats'][$seat]['bond_outpoint']")"
              --palw-fee-outpoint="$(manifest "m['seats'][$seat]['fee_float_outpoint']")"
              --palw-heartbeat-miner-address="$(manifest "m['heartbeat'][$seat]['address']")"
              --evm-fee-recipient="$(manifest "m['evm'][$seat]['address']")")
  [ -n "$PRODUCER_CLASS" ] && args+=(--palw-producer-class="$PRODUCER_CLASS")
  local peer
  for peer in $PEERS; do args+=(--addpeer="$peer"); done
  nohup "$KASPAD_BIN" "${args[@]}" >>"$NODE_LOG" 2>&1 &
  echo $! > "$NODE_DIR/kaspad.pid"
  log "drill node seat $seat started, pid $(cat "$NODE_DIR/kaspad.pid"), P2P $P2P_PORT, wRPC 127.0.0.1:$RPC_PORT, EVM 127.0.0.1:$EVM_RPC_PORT, log $NODE_LOG"
  evidence wait_after "$from" "PALW DRILL CHAIN .*genesis $(manifest "m['genesis_hash']")" "the node to announce this drill's genesis"
}

# `probe`: a throwaway process under WORK_DIR (its own app dir, loopback P2P, no gRPC or wRPC), killed on
# return. Used by D-1 only.
probe_pids=()
probe_stop() { local p; for p in "${probe_pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; probe_pids=(); }
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
  "report|(T10 step 11) the analyzer's three reports, and every step above PASS or attested"
)

cmd_steps() {
  local i=1 s
  for s in "${STEPS[@]}"; do printf '%2d  %s\n' "$i" "${s#*|}"; i=$((i + 1)); done
}

step_d1() {
  local drill_genesis public_genesis
  drill_genesis="$(manifest "m['genesis_hash']")"; public_genesis="$(manifest "m['public_genesis_hash']")"
  # (a) the drill refuses public testnet-12 on the genesis: a throwaway drill probe dials PUBLIC_PEER only.
  # Its OUTBOUND handshake failure is logged at debug, so the probe runs at debug (it lives PROBE_WAIT s).
  if [ -n "$PUBLIC_PEER" ]; then
    local dir="$WORK_DIR/probe-d1a"; rm -rf "$dir"; mkdir -p "$dir"
    need_ports_free "$((P2P_BASE + 50))"
    "$KASPAD_BIN" --testnet --netsuffix=12 --palw-drill-genesis-salt="$SALT" --appdir="$dir/app" --nodnsseed --disable-upnp \
      --nogrpc --loglevel=debug --listen="127.0.0.1:$((P2P_BASE + 50))" --connect="$PUBLIC_PEER" >"$dir/kaspad.out" 2>&1 &
    probe_pids+=($!)
    evidence wait_after 0 "handshake failed for outbound peer .*Genesis mismatch on network testnet-12 - local: $drill_genesis" \
      "the drill probe to refuse public testnet-12 on the genesis" "$dir/kaspad.out" "$PROBE_WAIT"
    probe_stop
  else
    manual "set PUBLIC_PEER=<a public testnet-12 node ip:port> to show the genesis refusal against the live network"
  fi
  # (b) a pre-R-core+ testnet-12 binary is refused on the RULES: two loopback-only probes on the PUBLIC
  # genesis, no DNS, connected to each other and to nothing else. The old binary dials; this build is the
  # inbound side, which logs its refusal at WARN. The handshake compares the genesis FIRST (then the params
  # id, then the fork id), so a params-id or fork-id refusal is itself proof that the two share the genesis;
  # a genesis refusal means the old build is another genesis's, and D-1(b) fails for the wrong reason.
  if [ -n "$OLD_KASPAD_BIN" ]; then
    local dir="$WORK_DIR/probe-d1b" line new_fp old_fp; rm -rf "$dir"; mkdir -p "$dir"
    need_ports_free "$((P2P_BASE + 60))" "$((P2P_BASE + 61))"
    "$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$dir/new" --nodnsseed --disable-upnp --nogrpc \
      --listen="127.0.0.1:$((P2P_BASE + 60))" >"$dir/new.out" 2>&1 &
    probe_pids+=($!)
    "$OLD_KASPAD_BIN" --testnet --netsuffix=12 --appdir="$dir/old" --nodnsseed --disable-upnp --nogrpc \
      --listen="127.0.0.1:$((P2P_BASE + 61))" --connect="127.0.0.1:$((P2P_BASE + 60))" >"$dir/old.out" 2>&1 &
    probe_pids+=($!)
    line="$(wait_after 0 "handshake failed for inbound peer .*(Consensus params mismatch|Fork-id mismatch|Genesis mismatch) on network" \
      "the pre-R-core+ binary to be refused" "$dir/new.out" "$PROBE_WAIT")" || exit $?
    case "$line" in
      *"Genesis mismatch"*) die "D-1(b): the old binary is on ANOTHER genesis — refused for the wrong reason; use a pre-R-core+ build of public testnet-12: $line" ;;
    esac
    new_fp="$(grep -m1 -o -E 'Consensus params fingerprint: [0-9a-f]+' "$dir/new.out" || true)"
    old_fp="$(grep -m1 -o -E 'Consensus params fingerprint: [0-9a-f]+' "$dir/old.out" || true)"
    [ -n "$new_fp" ] && [ "$new_fp" != "$old_fp" ] \
      || die "D-1(b): the two probes announce the same fingerprint ('$new_fp' / '$old_fp') — OLD_KASPAD_BIN is not a pre-R-core+ build"
    log "    $line"
    log "    ($new_fp here; the old build's $old_fp)"
    probe_stop
  else
    manual "set OLD_KASPAD_BIN=<a pre-R-core+ testnet-12 build> for D-1's second half"
  fi
}

step_d2() {
  local from; from="$(cursor)"
  log "    waiting for a seat on this host to license a floor claim (after the step began)"
  evidence wait_after "$from" "\\[palw-panel\\] claim [0-9a-f]+: licensed" "a licence"
  log "    waiting for a vesting row (the first Final): getPalwVesting totals.vesting_created > 0"
  evidence wait_vesting "(v.get('totals') or {}).get('vesting_created', 0) > 0" "a vesting row at a Final"
}

step_d3() {
  manual "stage a Withheld-seen claim (a seat with --palw-drill-refuse-leaf-evidence, allowed on a salted drill), a Sampled seat and an S2 licence;"
  manual "then read their rows: w+E held until the upgrade; the S2 claim's door becomes Coverage/Quorum through the V3 supplementary set."
  evidence wait_vesting "len((v.get('licence_histogram') or {})) >= 2" "licences through at least two doors"
}

step_d4() {
  manual "restart one seat's producer with --palw-drill-tamper-leaf=<leaf> (the shard-court drill's tamper path) on this drill only;"
  manual "a P2-8 seat files ExecutorRefuted from its capture arm before Final (S2)."
  local from; from="$(cursor)"
  evidence wait_vesting "(v.get('totals') or {}).get('vesting_burned', 0) > 0" "a row burned by a post-Final conviction (the Burned note)"
  log "    evidence after the action only: $(tail -c +"$((from + 1))" "$NODE_LOG" | grep -c -E 'ExecutorRefuted|PanelFalseValid' || true) conviction line(s) in this node's log"
}

step_d5() {
  manual "open two DefaultAccused on one claim from two seats (misaka --palw-drill-genesis-salt=\$SALT palw da …); a Valid signer answers the drawn units with MaterialDisclosedV2 (P2-7)."
}

step_d6() {
  manual "stop every drill producer (--palw-produce off; heartbeats only), then file a conviction and a reveal from a seat (the CLI with --palw-drill-genesis-salt)."
  local from; from="$(cursor)"
  evidence wait_after "$from" "heartbeat #[0-9]+ [0-9a-f]+ carries [1-9][0-9]* lifecycle carrier" "a heartbeat carrying the conviction (H-1)"
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
  evidence wait_after "$from" "holding: top up [0-9]+ sompi to reach the producer floor" "the producer to hold for a top-up"
  manual "re-register that operator with a NEW drill-only bond key (manifest 'bonds'[2], ≥ 13,000 MSK): production resumes, no block refused."
}

step_market() {
  # The EVM lane is the one the salt does not separate: the generator signs ONLY with these accounts (the
  # node's EVM ingress refuses any other sender; an account with value on public testnet-12 would sign
  # transactions valid there at the same nonce).
  log "    the drill's EVM accounts (manifest 'evm'; key files under $KEYRING) — the only senders the generator may use:"
  manifest "'\n'.join('      %s  %s' % (r['address'], r['key_file']) for r in m['evm'])" >&2
  manual "fund those accounts by a bridge deposit from the drill main wallet, then run a carrier and an EVM buy/sell generator over http://127.0.0.1:$EVM_RPC_PORT at > 8 rows per block while rows mature."
  evidence wait_vesting "(v.get('totals') or {}).get('backlog_legs', 0) <= 1032" "the queue at or under 1,032 legs"
}

step_maturity() {
  evidence wait_vesting "(v.get('totals') or {}).get('vesting_moved', 0) > 0" "the first natural maturity (a row moved)"
  manual "find the minted output (misaka wallet utxo list); submit its spend at mint+599 (refused by the mempool) and at mint+600 (accepted), 0 DNS anchors."
}

# The ledger: the latest recorded result of each step before `report`.
latest_status() { [ -f "$STATUS_FILE" ] && awk -F'\t' -v k="$1" '$1 == k { s = $3 } END { print s }' "$STATUS_FILE" || true; }

step_report() {
  [ -f "$WORK_DIR/snapshots.jsonl" ] || die "no snapshots at $WORK_DIR/snapshots.jsonl — run \`$0 snapshot\` from cron during the drill"
  local rc=0 k status open=""
  python3 "$ANALYZER" "$WORK_DIR" || rc=$?
  for ((k = 1; k < ${#STEPS[@]}; k++)); do
    status="$(latest_status "$k")"
    case "$status" in
      PASS|ATTESTED) ;;
      *) open="$open $k:${status:-NOT-RUN}" ;;
    esac
  done
  [ -z "$open" ] || { log "steps not passed:$open"; incomplete "the drill is not complete until every step above is PASS or ATTESTED"; }
  case "$rc" in
    0) ;;
    3) incomplete "the analyzer's reports are not all available yet" ;;
    *) die "the analyzer reports a failed criterion (exit $rc)" ;;
  esac
}

# `run_step_body <id>` — runs step_<id> and prints its exit status. The step runs in a subshell: its `die` /
# `incomplete` end the step, not this recorder, and its manual lines set MANUAL_NEEDED where the subshell
# can read it. NOT as `( … ) || rc=$?`: bash ignores `set -e` inside anything on the left of `||`, so a
# failing command would run on to PASS. errexit is switched off around the subshell and on again inside it,
# and the subshell kills its own probes (traps are not inherited into a subshell). The step's own output
# goes to stderr; stdout carries only the status.
run_step_body() {
  local id="$1" rc
  set +e
  (
    set -e
    trap probe_stop EXIT
    case "$id" in d1|report|selftest_*) ;; *) check_node_is_the_drill ;; esac
    "step_$id"
    [ "$MANUAL_NEEDED" = 0 ] || exit "$RC_MANUAL"
  ) >&2
  rc=$?
  set -e
  echo "$rc"
}

status_of_rc() {
  case "$1" in
    0) echo PASS ;;
    "$RC_INCOMPLETE") echo INCOMPLETE ;;
    "$RC_MANUAL") echo MANUAL ;;
    *) echo FAIL ;;
  esac
}

record_status() { printf '%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${4:-}" >> "$STATUS_FILE"; }

cmd_step() {
  local k="${1:-}"
  [[ "$k" =~ ^[0-9]+$ ]] && [ "$k" -ge 1 ] && [ "$k" -le "${#STEPS[@]}" ] || die "usage: $0 step <1..${#STEPS[@]}> (see \`$0 steps\`)"
  need_salt; need_bins; check_host
  local entry="${STEPS[$((k - 1))]}"
  local id="${entry%%|*}" rc=0 status
  mkdir -p "$WORK_DIR"
  log "$k/${#STEPS[@]} ${entry#*|} (seat $SEAT)"
  rc="$(run_step_body "$id")"
  status="$(status_of_rc "$rc")"
  record_status "$k" "$id" "$status"
  log "$k/${#STEPS[@]} $status$([ "$status" = MANUAL ] && echo " — do the MANUAL part, then \`$0 attest $k '<evidence>'\`")"
  exit "$rc"
}

cmd_attest() {
  local k="${1:-}" text="${2:-}"
  [[ "$k" =~ ^[0-9]+$ ]] && [ "$k" -ge 1 ] && [ "$k" -lt "${#STEPS[@]}" ] || die "usage: $0 attest <1..$((${#STEPS[@]} - 1))> '<evidence>'"
  [ -n "$text" ] || die "attest needs the evidence itself (a log line, an RPC answer, a tx id) — not a promise"
  [ "$(latest_status "$k")" = MANUAL ] || die "step $k's latest result is '$(latest_status "$k")', not MANUAL: only a step whose automated evidence passed can be attested"
  local entry="${STEPS[$((k - 1))]}"
  record_status "$k" "${entry%%|*}" ATTESTED "$text"
  log "$k attested"
}

cmd_snapshot() {
  need_salt
  local wall dag vesting
  wall="$(python3 -c 'import time; print(int(time.time() * 1000))')"
  dag="$(cli node dag-info --output json 2>/dev/null || echo null)"
  vesting="$(cli palw vesting --json 2>/dev/null || true)"; [ -n "$vesting" ] || vesting=null
  python3 - "$WORK_DIR/snapshots.jsonl" "$wall" "$(hostname)" "$SEAT" "$dag" "$vesting" <<'PY'
import json, sys
path, wall, host, seat, dag, vesting = sys.argv[1:7]
row = {"format": "misaka-palw-drill-snapshot/v1", "wall_ms": int(wall), "host": host, "seat": int(seat),
       "dag": json.loads(dag), "vesting": json.loads(vesting)}
with open(path, "a") as f:
    f.write(json.dumps(row, sort_keys=True) + "\n")
PY
}

cmd_analyze() { python3 "$ANALYZER" "$WORK_DIR" "$@"; }

# `self-test` — the step machinery on a scratch log, no node, no host (it runs anywhere, the Mac included):
# a wait that gives up FAILS its step (review finding 3: it used to print FATAL, then "done", and exit 0);
# a command that fails mid-step fails it; a missing getPalwVesting is INCOMPLETE; a manual part is MANUAL
# until attested; `report`'s ledger passes only PASS and ATTESTED.
step_selftest_gives_up() { evidence wait_after 0 "never written" "a line that never comes" "$SELFTEST_LOG" 1; log "reached"; }
step_selftest_fails_midway() { false; log "reached"; }
step_selftest_no_vesting() { evidence wait_vesting "True" "anything"; }
step_selftest_manual() { evidence wait_after 0 "licensed" "a licence" "$SELFTEST_LOG" 5; manual "do the rest by hand"; }
step_selftest_passes() { evidence wait_after 0 "licensed" "a licence" "$SELFTEST_LOG" 5; }
cmd_self_test() {
  local tmp got
  tmp="$(mktemp -d)"
  WORK_DIR="$tmp"; STATUS_FILE="$tmp/steps.status"; SELFTEST_LOG="$tmp/node.log"; CLI_BIN=/usr/bin/false
  printf '[palw-panel] claim ab: licensed\n' > "$SELFTEST_LOG"
  for pair in gives_up:FAIL fails_midway:FAIL no_vesting:INCOMPLETE manual:MANUAL passes:PASS; do
    got="$(status_of_rc "$(run_step_body "selftest_${pair%%:*}" 2>/dev/null)")"
    [ "$got" = "${pair#*:}" ] || { rm -rf "$tmp"; die "self-test: step selftest_${pair%%:*} is $got, not ${pair#*:}"; }
    log "self-test: selftest_${pair%%:*} → $got"
  done
  record_status 1 d1 PASS; record_status 2 d2 MANUAL; record_status 2 d2 ATTESTED "tx 00"; record_status 3 d3 INCOMPLETE
  [ "$(latest_status 2)" = ATTESTED ] && [ "$(latest_status 3)" = INCOMPLETE ] && [ -z "$(latest_status 4)" ] \
    || { rm -rf "$tmp"; die "self-test: the ledger does not read back the latest result"; }
  rm -rf "$tmp"
  log "self-test: a failed wait fails its step; missing evidence is INCOMPLETE; the ledger reads back"
}

case "${1:-}" in
  new-salt) cmd_new_salt ;;
  check-host) check_host ;;
  keyring) cmd_keyring ;;
  node) shift; cmd_node "$@" ;;
  steps) cmd_steps ;;
  step) shift; cmd_step "$@" ;;
  attest) shift; cmd_attest "$@" ;;
  snapshot) cmd_snapshot ;;
  analyze) shift; cmd_analyze "$@" ;;
  self-test) cmd_self_test ;;
  *) sed -n '2,60p' "$0"; exit 2 ;;
esac
