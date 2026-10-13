//! Signed Panel sponsorship, owned by native consensus. Historical onboarding
//! source targets and fee objects retain their codecs and semantics.
use crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1;
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use misaka_palw_challenge::hash::{Digest, object_id};
use misaka_palw_challenge::{BeaconContextV1, PostCommitChallengePolicyV1};
use misaka_palw_kernel::beacon_source::{BeaconSourceTargetV1, BeaconSourceTermsV1};
use misaka_palw_panel::{BeaconRequestV1, PanelPolicyV1};

/// New optional aux authority; 108/109/110 retain their source/fee/cost schemas.
pub const PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2: u8 = 112;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelSourceRequestV2 {
    pub network: Hash64,
    pub ruleset: Hash64,
    pub scheme: Hash64,
    pub epoch: u64,
    pub release_daa: u64,
    pub deadline_daa: u64,
}
impl From<BeaconRequestV1> for PalwPanelSourceRequestV2 {
    fn from(r: BeaconRequestV1) -> Self {
        Self {
            network: r.network,
            ruleset: r.ruleset,
            scheme: r.scheme,
            epoch: r.epoch,
            release_daa: r.release_daa,
            deadline_daa: r.deadline_daa,
        }
    }
}
impl PalwPanelSourceRequestV2 {
    pub fn request(&self) -> BeaconRequestV1 {
        BeaconRequestV1 {
            network: self.network,
            ruleset: self.ruleset,
            scheme: self.scheme,
            epoch: self.epoch,
            release_daa: self.release_daa,
            deadline_daa: self.deadline_daa,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelSourceFeeAuthorityV2 {
    pub version: u16,
    pub request: PalwPanelSourceRequestV2,
    pub snapshot_root: Digest,
    pub challenge_policy: PostCommitChallengePolicyV1,
    pub panel_policy: PanelPolicyV1,
    pub source_policy: PalwBeaconSourcePolicyV1,
    /// Derived from native challenge/court clocks by the consensus constructor,
    /// not supplied by the sponsor. Preserved alongside its ruleset-bound subject.
    pub native_liability_tail_daa: u64,
    pub authorized_daa: u64,
    pub sponsor: PalwBondKeyV2,
}

pub fn panel_source_target_v2(
    source: &PalwBeaconSourcePolicyV1,
    context: BeaconContextV1,
    request: &BeaconRequestV1,
    panel: &PanelPolicyV1,
    native_liability_tail_daa: u64,
    ledger: &misaka_palw_kernel::ledger::LedgerPolicyV1,
) -> Result<BeaconSourceTargetV1, String> {
    if !context.policy.is_paid_opt_in_source()
        || context.subject_kind != misaka_palw_challenge::SubjectKindV1::PanelAssignment
        || context.candidate_profile_id != misaka_palw_challenge::RootV1::Absent
        || panel.beacon_scheme.as_bytes() != context.policy.id()
        || request.scheme != panel.beacon_scheme
        || context.chain_genesis != request.network.as_bytes()
        || context.ruleset_id != request.ruleset.as_bytes()
        || context.challenge_epoch != request.epoch
        || context.commitment_position < request.release_daa
        || context.commitment_position > request.deadline_daa
        || request.epoch.checked_mul(panel.beacon_period_daa) != Some(request.release_daa)
        || request.release_daa.checked_add(panel.beacon_wait_daa) != Some(request.deadline_daa)
        || native_liability_tail_daa == 0
    {
        return Err("Panel source authority needs the exact funded epoch policy and native liability clocks".into());
    }
    panel.validate().map_err(|e| e.to_string())?;
    let floor = ledger
        .challenge_window_daa
        .checked_add(ledger.court_deadline_daa)
        .and_then(|n| n.checked_add(ledger.proof_grace_daa))
        .ok_or("source Final floor overflow")?;
    if source.source_final_slack_daa < floor {
        return Err("source Final slack does not cover its claim/court/grace".into());
    }
    let seal_start = context.commitment_position.checked_add(context.policy.anchor_delay_slots).ok_or("Panel source start overflow")?;
    let seal_end = seal_start.checked_add(context.policy.beacon_window_slots).ok_or("Panel source seal overflow")?;
    let reveal_end = seal_end.checked_add(context.policy.beacon_window_slots).ok_or("Panel source reveal overflow")?;
    let final_by = reveal_end.checked_add(source.source_final_slack_daa).ok_or("Panel source Final overflow")?;
    let last_lock = final_by.checked_add(context.policy.settlement_depth_d).ok_or("Panel source lock overflow")?;
    if last_lock > request.deadline_daa {
        return Err("Panel source Final/lock cannot fit the epoch's fixed beacon deadline".into());
    }
    let consumer_until = request
        .deadline_daa
        .checked_add(panel.assignment_delay_daa)
        .and_then(|n| n.checked_add(panel.receipt_window_daa))
        .and_then(|n| n.checked_add(native_liability_tail_daa))
        .ok_or("Panel source liability overflow")?;
    let target = BeaconSourceTargetV1 {
        terms: BeaconSourceTermsV1 {
            context: context.id(),
            seal_start,
            seal_end,
            reveal_end,
            final_by,
            hold_until: last_lock.max(consumer_until),
            deposit: source.deposit_sompi,
            max_per_producer: source.max_per_producer,
            max_total: source.max_total,
        },
        context,
    };
    target.validate()?;
    Ok(target)
}

impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn panel_source_fee_authority_v2(&self, context: &Digest) -> Result<PalwPanelSourceFeeAuthorityV2, String> {
        let a: PalwPanelSourceFeeAuthorityV2 = self
            .aux_row(PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, &borsh::to_vec(context).expect("context serializes"))
            .ok_or("Panel source fee authority missing")?;
        let snapshot = self.panel_source_snapshot_v1(a.request.epoch)?;
        let ctx = snapshot.context(&a.request.request(), &a.challenge_policy)?;
        if a.version != 2
            || a.snapshot_root != object_id(b"MISAKA/PALW/PANEL-SOURCE-SNAPSHOT/V1", &snapshot)
            || ctx.id() != *context
            || a.authorized_daa < snapshot.committed_daa
        {
            return Err("Panel source sponsor authority does not bind its frozen epoch".into());
        }
        let target = panel_source_target_v2(
            &a.source_policy,
            ctx,
            &a.request.request(),
            &a.panel_policy,
            a.native_liability_tail_daa,
            &self.header.policy,
        )?;
        if a.authorized_daa >= target.terms.seal_start || self.beacon_source_target_v4(context)? != target {
            return Err("Panel sponsor authorized after entropy or substituted source terms".into());
        }
        let monetary = self.beacon_source_fee_target_v5(context)?;
        if monetary.sponsor != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&a.sponsor) {
            return Err("Panel monetary sponsor differs from its signed native authority".into());
        }
        Ok(a)
    }

    pub fn panel_source_fee_context_v2(&self, epoch: u64) -> Result<Hash64, String> {
        let snapshot = self.panel_source_snapshot_v1(epoch)?;
        for ((table, _), bytes) in &self.aux {
            if *table != PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2 {
                continue;
            }
            let a: PalwPanelSourceFeeAuthorityV2 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
            if a.request.epoch == epoch && a.snapshot_root == object_id(b"MISAKA/PALW/PANEL-SOURCE-SNAPSHOT/V1", &snapshot) {
                let context = snapshot.context(&a.request.request(), &a.challenge_policy)?.id();
                self.panel_source_fee_authority_v2(&context)?;
                return Ok(Hash64::from_bytes(context));
            }
        }
        Err("Panel epoch has no accepted prepaid sponsor authority".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_source_clock_covers_consumers_and_refuses_impossible_entropy_windows() {
        let policy = misaka_palw_challenge::paid_opt_in_source_policy_v5(2, 2, 20, 2, 1);
        let panel = PanelPolicyV1 {
            seal_depth_blocks: 1,
            seal_wait_daa: 50,
            bond_maturity_daa: 1,
            beacon_period_daa: 300,
            beacon_wait_daa: 200,
            assignment_delay_daa: 2,
            receipt_window_daa: 20,
            seat_count: 3,
            outsider_seats: 0,
            max_retries: 1,
            min_collateral: 1,
            max_candidates: 64,
            max_pending: 64,
            max_pending_per_bond: 16,
            max_assignments_per_block: 8,
            max_admissions_per_block: 8,
            max_tracked_claims: 256,
            max_beacons_per_block: 2,
            max_beacon_proof_bytes: 4096,
            beacon_scheme: Hash64::from_bytes(policy.id()),
        };
        let request = BeaconRequestV1 {
            network: Hash64::from_u64_word(1),
            ruleset: Hash64::from_u64_word(2),
            scheme: panel.beacon_scheme,
            epoch: 1,
            release_daa: 300,
            deadline_daa: 500,
        };
        let context = BeaconContextV1 {
            chain_genesis: request.network.as_bytes(),
            ruleset_id: request.ruleset.as_bytes(),
            policy,
            subject_kind: misaka_palw_challenge::SubjectKindV1::PanelAssignment,
            commitment_root: [3; 64],
            commitment_position: 310,
            challenge_epoch: 1,
            eligible_profiles: [[4; 64]].into_iter().collect(),
            excluded_profiles: [[5; 64]].into_iter().collect(),
            candidate_profile_id: misaka_palw_challenge::RootV1::Absent,
        };
        let source = PalwBeaconSourcePolicyV1 { deposit_sompi: 100, max_per_producer: 8, max_total: 64, source_final_slack_daa: 120 };
        let ledger = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(request.network, request.ruleset);
        let target = panel_source_target_v2(&source, context.clone(), &request, &panel, 200, &ledger).unwrap();
        assert_eq!(
            (target.terms.seal_start, target.terms.seal_end, target.terms.reveal_end, target.terms.final_by, target.terms.hold_until),
            (312, 332, 352, 472, 722)
        );
        let short = BeaconRequestV1 { deadline_daa: 473, ..request };
        let short_panel = PanelPolicyV1 { beacon_wait_daa: 173, ..panel };
        assert!(panel_source_target_v2(&source, context.clone(), &short, &short_panel, 200, &ledger).is_err());
        assert!(panel_source_target_v2(&source, context.clone(), &request, &panel, 0, &ledger).is_err());
        assert!(panel_source_target_v2(&source, context.clone(), &request, &panel, u64::MAX, &ledger).is_err());
        let mut wrong = context;
        wrong.subject_kind = misaka_palw_challenge::SubjectKindV1::ClaimVerification;
        assert!(panel_source_target_v2(&source, wrong, &request, &panel, 200, &ledger).is_err());
    }
}
