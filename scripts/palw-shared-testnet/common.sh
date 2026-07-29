#!/usr/bin/env bash
# =============================================================================
# common.sh — shared foundation for the PALW closed two-node testnet harness
#             (Phase-0 wiring). SOURCE this from every script:
#
#                 set -euo pipefail
#                 SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#                 . "$SCRIPT_DIR/common.sh"
#                 load_env
#
# Design rules (all functions obey them):
#   * IDEMPOTENT   — safe to call twice; never double-creates / double-appends.
#   * FAIL-CLOSED  — on any ambiguity a gate returns non-zero (never a false OK).
#   * PORTABLE     — targets bash 3.2 (stock macOS) and Linux; BSD + GNU coreutils.
#                    No bashisms newer than 3.2 (no declare -A, mapfile, nameref).
#   * NO SECRETS TO ARGV / LOG — this file never echoes seeds or nullifiers.
#
# It defines helpers ONLY. It has NO side effects at source time: it does not
# start processes, does not call load_env, does not touch the filesystem until
# a function is invoked. Every command line it builds uses ONLY verified flags
# (see PHASE0-status.md and the binaries' --help). It invents no flags.
# =============================================================================

set -euo pipefail

# Guard against being executed instead of sourced.
if [ "${BASH_SOURCE[0]:-$0}" = "${0}" ]; then
    echo "common.sh is a library; source it, do not execute it." >&2
    exit 64
fi

# Absolute dir of this file == scripts/palw-shared-testnet/ (the harness root).
COMMON_SH_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
export COMMON_SH_DIR

# -----------------------------------------------------------------------------
# Logging  (all to stderr so command substitution of helpers stays clean)
# -----------------------------------------------------------------------------
_ts()  { date '+%Y-%m-%dT%H:%M:%S%z'; }
_tag() { printf '%s' "${PALW_LOG_TAG:-palw}"; }
# log  <msg...>  — informational line.
log()  { printf '%s [%s] %s\n'       "$(_ts)" "$(_tag)" "$*" >&2; }
# warn <msg...>  — non-fatal warning.
warn() { printf '%s [%s] WARN: %s\n'  "$(_ts)" "$(_tag)" "$*" >&2; }
# die  <msg...>  — fatal: print and exit 1 (fail-closed).
die()  { printf '%s [%s] FATAL: %s\n' "$(_ts)" "$(_tag)" "$*" >&2; exit 1; }

# require_cmd <name>...  — die unless every named external command is on PATH.
require_cmd() {
    local c missing=""
    for c in "$@"; do command -v "$c" >/dev/null 2>&1 || missing="$missing $c"; done
    [ -z "$missing" ] || die "missing required command(s):$missing"
}

# -----------------------------------------------------------------------------
# Path resolution (portable realpath for dirs that may not exist yet)
# -----------------------------------------------------------------------------
# realpath_p <path>  — echo an absolute, symlink-resolved path. Works when the
#                      leaf does not exist yet (resolves the parent).
realpath_p() {
    local p="$1"
    if [ -d "$p" ]; then
        (cd "$p" && pwd -P)
    else
        local d b
        d="$(dirname "$p")"; b="$(basename "$p")"
        if [ -d "$d" ]; then printf '%s/%s\n' "$(cd "$d" && pwd -P)" "$b"
        else printf '%s\n' "$p"; fi
    fi
}

# =============================================================================
# load_env — the entry point every script calls once, right after sourcing.
#   1. sources the config file  ($PALW_ENV_FILE | env.local | env.example)
#   2. derives REPO_ROOT if unset (two levels up from this dir) and realpaths it
#   3. creates PALW_DATA_ROOT + node-a node-b logs keys artifacts  (0700)
#   4. overlays the generated artifacts/state.env (discovered outpoints/addrs)
#   5. validates every required variable is non-empty (fail-closed)
#   6. binds KASPAD / VAL / MINER and verifies each is an executable file
# Re-runnable: reloads state.env each call, install -d is idempotent.
# =============================================================================
load_env() {
    # ---- 1. config file ----------------------------------------------------
    local cfg
    if [ -n "${PALW_ENV_FILE:-}" ]; then cfg="$PALW_ENV_FILE"
    elif [ -f "$COMMON_SH_DIR/env.local" ]; then cfg="$COMMON_SH_DIR/env.local"
    else cfg="$COMMON_SH_DIR/env.example"; fi
    [ -f "$cfg" ] || die "config not found: $cfg (copy env.example to env.local)"
    # shellcheck disable=SC1090
    set -a; . "$cfg"; set +a
    PALW_ENV_FILE="$cfg"; export PALW_ENV_FILE

    # ---- 2. REPO_ROOT ------------------------------------------------------
    : "${REPO_ROOT:=$(cd "$COMMON_SH_DIR/../.." && pwd -P)}"
    [ -d "$REPO_ROOT" ] || die "REPO_ROOT does not exist: $REPO_ROOT (set REPO_ROOT in env.local)"
    REPO_ROOT="$(realpath_p "$REPO_ROOT")"; export REPO_ROOT

    # ---- 3. data root + subdirs (0700) ------------------------------------
    [ -n "${PALW_DATA_ROOT:-}" ] || die "PALW_DATA_ROOT is unset (define it in the config file)"
    install -d -m 0700 "$PALW_DATA_ROOT" \
        || die "cannot create PALW_DATA_ROOT: $PALW_DATA_ROOT"
    PALW_DATA_ROOT="$(realpath_p "$PALW_DATA_ROOT")"; export PALW_DATA_ROOT
    install -d -m 0700 \
        "$PALW_DATA_ROOT/node-a" \
        "$PALW_DATA_ROOT/node-b" \
        "$PALW_DATA_ROOT/logs" \
        "$PALW_DATA_ROOT/keys" \
        "$PALW_DATA_ROOT/artifacts" \
        || die "cannot create PALW_DATA_ROOT subdirs under $PALW_DATA_ROOT"

    # ---- 4. discovered-state overlay --------------------------------------
    local sf="$PALW_DATA_ROOT/artifacts/state.env"
    if [ -f "$sf" ]; then
        # shellcheck disable=SC1090
        set -a; . "$sf"; set +a
    fi

    # ---- 5. validate required ---------------------------------------------
    local v missing=""
    for v in REPO_ROOT PALW_DATA_ROOT NETWORK NETWORK_BASE NETSUFFIX \
             NODE_A_HOST NODE_B_HOST RPC_BIND \
             A_P2P_PORT A_GRPC_PORT A_WRPC_PORT \
             B_P2P_PORT B_GRPC_PORT B_WRPC_PORT \
             MINER_INTERVAL_MS LEAF_COUNT TICKET_MODE; do
        [ -n "${!v:-}" ] || missing="$missing $v"
    done
    [ -z "$missing" ] || die "missing required env:$missing"
    case "$TICKET_MODE" in
        skip|mock|real) : ;;
        *) die "TICKET_MODE must be 'skip', 'mock', or 'real', got '$TICKET_MODE'" ;;
    esac

    # ---- 6. binaries -------------------------------------------------------
    KASPAD="$REPO_ROOT/target/release/kaspad"
    VAL="$REPO_ROOT/target/release/kaspa-pq-validator"
    MINER="$REPO_ROOT/target/release/misaminer"
    export KASPAD VAL MINER
    local bin
    for bin in "$KASPAD" "$VAL" "$MINER"; do
        [ -x "$bin" ] || die "binary missing or not executable: $bin (run: cargo build --release)"
    done

    _PALW_ENV_LOADED=1; export _PALW_ENV_LOADED
    log "env loaded: NETWORK=$NETWORK REPO_ROOT=$REPO_ROOT DATA=$PALW_DATA_ROOT TICKET_MODE=$TICKET_MODE"
}

# -----------------------------------------------------------------------------
# Node addressing helpers.  Label is 'a' | 'b' (also accepts node-a / node-b).
#   RPC (gRPC + wRPC-borsh) always binds loopback ($RPC_BIND, 127.0.0.1) — the
#   node only opens RPC on loopback, so a local client always talks 127.0.0.1.
#   P2P advertises $NODE_x_HOST so the *peer* can --connect to it (two-host).
# -----------------------------------------------------------------------------
_node_label() {
    case "$(printf '%s' "${1:-}" | tr 'AB' 'ab')" in
        a|node-a) printf 'a\n' ;;
        b|node-b) printf 'b\n' ;;
        *) die "node label must be a|b, got '${1:-}'" ;;
    esac
}
# _port <a|b> <P2P|GRPC|WRPC> — echo the configured port for that node/kind.
_port() {
    local n kind var
    n="$(_node_label "$1")"; kind="$2"
    case "${n}:${kind}" in
        a:P2P)  var=A_P2P_PORT  ;; a:GRPC) var=A_GRPC_PORT ;; a:WRPC) var=A_WRPC_PORT ;;
        b:P2P)  var=B_P2P_PORT  ;; b:GRPC) var=B_GRPC_PORT ;; b:WRPC) var=B_WRPC_PORT ;;
        *) die "_port: bad args '$1' '$2'" ;;
    esac
    printf '%s\n' "${!var}"
}
# node_wrpc <a|b> — the wRPC-borsh endpoint the CONTROLLER connects to (for
#   --node-wrpc-borsh). Single-host default: the node's loopback ($RPC_BIND:port).
#   TWO-HOST: set A_WRPC_ENDPOINT / B_WRPC_ENDPOINT to a controller-local address —
#   typically an SSH `-L` forward of the REMOTE node's loopback RPC (e.g.
#   127.0.0.1:37610). RPC is NEVER bound to a public interface; the controller reaches
#   a remote node only through a private tunnel/overlay. (STN-014 multi-host.)
node_wrpc() {
    local n; n="$(_node_label "$1")"
    if [ "$n" = a ] && [ -n "${A_WRPC_ENDPOINT:-}" ]; then printf '%s\n' "$A_WRPC_ENDPOINT"; return; fi
    if [ "$n" = b ] && [ -n "${B_WRPC_ENDPOINT:-}" ]; then printf '%s\n' "$B_WRPC_ENDPOINT"; return; fi
    printf '%s:%s\n' "$RPC_BIND" "$(_port "$n" WRPC)"
}
# node_grpc <a|b> — gRPC endpoint the controller/miner connects to (misaminer --pool).
#   Same single-host default / two-host override contract as node_wrpc (A/B_GRPC_ENDPOINT).
node_grpc() {
    local n; n="$(_node_label "$1")"
    if [ "$n" = a ] && [ -n "${A_GRPC_ENDPOINT:-}" ]; then printf '%s\n' "$A_GRPC_ENDPOINT"; return; fi
    if [ "$n" = b ] && [ -n "${B_GRPC_ENDPOINT:-}" ]; then printf '%s\n' "$B_GRPC_ENDPOINT"; return; fi
    printf '%s:%s\n' "$RPC_BIND" "$(_port "$n" GRPC)"
}
# node_p2p_addr <a|b>   — routable P2P host:port a peer uses in --connect.
node_p2p_addr() {
    local n host; n="$(_node_label "$1")"
    if [ "$n" = a ]; then host="$NODE_A_HOST"; else host="$NODE_B_HOST"; fi
    printf '%s:%s\n' "$host" "$(_port "$n" P2P)"
}
# node_appdir <a|b>     — kaspad --appdir for that node.
node_appdir() { printf '%s/node-%s\n' "$PALW_DATA_ROOT" "$(_node_label "$1")"; }
# node_log <a|b>        — conventional log path for that node (grepped by gates).
node_log()    { printf '%s/logs/node-%s.log\n' "$PALW_DATA_ROOT" "$(_node_label "$1")"; }

# -----------------------------------------------------------------------------
# Field extractors for the human-readable VAL status / palw-status output.
# Tolerant of  key: v  |  key = v  |  key(v)  |  key -> v  |  key v .
# Respects token boundaries so 'status' never matches inside 'bond_status'.
# Portable awk (no gawk IGNORECASE); keys are fixed lowercase_with_underscores.
# -----------------------------------------------------------------------------
# _kv <key>   — read stdin, echo the first single-token value for <key>.
_kv() {
    awk -v k="$1" '
    {
        s=$0
        while ((p=index(s,k))>0) {
            ok=1
            if (p>1) { lc=substr(s,p-1,1); if (lc ~ /[A-Za-z0-9_]/) ok=0 }
            if (ok) {
                v=substr(s, p+length(k))
                sub(/^[ \t]*(->|:|=|\()?[ \t]*/, "", v)   # drop separator
                sub(/[ \t,)].*$/, "", v)                  # stop at first terminator
                if (v != "") { print v; exit }
            }
            s=substr(s, p+length(k))
        }
    }'
}
# _line <key> — read stdin, echo the whole trailing value for <key> (keeps
#               internal spaces, trims edges). Used for dns_anchor / sink where
#               the value is "<hash> (daa <N>)".
_line() {
    awk -v k="$1" '
    {
        s=$0
        while ((p=index(s,k))>0) {
            ok=1
            if (p>1) { lc=substr(s,p-1,1); if (lc ~ /[A-Za-z0-9_]/) ok=0 }
            if (ok) {
                v=substr(s, p+length(k))
                sub(/^[ \t]*(->|:|=|\()?[ \t]*/, "", v)
                sub(/[ \t]+$/, "", v)
                print v; exit
            }
            s=substr(s, p+length(k))
        }
    }'
}

# -----------------------------------------------------------------------------
# Status wrappers.  Thin, verified-flag-only shells around the binaries.
# -----------------------------------------------------------------------------
# node_status <a|b> [stake_bond txid:index]  — VAL status one-shot for a node.
# _endpoint_open <host:port> — 0 iff a TCP connect succeeds. Portable fail-fast
# reachability probe (bash /dev/tcp builtin; no `timeout`/`nc` dependency — macOS has
# neither by default). Loopback / SSH-tunnel endpoints refuse instantly when down, so
# this returns fast; it exists because the one-shot `VAL status` uses a RETRY connect
# strategy and would otherwise HANG against a down node.
_endpoint_open() {
    local hp="${1:-}" host port
    host="${hp%:*}"; port="${hp##*:}"
    [ -n "$host" ] && [ -n "$port" ] || return 1
    ( exec 3<>"/dev/tcp/$host/$port" ) >/dev/null 2>&1 && return 0 || return 1
}
node_status() {
    local n="${1:?node label}" bond="${2:-}"
    # Fail-fast if the node's RPC endpoint is not accepting connections (down). Without
    # this, VAL status (RETRY strategy) hangs against a down node — e.g. preflight's
    # stray-node parity check on a clean start, before node-a/node-b have been launched.
    _endpoint_open "$(node_wrpc "$n")" || return 1
    if [ -n "$bond" ]; then
        "$VAL" status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" --stake-bond "$bond"
    else
        "$VAL" status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK"
    fi
}
# palw_provider_status <a|b> <provider_bond txid:index>  — VAL palw-status (provider.*).
palw_provider_status() {
    local n="${1:?node label}" bond="${2:?provider-bond txid:index}"
    _endpoint_open "$(node_wrpc "$n")" || return 1   # fail-fast if the node is down (VAL retry would hang)
    "$VAL" palw-status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" --provider-bond "$bond"
}
# palw_batch_status <a|b> <batch_id 128hex>  — VAL palw-status (batch.*).
palw_batch_status() {
    local n="${1:?node label}" bid="${2:?batch-id 128hex}"
    _endpoint_open "$(node_wrpc "$n")" || return 1   # fail-fast if the node is down (VAL retry would hang)
    "$VAL" palw-status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" --batch-id "$bid"
}

# node_sink_daa <a|b>  — echo the current sink DAA score (integer) or fail.
#   Primary source: SELECTOR-LESS `palw-status` (allowed since RPC wire v3: it reports
#   sink_daa_score with no batch/bond argument), so this works BEFORE any provider bond is
#   registered — the old bond-gated call left a fresh devnet reading the dns_anchor fallback.
#   Fallback: the "(daa <N>)" of the validator status dns_anchor — but only a NON-ZERO match:
#   on a pre-DNS-activation preset dns_anchor is the zero anchor "(daa 0)", which is an
#   unreadable coordinate here, not a real sink score (a literal 0 made the bootstrap maturity
#   gate fail-closed forever on a fresh devnet-111).
node_sink_daa() {
    local n="${1:-a}" out daa=""
    if _endpoint_open "$(node_wrpc "$n")"; then
        out="$("$VAL" palw-status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" 2>/dev/null || true)"
        daa="$(printf '%s\n' "$out" | _kv sink_daa_score)"
    fi
    case "$daa" in ''|*[!0-9]*)
        out="$(node_status "$n" 2>/dev/null || true)"
        daa="$(printf '%s\n' "$out" | grep -Eo 'daa[ \t]*[0-9]+' | grep -Ev 'daa[ \t]*0$' | head -n1 | grep -Eo '[0-9]+' | head -n1)"
    ;; esac
    case "$daa" in ''|*[!0-9]*) return 1 ;; esac
    printf '%s\n' "$daa"
}
# node_sink <a|b>  — echo an opaque sink identity string for parity comparison
#   (validator dns_anchor "<hash> (daa <N>)"; falls back to provider sink field).
node_sink() {
    local n="${1:-a}" out sink=""
    out="$(node_status "$n" 2>/dev/null || true)"
    sink="$(printf '%s\n' "$out" | _line dns_anchor)"
    if [ -z "$sink" ] && [ -n "${PROV_A_BOND:-}" ]; then
        out="$(palw_provider_status "$n" "$PROV_A_BOND" 2>/dev/null || true)"
        sink="$(printf '%s\n' "$out" | _line sink)"
    fi
    printf '%s\n' "$sink"
}

# current_epoch [<a|b>|<daa>]  — echo floor(sink_daa/100).
#   Arg may be a numeric DAA score (used directly) or a node label (default 'a',
#   the score is discovered via node_sink_daa). palw_epoch_length_daa = 100.
current_epoch() {
    local arg="${1:-a}" daa
    case "$arg" in
        ''|*[!0-9]*) daa="$(node_sink_daa "$arg")" || return 1 ;;
        *) daa="$arg" ;;
    esac
    printf '%s\n' "$(( daa / 100 ))"
}

# -----------------------------------------------------------------------------
# Readiness gates.  Each loops until its condition holds or a deadline passes;
# returns 0 on success, non-zero (with a WARN) on timeout. Callers MUST check
# the return code — a gate never silently proceeds. Defaults from env:
#   GATE_TIMEOUT_SECS, GATE_DNS_TIMEOUT_SECS, GATE_POLL_SECS, STOP_TIMEOUT_SECS.
# -----------------------------------------------------------------------------
# wait_rpc_up <a|b> [timeout] [interval]  — node's wRPC answers a status query.
wait_rpc_up() {
    local n="${1:?node}" timeout="${2:-$GATE_TIMEOUT_SECS}" interval="${3:-$GATE_POLL_SECS}"
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        # Fail-fast port probe first: VAL status uses a RETRY connect and would hang
        # against a not-yet-up node, defeating this gate's own timeout. Only call it
        # once the port accepts connections.
        if _endpoint_open "$(node_wrpc "$n")" && "$VAL" status --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" >/dev/null 2>&1; then
            log "gate ok: node-$n wRPC up"; return 0
        fi
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_rpc_up node-$n timeout after ${timeout}s"; return 1; }
        sleep "$interval"
    done
}
# wait_peer_connected <a|b> [timeout] [interval]  — node log shows a P2P peer.
wait_peer_connected() {
    local n="${1:?node}" timeout="${2:-$GATE_TIMEOUT_SECS}" interval="${3:-$GATE_POLL_SECS}"
    local lf p2p; lf="$(node_log "$n")"; p2p="$(_port "$n" P2P)"
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        if [ -f "$lf" ] && grep -Eiq 'connected to .*peer|peer .* connected|accepted connection from' "$lf"; then
            log "gate ok: node-$n has a connected peer"; return 0
        fi
        # A live node reattached to this harness can have its original log
        # elsewhere. Confirm an established socket when Linux `ss` is available
        # instead of waiting forever on a stale/empty local log.
        if command -v ss >/dev/null 2>&1 \
            && ss -Htn state established 2>/dev/null \
                | awk -v p=":$p2p" '$4 ~ (p "$") || $5 ~ (p "$") { found=1; exit } END { exit !found }'; then
            log "gate ok: node-$n has an established P2P socket on port $p2p"; return 0
        fi
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_peer_connected node-$n timeout after ${timeout}s (log $lf)"; return 1; }
        sleep "$interval"
    done
}
# wait_node_synced <a|b> [timeout] [interval]  — status node_synced == true.
wait_node_synced() {
    local n="${1:?node}" timeout="${2:-$GATE_TIMEOUT_SECS}" interval="${3:-$GATE_POLL_SECS}" synced
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        synced="$(node_status "$n" 2>/dev/null | _kv node_synced)"
        [ "$synced" = "true" ] && { log "gate ok: node-$n synced"; return 0; }
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_node_synced node-$n timeout after ${timeout}s (last='$synced')"; return 1; }
        sleep "$interval"
    done
}
# wait_same_sink [timeout] [interval]  — nodes A and B report an identical,
#   non-empty sink identity (proves they share a selected-chain view).
wait_same_sink() {
    local timeout="${1:-$GATE_TIMEOUT_SECS}" interval="${2:-$GATE_POLL_SECS}" sa sb
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        sa="$(node_sink a)"; sb="$(node_sink b)"
        if [ -n "$sa" ] && [ "$sa" = "$sb" ]; then log "gate ok: same sink ($sa)"; return 0; fi
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_same_sink timeout after ${timeout}s (a='$sa' b='$sb')"; return 1; }
        sleep "$interval"
    done
}
# wait_dns_confirmed <a|b> [timeout] [interval]  — gate on dns_confirmed:true
#   AND an advancing dns_anchor (two DISTINCT anchor samples while confirmed).
#   Deliberately does NOT read dns_health: it is liveness-only and flickers on
#   fresh nets (trailing window averages empty pre-validator epochs).
wait_dns_confirmed() {
    local n="${1:?node}" timeout="${2:-$GATE_DNS_TIMEOUT_SECS}" interval="${3:-$GATE_POLL_SECS}"
    local out conf anchor prev="" deadline=$(( $(date +%s) + timeout ))
    while :; do
        out="$(node_status "$n" 2>/dev/null || true)"
        conf="$(printf '%s\n' "$out" | _kv dns_confirmed)"
        anchor="$(printf '%s\n' "$out" | _line dns_anchor)"
        if [ "$conf" = "true" ] && [ -n "$anchor" ] && [ -n "$prev" ] && [ "$anchor" != "$prev" ]; then
            log "gate ok: node-$n dns_confirmed=true, anchor advancing ($prev -> $anchor)"; return 0
        fi
        [ -n "$anchor" ] && prev="$anchor"
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_dns_confirmed node-$n timeout after ${timeout}s (conf='$conf' anchor='$anchor')"; return 1; }
        sleep "$interval"
    done
}
# wait_palw_beacon_healthy <a|b> [need=3] [timeout] [interval] — poll VAL status until
#   dns_health == Active AND dns_confirmed == true for <need> CONSECUTIVE polls. This is
#   the observable proxy for the PALW beacon being Healthy — the gate that opens the
#   Certified->Active activation (palw_lagged_activation_open needs Healthy sustained across
#   >= 2 beacon epochs; no RPC returns activation_open/beacon_mode directly, so we watch
#   dns_health/derive_dns_health plus a confirmed anchor). Register a batch only AFTER this
#   passes so its rolling active window overlaps a sustained-Healthy stretch, not the cold
#   start (PHASE0 G3/G4 — the fix for the Certified->Expired stall).
wait_palw_beacon_healthy() {
    local n="${1:-a}" need="${2:-3}" timeout="${3:-${GATE_DNS_TIMEOUT_SECS:-600}}" interval="${4:-${GATE_POLL_SECS:-5}}"
    local out health conf ok=0 deadline=$(( $(date +%s) + timeout ))
    while :; do
        out="$(node_status "$n" 2>/dev/null || true)"
        health="$(printf '%s\n' "$out" | _kv dns_health)"
        conf="$(printf '%s\n' "$out" | _kv dns_confirmed)"
        if [ "$health" = "Active" ] && [ "$conf" = "true" ]; then
            ok=$(( ok + 1 ))
            log "beacon warm-up: node-$n dns_health=Active dns_confirmed=true ($ok/$need)"
            [ "$ok" -ge "$need" ] && { log "gate ok: node-$n beacon sustained Healthy (dns_health=Active x$need) — the activation window should open"; return 0; }
        else
            [ "$ok" -gt 0 ] && log "beacon warm-up reset on node-$n (dns_health='${health:-?}' dns_confirmed='${conf:-?}')"
            ok=0
        fi
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_palw_beacon_healthy node-$n timeout after ${timeout}s (last dns_health='${health:-?}' dns_confirmed='${conf:-?}') — beacon never reached sustained Healthy; tune pacing (KASPA_VALIDATOR_HEARTBEAT_SECS shorter, MINER_INTERVAL_MS slower) so DNS/beacon epochs are serviced every tick."; return 1; }
        sleep "$interval"
    done
}
# wait_batch_status <batch_id> <target_status> [node=a] [timeout] [interval]
#   Poll palw-status batch.* until status == target (registering|active|...).
#   NOTE: after each carrier, a child must be mined before batch fields update
#   (see wait_inclusion / invariant 3) — the supporting miner must be running.
wait_batch_status() {
    local bid="${1:?batch-id}" target="${2:?target status}" n="${3:-a}"
    local timeout="${4:-$GATE_TIMEOUT_SECS}" interval="${5:-$GATE_POLL_SECS}" st
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        st="$(palw_batch_status "$n" "$bid" 2>/dev/null | _kv status)"
        [ "$st" = "$target" ] && { log "gate ok: batch $bid status=$target"; return 0; }
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_batch_status timeout after ${timeout}s (want=$target last='$st')"; return 1; }
        sleep "$interval"
    done
}
# wait_inclusion <a|b> [min_children=1] [timeout] [interval]  — wait until the
#   selected chain advances >= min_children DAA-blocks past a baseline captured
#   at call time. Implements invariant (3): after a palw-submit carrier lands,
#   ensure >= N selected children exist so the past-relative palw-status view
#   reflects it before you read batch fields. (palw-submit itself already waits
#   for its own change-outpoint inclusion; this waits for the *following* child.)
wait_inclusion() {
    local n="${1:-a}" need="${2:-1}" timeout="${3:-$GATE_TIMEOUT_SECS}" interval="${4:-$GATE_POLL_SECS}"
    local base cur; base="$(node_sink_daa "$n")" || { warn "wait_inclusion: no baseline sink daa on node-$n"; return 1; }
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        cur="$(node_sink_daa "$n" 2>/dev/null || true)"
        case "$cur" in ''|*[!0-9]*) : ;; *)
            if [ "$cur" -ge "$(( base + need ))" ]; then
                log "gate ok: selected chain advanced $(( cur - base )) block(s) since $base (>= $need)"; return 0
            fi
        ;; esac
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_inclusion timeout after ${timeout}s (base=$base cur='$cur' need=$need)"; return 1; }
        sleep "$interval"
    done
}

# -----------------------------------------------------------------------------
# Hex helpers.
# -----------------------------------------------------------------------------
# h64 <2hex>  — repeat one byte (exactly 2 hex chars) 64 times -> 128 hex chars
#   (a Hash64 filler, e.g. h64 71). Lowercased.
h64() {
    local b="${1:?two hex chars required}" out="" i
    case "$b" in
        [0-9a-fA-F][0-9a-fA-F]) : ;;
        *) die "h64: need exactly 2 hex chars, got '$b'" ;;
    esac
    for i in $(seq 1 64); do out="$out$b"; done
    printf '%s\n' "$out" | tr 'A-F' 'a-f'
}
# zero128  — the all-zero 128-hex Hash64 (empty/unbound sentinel).
zero128() { h64 00; }
# rand_hex [nbytes=64]  — lowercase hex of N random bytes (unique nullifiers /
#   receipt_da_root / private_match_commitment). Uses od + /dev/urandom.
rand_hex() {
    local n="${1:-64}"
    LC_ALL=C od -An -v -tx1 -N "$n" /dev/urandom | tr -d ' \n'; printf '\n'
}
# reward_spk_p2pkh_mldsa <pubkey>  — build a leaf provider reward SPK hex:
#   000076c440 + <64-byte pubkey hex (128 chars)> + 88a6 .
#   <pubkey> may be the full 128-hex key, or a single 2-hex byte (expanded x64,
#   matching the verified SPK for byte 0x71). Result is lowercased.
reward_spk_p2pkh_mldsa() {
    local pk="${1:?pubkey (128 hex) or a 2-hex byte}"
    case "$pk" in
        [0-9a-fA-F][0-9a-fA-F]) pk="$(h64 "$pk")" ;;
    esac
    case "$pk" in
        *[!0-9a-fA-F]*) die "reward_spk: non-hex pubkey" ;;
    esac
    [ "${#pk}" -eq 128 ] || die "reward_spk: need 128-hex (64-byte) pubkey or a 2-hex byte, got ${#pk} chars"
    printf '000076c440%s88a6\n' "$(printf '%s' "$pk" | tr 'A-F' 'a-f')"
}

# -----------------------------------------------------------------------------
# Discovered-state persistence  (artifacts/state.env). Idempotent KEY=value.
#   Lines are written as `export KEY=<shell-quoted>` so a later load_env sources
#   them straight into the environment. state_set also updates the live env.
# -----------------------------------------------------------------------------
state_file() { printf '%s/artifacts/state.env\n' "$PALW_DATA_ROOT"; }
# state_set <KEY> <value>  — persist/overwrite one variable (idempotent).
state_set() {
    local key="${1:?KEY}" val="${2-}" f tmp
    case "$key" in [A-Za-z_][A-Za-z0-9_]*) : ;; *) die "state_set: invalid key '$key'" ;; esac
    f="$(state_file)"
    install -d -m 0700 "$(dirname "$f")"
    [ -f "$f" ] || { : > "$f"; chmod 0600 "$f" 2>/dev/null || true; }
    tmp="$(mktemp "${f}.XXXXXX")" || die "state_set: mktemp failed"
    grep -vE "^(export[[:space:]]+)?${key}=" "$f" > "$tmp" 2>/dev/null || true
    printf 'export %s=%q\n' "$key" "$val" >> "$tmp"
    mv "$tmp" "$f"
    chmod 0600 "$f" 2>/dev/null || true
    export "${key}=${val}"
    log "state_set $key"
}
# state_get <KEY>  — echo the current value (live env first, then state.env).
state_get() {
    local key="${1:?KEY}" v f
    v="${!key:-}"
    if [ -z "$v" ]; then
        f="$(state_file)"
        if [ -f "$f" ]; then
            v="$(grep -E "^(export[[:space:]]+)?${key}=" "$f" | tail -n1 | sed -E "s/^(export[[:space:]]+)?${key}=//")"
            # strip one layer of surrounding single/double quotes from %q output
            case "$v" in
                \'*\') v="${v#\'}"; v="${v%\'}" ;;
                \"*\") v="${v#\"}"; v="${v%\"}" ;;
            esac
        fi
    fi
    printf '%s\n' "$v"
}

# -----------------------------------------------------------------------------
# PID / process lifecycle.  Records pid + start-time + argv so is_running
# survives PID reuse (a recycled PID has a different start-time/cmd).
# Records live at $PALW_DATA_ROOT/<name>.pid (mode 0600). Fields: pid<TAB>lstart<TAB>cmd.
# -----------------------------------------------------------------------------
pid_file() { printf '%s/%s.pid\n' "$PALW_DATA_ROOT" "${1:?name}"; }
_proc_starttime() { ps -o lstart= -p "${1:?pid}" 2>/dev/null | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//'; }
_proc_cmd()       { ps -o command= -p "${1:?pid}" 2>/dev/null | tr '\t' ' ' | sed -E 's/[[:space:]]+$//'; }
# write_pid <name> <pid>  — record a supervised process under <name>.
write_pid() {
    local name="${1:?name}" pid="${2:?pid}" f
    f="$(pid_file "$name")"
    install -d -m 0700 "$(dirname "$f")"
    printf '%s\t%s\t%s\n' "$pid" "$(_proc_starttime "$pid")" "$(_proc_cmd "$pid")" > "$f"
    chmod 0600 "$f" 2>/dev/null || true
    log "write_pid $name -> $pid"
}
# read_pid <name>  — echo the recorded PID (non-zero exit if none).
read_pid() {
    local name="${1:?name}" f pid rest
    f="$(pid_file "$name")"; [ -f "$f" ] || return 1
    IFS="$(printf '\t')" read -r pid rest < "$f" || return 1
    [ -n "${pid:-}" ] || return 1
    printf '%s\n' "$pid"
}
# is_running <name>  — 0 iff the recorded process is alive AND its start-time
#   and argv still match the record (guards against PID reuse).
is_running() {
    local name="${1:?name}" f pid st cmd cst ccmd
    f="$(pid_file "$name")"; [ -f "$f" ] || return 1
    IFS="$(printf '\t')" read -r pid st cmd < "$f" || return 1
    [ -n "${pid:-}" ] || return 1
    kill -0 "$pid" 2>/dev/null || return 1
    cst="$(_proc_starttime "$pid")"; ccmd="$(_proc_cmd "$pid")"
    [ "$cst" = "$st" ] && [ "$ccmd" = "$cmd" ]
}
# stop_pid <name> [timeout=$STOP_TIMEOUT_SECS]  — SIGTERM, wait, then SIGKILL.
#   Idempotent: a stale/absent record just removes the file and returns 0.
stop_pid() {
    local name="${1:?name}" timeout="${2:-$STOP_TIMEOUT_SECS}" f pid deadline
    f="$(pid_file "$name")"
    if ! is_running "$name"; then rm -f "$f"; return 0; fi
    pid="$(read_pid "$name")" || { rm -f "$f"; return 0; }
    log "stopping $name (pid $pid) SIGTERM"
    kill -TERM "$pid" 2>/dev/null || true
    deadline=$(( $(date +%s) + timeout ))
    while is_running "$name"; do
        [ "$(date +%s)" -ge "$deadline" ] && break
        sleep 1
    done
    if is_running "$name"; then
        warn "$name (pid $pid) still alive after ${timeout}s; SIGKILL"
        kill -KILL "$pid" 2>/dev/null || true
        sleep 1
    fi
    rm -f "$f"
    if is_running "$name"; then warn "stop_pid: $name still running"; return 1; fi
    log "stopped $name"
    return 0
}

# -----------------------------------------------------------------------------
# Cleanup trap.  register_cleanup accumulates shell snippets run LIFO on
# EXIT/INT/TERM (each snippet best-effort; the original exit code is preserved).
# Typical use:  register_cleanup 'stop_pid miner-a; stop_pid node-a'
# -----------------------------------------------------------------------------
_CLEANUP_CMDS=()
_run_cleanup() {
    local rc=$? i
    set +e
    trap - EXIT INT TERM
    if [ "${#_CLEANUP_CMDS[@]}" -gt 0 ]; then
        i=$(( ${#_CLEANUP_CMDS[@]} - 1 ))
        while [ "$i" -ge 0 ]; do
            eval "${_CLEANUP_CMDS[$i]}" || true
            i=$(( i - 1 ))
        done
    fi
    _CLEANUP_CMDS=()
    exit "$rc"
}
# register_cleanup <shell-snippet>  — add a cleanup step and (re)arm the trap.
register_cleanup() {
    _CLEANUP_CMDS[${#_CLEANUP_CMDS[@]}]="$*"
    trap _run_cleanup EXIT INT TERM
}

# End of common.sh — helpers only, no side effects until called.
