# RFC-0006 court capacity sweep caller workspace

Status: coordinated build and dedicated runtime PASS; see [verification record](0006-0012-capacity-parent-head-verification-2026-10-25.md).

## Connected native consumer

The existing dispute tick calls `sweep_court_capacity_v1` before block objects. Under its existing explicit court-capacity mirror, it collects ended sessions/expired tickets, settles their occupancy fees through the existing bond writer, removes their court rows, reconstructs both occupancy pools, and grants queued prosecutions tickets in the original priority order.

That consumer constructed an `ended` Vec, cloned dispute records, allocated reserve/spare release/guaranteed/best-effort Vecs and the reserve-session BTreeSet, merged the reserve queues, and collected grants before its later keyed writers could admit those caller allocations. An expired ticket's occupancy settlement also called `slash_bond`, which cloned the actual registry bond before the bond writer's own reservation.

The new private sweep workspace is admitted immediately after the existing active-policy gate and before the first collection or fee. It derives all counts from actual borrowed dispute court rows and current court sessions. Each current dispute record's clone heap and each actual reserver bond's clone heap are included. Typed Vec envelopes cover the individual source buffers, reserve-queue merge and final grants, including old/new growth coexistence (four slots per maximum element, minimum eight for nonempty Vecs). The reserve-session set uses the pinned typed BTree membership envelope. Each live session is conservatively counted as a possible release in each pool instead of allocating the classification set to discover its own quote.

The initial rows form an upper envelope after ended rows are removed; the actual sweep does not insert new queued rows. Quote counts therefore need no duplicate scheduler or policy rule. All arrays, bookkeeping copies and native classification remain the unchanged original algorithm. Existing writers still separately admit their distinct state/index/journal output allocations.

The transition construction owner holds the extra workspace across occupancy charge, row removal, pool construction, sorting, merge, ticket grant and journal/output handoff; unused requests retire at output splitting. Refusal is the typed local `NativeResource`. A whole-phase Q failure occurs before any fee, state/root or journal change. Tracking off bypasses quote work; an inactive court-capacity fence returns before the new reservation.

## Prepared actual-consumer controls

Package `kaspa-consensus-core`, lib filter:

```
palw_state_v2::tests::court_capacity_workspace_v2
```

Three parents are prepared:

- Q is measured from the actual stored source. Zero/Q−1 refuses the real sweep before occupancy billing or row mutation; Q admits the workspace itself; a sufficient fresh checker retries through actual expired-ticket fee debit, court-row removal and ticket grant. The owner remains charged through that consumer and releases on builder drop.
- The unchanged scheduler grants a reserve queue entry and a guaranteed spare entry while keeping the older best-effort spare entry queued. Enabled and flags-off state/root and complete journal bytes agree, and existing delta replay/revert match.
- A dormant mirror with zero checker changes no state/journal and charges nothing.

The registry and claim are generated through the existing native fold helpers. Court scheduling rows are explicit stored-book fixture inputs. The placeholder capacity policy is the existing test mirror, with its slot counts checked using the actual reserve/spare formulas. This is not adoption of that policy, signed admission of those scheduling rows, a new approved capacity record, claim credit, or a full native court-funding campaign. Occupancy rows carry explicit frozen fee/escrow fields; no production defaults or policy replacements are installed.

## Remaining scope

Court admission uses the same raw occupancy helper elsewhere and remains a separate caller boundary, as do release/service-book lifecycle construction, broader aux/control serializers, replay and transport. This patch does not claim every internal court/bond consumer is now bounded. The all-source sweep quote can conservatively exceed the minimal work of a particular tick. No RSS/model cost/K3/15-gate completion is inferred. Activation, model tiers, selected-verifier approval, native settlement mappings and signed/wire data remain unchanged.
