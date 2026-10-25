# RFC06 court admission construction workspace — 2026-10-25

## Status

Coordinated build and three focused parents PASS; see [final verification](0006-0007-0008-0012-court-aux-provider-round-verification-2026-10-25.md).
Starting implementation HEAD was `49cfec978`. No deployment, fence arming or
policy approval was performed.

## Actual path

`court_ingress_admission_v1` calls `court_admission_v1` before charging the once-only
fee. Real `CourtOpened` and named-leaf dissection admission use that path; their
session bookkeeping calls `court_opened_v1`. The active-capacity path constructed
a deep dispute-record clone, both raw `court_pools_v1` results, selected queue/release
clones and `palw_court_schedule_v1` scratch before individually admitted writers.
The fee's `slash_bond` also clones its input bond before the bond writer.

The new admission workspace is admitted into the transition construction owner
before those allocations. Semantic admission checks first borrow the record;
missing reservation, bond/floor, existing ticket/queued/running and hard-deadline
checks keep their order. Existing-row admission no longer copies a record that
it only reads. New-row admission retains the original raw pool and scheduler
functions, so pool classification, queue order, deadline feasibility, escrow and
fees remain authoritative there.

The funded-original membership test now uses a borrowed iterator with the exact
same open/unsettled predicate shared by the historical set collector. The admission
caller no longer builds that set merely to ask whether one claim is present.
Other users of the historical set helper are unchanged.

## Derived envelope and lifetime

The request uses actual borrowed dispute rows, each pool's ticket/guaranteed/
best-effort counts, actual court-session count, the selected claim's real record
and the real opener bond. It covers:

- the deep record copy and an additional court-map membership;
- the fee input bond copy when a private ingress schedule is present;
- reserve-running session-set memberships and both pools' release/queue buffers;
- the selected guaranteed queue clone plus the new candidate and release clone;
- scheduler release heap, its usable free-slot upper bound, ordered index/reference
  array, full-size stable-sort scratch and scalar grant array.

Each possible old/replacement growing `Vec` has a separate existing typed Vec
capacity envelope. Session releases are conservatively counted once per possible
pool without allocating a classification set to quote that very set. The selected
pool's clone uses the maximum of the two actual pool counts; heap slots are bounded
by actual release count plus candidate-queue length. These are type-sized
conservative bounds, not serialized-byte ratios, minimal costs or physical RSS.

The admitted construction owner stays live while those raw functions execute and
while the resulting record/fee entries remain in the builder. Individual row and
journal writer reservations are unchanged. `court_opened_v1` separately admits the
actual updated record copy before its handoff clone, still retaining the writer's
separate admission. Existing output splitting determines the lifetime of returned
state/delta objects.

A refusal of the new admission request occurs before a new row, fee or journal
write. Public object/transition failure does not publish a partially mutated
builder. This does not claim that a later writer/physical-allocation failure
rolls back private builder mutations in place.

## Focused controls (runtime PASS)

Package `kaspa-consensus-core`, lib target, filter:

```text
palw_state_v2::tests::court_admission_workspace_v2
```

Three parents use a real folded registry and attempt, with an explicitly stored
capacity reservation book and private placeholder capacity/fee fixtures:

1. An occupied spare pool, a guaranteed queue and a best-effort queue force actual
   pool construction and scheduler execution. With a builder already constructed,
   capacity 0 and Q-1 fail at the target admission with identical state/root/journal
   and no ingress fee. Q is caller scratch only. A sufficient-capacity retry queues
   a guaranteed row and burns the private fee once; repeated queued admission does
   not burn or journal a second fee. Construction tickets release on builder drop.
2. The same active policy with local tracking off/on gives identical admission,
   state, root and journal Borsh bytes. Actual delta replay/undo restores roots.
3. An empty pool admits an open row, followed by actual session-row bookkeeping.
   Zero capacity at that handoff refuses before copying/writing the row. Retry
   records the session without another fee. Below the capacity fence admission
   and handoff need no allocation or payment under a zero checker.

The synthetic book fixture exercises admission/fee/scheduler and handoff mechanics;
it is not a full authenticated court verdict, release/campaign or production policy
approval. The fee schedule is test-only and reset by an RAII guard. Initial fixture
construction and parent copying are outside the target checker, so they cannot
stand in for the measured admission refusal.

## Remaining scope

This closes the active-capacity admission phase and its record handoff, not every
court caller. Legacy/paid-capacity admission outside this active-capacity route,
other raw funded-set consumers, public read-only pool callers, diagnostic strings
and unrelated auxiliary consumers remain separate work. Existing sweep reservation
and writer/journal contracts are not replaced. All-model cost bounds, K3 formal
inputs, the unformulated native39 settlement mapping and the UNKNOWN campaign gates
are unchanged. Public activation remains None; normal bootstrap/policy refusals,
model tiers and selected-verifier claim approval remain in force.
