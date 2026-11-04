#!/bin/bash
set -eu
printf '%s\n' "$$" >/tmp/misaka-positive-f9a4a9c80-owner.pid
while kill -0 1449561 2>/dev/null; do sleep 5; done
for previous_stage in clippy check generation relay; do
  test "$(cat /tmp/misaka-positive-50d5eda4c-$previous_stage.exit)" = 0
done
cd /dev/shm/misaka-oom-positive-next
test "$(git rev-parse HEAD)" = 50d5eda4cd92c506eef20b042a6f85b505fd4877
test -z "$(git status --porcelain)"
git fetch /tmp/misaka-positive-f9a4a9c80.bundle codex/positive-oom-integration:refs/heads/candidate-f9a4a9c80
git switch --detach candidate-f9a4a9c80
git rev-parse HEAD >/tmp/misaka-positive-f9a4a9c80-source.txt
export PATH=/root/.cargo/bin:$PATH
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/dev/shm/misaka-positive-pre-isolated-target
set +e
cargo +1.93.0 test --offline --locked -p kaspa-consensus-core --features evm --lib batch_native_dispatch_opens_and_finishes_the_same_rooted_demand -- --nocapture >/tmp/misaka-positive-f9a4a9c80-batch.log 2>&1
result=$?
printf '%s\n' "$result" >/tmp/misaka-positive-f9a4a9c80-batch.exit
exit "$result"
