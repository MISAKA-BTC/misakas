//! ADR-0037 Decision 4 / ADR-0038 Decision C: future-anchor panel selection — the audited
//! lottery, with its inputs finally bound.
//!
//! The audit's finding was never that [`crate::palw_schedule::select_replay_panel_v1`] was
//! wrong — the lottery is deterministic and executor-excluding by construction. The finding
//! was that its CALLER hardcoded eligibility. So the selection function is kept, and this
//! module is the caller that cannot cheat:
//!
//! * the ticket anchor is the **V3 panel seed** ([`crate::palw_job_identity::palw_panel_seed_v3`]),
//!   which binds network, job, commitment root, a block finalized AFTER the commitment, and
//!   the eligible-set snapshot root at that anchor — hardcoded eligibility no longer
//!   reproduces the seed;
//! * eligibility is decided HERE from the chain facts the caller assembles per candidate:
//!   `Active` bonds only (`Pending`/`Unbonding`/`Slashed` are out), exact class, not frozen,
//!   never the executor, one voice per bond outpoint and (best-effort) per operator root;
//! * the panel comes back as **bond outpoints** — the only payee identity the mint layer
//!   accepts (I4) — alongside the validator ids the lottery drew.
//!
//! Consensus-inert until the ADR-0038 change set wires and activates together.

use crate::dns_finality::BondStatus;
use crate::palw_job_identity::palw_panel_seed_v3;
use crate::palw_schedule::{PalwPanelCandidateV1, select_replay_panel_v1};
use crate::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

/// One candidate as the caller's chain view knows it at the anchor. The flags are chain
/// facts at the snapshot, not self-declarations (ADR-0037 Decision 8: self-declared
/// capacity buys nothing).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelCandidateV3 {
    pub validator_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    pub runtime_class_id: Hash64,
    pub bond_status: BondStatus,
    pub class_frozen: bool,
    /// Operator grouping fact when the chain knows one (shared registration root); `None`
    /// when unknown — dedup by operator is best-effort by design, dedup by bond is not.
    pub operator_root: Option<Hash64>,
}

/// One drawn panel seat: the lottery's identity and the payee identity, bound together at
/// selection so no later lookup can diverge (the audited payee-by-pubkey-hash bug becomes
/// unrepresentable downstream).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelSeatV3 {
    pub validator_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
}

/// The V3 panel draw. Filters to the ADR-0037 Decision 4 eligible set, dedups
/// deterministically (candidates ordered by `validator_id`; the first voice per bond
/// outpoint and per known operator root survives), then runs the audited v1 lottery with
/// the V3 seed as its anchor. A smaller-than-`q` eligible set yields a smaller panel —
/// whether that panel may license anything is the weight ramp's question, not this one's.
#[allow(clippy::too_many_arguments)]
pub fn select_job_panel_v3(
    network_id: &[u8],
    job_id: Hash64,
    commitment_root: Hash64,
    future_anchor_block_hash: Hash64,
    eligible_set_snapshot_root: Hash64,
    executor_id: &Hash64,
    execution_class_id: &Hash64,
    candidates: &[PalwPanelCandidateV3],
    q: usize,
) -> Vec<PalwPanelSeatV3> {
    let seed = palw_panel_seed_v3(network_id, job_id, commitment_root, future_anchor_block_hash, eligible_set_snapshot_root);

    // Deterministic pre-filter and dedup: order by validator_id so every node keeps the
    // same single voice per bond outpoint / operator root, regardless of input order.
    let mut ordered: Vec<&PalwPanelCandidateV3> = candidates.iter().collect();
    ordered.sort_by(|a, b| a.validator_id.cmp(&b.validator_id).then_with(|| {
        (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
    }));
    let mut seen_bonds: BTreeSet<(Hash64, u32)> = BTreeSet::new();
    let mut seen_operators: BTreeSet<Hash64> = BTreeSet::new();
    let mut eligible: Vec<&PalwPanelCandidateV3> = Vec::new();
    for candidate in ordered {
        if candidate.bond_status != BondStatus::Active
            || candidate.class_frozen
            || candidate.runtime_class_id != *execution_class_id
            || candidate.validator_id == *executor_id
        {
            continue;
        }
        if !seen_bonds.insert((candidate.bond_outpoint.transaction_id, candidate.bond_outpoint.index)) {
            continue;
        }
        if let Some(operator) = candidate.operator_root
            && !seen_operators.insert(operator)
        {
            continue;
        }
        eligible.push(candidate);
    }

    // The audited lottery, with the V3 seed as its anchor. The v1 eligibility flags are
    // vacuously true here — eligibility was decided above from chain facts.
    let v1_candidates: Vec<PalwPanelCandidateV1> = eligible
        .iter()
        .map(|c| PalwPanelCandidateV1 {
            validator_id: c.validator_id,
            runtime_class_id: c.runtime_class_id,
            bonded: true,
            frozen: false,
        })
        .collect();
    let drawn = select_replay_panel_v1(&commitment_root, executor_id, &seed, execution_class_id, &v1_candidates, q);

    // Bind each drawn id back to ITS bond outpoint from the same eligible view.
    drawn
        .into_iter()
        .map(|validator_id| {
            let candidate = eligible.iter().find(|c| c.validator_id == validator_id).expect("drawn ids come from the eligible set");
            PalwPanelSeatV3 { validator_id, bond_outpoint: candidate.bond_outpoint }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NET: &[u8] = b"misaka-testnet-11";

    fn class() -> Hash64 {
        Hash64::from_u64_word(7)
    }

    fn candidate(seed: u64) -> PalwPanelCandidateV3 {
        PalwPanelCandidateV3 {
            validator_id: Hash64::from_u64_word(seed),
            bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(seed + 1000), 0),
            runtime_class_id: class(),
            bond_status: BondStatus::Active,
            class_frozen: false,
            operator_root: None,
        }
    }

    fn draw(candidates: &[PalwPanelCandidateV3], q: usize) -> Vec<PalwPanelSeatV3> {
        select_job_panel_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            Hash64::from_u64_word(4),
            &Hash64::from_u64_word(999),
            &class(),
            candidates,
            q,
        )
    }

    /// Only Active, unfrozen, exact-class, non-executor candidates are drawable; every
    /// excluded category yields an empty panel on its own.
    #[test]
    fn eligibility_is_the_decision_4_set() {
        for status in [BondStatus::Pending, BondStatus::Unbonding, BondStatus::Slashed] {
            let mut c = candidate(10);
            c.bond_status = status;
            assert!(draw(&[c], 3).is_empty(), "{status:?} must not be drawable");
        }
        let mut frozen = candidate(10);
        frozen.class_frozen = true;
        assert!(draw(&[frozen], 3).is_empty());
        let mut wrong_class = candidate(10);
        wrong_class.runtime_class_id = Hash64::from_u64_word(99);
        assert!(draw(&[wrong_class], 3).is_empty());
        let mut executor = candidate(10);
        executor.validator_id = Hash64::from_u64_word(999); // == executor_id in draw()
        assert!(draw(&[executor], 3).is_empty());
        assert_eq!(draw(&[candidate(10)], 3).len(), 1);
    }

    /// One voice per bond outpoint and per known operator root; unknown operators (None)
    /// never dedup against each other.
    #[test]
    fn dedup_is_by_bond_and_operator() {
        let mut duplicate_bond = candidate(20);
        duplicate_bond.bond_outpoint = candidate(10).bond_outpoint;
        assert_eq!(draw(&[candidate(10), duplicate_bond], 5).len(), 1);

        let mut op_a = candidate(10);
        op_a.operator_root = Some(Hash64::from_u64_word(500));
        let mut op_b = candidate(20);
        op_b.operator_root = Some(Hash64::from_u64_word(500));
        assert_eq!(draw(&[op_a, op_b], 5).len(), 1);

        assert_eq!(draw(&[candidate(10), candidate(20)], 5).len(), 2);
    }

    /// The draw is deterministic and input-order invariant, and each seat carries the bond
    /// outpoint of exactly its drawn validator (the I4 binding).
    #[test]
    fn draw_is_deterministic_and_binds_seats() {
        let pool: Vec<_> = (10..30).map(candidate).collect();
        let mut reversed = pool.clone();
        reversed.reverse();
        let a = draw(&pool, 3);
        let b = draw(&reversed, 3);
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
        for seat in &a {
            let source = pool.iter().find(|c| c.validator_id == seat.validator_id).unwrap();
            assert_eq!(seat.bond_outpoint, source.bond_outpoint);
        }
    }

    /// The V3 seed moves the panel: a different future anchor or snapshot root draws a
    /// different panel over a pool large enough that collision is implausible — hardcoded
    /// eligibility can no longer reproduce the draw.
    #[test]
    fn seed_binds_anchor_and_snapshot() {
        let pool: Vec<_> = (10..200).map(candidate).collect();
        let base = draw(&pool, 5);
        let other_anchor = select_job_panel_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(33),
            Hash64::from_u64_word(4),
            &Hash64::from_u64_word(999),
            &class(),
            &pool,
            5,
        );
        let other_snapshot = select_job_panel_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            Hash64::from_u64_word(44),
            &Hash64::from_u64_word(999),
            &class(),
            &pool,
            5,
        );
        assert_ne!(base, other_anchor);
        assert_ne!(base, other_snapshot);
    }

    /// A short pool draws a short panel (whether it licenses anything is the ramp's call).
    #[test]
    fn short_pool_draws_short_panel() {
        assert_eq!(draw(&[candidate(10), candidate(20)], 5).len(), 2);
        assert!(draw(&[], 5).is_empty());
    }
}
