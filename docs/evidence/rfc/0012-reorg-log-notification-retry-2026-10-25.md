# RFC0012 — preserve a reorg log notification on native read failure

Status: implementation and four focused notification-consumer controls PASS.

## Existing contract and production defect

`rpc/eth/src/lib.rs::EthProvider::subscribe_logs` and `EthLogEvent` define the
existing stream order: detached blocks oldest-first, followed by attached blocks
oldest-first, preserving each block's log indices. The node pump consumes real
`VirtualChainChanged` notifications and reads immutable accepting-L1 hashes. It
must include detached blocks, so canonical-number substitution is inappropriate.

Before this change, `NodeEthProvider::spawn_head_pump` converted
`get_evm_block_logs` errors into empty blocks with `unwrap_or_default`. A failed
detached read could therefore be followed by successful attached events, and the
notification was permanently consumed. Checked native store errors from the
previous exact-log reader work were lost at this actual subscriber consumer.

## Change

`kaspad/src/eth_rpc.rs` collects a fallible complete reorg batch. It publishes no
log events until every immutable read succeeds, retaining the same notification
and retrying after 250 ms on error. Each attempt uses a fresh consensus session;
no session is held during the delay. Later notifications cannot overtake this
pending batch. A successful empty block remains empty. Heads from the current
notification are sent once before log retries, rather than repeatedly on retry.

The service's existing shutdown listener now reaches both the notification wait
and retry read/delay. Cancellation stops awaiting a blocking read; an already
running Tokio blocking closure is not forcibly interrupted. Loss of all log
subscribers drops the pending batch on the next check, without a retry loop.

## Dedicated controls

New node filter `eth_rpc::reorg_retry_tests` contains four parent tests:

- ConsensusApi-shaped immutable read errors at each detached/attached position
  produce no partial batch; genuine successful empty reads remain successful.
- Recovery rereads the same hashes, waits rather than busy-looping, and returns
  exactly one complete ordered batch before the caller proceeds.
- Subscriber departure ends retry without another read.
- Shutdown interrupts both a pending read and a delayed retry.

These are production notification-helper controls with a fault-injected API
reader, not a new actual accepted bridge transaction/reorg execution fixture.
Existing native cold-store malformed-row controls establish the upstream typed
errors separately.

## Verified execution

The parent-coordinated four-package no-run build passed in 283.15 seconds with
all 19 recorded source/configuration identities unchanged. The frozen node
binary is `/tmp/misaka-rfc-deadline-legacy-reorg-kaspad_lib-20261025`, SHA-256
`875e1b0fe87bb3cb73ceb4a8e2b24c3ae55a7f5248580d940ec17ffa40fa54ce`.
All four dedicated parents passed (0.01 seconds harness, 4.07 seconds wall),
with no ignored tests. The log is
`/tmp/misaka-rfc-deadline-legacy-reorg-reorg-20261025.log`.

Related fixed-binary checks also passed: old node reorg-ordering tests 2/2,
node exact-hash log controls 3/3, whole `kaspa-eth-rpc` crate 31/31, and actual
native cold-store/exact-side-hash/range log controls 5/5. These are separate
filters, not additional executions of the four new controls. Source, pin and
result manifests use the `/tmp/misaka-rfc-deadline-legacy-reorg-` prefix;
the source record is `wave-source-20261025/record.json`.

See the [combined verification record](0006-0012-deadline-legacy-reorg-verification-2026-10-25.md)
for the coordinated build, exact filters and complete related results. The
verified scope is notification retry and existing reader/adapter compatibility,
not a completed bridge execution or historical-signature campaign.

## Limits

This does not establish durable/exactly-once WebSocket delivery, an OS-crash
campaign, Safe/Finalized positive settlement, or RFC0012's full campaign. The
existing notifier queue is unbounded; a persistent read fault can delay later
heads/logs and grow its backlog. Existing bounded broadcast lag and client
reconnect/backfill behavior are unchanged. NewHeads read-error handling and its
best-effort parent field are outside this change. Pruned missing immutable rows
retain the existing native reader's empty/absence contract. No new native39
settlement mapping, K3 input, policy, activation height, model review, or claim
authentication rule is introduced.

## Final coordinated retry1

All ten new parents and eleven related groups passed after the existing readiness test input correction. Earlier pending/failure checkpoints remain historical. [Final source manifest, pins and 153 distinct passing parents](0006-0012-deadline-legacy-reorg-verification-2026-10-25.md). Scope limits above remain in force.
