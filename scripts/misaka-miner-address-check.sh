#!/usr/bin/env bash
# Read-only mining reward address checker for Discord/bot use.

set -uo pipefail

NETWORK="${MISAKA_NETWORK:-testnet-10}"
RPC="${MISAKA_RPC:-127.0.0.1:27210}"
MISAKA_BIN="${MISAKA_BIN:-misaka}"
ADDRESS=""
DISCORD=0
SELF_TEST=0

usage() {
  cat <<'EOF'
Usage:
  misaka-miner-address-check --address <misaka-address> [options]

Options:
  --address <addr>     Mining reward / wallet address.
  --network <id>       Network id. Default: testnet-10.
  --rpc <host:port>    Node wRPC Borsh endpoint. Default: 127.0.0.1:27210.
  --discord            Print one compact line for Discord bots.
  --self-test          Run parser test with embedded sample JSON.
  -h, --help           Show this help.

Examples:
  misaka-miner-address-check --address misakatest:...
  misaka-miner-address-check --address misakatest:... --discord
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --address)
      ADDRESS="${2:-}"
      shift 2
      ;;
    --network|--network-id)
      NETWORK="${2:-}"
      shift 2
      ;;
    --rpc|--node-rpc|--node-wrpc-borsh)
      RPC="${2:-}"
      shift 2
      ;;
    --discord)
      DISCORD=1
      shift
      ;;
    --self-test)
      SELF_TEST=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

have() {
  command -v "$1" >/dev/null 2>&1
}

msk_from_sompi() {
  awk -v s="$1" 'BEGIN { printf "%.8f", s / 100000000 }'
}

short_text() {
  awk -v s="$1" 'BEGIN { if (length(s) > 24) print substr(s,1,12) "..." substr(s,length(s)-7); else print s }'
}

# Plain-language gloss for a reward verdict, for display only.
reward_meaning() {
  case "$1" in
    RECENT_REWARD_SEEN)  echo "recent reward, immature coinbase present" ;;
    REWARD_HISTORY_SEEN) echo "past reward, mature only" ;;
    NO_REWARD_UTXO)      echo "no reward UTXO for this address" ;;
    *)                   echo "unknown" ;;
  esac
}

render() {
  local json="$1"

  if ! have jq; then
    echo "jq is required. Install on Ubuntu/Debian: apt -y install jq" >&2
    exit 2
  fi

  local address total mature_count mature_sompi immature_count immature_sompi verdict
  address="$(printf '%s' "$json" | jq -r '.address // "unknown"')"
  total="$(printf '%s' "$json" | jq -r '.total // 0')"
  mature_count="$(printf '%s' "$json" | jq -r '.mature.count // 0')"
  mature_sompi="$(printf '%s' "$json" | jq -r '.mature.sompi // 0')"
  immature_count="$(printf '%s' "$json" | jq -r '.immature.count // 0')"
  immature_sompi="$(printf '%s' "$json" | jq -r '.immature.sompi // 0')"

  if [ "$immature_count" -gt 0 ] 2>/dev/null; then
    verdict="RECENT_REWARD_SEEN"
  elif [ "$mature_count" -gt 0 ] 2>/dev/null; then
    verdict="REWARD_HISTORY_SEEN"
  else
    verdict="NO_REWARD_UTXO"
  fi

  # Display-only: total reward = mature + immature (already-fetched values).
  local total_sompi total_msk meaning
  total_sompi="$(awk -v a="$mature_sompi" -v b="$immature_sompi" 'BEGIN { printf "%.0f", a + b }')"
  total_msk="$(msk_from_sompi "$total_sompi")"
  meaning="$(reward_meaning "$verdict")"

  if [ "$DISCORD" -eq 1 ]; then
    printf 'MISAKA miner | Address:%s | Reward:%s (%s) | UTXO:%s | total:%sMSK | mature:%s/%sMSK | immature:%s/%sMSK\n' \
      "$(short_text "$address")" "$verdict" "$meaning" "$total" "$total_msk" "$mature_count" "$(msk_from_sompi "$mature_sompi")" "$immature_count" "$(msk_from_sompi "$immature_sompi")"
  else
    printf 'MISAKA miner address check\n'
    printf '%s\n' '=========================='
    printf '%-20s %s\n' 'Address' "$address"
    printf '%-20s %s\n' 'Total UTXOs' "$total"
    printf '%-20s %s (%s MSK)\n' 'Mature' "$mature_count" "$(msk_from_sompi "$mature_sompi")"
    printf '%-20s %s (%s MSK)\n' 'Immature' "$immature_count" "$(msk_from_sompi "$immature_sompi")"
    printf '%-20s %s MSK\n' 'Total reward' "$total_msk"
    printf '%-20s %s\n' 'Verdict' "$verdict"
    printf '%-20s %s\n' 'Meaning' "$meaning"
    printf '\n'
    printf 'Note: reward UTXOs can show mining reward history, but do not prove the miner process is running right now.\n'
  fi
}

run_real() {
  if [ -z "$ADDRESS" ]; then
    echo "--address <addr> is required." >&2
    exit 2
  fi
  if ! have "$MISAKA_BIN"; then
    echo "misaka binary not found: $MISAKA_BIN" >&2
    exit 2
  fi

  local json
  json="$("$MISAKA_BIN" --output json --network "$NETWORK" --rpc "$RPC" wallet utxo list --address "$ADDRESS" 2>&1)" || {
    echo "failed to query wallet utxo list:" >&2
    printf '%s\n' "$json" >&2
    exit 1
  }
  render "$json"
}

run_self_test() {
  render '{
    "ok": true,
    "address": "misakatest:qexampleaddress000000000000000000000000000000000000000000000000000000000000",
    "total": 4,
    "mature": {
      "count": 3,
      "sompi": 2500000000
    },
    "immature": {
      "count": 1,
      "sompi": 500000000
    }
  }'
}

if [ "$SELF_TEST" -eq 1 ]; then
  run_self_test
else
  run_real
fi
