//! Exact canonical metadata/parameter-set/range audit of the isolated ordinary scalar court.
//! No v7 candidate, shipping route, native registration, execution rights or Final is granted.
use misaka_palw_kernel::{flat_point9::PointRelationScopeV9, flat_scope9::*, trace::ParamCommitmentsV1};
use misaka_palw_tir::program::TirProgramV1;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Read, path::Path};
fn bounded(path: &Path, max: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut out = vec![];
    std::fs::File::open(path)?.take(max + 1).read_to_end(&mut out)?;
    if out.len() as u64 > max {
        return Err("metadata byte cap".into());
    }
    Ok(out)
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}
fn point(p: &PointRelationScopeV9) -> serde_json::Value {
    serde_json::json!({"position":p.position,"occurrence":p.occurrence,"node":p.node,"output_elements":p.output_elements,"output_leaf_bytes":p.output_leaf_bytes,"operand_kinds":p.operand_kinds.iter().map(|k|format!("{k:?}")).collect::<Vec<_>>(),"operand_leaves":p.operand_leaves,"operand_reads":p.operand_reads,"operand_leaf_bytes":p.operand_leaf_bytes,"filing_bytes":p.filing_bytes,"work":p.work,"scratch_bytes":p.scratch_bytes,"price_root":misaka_palw_sdk::runtime_pack::commit::hex(&p.root())})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: check-flat-checkpoint-admission9 <program.tir> <param-commitments.borsh> <max-positions>".into());
    }
    let bytes = bounded(Path::new(&args[0]), 8 << 20)?;
    let pcbytes = bounded(Path::new(&args[1]), 64 << 20)?;
    let pc: ParamCommitmentsV1 = borsh::from_slice(&pcbytes)?;
    let program = TirProgramV1::decode_canonical(&bytes)?;
    let maximum: u32 = args[2].parse()?;
    let mut want = BTreeSet::new();
    for (j, p) in program.params.iter().enumerate() {
        if p.per_layer {
            for layer in 0..program.schedule.layers.len() {
                want.insert((j as u16, Some(layer as u16)));
            }
        } else {
            want.insert((j as u16, None));
        }
    }
    if pc.by_instance.keys().copied().collect::<BTreeSet<_>>() != want {
        return Err("parameter commitment set differs from canonical declared instances".into());
    }
    misaka_palw_tir::interval::analyze_ranges(&program)?;
    let started = std::time::Instant::now();
    let first = first_legacy_ordinary_refusal_v9(&program, maximum, None).map_err(|e| format!("{e:?}"))?;
    let diagnostic=first.map(|d|serde_json::json!({"position":d.position,"occurrence":d.occurrence,"block":d.block,"node":d.node,"primitive":d.primitive,"output_shape":d.output_shape,"input_shapes":d.input_shapes,"v7_scope_refusal":d.error,"ordinary_point_replacement":d.point_replacement.as_ref().map(point).map_err(|s|s.as_str())}));
    let mut out = serde_json::json!({"schema":"misaka.rfc02.actual-checkpoint-flat-v9-metadata-audit.v1","program_sha256":digest(&bytes),"parameter_map_sha256":digest(&pcbytes),"parameter_root":misaka_palw_sdk::runtime_pack::commit::hex(&pc.root()),"parameter_instances":pc.by_instance.len(),"max_positions":maximum,"canonical_program_and_exact_ranges_and_parameter_set_passed":true,"first_v7_ordinary_scope_refusal":diagnostic,"source_weights_loaded_or_authenticated":false,"native_candidate_or_execution_or_final":false,"full_rfc02_complete":false});
    match flat_resource_scope_v9(&program, maximum, None) {
        Err(e) => {
            out["complete_v9_metadata_scope_passed"] = false.into();
            out["refusal"] = format!("{e:?}").into();
        }
        Ok(s) => {
            check_flat_resource_scope_v9(&program, maximum, None, &s).map_err(|e| format!("{e:?}"))?;
            out["complete_v9_metadata_scope_passed"] = true.into();
            out["scope_root"] = misaka_palw_sdk::runtime_pack::commit::hex(&s.root()).into();
            out["profile"] = misaka_palw_sdk::runtime_pack::commit::hex(&s.profile).into();
            out["court_set"] = misaka_palw_sdk::runtime_pack::commit::hex(&s.court_set).into();
            out["relations"] = s.relations.len().into();
            out["metadata_relation_visits"] = serde_json::json!(s.metadata_relation_visits);
            out["metadata_derivation_work"] = serde_json::json!(s.metadata_derivation_work);
            out["worst_court_bytes"] = s.worst_court_bytes.into();
            out["worst_court_work"] = s.worst_court_work.into();
            out["worst_court_scratch_bytes"] = s.worst_court_scratch_bytes.into();
            let full = misaka_palw_kernel::flat_court9::complete_scope_api_bounds_v9(&s).map_err(|e| format!("{e:?}"))?;
            out["complete_mixed_api_bound_including_scope_preparation"] =
                serde_json::json!({"filing_bytes":full.filing_bytes,"work":full.work,"scratch_bytes":full.scratch_bytes});
            out["maximum_ordinary_operand_leaves_plus_output"] = serde_json::json!(
                s.relations.iter().filter(|r| r.ordinary_point).map(|r| r.operand_leaves.iter().sum::<u64>() + 1).max().unwrap_or(0)
            );
            out["trace_value_bytes_per_position"] = serde_json::json!(s.trace_value_bytes_per_position);
            out["reference_tensor_bytes_per_position"] = serde_json::json!(s.reference_tensor_bytes_per_position);
            out["trace_value_bytes_per_claim"] = serde_json::json!(s.trace_value_bytes_per_claim);
            out["node_commitments_bytes_per_claim"] = serde_json::json!(s.node_commitments_bytes_per_claim);
        }
    }
    out["elapsed_ms"] = serde_json::json!(started.elapsed().as_millis());
    println!("{out}");
    Ok(())
}
