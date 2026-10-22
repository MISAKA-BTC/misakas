//! **Liveness hotfix (L-LC-02 / L-COURT-01): the H-1 gate's read-only pre-check — NODE-LOCAL, never a block rule.**
//!
//! The mempool's H-1 gate (`palw_mempool_h1_carrier_refusal`) puts a carrier to the acceptance layer and then to the object's own fold
//! arm through `palw_v2_apply_one_object_retained_v1`, whose `TransitionBuilder` deep-copies the whole PALW state before it reads the
//! object (L-LC-01: 65 µs / 594 µs / 5.6 ms at 1k / 10k / 50k claim rows). A `SeatReadinessProvedV2` signed by the sender's own bond
//! passes the acceptance layer (it checks only the signature) and is judged — possession proof and all — after that copy, so a bond
//! owner could buy one state copy per signed junk proof, for free (the refusal is never mined, so no fee is paid).
//!
//! [`palw_seat_readiness_v2_precheck_v1`] answers the refusals of `TransitionBuilder::apply_seat_readiness_v2` that are decided by the
//! object alone (or by a class root that never moves) BEFORE any copy. It is **sound by construction**, which is the only property the
//! gate needs: every refusal it returns is a refusal the fold's arm returns, with the same condition, the same inputs and the same text,
//! whenever the arm is reached; and an object refused earlier (fence, bond, signature, class scope, span window, replay) is refused
//! all the same — so the gate's accept/refuse decision is unchanged. It never accepts anything: `None` hands the object on to the
//! acceptance layer and the fold exactly as before.
//!
//! The refusal is also CLASSIFIED for the peer-penalty half of the hotfix ([`PalwH1PrecheckV1`]): a proof whose width, operand size,
//! multiproof shape or challenge opening is wrong is wrong at every tip — no honest seat, ahead of or behind this node, builds one —
//! so a peer that relays it is misbehaving. A proof that merely does not open a root the tip holds can be an honest race (a model line
//! publishing a root in the block this node has not seen yet, or a composite record landing), so it is refused without a penalty.

use crate::palw_artifact::{PalwArtifactMultiproofV1, reconstruct_artifact_multiproof_root_v1, verify_artifact_multiproof_v1};
use crate::palw_model_registry_v1 as registry;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2};
use kaspa_hashes::Hash64;

/// What the pre-check refuses, and whether the refusal is the object's own (the same at every tip: a relaying peer is misbehaving) or
/// the tip's (it can move with the next block: an honest race, never penalised).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwH1PrecheckV1 {
    /// Wrong at every tip — the object's own shape or a root that is never rewritten.
    Invalid(String),
    /// Refused at this tip; another block could change the answer.
    Refused(String),
}

impl PalwH1PrecheckV1 {
    pub fn reason(&self) -> &str {
        match self {
            Self::Invalid(why) | Self::Refused(why) => why,
        }
    }
}

/// The fold arm's object-and-root refusals of a V2 possession proof, read-only. `None`: nothing here refuses it (the gate goes on to
/// the acceptance layer and the fold). Mirrors `TransitionBuilder::apply_seat_readiness_v2` check for check, in its order, with the
/// span-window and replay checks (which read the registry clock and the seat's row) left to the fold.
pub fn palw_seat_readiness_v2_precheck_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    daa_score: u64,
    bond: &PalwBondKeyV2,
    class_id: &Hash64,
    span: u64,
    proof: &PalwArtifactMultiproofV1,
) -> Option<PalwH1PrecheckV1> {
    use PalwH1PrecheckV1::{Invalid, Refused};
    // The fold's arm refuses these before it reads the row (in this order); the conditions and texts are its own.
    if proof.opened.len() > registry::PALW_READINESS_V2_CHUNKS_V1 as usize {
        return Some(Invalid(format!(
            "the proof opens {} leaves; no challenge names more than {}",
            proof.opened.len(),
            registry::PALW_READINESS_V2_CHUNKS_V1
        )));
    }
    let legacy_full_challenge = registry::palw_readiness_v2_is_legacy_full_challenge_v1(proof.opened.len(), proof.operand_bytes());
    if proof.operand_bytes() > registry::PALW_READINESS_V2_OPERAND_MAX_BYTES_V1 && !legacy_full_challenge {
        return Some(Invalid(format!(
            "the proof carries {} bytes of operands, above the {} one carrier holds",
            proof.operand_bytes(),
            registry::PALW_READINESS_V2_OPERAND_MAX_BYTES_V1
        )));
    }
    // The root. A class the tip does not hold is the fold's `MissingClass` (an honest race: the class may land next block) — not
    // judged here.
    let class = state.class(class_id)?;
    let improve = params.improve_active_at(daa_score);
    let composite = state.improvement_composite_class(class_id).filter(|_| improve);
    let possession_root = match &composite {
        Some(record) => {
            if record.artifact_root() != class.artifact_root {
                return Some(Refused("the composite record does not root to the class's registered artifact root".to_string()));
            }
            record.adapter_root
        }
        None => class.artifact_root,
    };
    if params.audit_1004_active_at(daa_score) {
        let reconstructed = match reconstruct_artifact_multiproof_root_v1(proof) {
            Ok(root) => root,
            // A multiproof that reconstructs no root at all is malformed at every tip.
            Err(e) => return Some(Invalid(format!("{e}"))),
        };
        if reconstructed != possession_root
            && (state.improvement_composite_class(class_id).is_some()
                || !state.class_roots_in_force(class_id, daa_score).contains(&reconstructed))
        {
            // A model line can publish a root in a block this node has not seen yet: refused, never penalised.
            return Some(Refused("the proof reconstructs no root the class has in force".to_string()));
        }
    } else if let Err(e) = verify_artifact_multiproof_v1(proof, possession_root) {
        // Malformed (no root reconstructs) is the object's own; a root mismatch is too while no composite record can re-root the class
        // (below the improvement fence a class's registered artifact root is never rewritten).
        let own = reconstruct_artifact_multiproof_root_v1(proof).is_err() || !improve;
        return Some(if own { Invalid(format!("{e}")) } else { Refused(format!("{e}")) });
    }
    // The challenge: a function of (class, bond, span) and the proof's own leaf count — the object's own at every tip. The fold asks it
    // after the span window and the replay check; a proof refused there is refused all the same.
    let bond_bytes = borsh::to_vec(bond).expect("a bond key is borsh-serializable");
    let seed = registry::palw_readiness_v2_challenge_seed_v1(class_id, &bond_bytes, span);
    if legacy_full_challenge {
        let wanted = registry::palw_readiness_v2_leaves_v1(&seed, proof.leaf_count);
        if proof.opened_indices() != wanted {
            return Some(Invalid(format!(
                "the legacy full proof opens {:?} of {} leaves; this span's challenge names {:?}",
                proof.opened_indices(),
                proof.leaf_count,
                wanted
            )));
        }
    } else {
        let draw = registry::palw_readiness_v2_draw_v1(&seed, proof.leaf_count);
        let opened: Vec<(u32, usize)> = proof.opened.iter().map(|(index, operand)| (*index, operand.bytes.len())).collect();
        if let Err(why) = registry::palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened) {
            return Some(Invalid(why));
        }
    }
    None
}
