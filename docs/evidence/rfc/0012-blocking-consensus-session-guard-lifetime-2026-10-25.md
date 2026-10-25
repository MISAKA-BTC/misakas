# RFC0012 — blocking consensus session guard across awaiter cancellation

Status: RED reproduced, production repaired, unchanged GREEN **3/3 PASS**. See [verification record](0006-0008-0012-queue-session-verification-2026-10-25.md).

## Observed source boundary

The actual helper in `components/consensusmanager/src/session.rs` moves a closure
that only references `self.consensus` into Tokio's blocking pool. The session's
other field, `_session_guard`, protects bulk reads against pruning. Rust's
disjoint closure capture can leave that guard with the awaiting async future.
If that future is cancelled, the Tokio blocking closure can continue after the
guard was dropped. The consensus Arc remaining alive is not itself the session
read lock. This matters for the real head/log retry pump, which now selects its
blocking read against service shutdown.

This was reproduced with the unchanged production helper before fixing it.

## Actual helper/lock controls

New `session_guard_lifetime_tests` uses the real `SessionLock`, its owned reader,
`ConsensusSessionOwned::spawn_blocking`, and the real `RfRwLock::write` future.
An injected minimal ConsensusApi only supplies a sink identity; it does not
simulate lock behavior or manufacture native chain facts.

Three parent tests cover normal return, cancellation and worker panic. A worker
signals that it entered its actual blocking closure, then waits on a synchronous
release channel. The cancellation test aborts and awaits the caller task to
establish that its async future was dropped. While the worker is still held at
its latch, it polls the actual writer exactly once. `Ready` establishes premature
guard release; `Pending` establishes actual lock exclusion without a timed sleep.
The worker dispatches the API again after release, demonstrating that it really
continues independently of the cancelled caller.

Cleanup releases the worker before asserting the expected-red result. RAII also
releases the latch and aborts the awaiter if the harness itself fails. A completion
signal plus actual writer acquisition verifies eventual release, including
unwind. Five-second timeouts only bound a broken harness; elapsed time is not the
ownership observation. No blocking worker is intentionally left parked after a
failed assertion.

## Verification and limits

The baseline manager no-run build passed in 63.09 seconds with frozen source
identities unchanged. The immutable RED binary is
`/tmp/misaka-rfc-queue-session-kaspa_consensusmanager-red-20261025`, SHA-256
`1d0e4317b97170f857f5fae5a7acea17b0d7ed466cf0ac97a5dfaa2a1fb42959`.
All three parents executed: normal return and panic controls PASS; cancellation
FAILS with the exact premature-guard-release assertion. Thus this is an observed
actual-lock defect, not a timeout-based inference. Results and full log:
`/tmp/misaka-rfc-queue-session-red-results-20261025.json` and
`/tmp/misaka-rfc-queue-session-session-red-runtime-20261025.log`.

The production closure now calls `self.with_consensus_api_v1(f)`. The method's
whole-session receiver makes the blocking closure capture the entire owned
session, including `_session_guard`, rather than capturing only the consensus
field. Dropping the async awaiter therefore does not release the worker's guard.
The worker releases it on completion or unwind, while the existing return value
and blocking-panic propagation remain unchanged. No test was weakened or changed
for this correction.

Static Rust2024 parse and `git diff --check` pass. Parent-coordinated GREEN five-package compilation and all three unchanged runtime parents PASS. Exact filter: `session_guard_lifetime_` in
package `kaspa-consensusmanager --lib`.

No public policy/fence, native39 mapping, model tier, claim authentication or
campaign status changes. The tests
establish session locking behavior, not OS resource admission, a full pruning
campaign, or actual EVM block execution.
