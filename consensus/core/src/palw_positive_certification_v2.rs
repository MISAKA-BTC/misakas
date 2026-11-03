//! Native P-subject data and bounded codec. Only selected-verifier approval
//! under the adopted honest-stake premise (f<=0.25) authenticates Gate C. Neither
//! signatures, hashes, stored status nor matching local replay prove computation
//! or authorize a payment. Old S/K/V/PublicCheck/Reproduced are never decoded here.
use crate::{
    palw_kernel_route_v1::palw_kernel_bond_id_v1,
    palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError, assignment::AssignmentId, certification::*, identity::*, rows::*,
};
pub const POSITIVE_CERTIFICATION_TABLE_V2: u8 = 136;
pub const POSITIVE_CERTIFICATION_PREFIX_V2: u8 = b'P';
pub const MAX_POSITIVE_DRAW_POSITIONS_V2: u16 = 75;
pub const MAX_POSITIVE_CERTIFICATION_ROW_BYTES_V2: usize = 1 << 20;
pub(crate) fn p_hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut w = Writer(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(domain)
            .to_state(),
    );
    value
        .serialize(&mut w)
        .expect("canonical P data serializes");
    Digest(w.0.finalize().as_bytes().try_into().expect("64"))
}
pub fn positive_certification_key_v2(subject: Digest) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(POSITIVE_CERTIFICATION_PREFIX_V2);
    key.extend_from_slice(&subject.0);
    key
}
/// Point lookup in the shared S/K/V/N/P table. Own the temporary key
/// before allocation; the returned bytes remain borrowed from rooted state.
pub(crate) fn positive_certification_row_bytes_v2(
    state: &PalwChainStateV2,
    subject: Digest,
) -> Result<Option<&[u8]>, crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::{
        NativeCoverageV1, TirNativeRequestsV1, reserve_native_requests_v1,
    };

    let Some(route) = state.kernel_route_state_v1() else {
        return Ok(None);
    };
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<u8>(65)?;
    let _key_lease = reserve_native_requests_v1(requests.bytes_v1())?.ok_or_else(|| {
        NativeCoverageV1::unsupported::<Vec<u8>>("P row lookup", "actual checker is not armed")
    })?;
    let key = (
        POSITIVE_CERTIFICATION_TABLE_V2,
        positive_certification_key_v2(subject),
    );
    Ok(route.aux.get(&key).map(Vec::as_slice))
}
/// Live Result commits with a protected draw slot on this branch.
pub fn undrawn_positive_certification_subjects_v2(
    state: &PalwChainStateV2,
    now: u64,
) -> impl Iterator<Item = Digest> + '_ {
    state
        .kernel_route_state_v1()
        .into_iter()
        .flat_map(move |route| route.undrawn_positive_certification_subjects_v2(now))
        .map(|subject| Digest(subject.as_bytes()))
}

/// Subjects whose selected-parent history is needed for a possible decision.
pub fn ready_positive_certification_subjects_v2(
    state: &PalwChainStateV2,
    now: u64,
) -> impl Iterator<Item = Digest> + '_ {
    state
        .kernel_route_state_v1()
        .into_iter()
        .flat_map(move |route| route.ready_positive_certification_subjects_v2(now))
        .map(|subject| Digest(subject.as_bytes()))
}

pub fn ready_positive_certification_case_v2(
    state: &PalwChainStateV2,
    subject: Digest,
    now: u64,
) -> Result<Option<(CaseId, ResultId)>, crate::palw_state_v2::PalwStateV2Error> {
    use crate::palw_state_v2::PalwStateV2Error as Error;
    let Some(header) = live_positive_certification_header_v2(state, subject, true)? else { return Ok(None) };
    let bundle = crate::palw_positive_rows_v2::PositiveRowsV2::new(state)
        .bundle(header.claim)
        .map_err(|_| Error::DeltaMismatch("scheduled Positive certification Case missing"))?;
    check_positive_certification_result_links_v2(&header, &bundle.result, &bundle.case)
        .map_err(|_| Error::DeltaMismatch("scheduled Positive certification Case differs"))?;
    Ok((bundle.case.maturity_daa <= now).then_some((header.case, header.result)))
}

pub fn undrawn_positive_certification_case_v2(
    state: &PalwChainStateV2,
    subject: Digest,
) -> Result<Option<(CaseId, ResultId)>, crate::palw_state_v2::PalwStateV2Error> {
    Ok(live_positive_certification_header_v2(state, subject, false)?.map(|header| (header.case, header.result)))
}

fn live_positive_certification_header_v2(
    state: &PalwChainStateV2,
    subject: Digest,
    needs_finish: bool,
) -> Result<Option<PositiveCertificationHeaderV2>, crate::palw_state_v2::PalwStateV2Error> {
    use crate::palw_state_v2::PalwStateV2Error as Error;
    let bytes = positive_certification_row_bytes_v2(state, subject)
        .map_err(Error::NativeResource)?
        .ok_or(Error::DeltaMismatch(
            "scheduled Positive certification row missing",
        ))?;
    let fixed = PositiveCertificationFixedV2::deserialize(&mut &bytes[..])
        .map_err(|_| Error::DeltaMismatch("scheduled Positive certification row malformed"))?;
    if fixed.outcome.is_some() || (needs_finish && fixed.finality.is_none()) {
        return Ok(None);
    }
    Ok(Some(fixed.header))
}

pub fn positive_certification_policy_id_v2(policy: &PalwSecurityCertificationPolicyV1) -> Digest {
    p_hash(b"misaka/positive/certification-policy/v2/r1", policy)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PositiveCertificationOutcomeV2 {
    Certified,
    NoPopulation,
    Expired,
    NotCertified,
    Vetoed,
    Withdrawn,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationHeaderV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: AssignmentId,
    pub subject: Digest,
    pub claim: ClaimId,
    pub result: ResultId,
    pub case: CaseId,
    pub scope: ChainScope,
    pub policy: PalwSecurityCertificationPolicyV1,
    pub policy_id: Digest,
    pub commit_origin: AcceptedOrigin,
    pub committed_span: Option<u64>,
    pub worker: NativeOutpoint,
    pub worker_operator: Digest,
    pub worker_key: [u8; PUBKEY_BYTES],
    pub worker_gain: u64,
    pub liability: AcceptedMaterial,
}
impl PositiveCertificationHeaderV2 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.version != 2 || self.revision != 1 {
            return Err(ContractError::Version);
        }
        self.policy.validate().map_err(|_| ContractError::Range)?;
        if self.policy.m > MAX_POSITIVE_DRAW_POSITIONS_V2
            || self.policy_id != positive_certification_policy_id_v2(&self.policy)
            || self
                .commit_origin
                .accepted_daa
                .checked_add(self.policy.certify_by_daa)
                .and_then(|n| n.checked_add(self.policy.attest_window_daa))
                .and_then(|n| n.checked_add(1))
                .is_none()
        {
            return Err(ContractError::Range);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationFinalV2 {
    pub origin: AcceptedOrigin,
    pub witness: WitnessId,
    pub material: AcceptedMaterial,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationDrawOriginV2 {
    pub block: NativeHash64,
    pub daa: u64,
    pub blue_score: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationLiabilityReleaseV2 {
    pub point: PositiveCertificationDrawOriginV2,
    pub closed_material: AcceptedMaterial,
    pub latest_decision_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationFixedV2 {
    pub header: PositiveCertificationHeaderV2,
    pub finality: Option<PositiveCertificationFinalV2>,
    pub outcome: Option<(PositiveCertificationOutcomeV2, u64)>,
    /// Only a concrete native closed-liability owner may install this receipt.
    /// A failed/no-population verdict alone never releases attestor collateral.
    pub liability_release: Option<PositiveCertificationLiabilityReleaseV2>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationDrawMetaV2 {
    pub roster: Digest,
    pub roster_count: u64,
    pub roster_stake: u128,
    pub seed: Digest,
    pub anchor_span: u64,
    pub anchor_block: NativeHash64,
    pub execution_key: NativeHash64,
    pub origin: PositiveCertificationDrawOriginV2,
    pub deadline_daa: u64,
    pub id: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationDrawV2 {
    pub meta: PositiveCertificationDrawMetaV2,
    pub positions: Vec<Digest>,
}
impl PositiveCertificationDrawV2 {
    pub fn expected_id(&self, header: &PositiveCertificationHeaderV2) -> Digest {
        p_hash(
            b"misaka/positive/certification-draw/v2/r1",
            &(
                header,
                self.meta.roster,
                self.meta.roster_count,
                self.meta.roster_stake,
                self.meta.seed,
                self.meta.anchor_span,
                self.meta.anchor_block,
                self.meta.execution_key,
                self.meta.origin,
                self.meta.deadline_daa,
                &self.positions,
            ),
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationVoteV2 {
    pub bond: NativeOutpoint,
    pub key: [u8; PUBKEY_BYTES],
    pub statement: PositiveCourtAttestationV2,
    pub signature: [u8; SIGNATURE_BYTES],
    pub origin: AcceptedOrigin,
    pub liability_amount: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCertificationRowV2 {
    pub fixed: PositiveCertificationFixedV2,
    pub draw: Option<PositiveCertificationDrawV2>,
    pub votes: Vec<PositiveCertificationVoteV2>,
}
impl PositiveCertificationRowV2 {
    pub fn positive_positions(&self) -> u32 {
        self.positions_with(PositiveCourtVerdictV2::PositiveChecked)
    }
    pub fn negative_positions(&self) -> u32 {
        self.positions_with(PositiveCourtVerdictV2::NotReproducible)
    }
    fn positions_with(&self, verdict: PositiveCourtVerdictV2) -> u32 {
        self.draw.as_ref().map_or(0, |d| {
            d.positions
                .iter()
                .filter(|bond| {
                    self.votes.iter().any(|v| {
                        v.statement.verifier_bond == **bond && v.statement.verdict == verdict
                    })
                })
                .count() as u32
        })
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        let h = &self.fixed.header;
        h.validate()?;
        if self.votes.len() > usize::from(h.policy.m) {
            return Err(ContractError::Budget);
        }
        if let Some(finality) = self.fixed.finality
            && (finality.origin.accepted_daa < h.commit_origin.accepted_daa
                || finality.material.origin != finality.origin)
        {
            return Err(ContractError::Identity);
        }
        match &self.draw {
            None => {
                if !self.votes.is_empty() {
                    return Err(ContractError::Identity);
                }
            }
            Some(draw) => {
                if draw.positions.len() > usize::from(h.policy.m)
                    || draw.meta.id != draw.expected_id(h)
                    || draw.meta.origin.daa < h.commit_origin.accepted_daa
                    || draw.meta.origin.daa
                        > h.commit_origin
                            .accepted_daa
                            .checked_add(h.policy.certify_by_daa)
                            .ok_or(ContractError::Range)?
                    || draw.meta.deadline_daa
                        != draw
                            .meta
                            .origin
                            .daa
                            .checked_add(h.policy.attest_window_daa)
                            .ok_or(ContractError::Range)?
                    || draw.meta.anchor_span
                        < h.committed_span
                            .unwrap_or(0)
                            .checked_add(h.policy.anchor_delay_spans.max(1))
                            .ok_or(ContractError::Range)?
                    || (draw.meta.roster_count == 0) != (draw.positions.is_empty())
                    || (draw.meta.roster_count == 0) != (draw.meta.roster_stake == 0)
                    || draw.meta.seed
                        != Digest(
                            crate::palw_security_certification_v1::palw_certification_seed_v1(
                                &h.subject.0,
                                draw.meta.anchor_span,
                                &draw.meta.execution_key.0,
                            ),
                        )
                    || (!draw.positions.is_empty()
                        && draw.positions.len() != usize::from(h.policy.m))
                {
                    return Err(ContractError::Identity);
                }
                for (i, vote) in self.votes.iter().enumerate() {
                    let id = Digest(palw_kernel_bond_id_v1(&PalwBondKeyV2(
                        crate::tx::TransactionOutpoint::new(
                            crate::TransactionId::from_bytes(vote.bond.transaction.0),
                            vote.bond.index,
                        ),
                    )));
                    if id != vote.statement.verifier_bond
                        || !draw.positions.contains(&id)
                        || self.votes[..i]
                            .iter()
                            .any(|v| v.statement.verifier_bond == id)
                        || principal(&vote.key) != vote.statement.verifier
                        || vote.statement.scope != h.scope
                        || vote.statement.subject != h.subject
                        || vote.statement.result != h.result
                        || vote.statement.case != h.case
                        || vote.statement.claim != h.claim
                        || vote.statement.policy != h.policy_id
                        || vote.statement.draw_anchor != draw.meta.id
                        || vote.origin.accepted_daa < draw.meta.origin.daa
                        || vote.origin.accepted_daa > draw.meta.deadline_daa
                        || (vote.statement.verdict != PositiveCourtVerdictV2::PositiveChecked
                            && vote.liability_amount != 0)
                    {
                        return Err(ContractError::Identity);
                    }
                    if vote.statement.verdict == PositiveCourtVerdictV2::PositiveChecked
                        && vote.liability_amount
                            != crate::palw_g14_lane_closure_v1::palw_kernel_signer_lock_amount_v1(
                                h.worker_gain,
                                h.policy.k as u8,
                            )
                    {
                        return Err(ContractError::Identity);
                    }
                    vote.statement.validate_shape()?;
                    if let Some(finality) = self.fixed.finality {
                        if vote.statement.witness != finality.witness
                            || vote.statement.checked_material != finality.material.digest
                        {
                            return Err(ContractError::Identity);
                        }
                    } else {
                        return Err(ContractError::Identity);
                    }
                }
            }
        }
        if self
            .fixed
            .outcome
            .is_some_and(|(o, _)| o == PositiveCertificationOutcomeV2::Certified)
            && (self.fixed.finality.is_none() || self.positive_positions() < u32::from(h.policy.k))
        {
            return Err(ContractError::Identity);
        }
        if let Some(release) = self.fixed.liability_release
            && (release.point.daa <= release.latest_decision_daa
                || release.point.daa < h.commit_origin.accepted_daa)
        {
            return Err(ContractError::Range);
        }
        if let Some((outcome, at)) = self.fixed.outcome {
            match outcome {
                PositiveCertificationOutcomeV2::Certified => {
                    let finality = self.fixed.finality.ok_or(ContractError::Identity)?;
                    if at
                        < finality
                            .origin
                            .accepted_daa
                            .checked_add(h.policy.prosecution_window_daa)
                            .ok_or(ContractError::Range)?
                    {
                        return Err(ContractError::Range);
                    }
                }
                PositiveCertificationOutcomeV2::NoPopulation => {
                    if self
                        .draw
                        .as_ref()
                        .is_none_or(|d| d.meta.roster_count != 0 || !d.positions.is_empty())
                    {
                        return Err(ContractError::Identity);
                    }
                }
                PositiveCertificationOutcomeV2::Expired => {
                    if self.draw.is_some()
                        || at
                            <= h.commit_origin
                                .accepted_daa
                                .checked_add(h.policy.certify_by_daa)
                                .ok_or(ContractError::Range)?
                    {
                        return Err(ContractError::Identity);
                    }
                }
                PositiveCertificationOutcomeV2::NotCertified => {
                    if self.positive_positions() >= u32::from(h.policy.k)
                        || self.draw.as_ref().is_none_or(|d| at <= d.meta.deadline_daa)
                    {
                        return Err(ContractError::Identity);
                    }
                }
                PositiveCertificationOutcomeV2::Vetoed => {
                    if h.policy.veto == 0 || self.negative_positions() < u32::from(h.policy.veto) {
                        return Err(ContractError::Identity);
                    }
                }
                PositiveCertificationOutcomeV2::Withdrawn => {}
            }
        }
        Ok(())
    }
    /// Literal signature/Result binding diagnostics; accepted ancestry, native
    /// registry/pool provenance and complete current Session remain required.
    pub fn authenticate_votes(
        &self,
        result: &misaka_palw_positive_contract::messages::MinerResultCommit,
    ) -> Result<(), ContractError> {
        self.validate_shape()?;
        let h = &self.fixed.header;
        if h.subject != misaka_palw_positive_contract::native::certification_subject(result)
            || h.result != result.id()
            || h.case != result.case_id()
            || h.claim != result.claim_id()
            || h.scope != result.scope
        {
            return Err(ContractError::Identity);
        }
        for vote in &self.votes {
            let draw = self.draw.as_ref().ok_or(ContractError::Identity)?;
            let finality = self.fixed.finality.ok_or(ContractError::Identity)?;
            vote.statement.authenticate(
                result,
                &PositiveAttestationExpectationsV2 {
                    scope: h.scope,
                    policy: h.policy_id,
                    draw_anchor: draw.meta.id,
                    witness: finality.witness,
                    verifier_bond: vote.statement.verifier_bond,
                    verifier: principal(&vote.key),
                    checked_material: finality.material.digest,
                },
                &vote.key,
                &vote.signature,
            )?;
        }
        Ok(())
    }
    pub fn decode(key: &[u8], raw: &[u8]) -> Result<Self, ContractError> {
        if key.len() != 65 || key[0] != b'P' || raw.len() > MAX_POSITIVE_CERTIFICATION_ROW_BYTES_V2
        {
            return Err(ContractError::Budget);
        }
        crate::palw_native_resource_v1::preflight_native_value_v1::<Self>(raw, raw.len() as u64)
            .map_err(|_| ContractError::NonCanonical)?;
        let row = borsh::from_slice::<Self>(raw).map_err(|_| ContractError::NonCanonical)?;
        row.validate_shape()?;
        if key[1..] != row.fixed.header.subject.0 {
            return Err(ContractError::Identity);
        }
        Ok(row)
    }
}
/// Only an exact Case can terminate its P subject. A different Result can
/// share the CaseId, since CaseId does not bind output.
pub(crate) fn positive_certification_case_live_v2(
    header: &PositiveCertificationHeaderV2,
    case: &CaseRow,
) -> Result<bool, ContractError> {
    if case.case != header.case
        || case.result != header.result
        || case.coverage.scope != header.scope
        || case.liability != header.liability
    {
        return Err(ContractError::Identity);
    }
    Ok(!positive_case_ends_certification_v2(case))
}

pub(crate) fn positive_case_ends_certification_v2(case: &CaseRow) -> bool {
    use misaka_palw_positive_contract::messages::ClaimStatus;
    matches!(
        case.status,
        ClaimStatus::ClaimRefuted | ClaimStatus::Revoked | ClaimStatus::EndedUnverified
    )
}

/// Bind the certification header to the original accepted Result row and
/// its current Case. The Case status is independent of identity here.
pub(crate) fn check_positive_certification_result_links_v2(
    header: &PositiveCertificationHeaderV2,
    result: &ResultRow,
    case: &CaseRow,
) -> Result<(), ContractError> {
    positive_certification_case_live_v2(header, case)?;
    let commit = &result.result;
    if header.commit_origin != result.origin
        || header.assignment.0 != result.role_assignment.digest
        || header.worker_key != result.miner_key
        || header.subject != misaka_palw_positive_contract::native::certification_subject(commit)
        || header.claim != commit.claim_id()
        || header.result != commit.id()
        || header.case != commit.case_id()
        || header.scope != commit.scope
    {
        return Err(ContractError::Identity);
    }
    Ok(())
}

/// Failure-only pure projection. Positive certification requires the concrete
/// current native checked/closed-fraud session and is never returned here.
pub fn positive_certification_failure_v2(
    row: &PositiveCertificationRowV2,
    now: u64,
    current_case_live: bool,
) -> Option<PositiveCertificationOutcomeV2> {
    use PositiveCertificationOutcomeV2 as O;
    if !current_case_live {
        return (!row.fixed.outcome.is_some_and(|(o, _)| o == O::Withdrawn))
            .then_some(O::Withdrawn);
    }
    if row.fixed.outcome.is_some() {
        return None;
    }
    let h = &row.fixed.header;
    if row.draw.is_none() {
        return h
            .commit_origin
            .accepted_daa
            .checked_add(h.policy.certify_by_daa)
            .filter(|deadline| now > *deadline)
            .map(|_| O::Expired);
    }
    let draw = row.draw.as_ref()?;
    if draw.meta.roster_count == 0 {
        return Some(O::NoPopulation);
    }
    if h.policy.veto > 0 && row.negative_positions() >= u32::from(h.policy.veto) {
        return Some(O::Vetoed);
    }
    // Preserve actual existing k-positive semantics: pending native fraud or a
    // not-yet-known Final does not become deadline failure once k is present.
    if row.positive_positions() >= u32::from(h.policy.k) {
        return None;
    }
    (now > draw.meta.deadline_daa).then_some(O::NotCertified)
}

/// Allocation-free collateral projection. Malformed P state is maximally
/// restrictive, never an empty permissive lock map; normal ingress rejects it.
/// Borsh fixed pieces are lent one at a time, and both Vec extents are bounded
/// BEFORE traversal. No public saved-status query authorizes computation credit.
pub fn positive_verifier_liability_reserved_v2(
    state: &PalwChainStateV2,
    bond: &PalwBondKeyV2,
) -> u128 {
    let Some(route) = state.kernel_route_state_v1() else {
        return 0;
    };
    let wanted = palw_kernel_bond_id_v1(bond);
    let mut total = 0u128;
    for ((table, key), raw) in route.aux.range((136, Vec::new())..(137, Vec::new())) {
        if key.first() != Some(&b'P') {
            continue;
        }
        if *table != 136 || key.len() != 65 || raw.len() > MAX_POSITIVE_CERTIFICATION_ROW_BYTES_V2 {
            return u128::MAX;
        }
        let mut bytes = raw.as_slice();
        let Ok(fixed) = PositiveCertificationFixedV2::deserialize(&mut bytes) else {
            return u128::MAX;
        };
        if fixed.header.validate().is_err() || key[1..] != fixed.header.subject.0 {
            return u128::MAX;
        }
        let Ok(tag) = u8::deserialize(&mut bytes) else {
            return u128::MAX;
        };
        if tag == 1 {
            if PositiveCertificationDrawMetaV2::deserialize(&mut bytes).is_err() {
                return u128::MAX;
            }
            let Ok(n) = u32::deserialize(&mut bytes) else {
                return u128::MAX;
            };
            if n > u32::from(fixed.header.policy.m) {
                return u128::MAX;
            }
            let Some(rest) = bytes.get(n as usize * 64..) else {
                return u128::MAX;
            };
            bytes = rest;
        } else if tag != 0 {
            return u128::MAX;
        }
        let Ok(n) = u32::deserialize(&mut bytes) else {
            return u128::MAX;
        };
        if n > u32::from(fixed.header.policy.m) {
            return u128::MAX;
        }
        for _ in 0..n {
            let Ok(vote) = PositiveCertificationVoteV2::deserialize(&mut bytes) else {
                return u128::MAX;
            };
            if vote.statement.verifier_bond.0 == wanted && fixed.liability_release.is_none() {
                total = match total.checked_add(u128::from(vote.liability_amount)) {
                    Some(n) => n,
                    None => return u128::MAX,
                };
            }
        }
        if !bytes.is_empty() {
            return u128::MAX;
        }
    }
    total
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositivePSourceErrorV2 {
    Unavailable,
    Binding,
    NotDrawn,
    Closed,
    Deadline,
    Native(crate::palw_native_resource_v1::NativeCoverageV1),
}
/// New-request entitlement data only. The actual native source session owns its
/// generation/accepted-prefix check and aggregate C ledger. Existing responses
/// keep the ORIGINAL source-service cutoff even after a P failure decision.
pub struct PositivePSourceEntitlementV2 {
    bond: NativeOutpoint,
    draw: Digest,
    request_before: u64,
    service_before: u64,
}
impl PositivePSourceEntitlementV2 {
    pub fn verifier_bond(&self) -> NativeOutpoint {
        self.bond
    }
    pub fn draw(&self) -> Digest {
        self.draw
    }
    pub fn request_before_daa(&self) -> u64 {
        self.request_before
    }
    pub fn service_before_daa(&self) -> u64 {
        self.service_before
    }
}
pub fn authorize_assigned_p_source_request_v2(
    state: &PalwChainStateV2,
    ctx: &crate::palw_state_v2::PalwBlockContextV2,
    historical_origin: AcceptedOrigin,
    result: &misaka_palw_positive_contract::messages::MinerResultCommit,
    requester: PrincipalId,
    key: &[u8; PUBKEY_BYTES],
) -> Result<PositivePSourceEntitlementV2, PositivePSourceErrorV2> {
    use crate::palw_positive_rows_v2::PositiveRowsV2;
    use misaka_palw_positive_contract::native::certification_subject;
    if historical_origin.accepting_block.0 != ctx.block.as_bytes()
        || historical_origin.accepted_daa != ctx.daa_score
        || historical_origin.accepted_blue_score != ctx.blue_score
        || principal(key) != requester
    {
        return Err(PositivePSourceErrorV2::Binding);
    }
    let subject = certification_subject(result);
    let bytes = positive_certification_row_bytes_v2(state, subject)
        .map_err(PositivePSourceErrorV2::Native)?
        .ok_or(PositivePSourceErrorV2::Unavailable)?;
    let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<PositiveCertificationRowV2>(bytes, bytes.len() as u64)
        .map_err(|_| PositivePSourceErrorV2::Binding)?;
    let scratch = (8 * std::mem::size_of::<NativeRowValue>() + 8 * MAX_NATIVE_KEY_BYTES * 2) as u64;
    let requests = shape
        .logical_requests_v1()
        .checked_add(scratch)
        .ok_or_else(|| {
            PositivePSourceErrorV2::Native(
                crate::palw_native_resource_v1::NativeCoverageV1::overflow::<NativeRowValue>(
                    "P source native row workspace",
                ),
            )
        })?;
    let _owner = crate::palw_native_resource_v1::reserve_native_requests_v1(requests)
        .map_err(PositivePSourceErrorV2::Native)?
        .ok_or(PositivePSourceErrorV2::Unavailable)?;
    let row = PositiveCertificationRowV2::decode(&rowkey, bytes)
        .map_err(|_| PositivePSourceErrorV2::Binding)?;
    let h = &row.fixed.header;
    if h.result != result.id()
        || h.claim != result.claim_id()
        || h.case != result.case_id()
        || h.scope != result.scope
    {
        return Err(PositivePSourceErrorV2::Binding);
    }
    if row.fixed.outcome.is_some() {
        return Err(PositivePSourceErrorV2::Closed);
    }
    let draw = row.draw.as_ref().ok_or(PositivePSourceErrorV2::NotDrawn)?;
    let bundle = PositiveRowsV2::new(state)
        .bundle(h.claim)
        .map_err(|_| PositivePSourceErrorV2::Unavailable)?;
    if &bundle.result.result != result || bundle.result.origin != h.commit_origin {
        return Err(PositivePSourceErrorV2::Binding);
    }
    if !positive_certification_case_live_v2(h, &bundle.case)
        .map_err(|_| PositivePSourceErrorV2::Binding)?
    {
        return Err(PositivePSourceErrorV2::Closed);
    }
    let Some(native_bond) = state
        .bonds_for_positive_p_source_v2()
        .find_map(|(bond, record)| {
            let point = Digest(palw_kernel_bond_id_v1(bond));
            (draw.positions.contains(&point) && record.pubkey.as_slice() == key.as_slice())
                .then_some(*bond)
        })
    else {
        return Err(PositivePSourceErrorV2::NotDrawn);
    };
    let request_before = draw
        .meta
        .deadline_daa
        .checked_add(1)
        .ok_or(PositivePSourceErrorV2::Deadline)?
        .min(bundle.case.service.deadline);
    if ctx.daa_score < draw.meta.origin.daa || ctx.daa_score >= request_before {
        return Err(PositivePSourceErrorV2::Deadline);
    }
    Ok(PositivePSourceEntitlementV2 {
        bond: NativeOutpoint {
            transaction: NativeHash64(native_bond.0.transaction_id.as_bytes()),
            index: native_bond.0.index,
        },
        draw: draw.meta.id,
        request_before,
        service_before: bundle.case.service.deadline,
    })
}
