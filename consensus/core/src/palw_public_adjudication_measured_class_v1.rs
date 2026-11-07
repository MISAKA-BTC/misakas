//! P4 measured-class admission.
//!
//! A profile becomes a measured class only when [`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1`]
//! names it with a measurement digest. The list is empty. This module does not
//! time a model, does not fill CPU, RAM, I/O, or evidence bytes, and does not
//! arm a shipped preset. The G14 batch budget remains the provisional estimate
//! in `palw_g14_batch_replay_budget_estimate_v1`.

use crate::Hash64;

/// Pinned measurement records for a public class. Empty until a harness printout
/// is reviewed and added. No preset reads this list.
pub const PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1: &[PublicClassMeasurementRecordV1] = &[];

/// Identity of one harness document for one profile. Resource numbers stay in
/// the document the digest names. This struct does not carry timings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicClassMeasurementRecordV1 {
    pub profile: Hash64,
    /// BLAKE2b-256 of the harness document. An all-zero digest is not a record.
    pub digest: [u8; 32],
}

/// Why a profile stays outside the measured-class set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicMeasuredClassRefusalV1 {
    NoMeasurementRecord,
    MeasurementDigestMissing,
}

impl PublicMeasuredClassRefusalV1 {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoMeasurementRecord => "a profile without a measurement record cannot be admitted as a measured class",
            Self::MeasurementDigestMissing => "a measurement record needs the digest of a harness document",
        }
    }
}

/// Admit `profile` as a measured class from the pinned list.
pub fn admit_measured_class_v1(profile: Hash64) -> Result<(), PublicMeasuredClassRefusalV1> {
    admit_measured_class_from_records_v1(profile, PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1)
}

/// The same rule over an explicit record list. The pinned list is empty.
pub fn admit_measured_class_from_records_v1(
    profile: Hash64,
    records: &[PublicClassMeasurementRecordV1],
) -> Result<(), PublicMeasuredClassRefusalV1> {
    let Some(record) = records.iter().find(|record| record.profile == profile) else {
        return Err(PublicMeasuredClassRefusalV1::NoMeasurementRecord);
    };
    if record.digest == [0; 32] {
        return Err(PublicMeasuredClassRefusalV1::MeasurementDigestMissing);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_without_a_measurement_record_is_not_a_measured_class() {
        let profile = Hash64::from_u64_word(0x704);
        assert!(PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1.is_empty());
        assert_eq!(admit_measured_class_v1(profile), Err(PublicMeasuredClassRefusalV1::NoMeasurementRecord));
        assert_eq!(admit_measured_class_from_records_v1(profile, &[]), Err(PublicMeasuredClassRefusalV1::NoMeasurementRecord));
        let other = PublicClassMeasurementRecordV1 { profile: Hash64::from_u64_word(0x705), digest: [4; 32] };
        assert_eq!(admit_measured_class_from_records_v1(profile, &[other]), Err(PublicMeasuredClassRefusalV1::NoMeasurementRecord));
        let blank = PublicClassMeasurementRecordV1 { profile, digest: [0; 32] };
        assert_eq!(
            admit_measured_class_from_records_v1(profile, &[blank]),
            Err(PublicMeasuredClassRefusalV1::MeasurementDigestMissing)
        );
        // A stand-in digest admits only through an explicit list. The pinned
        // list stays empty, so the production entry still refuses the profile.
        // The stand-in is not a harness printout.
        let stand_in = PublicClassMeasurementRecordV1 { profile, digest: [9; 32] };
        assert_eq!(admit_measured_class_from_records_v1(profile, &[stand_in]), Ok(()));
        assert_eq!(admit_measured_class_v1(profile), Err(PublicMeasuredClassRefusalV1::NoMeasurementRecord));
    }

    #[test]
    fn the_g14_batch_budget_estimate_is_not_a_measured_class() {
        let budget = crate::palw_g14_values_v1::palw_g14_batch_replay_budget_estimate_v1();
        assert_eq!(budget.work.evaluation.max_elements, 2);
        assert_eq!(budget.work.evaluation.max_terms, 3);
        assert_eq!(budget.work.max_target_requests, 1);
        assert_eq!(budget.work.max_source_reads, 1);
        let profile = Hash64::from_u64_word(0x614);
        let estimate = crate::palw_g14_values_v1::palw_g14_batch_replay_profile_estimate_v1(profile);
        assert_eq!(estimate.service_profile, profile);
        assert_eq!(estimate.budget.work, budget.work);
        assert_eq!(admit_measured_class_v1(estimate.service_profile), Err(PublicMeasuredClassRefusalV1::NoMeasurementRecord));
        assert!(PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1.iter().all(|record| record.profile != estimate.service_profile));
    }
}
