//! **Registering an IR class** (RFC-0002 Phase F, F6's node half; design §2.3–§2.4).
//!
//! An IR class is the class its `PALWTIR1` artifact declares (program, layout, tokenizer) under the
//! TIR inventory root its bytes derive, so a node's candidates are its IR holdings — never a table —
//! and registering one is: pick it the way [`crate::PalwClassSdk::registration_candidate`] picks a
//! legacy class (drop what the chain already holds, honour the operator's pick, refuse ambiguity),
//! gate it, build `ClassRegisteredTirV1` with its admission carriage, and sign
//! `palw_tir_class_registration_message_v1` over every field the object carries.
//!
//! The object is consensus's (`palw_tir_admission_v1::palw_tir_post_genesis_registration_v1`: the
//! class id derived from the class and its root, `pwu_per_inference` counted from the carried
//! canonical job against the network's ladder, so the object and the gate's recount are one count),
//! and the gate is the one the acceptance path runs (`palw_tir_registration_preflight_at_v1`:
//! `palw_tir_v1` in force, then admission v10 under the rules at that height) — asked BEFORE the
//! object is signed, so a class the chain would refuse never reaches the signer, the mempool or the
//! fee.
//!
//! The share is 0‰: an IR class joins weightless until the chain can certify an IR family
//! (design §2.4 item 10 — registration stays permissionless at 0‰).

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwRegistrationTermsV2};
use kaspa_consensus_core::palw_tir_admission_v1::{palw_tir_post_genesis_registration_v1, palw_tir_registration_preflight_at_v1};
use kaspa_consensus_core::palw_tir_class_v1::palw_tir_class_registration_message_v1;
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

/// **The acceptance path's gate, asked of the unsigned object** (F6 B): `palw_tir_v1` in force at
/// `daa`, then admission v10 at that height (`palw_tir_registration_preflight_at_v1`) with the
/// chain's certified families. The error names the refusal by its code.
pub fn tir_registration_preflight_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    object: &PalwConsensusObjectV2,
    daa: u64,
    chain_certified: &[kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eFamilyV1],
) -> Result<(), String> {
    palw_tir_registration_preflight_at_v1(params, bundle, object, daa, chain_certified)
        .map(|_| ())
        .map_err(|e| format!("{} ({e})", e.code()))
}

/// **Build the IR registration** for `entry` under the chain's `terms`: the network's own pricing
/// (the base class's slash value and initial target), weightless, at the canonical job admission
/// v10 requires (`palw_tir_job_context_v1` at `palw_tir_attempt_canonical_v1`) — gated before it is
/// returned. Call once with an empty signature to learn the message to sign
/// ([`tir_registration_message_v1`]), then again with the signature.
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
    let object = palw_tir_post_genesis_registration_v1(
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
    .map_err(|e| format!("{}: {} ({e})", entry.model_id, e.code()))?;
    tir_registration_preflight_v1(params, bundle, &object, daa, &terms.chain_certified_families)
        .map_err(|e| format!("{}: the admission gate refuses the registration: {e}", entry.model_id))?;
    Ok(object)
}
