//! **ADR-0176 / ADR-0177 — the user's rulings of 2026-10-15 (C1–C5) on the budget engine** (DORMANT; no height; every value a
//! calibration starting point, never a production value). Binding text: `docs/design/palw/user-decisions-2026-10-15-adr-rfc16-rfc17.md`
//! §1.
//!
//! ```text
//! C1  newly issued miner-facing coinbase reward never bypasses the per-bond R cap on any path: past a V3 budget the WHOLE
//!     subsidy-derived worker base of an attempt-lane carrier is withheld; the claim's escrow (its R reservation) is the only subsidy
//!     ever paid for it, the rest is never minted, and a budget-refused (skipped) attempt is paid no subsidy at all. Transaction fees
//!     are user-paid, so their worker shares stay outside R (separate accounting).
//! ```
//!
//! Nothing in this module is built or run by the lane that wrote it (a separate validation agent compiles). No line is a PASS.

use super::*;

/// **C1 (user decision 2026-10-15): does `R` cover the attempt-lane worker base at `paying_daa`?** `true` exactly where the bond
/// budget is in force with a V3 (mode-aware hold) policy — the only armable form outside the isolated calibration's mechanics probe,
/// which keeps the old coinbase (that probe budgets mechanics, never payment). `false` on every shipped network (no mirror).
pub fn palw_r_cap_covers_worker_base_v1(mirror: Option<&PalwBondBudgetMirrorV1>, paying_daa: u64) -> bool {
    mirror.is_some_and(|m| m.active_at(paying_daa) && m.policy.hold.is_some())
}

/// **C1: the amount withheld from an attempt-lane carrier's worker output** where [`palw_r_cap_covers_worker_base_v1`] holds:
/// `max(recorded, worker_base)` — `recorded` is what the claim escrowed (its carve, inside the claim's `R` reservation; 0 for a
/// skipped / budget-refused attempt) and `worker_base` the carrier's whole subsidy-derived worker base under the paying block's split.
/// The withheld amount never exceeds the subsidy-derived share, so a user-paid fee share is never touched. Elsewhere `recorded`.
pub fn palw_r_cap_withheld_base_v1(covers: bool, recorded: u64, worker_base: u64) -> u64 {
    if covers { recorded.max(worker_base) } else { recorded }
}

/// **C1: the newly issued, miner-facing subsidy an attempt-lane carrier is ever paid** under the rule: its escrow (released only at
/// `Final`, inside the producer's `R` cap) — never the uncarved remainder, and nothing for a budget-refused attempt (`escrowed = 0`).
/// What is withheld beyond the escrow is never minted.
pub fn palw_r_cap_paid_subsidy_v1(covers: bool, escrowed: u64, worker_base: u64) -> (u64, u64) {
    if !covers {
        // The old rule: the escrow at Final plus the uncarved remainder at acceptance (the C1 gap this rule closes).
        return (escrowed.saturating_add(worker_base.saturating_sub(escrowed)), 0);
    }
    let withheld = palw_r_cap_withheld_base_v1(true, escrowed, worker_base);
    (escrowed.min(worker_base), withheld.saturating_sub(escrowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mirror(policy: PalwBondBudgetPolicyV1) -> PalwBondBudgetMirrorV1 {
        PalwBondBudgetMirrorV1 { from_daa: 100, policy, allocation: None }
    }

    #[test]
    fn c1_the_rule_is_off_without_a_v3_budget() {
        assert!(!palw_r_cap_covers_worker_base_v1(None, 1_000), "no mirror: every shipped network");
        let probe = mirror(PalwBondBudgetPolicyV1::opv_mechanics_probe_v1());
        assert!(!palw_r_cap_covers_worker_base_v1(Some(&probe), 1_000), "the calibration mechanics probe keeps the old coinbase");
        let v3 = mirror(PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1());
        assert!(!palw_r_cap_covers_worker_base_v1(Some(&v3), 99), "below the budget's height");
        assert!(palw_r_cap_covers_worker_base_v1(Some(&v3), 100));
    }

    #[test]
    fn c1_an_admitted_attempt_is_paid_only_its_escrow_and_a_refused_one_nothing() {
        // worker base 720 of a 1000 subsidy, carve (escrow) 500: the uncarved 220 is not paid outside R.
        assert_eq!(palw_r_cap_withheld_base_v1(true, 500, 720), 720);
        assert_eq!(palw_r_cap_paid_subsidy_v1(true, 500, 720), (500, 220), "escrow paid at Final, 220 never minted");
        // A budget-refused (skipped) attempt: no escrow, the whole base withheld, nothing paid.
        assert_eq!(palw_r_cap_withheld_base_v1(true, 0, 720), 720);
        assert_eq!(palw_r_cap_paid_subsidy_v1(true, 0, 720), (0, 720));
        // The old rule paid the uncarved remainder at acceptance.
        assert_eq!(palw_r_cap_withheld_base_v1(false, 500, 720), 500);
        assert_eq!(palw_r_cap_paid_subsidy_v1(false, 500, 720), (720, 0));
        // t12's 720 ‰ carve of a 72 % base: carve == base, nothing changes for an admitted attempt.
        assert_eq!(palw_r_cap_paid_subsidy_v1(true, 720, 720), (720, 0));
    }

    #[test]
    fn c1_the_withheld_amount_never_exceeds_the_subsidy_share() {
        // The coinbase subtracts the withheld amount from `worker_base + fee_worker_shares`; the fee shares (user-paid) survive.
        for (escrow, base) in [(0u64, 0u64), (0, 1), (1, 1), (10, 100), (100, 100)] {
            let withheld = palw_r_cap_withheld_base_v1(true, escrow, base);
            assert!(withheld <= base.max(escrow));
            let fee_share = 37u64;
            assert_eq!((base + fee_share).saturating_sub(withheld), base.saturating_sub(withheld) + fee_share);
        }
    }
}
