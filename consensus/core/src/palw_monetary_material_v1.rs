//! Explicit monetary material covenants. An absolute computation TTL does not
//! authorize retaining material across an unknown physical-payment clock.
//! These new signatures authorize that event-relative obligation themselves.
//! A fixed prepaid fee is an offer, not evidence of bounded time costs.
use crate::{
    Hash64,
    config::params::{ForkActivation, Params},
    palw_mode_v2::PalwModeV2Error,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_MONETARY_MATERIAL_TABLE_V1: u8 = 120;
pub const PALW_MONETARY_MATERIAL_ENROLL_TAG_V1: u8 = 240;
pub const PALW_MONETARY_MATERIAL_REPLACE_TAG_V1: u8 = 241;
pub const PALW_MONETARY_MATERIAL_ACTION_TAG_V1: u8 = 242;
pub const PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1: usize = 8;
pub const PALW_MONETARY_MATERIAL_MAX_DEPENDENCIES_V1: usize = 64;
pub const PALW_MONETARY_MATERIAL_MAX_CONTRACTS_V1: u32 = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialPolicyV1 {
    pub version: u16,
    pub max_contracts: u32,
    pub max_contracts_per_provider: u32,
    pub max_bytes_per_contract: u64,
    pub max_bytes_per_provider: u64,
    pub minimum_collateral_sompi: u64,
    pub minimum_fee_sompi: u64,
    pub window_daa: u64,
    pub response_window_daa: u64,
    pub max_enrollment_daa: u64,
    /// Proposed measurements/cost provenance. No report is approved implicitly.
    pub cost_report_root: Hash64,
}
impl PalwMonetaryMaterialPolicyV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || self.max_contracts == 0
            || self.max_contracts > PALW_MONETARY_MATERIAL_MAX_CONTRACTS_V1
            || self.max_contracts_per_provider == 0
            || self.max_contracts_per_provider > self.max_contracts
            || self.max_bytes_per_contract == 0
            || self.max_bytes_per_provider < self.max_bytes_per_contract
            || self.minimum_collateral_sompi == 0
            || self.minimum_fee_sompi == 0
            || self.window_daa == 0
            || self.response_window_daa == 0
            || self.max_enrollment_daa == 0
            || self.cost_report_root == Hash64::default()
        {
            return Err("monetary material policy has missing or excessive capacity/cost bounds");
        }
        Ok(())
    }
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/MONETARY-MATERIAL-POLICY/V1", self))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMonetaryMaterialFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwMonetaryMaterialPolicyV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMonetaryMaterialMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwMonetaryMaterialPolicyV1,
}

/// Each provider signs the entire enrollment, including the exact dependency
/// list and event-relative release rule. This never edits ProviderLeaseV1.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialTermsV1 {
    pub version: u16,
    pub root: Hash64,
    pub dependencies: Vec<Hash64>,
    /// Authorizes only subsequently authenticated slices of this exact EXEC plan.
    pub exec_plan_root: Option<Hash64>,
    pub sponsor: PalwBondKeyV2,
    pub nonce: Hash64,
    pub policy: Hash64,
    pub max_material_bytes: u64,
    material_manifest: None,
    /// Complete canonical bytes for the original and every continuation.
    pub material_manifest: Option<crate::palw_material_capacity_v1::PalwMaterialManifestV1>,
    pub window_daa: u64,
    pub enrollment_until_daa: u64,
    /// Pending rights and queues have no expiry. The provider explicitly accepts
    /// service until all rights terminate and last physical payment + W has
    /// passed, AND the tail block is finalized on the selected chain.
    pub payment_relative_until_finality: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialOfferV1 {
    pub provider: PalwBondKeyV2,
    pub collateral_sompi: u64,
    pub prepaid_fee_sompi: u64,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialEnrollmentV1 {
    pub terms: PalwMonetaryMaterialTermsV1,
    pub offers: Vec<PalwMonetaryMaterialOfferV1>,
}
impl PalwMonetaryMaterialEnrollmentV1 {
    /// Signatures are excluded from the consent preimage; all provider offers,
    /// fees/collateral and the sponsor remain bound by every signature.
    pub fn payload(&self) -> Vec<u8> {
        borsh::to_vec(&(
            &self.terms,
            self.offers.iter().map(|o| (o.provider, o.collateral_sompi, o.prepaid_fee_sompi)).collect::<Vec<_>>(),
        ))
        .expect("monetary material consent serializes")
    }
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/MONETARY-MATERIAL-CONSENT/V1", &self.payload()))
    }
}

/// Worker/provider adapter: every signer signs the same consent before it is
/// submitted. Missing keys produce no unsigned or partly signed carrier.
pub fn monetary_material_enrollment_object_v1(
    network: Hash64,
    mut enrollment: PalwMonetaryMaterialEnrollmentV1,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Option<crate::palw_state_v2::PalwConsensusObjectV2> {
    use crate::palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, palw_provider_court_message_v1};
    let payload = enrollment.payload();
    for offer in &mut enrollment.offers {
        let message = palw_provider_court_message_v1(network, PALW_MONETARY_MATERIAL_ENROLL_TAG_V1, &offer.provider, &payload);
        offer.signature =
            sign(&offer.provider, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1).filter(|s| !s.is_empty())?;
    }
    let message = palw_provider_court_message_v1(network, PALW_MONETARY_MATERIAL_ENROLL_TAG_V1, &enrollment.terms.sponsor, &payload);
    let signature =
        sign(&enrollment.terms.sponsor, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1).filter(|s| !s.is_empty())?;
    Some(crate::palw_state_v2::PalwConsensusObjectV2::MonetaryMaterialEnrolledV1 { enrollment: Box::new(enrollment), signature })
}

pub fn monetary_material_replacement_object_v1(
    network: Hash64,
    mut replacement: PalwMonetaryMaterialReplacementV1,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Option<crate::palw_state_v2::PalwConsensusObjectV2> {
    use crate::palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, palw_provider_court_message_v1};
    let payload = replacement.payload();
    for offer in &mut replacement.enrollment.offers {
        let message = palw_provider_court_message_v1(network, PALW_MONETARY_MATERIAL_REPLACE_TAG_V1, &offer.provider, &payload);
        offer.signature =
            sign(&offer.provider, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1).filter(|s| !s.is_empty())?;
    }
    let sponsor = replacement.enrollment.terms.sponsor;
    let message = palw_provider_court_message_v1(network, PALW_MONETARY_MATERIAL_REPLACE_TAG_V1, &sponsor, &payload);
    let signature = sign(&sponsor, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1).filter(|s| !s.is_empty())?;
    Some(crate::palw_state_v2::PalwConsensusObjectV2::MonetaryMaterialReplacedV1 { replacement: Box::new(replacement), signature })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialReplacementV1 {
    pub root: Hash64,
    pub previous: Hash64,
    pub sequence: u64,
    pub enrollment: PalwMonetaryMaterialEnrollmentV1,
}
impl PalwMonetaryMaterialReplacementV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/MONETARY-MATERIAL-REPLACEMENT/V1", &self.payload()))
    }
    pub fn payload(&self) -> Vec<u8> {
        borsh::to_vec(&(self.root, self.previous, self.sequence, self.enrollment.payload())).expect("replacement serializes")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwMonetaryMaterialActionV1 {
    SegmentedChallenge {
        root: Hash64,
        provider: PalwBondKeyV2,
        dependency: Hash64,
        position: u32,
        part: u32,
        valid_until_daa: u64,
    },
    SegmentedAnswer {
        root: Hash64,
        provider: PalwBondKeyV2,
        dependency: Hash64,
        position: u32,
        part: u32,
        bytes: Vec<u8>,
    },
    HeldChallenge {
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        missing: crate::palw_held_da_v1::PalwHeldMissingV1,
        binding: Box<crate::palw_step::PalwStepBindingV2>,
        valid_until_daa: u64,
    },
    HeldAnswer {
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        missing: crate::palw_held_da_v1::PalwHeldMissingV1,
        disclosure: Box<crate::palw_held_da_v1::PalwHeldDisclosureV1>,
    },
    Challenge {
        root: Hash64,
        provider: PalwBondKeyV2,
        dependency: Hash64,
        stage: u8,
        position: u32,
        valid_until_daa: u64,
    },
    Answer {
        root: Hash64,
        provider: PalwBondKeyV2,
        dependency: Hash64,
        stage: u8,
        position: u32,
        bytes: Vec<u8>,
    },
    /// Acceptance verifies exact tail ancestry against THIS block's finality
    /// point. Local virtual finality is never a consensus authorization.
    Release {
        root: Hash64,
        consent: Hash64,
        tail_block: Hash64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialProviderV1 {
    pub provider: PalwBondKeyV2,
    pub collateral_sompi: u64,
    pub prepaid_fee_sompi: u64,
    pub charged: bool,
    pub fee_income: Hash64,
    pub fee_context: Hash64,
    pub fee_accepted_daa: u64,
    pub fee_queued: bool,
    pub fee_paid: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialChallengeV1 {
    pub provider: PalwBondKeyV2,
    pub dependency: Hash64,
    pub stage: u8,
    pub position: u32,
    pub challenger: PalwBondKeyV2,
    pub deadline_daa: u64,
    pub answered: bool,
    pub native_missing: Option<crate::palw_held_da_v1::PalwHeldMissingV1>,
    pub native_binding: Option<Box<crate::palw_step::PalwStepBindingV2>>,
    pub segment_part: Option<u32>,
}
/// A validated native queue drain, not a kernel FeePaid promise. Transaction
/// attribution still requires the native block's authenticated coinbase/output
/// proof; this receipt supplies its exact source root/amount/block authority.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialPaymentObservationV1 {
    pub root: Hash64,
    pub income: Hash64,
    pub queue: Hash64,
    pub amount: u64,
    pub coinbase_queue_amount: u64,
    pub paid_block: Hash64,
    pub paid_daa: u64,
    pub material_receipt_id: Hash64,
    /// Exact original SourceFee/native entitlement; never guessed by amount.
    pub entitlement: Option<Hash64>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialRowV1 {
    pub terms: PalwMonetaryMaterialTermsV1,
    pub consent: Hash64,
    pub sequence: u64,
    pub accepted_daa: u64,
    pub root_seen: bool,
    pub providers: Vec<PalwMonetaryMaterialProviderV1>,
    pub challenges: Vec<PalwMonetaryMaterialChallengeV1>,
    /// Maximum real payment tail observed. A later unpaid right resets only the
    /// finality anchor, never this maximum, price, or provider's signed consent.
    pub last_tail_until: Option<u64>,
    pub tail_block: Option<Hash64>,
    pub tail_blue_score: Option<u64>,
    /// Bounded latest receipt and commitment to earlier exact observations.
    /// A historical economic receipt needs an authenticated delta/block proof.
    pub last_payment: Option<PalwMonetaryMaterialPaymentObservationV1>,
    pub payment_history_root: Hash64,
    /// Bounded accumulators include retired generations without retaining all rows.
    pub service_fee_count: u64,
    pub service_fee_total_sompi: u128,
    pub service_fee_paid_sompi: u128,
    pub service_fee_history_root: Hash64,
    /// Only live unpaid material queue receipts, not cumulative history.
    pub queued_entitlements: std::collections::BTreeMap<Hash64, Hash64>,
}
pub fn monetary_service_fee_history_next_v1(previous: Hash64, income: Hash64, amount: u64) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
        b"MISAKA/PALW/MONETARY-SERVICE-FEE-HISTORY/V1",
        &(previous, income, amount),
    ))
}
impl PalwMonetaryMaterialRowV1 {
    pub fn healthy(&self, state: &PalwChainStateV2) -> bool {
        let mut operators = std::collections::BTreeSet::new();
        for offer in self.providers.iter().filter(|p| !p.charged) {
            if let Some(bond) = state.bond(&offer.provider) {
                operators.insert(bond.operator_id);
            }
        }
        operators.len() >= 2
    }
}
impl PalwChainStateV2 {
    /// New native work is admitted only against an explicit complete wire
    /// manifest. An unused-root prepaid service enrollment carries no claim
    /// authority; numeric byte limits alone never authorize inference work.
    pub fn require_native_monetary_material_scope_v1(
        &self,
        params: &PalwStateParamsV2,
        root: Hash64,
        claim: &crate::palw_state_v2::PalwClaimStateV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        self.require_monetary_material_enrollment_v1(params, root, claim.accepted_daa, None)?;
        if params.monetary_material_at_v1(claim.accepted_daa).is_none() {
            return Ok(());
        }
        let row = self.monetary_material_row_v1(root).ok_or_else(|| {
            crate::palw_state_v2::PalwStateV2Error::ExecV2Refused("native material has no signed capacity manifest".into())
        })?;
        let scope = row.terms.material_manifest.as_ref().and_then(|m| m.scope(root)).ok_or_else(|| {
            crate::palw_state_v2::PalwStateV2Error::ExecV2Refused(
                "native material full codec capacity is unsupported or unenrolled".into(),
            )
        })?;
        if scope.codec != crate::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete
            || scope.commitment != claim.execution_root
        {
            return Err(crate::palw_state_v2::PalwStateV2Error::ExecV2Refused(
                "native material manifest substituted this execution".into(),
            ));
        }
        Ok(())
    }
    pub fn require_kernel_monetary_material_scope_v1(
        &self,
        params: &PalwStateParamsV2,
        root: Hash64,
        row: &misaka_palw_kernel::ledger::ClaimRowV1,
        now: u64,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        self.require_monetary_material_enrollment_v1(params, root, now, None)?;
        if params.monetary_material_at_v1(now).is_none() {
            return Ok(());
        }
        let contract = self
            .monetary_material_row_v1(root)
            .ok_or_else(|| crate::palw_state_v2::PalwStateV2Error::ExecV2Refused("kernel material has no signed manifest".into()))?;
        let scope = contract.terms.material_manifest.as_ref().and_then(|m| m.scope(root)).ok_or_else(|| {
            crate::palw_state_v2::PalwStateV2Error::ExecV2Refused("kernel material complete codec capacity is unenrolled".into())
        })?;
        let commitment = crate::palw_material_capacity_v1::kernel_material_commitment_v1(row);
        let expected = if matches!(row.body, misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { .. }) {
            crate::palw_material_capacity_v1::PalwMaterialCodecV1::SegmentedParts
        } else {
            crate::palw_material_capacity_v1::PalwMaterialCodecV1::KernelPositions
        };
        if scope.commitment != commitment || scope.codec != expected {
            return Err(crate::palw_state_v2::PalwStateV2Error::ExecV2Refused(
                "kernel material manifest substituted this claim body".into(),
            ));
        }
        Ok(())
    }
    pub fn monetary_material_row_v1(&self, root: Hash64) -> Option<PalwMonetaryMaterialRowV1> {
        self.kernel_route()?.aux_row(PALW_MONETARY_MATERIAL_TABLE_V1, root.as_byte_slice())
    }
    pub fn monetary_material_rows_v1(&self) -> Vec<PalwMonetaryMaterialRowV1> {
        self.kernel_route()
            .map(|r| {
                r.aux
                    .iter()
                    .filter(|((table, _), _)| *table == PALW_MONETARY_MATERIAL_TABLE_V1)
                    .filter_map(|(_, bytes)| borsh::from_slice(bytes).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn require_monetary_material_enrollment_v1(
        &self,
        params: &PalwStateParamsV2,
        root: Hash64,
        accepted_daa: u64,
        dependencies: Option<&[Hash64]>,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        let Some(policy) = params.monetary_material_at_v1(accepted_daa) else { return Ok(()) };
        if self.claim(&root).is_some_and(|c| params.monetary_material_from_v1().is_some_and(|from| c.accepted_daa < from))
            || self
                .kernel_route()
                .and_then(|r| r.kernel_claim_row_v1(&root))
                .is_some_and(|c| params.monetary_material_from_v1().is_some_and(|from| c.committed_daa < from))
        {
            return Ok(());
        }
        let row = self.monetary_material_row_v1(root).ok_or_else(|| {
            crate::palw_state_v2::PalwStateV2Error::ExecV2Refused("new paid material has no explicit signed monetary covenant".into())
        })?;
        if row.terms.policy != policy.id()
            || (!row.root_seen && accepted_daa > row.terms.enrollment_until_daa)
            || dependencies.is_some_and(|ids| ids != row.terms.dependencies.as_slice())
        {
            return Err(crate::palw_state_v2::PalwStateV2Error::ExecV2Refused(
                "monetary covenant policy or exact dependencies differ".into(),
            ));
        }
        Ok(())
    }
    pub fn monetary_material_reserved_v1(&self, bond: &PalwBondKeyV2) -> u128 {
        self.monetary_material_rows_v1()
            .iter()
            .flat_map(|r| &r.providers)
            .filter(|p| p.provider == *bond && !p.charged)
            .map(|p| u128::from(p.collateral_sompi))
            .sum::<u128>()
            .saturating_add(
                self.monetary_material_rows_v1()
                    .iter()
                    .flat_map(|r| &r.challenges)
                    .filter(|c| c.challenger == *bond && !c.answered)
                    .map(|_| u128::from(crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1))
                    .sum(),
            )
    }
    pub fn monetary_material_scope_v1(&self, row: &PalwMonetaryMaterialRowV1) -> std::collections::BTreeSet<Hash64> {
        let mut scope: std::collections::BTreeSet<_> =
            std::iter::once(row.terms.root).chain(row.terms.dependencies.iter().copied()).collect();
        if let Some(work) = self.exec_v2_root_v1(&row.terms.root).filter(|w| Some(w.plan_root) == row.terms.exec_plan_root) {
            scope.insert(work.prefix_claim);
            for ((root, _), slice) in &self.exec_v2_state_v1().slices {
                if *root == row.terms.root {
                    scope.insert(slice.evidence_root);
                }
            }
        }
        scope
    }
    pub fn monetary_material_hold_v1(&self, root: Hash64) -> Option<u64> {
        if self.kernel_route().is_some_and(|r| {
            r.aux.iter().any(|((t, _), bytes)| {
                *t == PALW_MONETARY_MATERIAL_TABLE_V1 && borsh::from_slice::<PalwMonetaryMaterialRowV1>(bytes).is_err()
            })
        }) {
            return Some(u64::MAX);
        }
        self.monetary_material_rows_v1().iter().any(|r| self.monetary_material_scope_v1(r).contains(&root)).then_some(u64::MAX)
    }
    pub fn monetary_material_release_anchor_v1(&self, root: Hash64) -> Option<Hash64> {
        self.monetary_material_row_v1(root)?.tail_block
    }
    pub fn monetary_material_recovery_needed_v1(&self, root: Hash64) -> bool {
        self.monetary_material_row_v1(root).is_some_and(|r| !r.healthy(self))
    }
    pub fn assert_monetary_material_consistency_v1(
        &self,
        params: &PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        let bad = |s: &str| PalwStateV2Error::CarriageInconsistent(format!("monetary material: {s}"));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let mut count = 0usize;
        for ((table, key), bytes) in &route.aux {
            if *table != PALW_MONETARY_MATERIAL_TABLE_V1 {
                continue;
            }
            let row: PalwMonetaryMaterialRowV1 = borsh::from_slice(bytes).map_err(|_| bad("malformed covenant"))?;
            count += 1;
            let p = params.monetary_material_at_v1(row.accepted_daa).ok_or_else(|| bad("covenant before its gate"))?;
            p.validate().map_err(bad)?;
            if let Some(manifest) = &row.terms.material_manifest {
                if manifest.root != row.terms.root {
                    return Err(bad("material manifest obligation differs"));
                }
                manifest.validate(row.terms.max_material_bytes).map_err(|_| bad("signed material manifest capacity"))?;
            }
            if key.as_slice() != row.terms.root.as_byte_slice()
                || row.terms.policy != p.id()
                || row.terms.version != 1
                || row.terms.window_daa != p.window_daa
                || !row.terms.payment_relative_until_finality
                || row.terms.max_material_bytes == 0
                || row.terms.max_material_bytes > p.max_bytes_per_contract
                || row.service_fee_paid_sompi > row.service_fee_total_sompi
                || row.providers.iter().any(|p| p.fee_paid && !p.fee_queued)
                || row.providers.len() > PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
                || row.providers.is_empty()
                || row.terms.dependencies.len() > PALW_MONETARY_MATERIAL_MAX_DEPENDENCIES_V1
                || row.terms.dependencies.windows(2).any(|v| v[0] >= v[1])
                || row.challenges.len() > crate::palw_provider_court_v1::PALW_PROVIDER_MAX_OPEN_CHALLENGES_V1
                || row.tail_block.is_some() != row.tail_blue_score.is_some()
                || row.last_payment.as_ref().is_some_and(|receipt| receipt.amount == 0 || receipt.paid_daa < row.accepted_daa)
                || count > p.max_contracts as usize
            {
                return Err(bad("substituted terms, malformed tail, payment or capacity"));
            }
            for (receipt, entitlement) in &row.queued_entitlements {
                let material = self
                    .material_monetary_ledger_v1()
                    .and_then(|b| b.material_income.get(receipt))
                    .ok_or_else(|| bad("queued entitlement has no live material receipt"))?;
                let crate::palw_bond_budget_v1::PalwMaterialPaymentV1::Queued {
                    income: Some(income), queued_daa, paid_daa: None, ..
                } = material.payment
                else {
                    return Err(bad("queued entitlement is already paid or lacks its exact native income"));
                };
                if crate::palw_bond_budget_v1::material_queue_receipt_id_v1(*entitlement, income, queued_daa) != *receipt
                    || material.root.is_none_or(|r| !self.monetary_material_scope_v1(&row).contains(&r))
                {
                    return Err(bad("queued entitlement preimage or root was substituted"));
                }
            }
            let distinct: std::collections::BTreeSet<_> = row.providers.iter().map(|p| p.provider).collect();
            if distinct.len() != row.providers.len() {
                return Err(bad("duplicate provider collateral"));
            }
            for challenge in &row.challenges {
                if challenge.native_missing.is_some() != challenge.native_binding.is_some()
                    || (challenge.native_missing.is_some()
                        && (!self.monetary_material_scope_v1(&row).contains(&challenge.dependency)
                            || challenge.stage != 255
                            || challenge.segment_part.is_some()))
                    || (challenge.segment_part.is_some() && challenge.stage != 0)
                {
                    return Err(bad("material court unit fields disagree"));
                }
            }
        }
        Ok(())
    }
}
impl Params {
    pub fn validate_palw_monetary_material_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(f) = self.palw_monetary_material_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        f.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        if !self.palw_material_income_v3.is_some_and(|a| a != ForkActivation::never() && a.daa_score() <= f.activation.daa_score())
            || !self.palw_provider_court_v1.is_some_and(|a| a != ForkActivation::never() && a.daa_score() <= f.activation.daa_score())
        {
            return Err(PalwModeV2Error::Invalid("monetary material requires the payment provenance and provider signature gates"));
        }
        Err(PalwModeV2Error::Invalid(
            "monetary material cannot be armed: event-relative time cost and recovery liveness approval remain pending",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }
    fn enrollment() -> PalwMonetaryMaterialEnrollmentV1 {
        PalwMonetaryMaterialEnrollmentV1 {
            terms: PalwMonetaryMaterialTermsV1 {
                version: 1,
                root: Hash64::from_bytes([1; 64]),
                dependencies: vec![Hash64::from_bytes([2; 64])],
                exec_plan_root: None,
                sponsor: bond(1),
                nonce: Hash64::from_bytes([3; 64]),
                policy: Hash64::from_bytes([4; 64]),
                max_material_bytes: 100,
                material_manifest: None,
                window_daa: 436,
                enrollment_until_daa: 1000,
                payment_relative_until_finality: true,
            },
            offers: vec![PalwMonetaryMaterialOfferV1 {
                provider: bond(2),
                collateral_sompi: 100,
                prepaid_fee_sompi: 10,
                signature: vec![1],
            }],
        }
    }
    #[test]
    fn consent_binds_all_payment_terms_and_replacement_has_no_signature_cycle() {
        let e = enrollment();
        let id = e.id();
        let mutations: [fn(&mut PalwMonetaryMaterialEnrollmentV1); 4] = [
            |e| e.terms.window_daa += 1,
            |e: &mut PalwMonetaryMaterialEnrollmentV1| e.terms.payment_relative_until_finality = false,
            |e: &mut PalwMonetaryMaterialEnrollmentV1| e.offers[0].prepaid_fee_sompi += 1,
            |e: &mut PalwMonetaryMaterialEnrollmentV1| e.terms.dependencies.clear(),
        ];
        for mutate in mutations {
            let mut changed = e.clone();
            mutate(&mut changed);
            assert_ne!(id, changed.id());
        }
        let mut signatures = e.clone();
        signatures.offers[0].signature = vec![9; 32];
        assert_eq!(id, signatures.id());
        assert_eq!(e, borsh::from_slice(&borsh::to_vec(&e).unwrap()).unwrap());
    }
}
