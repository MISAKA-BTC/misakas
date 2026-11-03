//! Offline resource quote for a retained TIR worker result under the compiled public t12 settings.
//! The EOF certificate is a fixture. This example does not contact a node or produce a carrier.
use kaspa_consensus_core::{
    Hash64,
    config::params::palw_t12_shipped_params,
    palw_artifact::{
        palw_artifact_multiproof_borsh_len_v1, palw_artifact_multiproof_sibling_count_v1, palw_artifact_operand_borsh_len_v1,
    },
    palw_class_semantics_v1::PalwClassSemanticsDeclarationV1,
    palw_fp_execution_v3::palw_fp_job_pin_v1,
    palw_freeprompt_v3::PalwFpWorkerResultV3,
    palw_mode_v2::PalwConsensusMode,
    palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    palw_staged_preclaim_v1::PalwStagedClaimGeometryV1,
    palw_state_v2::PalwBondKeyV2,
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_court_v1::{
        PalwTirInventoryIndexV1,
        parameter_cover::{ParameterCoverBudgetV1, derive_parameter_cover_v1},
    },
    palw_tir_frame_cost_v1::tir_public_owner_queue_wait_v1,
    palw_tir_public_allocation_v1::{
        PalwTirPublicAllocationQuoteV1, PalwTirPublicAllocationRulesV1, derive_tir_public_allocation_native_v2,
    },
    palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1,
    palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1,
};
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use serde_json::json;
use std::{io::Read, path::Path};

fn read(path: &Path, cap: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(format!("{} exceeds the diagnostic input cap of {cap} bytes", path.display()).into());
    }
    Ok(bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [artifact, tokenizer, result, binding, semantics, daa, custodians, rest @ ..] = args.as_slice() else {
        return Err("usage: check-t12-tfp1-door <model.palwtir> <tokenizer.json> <worker-result.frame> <tir-public-binding.hex> <class-semantics.borsh> <DAA> <custodian-upper> [disputed-leaf]".into());
    };
    let disputed_leaf: Option<u64> = match rest {
        [] => None,
        [leaf] => Some(leaf.to_str().ok_or("disputed leaf must be text")?.parse()?),
        _ => return Err("at most one disputed leaf can be measured per invocation".into()),
    };
    let daa: u64 = daa.to_str().ok_or("DAA must be text")?.parse()?;
    let maximum_custodians: u64 = custodians.to_str().ok_or("custodian upper must be text")?.parse()?;
    if daa < 2 || maximum_custodians == 0 {
        return Err("DAA must be at least 2 and custodian upper must be positive".into());
    }
    let entry = TirLineageV1::open_entry(Path::new(artifact))?;
    let declaration: PalwClassSemanticsDeclarationV1 = borsh::from_slice(&read(Path::new(semantics), 4096)?)?;
    declaration.check()?;
    let raw = read(Path::new(tokenizer), 32 << 20)?;
    if declaration.class_id != entry.class_id()
        || declaration.artifact_root != entry.artifact_root
        || declaration.tokenizer_id != entry.class.tokenizer_id
        || declaration.tokenizer_id != palw_tokenizer_raw_commitment_v1(&raw)
        || declaration.raw_tokenizer_bytes != raw.len() as u64
    {
        return Err("the declared class, artifact and exact raw tokenizer must agree".into());
    }
    let frame = read(Path::new(result), 16 << 20)?;
    let (length, body) = frame.split_at_checked(4).ok_or("truncated worker result length")?;
    if u32::from_le_bytes(length.try_into()?) as usize != body.len() {
        return Err("worker result length does not match its frame".into());
    }
    let result: PalwFpWorkerResultV3 = borsh::from_slice(body)?;
    let encoded = read(Path::new(binding), 4097)?;
    let encoded = std::str::from_utf8(&encoded)?.trim().as_bytes();
    if !encoded.len().is_multiple_of(2) {
        return Err("binding header has an odd number of hex digits".into());
    }
    let bytes: Vec<u8> = encoded
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    let PalwPublicExecutionBindingHeaderV1::Tir(header) = borsh::from_slice(&bytes)? else {
        return Err("a TIR binding header is required".into());
    };

    let params = palw_t12_shipped_params();
    let cost = params.palw_tir_frame_cost_v1.ok_or("the compiled t12 has no TFP1 cost")?;
    let PalwConsensusMode::ConsensusV2(ref bundle) = params.palw_consensus_mode else {
        return Err("the compiled t12 has no PALW V2 bundle".into());
    };
    let original = result.to_commitment(daa.checked_add(40_000).ok_or("fixture retention DAA overflow")?);
    let geometry = PalwStagedClaimGeometryV1 {
        class_id: entry.class_id(),
        producer: PalwBondKeyV2(original.job.executor_bond),
        execution_root: original.execution_root,
        job_identity: palw_fp_job_pin_v1(&original),
        accepted_daa: daa,
    };
    let inventory = PalwTokenizerArtifactInventoryCertificateV1 {
        artifact_hash: declaration.tokenizer_id,
        actual_bytes: raw.len() as u64,
        inventory_id: Hash64::from_u64_word(1),
        opener: geometry.producer,
        opened_daa: daa - 2,
        completed_daa: daa - 1,
    };
    let (record, _) = palw_tir_class_record_v1(&entry.class, &entry.artifact_root)?;
    let rules = PalwTirPublicAllocationRulesV1 {
        maximum_leaves: original.work_leaves,
        maximum_custodians,
        court: bundle.court,
        dissection: true,
        prompt_form: params.palw_prompt_ids_form_v1(),
        config: cost.config,
        clock: cost.clock,
    };
    let bound = declaration.as_bound_row(daa - 2);
    let quote = |rules, semantics, inventory| {
        derive_tir_public_allocation_native_v2(
            &geometry,
            &original,
            &header,
            &record,
            &entry.class.layout,
            semantics,
            inventory,
            rules,
        )
        .map_err(|err| format!("native quote failed: {err:?}"))
    };
    let public = match quote(rules, Some(&bound), Some(&inventory))? {
        PalwTirPublicAllocationQuoteV1::Complete(plan) => json!({"complete":true, "collect_daa":plan.terms.collect_until_daa-daa}),
        PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. } => json!({"complete":false,"missing":format!("{missing:?}")}),
    };
    // Isolate the non-tokenizer terms with an unbounded diagnostic door. The
    // resulting resource breakdown is not an admissible public quote.
    let mut diagnostic_rules = rules;
    diagnostic_rules.clock.maximum_door_daa = u64::MAX;
    let non_tokenizer = match quote(diagnostic_rules, None, None)? {
        PalwTirPublicAllocationQuoteV1::Complete(plan) => json!({
            "preparation_daa":plan.preparation_until_daa-daa,
            "acquisition_inclusion_daa":plan.acquisition.envelope.inclusion_daa,
            "owner_queue_daa":tir_public_owner_queue_wait_v1(
                cost.config.acquisition_daa + cost.config.prices.inclusion_margin_daa, &cost.config, &cost.clock
            )?,
            "collect_daa":plan.terms.collect_until_daa-daa,
        }),
        PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. } => json!({"missing":format!("{missing:?}")}),
    };
    let parameter_cover = if let Some(leaf) = disputed_leaf {
        let binding =
            header.resolve_registered_layout_v1(&record, &entry.class.layout, original.execution_root, rules.maximum_leaves)?;
        let budget = ParameterCoverBudgetV1 {
            max_edges: kaspa_consensus_core::palw_g14_values_v1::PALW_G14_PARAMETER_COVER_BUDGET_V1.max_total_edges,
            max_coordinates: kaspa_consensus_core::palw_g14_values_v1::PALW_G14_PARAMETER_COVER_BUDGET_V1.max_total_coordinates,
        };
        match derive_parameter_cover_v1(&binding, leaf, budget) {
            Err(why) => json!({"leaf":leaf,"refusal":why}),
            Ok(cover) if cover.leaves.is_empty() => json!({"leaf":leaf,"inventory_leaves":0,"multiproof_bytes":0}),
            Ok(cover) => {
                let program = &entry.artifact.plan().program;
                let index = PalwTirInventoryIndexV1::new(program).ok_or("the original program has no usable inventory index")?;
                let siblings = palw_artifact_multiproof_sibling_count_v1(index.leaf_count(), &cover.leaves)
                    .ok_or("the cover does not name a canonical set of inventory leaves")?;
                let operands = cover
                    .leaves
                    .iter()
                    .map(|&position| {
                        let (param, layer, _, len) =
                            index.piece_of(position).ok_or("cover position outside the original inventory")?;
                        let name = &program.params.get(usize::from(param)).ok_or("cover parameter outside the program")?.name;
                        Ok::<_, &str>(palw_artifact_operand_borsh_len_v1(name.len(), layer.is_some(), len as usize))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                json!({
                    "leaf":leaf,"inventory_leaves":cover.leaves.len(),"siblings":siblings,
                    "multiproof_bytes":palw_artifact_multiproof_borsh_len_v1(operands,siblings),
                    "explored_edges":cover.explored_edges,"explored_coordinates":cover.explored_coordinates,
                })
            }
        }
    } else {
        serde_json::Value::Null
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "params_id":params.consensus_params_id().to_string(),
            "class_id":entry.class_id().to_string(),
            "daa":daa,
            "fixture_eof":true,
            "custodian_upper":maximum_custodians,
            "maximum_door_daa":cost.clock.maximum_door_daa,
            "public_cost_quote":public,
            "non_tokenizer_diagnostic":non_tokenizer,
            "disputed_leaf_parameter_cover":parameter_cover,
        }))?
    );
    Ok(())
}
