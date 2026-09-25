# t12-drill-kit/lib.sh — sourced by drill-host.sh. Never executed.
#
# The POST-LAUNCH R-core+ drill (ADR-0152 §8.3 item 3's D-1…D-10 and §8.4's deferred drills), run with
# P2-12's tooling: the SHIPPING kaspad and misaka with `--palw-drill-genesis-salt`, driven by
# scripts/misaka-palw-t12-rcore-drill.sh and its analyzer. This file adds only what the fleet needs
# around that script:
#   * a host guard stricter than the script's own `check-host` — it also knows the deploy kit's units,
#     drop-ins, staged launch scripts, ports and app dirs (a public t12 host is refused however its node
#     was started, and whether or not it is running right now);
#   * the post-launch gate (the operator's decision of 2026-09-24: drills and measurements run AFTER
#     launch; this kit refuses to start one before public t12 is live and pinned);
#   * the shipping-binary rule (memory: "the drill must run the binary you are shipping"): kaspad and
#     misaka are accepted only at the sha256 fleet.env pins for the release.
# The previous kit (a drill-only commit 005e5c5f with a baked salt) is in legacy-005e5c5f/ for the
# record; it refuses to run on anything but that build.
set -euo pipefail

KIT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
DEPLOY_KIT="${DEPLOY_KIT:-$KIT_DIR/../t12-deploy-kit}"
DRILL_ROOT="${DRILL_ROOT:-/root/t12-drill}"
BIN_DIR="$DRILL_ROOT/bin"                                   # written by `stage-bins` only
WORK_DIR="${WORK_DIR:-$DRILL_ROOT/work}"                    # the rcore script's WORK_DIR (keyring, seat dirs, ledger)
# The rcore drill script and its analyzer, from the same checkout as this kit (scripts/ beside contrib/),
# or a copy of that scripts/ directory under DRILL_ROOT (the script finds its analyzer at ../scripts/).
RCORE_SCRIPT="${RCORE_SCRIPT:-$KIT_DIR/../../scripts/misaka-palw-t12-rcore-drill.sh}"

say()  { printf '[t12-drill-kit %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
die()  { say "REFUSED: $*"; exit 1; }

# ---- the release's identity: the deploy kit's fleet.env (the template when fleet.env is absent) ----
if [ -f "$DEPLOY_KIT/fleet.env" ]; then
    # shellcheck source=../t12-deploy-kit/fleet.env.example
    . "$DEPLOY_KIT/fleet.env"
    FLEET_ENV_SOURCE="$DEPLOY_KIT/fleet.env"
elif [ -f "$DEPLOY_KIT/fleet.env.example" ]; then
    # shellcheck source=../t12-deploy-kit/fleet.env.example
    . "$DEPLOY_KIT/fleet.env.example"
    FLEET_ENV_SOURCE="$DEPLOY_KIT/fleet.env.example"
else
    die "neither $DEPLOY_KIT/fleet.env nor fleet.env.example — the drill kit reads the release identity from the deploy kit"
fi

filled() { [ -n "${1:-}" ] && [ "$1" != __FILL_ME__ ]; }

# ---- what a public testnet-12 host looks like (the deploy kit's own footprint + the network defaults) ----
# Units: every t12 kaspad unit the fleet has ever run (node0/node1/node, seat2..7, the retired t12f-bN
# floor seats, the private t12p-N). The seeder unit (misaka-dnsseeder-t12) is not a node, but its host is
# refused anyway by the rcore script's check-host (its unit file names testnet-12 without a salt).
PUBLIC_UNIT_GLOBS="misaka-t12-node* misaka-t12-seat* misaka-t12f-* misaka-t12p-*"
# Ports: the deploy kit's P2P/gRPC/wRPC/EVM ports (install-*.sh NODES) and testnet's defaults.
PUBLIC_T12_PORTS="26311 26312 26313 26314 26321 26323 26324 26331 26333 26334 26341 26343 26344 26351 26353 26354 26210 27210 28210 8545"
DEPLOY_REL_ROOT="${REL_ROOT:-/root/t12-rel}"
DEPLOY_APPDIR_GLOB="${APPDIR_PREFIX:-/root/.t12r-b}*"

port_listening() {
    { command -v ss >/dev/null 2>&1 && ss -ltn "sport = :$1" 2>/dev/null | grep -q LISTEN; } \
        || { command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }
}

# drill_guard — refuse this host unless it is a fleet host with no public testnet-12 node on it, and
# public t12 is already live. Read-only.
drill_guard() {
    [ "$(uname -s)" != Darwin ] || die "never on the operator's Mac (or any macOS host): the drill runs on fleet hosts only (ADR-0152 §8.3 item 3)"
    [ "${T12_LAUNCHED:-}" = yes ] \
        || die "drills run AFTER the public launch (operator decision 2026-09-24; ADR-0152 §8.3 item 3, §8.4). Set T12_LAUNCHED=yes once public testnet-12 is live"
    filled "${EXPECT_GENESIS:-}" && filled "${EXPECT_FP:-}" && filled "${KASPAD_SHA256:-}" && filled "${MISAKA_SHA256:-}" \
        || die "$FLEET_ENV_SOURCE does not pin the launched release (EXPECT_GENESIS / EXPECT_FP / KASPAD_SHA256 / MISAKA_SHA256): the drill drills THAT binary and refuses THAT genesis"
    local u units f
    if command -v systemctl >/dev/null 2>&1; then
        # shellcheck disable=SC2086
        units=$(systemctl list-units --all --type=service --no-legend --plain $PUBLIC_UNIT_GLOBS 2>/dev/null | awk '{print $1}' || true)
        for u in $units; do
            if systemctl is-active --quiet "$u" || [ "$(systemctl is-enabled "$u" 2>/dev/null || true)" = enabled ]; then
                die "this host has the public testnet-12 unit $u ($(systemctl is-active "$u" 2>/dev/null || true)/$(systemctl is-enabled "$u" 2>/dev/null || true)) — never drill beside one"
            fi
        done
        for f in /etc/systemd/system/*.service.d/zz-t12-regenesis.conf; do
            [ -e "$f" ] && die "this host carries the deploy kit's drop-in $f — it is (or was staged as) a public testnet-12 host"
        done
    fi
    for f in "$DEPLOY_REL_ROOT"/*/launch/b*.sh; do
        [ -e "$f" ] && die "this host has a staged public launch script $f (deploy kit) — a public testnet-12 host"
    done
    for f in $DEPLOY_APPDIR_GLOB; do
        [ -e "$f" ] && die "this host has a public node's app dir $f (deploy kit) — a public testnet-12 host"
    done
    local line
    while IFS= read -r line; do
        [ -z "$line" ] && continue
        case "$line" in
            *--palw-drill-genesis-salt*) ;;                               # another drill node — allowed
            *) die "this host runs a public testnet-12 kaspad: ${line:0:160}" ;;
        esac
    done < <({ pgrep -af kaspad 2>/dev/null || true; } | grep -E -- '--netsuffix(=| )12|--configfile|(^| )-C ' || true)
    local port
    for port in $PUBLIC_T12_PORTS; do
        port_listening "$port" && die "port $port (a public testnet-12 port of the deploy kit or a testnet default) is listening here"
    done
    [ -x "$RCORE_SCRIPT" ] || die "no drill script at $RCORE_SCRIPT (RCORE_SCRIPT=…; copy the checkout's scripts/ directory beside this kit)"
    "$RCORE_SCRIPT" check-host
    say "guard: not macOS, public t12 is live and pinned (${EXPECT_GENESIS:0:16}…), no public t12 unit, drop-in, launch script, app dir, process or port on this host"
}

# assert_shipping_bins — BIN_DIR holds exactly the release's kaspad and misaka, and both know the salt.
assert_shipping_bins() {
    local b want got
    for b in kaspad misaka; do
        [ -x "$BIN_DIR/$b" ] || die "no $BIN_DIR/$b — run \`drill-host.sh stage-bins <dir with the release's kaspad and misaka>\`"
        case $b in kaspad) want=$KASPAD_SHA256 ;; misaka) want=$MISAKA_SHA256 ;; esac
        got=$(sha256sum "$BIN_DIR/$b" | cut -d' ' -f1)
        [ "$got" = "$want" ] || die "$BIN_DIR/$b sha256 ${got:0:16}… is not the release's ${want:0:16}… — the drill runs the binary that ships, nothing else"
        "$BIN_DIR/$b" --help 2>/dev/null | grep -q -- '--palw-drill-genesis-salt' \
            || die "$BIN_DIR/$b has no --palw-drill-genesis-salt (P2-12): not an R-core+ build"
    done
}

# assert_drill_genesis <manifest.json> — the keyring the salted SHIPPING binary wrote names a drill genesis
# that is not public t12's (EXPECT_GENESIS) and not any forbidden one, and names public t12's genesis as
# EXPECT_GENESIS (the binary is the launched one). Prints the drill genesis.
assert_drill_genesis() {
    local m=$1 drill public f
    drill=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["genesis_hash"])' "$m")
    public=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["public_genesis_hash"])' "$m")
    [ "$public" = "$EXPECT_GENESIS" ] || die "the keyring names public t12 genesis ${public:0:16}…, fleet.env pins ${EXPECT_GENESIS:0:16}… — not the launched release"
    [ "$drill" != "$EXPECT_GENESIS" ] || die "the drill genesis IS public t12's — the salt did not move it"
    for f in $FORBIDDEN_GENESIS; do [ "$drill" != "$f" ] || die "the drill genesis ${drill:0:16}… is a forbidden (retired/private) genesis"; done
    echo "$drill"
}
