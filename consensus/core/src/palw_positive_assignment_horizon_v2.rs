//! Prospective calendar/data obligations for the INITIAL assigned TIR topology.
//! This is a deterministic, result-free quote, not a lease or fraud clearance.
//! Native intake must separately establish the closed ingress topology and own
//! the actual global/local reservations. Actual C/Finish/P recheck the same
//! original policy; a positive-row counter or caller TTL is never that owner.
use crate::{
    config::params::PositiveCourtPolicyV2,
    palw_positive_assignment_certification_v2::{AssignmentGateCClockErrorV2, AssignmentGateCClockV2},
    palw_positive_assignment_resources_v2::{
        AssignmentCaseFeeScheduleV2, AssignmentCaseMoneyQuoteV2, AssignmentDerivedResourcesV2, AssignmentResourceErrorV2,
    },
    palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError,
    assignment::{
        AssignmentAdmissionBundleV2,
        consents::{AssignmentProverConsentV2, AssignmentSourceConsentV2},
        policy::AssignmentPolicyV2,
    },
    identity::Digest,
    messages::SourceKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentHorizonErrorV2 {
    Contract(ContractError),
    Clock(AssignmentGateCClockErrorV2),
    Resources(AssignmentResourceErrorV2),
    Binding,
    Budget,
    Overflow,
    Window,
}
impl From<ContractError> for AssignmentHorizonErrorV2 {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
impl From<AssignmentGateCClockErrorV2> for AssignmentHorizonErrorV2 {
    fn from(e: AssignmentGateCClockErrorV2) -> Self {
        Self::Clock(e)
    }
}
impl From<AssignmentResourceErrorV2> for AssignmentHorizonErrorV2 {
    fn from(e: AssignmentResourceErrorV2) -> Self {
        Self::Resources(e)
    }
}

/// Canonical DATA retained with the native reservation/material preimage. No
/// field is supplied as an accepted native duty/receipt or a P success flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentHorizonSpecV2 {
    pub version: u16,
    pub revision: u16,
    pub latest_result_before_daa: u64,
    pub prover_build_before_daa: u64,
    pub court_before_daa: u64,
    pub no_draw_expiry_daa: u64,
    pub last_failed_attestation_daa: u64,
    pub latest_prosecution_daa: u64,
    pub source_before_daa: u64,
    pub prover_retention_until_daa: u64,
    pub liability_until_daa: u64,
    pub refund_before_daa: u64,
    pub retention_until_daa: u64,
    pub maximum_distinct_p_readers: u16,
    pub maximum_prover_readers: u32,
    pub minimum_source_rounds: u64,
    pub minimum_unique_source_bytes: u64,
    pub minimum_transmitted_source_bytes: u64,
    pub record_bytes_all_attempts: u64,
    pub case_money: AssignmentCaseMoneyQuoteV2,
    pub positive_policy: Digest,
}
fn add(a: u64, b: u64) -> Result<u64, AssignmentHorizonErrorV2> {
    a.checked_add(b).ok_or(AssignmentHorizonErrorV2::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, AssignmentHorizonErrorV2> {
    a.checked_mul(b).ok_or(AssignmentHorizonErrorV2::Overflow)
}
fn ceil(n: u64, d: u64) -> Result<u64, AssignmentHorizonErrorV2> {
    if d == 0 {
        return Err(AssignmentHorizonErrorV2::Budget);
    }
    Ok(n / d + u64::from(!n.is_multiple_of(d)))
}
/// First DAA at which the entire quoted liability and retention envelope
/// can be closed. P release requires a DAA strictly after the last verdict.
fn assignment_refund_start_v2(
    clock: &AssignmentGateCClockV2,
    latest_prosecution: u64,
    liability_until: u64,
    source_before: u64,
    source_retention: u64,
    prover_retention: u64,
) -> Result<u64, AssignmentHorizonErrorV2> {
    let required = clock
        .no_draw_expiry_daa
        .max(clock.latest_failed_attestation_decision_daa)
        .max(add(latest_prosecution, 1)?)
        .max(liability_until)
        .max(source_before)
        .max(prover_retention);
    if source_retention < required {
        return Err(AssignmentHorizonErrorV2::Window);
    }
    Ok(source_retention)
}
impl AssignmentHorizonSpecV2 {
    /// Pure quote. The native factory must resolve certification from the
    /// actual original Params (not choose a shorter one), prove the INITIAL
    /// closed-ingress topology, and reserve all capacities/funds before using
    /// this data to admit A. Valid future C also rejects any legacy alias or
    /// unknown fraud topology; all checked Court events end at court_before.
    #[allow(clippy::too_many_arguments)]
    pub fn derive(
        bundle: &AssignmentAdmissionBundleV2,
        source: &AssignmentSourceConsentV2,
        prover: &AssignmentProverConsentV2,
        assignment: &AssignmentPolicyV2,
        positive: &PositiveCourtPolicyV2,
        certification: &PalwSecurityCertificationPolicyV1,
        case_fees: &AssignmentCaseFeeScheduleV2,
        resources: &AssignmentDerivedResourcesV2,
        max_source_units_per_request: u64,
        now: u64,
    ) -> Result<Self, AssignmentHorizonErrorV2> {
        assignment.check_terms(&bundle.terms, now)?;
        bundle.validate_shape()?;
        source.check(bundle)?;
        prover.check(bundle, assignment.positive_policy)?;
        positive.validate_shape().map_err(|_| AssignmentHorizonErrorV2::Binding)?;
        case_fees.validate()?;
        if resources.binding().terms() != &bundle.terms
            || bundle.job_context != Digest(resources.binding().job().context_hash().as_bytes())
            || bundle.source_catalog != resources.source_quote().digest
            || assignment.positive_policy.0 != positive.root().as_bytes()
            || assignment.case_fee_schedule != case_fees.id()
            || positive.funding_schedule_id.as_bytes() != case_fees.id().0
            || source.budget.witness_attempts != positive.max_attempts
            || prover.max_attempts != positive.max_attempts
        {
            return Err(AssignmentHorizonErrorV2::Binding);
        }
        let terms = &bundle.terms;
        // Inclusive upper bound deliberately includes the exclusive Result
        // cutoff: no legal later Result can require a longer envelope.
        let clock = AssignmentGateCClockV2::from_existing_policy(certification, terms.result_before_daa)?;
        if prover.build_before_daa > add(terms.result_before_daa, assignment.build_window_daa)? {
            return Err(AssignmentHorizonErrorV2::Window);
        }
        let court_before = add(prover.build_before_daa, assignment.check_window_daa)?;
        if court_before > bundle.future_case.service_before_daa {
            return Err(AssignmentHorizonErrorV2::Window);
        }
        let latest_prosecution = clock.prosecution_not_before_daa(court_before)?;
        // This numeric tail is an ORIGINAL signed liability rule, not the
        // pending-fraud proof. A real closed-ingress owner is mandatory.
        let liability_until = add(court_before, assignment.fraud_tail_daa)?;
        let service_need = court_before.max(clock.latest_failed_attestation_decision_daa);
        if source.service_before_daa < service_need {
            return Err(AssignmentHorizonErrorV2::Window);
        }
        let refund_start = assignment_refund_start_v2(
            &clock,
            latest_prosecution,
            liability_until,
            source.service_before_daa,
            source.budget.retention_until,
            prover.retention_until_daa,
        )?;
        let refund_before = add(refund_start, assignment.refund_window_daa)?;
        // The retention cap includes the whole refund window after the signed
        // source duty. Native rollback/custody must cover its own tail as well.
        let retention_until = refund_before;
        if retention_until.checked_sub(now).is_none_or(|n| n > positive.max_retention_daa) || retention_until > terms.reserve_until_daa
        {
            return Err(AssignmentHorizonErrorV2::Window);
        }
        let prover_readers = assignment.max_prover_joins.checked_add(1).ok_or(AssignmentHorizonErrorV2::Overflow)?;
        let readers = add(u64::from(prover_readers), u64::from(certification.m))?;
        let catalog = resources.source_quote();
        let ranges = add(
            ceil(resources.source().source_units(SourceKind::ArtifactInventory), max_source_units_per_request)?,
            ceil(resources.source().source_units(SourceKind::Input), max_source_units_per_request)?,
        )?;
        let minimum_source_rounds = mul(ranges, readers)?;
        let minimum_transmitted = mul(catalog.opening_bytes, readers)?;
        if source.budget.rounds < minimum_source_rounds
            || source.budget.unique_source_bytes < catalog.data_bytes
            || source.budget.transmitted_bytes < minimum_transmitted
            || source.budget.retained_bytes < catalog.data_bytes
        {
            return Err(AssignmentHorizonErrorV2::Budget);
        }
        let record_bytes_all_attempts = mul(
            misaka_palw_positive_contract::record::wire_size(resources.binding().job().exact_decode_tokens)?,
            u64::from(positive.max_attempts),
        )?;
        if prover.max_submitted_bytes < add(resources.all_attempt_cost().encoded_bytes, record_bytes_all_attempts)?
            || prover.max_retained_bytes < catalog.data_bytes
        {
            return Err(AssignmentHorizonErrorV2::Budget);
        }
        let case_money = case_fees.quote(
            &catalog,
            resources.all_attempt_relations(),
            record_bytes_all_attempts,
            source.budget.retained_bytes.max(positive.max_case_journal_bytes),
        )?;
        if case_money.fee > bundle.future_case.max_case_fee
            || case_money.capital > bundle.future_case.max_case_capital
            || add(case_money.fee, case_money.capital)? > bundle.future_case.max_case_locked
        {
            return Err(AssignmentHorizonErrorV2::Budget);
        }
        Ok(Self {
            version: 2,
            revision: 1,
            latest_result_before_daa: terms.result_before_daa,
            prover_build_before_daa: prover.build_before_daa,
            court_before_daa: court_before,
            no_draw_expiry_daa: clock.no_draw_expiry_daa,
            last_failed_attestation_daa: clock.latest_failed_attestation_decision_daa,
            latest_prosecution_daa: latest_prosecution,
            source_before_daa: source.service_before_daa,
            prover_retention_until_daa: prover.retention_until_daa,
            liability_until_daa: liability_until,
            refund_before_daa: refund_before,
            retention_until_daa: retention_until,
            maximum_distinct_p_readers: certification.m,
            maximum_prover_readers: prover_readers,
            minimum_source_rounds,
            minimum_unique_source_bytes: catalog.data_bytes,
            minimum_transmitted_source_bytes: minimum_transmitted,
            record_bytes_all_attempts,
            case_money,
            positive_policy: assignment.positive_policy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refund_window_starts_after_the_signed_retention_and_last_gate_c_verdict() {
        let policy = PalwSecurityCertificationPolicyV1 {
            m: 3,
            k: 2,
            attest_window_daa: 5,
            certify_by_daa: 7,
            prosecution_window_daa: 20,
            draws_per_epoch: 4,
            epoch_daa: 11,
            ..Default::default()
        };
        let clock = AssignmentGateCClockV2::from_existing_policy(&policy, 100).unwrap();
        let latest = clock.prosecution_not_before_daa(130).unwrap();
        assert_eq!(latest, 150);
        assert_eq!(assignment_refund_start_v2(&clock, latest, 140, 132, 150, 144), Err(AssignmentHorizonErrorV2::Window));
        assert_eq!(assignment_refund_start_v2(&clock, latest, 140, 132, 151, 144), Ok(151));
        assert_eq!(assignment_refund_start_v2(&clock, latest, 140, 132, 180, 144), Ok(180));
        assert_eq!(assignment_refund_start_v2(&clock, latest, 140, 132, 180, 181), Err(AssignmentHorizonErrorV2::Window));
        assert_eq!(assignment_refund_start_v2(&clock, u64::MAX, 140, 132, u64::MAX, 144), Err(AssignmentHorizonErrorV2::Overflow));
    }
}
