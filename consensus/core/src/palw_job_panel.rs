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
use std::collections::{BTreeMap, BTreeSet};

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
/// The candidate set a block's panel is drawn from, assembled from chain state.
///
/// [`select_job_panel_v3`] takes candidates; this is where they come from. Three rules, each of
/// which decides a case the obvious assembly gets wrong:
///
/// * **A bond that is not `Active` at `anchor_daa` is not a candidate.** The anchor is the point
///   the draw is bound to, so it is the point eligibility is asked at — not the reading node's tip,
///   which would make the panel depend on when a node looked and hand two nodes different seats for
///   one block.
/// * **A bond with no capability declaration is EXCLUDED, never defaulted.** `runtime_class_id`
///   says which determinism class a validator has staked collateral on being able to run. A
///   validator that never declared one cannot be assigned to replay a class it may not have, and
///   assigning it anyway would manufacture no-shows against honest operators — the duty accounting
///   in `palw_facts` charges exactly the seats this function names.
/// * **The result is returned in canonical order.** `eligible_seats_v3` sorts internally, so this is
///   not needed for the draw — it is needed because `ActiveBondView::records()` is HashMap-ordered
///   and this type's own doc warns that hashing an unsorted slice is "a chain split waiting for two
///   nodes with different insertion histories". Returning sorted removes the footgun rather than
///   documenting it again.
///
/// **`bond_status` here is the TRUTH, and the credit path's assembler writes something else into
/// the same field.** `palw_credit::panel_seats_at_anchor_v3` uses it as an eligibility verdict —
/// `Active` iff the bond may be seated, `Slashed` otherwise — because it must carry an exclusion
/// the type cannot hold (every bond of the executor's OWNER, while the draw is only told the
/// executor's validator id). Both are correct for their own draw and neither may read the other's
/// value as a bond fact. Named at both sites rather than at one, because a reader arrives at
/// whichever they were sent to.
///
/// `operator_root` used to be a second, unintended divergence — `None` here against the owner hash
/// there — and that one was a defect (audit P0-7), now fixed. The `bond_status` split stays because
/// it carries something the type cannot express; the operator one carried nothing.
///
/// `class_frozen` is carried per candidate rather than filtered here: a frozen class is a fact the
/// draw weighs (ADR-0038 I10 froze it for a coverage gap, which is not the candidate's fault), and
/// dropping those candidates here would silently shrink the eligible set for a reason the lottery
/// is supposed to see.
pub fn palw_panel_candidates_v1<C, F>(
    bonds: &crate::dns_finality::ActiveBondView,
    anchor_daa: u64,
    runtime_class_of_bond: C,
    class_is_frozen: F,
) -> Vec<PalwPanelCandidateV3>
where
    C: Fn(&TransactionOutpoint) -> Option<Hash64>,
    F: Fn(&Hash64) -> bool,
{
    let mut out: Vec<PalwPanelCandidateV3> = bonds
        .records()
        .into_iter()
        .filter(|record| crate::dns_finality::is_bond_active_at(record, anchor_daa))
        .filter_map(|record| {
            let runtime_class_id = runtime_class_of_bond(&record.bond_outpoint)?;
            Some(PalwPanelCandidateV3 {
                validator_id: record.validator_pubkey_hash,
                bond_outpoint: record.bond_outpoint,
                runtime_class_id,
                bond_status: crate::dns_finality::effective_bond_status(&record, anchor_daa),
                class_frozen: class_is_frozen(&runtime_class_id),
                // Audit P0-7: the bond's OWNER, not `None`.
                //
                // This said the chain knows no operator grouping here and that `None` merely cost a
                // weaker dedup. Both halves were wrong. The chain does know one — `owner_pubkey_hash`
                // — and `palw_credit::panel_seats_at_anchor_v3` has been using exactly it all along,
                // so the two assemblers disagreed about the same panel. And the cost is not weaker
                // dedup: with `None` an operator splits its stake across k bonds and collects k
                // seats on one panel, which is the quorum the receipt count is supposed to measure,
                // bought rather than drawn.
                operator_root: Some(record.owner_pubkey_hash),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.validator_id.cmp(&b.validator_id).then_with(|| {
            (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
        })
    });
    out
}

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
    let eligible = eligible_seats_v3(candidates, executor_id, execution_class_id);
    select_from_eligible_v3(&seed, commitment_root, executor_id, execution_class_id, &eligible, q)
}

/// The ADR-0037 Decision 4 eligible set, in canonical order.
///
/// Ordering is by `(validator_id, bond_outpoint)` and the filter runs over that order, so the
/// single surviving voice per bond outpoint and per known operator root is the same on every node
/// regardless of how the caller assembled its slice. Callers must never re-sort the result or hash
/// their own input slice instead — `ActiveBondView::records()` is HashMap-ordered, so an unsorted
/// preimage is a chain split waiting for two nodes with different insertion histories.
fn eligible_seats_v3<'a>(
    candidates: &'a [PalwPanelCandidateV3],
    executor_id: &Hash64,
    execution_class_id: &Hash64,
) -> Vec<&'a PalwPanelCandidateV3> {
    let mut ordered: Vec<&PalwPanelCandidateV3> = candidates.iter().collect();
    ordered.sort_by(|a, b| {
        a.validator_id.cmp(&b.validator_id).then_with(|| {
            (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
        })
    });
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
    eligible
}

fn select_from_eligible_v3(
    seed: &Hash64,
    commitment_root: Hash64,
    executor_id: &Hash64,
    execution_class_id: &Hash64,
    eligible: &[&PalwPanelCandidateV3],
    q: usize,
) -> Vec<PalwPanelSeatV3> {
    // The audited lottery, with the V3 seed as its anchor. The v1 eligibility flags are
    // vacuously true here — eligibility was decided above from chain facts.
    //
    // **The ticket key is the SEAT, not the validator key.** `select_replay_panel_v1` treats the id
    // it is handed as opaque: it hashes it into the ticket and uses it as the collision tie-break.
    // A validator key hash is not unique (`dns_finality` says so), so two bonds sharing one key
    // produced the SAME ticket and the SAME tie-break — they sorted adjacently, both could be
    // truncated into the panel, and the resolution below then bound both to whichever bond came
    // first. The result was a panel with fewer distinct verifiers than `q` and a bonded validator
    // that could never be drawn or paid (re-audit §3.4). Everything downstream — receipt counting,
    // payee resolution, dedup here — is keyed on the bond outpoint, so the lottery is too.
    let mut by_ticket_id: BTreeMap<Hash64, &PalwPanelCandidateV3> = BTreeMap::new();
    let v1_candidates: Vec<PalwPanelCandidateV1> = eligible
        .iter()
        .map(|c| {
            let ticket_id = panel_seat_ticket_id_v3(&c.validator_id, &c.bond_outpoint);
            by_ticket_id.insert(ticket_id, c);
            PalwPanelCandidateV1 { validator_id: ticket_id, runtime_class_id: c.runtime_class_id, bonded: true, frozen: false }
        })
        .collect();
    let drawn = select_replay_panel_v1(&commitment_root, executor_id, seed, execution_class_id, &v1_candidates, q);

    // Bind each drawn seat id back to the candidate it was minted from.
    drawn
        .into_iter()
        .map(|ticket_id| {
            let candidate = by_ticket_id.get(&ticket_id).expect("drawn ids are the ones minted above");
            PalwPanelSeatV3 { validator_id: candidate.validator_id, bond_outpoint: candidate.bond_outpoint }
        })
        .collect()
}

/// Domain separator for the per-seat lottery id.
pub const PALW_PANEL_DOMAIN_SEAT_TICKET_ID_V3: &[u8] = b"misaka-palw/panel/seat-ticket-id/v3\0\0\0\0\0";

/// Domain separator for the eligible-set snapshot root.
pub const PALW_PANEL_DOMAIN_ELIGIBLE_SET_ROOT_V3: &[u8] = b"misaka-palw/panel/eligible-set-root/v3";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_PANEL_ALL_DOMAINS: &[&[u8]] = &[PALW_PANEL_DOMAIN_SEAT_TICKET_ID_V3, PALW_PANEL_DOMAIN_ELIGIBLE_SET_ROOT_V3];

/// The `eligible_set_snapshot_root` the V3 seed binds, computed from the candidate set a chain
/// point assembles.
///
/// There is no chain-stored source for this value — nothing on this branch records an eligible-set
/// snapshot — so the only well-defined thing it can be is a commitment to the set the caller
/// actually drew from. Be precise about what force that carries: the hash alone proves nothing
/// against a lying caller, because a caller that hashes its own set is trivially self-consistent.
/// What makes the set honest is CONSENSUS — construction and validation each derive candidates from
/// their own chain-point bond view, so any disagreement moves the panel, moves the coinbase, and
/// the block is rejected (ADR-0033 §5). The root's jobs are narrower and both real: it makes the
/// seed a function of the WHOLE set rather than of one job's identifiers, so a caller cannot
/// silently shrink the set without moving every ticket; and it is the value the ADR-0038 class
/// state machine will record, so the two cannot drift when that lands.
///
/// Hashed over the SORTED output of the eligible filter. Never over the caller's input slice: that
/// slice commonly comes from `ActiveBondView::records()`, which is HashMap iteration order, and a
/// HashMap-ordered preimage is a chain split.
///
/// The count prefix is belt-and-braces, not load-bearing: every seat record is fixed-width under a
/// fixed-width header, so the stream is unambiguous without it and no test can show otherwise. It
/// earns its place only if a variable-width field is ever added — which is exactly when someone
/// would forget it.
pub fn eligible_seat_set_root_v3(
    execution_class_id: &Hash64,
    anchor_daa: u64,
    executor_id: &Hash64,
    candidates: &[PalwPanelCandidateV3],
) -> Hash64 {
    let eligible = eligible_seats_v3(candidates, executor_id, execution_class_id);
    let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_PANEL_DOMAIN_ELIGIBLE_SET_ROOT_V3).to_state();
    hasher.update(execution_class_id.as_byte_slice());
    hasher.update(&anchor_daa.to_le_bytes());
    hasher.update(&(eligible.len() as u32).to_le_bytes());
    for seat in &eligible {
        hasher.update(seat.validator_id.as_byte_slice());
        hasher.update(seat.bond_outpoint.transaction_id.as_byte_slice());
        hasher.update(&seat.bond_outpoint.index.to_le_bytes());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The V3 draw for a caller that has chain facts but no stored snapshot root: derive the root from
/// the candidate set, then draw.
///
/// This is the entry point live wiring should use. [`select_job_panel_v3`] stays for a caller that
/// already holds a recorded snapshot root (the ADR-0038 state machine, when it lands), and the two
/// agree by construction because this one runs the same filter through the same function.
#[allow(clippy::too_many_arguments)]
pub fn select_job_panel_at_anchor_v3(
    network_id: &[u8],
    job_id: Hash64,
    commitment_root: Hash64,
    future_anchor_block_hash: Hash64,
    anchor_daa: u64,
    executor_id: &Hash64,
    execution_class_id: &Hash64,
    candidates: &[PalwPanelCandidateV3],
    q: usize,
) -> Vec<PalwPanelSeatV3> {
    let snapshot_root = eligible_seat_set_root_v3(execution_class_id, anchor_daa, executor_id, candidates);
    let seed = palw_panel_seed_v3(network_id, job_id, commitment_root, future_anchor_block_hash, snapshot_root);
    let eligible = eligible_seats_v3(candidates, executor_id, execution_class_id);
    select_from_eligible_v3(&seed, commitment_root, executor_id, execution_class_id, &eligible, q)
}

/// The unique identity a V3 seat enters the lottery under: `H(validator_id || bond_outpoint)`.
///
/// Unique because the bond outpoint is, which the validator key hash is not. Binding the validator
/// id in as well means a seat's ticket still moves if the bond is re-delegated to another key, so
/// the draw is a function of the whole seat rather than of the outpoint alone.
pub fn panel_seat_ticket_id_v3(validator_id: &Hash64, bond_outpoint: &TransactionOutpoint) -> Hash64 {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_PANEL_DOMAIN_SEAT_TICKET_ID_V3).to_state();
    hasher.update(validator_id.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NET: &[u8] = b"misaka-testnet-11";

    fn class() -> Hash64 {
        Hash64::from_u64_word(7)
    }

    fn bond_record(seed: u8, unbond_at: Option<u64>) -> crate::dns_finality::StakeBondRecord {
        crate::dns_finality::StakeBondRecord {
            version: 1,
            bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), 0),
            owner_pubkey_hash: Hash64::from_u64_word(seed as u64),
            validator_pubkey_hash: Hash64::from_u64_word(seed as u64),
            validator_pubkey: vec![seed; 32],
            amount: 20_000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 100,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: unbond_at,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
        }
    }

    /// **Audit P0-7**: one operator gets one seat, however many bonds it splits into.
    ///
    /// The draw dedups by operator root, and this assembler used to supply `None` for it — so an
    /// operator that split its stake across k bonds collected k seats on one panel. The receipt
    /// count is meant to measure independent replay; k seats from one operator make it measure
    /// nothing, and buying quorum costs no more than the bond floor times k.
    ///
    /// Dedup by bond outpoint is unaffected and remains the one that must hold: it is exact, while
    /// operator dedup is only as good as what the chain knows about ownership.
    #[test]
    fn one_operator_takes_one_seat_however_many_bonds_it_holds() {
        let owner = Hash64::from_u64_word(0xAA);
        let mut a = bond_record(1, None);
        let mut b = bond_record(2, None);
        a.owner_pubkey_hash = owner;
        b.owner_pubkey_hash = owner;
        let outsider = bond_record(3, None);
        let bonds = crate::dns_finality::ActiveBondView::from_records([
            (a.bond_outpoint, a.clone()),
            (b.bond_outpoint, b.clone()),
            (outsider.bond_outpoint, outsider.clone()),
        ]);

        let candidates = palw_panel_candidates_v1(&bonds, 1_000, |_| Some(class()), |_| false);
        assert_eq!(candidates.len(), 3, "all three are candidates; the dedup happens in the draw");

        let seats = select_job_panel_at_anchor_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            1_000,
            &Hash64::from_u64_word(0xEE),
            &class(),
            &candidates,
            3,
        );
        let owners: Vec<_> = seats
            .iter()
            .map(|s| candidates.iter().find(|c| c.bond_outpoint == s.bond_outpoint).unwrap().operator_root.unwrap())
            .collect();
        let mut distinct = owners.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(owners.len(), distinct.len(), "one operator took more than one seat: {owners:?}");
    }

    /// The candidate set is assembled from the chain, and the two exclusions are the point.
    ///
    /// Bond 3 has asked to unbond before the anchor: not eligible at the point the draw is bound
    /// to. Bond 2 never declared a capability: it has staked nothing on being able to run any
    /// class, so it cannot be assigned to replay one — and assigning it would manufacture a
    /// no-show against an honest operator, since the duty accounting charges exactly these seats.
    #[test]
    fn a_candidate_must_be_bonded_at_the_anchor_and_have_declared_a_class() {
        let bonds = crate::dns_finality::ActiveBondView::from_records([
            (bond_record(1, None).bond_outpoint, bond_record(1, None)),
            (bond_record(2, None).bond_outpoint, bond_record(2, None)),
            (bond_record(3, Some(500)).bond_outpoint, bond_record(3, Some(500))),
        ]);
        let declared = |o: &TransactionOutpoint| {
            // Bond 2 declared nothing.
            (o.transaction_id != Hash64::from_bytes([2u8; 64])).then(class)
        };

        let got = palw_panel_candidates_v1(&bonds, 1_000, declared, |_| false);
        assert_eq!(got.len(), 1, "only bond 1 is both bonded at the anchor and declared: {got:?}");
        assert_eq!(got[0].bond_outpoint.transaction_id, Hash64::from_bytes([1u8; 64]));

        // Before bond 3 asked to unbond it WAS eligible — the anchor is what decides, not the
        // reading node's tip, which is why the same view answers differently at a different anchor.
        assert_eq!(palw_panel_candidates_v1(&bonds, 400, declared, |_| false).len(), 2);
    }

    /// The order is canonical, so no caller can build the split this type's own doc warns about.
    ///
    /// `ActiveBondView::records()` is HashMap-ordered. `eligible_seats_v3` sorts internally, so the
    /// DRAW is safe either way — what is not safe is a caller hashing the slice it was handed.
    /// Returning it sorted removes that rather than documenting it again.
    #[test]
    fn the_candidate_set_is_returned_in_canonical_order() {
        let records: Vec<_> = [9u8, 1, 5, 3].into_iter().map(|s| (bond_record(s, None).bond_outpoint, bond_record(s, None))).collect();
        let bonds = crate::dns_finality::ActiveBondView::from_records(records);
        let got = palw_panel_candidates_v1(&bonds, 1_000, |_| Some(class()), |_| false);
        let ids: Vec<_> = got.iter().map(|c| c.validator_id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "candidates must come back canonically ordered");
        assert_eq!(ids.len(), 4);
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

    fn draw_at_anchor(candidates: &[PalwPanelCandidateV3], q: usize) -> Vec<PalwPanelSeatV3> {
        select_job_panel_at_anchor_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            5_000,
            &Hash64::from_u64_word(999),
            &class(),
            candidates,
            q,
        )
    }

    /// The snapshot root commits to the WHOLE eligible set, in a canonical order, and to nothing
    /// the caller can vary without varying the set.
    #[test]
    fn the_snapshot_root_is_a_commitment_to_the_sorted_eligible_set() {
        let executor = Hash64::from_u64_word(999);
        let cands: Vec<PalwPanelCandidateV3> = (10..16).map(candidate).collect();
        let root = eligible_seat_set_root_v3(&class(), 5_000, &executor, &cands);

        // Input order cannot move it — the whole reason the filter sorts. `ActiveBondView::records()`
        // is HashMap-ordered, so an order-sensitive root would split the chain between two nodes
        // with different insertion histories.
        let mut shuffled = cands.clone();
        shuffled.reverse();
        assert_eq!(eligible_seat_set_root_v3(&class(), 5_000, &executor, &shuffled), root);
        // Nor can a duplicate: the filter keeps one voice per bond outpoint.
        let mut with_dup = cands.clone();
        with_dup.push(cands[2].clone());
        assert_eq!(eligible_seat_set_root_v3(&class(), 5_000, &executor, &with_dup), root);

        // Dropping ONE eligible seat moves it — the property that makes shrinking the set visible.
        assert_ne!(eligible_seat_set_root_v3(&class(), 5_000, &executor, &cands[1..]), root);
        // So does an eligible candidate becoming ineligible, and every other bound input.
        let mut slashed = cands.clone();
        slashed[0].bond_status = BondStatus::Slashed;
        assert_ne!(eligible_seat_set_root_v3(&class(), 5_000, &executor, &slashed), root);
        assert_ne!(eligible_seat_set_root_v3(&class(), 5_001, &executor, &cands), root, "anchor daa is bound");
        assert_ne!(eligible_seat_set_root_v3(&Hash64::from_u64_word(8), 5_000, &executor, &cands), root, "class is bound");
        assert_ne!(
            eligible_seat_set_root_v3(&class(), 5_000, &Hash64::from_u64_word(10), &cands),
            root,
            "the executor is bound — it changes who is excluded"
        );
        // Adding an eligible seat moves it too.
        let mut widened = cands.clone();
        widened.push(candidate(20));
        assert_ne!(eligible_seat_set_root_v3(&class(), 5_000, &executor, &widened), root);
        // NOT asserted: that the count prefix is load-bearing. Every seat record is fixed-width
        // (64 + 64 + 4 bytes) under a fixed-width header, so the stream is already unambiguous
        // without it and no mutation of this fixture can distinguish the two. It stays as
        // belt-and-braces against a future variable-width field, and saying so is better than a
        // test that appears to prove something it cannot — deleting the prefix passes this test.
    }

    /// The anchor entry point draws the same seats the stored-root one would, given the root it
    /// derives — the two callers cannot disagree because they run one filter through one function.
    #[test]
    fn the_anchor_entry_point_agrees_with_the_stored_root_one() {
        let executor = Hash64::from_u64_word(999);
        let cands: Vec<PalwPanelCandidateV3> = (10..18).map(candidate).collect();
        let root = eligible_seat_set_root_v3(&class(), 5_000, &executor, &cands);
        let stored = select_job_panel_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            root,
            &executor,
            &class(),
            &cands,
            3,
        );
        assert_eq!(draw_at_anchor(&cands, 3), stored);
        assert_eq!(stored.len(), 3);

        // And the seed genuinely depends on the set: a panel drawn over a shrunken set is not the
        // same panel with one seat removed, because every ticket moved.
        let shrunk = draw_at_anchor(&cands[..7], 3);
        assert_ne!(shrunk, stored, "shrinking the eligible set must move the whole draw");
    }

    /// Two bonds under ONE validator key both get seats — the liveness half of the seat rewrite.
    /// `select_replay_panel_v1` has to drop such an id entirely (its ticket and tie-break are
    /// identical for both, so it cannot seat them apart); the V3 draw keys on the seat, so both
    /// bonds are drawable and payable.
    #[test]
    fn two_bonds_under_one_validator_key_both_get_seats() {
        let mut twin = candidate(10);
        twin.bond_outpoint = TransactionOutpoint::new(Hash64::from_u64_word(4242), 7);
        let cands = vec![candidate(10), twin.clone()];
        let seats = draw_at_anchor(&cands, 2);
        assert_eq!(seats.len(), 2, "both bonds are seated: {seats:?}");
        let mut outpoints: Vec<_> = seats.iter().map(|s| s.bond_outpoint).collect();
        outpoints.sort_by_key(|o| (o.transaction_id, o.index));
        let mut expected = vec![candidate(10).bond_outpoint, twin.bond_outpoint];
        expected.sort_by_key(|o| (o.transaction_id, o.index));
        assert_eq!(outpoints, expected, "each seat names its own bond, not whichever came first");
        assert!(seats.iter().all(|s| s.validator_id == Hash64::from_u64_word(10)), "both share the key, and that is fine");

        // Contrast, pinned: the v1 lottery cannot do this and correctly refuses to guess.
        let v1: Vec<PalwPanelCandidateV1> = cands
            .iter()
            .map(|c| PalwPanelCandidateV1 {
                validator_id: c.validator_id,
                runtime_class_id: c.runtime_class_id,
                bonded: true,
                frozen: false,
            })
            .collect();
        assert!(
            select_replay_panel_v1(
                &Hash64::from_u64_word(2),
                &Hash64::from_u64_word(999),
                &Hash64::from_u64_word(3),
                &class(),
                &v1,
                2
            )
            .is_empty(),
            "the id-keyed lottery drops the ambiguous id — that is the floor this replaces"
        );
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

    /// Two bonds sharing one validator key are two DISTINCT seats, and both must be drawable.
    ///
    /// `validator_pubkey_hash` is not unique, and the lottery used to key on it: the two seats then
    /// minted the same ticket and the same tie-break, so they sorted adjacently, both could be
    /// truncated into the panel, and seat resolution bound both to whichever bond came first. The
    /// panel ended up with fewer distinct verifiers than `q` and one bonded validator that could
    /// never be drawn or paid (re-audit §3.4).
    #[test]
    fn two_bonds_under_one_validator_key_are_two_seats() {
        let a = candidate(10);
        let mut b = candidate(20);
        // Same key, different bonds — the case `dns_finality` says is representable.
        b.validator_id = a.validator_id;
        assert_ne!(a.bond_outpoint, b.bond_outpoint);

        // Both are eligible (dedup is by bond, and these are two bonds).
        let panel = draw(&[a.clone(), b.clone()], 5);
        assert_eq!(panel.len(), 2, "two bonds are two seats");
        // Each seat carries its OWN bond, so both are payable and neither is a duplicate.
        let bonds: BTreeSet<_> = panel.iter().map(|s| (s.bond_outpoint.transaction_id, s.bond_outpoint.index)).collect();
        assert_eq!(bonds.len(), 2, "the two seats must not collapse onto one bond");
        assert!(bonds.contains(&(a.bond_outpoint.transaction_id, a.bond_outpoint.index)));
        assert!(bonds.contains(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index)));

        // With q = 1 exactly one is drawn, deterministically, and it is a real seat.
        let one = draw(&[a.clone(), b.clone()], 1);
        assert_eq!(one.len(), 1);
        assert!(one[0].bond_outpoint == a.bond_outpoint || one[0].bond_outpoint == b.bond_outpoint);
        // Order-invariant: reversing the pool draws the same single seat.
        let reversed = draw(&[b, a], 1);
        assert_eq!(one, reversed, "the draw must not depend on input order");
    }

    /// The seat ticket id is unique per (validator, bond) and binds both halves — so re-delegating a
    /// bond to another key moves its ticket, and two bonds never share one.
    #[test]
    fn the_seat_ticket_id_is_unique_and_binds_both_halves() {
        let a = candidate(10);
        let b = candidate(20);
        let id = |c: &PalwPanelCandidateV3| panel_seat_ticket_id_v3(&c.validator_id, &c.bond_outpoint);
        assert_ne!(id(&a), id(&b));
        // Same bond, different validator key -> different ticket.
        let mut redelegated = a.clone();
        redelegated.validator_id = b.validator_id;
        assert_ne!(id(&a), id(&redelegated), "the ticket must move when the bond changes hands");
        // Same validator key, different bond -> different ticket (the defect above).
        let mut second_bond = a.clone();
        second_bond.bond_outpoint = b.bond_outpoint;
        assert_ne!(id(&a), id(&second_bond), "two bonds under one key must not share a ticket");
        // Deterministic.
        assert_eq!(id(&a), id(&a.clone()));
    }

    /// A short pool draws a short panel (whether it licenses anything is the ramp's call).
    #[test]
    fn short_pool_draws_short_panel() {
        assert_eq!(draw(&[candidate(10), candidate(20)], 5).len(), 2);
        assert!(draw(&[], 5).is_empty());
    }
}
