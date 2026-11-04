#!/bin/bash
set -eu
printf '%s\n' "$$" >/tmp/misaka-positive-5917693dd-owner.pid
while kill -0 1467362 2>/dev/null; do sleep 5; done
for previous_stage in clippy check source-work; do
 test "$(cat /tmp/misaka-positive-efb8862fc-$previous_stage.exit)" = 0
done
cd /dev/shm/misaka-oom-positive-next
test "$(git rev-parse HEAD)" = efb8862fcca43b088279847c4a8ed91714f8d492
test -z "$(git status --porcelain)"
git fetch /tmp/misaka-positive-5917693dd.bundle codex/positive-oom-integration:refs/heads/candidate-5917693dd
git switch --detach candidate-5917693dd
git rev-parse HEAD >/tmp/misaka-positive-5917693dd-source.txt
export PATH=/root/.cargo/bin:$PATH
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/dev/shm/misaka-positive-pre-isolated-target
run_stage() {
 stage=$1
 shift
 set +e
 "$@" >"/tmp/misaka-positive-5917693dd-$stage.log" 2>&1
 result=$?
 printf '%s\n' "$result" >"/tmp/misaka-positive-5917693dd-$stage.exit"
 set -e
 test "$result" = 0 || exit "$result"
}
run_stage check cargo +1.93.0 check --offline --locked -p kaspad --features evm,positive-miner-client --tests
run_stage native-refusal cargo +1.93.0 test --offline --locked -p kaspad --features evm,positive-miner-client --lib native_refusal_cannot_pin_a_result_or_journal_even_across_cold_reopen -- --nocapture
