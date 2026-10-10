//! **ECON's M\*-49 verifier pay and the held default share** (readiness §3f, the user's rulings of 2026-10-10; design
//! `docs/design/palw/econ-verifier-incentive-and-allocation.md` §2.2 O2 and §3.2). Dormant: in force only where the consumer injects a
//! [`VerifierPayPolicyV1`] (the node's `palw_verifier_pay_v1` fence, refused when armed) and the ledger's DAA has reached it.
//!
//! * **The check fee (M\*-49 item 3).** A job posted past the fence escrows `slots × check_fee` from its poster beside the GAP-5
//!   escrow (a reservation on the poster's bond, table 27 `FeeEscrow`). When the post-commit draw for a claim of the job lands
//!   ([`KernelLedgerV1::assign_drawn_v1`], the consumer's call with OPVB's draw), the drawn slots keep `check_fee` each and the undrawn
//!   rest returns to the poster. A drawn slot is paid `check_fee` on its attestation ([`KernelLedgerV1::attest_drawn_v1`]) or on the
//!   claim's conviction or producer default before the attestation deadline ("pay on fate"); a slot neither attested nor paid by its
//!   fate returns its fee to the poster. The fee is a user transfer — the poster's debit funds the slot's payout exactly (the
//!   GAP-5 settlement pair `PayJobEscrow` + `FinalReward`) — never issuance, never a producer's reward: it does NOT draw on
//!   ADR-0176's producer budget hook `bond_budget_final_reward_v1` (BUDGET; that hook budgets the Final reward only).
//! * **Drawn sealers first, inside the unchanged 49 % (item 4).** Of a conviction's bounty, the drawn slots that sealed the convicting
//!   bytes share up to `bounty_cap` equally; the rest goes to the earliest sealer, as before. Rate and total are unchanged.
//! * **A-DEM (item 3).** A drawn slot's served demand bond is refunded, never burned at the horizon.
//! * **The held default share (O2).** A pre-Final default collects only its burned part; the demanders' share stays RESERVED on the
//!   producer's bond (table 27 `HeldShare`). A conviction inside the liability horizon slashes it with the rest, and the one pool is
//!   `⌊share × everything collected⌋` — an honest accuser after a self-inflicted default is paid as if no default had come first (490
//!   of 1,000 at the interim terms), and the coalition still recovers at most 49 %. With no conviction by the horizon the held share is
//!   slashed then and paid to the demanders (equally; the remainder burned).
//!
//! Every movement is a settlement instruction the consumer applies, so escrow, fee, burn and payouts conserve exactly.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::Digest;
use crate::ledger::{KernelLedgerV1, KernelRefusalV1, LedgerEventV1, SettlementInstructionV1};
use crate::lifecycle::ClaimStateV1;
use crate::settle::SettlementKindV1;

/// Table 27's key kinds.
pub const VERIFIER_PAY_FEE_ESCROW_V1: u8 = 0;
pub const VERIFIER_PAY_DRAW_V1: u8 = 1;
pub const VERIFIER_PAY_HELD_SHARE_V1: u8 = 2;

/// **The verifier-pay terms** (POLICY, interim and unapproved). Consumer-injected, not part of the state or its root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VerifierPayPolicyV1 {
    /// The fence's height on the consumer's chain.
    pub activation_daa: u64,
    /// `F`: what one drawn slot is paid for one check.
    pub check_fee: u64,
    /// `m`: the drawn slots a claim's check fee escrow covers.
    pub slots: u8,
    /// `B_cap = m·G`: what the drawn sealers of a convicting proof share, first, out of the bounty.
    pub bounty_cap: u64,
}

/// **Table 27** rows (key `(kind, digest)`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum VerifierPayRowV1 {
    /// Key `(0, job)`: the job's check-fee escrow, reserved on its poster's bond.
    FeeEscrow { poster: Digest, amount: u64 } = 0,
    /// Key `(1, claim)`: the claim's drawn slots (`paid` once its fee was paid or returned) and the attestation deadline. Kept until the
    /// claim's served demand bonds are settled (A-DEM reads it).
    Draw { job: Digest, poster: Digest, fee: u64, slots: Vec<(Digest, bool)>, deadline_daa: u64 } = 1,
    /// Key `(2, claim)`: a pre-Final default's demanders' share, still reserved on the producer's bond; `burned` is what the default
    /// itself collected (and burned).
    HeldShare { held: u64, burned: u64, demanders: Vec<Digest> } = 2,
}

fn settle(out: &mut Vec<LedgerEventV1>, bond: Digest, amount: u64, kind: SettlementKindV1, claim: Option<Digest>) {
    if amount > 0 {
        out.push(LedgerEventV1::Settlement(SettlementInstructionV1 { bond, amount, kind, claim }));
    }
}

impl KernelLedgerV1 {
    /// The verifier-pay terms where they are in force at this ledger's DAA.
    pub fn verifier_pay_in_force(&self) -> Option<VerifierPayPolicyV1> {
        self.verifier_pay_policy.filter(|p| self.daa >= p.activation_daa)
    }

    /// The check-fee escrow a job posted now must add to the GAP-5 escrow (0 below the fence).
    pub(crate) fn check_fee_escrow_v1(&self) -> u64 {
        self.verifier_pay_in_force().map_or(0, |p| p.check_fee.saturating_mul(u64::from(p.slots)))
    }

    /// Open a job's check-fee escrow (the poster's affordability was checked with the GAP-5 escrow).
    pub(crate) fn open_check_fee_escrow_v1(&mut self, poster: &Digest, job: Digest, out: &mut Vec<LedgerEventV1>) {
        let amount = self.check_fee_escrow_v1();
        if amount == 0 {
            return;
        }
        if let Some(b) = self.bonds.get_mut(poster) {
            b.reserved += amount;
        }
        self.verifier_pay.insert((VERIFIER_PAY_FEE_ESCROW_V1, job), VerifierPayRowV1::FeeEscrow { poster: *poster, amount });
        settle(out, *poster, amount, SettlementKindV1::ReserveJobEscrow, None);
    }

    /// Return a job's unused check-fee escrow (the GAP-5 idle-escrow rule returns the job's escrow at the same time).
    pub(crate) fn return_check_fee_escrow_v1(&mut self, job: &Digest, out: &mut Vec<LedgerEventV1>) {
        if let Some(VerifierPayRowV1::FeeEscrow { poster, amount }) = self.verifier_pay.remove(&(VERIFIER_PAY_FEE_ESCROW_V1, *job)) {
            if let Some(b) = self.bonds.get_mut(&poster) {
                b.reserved = b.reserved.saturating_sub(amount);
            }
            settle(out, poster, amount, SettlementKindV1::ReleaseJobEscrow, None);
        }
    }

    /// **The post-commit draw for `claim` landed** (the consumer's call with OPVB's v3 `ClaimVerification` draw; `slots` empty: the
    /// claim is not checked). The job's fee escrow becomes the claim's: `check_fee` per drawn slot, the rest returned to the poster.
    pub fn assign_drawn_v1(&mut self, claim: &Digest, slots: &[Digest]) -> Result<Vec<LedgerEventV1>, KernelRefusalV1> {
        const NAME: &str = "AssignDrawn";
        let rule = |why: &str| KernelRefusalV1::rule(NAME, why);
        let p = self.verifier_pay_in_force().ok_or_else(|| rule("palw_verifier_pay_v1 is not in force"))?;
        if slots.len() > usize::from(p.slots) {
            return Err(rule("more drawn slots than the escrow covers"));
        }
        if self.verifier_pay.contains_key(&(VERIFIER_PAY_DRAW_V1, *claim)) {
            return Err(rule("the claim's draw already landed"));
        }
        let row = self.claims.get(claim).ok_or_else(|| rule("no such claim"))?;
        let deadline_daa = match &row.life.state {
            ClaimStateV1::Checking { deadline_daa, .. } => *deadline_daa,
            ClaimStateV1::ProbabilisticPass { window_end_daa, .. } | ClaimStateV1::Challengeable { window_end_daa, .. } => {
                *window_end_daa
            }
            ClaimStateV1::Disputed { resume, .. } => match &**resume {
                ClaimStateV1::Checking { deadline_daa, .. } => *deadline_daa,
                ClaimStateV1::ProbabilisticPass { window_end_daa, .. } | ClaimStateV1::Challengeable { window_end_daa, .. } => {
                    *window_end_daa
                }
                _ => return Err(rule("the claim is past its check window")),
            },
            _ => return Err(rule("the claim is past its check window")),
        };
        let job = row.job_id;
        let Some(VerifierPayRowV1::FeeEscrow { poster, amount }) = self.verifier_pay.remove(&(VERIFIER_PAY_FEE_ESCROW_V1, job)) else {
            return Err(rule("the claim's job has no check-fee escrow"));
        };
        let mut out = Vec::new();
        let kept = p.check_fee.saturating_mul(slots.len() as u64).min(amount);
        let back = amount - kept;
        if let Some(b) = self.bonds.get_mut(&poster) {
            b.reserved = b.reserved.saturating_sub(back);
        }
        settle(&mut out, poster, back, SettlementKindV1::ReleaseJobEscrow, Some(*claim));
        if !slots.is_empty() {
            let draw = VerifierPayRowV1::Draw {
                job,
                poster,
                fee: kept / slots.len() as u64,
                slots: slots.iter().map(|s| (*s, false)).collect(),
                deadline_daa,
            };
            self.verifier_pay.insert((VERIFIER_PAY_DRAW_V1, *claim), draw);
        }
        Ok(out)
    }

    /// **A drawn slot's attestation** (the consumer's call; M\*-49 item 3): every unpaid slot `verifier` holds on `claim` is paid
    /// `check_fee` out of the poster's escrow, if the deadline has not passed.
    pub fn attest_drawn_v1(&mut self, claim: &Digest, verifier: &Digest) -> Result<Vec<LedgerEventV1>, KernelRefusalV1> {
        const NAME: &str = "AttestDrawn";
        let rule = |why: &str| KernelRefusalV1::rule(NAME, why);
        let daa = self.daa;
        let Some(VerifierPayRowV1::Draw { slots, deadline_daa, .. }) = self.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, *claim)) else {
            return Err(rule("no draw for this claim"));
        };
        if daa > *deadline_daa {
            return Err(rule("past the attestation deadline"));
        }
        if !slots.iter().any(|(s, paid)| s == verifier && !paid) {
            return Err(rule("the bond holds no unpaid drawn slot of this claim"));
        }
        let mut out = Vec::new();
        self.settle_draw_slots_v1(claim, Some(verifier), true, &mut out);
        Ok(out)
    }

    /// Pay (`pay`) or return the fee of the unpaid slots of `claim`'s draw held by `only` (every unpaid slot with `None`).
    fn settle_draw_slots_v1(&mut self, claim: &Digest, only: Option<&Digest>, pay: bool, out: &mut Vec<LedgerEventV1>) {
        let key = (VERIFIER_PAY_DRAW_V1, *claim);
        let Some(VerifierPayRowV1::Draw { poster, fee, slots, .. }) = self.verifier_pay.get_mut(&key) else { return };
        let (poster, fee) = (*poster, *fee);
        let mut due = Vec::new();
        for (s, paid) in slots.iter_mut() {
            if !*paid && only.is_none_or(|o| o == s) {
                *paid = true;
                due.push(*s);
            }
        }
        for s in due {
            let collateral = self.bonds.get(&poster).map_or(0, |b| b.collateral);
            let taken = if pay { fee.min(collateral) } else { 0 };
            if let Some(b) = self.bonds.get_mut(&poster) {
                b.reserved = b.reserved.saturating_sub(fee);
                b.collateral -= taken;
            }
            settle(out, poster, taken, SettlementKindV1::PayJobEscrow, Some(*claim));
            settle(out, poster, fee - taken, SettlementKindV1::ReleaseJobEscrow, Some(*claim));
            if taken > 0 {
                settle(out, s, taken, SettlementKindV1::FinalReward, Some(*claim));
                out.push(LedgerEventV1::CheckFeePaid { claim: *claim, verifier: s, amount: taken });
            }
        }
    }

    /// **Pay on fate** (the closing tick): a draw whose claim was convicted or defaulted by its producer before the deadline pays every
    /// unpaid slot; a draw whose claim ended otherwise, or whose deadline passed, returns the unpaid fees. A settled draw is dropped once
    /// the claim holds nothing and has no served demand bond left (A-DEM read it until then).
    pub(crate) fn tick_verifier_pay_v1(&mut self, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        // A job's check-fee escrow no draw can still take: its reward escrow is gone (spent at a Final, or returned idle).
        let idle: Vec<Digest> = self
            .verifier_pay
            .keys()
            .filter(|(kind, job)| *kind == VERIFIER_PAY_FEE_ESCROW_V1 && !self.job_escrows.contains_key(job))
            .map(|(_, job)| *job)
            .collect();
        for job in idle {
            self.return_check_fee_escrow_v1(&job, out);
        }
        let draws: Vec<(Digest, u64)> = self
            .verifier_pay
            .iter()
            .filter_map(|((kind, claim), row)| match row {
                VerifierPayRowV1::Draw { deadline_daa, .. } if *kind == VERIFIER_PAY_DRAW_V1 => Some((*claim, *deadline_daa)),
                _ => None,
            })
            .collect();
        for (claim, deadline) in draws {
            let (fate, ended) = match self.claims.get(&claim) {
                None => (false, true),
                Some(r) => match &r.life.state {
                    ClaimStateV1::Convicted { daa: at } => (*at <= deadline, true),
                    ClaimStateV1::Unavailable { daa: at, producer_defaulted: true } => (*at <= deadline, true),
                    s => (false, s.is_terminal() || r.convicted),
                },
            };
            if fate {
                self.settle_draw_slots_v1(&claim, None, true, out);
            } else if ended || daa > deadline {
                self.settle_draw_slots_v1(&claim, None, false, out);
            }
            let done = self.claims.get(&claim).is_none_or(|r| r.reserved == 0)
                && !self.served_demands.keys().any(|(c, _, _)| *c == claim)
                && matches!(
                    self.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, claim)),
                    Some(VerifierPayRowV1::Draw { slots, .. }) if slots.iter().all(|(_, paid)| *paid)
                );
            if done {
                self.verifier_pay.remove(&(VERIFIER_PAY_DRAW_V1, claim));
            }
        }
    }

    /// A-DEM: is `bond` a drawn slot of `claim` (its served demand bond is refunded, never burned)?
    pub(crate) fn is_drawn_slot_v1(&self, claim: &Digest, bond: &Digest) -> bool {
        matches!(self.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, *claim)), Some(VerifierPayRowV1::Draw { slots, .. }) if slots.iter().any(|(s, _)| s == bond))
    }

    /// The drawn slots of `claim` among `sealers` (each once, in slot order).
    pub(crate) fn drawn_sealers_v1(&self, claim: &Digest, sealers: &[Digest]) -> Vec<Digest> {
        let Some(VerifierPayRowV1::Draw { slots, .. }) = self.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, *claim)) else {
            return Vec::new();
        };
        let mut out: Vec<Digest> = Vec::new();
        for (s, _) in slots {
            if sealers.contains(s) && !out.contains(s) {
                out.push(*s);
            }
        }
        out
    }

    /// O2: the held share of `claim` (`(held, burned at the default)`), if any.
    pub(crate) fn held_share_v1(&self, claim: &Digest) -> Option<(u64, u64)> {
        match self.verifier_pay.get(&(VERIFIER_PAY_HELD_SHARE_V1, *claim)) {
            Some(VerifierPayRowV1::HeldShare { held, burned, .. }) => Some((*held, *burned)),
            _ => None,
        }
    }

    /// O2: hold a pre-Final default's demanders' share (still reserved on the producer's bond).
    pub(crate) fn hold_share_v1(
        &mut self,
        claim: Digest,
        held: u64,
        burned: u64,
        demanders: Vec<Digest>,
        out: &mut Vec<LedgerEventV1>,
    ) {
        self.verifier_pay.insert((VERIFIER_PAY_HELD_SHARE_V1, claim), VerifierPayRowV1::HeldShare { held, burned, demanders });
        out.push(LedgerEventV1::DefaultShareHeld { claim, held, outcome: "held" });
    }

    /// O2: a conviction took the held share into the one pool (it was slashed with the rest of the reservation).
    pub(crate) fn pool_held_share_v1(&mut self, claim: &Digest, out: &mut Vec<LedgerEventV1>) {
        if let Some(VerifierPayRowV1::HeldShare { held, .. }) = self.verifier_pay.remove(&(VERIFIER_PAY_HELD_SHARE_V1, *claim)) {
            out.push(LedgerEventV1::DefaultShareHeld { claim: *claim, held, outcome: "joined_the_pool" });
        }
    }

    /// O2: the liability horizon ended with no conviction — slash the held share now and pay it to the demanders (equally; the
    /// remainder burned). Returns what was slashed (the claim's reservation drops by `held`).
    pub(crate) fn pay_held_share_v1(&mut self, claim: &Digest, producer: Digest, out: &mut Vec<LedgerEventV1>) -> u64 {
        let Some(VerifierPayRowV1::HeldShare { held, demanders, .. }) =
            self.verifier_pay.remove(&(VERIFIER_PAY_HELD_SHARE_V1, *claim))
        else {
            return 0;
        };
        let collateral = self.bonds.get(&producer).map_or(0, |b| b.collateral);
        let taken = held.min(collateral);
        if let Some(b) = self.bonds.get_mut(&producer) {
            b.reserved = b.reserved.saturating_sub(held);
            b.collateral -= taken;
        }
        let share = if demanders.is_empty() { 0 } else { taken / demanders.len() as u64 };
        let burn = taken - share * demanders.len() as u64;
        self.burned += burn;
        settle(out, producer, taken, SettlementKindV1::SlashDefault, Some(*claim));
        settle(out, producer, held - taken, SettlementKindV1::ReleaseClaim, Some(*claim));
        for d in &demanders {
            settle(out, *d, share, SettlementKindV1::DemanderShare, Some(*claim));
        }
        settle(out, producer, burn, SettlementKindV1::Burn, Some(*claim));
        out.push(LedgerEventV1::DefaultShareHeld { claim: *claim, held: taken, outcome: "paid_to_demanders" });
        held
    }
}

/// The table's map type (the ledger field).
pub type VerifierPayTableV1 = BTreeMap<(u8, Digest), VerifierPayRowV1>;
