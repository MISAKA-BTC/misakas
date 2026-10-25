# RFC-0006 / RFC-0008 vesting-to-payout construction workspace

Status: scoped implementation and runtime controls PASS. Builds and executions were coordinated by the root agent; this worker ran no Cargo.

## Actual consumer and ownership

The production `apply_vesting_maturity` path calls `latch_matured_vesting_rows`, builds the existing `palw_vesting_mint_plan_v1`, and feeds its moves to `apply_vesting_move`, which writes the pending payout queue and removes the released entitlement. The existing keyed writers admitted their own copies/index/journal allocations, but that did not admit the caller's earlier candidate vectors, move/leg vectors, key sets, cloned vesting row, recipient map, or the moved note's incoming leg clone.

The new private `vesting_workspace_v2` quotes these actual typed roles before the first latch copy. It derives counts from both actual unlatched indexes, vesting rows and their seat lengths, and reporter reward rows. It counts every possible source row, including the first non-fitting move; it does not construct a plan to discover its own quote or duplicate the planner's maturity/budget rules. This is deliberately a conservative full-source envelope, including rows a particular block may not move, rather than a minimal allocation measurement.

Three independent candidate/walk vectors, the outer move vector and each inner leg vector receive typed Vec growth envelopes: four slots per final element, minimum eight for a nonempty vector, covering the pinned old/new push-buffer coexistence. Reporter `vec![leg]` and note copies are exact typed slots. `next.new_keys`, accumulated created keys and per-row recipient aggregation use the pinned BTree node envelope. Actual row clone heap is measured from the borrowed native row, not its serialized size. Unknown native heap, unsupported BTree layout and checked request overflow return `NativeResource` before the maturity consumer mutates state or its journal.

Admission uses the existing transition construction owner. That reservation stays alive across latch, planner, recipient aggregation, queue writes, entitlement removal and note journal insertion. It is conservatively retained until transition output tickets are split; unused construction requests retire at finish. In particular, incoming `Moved` note leg copies have already been admitted when `note_vesting` takes them, and the construction ticket cannot disappear before their journal/output ownership handoff. Existing writer reservations remain necessary for the distinct state/index/journal copies they construct.

Tracking off returns before quote computation and runs the historical functions unchanged. No public planner, wire representation, maturity/budget ordering, settlement mapping, model coefficient, selected approval rule or activation fence changes.

## Verification

Package: `kaspa-consensus-core`, lib test filter:

```
palw_state_v2::tests::vesting_workspace_v2
```

Four parent tests exercise the real maturity consumer on a small deterministic native state fixture with three vesting rows, a shared Panel payee, reserve legs and a reporter entitlement:

- Measure the actual phase quote with an independent native checker; zero and Q−1 refuse at the consumer before any latch, state/root or journal change. Q admits the workspace itself. A fresh sufficient checker retries the same source through actual latch, planning, queue and note writes; the construction lease remains charged until the builder drops and then fully releases.
- Compare enabled construction with flags-off state/root and complete journal bytes; verify exact payout amount, common-payee accumulation, reserve amount and entitlement removal. The public production pre-object path then drains the actual queued prefix which the next block's coinbase pays.

The two additional activation-pool parents exercise the actual `apply_activation_pool_flush_v1` consumer. It now admits its move vector, fresh-key set, by-class aggregate and borrowed pool row clones before collection. Counts come from the actual scheduled map; class grouping follows its existing `(class,payee)` BTree order without an allocating dedup set. The quote includes each scheduled class's actual pool vectors, not a serialized ratio. Twelve classes sharing one new payout key deliberately exceed the eight-output drain width, verifying that width is not misused as a bound on source rows. Q−1 refuses before any state/root/journal change, Q admits the workspace, and retry releases all scheduled amounts into the shared queue row. The combined maturity→flush test holds both phase reservations, compares complete flags-off state/root/journal bytes and drains the six resulting payout rows through the real next-block pre-object API.

These four tests passed on the fixed production Core binary described below. A test fixture is not a completed authenticated claim campaign or a new settlement policy. The fixture exercises the existing dormant/default settlement branch; funded native income or settlement-once fences are not armed to manufacture authority.

## Remaining boundaries

This closes only the caller workspace of the vesting maturity path. The raw public planner/RPC ownership boundary, other payout/court/aux callers, pre-object payout-prefix temporary allocation, transport/serialization copies and full model/prefix construction remain separate. The full-source quote may conservatively refuse a large live book even when this particular block would move few rows. No RSS or production cost bound, K3 artifact, multi-process campaign or 15-gate PASS is claimed. Native39-to-RFC12 settlement mapping remains explicitly unformulated by the user; no mapping is introduced here.


## Recorded actual results

The final five-package lib no-run build passed in 158.27 seconds, including dependency and lock work, with all 31 recorded source/config identities matching. This duration is build evidence, not a payout or model cost benchmark.

- Source record: `/tmp/misaka-rfc-payout-provider-market-wave-source-retry3-20261025/record.json`.
- Immutable Core binary: `/tmp/misaka-rfc-payout-provider-market-kaspa_consensus_core-retry3-20261025`.
- SHA-256: `aaa7b055ead002922e7f3d21ee5d041c6dff5e00d9e1f3f0a4b16ddec92a7593`.
- New workspace parents: **4/4 PASS**, harness 0.03 s (controller wall 2.28 s), `/tmp/misaka-rfc-payout-provider-market-payout-20261025.log`.
- Same-pin existing `vesting_fold_v1`: **25/25 PASS**, 0.33 s, `/tmp/misaka-rfc-payout-provider-market-vesting-regression-20261025.log`.
- Same-pin existing activation R1: **6/6 PASS**, 0.15 s; R2: **3/3 PASS**, 0.01 s; activation v1: **25/25 PASS**, 0.12 s. Logs: `/tmp/misaka-rfc-payout-provider-market-activation-{r1,r2,v1}-regression-20261025.log`.
- Same-pin existing settlement-once: **21/21 PASS**, 0.03 s, `/tmp/misaka-rfc-payout-provider-market-settlement-once-regression-20261025.log`.

Focused controller: `/tmp/misaka-rfc-payout-provider-market-focused-results-20261025.json`; related controller: `/tmp/misaka-rfc-payout-provider-market-regression-results-20261025.json`; immutable pin registry: `/tmp/misaka-rfc-payout-provider-market-pins-20261025.json`. Repeated earlier runs of these same four parents are not additional independent tests. Wider remote-library fixture failures and their corrections belong to that lane's records, not a failure or success claim for this payout scope. The root common verification record separately tracks the entire combined build and regressions.

## Final coordinated checkpoint

Retry4 compiled with 33 identities matching and all 14 dedicated parents passed. All 13 related groups passed, with 375 distinct passing parents across overlapping selections; four ignored parents are excluded. Private-node and non-EVM checks passed before only the two Remote test-only repairs. Earlier pending statements describe earlier checkpoints. [Final pins, failures, counts and scope](0006-0008-0012-payout-provider-market-verification-2026-10-25.md).
