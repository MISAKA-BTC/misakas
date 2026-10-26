//! **G14 lane CLOCK: the kernel route claim's notice / hard clock** (X3, `palw_claim_hard_deadline_v1`, extended to the kernel route).
//!
//! ADR-0198 fixes every claim's court clock on its acceptance `a`: an accusation is admissible at `a ≤ d ≤ a + 3,120` (inclusive
//! notice) and every dispute resolves by `a + 10,000` (hard, never extended). The kernel ledger's own interim windows (a 200-DAA
//! `liability_daa` after Final or after a pre-Final default) cut an outsider's `FileProof` off long before that. Past X3 (the
//! consumer injects [`crate::ledger::KernelLedgerV1::claim_clock_from_daa`] at every load; never a row, never in the root), a claim
//! COMMITTED at or after the fence keeps its liability horizon at least
//!
//! `committed + NOTICE + court_deadline + proof_grace`
//!
//! so a proof (or a demand whose whole path must fit the horizon) filed at any `d ≤ a + NOTICE` still convicts, and the horizon stays
//! inside `a + HARD` ([`claim_clock_floor_v1`] refuses a policy that would not). The reservation, a held Final reward and the
//! signers' locks all follow `liability_until`, so they are held for the same span. Claims committed below the fence, and every
//! ledger with no clock injected, keep the historical horizon byte for byte.
//!
//! Hashes, signatures and DAA arithmetic only — no proof of computation (PRINCIPLES §8), no per-model rule (§9).

use crate::ledger::{KernelLedgerV1, LedgerPolicyV1};

/// ADR-0198's inclusive notice window (DAA after acceptance). Equal to the consensus `NOTICE_WINDOW_DAA`.
pub const KERNEL_CLAIM_NOTICE_DAA_V1: u64 = 3_120;
/// ADR-0198's hard window (DAA after acceptance; never extended). Equal to the consensus `DISPUTE_WINDOW_DAA`.
pub const KERNEL_CLAIM_HARD_DAA_V1: u64 = 10_000;

/// **The liability floor of a claim committed at `committed_daa`** under the clock: `committed + NOTICE + court + grace`. `Err` when
/// the policy's demand path would push it past `committed + HARD` (the consumer then refuses to arm the clock).
pub fn claim_clock_floor_v1(policy: &LedgerPolicyV1, committed_daa: u64) -> Result<u64, &'static str> {
    let tail = policy.court_deadline_daa.checked_add(policy.proof_grace_daa).ok_or("claim clock: overflow")?;
    let span = KERNEL_CLAIM_NOTICE_DAA_V1.checked_add(tail).ok_or("claim clock: overflow")?;
    if span > KERNEL_CLAIM_HARD_DAA_V1 {
        return Err("claim clock: notice + court deadline + proof grace exceeds the hard window");
    }
    Ok(committed_daa.saturating_add(span))
}

/// The exposure (DAA) during which an outsider's filing must reach a clocked claim: commit → liability floor.
pub fn claim_clock_exposure_daa_v1(policy: &LedgerPolicyV1) -> Result<u64, &'static str> {
    claim_clock_floor_v1(policy, 0)
}

impl KernelLedgerV1 {
    /// **The clock's floor on `claim`'s liability horizon**, `None` where the clock is not injected or the claim was committed below
    /// it. A policy the floor refuses is refused by the consumer's validation; here it is clamped to the hard window, never dropped.
    pub fn claim_clock_floor_of_v1(&self, claim: &crate::hash::Digest) -> Option<u64> {
        let from = self.claim_clock_from_daa?;
        let row = self.claims.get(claim)?;
        if row.committed_daa < from {
            return None;
        }
        Some(
            claim_clock_floor_v1(&self.policy, row.committed_daa)
                .unwrap_or(row.committed_daa.saturating_add(KERNEL_CLAIM_HARD_DAA_V1)),
        )
    }
}
