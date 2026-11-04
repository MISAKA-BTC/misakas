#!/bin/bash
set -eu
printf '%s\n' "$$" >/tmp/misaka-positive-efb8862fc-owner.pid
while kill -0 1462423 2>/dev/null; do sleep 5; done
test "$(cat /tmp/misaka-positive-f9a4a9c80-batch.exit)" = 0
cd /dev/shm/misaka-oom-positive-next
test "$(git rev-parse HEAD)" = f9a4a9c80dbe64657863a9d22f85b97db1eed198
test -z "$(git status --porcelain)"
git fetch /tmp/misaka-positive-efb8862fc.bundle codex/positive-oom-integration:refs/heads/candidate-efb8862fc
git switch --detach candidate-efb8862fc
git rev-parse HEAD >/tmp/misaka-positive-efb8862fc-source.txt
export PATH=/root/.cargo/bin:$PATH
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/dev/shm/misaka-positive-pre-isolated-target
run_stage() {
 stage=$1
 shift
 set +e
 "$@" >"/tmp/misaka-positive-efb8862fc-$stage.log" 2>&1
 result=$?
 printf '%s\n' "$result" >"/tmp/misaka-positive-efb8862fc-$stage.exit"
 set -e
 test "$result" = 0 || exit "$result"
}
run_stage clippy cargo +1.93.0 clippy --offline --locked -p kaspa-consensus-core -p kaspa-consensus --features evm --lib -- -D warnings
run_stage check cargo +1.93.0 check --offline --locked -p kaspa-consensus-core -p kaspa-consensus --features evm --tests
run_stage source-work cargo +1.93.0 test --offline --locked -p kaspa-consensus-core --features evm --test positive_assignment_resources_v2 source_preparation_covers_both_children_at_every_u32_merkle_level -- --nocapture
