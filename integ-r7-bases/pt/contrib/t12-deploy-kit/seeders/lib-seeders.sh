#!/usr/bin/env bash
# deploy-t12/seeders/lib-seeders.sh — sourced by every seeder script. Mac-side only.
#
# The seeder inventory below was MEASURED read-only on 2026-09-23 ~15:28 CEST (see ../SEEDERS.md).
# Row order is the swap order: least blast radius first (not delegated, no public node) →
# delegated, no node → public-node host, not delegated → delegated AND public-node host.
set -euo pipefail

SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KIT_DIR="$(dirname "$SEED_DIR")"
# fleet.env belongs to the node-switch kit; we only READ it (SEEDER_SHA256, EXPECT_FP, ...).
# shellcheck disable=SC1091
[ -f "$KIT_DIR/fleet.env" ] && source "$KIT_DIR/fleet.env"

SSH_OPTS=(-o IdentitiesOnly=yes -i "$HOME/.ssh/claude_key" -o ConnectTimeout=10 -o BatchMode=yes)
UNIT=misaka-dnsseeder-t12
P2P_PORT=26311
# The binary running on all four hosts since 2026-09-22 21:59 CEST (built at the first t12 deploy).
LIVE_SEEDER_SHA256=1174b96503895aec6c935934723121814d356872e7c040f7202b6c57e82e7497
LIVE_ANCHORS=169.58.232.113,169.58.39.220

#        name     ssh target           listen ip        binary path                           style config file                                       delegated name           public node on host
SEEDER_TABLE='
c5104    root@5.104.81.23     5.104.81.23      /usr/local/bin/misaka-dnsseeder-t12   env   /etc/default/misaka-dnsseeder-t12                 -                        no
seeder3  root@95.111.236.186  95.111.236.186   /root/misaka-dnsseeder-t12            unit  /etc/systemd/system/misaka-dnsseeder-t12.service  seeder3.misakascan.com   no
ibm      root@169.58.39.220   169.58.39.220    /usr/local/bin/misaka-dnsseeder-t12   env   /etc/default/misaka-dnsseeder-t12                 -                        yes
seeder1  root@169.58.232.113  169.58.232.113   /usr/local/bin/misaka-dnsseeder-t12   unit  /etc/systemd/system/misaka-dnsseeder-t12.service  seeder1.misakascan.com   yes
'

seeder_names() { awk 'NF{print $1}' <<<"$SEEDER_TABLE"; }
seeder_row() { awk -v n="$1" '$1==n' <<<"$SEEDER_TABLE"; }

load_seeder() {
  local row
  row="$(seeder_row "${1:-}")"
  if [ -z "$row" ]; then
    echo "unknown seeder '${1:-}' (one of: $(seeder_names | tr '\n' ' '))" >&2
    exit 2
  fi
  # shellcheck disable=SC2034
  read -r S_NAME S_SSH S_IP S_BIN S_STYLE S_CONF S_DNS S_PUBLIC <<<"$row"
}

rsh() { ssh "${SSH_OPTS[@]}" "$S_SSH" "$@"; }

need_confirm() {
  if [ "${CONFIRM:-}" != yes ]; then
    echo "refusing: this step WRITES on $S_SSH ($S_NAME). Re-run with CONFIRM=yes after reading ../SEEDERS.md." >&2
    exit 3
  fi
}

valid_anchors() { [[ "$1" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}(,([0-9]{1,3}\.){3}[0-9]{1,3})*$ ]]; }

# The sha a seeder is EXPECTED to run: the release's if fleet.env names one, else the live one.
expected_seeder_sha() {
  if [ -n "${SEEDER_SHA256:-}" ] && [ "${SEEDER_SHA256}" != KEEP ] && [ "${SEEDER_SHA256}" != __FILL_ME__ ]; then
    echo "$SEEDER_SHA256"
  else
    echo "$LIVE_SEEDER_SHA256"
  fi
}
