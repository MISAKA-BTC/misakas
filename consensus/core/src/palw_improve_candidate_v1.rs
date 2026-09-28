//! **RFC-0004 §6: the candidate's wire payload** (`CandidateSubmitted`, tag 80).
//!
//! Skeleton (step 0) of the candidates lane's module (`rfc4/cand` owns it from the skeleton's sha on):
//! the payload's shape. Its validation (the family rule, the composite rule, the provenance policy's
//! form and references) is the lane's.

use crate::Hash64;
use crate::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use borsh::{BorshDeserialize, BorshSerialize};

/// Key of [`palw_candidate_declarations_digest_v1`].
pub const PALW_IMPROVE_DECLARATIONS_DOMAIN_V1: &[u8] = b"misaka-palw/improve/candidate-declarations/v1";

/// **A candidate's declarations** (RFC-0004 §2.1 check 2, §8.2, §9): what the provenance policy checks
/// in form and references (never in truth).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCandidateDeclarationsV1 {
    /// Registered datasets the trainer declares it used, each with its declared weight in permille (S2).
    pub datasets: Vec<(Hash64, u16)>,
    /// Registered `TeacherLicence` ids the candidate's training material relied on.
    pub licences: Vec<Hash64>,
    /// A mask of [`crate::palw_improve_state_v1::PalwTeacherClassV1::bit`].
    pub teacher_classes: u8,
}

/// **The `CandidateSubmitted` payload** (tag 80): an admitted IR class entered in an epoch.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCandidateSubmissionV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    /// The candidate's IR class id (Phase F's `tir_class_id_v1` over its artifact root).
    pub class_id: Hash64,
    pub artifact: PalwTirArtifactRefV1,
    /// The candidate class's layout, carried because the `tir_classes` record keeps only its digest:
    /// admission rebuilds the class over the composite root to recheck `class_id`, and sizes the
    /// composite close's step space from it.
    pub layout: crate::palw_tir_class_v1::PalwTirLayoutV1,
    pub declarations: PalwCandidateDeclarationsV1,
}

/// **The declarations' digest**, as the epoch keeps it ([`crate::palw_improve_state_v1::PalwEpochCandidateV1`]).
pub fn palw_candidate_declarations_digest_v1(declarations: &PalwCandidateDeclarationsV1) -> Hash64 {
    let bytes = borsh::to_vec(declarations).expect("declarations are borsh-serializable");
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_IMPROVE_DECLARATIONS_DOMAIN_V1).to_state();
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// =================================================================================================
// The candidate's acceptance (RFC-0004 §6; PALW-MIP-8, PALW-MIP-15) — the acceptance layer's half
// =================================================================================================

/// Key of [`palw_candidate_submission_message_v1`].
pub const PALW_IMPROVE_CANDIDATE_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/improve/candidate-submitted/message/v1";
/// The ML-DSA-87 context a submitter bond signs a `CandidateSubmitted` under.
pub const PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/improve/candidate-submitted/mldsa87/v1";
/// The most datasets and licences a candidate may declare (each list), so its declarations stay a
/// bounded read for the fold and the reward path.
pub const PALW_IMPROVE_CANDIDATE_MAX_DECLARED_V1: usize = 64;

/// **The message a submitter bond signs** for `CandidateSubmitted`: the network domain, the payload
/// whole and the submitter — so a submission can be neither replayed on another network nor lifted
/// onto another bond.
pub fn palw_candidate_submission_message_v1(
    network_domain: &Hash64,
    payload: &PalwCandidateSubmissionV1,
    submitter: &crate::palw_state_v2::PalwBondKeyV2,
) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_IMPROVE_CANDIDATE_MESSAGE_DOMAIN_V1).to_state();
    state.update(network_domain.as_byte_slice());
    state.update(&borsh::to_vec(payload).expect("a payload is borsh-serializable"));
    state.update(&borsh::to_vec(submitter).expect("a bond key is borsh-serializable"));
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The declarations' form** (RFC-0004 §2.1 check 2, "V for the form and references"): at most
/// [`PALW_IMPROVE_CANDIDATE_MAX_DECLARED_V1`] datasets and licences, none twice, dataset weights in
/// permille summing to at most 1,000, and teacher classes that name only defined classes, every one
/// the line's policy allows. `LICENSED_DISTILL` declared means a licence is cited. The references
/// (each dataset and licence registered, unexpired and covering the use) are the fold's, against its
/// tables.
pub fn palw_candidate_declarations_form_v1(d: &PalwCandidateDeclarationsV1, allowed_teacher_classes: u8) -> Result<(), &'static str> {
    use crate::palw_improve_state_v1::PalwTeacherClassV1 as T;
    if d.datasets.len() > PALW_IMPROVE_CANDIDATE_MAX_DECLARED_V1 || d.licences.len() > PALW_IMPROVE_CANDIDATE_MAX_DECLARED_V1 {
        return Err("more declared datasets or licences than a candidate may carry");
    }
    let mut ids: Vec<&Hash64> = d.datasets.iter().map(|(id, _)| id).collect();
    ids.sort();
    if ids.windows(2).any(|w| w[0] == w[1]) {
        return Err("a dataset declared twice");
    }
    let mut licences: Vec<&Hash64> = d.licences.iter().collect();
    licences.sort();
    if licences.windows(2).any(|w| w[0] == w[1]) {
        return Err("a licence cited twice");
    }
    if d.datasets.iter().map(|(_, w)| *w as u32).sum::<u32>() > 1000 {
        return Err("declared dataset weights past 1,000 permille");
    }
    let defined = T::ALL.iter().fold(0u8, |m, c| m | c.bit());
    if d.teacher_classes & !defined != 0 {
        return Err("a teacher class no protocol defines");
    }
    if d.teacher_classes & !allowed_teacher_classes != 0 {
        return Err("a teacher class the line's policy does not allow");
    }
    if d.teacher_classes & T::LicensedDistill.bit() != 0 && d.licences.is_empty() {
        return Err("LICENSED_DISTILL declared with no TeacherLicence cited");
    }
    Ok(())
}

/// **What the acceptance layer reads beside a candidate**: the line's head and every class its
/// versions name (a composite's parent must be one), the policy's full-weight switch and teacher
/// classes, and the artifact facts ([`crate::palw_improve_composite_v1::PalwTirCandidateFactsV1`]
/// without its parent, which this resolves).
#[derive(Clone, Copy, Debug)]
pub struct PalwCandidateAcceptanceV1<'a> {
    pub head: Hash64,
    pub line_classes: &'a [Hash64],
    pub full_weight_candidates: bool,
    pub allowed_teacher_classes: u8,
    /// The candidate class's `tir_classes` record and registered artifact root.
    pub record: &'a crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    pub artifact_root: Hash64,
    /// The parent's record and registered root, looked up by the caller at [`Self::parent_of`].
    pub parent_record: &'a crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    pub parent_root: Hash64,
    pub rules: crate::palw_improve_composite_v1::PalwTirCompositeAdmissionV1,
}

impl PalwCandidateAcceptanceV1<'_> {
    /// **The parent a candidate is judged against**: the line's head for full weights; for a
    /// composite, the parent it names — which must be the head or a class of the line's versions.
    pub fn parent_of(head: &Hash64, line_classes: &[Hash64], artifact: &PalwTirArtifactRefV1) -> Result<Hash64, &'static str> {
        match artifact {
            PalwTirArtifactRefV1::Single { .. } => Ok(*head),
            PalwTirArtifactRefV1::Composite { parent_class, .. } => {
                if parent_class == head || line_classes.contains(parent_class) {
                    Ok(*parent_class)
                } else {
                    Err("a composite over a class that is not of the line")
                }
            }
        }
    }
}

/// **The acceptance layer's half of `CandidateSubmitted`** (RFC-0004 §6; PALW-MIP-8, PALW-MIP-15):
/// the declarations' form, the parent (the head, or a class of the line for a composite), not the
/// head itself, and the artifact's admission — full weights of the parent's family where the policy
/// admits them, or a composite whose every terminal close is carriable in the composite form
/// ([`crate::palw_improve_composite_v1::palw_tir_candidate_artifact_admits_v1`]). The fold's half
/// (the epoch's window, `k_max`, the fees and the bond) is the epoch machine's.
pub fn palw_candidate_acceptance_v1(payload: &PalwCandidateSubmissionV1, a: &PalwCandidateAcceptanceV1<'_>) -> Result<(), String> {
    use crate::palw_improve_composite_v1 as c;
    palw_candidate_declarations_form_v1(&payload.declarations, a.allowed_teacher_classes).map_err(str::to_string)?;
    if payload.class_id == a.head {
        return Err("the line's head is no candidate of its own epoch".into());
    }
    let parent = PalwCandidateAcceptanceV1::parent_of(&a.head, a.line_classes, &payload.artifact).map_err(str::to_string)?;
    let composite = payload.artifact.composite();
    let artifact = match (&payload.artifact, composite.as_ref()) {
        (PalwTirArtifactRefV1::Single { root }, _) => c::PalwTirCandidateArtifactV1::Single(*root),
        (_, Some(r)) => c::PalwTirCandidateArtifactV1::Composite(r),
        _ => unreachable!("a non-single artifact is a composite"),
    };
    let facts = c::PalwTirCandidateFactsV1 {
        record: a.record,
        artifact_root: a.artifact_root,
        layout: &payload.layout,
        parent_class_id: parent,
        parent_record: a.parent_record,
        parent_root: a.parent_root,
        full_weights_allowed: a.full_weight_candidates,
        rules: a.rules,
    };
    c::palw_tir_candidate_artifact_admits_v1(&payload.class_id, artifact, &facts).map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_improve_state_v1::PalwTeacherClassV1 as T;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn d(datasets: Vec<(Hash64, u16)>, licences: Vec<Hash64>, teacher_classes: u8) -> PalwCandidateDeclarationsV1 {
        PalwCandidateDeclarationsV1 { datasets, licences, teacher_classes }
    }

    #[test]
    fn the_declarations_form_is_checked_and_nothing_more() {
        let all = T::ALL.iter().fold(0u8, |m, c| m | c.bit());
        let ok = d(vec![(h(1), 600), (h(2), 400)], vec![h(9)], T::SelfPlay.bit() | T::LicensedDistill.bit());
        assert_eq!(palw_candidate_declarations_form_v1(&ok, all), Ok(()));
        assert_eq!(palw_candidate_declarations_form_v1(&d(vec![], vec![], 0), 0), Ok(()), "a candidate may declare nothing");
        for (bad, allowed, why) in [
            (d(vec![(h(1), 1), (h(1), 1)], vec![], 0), all, "a dataset declared twice"),
            (d(vec![], vec![h(3), h(3)], 0), all, "a licence cited twice"),
            (d(vec![(h(1), 600), (h(2), 401)], vec![], 0), all, "declared dataset weights past 1,000 permille"),
            (d(vec![], vec![], 1 << 7), all, "a teacher class no protocol defines"),
            (d(vec![], vec![], T::Human.bit()), T::SelfPlay.bit(), "a teacher class the line's policy does not allow"),
            (d(vec![], vec![], T::LicensedDistill.bit()), all, "LICENSED_DISTILL declared with no TeacherLicence cited"),
            (d((0..65).map(|i| (h(i), 0)).collect(), vec![], 0), all, "more declared datasets or licences than a candidate may carry"),
        ] {
            assert_eq!(palw_candidate_declarations_form_v1(&bad, allowed), Err(why));
        }
    }

    #[test]
    fn a_composite_is_judged_against_a_class_of_its_line() {
        let composite = |parent| PalwTirArtifactRefV1::Composite { parent_class: parent, parent_root: h(2), adapter_root: h(3), p: 4 };
        let (head, line) = (h(10), [h(11), h(12)]);
        assert_eq!(PalwCandidateAcceptanceV1::parent_of(&head, &line, &PalwTirArtifactRefV1::Single { root: h(5) }), Ok(head));
        assert_eq!(PalwCandidateAcceptanceV1::parent_of(&head, &line, &composite(head)), Ok(head));
        assert_eq!(PalwCandidateAcceptanceV1::parent_of(&head, &line, &composite(h(12))), Ok(h(12)), "an earlier version");
        assert!(PalwCandidateAcceptanceV1::parent_of(&head, &line, &composite(h(13))).is_err(), "another line's class");
    }

    #[test]
    fn the_signed_message_binds_the_network_the_payload_and_the_submitter() {
        let payload = PalwCandidateSubmissionV1 {
            line_id: h(1),
            epoch: 3,
            class_id: h(2),
            artifact: PalwTirArtifactRefV1::Single { root: h(3) },
            layout: crate::palw_tir_class_v1::PalwTirLayoutV1 {
                version: 1,
                max_context: 8,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: vec![],
                state_tiles: vec![],
            },
            declarations: PalwCandidateDeclarationsV1 { datasets: vec![], licences: vec![], teacher_classes: 0 },
        };
        let bond = |i| {
            crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_bytes([i; 64]),
                index: 0,
            })
        };
        let m = palw_candidate_submission_message_v1(&h(7), &payload, &bond(1));
        assert_ne!(m, palw_candidate_submission_message_v1(&h(8), &payload, &bond(1)), "another network");
        assert_ne!(m, palw_candidate_submission_message_v1(&h(7), &payload, &bond(2)), "another submitter");
        let mut other = payload.clone();
        other.epoch = 4;
        assert_ne!(m, palw_candidate_submission_message_v1(&h(7), &other, &bond(1)), "another epoch");
    }
}
