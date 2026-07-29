#!/usr/bin/env bash
# Keep one successor PALW batch in flight and hand mining to it once active.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
. "$SCRIPT_DIR/common.sh"
PALW_LOG_TAG=roll-lifecycle
export PALW_LOG_TAG
load_env

: "${PALW_NODE_UNIT:=kaspad-t200.service}"
: "${PALW_BASE_MINER_UNIT:=kaspa-t200-miner.service}"
: "${PALW_MINE_ENV:=$PALW_DATA_ROOT/palw-mine.env}"
: "${PALW_ROLL_TARGET_MOD_MIN:=20}"
: "${PALW_ROLL_TARGET_MOD_MAX:=35}"
: "${PALW_ROLL_VERIFY_TIMEOUT_SECS:=300}"
: "${PALW_AUX_MINER_UNITS:=}"

[ "$(id -u)" -eq 0 ] || die "roll-lifecycle must run as root (it updates systemd PALW mining state)."
require_cmd systemctl journalctl sed flock mv chmod date sleep

LOCK_FILE="$PALW_DATA_ROOT/.roll-lifecycle.lockfile"
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
    log "another lifecycle roll is already running; exiting idempotently."
    exit 0
fi

success=0
start_aux_miners() {
    local unit units=()
    read -r -a units <<<"$PALW_AUX_MINER_UNITS"
    for unit in "${units[@]}"; do
        systemctl start "$unit"
    done
}
stop_aux_miners() {
    local unit units=()
    read -r -a units <<<"$PALW_AUX_MINER_UNITS"
    for unit in "${units[@]}"; do
        systemctl stop "$unit"
    done
}
enable_aux_miners() {
    local unit units=()
    read -r -a units <<<"$PALW_AUX_MINER_UNITS"
    for unit in "${units[@]}"; do
        systemctl enable --now "$unit"
    done
}
cleanup() {
    rc=$?
    trap - EXIT INT TERM
    if [ "$success" -ne 1 ]; then
        warn "roll did not complete; restoring the persistent base miner for chain liveness."
        PALW_ENV_FILE="$PALW_ENV_FILE" "$SCRIPT_DIR/supporting-miner.sh" stop >/dev/null 2>&1 || true
        systemctl start "$PALW_BASE_MINER_UNIT" >/dev/null 2>&1 || true
        start_aux_miners >/dev/null 2>&1 || true
    fi
    exit "$rc"
}
trap cleanup EXIT INT TERM

status_of() {
    local batch_id="$1"
    palw_batch_status a "$batch_id" 2>/dev/null | _kv batch.status
}

current_batch="$(state_get PALW_BATCH_ID 2>/dev/null || true)"
current_status=""
if [ -n "$current_batch" ]; then
    current_status="$(status_of "$current_batch" 2>/dev/null || true)"
fi
log "local lifecycle state: batch=${current_batch:-none} status=${current_status:-missing}"

# Never run two independent base miners. A failed prior attempt may have left the
# harness miner alive, so normalize to the persistent systemd miner first.
PALW_ENV_FILE="$PALW_ENV_FILE" "$SCRIPT_DIR/supporting-miner.sh" stop >/dev/null 2>&1 || true
systemctl start "$PALW_BASE_MINER_UNIT"
start_aux_miners

case "$current_status" in
    registering|committed|auditing|certified)
        log "resuming the existing in-flight successor $current_batch."
        systemctl stop "$PALW_BASE_MINER_UNIT"
        stop_aux_miners
        ;;
    *)
        # Registering near an epoch boundary is invalid. Let the persistent miner
        # reach a fresh epoch's early window, then freeze DAA before authoring.
        log "waiting for a fresh registration window (DAA mod 100 in [$PALW_ROLL_TARGET_MOD_MIN,$PALW_ROLL_TARGET_MOD_MAX])."
        deadline=$(( $(date +%s) + 900 ))
        while :; do
            daa="$(node_sink_daa a)" || die "cannot read node-a DAA while scheduling the successor."
            mod=$(( daa % 100 ))
            if [ "$mod" -ge "$PALW_ROLL_TARGET_MOD_MIN" ] && [ "$mod" -le "$PALW_ROLL_TARGET_MOD_MAX" ]; then
                systemctl stop "$PALW_BASE_MINER_UNIT"
                stop_aux_miners
                log "registration window pinned at DAA=$daa (mod=$mod)."
                break
            fi
            [ "$(date +%s)" -lt "$deadline" ] || die "timed out waiting for a fresh registration window."
            sleep 5
        done
        PALW_ENV_FILE="$PALW_ENV_FILE" LIFECYCLE_FORCE=1 "$SCRIPT_DIR/create-lifecycle.sh"
        # create-lifecycle runs in a child process, so its state_set updates the
        # persisted state.env but cannot update this shell's already-exported
        # PALW_BATCH_ID. Reload the authoritative file before continuing.
        # shellcheck disable=SC1090
        . "$(state_file)"
        current_batch="${PALW_BATCH_ID:-}"
        [ -n "$current_batch" ] || die "create-lifecycle completed without persisting PALW_BATCH_ID."
        current_status="$(status_of "$current_batch" 2>/dev/null || true)"
        log "authored successor batch=$current_batch status=${current_status:-not-yet-carried}."
        ;;
esac

PALW_ENV_FILE="$PALW_ENV_FILE" "$SCRIPT_DIR/submit-lifecycle.sh"
[ "$(status_of "$current_batch")" = active ] || die "successor $current_batch is not active on node-a after submission."
[ "$(palw_batch_status b "$current_batch" 2>/dev/null | _kv batch.status)" = active ] \
    || die "successor $current_batch is not active on node-b after submission."

# submit-lifecycle leaves the harness miner running. Freeze it before restarting
# node A with the new leaf, then restore the persistent systemd base miner.
PALW_ENV_FILE="$PALW_ENV_FILE" "$SCRIPT_DIR/supporting-miner.sh" stop
mine_tmp="${PALW_MINE_ENV}.tmp.$$"
umask 077
printf 'PALW_LEAF=%s:0\n' "$current_batch" >"$mine_tmp"
chmod 0600 "$mine_tmp"
mv "$mine_tmp" "$PALW_MINE_ENV"

started_at="$(date +%s)"
systemctl daemon-reload
systemctl restart "$PALW_NODE_UNIT"
wait_rpc_up a 120 1 || die "node-a RPC did not recover after switching to successor $current_batch."
systemctl enable --now "$PALW_BASE_MINER_UNIT"
enable_aux_miners

deadline=$(( $(date +%s) + PALW_ROLL_VERIFY_TIMEOUT_SECS ))
while :; do
    if journalctl -u "$PALW_NODE_UNIT" --since "@$started_at" --no-pager \
        | grep "mined + submitted algo-4 block" >/dev/null; then
        break
    fi
    [ "$(date +%s)" -lt "$deadline" ] || die "no algo-4 block was submitted after switching to successor $current_batch."
    sleep 2
done

success=1
log "SUCCESS: successor $current_batch is active on A/B, installed for PALW mining, and produced an algo-4 block."
