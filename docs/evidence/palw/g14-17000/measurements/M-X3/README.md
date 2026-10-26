# M-X3 — measured deadline budget status

**NOT_MET.** No V10 term is promoted to an arming value. The existing passive t12 observation [2026-10-19-t12-passive-daa-blocks.md](../../../../../measurements/2026-10-19-t12-passive-daa-blocks.md) reports a median 184 s per DAA and observed maximum 111,949 transaction bytes per block. These are conversion inputs, not measurements of a reserved court carrier lane or a worst-case inclusion delay.

The SPINE checkpoint `ebb8b911f` identifies the slice as card 1's testnet-12 genesis class and the single-prompt decode job from `canonical::claimed`. This selects the fixture family, but it does not pin a real artifact or maximum legal profile. Required run: fix those inputs, then perform the complete cold material fetch, deterministic check, every localization round, disclosure/court, carrier submission and observed inclusion, and reorg replay on the supported minimum-specification node. Record each stage's warm-up and at least 20 repetitions, traffic conditions, byte totals, load, and a nonzero retention price from the actual storage economics. Convert each duration to DAA with the observed 184 s/DAA only as an explicitly labeled estimate; the accepted bound must cover observed tails and congestion. The five `B_*` terms and `c_retain` remain blank until those measurements exist. Do not fill any term with zero or carry forward the old candidate 5,520 DAA as a measurement.

The arming gate in `consensus/core/src/palw_g14_gates_v1.rs` additionally requires X3 fold evidence, node E2E, and a shipped-binary fence crossing with restart/reorg. This document supplies none of those gate digests.

## Scope and inclusion review (continuation)

At inspected source `2119c943b`, `PALW_T12_KERNEL_SLICE_FENCES_V1` explicitly includes `PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY` (`full_activation_v1.rs:309`). Therefore the VAL report's statement that X3 can be postponed does **not** describe the current slice list. M-X3 remains required for that list. Neither the older 5,520-DAA scenario nor the later provisional Δ=2,016 is a measurement.

The [inclusion calculation review](../approval-material.md) also rejects treating the 4-DAA fluid-byte estimate as a demonstrated default-liability bound. Measure actual carrier serialization, priority/fragmentation, backlog and reorg before adopting the premise.

## Explicit interim parameter judgment

Retain X3 in the current slice and retain its existing **provisional** policy candidate: cold/check/localize/carrier/reorg = **512/512/512/120/360 DAA**, Δ = **2,016 DAA**, retention = **36,058 sompi/DAA**. This is the choice to keep an estimated candidate, not to remove the deadline fence or to mark the measured requirement met. Product policy and evidence pins remain ACT's integration step; this branch changes neither.

The candidate is not established for the newly available Dolphin 32k profile. Its check-term comment assumes 4,096 tokens at 60 s per 16 tokens (128 DAA at the 120 s target), then multiplies by 4 to get 512. Substituting 8,192 or 32,768 tokens at that same assumed rate gives 256 or 1,024 DAA **before** the 4× factor. Do not silently reuse the 4k coverage for those profiles. The forthcoming real execution measures the rate; it still cannot establish minimum-node performance. Likewise 2,250,000 B in the retention-price formula is one maximum close, not an observed entire 32k claim's retained material. Recompute on the selected legal profile and actual retention obligation.

The arithmetic T06 margin (303 DAA under the old candidate) remains conditional. Any enlarged measured term must be propagated through the complete deadline/retention composition before ACT pins it.


## Latest-source scope finding — 648544c46

The integration branch advanced to `648544c4628387a63e51824098b88cda5843dc2d`, including an independent review and a deferred Panel=1 draw. [Exact source blobs and line evidence](latest-source-scope-2026-10-26.json) pin this audit separately from the timing binaries.

The earlier recommendation to retain the provisional X3 candidate does **not** establish that X3 governs kernel claims:

1. `full_activation_v1.rs` includes X3 in the kernel slice. Its setter preserves/re-times an installed policy, but when none exists it creates `unapproved_probe_at`; an estimated numeric table is not an installed policy or arming record.
2. `palw_ruling_fences_v1.rs::palw_claim_deadlines_v1` takes `PalwClaimStateV2`. The kernel ledger has its own claim lifecycle and does not call that helper. Its challenge/court/grace/liability windows are separate from X3's fixed deadline/retention contract.
3. The kernel settlement and admission branches in `palw_settlement_liability_fold_v1.rs` read `paid_claim_signed_lifetime_v2(...).liability_until` and the material covenant. They do **not** call `native_claim_signed_hard_deadline_v1`. The latter's X3-required error belongs to the NativeClaim branch. This resolves that specific caller ambiguity in the independent review; it does not prove the ordinary canonical kernel claim has every required paid-lifetime/covenant input.

**Revised judgment:** keeping Δ=2,016 as a documented estimate is acceptable, but pinning it alone cannot complete this kernel slice. Before ACT arms the slice, its owner must explicitly reconcile the fence list and the actual claim contract: implement the adopted X3 contract for kernel claims, or adopt a kernel scope that excludes X3 while retaining its required liability/retention guarantees and reviewing all other enabled consumers. This branch changes neither semantics nor the slice list. Simply deleting the entry, or recording the A16 runtime as its evidence, would not close that dependency review.

The measured Dolphin 8k forward time is 1,030.075 s on a loaded Mac; it does not measure the kernel deadline contract, minimum-node tails, cold fetch, legal inclusion, reorg slack or retention price. Every non-measured term remains nonzero provisional material, not an arming result. The earlier numerical candidate and T06 arithmetic are retained as history; their applicability to kernel claims is now explicitly unresolved.

## Parallel CLOCK candidate — source review, not integrated evidence

The local `claude/g14-clock2` branch now contains `d512519540c8bc2a2dd8193776124df103efe75e` and `6539cdf4708365e4884fb148fe5f317678723495`. These are **not** ancestors of this measurement branch at review time. [Pinned source excerpts](clock-candidate-source-review-2026-10-26.json) record the change: the X3 entry installs the existing provisional policy, and the kernel consumer injects the activation DAA to raise Final/default liability to at least `committed + 3,120 + court + grace`. The provider court is added to the slice list. Thus the earlier finding that the kernel has no X3 hook is historical for this candidate; it remains true of the older measured source.

This is progress toward retaining X3, but the hook is not yet evidence for the complete X3 contract. The kernel receives the activation DAA only; the new floor reads fixed 3,120/10,000 constants and its ledger court/grace windows, not the five measured `B_*` terms or `c_retain`. Replacing those numerical estimates alone therefore does not demonstrate that the kernel uses the measured pursuit budget.

Two boundaries need executable validation before claiming closure:

- **Initial notice:** `proof_open` still accepts a Final claim through `liability_until`. With acceptance `a`, court 20, grace 10 and no longer hold, the new floor is `a+3,150`; a fresh direct proof at `a+3,121` passes that time predicate even though ADR-0198 describes initial notice ending at `a+3,120`. This is a source-derived predicate example, not an executed full-route result. Distinguish fresh notice from a response/proof completing an already-open pursuit; test both at 3,120/3,121/3,150/3,151.
- **Hard limit:** the new helper bounds its floor, while Final/default assignment computes `max(existing horizon, paid lifetime or legacy horizon, floor)`. That expression alone does not impose an upper bound of `a+10,000`. Demonstrate the upstream bounds on valid paid/source/EXEC inputs and the retained material, including boundary/overflow cases, rather than inferring a whole-horizon bound from the floor helper.

The candidate's two native tests compare an old-horizon filing with/without X3; they do not cover these boundaries. Its test configuration also uses gain 1 MSK and cap 4, while the measurement follow-up keeps approved gain 500 MSK and cap 64. The recommendation remains to preserve the nonzero estimates as **provisional**, retain X3 pending completion of its actual kernel contract, and keep measurement/arming entries unfilled. No fence-list removal, activation, production policy adoption or test success is asserted here.
