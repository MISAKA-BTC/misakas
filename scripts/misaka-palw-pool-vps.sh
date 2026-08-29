#!/usr/bin/env bash
# misaka-palw-pool-vps.sh — stand up a PALW miner pool on a VPS, for testnet-11.
#
# A pool is a NODE that also listens for miners. It needs no key, no bond and no pay address of
# its own: each miner's template is built around the address that miner authenticated with, so the
# coinbase pays the miner directly and this host never holds a secret. What it does hold is the
# obligation a pooled miner cannot discharge — retaining each miner's execution material and
# gossiping it to the panel — which is why the pool is the node and not a sidecar.
#
#   ./misaka-palw-pool-vps.sh up        build if needed, start, and wait for the pool to listen
#   ./misaka-palw-pool-vps.sh status    fingerprint, sync, peers, and whether the pool port is open
#   ./misaka-palw-pool-vps.sh logs      follow the node log, pool lines highlighted
#   ./misaka-palw-pool-vps.sh down      stop it
#
# Environment (all optional):
#   BIN=<kaspad>          default ./target/release/kaspad
#   APPDIR=<dir>          default $HOME/.misaka-t11
#   POOL_LISTEN=<ip:port> default 0.0.0.0:26350   — what miners dial
#   MAX_MINERS=<n>        default 256
#   P2P=<ip:port>         default 0.0.0.0:26311
#   RPC=<ip:port>         default 127.0.0.1:26312  — LOOPBACK on purpose; see below
#
# **RPC stays on loopback.** `--unsaferpc` on a public interface is an unauthenticated door into a
# node holding a chain. The pool port is the only one that needs to be reachable, and it is
# authenticated by the bond handshake.
set -euo pipefail

BIN="${BIN:-$(cd "$(dirname "$0")/.." && pwd)/target/release/kaspad}"
APPDIR="${APPDIR:-$HOME/.misaka-t11}"
POOL_LISTEN="${POOL_LISTEN:-0.0.0.0:26350}"
MAX_MINERS="${MAX_MINERS:-256}"
P2P="${P2P:-0.0.0.0:26311}"
RPC="${RPC:-127.0.0.1:26312}"
LOG="$APPDIR/kaspad.log"
PIDFILE="$APPDIR/kaspad.pid"

# The identity this script expects to see in the log. If the binary prints a different one, it is
# a different ruleset and the pool would be serving jobs for a network its miners are not on.
FINGERPRINT="15bab795442ec3efc3a58e02dd9c7a6f3015ff0634bc4a50a7af589338857ad0"
# Fallback entry nodes, for a host where DNS seeding is blocked.
PEERS="${PEERS:-169.58.232.113:26311 169.58.232.114:26311 169.58.39.220:26311}"

die() { echo "misaka-palw-pool-vps: $*" >&2; exit 1; }

pool_port() { echo "${POOL_LISTEN##*:}"; }

cmd_up() {
  [ -x "$BIN" ] || die "no kaspad at $BIN — build it with: cargo build --release -p kaspad --bin kaspad"
  if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
    echo "already running (pid $(cat "$PIDFILE"))"; exit 0
  fi
  mkdir -p "$APPDIR"
  local args=(--testnet --netsuffix=11 --appdir="$APPDIR"
              --listen="$P2P" --rpclisten="$RPC" --utxoindex
              --palw-pool-listen="$POOL_LISTEN" --palw-pool-max-miners="$MAX_MINERS")
  # DNS seeding is live; the explicit peers are belt and braces for a host that cannot resolve.
  for p in $PEERS; do args+=(--addpeer="$p"); done

  nohup "$BIN" "${args[@]}" >>"$LOG" 2>&1 &
  echo $! > "$PIDFILE"
  echo "starting kaspad (pid $(cat "$PIDFILE")), appdir=$APPDIR"

  # Wait for the two lines that say this is the right network and the pool is open. A pool that
  # came up on the wrong ruleset is worse than one that did not come up.
  local waited=0
  while [ "$waited" -lt 120 ]; do
    kill -0 "$(cat "$PIDFILE")" 2>/dev/null || { echo "node died on startup:"; tail -20 "$LOG"; exit 1; }
    if grep -q "\[palw-pool\] listening on" "$LOG" 2>/dev/null; then break; fi
    sleep 2; waited=$((waited + 2))
  done

  if ! grep -q "$FINGERPRINT" "$LOG" 2>/dev/null; then
    echo "WARNING: this node did NOT print testnet-11's fingerprint ($FINGERPRINT)." >&2
    echo "         It is on a different ruleset; miners pointed at it would work for another network." >&2
    grep -m1 "Consensus params fingerprint" "$LOG" >&2 || true
  fi
  grep -m1 "\[palw-pool\] listening on" "$LOG" || {
    echo "the pool did not open in 120 s — the last lines were:"; tail -20 "$LOG"; exit 1; }
  echo
  echo "Pool is up. Miners connect with:"
  echo "  misaka-palw-pool-miner --pool <this-host>:$(pool_port) \\"
  echo "      --bond <txid>:<index> --key miner-seed.hex --pay-address misakatest:..."
  echo
  echo "Open the pool port to miners (and ONLY the pool port and p2p):"
  echo "  ufw allow $(pool_port)/tcp comment 'palw pool'"
  echo "  ufw allow ${P2P##*:}/tcp comment 'palw p2p'"
}

cmd_status() {
  if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
    echo "node:        running (pid $(cat "$PIDFILE"))"
  else
    echo "node:        NOT running"; exit 1
  fi
  grep -m1 "Consensus params fingerprint" "$LOG" 2>/dev/null | sed 's/^/fingerprint: /' || echo "fingerprint: (not yet printed)"
  grep -m1 "\[palw-pool\] listening on" "$LOG" 2>/dev/null | sed 's/^/pool:        /' || echo "pool:        (not yet listening)"
  # **Is the port actually accepting?** Asked by connecting to it, not by parsing a tool's output:
  # bash's /dev/tcp needs nothing installed, and the first draft of this check reported
  # "NOT listening" on a host that simply had no `ss` — a false alarm that sends an operator
  # chasing a firewall problem that does not exist.
  local probe_host="${POOL_LISTEN%:*}"
  [ "$probe_host" = "0.0.0.0" ] && probe_host="127.0.0.1"
  if timeout 3 bash -c "echo > /dev/tcp/$probe_host/$(pool_port)" 2>/dev/null; then
    echo "port:        $(pool_port) is accepting connections on $probe_host"
  else
    echo "port:        $(pool_port) is NOT accepting on $probe_host"
  fi
  # Reachability from OUTSIDE this host is a different question, and this cannot answer it — a
  # bound port behind a closed firewall looks exactly like this. Miners are the test for that.
  if [ "${POOL_LISTEN%:*}" = "127.0.0.1" ]; then
    echo "             (bound to loopback — no remote miner can reach it; use 0.0.0.0 to serve)"
  fi
  echo "--- recent pool activity ---"
  grep "\[palw-pool\]" "$LOG" 2>/dev/null | tail -10 || echo "(none yet — no miner has connected)"
  echo "--- sync ---"
  tail -200 "$LOG" 2>/dev/null | grep -E "Accepted block|IBD|Processed .* headers|peers" | tail -5 || true
}

cmd_logs() { tail -f "$LOG"; }

cmd_down() {
  [ -f "$PIDFILE" ] || die "no pidfile at $PIDFILE"
  local pid; pid="$(cat "$PIDFILE")"
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 30); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
  kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null || true
  rm -f "$PIDFILE"
  echo "stopped"
}

case "${1:-}" in
  up) cmd_up ;;
  status) cmd_status ;;
  logs) cmd_logs ;;
  down) cmd_down ;;
  *) echo "usage: $0 {up|status|logs|down}" >&2; exit 1 ;;
esac
