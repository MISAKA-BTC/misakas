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
//! | `AccuserReward`, `DemanderShare` | paid out to the bond's owner | from the preceding slash |
//! | `Burn` | destroyed | from the preceding slash (`bond` is the slashed bond) |
//! | `FinalReward` | paid out to the bond's owner | newly issued by the consumer's reward path |
//! | `Withdraw` | `C −= amount`; the ledger forgets the bond | collateral returns to its owner |
//!
//! Conservation: within one receipt batch, `Σ slashes == Σ (AccuserReward + DemanderShare + Burn)`. [`SettlementBookV1`] is a
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
}

impl SettlementKindV1 {
    pub const fn is_slash(self) -> bool {
        matches!(self, Self::SlashFraud | Self::SlashDefault | Self::SlashFiling)
    }

    /// Paid out of a slash.
    pub const fn is_routed_payout(self) -> bool {
        matches!(self, Self::AccuserReward | Self::DemanderShare | Self::Burn)
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
    /// Totals: slashed, routed to payouts or burned out of slashes, burned, and newly issued by `FinalReward`.
    pub slashed: u64,
    pub routed: u64,
    pub burned: u64,
    pub issued: u64,
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
            K::ReserveClaim | K::ReserveDemand => {
                if a > c.saturating_sub(r) {
                    return Err(format!("reserve {a} past the free collateral {}", c.saturating_sub(r)));
                }
                self.reserved.insert(b, r + a);
            }
            K::ReleaseClaim | K::ReleaseDemand => {
                if a > r {
                    return Err(format!("release {a} of a {r} reservation"));
                }
                self.reserved.insert(b, r - a);
            }
            K::SlashFraud | K::SlashDefault => {
                if a > r || a > c {
                    return Err(format!("slash {a} of a {r} reservation / {c} collateral"));
                }
                self.reserved.insert(b, r - a);
                self.collateral.insert(b, c - a);
                self.slashed += a;
            }
            K::SlashFiling => {
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
                *self.paid.entry(b).or_default() += a;
                self.issued += a;
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
