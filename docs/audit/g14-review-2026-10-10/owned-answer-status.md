# Session-owned multipart DA answer checkpoint — 2026-10-11

This checkpoint implements the dormant consensus transport foundation for G14. It does not complete G14 and does not arm the new objects. ADR-0177 D7 continues to assume a verifier holding the correct registered model.

## Implemented

Objects 160/161 carry a signed manifest and authenticated parts owned by an existing, unfulfilled public DA session. They use separate pending-answer state rather than the eight certification groups. Admission fixes the deadline to the existing session, bounds allocation before signing/serialization, rejects replacement and malformed parts, and permits completion only after checking the assembled original answer signature and the existing court authentication. State roots, snapshot tail, apply/revert journal, cleanup and carrier classification include the pending answers.

## Verification at this checkpoint

- 59 unique core/node tests passed: the earlier 38 core, 15 legacy node, three carrier and one real MLDSA gate tests; one additional maximum-32-part boundary test; and one repaired V-fold completion/default/restart regression. Repeated tests are not counted again.
- The maximum-32-part test checks each carrier fits, reverse arrival completes exactly once, and a one-byte-smaller envelope budget fails before signing.
- The V-fold regression exercises all eight occupied certification groups, partial snapshot restart, malformed snapshot rejection, duplicate/corrupt parts, direct-answer cleanup, multipart completion and withholding default with producer forfeiture. It also checks that an overly fragmented manifest is refused when the existing DA window cannot carry it.
- Shipping integration harvest completed successfully. The mechanical shipping comparison reports 361 pins OK, no drift, all required gates passed and no unregistered literals. These results are recorded in [shipping tests](evidence/owned-answer-shipping-tests.log) and [pin comparison](evidence/owned-answer-shipping-pins.log).
- Final core/node test builds and the three-part test log are preserved alongside the prior check, gate and regression logs. Initial compiler/test failures are preserved as historical evidence; corrected runs are explicitly named repaired/boundary/window.
- The H1 gate uses real MLDSA signatures but synthetic pending-answer state. It is not an end-to-end real service proof of this multipart transport. The V-fold test uses its existing licensed fixture; it is not a maximum-profile service benchmark.

## Remaining implementation

1. Producer automatic splitting, scheduling and restart continuation against accepted session progress.
2. Bounded reconstruction of authenticated multipart answers by a fresh verifier reading public history; missing/pruned history must lead to the existing reacquisition court.
3. Full admission/profile alignment for maximum job/input sizes and court byte/time/memory budgets. The existing 64 KiB job limit is unchanged, and 32 parts are not a proof of coverage for every active profile.
4. Initial/prefix state and non-prompt material, all active computation/output/state families through normal admission, cumulative scope/hiding, inclusion/resource bounds, reorg/pruned IBD/RPC/P2P and activation/economic/soundness gates.

Until those conditions are closed, arming remains fail-closed and full G14 remains unfinished.
