#!/usr/bin/env bash
# =============================================================================
# stop.sh — trap-safe clean shutdown of the closed two-node PALW Phase-0 testnet
#           (STN-005: SIGTERM -> timeout -> SIGKILL for every supervised process).
#
#   usage:  ./stop.sh              # stop all supervised processes (default timeout)
#           ./stop.sh <seconds>    # override the per-process SIGTERM->SIGKILL grace
#           ./stop.sh --help
#
# WHAT IT DOES (and, honestly, what it does NOT):
#   It stops ONLY the three processes this harness supervises, via write_pid /
#   is_running records under $PALW_DATA_ROOT:
#       supporting-miner   (the continuous REAL algo-3 misaminer — STN-008)
#       node-a             (kaspad node A: bootstrap OR in-process validator)
#       node-b             (kaspad node B: the P2P peer)
#   Each is stopped with common.sh's stop_pid: SIGTERM, wait up to the grace
#   window, then SIGKILL, and the pidfile is removed. Shutdown order is the
#   reverse of the dependency order — the miner drives node A's gRPC pool, so it
#   is stopped first; then node A (which may host the in-process DNS/beacon
#   validator); then node B.
#
#   It DELETES NOTHING but the pidfiles. Chain data (node-a/ node-b/ appdirs),
#   logs, keys/*.seed, artifacts/state.env, and the discovered bond outpoints
#   recorded there are ALL LEFT INTACT so the net can be restarted in place.
#   This is NOT the seeded test-only `palw_demo` path — it never was; there is
#   nothing demo to unwind. It stops real daemons and returns.
#
# Design rules (shared with the whole harness; enforced here):
#   * IDEMPOTENT   — stop_pid is a no-op on an already-stopped or absent record
#                    (it just removes any stale pidfile and returns 0). Running
#                    stop.sh twice, or on an already-down net, is safe and quiet.
#                    It creates no keys/outpoints/files and overwrites nothing.
#   * FAIL-CLOSED  — if a process is STILL alive after SIGKILL + grace, that name
#                    is recorded and, after attempting to stop every process, the
#                    script exits non-zero with an actionable message naming the
#                    survivor(s). It never reports a clean stop it did not achieve.
#   * TRAP-SAFE    — a register_cleanup trap fires on EXIT/INT/TERM. If the run is
#                    interrupted mid-shutdown (operator Ctrl-C), it reports which
#                    supervised processes are still up so a partial teardown is
#                    never silent. On a fully successful stop it finds none and
#                    stays quiet.
#
# All shared behaviour lives in common.sh and is CALLED, never reimplemented.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"
# shellcheck source=remote.sh
. "$SCRIPT_DIR/remote.sh"   # node_is_remote / node_dispatch — stop remote node hosts too

# Tag every log/warn/die line from this stage.
PALW_LOG_TAG="${PALW_LOG_TAG:-stop}"; export PALW_LOG_TAG

# Supervised-process names, in SHUTDOWN order (reverse of dependency order).
# These are the exact <name> values used by write_pid / is_running / stop_pid in
# supporting-miner.sh, node-a.sh, and node-b.sh — pidfiles live at
# $PALW_DATA_ROOT/<name>.pid. Kept as a space-separated list (bash-3.2 safe; no
# declare -A / arrays needed for a fixed, ordered set).
# "miner-supporting" is a safety-net alias: earlier builds mis-defaulted the
# supporting miner to that name; include it so a pre-existing orphan is always reaped.
STOP_ORDER="supporting-miner miner-supporting node-a node-b"

# ---------------------------------------------------------------------------
usage() {
    cat >&2 <<EOF
usage: ${0##*/} [seconds|--help]

  (no arg)    Stop every supervised process (supporting-miner, node-a, node-b)
              with SIGTERM -> wait -> SIGKILL, then remove each pidfile. Data
              dirs, logs, keys, and artifacts/state.env are left INTACT.
              Idempotent: safe to run twice or on an already-stopped net.
  <seconds>   Positive integer: override the per-process SIGTERM->SIGKILL grace
              window (default: \$STOP_TIMEOUT_SECS from the config).
  --help      Show this help and exit.
EOF
}

# _still_running_names — echo (space-separated) any names in STOP_ORDER whose
#   recorded process is still alive. Used by the shutdown loop's fail-closed
#   check and by the interrupt-safety trap below. Reads only pid records; it
#   never signals anything.
_still_running_names() {
    local n out=""
    for n in $STOP_ORDER; do
        if is_running "$n"; then out="$out $n"; fi
    done
    printf '%s\n' "${out# }"
}

# _stop_on_exit — cleanup snippet armed via register_cleanup. On an interrupted
#   or aborted run it names any supervised process still up so a partial teardown
#   is never silent. On a clean full stop it finds nothing and prints nothing.
#   (Best-effort: the trap handler runs it under set +e.)
_stop_on_exit() {
    local left
    left="$(_still_running_names)"
    if [ -n "$left" ]; then
        warn "shutdown did not fully complete — still running:$( printf ' %s' $left ). Re-run ${0##*/} (raise the grace window: ${0##*/} <seconds>), or inspect the pidfiles under $PALW_DATA_ROOT."
    fi
}

# ---------------------------------------------------------------------------
# Dispatch / arg validation BEFORE load_env so --help works unconfigured.
GRACE=""                                   # empty => stop_pid uses $STOP_TIMEOUT_SECS
case "${1:-}" in
    -h|--help|help) usage; exit 0 ;;
    "")             : ;;                   # no override; use the configured default
    *[!0-9]*)       usage; die "invalid grace-seconds argument '$1' (expected a positive integer or --help)" ;;
    0)              usage; die "grace-seconds must be a positive integer, got '0'" ;;
    *)              GRACE="$1" ;;          # validated all-digits, non-zero
esac
# Reject stray extra arguments fail-closed rather than silently ignore them.
if [ "$#" -gt 1 ]; then usage; die "unexpected extra argument(s): ${*:2}"; fi

# Load config + state.env, resolve REPO_ROOT / PALW_DATA_ROOT, verify binaries.
# We need PALW_DATA_ROOT (where the pidfiles live) and STOP_TIMEOUT_SECS; this is
# the same entry point every stage uses, so a stop always matches the start.
load_env

# Arm the interrupt-safety trap for the whole shutdown window. On EXIT/INT/TERM
# it reports any process still up (honest partial-teardown signal); on a clean
# full stop it stays quiet. It removes nothing and creates nothing.
register_cleanup '_stop_on_exit'

log "stopping supervised processes in order: $STOP_ORDER (grace=${GRACE:-\$STOP_TIMEOUT_SECS=$STOP_TIMEOUT_SECS}s each); data dirs / logs / keys / state.env left intact"

# ---------------------------------------------------------------------------
# Phase 1 (multi-host, §5.4 condition 5): tear down every REMOTE node host via
# its own agent. `node_dispatch <n> stop` runs the node host's OWN stop.sh over
# SSH, reaping that host's node + any co-located supporting miner. On a single
# host both nodes are LOCAL, so this loop no-ops and Phase 2 does the classic
# local teardown unchanged (no agent round-trip, so no self-recursion). The
# remote stop.sh is itself fail-closed, and node_dispatch propagates its exit
# code — a remote survivor is recorded and surfaced in the final verdict.
# ---------------------------------------------------------------------------
REMOTE_FAILED=""
STOPPED_HOSTS=""
for n in a b; do
    node_is_remote "$n" || continue
    host="$(node_ssh_host "$n")"
    case " $STOPPED_HOSTS " in *" $host "*) continue ;; esac   # host already torn down
    log "stopping ALL supervised processes on node-$n host ($host) via its agent"
    if node_dispatch "$n" stop; then
        STOPPED_HOSTS="$STOPPED_HOSTS $host"
    else
        REMOTE_FAILED="$REMOTE_FAILED node-$n@$host"
        warn "remote teardown of node-$n host ($host) reported failure — see the agent output above"
    fi
done

# ---------------------------------------------------------------------------
# Phase 2 — stop each LOCAL supervised process. stop_pid is idempotent (a
# stale/absent pidfile is just removed, returning 0) and PID-reuse safe
# (is_running re-checks argv + start-time). A node whose host is REMOTE was
# already stopped in Phase 1, so it is skipped here (its pid record lives on the
# node's host, never on the controller). We attempt EVERY local process even if
# one fails, then fail-closed at the end if any survived.
# ---------------------------------------------------------------------------
FAILED=""
for name in $STOP_ORDER; do
    case "$name" in
        node-a) if node_is_remote a; then log "node-a is remote ($(node_ssh_host a)); stopped via its agent in phase 1"; continue; fi ;;
        node-b) if node_is_remote b; then log "node-b is remote ($(node_ssh_host b)); stopped via its agent in phase 1"; continue; fi ;;
    esac
    if is_running "$name"; then
        log "stopping $name (pid $(read_pid "$name" 2>/dev/null || printf '?'))"
    else
        # Not running: either genuinely down, or only a stale pidfile remains.
        # stop_pid still cleans up a stale record; report the condition.
        if [ -f "$(pid_file "$name")" ]; then
            log "$name not running; removing stale pid record"
        else
            log "$name not running (no pid record); nothing to do"
        fi
    fi

    # stop_pid: SIGTERM -> wait up to grace -> SIGKILL; removes the pidfile.
    # Passing an empty GRACE lets stop_pid fall back to its own default; a
    # validated GRACE overrides it.
    if [ -n "$GRACE" ]; then
        stop_pid "$name" "$GRACE" || FAILED="$FAILED $name"
    else
        stop_pid "$name" || FAILED="$FAILED $name"
    fi
done

# ---------------------------------------------------------------------------
# Fail-closed verdict. Belt-and-suspenders: re-scan live state in case a name
# reported stopped but a same-named survivor lingers.
# ---------------------------------------------------------------------------
LEFT="$(_still_running_names)"
if [ -n "$FAILED" ] || [ -n "$LEFT" ] || [ -n "$REMOTE_FAILED" ]; then
    # Merge the local signals into a single, de-duplicated, actionable list.
    _bad=""
    for name in $FAILED $LEFT; do
        case " $_bad " in *" $name "*) : ;; *) _bad="$_bad $name" ;; esac
    done
    if [ -n "$REMOTE_FAILED" ]; then
        die "failed to stop:${_bad}${REMOTE_FAILED} — local survivor(s) are still alive after SIGKILL + grace (inspect the pidfiles under $PALW_DATA_ROOT); remote host(s) reported a teardown failure (see the agent output above and re-run ${0##*/})."
    fi
    die "failed to stop:$_bad — still alive after SIGKILL + grace. Inspect the pidfiles under $PALW_DATA_ROOT and stop the process(es) manually, then re-run ${0##*/}."
fi

log "all supervised processes stopped (local + any remote node hosts via their agents); pidfiles cleared. Chain data, logs, keys, and artifacts/state.env preserved — restart in place with node-b.sh / node-a.sh / supporting-miner.sh (or the agent's start verb per host)."
exit 0
