#!/bin/bash
set -eu
printf '%s\n' "$$" >/tmp/misaka-positive-50d5eda4c-owner.pid
while kill -0 1435764 2>/dev/null; do
    sleep 5
done
cd /dev/shm/misaka-oom-positive-next
test "$(git rev-parse --short HEAD)" = 806a07f13
test -z "$(git status --porcelain)"
git switch --detach candidate-50d5eda4c
git rev-parse HEAD >/tmp/misaka-positive-50d5eda4c-source.txt
export PATH=/root/.cargo/bin:$PATH
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/dev/shm/misaka-positive-pre-isolated-target
run_stage() {
    verification_stage=$1
    shift
    set +e
    "$@" >"/tmp/misaka-positive-50d5eda4c-$verification_stage.log" 2>&1
    verification_result=$?
    printf '%s\n' "$verification_result" >"/tmp/misaka-positive-50d5eda4c-$verification_stage.exit"
    set -e
    if test "$verification_result" -ne 0; then
        exit "$verification_result"
    fi
}
run_stage clippy cargo +1.93.0 clippy --offline --locked -p kaspa-consensus-core -p kaspa-consensus --features evm --lib -- -D warnings
run_stage check cargo +1.93.0 check --offline --locked -p kaspa-consensus-core -p kaspa-consensus --features evm --tests
run_stage generation cargo +1.93.0 test --offline --locked -p kaspa-consensus --features evm --lib lkg_generation_tests -- --nocapture
sha256sum "$CARGO_TARGET_DIR/debug/deps/kaspa_consensus-a625c93f0f85ef22" >/tmp/misaka-positive-50d5eda4c-runtime.sha256
quota_directory=$(ls -dt /tmp/misaka-relay-quota.* | head -1)
if mountpoint -q "$quota_directory/mnt"; then
    exit 125
fi
mount -o loop,prjquota "$quota_directory/disk" "$quota_directory/mnt"
trap 'umount "$quota_directory/mnt"' EXIT
export TMPDIR="$quota_directory/mnt/test"
run_stage relay cargo +1.93.0 test --offline --locked -p kaspa-consensus --features evm --lib positive_initial_relay_burns_once_through_the_native_node -- --nocapture
