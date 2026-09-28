#!/usr/bin/env bash
# misaka-palw-tir-drill-df3.sh — RFC-0002 Phase F drill D-F3: the 2026-09-07 forged-output red-team
# (eight attacks) against a drill node whose producer mines an IR class, past `palw_tir_v1`.
#
# One step of the ONE salted testnet-12 drill chain D-F1…D-F4 run on. The runner and the node layout are
# the node lane's (audit-tir/df.sh on tir/node: `df.sh df3` calls `run` here with the layout exported);
# this script reads the layout from the environment only and starts nothing but the harness. The eight
# attacks — raw_skeleton, nonce_grind, algo_downgrade_khh, forged_commitment, tamper_state_root,
# fat_coinbase, inflate_position, insider_wellformed_envelope — take a real template from new0, tamper
# with it and submit it; a refused block fails local validation and is never gossiped, so the battery is
# safe on a drill. The insider envelope names the drill's IR class and its inventory root
# (`--class-id`/`--artifact-root`), so the deepest forgery is refused on the IR class's own path. The
# templates are paid to the drill keyring's main address: a drill serves templates only to its own
# keyring's addresses (ADR-0152 §8.2).
#
# PASS: 8/8 blocked, new0 alive after the battery (its wRPC answers, its log gained no panic), and new0's
# DAA past TIR_AT (the battery runs under the IR rules). FAIL otherwise; INCOMPLETE (3) when the chain is
# not past TIR_AT yet or the IR class files are not written yet.
#
#   misaka-palw-tir-drill-df3.sh dry   check the binaries, the environment and the plan; start nothing
#   misaka-palw-tir-drill-df3.sh run   the battery, then the evidence
#
# ENV (the drill layout, shared with D-F1/D-F2/D-F4; defaults are audit-tir/lib-df.sh's):
#   SALT            the drill's salt (64 hex); else $WORK_DIR/SALT
#   WORK_DIR        ~/.misaka-palw-tir-drill; ir-class.id, ir-artifact.root, keyring/manifest.json live here
#   KASPAD_BIN      the release under test (its directory holds the CLI and the harness by default)
#   CLI_BIN         misaka (reads new0's DAA over wRPC, HOME=$WORK_DIR/userhome)
#   REDTEAM_BIN     redteam (misaminer's second binary: cargo build --release -p misaminer --bin redteam)
#   NEW0_GRPC       new0's gRPC loopback port (55100) — the harness's transport
#   NEW0_RPC        new0's wRPC borsh loopback port (57100)
#   NEW0_LOG        new0's log ($WORK_DIR/new0/kaspad.out)
#   TIR_AT          the drill's palw_tir_v1 height (20)
#   PAY_ADDRESS     the templates' pay address (default: the keyring manifest's main address)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-tir-drill}"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
BIN_DIR="$(dirname "$KASPAD_BIN")"
CLI_BIN="${CLI_BIN:-$BIN_DIR/misaka}"
REDTEAM_BIN="${REDTEAM_BIN:-$BIN_DIR/redteam}"
NEW0_GRPC="${NEW0_GRPC:-55100}"
NEW0_RPC="${NEW0_RPC:-57100}"
NEW0_LOG="${NEW0_LOG:-$WORK_DIR/new0/kaspad.out}"
TIR_AT="${TIR_AT:-20}"
PAY_ADDRESS="${PAY_ADDRESS:-}"
UHOME="$WORK_DIR/userhome"
EVIDENCE="$WORK_DIR/evidence/df3"

RC_FAIL=1
RC_INCOMPLETE=3

log() { printf '[tir-drill D-F3] %s\n' "$*" >&2; }
die() { log "FAIL: $*"; exit "$RC_FAIL"; }
incomplete() { log "INCOMPLETE: $*"; exit "$RC_INCOMPLETE"; }

hex128() { [[ "$1" =~ ^[0-9a-f]{128}$ ]]; }

# The salt: SALT from the environment, else $WORK_DIR/SALT (never printed).
load_salt() {
  SALT="${SALT:-}"
  [ -n "$SALT" ] || SALT="$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)"
  [[ "$SALT" =~ ^[0-9a-f]{64}$ ]] || die "no drill salt (SALT, or $WORK_DIR/SALT)"
}

need_env() {
  load_salt
  [[ "$NEW0_GRPC" =~ ^[0-9]+$ ]] || die "NEW0_GRPC must be new0's gRPC loopback port"
  [[ "$NEW0_RPC" =~ ^[0-9]+$ ]] || die "NEW0_RPC must be new0's wRPC borsh loopback port"
  [[ "$TIR_AT" =~ ^[0-9]+$ ]] && [ "$TIR_AT" -gt 0 ] || die "TIR_AT must be a positive DAA"
}

need_bins() {
  [ -x "$REDTEAM_BIN" ] || die "no red-team harness at $REDTEAM_BIN (cargo build --release -p misaminer --bin redteam)"
  [ -x "$CLI_BIN" ] || die "no misaka CLI at $CLI_BIN"
}

# The templates' pay address: PAY_ADDRESS, else the drill keyring's main address.
pay_address() {
  if [ -n "$PAY_ADDRESS" ]; then echo "$PAY_ADDRESS"; return 0; fi
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["main"]["address"])' "$WORK_DIR/keyring/manifest.json" 2>/dev/null || true
}

# new0's virtual DAA through the CLI (the rcore drill script's reading, `node dag-info`).
virtual_daa() {
  mkdir -p "$UHOME"
  HOME="$UHOME" "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$NEW0_RPC" --palw-drill-genesis-salt="$SALT" node dag-info --output json 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["virtual_daa"])' 2>/dev/null || true
}

cursor() { { wc -c < "$NEW0_LOG"; } 2>/dev/null | tr -d ' ' || echo 0; }

cmd_dry() {
  need_env
  local problems=0
  [ -x "$REDTEAM_BIN" ] || { log "dry: the harness is not built yet ($REDTEAM_BIN)"; problems=1; }
  [ -x "$CLI_BIN" ] || { log "dry: the CLI is not built yet ($CLI_BIN)"; problems=1; }
  log "dry: the plan —"
  log "  1. INCOMPLETE until new0's virtual DAA is past TIR_AT=$TIR_AT and $WORK_DIR/ir-class.id,"
  log "     $WORK_DIR/ir-artifact.root are written (df.sh class)"
  log "  2. $REDTEAM_BIN --rpc 127.0.0.1:$NEW0_GRPC --network-id testnet-12 --attack all \\"
  log "       --class-id <ir-class.id> --artifact-root <ir-artifact.root> --pay-address <keyring main>  → $EVIDENCE/battery.out"
  log "  3. PASS iff '8/8 forgeries BLOCKED', new0's wRPC still answers, and new0's log gained no 'panicked'"
  if [ "$problems" -ne 0 ]; then
    log "dry: the plan is well-formed; build the missing binaries before the run"
  fi
  log "dry: OK"
}

cmd_run() {
  need_env
  need_bins
  local daa
  daa="$(virtual_daa)"
  [[ "$daa" =~ ^[0-9]+$ ]] || incomplete "new0's wRPC at 127.0.0.1:$NEW0_RPC does not answer a DAG info"
  [ "$daa" -gt "$TIR_AT" ] || incomplete "virtual DAA $daa is not past TIR_AT=$TIR_AT yet: the battery runs under the IR rules"
  local class root pay
  class="$(tr -d ' \n' < "$WORK_DIR/ir-class.id" 2>/dev/null || true)"
  root="$(tr -d ' \n' < "$WORK_DIR/ir-artifact.root" 2>/dev/null || true)"
  hex128 "$class" || incomplete "no IR class id in $WORK_DIR/ir-class.id (df.sh class writes it)"
  hex128 "$root" || incomplete "no IR artifact root in $WORK_DIR/ir-artifact.root"
  pay="$(pay_address)"
  [ -n "$pay" ] || incomplete "no pay address (PAY_ADDRESS, or $WORK_DIR/keyring/manifest.json's main address)"
  mkdir -p "$EVIDENCE"
  local from; from="$(cursor)"
  log "battery at DAA $daa against 127.0.0.1:$NEW0_GRPC, insider envelope on IR class ${class:0:16}…"
  set +e
  "$REDTEAM_BIN" --rpc "127.0.0.1:$NEW0_GRPC" --network-id testnet-12 --attack all --class-id "$class" --artifact-root "$root" \
    --pay-address "$pay" > "$EVIDENCE/battery.out" 2> "$EVIDENCE/battery.err"
  local rc=$?
  set -e
  cat "$EVIDENCE/battery.out" >&2
  grep -q "=== result: 8/8 forgeries BLOCKED by the live node ===" "$EVIDENCE/battery.out" \
    || die "not 8/8 blocked (harness exit $rc; $EVIDENCE/battery.out, $EVIDENCE/battery.err)"
  [ "$rc" -eq 0 ] || die "the harness exited $rc"
  local after; after="$(virtual_daa)"
  [[ "$after" =~ ^[0-9]+$ ]] || die "new0 stopped answering after the battery"
  if tail -c +"$((from + 1))" "$NEW0_LOG" 2>/dev/null | grep -q "panicked"; then
    die "new0's log gained a panic during the battery"
  fi
  # The reasons, as the node logged them (the 2026-09-07 three layers: shape, algo, stateless admission).
  tail -c +"$((from + 1))" "$NEW0_LOG" 2>/dev/null | grep -E "triggered an error|Reject" | tail -n 16 > "$EVIDENCE/reasons.log" || true
  log "PASS: 8/8 forgeries refused past palw_tir_v1 (DAA $daa → $after), no panic; reasons in $EVIDENCE/reasons.log"
}

case "${1:-help}" in
  dry) cmd_dry ;;
  run) cmd_run ;;
  help | -h | --help | *) sed -n '2,37p' "$0"; [ "${1:-help}" = help ] || [ "${1:-}" = -h ] || [ "${1:-}" = --help ] || exit 2 ;;
esac
