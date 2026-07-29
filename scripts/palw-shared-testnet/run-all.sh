#!/usr/bin/env bash
# =============================================================================
# run-all.sh — ONE-COMMAND orchestrator for the PALW closed two-node testnet
#              harness (Phase-0 wiring). It CHAINS the per-stage scripts in
#              dependency order, gating each one, and prints a final
#              PASS / PARTIAL summary of what was actually reached.
#
#   usage:  ./run-all.sh            # full bring-up (default action "all")
#           ./run-all.sh <step>     # run exactly ONE stage (individually re-runnable)
#           ./run-all.sh list       # print the ordered stage plan and exit
#           ./run-all.sh --help
#
# SCOPE:
#   A thin sequencer. Every stage's real work — starting kaspad, mining, keygen,
#   bonding, the batch lifecycle, verification — lives in its OWN sibling script
#   (node-a.sh, bootstrap-funds.sh, register-providers.sh, ...). run-all.sh only
#   decides the ORDER, runs each stage as a fail-closed gate, and summarises.
#   It reimplements NOTHING from common.sh and drives ONLY the real stage scripts:
#   it never invokes the seeded, test-only palw_demo path and it mints no block
#   itself (a mint, if any, is done by start-palw-miner.sh under TICKET_MODE=mock).
#
# TICKET_MODE (env; default 'skip'):
#   skip : the pipeline reaches batch.status=active but can NEVER mint an algo-4
#          block (the leaf-chunk is registered with --unsafe-skip-ticket-secret-
#          check; no ticket). That is the honest Phase-0 end state -> PARTIAL.
#   mock : additionally runs start-palw-miner.sh to mint a WIRING-ONLY, non-
#          inference block (needs the mock-ticket helper (built by build-and-hash.sh);
#          see README §Scope). A verified mint -> PASS. Real inference needs the
#          provider GPU tool and is out of scope here.
#
# STAGE ORDER (and WHY it differs from a naive reading of the spec arrows):
#   The nominal step list is
#     preflight -> build-and-hash -> node-b -> node-a -> supporting-miner ->
#     bootstrap-funds -> dns-validator -> register-providers -> create-lifecycle
#     -> submit-lifecycle -> (mock) start-palw-miner -> verify-consensus ->
#     verify-coinbase -> collect-artifacts
#   Three ordering constraints are handled here because the stage scripts'
#   fail-closed preconditions demand it (documented, not silently reshuffled):
#     1. build-and-hash BEFORE preflight. preflight.sh calls load_env, which
#        fail-closes when the release binaries are absent; build-and-hash.sh is
#        the ONE stage that bootstraps REPO_ROOT without them (it PRODUCES them).
#        On a warm tree cargo is incremental (cheap + idempotent); pass
#        BUILD_SKIP=1 to skip the cargo invocation when the binaries already
#        exist, or start from `./run-all.sh preflight` to bypass building.
#     2. node-a (bootstrap block source) BEFORE node-b. node-b.sh fail-closes on
#        a single host unless node A is already running (node B dials A), and the
#        bootstrap node has no peer dependency of its own — the source precedes
#        the follower.
#     3. bootstrap-funds BEFORE supporting-miner. supporting-miner.sh dies unless
#        SUPPORTING_ADDR is set, and that address is keygen'd by bootstrap-funds.
#        bootstrap-funds also brings up the persistent supporting miner itself; we
#        align its supervised NAME to "supporting-miner" (SUPPORTING_MINER_NAME)
#        so the one canonical miner is what supporting-miner.sh, restart-a-synced
#        and register-providers all refer to (no duplicate, no name skew).
#
# Design rules (shared with the whole harness):
#   * set -euo pipefail.
#   * IDEMPOTENT / SAFE TO RESUME — run-all does not itself track "done" state; it
#     re-invokes each idempotent stage, and every stage detects its own existing
#     pids / keys / outpoints / files and no-ops (never silently overwriting) when
#     already complete. Re-running ./run-all.sh after a fix resumes cheaply.
#   * FAIL-CLOSED — the pipeline STOPS at the first stage that fails OR whose
#     script is absent, with an actionable message; it never claims a stage it did
#     not reach. It leaves the already-brought-up daemons RUNNING (like the node /
#     miner launchers) so the operator can inspect or resume in place.
#   * PORTABLE — bash 3.2 (stock macOS) + Linux; BSD + GNU coreutils. No arrays /
#     declare -A / mapfile; space-delimited plan lists, matching common.sh.
#   * register_cleanup TRAP — a single summary emitter is armed up front so a
#     PASS / PARTIAL / STOPPED report is printed on EVERY exit path (including an
#     interrupt or a mid-build abort). It stops NO daemon (bring-up must persist).
#
# All shared behaviour lives in common.sh and is CALLED, never reimplemented.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"
# shellcheck source=remote.sh
. "$SCRIPT_DIR/remote.sh"   # STN-014 multi-host transport (node_is_remote/remote_*/preflight_ssh)

# Tag every log/warn/die line from this orchestrator (respects an operator override).
PALW_LOG_TAG="${PALW_LOG_TAG:-run-all}"; export PALW_LOG_TAG

# -----------------------------------------------------------------------------
# Stage plan (ordered). build-and-hash is separated out because it must run
# BEFORE run-all's own load_env (it produces the binaries load_env verifies).
# -----------------------------------------------------------------------------
BUILD_STEP="build-and-hash"
REST_PLAN="preflight node-a node-b bootstrap-funds supporting-miner dns-validator register-providers create-lifecycle submit-lifecycle start-palw-miner verify-consensus verify-coinbase negative-tests collect-artifacts"
FULL_PLAN="$BUILD_STEP $REST_PLAN"
# Stages that SHIP with the Phase-0 harness (a missing one means a broken/partial
# checkout, not a not-yet-authored gap). The remaining stages (create-lifecycle,
# submit-lifecycle, start-palw-miner, verify-coinbase, collect-artifacts) may be
# legitimately absent in an early Phase-0 checkout — the message distinguishes.
CORE_STAGES="build-and-hash preflight node-a node-b bootstrap-funds supporting-miner dns-validator register-providers verify-consensus"
# STN-014 §5.4: stages that MUTATE node A's kaspad process (restart it into
# validator/mining mode). In the controller role with a REMOTE node A these run
# ON node A's own host via its agent (node_dispatch run-stage), so the controller
# never restarts kaspad itself, keeps no node pid file, and never receives node
# A's DNS seed (kept host-local there). See run_one + the controller role case.
NODE_A_MUTATING="dns-validator start-palw-miner"

# -----------------------------------------------------------------------------
# Progress / summary state (GLOBAL — the EXIT trap reads these after the body has
# returned; a `local` would be gone by then). Initialised so `set -u` never trips.
# -----------------------------------------------------------------------------
_PIPE_STARTED=0        # 1 once a pipeline run (all or single) has begun
_PIPE_COMPLETE=0       # 1 only when the full ordered plan ran without a stop
_SUMMARY_EMITTED=0     # guard so the summary prints exactly once
CUR_STEP=""            # step currently executing (for an unexpected-die report)
LAST_OK=""             # last stage that completed successfully
STEPS_DONE=""          # space-list of completed stages
SKIPPED_MINT=0         # 1 when start-palw-miner was skipped (TICKET_MODE != mock)
STOP_STEP=""           # stage the pipeline stopped at (if any)
STOP_REASON=""         # actionable reason for the stop

# =============================================================================
# Helpers (thin; none duplicate common.sh).
# =============================================================================

# _in_list <needle> <space-separated-list>  — 0 iff <needle> is a member.
_in_list() {
    local needle="$1" item
    for item in $2; do [ "$item" = "$needle" ] && return 0; done
    return 1
}

# _step_script <name>  — echo the sibling script basename for a step, or return 1.
_step_script() {
    case "$1" in
        build-and-hash)     printf 'build-and-hash.sh\n' ;;
        preflight)          printf 'preflight.sh\n' ;;
        node-a)             printf 'node-a.sh\n' ;;
        node-b)             printf 'node-b.sh\n' ;;
        bootstrap-funds)    printf 'bootstrap-funds.sh\n' ;;
        supporting-miner)   printf 'supporting-miner.sh\n' ;;
        dns-validator)      printf 'dns-validator.sh\n' ;;
        register-providers) printf 'register-providers.sh\n' ;;
        create-lifecycle)   printf 'create-lifecycle.sh\n' ;;
        submit-lifecycle)   printf 'submit-lifecycle.sh\n' ;;
        start-palw-miner)   printf 'start-palw-miner.sh\n' ;;
        verify-consensus)   printf 'verify-consensus.sh\n' ;;
        verify-coinbase)    printf 'verify-coinbase.sh\n' ;;
        negative-tests)     printf 'negative-tests.sh\n' ;;
        collect-artifacts)  printf 'collect-artifacts.sh\n' ;;
        *) return 1 ;;
    esac
}

# run_one <name> <script-abs>  — invoke a stage with its correct args + per-stage
#   env, capturing the exit code WITHOUT tripping set -e (the `|| rc=$?` guard).
#   Verified dispatch surfaces of each sub-script (see their headers):
#     node-a.sh          reads NODE_A_MODE (no positional arg)
#     node-b.sh          takes {start|stop} (default start)
#     supporting-miner.sh REQUIRES an explicit {start|stop}
#     bootstrap-funds.sh  honours SUPPORTING_MINER_NAME (align to the canonical name)
#     everything else     runs with no positional argument
run_one() {
    local name="$1" script="$2" rc=0
    # STN-014 §5.4 — controller + REMOTE node A: a node-A-MUTATING stage runs ON
    # node A's own host through its agent (run-stage). The controller triggers it
    # but performs NO local kaspad restart, writes NO node pid file, and never
    # receives the DNS seed (dns-validator keygens it host-local on node A).
    # Single-host / role=all: node_is_remote a is false, so this never fires and
    # behaviour is unchanged (the stage runs locally == on node A's host anyway).
    if [ "${PALW_ROLE:-all}" = "controller" ] && node_is_remote a && _in_list "$name" "$NODE_A_MUTATING"; then
        log "dispatching node-A-mutating stage '$name' to node A's host agent ($(node_ssh_host a)) via run-stage (no local kaspad restart; seed stays on node A)"
        node_dispatch a run-stage "$name" || rc=$?
        return "$rc"
    fi
    case "$name" in
        node-a)           NODE_A_MODE=bootstrap bash "$script" || rc=$? ;;
        node-b)           bash "$script" start || rc=$? ;;
        supporting-miner) bash "$script" start || rc=$? ;;
        bootstrap-funds)  SUPPORTING_MINER_NAME=supporting-miner bash "$script" || rc=$? ;;
        negative-tests)   NEG_RELEASE=1 bash "$script" all || rc=$? ;;
        *)                bash "$script" || rc=$? ;;
    esac
    return "$rc"
}

# run_step <name>  — resolve the sibling script, fail-closed if it is absent, run
#   the stage, record progress, and return 0 on success / non-zero on stop. Sets
#   STOP_STEP / STOP_REASON on a stop so the summary can be actionable. NEVER dies
#   itself (the caller decides how a stop propagates), so a stop still reaches the
#   summary rather than aborting mid-message.
run_step() {
    local name="$1" base script rc=0
    CUR_STEP="$name"

    base="$(_step_script "$name")" || die "internal error: no sibling script mapped for step '$name'"
    script="$SCRIPT_DIR/$base"

    if [ ! -f "$script" ]; then
        STOP_STEP="$name"
        if _in_list "$name" "$CORE_STAGES"; then
            STOP_REASON="core stage script is missing: $script — this checkout of the harness looks incomplete/corrupt. Restore $base (it ships with the harness) and re-run."
        else
            STOP_REASON="stage script is not present: $script — this Phase-0 checkout brings the net up through the shipped stages; the lifecycle/mint/collect stage '$base' is not authored here yet. Add it (or check out the full harness), then re-run ./run-all.sh — every earlier stage is idempotent, so the resume is cheap."
        fi
        warn "step '$name' cannot run: $STOP_REASON"
        return 3
    fi

    log "================= step: $name  ($base) ================="
    run_one "$name" "$script" || rc=$?
    if [ "$rc" -ne 0 ]; then
        STOP_STEP="$name"
        STOP_REASON="stage '$name' exited non-zero (code $rc). See its log lines above and the per-node logs under ${PALW_DATA_ROOT:-\$PALW_DATA_ROOT}/logs. Fix the cause, then re-run ./run-all.sh (idempotent resume) or just this stage: ./run-all.sh $name."
        warn "step '$name' FAILED (exit $rc)"
        return "$rc"
    fi

    LAST_OK="$name"
    STEPS_DONE="$STEPS_DONE $name"
    # Best-effort resume breadcrumb (only meaningful once our own env is loaded;
    # a single-step run of build-and-hash deliberately runs before load_env).
    if [ "${_PALW_ENV_LOADED:-}" = "1" ]; then
        state_set RUNALL_LAST_OK "$name" >/dev/null 2>&1 || true
    fi
    log "step '$name' OK"
    return 0
}

# =============================================================================
# Final summary — the SOLE printer, armed via register_cleanup so it fires on
# every exit path (normal, die, interrupt). Derives everything from in-memory
# progress so it works even if we stopped before load_env. Prints to stderr, the
# harness convention. Emits once (guarded); stops no daemon.
# =============================================================================
emit_summary() {
    [ "${_SUMMARY_EMITTED:-0}" = "1" ] && return 0
    _SUMMARY_EMITTED=1
    [ "${_PIPE_STARTED:-0}" = "1" ] || return 0    # nothing to summarise (help/list)

    local mode="${TICKET_MODE:-skip}"
    local reached_nodes=no reached_dns=no reached_providers=no reached_batch=no reached_mint=no
    local mint_hash=""
    if _in_list node-a "$STEPS_DONE" && _in_list node-b "$STEPS_DONE"; then reached_nodes=yes; fi
    if _in_list dns-validator "$STEPS_DONE"; then reached_dns=yes; fi
    if _in_list register-providers "$STEPS_DONE"; then reached_providers=yes; fi
    if _in_list submit-lifecycle "$STEPS_DONE"; then reached_batch=yes; fi
    # Review §7 (P0-3): block-minted is an EVIDENCE claim, not a stage-completion
    # claim — require the recorded 128-hex hash on BOTH nodes, identical, in
    # addition to the (now itself evidence-gated) verify-consensus success.
    if [ "$mode" = "mock" ] && _in_list start-palw-miner "$STEPS_DONE" && _in_list verify-consensus "$STEPS_DONE" \
        && [ "${_PALW_ENV_LOADED:-}" = "1" ]; then
        local _hA _hB
        _hA="$(state_get PALW_ALGO4_BLOCK_HASH_A 2>/dev/null || true)"
        _hB="$(state_get PALW_ALGO4_BLOCK_HASH_B 2>/dev/null || true)"
        if [ -n "$_hA" ] && [ "$_hA" = "$_hB" ] && [ "${#_hA}" -eq 128 ]; then
            reached_mint=yes
            mint_hash="$_hA"
        fi
    fi

    local result headline
    if [ "${_PIPE_COMPLETE:-0}" = "1" ]; then
        if [ "$reached_mint" = "yes" ]; then
            result="PASS"
            headline="minted a WIRING-ONLY (non-inference) algo-4 block and verified both-node consensus parity."
        else
            result="PARTIAL"
            if [ "$mode" = "mock" ]; then
                headline="pipeline completed but NO algo-4 block was minted — TICKET_MODE=mock needs the mock-ticket helper, which is built by build-and-hash.sh (see README §Scope)."
            else
                headline="reached batch.status=active. TICKET_MODE=skip cannot mint an algo-4 block (no ticket) — this IS the honest Phase-0 end state, not a failure."
            fi
        fi
    else
        result="STOPPED"
        headline="pipeline stopped at stage '${STOP_STEP:-${CUR_STEP:-?}}' before completing."
    fi

    printf '\n' >&2
    warn "===================== run-all summary ====================="
    warn "result:            $result"
    warn "what happened:     $headline"
    warn "ticket mode:       $mode  (skip -> reaches batch active, never mints; mock -> wiring-only non-inference block via the mock-ticket helper (built by build-and-hash.sh))"
    warn "stages completed:  ${STEPS_DONE:-<none>}"
    warn "milestones:        nodes-up=$reached_nodes  dns-confirmed=$reached_dns  providers-registered=$reached_providers  batch-active=$reached_batch  block-minted=$reached_mint${mint_hash:+ ($mint_hash)}"
    if [ "$SKIPPED_MINT" = "1" ]; then
        warn "mint stage:        SKIPPED (start-palw-miner runs only under TICKET_MODE=mock)."
    fi
    if [ "${_PALW_ENV_LOADED:-}" = "1" ]; then
        local _bid
        _bid="$(state_get PALW_BATCH_ID 2>/dev/null || true)"
        [ -n "$_bid" ] && warn "batch id:          $_bid"
        if _in_list verify-consensus "$STEPS_DONE"; then
            warn "consensus report:  ${PALW_DATA_ROOT:-\$PALW_DATA_ROOT}/artifacts/verify-consensus.txt"
        fi
    fi
    if [ "$result" = "STOPPED" ]; then
        warn "stop reason:       ${STOP_REASON:-an earlier fatal error occurred; see the log lines above.}"
        warn "resume:            fix the cause, then re-run './run-all.sh' (completed stages are idempotent no-ops) or re-run just: './run-all.sh ${STOP_STEP:-<step>}'."
    fi
    warn "data root:         ${PALW_DATA_ROOT:-<PALW_DATA_ROOT unset — stopped before load_env>}"
    warn "logs / state:      ${PALW_DATA_ROOT:-<data>}/logs  ,  ${PALW_DATA_ROOT:-<data>}/artifacts/state.env"
    warn "daemons:           left RUNNING for inspection/resume; stop everything with ./stop.sh"
    warn "scope:             drives only the stage scripts; never the seeded test-only palw_demo path; run-all mints nothing itself."
    warn "==========================================================="
    return 0
}

# =============================================================================
# Usage / plan listing.
# =============================================================================
usage() {
    cat >&2 <<EOF
usage: ${0##*/} [all | <step> | list | --help]

  all (default)  Run the full Phase-0 bring-up in dependency order, STOP at the
                 first failing or absent stage, then print a PASS / PARTIAL
                 summary. Idempotent + safe to resume: every stage detects its own
                 existing pids/keys/outpoints/files and no-ops when already done.
  <step>         Run exactly ONE stage (individually re-runnable). See 'list'.
  list           Print the ordered stage plan and exit.
  --help         Show this help and exit.

TICKET_MODE (env; default 'skip'):
  skip  reaches batch.status=active but can NEVER mint an algo-4 block (no ticket).
  mock  additionally attempts a WIRING-ONLY, non-inference mint (needs the
        mock-ticket helper (built by build-and-hash.sh)). Real inference needs the
        provider GPU tool (out of scope here).

Handy env overrides (see env.example for the full list):
  BUILD_SKIP=1   skip the cargo build in build-and-hash (binaries must already exist).
  PALW_ENV_FILE=/path/to/env   use a specific config instead of env.local/env.example.
EOF
}

print_plan() {
    local s note
    log "run-all stage plan (executed top-to-bottom; STOP at the first failing/absent stage):"
    for s in $FULL_PLAN; do
        note=""
        [ "$s" = "start-palw-miner" ] && note="  [only when TICKET_MODE=mock]"
        _in_list "$s" "$CORE_STAGES" || note="$note  [lifecycle/mint/collect stage — may be absent in an early Phase-0 checkout]"
        printf '    %s%s\n' "$s" "$note" >&2
    done
    printf '\n' >&2
    log "order notes: build-and-hash precedes preflight (load_env needs the binaries it builds); node-a(bootstrap) precedes node-b (node B dials A and fail-closes without it on one host); bootstrap-funds precedes supporting-miner (the miner needs the SUPPORTING_ADDR that funding keygen's)."
}

# =============================================================================
# Full pipeline.
# =============================================================================
do_all() {
    require_cmd bash
    _PIPE_STARTED=1
    register_cleanup 'emit_summary'

    log "PALW closed two-node testnet — one-command bring-up. Stages run in dependency order; the run STOPS fail-closed at the first stage that fails or is absent."

    # 1. build-and-hash FIRST — it produces the release binaries that load_env
    #    (and every later stage) fail-closes without. Idempotent + incremental;
    #    honours BUILD_SKIP=1 when the binaries already exist.
    run_step "$BUILD_STEP" || return 0     # stop; the trap emits the summary

    # 2. run-all's OWN load_env — now the binaries exist, so its fail-closed bind
    #    succeeds. Gives us TICKET_MODE (for the mint gate) + PALW_DATA_ROOT (for
    #    the summary) and validates the config once for the whole run.
    CUR_STEP="load-env"
    load_env

    log "config loaded (NETWORK=$NETWORK TICKET_MODE=$TICKET_MODE DATA=$PALW_DATA_ROOT). Running the remaining stages."

    # 2b. STN-014 / G2 role split. 'all' (single host) runs everything; a per-node
    #     host runs ONLY its node daemon (so ONE host NEVER launches both nodes);
    #     'controller' drives the lifecycle against A/B over their RPC endpoints and
    #     launches no node locally. Node addressing already routes reads to the
    #     configured A_/B_WRPC_ENDPOINT tunnels (common.sh node_wrpc/node_grpc).
    case "${PALW_ROLE:-all}" in
        all) : ;;   # full REST_PLAN (unchanged single-host behaviour)
        node-a)
            log "PALW_ROLE=node-a: starting ONLY node A on this host (daemon left running); the controller drives the lifecycle."
            REST_PLAN="node-a" ;;
        node-b)
            log "PALW_ROLE=node-b: starting ONLY node B on this host (daemon left running); the controller drives the lifecycle."
            REST_PLAN="node-b" ;;
        controller)
            log "PALW_ROLE=controller: launching NO node locally; driving the lifecycle against A/B over their configured RPC endpoints (A_WRPC_ENDPOINT=${A_WRPC_ENDPOINT:-loopback}, B_WRPC_ENDPOINT=${B_WRPC_ENDPOINT:-loopback})."
            preflight_ssh a; preflight_ssh b
            _rp=""; for _s in $REST_PLAN; do case "$_s" in node-a|node-b) : ;; *) _rp="$_rp $_s" ;; esac; done
            REST_PLAN="$_rp"
            if node_is_remote a; then
                log "controller role: node A is remote ($(node_ssh_host a)) — node-A-mutating stages ($NODE_A_MUTATING) run ON node A's host via its agent (run_one -> node_dispatch run-stage). The controller restarts no kaspad, writes no node pid file, and never receives node A's DNS seed. Read/verify stages use the RPC tunnels."
                # Node A's host must own its DNS-validator seed BEFORE the dispatched
                # dns-validator stage bonds+restarts it — keygen it host-local now
                # (public identity/address only ever return; §5.4 condition 3).
                if _in_list dns-validator "$REST_PLAN"; then
                    log "ensuring node A's host holds its own DNS-validator seed (host-local keygen via the agent; controller receives public identity only)"
                    node_dispatch a generate-dns-key || die "controller role: node A host-local DNS keygen failed via its agent (node_dispatch a generate-dns-key)"
                fi
            fi
            if node_is_remote b; then
                log "controller role: node B is remote ($(node_ssh_host b)) — its logs/artifacts are pulled from its host by collect-artifacts (agent collect-tar); node B runs no mutating stage in this harness."
            fi ;;
        *) die "PALW_ROLE='${PALW_ROLE:-}' is not one of: all | controller | node-a | node-b (see env.example)." ;;
    esac

    # 3. the remaining stages, in order. The mint stage is TICKET_MODE-gated; a
    #    missing/failing stage stops the run (fail-closed) and reaches the summary.
    local step
    for step in $REST_PLAN; do
        if [ "$step" = "start-palw-miner" ] && [ "${TICKET_MODE:-skip}" = "skip" ]; then
            log "skipping stage 'start-palw-miner': TICKET_MODE=skip reaches batch.status=active but has no mintable ticket."
            SKIPPED_MINT=1
            continue
        fi
        run_step "$step" || return 0       # stop at the first failing/absent stage
    done

    _PIPE_COMPLETE=1
    return 0
}

# =============================================================================
# Single stage (individually re-runnable). Same invocation as within the full
# pipeline, so a resume of one stage matches exactly what 'all' would run. This
# path reports the ONE stage's outcome concisely — the milestone PASS/PARTIAL
# summary belongs to a full 'all' run, so it is intentionally NOT emitted here.
# =============================================================================
do_step() {
    local name="$1" rc=0
    _step_script "$name" >/dev/null 2>&1 \
        || { usage; die "unknown step '$name' — run './run-all.sh list' to see the valid stage names."; }

    log "running single stage '$name' (idempotent; identical to its step inside the full pipeline)."
    run_step "$name" || rc=$?
    if [ "$rc" -eq 0 ]; then
        log "single stage '$name': OK. Any daemons it started are left running (stop with ./stop.sh); resume the full run with ./run-all.sh."
        exit 0
    fi
    # Non-zero: run_step already recorded STOP_REASON and warned; preserve the code.
    warn "single stage '$name' did not complete (exit $rc): ${STOP_REASON:-see the log lines above}"
    exit "$rc"
}

# =============================================================================
# Dispatch. Validate the action BEFORE any load_env so --help / list work on an
# unconfigured / unbuilt tree.
# =============================================================================
ACTION="${1:-all}"
if [ "$#" -gt 1 ]; then usage; die "too many arguments — expected exactly one of: all | <step> | list | --help"; fi

case "$ACTION" in
    -h|--help|help)
        usage
        exit 0
        ;;
    list|--list|-l)
        print_plan
        exit 0
        ;;
    all|"")
        do_all
        # do_all returns; decide the exit code (the trap prints the summary).
        if [ "$_PIPE_COMPLETE" = "1" ]; then
            exit 0        # completed the intended arc: PASS (mock+mint) or PARTIAL (skip)
        fi
        exit 1            # stopped fail-closed before completing
        ;;
    *)
        do_step "$ACTION"  # exits with the stage's own outcome
        ;;
esac
