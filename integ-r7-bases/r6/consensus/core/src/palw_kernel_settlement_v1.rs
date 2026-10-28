//! **RFC-0015 §9 / §11.3: the one settlement predicate of a kernel-route claim** — what every reader of a kernel claim's `Final`
//! (an OPV claim's above all) may conclude: the EXEC work-slice verification (RFC-0008 v2, `palw_exec_v2_claim_outcome_v1`), the
//! public claim read (RPC op `getPalwKernelClaim`, the web view), and any bridge / native-settlement consumer.
//!
//! ```text
//! convicted (before or after Final)                          → Convicted   (void; a slice is ProvenFalse)
//! Unavailable / TimedOut, or Final with the reservation gone
//!   while still liable, or an OPV post-Final default         → Defaulted   (never safe)
//! Final, liability horizon not passed                        → FinalRevocable { final_daa, liability_until }
//!                                                               (a slice verifies, its leg stays held; NOT safe for an
//!                                                                irreversible release, NOT shown as settled)
//! Final, past the liability horizon, unconvicted             → Settled { final_daa, settled_from_daa = liability_until + 1 }
//! anything else (Committed, Challengeable, ProofGrace, ...)  → Pending
//! ```
//!
//! **Why one predicate.** An OPV `Final` means only that the fixed challenge window closed with no accepted dispute; a valid proof
//! within the liability horizon still convicts it (and the beacon withdraws its fact, `FinalStandingV1::ConvictedAfterFinal`). A
//! reader that treated `Final` as irreversible would release value the court can still void. The fraud-expiry of every reader is the
//! kernel claim's own `liability_until` — the same horizon the EXEC reward / native-settlement maturity already folds in
//! (`palw_exec_v2_rewards_v1::horizon`) — so no reader can become "safe" earlier than the route can stop convicting.
//!
//! **Weight.** Kernel-route claims reserve no reward block and no Final weight in the ADR-0176 budget (`B = F = 0`): an OPV `Final`
//! adds no consensus weight and no native-settlement work of its own; work-slice credit reaches settlement only through the V2 root it
//! verifies, whose maturity already includes this horizon. Nothing here writes state.

use misaka_palw_kernel::lifecycle::ClaimStateV1;

/// The settlement standing of one kernel claim at one DAA (module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwKernelSettlementV1 {
    Pending,
    FinalRevocable { final_daa: u64, liability_until: u64 },
    Settled { final_daa: u64, settled_from_daa: u64 },
    Convicted,
    Defaulted,
}

impl PalwKernelSettlementV1 {
    /// The stable label readers display (`pending`, `final-revocable`, `settled`, `convicted`, `defaulted`).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::FinalRevocable { .. } => "final-revocable",
            Self::Settled { .. } => "settled",
            Self::Convicted => "convicted",
            Self::Defaulted => "defaulted",
        }
    }

    /// Whether an irreversible release (a bridge, an external settlement, a "safe" display) may rely on the claim.
    pub const fn is_safe(self) -> bool {
        matches!(self, Self::Settled { .. })
    }

    /// The first DAA at which the claim is (or would be, absent a conviction or default) settled; `None` before `Final`.
    pub const fn settled_from_daa(self) -> Option<u64> {
        match self {
            Self::Settled { settled_from_daa, .. } => Some(settled_from_daa),
            Self::FinalRevocable { liability_until, .. } => Some(liability_until.saturating_add(1)),
            _ => None,
        }
    }
}

/// **The predicate** over a kernel claim row's facts at `daa`. `forfeited_after_final` is the OPV row's post-Final default flag
/// (`OpvClaimRowV1::forfeited_after_final`; `false` for a Panel-licensed claim, whose row has none). A `Final` with no recorded
/// horizon is never settled.
pub fn palw_kernel_claim_settlement_v1(
    state: &ClaimStateV1,
    convicted: bool,
    reserved: u64,
    liability_until: Option<u64>,
    forfeited_after_final: bool,
    daa: u64,
) -> PalwKernelSettlementV1 {
    if convicted {
        return PalwKernelSettlementV1::Convicted;
    }
    match *state {
        ClaimStateV1::Final { final_daa } => {
            if forfeited_after_final {
                return PalwKernelSettlementV1::Defaulted;
            }
            let until = liability_until.unwrap_or(u64::MAX);
            if daa <= until && reserved == 0 {
                // The reservation is gone while the claim is still liable: a post-Final default (the slice's `Defaulted`).
                PalwKernelSettlementV1::Defaulted
            } else if daa > until {
                PalwKernelSettlementV1::Settled { final_daa, settled_from_daa: until.saturating_add(1) }
            } else {
                PalwKernelSettlementV1::FinalRevocable { final_daa, liability_until: until }
            }
        }
        ClaimStateV1::Convicted { .. } => PalwKernelSettlementV1::Convicted,
        ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. } => PalwKernelSettlementV1::Defaulted,
        _ => PalwKernelSettlementV1::Pending,
    }
}

/// The same predicate over a ledger row (and the ledger's OPV row, if any).
pub fn palw_kernel_row_settlement_v1(
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    claim: &misaka_palw_kernel::hash::Digest,
    daa: u64,
) -> Option<PalwKernelSettlementV1> {
    let row = ledger.claims.get(claim)?;
    let forfeited = ledger.opv.claims.get(claim).is_some_and(|o| o.forfeited_after_final);
    Some(palw_kernel_claim_settlement_v1(&row.life.state, row.convicted, row.reserved, row.liability_until, forfeited, daa))
}

#[cfg(test)]
mod tests {
    use super::*;
    use PalwKernelSettlementV1 as S;

    fn at(state: ClaimStateV1, convicted: bool, reserved: u64, until: Option<u64>, forfeited: bool, daa: u64) -> S {
        palw_kernel_claim_settlement_v1(&state, convicted, reserved, until, forfeited, daa)
    }

    #[test]
    fn an_opv_final_is_revocable_until_its_liability_horizon_then_settled() {
        let fin = ClaimStateV1::Final { final_daa: 50 };
        assert_eq!(at(fin.clone(), false, 10, Some(90), false, 50), S::FinalRevocable { final_daa: 50, liability_until: 90 });
        assert!(!at(fin.clone(), false, 10, Some(90), false, 90).is_safe(), "the last liable DAA is not safe");
        assert_eq!(at(fin.clone(), false, 0, Some(90), false, 91), S::Settled { final_daa: 50, settled_from_daa: 91 });
        assert!(at(fin.clone(), false, 0, Some(90), false, 91).is_safe());
        assert_eq!(at(fin.clone(), false, 10, None, false, u64::MAX), S::FinalRevocable { final_daa: 50, liability_until: u64::MAX });
    }

    #[test]
    fn convictions_and_defaults_are_never_safe_whenever_they_land() {
        let fin = ClaimStateV1::Final { final_daa: 50 };
        assert_eq!(at(fin.clone(), true, 0, Some(90), false, 60), S::Convicted, "a conviction after Final voids it");
        assert_eq!(at(fin.clone(), true, 0, Some(90), false, 1_000), S::Convicted, "and stays void past the horizon");
        assert_eq!(at(fin.clone(), false, 0, Some(90), false, 60), S::Defaulted, "the reservation gone while liable");
        assert_eq!(at(fin.clone(), false, 0, Some(90), true, 1_000), S::Defaulted, "an OPV post-Final default is never settled");
        assert_eq!(at(ClaimStateV1::Unavailable { daa: 9, producer_defaulted: true }, false, 0, None, false, 9), S::Defaulted);
    }

    #[test]
    fn nothing_before_final_is_settled() {
        for state in [ClaimStateV1::Committed, ClaimStateV1::Challengeable { since_daa: 3, window_end_daa: 53 }] {
            assert_eq!(at(state, false, 10, None, false, 52), S::Pending);
        }
        assert_eq!(S::Pending.settled_from_daa(), None);
        assert_eq!(S::FinalRevocable { final_daa: 1, liability_until: 9 }.settled_from_daa(), Some(10));
    }
}
