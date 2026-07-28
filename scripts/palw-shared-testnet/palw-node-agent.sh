#!/usr/bin/env bash
# =============================================================================
# palw-node-agent.sh — STN-014 / G2 §5.4 HOST-LOCAL node agent.
#
#   usage:  ./palw-node-agent.sh <verb> [args...]
#
# SCOPE:
#   The agent that RUNS ON a node host. It OWNS that host's process lifecycle
#   (PID records), its private key material (seeds under keys/), and its logs.
#   The controller NEVER touches a remote node's PID files or secrets directly:
#   it sends verb commands to this agent over remote.sh's node_dispatch (SSH for
#   a real remote host, or a plain local exec on a single box). remote.sh is the
#   reception desk; THIS is the clerk behind it that actually does the work
#   (§5.1's "立派な受付カウンターだけ建っていて、奥に職員がいない" — the clerk).
#
#   Because every mutation of a node runs THROUGH the agent on the node's own
#   host, the six §5.4 completion conditions hold without the controller holding
#   any node PID file or any operator seed:
#     1. controller completes the pipeline with NO node PID files of its own
#        (they live under the agent's host-local PALW_DATA_ROOT).
#     2. node A/B restarts are performed by the host's agent (`restart` verb).
#     3. the controller never receives a private seed — `generate-dns-key`
#        keygens host-local and returns ONLY the public identity + address.
#     4. `collect` bundles host-local logs/argv/disk for the controller to pull.
#     5. `stop` reliably stops this host's supervised services.
#     6. host-key mismatch fails closed — enforced by remote.sh's pinned
#        known_hosts (this agent never relaxes it).
#
# VERBS
#   preflight [<a|b>]              host-local readiness facts (kv); fail-closed.
#   start   <a|b> <bootstrap|validator|miner>
#                                 start the node in a mode (host-local pid record).
#   stop    [<a|b>]               stop one node, or (no arg) every supervised
#                                 process on this host (delegates to stop.sh).
#   restart <a|b> <bootstrap|validator|miner> [--force]
#                                 stop+start; ASSERT the pid/start-time changed
#                                 (with --force a same-pid outcome is fatal).
#   status  [<a|b>] [--json]      host-local process/argv/sync/disk facts.
#   collect <bundle-dir>          copy host-local logs + pid/argv + disk metrics
#                                 into <bundle-dir> (secrets are never copied).
#   run-stage <stage> [args...]   run a sibling harness stage script locally on
#                                 THIS host (how a controller executes a
#                                 node-mutating stage on the node's own host).
#   generate-dns-key              host-local validator keygen; print PUBLIC
#                                 identity + funding address ONLY (seed stays here).
#   prepare-ticket-store          report the host-local ticket-store PUBLIC
#                                 commitments (never the secrets); create the dir.
#
# Design rules (shared with the whole harness):
#   * set -euo pipefail; bash 3.2 safe (no arrays-as-maps / mapfile); BSD+GNU.
#   * IDEMPOTENT — start/stop/restart reuse common.sh's is_running/stop_pid,
#     which already no-op on an already-correct state and are PID-reuse safe.
#   * FAIL-CLOSED — every verb validates its args and returns non-zero with an
#     actionable message on any failure; `restart --force` treats an unchanged
#     pid as a failure (it did not actually restart).
#   * SECRET-SAFE — seeds it generates stay under $PALW_DATA_ROOT/keys on THIS
#     host; nothing secret is ever printed, returned, or bundled.
#
# All shared behaviour lives in common.sh and is CALLED, never reimplemented.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="${PALW_LOG_TAG:-node-agent}"; export PALW_LOG_TAG

# ---------------------------------------------------------------------------
usage() {
    cat >&2 <<EOF
usage: ${0##*/} <verb> [args]

  preflight [a|b]                         host-local readiness facts (kv)
  start   <a|b> <bootstrap|validator|miner>
  stop    [a|b]                           one node, or all supervised (no arg)
  restart <a|b> <bootstrap|validator|miner> [--force]
  status  [a|b] [--json]
  collect <bundle-dir|name>               bundle host-local logs/argv/disk
  collect-tar <name|dir>                  stream a bundle as tar on stdout (pull)
  run-stage <stage> [args...]             e.g. run-stage dns-validator
  generate-dns-key                        prints PUBLIC identity + address only
  prepare-ticket-store                    prints PUBLIC commitments only
  --help

This agent runs ON a node host and owns its pids/secrets/logs. The controller
reaches it via remote.sh's node_dispatch (ssh for a remote host, local exec on
one box). Seeds it creates NEVER leave this host.
EOF
}

# _supervised_name <a|b> — the write_pid/is_running/stop_pid name for a node.
_supervised_name() { printf 'node-%s\n' "$(_node_label "$1")"; }

# _redact — filter stdin so nothing that looks secret is ever emitted. A belt-
# and-suspenders guard around tool output that is SUPPOSED to be public-only.
_redact() {
    grep -viE 'seed|secret|private[_-]?key|mnemonic|-----BEGIN' || true
}

# _stage_script <stage> — resolve a sibling stage basename (fail-closed).
_stage_script() {
    case "$1" in
        node-a)            printf 'node-a.sh\n' ;;
        node-b)            printf 'node-b.sh\n' ;;
        dns-validator)     printf 'dns-validator.sh\n' ;;
        restart-a-synced)  printf 'restart-a-synced.sh\n' ;;
        start-palw-miner)  printf 'start-palw-miner.sh\n' ;;
        bootstrap-funds)   printf 'bootstrap-funds.sh\n' ;;
        supporting-miner)  printf 'supporting-miner.sh\n' ;;
        register-providers) printf 'register-providers.sh\n' ;;
        create-lifecycle)  printf 'create-lifecycle.sh\n' ;;
        submit-lifecycle)  printf 'submit-lifecycle.sh\n' ;;
        *) return 1 ;;
    esac
}

# _start_node <a|b> <mode> — start a node in a mode via its canonical launcher.
#   node-a: bootstrap|validator -> node-a.sh (NODE_A_MODE); miner -> start-palw-miner.sh.
#   node-b: any mode -> node-b.sh start (node B has no validator/miner role here).
_start_node() {
    local n mode; n="$(_node_label "$1")"; mode="$2"
    if [ "$n" = a ]; then
        case "$mode" in
            bootstrap|validator) NODE_A_MODE="$mode" bash "$SCRIPT_DIR/node-a.sh" ;;
            miner)               bash "$SCRIPT_DIR/start-palw-miner.sh" ;;
            *) die "start node-a: mode must be bootstrap|validator|miner, got '$mode'" ;;
        esac
    else
        case "$mode" in
            bootstrap|validator|miner) bash "$SCRIPT_DIR/node-b.sh" start ;;
            *) die "start node-b: mode must be bootstrap|validator|miner, got '$mode'" ;;
        esac
    fi
}

# _pid_line <a|b> — "pid=<n> since=<lstart>" for the node, or "pid=- since=-".
_pid_line() {
    local name pid st cmd; name="$(_supervised_name "$1")"
    if is_running "$name"; then
        pid="$(read_pid "$name")"
        IFS="$(printf '\t')" read -r pid st cmd < "$(pid_file "$name")" || true
        printf 'pid=%s since=%s\n' "${pid:-?}" "${st:-?}"
    else
        printf 'pid=- since=-\n'
    fi
}

# =============================================================================
# Verbs.
# =============================================================================

verb_preflight() {
    local target="${1:-}" n
    log "host-local preflight on $(hostname 2>/dev/null || printf unknown)"
    printf 'agent.host: %s\n' "$(hostname 2>/dev/null || printf unknown)"
    printf 'agent.data_root: %s\n' "$PALW_DATA_ROOT"
    printf 'agent.network: %s\n' "$NETWORK"
    # data root ownership + mode (must be 0700, ours).
    printf 'agent.data_root_mode: %s\n' "$(_stat_mode "$PALW_DATA_ROOT" 2>/dev/null || printf '?')"
    # binaries.
    local bin ok=1
    for bin in "$KASPAD" "$VAL" "$MINER"; do
        if [ -x "$bin" ]; then printf 'binary.ok: %s\n' "$bin"; else printf 'binary.MISSING: %s\n' "$bin"; ok=0; fi
    done
    # disk free + §13.3 storage SLO metrics (free%, DB bytes, growth, gates).
    printf 'disk.free: %s\n' "$(_disk_free_h "$PALW_DATA_ROOT")"
    if [ -f "$SCRIPT_DIR/disk-slo.sh" ]; then
        bash "$SCRIPT_DIR/disk-slo.sh" status 2>/dev/null | grep -E '^(disk\.free_pct|db\.|growth\.)' || true
    fi
    # per-node process state.
    for n in ${target:-a b}; do
        printf 'node-%s.state: %s\n' "$(_node_label "$n")" "$(_pid_line "$n")"
    done
    [ "$ok" = "1" ] || die "host preflight: one or more release binaries are missing (build on this host: cargo build --release)"
    log "host preflight OK"
}

verb_start() {
    [ "$#" -ge 2 ] || { usage; die "start needs <a|b> <bootstrap|validator|miner>"; }
    local n mode; n="$1"; mode="$2"
    log "start node-$(_node_label "$n") mode=$mode (host-local)"
    _start_node "$n" "$mode"
    log "node-$(_node_label "$n") $(_pid_line "$n")"
}

verb_stop() {
    local n="${1:-}"
    if [ -z "$n" ]; then
        # Full host teardown: reuse stop.sh (SIGTERM->grace->SIGKILL, fail-closed).
        log "stopping ALL supervised processes on this host (via stop.sh)"
        bash "$SCRIPT_DIR/stop.sh"
        return 0
    fi
    local name; name="$(_supervised_name "$n")"
    log "stopping node-$(_node_label "$n") (host-local)"
    if [ "$(_node_label "$n")" = b ]; then
        bash "$SCRIPT_DIR/node-b.sh" stop
    else
        stop_pid "$name" || die "failed to stop $name on this host — inspect $(pid_file "$name")"
    fi
    is_running "$name" && die "node-$(_node_label "$n") still running after stop"
    log "node-$(_node_label "$n") stopped"
}

verb_restart() {
    [ "$#" -ge 2 ] || { usage; die "restart needs <a|b> <bootstrap|validator|miner> [--force]"; }
    local n mode force=0; n="$1"; mode="$2"; shift 2
    [ "${1:-}" = "--force" ] && force=1
    local name; name="$(_supervised_name "$n")"

    # Capture the BEFORE identity so we can prove a real restart happened.
    local before="-"
    if is_running "$name"; then before="$(read_pid "$name")@$(_proc_starttime "$(read_pid "$name")")"; fi
    log "restart node-$(_node_label "$n") mode=$mode force=$force (before: $before)"

    if [ "$mode" = "miner" ] && [ "$(_node_label "$n")" = a ]; then
        # start-palw-miner.sh OWNS node A's mine relaunch (stop live validator ->
        # relaunch with --palw-* flags). Under --force it must take the REAL
        # stop+relaunch path even when node A already mines the current batch's
        # leaf, so pass PALW_MINER_FORCE_RESTART=1 through. (The old detour via
        # restart-a-synced.sh silently NO-OP'd: a mining node still carries
        # --enable-validator and no --enable-unsynced-mining, which that script's
        # idempotency check reads as "already a clean validator" — chaining two
        # no-ops and failing the pid-changed proof below.)
        if [ "$force" = "1" ]; then
            PALW_MINER_FORCE_RESTART=1 bash "$SCRIPT_DIR/start-palw-miner.sh"
        else
            bash "$SCRIPT_DIR/start-palw-miner.sh"
        fi
    else
        # bootstrap/validator (and node-b): stop then start in the target mode.
        if is_running "$name"; then
            if [ "$(_node_label "$n")" = b ]; then bash "$SCRIPT_DIR/node-b.sh" stop; else stop_pid "$name"; fi
        fi
        _start_node "$n" "$mode"
    fi

    # Prove the restart: the pid+start-time must differ from BEFORE. Under --force
    # an unchanged identity is fatal (the "restart" was a no-op — §9.3's real bug).
    local after="-"
    if is_running "$name"; then after="$(read_pid "$name")@$(_proc_starttime "$(read_pid "$name")")"; fi
    printf 'restart.node: node-%s\nrestart.before: %s\nrestart.after: %s\n' "$(_node_label "$n")" "$before" "$after"
    if [ "$after" = "-" ]; then
        die "restart node-$(_node_label "$n"): node is not running after the restart"
    fi
    if [ "$before" = "$after" ]; then
        if [ "$force" = "1" ]; then
            die "restart --force node-$(_node_label "$n"): pid/start-time UNCHANGED ($after) — the process was NOT actually restarted"
        fi
        warn "restart node-$(_node_label "$n"): identity unchanged ($after) — the launcher treated it as an idempotent no-op (pass --force to require a real restart)"
    else
        log "restart node-$(_node_label "$n"): $before -> $after (real restart confirmed)"
    fi
}

verb_status() {
    local json=0 target="" a
    for a in "$@"; do
        case "$a" in
            --json) json=1 ;;
            a|b|node-a|node-b) target="$a" ;;
            *) : ;;
        esac
    done
    local nodes; nodes="${target:-a b}"
    if [ "$json" = "1" ]; then
        printf '{"host":"%s","data_root":"%s","network":"%s","disk_free":"%s","nodes":[' \
            "$(hostname 2>/dev/null || printf unknown)" "$PALW_DATA_ROOT" "$NETWORK" "$(_disk_free_h "$PALW_DATA_ROOT")"
        local first=1 n name pid alive
        for n in $nodes; do
            name="$(_supervised_name "$n")"
            if is_running "$name"; then alive=true; pid="$(read_pid "$name")"; else alive=false; pid="null"; fi
            [ "$first" = "1" ] || printf ','
            first=0
            printf '{"node":"%s","alive":%s,"pid":%s}' "$(_node_label "$n")" "$alive" "$pid"
        done
        printf ']}\n'
    else
        printf 'host: %s\n' "$(hostname 2>/dev/null || printf unknown)"
        printf 'data_root: %s\n' "$PALW_DATA_ROOT"
        printf 'disk_free: %s\n' "$(_disk_free_h "$PALW_DATA_ROOT")"
        for n in $nodes; do
            printf 'node-%s: %s\n' "$(_node_label "$n")" "$(_pid_line "$n")"
        done
    fi
}

verb_collect() {
    [ "$#" -ge 1 ] || { usage; die "collect needs <bundle-dir | name>"; }
    local arg="$1" dst name
    # A bare NAME resolves under this host's artifacts/ (the controller does not
    # know the remote PALW_DATA_ROOT); an absolute path is used as-is.
    case "$arg" in
        /*) dst="$arg" ;;
        *)  dst="$PALW_DATA_ROOT/artifacts/$arg" ;;
    esac
    install -d -m 0700 "$dst" || die "cannot create bundle dir: $dst"
    log "collecting host-local artifacts into $dst (secrets are NOT copied)"
    # logs (non-secret) — node + supporting miner.
    if [ -d "$PALW_DATA_ROOT/logs" ]; then
        cp -f "$PALW_DATA_ROOT/logs/"*.log "$dst/" 2>/dev/null || true
    fi
    # pid records (argv contains only key FILE paths, never secrets) + effective
    # live argv from ps for each supervised process.
    {
        printf '# host-local process facts — %s\n' "$(hostname 2>/dev/null || printf unknown)"
        for name in supporting-miner node-a node-b; do
            if is_running "$name"; then
                printf '%s: RUNNING pid=%s\n' "$name" "$(read_pid "$name")"
                printf '  argv: %s\n' "$(_proc_cmd "$(read_pid "$name")")"
            else
                printf '%s: not-running\n' "$name"
            fi
        done
        printf '# disk (df + §13.3 SLO metrics)\n'
        df -h "$PALW_DATA_ROOT" 2>/dev/null || true
        if [ -f "$SCRIPT_DIR/disk-slo.sh" ]; then
            bash "$SCRIPT_DIR/disk-slo.sh" status 2>/dev/null || true
        fi
    } > "$dst/host-status.txt"
    # explicit guard: never let a stray seed slip into the bundle.
    rm -f "$dst"/*.seed "$dst"/*secret* 2>/dev/null || true
    log "collected: $(ls "$dst" 2>/dev/null | tr '\n' ' ')"
    printf 'collect.path: %s\n' "$dst"
}

# collect-tar <name|dir> — stream a previously-`collect`ed bundle as a tar on
#   STDOUT so the controller can pull it back over one SSH hop (log/warn/die all
#   go to STDERR, so redirecting stdout to a file yields a clean archive). A bare
#   NAME resolves under this host's artifacts/, matching `collect`.
verb_collect_tar() {
    [ "$#" -ge 1 ] || { usage; die "collect-tar needs <name|dir>"; }
    local arg="$1" dir
    case "$arg" in
        /*) dir="$arg" ;;
        *)  dir="$PALW_DATA_ROOT/artifacts/$arg" ;;
    esac
    [ -d "$dir" ] || die "collect-tar: bundle dir not present: $dir (run 'collect $arg' on this host first)"
    # guard again: never tar key material even if something placed it here.
    rm -f "$dir"/*.seed "$dir"/*secret* 2>/dev/null || true
    log "streaming host bundle $dir as tar on stdout"
    tar -C "$dir" -cf - .
}

verb_run_stage() {
    [ "$#" -ge 1 ] || { usage; die "run-stage needs <stage>"; }
    local stage="$1"; shift
    local base; base="$(_stage_script "$stage")" || die "unknown stage '$stage' (see usage)"
    [ -f "$SCRIPT_DIR/$base" ] || die "stage script not present on this host: $SCRIPT_DIR/$base"
    log "running stage '$stage' locally on this host ($base) $*"
    bash "$SCRIPT_DIR/$base" "$@"
}

# _generate_key <name> — host-local ML-DSA-87 keygen for one operator identity.
#   The seed FILE stays under keys/ on THIS host at 0600; ONLY the public
#   identity + funding address are printed back. This is how a host becomes an
#   independent operator (DNS validator, provider, auditor) without ever handing
#   its seed to the controller (§5.2 option 2 / §12.2 / §5.4 condition 3).
#
#   keygen writes the seed to --out AND prints its PUBLIC identity + funding
#   address to stdout. We capture that PUBLIC block ONCE at creation into a
#   host-local .pub sidecar (redacted, belt-and-suspenders) and thereafter echo
#   the sidecar — so the verb is idempotent and the seed value never leaves.
_generate_key() {
    local name="$1"
    case "$name" in
        dns-validator|provider-a|provider-b|auditor-c) : ;;
        *) die "operator key name must be one of: dns-validator | provider-a | provider-b | auditor-c (got '$name')" ;;
    esac
    local out="$PALW_DATA_ROOT/keys/$name.seed"
    local pub="$PALW_DATA_ROOT/keys/$name.pub"
    install -d -m 0700 "$PALW_DATA_ROOT/keys"
    if [ -s "$out" ]; then
        log "$name seed already present on this host (not regenerating): $out"
    else
        log "generating a host-local ML-DSA-87 seed for $name -> $out (stays on this host)"
        # Capture stdout (public identity+address); redact any stray secret line
        # before it ever touches disk in the sidecar.
        "$VAL" keygen --out "$out" 2>/dev/null | _redact > "$pub" \
            || die "kaspa-pq-validator keygen failed on this host"
        chmod 0600 "$out" 2>/dev/null || true
        chmod 0644 "$pub" 2>/dev/null || true
    fi
    printf '%s.seed_path: %s   (host-local; NEVER returned)\n' "$name" "$out"
    log "seed retained host-local; controller receives the PUBLIC identity + address only"
    if [ -s "$pub" ]; then
        _redact < "$pub"
    else
        warn "no public sidecar found ($pub) — the seed pre-existed without one; run the bond stage to emit its public identity, or remove $out and re-run to regenerate the pair."
    fi
}

verb_generate_dns_key() { _generate_key dns-validator; }

# generate-operator-key <name> — per-operator host-local keygen (§12.2): a
#   provider/auditor host creates ITS OWN identity; the controller collects only
#   the public block (funding address to fund, identity for the manifest).
verb_generate_operator_key() {
    [ "$#" -ge 1 ] || { usage; die "generate-operator-key needs <dns-validator|provider-a|provider-b|auditor-c>"; }
    _generate_key "$1"
}

verb_prepare_ticket_store() {
    # The ticket store (mock-batch/ticket-secrets.json) holds ticket SECRETS and
    # must stay host-local on the mining host. This verb only ensures the dir
    # exists and reports the PUBLIC commitments if a store is already present.
    local store_dir="$PALW_DATA_ROOT/mock-batch"
    install -d -m 0700 "$store_dir"
    printf 'ticket_store.dir: %s   (host-local; secrets never returned)\n' "$store_dir"
    if [ -f "$store_dir/mock-env.sh" ]; then
        # mock-env.sh exports PUBLIC commitments (batch id, TNC, TAPKH) — echo those.
        # shellcheck disable=SC1090
        ( set -a; . "$store_dir/mock-env.sh"; set +a
          printf 'ticket_store.batch_id: %s\n' "${MOCK_BATCH_ID:-none}"
          printf 'ticket_store.nullifier_commitment: %s\n' "${MOCK_TNC:-none}"
          printf 'ticket_store.authority_pk_hash: %s\n' "${MOCK_TAPKH:-none}" ) | _redact
    else
        log "no ticket store on this host yet — it is populated by the lifecycle (create-lifecycle) on the mining host"
    fi
}

# _stat_mode <path> — portable octal mode. GNU (`stat -c`) FIRST: on Linux
# `stat -f` means filesystem-status (not file mode) and would mis-fire, so the
# GNU form must be tried before the BSD/macOS `stat -f '%Lp'` fallback.
_stat_mode() {
    stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1" 2>/dev/null
}
# _disk_free_h <path> — human-readable free space (portable df).
_disk_free_h() {
    df -h "$1" 2>/dev/null | awk 'NR==2{print $4" free / "$2" total"}'
}

# =============================================================================
# Dispatch. Validate the verb BEFORE load_env so --help works unconfigured.
# =============================================================================
VERB="${1:-}"
case "$VERB" in
    -h|--help|help|"") usage; [ -z "$VERB" ] && exit 1 || exit 0 ;;
esac
shift || true

# Everything else needs the host config + binaries (fail-closed if unbuilt).
load_env

case "$VERB" in
    preflight)           verb_preflight "$@" ;;
    start)               verb_start "$@" ;;
    stop)                verb_stop "$@" ;;
    restart)             verb_restart "$@" ;;
    status)              verb_status "$@" ;;
    collect)             verb_collect "$@" ;;
    collect-tar)         verb_collect_tar "$@" ;;
    run-stage)           verb_run_stage "$@" ;;
    generate-dns-key)    verb_generate_dns_key "$@" ;;
    prepare-ticket-store) verb_prepare_ticket_store "$@" ;;
    *) usage; die "unknown verb '$VERB'" ;;
esac
