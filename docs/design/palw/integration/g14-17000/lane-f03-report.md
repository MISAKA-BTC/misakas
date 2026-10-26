# Lane F03 report (P0-2, T05 slice) — 2026-10-26

Branch `claude/g14-f03` (base `55510b56a`). Scope narrowed per the coordinator's vertical-slice message to the paid fair ingress / one-outsider path the SPINE slice will call. **G14 is still incomplete. Nothing is armed.**

## Requirement
T05: every owner slot held and refilled by colluders, and one external verifier still convicts through the paid public ingress. The fee schedule (V4) belongs to the values lane, so it stays typed-missing.

## Existing implementation and real callers (mapped, not changed)
| Piece | File | Real caller |
|---|---|---|
| Fee schedule, typed-missing in every non-test build; `R = ⌊cap/2⌋` | `consensus/core/src/palw_court_paid_ingress_v1.rs` | capacity fold, g14 gates, user views |
| Claim-admission bound (at most R claims in flight, each holding one outside reservation) | `palw_court_capacity_fold_v1.rs` `palw_court_ingress_claim_admits_v1` / `_excluding_v1` | attempt fold (`palw_state_v2.rs:12806`), funded install (`:927`) |
| Session admission + one-time fee | `court_ingress_admission_v1` → `paid_court_cap_admits_v1` | `palw_state_v2/da_sessions.rs:764` (`CourtOpened`, named-leaf dissection) |
| Inside/outside side rule (producer, seats, linked operator/payout) | `court_outsider_lane_inside_v1` | the same |

The common semantic/court coordinator, the all-branch/all-resource reservation at original admission, and node/wire ingress are **not** in this lane's change.

## Ownership
New file only: `consensus/core/src/palw_state_v2/tests/f03_outsider_sybil_refill_v1.rs`. One `mod` line in `palw_state_v2/tests.rs`. I did not touch any Codex or F02 files.

## Finding (invariant)
An **unlinked** coalition Sybil (different operator id and payout) counts as an outsider, so it can take the target's reservation first. While its session runs, the honest outsider is an outside follow-on and gets `CourtSessionsAtCapacity`. A silent stall does not work for the coalition, because the colluding producer owes the opening-rung move and the sweep's default voids the lie. The open residual is a coalition whose producer answers every rung while the Sybil goes silent on its own turn, after which a fresh Sybil retakes the freed reservation. That refill is bounded only by cost: fee plus challenger-side loss per cycle, at most `hard window / session window` cycles. Outside filers are not ordered fairly. There is no test for this case yet, because it needs ladder moves.

## Tests
`buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline -p kaspa-consensus-core --lib -- f03_t05 r5_f03`. Result: exit 0, **12 passed / 0 failed** (2 new + 10 existing r5_f03). The first draft assumed a challenger-side close and failed (exit 101). Its assumptions were corrected to match the fold.

Test-only values: fee 5 sompi (injected), rung window 8 (cap 8, R = 4), collateral 1,000. Distinct fixture keys do not show independence or lottery approval.

## Remaining
- Fair ordering of outside filers on a freed reservation, a FIFO hand-off or similar. This needs a design decision and probably state rows. No schema change was made.
- T05 on an actual node with continuous refill; T03 (forged signature / fake Panel report before monetary effect); T08 (miner offline, distinct court signer, third-party relay). Not done.
- V4 fee value (values lane). The deterrence condition the fee must satisfy is `fee × (W_hard / W_session) > gain from a lie standing`.
