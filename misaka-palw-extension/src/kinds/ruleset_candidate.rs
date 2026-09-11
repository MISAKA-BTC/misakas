//! **`ruleset-candidate`** (ADR-0108 Decision 6): a fence and a height, described and costed —
//! never activated. The verifier takes this build's `Params` for the network, sets the fences as
//! asked, and reports what the arming build would print beside what this build prints, whether
//! the identity moves, and whether the build refuses the combination. The classification is
//! always `RulesetChange`.

use kaspa_consensus_core::config::params::{ForkActivation, Params};

use crate::manifest::{PalwExtensionError, PalwFenceRequestV1};
use crate::report::{PalwExtensionClassificationV1, PalwFenceDeltaV1, PalwWouldPrintV1};
use crate::verify::{KindOutcomeV1, VerifyCx, fence_value_string};

/// Set one fence of `Params` by the name `palw_fences_v1` gives it. One arm per name, so a fence
/// added to `Params` reaches here only when somebody spells it (the test in this module walks the
/// build's list). A Some-only fence that carries a companion value cannot be armed from nothing:
/// its activation is set where the preset already carries the value, and refused otherwise.
pub fn set_fence_by_name(params: &mut Params, name: &str, at: ForkActivation) -> Result<(), String> {
    fn companion<T>(
        slot: &mut Option<T>,
        name: &str,
        what: &str,
        at: ForkActivation,
        set: impl FnOnce(&mut T, ForkActivation),
    ) -> Result<(), String> {
        match slot.as_mut() {
            Some(value) => {
                set(value, at);
                Ok(())
            }
            None => Err(format!(
                "`{name}` carries a companion value ({what}) this preset does not set — the candidate needs a build before it needs a height"
            )),
        }
    }
    match name {
        "palw_bootstrap_activation" => params.palw_bootstrap_activation = Some(at),
        "palw_unavailable_abstains" => params.palw_unavailable_abstains = Some(at),
        "palw_bond_maturity" => companion(&mut params.palw_bond_maturity, name, "window_daa", at, |f, at| f.activation = at)?,
        "palw_frontier_provenance" => params.palw_frontier_provenance = Some(at),
        "palw_heartbeat" => {
            companion(&mut params.palw_heartbeat, name, "work_log2 and the mergeset bound", at, |f, at| f.activation = at)?
        }
        "palw_attempt_work" => {
            companion(&mut params.palw_attempt_work, name, "work_log2 and the nonce budget", at, |f, at| f.activation = at)?
        }
        "palw_attempt_activation" => params.palw_attempt_activation = Some(at),
        "palw_inactivity_leak" => {
            companion(&mut params.palw_inactivity_leak, name, "t_leak_daa and the re-entry depth", at, |f, at| f.activation = at)?
        }
        "palw_beacon_fold" => companion(&mut params.palw_beacon_fold, name, "k", at, |f, at| f.activation = at)?,
        "palw_capability_bound" => params.palw_capability_bound = Some(at),
        "palw_context_ladder" => params.palw_context_ladder = Some(at),
        "palw_panel_da" => params.palw_panel_da = Some(at),
        "palw_certification_rent" => params.palw_certification_rent = Some(at),
        "palw_uncertified_weightless" => params.palw_uncertified_weightless = Some(at),
        "palw_da_court" => params.palw_da_court = Some(at),
        "palw_court_ladder" => params.palw_court_ladder = Some(at),
        "palw_fp_da_pins" => params.palw_fp_da_pins = Some(at),
        "palw_validator_payout_bounds" => params.palw_validator_payout_bounds = Some(at),
        "palw_epoch_boundary_budget" => params.palw_epoch_boundary_budget = Some(at),
        "palw_fp_ruleset_caps" => params.palw_fp_ruleset_caps = Some(at),
        "palw_model_market" => params.palw_model_market = Some(at),
        "palw_model_lines" => params.palw_model_lines = Some(at),
        "palw_model_benefits" => params.palw_model_benefits = Some(at),
        "palw_model_evm" => params.palw_model_evm = Some(at),
        "palw_chunk_cap_charge" => params.palw_chunk_cap_charge = Some(at),
        "palw_prompt_ids_merkle" => params.palw_prompt_ids_merkle = Some(at),
        "palw_kary_court" => params.palw_kary_court = Some(at),
        "palw_court_responder_coverage" => params.palw_court_responder_coverage = Some(at),
        "palw_fp_decode_rules" => params.palw_fp_decode_rules = Some(at),
        "palw_fp_decode_constraint" => params.palw_fp_decode_constraint = Some(at),
        "palw_difficulty_priced_rows" => params.palw_difficulty_priced_rows = Some(at),
        "palw_receipt_rows_unpriced" => params.palw_receipt_rows_unpriced = Some(at),
        "palw_attempt_header_pins" => params.palw_attempt_header_pins = Some(at),
        "palw_signature_contexts_v2" => params.palw_signature_contexts_v2 = Some(at),
        "palw_heartbeat_transparent" => params.palw_heartbeat_transparent = Some(at),
        other => return Err(format!("this build has no fence `{other}`: the candidate needs a build before it needs a height")),
    }
    Ok(())
}

/// The four fingerprints of one `Params`, as the report prints them.
pub fn would_print_v1(params: &Params) -> PalwWouldPrintV1 {
    PalwWouldPrintV1 {
        params_id: params.consensus_params_id().to_string(),
        identity_id: params.consensus_identity_id().to_string(),
        schedule_id: params.consensus_schedule_id().to_string(),
        fence_schedule: params.fence_schedule_v1(),
    }
}

pub(crate) fn verify(cx: &mut VerifyCx<'_>) -> Result<KindOutcomeV1, PalwExtensionError> {
    let manifest = cx.manifest();
    let requested: Vec<(String, PalwFenceRequestV1)> = manifest.requires.fences.iter().map(|(n, r)| (n.clone(), r.clone())).collect();
    if requested.is_empty() {
        return Ok(KindOutcomeV1::at(
            cx.refuse("requires.fences", "a candidate names at least one fence, at `genesis` or a height"),
            cx.depth,
        ));
    }
    let this_build = would_print_v1(&cx.params);
    cx.record("this_build.params_id", &this_build.params_id);
    cx.record("this_build.identity_id", &this_build.identity_id);
    cx.record("this_build.schedule_id", &this_build.schedule_id);

    let mut arming = cx.params.clone();
    let mut deltas = Vec::with_capacity(requested.len());
    let mut needs_build = Vec::new();
    let mut unchanged = 0usize;
    let mut at_genesis = 0usize;
    let mut at_height = 0usize;
    let known = cx.params.palw_fences_v1();
    for (name, request) in &requested {
        let check = format!("requires.fences.{name}");
        let current = known.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        let at = match request {
            PalwFenceRequestV1::Height(h) => ForkActivation::new(*h),
            PalwFenceRequestV1::Named(word) if word == "genesis" => ForkActivation::always(),
            PalwFenceRequestV1::Named(word) => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(check, format!("a candidate names `genesis` or a height, not `{word}`")),
                    cx.depth,
                ));
            }
        };
        let Some(current) = current else {
            let why = format!("this build has no fence `{name}`: the candidate needs a build before it needs a height");
            cx.fail(&check, why.clone());
            needs_build.push(why);
            deltas.push(PalwFenceDeltaV1 { name: name.clone(), requested: request.to_string(), this_build: "absent".to_string() });
            continue;
        };
        deltas.push(PalwFenceDeltaV1 { name: name.clone(), requested: request.to_string(), this_build: fence_value_string(current) });
        match set_fence_by_name(&mut arming, name, at) {
            Ok(()) => {
                if current == Some(at) {
                    unchanged += 1;
                } else if at == ForkActivation::always() {
                    at_genesis += 1;
                } else {
                    at_height += 1;
                }
                cx.pass(&check);
            }
            Err(why) => {
                cx.fail(&check, why.clone());
                needs_build.push(why);
            }
        }
    }

    let arm = would_print_v1(&arming);
    cx.record("arming.params_id", &arm.params_id);
    cx.record("arming.identity_id", &arm.identity_id);
    cx.record("arming.schedule_id", &arm.schedule_id);
    cx.record("arming.fence_schedule", format!("{:?}", arm.fence_schedule));
    let identity_moves = arm.identity_id != this_build.identity_id;
    let params_moves = arm.params_id != this_build.params_id;
    let schedule_moves = arm.schedule_id != this_build.schedule_id;
    cx.record("identity_moves", identity_moves);
    cx.record("params_id_moves", params_moves);
    cx.record("schedule_id_moves", schedule_moves);
    let refusal = match arming.validate_palw_v2() {
        Ok(()) => {
            cx.pass("validate_palw_v2");
            None
        }
        Err(e) => {
            let text = format!("{e:?}");
            cx.fail("validate_palw_v2", text.clone());
            Some(text)
        }
    };

    let mut reasons = needs_build.clone();
    if let Some(refused) = &refusal {
        reasons.push(format!("this build refuses the combination (validate_palw_v2): {refused}"));
    }
    let flag_day;
    if identity_moves {
        flag_day = true;
        reasons.push("identity moves: refused at the handshake by every un-upgraded peer".to_string());
    } else if params_moves || schedule_moves {
        flag_day = true;
        reasons.push(
            "the fork-id gate (ADR-0072 SA-2) compares heights the moment the schedule is announced; identity unchanged, params id and schedule id move"
                .to_string(),
        );
    } else if needs_build.is_empty() && refusal.is_none() {
        flag_day = false;
        reasons.push(format!(
            "the candidate names the values this build already carries ({unchanged} unchanged); nothing would print differently"
        ));
    } else {
        flag_day = true;
    }
    if at_genesis > 0 && !identity_moves {
        // Reachable only for a fence that is already at genesis in this build (then unchanged).
        reasons.push(format!("{at_genesis} fence(s) asked at genesis"));
    }
    let _ = at_height;
    cx.record("flag_day", flag_day);
    Ok(KindOutcomeV1::at(
        PalwExtensionClassificationV1::RulesetChange { fences: deltas, would_print: Some(arm), flag_day, reason: reasons.join("; ") },
        cx.depth,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};

    /// **Every fence this build's `Params` names has an arm here.** A fence added to `Params` is
    /// named in `palw_fences_v1` by the compiler's exhaustive destructure; this walks that list
    /// so the same fence cannot be missing from the candidate kind without this going red.
    #[test]
    fn every_fence_the_build_names_can_be_set_by_name() {
        let params: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        for (name, _) in params.palw_fences_v1() {
            let mut arming = params.clone();
            match set_fence_by_name(&mut arming, name, ForkActivation::new(9_000_000)) {
                Ok(()) => {
                    let after = arming.palw_fences_v1().into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| v);
                    assert_eq!(
                        after,
                        Some(ForkActivation::new(9_000_000)),
                        "{name}: setting by name did not land on the fence the list reads"
                    );
                }
                Err(why) => assert!(why.contains("companion value"), "{name}: {why}"),
            }
        }
        assert!(set_fence_by_name(&mut params.clone(), "palw_no_such_fence", ForkActivation::always()).is_err());
    }
}
