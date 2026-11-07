//! RFC-0045 auxiliary state mutations inside the canonical PALW writer.
//!
//! Plans below are private native-fold data. They are not admission capabilities:
//! the native ingress must derive costs, current UTXOs, accepted-prefix origin,
//! original rule scope and actual physical/service reservations before calling.
//! No independent store or success flag is maintained here. Every row uses the
//! existing KernelRouteRow delta, and rollback restores nonce and locks together.
use super::*;
use crate::{
    palw_positive_assignment_material_v2::{
        AssignmentAdmissionMaterialV2, AssignmentAdmissionMaterialViewV2,
    },
    palw_positive_rows_v2::PositiveRowsV2,
};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId, AssignmentNonceV2, AssignmentRoleV2,
        control_material::ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2,
        funding_relay::{
            FundingRelayRegistrationBodyV2, FundingRelayTermsV2, RelayFundingPurposeV2,
            RelayTargetV2,
        },
        funding_relay_policy::{FundingRelayAuxScanBudgetV2, FundingRelayPolicyV2},
        funding_relay_rows::{
            FundingRelayAuthorizationRowV2, FundingRelayBindingV2, FundingRelayCarrierFeesV2,
            FundingRelayCustodyRowV2,
        },
        journal::{AssignmentJournalEventKindV2, assignment_frame_v2},
        open_material::AssignmentOpenLimitsV2,
        rows::*,
    },
    identity::{Digest, PUBKEY_BYTES, SIGNATURE_BYTES},
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{
        AcceptedOrigin, CaseRow, ClaimIndexRow, FundingRow, NativeOutpoint, NativeRowBody,
        NativeRowValue, ResultRow, validate_bundle,
    },
};

/// Checked first close of an Assignment that never opened a Case.
/// The settlement owner qualifies the refund carrier against this snapshot.
#[allow(dead_code)]
pub struct CheckedUnconsumedAssignmentCloseV2 {
    assignment: misaka_palw_positive_contract::assignment::AssignmentId,
    original: misaka_palw_positive_contract::rows::AcceptedMaterial,
    status: misaka_palw_positive_contract::assignment::rows::AssignmentStatusV2,
    receipt: misaka_palw_positive_contract::rows::AcceptedMaterial,
    journal_tip: Digest,
    funding_image: Digest,
    limits: crate::palw_positive_court_v2::coordinator::assignment_body::AssignmentBodyLimitsV2,
    max_frame_bytes: u64,
    max_history_frames: u32,
    history_storage_limit: u64,
    cooperative: bool,
}

pub(super) enum InitialRelayFeeStageV2 {
    Preview,
    Observed,
}

fn refusal(detail: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(format!("Positive Assignment transition: {detail:?}"))
}
fn assignment_resource_error(
    error: crate::palw_positive_assignment_resources_v2::AssignmentResourceErrorV2,
) -> PalwStateV2Error {
    use crate::palw_positive_assignment_resources_v2::AssignmentResourceErrorV2 as E;
    match error {
        E::Native(why) => PalwStateV2Error::NativeResource(why),
        E::MissingNativeLease => PalwStateV2Error::NativeResource(
            crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                PositiveAssignmentRawActionV2,
            >(
                "Assignment derived construction",
                "actual native allocation lease is not armed",
            ),
        ),
        other => refusal(other),
    }
}
fn missing_assignment_history() -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(
        crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
    )
}
pub(super) fn assignment_horizon_v2(
    rules: &NativeAssignmentRulesV2,
    bundle: &misaka_palw_positive_contract::assignment::AssignmentAdmissionBundleV2,
    source: &misaka_palw_positive_contract::assignment::consents::AssignmentSourceConsentV2,
    prover: &misaka_palw_positive_contract::assignment::consents::AssignmentProverConsentV2,
    resources: &crate::palw_positive_assignment_resources_v2::AssignmentDerivedResourcesV2,
    original_daa: u64,
) -> Result<
    (
        crate::palw_positive_assignment_horizon_v2::AssignmentHorizonSpecV2,
        crate::palw_positive_assignment_horizon_v2::AssignmentSourceLedgerFloorV2,
    ),
    PalwStateV2Error,
> {
    let preimages = rules
        .native_preimages()
        .ok_or_else(|| refusal("missing original Case cost preimages"))?;
    let certification = rules
        .certification_policy()
        .ok_or_else(|| refusal("missing actual original Gate C policy"))?;
    resources
        .check_court_replay_work_v2(
            &preimages.source_policy.work,
            rules.assignment_policy(),
            rules.positive_policy(),
        )
        .map_err(assignment_resource_error)?;
    crate::palw_positive_assignment_horizon_v2::AssignmentHorizonSpecV2::derive(
        bundle,
        source,
        prover,
        rules.assignment_policy(),
        rules.positive_policy(),
        certification,
        &preimages.case_fees,
        rules.funding_relay_profile(),
        resources,
        &preimages.source_policy,
        original_daa,
    )
    .map_err(refusal)
}
fn assignment_service_demand_v2(
    source: &misaka_palw_positive_contract::assignment::consents::AssignmentSourceConsentV2,
    prover: &misaka_palw_positive_contract::assignment::consents::AssignmentProverConsentV2,
) -> [u64; 6] {
    [
        source.budget.additional_work,
        source.budget.transmitted_bytes,
        source.budget.retained_bytes,
        prover.max_build_work,
        prover.max_submitted_bytes,
        prover.max_retained_bytes,
    ]
}

fn assignment_join_service_demand_v2(
    join: &misaka_palw_positive_contract::assignment::role_actions::AssignmentProverJoinV2,
) -> [u64; 6] {
    [
        0,
        0,
        0,
        join.max_build_work,
        join.max_submitted_bytes,
        join.max_retained_bytes,
    ]
}

fn reserve_assignment_service_v2(
    available: &mut [u64; 6],
    demand: [u64; 6],
) -> Result<(), PalwStateV2Error> {
    let mut next = *available;
    for (remaining, requested) in next.iter_mut().zip(demand) {
        *remaining = remaining
            .checked_sub(requested)
            .ok_or_else(|| refusal("shared Assignment service capacity exhausted"))?;
    }
    *available = next;
    Ok(())
}

#[cfg(test)]
mod service_capacity_tests {
    use super::reserve_assignment_service_v2;

    #[test]
    fn combined_assignment_and_join_reservations_cannot_spend_another_dimension() {
        let mut supply = [100, 200, 300, 400, 500, 600];
        reserve_assignment_service_v2(&mut supply, [10, 20, 30, 40, 50, 60]).unwrap();
        reserve_assignment_service_v2(&mut supply, [0, 0, 0, 10, 20, 30]).unwrap();
        assert_eq!(supply, [90, 180, 270, 350, 430, 510]);
        let original = supply;
        assert!(reserve_assignment_service_v2(&mut supply, [1, 1, 1, 1, 1, 511]).is_err());
        assert_eq!(supply, original);
        reserve_assignment_service_v2(&mut supply, original).unwrap();
        assert_eq!(supply, [0; 6]);
    }
}

fn native_bond(outpoint: NativeOutpoint) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(
        Hash64::from_bytes(outpoint.transaction.0),
        outpoint.index,
    ))
}

pub(super) struct AssignmentInstallV2<'a> {
    pub open: &'a [u8],
    pub assignment: AssignmentRow,
}

struct AssignedCaseInstallV2<'a> {
    pub material: &'a AssignmentAdmissionMaterialV2,
    pub expected_assignment: AssignmentRow,
    pub index: ClaimIndexRow,
    pub result: ResultRow,
    pub case: CaseRow,
    pub funding: FundingRow,
}

/// Pending R authenticated against the selected parent and current prefix.
/// The workspace stays admitted until the native writer drops the package.
struct CheckedInitialFundingRelayV2 {
    custody: FundingRelayCustodyRowV2,
    service_cutoff: u64,
    _lease: crate::palw_native_resource_v1::NativeDemandLeaseV1,
}

impl TransitionBuilder<'_> {
    fn assignment_row_v2(
        &self,
        id: AssignmentId,
    ) -> Result<Option<AssignmentRow>, PalwStateV2Error> {
        match PositiveRowsV2::new(&self.state)
            .read(NativeRowKey::Assignment(id))
            .map_err(refusal)?
        {
            None => Ok(None),
            Some(value) => match value.body {
                NativeRowBody::Assignment(row) => Ok(Some(row)),
                _ => Err(refusal("wrong Assignment row kind")),
            },
        }
    }
    fn require_absent_assignment_key_v2(&self, key: NativeRowKey) -> Result<(), PalwStateV2Error> {
        if PositiveRowsV2::new(&self.state)
            .read(key)
            .map_err(refusal)?
            .is_some()
        {
            return Err(refusal(
                "an existing accepted row is never overwritten by intake",
            ));
        }
        Ok(())
    }
    /// Current registry ownership and ordinary work eligibility, distinct from
    /// an old signed duty which survives retirement. This is intake only.
    fn current_assignment_role_v2(&self, role: &AssignmentRoleRow) -> Result<(), PalwStateV2Error> {
        let key = native_bond(role.bond);
        let bond = self
            .state
            .bond(&key)
            .ok_or_else(|| refusal("missing current native role bond"))?;
        crate::palw_positive_metadata_v2::positive_registered_principal_v2(&self.state, &key)
            .map_err(refusal)?;
        if !palw_bond_may_take_work_v2(bond, self.params.min_collateral_sompi)
            || bond.pubkey.as_slice() != role.key.as_slice()
        {
            return Err(refusal("role key or current work eligibility changed"));
        }
        Ok(())
    }
    fn assignment_origin_v2(
        &self,
        origin: AcceptedOrigin,
        ctx: &PalwBlockContextV2,
    ) -> Result<(), PalwStateV2Error> {
        if origin.accepting_block.0 != ctx.block.as_bytes()
            || origin.accepted_daa != ctx.daa_score
            || origin.accepted_blue_score != ctx.blue_score
        {
            return Err(refusal(
                "event does not belong to this native accepting point",
            ));
        }
        Ok(())
    }
    pub(super) fn require_raw_assignment_origin_v2(
        &self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
    ) -> Result<(), PalwStateV2Error> {
        self.assignment_origin_v2(action.origin(), ctx)?;
        let tx = action.transaction();
        if tx.id().as_bytes() != action.origin().carrier_transaction.0
            || tx.inputs.len() != action.entries().len()
            || tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
            || tx.payload.get(..3) != Some(&[1, 0, 176])
        {
            return Err(refusal(
                "raw action lost its actual native transaction/origin binding",
            ));
        }
        Ok(())
    }
    /// Admits the canonical key before encoding, then the value/map/journal
    /// copies through the existing native typed writer. Never silently drops a
    /// write when the kernel header is absent.
    pub(super) fn write_assignment_aux_v2(
        &mut self,
        body: NativeRowBody,
    ) -> Result<(), PalwStateV2Error> {
        if self.state.kernel_route.is_none() {
            return Err(refusal("missing actual kernel route header"));
        }
        let key = body.key();
        if self.native_writers_tracking_v1() {
            let bytes = borsh::object_length(&(2u16, 1u16, &key)).map_err(refusal)? as u64;
            let mut quote =
                native_writers::NativeWriteQuoteV1::new("Positive Assignment aux151 key");
            quote.exact::<u8>(bytes)?;
            self.admit_native_write_v1(quote)?;
        }
        let key = key.encode();
        let value = NativeRowValue::new(body).map_err(refusal)?;
        if let NativeRowBody::Case(case) = &value.body
            && crate::palw_positive_certification_v2::positive_case_ends_certification_v2(case)
        {
            return self.write_terminal_assigned_case_and_withdraw_v2(&key, &value, case);
        }
        self.write_kernel_row_encoded_v1(KERNEL_AUX_TABLE, &key, Some(&value))
    }
    pub(super) fn append_assignment_frame_v2(
        &mut self,
        frame: AssignmentJournalFrameRow,
        retention: u64,
    ) -> Result<(), PalwStateV2Error> {
        let key = NativeRowKey::AssignmentJournalHead(frame.header.assignment);
        let previous = match PositiveRowsV2::new(&self.state)
            .read(key)
            .map_err(refusal)?
        {
            None => None,
            Some(value) => match value.body {
                NativeRowBody::AssignmentJournalHead(head) => Some(head),
                _ => return Err(refusal("wrong Assignment journal head kind")),
            },
        };
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentJournalFrame {
            assignment: frame.header.assignment,
            ordinal: frame.header.ordinal,
        })?;
        let head = frame
            .append_to(previous.as_ref(), retention)
            .map_err(refusal)?;
        self.write_assignment_aux_v2(NativeRowBody::AssignmentJournalFrame(frame))?;
        self.write_assignment_aux_v2(NativeRowBody::AssignmentJournalHead(head))
    }
    fn assignment_head_v2(
        &self,
        id: AssignmentId,
    ) -> Result<AssignmentJournalHeadRow, PalwStateV2Error> {
        match PositiveRowsV2::new(&self.state)
            .read(NativeRowKey::AssignmentJournalHead(id))
            .map_err(refusal)?
        {
            Some(value) => match value.body {
                NativeRowBody::AssignmentJournalHead(head) => Ok(head),
                _ => Err(refusal("wrong Assignment journal head kind")),
            },
            None => Err(refusal("missing accepted Assignment journal head")),
        }
    }
    pub(super) fn check_assignment_next_journal_extent_v2(
        &self,
        row: &AssignmentRow,
        head: &AssignmentJournalHeadRow,
        policy: &misaka_palw_positive_contract::assignment::policy::AssignmentPolicyV2,
        storage_limit: u64,
        native_body_bytes: usize,
    ) -> Result<(), PalwStateV2Error> {
        let max_frames_before = u64::from(policy.max_history_frames)
            .checked_sub(1)
            .ok_or_else(|| refusal("empty history policy"))?;
        let header_bytes =
            misaka_palw_positive_contract::assignment::journal::ASSIGNMENT_JOURNAL_HEADER_BYTES_V2
                as u64;
        let proposed = (native_body_bytes as u64)
            .checked_add(ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2 as u64)
            .filter(|n| *n <= policy.max_frame_bytes)
            .and_then(|n| n.checked_add(header_bytes))
            .ok_or_else(|| refusal("Assignment control exceeds the original frame cap"))?;
        let remaining = storage_limit
            .checked_sub(proposed)
            .ok_or_else(|| refusal("Assignment history budget exhausted"))?;
        let max_record_bytes = policy
            .max_frame_bytes
            .checked_add(header_bytes)
            .ok_or_else(|| refusal("original frame cap"))?;
        PositiveRowsV2::new(&self.state)
            .assignment_journal_extent_v2(
                head,
                row.origin,
                max_frames_before,
                max_record_bytes,
                remaining,
            )
            .map_err(|error| match error {
                crate::palw_positive_rows_v2::PositiveRowErrorV2::Contract(
                    misaka_palw_positive_contract::ContractError::Budget,
                ) => refusal("Assignment control exceeds the original cumulative history budget"),
                _ => missing_assignment_history(),
            })?;
        Ok(())
    }

    /// All purpose accounts share one ordinary native UTXO namespace. An
    /// output cannot simultaneously back a bond, legacy escrow, A or C.
    pub(super) fn require_assignment_output_unlocked_v2(
        &self,
        point: NativeOutpoint,
    ) -> Result<(), PalwStateV2Error> {
        let native = native_bond(point).0;
        if self.state.bonds.contains_key(&PalwBondKeyV2(native))
            || self
                .state
                .funded_escrows
                .values()
                .any(|row| row.settled_by.is_none() && row.funding_outpoint == native)
        {
            return Err(refusal("funding collides with another native lock"));
        }
        let route = self
            .state
            .kernel_route
            .as_ref()
            .ok_or_else(|| refusal("missing kernel route"))?;
        for ((table, key), bytes) in &route.aux {
            if *table != KERNEL_AUX_TABLE || !matches!(key.get(4), Some(7 | 13 | 18)) {
                continue;
            }
            let value = NativeRowValue::decode(key, bytes).map_err(refusal)?;
            let collision = match value.body {
                NativeRowBody::AssignmentFunding(row) => {
                    row.original_outpoint == point || row.current_outpoint == Some(point)
                }
                NativeRowBody::Funding(row) => {
                    row.original_outpoint == point || row.current_outpoint == Some(point)
                }
                NativeRowBody::AssignmentProverFunding(row) => {
                    row.original_outpoint == point || row.current_outpoint == Some(point)
                }
                _ => false,
            };
            if collision {
                return Err(refusal(
                    "funding output belongs to another Positive purpose account",
                ));
            }
        }
        Ok(())
    }
    /// The canonical Assignment row holds a slot until the native retention
    /// owner removes the released contract. A terminal status or clock alone
    /// cannot make room while funded roles or recovery material still exist.
    fn check_assignment_live_slots_v2(
        &mut self,
        prover: NativeOutpoint,
        new_assignment: Option<&misaka_palw_positive_contract::assignment::AssignmentTermsV2>,
        rules: &NativeAssignmentRulesV2,
    ) -> Result<(), PalwStateV2Error> {
        let policy = rules.assignment_policy();
        let native = rules
            .native_preimages()
            .ok_or_else(missing_assignment_history)?;
        let max_rows = policy
            .max_replay_work
            .checked_div(native.source_policy.work.per_control)
            .ok_or_else(|| refusal("invalid Assignment live census work schedule"))?
            / 2; // At most one Assignment row and one prover-role lookup per visit.
        let mut quote =
            native_writers::NativeWriteQuoteV1::new("native Assignment live census key workspace");
        // Two canonical prefix buffers and the concurrent exact role key pair.
        quote.grown::<u8>(1024)?;
        quote.grown::<u8>(1024)?;
        quote.exact::<u8>(2 * misaka_palw_positive_contract::rows::MAX_NATIVE_KEY_BYTES as u64)?;
        self.admit_native_write_v1(quote)?;
        let mut start = NativeRowKey::Assignment(AssignmentId(Digest([0; 64]))).encode();
        start.truncate(start.len() - 64);
        let start = (KERNEL_AUX_TABLE, start);
        let route = self
            .state
            .kernel_route
            .as_ref()
            .ok_or_else(missing_assignment_history)?;
        let rows = PositiveRowsV2::new(&self.state);
        let mut available = [
            policy.live.global,
            policy.live.per_poster,
            policy.live.per_worker,
            policy.live.per_prover,
            policy.live.per_class,
        ];
        for (ordinal, ((_, key), bytes)) in route
            .aux
            .range(start.clone()..)
            .take_while(|((table, key), _)| *table == KERNEL_AUX_TABLE && key.starts_with(&start.1))
            .enumerate()
        {
            if ordinal as u64 >= max_rows {
                return Err(refusal(
                    "Assignment live census exceeds original finite work budget",
                ));
            }
            let NativeRowBody::Assignment(existing) =
                NativeRowValue::decode(key, bytes).map_err(refusal)?.body
            else {
                return Err(refusal("wrong native live Assignment row kind"));
            };
            let occupied = &existing.bundle.terms;
            let prover_occupied = if occupied.prover == prover {
                true
            } else {
                let key = NativeRowKey::AssignmentRole {
                    assignment: existing.id(),
                    role: AssignmentRoleV2::Prover,
                    bond: prover,
                };
                rows.read(key).map_err(refusal)?.is_some()
            };
            let uses = [
                new_assignment.is_some(),
                new_assignment.is_some_and(|terms| occupied.poster == terms.poster),
                new_assignment.is_some_and(|terms| occupied.worker == terms.worker),
                prover_occupied,
                new_assignment
                    .is_some_and(|terms| occupied.registered_class == terms.registered_class),
            ];
            for (slots, occupied) in available.iter_mut().zip(uses) {
                if occupied {
                    *slots = slots
                        .checked_sub(1)
                        .filter(|n| *n != 0)
                        .ok_or_else(|| refusal("Assignment live cap exhausted"))?;
                }
            }
        }
        Ok(())
    }

    /// Rebuild occupancy from the canonical prefix. The same funding rows
    /// count after restart or rollback and across A-to-C consumption. A clock
    /// or terminal status by itself does not retire the original obligation.
    fn check_assignment_service_capacity_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        rules: &NativeAssignmentRulesV2,
        demand: [u64; 6],
    ) -> Result<(), PalwStateV2Error> {
        let supply = rules.service_capacity_at_v2(ctx.daa_score).ok_or_else(|| {
            refusal("shared Assignment service capacity is unavailable at this prefix")
        })?;
        supply.validate_v2().map_err(refusal)?;
        let mut available = supply.dimensions_v2();
        reserve_assignment_service_v2(&mut available, demand)?;
        let native = rules
            .native_preimages()
            .ok_or_else(missing_assignment_history)?;
        let mut remaining_work = rules.assignment_policy().max_replay_work;
        let mut quote = native_writers::NativeWriteQuoteV1::new(
            "native Assignment service census key workspace",
        );
        quote
            .exact::<u8>((2 * misaka_palw_positive_contract::rows::MAX_NATIVE_KEY_BYTES) as u64)?;
        self.admit_native_write_v1(quote)?;
        let id = AssignmentId(Digest([0; 64]));
        let bond = NativeOutpoint {
            transaction: misaka_palw_positive_contract::rows::NativeHash64([0; 64]),
            index: 0,
        };
        let route = self
            .state
            .kernel_route
            .as_ref()
            .ok_or_else(missing_assignment_history)?;
        for subject in [
            NativeRowKey::Assignment(id),
            NativeRowKey::AssignmentProverFunding {
                assignment: id,
                prover: bond,
            },
        ] {
            let mut prefix = subject.encode();
            prefix.truncate(5);
            let start = (KERNEL_AUX_TABLE, prefix);
            for ((_, key), bytes) in
                route
                    .aux
                    .range(start.clone()..)
                    .take_while(|((table, key), _)| {
                        *table == KERNEL_AUX_TABLE && key.starts_with(&start.1)
                    })
            {
                let scan = native
                    .source_policy
                    .work
                    .court_body_work(bytes.len() as u64)
                    .map_err(refusal)?;
                remaining_work = remaining_work
                    .checked_sub(scan)
                    .and_then(|n| n.checked_sub(native.source_policy.work.per_control))
                    .ok_or_else(|| {
                        refusal("Assignment service census exceeds original finite work budget")
                    })?;
                let used = match NativeRowValue::decode(key, bytes).map_err(refusal)?.body {
                    NativeRowBody::Assignment(row) => {
                        assignment_service_demand_v2(&row.source_consent, &row.prover_consent)
                    }
                    NativeRowBody::AssignmentProverFunding(row) => {
                        assignment_join_service_demand_v2(&row.consent)
                    }
                    _ => return Err(refusal("wrong native Assignment service row kind")),
                };
                reserve_assignment_service_v2(&mut available, used)?;
            }
        }
        Ok(())
    }

    fn assignment_open_roles_v2(
        &self,
        action: &PositiveAssignmentRawActionV2,
        row: &AssignmentRow,
    ) -> Result<[AssignmentRoleRow; 3], PalwStateV2Error> {
        use borsh::BorshDeserialize;
        use misaka_palw_positive_contract::assignment::{
            AssignmentAdmissionBundleV2,
            consents::{AssignmentProverConsentV2, AssignmentSourceConsentV2},
        };

        let body = &action.transaction().payload[3..];
        let limits = action.rules().body_limits().map_err(refusal)?;
        crate::palw_positive_court_v2::coordinator::assignment_body::scan_assignment_body_v2(
            body, limits,
        )
        .map_err(refusal)?;
        // Read only the fixed signed fields, leaving the already scanned
        // layout borrowed in the carrier.
        let mut fixed = &body[5..];
        let bundle = AssignmentAdmissionBundleV2::deserialize(&mut fixed).map_err(refusal)?;
        let source = AssignmentSourceConsentV2::deserialize(&mut fixed).map_err(refusal)?;
        let prover = AssignmentProverConsentV2::deserialize(&mut fixed).map_err(refusal)?;
        if bundle != row.bundle || source != row.source_consent || prover != row.prover_consent {
            return Err(refusal(
                "Open rows do not match the literal carrier consents",
            ));
        }
        let mut read_role = |role, point| -> Result<AssignmentRoleRow, PalwStateV2Error> {
            let (signature, rest) = fixed
                .split_at_checked(SIGNATURE_BYTES)
                .ok_or_else(|| refusal("Open role signature is truncated"))?;
            fixed = rest;
            let bond = self
                .state
                .bond(&native_bond(point))
                .ok_or_else(|| refusal("missing current native role bond"))?;
            let role = AssignmentRoleRow {
                assignment: row.id(),
                role,
                bond: point,
                key: bond.pubkey.as_slice().try_into().map_err(refusal)?,
                signature: signature.try_into().map_err(refusal)?,
                bundle: row.bundle.id(),
                authorization: AssignmentRoleAuthorizationV2::Initial,
                admission: row.admission_material,
                funding: (role == AssignmentRoleV2::Poster).then_some(row.admission_material),
                lease: row.reservation,
                origin: row.origin,
                retired: false,
            };
            role.authenticate(row).map_err(refusal)?;
            self.current_assignment_role_v2(&role)?;
            self.require_absent_assignment_key_v2(NativeRowKey::AssignmentRole {
                assignment: row.id(),
                role: role.role,
                bond: point,
            })?;
            Ok(role)
        };
        let terms = &row.bundle.terms;
        Ok([
            read_role(AssignmentRoleV2::Poster, terms.poster)?,
            read_role(AssignmentRoleV2::Worker, terms.worker)?,
            read_role(AssignmentRoleV2::Prover, terms.prover)?,
        ])
    }

    /// RFC-0045's initial topology protects the exclusive native job at Open
    /// and rechecks the exact claim and empty dependencies at Bind and replay.
    /// This bound applies only to that closed ingress.
    fn check_initial_assignment_gate_c_envelope_v2(
        &self,
        rules: &NativeAssignmentRulesV2,
        terms: &misaka_palw_positive_contract::assignment::AssignmentTermsV2,
        job: Hash64,
        horizon: &crate::palw_positive_assignment_horizon_v2::AssignmentHorizonSpecV2,
    ) -> Result<(), PalwStateV2Error> {
        use misaka_palw_positive_contract::assignment::AssignmentModeV2;

        self.require_unowned_assignment_job_v2(job)?;
        if !matches!(terms.mode, AssignmentModeV2::CanonicalTirAssignmentV1)
            || job.as_bytes() != terms.id().0.0
        {
            return Err(refusal(
                "Gate C requires the exclusive canonical Assignment topology",
            ));
        }
        let clock = rules
            .gate_c_clock(terms.result_before_daa)
            .map_err(refusal)?;
        let prosecution = clock
            .prosecution_not_before_daa(horizon.court_before_daa)
            .map_err(refusal)?;
        let end = clock
            .no_draw_expiry_daa
            .max(clock.latest_failed_attestation_decision_daa)
            .max(
                prosecution
                    .checked_add(1)
                    .ok_or_else(|| refusal("Gate C prosecution bound overflow"))?,
            )
            .max(horizon.liability_until_daa);
        let refund = end
            .checked_add(rules.assignment_policy().refund_window_daa)
            .ok_or_else(|| refusal("Gate C refund bound overflow"))?;
        if horizon.latest_prosecution_daa != prosecution
            || refund > horizon.refund_before_daa
            || horizon.refund_before_daa > terms.reserve_until_daa
        {
            return Err(refusal(
                "original Assignment reserve cannot cover all Gate C clocks and refund",
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_positive_assignment_open_v2(
        &self,
        action: &PositiveAssignmentRawActionV2,
        bundle: &misaka_palw_positive_contract::assignment::AssignmentAdmissionBundleV2,
        source: &misaka_palw_positive_contract::assignment::consents::AssignmentSourceConsentV2,
        prover: &misaka_palw_positive_contract::assignment::consents::AssignmentProverConsentV2,
        resources: &crate::palw_positive_assignment_resources_v2::AssignmentDerivedResourcesV2,
        registered: crate::palw_positive_assignment_material_v2::AssignmentRegisteredDescriptorV2,
        money: crate::palw_positive_assignment_funds_v2::AssignmentMoneyQuoteV2,
        cohort: &[u8],
        encode_cap: u64,
    ) -> Result<(AssignmentRow, Vec<u8>), PalwStateV2Error> {
        use misaka_palw_positive_contract::rows::AcceptedMaterial;

        let rules = action.rules();
        let terms = &bundle.terms;
        let origin = action.origin();
        let worker_key = self
            .state
            .bond(&native_bond(terms.worker))
            .ok_or_else(missing_assignment_history)?
            .pubkey
            .as_slice()
            .try_into()
            .map_err(refusal)?;
        let class = self
            .state
            .tir_class_v1(&Hash64::from_bytes(terms.registered_class.0))
            .ok_or_else(missing_assignment_history)?;
        let job = resources.binding().job();
        let pin = |digest| AcceptedMaterial { origin, digest };
        // The Open encoder replaces the three custody pins before publishing.
        let pending = pin(Digest([0; 64]));
        let row = AssignmentRow {
            bundle: *bundle,
            source_consent: *source,
            prover_consent: *prover,
            origin,
            worker_key,
            positive_policy: rules.assignment_policy().positive_policy,
            assignment_policy: rules.assignment_policy().id(),
            fired_rules: rules.rule_scope().fired_rules_digest(),
            job: Digest(job.job_id.as_bytes()),
            input: Digest(job.prompt_token_ids_hash.as_bytes()),
            context: Digest(job.context_hash().as_bytes()),
            registered_metadata: registered.id(),
            status: AssignmentStatusV2::Open,
            derivation: pin(terms.derivation_policy),
            dependencies: pin(terms.empty_dependencies_id()),
            reservation: pending,
            liability: pending,
            last_event: origin,
            admission_material: pending,
        };
        let material = AssignmentAdmissionMaterialV2::from_native_open_v2(
            &row,
            rules,
            resources,
            registered,
            money,
            class.facts.prompt_ids_form(rules.prompt_form()),
            rules.assignment_policy().max_replay_work,
        )
        .map_err(refusal)?;
        crate::palw_positive_assignment_material_v2::encode_native_assignment_open_v2(
            row,
            material,
            &action.transaction().payload[3..],
            cohort,
            AssignmentOpenLimitsV2::within_payload(encode_cap),
        )
        .map_err(refusal)
    }

    /// One admission checkpoint owns the permanent poster counter, all three
    /// role signatures, initial A lock, reservation/liability references and
    /// journal. A later error undoes every one of them.
    pub(super) fn install_positive_assignment_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        plan: AssignmentInstallV2<'_>,
    ) -> Result<(), PalwStateV2Error> {
        let row = &plan.assignment;
        let id = row.id();
        self.assignment_origin_v2(row.origin, ctx)?;
        let action = self
            .assignment_active_raw
            .clone()
            .ok_or_else(missing_assignment_history)?;
        if action.origin() != row.origin
            || action.transaction().payload.get(..8) != Some(&[1, 0, 176, 2, 0, 1, 0, 8])
            || self
                .assignment_raw_boundary
                .is_none_or(|(_, end)| end != self.entries.len())
        {
            return Err(refusal(
                "Open does not belong to the current pre-op8 boundary",
            ));
        }
        let rules = action.rules();
        let frame_cap = rules.assignment_policy().max_frame_bytes;
        let payload_bytes = (plan.open.len() as u64)
            .checked_add(ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2 as u64)
            .filter(|bytes| *bytes <= frame_cap)
            .ok_or_else(|| refusal("Open exceeds the actual intake frame cap"))?;
        let original = AssignmentAdmissionMaterialViewV2::decode(
            row,
            plan.open,
            &action.transaction().payload[3..],
            AssignmentOpenLimitsV2::within_payload(frame_cap),
        )
        .map_err(refusal)?;
        let material = &original.material;
        if material.network != rules.network()
            || material.fired_rules != rules.rule_scope().fired_rules_digest()
            || &material.assignment_policy != rules.assignment_policy()
            || &material.positive_policy != rules.positive_policy()
            || Some(&material.native_preimages) != rules.native_preimages()
            || Some(&material.certification_policy) != rules.certification_policy()
            || &material.funding_relay != rules.funding_relay_profile()
            || material.network_prompt_form != rules.prompt_form()
        {
            return Err(refusal(
                "Open material differs from the actual fired intake profile",
            ));
        }
        material
            .check_native_open_record_extent_v2(payload_bytes)
            .map_err(refusal)?;
        material
            .check_native_open_quote_v2(row, original.open.native_body())
            .map_err(refusal)?;
        material
            .check_registered_class_v2(&self.state)
            .map_err(refusal)?;
        if action.registered_class_v2() != Some(material.registered) {
            return Err(refusal(
                "Open registration differs from the native selected-parent archive",
            ));
        }
        let (anchor, anchor_daa) = action
            .signed_anchor_clock_v2()
            .ok_or_else(missing_assignment_history)?;
        if anchor.as_bytes() != row.bundle.terms.branch_anchor.0 {
            return Err(refusal("Open uses a different signed ancestor"));
        }
        self.state
            .check_original_assignment_verifier_candidates_v2(
                &original,
                anchor_daa,
                row.bundle.terms.result_before_daa,
                material.custody.replay_work,
            )?;
        if row.status != AssignmentStatusV2::Open || row.last_event != row.origin {
            return Err(refusal(
                "initial Assignment row must be open at the current accepted origin",
            ));
        }
        self.require_absent_assignment_key_v2(NativeRowKey::Assignment(id))?;
        self.require_unowned_assignment_job_v2(Hash64::from_bytes(row.job.0))?;
        self.require_new_positive_draw_slot_v2(&material.certification_policy, ctx.daa_score)?;
        self.check_assignment_live_slots_v2(
            row.bundle.terms.prover,
            Some(&row.bundle.terms),
            rules,
        )?;
        self.check_assignment_service_capacity_v2(
            ctx,
            rules,
            assignment_service_demand_v2(&row.source_consent, &row.prover_consent),
        )?;
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentFunding(id))?;
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentUse(id))?;
        let roles = self.assignment_open_roles_v2(&action, row)?;
        let poster = &roles[0];
        let funding =
            crate::palw_positive_assignment_funds_v2::qualify_assignment_funding_output_v2(
                &row.bundle,
                action.transaction(),
                &poster.key,
                action.current_output(),
                material.money.fee,
                material.money.capital,
                ctx.daa_score,
            )
            .map_err(refusal)?;
        self.require_assignment_output_unlocked_v2(funding.outpoint())?;
        let account = funding
            .initial_account_v2(row, poster.key, poster.signature)
            .map_err(refusal)?;
        let relay = self.check_initial_assignment_relay_v2(
            ctx,
            &action,
            &row.bundle,
            (funding.outpoint(), funding.amount()),
            &poster.key,
        )?;
        let nonce_key = NativeRowKey::AssignmentNonce(row.bundle.terms.poster);
        let previous = match PositiveRowsV2::new(&self.state)
            .read(nonce_key)
            .map_err(refusal)?
        {
            None => AssignmentNonceV2::default(),
            Some(value) => match value.body {
                NativeRowBody::AssignmentNonce(nonce)
                    if nonce.poster == row.bundle.terms.poster =>
                {
                    nonce.nonce
                }
                _ => return Err(refusal("wrong permanent poster nonce row")),
            },
        };
        let nonce = previous
            .after_accepting(row.bundle.terms.poster_nonce)
            .map_err(refusal)?;
        let poster = row.bundle.terms.poster;
        let origin = row.origin;
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            let payload = self.encode_assignment_control_payload_v2(id, plan.open, frame_cap)?;
            let frame = assignment_frame_v2(
                id,
                0,
                Digest([0; 64]),
                AssignmentJournalEventKindV2::Open,
                origin,
                &payload,
                frame_cap,
            )
            .map_err(refusal)?;
            self.write_assignment_aux_v2(NativeRowBody::AssignmentNonce(AssignmentNonceRow {
                poster,
                nonce,
            }))?;
            self.write_assignment_aux_v2(NativeRowBody::Assignment(plan.assignment))?;
            self.write_assignment_aux_v2(NativeRowBody::AssignmentFunding(account))?;
            for role in roles {
                self.write_assignment_aux_v2(NativeRowBody::AssignmentRole(role))?;
            }
            if let Some(CheckedInitialFundingRelayV2 {
                mut custody,
                _lease,
                ..
            }) = relay
            {
                custody.binding = Some(FundingRelayBindingV2 {
                    origin,
                    original_funding: funding.outpoint(),
                    predecessor: None,
                    original_contract: original.material.unsigned_bundle,
                    reservation: material.custody.reservation,
                });
                self.write_assignment_aux_v2(NativeRowBody::FundingRelayCustody(custody))?;
            }
            self.append_assignment_frame_v2(frame, material.custody.retention_until_daa)?;
            self.emit_assignment_material_v2(&frame, &payload, frame_cap)
        })();
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }

    /// A binds exactly one literal r1 Result and C. No old op0 row is attached
    /// retroactively and no candidate output is copied into expected metadata.
    fn consume_positive_assignment_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        plan: AssignedCaseInstallV2<'_>,
        relay: Option<CheckedInitialFundingRelayV2>,
        horizon: crate::palw_positive_assignment_horizon_v2::AssignmentHorizonSpecV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_positive_court_v2::coordinator::assignment_body::{
            AssignedResultPayloadV2, scan_assignment_body_v2, scan_assignment_continuation_id_v2,
        };
        use misaka_palw_positive_contract::{
            messages::{ClaimStatus, MinerDataStatus, WitnessStatus},
            rows::{AcceptedMaterial, AggregateUsage},
        };

        let expected = &plan.expected_assignment;
        let id = expected.id();
        let origin = plan.result.origin;
        self.assignment_origin_v2(origin, ctx)?;
        let action = self
            .assignment_active_raw
            .clone()
            .ok_or_else(missing_assignment_history)?;
        if action.origin() != origin
            || action.transaction().payload.get(..8) != Some(&[1, 0, 176, 2, 0, 1, 0, 9])
            || self
                .assignment_raw_boundary
                .is_none_or(|(_, end)| end != self.entries.len())
            || action.rules().original_material() != Some(expected.admission_material)
        {
            return Err(refusal(
                "BindResult does not belong to the current pre-op9 boundary",
            ));
        }
        plan.material.check_row(expected).map_err(refusal)?;
        let literal = &action.transaction().payload[3..];
        scan_assignment_body_v2(literal, action.rules().body_limits().map_err(refusal)?)
            .map_err(refusal)?;
        let carried =
            borsh::from_slice::<AssignedResultPayloadV2>(&literal[69..]).map_err(refusal)?;
        if scan_assignment_continuation_id_v2(literal).map_err(refusal)? != id
            || carried.result != plan.result.result
            || carried.miner_signature != plan.result.miner_signature
            || carried.service != plan.case.service
            || carried.basis != plan.funding.basis
            || carried.consent != plan.funding.consent
            || carried.payer_signature != plan.funding.payer_signature
        {
            return Err(refusal("BindResult rows differ from the literal carrier"));
        }
        if self.assignment_row_v2(id)?.as_ref() != Some(expected)
            || expected.status != AssignmentStatusV2::Open
            || ctx.daa_score >= expected.bundle.terms.result_before_daa
            || plan.case.last_event != origin
            || plan.case.status != ClaimStatus::ResultCommitted
            || plan.case.miner_data != MinerDataStatus::NotRequested
            || plan.case.witness_status != WitnessStatus::Building
            || plan.case.positive_witness.is_some()
            || plan.case.usage != AggregateUsage::default()
            || plan.case.unresolved_fraud != 0
            || plan.case.maturity_daa < origin.accepted_daa
            || plan.case.maturity_daa >= expected.bundle.terms.reserve_until_daa
            || plan.result.policy != expected.positive_policy
            || plan.result.fired_fork != expected.fired_rules
            || plan.result.dependencies != expected.dependencies
            || plan.result.miner_key != expected.worker_key
            || plan.result.role_assignment
                != (AcceptedMaterial {
                    origin: expected.origin,
                    digest: id.0,
                })
        {
            return Err(refusal(
                "Assignment is stale, expired, consumed or differently bound",
            ));
        }
        validate_bundle(&plan.index, &plan.result, &plan.case, &plan.funding).map_err(refusal)?;
        super::positive_assignment_case::reject_legacy_aliases(
            &self.state,
            expected,
            &plan.result.result,
        )?;
        let future = expected.bundle.future_case;
        if plan.funding.consent.payer != future.payer
            || plan.funding.consent.refund_payload != future.refund_payload
            || plan.funding.basis.required_fee > future.max_case_fee
            || plan.funding.basis.required_capital > future.max_case_capital
            || plan.funding.initial_locked > future.max_case_locked
            || ctx.daa_score >= future.case_admission_before_daa
        {
            return Err(refusal("C exceeds original signed future-Case bounds"));
        }
        for key in [
            NativeRowKey::AssignmentUse(id),
            NativeRowKey::Result(plan.result.result.id()),
            NativeRowKey::ClaimIndex(plan.index.claim),
            NativeRowKey::Case(plan.case.case),
            NativeRowKey::Funding(plan.case.case),
            NativeRowKey::JournalHead(plan.case.case),
        ] {
            self.require_absent_assignment_key_v2(key)?;
        }
        let a_funding = PositiveRowsV2::new(&self.state)
            .read(NativeRowKey::AssignmentFunding(id))
            .map_err(refusal)?
            .ok_or_else(|| refusal("missing original A funding"))?;
        let NativeRowBody::AssignmentFunding(a_funding) = a_funding.body else {
            return Err(refusal("wrong A funding row kind"));
        };
        let output =
            crate::palw_positive_assignment_funds_v2::qualify_assigned_case_funding_output_v2(
                expected,
                &a_funding,
                &carried.result,
                &carried.service,
                &carried.basis,
                &carried.consent,
                &plan.funding.payer_key,
                action.transaction(),
                action.current_output(),
                ctx.daa_score,
            )
            .map_err(refusal)?;
        if plan.funding.original_outpoint != output.outpoint()
            || plan.funding.current_outpoint != Some(output.outpoint())
            || plan.funding.initial_locked != output.amount()
            || [plan.funding.fee, plan.funding.capital, plan.funding.surplus]
                != output.initial_legs()[1..]
            || plan.funding.liability != plan.case.liability
            || plan.funding.last_settlement.is_some()
        {
            return Err(refusal(
                "initial Case funding differs from the actual funded carrier",
            ));
        }
        self.require_assignment_output_unlocked_v2(output.outpoint())?;
        // No mutation intervenes between native carriage validation and this
        // checkpoint. Keep the checked relay's capacity owner through install.
        horizon
            .check_case_maturity_v2(
                ctx.daa_score,
                plan.case.maturity_daa,
                plan.material.assignment_policy.refund_window_daa,
            )
            .map_err(refusal)?;
        if plan.case.reservation
            != crate::palw_positive_assignment_material_v2::assigned_case_reservation_v2(
                expected,
                origin,
                &plan.case,
                &plan.funding,
            )
        {
            return Err(refusal(
                "Case reservation does not carry the original Assignment custody and literal funding",
            ));
        }

        let head = self.assignment_head_v2(id)?;
        let policy = &plan.material.assignment_policy;
        self.check_assignment_next_journal_extent_v2(
            expected,
            &head,
            policy,
            plan.material.native_assignment_journal_storage_limit_v2(),
            literal.len(),
        )?;
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            let payload =
                self.encode_assignment_control_payload_v2(id, literal, policy.max_frame_bytes)?;
            let frame = assignment_frame_v2(
                id,
                head.next_ordinal,
                head.last_frame,
                AssignmentJournalEventKindV2::BindResult,
                origin,
                &payload,
                policy.max_frame_bytes,
            )
            .map_err(refusal)?;
            let case = plan.case.case;
            let result = plan.result.result.id();
            let use_row = AssignmentUseRow {
                assignment: id,
                case,
                result,
                origin,
                binding: AcceptedMaterial {
                    origin,
                    digest: frame.header.id(),
                },
            };
            let scope = plan.result.result.scope;
            let mut row = plan.expected_assignment;
            row.status = AssignmentStatusV2::Consumed { case, result };
            row.last_event = origin;
            self.write_assignment_aux_v2(NativeRowBody::Assignment(row))?;
            self.write_assignment_aux_v2(NativeRowBody::AssignmentUse(use_row))?;
            self.write_assignment_aux_v2(NativeRowBody::Result(plan.result))?;
            self.write_assignment_aux_v2(NativeRowBody::ClaimIndex(plan.index))?;
            if let Some(CheckedInitialFundingRelayV2 {
                mut custody,
                _lease,
                ..
            }) = relay
            {
                custody.binding = Some(FundingRelayBindingV2 {
                    origin,
                    original_funding: output.outpoint(),
                    predecessor: None,
                    original_contract: carried.consent.id().0,
                    reservation: plan.case.reservation,
                });
                self.write_assignment_aux_v2(NativeRowBody::FundingRelayCustody(custody))?;
            }
            self.write_assignment_aux_v2(NativeRowBody::Case(plan.case))?;
            self.write_assignment_aux_v2(NativeRowBody::Funding(plan.funding))?;
            self.begin_positive_certification_v2(ctx, id, plan.material)?;
            self.append_assignment_frame_v2(frame, head.retention_until_daa)?;
            self.emit_assignment_material_v2(&frame, &payload, policy.max_frame_bytes)?;
            self.write_initial_assigned_case_journal_v2(
                &use_row,
                scope,
                &frame.header,
                &plan.material.positive_policy,
            )
        })();
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }
}

impl TransitionBuilder<'_> {
    /// Check original and joined roles by bounded Assignment-specific keys.
    /// This census covers retired roles as long as their accounts are retained.
    fn assignment_role_accounts_v2(
        &self,
        row: &AssignmentRow,
        max_roles: u32,
        max_joins: u32,
    ) -> Result<(u32, u32), PalwStateV2Error> {
        let id = row.id();
        let rows = PositiveRowsV2::new(&self.state);
        let route = self
            .state
            .kernel_route
            .as_ref()
            .ok_or_else(missing_assignment_history)?;
        let mut role_start = NativeRowKey::AssignmentRole {
            assignment: id,
            role: AssignmentRoleV2::Poster,
            bond: row.bundle.terms.poster,
        }
        .encode();
        role_start.truncate(role_start.len() - (1 + 64 + 4));
        let role_start = (KERNEL_AUX_TABLE, role_start);
        let mut initial = 0u8;
        let mut roles = 0u32;
        let mut joins = 0u32;
        for ((table, key), raw) in route.aux.range(role_start.clone()..) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&role_start.1) {
                break;
            }
            roles = roles
                .checked_add(1)
                .ok_or_else(|| refusal("Assignment role count overflow"))?;
            if roles > max_roles {
                return Err(refusal("all-role close scan exceeds original role cap"));
            }
            let NativeRowBody::AssignmentRole(role) =
                NativeRowValue::decode(key, raw).map_err(refusal)?.body
            else {
                return Err(refusal("wrong Assignment role row kind"));
            };
            role.authenticate(row).map_err(refusal)?;
            match role.authorization {
                AssignmentRoleAuthorizationV2::Initial => initial |= 1 << (role.role as u8),
                AssignmentRoleAuthorizationV2::Joined(join) => {
                    joins = joins
                        .checked_add(1)
                        .ok_or_else(|| refusal("Assignment join count overflow"))?;
                    if joins > max_joins
                        || join.case.is_some_and(|(case, result)| {
                            row.status != (AssignmentStatusV2::Consumed { case, result })
                        })
                        || join.cancellation_policy != row.bundle.cancellation_policy
                    {
                        return Err(refusal(
                            "joined role does not meet original Assignment scope",
                        ));
                    }
                    let funding = rows
                        .read(NativeRowKey::AssignmentProverFunding {
                            assignment: id,
                            prover: role.bond,
                        })
                        .map_err(refusal)?
                        .ok_or_else(missing_assignment_history)?;
                    let NativeRowBody::AssignmentProverFunding(funds) = funding.body else {
                        return Err(refusal("wrong joined funding row kind"));
                    };
                    funds.validate_shape().map_err(refusal)?;
                    if funds.consent != join
                        || funds.payer_key != role.key
                        || funds.payer_signature != role.signature
                        || funds.funding_origin != role.origin
                        || funds.reservation != role.lease
                        || role.funding.is_none_or(|r| {
                            r.digest != join.id() || r.origin != funds.funding_origin
                        })
                    {
                        return Err(refusal("joined role and outstanding funding differ"));
                    }
                }
            }
        }
        if initial != 0b111 {
            return Err(missing_assignment_history());
        }
        // Each matched joined role names its own unique funding key. The
        // reverse count catches any orphan account without another decode.
        let mut funding_start = NativeRowKey::AssignmentProverFunding {
            assignment: id,
            prover: row.bundle.terms.prover,
        }
        .encode();
        funding_start.truncate(funding_start.len() - (64 + 4));
        let funding_start = (KERNEL_AUX_TABLE, funding_start);
        let funding_count = route
            .aux
            .range(funding_start.clone()..)
            .take_while(|((table, key), _)| {
                *table == KERNEL_AUX_TABLE && key.starts_with(&funding_start.1)
            })
            .take(joins as usize + 1)
            .count();
        if funding_count != joins as usize {
            return Err(refusal("unmatched joined Assignment funding account"));
        }
        Ok((roles, joins))
    }

    fn check_assignment_prover_join_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        consent: &misaka_palw_positive_contract::assignment::role_actions::AssignmentProverJoinV2,
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), PalwStateV2Error> {
        use misaka_palw_positive_contract::messages::ClaimStatus;

        let id = consent.assignment;
        let row = self
            .assignment_row_v2(id)?
            .ok_or_else(|| refusal("missing accepted Assignment"))?;
        let rules = action.rules();
        if rules.original_material() != Some(row.admission_material) {
            return Err(missing_assignment_history());
        }
        match row.status {
            AssignmentStatusV2::Open
                if consent.case.is_none() && ctx.daa_score < row.bundle.terms.result_before_daa => {
            }
            AssignmentStatusV2::Consumed { case, result }
                if consent.case == Some((case, result)) =>
            {
                let rooted = PositiveRowsV2::new(&self.state)
                    .read(NativeRowKey::Case(case))
                    .map_err(refusal)?
                    .ok_or_else(missing_assignment_history)?;
                let NativeRowBody::Case(rooted) = rooted.body else {
                    return Err(refusal("wrong assigned Case row kind"));
                };
                if rooted.result != result
                    || !matches!(
                        rooted.status,
                        ClaimStatus::ResultCommitted | ClaimStatus::Pending
                    )
                    || ctx.daa_score >= rooted.service.deadline
                {
                    return Err(refusal(
                        "assigned Case no longer accepts additional prover work",
                    ));
                }
            }
            _ => {
                return Err(refusal(
                    "join does not match the current open Assignment or active Case",
                ));
            }
        }
        consent.check(&row.bundle, ctx.daa_score).map_err(refusal)?;
        if consent.max_attempts > row.prover_consent.max_attempts {
            return Err(refusal(
                "joined role exceeds the original shared attempt cap",
            ));
        }
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentRole {
            assignment: id,
            role: AssignmentRoleV2::Prover,
            bond: consent.prover,
        })?;
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentProverFunding {
            assignment: id,
            prover: consent.prover,
        })?;
        let policy = rules.assignment_policy();
        let head = self.assignment_head_v2(id)?;
        self.check_assignment_next_journal_extent_v2(
            &row,
            &head,
            policy,
            rules
                .original_history_storage_limit_v2()
                .ok_or_else(missing_assignment_history)?,
            action.transaction().payload[3..].len(),
        )?;
        let (roles, joins) = self.assignment_role_accounts_v2(
            &row,
            policy.max_role_candidates,
            policy.max_prover_joins,
        )?;
        if roles >= policy.max_role_candidates || joins >= policy.max_prover_joins {
            return Err(refusal("original Assignment role or join cap exhausted"));
        }
        self.check_assignment_live_slots_v2(consent.prover, None, rules)?;
        self.check_assignment_service_capacity_v2(
            ctx,
            rules,
            assignment_join_service_demand_v2(consent),
        )?;

        let bond = self
            .state
            .bond(&native_bond(consent.prover))
            .ok_or_else(|| refusal("missing current prover bond"))?;
        if !palw_bond_may_take_work_v2(bond, self.params.min_collateral_sompi) {
            return Err(refusal("current prover cannot accept work"));
        }
        let key: &[u8; PUBKEY_BYTES] = bond.pubkey.as_slice().try_into().map_err(refusal)?;
        consent
            .authenticate(&row.bundle, key, signature, ctx.daa_score)
            .map_err(refusal)?;
        let (outpoint, amount) =
            crate::palw_positive_assignment_funds_v2::check_assignment_prover_funding_carriage_v2(
                &row.bundle,
                consent,
                key,
                action.transaction(),
                action.current_output(),
                ctx.daa_score,
            )
            .map_err(refusal)?;
        if amount == 0 || amount > consent.max_locked {
            return Err(refusal(
                "joined funding amount exceeds the signed nonzero envelope",
            ));
        }
        self.require_assignment_output_unlocked_v2(outpoint)
    }

    /// A native close only stops new work. A later independent settlement
    /// must derive all eligible dues, current coins and remaining liabilities.
    fn close_positive_assignment_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        operation: &crate::palw_positive_court_v2::coordinator::assignment_body::PositiveAssignmentNativeOperationV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_positive_court_v2::coordinator::assignment_body::PositiveAssignmentNativeOperationV2 as O;

        let (id, kind, status) = match operation {
            O::CancelAssignment { cancellation, .. } => (
                cancellation.assignment,
                AssignmentJournalEventKindV2::Cancel,
                AssignmentStatusV2::Cancelled,
            ),
            O::ExpireAssignment { assignment } => (
                *assignment,
                AssignmentJournalEventKindV2::Expire,
                AssignmentStatusV2::ExpiredUnconsumed,
            ),
            _ => return Err(refusal("wrong Assignment close operation")),
        };
        let rooted = PositiveRowsV2::new(&self.state)
            .assignment(id)
            .map_err(refusal)?;
        let mut row = rooted.assignment;
        if row.status != AssignmentStatusV2::Open {
            return Err(refusal("only the current unconsumed Assignment can close"));
        }
        let rules = action.rules();
        if rules.original_material() != Some(row.admission_material) {
            return Err(missing_assignment_history());
        }
        let storage_limit = rules
            .original_history_storage_limit_v2()
            .ok_or_else(missing_assignment_history)?;
        let policy = rules.assignment_policy();
        let head = rooted.journal;
        let literal = &action.transaction().payload[3..];
        let origin = action.origin();
        self.assignment_origin_v2(origin, ctx)?;
        self.check_assignment_next_journal_extent_v2(
            &row,
            &head,
            policy,
            storage_limit,
            literal.len(),
        )?;
        match operation {
            O::CancelAssignment {
                cancellation,
                poster_signature,
                worker_signature,
            } => {
                let native = rules
                    .native_preimages()
                    .ok_or_else(missing_assignment_history)?;
                native.cancellation.validate().map_err(refusal)?;
                if native.cancellation.id() != row.bundle.cancellation_policy {
                    return Err(refusal("actual original cancellation policy differs"));
                }
                let poster = self
                    .state
                    .bond(&native_bond(row.bundle.terms.poster))
                    .ok_or_else(|| refusal("missing registered poster"))?;
                let worker = self
                    .state
                    .bond(&native_bond(row.bundle.terms.worker))
                    .ok_or_else(|| refusal("missing registered worker"))?;
                if poster.pubkey.as_slice() != rooted.funding.payer_key
                    || worker.pubkey.as_slice() != row.worker_key
                {
                    return Err(refusal(
                        "cancellation role keys differ from original acceptance",
                    ));
                }
                cancellation
                    .authenticate_poster(
                        &row.bundle,
                        &rooted.funding.payer_key,
                        poster_signature,
                        ctx.daa_score,
                    )
                    .map_err(refusal)?;
                cancellation
                    .authenticate_worker(
                        &row.bundle,
                        &row.worker_key,
                        worker_signature,
                        ctx.daa_score,
                    )
                    .map_err(refusal)?;
            }
            O::ExpireAssignment { .. } if ctx.daa_score < row.bundle.terms.result_before_daa => {
                return Err(refusal("expiry needs the original result cutoff"));
            }
            _ => {}
        }
        self.assignment_role_accounts_v2(
            &row,
            policy.max_role_candidates,
            policy.max_prover_joins,
        )?;
        // The enclosing raw checkpoint rolls back both rows and retained bytes.
        let payload =
            self.encode_assignment_control_payload_v2(id, literal, policy.max_frame_bytes)?;
        let frame = assignment_frame_v2(
            id,
            head.next_ordinal,
            head.last_frame,
            kind,
            origin,
            &payload,
            policy.max_frame_bytes,
        )
        .map_err(refusal)?;
        row.status = status;
        row.last_event = origin;
        self.write_assignment_aux_v2(NativeRowBody::Assignment(row))?;
        self.append_assignment_frame_v2(frame, head.retention_until_daa)?;
        self.emit_assignment_material_v2(&frame, &payload, policy.max_frame_bytes)
    }
}

/// Replays raw inputs at the same accepted-object boundary in the final fold.
/// Semantic refusals affect only the Assignment; local capacity cannot be
/// swallowed as though it were a peer-invalid operation.
pub(super) fn apply_raw_before_object_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    index: usize,
    before_ordinal: Option<usize>,
    cursor: &mut usize,
    previous_ordinal: &mut Option<u32>,
) -> Result<(), PalwStateV2Error> {
    while let Some(scheduled) = builder.extras.positive_assignment_actions.get(*cursor) {
        let slot = scheduled.before_object as usize;
        if slot > index {
            break;
        }
        if slot < index {
            return Err(refusal("raw action schedule moved backward"));
        }
        let origin = scheduled.action.origin();
        if let Some(limit) = before_ordinal {
            let ordinal = origin.accepted_tx_ordinal as usize;
            if ordinal == limit {
                return Err(refusal("Batch and Assignment share an accepted transaction ordinal"));
            }
            if ordinal > limit {
                break;
            }
        }
        if previous_ordinal.is_some_and(|previous| origin.accepted_tx_ordinal <= previous) {
            return Err(refusal(
                "raw native accepted ordinals are not strictly increasing",
            ));
        }
        // Arc clone retains the actual immutable transport allocation. It is
        // not a decoded/saved admission capability and cannot bypass this hook.
        let action = scheduled.action.clone();
        match builder.apply_positive_assignment_raw_v2(ctx, &action) {
            Ok(()) => {}
            Err(
                local @ (PalwStateV2Error::NativeResource(_)
                | PalwStateV2Error::NativeContextUnavailable(_)),
            ) => return Err(local),
            Err(why)
                if action.transaction().payload.get(7) == Some(&12)
                    && action.case_backing().is_ok() =>
            {
                return Err(why);
            }
            Err(_semantic) => {}
        }
        *previous_ordinal = Some(origin.accepted_tx_ordinal);
        *cursor += 1;
    }
    Ok(())
}
impl TransitionBuilder<'_> {
    fn read_relay_occupancy_row_v2(
        &self,
        key: NativeRowKey,
        budget: &mut FundingRelayAuxScanBudgetV2,
    ) -> Result<Option<NativeRowBody>, PalwStateV2Error> {
        let route = self
            .state
            .kernel_route
            .as_ref()
            .ok_or_else(missing_assignment_history)?;
        let key = (KERNEL_AUX_TABLE, key.encode());
        let value = route.aux.get(&key);
        budget
            .inspect(key.1.len(), value.map_or(0, Vec::len))
            .map_err(refusal)?;
        value
            .map(|bytes| {
                NativeRowValue::decode(&key.1, bytes)
                    .map(|row| row.body)
                    .map_err(|_| missing_assignment_history())
            })
            .transpose()
    }

    /// An existing relay owns the whole outpoint across all purposes. An
    /// exact pending retry is a read-only outcome of this current prefix.
    #[inline(never)]
    fn check_relay_outpoint_v2(
        &self,
        body: &FundingRelayRegistrationBodyV2,
        action: &PositiveAssignmentRawActionV2,
        budget: &mut FundingRelayAuxScanBudgetV2,
    ) -> Result<bool, PalwStateV2Error> {
        use RelayFundingPurposeV2::{CaseC, PosterA, ProverJoin};

        let funding = body.terms.funding_outpoint;
        let mut existing = None;
        for purpose in [PosterA, CaseC, ProverJoin] {
            let auth = self.read_relay_occupancy_row_v2(
                NativeRowKey::FundingRelayAuthorization { funding, purpose },
                budget,
            )?;
            let custody = self.read_relay_occupancy_row_v2(
                NativeRowKey::FundingRelayCustody { funding, purpose },
                budget,
            )?;
            match (auth, custody) {
                (None, None) => {}
                (Some(auth), Some(custody)) if existing.is_none() => {
                    let (
                        NativeRowBody::FundingRelayAuthorization(auth),
                        NativeRowBody::FundingRelayCustody(custody),
                    ) = (auth, custody)
                    else {
                        return Err(missing_assignment_history());
                    };
                    custody
                        .check_pair(&auth)
                        .map_err(|_| missing_assignment_history())?;
                    let origin = action.origin();
                    let result = if auth.terms != body.terms
                        || auth.purpose_signature != body.purpose_signature
                        || custody.relay_signature != body.relay_signature
                    {
                        Err(refusal(
                            "relay funding outpoint already has a different signed package",
                        ))
                    } else if custody.binding.is_some()
                        || origin.accepted_daa >= custody.retention_until_daa
                    {
                        Err(refusal(
                            "relay retry does not name a live pending custody slot",
                        ))
                    } else if auth.origin.accepting_block == origin.accepting_block {
                        // The same block's canonical prefix already wrote
                        // this pair. Its accepted debit will be reconciled
                        // with that write in the enclosing commit.
                        if auth.origin.accepted_daa != origin.accepted_daa
                            || auth.origin.accepted_blue_score != origin.accepted_blue_score
                            || auth.origin.event_ordinal != 0
                            || auth.origin.accepted_tx_ordinal >= origin.accepted_tx_ordinal
                        {
                            Err(missing_assignment_history())
                        } else {
                            Ok(true)
                        }
                    } else if action.pending_relay_replay_v2().is_some_and(|archive| {
                        archive.reservation == custody.reservation
                            && archive.physical == auth.physical
                            && auth.carrier_charge_material(archive.fees).ok()
                                == Some(auth.metadata_charge)
                    }) {
                        Ok(true)
                    } else {
                        Err(missing_assignment_history())
                    };
                    existing = Some(result);
                }
                _ => return Err(missing_assignment_history()),
            }
        }
        match existing {
            Some(outcome) => outcome,
            None if action.pending_relay_replay_v2().is_some() => {
                Err(refusal("original pending relay changed before the retry"))
            }
            None => Ok(false),
        }
    }

    /// Count current canonical rows. Until the retirement owner removes a
    /// pending pair, even a passed registration or custody clock occupies it.
    #[inline(never)]
    fn check_initial_relay_slots_v2(
        &self,
        terms: &FundingRelayTermsV2,
        policy: &FundingRelayPolicyV2,
        budget: &mut FundingRelayAuxScanBudgetV2,
    ) -> Result<(), PalwStateV2Error> {
        let route = self
            .state
            .kernel_route
            .as_ref()
            .ok_or_else(missing_assignment_history)?;
        let funding = terms.funding_outpoint;
        let purpose = terms.funding_purpose;
        let kind_range = |key: NativeRowKey| {
            let mut first = key.encode();
            first.truncate(first.len() - (64 + 4 + 1)); // funding outpoint and purpose
            let mut end = first.clone();
            // These fixed kinds are 19 and 20, with a one-byte kind tag.
            *end.last_mut().expect("relay key kind") += 1;
            (KERNEL_AUX_TABLE, first)..(KERNEL_AUX_TABLE, end)
        };
        let auth_range = kind_range(NativeRowKey::FundingRelayAuthorization { funding, purpose });
        let custody_range = kind_range(NativeRowKey::FundingRelayCustody { funding, purpose });
        // Two rows for every registration, plus two for this new package.
        let available_pairs = policy.max_stored_rows as usize / 2 - 1;
        let mut total = 0;
        let mut pending = 0;
        let mut poster_pending = 0;
        let mut account_pending = 0;
        let mut account_continuations = 0;
        for ((_, key), bytes) in route.aux.range(auth_range) {
            budget.inspect(key.len(), bytes.len()).map_err(refusal)?;
            if total == available_pairs {
                return Err(refusal("relay stored row cap exhausted"));
            }
            total += 1;
            let NativeRowBody::FundingRelayAuthorization(auth) = NativeRowValue::decode(key, bytes)
                .map_err(|_| missing_assignment_history())?
                .body
            else {
                return Err(missing_assignment_history());
            };
            let Some(NativeRowBody::FundingRelayCustody(custody)) = self
                .read_relay_occupancy_row_v2(
                    NativeRowKey::FundingRelayCustody {
                        funding: auth.terms.funding_outpoint,
                        purpose: auth.terms.funding_purpose,
                    },
                    budget,
                )?
            else {
                return Err(missing_assignment_history());
            };
            if custody.material != auth.material {
                return Err(missing_assignment_history());
            }
            let same_case_continuation = match (terms.target, auth.terms.target) {
                (
                    RelayTargetV2::Continuation {
                        original_funding,
                        original_contract,
                        ..
                    },
                    RelayTargetV2::Continuation {
                        original_funding: existing,
                        original_contract: contract,
                        ..
                    },
                ) => {
                    terms.funding_purpose == RelayFundingPurposeV2::CaseC
                        && auth.terms.funding_purpose == terms.funding_purpose
                        && auth.terms.assignment == terms.assignment
                        && original_funding == existing
                        && original_contract == contract
                }
                _ => false,
            };
            if same_case_continuation {
                account_continuations += 1;
                if account_continuations >= policy.max_account_continuations {
                    return Err(refusal(
                        "Case relay continuation registration cap exhausted",
                    ));
                }
            }
            if custody.binding.is_none() {
                pending += 1;
                if auth.terms.payer_bond == terms.payer_bond
                    && auth.terms.funding_purpose != RelayFundingPurposeV2::ProverJoin
                {
                    poster_pending += 1;
                    let same_account = match (terms.target, auth.terms.target) {
                        (
                            RelayTargetV2::InitialAssignment { poster_nonce, .. },
                            RelayTargetV2::InitialAssignment {
                                poster_nonce: existing,
                                ..
                            },
                        ) => poster_nonce == existing,
                        (RelayTargetV2::InitialCase { .. }, RelayTargetV2::InitialCase { .. }) => {
                            terms.assignment == auth.terms.assignment
                        }
                        _ => same_case_continuation,
                    };
                    if same_account {
                        account_pending += 1;
                    }
                }
                if pending >= policy.max_pending_global
                    || poster_pending >= policy.max_pending_per_poster
                    || account_pending >= policy.max_account_candidates
                {
                    return Err(refusal("relay pending registration cap exhausted"));
                }
            }
        }
        let mut custody_count = 0;
        for ((_, key), _) in route.aux.range(custody_range).take(total + 1) {
            budget.inspect(key.len(), 0).map_err(refusal)?;
            custody_count += 1;
        }
        if custody_count != total {
            return Err(missing_assignment_history());
        }
        Ok(())
    }

    #[inline(never)]
    /// Recheck the same rooted registration at the current F checkpoint,
    /// after qualifying F's actual output and current native payer key.
    fn check_pending_initial_f_relay_v2(
        &self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        purpose: RelayFundingPurposeV2,
        funding: NativeOutpoint,
        payer_key: &[u8; PUBKEY_BYTES],
        check_terms: &dyn Fn(
            &FundingRelayTermsV2,
        ) -> Result<(), misaka_palw_positive_contract::ContractError>,
    ) -> Result<CheckedInitialFundingRelayV2, PalwStateV2Error> {
        use crate::{
            palw_native_resource_v1::{
                NativeCoverageV1, TirNativeRequestsV1, reserve_native_requests_v1,
            },
            palw_positive_funding_relay_v2::{
                FundingRelayBodyErrorV2, verify_funding_relay_package_signatures_v2,
            },
        };
        use misaka_palw_positive_contract::{
            CONTRACT_REVISION, CONTRACT_VERSION,
            assignment::funding_relay::FUNDING_RELAY_OPERATION_V2,
        };
        let replay = action
            .pending_relay_replay_v2()
            .ok_or_else(|| refusal("missing matching relay in the selected parent"))?;
        if replay.selected_parent != action.selected_parent() {
            return Err(missing_assignment_history());
        }
        let mut requests = TirNativeRequestsV1::new();
        requests
            .exact_v1::<NativeRowValue>(4)
            .map_err(PalwStateV2Error::NativeResource)?;
        requests
            .exact_v1::<FundingRelayRegistrationBodyV2>(1)
            .map_err(PalwStateV2Error::NativeResource)?;
        requests
            .exact_v1::<u8>(2 * 1024 + 2 * 74)
            .map_err(PalwStateV2Error::NativeResource)?;
        let workspace = requests.bytes_v1();
        let lease = reserve_native_requests_v1(workspace)
            .map_err(PalwStateV2Error::NativeResource)?
            .ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<NativeRowValue>(
                    "initial funding relay replay",
                    "native owner is not armed",
                ))
            })?;
        let mut budget = FundingRelayAuxScanBudgetV2::new(&replay.physical).map_err(refusal)?;
        let Some(NativeRowBody::FundingRelayAuthorization(auth)) = self
            .read_relay_occupancy_row_v2(
                NativeRowKey::FundingRelayAuthorization { funding, purpose },
                &mut budget,
            )?
        else {
            return Err(refusal(
                "parent relay authorization is unavailable at this prefix",
            ));
        };
        let Some(NativeRowBody::FundingRelayCustody(custody)) = self.read_relay_occupancy_row_v2(
            NativeRowKey::FundingRelayCustody { funding, purpose },
            &mut budget,
        )?
        else {
            return Err(missing_assignment_history());
        };
        custody
            .check_pair(&auth)
            .map_err(|_| missing_assignment_history())?;
        if replay.reservation != custody.reservation
            || replay.physical != auth.physical
            || auth.carrier_charge_material(replay.fees).ok() != Some(auth.metadata_charge)
        {
            return Err(missing_assignment_history());
        }
        if custody.binding.is_some()
            || ctx.daa_score >= custody.retention_until_daa
            || auth.payer_key != *payer_key
        {
            return Err(refusal(
                "initial funding relay is bound, expired, or has a different payer",
            ));
        }
        check_terms(&auth.terms).map_err(refusal)?;
        let body = FundingRelayRegistrationBodyV2 {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            operation: FUNDING_RELAY_OPERATION_V2,
            terms: auth.terms,
            purpose_signature: auth.purpose_signature,
            relay_signature: custody.relay_signature,
        };
        let signature_workspace = auth
            .quote
            .usage
            .heap_bytes
            .checked_sub(workspace)
            .ok_or_else(missing_assignment_history)?;
        verify_funding_relay_package_signatures_v2(&body, payer_key, signature_workspace).map_err(
            |why| match why {
                FundingRelayBodyErrorV2::Contract(_) => missing_assignment_history(),
                FundingRelayBodyErrorV2::Native(why) => PalwStateV2Error::NativeResource(why),
            },
        )?;
        // Preserve the rollback tail from the original authenticated R
        // policy. It cannot also be sold as Assignment service time.
        let service_cutoff = custody
            .retention_until_daa
            .checked_sub(auth.policy.rollback_margin_daa)
            .ok_or_else(missing_assignment_history)?;
        Ok(CheckedInitialFundingRelayV2 {
            custody,
            service_cutoff,
            _lease: lease,
        })
    }

    fn check_initial_assignment_relay_v2(
        &self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        bundle: &misaka_palw_positive_contract::assignment::AssignmentAdmissionBundleV2,
        (funding, amount): (NativeOutpoint, u64),
        payer_key: &[u8; PUBKEY_BYTES],
    ) -> Result<Option<CheckedInitialFundingRelayV2>, PalwStateV2Error> {
        if !action.rules().funding_relay_profile().requires_relay() {
            return Ok(None);
        }
        let checked = self.check_pending_initial_f_relay_v2(
            ctx,
            action,
            RelayFundingPurposeV2::PosterA,
            funding,
            payer_key,
            &|terms| {
                crate::palw_positive_assignment_funds_v2::check_initial_assignment_relay_target_v2(
                    terms,
                    bundle,
                    funding,
                    amount,
                    ctx.daa_score,
                )
            },
        )?;
        if checked.service_cutoff < bundle.terms.reserve_until_daa {
            return Err(refusal(
                "initial Assignment relay does not cover the signed reservation",
            ));
        }
        Ok(Some(checked))
    }

    fn same_block_case_relay_material_v2(
        &self,
        assignment: &AssignmentRow,
        budget: &mut FundingRelayAuxScanBudgetV2,
        remaining_heap: u64,
    ) -> Result<Option<misaka_palw_positive_contract::rows::AcceptedMaterial>, PalwStateV2Error>
    {
        use crate::palw_native_resource_v1::{NativeCoverageV1, reserve_native_requests_v1};
        use misaka_palw_positive_contract::assignment::{
            control_material::AssignmentControlViewV2,
            journal::{AssignmentJournalHeaderV2, AssignmentJournalRecordViewV2},
        };
        let scratch = (2 * size_of::<AssignmentAdmissionMaterialV2>()
            + 2 * size_of::<AssignmentJournalHeaderV2>()) as u64;
        if remaining_heap < scratch {
            return Err(refusal(
                "same-block Case relay profile exceeds registration heap",
            ));
        }
        let _lease = reserve_native_requests_v1(scratch)
            .map_err(PalwStateV2Error::NativeResource)?
            .ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<
                    AssignmentAdmissionMaterialV2,
                >(
                    "same-block Case relay profile",
                    "native owner is not armed",
                ))
            })?;
        for accepted in &self.assignment_materials {
            let header = accepted.header();
            budget
                .inspect(size_of::<AssignmentJournalHeaderV2>(), 0)
                .map_err(refusal)?;
            if header.kind != AssignmentJournalEventKindV2::Open
                || header.origin != assignment.origin
                || header.assignment != assignment.id()
            {
                continue;
            }
            budget
                .inspect(
                    0,
                    accepted
                        .raw()
                        .len()
                        .checked_mul(8)
                        .ok_or_else(|| refusal("same-block Case relay profile work overflow"))?,
                )
                .map_err(refusal)?;
            let record =
                AssignmentJournalRecordViewV2::decode(accepted.raw(), accepted.max_record_bytes())
                    .map_err(refusal)?;
            let max_payload = u64::from(header.payload_bytes);
            let envelope =
                AssignmentControlViewV2::decode(record.payload(), max_payload).map_err(refusal)?;
            let (_, body) =
                AssignmentControlBoundaryV2::native_body_from_record_data_v2(&record, max_payload)
                    .map_err(refusal)?;
            let view = AssignmentAdmissionMaterialViewV2::decode(
                assignment,
                envelope.inner(),
                body,
                AssignmentOpenLimitsV2::within_payload(max_payload),
            )
            .map_err(refusal)?;
            view.material
                .check_native_open_record_extent_v2(max_payload)
                .map_err(refusal)?;
            return Ok(view
                .material
                .funding_relay
                .requires_relay()
                .then_some(assignment.admission_material));
        }
        Err(missing_assignment_history())
    }

    #[inline(never)]
    pub(super) fn install_initial_assignment_relay_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        stage: InitialRelayFeeStageV2,
    ) -> Result<FundingRelayCarrierFeesV2, PalwStateV2Error> {
        use crate::{
            config::params::AssignmentFundingRelayProfileV2,
            palw_native_resource_v1::{
                NativeCoverageV1, TirNativeRequestsV1, reserve_native_requests_v1,
            },
            palw_positive_funding_relay_v2::{
                FundingRelayBodyErrorV2, ReservedFundingRelayBodyV2,
                verify_funding_relay_package_signatures_v2,
            },
        };

        let rules = action.rules();
        let AssignmentFundingRelayProfileV2::RequiresRelayV2 {
            ruleset,
            policy,
            fees,
            physical,
            ..
        } = rules.funding_relay_profile()
        else {
            return Err(refusal("initial relay fence is dormant"));
        };
        let replay = action.pending_relay_replay_v2();
        let decode_profile = replay
            .as_ref()
            .map_or(physical, |original| &original.physical);
        let reserved = ReservedFundingRelayBodyV2::decode(
            &action.transaction().payload[3..],
            decode_profile.max_frame_bytes,
            decode_profile.max_heap_bytes,
        )
        .map_err(|error| match error {
            FundingRelayBodyErrorV2::Contract(why) => refusal(why),
            FundingRelayBodyErrorV2::Native(why) => PalwStateV2Error::NativeResource(why),
        })?;
        let body = reserved.body();
        let terms = &body.terms;
        let observed = action
            .relay_carrier_fees_v2()
            .ok_or_else(missing_assignment_history)?;
        let fee_input = match stage {
            InitialRelayFeeStageV2::Preview => observed.before_registration(),
            InitialRelayFeeStageV2::Observed => observed,
        };
        let case_continuation = terms.funding_purpose == RelayFundingPurposeV2::CaseC
            && matches!(terms.target, RelayTargetV2::Continuation { .. });
        if !matches!(
            terms.target,
            RelayTargetV2::InitialAssignment { .. } | RelayTargetV2::InitialCase { .. }
        ) && !case_continuation
        {
            return Err(refusal("relay target needs its original account owner"));
        }
        if reserved.quote().decoded_inline_bytes > decode_profile.max_decoded_bytes {
            return Err(refusal("initial relay exceeds the decoded size cap"));
        }
        let (registration, expected_case_key) = {
            let mut expected_case_key = None;
            let mut requests = TirNativeRequestsV1::new();
            // Two decoded rows plus decoder scratch and the reconstructed
            // literal used to check an existing package's stored crosslinks.
            requests
                .exact_v1::<NativeRowValue>(4)
                .map_err(PalwStateV2Error::NativeResource)?;
            requests
                .exact_v1::<FundingRelayRegistrationBodyV2>(1)
                .map_err(PalwStateV2Error::NativeResource)?;
            // Two Borsh key buffers, two exclusive range endpoints and
            // exact lookup/decode key workspace.
            requests
                .exact_v1::<u8>(3 * 1024 + 4 * 74)
                .map_err(PalwStateV2Error::NativeResource)?;
            if case_continuation {
                requests
                    .exact_v1::<NativeRowValue>(2)
                    .map_err(PalwStateV2Error::NativeResource)?;
                requests
                    .exact_v1::<u8>(2 * 74)
                    .map_err(PalwStateV2Error::NativeResource)?;
            }
            let workspace = requests.bytes_v1();
            if reserved
                .quote()
                .logical_decode_requests
                .checked_add(workspace)
                .is_none_or(|n| n > decode_profile.max_heap_bytes)
            {
                return Err(refusal(
                    "initial relay rooted checks exceed the native heap cap",
                ));
            }
            let _lease = reserve_native_requests_v1(workspace)
                .map_err(PalwStateV2Error::NativeResource)?
                .ok_or_else(|| {
                    PalwStateV2Error::NativeResource(
                        NativeCoverageV1::unsupported::<NativeRowValue>(
                            "initial relay rooted checks",
                            "native owner is not armed",
                        ),
                    )
                })?;
            let mut budget = FundingRelayAuxScanBudgetV2::new(decode_profile).map_err(refusal)?;
            if self.check_relay_outpoint_v2(body, action, &mut budget)? {
                if fee_input.relay_charge() != 0 {
                    return Err(refusal(
                        "an exact relay retry cannot charge a second registration fee",
                    ));
                }
                return Ok(fee_input);
            }
            let (anchor, _) = action
                .signed_anchor_clock_v2()
                .ok_or_else(missing_assignment_history)?;
            if terms.contract_network != rules.network()
                || terms.relay_ruleset != *ruleset
                || terms.contract_anchor.0 != anchor.as_bytes()
                || terms.expected_amount > crate::constants::MAX_SOMPI
            {
                return Err(refusal(
                    "initial relay differs from the current contract or native relay scope",
                ));
            }
            match terms.target {
                RelayTargetV2::InitialAssignment { .. } => {
                    if terms.contract_ruleset.0 != rules.rule_scope().ruleset().as_bytes() {
                        return Err(refusal(
                            "initial Assignment relay differs from the current Assignment scope",
                        ));
                    }
                }
                RelayTargetV2::InitialCase { .. } | RelayTargetV2::Continuation { .. } => {
                    let Some(NativeRowBody::Assignment(assignment)) = self
                        .read_relay_occupancy_row_v2(
                            NativeRowKey::Assignment(terms.assignment),
                            &mut budget,
                        )?
                    else {
                        return Err(refusal(
                            "initial Case relay requires an accepted Assignment",
                        ));
                    };
                    let Some(NativeRowBody::AssignmentJournalFrame(open)) = self
                        .read_relay_occupancy_row_v2(
                            NativeRowKey::AssignmentJournalFrame {
                                assignment: terms.assignment,
                                ordinal: 0,
                            },
                            &mut budget,
                        )?
                    else {
                        return Err(missing_assignment_history());
                    };
                    if open.header.kind != AssignmentJournalEventKindV2::Open
                        || open.header.origin != assignment.origin
                    {
                        return Err(missing_assignment_history());
                    }
                    crate::palw_positive_funding_relay_v2::check_case_relay_minimum_replay_v2(
                        terms,
                        physical,
                        open.header.payload_bytes,
                    )
                    .map_err(refusal)?;
                    let material =
                        if assignment.origin.accepting_block == action.origin().accepting_block {
                            let remaining_heap = decode_profile.max_heap_bytes
                                - workspace
                                - reserved.quote().logical_decode_requests;
                            self.same_block_case_relay_material_v2(
                                &assignment,
                                &mut budget,
                                remaining_heap,
                            )?
                        } else {
                            action.case_relay_contract_v2().filter(|(id, _)| *id == assignment.id()).map(|(_, material)| material)
                        };
                    if material != Some(assignment.admission_material) {
                        return Err(refusal(
                            "initial Case relay requires the original relay-enabled Assignment material",
                        ));
                    }
                    if case_continuation {
                        let AssignmentStatusV2::Consumed { case, .. } = assignment.status else {
                            return Err(refusal(
                                "Case successor relay requires a consumed Assignment",
                            ));
                        };
                        let Some(NativeRowBody::Funding(account)) = self
                            .read_relay_occupancy_row_v2(
                                NativeRowKey::Funding(case),
                                &mut budget,
                            )?
                        else {
                            return Err(refusal(
                                "Case successor relay requires a current funding account",
                            ));
                        };
                        crate::palw_positive_assignment_funds_v2::check_case_relay_continuation_account_v2(
                            terms,
                            &assignment,
                            &account,
                            action.origin(),
                        )
                        .map_err(refusal)?;
                        let previous = account
                            .current_outpoint
                            .ok_or_else(missing_assignment_history)?;
                        let purpose = RelayFundingPurposeV2::CaseC;
                        let Some(NativeRowBody::FundingRelayAuthorization(previous_auth)) = self
                            .read_relay_occupancy_row_v2(
                                NativeRowKey::FundingRelayAuthorization {
                                    funding: previous,
                                    purpose,
                                },
                                &mut budget,
                            )?
                        else {
                            return Err(missing_assignment_history());
                        };
                        let Some(NativeRowBody::FundingRelayCustody(previous_custody)) = self
                            .read_relay_occupancy_row_v2(
                                NativeRowKey::FundingRelayCustody {
                                    funding: previous,
                                    purpose,
                                },
                                &mut budget,
                            )?
                        else {
                            return Err(missing_assignment_history());
                        };
                        previous_custody
                            .check_pair(&previous_auth)
                            .map_err(refusal)?;
                        let current_origin = account
                            .last_settlement
                            .map_or(account.funding_origin, |material| material.origin);
                        if previous_auth.payer_key != account.payer_key
                            || previous_auth.terms.assignment != assignment.id()
                            || previous_custody.binding.is_none_or(|binding| {
                                binding.origin != current_origin
                                    || binding.original_funding != account.original_outpoint
                                    || binding.original_contract != account.consent.id().0
                            })
                        {
                            return Err(refusal(
                                "Case successor relay requires the current bound package",
                            ));
                        }
                        expected_case_key = Some(account.payer_key);
                    } else {
                        crate::palw_positive_assignment_funds_v2::check_case_relay_registration_assignment_v2(
                            terms,
                            &assignment,
                            action.origin(),
                        )
                        .map_err(refusal)?;
                    }
                }
                _ => unreachable!("checked initial relay target"),
            }
            let minimum_replay =
                crate::palw_positive_funding_relay_v2::minimum_pending_funding_relay_replay_heap_v2(terms).map_err(refusal)?;
            if physical.max_heap_bytes < minimum_replay {
                return Err(refusal(
                    "initial relay profile cannot replay its own registration delta",
                ));
            }
            let registration = fees
                .registration_quote(policy, physical, terms, ctx.daa_score)
                .map_err(refusal)?;
            if registration.usage.frame_bytes != reserved.quote().encoded_bytes {
                return Err(refusal(
                    "initial relay frame differs from the registration quote",
                ));
            }
            if registration.fee > fee_input.before_relay() {
                return Err(refusal(
                    "initial relay cannot fund the full registration fee after mandatory burns",
                ));
            }

            self.check_initial_relay_slots_v2(terms, policy, &mut budget)?;
            (registration, expected_case_key)
        };
        let key: [u8; PUBKEY_BYTES] = if let Some(retained_key) = expected_case_key {
            retained_key
        } else {
            let native_key = native_bond(terms.payer_bond);
            let bond = self
                .state
                .bond(&native_key)
                .ok_or_else(|| refusal("initial relay poster bond missing"))?;
            if !palw_bond_may_take_work_v2(bond, self.params.min_collateral_sompi) {
                return Err(refusal("initial relay poster cannot take new work"));
            }
            bond.pubkey.as_slice().try_into().map_err(refusal)?
        };
        if let RelayTargetV2::InitialAssignment { poster_nonce, .. } = terms.target {
            let rows = PositiveRowsV2::new(&self.state);
            let nonce = match rows
                .read(NativeRowKey::AssignmentNonce(terms.payer_bond))
                .map_err(refusal)?
            {
                None => AssignmentNonceV2::default(),
                Some(row) => match row.body {
                    NativeRowBody::AssignmentNonce(row) => row.nonce,
                    _ => return Err(refusal("wrong initial relay poster nonce row")),
                },
            };
            nonce.after_accepting(poster_nonce).map_err(refusal)?;
        }
        let signature_heap = physical
            .max_heap_bytes
            .checked_sub(reserved.quote().logical_decode_requests)
            .ok_or_else(|| refusal("initial relay signature workspace overflow"))?;
        verify_funding_relay_package_signatures_v2(body, &key, signature_heap).map_err(
            |error| match error {
                FundingRelayBodyErrorV2::Contract(why) => refusal(why),
                FundingRelayBodyErrorV2::Native(why) => PalwStateV2Error::NativeResource(why),
            },
        )?;

        let mut requests = TirNativeRequestsV1::new();
        requests
            .exact_v1::<NativeRowValue>(4)
            .map_err(PalwStateV2Error::NativeResource)?;
        requests
            .exact_v1::<FundingRelayRegistrationBodyV2>(1)
            .map_err(PalwStateV2Error::NativeResource)?;
        let row_workspace = requests.bytes_v1();
        if row_workspace > signature_heap {
            return Err(refusal(
                "initial relay row construction exceeds the native heap cap",
            ));
        }
        let _lease = reserve_native_requests_v1(row_workspace)
            .map_err(PalwStateV2Error::NativeResource)?
            .ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<NativeRowValue>(
                    "initial relay row construction",
                    "native owner is not armed",
                ))
            })?;
        let allocated = match stage {
            InitialRelayFeeStageV2::Preview => fee_input
                .plan_registration_charge(registration.fee)
                .map_err(refusal)?,
            InitialRelayFeeStageV2::Observed => fee_input,
        };
        let (auth, custody) = FundingRelayAuthorizationRowV2::registration_rows(
            body,
            key,
            action.origin(),
            *policy,
            *fees,
            *physical,
            allocated,
        )
        .map_err(refusal)?;
        // The enclosing raw checkpoint owns both writes. Their adjacency is
        // consumed by the actual material writer and historical replay.
        self.write_assignment_aux_v2(NativeRowBody::FundingRelayAuthorization(auth))?;
        self.write_assignment_aux_v2(NativeRowBody::FundingRelayCustody(custody))?;
        Ok(allocated)
    }
}

impl TransitionBuilder<'_> {
    /// The raw executor is shared by rehearsal and final native fold. Every
    /// operation enters one canonical checkpoint and decodes afresh. No saved
    /// prepared rows, public boolean or generic authority trait is consumed.
    pub(super) fn apply_positive_assignment_raw_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_positive_court_v2::coordinator::assignment_body::{
            PositiveAssignmentNativeOperationV2 as O, decode_assignment_body_v2,
            scan_assignment_continuation_id_v2,
        };
        // Capture before ANY event write, including resource/fee/P rows. This
        // exact delta position is persisted as DATA and re-executed by archive
        // loading; neither the digest nor a recorded origin grants authority.
        let boundary = (
            self.native_reservation
                .state_root_v2(&self.state)
                .map_err(PalwStateV2Error::NativeResource)?,
            self.entries.len(),
        );
        let checkpoint = self.try_checkpoint()?;
        let previous_boundary = self.assignment_raw_boundary.replace(boundary);
        let previous_active = self.assignment_active_raw.replace(action.clone());
        let result = (|| {
            self.require_raw_assignment_origin_v2(ctx, action)?;
            let tx = action.transaction();
            let rules = action.rules();
            if tx.payload.get(7) == Some(&14) {
                return self
                    .install_initial_assignment_relay_v2(
                        ctx,
                        action,
                        InitialRelayFeeStageV2::Observed,
                    )
                    .map(|_| ());
            }
            if matches!(tx.payload.get(7), Some(0..=7)) {
                let old_limits = crate::palw_positive_court_v2::coordinator::native_body::NativeBodyLimitsV2 {
                    max_body_bytes: crate::palw_positive_court_v2::coordinator::native_body::MAX_BODY_BYTES_V2,
                    max_opaque_bytes: crate::palw_positive_court_v2::coordinator::native_body::MAX_OPAQUE_BYTES_V2,
                    max_exact_relations: crate::palw_positive_court_v2::coordinator::native_body::MAX_EXACT_RELATIONS_V2,
                    max_signatures: crate::palw_positive_court_v2::coordinator::native_body::MAX_SIGNATURES_V2,
                };
                let old =
                    crate::palw_positive_court_v2::coordinator::native_body::decode_native_body_v2(
                        &tx.payload[3..],
                        old_limits,
                    )
                    .map_err(refusal)?;
                return match &old.operation {
                    crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeOperationV2::ResultAdmission { .. } => {
                        Err(refusal("assigned mode requires atomic op9, never old op0 then attach"))
                    }
                    crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeOperationV2::PositiveCourtAttestation {
                        ..
                    } => self.apply_positive_assignment_attestation_raw_v2(ctx, action, &old),
                    _ => self.apply_positive_assignment_case_raw_v2(ctx, action, &old),
                };
            }
            let limits = rules.body_limits().map_err(refusal)?;
            let body = decode_assignment_body_v2(&tx.payload[3..], limits).map_err(refusal)?;
            if let Some(original) = rules.original_material() {
                let subject =
                    scan_assignment_continuation_id_v2(&tx.payload[3..]).map_err(refusal)?;
                if self
                    .assignment_row_v2(subject)?
                    .is_none_or(|row| row.admission_material != original)
                {
                    return Err(refusal(
                        "original Assignment material changed at the current prefix",
                    ));
                }
            }
            match &body.operation {
                O::AcceptCanonicalAssignment {
                    bundle,
                    source,
                    prover,
                    signatures,
                    layout,
                } => {
                    let scope = rules.rule_scope();
                    if bundle.terms.network != rules.network()
                        || bundle.terms.ruleset.0 != scope.ruleset().as_bytes()
                    {
                        return Err(refusal(
                            "Assignment does not bind the actual original native scope",
                        ));
                    }
                    rules
                        .assignment_policy()
                        .check_terms(&bundle.terms, ctx.daa_score)
                        .map_err(refusal)?;
                    let preimages = rules.native_preimages().ok_or_else(|| {
                        refusal("actual native fee/physical preimages are missing")
                    })?;
                    preimages
                        .validate(rules.assignment_policy())
                        .map_err(refusal)?;
                    source.check(bundle).map_err(refusal)?;
                    prover
                        .check(bundle, rules.assignment_policy().positive_policy)
                        .map_err(refusal)?;
                    // Literal layout workspace was independently reserved by
                    // the raw scanner. The heavier registry/program/prompt/
                    // inventory/CoveragePlan now has its own real typed owner.
                    // This reads the CURRENT prefix; later registration/nonce
                    // changes cannot authorize the earlier carrier.
                    self.require_absent_assignment_key_v2(NativeRowKey::Assignment(
                        bundle.terms.id(),
                    ))?;
                    self.check_assignment_live_slots_v2(
                        bundle.terms.prover,
                        Some(&bundle.terms),
                        rules,
                    )?;
                    self.check_assignment_service_capacity_v2(
                        ctx,
                        rules,
                        assignment_service_demand_v2(source, prover),
                    )?;
                    let registered = action
                        .registered_class_v2()
                        .ok_or_else(|| refusal("missing actual TIR registration carriage"))?;
                    if registered.class != bundle.terms.registered_class {
                        return Err(refusal(
                            "registration archive differs from the signed Assignment class",
                        ));
                    }
                    registered
                        .check_rooted_class_v2(&self.state)
                        .map_err(refusal)?;
                    let nonce = match PositiveRowsV2::new(&self.state)
                        .read(NativeRowKey::AssignmentNonce(bundle.terms.poster))
                        .map_err(refusal)?
                    {
                        None => AssignmentNonceV2::default(),
                        Some(value) => match value.body {
                            NativeRowBody::AssignmentNonce(row) => row.nonce,
                            _ => return Err(refusal("wrong nonce row kind")),
                        },
                    };
                    nonce
                        .after_accepting(bundle.terms.poster_nonce)
                        .map_err(refusal)?;
                    for point in [
                        bundle.terms.poster,
                        bundle.terms.worker,
                        bundle.terms.prover,
                    ] {
                        let bond = self
                            .state
                            .bond(&native_bond(point))
                            .ok_or_else(|| refusal("missing current role bond"))?;
                        if !palw_bond_may_take_work_v2(bond, self.params.min_collateral_sompi) {
                            return Err(refusal("current role cannot accept work"));
                        }
                    }
                    let poster = self
                        .state
                        .bond(&native_bond(bundle.terms.poster))
                        .ok_or_else(|| refusal("missing current poster"))?;
                    let payer_key: &[u8; PUBKEY_BYTES] =
                        poster.pubkey.as_slice().try_into().map_err(refusal)?;
                    let (quote, body_heap) =
                        crate::palw_positive_assignment_funds_v2::quote_native_assignment_open_v2(
                            rules.assignment_policy(),
                            &bundle.terms,
                            &preimages.assignment_fees,
                            &preimages.physical,
                            &tx.payload[3..],
                            ctx.daa_score,
                        )
                        .map_err(refusal)?;
                    if quote.id() != bundle.quote {
                        return Err(refusal(
                            "signed Assignment quote differs from the actual native Open quote",
                        ));
                    }
                    let funding = crate::palw_positive_assignment_funds_v2::qualify_assignment_funding_output_v2(
                        bundle,
                        tx,
                        payer_key,
                        action.current_output(),
                        quote.fee,
                        quote.capital,
                        ctx.daa_score,
                    )
                    .map_err(refusal)?;
                    self.require_assignment_output_unlocked_v2(funding.outpoint())?;
                    let layout: crate::palw_tir_class_v1::PalwTirLayoutV1 =
                        borsh::from_slice(layout).map_err(refusal)?;
                    let reserved = crate::palw_positive_assignment_resources_v2::ReservedAssignmentDerivedResourcesV2::derive(
                        &self.state,
                        bundle.terms,
                        &layout,
                        rules.assignment_policy(),
                        rules.positive_policy(),
                        &preimages.source_policy,
                        rules.prompt_form(),
                        preimages.physical.max_heap_bytes.min(rules.assignment_policy().max_heap_bytes),
                    )
                    .map_err(assignment_resource_error)?;
                    let resources = reserved.resources();
                    self.require_unowned_assignment_job_v2(resources.binding().job().job_id)?;
                    if resources.source_quote().digest != bundle.source_catalog
                        || resources.binding().job().context_hash().as_bytes()
                            != bundle.job_context.0
                    {
                        return Err(refusal(
                            "independently derived native job/source catalog differs from signed bundle",
                        ));
                    }
                    resources
                        .binding()
                        .authenticate_bundle_roles(&self.state, bundle, signatures)
                        .map_err(refusal)?;
                    let (horizon, _) = assignment_horizon_v2(
                        rules,
                        bundle,
                        source,
                        prover,
                        resources,
                        ctx.daa_score,
                    )?;
                    self.check_initial_assignment_relay_v2(
                        ctx,
                        action,
                        bundle,
                        (funding.outpoint(), funding.amount()),
                        payer_key,
                    )?;
                    let (anchor, anchor_daa) = action
                        .signed_anchor_clock_v2()
                        .ok_or_else(missing_assignment_history)?;
                    if anchor.as_bytes() != bundle.terms.branch_anchor.0 {
                        return Err(refusal(
                            "Assignment verifier bound uses a different signed ancestor",
                        ));
                    }
                    let certification = rules
                        .certification_policy()
                        .ok_or_else(|| refusal("missing actual Gate C policy"))?;
                    let maximum_scan = rules.assignment_policy().max_replay_work
                        / preimages.source_policy.work.per_control;
                    let draw_epochs = self.check_assignment_draw_calendar_v2(
                        certification,
                        ctx.daa_score,
                        bundle.terms.result_before_daa,
                        maximum_scan,
                    )?;
                    let maximum_scan = maximum_scan - draw_epochs;
                    let heap_budget = rules
                        .assignment_policy()
                        .max_heap_bytes
                        .min(preimages.physical.max_heap_bytes)
                        .min(rules.positive_policy().max_case_heap_bytes);
                    let cohort = super::positive_assignment_verifiers::derive_assignment_verifier_candidates_v2(
                        &self.state,
                        certification,
                        anchor_daa,
                        bundle.terms.result_before_daa,
                        ctx.daa_score,
                        maximum_scan,
                        rules
                            .assignment_policy()
                            .max_history_bytes
                            .min(preimages.physical.max_retained_bytes)
                            .min(preimages.physical.max_replay_bytes)
                            .min(heap_budget),
                    )?;
                    let workspace = super::positive_assignment_certification::positive_certification_workspace_bytes_v2(
                        certification.m,
                        cohort.count(),
                    )?;
                    let simultaneous = reserved
                        .allocation_quote()
                        .total_requests
                        .checked_add(body_heap)
                        .and_then(|n| n.checked_add(workspace))
                        .and_then(|n| n.checked_add(cohort.bytes().len() as u64))
                        .ok_or_else(|| refusal("simultaneous Open workspace overflow"))?;
                    if simultaneous > heap_budget {
                        return Err(refusal(
                            "original Assignment budget cannot cover Gate C draw workspace",
                        ));
                    }
                    // Check the real local workspace now. Persisting a future
                    // slot and protecting its work/custody remain separate.
                    let _p_workspace =
                        super::positive_assignment_certification::reserve_p_workspace(workspace)?;
                    self.check_initial_assignment_gate_c_envelope_v2(
                        rules,
                        &bundle.terms,
                        resources.binding().job().job_id,
                        &horizon,
                    )?;
                    let encode_cap = rules
                        .assignment_policy()
                        .max_frame_bytes
                        .min(quote.usage.retained_bytes)
                        .min(quote.usage.replay_bytes / 2);
                    let encode_heap = encode_cap
                        .checked_mul(3)
                        .and_then(|n| {
                            n.checked_add(
                                (2 * size_of::<AssignmentAdmissionMaterialV2>()
                                    + 2 * size_of::<AssignmentRow>())
                                    as u64,
                            )
                        })
                        .ok_or_else(|| refusal("Open encoding workspace overflow"))?;
                    if simultaneous
                        .checked_add(encode_heap)
                        .is_none_or(|peak| peak > heap_budget)
                    {
                        return Err(refusal(
                            "original Assignment budget cannot cover Open encoding",
                        ));
                    }
                    let _encoding = crate::palw_native_resource_v1::reserve_native_requests_v1(encode_heap)
                        .map_err(PalwStateV2Error::NativeResource)?
                        .ok_or(PalwStateV2Error::NativeContextUnavailable(
                            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable,
                        ))?;
                    let (assignment, open) = self.prepare_positive_assignment_open_v2(
                        action,
                        bundle,
                        source,
                        prover,
                        resources,
                        registered,
                        quote,
                        cohort.bytes(),
                        encode_cap,
                    )?;
                    if open.len() as u64 > encode_cap {
                        return Err(refusal(
                            "Open encoding exceeds its original capacity reservation",
                        ));
                    }
                    self.install_positive_assignment_v2(
                        ctx,
                        AssignmentInstallV2 {
                            open: &open,
                            assignment,
                        },
                    )
                }
                O::AdmitAssignedResult {
                    assignment,
                    payload,
                } => {
                    let row = self.assignment_row_v2(*assignment)?.ok_or_else(|| {
                        refusal("missing actual accepted Assignment at this prefix")
                    })?;
                    if row.status != AssignmentStatusV2::Open
                        || ctx.daa_score >= row.bundle.terms.result_before_daa
                    {
                        return Err(refusal(
                            "Assignment unavailable for single Result consumption",
                        ));
                    }
                    use misaka_palw_positive_contract::{
                        funding::funding_scope,
                        messages::{ClaimStatus, MinerDataStatus, WitnessStatus},
                        rows::{AcceptedMaterial, AggregateUsage},
                    };
                    let material = action
                        .original_admission_v2()
                        .ok_or_else(missing_assignment_history)?;
                    material.check_row(&row).map_err(refusal)?;
                    let (relay, horizon, coverage, output, payer_key) =
                        self.check_assigned_result_carriage_v2(ctx, action, &row, payload)?;
                    let origin = action.origin();
                    let result = ResultRow {
                        result: payload.result.clone(),
                        miner_key: row.worker_key,
                        miner_signature: payload.miner_signature,
                        origin,
                        policy: row.positive_policy,
                        fired_fork: row.fired_rules,
                        funding_scope: funding_scope(
                            &payload.result,
                            &payload.service,
                            &payload.basis,
                        ),
                        dependencies: row.dependencies,
                        role_assignment: AcceptedMaterial {
                            origin: row.origin,
                            digest: row.id().0,
                        },
                    };
                    let index = ClaimIndexRow {
                        claim: payload.result.claim_id(),
                        result: payload.result.id(),
                        case: payload.result.case_id(),
                    };
                    let [_, fee, capital, surplus] = output.initial_legs();
                    let funding = FundingRow {
                        consent: payload.consent.clone(),
                        basis: payload.basis,
                        payer_key,
                        payer_signature: payload.payer_signature,
                        funding_origin: origin,
                        original_outpoint: output.outpoint(),
                        current_outpoint: Some(output.outpoint()),
                        initial_locked: output.amount(),
                        fee,
                        capital,
                        surplus,
                        liability: row.liability,
                        last_settlement: None,
                    };
                    let mut case = CaseRow {
                        case: index.case,
                        result: index.result,
                        service: payload.service,
                        coverage,
                        status: ClaimStatus::ResultCommitted,
                        miner_data: MinerDataStatus::NotRequested,
                        witness_status: WitnessStatus::Building,
                        positive_witness: None,
                        usage: AggregateUsage::default(),
                        reservation: row.reservation,
                        liability: row.liability,
                        maturity_daa: horizon.latest_prosecution_daa.max(origin.accepted_daa),
                        unresolved_fraud: 0,
                        last_event: origin,
                    };
                    case.reservation =
                        crate::palw_positive_assignment_material_v2::assigned_case_reservation_v2(
                            &row, origin, &case, &funding,
                        );
                    self.consume_positive_assignment_v2(
                        ctx,
                        AssignedCaseInstallV2 {
                            material,
                            expected_assignment: row,
                            index,
                            result,
                            case,
                            funding,
                        },
                        relay,
                        horizon,
                    )
                }
                O::CancelAssignment { .. } | O::ExpireAssignment { .. } => {
                    self.close_positive_assignment_v2(ctx, action, &body.operation)
                }
                O::JoinAssignmentProver { consent, signature } => {
                    self.check_assignment_prover_join_v2(ctx, action, consent, signature)?;
                    Err(refusal(
                        "native cumulative join capacity/funds/material owner is not yet resolved",
                    ))
                }
                O::SettleAssignment { .. } => {
                    self.settle_positive_assignment_raw_funds_v2(ctx, action, &body)
                }
            }
        })();
        self.assignment_active_raw = previous_active;
        self.assignment_raw_boundary = previous_boundary;
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }
}

impl TransitionBuilder<'_> {
    fn check_assigned_result_carriage_v2(
        &self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        row: &AssignmentRow,
        payload: &crate::palw_positive_court_v2::coordinator::assignment_body::AssignedResultPayloadV2,
    ) -> Result<
        (
            Option<CheckedInitialFundingRelayV2>,
            crate::palw_positive_assignment_horizon_v2::AssignmentHorizonSpecV2,
            misaka_palw_positive_contract::messages::CoverageDescriptor,
            crate::palw_positive_assignment_funds_v2::AssignmentFundingOutputV2,
            [u8; PUBKEY_BYTES],
        ),
        PalwStateV2Error,
    > {
        use crate::{
            palw_native_resource_v1::reserve_native_requests_v1,
            palw_positive_assignment_funds_v2::qualify_assigned_case_funding_output_v2,
            palw_positive_assignment_resources_v2::ReservedAssignmentDerivedResourcesV2,
            palw_positive_court_v2::coordinator::assignment_body::{
                PositiveAssignmentNativeOperationV2 as O, decode_assignment_body_v2,
                scan_assignment_body_v2,
            },
        };
        use misaka_palw_positive_contract::identity::Digest;
        let job = Hash64::from_bytes(row.job.0);
        let route = self
            .state
            .kernel_route_state_v1()
            .ok_or_else(missing_assignment_history)?;
        if row.status != AssignmentStatusV2::Open
            || payload.result.job != row.job
            || route.contains_positive_job_v2(job)
            || !route.owns_open_assignment_job_v2(job, Hash64::from_bytes(row.id().0.0))
        {
            return Err(refusal(
                "BindResult requires the original exclusive open Assignment job",
            ));
        }
        if !route.owns_open_assignment_draw_slot_v2(
            row.bundle.terms.result_before_daa,
            Hash64::from_bytes(row.id().0.0),
        ) {
            return Err(refusal(
                "BindResult requires the original protected shared Gate C draw slot",
            ));
        }
        let rules = action.rules();
        let open = action
            .original_open_carriage_v2()
            .ok_or_else(missing_assignment_history)?;
        if open.origin() != row.origin || rules.original_material() != Some(row.admission_material)
        {
            return Err(missing_assignment_history());
        }
        let rows = PositiveRowsV2::new(&self.state);
        let funding = rows
            .read(NativeRowKey::AssignmentFunding(row.id()))
            .map_err(refusal)?
            .ok_or_else(missing_assignment_history)?;
        let NativeRowBody::AssignmentFunding(funding) = funding.body else {
            return Err(missing_assignment_history());
        };
        payload
            .result
            .authenticate(&row.worker_key, &payload.miner_signature)
            .map_err(refusal)?;
        payload
            .consent
            .authenticate(
                &payload.result,
                &payload.service,
                &payload.basis,
                ctx.daa_score,
                &funding.payer_key,
                &payload.payer_signature,
            )
            .map_err(refusal)?;
        let source = &row.source_consent;
        let budget = &payload.service.budget;
        if payload.service.source_catalog != source.source_catalog
            || payload.basis.policy != row.positive_policy
            || payload.basis.schedule != rules.assignment_policy().case_fee_schedule
            || payload.service.deadline > source.service_before_daa
            || budget.rounds > source.budget.rounds
            || budget.unique_source_bytes > source.budget.unique_source_bytes
            || budget.transmitted_bytes > source.budget.transmitted_bytes
            || budget.additional_work > source.budget.additional_work
            || budget.retained_bytes > source.budget.retained_bytes
            || budget.retention_until > source.budget.retention_until
            || budget.fee > source.budget.fee
            || budget.witness_attempts > source.budget.witness_attempts
        {
            return Err(refusal(
                "Case terms exceed the original signed policy and source bounds",
            ));
        }
        // These are the payer-signed amounts. The reservation owner must still
        // derive and compare the complete native resource and price quote.
        let output = qualify_assigned_case_funding_output_v2(
            row,
            &funding,
            &payload.result,
            &payload.service,
            &payload.basis,
            &payload.consent,
            &funding.payer_key,
            action.transaction(),
            action.current_output(),
            ctx.daa_score,
        )
        .map_err(refusal)?;
        self.require_assignment_output_unlocked_v2(output.outpoint())?;

        let tx = open.transaction();
        if tx.payload.get(..8) != Some(&[1, 0, 176, 2, 0, 1, 0, 8]) {
            return Err(missing_assignment_history());
        }
        let limits = rules
            .body_limits()
            .map_err(|_| missing_assignment_history())?;
        let raw = &tx.payload[3..];
        let summary =
            scan_assignment_body_v2(raw, limits).map_err(|_| missing_assignment_history())?;
        let _decode = reserve_native_requests_v1(summary.heap).map_err(PalwStateV2Error::NativeResource)?.ok_or({
            PalwStateV2Error::NativeContextUnavailable(
                crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable,
            )
        })?;
        let original =
            decode_assignment_body_v2(raw, limits).map_err(|_| missing_assignment_history())?;
        let O::AcceptCanonicalAssignment {
            bundle,
            source,
            prover,
            signatures,
            layout,
        } = &original.operation
        else {
            return Err(missing_assignment_history());
        };
        if bundle != &row.bundle || source != &row.source_consent || prover != &row.prover_consent {
            return Err(missing_assignment_history());
        }
        let prefix = open
            .prefix_state()
            .map_err(|_| missing_assignment_history())?;
        let layout: crate::palw_tir_class_v1::PalwTirLayoutV1 =
            borsh::from_slice(layout).map_err(|_| missing_assignment_history())?;
        let preimages = rules
            .native_preimages()
            .ok_or_else(missing_assignment_history)?;
        let resources = ReservedAssignmentDerivedResourcesV2::derive(
            prefix,
            bundle.terms,
            &layout,
            rules.assignment_policy(),
            rules.positive_policy(),
            &preimages.source_policy,
            rules.prompt_form(),
            preimages
                .physical
                .max_heap_bytes
                .min(rules.assignment_policy().max_heap_bytes),
        )
        .map_err(assignment_resource_error)?;
        let resources = resources.resources();
        let binding = resources.binding();
        if resources.source_quote().digest != bundle.source_catalog
            || Digest(binding.job().job_id.as_bytes()) != row.job
            || Digest(binding.job().context_hash().as_bytes()) != row.context
            || Digest(binding.job().prompt_token_ids_hash.as_bytes()) != row.input
            || prefix
                .bond(&native_bond(bundle.terms.worker))
                .is_none_or(|bond| bond.pubkey.as_slice() != row.worker_key)
        {
            return Err(missing_assignment_history());
        }
        binding
            .authenticate_bundle_roles(prefix, bundle, signatures)
            .map_err(|_| missing_assignment_history())?;
        binding.check_result(&payload.result).map_err(refusal)?;
        use misaka_palw_positive_contract::{
            messages::{CheckProfile, CoverageDescriptor, coverage_plan_id},
            tir::exact_derivation_id,
        };
        let plan = resources.plan();
        let derivation = exact_derivation_id();
        let profile = CheckProfile::ExactStreamV1;
        let relations = plan.leaf_count();
        if relations != payload.basis.mandatory_relations
            || payload.basis.profile != profile
            || payload.basis.derivation != derivation
        {
            return Err(refusal(
                "Case coverage basis differs from the original registered program",
            ));
        }
        let cost = plan.cost();
        let coverage = CoverageDescriptor {
            scope: payload.result.scope,
            result: payload.result.id(),
            plan: coverage_plan_id(&payload.result, profile, derivation, relations)
                .map_err(refusal)?,
            profile,
            mandatory_relations: relations,
            reserved_elements: cost.evaluation_elements,
            reserved_terms: cost.evaluation_terms,
            max_part_bytes: resources.check_limits().max_part_bytes,
        };
        let (horizon, source_ledger) = assignment_horizon_v2(
            rules,
            bundle,
            source,
            prover,
            resources,
            row.origin.accepted_daa,
        )?;
        let gate_c = rules.gate_c_clock(ctx.daa_score).map_err(refusal)?;
        horizon
            .check_case_source_floor_v2(
                &payload.service,
                &gate_c,
                rules.assignment_policy().refund_window_daa,
                rules.positive_policy().max_attempts,
                resources
                    .all_attempt_signed_frame_bytes_v2()
                    .map_err(assignment_resource_error)?,
                source_ledger,
            )
            .map_err(refusal)?;
        let relay = if rules.funding_relay_profile().requires_relay() {
            let checked = self.check_pending_initial_f_relay_v2(
                ctx,
                action,
                RelayFundingPurposeV2::CaseC,
                output.outpoint(),
                &funding.payer_key,
                &|terms| {
                    crate::palw_positive_assignment_funds_v2::check_initial_case_relay_target_v2(
                        terms,
                        bundle,
                        payload.result.id(),
                        &payload.consent,
                        output.outpoint(),
                        output.amount(),
                        ctx.daa_score,
                    )
                },
            )?;
            if checked.service_cutoff < horizon.retention_until_daa {
                return Err(refusal(
                    "initial Case relay does not cover the signed retention horizon",
                ));
            }
            Some(checked)
        } else {
            None
        };
        if payload.basis.required_fee < horizon.case_money.fee
            || payload.basis.required_capital < horizon.case_money.capital
        {
            return Err(refusal(
                "Case funding does not cover the original native quote",
            ));
        }
        Ok((relay, horizon, coverage, output, funding.payer_key))
    }
}
