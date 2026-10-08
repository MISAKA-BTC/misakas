//! **Settlement: the ledger never mints, burns or moves a coin — it tells the consumer to.**
//!
//! Bonds are the consumer's: real locked collateral that consensus owns (`PalwBondKeyV2`, an account's stake). The kernel ledger
//! keeps only a *view* of each bond it is told about (`KernelLedgerV1::sync_bond`) and, for every money-relevant decision, emits an
//! explicit [`SettlementInstructionV1`] as a receipt (`LedgerEventV1::Settlement`) that the consumer applies to its real bonds in
//! the same block. There is no internal balance a rule could credit.
//!
//! | kind | effect on the named bond's collateral `C` / reservation `R` | funds |
//! |---|---|---|
//! | `ReserveClaim`, `ReserveDemand` | `R += amount` (never past `C`) | — |
//! | `ReleaseClaim`, `ReleaseDemand` | `R −= amount` | — |
//! | `SlashFraud`, `SlashDefault` | `R −= amount`, `C −= amount` (the slashed reservation) | routed by the payouts and the burn that follow |
//! | `SlashFiling` | `C −= amount` (free collateral: a dismissed filing's fee) | routed by the burn that follows |
//! | `AdmissionFee` | `C −= amount` (free collateral: an OPV claim's non-refundable admission fee) | routed by the burn that follows |
//! | `AccuserReward`, `DemanderShare` | paid out to the bond's owner | from the preceding slash |
//! | `Burn` | destroyed | from the preceding slash (`bond` is the slashed bond) |
//! | `ReserveJobEscrow`, `ReleaseJobEscrow` | `R += amount` / `R −= amount` (the poster's escrow of a posted job, GAP-5) | — |
//! | `PayJobEscrow` | `R −= amount`, `C −= amount` (the poster's escrow, spent at the job's Final) | routed by the `FinalReward` that follows |
//! | `JobFee` | `C −= amount` (free collateral: posting a job's non-refundable fee, GAP-5) | routed by the burn that follows |
//! | `FinalReward` | paid out to the bond's owner | from the `PayJobEscrow` before it — the poster pays; **never newly issued** (GAP-5) |
//! | `Withdraw` | `C −= amount`; the ledger forgets the bond | collateral returns to its owner |
//!
//! Conservation: within one receipt batch, `Σ debits == Σ (AccuserReward + DemanderShare + FinalReward + Burn)` — every payout,
//! the Final reward included, is routed out of a debit of a real bond in the same batch; nothing is issued. [`SettlementBookV1`] is a
//! reference consumer that applies instructions to a toy bond book and checks that and the reservation rules; the tests use it to
//! prove the ledger's own view and the instructions agree.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::Digest;
use crate::ledger::LedgerEventV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SettlementKindV1 {
    /// Lock a committed claim's collateral behind the claim (producer).
    ReserveClaim = 1,
    /// Unlock it: the claim timed out, went unavailable, or its liability horizon ended unprosecuted (producer).
    ReleaseClaim = 2,
    /// Lock a demand bond (demander).
    ReserveDemand = 3,
    /// Refund it: served, moot after a conviction or default, or its claim was decided (demander).
    ReleaseDemand = 4,
    /// A fraud conviction took this much of the producer's reservation.
    SlashFraud = 5,
    /// An availability default took this much of the producer's reservation (never a fraud conviction).
    SlashDefault = 6,
    /// A dismissed filing's fee, taken from the accuser's free collateral.
    SlashFiling = 7,
    /// The convicting accuser's share of a slash.
    AccuserReward = 8,
    /// A demander's share of a default's forfeit.
    DemanderShare = 9,
    /// The producer's reward at Final.
    FinalReward = 10,
    /// The part of a slash nobody is paid: destroyed.
    Burn = 11,
    /// The bond's collateral returns to its owner (it exited with nothing reserved).
    Withdraw = 12,
    /// C4 F-C4R3-05: an OPV claim's non-refundable admission fee, taken from the producer's free collateral (burned).
    AdmissionFee = 13,
    /// GAP-5: a posted job's escrow, reserved from its poster's free collateral.
    ReserveJobEscrow = 14,
    /// GAP-5: an escrow no claim can still use (or the part a debit could not take), back to its poster.
    ReleaseJobEscrow = 15,
    /// GAP-5: a job's escrow spent at its Final: debited from the poster, routed to the producer by the `FinalReward` after it.
    PayJobEscrow = 16,
    /// GAP-5: posting a job's non-refundable fee, from the poster's free collateral (burned).
    JobFee = 17,
    /// A claim seal's deposit, reserved from its producer's free collateral until the seal is revealed.
    ReserveSealDeposit = 18,
    /// The deposit back: the seal was revealed (or the part a forfeit could not take).
    ReleaseSealDeposit = 19,
    /// An unrevealed seal expired: its deposit is forfeited (routed by the burn after it).
    ForfeitSealDeposit = 20,
    /// The demand bond of a position served on chain, burned because the claim's liability horizon ended with no conviction (routed
    /// by the burn after it). Refunded instead (`ReleaseDemand`) on conviction, default or timeout.
    ForfeitDemandBond = 21,
}

impl SettlementKindV1 {
    pub const fn is_slash(self) -> bool {
        matches!(
            self,
            Self::SlashFraud
                | Self::SlashDefault
                | Self::SlashFiling
                | Self::AdmissionFee
                | Self::JobFee
                | Self::PayJobEscrow
                | Self::ForfeitSealDeposit
                | Self::ForfeitDemandBond
        )
    }

    /// Paid out of a debit (a slash, a fee, a spent escrow).
    pub const fn is_routed_payout(self) -> bool {
        matches!(self, Self::AccuserReward | Self::DemanderShare | Self::FinalReward | Self::Burn)
    }
}

/// **One instruction to the consumer**: `amount` of `kind` against `bond`, about `claim` (when one applies).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct SettlementInstructionV1 {
    pub bond: Digest,
    pub amount: u64,
    pub kind: SettlementKindV1,
    pub claim: Option<Digest>,
}

/// **A reference consumer**: a toy bond book that applies instructions and refuses any that break a bond rule. Real consensus maps
/// these onto its own bond state; this is the executable statement of what each instruction means.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettlementBookV1 {
    pub collateral: BTreeMap<Digest, u64>,
    pub reserved: BTreeMap<Digest, u64>,
    /// Paid out to each bond's owner (rewards, shares, final rewards).
    pub paid: BTreeMap<Digest, u64>,
    /// Totals: debited (slashes, fees, spent escrows), routed to payouts or burned out of them, burned, and paid out as Final rewards
    /// (GAP-5: a subset of `routed`, never issued).
    pub slashed: u64,
    pub routed: u64,
    pub burned: u64,
    pub final_rewards: u64,
    /// Σ `PayJobEscrow`: the escrows Final rewards were paid out of.
    pub escrow_spent: u64,
}

impl SettlementBookV1 {
    /// The consumer's real locked collateral (what it tells the ledger through `sync_bond`).
    pub fn set_collateral(&mut self, bond: Digest, collateral: u64) {
        self.collateral.insert(bond, collateral);
    }

    pub fn free(&self, bond: &Digest) -> u64 {
        self.collateral.get(bond).copied().unwrap_or(0).saturating_sub(self.reserved.get(bond).copied().unwrap_or(0))
    }

    pub fn apply(&mut self, s: &SettlementInstructionV1) -> Result<(), String> {
        use SettlementKindV1 as K;
        let (b, a) = (s.bond, s.amount);
        let c = self.collateral.get(&b).copied().unwrap_or(0);
        let r = self.reserved.get(&b).copied().unwrap_or(0);
        match s.kind {
            K::ReserveClaim | K::ReserveDemand | K::ReserveJobEscrow | K::ReserveSealDeposit => {
                if a > c.saturating_sub(r) {
                    return Err(format!("reserve {a} past the free collateral {}", c.saturating_sub(r)));
                }
                self.reserved.insert(b, r + a);
            }
            K::ReleaseClaim | K::ReleaseDemand | K::ReleaseJobEscrow | K::ReleaseSealDeposit => {
                if a > r {
                    return Err(format!("release {a} of a {r} reservation"));
                }
                self.reserved.insert(b, r - a);
            }
            K::SlashFraud | K::SlashDefault | K::PayJobEscrow | K::ForfeitSealDeposit | K::ForfeitDemandBond => {
                if a > r || a > c {
                    return Err(format!("slash {a} of a {r} reservation / {c} collateral"));
                }
                self.reserved.insert(b, r - a);
                self.collateral.insert(b, c - a);
                self.slashed += a;
                if s.kind == K::PayJobEscrow {
                    self.escrow_spent += a;
                }
            }
            K::SlashFiling | K::AdmissionFee | K::JobFee => {
                if a > c.saturating_sub(r) {
                    return Err(format!("fee {a} past the free collateral {}", c.saturating_sub(r)));
                }
                self.collateral.insert(b, c - a);
                self.slashed += a;
            }
            K::AccuserReward | K::DemanderShare => {
                *self.paid.entry(b).or_default() += a;
                self.routed += a;
            }
            K::Burn => {
                self.burned += a;
                self.routed += a;
            }
            K::FinalReward => {
                // GAP-5: a Final reward is a spent escrow routed to the producer, never new money.
                if self.final_rewards.saturating_add(a) > self.escrow_spent {
                    return Err(format!(
                        "a Final reward of {a} with only {} of spent escrow behind it",
                        self.escrow_spent - self.final_rewards
                    ));
                }
                *self.paid.entry(b).or_default() += a;
                self.final_rewards += a;
                self.routed += a;
            }
            K::Withdraw => {
                if r != 0 || a != c {
                    return Err(format!("withdraw {a} of {c} collateral with {r} reserved"));
                }
                self.collateral.remove(&b);
                self.reserved.remove(&b);
            }
        }
        Ok(())
    }

    /// Apply every settlement in a receipt batch, then check that everything slashed was routed.
    pub fn apply_events(&mut self, events: &[LedgerEventV1]) -> Result<(), String> {
        for e in events {
            if let LedgerEventV1::Settlement(s) = e {
                self.apply(s).map_err(|why| format!("{s:?}: {why}"))?;
            }
        }
        self.balanced()
    }

    /// `slashed == routed`: nothing a slash took is unaccounted for.
    pub fn balanced(&self) -> Result<(), String> {
        if self.slashed == self.routed { Ok(()) } else { Err(format!("slashed {} but routed {}", self.slashed, self.routed)) }
    }
}
