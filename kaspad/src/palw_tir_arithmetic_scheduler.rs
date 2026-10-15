//! Public TIR arithmetic localization. Only accepted, original-root-authenticated
//! frontiers advance the search; agreement of this tree is not an Honest verdict.
use kaspa_consensus_core::{
    Hash64,
    palw_backend::PalwTirOwnStepSourceV1,
    palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1},
    palw_legacy_public_filer_v1::{
        PALW_DISPUTE_RESERVATION_VERSION_V1, PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1, PalwDisputeReacquireV1,
    },
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwBondKeyV2, PalwLegacyDisputeViewV1},
    palw_tir_court_v1::{
        PalwTirStepNodeDisclosureV1, palw_tir_step_node_frontier_v1, palw_tir_step_node_reaches_v1, palw_tir_step_tree_height_v1,
    },
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ArithmeticStepV1 {
    Need(PalwDaUnitV1),
    /// Localization has selected a leaf. Its preimage, decode pin and operands
    /// still have to be authenticated and judged by the native TIR court.
    Leaf {
        index: u64,
    },
    /// The independent steps agree. Trace, output and all required semantic
    /// families remain separate obligations; this never certifies arithmetic.
    StepAgreement,
}

fn first_difference_v1(
    count: u64,
    level: u8,
    index: u64,
    frontier: &[Hash64],
    own_node: &mut impl FnMut(u8, u64) -> Result<Hash64, String>,
) -> Result<Option<(u8, u64)>, String> {
    let (below, lo, hi) =
        palw_tir_step_node_frontier_v1(count, level, index).ok_or("TIR frontier geometry is outside the original step tree")?;
    if frontier.len() as u64 != hi - lo {
        return Err("TIR frontier has a noncanonical length".into());
    }
    for (offset, alleged) in frontier.iter().enumerate() {
        let child = lo.checked_add(offset as u64).ok_or("TIR frontier index overflow")?;
        if own_node(below, child)? != *alleged {
            return Ok(Some((below, child)));
        }
    }
    Ok(None)
}

fn next_tree_v1(
    count: u64,
    original_step_root: Hash64,
    own_count: u64,
    mut own_node: impl FnMut(u8, u64) -> Result<Hash64, String>,
    mut authenticated_frontier: impl FnMut(u8, u64) -> Result<Option<Vec<Hash64>>, String>,
) -> Result<ArithmeticStepV1, String> {
    if count == 0 || own_count != count {
        return Err("independent TIR execution has another step geometry; an identity proof is required".into());
    }
    let height = palw_tir_step_tree_height_v1(count);
    if own_node(height, 0)? == original_step_root {
        return Ok(ArithmeticStepV1::StepAgreement);
    }
    let (mut level, mut index) = (height, 0);
    // Every accepted frontier strictly lowers the level. No count-sized search,
    // unbounded response history or caller-provided iteration cap is needed.
    while level != 0 {
        let Some(frontier) = authenticated_frontier(level, index)? else {
            return Ok(ArithmeticStepV1::Need(PalwDaUnitV1::TirStepNode { level, index }));
        };
        let Some((below, child)) = first_difference_v1(count, level, index, &frontier, &mut own_node)? else {
            return Err("authenticated divergent TIR node has no independently divergent child".into());
        };
        if below >= level {
            return Err("TIR frontier does not advance the search".into());
        }
        (level, index) = (below, child);
    }
    Ok(ArithmeticStepV1::Leaf { index })
}

fn check_node_v1(
    original: &PalwPublicTirBindingHeaderV1,
    level: u8,
    index: u64,
    disclosure: &PalwTirStepNodeDisclosureV1,
) -> Result<(), String> {
    // The original header comes from the native accepted claim, not this answer.
    // Node authentication needs only the original tree geometry and hashes. The
    // program is rehydrated from native registration later for semantic judging.
    if !disclosure.binding.class.program.is_empty() || PalwPublicTirBindingHeaderV1::of_binding_v1(&disclosure.binding) != *original {
        return Err("TIR arithmetic frontier changes the original public binding".into());
    }
    palw_tir_step_node_reaches_v1(
        original.step_leaf_count,
        &original.step_merkle_root,
        level,
        index,
        &disclosure.frontier,
        &disclosure.siblings,
    )
    .map_err(str::to_string)
}

/// Read-only scheduling over native accepted answers. Callers must retain the
/// independent source's resource ticket and the original paid acquisition clock.
/// This function does not authorize a new DA session or extend that clock.
pub(super) fn next_v1(
    original: &PalwPublicTirBindingHeaderV1,
    maximum_leaves: u64,
    own_count: u64,
    own_node: impl FnMut(u8, u64) -> Result<Hash64, String>,
    mut answer: impl FnMut(PalwDaUnitV1) -> Option<Arc<PalwDaAnswerV1>>,
) -> Result<ArithmeticStepV1, String> {
    if original.step_leaf_count > maximum_leaves {
        return Err("TIR arithmetic header exceeds the native class ladder".into());
    }
    next_tree_v1(original.step_leaf_count, original.step_merkle_root, own_count, own_node, |level, index| {
        match answer(PalwDaUnitV1::TirStepNode { level, index }).as_deref() {
            None => Ok(None),
            Some(PalwDaAnswerV1::TirStepNode(disclosure)) => {
                check_node_v1(original, level, index, &disclosure)?;
                Ok(Some(disclosure.frontier.clone()))
            }
            Some(_) => Err("accepted TIR node key contains another answer kind".into()),
        }
    })
}

/// The runtime entry uses the backend's independent, resource-held replay, not
/// arbitrary own-tree hashes supplied alongside the accused binding.
pub(super) fn next_from_source_v1(
    original: &PalwPublicTirBindingHeaderV1,
    maximum_leaves: u64,
    own: &dyn PalwTirOwnStepSourceV1,
    answer: impl FnMut(PalwDaUnitV1) -> Option<Arc<PalwDaAnswerV1>>,
) -> Result<ArithmeticStepV1, String> {
    let info = own.info_v1();
    if info.header.version != original.version
        || info.header.job_context != original.job_context
        || info.header.artifact_root != original.artifact_root
        || info.generated_ids.len() as u64 != u64::from(original.job_context.exact_decode_tokens)
        || info.header.step_leaf_count > maximum_leaves
    {
        return Err("independent TIR source is not the original registered job's replay".into());
    }
    let own_height = palw_tir_step_tree_height_v1(info.header.step_leaf_count);
    if own.node_v1(own_height, 0)? != info.header.step_merkle_root {
        return Err("independent TIR source no longer reaches its computed step root".into());
    }
    next_v1(original, maximum_leaves, info.header.step_leaf_count, |level, index| own.node_v1(level, index), answer)
}

/// Material loss does not erase the native answered bit. Reopen only with this
/// owner's original, still-live reservation and its next accepted nonce. This is
/// the existing finite DA budget, not the prospective maximum-size paid envelope.
pub(super) fn reacquisition_v1(
    view: &PalwLegacyDisputeViewV1,
    owner: PalwBondKeyV2,
    unit: PalwDaUnitV1,
    now_daa: u64,
) -> Option<PalwDisputeReacquireV1> {
    if !matches!(
        unit,
        PalwDaUnitV1::TirStepLeaf { .. }
            | PalwDaUnitV1::TirStepNode { .. }
            | PalwDaUnitV1::TirStepRun { .. }
            | PalwDaUnitV1::TirRowNode { .. }
    ) || !view.unit_was_answered_v1(&unit)
        || view.sessions.iter().any(|(bond, _)| *bond == owner)
        || view.executor_refuted
        || view.court_convicted
        || view.da_defaulted
        || now_daa > view.hard_deadline_daa
    {
        return None;
    }
    let record = view.record.as_ref()?;
    let row = record.live.get(&owner)?;
    let session_number = row.sessions_opened.checked_add(1)?;
    if record.hard_deadline_daa != view.hard_deadline_daa
        || row.reserved_daa > now_daa
        || session_number > PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1
    {
        return None;
    }
    Some(PalwDisputeReacquireV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: view.claim_id,
        reserver: owner,
        reserved_daa: row.reserved_daa,
        session_number,
        valid_until_daa: view.hard_deadline_daa,
        unit,
        binding: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(7),
            index: 0,
        })
    }

    fn view() -> PalwLegacyDisputeViewV1 {
        use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwDisputeClaimV1, PalwDisputeReservationRowV1};
        let mut record = PalwDisputeClaimV1 { opened_daa: 20, hard_deadline_daa: 80, ..Default::default() };
        record.live.insert(owner(), PalwDisputeReservationRowV1 { reserved_daa: 20, deposit: 9, sessions_opened: 3, lanes_joined: 0 });
        // A policy fixture only; production scheduling reads this view from native state.
        PalwLegacyDisputeViewV1 {
            claim_id: hash(1),
            class_id: hash(2),
            producer: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
                transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(8),
                index: 0,
            }),
            phase: kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Provisional,
            execution_root: hash(3),
            trace_root: hash(4),
            accepted_daa: 10,
            trace_retention_daa: 100,
            job_identity: hash(5),
            public_fp_job: None,
            registered_semantics: None,
            work_leaves: 64,
            deadline_daa: None,
            hard_deadline_daa: 80,
            record: Some(record),
            sessions: Vec::new(),
            answered: vec![PalwDaUnitV1::TirStepNode { level: 6, index: 0 }],
            flat_answered_rows: 0,
            open_courts: 0,
            executor_refuted: false,
            court_convicted: false,
            da_defaulted: false,
        }
    }

    fn hash(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    #[test]
    fn maximum_ladder_descent_uses_typed_nodes_without_seat_reach_cap() {
        // Geometry fixture, not a cryptographic or native admission credential.
        let count = 1u64 << 40;
        let target = count - 1;
        let mut requests = Vec::new();
        let step = next_tree_v1(
            count,
            hash(2),
            count,
            |_, _| Ok(hash(1)),
            |level, index| {
                requests.push((level, index));
                let (below, lo, hi) = palw_tir_step_node_frontier_v1(count, level, index).unwrap();
                let mut frontier = vec![hash(1); (hi - lo) as usize];
                frontier[((target >> below) - lo) as usize] = hash(2);
                Ok(Some(frontier))
            },
        )
        .unwrap();
        assert_eq!(step, ArithmeticStepV1::Leaf { index: target });
        assert_eq!(requests, vec![(40, 0), (30, 1023), (20, (1 << 20) - 1), (10, (1 << 30) - 1)]);
    }

    #[test]
    fn missing_accepted_frontier_does_not_advance_from_unsigned_or_old_progress() {
        let count = 1u64 << 40;
        assert_eq!(
            next_tree_v1(count, hash(2), count, |_, _| Ok(hash(1)), |_, _| Ok(None)).unwrap(),
            ArithmeticStepV1::Need(PalwDaUnitV1::TirStepNode { level: 40, index: 0 })
        );
    }

    #[test]
    fn step_agreement_is_distinct_from_honest_and_does_not_request_a_leaf() {
        assert_eq!(
            next_tree_v1(1, hash(1), 1, |_, _| Ok(hash(1)), |_, _| panic!("no frontier needed")).unwrap(),
            ArithmeticStepV1::StepAgreement
        );
        assert_eq!(
            next_tree_v1(1, hash(2), 1, |_, _| Ok(hash(1)), |_, _| panic!("a singleton has no interior node")).unwrap(),
            ArithmeticStepV1::Leaf { index: 0 }
        );
    }

    #[test]
    fn noncanonical_frontier_and_changed_geometry_are_not_localization() {
        assert!(next_tree_v1(5, hash(2), 5, |_, _| Ok(hash(1)), |_, _| Ok(Some(vec![hash(2); 4]))).is_err());
        assert!(next_tree_v1(5, hash(2), 6, |_, _| Ok(hash(1)), |_, _| panic!("geometry checked first")).is_err());
        assert!(next_tree_v1(0, hash(2), 0, |_, _| Ok(hash(1)), |_, _| panic!("empty tree refused")).is_err());
    }

    #[test]
    fn own_source_failure_or_inconsistent_children_cannot_become_agreement() {
        assert!(next_tree_v1(8, hash(2), 8, |_, _| Err("own disk read failed".into()), |_, _| Ok(None)).is_err());
        assert!(next_tree_v1(8, hash(2), 8, |_, _| Ok(hash(1)), |_, _| Ok(Some(vec![hash(1); 8]))).is_err());
    }

    #[test]
    fn reacquisition_uses_original_native_reservation_nonce_and_absolute_clock() {
        let view = view();
        let unit = view.answered[0];
        let request = reacquisition_v1(&view, owner(), unit, 30).unwrap();
        assert_eq!(request.claim, view.claim_id);
        assert_eq!(request.reserved_daa, 20);
        assert_eq!(request.session_number, 4);
        assert_eq!(request.valid_until_daa, 80);
        assert_eq!(request.binding, None);
        assert_eq!(request.unit, unit);
        assert!(reacquisition_v1(&view, owner(), unit, 19).is_none());
        assert!(reacquisition_v1(&view, owner(), unit, 81).is_none());
        assert!(reacquisition_v1(&view, owner(), PalwDaUnitV1::TirStepLeaf { index: 0 }, 30).is_none());
    }

    #[test]
    fn paid_semantic_row_or_exhausted_da_budget_cannot_authorize_a_fresh_reacquisition() {
        let mut view = view();
        let unit = view.answered[0];
        view.record.as_mut().unwrap().live.get_mut(&owner()).unwrap().sessions_opened = PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
        view.record = None;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
        let mut view = self::view();
        view.record.as_mut().unwrap().hard_deadline_daa += 1;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
        view.record.as_mut().unwrap().hard_deadline_daa -= 1;
        view.da_defaulted = true;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
    }
}
