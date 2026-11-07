//! Activation contract for the new public-adjudication rule.
//!
//! The rule stays unarmed. No fleet claim was run, and no shipped preset reads
//! this module. [`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1`] is empty, so
//! activation refuses. Rollback is stop-new-admission plus continuation of a
//! claim already admitted under the clock stored on that claim. It does not
//! rewrite the row. The trace list is empty and names no live block.

use crate::palw_public_adjudication_measured_class_v1::{
    admit_measured_class_from_records_v1, PublicClassMeasurementRecordV1, PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1,
};
use crate::palw_tir_frame_cost_v1::{batch_public_hold_end_v5, is_batch_public_clock_v5, PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5};
use crate::Hash64;

/// The new rule is not armed. No preset is selected from this module.
pub const PALW_PUBLIC_ADJUDICATION_RULE_ARMED_V1: bool = false;

/// Pinned fleet traces. Empty. This slice names no live block.
pub const PALW_PUBLIC_ADJUDICATION_TRACE_RECORDS_V1: &[PublicAdjudicationTraceRecordV1] = &[];

/// Why the new public-adjudication rule cannot be activated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicAdjudicationActivationRefusalV1 {
    /// [`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1`] has no measurement record.
    MeasuredClassRecordsEmpty,
    /// A listed record does not pass [`admit_measured_class_from_records_v1`].
    MeasuredClassNotAdmitted,
}

impl PublicAdjudicationActivationRefusalV1 {
    pub fn reason(self) -> &'static str {
        match self {
            Self::MeasuredClassRecordsEmpty => {
                "the public-adjudication rule refuses activation while PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1 is empty"
            }
            Self::MeasuredClassNotAdmitted => "a measured-class record in the activation list is not admitted",
        }
    }
}

/// Activate the new rule from the pinned measured-class list.
///
/// The pinned list is empty, so this refuses. `Ok` is not an armed preset and
/// is not reachable while that list stays empty.
pub fn activate_public_adjudication_rule_v1() -> Result<(), PublicAdjudicationActivationRefusalV1> {
    debug_assert!(!PALW_PUBLIC_ADJUDICATION_RULE_ARMED_V1);
    activate_public_adjudication_rule_from_records_v1(PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1)
}

/// The same activation rule over an explicit record list.
///
/// An empty list refuses. A non-empty list still has to pass the measured-class
/// predicate. `Ok` does not arm a preset, does not select `palw_batch_court_v1`,
/// and does not write a claim row.
pub fn activate_public_adjudication_rule_from_records_v1(
    records: &[PublicClassMeasurementRecordV1],
) -> Result<(), PublicAdjudicationActivationRefusalV1> {
    if records.is_empty() {
        return Err(PublicAdjudicationActivationRefusalV1::MeasuredClassRecordsEmpty);
    }
    for record in records {
        if admit_measured_class_from_records_v1(record.profile, records).is_err() {
            return Err(PublicAdjudicationActivationRefusalV1::MeasuredClassNotAdmitted);
        }
    }
    Ok(())
}

/// A claim row as stored at admission. Rollback copies this value and does not
/// replace its clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredPublicClaimRowV1 {
    pub claim_hash: Hash64,
    pub clock_version: u16,
}

/// Rollback of the new rule: stop new admission, and keep each already admitted
/// claim on the clock stored with that claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicAdjudicationRollbackV1 {
    pub stop_new_admission: bool,
    pub continued: StoredPublicClaimRowV1,
}

/// Stop new admission and continue `already_admitted` unchanged.
pub fn rollback_public_adjudication_v1(already_admitted: StoredPublicClaimRowV1) -> PublicAdjudicationRollbackV1 {
    PublicAdjudicationRollbackV1 { stop_new_admission: true, continued: already_admitted }
}

/// Why a new public-adjudication admission is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicNewAdmissionRefusalV1 {
    MeasuredClassRecordsEmpty,
    MeasuredClassNotAdmitted,
    /// Rollback has stopped new admission.
    Stopped,
}

impl PublicNewAdmissionRefusalV1 {
    pub fn reason(self) -> &'static str {
        match self {
            Self::MeasuredClassRecordsEmpty => PublicAdjudicationActivationRefusalV1::MeasuredClassRecordsEmpty.reason(),
            Self::MeasuredClassNotAdmitted => PublicAdjudicationActivationRefusalV1::MeasuredClassNotAdmitted.reason(),
            Self::Stopped => "a new admission after stop-new-admission is refused",
        }
    }
}

/// Admit a new claim under the public-adjudication rule.
///
/// Stop-new-admission refuses before the measured-class list is consulted.
/// While the pinned list is empty, an open gate still refuses. This function
/// writes no row.
pub fn admit_new_public_claim_v1(stop_new_admission: bool) -> Result<(), PublicNewAdmissionRefusalV1> {
    admit_new_public_claim_from_records_v1(stop_new_admission, PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1)
}

/// The admission gate over an explicit measured-class list. `Ok` writes nothing
/// and leaves [`PALW_PUBLIC_ADJUDICATION_RULE_ARMED_V1`] false.
pub fn admit_new_public_claim_from_records_v1(
    stop_new_admission: bool,
    records: &[PublicClassMeasurementRecordV1],
) -> Result<(), PublicNewAdmissionRefusalV1> {
    if stop_new_admission {
        return Err(PublicNewAdmissionRefusalV1::Stopped);
    }
    activate_public_adjudication_rule_from_records_v1(records).map_err(|refusal| match refusal {
        PublicAdjudicationActivationRefusalV1::MeasuredClassRecordsEmpty => PublicNewAdmissionRefusalV1::MeasuredClassRecordsEmpty,
        PublicAdjudicationActivationRefusalV1::MeasuredClassNotAdmitted => PublicNewAdmissionRefusalV1::MeasuredClassNotAdmitted,
    })
}

/// Why a stored claim row must not be rewritten by rollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredClaimRewriteRefusalV1 {
    /// A different clock would replace the stored one.
    BlindDowngrade,
    /// An older stored clock would be moved onto Batch clock version 5.
    ReclassifiedOntoBatchClockV5,
}

impl StoredClaimRewriteRefusalV1 {
    pub fn reason(self) -> &'static str {
        match self {
            Self::BlindDowngrade => "rollback is not a blind downgrade that rewrites old claim rows",
            Self::ReclassifiedOntoBatchClockV5 => "an already admitted claim is not reclassified onto Batch clock version 5",
        }
    }
}

/// The clock an already admitted claim follows after stop-new-admission.
///
/// The stored version is the answer. Batch clock version 5 is not substituted
/// for an older stored clock, and an already stored version 5 is not rewritten
/// downward.
pub fn stored_clock_after_rollback_v1(row: StoredPublicClaimRowV1) -> Result<u16, StoredClaimRewriteRefusalV1> {
    let rollback = rollback_public_adjudication_v1(row);
    if !rollback.stop_new_admission || rollback.continued != row {
        return Err(StoredClaimRewriteRefusalV1::BlindDowngrade);
    }
    let stored = row.clock_version;
    if !is_batch_public_clock_v5(stored) && batch_public_hold_end_v5(stored, 0, 0).is_ok() {
        return Err(StoredClaimRewriteRefusalV1::ReclassifiedOntoBatchClockV5);
    }
    Ok(stored)
}

/// Refuse a write that would change the clock stored on an admitted claim.
pub fn rewrite_stored_claim_row_v1(
    row: StoredPublicClaimRowV1,
    next_clock: u16,
) -> Result<StoredPublicClaimRowV1, StoredClaimRewriteRefusalV1> {
    if next_clock == row.clock_version {
        return Ok(row);
    }
    if is_batch_public_clock_v5(next_clock) && !is_batch_public_clock_v5(row.clock_version) {
        return Err(StoredClaimRewriteRefusalV1::ReclassifiedOntoBatchClockV5);
    }
    Err(StoredClaimRewriteRefusalV1::BlindDowngrade)
}

/// One fleet trace. Every field on [`PublicAdjudicationTraceRecordV1::empty`]
/// is blank. Filling a value does not insert it into
/// [`PALW_PUBLIC_ADJUDICATION_TRACE_RECORDS_V1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicAdjudicationTraceRecordV1 {
    pub source_commit: [u8; 20],
    pub build_settings: &'static str,
    pub binary_hash: [u8; 32],
    pub rule_version: u16,
    pub claim_hash: Option<Hash64>,
    pub signature_payload_hash: Option<Hash64>,
    pub acceptance_daa: Option<u64>,
    pub notice_daa: Option<u64>,
    pub evidence_root: Option<Hash64>,
    pub adjudication_daa: Option<u64>,
    pub final_daa: Option<u64>,
    pub block_reference: Option<Hash64>,
}

/// The blank trace. Its live fields stay empty. It is not a block.
pub const PALW_PUBLIC_ADJUDICATION_EMPTY_TRACE_V1: PublicAdjudicationTraceRecordV1 = PublicAdjudicationTraceRecordV1::empty();

impl PublicAdjudicationTraceRecordV1 {
    pub const fn empty() -> Self {
        Self {
            source_commit: [0; 20],
            build_settings: "",
            binary_hash: [0; 32],
            rule_version: 0,
            claim_hash: None,
            signature_payload_hash: None,
            acceptance_daa: None,
            notice_daa: None,
            evidence_root: None,
            adjudication_daa: None,
            final_daa: None,
            block_reference: None,
        }
    }

    /// Source commit, build settings, binary hash, and rule version are blank.
    pub fn identity_fields_empty(self) -> bool {
        self.source_commit == [0; 20] && self.build_settings.is_empty() && self.binary_hash == [0; 32] && self.rule_version == 0
    }

    /// Claim, signature payload, DAAs, evidence root, and block reference are blank.
    pub fn live_fields_empty(self) -> bool {
        self.claim_hash.is_none()
            && self.signature_payload_hash.is_none()
            && self.acceptance_daa.is_none()
            && self.notice_daa.is_none()
            && self.evidence_root.is_none()
            && self.adjudication_daa.is_none()
            && self.final_daa.is_none()
            && self.block_reference.is_none()
    }

    pub fn names_a_block(self) -> bool {
        self.block_reference.is_some()
    }

    /// Local conviction fill. Claim hash, evidence root, acceptance DAA, and
    /// adjudication DAA come from the caller. Binary hash, block reference,
    /// source commit, and the other fields that would name a live T12 block
    /// stay empty. This value is not inserted into
    /// [`PALW_PUBLIC_ADJUDICATION_TRACE_RECORDS_V1`].
    pub fn fill_from_local_conviction_v1(
        claim_hash: Hash64,
        evidence_root: Hash64,
        acceptance_daa: u64,
        adjudication_daa: u64,
    ) -> Self {
        Self {
            claim_hash: Some(claim_hash),
            evidence_root: Some(evidence_root),
            acceptance_daa: Some(acceptance_daa),
            adjudication_daa: Some(adjudication_daa),
            ..Self::empty()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA};
    use crate::palw_public_adjudication_measured_class_v1::{admit_measured_class_v1, PublicMeasuredClassRefusalV1};
    use crate::palw_tir_frame_cost_v1::{
        PALW_BATCH_PUBLIC_BUDGET_AFTER_NOTICE_DAA_V1, PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4, PALW_TIR_FRAME_CLOCK_VERSION_V1,
    };

    #[test]
    fn activation_refuses_while_measured_class_records_are_empty() {
        let profile = Hash64::from_u64_word(0xA571);
        assert!(PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1.is_empty());
        assert!(!PALW_PUBLIC_ADJUDICATION_RULE_ARMED_V1);
        assert_eq!(activate_public_adjudication_rule_v1(), Err(PublicAdjudicationActivationRefusalV1::MeasuredClassRecordsEmpty));
        assert_eq!(admit_measured_class_v1(profile), Err(PublicMeasuredClassRefusalV1::NoMeasurementRecord));
        assert_eq!(admit_new_public_claim_v1(false), Err(PublicNewAdmissionRefusalV1::MeasuredClassRecordsEmpty));
        let blank = PublicClassMeasurementRecordV1 { profile, digest: [0; 32] };
        assert_eq!(
            activate_public_adjudication_rule_from_records_v1(&[blank]),
            Err(PublicAdjudicationActivationRefusalV1::MeasuredClassNotAdmitted)
        );
        // A stand-in digest opens only the explicit list. The pinned list stays
        // empty, so the production entry still refuses. The stand-in is not a
        // harness printout and does not arm a preset.
        let stand_in = PublicClassMeasurementRecordV1 { profile, digest: [9; 32] };
        assert_eq!(activate_public_adjudication_rule_from_records_v1(&[stand_in]), Ok(()));
        assert_eq!(admit_new_public_claim_from_records_v1(false, &[stand_in]), Ok(()));
        assert_eq!(activate_public_adjudication_rule_v1(), Err(PublicAdjudicationActivationRefusalV1::MeasuredClassRecordsEmpty));
        assert_eq!(admit_new_public_claim_v1(false), Err(PublicNewAdmissionRefusalV1::MeasuredClassRecordsEmpty));
        assert!(!PALW_PUBLIC_ADJUDICATION_RULE_ARMED_V1);
        assert!(PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1.is_empty());
        assert!(crate::config::params::palw_t12_shipped_params().palw_batch_court_v1.is_none());
    }

    #[test]
    fn rollback_stops_new_admission_and_keeps_the_stored_clock() {
        assert_eq!(NOTICE_WINDOW_DAA, 3_120);
        assert_eq!(DISPUTE_WINDOW_DAA, 10_000);
        assert_eq!(PALW_BATCH_PUBLIC_BUDGET_AFTER_NOTICE_DAA_V1, 6_880);
        let old = StoredPublicClaimRowV1 { claim_hash: Hash64::from_u64_word(0x01D), clock_version: PALW_TIR_FRAME_CLOCK_VERSION_V1 };
        let rollback = rollback_public_adjudication_v1(old);
        assert!(rollback.stop_new_admission);
        assert_eq!(rollback.continued, old);
        assert_eq!(admit_new_public_claim_v1(true), Err(PublicNewAdmissionRefusalV1::Stopped));
        let stand_in = PublicClassMeasurementRecordV1 { profile: Hash64::from_u64_word(0xA571), digest: [9; 32] };
        assert_eq!(admit_new_public_claim_from_records_v1(true, &[stand_in]), Err(PublicNewAdmissionRefusalV1::Stopped));
        assert_eq!(stored_clock_after_rollback_v1(old), Ok(PALW_TIR_FRAME_CLOCK_VERSION_V1));
        assert!(!is_batch_public_clock_v5(old.clock_version));
        assert!(batch_public_hold_end_v5(old.clock_version, 1_000, 0).is_err());
        assert_ne!(stored_clock_after_rollback_v1(old).unwrap(), PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5);
        assert_eq!(
            rewrite_stored_claim_row_v1(old, PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5),
            Err(StoredClaimRewriteRefusalV1::ReclassifiedOntoBatchClockV5)
        );
        assert_eq!(rewrite_stored_claim_row_v1(old, old.clock_version), Ok(old));
        let family = StoredPublicClaimRowV1 { claim_hash: old.claim_hash, clock_version: PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4 };
        assert_eq!(stored_clock_after_rollback_v1(family), Ok(PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4));
        assert_eq!(
            rewrite_stored_claim_row_v1(family, PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5),
            Err(StoredClaimRewriteRefusalV1::ReclassifiedOntoBatchClockV5)
        );
        assert!(batch_public_hold_end_v5(family.clock_version, 1_000, 0).is_err());
        let already_v5 =
            StoredPublicClaimRowV1 { claim_hash: Hash64::from_u64_word(0x0B5), clock_version: PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5 };
        let kept = rollback_public_adjudication_v1(already_v5);
        assert_eq!(kept.continued, already_v5);
        assert_eq!(stored_clock_after_rollback_v1(already_v5), Ok(PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5));
        assert_eq!(
            rewrite_stored_claim_row_v1(already_v5, PALW_TIR_FRAME_CLOCK_VERSION_V1),
            Err(StoredClaimRewriteRefusalV1::BlindDowngrade)
        );
        assert!(!PALW_PUBLIC_ADJUDICATION_RULE_ARMED_V1);
    }

    #[test]
    fn a_trace_record_fills_from_fixture_values_and_names_no_live_block() {
        let empty = PALW_PUBLIC_ADJUDICATION_EMPTY_TRACE_V1;
        assert!(empty.identity_fields_empty());
        assert!(empty.live_fields_empty());
        assert!(!empty.names_a_block());
        assert!(PALW_PUBLIC_ADJUDICATION_TRACE_RECORDS_V1.is_empty());
        assert!(PALW_PUBLIC_ADJUDICATION_TRACE_RECORDS_V1.iter().all(|record| record.live_fields_empty() && !record.names_a_block()));
        let acceptance_daa = 100u64;
        let filled = PublicAdjudicationTraceRecordV1 {
            source_commit: [0x11; 20],
            build_settings: "fixture-offline",
            binary_hash: [0xA5; 32],
            rule_version: PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5,
            claim_hash: Some(Hash64::from_u64_word(0xC1A1)),
            signature_payload_hash: Some(Hash64::from_u64_word(0x516)),
            acceptance_daa: Some(acceptance_daa),
            notice_daa: Some(acceptance_daa + NOTICE_WINDOW_DAA),
            evidence_root: Some(Hash64::from_u64_word(0xE71D)),
            adjudication_daa: Some(acceptance_daa + 6_000),
            final_daa: Some(acceptance_daa + 9_000),
            block_reference: Some(Hash64::from_u64_word(0xF15E)),
        };
        assert_eq!(filled.source_commit, [0x11; 20]);
        assert_eq!(filled.build_settings, "fixture-offline");
        assert_eq!(filled.binary_hash, [0xA5; 32]);
        assert_eq!(filled.rule_version, PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5);
        assert_eq!(filled.claim_hash, Some(Hash64::from_u64_word(0xC1A1)));
        assert_eq!(filled.signature_payload_hash, Some(Hash64::from_u64_word(0x516)));
        assert_eq!(filled.acceptance_daa, Some(100));
        assert_eq!(filled.notice_daa, Some(100 + 3_120));
        assert_eq!(filled.evidence_root, Some(Hash64::from_u64_word(0xE71D)));
        assert_eq!(filled.adjudication_daa, Some(6_100));
        assert_eq!(filled.final_daa, Some(9_100));
        assert_eq!(filled.block_reference, Some(Hash64::from_u64_word(0xF15E)));
        assert!(filled.names_a_block());
        let live_binary = [0x8c_u8, 0xac, 0x52, 0xec, 0x4e, 0xd1];
        assert_ne!(&filled.binary_hash[..6], &live_binary);
        assert_ne!(&filled.source_commit[..6], &live_binary);
        assert_eq!(empty.binary_hash, [0; 32]);
        assert!(empty.live_fields_empty());
        assert!(!empty.names_a_block());
        assert!(PALW_PUBLIC_ADJUDICATION_TRACE_RECORDS_V1.is_empty());
        assert!(PALW_PUBLIC_ADJUDICATION_TRACE_RECORDS_V1.iter().all(|record| record.block_reference != filled.block_reference));
        assert_eq!(NOTICE_WINDOW_DAA, 3_120);
        assert_eq!(DISPUTE_WINDOW_DAA, 10_000);
    }
}
