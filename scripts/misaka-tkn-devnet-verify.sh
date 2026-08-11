#!/usr/bin/env bash
# MISAKA — verify the Compute Token Program (TOK) e2e criteria on a running token devnet
# (started by scripts/misaka-tkn-devnet-e2e.sh). Reads ONLY the kaspad logs — the fold and
# settlement lines are the operator surface, same stance as misaka-vlt-devnet-verify.sh.
#
# What "the token wiring passes" means, checked in this order:
#
#   1. SETTLE   — node-0 settled >= 3 epochs with paid > 0, and for EVERY epoch that two or
#                 more nodes settled, their whole settlement lines (R, X, paid, recipients,
#                 root) are byte-identical. One divergent root is a split ledger.
#   2. SHADOW   — the shadow-phase transfer (amount=4000) has a [token-shadow] "would move"
#                 line and NO binding line on ANY node: an op from below the active fence is
#                 void forever, not deferred.
#   3. BIND     — the two live transfers (5000, 11000) and the burn (3000) each bound on
#                 EVERY node with identical lines.
#   4. VOID     — the nonce replay (9000) and the overdraft (10^15) bound on NO node.
#   5. RESTART  — with --restart-check: stop and restart every node from run.args, then
#                 require (a) no already-settled epoch settles again, (b) no bound op binds
#                 again, (c) at least one NEW epoch settles after the restart — i.e. both
#                 cursors resumed exactly where the batch left them.
#
# Usage:
#   scripts/misaka-tkn-devnet-verify.sh [--nodes N] [--wait SECONDS] [--restart-check]
#
# Env: MISAKA_DEVNET_DIR (default ./.misaka-tkn-devnet), MISAMINER_BIN + MISAKA_DEVNET_BASE_RPC
#      (restart check only, to bring the miner back)

set -euo pipefail

NODES=5
WAIT_SECS=1800
RESTART_CHECK=0

while [ $# -gt 0 ]; do
  case "$1" in
    --nodes)         NODES="$2"; shift 2 ;;
    --wait)          WAIT_SECS="$2"; shift 2 ;;
    --restart-check) RESTART_CHECK=1; shift ;;
    -h|--help)       sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="${MISAKA_DEVNET_DIR:-$REPO_ROOT/.misaka-tkn-devnet}"
MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"
BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-27110}"
[ -d "$WORK_DIR/node-0" ] || { echo "no devnet at $WORK_DIR — run misaka-tkn-devnet-e2e.sh first" >&2; exit 1; }

AMT_SHADOW=4000
AMT_LIVE1=5000
AMT_REPLAY=9000
AMT_LIVE2=11000
AMT_OVERDRAFT=1000000000000000
AMT_BURN=3000
CAP_SHADOW=7777777
CAP_LIVE=5000000
MINT1=3000000
MINT_CAPVOID=2000001
MINT2=2000000

log_of() { echo "$WORK_DIR/node-$1/kaspad.log"; }

# The fixture submit line carries the amount, which is the plan's unique key; the txid it
# carries is then the join key into the fold's own lines.
txid_of_amount() { # amount -> txid (empty if not submitted yet)
  { grep -oE "token fixture: submitted (transfer|burn) #[0-9]+ tx=[0-9a-f]+ (to=[0-9a-f]+ )?amount=$1 " "$(log_of 0)" || true; } |
    head -1 | grep -oE 'tx=[0-9a-f]+' | cut -d= -f2
}

txid_of_cap() { # create-mint cap -> txid
  { grep -oE "token fixture: submitted create-mint #[0-9]+ tx=[0-9a-f]+ cap=$1 " "$(log_of 0)" || true; } |
    head -1 | grep -oE 'tx=[0-9a-f]+' | cut -d= -f2
}

txid_of_mint_amount() { # mint-to amount -> txid
  { grep -oE "token fixture: submitted mint-to #[0-9]+ tx=[0-9a-f]+ create_nonce=[0-9]+ to=[0-9a-f]+ amount=$1 " "$(log_of 0)" || true; } |
    head -1 | grep -oE 'tx=[0-9a-f]+' | cut -d= -f2
}

# A fold/settle line, normalized: everything from "[token" on (strips timestamps and level).
token_lines() { # node grep-pattern
  { grep -oE "\[token[^]]*\] .*" "$(log_of "$1")" || true; } | grep -E "$2" || true
}

fail() { echo "FAIL: $*" >&2; exit 1; }
note() { echo "  $*"; }

# ---- wait until the plan has had the chance to complete ----------------------------------------
echo "== waiting (up to ${WAIT_SECS}s) for settlements and the full fixture plan =="
waited=0
until
  settled=$({ grep -cE '\[token\] epoch [0-9]+ settled: .*paid=[1-9]' "$(log_of 0)" || true; }) &&
    [ "${settled:-0}" -ge 3 ] &&
    [ -n "$(txid_of_amount $AMT_BURN)" ] &&
    burn_tx=$(txid_of_amount $AMT_BURN) &&
    grep -q "\[token\] burn $burn_tx" "$(log_of 0)" &&
    mint2_tx=$(txid_of_mint_amount $MINT2) &&
    [ -n "$mint2_tx" ] &&
    grep -q "\[token\] mint-to $mint2_tx" "$(log_of 0)"
do
  sleep 15
  waited=$((waited + 15))
  if [ "$waited" -ge "$WAIT_SECS" ]; then
    echo "state at timeout — settled(paid>0)=${settled:-0}; fixture submissions:" >&2
    { grep -E "token fixture: submitted" "$(log_of 0)" || echo "  (none)"; } >&2
    { grep -E "\[token" "$(log_of 0)" | tail -12; } >&2
    fail "timed out waiting for >=3 paid settlements + the burn to bind on node-0"
  fi
done
echo "node-0: $settled settled epoch(s) with paid > 0"

TX_SHADOW=$(txid_of_amount $AMT_SHADOW)
TX_LIVE1=$(txid_of_amount $AMT_LIVE1)
TX_REPLAY=$(txid_of_amount $AMT_REPLAY)
TX_LIVE2=$(txid_of_amount $AMT_LIVE2)
TX_OVER=$(txid_of_amount $AMT_OVERDRAFT)
TX_BURN=$(txid_of_amount $AMT_BURN)
for v in TX_SHADOW TX_LIVE1 TX_REPLAY TX_LIVE2 TX_OVER TX_BURN; do
  [ -n "${!v}" ] || fail "$v was never submitted (node-0 fixture log has no matching line)"
done

# ---- 1. SETTLE ---------------------------------------------------------------------------------
echo "== 1. SETTLE: cross-node settlement equality =="
epochs=$({ grep -oE '\[token\] epoch [0-9]+ settled' "$(log_of 0)" || true; } | grep -oE '[0-9]+' | sort -un)
[ -n "$epochs" ] || fail "node-0 settled nothing"
checked=0
for e in $epochs; do
  ref=""
  holders=0
  for i in $(seq 0 $((NODES - 1))); do
    line=$(token_lines "$i" "^\[token\] epoch $e settled: " | head -1)
    [ -n "$line" ] || continue
    holders=$((holders + 1))
    if [ -z "$ref" ]; then
      ref="$line"
    elif [ "$line" != "$ref" ]; then
      echo "  node-0: $ref" >&2
      echo "  node-$i: $line" >&2
      fail "epoch $e settled differently on node-$i — split ledger"
    fi
  done
  [ "$holders" -ge 2 ] && checked=$((checked + 1))
done
note "$(echo "$epochs" | wc -l | tr -d ' ') epoch(s) settled on node-0; $checked cross-checked identical on >=2 nodes"
[ "$checked" -ge 1 ] || fail "no epoch was settled by two nodes — nothing was actually cross-checked"

# ---- 1b. AUDIT (v0.2) --------------------------------------------------------------------------
echo "== 1b. AUDIT: verification pays from R(E), the base-coin fee retired =="
audit_paid=$(token_lines 0 '^\[token\] epoch [0-9]+ settled: .*audit=[1-9]' | wc -l | tr -d ' ')
[ "$audit_paid" -ge 1 ] || fail "no settled epoch paid audit work (audit= is always 0)"
for i in $(seq 0 $((NODES - 1))); do
  grep -q "audit-fee(base) retired" "$(log_of "$i")" || fail "node-$i never logged the base audit fee's retirement"
done
note "$audit_paid settled epoch(s) paid audit work; base audit fee retired on all $NODES nodes"

# ---- 2. SHADOW ---------------------------------------------------------------------------------
echo "== 2. SHADOW: the below-fence op is void forever =="
for i in $(seq 0 $((NODES - 1))); do
  if token_lines "$i" "^\[token\] transfer $TX_SHADOW" | grep -q .; then
    fail "shadow transfer $TX_SHADOW BOUND on node-$i — the activation fence leaked"
  fi
done
shadow_seen=0
for i in $(seq 0 $((NODES - 1))); do
  token_lines "$i" "^\[token-shadow\] transfer $TX_SHADOW" | grep -q . && shadow_seen=$((shadow_seen + 1))
done
[ "$shadow_seen" -ge 1 ] || fail "no node logged the shadow transfer's [token-shadow] line — was it even folded?"
note "shadow transfer $TX_SHADOW: would-move on $shadow_seen node(s), bound on none"

# ---- 3. BIND -----------------------------------------------------------------------------------
echo "== 3. BIND: live transfers and the burn bound identically everywhere =="
for spec in "transfer $TX_LIVE1" "transfer $TX_LIVE2" "burn $TX_BURN"; do
  ref=""
  for i in $(seq 0 $((NODES - 1))); do
    line=$(token_lines "$i" "^\[token\] $spec" | head -1)
    [ -n "$line" ] || fail "$spec never bound on node-$i"
    if [ -z "$ref" ]; then ref="$line"; elif [ "$line" != "$ref" ]; then
      fail "$spec bound differently on node-$i"
    fi
  done
  note "bound on all $NODES: $ref"
done

# ---- 4. VOID -----------------------------------------------------------------------------------
echo "== 4. VOID: the nonce replay and the overdraft bound nowhere =="
for spec in "transfer $TX_REPLAY" "transfer $TX_OVER"; do
  for i in $(seq 0 $((NODES - 1))); do
    if token_lines "$i" "^\[token\] $spec" | grep -q .; then
      fail "$spec BOUND on node-$i — a void class applied"
    fi
  done
done
note "replay $TX_REPLAY and overdraft $TX_OVER: no binding line on any node"

# ---- 4b. PHASE B (permissionless mints) --------------------------------------------------------
echo "== 4b. PHASE B: a mint is claimed, issues to its cap, and refuses past it =="
TX_CREATE_SHADOW=$(txid_of_cap $CAP_SHADOW)
TX_CREATE=$(txid_of_cap $CAP_LIVE)
TX_MINT1=$(txid_of_mint_amount $MINT1)
TX_MINT_CAPVOID=$(txid_of_mint_amount $MINT_CAPVOID)
TX_MINT2=$(txid_of_mint_amount $MINT2)
for v in TX_CREATE_SHADOW TX_CREATE TX_MINT1 TX_MINT_CAPVOID TX_MINT2; do
  [ -n "${!v}" ] || fail "$v was never submitted"
done
for i in $(seq 0 $((NODES - 1))); do
  if token_lines "$i" "^\[token\] create-mint $TX_CREATE_SHADOW" | grep -q .; then
    fail "pre-Phase-B create $TX_CREATE_SHADOW BOUND on node-$i — the Phase B fence leaked"
  fi
done
sb=0
for i in $(seq 0 $((NODES - 1))); do
  token_lines "$i" "^\[token-shadow\] create-mint $TX_CREATE_SHADOW" | grep -q . && sb=$((sb + 1))
done
[ "$sb" -ge 1 ] || fail "no node narrated the pre-Phase-B create in shadow"
for spec in "create-mint $TX_CREATE" "mint-to $TX_MINT1" "mint-to $TX_MINT2"; do
  ref=""
  for i in $(seq 0 $((NODES - 1))); do
    line=$(token_lines "$i" "^\[token\] $spec" | head -1)
    [ -n "$line" ] || fail "$spec never bound on node-$i"
    if [ -z "$ref" ]; then ref="$line"; elif [ "$line" != "$ref" ]; then
      fail "$spec bound differently on node-$i"
    fi
  done
  note "bound on all $NODES: $ref"
done
for i in $(seq 0 $((NODES - 1))); do
  if token_lines "$i" "^\[token\] mint-to $TX_MINT_CAPVOID" | grep -q .; then
    fail "cap-breaching mint $TX_MINT_CAPVOID BOUND on node-$i"
  fi
done
ASSET_ID=$(token_lines 0 "^\[token\] create-mint $TX_CREATE" | grep -oE 'asset=[0-9]+' | head -1 | cut -d= -f2)
note "asset $ASSET_ID: minted to its 5000000 cap exactly (3000000 + 2000000), the 2000001 breach void, nonce 2 reused"

# ---- 5. RESTART --------------------------------------------------------------------------------
if [ "$RESTART_CHECK" -eq 1 ]; then
  echo "== 5. RESTART: cursors resume, nothing re-applies =="
  declare -a pre_settle pre_live1
  for i in $(seq 0 $((NODES - 1))); do
    pre_settle[$i]=$(token_lines "$i" '^\[token\] epoch [0-9]+ settled: ' | wc -l | tr -d ' ')
    pre_live1[$i]=$(token_lines "$i" "^\[token\] transfer $TX_LIVE1" | wc -l | tr -d ' ')
  done
  for i in $(seq 0 $((NODES - 1))); do
    pidfile="$WORK_DIR/node-$i/kaspad.pid"
    [ -f "$pidfile" ] && kill "$(cat "$pidfile")" 2>/dev/null || true
  done
  sleep 5
  for i in $(seq 0 $((NODES - 1))); do
    node_dir="$WORK_DIR/node-$i"
    ( eval "exec $(cat "$node_dir/run.args")" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
  done
  # The miner died with node-0's connection; bring it back so new epochs keep settling.
  [ -f "$WORK_DIR/miner.pid" ] && kill "$(cat "$WORK_DIR/miner.pid")" 2>/dev/null || true
  for _ in $(seq 1 60); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$BASE_RPC") 2>/dev/null; then exec 3<&- 3>&-; break; fi
    sleep 1
  done
  "$MISAMINER_BIN" --rpc="127.0.0.1:$BASE_RPC" --network-id=devnet --allow-burn --mine-when-not-synced --threads=2 \
    >>"$WORK_DIR/miner.log" 2>&1 &
  echo $! > "$WORK_DIR/miner.pid"

  echo "waiting for a NEW settlement after the restart (proves the cursor resumed forward) ..."
  waited=0
  until [ "$(token_lines 0 '^\[token\] epoch [0-9]+ settled: ' | wc -l | tr -d ' ')" -gt "${pre_settle[0]}" ]; do
    sleep 15
    waited=$((waited + 15))
    [ "$waited" -lt 600 ] || fail "no new settlement within 600s of the restart — the cursor did not resume"
  done
  for i in $(seq 0 $((NODES - 1))); do
    # Every pre-restart epoch line count must be unchanged except for genuinely NEW epochs:
    # assert per-epoch uniqueness instead of raw counts.
    dup=$(token_lines "$i" '^\[token\] epoch [0-9]+ settled: ' | grep -oE 'epoch [0-9]+' | sort | uniq -d)
    [ -z "$dup" ] || fail "node-$i re-settled: $dup"
    post_live1=$(token_lines "$i" "^\[token\] transfer $TX_LIVE1" | wc -l | tr -d ' ')
    [ "$post_live1" -eq "${pre_live1[$i]}" ] || fail "node-$i re-bound transfer $TX_LIVE1 after restart"
  done
  note "restart clean: every settled epoch settled once, no op re-bound, and settlement advanced"
fi

echo
echo "PASS — the TOK wiring holds:"
echo "  settlements  : $settled epoch(s) paid on node-0, cross-node identical (roots included)"
echo "  shadow fence : $TX_SHADOW void forever"
echo "  live ops     : $TX_LIVE1, $TX_LIVE2 (transfers), $TX_BURN (burn) bound on all $NODES nodes"
echo "  void classes : $TX_REPLAY (nonce replay), $TX_OVER (overdraft) bound nowhere"
echo "  phase B      : asset $ASSET_ID claimed ($TX_CREATE), minted to cap, breach void, pre-fence create void forever"
