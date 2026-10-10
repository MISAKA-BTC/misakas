//! `static`: everything the plan and the program fix about a class, with no weight read — the prosecution bounds the registration
//! gate derives, whether they fit the interim OPV carriers (and the route's own ceilings), and the OPV registration economics.
use crate::util::*;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_kernel_route_v1::{PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1, palw_kernel_route_policy_v1};
use kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1;
use kaspa_hashes::Hash64;
use misaka_palw_kernel::check::check_plan_v1;
use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::gate::public_prosecution_complete_v1;
use misaka_palw_kernel::ledger::carrier_fit_v1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{ProfileMaterialV1, program_root_v1};
use misaka_palw_kernel::route::{MAX_COMMIT_CLAIM_BYTES_V1, MAX_FILE_PROOF_BYTES_V1, MAX_RESPOND_BYTES_V1};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use serde_json::{Value, json};
use std::path::Path;

pub fn run(args: &[String]) -> Result<Value, String> {
    let path = arg(args, "--container").ok_or("--container PATH")?;
    let label = arg(args, "--label").unwrap_or_else(|| path.clone());
    let positions: Vec<u32> = arg(args, "--positions")
        .unwrap_or_else(|| "4,64,512".into())
        .split(',')
        .map(|s| s.trim().parse().map_err(|e| format!("--positions: {e}")))
        .collect::<Result<_, _>>()?;
    let c = PalwTirContainerV1::open(Path::new(&path)).map_err(|e| e.to_string())?;
    let program = &c.program;
    let program_bytes = c.header.program.clone();
    let root = program_root_v1(&program_bytes);
    let params: u64 =
        c.header.tensors.iter().map(|e| program.params[e.param as usize].shape.iter().map(|x| *x as u64).product::<u64>()).sum();
    let nodes: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    let ledger_policy = palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    let fence = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), vec![]);
    let opv = fence.opv_policy();
    let mut per_p = Vec::new();
    for p in positions {
        let mut row = json!({"max_positions": p});
        for (dname, d) in [("K2-TIR-v1", k2_tir_v1_descriptor()), ("K2-TIR-v2", k2_tir_v2_descriptor())] {
            let plan = match plan_for_tir_program_v1(&d, program, root, p) {
                Ok(pl) => pl,
                Err((fam, why)) => {
                    row[dname] = json!({"plan": format!("refused: {fam:?}: {why}")});
                    continue;
                }
            };
            let b = plan.budgets;
            let schedule = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
            let accept = match check_plan_v1(&schedule, &d, program, root, &plan, 0) {
                Ok(a) => {
                    json!({"ok": true, "error_bits": a.error_bits, "claim_verifier_work": a.claim_verifier_work.to_string(), "claim_evidence_bytes": a.claim_evidence_bytes.to_string()})
                }
                Err(o) => json!({"ok": false, "why": format!("{o:?}")}),
            };
            let gate =
                public_prosecution_complete_v1(&d, &plan, nodes, &ProfileMaterialV1::kernel_route(true), &ledger_policy.prosecution);
            let (gate_json, bounds) = match &gate {
                Ok(bd) => (
                    json!({"ok": true, "max_public_bytes": bd.max_public_bytes.to_string(), "max_opening_bytes": bd.max_opening_bytes,
                           "max_filing_bytes": bd.max_filing_bytes, "max_response_bytes": bd.max_response_bytes.to_string(),
                           "max_court_work": bd.max_court_work, "max_verifier_ram": bd.max_verifier_ram.to_string(),
                           "max_retained_state": bd.max_retained_state.to_string(), "max_concurrent_sessions": bd.max_concurrent_sessions}),
                    Some(*bd),
                ),
                Err(gaps) => (json!({"ok": false, "gaps": gaps.iter().map(|g| format!("{g:?}")).collect::<Vec<_>>()}), None),
            };
            let mut fit = Value::Null;
            if let Some(bd) = bounds {
                let interim = carrier_fit_v1(
                    &bd,
                    opv.carrier.filing_cap as usize,
                    opv.carrier.response_cap as usize,
                    opv.carrier.commit_cap as usize,
                );
                let ceilings = carrier_fit_v1(&bd, MAX_FILE_PROOF_BYTES_V1, MAX_RESPOND_BYTES_V1, MAX_COMMIT_CLAIM_BYTES_V1);
                let cost = opv.censorship_cost(&ledger_policy, bd.max_court_work);
                let gain = opv.max_gain_per_claim(&ledger_policy);
                fit = json!({
                    "interim_carriers": {"filing_cap": opv.carrier.filing_cap, "response_cap": opv.carrier.response_cap, "commit_cap": opv.carrier.commit_cap,
                                          "fits": interim.is_ok(), "why": interim.err()},
                    "route_ceilings": {"fits": ceilings.is_ok(), "why": ceilings.err()},
                    "censorship_cost_sompi": cost.to_string(), "max_gain_sompi": gain.to_string(), "censorship_cost_exceeds_gain": cost > gain,
                });
            }
            row[dname] = json!({
                "budgets": {"verifier_work_per_position": b.verifier_work_per_position.to_string(), "evidence_bytes_per_position": b.evidence_bytes_per_position.to_string(),
                            "artifact_bytes": b.artifact_bytes.to_string(), "probabilistic_instances_per_position": b.probabilistic_instances_per_position,
                            "worst_court_bytes": b.worst_court_bytes, "worst_court_work": b.worst_court_work},
                "declared_error_bits": plan.declared_error_bits,
                "check_plan": accept, "gate": gate_json, "opv": fit,
            });
        }
        per_p.push(row);
    }
    Ok(json!({
        "cmd": "static", "label": label, "container": path, "container_bytes": c.file_len, "program_bytes": program_bytes.len(),
        "params": params, "param_instances": c.header.tensors.len(), "blocks": program.blocks.len(), "occurrences": program.occurrences().len(),
        "committed_nodes_per_position": nodes, "token_bound": program.token_bound, "history_bound": program.history_bound,
        "route_object_cap": PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1, "load": loadavg(), "by_positions": per_p,
    }))
}
