#!/usr/bin/env bash
# =============================================================================
# test-multihost-agent.sh — §5.4 acceptance test for the host-local node agent
#                           + remote.sh control plane.
#
#   usage:  ./test-multihost-agent.sh            # run the whole suite
#           ./test-multihost-agent.sh --help
#
# WHAT IT PROVES (the six §5.4 completion conditions), on ONE box, using the REAL
# agent / remote.sh code paths (no stubs of the control plane):
#
#   (1) the controller holds NO node PID file — the node's pid record lives under
#       the NODE HOST's data root (a second, isolated PALW_DATA_ROOT here),
#       written by the agent, never under the controller's data root.
#   (2) a node restart is performed BY THE HOST'S AGENT and actually changes the
#       pid/start-time (`restart --force` fails closed if it did not).
#   (3) the controller never RECEIVES a private seed — `generate-dns-key` keygens
#       host-local (0600 under the host's keys/) and returns PUBLIC lines only;
#       the seed value never crosses back and never lands in the controller root.
#   (4) remote logs/artifacts are pulled into a controller bundle (`collect` +
#       `collect-tar` over the agent), and the pulled bundle carries NO *.seed.
#   (5) `stop` via the agent reliably stops the host's node.
#   (6) an SSH host-key MISMATCH / missing pin FAILS CLOSED (remote.sh's pinned
#       known_hosts + StrictHostKeyChecking=yes; a real loopback ssh is used when
#       reachable, else the deterministic pin-check layer).
#
# ISOLATION: everything runs under a scratch base (PALW_TEST_BASE, default under
# the system temp dir) with a node A on HIGH ports (37xxx/36xxx) so it never
# touches a real warm chain on the default 26xxx/27xxx ports. The node it starts
# is a standalone BOOTSTRAP kaspad (no peer needed to reach wRPC readiness). A
# trap tears the test node down on every exit path.
#
# SCOPE: this validates the control plane (who owns pids/secrets, who
# restarts, remote collect/stop, host-key policy) — NOT a full funded two-host
# lifecycle (cross-host funding/bonding is a Phase-B concern, §12). The two data
# roots on one box faithfully model the controller/node-host separation for the
# six conditions above; a genuine second host is still required for a real
# two-host soak.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AGENT="$SCRIPT_DIR/palw-node-agent.sh"

case "${1:-}" in
    -h|--help|help)
        sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
        exit 0 ;;
    "") : ;;
    *) echo "unknown arg '$1' (this test takes no arguments; see --help)" >&2; exit 2 ;;
esac

# ---------------------------------------------------------------------------
# Scratch layout + isolated ports (kept well away from the default port map).
# ---------------------------------------------------------------------------
REPO_ROOT_DEFAULT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
REPO_ROOT="${REPO_ROOT:-$REPO_ROOT_DEFAULT}"
TEST_BASE="${PALW_TEST_BASE:-${TMPDIR:-/tmp}/palw-5.4-agent-test}"
CTRL_ROOT="$TEST_BASE/controller"
HOSTA_ROOT="$TEST_BASE/node-a-host"
CTRL_ENV="$TEST_BASE/controller.env"
HOSTA_ENV="$TEST_BASE/node-a-host.env"
BUNDLE_LABEL="mh54"

rm -rf "$TEST_BASE"
mkdir -p "$TEST_BASE" "$CTRL_ROOT" "$HOSTA_ROOT"

# Write a role's config: pre-export the overrides (they win over env.example's
# ${VAR:-default}), then source env.example for everything else.
_mk_env() {   # <out-file> <data-root>
    cat > "$1" <<EOF
export REPO_ROOT="$REPO_ROOT"
export PALW_DATA_ROOT="$2"
export A_P2P_PORT=36611 A_GRPC_PORT=36610 A_WRPC_PORT=37610
export B_P2P_PORT=36621 B_GRPC_PORT=36620 B_WRPC_PORT=37620
export GATE_TIMEOUT_SECS=60 GATE_POLL_SECS=1 STOP_TIMEOUT_SECS=8
export TICKET_MODE=skip
export PALW_ENABLE_ALGO4=0
. "$SCRIPT_DIR/env.example"
EOF
}
_mk_env "$CTRL_ENV"  "$CTRL_ROOT"
_mk_env "$HOSTA_ENV" "$HOSTA_ROOT"

# The agent, run as if ON node A's host (reads that host's own config). This is
# exactly what node_dispatch does remotely (the remote agent reads its host env).
agent_hosta() { PALW_ENV_FILE="$HOSTA_ENV" PALW_LOG_TAG=agent-hostA bash "$AGENT" "$@"; }

# ---------------------------------------------------------------------------
# Tiny assert harness.
# ---------------------------------------------------------------------------
PASS=0; FAIL=0; FAILED_NAMES=""
ok()   { PASS=$((PASS+1)); printf '  \033[32mPASS\033[0m  %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); FAILED_NAMES="$FAILED_NAMES; $1"; printf '  \033[31mFAIL\033[0m  %s\n' "$1"; }
note() { printf '  ----  %s\n' "$1"; }
sect() { printf '\n== %s ==\n' "$1"; }

_cleanup() {
    note "cleanup: stopping any test node A"
    agent_hosta stop a >/dev/null 2>&1 || true
    agent_hosta stop   >/dev/null 2>&1 || true
}
trap _cleanup EXIT INT TERM

echo "§5.4 multi-host agent acceptance test"
echo "  repo:        $REPO_ROOT"
echo "  test base:   $TEST_BASE"
echo "  ctrl root:   $CTRL_ROOT"
echo "  host-A root: $HOSTA_ROOT (node A on ports 36610/36611/37610)"

# ===========================================================================
sect "Condition 6 — SSH host-key mismatch / missing pin FAILS CLOSED"
# ---------------------------------------------------------------------------
# 6a: with A_SSH_HOST set but NO pinned known_hosts, remote_preflight_hostkey
#     must die (deterministic; no ssh needed).
if (
    set -euo pipefail
    export PALW_ENV_FILE="$CTRL_ENV"
    . "$SCRIPT_DIR/common.sh"; . "$SCRIPT_DIR/remote.sh"; load_env >/dev/null 2>&1
    export A_SSH_HOST="localhost" PALW_KNOWN_HOSTS="$TEST_BASE/nonexistent_known_hosts" PALW_SSH_TOFU=0
    remote_preflight_hostkey a
) >/dev/null 2>&1; then
    bad "6a missing host-key pin should fail closed (it did NOT)"
else
    ok  "6a missing host-key pin fails closed (remote_preflight_hostkey a dies)"
fi

# 6b: TOFU opt-in (PALW_SSH_TOFU=1) deliberately bypasses the pin requirement.
if (
    set -euo pipefail
    export PALW_ENV_FILE="$CTRL_ENV"
    . "$SCRIPT_DIR/common.sh"; . "$SCRIPT_DIR/remote.sh"; load_env >/dev/null 2>&1
    export A_SSH_HOST="localhost" PALW_KNOWN_HOSTS="$TEST_BASE/nonexistent_known_hosts" PALW_SSH_TOFU=1
    remote_preflight_hostkey a
) >/dev/null 2>&1; then
    ok  "6b PALW_SSH_TOFU=1 deliberately allows first-connect (opt-in relaxation)"
else
    bad "6b PALW_SSH_TOFU=1 should allow first-connect pinning (it did not)"
fi

# 6c: a WRONG pinned host key -> a real loopback ssh must REFUSE to connect
#     (StrictHostKeyChecking=yes vs the pin). Uses real ssh; the host-key check
#     precedes auth, so it works even without passwordless login. Skipped only if
#     ssh to localhost is entirely unreachable.
WRONG_KH="$TEST_BASE/wrong_known_hosts"
# a syntactically valid but WRONG ed25519 key for localhost.
printf 'localhost ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n' > "$WRONG_KH"
if command -v ssh >/dev/null 2>&1; then
    if (
        set -euo pipefail
        export PALW_ENV_FILE="$CTRL_ENV"
        . "$SCRIPT_DIR/common.sh"; . "$SCRIPT_DIR/remote.sh"; load_env >/dev/null 2>&1
        export A_SSH_HOST="localhost" PALW_KNOWN_HOSTS="$WRONG_KH" PALW_SSH_TOFU=0
        # node_dispatch must fail closed: either the pin lookup rejects, or ssh
        # refuses on the key mismatch. A success here would be the bug.
        node_dispatch a status a
    ) >/dev/null 2>&1; then
        bad "6c wrong pinned host key should make node_dispatch fail closed (it SUCCEEDED)"
    else
        ok  "6c wrong pinned host key -> node_dispatch fails closed (ssh refuses / pin rejects)"
    fi
else
    note "6c skipped: no ssh on PATH"
fi

# ===========================================================================
sect "Start node A (bootstrap) on the node-A host via its agent"
# ---------------------------------------------------------------------------
if agent_hosta start a bootstrap; then
    ok "agent started node A (bootstrap) on the host root"
    STARTED=1
else
    bad "agent failed to start node A (bootstrap) — remaining functional checks skipped"
    STARTED=0
fi

if [ "$STARTED" = "1" ]; then
    # -----------------------------------------------------------------------
    sect "Condition 1 — controller holds NO node PID file (it lives on the host)"
    # -----------------------------------------------------------------------
    if [ -f "$HOSTA_ROOT/node-a.pid" ]; then
        ok "node-a.pid exists under the NODE HOST root ($HOSTA_ROOT/node-a.pid)"
    else
        bad "node-a.pid missing under the node host root"
    fi
    if [ -f "$CTRL_ROOT/node-a.pid" ]; then
        bad "controller root unexpectedly holds a node-a.pid ($CTRL_ROOT/node-a.pid)"
    else
        ok "controller root holds NO node-a.pid (the controller owns no node process)"
    fi

    # -----------------------------------------------------------------------
    sect "Condition 2 — restart is performed by the agent and CHANGES the pid"
    # -----------------------------------------------------------------------
    PID1="$(cut -f1 "$HOSTA_ROOT/node-a.pid" 2>/dev/null || true)"
    if agent_hosta restart a bootstrap --force; then
        PID2="$(cut -f1 "$HOSTA_ROOT/node-a.pid" 2>/dev/null || true)"
        if [ -n "$PID1" ] && [ -n "$PID2" ] && [ "$PID1" != "$PID2" ]; then
            ok "restart --force changed the pid ($PID1 -> $PID2)"
        else
            bad "restart --force did not change the pid ($PID1 -> $PID2)"
        fi
    else
        bad "agent restart --force returned non-zero"
    fi

    # -----------------------------------------------------------------------
    sect "Condition 3 — controller never receives the private seed"
    # -----------------------------------------------------------------------
    KEYOUT="$TEST_BASE/generate-dns-key.stdout"
    if agent_hosta generate-dns-key > "$KEYOUT" 2>/dev/null; then
        SEED="$HOSTA_ROOT/keys/dns-validator.seed"
        if [ -f "$SEED" ]; then
            ok "seed written host-local under the node host root ($SEED)"
            MODE="$(stat -f '%Lp' "$SEED" 2>/dev/null || stat -c '%a' "$SEED" 2>/dev/null)"
            [ "$MODE" = "600" ] && ok "seed file is 0600 (owner-only)" || bad "seed file mode is $MODE, expected 600"
        else
            bad "seed file not created under the node host keys/"
        fi
        # The bytes that cross back to the controller = stdout. It must carry the
        # PUBLIC identity/address only, and NEVER a seed value / a seed-content line.
        if grep -qiE 'seed|secret|private|-----BEGIN' "$KEYOUT"; then
            # the seed_path line is allowed (it is a PATH, not the value); anything
            # else matching is a leak.
            LEAK="$(grep -iE 'seed|secret|private|-----BEGIN' "$KEYOUT" | grep -viE 'seed_path|NEVER returned|host-local' || true)"
            if [ -n "$LEAK" ]; then
                bad "generate-dns-key stdout leaked secret-looking content: $LEAK"
            else
                ok "generate-dns-key stdout carries only the seed PATH + public block (no seed value)"
            fi
        else
            ok "generate-dns-key stdout carries no seed/secret content"
        fi
        # The controller root must not hold the seed.
        if [ -f "$CTRL_ROOT/keys/dns-validator.seed" ]; then
            bad "controller root unexpectedly holds a dns-validator.seed"
        else
            ok "controller root holds NO seed (never received it)"
        fi
    else
        bad "agent generate-dns-key returned non-zero"
    fi

    # -----------------------------------------------------------------------
    sect "Condition 4 — remote logs/artifacts pulled into a controller bundle"
    # -----------------------------------------------------------------------
    # Model the controller pulling the host bundle: agent collect (on the host),
    # then agent collect-tar streamed into the CONTROLLER root, extracted there.
    CTRL_BUNDLE="$CTRL_ROOT/artifacts/pulled-node-a"
    mkdir -p "$CTRL_BUNDLE"
    if agent_hosta collect "agent-collect-$BUNDLE_LABEL" >/dev/null 2>"$TEST_BASE/collect.log"; then
        if agent_hosta collect-tar "agent-collect-$BUNDLE_LABEL" > "$CTRL_BUNDLE/bundle.tar" 2>>"$TEST_BASE/collect.log"; then
            ( cd "$CTRL_BUNDLE" && tar -xf bundle.tar && rm -f bundle.tar )
            if ls "$CTRL_BUNDLE"/node-a*.log >/dev/null 2>&1; then
                ok "pulled the node host's node-a log into the controller bundle"
            else
                bad "pulled bundle has no node-a log ($(ls "$CTRL_BUNDLE" 2>/dev/null | tr '\n' ' '))"
            fi
            if [ -f "$CTRL_BUNDLE/host-status.txt" ]; then
                ok "pulled bundle carries host-status.txt (argv/disk facts)"
            else
                bad "pulled bundle missing host-status.txt"
            fi
            # MATERIALISE, never gate on a pipeline: under `set -o pipefail` a
            # `find ... | grep -q .` returns 141 (SIGPIPE) precisely WHEN a match is
            # found — grep -q exits on the first line and kills find — so this check
            # would report "secret-safe" exactly when a *.seed WAS present. Verified
            # on bash 3.2.57 (rc=141, 3/3 runs). Same fix as collect-artifacts.sh.
            _seed_hits="$(find "$CTRL_BUNDLE" -type f -name '*.seed' -print 2>/dev/null || true)"
            if [ -n "$_seed_hits" ]; then
                bad "pulled bundle CONTAINS a *.seed (secret leak)"
            else
                ok "pulled bundle contains NO *.seed (secret-safe)"
            fi
        else
            bad "collect-tar stream failed"
        fi
    else
        bad "agent collect failed"
    fi

    # -----------------------------------------------------------------------
    sect "Condition 5 — stop via the agent reliably stops the host's node"
    # -----------------------------------------------------------------------
    if agent_hosta stop a; then
        if [ -f "$HOSTA_ROOT/node-a.pid" ] && kill -0 "$(cut -f1 "$HOSTA_ROOT/node-a.pid")" 2>/dev/null; then
            bad "node A still alive after agent stop"
        else
            ok "agent stop terminated node A (no live pid record)"
        fi
    else
        bad "agent stop returned non-zero"
    fi
fi

# ===========================================================================
sect "Summary"
echo "  PASS: $PASS    FAIL: $FAIL"
if [ "$FAIL" -ne 0 ]; then
    echo "  failing:$FAILED_NAMES"
    exit 1
fi
echo "  all §5.4 control-plane conditions PASSED on this box."
exit 0
