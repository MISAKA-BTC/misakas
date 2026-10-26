//! Fresh Panel fee/source verification against authenticated ledger/aux roots.
//! RPC-provided self-consistent roots still need the reader's chain authority.
use crate::runtime_pack::commit::Refusal;
use kaspa_consensus_core::palw_panel_source_fee_v2::PalwPanelSourceFeeAuthorityV2;
use kaspa_hashes::Hash64;
use misaka_palw_challenge::{BeaconContextV1, OptInSourceV4, PaidOptInContractV5, PaidSourcePromiseV5, WorkBeaconV1};

pub struct VerifiedPaidPanelSourcesV2 {
    pub authority: PalwPanelSourceFeeAuthorityV2,
    pub funding: misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeFundingV1,
    pub cost_bounds: kaspa_consensus_core::palw_beacon_source_fee_v1::PalwBeaconSourceCostBoundsV1,
    context: BeaconContextV1,
    contract: PaidOptInContractV5,
    sources: Vec<OptInSourceV4>,
    promises: Vec<PaidSourcePromiseV5>,
    tip: u64,
}
impl VerifiedPaidPanelSourcesV2 {
    pub fn context(&self) -> &BeaconContextV1 {
        &self.context
    }
    pub fn verify_beacon(&self, presented: &WorkBeaconV1) -> Result<(), Refusal> {
        misaka_palw_challenge::verify_paid_opt_in_work_beacon_v5(
            &self.context,
            &self.contract,
            &self.sources,
            &self.promises,
            self.tip,
            presented,
        )
        .map(|_| ())
        .map_err(|e| Refusal::new("PANEL_BEACON_INVALID", e.to_string()))
    }
    pub fn derive_beacon(&self) -> Result<misaka_palw_challenge::SealedBeaconStateV3, Refusal> {
        misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(
            &self.context,
            &self.contract,
            &self.sources,
            &self.promises,
            self.tip,
        )
        .map_err(|e| Refusal::new("PANEL_BEACON_INVALID", e.to_string()))
    }
}

pub fn paid_panel_sources_from_kernel_rows_v2(
    header: &[u8],
    rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    ledger_root: &Hash64,
    aux_root: &Hash64,
    context: &misaka_palw_challenge::hash::Digest,
) -> Result<VerifiedPaidPanelSourcesV2, Refusal> {
    let route =
        kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(header, rows, ledger_root, aux_root)
            .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
    let authority = route.panel_source_fee_authority_v2(context).map_err(|e| Refusal::new("PANEL_AUTHORITY_INVALID", e))?;
    let snapshot = route.panel_source_snapshot_v1(authority.request.epoch).map_err(|e| Refusal::new("PANEL_SNAPSHOT_INVALID", e))?;
    let derived_context = snapshot
        .context(&authority.request.request(), &authority.challenge_policy)
        .map_err(|e| Refusal::new("PANEL_CONTEXT_INVALID", e))?;
    let (contract, sources, promises) = route
        .paid_beacon_material_v5(context)
        .map_err(|e| Refusal::new("PANEL_SOURCE_FEE_INVALID", e))?
        .ok_or_else(|| Refusal::new("PANEL_SOURCE_FEE_UNFUNDED", "Panel sponsor has not prepaid"))?;
    if sources.iter().any(|source| {
        matches!(source.source.reveal.as_ref().map(|r| &r.fate),
        Some(misaka_palw_challenge::SourceFateV3::Final(event)) if snapshot.assigned_work_ids.contains(&event.canonical_work_id))
    }) {
        return Err(Refusal::new("PANEL_CIRCULAR_SOURCE", "assigned work cannot seed its own Panel epoch"));
    }
    let ledger = route.ledger().map_err(|e| Refusal::new("PANEL_LEDGER_INVALID", e))?;
    let funding = ledger
        .beacon_source_fees
        .get(context)
        .cloned()
        .ok_or_else(|| Refusal::new("PANEL_SOURCE_FEE_UNFUNDED", "Panel sponsor has not prepaid"))?;
    let cost_bounds = route.beacon_source_cost_bounds_v5(context).map_err(|e| Refusal::new("PANEL_COST_INVALID", e))?;
    Ok(VerifiedPaidPanelSourcesV2 {
        authority,
        funding,
        cost_bounds,
        context: derived_context,
        contract,
        sources,
        promises,
        tip: ledger.daa,
    })
}

/// Build the exact native sponsor permit from one authenticated epoch snapshot
/// and the client's compiled network params. This does not sign, approve a
/// policy, or infer native free collateral from a kernel balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelSponsorRequestV2 {
    pub epoch: u64,
    pub sponsor: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    pub fee_per_source: u64,
    pub max_sources: u32,
}

pub fn panel_funding_object_from_kernel_rows_v2(
    header: &[u8],
    rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    ledger_root: &Hash64,
    aux_root: &Hash64,
    params: &kaspa_consensus_core::config::params::Params,
    challenge_policy: &misaka_palw_challenge::PostCommitChallengePolicyV1,
    proposal: &PanelSponsorRequestV2,
) -> Result<misaka_palw_kernel::route::KernelRouteObjectV1, Refusal> {
    let PanelSponsorRequestV2 { epoch, sponsor, fee_per_source, max_sources } = *proposal;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let route =
        kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(header, rows, ledger_root, aux_root)
            .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(Refusal::new("PANEL_POLICY_MISSING", "Panel funding needs compiled V2 params"));
    };
    let mirror = bundle.state.panel_v3().ok_or_else(|| Refusal::new("PANEL_POLICY_MISSING", "Panel policy is absent"))?;
    let source = params.palw_beacon_source_opt_in_v1.ok_or_else(|| Refusal::new("SOURCE_POLICY_MISSING", "source fence is absent"))?;
    let fee = params.palw_beacon_source_fee_v1.ok_or_else(|| Refusal::new("SOURCE_FEE_POLICY_MISSING", "monetary fence is absent"))?;
    let tip = route.ledger().map_err(|e| Refusal::new("PANEL_LEDGER_INVALID", e))?.daa;
    let snapshot = route.panel_source_snapshot_v1(epoch).map_err(|e| Refusal::new("PANEL_SNAPSHOT_INVALID", e))?;
    if snapshot.network != mirror.network
        || snapshot.ruleset != mirror.ruleset
        || route.header.policy.network_domain != mirror.network.as_bytes()
        || route.header.policy.ruleset_digest != mirror.ruleset.as_bytes()
        || snapshot.scheme != mirror.policy.beacon_scheme
        || !source.activation.is_active(tip)
        || !fee.activation.is_active(tip)
        || mirror.from_daa > tip
        || !challenge_policy.is_paid_opt_in_source()
    {
        return Err(Refusal::new(
            "PANEL_POLICY_SUBSTITUTED",
            "epoch or active source/fee policy differs from compiled network params",
        ));
    }
    let request = kaspa_consensus_core::palw_panel_source_fee_v2::PalwPanelSourceRequestV2 {
        network: snapshot.network,
        ruleset: snapshot.ruleset,
        scheme: snapshot.scheme,
        epoch,
        release_daa: snapshot.release_daa,
        deadline_daa: snapshot.deadline_daa,
    };
    let context = snapshot.context(&request.request(), challenge_policy).map_err(|e| Refusal::new("PANEL_CONTEXT_INVALID", e))?;
    let tail = bundle
        .state
        .window_court()
        .checked_mul(2)
        .and_then(|n| n.checked_add(bundle.state.window_challenge()))
        .ok_or_else(|| Refusal::new("PANEL_CLOCK_OVERFLOW", "native liability overflow"))?;
    let target = kaspa_consensus_core::palw_panel_source_fee_v2::panel_source_target_v2(
        &source.policy,
        context,
        &request.request(),
        &mirror.policy,
        tail,
        &route.header.policy,
    )
    .map_err(|e| Refusal::new("PANEL_TARGET_INVALID", e))?;
    let minimum = fee.policy.minimum_fee_for(&target).map_err(|e| Refusal::new("PANEL_COST_INVALID", e))?;
    if tip < snapshot.committed_daa
        || tip >= target.terms.seal_start
        || snapshot.committed_daa < source.activation.daa_score().max(fee.activation.daa_score())
        || fee_per_source < minimum
        || max_sources < challenge_policy.work_count_k
        || max_sources > fee.policy.max_funded_sources.min(target.terms.max_total)
        || route.aux.contains_key(&(
            kaspa_consensus_core::palw_panel_source_fee_v2::PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2,
            borsh::to_vec(&target.terms.context).expect("context serializes"),
        ))
        || route.beacon_source_target_v4(&target.terms.context).is_ok()
    {
        return Err(Refusal::new("PANEL_PERMIT_REFUSED", "late, exhausted, replayed or insufficiently funded sponsor permit"));
    }
    let terms = misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1 {
        source: target.terms,
        sponsor: kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&sponsor),
        fee_per_source,
        max_sources,
    };
    terms.validate().map_err(|e| Refusal::new("PANEL_PERMIT_REFUSED", e))?;
    Ok(misaka_palw_kernel::route::KernelRouteObjectV1::FundPanelBeaconSourceFeeV2 { epoch, terms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::params::ForkActivation;
    use kaspa_consensus_core::palw_beacon_source_fee_v1::{
        PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5, PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5, PalwBeaconSourceFeeFenceV1,
    };
    use kaspa_consensus_core::palw_beacon_source_opt_in_v1::{PALW_BEACON_SOURCE_TABLE_TARGETS_V4, PalwBeaconSourceFenceV1};
    use kaspa_consensus_core::palw_kernel_route_v1::{
        PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_policy_v1, palw_kernel_route_template_opv_v1,
    };
    use kaspa_consensus_core::palw_panel_source_fee_v2::{
        PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, PalwPanelSourceRequestV2, panel_source_target_v2,
    };
    use kaspa_consensus_core::palw_panel_source_snapshot_v1::{PALW_PANEL_SOURCE_SNAPSHOT_TABLE_V1, PalwPanelSourceSnapshotV1};
    use kaspa_consensus_core::palw_permissionless_panel_v1::PanelPolicyV1;
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use misaka_palw_kernel::beacon_source::BeaconSourceInputsV1;
    use misaka_palw_kernel::beacon_source_fee::{BeaconSourceFeeInputsV1, BeaconSourceFeeTargetV1, BeaconSourceFeeTermsV1};
    use misaka_palw_kernel::ledger::{AuthV1, KernelRouteObjectV1};

    fn funded_rows() -> (PalwKernelRouteStateV1, [u8; 64]) {
        let policy = misaka_palw_challenge::paid_opt_in_source_policy_v5(2, 2, 20, 2, 1);
        let network = Hash64::from_u64_word(1);
        let ruleset = Hash64::from_u64_word(2);
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
        let request =
            PalwPanelSourceRequestV2 { network, ruleset, scheme: panel.beacon_scheme, epoch: 1, release_daa: 300, deadline_daa: 500 };
        let snapshot = PalwPanelSourceSnapshotV1 {
            version: 1,
            network,
            ruleset,
            scheme: panel.beacon_scheme,
            epoch: 1,
            release_daa: 300,
            deadline_daa: 500,
            committed_daa: 310,
            committed_block: Hash64::from_u64_word(3),
            checkpoint: Hash64::from_u64_word(4),
            checkpoint_daa: 100,
            eligible_profiles: [[5; 64]].into_iter().collect(),
            excluded_profiles: [[6; 64]].into_iter().collect(),
            assigned_work_ids: [[7; 64]].into_iter().collect(),
        };
        let ledger_policy = palw_kernel_route_policy_v1(network, ruleset);
        let opv = kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1::at(ForkActivation::new(0)).opv_policy();
        let mut ledger = palw_kernel_route_template_opv_v1(ledger_policy, Some(opv));
        ledger.daa = 311;
        let source = PalwBeaconSourceFenceV1::unapproved_probe_at(ForkActivation::new(0));
        let target = panel_source_target_v2(
            &source.policy,
            snapshot.context(&request.request(), &policy).unwrap(),
            &request.request(),
            &panel,
            200,
            &ledger_policy,
        )
        .unwrap();
        let context = target.terms.context;
        let sponsor = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(10), index: 0 });
        let kid = palw_kernel_bond_id_v1(&sponsor);
        let monetary = PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::new(0));
        let fee_target = BeaconSourceFeeTargetV1 { target: target.clone(), sponsor: kid, minimum_fee: 50, max_sources: 2 };
        ledger
            .set_beacon_source_inputs(BeaconSourceInputsV1 { activation_daa: Some(0), targets: [(context, target.clone())].into() })
            .unwrap();
        ledger
            .set_beacon_source_fee_inputs(BeaconSourceFeeInputsV1 {
                activation_daa: Some(0),
                targets: [(context, fee_target.clone())].into(),
            })
            .unwrap();
        ledger.sync_bond(kid, 1000);
        ledger
            .apply_object(
                &KernelRouteObjectV1::FundBeaconSourceFee {
                    terms: BeaconSourceFeeTermsV1 { source: target.terms, sponsor: kid, fee_per_source: 50, max_sources: 2 },
                },
                &AuthV1 { signer_bond: kid },
            )
            .unwrap();
        let mut route = PalwKernelRouteStateV1::new(ledger_policy, Some(opv), ledger.scalars());
        route.rows = ledger.to_rows();
        let authority = PalwPanelSourceFeeAuthorityV2 {
            version: 2,
            request,
            snapshot_root: misaka_palw_challenge::hash::object_id(b"MISAKA/PALW/PANEL-SOURCE-SNAPSHOT/V1", &snapshot),
            challenge_policy: policy,
            panel_policy: panel,
            source_policy: source.policy,
            native_liability_tail_daa: 200,
            authorized_daa: 311,
            sponsor,
        };
        for (table, bytes) in [
            (PALW_BEACON_SOURCE_TABLE_TARGETS_V4, borsh::to_vec(&target).unwrap()),
            (PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5, borsh::to_vec(&fee_target).unwrap()),
            (PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5, borsh::to_vec(&monetary.policy.cost_bounds.unwrap()).unwrap()),
            (PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, borsh::to_vec(&authority).unwrap()),
        ] {
            route.aux.insert((table, borsh::to_vec(&context).unwrap()), bytes);
        }
        route.aux.insert((PALW_PANEL_SOURCE_SNAPSHOT_TABLE_V1, borsh::to_vec(&1u64).unwrap()), borsh::to_vec(&snapshot).unwrap());
        (route, context)
    }
    fn read(route: &PalwKernelRouteStateV1, context: &[u8; 64]) -> Result<VerifiedPaidPanelSourcesV2, Refusal> {
        paid_panel_sources_from_kernel_rows_v2(
            &borsh::to_vec(&route.header).unwrap(),
            route.rows_page_v1(None, usize::MAX).rows,
            &route.ledger_root(),
            &route.aux_root(),
            context,
        )
    }
    #[test]
    fn rooted_panel_permit_backing_and_full_snapshot_are_required_by_fresh_reader() {
        // Synthetic accepted authority/ledger rows: no native signature or useful-work
        // Final is asserted by this SDK reconstruction test.
        let (route, context) = funded_rows();
        let verified = read(&route, &context).unwrap();
        assert_eq!(verified.funding.held, 100);
        assert_eq!(verified.context().commitment_position, 310);
        assert!(matches!(verified.derive_beacon().unwrap(), misaka_palw_challenge::SealedBeaconStateV3::Sealing { sealed: 0, .. }));
        for table in [
            PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2,
            PALW_PANEL_SOURCE_SNAPSHOT_TABLE_V1,
            PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5,
            PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5,
        ] {
            let mut changed = route.clone();
            changed.aux.retain(|(t, _), _| *t != table);
            assert!(read(&changed, &context).is_err(), "missing authority {table} cannot be hidden by internally consistent roots");
        }
        let mut changed = route.clone();
        let key = (PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, borsh::to_vec(&context).unwrap());
        let mut authority: PalwPanelSourceFeeAuthorityV2 = borsh::from_slice(&changed.aux[&key]).unwrap();
        authority.sponsor.0.index += 1;
        changed.aux.insert(key, borsh::to_vec(&authority).unwrap());
        assert!(read(&changed, &context).is_err(), "fee sponsor cannot be replaced in a self-consistent rooted authority");
        let roots = (route.ledger_root(), route.aux_root());
        let mut rows = route.rows_page_v1(None, usize::MAX).rows;
        rows.pop();
        assert!(
            paid_panel_sources_from_kernel_rows_v2(&borsh::to_vec(&route.header).unwrap(), rows, &roots.0, &roots.1, &context)
                .is_err()
        );
    }
    #[test]
    fn sponsor_builder_derives_the_exact_native_clock_and_refuses_unfunded_or_late_terms() {
        use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
        use kaspa_consensus_core::palw_permissionless_panel_v1::PalwPanelV3ParamsV1;
        let (funded, context) = funded_rows();
        let authority = funded.panel_source_fee_authority_v2(&context).unwrap();
        let mut ledger = funded.ledger().unwrap();
        ledger.beacon_source_fees.clear();
        ledger.sync_bond(palw_kernel_bond_id_v1(&authority.sponsor), 1000);
        let mut route = PalwKernelRouteStateV1::new(funded.header.policy, funded.header.opv, ledger.scalars());
        route.rows = ledger.to_rows();
        route.aux = funded
            .aux
            .iter()
            .filter(|((table, _), _)| *table == PALW_PANEL_SOURCE_SNAPSHOT_TABLE_V1)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let mut params = kaspa_consensus_core::config::params::palw_t12_shipped_params().clone();
        let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { panic!("V2 fixture") };
        bundle.state = bundle.state.clone().with_panel_v3(Some(PalwPanelV3ParamsV1 {
            wire_version: 2,
            from_daa: 0,
            network: authority.request.network,
            ruleset: authority.request.ruleset,
            policy: authority.panel_policy,
        }));
        let tail = bundle.state.window_challenge() + 2 * bundle.state.window_court();
        params.palw_beacon_source_opt_in_v1 = Some(PalwBeaconSourceFenceV1::unapproved_probe_at(ForkActivation::new(0)));
        let mut fee = PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::new(0));
        fee.policy.cost_bounds.as_mut().unwrap().maximum_liability_daa = 20_000;
        params.palw_beacon_source_fee_v1 = Some(fee);
        let mut proposal = PanelSponsorRequestV2 { epoch: 1, sponsor: authority.sponsor, fee_per_source: 50, max_sources: 2 };
        let build = |route: &PalwKernelRouteStateV1,
                     params: &kaspa_consensus_core::config::params::Params,
                     proposal: &PanelSponsorRequestV2| {
            panel_funding_object_from_kernel_rows_v2(
                &borsh::to_vec(&route.header).unwrap(),
                route.rows_page_v1(None, usize::MAX).rows,
                &route.ledger_root(),
                &route.aux_root(),
                params,
                &authority.challenge_policy,
                proposal,
            )
        };
        let KernelRouteObjectV1::FundPanelBeaconSourceFeeV2 { epoch, terms } = build(&route, &params, &proposal).unwrap() else {
            panic!("new signed native kind only")
        };
        assert_eq!(epoch, 1);
        assert_eq!(terms.sponsor, palw_kernel_bond_id_v1(&authority.sponsor));
        assert_eq!(
            terms.source.hold_until,
            authority.request.deadline_daa
                + authority.panel_policy.assignment_delay_daa
                + authority.panel_policy.receipt_window_daa
                + tail
        );
        proposal.fee_per_source = 49;
        assert!(build(&route, &params, &proposal).is_err());
        proposal.fee_per_source = 50;
        proposal.max_sources = 1;
        assert!(build(&route, &params, &proposal).is_err());
        proposal.max_sources = 2;
        assert!(build(&funded, &params, &proposal).is_err(), "already sponsored authority cannot reset its promise book");
        let mut unknown = route.clone();
        unknown.aux.insert((PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, borsh::to_vec(&context).unwrap()), vec![255]);
        assert!(build(&unknown, &params, &proposal).is_err(), "unknown authority bytes cannot reset the sponsor lifetime");
        let mut late = route.clone();
        late.header.scalars.daa = terms.source.seal_start;
        assert!(build(&late, &params, &proposal).is_err());
        params.palw_beacon_source_fee_v1 = None;
        assert!(build(&route, &params, &proposal).is_err());
    }
}
