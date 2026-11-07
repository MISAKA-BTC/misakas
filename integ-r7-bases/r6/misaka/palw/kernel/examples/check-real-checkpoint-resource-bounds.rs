//! Geometry-only active-code quotation from a bounded actual container header.
//! Long-context scopes that exceed fixed courts remain refusals. No model tensor
//! is imported or executed, and these reports cannot count as Final or coverage.
use misaka_palw_kernel::{flat_scope9::flat_resource_scope_v9, merkle5::flat_material_bounds_v5, trace::WiringV1};
use misaka_palw_tir::{Prim, TirProgramV1};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut a = std::env::args().skip(1).collect::<Vec<_>>();
    let transformed = a.iter().any(|s| s == "--transpose-history15");
    a.retain(|s| s != "--transpose-history15");
    let price20 = a.iter().any(|s| s == "--price20");
    a.retain(|s| s != "--price20");
    let program_output = a.iter().find_map(|s| s.strip_prefix("--derived-program=").map(str::to_owned));
    a.retain(|s| !s.starts_with("--derived-program="));
    if a.len() < 2 {
        return Err("usage: check-real-checkpoint-resource-bounds <artifact> <positions>...".into());
    }
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open_bounded(Path::new(&a[0]), 32 << 30, 4 << 20)?;
    let original = TirProgramV1::decode_canonical(&container.header.program)?;
    let pass =
        if transformed { Some(misaka_palw_tir_lower::layout_pass15::transpose_history_contractions_v15(&original)?) } else { None };
    let p = pass.as_ref().map_or(&original, |r| &r.program);
    if let Some(path) = program_output {
        use std::io::Write;
        std::fs::OpenOptions::new().write(true).create_new(true).open(path)?.write_all(&p.encode())?;
    }
    let w = WiringV1::new(&p)?;
    let nodes = w.occurrences.iter().map(|(b, _)| p.blocks[*b as usize].nodes.len() as u64).sum::<u64>();
    let max_inputs = p.blocks.iter().flat_map(|b| &b.nodes).map(|n| n.inputs.len()).max().unwrap_or(0);
    let mut reports = vec![];
    for arg in &a[1..] {
        let positions: u32 = arg.parse()?;
        if positions == 0 || positions > p.history_bound {
            return Err("positions outside original program history bound".into());
        }
        let mut topk = 0;
        let mut history = 0;
        let mut derived = 0;
        let mut maximum_leaf_replies = 0;
        let mut primitive_histogram = std::collections::BTreeMap::<String, u64>::new();
        let (mut added_payload, mut added_reference) = (0u128, 0u128);
        if let Some(pass) = &pass {
            for r in &pass.rewrites {
                for (s, (b, _)) in w.occurrences.iter().enumerate().filter(|(_, (b, _))| *b == r.block) {
                    let h = w.h(s as u16, positions - 1);
                    for n in r.first_added_node..r.replacement_node {
                        let ty = &p.blocks[*b as usize].nodes[n as usize].out;
                        let elements = ty.resolve(h).iter().map(|n| *n as u128).product::<u128>();
                        added_payload += elements * ty.dtype.width() as u128;
                        added_reference += elements * 16;
                    }
                }
            }
        }
        for (s, (b, _)) in w.occurrences.iter().enumerate() {
            let h = w.h(s as u16, positions - 1);
            for (n, node) in p.blocks[*b as usize].nodes.iter().enumerate() {
                *primitive_histogram.entry(format!("{:?}", node.prim)).or_default() += 1;
                topk += usize::from(matches!(node.prim, Prim::TopK { .. }));
                history += usize::from(matches!(node.prim, Prim::HistAppend { .. }));
                derived += usize::from(w.is_derived(s as u16, n as u16));
                let shape = node.out.resolve(h).iter().map(|n| *n as u64).collect::<Vec<_>>();
                if let Some(m) = flat_material_bounds_v5(node.out.dtype, &shape) {
                    maximum_leaf_replies = maximum_leaf_replies.max(m.localization_replies);
                }
            }
        }
        // Same geometry proved by the SDK root locator tests; this standalone
        // quote does not execute discovery or authenticate any reply.
        let log = |n: u64| if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() };
        let root_reads = log(positions.min(misaka_palw_kernel::seg::SEG_LEN_V4) as u64) + log(nodes) + 1;
        let scope = match flat_resource_scope_v9(&p, positions, None) {
            Ok(s) => {
                serde_json::json!({"status":"PASS","relations":s.relations.len(),"metadata_relation_visits":s.metadata_relation_visits,
                "metadata_derivation_work":s.metadata_derivation_work,"worst_court_bytes":s.worst_court_bytes,
                "worst_court_work":s.worst_court_work,"worst_court_scratch_bytes":s.worst_court_scratch_bytes,
                "trace_value_bytes_per_position":s.trace_value_bytes_per_position,"reference_tensor_bytes_per_position":s.reference_tensor_bytes_per_position,
                "trace_value_bytes_per_claim":s.trace_value_bytes_per_claim,"node_commitments_bytes_per_claim":s.node_commitments_bytes_per_claim,
                "maximum_operand_leaves":s.relations.iter().flat_map(|r|&r.operand_leaves).max()})
            }
            Err(e) => serde_json::json!({"status":"FAIL","refusal":format!("{e:?}")}),
        };
        let mut new_failures = std::collections::BTreeMap::<String, u64>::new();
        let mut new_max_bytes = 0;
        let mut new_max_work = 0;
        let mut new_max_scratch = 0;
        let mut new_max_leaves = 0;
        let mut candidate_passed = 0;
        let mut sample_failures = vec![];
        let mut reduction_samples = vec![];
        // A report-only field image lets the same renderer show both distinct
        // price namespaces. It is never serialized as a v15 filing/root/fence.
        let quotes = if price20 {
            misaka_palw_kernel::flat_price20::all_point_relation_quotes_v20(&p, positions, None, positions - 1)
                .map_err(|e| format!("price20 metadata quote refused: {e:?}"))?
                .into_iter()
                .map(|(s, n, q)| {
                    (
                        s,
                        n,
                        q.map(|q| misaka_palw_kernel::flat_price15::PointRelationPriceV15 {
                            version: q.version,
                            court_id: q.court_id,
                            position: q.position,
                            occurrence: q.occurrence,
                            node: q.node,
                            output_elements: q.output_elements,
                            output_leaf_bytes: q.output_leaf_bytes,
                            operand_kinds: q.operand_kinds,
                            operand_leaves: q.operand_leaves,
                            operand_reads: q.operand_reads,
                            operand_leaf_bytes: q.operand_leaf_bytes,
                            filing_bytes: q.filing_bytes,
                            work: q.work,
                            scratch_bytes: q.scratch_bytes,
                        }),
                    )
                })
                .collect::<Vec<_>>()
        } else {
            misaka_palw_kernel::flat_price15::all_point_relation_quotes_v15(&p, positions, None, positions - 1)
                .map_err(|e| format!("price15 metadata quote refused: {e:?}"))?
        };
        for (s, n, price) in quotes {
            let prim = &w.node(s, n).prim;
            match price {
                Ok(price) => {
                    if !misaka_palw_kernel::flat_price15::price_within_frozen_court_caps_v15(&price) {
                        let why = format!("{prim:?}: ordinary point relation exceeds frozen court resource caps");
                        *new_failures.entry(why.clone()).or_default() += 1;
                        if sample_failures.len() < 8 {
                            sample_failures.push(serde_json::json!({"occurrence":s,"node":n,"refusal":why,
                                "requested_operand_leaves":price.operand_leaves,"requested_operand_reads":price.operand_reads,
                                "requested_operand_leaf_bytes":price.operand_leaf_bytes,"requested_filing_bytes":price.filing_bytes,
                                "requested_work":price.work,"requested_scratch_bytes":price.scratch_bytes,
                                "requested_chunks32KiB":(price.filing_bytes+32767)/32768,"bounded_price_refuses":true}));
                        }
                        continue;
                    }
                    candidate_passed += 1;
                    new_max_bytes = new_max_bytes.max(price.filing_bytes);
                    new_max_work = new_max_work.max(price.work);
                    new_max_scratch = new_max_scratch.max(price.scratch_bytes);
                    new_max_leaves = new_max_leaves.max(price.operand_leaves.iter().sum::<u64>());
                    if matches!(prim, Prim::ReduceMax { .. } | Prim::ReduceSum { .. }) && reduction_samples.len() < 8 {
                        reduction_samples.push(serde_json::json!({"occurrence":s,"node":n,"primitive":format!("{prim:?}"),
                            "operand_leaves":price.operand_leaves,"operand_reads":price.operand_reads,"operand_leaf_bytes":price.operand_leaf_bytes,
                            "filing_bytes":price.filing_bytes,"work":price.work,"scratch_bytes":price.scratch_bytes,
                            "route_chunks16KiB":(price.filing_bytes+16383)/16384}));
                    }
                }
                Err(e) => {
                    let why = format!("{prim:?}: {e:?}");
                    *new_failures.entry(why.clone()).or_default() += 1;
                    if sample_failures.len() < 8 {
                        sample_failures.push(serde_json::json!({"occurrence":s,"node":n,"refusal":why}));
                    }
                }
            }
        }
        let mut report = serde_json::json!({"positions":positions,"node_count":nodes,"maximum_primitive_inputs":max_inputs,
            "root_localization_successful_read_bound":root_reads,"maximum_flat_leaf_localization_replies":maximum_leaf_replies,
            "ordinary_followup_callback_bound":3+maximum_leaf_replies+max_inputs as u64,
            "ordinary_total_callback_bound":root_reads as u64+3+maximum_leaf_replies+max_inputs as u64,
            "TopK_relations":topk,"history_append_relations":history,"derived_history_relations":derived,
            "greedy_decode_logits_elements":p.token_bound,"primitive_histogram":primitive_histogram,"scope9":scope,
            "candidate_price15_at_max_position_only":{"passed_relations":candidate_passed,"refusals":new_failures,
                "maximum_passed_filing_bytes":new_max_bytes,"maximum_passed_work":new_max_work,"maximum_passed_scratch_bytes":new_max_scratch,
                "maximum_passed_input_leaves":new_max_leaves,"first_refusals":sample_failures,"reduction_samples":reduction_samples,
                "no_activated_descriptor_checker_or_terminal_court":true,"full_scope_or_admission_or_final_established":false},
            "layout_pass15":{"applied":transformed,"added_trace_payload_bytes_at_final_position":added_payload,
                "added_reference_i128_bytes_at_final_position":added_reference,
                "added_trace_payload_per_claim_upper_bound":added_payload*positions as u128,
                "not_a_measurement_or_complete_execution_retention_peak":true},
            "ordinary_bound_does_not_certify_history_TopK_decode_or_encoder":true});
        if price20 {
            let cost = report.as_object_mut().unwrap().remove("candidate_price15_at_max_position_only").unwrap();
            report.as_object_mut().unwrap().insert("candidate_price20_at_max_position_only".into(), cost);
        }
        reports.push(report);
    }
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.actual-checkpoint-geometry-bounds.v1",
        "program_history_bound":p.history_bound,"source_program_bytes":container.header.program.len(),"quoted_program_bytes":p.encode().len(),
        "layout_pass15_rewrites":pass.as_ref().map(|r|r.rewrites.len()),"new_class_and_program_binding_required":transformed,
        "candidate_price_version":if price20 {20} else {15},
        "candidate_cost_plan_id":(if price20 {misaka_palw_kernel::flat_price20::candidate_point_price_id_v20()} else {misaka_palw_kernel::flat_price15::candidate_point_price_id_v15()}).iter().map(|b|format!("{b:02x}")).collect::<String>(),
        "reports":reports,"actual_weight_import_execution_final_or_coverage":false})
    );
    Ok(())
}
