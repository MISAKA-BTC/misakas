#!/usr/bin/env bash
# t12-drill-kit/drill-host.sh — the fleet half of the POST-LAUNCH R-core+ drill (ADR-0152 §8.3 item 3,
# D-1…D-10; §8.4's deferred drills). Runs on a drill host — a fleet host with NO public testnet-12
# node — and drives scripts/misaka-palw-t12-rcore-drill.sh (P2-12) on the SHIPPING binaries.
#
#   drill-host.sh guard                     read-only: may this host drill? (lib.sh drill_guard)
#   drill-host.sh stage-bins <dir>          copy <dir>/kaspad and <dir>/misaka into $DRILL_ROOT/bin, only
#                                           at the sha256 fleet.env pins for the release
#   drill-host.sh drill-genesis             SALT=<64 hex>: the drill's keyring (via the rcore script's
#                                           `keyring`) and its genesis, checked against public t12 and
#                                           every forbidden genesis; prints the DRILL_GENESES line the
#                                           operator adds to the deploy kit's fleet.env on the Mac
#   drill-host.sh run <rcore subcommand…>   guard + shipping-binary check, then the rcore drill script with
#                                           KASPAD_BIN / CLI_BIN / WORK_DIR set (node <seat>, steps,
#                                           step <k>, attest, snapshot, analyze …; see that script)
#   drill-host.sh self-test                 anywhere (the Mac included): bash -n of this kit and the rcore
#                                           script's own self-test — no node, no host touched
#
# ENV: T12_LAUNCHED=yes (required for everything but self-test), SALT (the drill's, out of band, the SAME
# on every drill host), DRILL_ROOT (/root/t12-drill), WORK_DIR, RCORE_SCRIPT, DEPLOY_KIT, and the rcore
# script's own PEERS / SEAT / P2P_BASE / RPC_BASE / EVM_RPC_BASE / PUBLIC_PEER / OLD_KASPAD_BIN.
set -euo pipefail
# shellcheck source=lib.sh
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"

cmd=${1:-help}
case "$cmd" in
guard)
    drill_guard ;;
stage-bins)
    src=${2:?usage: $0 stage-bins <dir holding the release kaspad and misaka>}
    drill_guard
    mkdir -p "$BIN_DIR"
    for b in kaspad misaka; do
        case $b in kaspad) want=$KASPAD_SHA256 ;; misaka) want=$MISAKA_SHA256 ;; esac
        got=$(sha256sum "$src/$b" | cut -d' ' -f1)
        [ "$got" = "$want" ] || die "$src/$b sha256 ${got:0:16}… is not the release's ${want:0:16}… (fleet.env) — the drill runs the binary that ships"
        install -m 0755 "$src/$b" "$BIN_DIR/$b.tmp.$$" && mv -f "$BIN_DIR/$b.tmp.$$" "$BIN_DIR/$b"
    done
    echo "${REV:-?}" > "$BIN_DIR/REV"
    assert_shipping_bins
    say "staged the release's kaspad and misaka in $BIN_DIR (rev ${REV:-?})" ;;
drill-genesis)
    [ -n "${SALT:-}" ] || die "SALT is required (the drill's 64-hex salt; \`$0 run new-salt\` draws one)"
    drill_guard
    assert_shipping_bins
    KASPAD_BIN="$BIN_DIR/kaspad" CLI_BIN="$BIN_DIR/misaka" WORK_DIR="$WORK_DIR" "$RCORE_SCRIPT" keyring
    g=$(assert_drill_genesis "$WORK_DIR/keyring/manifest.json")
    say "drill genesis ${g:0:16}… (public t12 ${EXPECT_GENESIS:0:16}…). On the Mac, add to the deploy kit's fleet.env BEFORE the drill starts:"
    echo "DRILL_GENESES+=\" $g\"" ;;
run)
    shift
    [ $# -ge 1 ] || die "usage: $0 run <rcore subcommand…>"
    drill_guard
    assert_shipping_bins
    if [ -f "$WORK_DIR/keyring/manifest.json" ]; then assert_drill_genesis "$WORK_DIR/keyring/manifest.json" >/dev/null; fi
    export KASPAD_BIN="$BIN_DIR/kaspad" CLI_BIN="$BIN_DIR/misaka" WORK_DIR
    exec "$RCORE_SCRIPT" "$@" ;;
self-test)
    for f in "$KIT_DIR/lib.sh" "$KIT_DIR/drill-host.sh" "$RCORE_SCRIPT"; do bash -n "$f" || die "bash -n $f"; done
    "$RCORE_SCRIPT" self-test
    python3 "$(dirname "$RCORE_SCRIPT")/misaka-palw-t12-rcore-analyze.py" --self-test
    say "self-test: the kit parses; the rcore step machinery and the analyzer's arithmetic pass (no node, no host)" ;;
help|-h|--help|*)
    sed -n '2,24p' "$0"; [ "$cmd" = help ] || [ "$cmd" = -h ] || [ "$cmd" = --help ] || exit 2 ;;
esac
