//! Shared contracts for the public-materials profile. These values carry no
//! native admission, availability, verdict, or settlement authority.

use crate::ContractError;
use crate::identity::{ChainScope, Digest, PUBKEY_BYTES, PrincipalId, SIGNATURE_BYTES, hash, principal, verify_signature};
use crate::reservations::RemainingCapacity;
use borsh::{BorshDeserialize, BorshSerialize};

pub const ORIGINAL_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-original/v1";
pub const FULL_REPLAY_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-full-replay/v1";
pub const MANIFEST_REQUEST_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-manifest-request/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OriginalId(pub Digest);

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialId(pub Digest);

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CaseId(pub Digest);

/// The canonical identity native admission must resolve from the draw or
/// from admitted outsider eligibility before using a request's case ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Claimant {
    DrawnBond { transaction: Digest, index: u32 },
    Outsider(PrincipalId),
}

/// The version of this signed body is bound by its domain. The ruleset in
/// scope binds the execution profile and its parameters independently of any
/// future wire tag or activation height.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OriginalCommit {
    pub scope: ChainScope,
    pub job: Digest,
    pub execution: Digest,
    pub miner: PrincipalId,
    pub registered_class: Digest,
    pub artifact: Digest,
    pub program: Digest,
    pub prompt: Digest,
    pub input_tokens: Digest,
    pub tokenizer: Digest,
    pub execution_rules: Digest,
    pub generation_seed: [u8; 32],
    pub initial_state: Digest,
    pub dependencies: Digest,
    pub prefill: u32,
    pub decode: u32,
    pub max_context: u32,
    pub output: Digest,
    pub trace_root: Digest,
    pub manifest_root: Digest,
    pub service_contract: Digest,
}

impl OriginalCommit {
    pub fn id(&self) -> OriginalId {
        OriginalId(hash(b"misaka/palw/public-original-id/v1", self))
    }

    pub fn authenticate(&self, key: &[u8; PUBKEY_BYTES], signature: &[u8; SIGNATURE_BYTES]) -> Result<(), ContractError> {
        let positions =
            self.prefill.checked_add(self.decode.checked_sub(1).ok_or(ContractError::Range)?).ok_or(ContractError::Range)?;
        if self.prefill == 0 || positions > self.max_context {
            return Err(ContractError::Range);
        }
        authenticate_actor(self.miner, key, signature, self.id().0, ORIGINAL_SIGN_CONTEXT)
    }
}

/// A Panel member's assertion of complete replay under the rules bound by the
/// original. Native consumers must check that this member belongs to the
/// original's actual post-commit draw and independently enforce the Final gate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FullReplayApproval {
    pub original: OriginalId,
    pub draw: Digest,
    pub member: PrincipalId,
}

impl FullReplayApproval {
    pub fn signing_digest(&self) -> Digest {
        hash(b"misaka/palw/public-full-replay-id/v1", self)
    }

    pub fn authenticate(&self, key: &[u8; PUBKEY_BYTES], signature: &[u8; SIGNATURE_BYTES]) -> Result<(), ContractError> {
        authenticate_actor(self.member, key, signature, self.signing_digest(), FULL_REPLAY_SIGN_CONTEXT)
    }
}

/// Request the original's manifest itself, before a claimant knows any
/// chunk index or has obtained any execution data. This request covers the
/// canonical manifest entry point. The profile must supply its size cap and
/// the native rules for validating the inspection scope and reservation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ManifestRequest {
    pub original: OriginalId,
    pub manifest_root: Digest,
    pub claimant: Claimant,
    pub signer: PrincipalId,
    pub inspection_scope: Digest,
    pub supply_obligation: Digest,
    pub collect_until: u64,
    pub reservation: Digest,
}

/// Expected values read by the consumer from the accepted original, native
/// eligibility, and this claimant's booked service opportunity. It must never
/// be constructed from the submitted request.
pub struct ManifestRequestContext<'a> {
    pub original: &'a OriginalCommit,
    pub claimant: Claimant,
    pub signer: PrincipalId,
    pub inspection_scope: Digest,
    pub supply_obligation: Digest,
    pub reservation: Digest,
    pub schedule: VerificationSchedule,
    pub inclusion_daa: u64,
}

impl ManifestRequest {
    pub fn material_id(&self, original: &OriginalCommit) -> Result<MaterialId, ContractError> {
        if self.original != original.id() || self.manifest_root != original.manifest_root {
            return Err(ContractError::Scope);
        }
        Ok(MaterialId(hash(b"misaka/palw/public-manifest-material/v1", &(original.scope.ruleset, self.manifest_root))))
    }

    pub fn case_id(&self, original: &OriginalCommit, eligible_claimant: Claimant) -> Result<CaseId, ContractError> {
        if self.claimant != eligible_claimant {
            return Err(ContractError::Identity);
        }
        let material = self.material_id(original)?;
        Ok(CaseId(hash(b"misaka/palw/public-manifest-case/v1", &(self.original, eligible_claimant, material, self.inspection_scope))))
    }

    pub fn signing_digest(&self) -> Digest {
        hash(b"misaka/palw/public-manifest-request-id/v1", self)
    }

    pub fn authenticate(
        &self,
        eligible_signer: PrincipalId,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        if self.signer != eligible_signer {
            return Err(ContractError::Identity);
        }
        authenticate_actor(eligible_signer, key, signature, self.signing_digest(), MANIFEST_REQUEST_SIGN_CONTEXT)
    }

    /// Authenticate a candidate against its expected case and original
    /// schedule before the native fold installs the case and debits its
    /// reservation atomically.
    pub fn verify_for_inclusion(
        &self,
        context: &ManifestRequestContext<'_>,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<CaseId, ContractError> {
        let id = self.check_inclusion(context)?;
        self.authenticate(context.signer, key, signature)?;
        Ok(id)
    }

    fn check_inclusion(&self, context: &ManifestRequestContext<'_>) -> Result<CaseId, ContractError> {
        let id = self.case_id(context.original, context.claimant)?;
        if self.signer != context.signer {
            return Err(ContractError::Identity);
        }
        if self.inspection_scope != context.inspection_scope || self.supply_obligation != context.supply_obligation {
            return Err(ContractError::Scope);
        }
        if self.reservation != context.reservation {
            return Err(ContractError::Budget);
        }
        if !context.schedule.clock.accepts_first_notice(context.inclusion_daa) || self.collect_until != context.schedule.filed_by() {
            return Err(ContractError::Range);
        }
        Ok(id)
    }
}

fn authenticate_actor(
    actor: PrincipalId,
    key: &[u8; PUBKEY_BYTES],
    signature: &[u8; SIGNATURE_BYTES],
    id: Digest,
    context: &[u8],
) -> Result<(), ContractError> {
    if principal(key) != actor {
        return Err(ContractError::Identity);
    }
    if !verify_signature(key, id, signature, context) {
        return Err(ContractError::Signature);
    }
    Ok(())
}

/// Timing derived only from canonical original admission. On restore,
/// reconstruct it from that admission rather than trusting saved deadlines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalClock {
    accepted_at: u64,
}

impl OriginalClock {
    pub fn new(accepted_at: u64) -> Result<Self, ContractError> {
        accepted_at.checked_add(10_001).ok_or(ContractError::Range)?;
        Ok(Self { accepted_at })
    }

    pub fn notice_until(self) -> u64 {
        self.accepted_at + 3_120
    }

    pub fn hard(self) -> u64 {
        self.accepted_at + 10_000
    }

    pub fn accepts_first_notice(self, now: u64) -> bool {
        (self.accepted_at..=self.notice_until()).contains(&now)
    }

    pub fn accepts_case_evidence(self, now: u64, opened_at: u64, collect_until: u64) -> bool {
        self.accepts_first_notice(opened_at) && collect_until <= self.hard() && (opened_at..=collect_until).contains(&now)
    }

    /// This predicate covers only time. Final also requires the native
    /// verification, unresolved-case and dependency checks.
    pub fn permits_final_time(self, now: u64) -> bool {
        now > self.notice_until() && now <= self.hard()
    }

    pub fn is_past_hard(self, now: u64) -> bool {
        now > self.hard()
    }
}

/// Bounds for the path from a fixed draw and available material to the first
/// canonical notice. Bounds must come from the selected profile's service
/// reservations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstNoticeDemand {
    pub draw_ready_by: u64,
    pub material_ready_by: u64,
    pub cold_acquisition_daa: u64,
    pub full_inspection_daa: u64,
    pub notice_inclusion_daa: u64,
    pub missing_material_detected_by: u64,
    pub material_request_inclusion_daa: u64,
}

/// Worst-case path when a claimant opens a material case at the last legal
/// notice DAA. Filing includes evidence assembly and canonical inclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialCaseDemand {
    pub material_delivery_daa: u64,
    pub full_inspection_daa: u64,
    pub localization_daa: u64,
    pub filing_daa: u64,
    pub adjudication_and_recovery_daa: u64,
}

/// Timing contract only. The native admission path must also fund and book
/// both paths and preserve the per-claimant service and response capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerificationSchedule {
    clock: OriginalClock,
    first_notice_by: u64,
    first_material_request_by: u64,
    material_ready_by: u64,
    inspection_by: u64,
    localized_by: u64,
    filed_by: u64,
    terminal_by: u64,
}

impl VerificationSchedule {
    pub fn new(clock: OriginalClock, notice: FirstNoticeDemand, case: MaterialCaseDemand) -> Result<Self, ContractError> {
        if notice.draw_ready_by < clock.accepted_at
            || notice.material_ready_by < clock.accepted_at
            || notice.missing_material_detected_by < clock.accepted_at
        {
            return Err(ContractError::Range);
        }
        let first_notice_by = notice
            .draw_ready_by
            .max(notice.material_ready_by)
            .checked_add(notice.cold_acquisition_daa)
            .and_then(|daa| daa.checked_add(notice.full_inspection_daa))
            .and_then(|daa| daa.checked_add(notice.notice_inclusion_daa))
            .ok_or(ContractError::Range)?;
        let first_material_request_by = notice
            .draw_ready_by
            .max(notice.missing_material_detected_by)
            .checked_add(notice.material_request_inclusion_daa)
            .ok_or(ContractError::Range)?;
        let material_ready_by = clock.notice_until().checked_add(case.material_delivery_daa).ok_or(ContractError::Range)?;
        let inspection_by = material_ready_by.checked_add(case.full_inspection_daa).ok_or(ContractError::Range)?;
        let localized_by = inspection_by.checked_add(case.localization_daa).ok_or(ContractError::Range)?;
        let filed_by = localized_by.checked_add(case.filing_daa).ok_or(ContractError::Range)?;
        let terminal_by = filed_by.checked_add(case.adjudication_and_recovery_daa).ok_or(ContractError::Range)?;
        if first_notice_by.max(first_material_request_by) > clock.notice_until() || terminal_by > clock.hard() {
            return Err(ContractError::Budget);
        }
        Ok(Self {
            clock,
            first_notice_by,
            first_material_request_by,
            material_ready_by,
            inspection_by,
            localized_by,
            filed_by,
            terminal_by,
        })
    }

    pub fn first_notice_by(self) -> u64 {
        self.first_notice_by
    }

    pub fn first_material_request_by(self) -> u64 {
        self.first_material_request_by
    }

    pub fn material_ready_by(self) -> u64 {
        self.material_ready_by
    }

    pub fn inspection_by(self) -> u64 {
        self.inspection_by
    }

    pub fn localized_by(self) -> u64 {
        self.localized_by
    }

    pub fn filed_by(self) -> u64 {
        self.filed_by
    }

    pub fn terminal_by(self) -> u64 {
        self.terminal_by
    }

    /// Run only with the profile's authenticated availability observation.
    /// This checks time, leaving authentication and the inclusion opportunity
    /// to the native admission procedure.
    pub fn material_arrival_fits(self, opened_at: u64, ready_at: u64) -> bool {
        self.clock.accepts_first_notice(opened_at) && (opened_at..=self.material_ready_by).contains(&ready_at)
    }
}

/// Pre-draw demand in a common physical resource pool. Reserve an entire
/// protected opportunity for every possible drawn bond. Outsider and
/// defensive demand are accounted for separately from that protected demand.
/// The native owner derives the projection and assigns it to distinct bonds
/// after the draw. This does not fund fees or allocate actual service slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectedRescueDemand {
    pub max_drawn_bonds: u64,
    pub per_drawn_bond: RemainingCapacity,
    pub max_outsiders: u64,
    pub per_outsider: RemainingCapacity,
    pub defensive: RemainingCapacity,
}

impl ProtectedRescueDemand {
    pub fn total(self) -> Result<RemainingCapacity, ContractError> {
        if self.max_drawn_bonds == 0 {
            return Err(ContractError::Range);
        }
        self.per_drawn_bond
            .checked_scale(self.max_drawn_bonds)?
            .checked_add(self.per_outsider.checked_scale(self.max_outsiders)?)?
            .checked_add(self.defensive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rescue_demand_includes_every_drawn_bond_and_independent_response_capacity() {
        let demand = ProtectedRescueDemand {
            max_drawn_bonds: 3,
            per_drawn_bond: RemainingCapacity { transmitted_bytes: 100, additional_work: 40, retained_bytes: 20 },
            max_outsiders: 2,
            per_outsider: RemainingCapacity { transmitted_bytes: 50, additional_work: 15, retained_bytes: 5 },
            defensive: RemainingCapacity { transmitted_bytes: 25, additional_work: 10, retained_bytes: 15 },
        };
        assert_eq!(demand.total(), Ok(RemainingCapacity { transmitted_bytes: 425, additional_work: 160, retained_bytes: 85 }));
        assert_eq!(ProtectedRescueDemand { max_drawn_bonds: 0, ..demand }.total(), Err(ContractError::Range));
        assert_eq!(ProtectedRescueDemand { max_drawn_bonds: u64::MAX, ..demand }.total(), Err(ContractError::Budget));
        assert_eq!(
            ProtectedRescueDemand { defensive: RemainingCapacity { retained_bytes: u64::MAX, ..demand.defensive }, ..demand }.total(),
            Err(ContractError::Budget)
        );
    }

    #[test]
    fn verification_schedule_reserves_both_windows_from_original_admission() {
        let clock = OriginalClock::new(100).unwrap();
        let notice = FirstNoticeDemand {
            draw_ready_by: 120,
            material_ready_by: 200,
            cold_acquisition_daa: 1_000,
            full_inspection_daa: 1_000,
            notice_inclusion_daa: 1_020,
            missing_material_detected_by: 3_200,
            material_request_inclusion_daa: 20,
        };
        let case = MaterialCaseDemand {
            material_delivery_daa: 3_000,
            full_inspection_daa: 1_000,
            localization_daa: 1_000,
            filing_daa: 1_000,
            adjudication_and_recovery_daa: 880,
        };
        let schedule = VerificationSchedule::new(clock, notice, case).unwrap();
        assert_eq!(schedule.first_notice_by(), clock.notice_until());
        assert_eq!(schedule.first_material_request_by(), clock.notice_until());
        assert_eq!(schedule.material_ready_by(), 6_220);
        assert_eq!(schedule.inspection_by(), 7_220);
        assert_eq!(schedule.localized_by(), 8_220);
        assert_eq!(schedule.filed_by(), 9_220);
        assert_eq!(schedule.terminal_by(), clock.hard());
        assert!(schedule.material_arrival_fits(clock.notice_until(), schedule.material_ready_by()));
        assert!(!schedule.material_arrival_fits(clock.notice_until(), schedule.material_ready_by() + 1));
        assert!(!schedule.material_arrival_fits(clock.notice_until() + 1, schedule.material_ready_by()));
        assert_eq!(
            VerificationSchedule::new(clock, FirstNoticeDemand { material_ready_by: 201, ..notice }, case),
            Err(ContractError::Budget)
        );
        assert_eq!(
            VerificationSchedule::new(clock, FirstNoticeDemand { material_request_inclusion_daa: 21, ..notice }, case),
            Err(ContractError::Budget)
        );
        assert_eq!(
            VerificationSchedule::new(clock, notice, MaterialCaseDemand { filing_daa: 1_001, ..case }),
            Err(ContractError::Budget)
        );
        assert_eq!(
            VerificationSchedule::new(clock, notice, MaterialCaseDemand { filing_daa: u64::MAX, ..case }),
            Err(ContractError::Range)
        );
    }

    #[test]
    fn manifest_bytes_are_shared_but_each_claimant_keeps_a_case() {
        let d = Digest([7; 64]);
        let original = OriginalCommit {
            scope: ChainScope { network: d, ruleset: d, branch: d },
            job: d,
            execution: d,
            miner: PrincipalId(d),
            registered_class: d,
            artifact: d,
            program: d,
            prompt: d,
            input_tokens: d,
            tokenizer: d,
            execution_rules: d,
            generation_seed: [0; 32],
            initial_state: d,
            dependencies: d,
            prefill: 1,
            decode: 1,
            max_context: 1,
            output: d,
            trace_root: d,
            manifest_root: d,
            service_contract: d,
        };
        let first = Claimant::DrawnBond { transaction: d, index: 0 };
        let second = Claimant::DrawnBond { transaction: d, index: 1 };
        let request = ManifestRequest {
            original: original.id(),
            manifest_root: d,
            claimant: first,
            signer: PrincipalId(d),
            inspection_scope: d,
            supply_obligation: d,
            collect_until: 100,
            reservation: d,
        };
        let other = ManifestRequest { claimant: second, ..request.clone() };
        assert_eq!(request.material_id(&original), other.material_id(&original));
        assert_ne!(request.case_id(&original, first), other.case_id(&original, second));
        assert_eq!(other.case_id(&original, first), Err(ContractError::Identity));

        let retry = ManifestRequest { collect_until: 101, reservation: Digest([8; 64]), ..request.clone() };
        assert_eq!(request.case_id(&original, first), retry.case_id(&original, first));
        assert_ne!(request.signing_digest(), retry.signing_digest());

        let wrong_root = ManifestRequest { manifest_root: Digest([8; 64]), ..request.clone() };
        assert_eq!(wrong_root.material_id(&original), Err(ContractError::Scope));

        let clock = OriginalClock::new(100).unwrap();
        let schedule = VerificationSchedule {
            clock,
            first_notice_by: 3_220,
            first_material_request_by: 3_220,
            material_ready_by: 6_220,
            inspection_by: 7_220,
            localized_by: 8_220,
            filed_by: 9_220,
            terminal_by: 10_100,
        };
        let mut context = ManifestRequestContext {
            original: &original,
            claimant: first,
            signer: PrincipalId(d),
            inspection_scope: d,
            supply_obligation: d,
            reservation: d,
            schedule,
            inclusion_daa: clock.notice_until(),
        };
        let request = ManifestRequest { collect_until: schedule.filed_by(), ..request };
        assert_eq!(request.check_inclusion(&context), request.case_id(&original, first));
        assert_eq!(wrong_root.check_inclusion(&context), Err(ContractError::Scope));
        let other_reservation = ManifestRequest { reservation: Digest([8; 64]), ..request.clone() };
        assert_eq!(other_reservation.check_inclusion(&context), Err(ContractError::Budget));
        let shortened = ManifestRequest { collect_until: schedule.filed_by() - 1, ..request.clone() };
        assert_eq!(shortened.check_inclusion(&context), Err(ContractError::Range));
        context.inclusion_daa += 1;
        assert_eq!(request.check_inclusion(&context), Err(ContractError::Range));
    }

    #[test]
    fn original_clock_keeps_inclusive_notice_and_hard_boundaries() {
        let clock = OriginalClock::new(100).unwrap();
        let notice = 3_220;
        let hard = 10_100;
        assert!(!clock.accepts_first_notice(99));
        assert!(clock.accepts_first_notice(100));
        assert!(clock.accepts_first_notice(notice));
        assert!(!clock.accepts_first_notice(notice + 1));
        assert!(!clock.permits_final_time(notice));
        assert!(clock.permits_final_time(notice + 1));
        assert!(clock.permits_final_time(hard));
        assert!(!clock.permits_final_time(hard + 1));
        assert!(clock.accepts_case_evidence(hard, notice, hard));
        assert!(!clock.accepts_case_evidence(hard, notice, hard - 1));
        assert!(!clock.accepts_case_evidence(hard, notice + 1, hard));
        assert!(!clock.accepts_case_evidence(hard, notice, hard + 1));
        assert!(!clock.is_past_hard(hard));
        assert!(clock.is_past_hard(hard + 1));
        assert_eq!(OriginalClock::new(u64::MAX - 10_000), Err(ContractError::Range));
    }
}
