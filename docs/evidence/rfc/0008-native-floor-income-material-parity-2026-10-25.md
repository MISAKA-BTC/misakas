# RFC-0008: native BASE issuer and selected-Panel material parity

## Result and boundary

The focused core tests passed **5/5 in 0.04s**, and the existing state budget-fold regression group passed **30/30 in 0.38s** using the same pinned core binary. The core build completed in **2m55s**. These are private mechanics and existing fold regressions. The subsequent pinned native Typed pruning Run 4 reached actual native Final at DAA 1520, with safe frontier blue score 2758 beyond Release blue score 734; that run was later intentionally interrupted by the controller after Run 5 startup, without a physical-prune result. Run 5 independently reached actual native Final at DAA1520 with the same blue frontier relation; its physical-pruning runtime remains live/PENDING. Exact runtime/source records are in [the pruning evidence](0008-typed-native-pruning-2026-10-25.md). Physical pruning and pruned IBD are not yet established.

- Focused log: `/tmp/misaka-rfc-native-floor-material-budget-tests-20261025.log`.
- Existing fold regression log: `/tmp/misaka-rfc-native-floor-budget-fold-regression-20261025.log`.
- Pinned core binary: `/tmp/misaka-rfc-native-floor-material-core-stage2-20261025`.
- SHA-256: `63dfe08473701f4e5cdfc4393f21fe10ef9412efe4d97f8d8ec5dbbb951e048d`.
- Source snapshot: `/tmp/misaka-floor-native-payment-parity-stage2-source-20261025/`.

## Concrete failure and existing contract

The actual native pruning Run 3 successfully registered and matured its real funded ninth bond, then disqualified its first native BASE attempt with `new material lacks signed historical-release recovery enrollment`. That failed run and its immutable source/binary/log records are retained in [the pruning evidence](0008-typed-native-pruning-2026-10-25.md). No native Final or long pruning loop was reached in Run 3.

The existing native admission in `apply_attempt` excludes BASE from paid native custody. The existing public `native_recovery_session_quote_v4` API independently refuses BASE with `BASE floor has no paid native recovery session`. Downstream budget reservation and income consumers nevertheless required that unavailable session. This change aligns those consumers with the existing admission/API boundary. Historical design discussion of a broader floor exemption includes pending-review wording; this correction does not approve that proposal or create a public policy.

## Narrow implementation

Only an actual native `Attempt` claim in `params.base_class_id()`, with actual allocation origin `Attempt` or `Rider` and the matching producer bond, receives the issuer exception. The reservation commit still performs normal Q/B/R/F admission, reservation and block consumption. The dedicated issuer binder computes the canonical role-0 income identity itself, then performs the normal journaled root binding. Issuer payment uses it only for role 0 and the claim's actual producer bond.

Selected-Panel role-1 income additionally requires membership in the actual stored Panel. Both native V2 and permissionless V3 paths write the validated selected Panel before reserving seat duties. The hold consumer requires exact equality between the supplied and stored seat lists; its dedicated binder computes the canonical role-1 income identity. Payment checks the stored selected recipient again. The ordinary budget hold and payment operations, existing Panel pool and physical-payment provenance remain intact.

Generic income binding and generic holds remain strict even on the same BASE root. FreePrompt, nonfloor, missing claims, other allocation origins, wrong producer bonds, absent/mismatched Panels and unselected recipients retain enrollment requirements. Other roles receive no exception. Inactive-material paths retain their previous behavior, including historical native income with no root allocation row. No policy rates, capital/escrow checks, wire tags, public presets, activation pins or approved economics inputs change.

## Executed controls

The five focused tests drive the actual planner, reservation commit, dedicated issuer binder, selected-Panel hold and recipient payment consumers:

1. Attempt and Rider commits preserve Q/B/R/F accounting, native issuer income and root provenance without an unavailable paid session; a generic binding on that exact same root still refuses.
2. Seven nonfloor/source/origin/missing-claim cases refuse commit and dedicated issuer binding without dirty state/journal changes.
3. A real selected-Panel hold preserves the existing 200‰ pool, then queues exact issuer/Panel amounts of 32/8 from the explicit 40-sompi private fixture. Exact queued provenance is checked, and quota is not freed by reclassification or queue creation.
4. Eleven payment-authority mismatches refuse before writes; generic holds remain strict and their enclosing checkpoint restores state/journal. Absent/mismatched stored Panel controls refuse before a privileged hold.
5. Inactive material preserves historical native hold/payment without a root allocation row.

Queued receipts remain distinct from actual coinbase payment: their material hold is still unknown until a physical payout consumes them. This focused evidence does not establish real native pipeline completion, reward certification, a K3/model campaign, allocator bounds or completed pruning/IBD. Existing public refusals and UNKNOWN campaign gates remain unchanged.
