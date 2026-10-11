//! Metadata-only audit of a pinned canonical checkpoint program/parameter map at an exact length.
//! This derives current production v7 scope/plan and reports the interim consumer's actual
//! component caps and deterministic registration tariff. It neither attests nor registers a
//! class, and cannot grant OPV eligibility, execution rights or Final.
use kaspa_consensus_core::{Hash64, palw_kernel_route_v1::palw_kernel_route_policy_v1};
use misaka_palw_kernel::{
    descriptor::k2_tir_v4_descriptor,
    ledger::flat_candidate7::{
        FLAT_CANDIDATE_METADATA_PASSES_V7, derive_flat_candidate_v7, flat_candidate_metadata_work_v7,
        flat_candidate_registration_work_v7,
    },
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::TirProgramV1;
use std::io::Read;
use std::path::Path;

fn read(path: &Path, maximum: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err("metadata file byte limit".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: check-flat-checkpoint-admission7 <program.tir> <params.borsh> <max-positions>".into());
    }
    let program = read(Path::new(&args[0]), 8 << 20)?;
    let pc: ParamCommitmentsV1 = borsh::from_slice(&read(Path::new(&args[1]), 64 << 20)?)?;
    let maximum: u32 = args[2].parse()?;
    let role = k2_tir_v4_descriptor();
    let policy = palw_kernel_route_policy_v1(Hash64::from_bytes([0; 64]), Hash64::from_bytes([0; 64]));
    let predecode_work = flat_candidate_registration_work_v7(program.len(), pc.by_instance.len());
    let decoded = TirProgramV1::decode_canonical(&program);
    let metadata_work = decoded.as_ref().ok().and_then(flat_candidate_metadata_work_v7);
    let work = predecode_work.zip(metadata_work).and_then(|(p, m)| p.checked_add(m));
    let priced = work.is_some_and(|work| work <= policy.admission_work_limit_v1());
    let mut result = serde_json::json!({
        "schema": "misaka.rfc02.actual-checkpoint-flat-v7-staged-metadata-audit.v2",
        "program_bytes": program.len(), "parameter_instances": pc.by_instance.len(),
        "max_positions": maximum, "descriptor": misaka_palw_sdk::runtime_pack::commit::hex(&role.digest()),
        "parameter_root": misaka_palw_sdk::runtime_pack::commit::hex(&pc.root()),
        "canonical_program_decode_passed": decoded.is_ok(),
        "predecode_work": predecode_work, "scheduled_metadata_work": metadata_work,
        "scheduled_metadata_passes": FLAT_CANDIDATE_METADATA_PASSES_V7,
        "registration_work": work, "interim_consumer_admission_work_limit": policy.admission_work_limit_v1(),
        "predecode_tariff_fits_interim_consumer": predecode_work.is_some_and(|w| w <= policy.admission_work_limit_v1()),
        "registration_tariff_fits_interim_consumer": priced,
        "guaranteed_proof_work": policy.guaranteed_proof_work_v1(),
        "ordinary_component_memory_limit": policy.prosecution.max_verifier_ram,
        "source_weights_loaded_or_authenticated_by_this_run": false,
        "node_state_admission_or_activation_checked": false,
        "complete_task_or_source_fidelity_proven": false,
        "class_registered_execution_eligible_or_final_proven": false,
        "full_rfc02_complete": false,
    });
    if let Ok(p) = decoded {
        result["template_nodes"] = p.blocks.iter().map(|b| b.nodes.len()).sum::<usize>().into();
        result["scheduled_nodes"] = p.occurrences().iter().map(|(b, _)| p.blocks[*b as usize].nodes.len()).sum::<usize>().into();
    }
    match derive_flat_candidate_v7(&role.digest(), &program, &pc, maximum) {
        Err(error) => {
            result["scope_derivation_passed"] = false.into();
            result["scope_refusal"] = error.into();
            result["all_metadata_components_fit_interim_consumer"] = false.into();
        }
        Ok(view) => {
            let components = [
                view.replay.reference.position_tensor_bytes,
                view.replay.reference.largest_operand_tensor_bytes,
                view.replay.reference.history_rows_tensor_bytes,
                view.replay.topk_auxiliary_bytes,
            ];
            let ram = components.iter().all(|n| *n <= policy.prosecution.max_verifier_ram);
            let court = view.scope.worst_court_work <= policy.guaranteed_proof_work_v1();
            result["scope_derivation_passed"] = true.into();
            result["scope_root"] = misaka_palw_sdk::runtime_pack::commit::hex(&view.plan.scope_root).into();
            result["replay_root"] = misaka_palw_sdk::runtime_pack::commit::hex(&view.plan.replay_root).into();
            result["plan_root"] = misaka_palw_sdk::runtime_pack::commit::hex(&view.plan.root()).into();
            result["ordinary_component_memory_bytes"] = serde_json::to_value(components)?;
            result["ordinary_components_fit_interim_memory"] = ram.into();
            result["worst_court_bytes"] = view.scope.worst_court_bytes.into();
            result["worst_court_work"] = view.scope.worst_court_work.into();
            result["worst_court_scratch_bytes"] = view.scope.worst_court_scratch_bytes.into();
            result["court_fits_guaranteed_proof_work"] = court.into();
            result["all_metadata_components_fit_interim_consumer"] = (priced && ram && court).into();
        }
    }
    // A refusal is a successfully measured refusal. Do not disguise it as admission.
    println!("{result}");
    Ok(())
}
