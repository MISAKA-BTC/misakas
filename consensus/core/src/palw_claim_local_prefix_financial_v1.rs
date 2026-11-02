//! Financial observations of one accepted FP source cohort. Read complete
//! authenticated F1 contexts only inside their callbacks; never build a partial
//! historical world or substitute the current row of a re-admitted raw ID.
use super::*;
use super::super::palw_claim_local_rewards_v1 as rewards;
use crate::palw_claim_local_v1::{PalwClaimLiabilityFrameV1, PalwClaimLocalRowOwnerV1};
use crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1;

type Order = (u64, u64, u8, usize);
fn bad(message: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(message.into())
}
fn unavailable() -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(
        crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
    )
}
fn same_birth(claim: &PalwClaimStateV2, row: &PalwPrefixExportReservationV1) -> bool {
    matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. })
        && claim.bond == row.bond && claim.accepted_block == row.point.block
        && claim.accepted_daa == row.point.daa_score && claim.accepted_blue_score == row.point.blue_score
}
pub(super) fn live_birth_matches(state: &PalwChainStateV2, row: &PalwPrefixExportReservationV1) -> bool {
    state.claims.get(&row.source_claim).is_some_and(|claim| same_birth(claim, row))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OldSourceFinancialBasisV1 {
    pub g: u128,
    pub fp_first: Option<u128>,
}
/// Semantic view only; A4 owns the authenticated codec, pointer and clocks.
/// Slot replacement preserves Retained standing. Only the existing native
/// old-obligation expiry witness may select DeletedByNativeExpiry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OldLiabilityStandingV4 {
    NeverMinted,
    Retained,
    DeletedByNativeExpiry,
}

pub(super) fn basis_from_retired_observation_v4(
    terminal_g: Option<u128>,
    liability: Option<&crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>,
    standing: OldLiabilityStandingV4,
    once_consumed: bool,
) -> Result<OldSourceFinancialBasisV1, PalwStateV2Error> {
    let terminal_g = terminal_g.ok_or_else(|| bad("old prefix financial observation has no terminal frozen G"))?;
    match standing {
        OldLiabilityStandingV4::NeverMinted => {
            if liability.is_some() { return Err(bad("unminted prefix financial observation adopts a liability")) }
            Ok(OldSourceFinancialBasisV1 { g: terminal_g, fp_first: None })
        }
        OldLiabilityStandingV4::Retained | OldLiabilityStandingV4::DeletedByNativeExpiry => {
            let row = liability.ok_or_else(|| bad("minted prefix financial observation lost its actual liability body"))?;
            let g = row.g_res_sompi.saturating_add(u128::from(row.escrowed_reward));
            let eligible = standing == OldLiabilityStandingV4::Retained && !once_consumed
                && row.free_prompt && row.voided_daa.is_none() && row.void_reason.is_none();
            Ok(OldSourceFinancialBasisV1 { g, fp_first: eligible.then_some(g) })
        }
    }
}
#[derive(Clone, Copy)]
struct LiabilityFacts {
    g: u128,
    fp_first: bool,
}
struct Observations {
    first_terminal: Option<Order>,
    last_liability_write: Option<Order>,
    last_terminal: Option<(Order, u128, bool)>,
    last_liability: Option<(Order, Option<LiabilityFacts>)>,
}
impl Observations {
    fn new() -> Self { Self { first_terminal: None, last_liability_write: None, last_terminal: None, last_liability: None } }
    fn claim(&mut self, order: Order, claim: &PalwClaimStateV2, row: &PalwPrefixExportReservationV1, params: &PalwStateParamsV2) {
        if !same_birth(claim, row) || !claim.phase.is_terminal() { return }
        let scale = crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(params);
        let g = palw_claim_frozen_g_res_v1(&scale, claim).saturating_add(u128::from(claim.escrowed_reward));
        self.terminal(order, g, matches!(claim.phase, PalwClaimPhaseV2::Final { .. }));
    }
    fn terminal(&mut self, order: Order, g: u128, is_final: bool) {
        self.first_terminal = Some(self.first_terminal.map_or(order, |old| old.min(order)));
        if self.last_terminal.as_ref().is_none_or(|(old, _, _)| *old < order) {
            self.last_terminal = Some((order, g, is_final));
        }
    }
    fn liability(
        &mut self, order: Order,
        previous: Option<&crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>,
        next: Option<&crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>,
        original: &PalwClaimStateV2, row: &PalwPrefixExportReservationV1,
    ) -> Result<(), PalwStateV2Error> {
        // A liability inherited unchanged at birth may belong to an older
        // acceptance of this same raw ID. Require an actual cohort-era write.
        if previous == next { return Ok(()) }
        let Some(next) = next else {
            if self.last_liability.as_ref().is_none_or(|(old, _)| *old < order) {
                self.last_liability = Some((order, None));
            }
            return Ok(())
        };
        if next.claim_id != row.source_claim || next.executor_bond != row.bond
            || next.class_id != original.class_id || next.execution_root != original.execution_root
            || next.output_root != original.output_root {
            return Err(bad("old prefix source financial row changes its admitted subject"));
        }
        self.last_liability_write = Some(self.last_liability_write.map_or(order, |old| old.max(order)));
        if self.last_liability.as_ref().is_none_or(|(old, _)| *old < order) {
            self.last_liability = Some((order, Some(LiabilityFacts {
                g: next.g_res_sompi.saturating_add(u128::from(next.escrowed_reward)),
                fp_first: next.free_prompt && next.voided_daa.is_none() && next.void_reason.is_none(),
            })));
        }
        Ok(())
    }
    fn snapshot(
        &mut self, order: Order,
        next: Option<&crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>,
        original: &PalwClaimStateV2, row: &PalwPrefixExportReservationV1,
    ) {
        if self.last_liability.as_ref().is_some_and(|(old, _)| *old >= order) { return }
        // Before the actual cohort-era mint this can still be a stale row
        // from an earlier acceptance. Such a row never authenticates a tier.
        let facts = next.filter(|next| next.claim_id == row.source_claim && next.executor_bond == row.bond
            && next.class_id == original.class_id && next.execution_root == original.execution_root
            && next.output_root == original.output_root).map(|next| LiabilityFacts {
                g: next.g_res_sompi.saturating_add(u128::from(next.escrowed_reward)),
                fp_first: next.free_prompt && next.voided_daa.is_none() && next.void_reason.is_none(),
            });
        self.last_liability = Some((order, facts));
    }
    fn finish(self) -> Result<OldSourceFinancialBasisV1, PalwStateV2Error> {
        let (_, terminal_g, _) = self.last_terminal.ok_or_else(unavailable)?;
        let first = self.first_terminal.expect("observed terminal");
        let minted = self.last_liability_write.filter(|at| (at.0, at.1) >= (first.0, first.1));
        if let Some((at, Some(liability))) = self.last_liability.filter(|(at, _)| minted.is_some_and(|mint| *at >= mint)) {
            let _ = at;
            // This branch is a retired old cohort. Legacy neutral reversal
            // can leave its FP liability unmarked; retirement still permits
            // U3 exactly as post_final_fp_g_v1 does. Phase alone is no burn.
            return Ok(OldSourceFinancialBasisV1 { g: liability.g, fp_first: liability.fp_first.then_some(liability.g) });
        }
        // No legacy liability was minted for this source. Its actual terminal
        // claim still supplies frozen G, while the existing post-Final FP tier
        // remains absent exactly as it was without a liability record.
        Ok(OldSourceFinancialBasisV1 { g: terminal_g, fp_first: None })
    }
}

pub(super) fn read_old_source_basis_v1(
    state: &PalwChainStateV2, params: &PalwStateParamsV2, now: &PalwBlockContextV2,
    journal: &[PalwDeltaEntryV2], row: &PalwPrefixExportReservationV1,
) -> Result<OldSourceFinancialBasisV1, PalwStateV2Error> {
    use super::super::palw_claim_local_prefix_financial_archive_v4 as archive;
    if let Some(basis) = archive::with_financial_view_v4(state, row, |view| {
        if view.birth() != row.point || view.producer() != row.bond {
            return Err(bad("old prefix financial archive changes its source birth"));
        }
        let standing = match view.attachment() {
            archive::AttachmentV4::NeverMinted => OldLiabilityStandingV4::NeverMinted,
            archive::AttachmentV4::Attached | archive::AttachmentV4::DetachedByReplacement => OldLiabilityStandingV4::Retained,
            archive::AttachmentV4::DeletedByNativeExpiry => OldLiabilityStandingV4::DeletedByNativeExpiry,
        };
        basis_from_retired_observation_v4(view.terminal_g(), view.liability(), standing, view.consumed().is_some())
    })? {
        return Ok(basis);
    }
    let origin = state.claim_local_admission_context_v1(params, &row.point, row.source_claim)?
        .ok_or_else(unavailable)?;
    let original = origin.original_claim();
    if !same_birth(original, row) { return Err(bad("old prefix financial source changes its birth")) }
    // First *actual* later admission, not a later phase-change frame. Current
    // transition journal entries cover retire/re-admit within this very block.
    let mut boundary = None::<(u64, u64)>;
    for (_, raw) in rewards::frames(state) {
        let frame = rewards::decoded::<PalwClaimLiabilityFrameV1>(raw)?;
        // An intermediate re-admission can retire again inside this frame,
        // leaving no current row for the ordinary birth reader. Native writes
        // still establish that the raw key was reused. This only withholds an
        // ambiguous legacy financial tier; it never authenticates a source.
        for change in &frame.native_changes {
            if change.owner != PalwClaimLocalRowOwnerV1::ChainClaims || change.key.as_slice() != row.source_claim.as_byte_slice() { continue }
            if let Some(bytes) = change.new.as_deref() {
                let claim = rewards::decoded::<PalwClaimStateV2>(bytes)?;
                if !same_birth(&claim, row) && claim.accepted_blue_score > row.point.blue_score {
                    if claim.accepted_blue_score > frame.point.blue_score || claim.accepted_daa > frame.point.daa_score {
                        return Err(bad("old prefix financial history contains a future birth"));
                    }
                    let key = (claim.accepted_blue_score, claim.accepted_daa);
                    boundary = Some(boundary.map_or(key, |old| old.min(key)));
                }
            }
        }
        if frame.point.blue_score <= row.point.blue_score || !frame.claims.contains(&row.source_claim) { continue }
        if let Some(next) = state.claim_local_admission_context_v1(params, &frame.point, row.source_claim)? {
            if !same_birth(next.original_claim(), row) {
                let point = next.accepting_point();
                let key = (point.blue_score, point.daa_score);
                boundary = Some(boundary.map_or(key, |old| old.min(key)));
            }
        }
    }
    let current_boundary = journal.iter().position(|entry| match entry {
        PalwDeltaEntryV2::Claim { key, new: Some(claim), .. } => *key == row.source_claim && !same_birth(claim, row),
        _ => false,
    });
    if current_boundary.is_some() {
        let key = (now.blue_score, now.daa_score);
        boundary = Some(boundary.map_or(key, |old| old.min(key)));
    }
    let mut observed = Observations::new();
    for (_, raw) in rewards::frames(state) {
        let frame = rewards::decoded::<PalwClaimLiabilityFrameV1>(raw)?;
        let point = (frame.point.blue_score, frame.point.daa_score);
        if point.0 < row.point.blue_score || boundary.is_some_and(|next| point > next) { continue }
        let at_boundary = boundary == Some(point);
        let mut newer_terminal = false;
        if at_boundary {
            for change in &frame.native_changes {
                if change.owner != PalwClaimLocalRowOwnerV1::ChainClaims || change.key.as_slice() != row.source_claim.as_byte_slice() { continue }
                if let Some(bytes) = change.new.as_deref() {
                    let claim = rewards::decoded::<PalwClaimStateV2>(bytes)?;
                    newer_terminal |= !same_birth(&claim, row) && claim.phase.is_terminal();
                }
            }
        }
        rewards::frame_read(&frame, |prior, current| {
            if point.0 > row.point.blue_score {
                if let Some(claim) = prior.claims.get(&row.source_claim) {
                    observed.claim((point.0, point.1, 0, 0), claim, row, params);
                }
            }
            observed.snapshot((point.0, point.1, 0, 0), prior.panel_liabilities.get(&row.source_claim), original, row);
            if !at_boundary {
                if let Some(claim) = current.claims.get(&row.source_claim) {
                    observed.claim((point.0, point.1, 2, 0), claim, row, params);
                }
                observed.liability((point.0, point.1, 2, 0), prior.panel_liabilities.get(&row.source_claim),
                    current.panel_liabilities.get(&row.source_claim), original, row)?;
                observed.snapshot((point.0, point.1, 2, 0), current.panel_liabilities.get(&row.source_claim), original, row);
            } else {
                let current_liability = current.panel_liabilities.get(&row.source_claim);
                let newer_terminal = newer_terminal || current.claims.get(&row.source_claim)
                    .is_some_and(|claim| !same_birth(claim, row) && claim.phase.is_terminal());
                // A new nonterminal claim cannot mint a liability. Therefore
                // this snapshot still authoritatively observes an old-row
                // expiry/conviction even when it happened in the birth block.
                // If a new terminal cohort overwrote it, F1 has no ordered
                // panel events to recover the old value: never guess a tier.
                if newer_terminal && current_liability.is_some() {
                    return Err(unavailable());
                }
                observed.snapshot((point.0, point.1, 2, 0), current_liability, original, row);
            }
            Ok(())
        })?;
        for (index, change) in frame.native_changes.iter().enumerate() {
            if change.owner != PalwClaimLocalRowOwnerV1::ChainClaims || change.key.as_slice() != row.source_claim.as_byte_slice() { continue }
            for (slot, bytes) in [change.old.as_deref(), change.new.as_deref()].into_iter().enumerate() {
                let Some(bytes) = bytes else { continue };
                let claim = rewards::decoded::<PalwClaimStateV2>(bytes)?;
                observed.claim((point.0, point.1, 1, index.saturating_mul(2).saturating_add(slot)), &claim, row, params);
            }
        }
    }
    // No new birth before this block: read only real typed journal entries up
    // to its first replacement. No current/new-cohort panel row is consulted.
    if boundary.is_none_or(|point| point >= (now.blue_score, now.daa_score)) {
        for (index, entry) in journal.iter().enumerate().take(current_boundary.unwrap_or(journal.len())) {
            let order = (now.blue_score, now.daa_score, 1, index.saturating_mul(2).saturating_add(1));
            match entry {
                PalwDeltaEntryV2::Claim { key, old, new } if *key == row.source_claim => {
                    if let Some(claim) = old {
                        observed.claim((order.0, order.1, order.2, order.3 - 1), claim, row, params);
                    }
                    if let Some(claim) = new { observed.claim(order, claim, row, params); }
                }
                PalwDeltaEntryV2::PanelLiability { key, old, new } if *key == row.source_claim => {
                    observed.liability(order, old.as_ref(), new.as_ref(), original, row)?;
                }
                _ => {}
            }
        }
    }
    if boundary.is_none() {
        observed.snapshot((now.blue_score, now.daa_score, 3, 0), state.panel_liabilities.get(&row.source_claim), original, row);
    }
    let basis = observed.finish()?;
    if boundary.is_some() && basis.fp_first.is_some() {
        // Later liability mints may supersede the old raw row even after the
        // first birth-boundary frame. F1 omitted their ordered ownership; a
        // birth-qualified normal financial record must resolve this case.
        return Err(unavailable());
    }
    Ok(basis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc40_old_financial_basis_excludes_inherited_liability_and_missing_terminal() {
        let mut observed = Observations::new();
        observed.last_liability = Some(((2, 2, 2, 0), Some(LiabilityFacts { g: 900, fp_first: true })));
        assert!(observed.finish().is_err(), "a liability alone cannot identify the old terminal cohort");
        let mut observed = Observations::new();
        observed.terminal((8, 8, 1, 0), 70, true);
        observed.last_liability = Some(((2, 2, 2, 0), Some(LiabilityFacts { g: 900, fp_first: true })));
        assert_eq!(observed.finish().unwrap(), OldSourceFinancialBasisV1 { g: 70, fp_first: None });
    }

    #[test]
    fn rfc40_old_financial_basis_uses_frozen_terminal_liability_and_conviction_marker() {
        let mut observed = Observations::new();
        observed.terminal((8, 8, 1, 0), 70, true);
        observed.last_liability_write = Some((8, 8, 2, 0));
        observed.last_liability = Some(((8, 8, 2, 0), Some(LiabilityFacts { g: 61, fp_first: true })));
        assert_eq!(observed.finish().unwrap(), OldSourceFinancialBasisV1 { g: 61, fp_first: Some(61) });
        let mut observed = Observations::new();
        observed.terminal((8, 8, 1, 0), 70, true);
        observed.terminal((9, 9, 1, 0), 70, false);
        observed.last_liability_write = Some((8, 8, 2, 0));
        observed.last_liability = Some(((8, 8, 2, 0), Some(LiabilityFacts { g: 61, fp_first: true })));
        assert_eq!(observed.finish().unwrap().fp_first, Some(61), "a neutral reversal is not a liability conviction marker");
        let mut observed = Observations::new();
        observed.terminal((8, 8, 1, 0), 70, true);
        observed.last_liability_write = Some((8, 8, 2, 0));
        observed.last_liability = Some(((9, 9, 2, 0), Some(LiabilityFacts { g: 61, fp_first: false })));
        assert_eq!(observed.finish().unwrap().fp_first, None);
        let mut observed = Observations::new();
        observed.terminal((8, 8, 1, 0), 70, true);
        observed.last_liability_write = Some((8, 8, 2, 0));
        observed.last_liability = Some(((9, 9, 2, 0), None));
        assert_eq!(observed.finish().unwrap(), OldSourceFinancialBasisV1 { g: 70, fp_first: None }, "deleted old liability must not resurrect a tier");
    }

    fn h(n: u64) -> Hash64 { Hash64::from_u64_word(n) }
    fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(100 + n), daa_score: n, blue_score: n, subsidy: 0 }
    }
    fn original_claim() -> PalwClaimStateV2 {
        PalwClaimStateV2 {
            source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
            class_id: h(2), bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(3), 0)),
            pwu: 10, accepted_daa: 1, rebound_daa: None, accepted_blue_score: 1, accepted_block: point(1).block,
            trace_root: h(4), output_root: h(5), execution_root: h(6), trace_chunk_count: 1,
            trace_retention_daa: 100, reserved: 70, immature_contribution: 0, escrowed_reward: 0,
            work_leaves: 10, work_id: None, phase: PalwClaimPhaseV2::Provisional,
            rights_reserved: 0, job_identity: h(7), rcore: PalwClaimRcoreV1::default(),
        }
    }
    fn liability(original: &PalwClaimStateV2) -> crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 {
        crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 {
            claim_id: h(1), work_id: h(8), class_id: original.class_id, execution_root: original.execution_root,
            output_root: original.output_root, executor_bond: original.bond, voided_daa: None, void_reason: None,
            valid_signers: vec![], locked_sompi: 0, expiry_daa: 3, settled_at_final: 0,
            job_identity: original.job_identity, free_prompt: true, trace_root: original.trace_root,
            segment_count: 0, licence_door: None, basis_k: 0, g_res_sompi: 61, escrowed_reward: 0,
        }
    }
    fn object(claim: &PalwClaimStateV2) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::FreePromptCommitted {
            claim: h(1), class_id: claim.class_id, bond: claim.bond, executor_pubkey: vec![], work_leaves: 10,
            prompt_token_ids_hash: h(9), prompt_tokens: 1, prompt_token_ids: vec![1], decode_tokens_executed: 1,
            trace_root: claim.trace_root, output_root: claim.output_root, execution_root: claim.execution_root,
            trace_chunk_count: 1, trace_retention_daa: 100,
            consumed_prefix_state: crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(claim.class_id),
            job_pin: claim.job_identity, public_job: None, eval: None,
        }
    }
    fn carriage(
        at: Option<PalwBlockContextV2>, claim: Option<&PalwClaimStateV2>,
        financial: Option<&crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>,
    ) -> PalwStateCarriageV2 {
        let mut value = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
        value.last_point = at;
        if let Some(claim) = claim { value.claims.insert(h(1), claim.clone()); }
        if let Some(financial) = financial { value.panel_liabilities.insert(h(1), financial.clone()); }
        value
    }
    fn frame(
        params: &PalwStateParamsV2, at: PalwBlockContextV2,
        prior: PalwStateCarriageV2, current: PalwStateCarriageV2,
        events: &[(Option<&PalwClaimStateV2>, Option<&PalwClaimStateV2>)],
    ) -> PalwClaimLiabilityFrameV1 {
        use crate::palw_claim_local_v1::{PalwClaimLocalInputV1, PalwClaimLocalNativeChangeV1, PalwClaimLocalPolicyContextV1};
        let claim = current.claims.get(&h(1)).unwrap();
        PalwClaimLiabilityFrameV1 {
            point: at, network_identity: [11; 32], fired_forks: [12; 32],
            policy: PalwClaimLocalPolicyContextV1::from_rules_v1(params, at.daa_score),
            claims: [h(1)].into_iter().collect(),
            native_changes: events.iter().map(|(old, new)| PalwClaimLocalNativeChangeV1 {
                owner: PalwClaimLocalRowOwnerV1::ChainClaims, table: 0, key: h(1).as_byte_slice().to_vec(),
                old: old.map(|v| borsh::to_vec(v).unwrap()), new: new.map(|v| borsh::to_vec(v).unwrap()),
            }).collect(),
            prior_state: borsh::to_vec(&prior).unwrap(), current_state: borsh::to_vec(&current).unwrap(),
            accepted_objects: vec![object(claim)], origin_headers: Default::default(),
            inputs: vec![PalwClaimLocalInputV1::FreePromptNative {
                claim: h(1), job_pin: claim.job_identity, public_job: None, eval: None,
            }],
        }
    }
    fn install_frame(state: &mut PalwChainStateV2, frame: PalwClaimLiabilityFrameV1) {
        frame.validate_context_v1().unwrap();
        let key = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(frame.point.block);
        // Exercise the serialized retained-frame reader rather than Observation setters.
        let wire = borsh::to_vec(&frame).unwrap();
        state.kernel_route.as_mut().unwrap().aux.insert(
            (crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()), wire);
    }

    #[test]
    fn rfc40_old_financial_boundary_expiry_readmit_matches_live_journal_after_frame_persistence() {
        // Financial-history fixture, not a signed native admission test. The real
        // reader must distinguish two authenticated births sharing the raw ID,
        // including the sweep deletion that preceded the second admission.
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(2), 4, 1000, 100, 1000, 0)
            .unwrap().with_claim_local_settlement_from_daa(Some(0));
        let original = original_claim();
        let terminal = PalwClaimStateV2 { phase: PalwClaimPhaseV2::Final { final_daa: 2 }, ..original.clone() };
        let newer = PalwClaimStateV2 {
            accepted_daa: 3, accepted_blue_score: 3, accepted_block: point(3).block, ..original.clone()
        };
        let financial = liability(&original);
        let row = PalwPrefixExportReservationV1 {
            source_claim: h(1), point: point(1), bond: original.bond, budget_id: h(10),
            liability_until: 90, material_until: 100, liability_sompi: 61, convicted_daa: None,
        };
        let mut state = PalwChainStateV2::genesis();
        state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(11), h(12)), None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 3, burned: 0 }));
        state.last_point = Some(point(3));
        state.claims.insert(h(1), newer.clone());
        install_frame(&mut state, frame(&params, point(1), carriage(None, None, None),
            carriage(Some(point(1)), Some(&original), None), &[(None, Some(&original))]));
        install_frame(&mut state, frame(&params, point(2), carriage(Some(point(1)), Some(&original), None),
            carriage(Some(point(2)), Some(&terminal), Some(&financial)), &[(Some(&original), Some(&terminal))]));
        let journal = vec![
            PalwDeltaEntryV2::PanelLiability { key: h(1), old: Some(financial.clone()), new: None },
            PalwDeltaEntryV2::Claim { key: h(1), old: Some(terminal.clone()), new: None },
            PalwDeltaEntryV2::Claim { key: h(1), old: None, new: Some(newer.clone()) },
        ];
        let live = read_old_source_basis_v1(&state, &params, &point(3), &journal, &row).unwrap();
        assert_eq!(live.fp_first, None);
        install_frame(&mut state, frame(&params, point(3), carriage(Some(point(2)), Some(&terminal), Some(&financial)),
            carriage(Some(point(3)), Some(&newer), None), &[(Some(&terminal), None), (None, Some(&newer))]));
        state.last_point = Some(point(4));
        let persisted = read_old_source_basis_v1(&state, &params, &point(4), &[], &row).unwrap();
        assert_eq!(persisted, live, "persisting the birth-boundary frame must not resurrect the expired first-conviction tier");
        assert_eq!(state.claims[&h(1)], newer, "financial reading cannot mutate the later cohort");
        assert!(!state.panel_liabilities.contains_key(&h(1)));
    }


    #[test]
    fn rfc40_old_financial_final_mint_survives_an_earlier_inherited_liability_extension() {
        let original = original_claim();
        let row = PalwPrefixExportReservationV1 {
            source_claim: h(1), point: point(1), bond: original.bond, budget_id: h(10),
            liability_until: 90, material_until: 100, liability_sompi: 61, convicted_daa: None,
        };
        let inherited = liability(&original);
        let mut extended = inherited.clone();
        extended.expiry_daa = 10;
        let mut actual_final = inherited.clone();
        actual_final.expiry_daa = 20;
        let mut observed = Observations::new();
        // DA-5 may extend an inherited raw-ID row while this birth is still
        // nonterminal. It does not erase the later genuine Final-era mint.
        observed.liability((2, 2, 2, 0), Some(&inherited), Some(&extended), &original, &row).unwrap();
        observed.terminal((8, 8, 1, 0), 70, true);
        observed.liability((8, 8, 2, 0), Some(&extended), Some(&actual_final), &original, &row).unwrap();
        assert_eq!(observed.finish().unwrap(), OldSourceFinancialBasisV1 { g: 61, fp_first: Some(61) });
    }


    #[test]
    fn rfc40_a4_retained_old_body_preserves_tier_until_once_or_actual_void_marker() {
        use OldLiabilityStandingV4::Retained;
        let body = liability(&original_claim());
        // The authenticated A4 adapter maps BOTH AttachedToMainSlot and
        // DetachedByReplacement to Retained. Slot location is not a penalty
        // expiry, and the semantic interface intentionally has no phase input.
        let expected = OldSourceFinancialBasisV1 { g: 61, fp_first: Some(61) };
        assert_eq!(basis_from_retired_observation_v4(Some(70), Some(&body), Retained, false).unwrap(), expected);
        assert_eq!(basis_from_retired_observation_v4(Some(70), Some(&body), Retained, true).unwrap(),
            OldSourceFinancialBasisV1 { g: 61, fp_first: None });
        for marker in 0..3 {
            let mut convicted = body.clone();
            match marker {
                0 => convicted.voided_daa = Some(9),
                1 => convicted.void_reason = Some(PalwVoidReasonV2::PrefixExportFraud),
                _ => { convicted.voided_daa = Some(9); convicted.void_reason = Some(PalwVoidReasonV2::CourtFraud); }
            }
            assert_eq!(basis_from_retired_observation_v4(Some(70), Some(&convicted), Retained, false).unwrap(),
                OldSourceFinancialBasisV1 { g: 61, fp_first: None });
        }
        let mut not_fp = body.clone();
        not_fp.free_prompt = false;
        assert_eq!(basis_from_retired_observation_v4(Some(70), Some(&not_fp), Retained, false).unwrap().fp_first, None);
        assert!(body.voided_daa.is_none() && body.void_reason.is_none(), "the semantic read never mutates actual retained body");
    }

    #[test]
    fn rfc40_a4_native_expiry_keeps_actual_body_g_without_resurrecting_the_first_tier() {
        let body = liability(&original_claim());
        for once in [false, true] {
            assert_eq!(basis_from_retired_observation_v4(Some(70), Some(&body),
                OldLiabilityStandingV4::DeletedByNativeExpiry, once).unwrap(),
                OldSourceFinancialBasisV1 { g: 61, fp_first: None },
                "actual native expiry removes eligibility while immutable body remains the exact G evidence");
        }
        assert_eq!(basis_from_retired_observation_v4(Some(70), None, OldLiabilityStandingV4::NeverMinted, false).unwrap(),
            OldSourceFinancialBasisV1 { g: 70, fp_first: None });
    }

    #[test]
    fn rfc40_a4_financial_semantics_refuse_missing_terminal_or_inconsistent_body_standing() {
        use OldLiabilityStandingV4::*;
        let body = liability(&original_claim());
        assert!(basis_from_retired_observation_v4(Some(70), Some(&body), NeverMinted, false).is_err(),
            "an unminted source cannot adopt an inherited raw-ID liability");
        for standing in [Retained, DeletedByNativeExpiry] {
            assert!(basis_from_retired_observation_v4(Some(70), None, standing, false).is_err(),
                "actual minted/expired observations retain their authenticated immutable liability body");
        }
        for (standing, actual_body) in [(NeverMinted, None), (Retained, Some(&body)), (DeletedByNativeExpiry, Some(&body))] {
            assert!(basis_from_retired_observation_v4(None, actual_body, standing, false).is_err(),
                "a retained liability never substitutes for actual old terminal frozen-G provenance");
        }
    }

}
