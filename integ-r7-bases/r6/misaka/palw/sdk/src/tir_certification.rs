//! **Certifying an IR family** (RFC-0002 Phase F, F6's node half; design §5 Q6).
//!
//! An IR class registers weightless (0‰). It is seated at the floor once the chain has certified a
//! family covering every primitive its program reaches: a `FamilyCertified { TirAttempt }` carrying
//! the class's drill (`misaka_palw_tir_exec::node::tir_family_evidence_v1`, graded on chain by
//! `palw_tir_certify_v1::certify_tir_e2e_family_v1`), then `ClassLaneCertifiedTirV1` naming the
//! class. Both objects are unsigned: the court grades the evidence and the carrier fee is the rent.
//!
//! The drill runs the class's own backend (the one a seat and a producer resolve) at a job fixed by
//! the class id, so two runs of one build over one artifact write the same evidence; it is graded
//! here before it is returned, so a drill the chain would refuse never leaves the machine.

use kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eFamilyV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::{PalwCertificationEvidenceV1, PalwConsensusObjectV2};
use kaspa_hashes::Hash64;

use crate::lineage::PalwTirClassEntryV1;
use crate::lineages::tir::TirLineageV1;

/// The anchor an IR class's certification drill runs at: fixed by the class, so the evidence is a
/// function of the build and the artifact alone.
pub fn tir_drill_anchor_v1(class_id: Hash64) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tir/drill-anchor/v1").to_state();
    s.update(class_id.as_byte_slice());
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The `FamilyCertified { TirAttempt }` object for `entry`** — its drill under the network's
/// `court` and prompt `form`, named `family_id` (the drill's default, the digest of the program's
/// reachable primitives, when `None`) — and the family the chain's grader returns for it.
pub fn tir_family_certification_v1(
    entry: &PalwTirClassEntryV1,
    court: &PalwCourtParamsV2,
    form: PalwPromptIdsFormV1,
    family_id: Option<Hash64>,
) -> Result<(PalwConsensusObjectV2, PalwE2eFamilyV1), String> {
    let backend = TirLineageV1::backend(entry, court, form)?;
    let rules = backend.court_rules(court);
    let (_, evidence) = misaka_palw_tir_exec::node::tir_family_evidence_v1(
        &backend,
        tir_drill_anchor_v1(entry.class_id()),
        &rules,
        family_id.unwrap_or_default(),
    )
    .map_err(|e| format!("{}: the drill: {e}", entry.model_id))?;
    let evidence = PalwCertificationEvidenceV1::TirAttempt(evidence);
    let family = evidence.grade().map_err(|e| format!("{}: this build's court refuses its own drill: {e}", entry.model_id))?;
    Ok((PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(evidence) }, family))
}

/// **The `ClassLaneCertifiedTirV1` object that seats `entry`'s registered class at the floor**,
/// once a family covering its program is certified (the fold refuses it before).
pub fn tir_lane_certification_v1(entry: &PalwTirClassEntryV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ClassLaneCertifiedTirV1 {
        class_id: entry.class_id(),
        artifact_root: entry.artifact_root,
        class: Box::new(entry.class.as_ref().clone()),
    }
}
