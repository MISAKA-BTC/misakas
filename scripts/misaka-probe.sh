#!/usr/bin/env bash
# Read-only MISAKA node/DNS/validator probe.
#
# This script is intentionally a VPS-side PoC. It does not mutate node state.
# It is suitable for trying the operator UX before porting it into `misaka node probe`.

set -uo pipefail

NETWORK="${MISAKA_NETWORK:-testnet-10}"
RPC="${MISAKA_RPC:-127.0.0.1:27210}"
SEED="${MISAKA_PROBE_SEED:-seeder2.misakascan.com}"
P2P_PORT="${MISAKA_PROBE_P2P_PORT:-26211}"
DNS_PORT="${MISAKA_PROBE_DNS_PORT:-53}"
TIMEOUT="${MISAKA_PROBE_TIMEOUT:-3}"
TARGET_IP=""
STAKE_BOND=""
SKIP_LOCAL=0

fail_count=0
warn_count=0

usage() {
  cat <<'EOF'
Usage:
  misaka-probe.sh --ip <ipv4> [options]

Options:
  --ip <ipv4>              Target public IPv4. If omitted, tries api.ipify.org.
  --network <id>           Network id. Default: testnet-10.
  --rpc <host:port>        Local node wRPC Borsh endpoint. Default: 127.0.0.1:27210.
  --seed <domain>          Seed domain to resolve. Default: seeder2.misakascan.com.
  --p2p-port <port>        P2P port. Default: 26211.
  --dns-port <port>        DNS seeder port. Default: 53.
  --stake-bond <txid:n>    Optional bond outpoint for validator registry check.
  --skip-local             Skip systemctl / local doctor / ss checks.
  --timeout <seconds>      Network timeout. Default: 3.
  -h, --help               Show this help.

Examples:
  misaka-probe.sh --ip 217.76.57.217
  misaka-probe.sh --ip 217.76.57.217 --stake-bond <txid>:0
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --ip)
      TARGET_IP="${2:-}"
      shift 2
      ;;
    --network|--network-id)
      NETWORK="${2:-}"
      shift 2
      ;;
    --rpc|--node-wrpc-borsh|--node-rpc)
      RPC="${2:-}"
      shift 2
      ;;
    --seed)
      SEED="${2:-}"
      shift 2
      ;;
    --p2p-port)
      P2P_PORT="${2:-}"
      shift 2
      ;;
    --dns-port)
      DNS_PORT="${2:-}"
      shift 2
      ;;
    --stake-bond)
      STAKE_BOND="${2:-}"
      shift 2
      ;;
    --timeout)
      TIMEOUT="${2:-}"
      shift 2
      ;;
    --skip-local)
      SKIP_LOCAL=1
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

status_line() {
  # status_line "Label" "Value" "OK|WARN|FAIL|INFO"
  printf '%-30s  %-48s  %s\n' "$1" "$2" "$3"
}

ok() {
  status_line "$1" "$2" "OK"
}

info() {
  status_line "$1" "$2" "INFO"
}

warn() {
  warn_count=$((warn_count + 1))
  status_line "$1" "$2" "WARN"
}

fail() {
  fail_count=$((fail_count + 1))
  status_line "$1" "$2" "FAIL"
}

have() {
  command -v "$1" >/dev/null 2>&1
}

valid_ipv4() {
  case "$1" in
    ""|*[!0-9.]*|*.*.*.*.*|.*|*.) return 1 ;;
  esac
  IFS=. read -r a b c d extra <<EOF
$1
EOF
  [ -z "${extra:-}" ] || return 1
  for x in "$a" "$b" "$c" "$d"; do
    [ -n "$x" ] || return 1
    [ "$x" -ge 0 ] 2>/dev/null && [ "$x" -le 255 ] || return 1
  done
  return 0
}

if [ -z "$TARGET_IP" ]; then
  if have curl; then
    TARGET_IP="$(curl -4fsSL --max-time "$TIMEOUT" https://api.ipify.org 2>/dev/null || true)"
  fi
fi

if ! valid_ipv4 "$TARGET_IP"; then
  echo "target IP is required and must be IPv4. Pass --ip <ipv4>." >&2
  exit 2
fi

printf 'MISAKA probe\n'
printf '%s\n' '============'
status_line "Target IP" "$TARGET_IP" "INFO"
status_line "Network" "$NETWORK" "INFO"
status_line "Local wRPC Borsh" "$RPC" "INFO"
status_line "Seed domain" "$SEED" "INFO"
printf '\n'

p2p_ok=0
seed_has_ip=0
dns_udp_ok=0
dns_tcp_ok=0
validator_ok=0

printf 'External checks\n'
printf '%s\n' '---------------'

if have nc; then
  if nc -vz -w "$TIMEOUT" "$TARGET_IP" "$P2P_PORT" >/dev/null 2>&1; then
    p2p_ok=1
    ok "P2P ${P2P_PORT}/tcp" "reachable"
  else
    fail "P2P ${P2P_PORT}/tcp" "not reachable"
  fi
else
  warn "P2P ${P2P_PORT}/tcp" "nc not installed; skipped"
fi

if have dig; then
  seed_answers="$(dig +time="$TIMEOUT" +tries=1 "$SEED" A +short 2>/dev/null || true)"
  seed_count="$(printf '%s\n' "$seed_answers" | sed '/^$/d' | wc -l | tr -d ' ')"
  if printf '%s\n' "$seed_answers" | grep -Fxq "$TARGET_IP"; then
    seed_has_ip=1
    ok "Seed contains IP" "yes (${seed_count} A records)"
  elif [ "$seed_count" -gt 0 ]; then
    warn "Seed contains IP" "no (${seed_count} A records)"
  else
    warn "Seed contains IP" "no A records returned"
  fi

  udp_answers="$(dig +time="$TIMEOUT" +tries=1 @"$TARGET_IP" -p "$DNS_PORT" "$SEED" A +short 2>/dev/null || true)"
  udp_count="$(printf '%s\n' "$udp_answers" | sed '/^$/d' | wc -l | tr -d ' ')"
  if [ "$udp_count" -ge 2 ]; then
    dns_udp_ok=1
    ok "DNS seeder UDP ${DNS_PORT}" "answers ${udp_count} A records"
  elif [ "$udp_count" -eq 1 ]; then
    warn "DNS seeder UDP ${DNS_PORT}" "answers 1 A record"
  else
    info "DNS seeder UDP ${DNS_PORT}" "no A response; OK for non-seeder nodes"
  fi

  tcp_answers="$(dig +tcp +time="$TIMEOUT" +tries=1 @"$TARGET_IP" -p "$DNS_PORT" "$SEED" A +short 2>/dev/null || true)"
  tcp_count="$(printf '%s\n' "$tcp_answers" | sed '/^$/d' | wc -l | tr -d ' ')"
  if [ "$tcp_count" -ge 2 ]; then
    dns_tcp_ok=1
    ok "DNS seeder TCP ${DNS_PORT}" "answers ${tcp_count} A records"
  elif [ "$tcp_count" -eq 1 ]; then
    warn "DNS seeder TCP ${DNS_PORT}" "answers 1 A record"
  else
    info "DNS seeder TCP ${DNS_PORT}" "no A response; OK for non-seeder nodes"
  fi
else
  warn "DNS checks" "dig not installed; skipped"
fi

printf '\n'
printf 'Local VPS checks\n'
printf '%s\n' '----------------'

if [ "$SKIP_LOCAL" -eq 1 ]; then
  info "Local checks" "skipped"
else
  if have systemctl; then
    if systemctl is-active --quiet misaka-kaspad 2>/dev/null; then
      ok "systemd misaka-kaspad" "active"
    else
      warn "systemd misaka-kaspad" "not active or not installed"
    fi

    if systemctl is-active --quiet misaka-dnsseeder 2>/dev/null; then
      ok "systemd misaka-dnsseeder" "active"
    else
      info "systemd misaka-dnsseeder" "not active or not installed"
    fi

    if systemctl list-unit-files misaka-validator.service >/dev/null 2>&1; then
      if systemctl is-active --quiet misaka-validator 2>/dev/null; then
        ok "systemd misaka-validator" "active"
      else
        warn "systemd misaka-validator" "installed but not active"
      fi
    else
      info "systemd misaka-validator" "not installed"
    fi
  else
    info "systemd checks" "systemctl not available"
  fi

  if have ss; then
    if ss -tn 2>/dev/null | grep -F "${TARGET_IP}:" >/dev/null 2>&1; then
      ok "Active TCP to target" "seen in ss output"
    else
      info "Active TCP to target" "not currently connected from this host"
    fi
  else
    info "Active TCP to target" "ss not available"
  fi

  if have misaka; then
    if [ "$(id -u)" = "0" ] && id misahiro >/dev/null 2>&1 && have sudo; then
      doctor_output="$(sudo -u misahiro HOME=/var/lib/misaka misaka --network "$NETWORK" --rpc "$RPC" node doctor 2>&1 || true)"
    else
      doctor_output="$(misaka --network "$NETWORK" --rpc "$RPC" node doctor 2>&1 || true)"
    fi
    if printf '%s\n' "$doctor_output" | grep -q "doctor: OK"; then
      ok "Local node doctor" "doctor: OK"
    elif printf '%s\n' "$doctor_output" | grep -q "Synced[[:space:]]*true"; then
      warn "Local node doctor" "synced but doctor did not return OK"
    else
      warn "Local node doctor" "not OK or unreachable"
    fi
  else
    info "Local node doctor" "misaka binary not available"
  fi
fi

printf '\n'
printf 'Validator registry check\n'
printf '%s\n' '------------------------'

if [ -n "$STAKE_BOND" ]; then
  if have kaspa-pq-validator; then
    validator_output="$(kaspa-pq-validator status --node-rpc "$RPC" --stake-bond "$STAKE_BOND" --network "$NETWORK" 2>&1 || true)"
    if printf '%s\n' "$validator_output" | grep -q "bond_status:[[:space:]]*active"; then
      validator_ok=1
      ok "Stake bond" "active"
      vid="$(printf '%s\n' "$validator_output" | awk '/validator_id:/ {print $2; exit}')"
      [ -n "$vid" ] && info "Validator ID" "$vid"
    elif printf '%s\n' "$validator_output" | grep -q "bond_status:"; then
      bond_status="$(printf '%s\n' "$validator_output" | awk '/bond_status:/ {print $2; exit}')"
      warn "Stake bond" "${bond_status:-not active}"
    elif printf '%s\n' "$validator_output" | grep -q "not found"; then
      fail "Stake bond" "not found in registry"
    else
      warn "Stake bond" "query failed"
    fi
  else
    warn "Stake bond" "kaspa-pq-validator not installed; skipped"
  fi
else
  info "Stake bond" "not provided; validator status is UNKNOWN from IP alone"
fi

printf '\n'
printf 'Verdict\n'
printf '%s\n' '-------'

if [ "$p2p_ok" -eq 1 ] && [ "$seed_has_ip" -eq 1 ]; then
  ok "Node verdict" "NODE_OK: reachable and advertised by seed"
elif [ "$p2p_ok" -eq 1 ]; then
  warn "Node verdict" "NODE_REACHABLE: reachable but not seen in seed"
else
  fail "Node verdict" "NOT_REACHABLE: P2P check failed"
fi

if [ "$dns_udp_ok" -eq 1 ] && [ "$dns_tcp_ok" -eq 1 ]; then
  ok "DNS seeder verdict" "DNS_SEEDER_OK"
elif [ "$dns_udp_ok" -eq 1 ] || [ "$dns_tcp_ok" -eq 1 ]; then
  warn "DNS seeder verdict" "PARTIAL_DNS_SEEDER"
else
  info "DNS seeder verdict" "NOT_A_DNS_SEEDER_OR_NOT_PUBLIC"
fi

if [ -n "$STAKE_BOND" ]; then
  if [ "$validator_ok" -eq 1 ]; then
    ok "Validator verdict" "VALIDATOR_REGISTERED_ACTIVE"
  else
    warn "Validator verdict" "BOND_NOT_ACTIVE_OR_UNKNOWN"
  fi
else
  info "Validator verdict" "UNKNOWN: IP alone cannot prove validator participation"
fi

printf '\n'
printf 'Note: validator participation is identified by stake bond / validator_id, not by IP alone.\n'
printf '      This probe is read-only and does not change node, DNS, or validator state.\n'

if [ "$fail_count" -gt 0 ]; then
  exit 1
fi
exit 0
