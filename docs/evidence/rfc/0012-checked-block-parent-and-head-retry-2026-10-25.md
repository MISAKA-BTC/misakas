# RFC0012 — checked block parents and complete head notifications

Status: coordinated build and dedicated runtime PASS; see [verification record](0006-0012-capacity-parent-head-verification-2026-10-25.md).

## Actual consumers

The node's `block_by_number`, `block_by_hash`, `block_by_tag` and `newHeads`
notification renderer shared a best-effort parent helper. A parent store error
became the all-zero genesis `parentHash`, and the head collector separately
discarded primary block errors using an Ok-only `filter_map`. Consequently a
committed notification could be consumed with one head permanently omitted.

The checked node renderer now propagates both primary block and immutable-parent
read errors. Unknown/non-EVM primary blocks retain `None`; genesis number zero
needs no lookup. A genuine first-EVM boundary returned as `None` by the native
parent API remains a zero parent. The native API defaults to unavailable;
there is no canonical-number fallback on absence or error.

The parent-owned `ConsensusApi::get_evm_parent_l1_hash` supplies the immutable
selected-parent identity rather than the mutable canonical number N−1. Its
native validation and actual cold-store/branch tests are recorded separately by
the parent; this note covers the actual node consumer and notification wiring.

## Retry and ownership

The head stage collects the entire added-block list before sending any heads.
On error it retains the same notification and retries after 250 ms, using the
existing shutdown-aware retry primitive. Each attempt's consensus session is
consumed and released before the delay. The completed head stage is sent once,
then the log stage runs separately: head retries send no logs and log retries
cannot repeat completed heads. Subscriber departure discards the no-longer-
consumed pending stage. Shutdown cancels both read/delay awaits; an already
running Tokio blocking closure is not forcibly interrupted.

## Dedicated tests

Filter `eth_rpc::head_fault_tests` contains four parents:

- Primary error/unknown stop before parent/render; genesis avoids lookup;
  an unsupported native parent API fails instead of producing zero.
- Parent error differs from true boundary absence and exact successful identity;
  the reader panics if canonical-number lookup is attempted.
- A fault in either an added primary or its parent yields no partial head batch;
  genuine non-EVM blocks still skip.
- A recovered head notification publishes its ordered heads once into a real
  broadcast channel; subsequent log-stage retry cannot repeat those heads.

These use the actual production renderer/collector/retry helpers and an injected
ConsensusApi reader, not fabricated accepted EVM maturity or bridge state.
Existing `reorg_retry_tests` separately cover shutdown, subscriber departure,
complete log ordering and retry delay. Runtime results are recorded in the linked verification record.

## Limits

The preexisting unbounded consensus-notification queue can grow while a local
read fault delays subsequent notifications. Bounded broadcast lag and client
reconnect/backfill remain unchanged; this is not durable/exactly-once network
delivery. No public policy/fence, native39 settlement mapping, K3 input, model
tier or selected-verifier claim authentication is changed. Actual Safe/Finalized
mature-work and full RFC0012 campaign completion are not established here.
