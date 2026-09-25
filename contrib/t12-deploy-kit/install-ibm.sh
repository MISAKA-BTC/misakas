#!/bin/bash
# deploy-t12/install-ibm.sh — ibm (169.58.39.220, hostname vmi3450148): bonds 0 and 1, both PUBLIC.
#
#   b0  misaka-t12-node0 (NEW unit)      0.0.0.0:26311  borsh 26313  json 26314   8k producer + 8k seat + HEARTBEAT
#   b1  misaka-t12-node1 (drop-in)       0.0.0.0:26321  borsh 26323  json 26324   floor producer + 8k seat
#
# b0 takes the ports the host's other services already point at and that nothing serves today:
#   misaka-dnsseeder-t12  --node-wrpc-borsh 127.0.0.1:26313   (dead since t11 node0 failed)
#   misaka-explorer-vantage-tunnel  .113:28014 -> here 127.0.0.1:26314  (misakascan /kaspa-hub; dead today)
#   misaka-faucet  FAUCET_RPC=127.0.0.1:26313  (a testnet-11 faucet — see PLAN.md §7)
# and the seeders dial the anchors on the default port 26311, which nothing on ibm answers today.
#
# Run as root on ibm from $REL_ROOT/kit:  ./install-ibm.sh preflight | stage | switch | check | rollback
. "$(dirname "$0")/lib.sh"

HOST_NAME_EXPECTED=vmi3450148
RESERVE_MIB=2048              # ollama, journald, seeder, faucet, tunnel: 0.3 GiB measured; the rest is margin
START_GAP=20
BINARIES=(kaspad misaka palw-class)
# id|unit|mode|listen|borsh|json|grpc|evm|share_mib|memmax_gib|produce|heartbeat|seat8k|peers
NODES=(
  "0|misaka-t12-node0|new|0.0.0.0:26311|26313|26314|-|-|7168|10|8k|1|1|127.0.0.1:26321,169.58.232.113:26311"
  "1|misaka-t12-node1|dropin|0.0.0.0:26321|26323|26324|-|-|6144|9|floor|0|1|127.0.0.1:26311,169.58.232.113:26311"
)
# old chain data: .t12 = the old node0 appdir (1.6 GB, unused since t11 node0 failed), .t12b = node1's
OLD_APPDIRS=(/root/.t12 /root/.t12b)

host_preflight() {
    local e
    e=$(systemctl is-enabled misaka-t11-node0 2>/dev/null || true)
    say "  misaka-t11-node0: $(systemctl show -p ActiveState --value misaka-t11-node0) / $e — its /root/t11/ibm-node0.sh binds 0.0.0.0:26311 and 26313 at boot"
    [ "$e" = enabled ] && warn "misaka-t11-node0 is ENABLED: after a reboot it races b0 for 26311/26313. switch needs DISABLE_T11_NODE0=1 (operator decision)"
    say "  misaka-faucet FAUCET_RPC → $(systemctl show -p Environment --value misaka-faucet | grep -oE 'FAUCET_RPC=[^ ]+' || echo '?') (b0 will answer it)"
    say "  vantage tunnel → $(grep -E '^(LOCAL|REMOTE)_RPC_PORT=' /etc/misaka-explorer/vantage-tunnel.env 2>/dev/null | tr '\n' ' ')(b0 json 26314 feeds misakascan /kaspa-hub)"
    local free_gb; free_gb=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
    [ "$free_gb" -ge 6 ] || warn "only ${free_gb} GB free on / (95 % used on 09-23)"
    ls -l /root/palw-class/qwen25-1.5b-a16-8k.palwart* 2>/dev/null | sed 's/^/  /' || true
}

host_guard_switch() {
    local e; e=$(systemctl is-enabled misaka-t11-node0 2>/dev/null || true)
    systemctl is-active --quiet misaka-t11-node0 && die "misaka-t11-node0 is RUNNING on 26311 — stop it first (operator decision)"
    if [ "$e" = enabled ]; then
        [ "${DISABLE_T11_NODE0:-0}" = 1 ] || die "misaka-t11-node0 is enabled and would take 26311/26313 at the next boot. Re-run with DISABLE_T11_NODE0=1 to disable it (its files stay)."
        systemctl disable misaka-t11-node0 && record_state "DISABLED misaka-t11-node0" && say "  disabled misaka-t11-node0 (unit and script left in place)"
    fi
    local free_gb; free_gb=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
    [ "$free_gb" -ge 5 ] || die "only ${free_gb} GB free on / — free space first (PLAN.md §7)"
}

host_usage() { echo "  (ibm) DISABLE_T11_NODE0=1 ./install-ibm.sh switch   # when misaka-t11-node0 is still enabled"; }
host_cmd() { usage_common; host_usage; die "unknown command $1"; }

main_dispatch "$@"
