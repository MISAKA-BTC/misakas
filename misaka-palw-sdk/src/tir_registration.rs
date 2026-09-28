//! **Registering an IR class** (RFC-0002 Phase F, F6's node half; design §2.3–§2.4).
//!
//! An IR class is the class its `PALWTIR1` artifact declares (program, layout, tokenizer) under the
//! TIR inventory root its bytes derive, so a node's candidates are its IR holdings — never a table —
//! and registering one is: pick it the way [`crate::PalwClassSdk::registration_candidate`] picks a
//! legacy class (drop what the chain already holds, honour the operator's pick, refuse ambiguity),
//! gate it, build `ClassRegisteredTirV1` with its admission carriage, and sign
//! `palw_tir_class_registration_message_v1` over every field the object carries.
//!
//! **Two pieces here stand in for consensus functions F6 has not landed yet**, with the signatures
//! requested from Phase F so the switch is a rename:
//!
//! * [`palw_tir_post_genesis_registration_stub_v1`] for `palw_tir_post_genesis_registration_v1`
//!   (the object, counted as the gate recounts it: `pwu_per_inference` is the canonical job's step
//!   leaf count under the ladder);
//! * [`tir_registration_preflight_v1`] for the acceptance layer's v10 gate: the fence in force, the
//!   fence's program and context ceilings, the IR conformance battery (`tir_admit_v1` and the step
//!   space), and the canonical job at the formula.
//!
//! The share is 0‰: an IR class joins weightless until the chain can certify an IR family
//! (design §2.4 item 10 — registration stays permissionless at 0‰).

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwRegistrationTermsV2};
use kaspa_consensus_core::palw_tir_class_v1::{PalwTirAdmissionCarriageV1, PalwTirClassV1, palw_tir_class_registration_message_v1};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

use crate::lineage::{PalwLoadedArtifactV1, PalwTirClassEntryV1};
use crate::lineages::tir::TIR_LINEAGE_ID_V1;

/// **The IR classes of `holdings`** — every artifact the IR lineage loaded, one entry per class id
/// (first holding wins). Read off the holdings themselves, so any SDK instance over them agrees.
pub fn tir_entries_of_v1(holdings: &[PalwLoadedArtifactV1]) -> Vec<PalwTirClassEntryV1> {
    let mut out: Vec<PalwTirClassEntryV1> = Vec::new();
    for h in holdings.iter().filter(|h| h.lineage_id == TIR_LINEAGE_ID_V1) {
        if let Some(entry) = h.payload().downcast_ref::<PalwTirClassEntryV1>()
            && !out.iter().any(|e| e.class_id() == entry.class_id())
        {
            out.push(entry.clone());
        }
    }
    out
}

/// Does `wanted` name `entry` — its model id, or its class id in hex?
fn names(entry: &PalwTirClassEntryV1, wanted: &str) -> bool {
    let w = wanted.trim_start_matches("0x");
    entry.model_id == wanted || entry.class_id().to_string() == w
}

/// **The one IR registration this node should attempt, or why there is none** — the legacy
/// selection's sentences, over IR holdings: nothing held, everything already registered (by class
/// id, or by artifact root: known weights are never re-registered under a fresh id), the operator's
/// `--palw-register-class` matching nothing, or more than one left and nothing picked.
pub fn tir_registration_candidate_v1(
    holdings: &[PalwLoadedArtifactV1],
    terms: &PalwRegistrationTermsV2,
    wanted: Option<&str>,
) -> Result<PalwTirClassEntryV1, String> {
    let wanted = wanted.filter(|s| !s.is_empty());
    let all = tir_entries_of_v1(holdings);
    let named: Vec<PalwTirClassEntryV1> = all.into_iter().filter(|e| wanted.is_none_or(|w| names(e, w))).collect();
    if named.is_empty() {
        return Err(match wanted {
            Some(w) => format!("--palw-register-class {w} names no IR class this node's artifacts declare"),
            None => "no --palw-class-artifact is an IR artifact, so there is no IR class to register".to_string(),
        });
    }
    let fresh: Vec<PalwTirClassEntryV1> = named
        .into_iter()
        .filter(|e| !terms.registered_class_ids.contains(&e.class_id()) && !terms.registered_artifact_roots.contains(&e.artifact_root))
        .collect();
    match fresh.len() {
        0 => Err("every IR class this node's artifacts declare is already registered on this chain (or its weights are)".to_string()),
        1 => Ok(fresh.into_iter().next().expect("one")),
        n => Err(format!(
            "this node's artifacts declare {n} unregistered IR classes ({}) — name one with --palw-register-class <model-id>",
            fresh.iter().map(|e| e.model_id.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// **Stand-in for consensus's `palw_tir_post_genesis_registration_v1`** (requested from Phase F,
/// same signature): the registration object for one IR class, its `pwu_per_inference` counted from
/// the canonical job the carriage carries (the count the gate recounts), `class_id` derived from the
/// class and its root. The signature is over [`tir_registration_message_v1`] of this object.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_post_genesis_registration_stub_v1(
    class: PalwTirClassV1,
    canonical: PalwJobContextV2,
    artifact_root: Hash64,
    share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
    registrant_bond: PalwBondKeyV2,
    signature: Vec<u8>,
    ladder: u64,
) -> Result<PalwConsensusObjectV2, String> {
    let class_id = class.class_id(&artifact_root);
    if canonical.shape_profile_id != class_id {
        return Err("the canonical job names another class".into());
    }
    let space = PalwTirStepSpaceV1::new(&class).map_err(|e| format!("the class builds no step space: {e}"))?;
    let counted = space.leaf_count_capped(&canonical, ladder).map_err(|e| format!("the canonical job does not count: {e}"))?;
    Ok(PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa,
        admission: Box::new(PalwTirAdmissionCarriageV1 { class, canonical, registrant_bond, signature }),
    })
}

/// **The message the registrant bond signs** — `palw_tir_class_registration_message_v1` over every
/// field `object` carries (never over a field assembled beside it). `None` for any other object.
pub fn tir_registration_message_v1(network_domain: Hash64, object: &PalwConsensusObjectV2) -> Option<Hash64> {
    let PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission,
    } = object
    else {
        return None;
    };
    Some(palw_tir_class_registration_message_v1(
        network_domain,
        *class_id,
        *share_permille,
        *activation_daa,
        &admission.registrant_bond,
        *artifact_root,
        *slash_value_per_pwu,
        *initial_target,
        pwu_rule,
        &admission.canonical,
        &admission.class,
    ))
}

/// **Stand-in for the acceptance layer's v10 gate** (F6): refuse before anything is signed what
/// this network would refuse — the fence not in force at `daa`, a program or a context past the
/// fence's ceilings, a class the IR battery refuses (`tir_admit_v1`, the step space, the range
/// proof), or a canonical job other than the formula's.
pub fn tir_registration_preflight_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    entry: &PalwTirClassEntryV1,
    daa: u64,
) -> Result<(), String> {
    let fence = params.palw_tir_v1_fence().ok_or("this network has not armed palw_tir_v1, so it registers no IR class")?;
    if !fence.activation.is_active(daa) {
        return Err(format!("palw_tir_v1 is not in force at DAA {daa} (it arms at {})", fence.activation.daa_score()));
    }
    let who = &entry.model_id;
    if entry.class.program.len() as u64 > fence.ceilings.max_program_bytes as u64 {
        return Err(format!(
            "{who}: the program is {} bytes, past the network's {}",
            entry.class.program.len(),
            fence.ceilings.max_program_bytes
        ));
    }
    if entry.class.layout.max_context > fence.ceilings.max_context {
        return Err(format!(
            "{who}: max_context {} is past the network's {}",
            entry.class.layout.max_context, fence.ceilings.max_context
        ));
    }
    crate::conformance::check_tir_entry_v1(TIR_LINEAGE_ID_V1, entry, &bundle.court)?;
    if Some(entry.canonical_job) != kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_attempt_canonical_v1(&entry.class) {
        return Err(format!("{who}: the canonical job is not the formula's"));
    }
    Ok(())
}

/// **Build the IR registration** for `entry` under the chain's `terms`: gated first
/// ([`tir_registration_preflight_v1`]), then built at the network's own pricing (the base class's
/// slash value and initial target), weightless, at the formula's canonical job. Call once with an
/// empty signature to learn the object to sign, then again with the signature.
#[allow(clippy::too_many_arguments)]
pub fn build_tir_registration_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    entry: &PalwTirClassEntryV1,
    terms: &PalwRegistrationTermsV2,
    activation_daa: u64,
    registrant_bond: PalwBondKeyV2,
    signature: Vec<u8>,
    daa: u64,
) -> Result<PalwConsensusObjectV2, String> {
    tir_registration_preflight_v1(params, bundle, entry, daa)?;
    palw_tir_post_genesis_registration_stub_v1(
        entry.class.as_ref().clone(),
        entry.canonical_context(),
        entry.artifact_root,
        0,
        terms.initial_target,
        terms.slash_value_per_pwu,
        activation_daa,
        registrant_bond,
        signature,
        bundle.court.max_step_leaf_count(),
    )
    .map_err(|e| format!("{}: {e}", entry.model_id))
}
