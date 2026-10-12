//! **The sealed-bounty rule for kernel filings** (RFC-0002 P0 residual, lane RFC02-T, 2026-10-11) — consumer-injected past
//! `palw_model_refutation_seal_v1`, never part of the state's policy or root. Below it every rule here is the historical one.
//!
//! **The failure it closes.** GAP-R7 pays the bounty of a conviction to the earliest bond holding a seal of the convicting proof's
//! EXACT bytes, else to the filer. A `ProsecutionV1` is not a canonical encoding of its conviction: a `Kernel` fault proof's
//! `scalar` field is read only for a MatMul scalar fault, so any other kind convicts identically with any `scalar`; a segmented
//! filing may carry a redundant authentic prompt tile. A copyist that reads an honest finder's filing in the mempool re-encodes it,
//! is included first, matches no seal, and is paid as the filer; the finder's own filing is then a `Duplicate`.
//!
//! **The rule past the fence.** A filing still convicts exactly as before (any authentic proof, sealed or not: the court's liveness
//! is unchanged). Its bounty is paid at once only when some bond holds a mature seal of the convicting bytes (GAP-R7 unchanged).
//! Otherwise the filer's share is **held** in table 27 (kind 4, `VerifierPayRowV1::HeldBounty`) for `contest_daa`: in that window any bond whose OWN seal on the claim was
//! posted before the conviction, is mature and opens to the proof it now files, may file that proof against the convicted claim; the
//! court runs (fee and budget as any filing), and an authentic one takes the held share. A copyist learns the bytes only from the
//! finder's filing, so it cannot hold a seal of them posted before the conviction. With no such reveal by the window's end the held
//! share goes to the original filer (an honest unsealed finder is paid, one window late). The poster's own operator gains nothing
//! either way: the slash and the 51 % burn are unchanged.
//!
//! Residuals (stated, not removed): a finder whose reveal is censored for the whole window loses the share to the filer; a finder's
//! seal must still be live (`seal_ttl_daa` from its posting) when it reveals.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::Digest;
use crate::ledger::{KernelLedgerV1, LedgerEventV1, settle};
use crate::route::{ProsecutionV1, proof_digest_v1, proof_seal_of_digest_v1};
use crate::settle::SettlementKindV1;
use crate::verifier_pay::{VERIFIER_PAY_HELD_BOUNTY_V1, VerifierPayRowV1};

/// **The sealed-bounty terms**, consumer-injected (never part of the state or its root).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SealedBountyPolicyV1 {
    pub activation_daa: u64,
    /// How long an unsealed conviction's bounty is held for an earlier sealer's reveal.
    pub contest_daa: u64,
}

/// **Table 31's row of one convicted claim**: the held share, who filed the convicting proof, and the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HeldBountyRowV1 {
    pub filer: Digest,
    /// The convicted producer: the held share stays reserved on its bond until the contest resolves.
    pub producer: Digest,
    pub amount: u64,
    pub convicted_daa: u64,
    pub until_daa: u64,
}

impl KernelLedgerV1 {
    /// The sealed-bounty terms, where they are in force at the ledger's clock.
    pub fn sealed_bounty_in_force(&self) -> Option<SealedBountyPolicyV1> {
        self.sealed_bounty_policy.filter(|p| self.daa >= p.activation_daa)
    }

    /// Does any bond hold a mature seal of these exact proof bytes on `claim` (GAP-R7's beneficiary exists)?
    pub(crate) fn proof_is_sealed_v1(&self, claim: &Digest, proof: &ProsecutionV1) -> bool {
        let digest = proof_digest_v1(proof);
        let delay = self.policy.claim_seal_delay_daa;
        self.proof_seals.range((*claim, [0u8; 64])..=(*claim, [0xFFu8; 64])).any(|((_, accuser), row)| {
            row.daa.saturating_add(delay) <= self.daa && row.seal == proof_seal_of_digest_v1(claim, accuser, &digest)
        })
    }

    /// Hold `amount` of `claim`'s bounty for the contest window (called by the conviction instead of paying the filer).
    pub(crate) fn hold_bounty_v1(
        &mut self,
        claim: &Digest,
        filer: &Digest,
        producer: &Digest,
        amount: u64,
        out: &mut Vec<LedgerEventV1>,
    ) {
        let p = self.sealed_bounty_in_force().expect("held only past the fence");
        let until_daa = self.daa.saturating_add(p.contest_daa);
        let row = HeldBountyRowV1 { filer: *filer, producer: *producer, amount, convicted_daa: self.daa, until_daa };
        self.put_held_bounty_v1(claim, row);
        out.push(LedgerEventV1::BountyHeld { claim: *claim, filer: *filer, amount, until_daa });
    }

    /// May `accuser` contest `claim`'s held bounty with `proof` now? (Its own seal, posted before the conviction, mature, opening to
    /// exactly these bytes, inside the window.) The court has not run yet.
    pub(crate) fn held_bounty_contest_allows_v1(
        &self,
        accuser: &Digest,
        claim: &Digest,
        proof: &ProsecutionV1,
    ) -> Result<(), &'static str> {
        let held = self.held_bounty_v1(claim).ok_or("the claim is convicted and holds no bounty")?;
        if self.daa >= held.until_daa {
            return Err("the held bounty's contest window has closed");
        }
        let row = self.proof_seals.get(&(*claim, *accuser)).ok_or("no seal of this accuser on the claim")?;
        if row.daa >= held.convicted_daa {
            return Err("the accuser's seal was not posted before the conviction");
        }
        if row.daa.saturating_add(self.policy.claim_seal_delay_daa) > self.daa {
            return Err("the accuser's seal is not mature");
        }
        if row.seal != proof_seal_of_digest_v1(claim, accuser, &proof_digest_v1(proof)) {
            return Err("the proof does not open the accuser's seal");
        }
        Ok(())
    }

    /// An authentic contesting proof: the held share is the contester's, once; the claim's seals are spent.
    pub(crate) fn pay_held_bounty_v1(&mut self, claim: &Digest, accuser: &Digest, out: &mut Vec<LedgerEventV1>) {
        if let Some(held) = self.take_held_bounty_v1(claim) {
            // The share was kept reserved on the producer's bond: slash it now and pay it, one balanced step.
            let amount = match self.bonds.get_mut(&held.producer) {
                Some(b) => {
                    let amount = held.amount.min(b.collateral).min(b.reserved);
                    b.reserved -= amount;
                    b.collateral -= amount;
                    amount
                }
                None => 0,
            };
            settle(out, held.producer, amount, SettlementKindV1::SlashFraud, Some(*claim));
            settle(out, *accuser, amount, SettlementKindV1::AccuserReward, Some(*claim));
            out.push(LedgerEventV1::HeldBountyPaid { claim: *claim, accuser: *accuser, amount });
        }
        self.spend_proof_seals_v1(claim);
    }

    /// Windows that closed with no earlier sealer's reveal: the held share is the original filer's.
    pub(crate) fn expire_held_bounties_v1(&mut self, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        let due: Vec<(Digest, Digest)> =
            self.held_bounties_v1().into_iter().filter(|(_, h)| daa >= h.until_daa).map(|(c, h)| (c, h.filer)).collect();
        for (claim, filer) in due {
            self.pay_held_bounty_v1(&claim, &filer, out);
        }
    }

    /// The held bounty of `claim` (table 27, kind 4), if any.
    pub fn held_bounty_v1(&self, claim: &Digest) -> Option<HeldBountyRowV1> {
        match self.verifier_pay.get(&(VERIFIER_PAY_HELD_BOUNTY_V1, *claim)) {
            Some(VerifierPayRowV1::HeldBounty { filer, producer, amount, convicted_daa, until_daa }) => Some(HeldBountyRowV1 {
                filer: *filer,
                producer: *producer,
                amount: *amount,
                convicted_daa: *convicted_daa,
                until_daa: *until_daa,
            }),
            _ => None,
        }
    }

    /// Every held bounty, by claim.
    pub fn held_bounties_v1(&self) -> Vec<(Digest, HeldBountyRowV1)> {
        self.verifier_pay
            .range((VERIFIER_PAY_HELD_BOUNTY_V1, [0u8; 64])..=(VERIFIER_PAY_HELD_BOUNTY_V1, [0xFFu8; 64]))
            .filter_map(|((_, claim), _)| self.held_bounty_v1(claim).map(|h| (*claim, h)))
            .collect()
    }

    fn put_held_bounty_v1(&mut self, claim: &Digest, h: HeldBountyRowV1) {
        let row = VerifierPayRowV1::HeldBounty {
            filer: h.filer,
            producer: h.producer,
            amount: h.amount,
            convicted_daa: h.convicted_daa,
            until_daa: h.until_daa,
        };
        self.verifier_pay.insert((VERIFIER_PAY_HELD_BOUNTY_V1, *claim), row);
    }

    fn take_held_bounty_v1(&mut self, claim: &Digest) -> Option<HeldBountyRowV1> {
        let h = self.held_bounty_v1(claim)?;
        self.verifier_pay.remove(&(VERIFIER_PAY_HELD_BOUNTY_V1, *claim));
        Some(h)
    }

    pub(crate) fn spend_proof_seals_v1(&mut self, claim: &Digest) {
        let spent: Vec<(Digest, Digest)> =
            self.proof_seals.range((*claim, [0u8; 64])..=(*claim, [0xFFu8; 64])).map(|(k, _)| *k).collect();
        for k in spent {
            self.proof_seals.remove(&k);
        }
    }
}
