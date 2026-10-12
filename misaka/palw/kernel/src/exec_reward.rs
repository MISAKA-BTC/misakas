//! Trusted EXEC consumer hooks. Computation Final does not authorize a reward before root Final.
//! Holds have a finite consumer deadline; the consumer must settle or cancel them in its rooted journal.

use crate::hash::Digest;
use crate::ledger::{ClaimBodyV1, KernelLedgerV1, LedgerEventV1, settle};
use crate::lifecycle::ClaimStateV1;
use crate::settle::SettlementKindV1;
use borsh::{BorshDeserialize, BorshSerialize};

/// 256 consumer roots, each with at most 128 slices and one prefix.
pub const MAX_EXEC_REWARD_HOLDS_V1: usize = 256 * 129;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecRewardHoldV1 {
    pub owner_root: Digest,
    pub expires_daa: u64,
}

impl KernelLedgerV1 {
    pub fn validate_exec_reward_holds(&self) -> Result<(), String> {
        if self.exec_reward_holds.len() > MAX_EXEC_REWARD_HOLDS_V1 {
            return Err("too many EXEC reward holds".into());
        }
        for claim in self.exec_reward_holds.keys() {
            let row = self.claims.get(claim).ok_or("an EXEC reward hold has no claim")?;
            if row.rewarded || !matches!(row.body, ClaimBodyV1::Program { .. }) {
                return Err("an EXEC reward hold names paid or unsupported work".into());
            }
            if row.liability_until.is_none_or(|until| until < self.exec_reward_holds[claim].expires_daa) {
                return Err("an EXEC reward hold outlives its claim liability".into());
            }
        }
        Ok(())
    }

    /// No route object can call this: only the consumer that admitted the root and its execution binding.
    pub fn bind_exec_reward(&mut self, root: Digest, claim: Digest, expires_daa: u64) -> Result<(), String> {
        self.bind_exec_reward_until(root, claim, expires_daa, expires_daa)
    }

    /// The consumer freezes the latest root Final plus its liability/court path, before any escrow or exposure write.
    /// This extends an existing reservation; it never recreates collateral a previous horizon released.
    pub fn bind_exec_reward_until(
        &mut self,
        root: Digest,
        claim: Digest,
        expires_daa: u64,
        liability_floor: u64,
    ) -> Result<(), String> {
        let row = self.claims.get(&claim).ok_or("the EXEC reward claim is missing")?;
        if row.rewarded
            || row.convicted
            || row.reserved == 0
            || matches!(
                row.life.state,
                ClaimStateV1::Convicted { .. } | ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. }
            )
        {
            return Err("paid or failed work cannot attach to EXEC".into());
        }
        if matches!(row.life.state, ClaimStateV1::Final { .. }) && row.liability_until.is_none_or(|until| self.daa > until) {
            return Err("expired work cannot reopen its liability for EXEC".into());
        }
        if !matches!(row.body, ClaimBodyV1::Program { .. }) || expires_daa <= self.daa || liability_floor < expires_daa {
            return Err("unsupported or expired EXEC reward hold".into());
        }
        let hold = ExecRewardHoldV1 { owner_root: root, expires_daa };
        if let Some(old) = self.exec_reward_holds.get(&claim) {
            if *old != hold {
                return Err("the EXEC reward claim already belongs to another root".into());
            }
        }
        if !self.exec_reward_holds.contains_key(&claim) && self.exec_reward_holds.len() >= MAX_EXEC_REWARD_HOLDS_V1 {
            return Err("too many EXEC reward holds".into());
        }
        self.exec_reward_holds.insert(claim, hold);
        let row = self.claims.get_mut(&claim).expect("checked");
        row.liability_until = Some(row.liability_until.unwrap_or(0).max(liability_floor));
        Ok(())
    }

    /// The cap is the executor's remaining share of the root's ONE funded allocation.
    /// Unspent escrow is refunded; actual paid fees replace native issuance within that share.
    pub fn finish_exec_reward(&mut self, root: Digest, claim: Digest, cap: u64, out: &mut Vec<LedgerEventV1>) -> Result<u64, String> {
        let hold = self.exec_reward_holds.get(&claim).ok_or("the EXEC reward hold is missing")?;
        let row = self.claims.get(&claim).ok_or("the EXEC reward claim is missing")?;
        if hold.owner_root != root
            || self.daa >= hold.expires_daa
            || row.convicted
            || row.rewarded
            || (row.reserved == 0 && row.liability_until.is_some_and(|until| self.daa <= until))
            || !matches!(row.life.state, ClaimStateV1::Final { .. })
        {
            return Err("the EXEC reward is not eligible for root Final".into());
        }
        let (job, producer) = (row.job_id, row.producer);
        self.exec_reward_holds.remove(&claim);
        let paid = self.pay_from_job_escrow_capped(&job, &claim, producer, cap, out);
        self.claims.get_mut(&claim).expect("checked").rewarded = paid > 0;
        Ok(paid)
    }

    /// Void/expiry releases poster escrow without debiting collateral or paying a producer.
    /// It never shortens the computation claim's consumer-frozen fraud liability.
    pub fn cancel_exec_reward(&mut self, root: Digest, claim: Digest, out: &mut Vec<LedgerEventV1>) -> Result<(), String> {
        let Some(hold) = self.exec_reward_holds.get(&claim) else { return Ok(()) };
        if hold.owner_root != root {
            return Err("the EXEC reward hold belongs to another root".into());
        }
        let job = self.claims.get(&claim).ok_or("the EXEC reward claim is missing")?.job_id;
        self.exec_reward_holds.remove(&claim);
        if let Some(escrow) = self.job_escrows.remove(&job) {
            if let Some(bond) = self.bonds.get_mut(&escrow.poster) {
                bond.reserved = bond.reserved.saturating_sub(escrow.amount);
            }
            settle(out, escrow.poster, escrow.amount, SettlementKindV1::ReleaseJobEscrow, Some(claim));
        }
        Ok(())
    }
}
