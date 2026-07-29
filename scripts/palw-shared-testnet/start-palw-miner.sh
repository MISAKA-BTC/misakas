#!/usr/bin/env bash
# =============================================================================
# start-palw-miner.sh — STN-012: bring up the algo-4 PALW miner on node A and
#                       prove a minted block is accepted on BOTH nodes.
#
#   usage:  ./start-palw-miner.sh
#
# SCOPE:
#   Node A is, by this point in the harness, the SYNCED in-process DNS validator
#   (bootstrap -> validator via restart-a-synced.sh; no --enable-unsynced-mining).
#   This stage RESTARTS node A adding the algo-4 miner flags:
#       --palw-mine
#       --palw-mine-address=$PALW_MINE_ADDR
#       --palw-ticket-authority-key-file=$TICKET_AUTHORITY_SEED
#       --palw-ticket-secret-file=$TICKET_SECRET_FILE
#       --palw-leaf=$PALW_BATCH_ID:0
#   while PRESERVING node A's validator/beacon identity (invariant 5: the DNS
#   validator runs in-process inside kaspad, so the same node keeps
#   --enable-validator/--enable-beacon/--validator-mode/--validator-key/
#   --stake-bond). It then waits for node A to mine an algo-4 block (log marker
#   pow_algo_id=replica) and confirms it is accepted (StatusUTXOValid) on node A
#   AND node B.
#
#   node-a.sh is the canonical launcher, but it has only bootstrap/validator
#   modes — it has NO mining mode and cannot be modified from here — so this
#   stage constructs node A's validator+mining argv itself. That argv MIRRORS
#   node-a.sh's validator branch verbatim (same verified flags, in the same
#   shape) and only APPENDS the five --palw-* miner flags; if node-a.sh's
#   validator argv changes, this must be kept in sync.
#
# HONESTY (matches PHASE0-status.md / README §Scope & limits):
#   * This is reachable ONLY with TICKET_MODE=mock. TICKET_MODE=skip registers
#     the leaf-chunk with NO ticket (--unsafe-skip-ticket-secret-check): the
#     batch can reach status=active but a block with that leaf can NEVER be
#     mined. In skip mode this script prints that fact and exits 0 cleanly.
#   * The minted block is a WIRING-ONLY, non-inference MOCK-TICKET block. The
#     ticket-authority seed and the TicketSecretStore are produced by the
#     mock-ticket helper (mock-ticket/README.md — a workspace member built by build-and-hash.sh). This
#     script NEVER fabricates ticket secrets and NEVER invokes the seeded
#     test-only palw_demo path. Real inference needs the provider GPU tool
#     (out of scope, Phase 1).
#   * The algo-4 chain has fork-choice weight 0 here (PALW-014): this proves
#     algo-4 block validity, propagation, and reward plumbing — NOT PALW chain
#     security. Acceptance is therefore confirmed via the block's UTXO-validation
#     status (StatusUTXOValid), never via a sink change (an algo-4 block never
#     becomes the sink).
#
# Design rules (shared with the whole harness): set -euo pipefail; IDEMPOTENT
# (if node A already runs WITH --palw-mine, the restart is SKIPPED and only the
# acceptance gate is re-verified — nothing is stopped or clobbered); FAIL-CLOSED
# with actionable messages (a gate that cannot confirm returns non-zero and the
# script dies with a next step); a register_cleanup trap tears down a broken
# mining relaunch but LEAVES a healthy mining node running if only the block gate
# was inconclusive. Nothing secret (seed/nullifier value) ever reaches argv or a
# log — only FILE PATHS and public identifiers are passed. It SOURCES common.sh
# and calls ONLY its helpers; the sole locally-defined helpers are the log-marker
# and algo-4 gates (there is no common.sh gate for either), which obey the same
# gate contract (0 ok / non-zero + WARN on timeout; the caller checks the rc).
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

PALW_LOG_TAG="${PALW_LOG_TAG:-start-palw-miner}"; export PALW_LOG_TAG

# Supervised-process names (as recorded by the sibling stage scripts via
# write_pid / is_running / stop_pid). We never invent our own names.
NODE_A_NAME="node-a"
NODE_B_NAME="node-b"
MINER_NAME="supporting-miner"

# -----------------------------------------------------------------------------
# GLOBAL trap-state flags. They MUST be global (not local): the EXIT trap runs
# after this script's body returns, so a scoped flag would already be gone.
#   _STOPPED_ORIG      1 once we have stopped the original node A for the restart
#   _RELAUNCHED        1 once we have launched the new mining kaspad
#   _NODE_RELAUNCH_OK  1 once node A is confirmed UP+healthy as a mining validator
#                        (also set on the idempotent path — node A is already one)
#   _MINER_OK          1 only after BOTH nodes accepted the algo-4 block
#   _ALGO4_HASH        the accepted algo-4 block hash (when parseable from a log)
# -----------------------------------------------------------------------------
_STOPPED_ORIG=0
_RELAUNCHED=0
_NODE_RELAUNCH_OK=0
_MINER_OK=0
_ALGO4_HASH=""

# =============================================================================
# Local helpers. None duplicate common.sh — they add ONLY what it does not
# provide: a baseline-aware log-marker gate and the algo-4 acceptance gate.
# =============================================================================

# add_arg <flag> — append one verified flag to the kaspad argv (bash-3.2 safe
#   index assignment; the same idiom node-a.sh uses).
add_arg() { ARGS[${#ARGS[@]}]="$1"; }

# _log_lines <file> — current line count (0 if the file is absent). Used to
#   BASELINE a log before the mining relaunch so the acceptance gate matches
#   only NEW lines (never a stale algo-4 marker from a previous mock run).
_log_lines() {
    local f="${1:?file}"
    if [ -f "$f" ]; then wc -l < "$f" 2>/dev/null | tr -d ' \n'; else printf '0'; fi
}

# _fail_node <a|b> <msg> — dump the tail of that node's log for context, then die
#   (fail-closed). Used only for BROKEN-relaunch failures; the cleanup trap then
#   stops the half-started node because _NODE_RELAUNCH_OK is still unset.
_fail_node() {
    local n="${1:?node}" msg="${2:?msg}" lf
    lf="$(node_log "$n")"
    if [ -f "$lf" ]; then
        warn "last 25 log lines ($lf):"
        tail -n 25 "$lf" 1>&2 2>/dev/null || true
    fi
    die "$msg"
}

# _wait_log_marker <logfile> <ext-regex> [baseline=0] [timeout] [interval]
#   Gate: the log (only lines AFTER <baseline>) matches <ext-regex>. Mirrors the
#   common.sh gate contract: 0 ok, non-zero + WARN on timeout. baseline=0 scans
#   the whole file. (node-a.sh defines an equivalent marker gate; there is no
#   common.sh helper for a log-line match.)
_wait_log_marker() {
    local lf="${1:?logfile}" re="${2:?regex}" base="${3:-0}"
    local timeout="${4:-$GATE_TIMEOUT_SECS}" interval="${5:-$GATE_POLL_SECS}"
    local deadline=$(( $(date +%s) + timeout ))
    while :; do
        if [ -f "$lf" ] && tail -n "+$(( base + 1 ))" "$lf" 2>/dev/null | grep -Eiq "$re"; then
            return 0
        fi
        [ "$(date +%s)" -ge "$deadline" ] && { warn "log marker /$re/ not seen in $lf (after line $base) within ${timeout}s"; return 1; }
        sleep "$interval"
    done
}

# _wait_algo4 <a|b> <baseline> <pinned-hash-or-empty> [timeout] [interval]
#   Gate: an ACCEPTED algo-4 block is present in the node's log (after baseline).
#   Two modes:
#     * <pinned-hash> non-empty  — CROSS-NODE confirmation: some post-baseline log
#       line contains that exact block hash AND StatusUTXOValid (proves this node
#       validated the SAME algo-4 block node A mined).
#     * <pinned-hash> empty      — LOCAL confirmation on the miner (node A): find
#       the newest algo-4 marker line (pow_algo_id AND replica). Prefer to pin it
#       to its own block hash (a line with that hash AND StatusUTXOValid) and
#       export the hash into _ALGO4_HASH; if the hash is not parseable from the
#       log format, fall back to a MARKER-level confirmation (an algo-4 marker
#       and a StatusUTXOValid line both present) and report the observed state.
#   Contract: 0 ok, non-zero + WARN on timeout. The caller MUST check the rc.
#
#   ACCEPTANCE EVIDENCE IS RPC-GROUNDED, NOT LOG-STRING-GROUNDED. The log-scrape
#   above ("pow_algo_id"+"replica", "StatusUTXOValid") is retained as a fast path,
#   but this kaspad build does NOT emit those tokens: the miner logs
#   `[palw-mine-service] mined + submitted algo-4 block <128hex> off sink <128hex>`
#   and acceptance shows only as `Accepted N blocks ...<hash>... via submit block`.
#   Scraping alone therefore times out on a GENUINELY accepted block — a gate that
#   reports failure on success. So we also (a) take the hash from the miner's own
#   marker line and (b) confirm acceptance over RPC with `find-reward-settlement`,
#   which parses the block's PALW ticket and walks the DAG to the first CHAIN block
#   that merges it, reporting `classification: blue` (admitted) vs `red` (weight-0
#   fork). That is strictly stronger than any log string: it proves the node itself
#   considers the block merged and blue.

# _algo4_accepted_rpc <a|b> <128-hex hash> — 0 iff that node's RPC reports the
#   algo-4 block merged BLUE by a chain block. Non-zero while still unmerged
#   (find-reward-settlement exit 2), so the caller can keep polling.
_algo4_accepted_rpc() {
    local n="${1:?node}" h="${2:?hash}" out
    out="$("$VAL" find-reward-settlement \
            --node-wrpc-borsh "$(node_wrpc "$n")" --network "$NETWORK" \
            --source-block "$h" 2>/dev/null || true)"
    printf '%s\n' "$out" | grep -q '^settlement.classification: blue$'
}

_wait_algo4() {
    local n="${1:?node}" base="${2:-0}" pin="${3:-}"
    local timeout="${4:-$GATE_TIMEOUT_SECS}" interval="${5:-$GATE_POLL_SECS}"
    local lf; lf="$(node_log "$n")"
    local deadline=$(( $(date +%s) + timeout ))
    local stream marker hash
    while :; do
        if [ -f "$lf" ]; then
            # Post-baseline slice, lowercased for portable (no gawk IGNORECASE)
            # token matching. Log values (hashes, StatusUTXOValid, pow_algo_id,
            # replica) are hex/lowercase identifiers, so lowercasing is safe.
            stream="$(tail -n "+$(( base + 1 ))" "$lf" 2>/dev/null | tr 'A-Z' 'a-z')"
            if [ -n "$pin" ]; then
                if printf '%s\n' "$stream" | grep -F -- "$pin" | grep -q 'statusutxovalid'; then
                    log "gate ok: node-$n accepted algo-4 block $pin (StatusUTXOValid, hash-pinned)"
                    return 0
                fi
                # RPC evidence (see header): this node merged the SAME block, blue.
                if _algo4_accepted_rpc "$n" "$pin"; then
                    log "gate ok: node-$n accepted algo-4 block $pin (RPC: find-reward-settlement classification=blue, hash-pinned)"
                    return 0
                fi
            else
                # newest line carrying BOTH the pow_algo_id and replica tokens.
                # Accept EITHER the (unemitted-by-this-build) pow_algo_id/replica
                # marker or the miner's real one: "mined + submitted algo-4 block".
                marker="$(printf '%s\n' "$stream" | awk '
                    (/pow_algo_id/ && /replica/) || /mined \+ submitted algo-4 block/ { ln=$0 }
                    END { if (ln!="") print ln }')"
                if [ -n "$marker" ]; then
                    # Review §7 (P0-3): mock-mode mint success REQUIRES the full 128-hex
                    # Hash64 block hash — a marker line without a parseable hash keeps
                    # POLLING (and times out fail-closed) instead of succeeding hashless.
                    # RPC get-block needs the 128-hex form, so a 64-hex token is only a
                    # diagnostic breadcrumb, never a confirmation.
                    hash="$(printf '%s\n' "$marker" | grep -Eo '[0-9a-f]{128}' | head -n1)"
                    if [ -n "$hash" ]; then
                        if printf '%s\n' "$stream" | grep -F -- "$hash" | grep -q 'statusutxovalid'; then
                            _ALGO4_HASH="$hash"
                            log "gate ok: node-$n mined+accepted algo-4 block $hash (pow_algo_id=replica, StatusUTXOValid, 128-hex-pinned)"
                            return 0
                        fi
                        # RPC evidence (see header): merged blue by a chain block.
                        if _algo4_accepted_rpc "$n" "$hash"; then
                            _ALGO4_HASH="$hash"
                            log "gate ok: node-$n mined+accepted algo-4 block $hash (RPC: find-reward-settlement classification=blue, 128-hex-pinned)"
                            return 0
                        fi
                    elif printf '%s\n' "$marker" | grep -Eoq '[0-9a-f]{64}'; then
                        warn "node-$n: marker line carries only a 64-hex token — NOT accepted as mint evidence (128-hex Hash64 required for RPC verification); continuing to poll."
                    fi
                fi
            fi
        fi
        [ "$(date +%s)" -ge "$deadline" ] && { warn "wait_algo4 node-$n timeout after ${timeout}s (pin='${pin:-}' base=$base log=$lf)"; return 1; }
        sleep "$interval"
    done
}

# _miner_on_exit — cleanup trap (LIFO via register_cleanup). On a clean, complete
#   run (_MINER_OK=1) it is a no-op and node A SURVIVES this launcher's exit.
#   Otherwise it distinguishes a BROKEN relaunch (stop the half-started node —
#   fail-closed) from a HEALTHY node whose block gate was merely inconclusive
#   (leave node A running; it is a valid mining validator) and always prints the
#   exact recovery step. It never touches keys or state files.
_miner_on_exit() {
    [ "${_MINER_OK:-0}" = "1" ] && return 0
    if [ "${_RELAUNCHED:-0}" = "1" ] && [ "${_NODE_RELAUNCH_OK:-0}" != "1" ]; then
        warn "the mining relaunch of node A did not reach readiness — stopping the half-started node A (fail-closed)."
        stop_pid "$NODE_A_NAME" >/dev/null 2>&1 || true
    fi
    if [ "${_STOPPED_ORIG:-0}" = "1" ] && [ "${_NODE_RELAUNCH_OK:-0}" != "1" ]; then
        warn "node A is STOPPED after an interrupted mining restart. Restore the synced validator, then retry:"
        warn "    ./restart-a-synced.sh        # or:  NODE_A_MODE=validator ./node-a.sh"
        warn "    ./start-palw-miner.sh        # idempotent"
    elif [ "${_NODE_RELAUNCH_OK:-0}" = "1" ]; then
        warn "node A is UP as a mining validator, but an accepted algo-4 block was NOT confirmed on both nodes."
        warn "node A is left RUNNING (it is healthy). Investigate: is the batch active (leaf 0 registered), is the mock ticket valid for this leaf, is PALW_ENABLE_ALGO4=1 on every node, and is the supporting miner advancing the base chain? Then re-run ./start-palw-miner.sh (idempotent)."
    fi
    return 0
}

# =============================================================================
# load_env: source config + state.env, create data dirs, validate required vars,
# bind+verify KASPAD/VAL/MINER. Fail-closed and re-runnable.
# =============================================================================
load_env
require_cmd awk grep tail tr wc

# =============================================================================
# 0. Ticketed modes only. skip -> honest note + clean exit 0.
# =============================================================================
case "$TICKET_MODE" in
    mock|real) : ;;
    skip)
        log "TICKET_MODE=skip: the leaf-chunk is registered WITHOUT a ticket (--unsafe-skip-ticket-secret-check), so the batch can reach status=active but NO algo-4 block can EVER be minted from it."
        log "STN-012 needs a ticketed lifecycle (TICKET_MODE=mock or real). Skipping the miner; nothing to mint."
        exit 0
        ;;
    *)
        die "TICKET_MODE must be 'skip', 'mock', or 'real', got '$TICKET_MODE' (load_env should have caught this)."
        ;;
esac

# =============================================================================
# 1. Resolve + validate the miner inputs (fail-closed, actionable). Nothing here
#    starts a process or mutates the running net, so any early die is safe.
#    FILE PATHS only reach argv — never a seed or nullifier value.
# =============================================================================

# 1a. Ticket-authority seed FILE. The STAGE SPEC names it $TICKET_AUTHORITY_SEED;
#     env.example ships the same path under $TICKET_AUTHORITY_KEY. Accept either
#     name (they point at the same seed file), preferring the spec name.
TICKET_AUTHORITY_SEED="${TICKET_AUTHORITY_SEED:-${TICKET_AUTHORITY_KEY:-}}"
[ -n "$TICKET_AUTHORITY_SEED" ] \
    || die "TICKET_AUTHORITY_SEED (a.k.a. TICKET_AUTHORITY_KEY) is empty — create-lifecycle.sh generates it for ticketed modes. Set it in env.local and rerun create-lifecycle."
[ -f "$TICKET_AUTHORITY_SEED" ] \
    || die "TICKET_AUTHORITY_SEED is not readable: $TICKET_AUTHORITY_SEED. Refusing to start algo-4 without its ticket authority."

# 1b. TicketSecretStore FILE (raw-nullifier store, authority-bound). Same name in
#     the spec and env.example. Produced by the mock-ticket helper.
[ -n "${TICKET_SECRET_FILE:-}" ] \
    || die "TICKET_SECRET_FILE is empty — create-lifecycle.sh populates it for ticketed modes."
[ -f "$TICKET_SECRET_FILE" ] \
    || die "TICKET_SECRET_FILE is not readable: $TICKET_SECRET_FILE. Refusing to mine without the lifecycle's TicketSecretStore."
[ -s "$TICKET_SECRET_FILE" ] \
    || die "TICKET_SECRET_FILE is empty: $TICKET_SECRET_FILE — the TicketSecretStore has no content. Re-run the mock-ticket helper to populate it."
# Light shape sanity (WARN only — this stage does not own the store schema).
if ! grep -q 'secrets' "$TICKET_SECRET_FILE" 2>/dev/null; then
    warn "TICKET_SECRET_FILE ($TICKET_SECRET_FILE) does not contain a 'secrets' field — it may not be a valid TicketSecretStore; the miner will reject a malformed store."
fi
# Both ticket files hold secret material in-FILE. We pass only their PATHS to
# argv (never their contents). Warn (non-fatal) if their mode is looser than 0600.
for _kf in "$TICKET_AUTHORITY_SEED" "$TICKET_SECRET_FILE"; do
    _mode="$(ls -l "$_kf" 2>/dev/null | awk '{print $1}')"
    case "$_mode" in
        -rw-------*|-r--------*) : ;;                       # 0600 / 0400 — good
        "") : ;;                                            # ls failed; skip
        *) warn "$_kf is not mode 0600 (perm '$_mode') — tighten it: chmod 0600 '$_kf' (ticket material must not be world/group readable)." ;;
    esac
done

# 1c. algo-4 miner payout address (ML-DSA-87 P2PKH). Prefer the discovered slot
#     PALW_MINE_ADDR (state.env), fall back to the configured PALW_MINE_ADDRESS.
PALW_MINE_ADDR="$(state_get PALW_MINE_ADDR)"
[ -n "$PALW_MINE_ADDR" ] || PALW_MINE_ADDR="${PALW_MINE_ADDRESS:-}"
[ -n "$PALW_MINE_ADDR" ] \
    || die "PALW_MINE_ADDR is empty — the algo-4 coinbase payout address. Set it, e.g.  state_set PALW_MINE_ADDR ${ADDR_PREFIX:-misakadev}:<...>  (typically the mock miner's reward address), then retry."
# Refuse a wrong-network payout address (unspendable coinbase here).
if [ -n "${ADDR_PREFIX:-}" ]; then
    case "$PALW_MINE_ADDR" in
        "$ADDR_PREFIX":?*) : ;;
        *) die "PALW_MINE_ADDR '$PALW_MINE_ADDR' does not match the network address prefix '${ADDR_PREFIX}:' (NETWORK=$NETWORK); refusing to mine to a wrong-network address." ;;
    esac
else
    warn "ADDR_PREFIX unset; skipping payout-address prefix check for PALW_MINE_ADDR."
fi

# 1d. Batch id (128-hex) whose leaf 0 the miner will build. From the lifecycle.
PALW_BATCH_ID="$(state_get PALW_BATCH_ID)"
PALW_BATCH_ID="$(printf '%s' "$PALW_BATCH_ID" | tr 'A-F' 'a-f')"
[ -n "$PALW_BATCH_ID" ] \
    || die "PALW_BATCH_ID is empty — the batch whose leaf 0 gets mined. Run the lifecycle to an active batch first (./create-lifecycle.sh then ./submit-lifecycle.sh), which state_set's PALW_BATCH_ID."
case "$PALW_BATCH_ID" in
    *[!0-9a-f]*) die "PALW_BATCH_ID is not hex: '$PALW_BATCH_ID' (expected 128 lowercase hex chars)." ;;
esac
[ "${#PALW_BATCH_ID}" -eq 128 ] \
    || die "PALW_BATCH_ID must be 128 hex chars (64-byte Hash64), got ${#PALW_BATCH_ID}: '$PALW_BATCH_ID'."
if [ "$PALW_BATCH_ID" = "$(zero128)" ]; then
    die "PALW_BATCH_ID is the all-zero sentinel (unbound) — no real batch to mine. Run the lifecycle (./create-lifecycle.sh / ./submit-lifecycle.sh) to bind a batch id first."
fi

log "miner inputs OK: mine_address=$PALW_MINE_ADDR leaf=${PALW_BATCH_ID}:0 (ticket files present; paths only, no secrets logged)"

# =============================================================================
# 2. Preconditions on the running net (fail-closed, actionable). None mutates
#    anything, so an early die leaves the net untouched.
# =============================================================================

# node A must be running — this stage RESTARTS a live validator, not cold-start.
is_running "$NODE_A_NAME" \
    || die "node A is not running — bring it up as the synced DNS validator first (./node-a.sh then ./restart-a-synced.sh) before starting the algo-4 miner."

# node B must be running — the both-node acceptance check confirms StatusUTXOValid
# on node B's log. (Single-host layout: both logs live here. Two-host layout
# supervises node B on its own host — see the node-B log check at step 6.)
is_running "$NODE_B_NAME" \
    || die "node B is not running — start it with ./node-b.sh; the both-node algo-4 acceptance check needs node B (this harness's both-node checks are single-host, as documented)."

# The supporting algo-3 miner must be running: node A (a validator without
# --enable-unsynced-mining) does NOT self-mine the base chain, so the supporting
# miner is what keeps DAA/epochs advancing and the batch in the sink view — the
# base chain must progress for node A to build algo-4 blocks on top of it.
is_running "$MINER_NAME" \
    || die "the supporting miner ('$MINER_NAME') is not running — start it with ./supporting-miner.sh start before mining; without an advancing base chain node A cannot build an algo-4 block."

# node A must answer RPC before we inspect/restart it.
wait_rpc_up a \
    || die "node A wRPC did not answer on $(node_wrpc a) — inspect $(node_log a) before restarting it to mine."

# node A must actually be meshed with a peer (needed to propagate the algo-4
# block to node B for the both-node acceptance check).
wait_peer_connected a \
    || die "node A has no connected P2P peer (log $(node_log a)) — node B must be connected so the minted algo-4 block propagates for the both-node acceptance check."

# The batch must be ACTIVE (leaf 0 registered) — an algo-4 block can only be
# mined for an active batch. wait_batch_status needs the supporting miner (above)
# to have mined a child so the past-relative palw-status view reflects it.
if ! wait_batch_status "$PALW_BATCH_ID" active a; then
    die "batch $PALW_BATCH_ID is not 'active' (leaf 0 not registered/active yet) — an algo-4 block can only be minted for an ACTIVE batch. Complete the lifecycle to active first (./create-lifecycle.sh then ./submit-lifecycle.sh), then retry."
fi

# =============================================================================
# 3. Inspect node A's LIVE argv (same mechanism restart-a-synced.sh uses) to
#    decide idempotency and to fail-closed on the wrong node-A role.
# =============================================================================
NA_PID="$(read_pid "$NODE_A_NAME")" || die "internal: node A pid record missing despite is_running."
NA_CMD="$(_proc_cmd "$NA_PID")"

# algo-4 acceptance is a START-TIME override; node A MUST already carry it (it is
# not RPC-introspectable, so we read the live argv). Without it, no algo-4 block
# — from node A or node B — could be accepted.
case "$NA_CMD" in
    *--palw-enable-algo4*) : ;;
    *) die "node A is running WITHOUT --palw-enable-algo4 — algo-4 blocks cannot be accepted. Restart the WHOLE set with PALW_ENABLE_ALGO4=1 (identical on every node, never a subset), then retry." ;;
esac

ALREADY_MINING=0
case "$NA_CMD" in
    *--palw-mine*)
        # --palw-mine alone is NOT enough to call this idempotent: the running node
        # is pinned to whatever `--palw-leaf=<batch_id>:<index>` it was launched
        # with. A batch is single-use and expires, so after LIFECYCLE_FORCE built a
        # fresh one the node would keep mining for the RETIRED batch — producing no
        # block at all while every gate reports "already mining". Relaunch unless
        # the pinned leaf is exactly this batch's.
        #
        # PALW_MINER_FORCE_RESTART=1 overrides even a correct pin: the node agent's
        # `restart <a> miner --force` (G7 restart-a) PROVES a restart by pid/start-
        # time change, so an idempotent no-op here — however correct — is a FAIL
        # there. Force always takes the real stop+relaunch path.
        if [ "${PALW_MINER_FORCE_RESTART:-0}" = 1 ]; then
            log "PALW_MINER_FORCE_RESTART=1: node A already mines this batch's leaf, but a REAL restart is demanded (agent restart --force / G7 restart-a) — taking the stop+relaunch path."
        elif [ "${NA_CMD#*--palw-leaf=${PALW_BATCH_ID}:0}" != "$NA_CMD" ]; then
            log "node A already running WITH --palw-mine pinned to THIS batch's leaf (${PALW_BATCH_ID}:0) — skipping the mining relaunch (idempotent); re-verifying the algo-4 acceptance gate only."
            ALREADY_MINING=1
        else
            log "node A is running WITH --palw-mine but pinned to a DIFFERENT (stale/retired) --palw-leaf than the current batch ${PALW_BATCH_ID}:0 — relaunching it onto this batch's leaf. Mining for a retired batch yields no block while every readiness gate still passes."
        fi
        ;;
    *--enable-unsynced-mining*)
        die "node A is still the BOOTSTRAP node (--enable-unsynced-mining) — transition it to the synced validator first with ./restart-a-synced.sh, then re-run ./start-palw-miner.sh."
        ;;
    *--enable-validator*)
        log "node A is a synced validator without --palw-mine — will relaunch it, PRESERVING the validator identity and ADDING the algo-4 miner flags."
        ;;
    *)
        die "node A is running but is neither a validator nor the bootstrap node (unexpected argv) — bring it up as the synced DNS validator (./node-a.sh / ./restart-a-synced.sh) before mining."
        ;;
esac

# Arm the cleanup trap now: from here on, node A may be stopped/half-restarted,
# and an abort must warn (or tear down a broken relaunch) rather than leave a
# silent broken state. On a clean, complete run it becomes a no-op.
register_cleanup '_miner_on_exit'

# =============================================================================
# 4. Restart node A into validator+mining mode (skipped when already mining).
# =============================================================================
if [ "$ALREADY_MINING" != "1" ]; then

    # --- prerequisites for reconstructing the validator argv --------------------
    # DNS validator seed FILE + stake-bond outpoint (from dns-validator.sh, via
    # state.env). Required to keep node A the in-process DNS validator across the
    # restart (invariant 5). We validate the FILE exists; we never read/print it.
    DNS_SEED="$(state_get DNS_SEED)"
    [ -n "$DNS_SEED" ] \
        || die "DNS_SEED is not set in state.env — needed to restart node A as the in-process validator. Run the bond / dns-validator.sh stage first (it keygen's the seed and state_set's DNS_SEED)."
    [ -f "$DNS_SEED" ] \
        || die "DNS_SEED points to a missing file: $DNS_SEED — re-run dns-validator.sh; refusing to restart node A's validator without its key."
    DNS_BOND="$(state_get DNS_BOND)"
    [ -n "$DNS_BOND" ] \
        || die "DNS_BOND is not set in state.env — the validator stake-bond outpoint. Run the bond / dns-validator.sh stage first."
    case "$DNS_BOND" in
        *:*) : ;;
        *)   die "DNS_BOND is malformed: '$DNS_BOND' (expected <txid>:<index>) — re-run the bond stage." ;;
    esac
    _bidx="${DNS_BOND##*:}"; _btxid="${DNS_BOND%:*}"
    case "$_bidx" in ''|*[!0-9]*) die "DNS_BOND index must be numeric: '$DNS_BOND'." ;; esac
    [ -n "$_btxid" ] || die "DNS_BOND txid is empty: '$DNS_BOND'."

    # --- network family flag (verified: --devnet | --testnet) -------------------
    case "$NETWORK_BASE" in
        devnet)  NET_FLAG="--devnet"  ;;
        testnet) NET_FLAG="--testnet" ;;
        *) die "unsupported NETWORK_BASE='$NETWORK_BASE' (this harness verifies devnet|testnet only; see env.example)." ;;
    esac

    # --- algo-4 must be ON for the miner (and node A already carries it) ---------
    case "$(printf '%s' "${PALW_ENABLE_ALGO4:-1}" | tr 'A-Z' 'a-z')" in
        1|true|yes|on) : ;;
        0|false|no|off) die "algo-4 mining requires --palw-enable-algo4, but PALW_ENABLE_ALGO4='${PALW_ENABLE_ALGO4:-}' is OFF. Enable it on EVERY node (identical, never a subset) and restart the set, then retry." ;;
        *) die "PALW_ENABLE_ALGO4 must be 0/1 (true/false), got '${PALW_ENABLE_ALGO4:-}'." ;;
    esac

    # --- assemble the validator+mining argv (verified flags only) ---------------
    # This MIRRORS node-a.sh's validator branch verbatim and only APPENDS the
    # five --palw-* miner flags. RPC binds loopback ($RPC_BIND); P2P listens on
    # 0.0.0.0:A_P2P (the PALW preset still rejects any IP not in --connect
    # pre-handshake); --connect points at node B. The miner flags carry FILE
    # PATHS / public identifiers only — never a ticket secret or seed value.
    ARGS=()
    add_arg "$NET_FLAG"
    add_arg "--netsuffix=$NETSUFFIX"
    add_arg "--appdir=$(node_appdir a)"
    # Archival is OPT-IN (PALW_ARCHIVAL=1); see node-a.sh for the rationale. --yes
    # answers the one-time "previously archival" prompt (headless harness).
    if [ "${PALW_ARCHIVAL:-0}" = 1 ]; then add_arg "--archival"; else add_arg "--yes"; fi
    add_arg "--utxoindex"
    add_arg "--listen=0.0.0.0:$A_P2P_PORT"
    add_arg "--rpclisten=$RPC_BIND:$A_GRPC_PORT"
    add_arg "--rpclisten-borsh=$RPC_BIND:$A_WRPC_PORT"
    add_arg "--connect=$(node_p2p_addr b)"
    add_arg "--palw-enable-algo4"
    # algo-4 PALW miner (STN-012). --palw-mine kept early so its presence stays
    # robustly detectable in the live argv on the idempotent re-run above.
    add_arg "--palw-mine"
    add_arg "--palw-mine-address=$PALW_MINE_ADDR"
    add_arg "--palw-ticket-authority-key-file=$TICKET_AUTHORITY_SEED"
    add_arg "--palw-ticket-secret-file=$TICKET_SECRET_FILE"
    add_arg "--palw-leaf=$PALW_BATCH_ID:0"
    # in-process DNS validator + beacon — node A keeps its validator identity
    # (invariant 5). --validator-key is a FILE PATH; --stake-bond a public outpoint.
    add_arg "--enable-validator"
    add_arg "--enable-beacon"
    add_arg "--validator-mode=active"
    add_arg "--validator-key=$DNS_SEED"
    add_arg "--stake-bond=$DNS_BOND"

    # --- BASELINE both logs BEFORE the restart, so the acceptance gate matches
    #     only NEW algo-4 blocks (never a stale marker from a previous mock run).
    LOGF_A="$(node_log a)"
    LOGF_B="$(node_log b)"
    BASE_A="$(_log_lines "$LOGF_A")"
    BASE_B="$(_log_lines "$LOGF_B")"

    # --- stop the current validator, then relaunch it as a mining validator -----
    _STOPPED_ORIG=1
    stop_pid "$NODE_A_NAME" \
        || die "failed to stop node A cleanly before the mining relaunch (pid was $NA_PID) — inspect it manually; not relaunching over a process that would not stop."

    log "relaunching node A as a mining validator (adds --palw-mine + address/authority/secret/leaf; keeps the DNS validator) ..."
    log "  argv: $KASPAD ${ARGS[*]}"
    # Append to the SAME log (never clobber); the baseline above fences the new run.
    # nohup + </dev/null detaches from the controlling terminal (SIGHUP-safe), matching node-b.sh.
    nohup "$KASPAD" "${ARGS[@]}" >> "$LOGF_A" 2>&1 </dev/null &
    NEW_PID=$!
    _RELAUNCHED=1
    write_pid "$NODE_A_NAME" "$NEW_PID"

    # --- node relaunch readiness (each fail-closed; a failure here means a BROKEN
    #     relaunch, so _fail_node dies and the trap stops the half-started node) --
    sleep 1
    is_running "$NODE_A_NAME" \
        || _fail_node a "node A exited immediately after the mining relaunch (bad --palw-* flags? invalid ticket files? A ports in use?)."
    wait_rpc_up a \
        || _fail_node a "node A wRPC did not come up after the mining relaunch on $(node_wrpc a)."
    _wait_log_marker "$LOGF_A" 'MISAKA node endpoints' "$BASE_A" \
        || _fail_node a "node A did not log 'MISAKA node endpoints' after the mining relaunch (startup incomplete)."
    _wait_log_marker "$LOGF_A" '\[validator-service\]' "$BASE_A" \
        || _fail_node a "node A's validator/beacon service did not restart after the mining relaunch (check DNS_SEED / DNS_BOND)."

    # node A is up and healthy as a mining validator. From here a failure is NOT a
    # broken node — the block gate is inconclusive at worst — so the trap will
    # LEAVE node A running.
    _NODE_RELAUNCH_OK=1
    log "node A relaunched as a mining validator (pid $NEW_PID): wrpc=$(node_wrpc a) listen=0.0.0.0:$A_P2P_PORT connect=$(node_p2p_addr b)"

    # The supporting miner's --pool is node A's gRPC, which dropped while node A
    # restarted; misaminer may have exited. Surface it (non-fatal) — the base
    # chain must keep advancing for node A to build algo-4 blocks.
    # The base chain MUST keep advancing for node A to build algo-4 blocks, so
    # proactively resume the miner (mirrors submit-lifecycle.sh's resume) rather
    # than only warning — otherwise the algo-4 gate below can time out on a clean run.
    if ! is_running "$MINER_NAME"; then
        warn "supporting miner not running after the node A relaunch (its --pool is node A's gRPC, which dropped during the restart); resuming it ..."
        bash "$SCRIPT_DIR/supporting-miner.sh" start \
            || warn "could not auto-resume the supporting miner — start it manually (./supporting-miner.sh start) or the algo-4 gate below will time out."
    fi
else
    # Idempotent path: node A is already a healthy mining validator; do not touch
    # it. Scan the WHOLE logs (baseline 0) — the algo-4 block may already be there.
    BASE_A=0
    BASE_B=0
    _NODE_RELAUNCH_OK=1
fi

# =============================================================================
# 5. Prove an algo-4 block was MINED by node A and ACCEPTED on BOTH nodes.
#    Acceptance is confirmed via StatusUTXOValid (the algo-4 chain has
#    fork-choice weight 0, so it never becomes the sink — a sink check would be
#    meaningless here).
# =============================================================================
log "waiting for node A to mine an accepted algo-4 block (pow_algo_id=replica, StatusUTXOValid), then confirming BOTH nodes accept it ..."

# 5a. node A: local mine+accept (captures the block hash into _ALGO4_HASH).
_wait_algo4 a "$BASE_A" "" \
    || die "no accepted algo-4 block appeared on node A within the gate window. Check: is the batch active (leaf 0 registered), is the mock ticket valid for this leaf, is PALW_ENABLE_ALGO4=1 on every node, and is the supporting miner advancing the base chain? Node A is left running as a mining validator; re-run ./start-palw-miner.sh (idempotent) once addressed."

# 5b. node B: confirm the SAME block was accepted there. The both-node check reads
#     node B's log; on a single-host layout it lives here. On a two-host layout
#     node B's log is on node B's host (this stage runs per host) — fail-closed
#     with that guidance rather than claim a success we cannot observe.
LOGF_B="$(node_log b)"
[ -f "$LOGF_B" ] \
    || die "cannot confirm algo-4 acceptance on node B: its log is not present at $LOGF_B on this host. Single-host runs keep both logs here; on a TWO-HOST layout, confirm StatusUTXOValid for the algo-4 block in node B's log on node B's own host."

# Review §7 (P0-3): a mock-mode mint success REQUIRES the 128-hex hash — 5a can no
# longer return hashless, so this guard is unreachable belt-and-suspenders.
[ -n "$_ALGO4_HASH" ] \
    || die "internal: node A's algo-4 gate returned without a 128-hex block hash — mock-mode success requires a hash-pinned both-node confirmation (this is a bug in _wait_algo4; report it)."
_wait_algo4 b "$BASE_B" "$_ALGO4_HASH" \
    || die "node B did not record StatusUTXOValid for the algo-4 block $_ALGO4_HASH within the gate window — check P2P propagation A->B and that node B also runs --palw-enable-algo4 (identical on every node)."

# =============================================================================
# 6. Success. Record the minted block hash for the downstream coinbase/consensus
#    verifiers, disarm the cleanup, and report the result.
# =============================================================================
# Record the minted-block axes the downstream verifiers actually read:
#   verify-consensus.sh    -> PALW_ALGO4_BLOCK_HASH_A/_B + PALW_ALGO4_ACCEPT_A/_B
#   collect-artifacts.sh   -> the same four slots
# We are only here after BOTH per-node acceptance gates (5a/5b) passed AND with a
# parsed 128-hex hash (P0-3): 5b was hash-pinned, so A and B accepted the SAME
# block. The ACCEPT slots are recorded together with the hash slots — an accept
# verdict can never exist without its hash evidence.
state_set PALW_ALGO4_ACCEPT_A "true"
state_set PALW_ALGO4_ACCEPT_B "true"
if [ -n "$_ALGO4_HASH" ]; then
    state_set PALW_ALGO4_BLOCK_HASH_A "$_ALGO4_HASH"
    state_set PALW_ALGO4_BLOCK_HASH_B "$_ALGO4_HASH"
    state_set PALW_ALGO4_BLOCK "$_ALGO4_HASH"   # legacy alias (kept for compatibility)

    # Capture the block subsidy S from the minted block's OWN coinbase payload — the
    # only coinbase axis observable at mint time. `kaspa-pq-validator get-block` prints
    # `coinbase_subsidy_sompi: <S>`; verify-coinbase.sh derives the full split from S.
    # Requires the FULL 128-hex Hash64 block hash (RPC get-block rejects a short hash).
    _SUBSIDY=""
    if [ "${#_ALGO4_HASH}" -eq 128 ]; then
        _CB_BLOB="$("$VAL" get-block --hash "$_ALGO4_HASH" --node-wrpc-borsh "$(node_wrpc a)" --network "$NETWORK" 2>/dev/null || true)"
        _SUBSIDY="$(printf '%s\n' "$_CB_BLOB" | awk -F': ' '/^coinbase_subsidy_sompi: [0-9][0-9]*$/{print $2; exit}')"
    fi
    if [ -n "$_SUBSIDY" ]; then
        state_set PALW_ALGO4_SUBSIDY_SOMPI "$_SUBSIDY"
        state_set PALW_ALGO4_PREMIUM_PI_BPS "${PALW_ALGO4_PREMIUM_PI_BPS:-10000}"   # neutral (pi = 1.0) default
        state_set PALW_ALGO4_SOURCE_CLASS "${PALW_ALGO4_SOURCE_CLASS:-replica_palw}"
        log "STN-012 coinbase: captured block subsidy S=$_SUBSIDY sompi from the minted block's coinbase payload."
    else
        warn "STN-012 coinbase: could not read coinbase_subsidy_sompi via 'kaspa-pq-validator get-block $_ALGO4_HASH' (node A wRPC) — verify-coinbase.sh will fail-closed on the missing subsidy. Re-run ./verify-coinbase.sh once node A RPC is reachable."
    fi
fi
# SCOPE (5.5): only the subsidy S is captured here. The observed provider A/B,
# §D inclusion and §E validator payouts are NOT in this block's own coinbase — an algo-4
# block's coinbase pays its mergeset (the algo-3 base blocks it merges), and the providers
# are paid only in a LATER block that merges THIS block as a blue ReplicaPalw source (red ->
# providers paid 0, or absent, on the weight-0 wiring fork). verify-coinbase.sh therefore
# verifies S + derives the expected split and reports the provider payouts as DEFERRED
# (honest PASS). Capturing the observed payouts is a follow-up: locate the descendant
# blue-merge block and parse its coinbase (get-block already returns any block's outputs).

_MINER_OK=1
log "STN-012 complete: node A mined algo-4 block $_ALGO4_HASH; BOTH nodes accepted it (StatusUTXOValid, hash-pinned)."
if [ "$TICKET_MODE" = real ]; then
    log "This block carries the verified real-provider Qwen Receipt-v3 projection and an inference-bound ticket (NOT palw_demo)."
else
    log "This is a WIRING-ONLY, non-inference MOCK-TICKET block (NOT palw_demo)."
fi
log "next: ./verify-coinbase.sh (A/B/Inclusion/Validator sompi split for the minted block) and ./verify-consensus.sh (both-node parity)."
exit 0
