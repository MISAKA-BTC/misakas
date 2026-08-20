//! V2 reward escrow rules (ADR-0042 Decision 10, PR-09): the reward carve and the spendability
//! ladder, as pure functions the coinbase/UTXO layer consumes when the mode exists to demand
//! them (PR-10).
//!
//! Two facts, and nothing else:
//!
//! * **The carve.** PALW reward is a carve of the fixed subsidy — the ADR-0035 §5 worker-share
//!   shape — never an addition to it. [`palw_reward_carve_v2`] splits one subsidy into
//!   `(worker, remainder)` with `worker + remainder == subsidy` EXACTLY, floor on the worker
//!   side; the emission schedule cannot be exceeded by construction (I6/I15), and the test
//!   sweeps the edges to keep it that way.
//!
//! * **The escrow.** A `Provisional` block can still become `Voided`, so its reward must not be
//!   spendable immediately — and a fixed long coinbase maturity is the wrong tool, because each
//!   claim reaches `Final` at its own point (too short spends voided reward; long enough is
//!   needlessly long for the common case). [`palw_reward_status_v2`] is the whole rule: the
//!   claim's OWN phase decides — escrowed while immature, spendable at `Final`, forfeited at
//!   `Voided`. The mapping is total over the lattice, so no phase can slip through as
//!   accidentally spendable.

use crate::palw_state_v2::{PalwClaimPhaseV2, PalwStateV2Error};

/// Reward-side network constants (part of the atomic bundle; the fingerprint commits to them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRewardParamsV2 {
    /// The worker's carve of the block subsidy, in permille (the ADR-0035 §5 62 % shape lands
    /// here as a measured value at PR-10; any value ≤ 1000 is structurally sound).
    worker_carve_permille: u16,
}

impl PalwRewardParamsV2 {
    pub fn new(worker_carve_permille: u16) -> Result<Self, PalwStateV2Error> {
        if worker_carve_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("the worker carve cannot exceed the whole subsidy"));
        }
        Ok(Self { worker_carve_permille })
    }

    pub fn worker_carve_permille(&self) -> u16 {
        self.worker_carve_permille
    }
}

/// One subsidy, split. `worker + remainder == subsidy` always — the type exists so no caller
/// re-derives one side and rounds the schedule upward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRewardSplitV2 {
    pub worker: u64,
    pub remainder: u64,
}

/// `⌊subsidy · carve / 1000⌋` to the worker, the exact rest to the remainder. Floor on the
/// worker side: rounding must never mint a sompi the schedule does not contain.
pub fn palw_reward_carve_v2(subsidy: u64, params: &PalwRewardParamsV2) -> PalwRewardSplitV2 {
    let worker = ((subsidy as u128) * (params.worker_carve_permille as u128) / 1000) as u64;
    PalwRewardSplitV2 { worker, remainder: subsidy - worker }
}

/// Where a claim's escrowed reward stands. Total over the lattice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRewardStatusV2 {
    /// The claim is immature (`Provisional` / `PanelBound` / `ReceiptLicensed`): the reward
    /// exists but no transaction may spend it.
    Escrowed,
    /// The claim reached `Final`: spendable.
    Spendable,
    /// The claim was `Voided`: the reward is forfeit — a voided block's carve never enters
    /// circulation. (Where it goes — burn vs redistribution — is a params question the bundle
    /// answers; that it never reaches the producer is this rule.)
    Forfeited,
}

/// Decision 10's ladder: `block accepted → escrow → Final → spendable`, with `Voided → forfeit`.
pub fn palw_reward_status_v2(phase: &PalwClaimPhaseV2) -> PalwRewardStatusV2 {
    match phase {
        PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::ReceiptLicensed { .. } => {
            PalwRewardStatusV2::Escrowed
        }
        PalwClaimPhaseV2::Final { .. } => PalwRewardStatusV2::Spendable,
        PalwClaimPhaseV2::Voided { .. } => PalwRewardStatusV2::Forfeited,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::PalwVoidReasonV2;

    #[test]
    fn the_carve_never_exceeds_the_schedule_and_always_sums_exactly() {
        for permille in [0u16, 1, 333, 500, 620, 999, 1000] {
            let params = PalwRewardParamsV2::new(permille).unwrap();
            for subsidy in [0u64, 1, 2, 999, 1_000, 50_000_000_000, u64::MAX] {
                let split = palw_reward_carve_v2(subsidy, &params);
                assert_eq!(split.worker as u128 + split.remainder as u128, subsidy as u128, "the schedule is exact, always");
                assert!(split.worker as u128 <= (subsidy as u128) * (permille as u128) / 1000 + 1);
            }
        }
        assert!(PalwRewardParamsV2::new(1001).is_err(), "a carve above the whole subsidy is refused");
        // The floor direction: 62 % of 100 sompi is 62, and one indivisible sompi stays with the
        // remainder rather than being minted twice.
        let params = PalwRewardParamsV2::new(625).unwrap();
        let split = palw_reward_carve_v2(101, &params);
        assert_eq!((split.worker, split.remainder), (63, 38), "⌊101·625/1000⌋ = 63");
    }

    #[test]
    fn the_escrow_ladder_is_total_over_the_lattice() {
        assert_eq!(palw_reward_status_v2(&PalwClaimPhaseV2::Provisional), PalwRewardStatusV2::Escrowed);
        assert_eq!(palw_reward_status_v2(&PalwClaimPhaseV2::PanelBound { bound_daa: 1 }), PalwRewardStatusV2::Escrowed);
        assert_eq!(palw_reward_status_v2(&PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1 }), PalwRewardStatusV2::Escrowed);
        assert_eq!(palw_reward_status_v2(&PalwClaimPhaseV2::Final { final_daa: 1 }), PalwRewardStatusV2::Spendable);
        for reason in [
            PalwVoidReasonV2::BindTimeout,
            PalwVoidReasonV2::ReceiptTimeout,
            PalwVoidReasonV2::CourtFraud,
            PalwVoidReasonV2::ProducerWithholding,
        ] {
            assert_eq!(
                palw_reward_status_v2(&PalwClaimPhaseV2::Voided { voided_daa: 1, reason }),
                PalwRewardStatusV2::Forfeited,
                "every void reason forfeits — no reason is a discount"
            );
        }
    }
}
