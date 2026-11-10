//! Offline resource quote for a retained TIR worker result under the compiled public t12 settings.
//! The EOF certificate is a fixture. The optional authenticated table comparison uses the
//! family V4 allocation locally. It does not check network activation or produce a carrier.
//! `--dense-replay` checks the retained original's numerical roots with the existing court
//! builder and checker and reports their maximum replay cost.
use kaspa_consensus_core::{
    Hash64,
    config::params::palw_t12_shipped_params,
    palw_artifact::{
        palw_artifact_multiproof_borsh_len_v1, palw_artifact_multiproof_sibling_count_v1, palw_artifact_operand_borsh_len_v1,
    },
    palw_class_semantics_v1::PalwClassSemanticsDeclarationV1,
    palw_court_family_material_v1::prepare_family_table_inventory_v1,
    palw_fp_constraint_job_v1::PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1,
    palw_fp_execution_v3::palw_fp_job_pin_v1,
    palw_freeprompt_v3::{PALW_FP_COMMITMENT_TX_MAX_BYTES, PalwFpWorkerResultV3, PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_mode_v2::PalwConsensusMode,
    palw_native_resource_v1::{
        NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, native_declared_logical_limits_v1, reserve_native_requests_v1,
    },
    palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    palw_staged_preclaim_v1::PalwStagedClaimGeometryV1,
    palw_state_v2::PalwBondKeyV2,
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_court_v1::{
        PalwTirInventoryIndexV1,
        parameter_cover::{ParameterCoverBudgetV1, derive_parameter_cover_v1},
    },
    palw_tir_frame_cost_v1::{
        PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5, PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4, tir_public_owner_queue_wait_v1,
    },
    palw_tir_public_allocation_v1::{
        PalwTirPublicAllocationQuoteV1, PalwTirPublicAllocationRulesV1, derive_tir_public_allocation_family_native_v2,
        derive_tir_public_allocation_native_v2,
    },
    palw_tir_public_source_v1::{PalwTirPublicSourceV1, parameter_cover_v1::OriginalParameterCoverBudgetV1},
    palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1,
    palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1,
};
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use serde_json::json;
use std::{ffi::OsString, io::Read, path::Path};

fn read(path: &Path, cap: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(format!("{} exceeds the diagnostic input cap of {cap} bytes", path.display()).into());
    }
    Ok(bytes)
}

#[derive(Clone, Copy)]
enum CoverMode {
    None,
    Named(u64),
    All { budget: OriginalParameterCoverBudgetV1, native_bytes: u64 },
}

fn cover_mode(args: &[OsString]) -> Result<CoverMode, Box<dyn std::error::Error>> {
    fn bound(text: &OsString) -> Result<u64, Box<dyn std::error::Error>> {
        let value = text.to_str().ok_or("parameter cover limit must be text")?.parse()?;
        if value == 0 {
            return Err("all-root parameter cover limits must be positive".into());
        }
        Ok(value)
    }

    match args {
        [] => Ok(CoverMode::None),
        [leaf] => Ok(CoverMode::Named(leaf.to_str().ok_or("disputed leaf must be text")?.parse()?)),
        [flag, leaves, edges, coordinates, memberships, memory] if flag == "--all-roots" => Ok(CoverMode::All {
            budget: OriginalParameterCoverBudgetV1 {
                max_original_leaves: bound(leaves)?,
                max_total_edges: bound(edges)?,
                max_total_coordinates: bound(coordinates)?,
                max_root_memberships: bound(memberships)?,
            },
            native_bytes: bound(memory)?,
        }),
        _ => Err("expected one disputed leaf or --all-roots <max-leaves> <max-edges> <max-coordinates> <max-memberships> <max-native-bytes>".into()),
    }
}

#[derive(Default)]
struct ParameterShape {
    siblings: u64,
    value_bytes: u64,
    multiproof_bytes: u64,
}

fn parameter_shape(
    program: &misaka_palw_tir::TirProgramV1,
    index: &PalwTirInventoryIndexV1,
    positions: &[u32],
) -> Result<ParameterShape, Box<dyn std::error::Error>> {
    if positions.is_empty() {
        return Ok(ParameterShape::default());
    }
    // The native sibling counter retains the current and next u64 frontier.
    // Other sizing is scalar; no parameter values or opening bytes are copied.
    let scratch = (positions.len() as u64).checked_mul(2 * size_of::<u64>() as u64).ok_or("parameter sizing scratch overflow")?;
    let scratch = scratch.checked_add(32 * size_of::<u64>() as u64).ok_or("parameter sizing scratch overflow")?;
    let _lease = reserve_native_requests_v1(scratch).map_err(|error| error.to_string())?;
    let siblings = palw_artifact_multiproof_sibling_count_v1(index.leaf_count(), positions)
        .ok_or("the cover does not name a canonical set of inventory leaves")?;
    let mut value_bytes = 0u64;
    let mut opened_bytes = 0u64;
    for &position in positions {
        let (param, layer, _, len) = index.piece_of(position).ok_or("cover position outside the original inventory")?;
        let name = &program.params.get(usize::from(param)).ok_or("cover parameter outside the program")?.name;
        value_bytes = value_bytes.checked_add(u64::from(len)).ok_or("parameter values size overflow")?;
        let entry = 4 + palw_artifact_operand_borsh_len_v1(name.len(), layer.is_some(), len as usize);
        opened_bytes = opened_bytes.checked_add(entry).ok_or("parameter opening size overflow")?;
    }
    // The canonical codec helper accounts for headers and siblings. Add the
    // checked sum of entries (each includes its inventory index).
    let multiproof_bytes =
        palw_artifact_multiproof_borsh_len_v1([], siblings).checked_add(opened_bytes).ok_or("parameter multiproof size overflow")?;
    Ok(ParameterShape { siblings, value_bytes, multiproof_bytes })
}

/// Measure the local numerical builder and checker against a retained dense capture.
fn dense_replay(
    artifact: &misaka_palw_tir_exec::node::TirArtifactV1,
    original: &PalwFreePromptCommitmentV3,
    header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    record: &kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1,
    allocation: PalwTirPublicAllocationRulesV1,
    demand: kaspa_consensus_core::palw_tir_fence2_v1::PalwTirDemandRulesV1,
    capture_file: &Path,
    max_roots: u64,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use kaspa_consensus_core::{
        palw_court_numerical_geometry_v3::{NativeNumericalGeometryV3, NumericalGeometryBudgetV3},
        palw_step_refute::PalwStepRefuteError,
        palw_tir_court_v1::{PalwTirCourtRulesV1, build_tir_cone_refutation_bounded_v1, check_tir_cone_refutation_v1},
    };
    use misaka_palw_tir_exec::node::{TirCaptureV1, TirClassRunnerV1, TirEvidenceV1, tir_court_builder_limits_v1};

    let source = PalwTirPublicSourceV1::new_native_v1(
        record,
        layout,
        header,
        original.execution_root,
        original.trace_root,
        allocation.maximum_leaves,
    )
    .map_err(|error| format!("dense replay source: {error:?}"))?;
    let [_, _, max_paths, max_edges, max_path_bytes] = kaspa_consensus_core::palw_g14_values_v1::PALW_G14_NUMERICAL_GEOMETRY_V1;
    let geometry = NativeNumericalGeometryV3::derive_native_v3(
        &source,
        record,
        layout,
        &allocation.court,
        demand,
        NumericalGeometryBudgetV3 { max_original_leaves: allocation.maximum_leaves, max_roots, max_paths, max_edges, max_path_bytes },
    )
    .map_err(|error| format!("dense replay geometry: {error:?}"))?;
    if geometry.roots_v3().is_empty() {
        return Err("dense replay found no numerical roots".into());
    }
    let capture = TirCaptureV1::decode(&read(capture_file, 16 << 20)?)?;
    let binding = header.resolve_registered_layout_v1(record, layout, original.execution_root, allocation.maximum_leaves)?;
    if capture.binding != binding
        || !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
            allocation.prompt_form,
            &capture.prompt,
            &binding.job_context.prompt_token_ids_hash,
        )
    {
        return Err("dense capture does not match the retained original".into());
    }
    let verified = kaspa_consensus_core::palw_tir_step_v1::verify_tir_binding_v1(&binding, allocation.maximum_leaves)
        .map_err(|error| error.to_string())?;
    let runner = TirClassRunnerV1::new(&verified.space, artifact.plan(), artifact.params(), original.job.class_id)?;
    let store = TirEvidenceV1::dense(
        &runner,
        &binding,
        &capture.prompt,
        &capture.logits_rows,
        &capture.generated,
        &capture.leaves,
        artifact,
        allocation.prompt_form,
        allocation.maximum_leaves,
    )?;
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: allocation.maximum_leaves,
        prompt_form: allocation.prompt_form,
        limits: kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&allocation.court),
    };
    let mut maximum = [0; 5];
    for root in geometry.roots_v3() {
        let leaf = root.leaf_v3();
        let (mut cone, meter) =
            build_tir_cone_refutation_bounded_v1(&binding, leaf, &store, &rules, tir_court_builder_limits_v1(&rules))
                .map_err(|error| format!("dense replay leaf {leaf}: {error}"))?;
        let verdict = check_tir_cone_refutation_v1(&cone, &rules);
        if !matches!(verdict, Err(PalwStepRefuteError::NoFaultFound)) {
            return Err(format!("dense replay leaf {leaf}: {verdict:?}").into());
        }
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut cone.binding);
        let sample =
            [meter.work.elements, meter.work.terms, meter.target_requests, meter.source_reads, borsh::object_length(&cone)? as u64];
        for (maximum, value) in maximum.iter_mut().zip(sample) {
            *maximum = (*maximum).max(value);
        }
    }
    Ok(json!({
        "scope": "local numerical replay using the retained dense capture",
        "checked_roots": geometry.roots_v3().len(),
        "max_elements": maximum[0],
        "max_terms": maximum[1],
        "max_target_requests": maximum[2],
        "max_source_reads": maximum[3],
        "max_program_free_cone_bytes": maximum[4],
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [artifact, tokenizer, result, binding, semantics, daa, custodians, rest @ ..] = args.as_slice() else {
        return Err("usage: check-t12-tfp1-door <model.palwtir> <tokenizer.json> <worker-result.frame> <tir-public-binding.hex> <class-semantics.borsh> <DAA> <custodian-upper> [--original-commitment <file>] [--exact-token-table <file>] [--dense-replay <capture> <max-roots>] [disputed-leaf | --all-roots <max-leaves> <max-edges> <max-coordinates> <max-memberships> <max-native-bytes>]".into());
    };
    let (original_file, rest) = match rest {
        [flag, file, tail @ ..] if flag == "--original-commitment" => (Some(file), tail),
        [flag] if flag == "--original-commitment" => return Err("--original-commitment needs a Borsh commitment file".into()),
        _ => (None, rest),
    };
    let (table_file, rest) = match rest {
        [flag, file, tail @ ..] if flag == "--exact-token-table" => (Some(file), tail),
        [flag] if flag == "--exact-token-table" => return Err("--exact-token-table needs a canonical PALWTTB1 file".into()),
        _ => (None, rest),
    };
    let (replay, cover_args) = match rest {
        [flag, file, roots, tail @ ..] if flag == "--dense-replay" => {
            let max_roots: u64 = roots.to_str().ok_or("replay root limit must be text")?.parse()?;
            if original_file.is_none() || max_roots == 0 {
                return Err("--dense-replay requires a retained --original-commitment and a positive root limit".into());
            }
            (Some((file, max_roots)), tail)
        }
        [flag, ..] if flag == "--dense-replay" => return Err("--dense-replay needs a capture and a root limit".into()),
        _ => (None, rest),
    };
    let cover_mode = cover_mode(cover_args)?;
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
    let original = if let Some(file) = original_file {
        let original: PalwFreePromptCommitmentV3 = borsh::from_slice(&read(Path::new(file), PALW_FP_COMMITMENT_TX_MAX_BYTES as u64)?)?;
        if original != result.to_commitment(original.trace_retention_daa) {
            return Err("the retained commitment does not match the worker result".into());
        }
        original
    } else {
        result.to_commitment(daa.checked_add(40_000).ok_or("fixture retention DAA overflow")?)
    };
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
    // Compare the existing shared intake clock without treating it as a live
    // fence or substituting its result for the public quote.
    let mut batch_rules = rules;
    batch_rules.clock.version = PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5;
    batch_rules.clock.maximum_door_daa = kaspa_consensus_core::palw_court_policy_v1::DISPUTE_WINDOW_DAA;
    let batch = match quote(batch_rules, Some(&bound), Some(&inventory))? {
        PalwTirPublicAllocationQuoteV1::Complete(plan) => json!({"complete":true, "collect_daa":plan.terms.collect_until_daa-daa}),
        PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. } => json!({"complete":false,"missing":format!("{missing:?}")}),
    };
    let exact_table_diagnostic = if let Some(file) = table_file {
        let literal = read(Path::new(file), PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u64)?;
        let table = prepare_family_table_inventory_v1(&literal, &bound, record.logits_vocab)
            .map_err(|err| format!("the table does not authenticate against the class declaration: {err:?}"))?;
        let mut family_rules = rules;
        family_rules.clock.version = PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4;
        let family_quote = |family_rules| {
            derive_tir_public_allocation_family_native_v2(
                &geometry,
                &original,
                &header,
                &record,
                &entry.class.layout,
                Some(&bound),
                Some(&inventory),
                family_rules,
                Some(&table),
            )
            .map_err(|err| format!("offline family quote failed: {err:?}"))
        };
        let quote = match family_quote(family_rules)? {
            PalwTirPublicAllocationQuoteV1::Complete(plan) => json!({
                "complete": true, "collect_daa": plan.terms.collect_until_daa - daa,
            }),
            PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. } => json!({
                "complete": false, "missing": format!("{missing:?}"),
            }),
        };
        // The comparison below exposes the derived component clocks. Keep
        // its altered limit out of the quote reported above.
        family_rules.clock.maximum_door_daa = u64::MAX;
        let breakdown = match family_quote(family_rules)? {
            PalwTirPublicAllocationQuoteV1::Complete(plan) => {
                let stages: Vec<_> = plan
                    .semantic
                    .iter()
                    .flat_map(|bundle| &bundle.stages)
                    .map(|stage| {
                        json!({
                            "family": format!("{:?}", stage.family),
                            "open_offset_daa": stage.terms.open_until_daa - daa,
                            "stream_offset_daa": stage.terms.stream_until_daa - daa,
                            "resource_inclusion_daa": stage.terms.plan.inclusion_daa,
                            "maximum_moves": stage.terms.plan.maximum_moves,
                        })
                    })
                    .collect();
                json!({
                    "collect_offset_daa": plan.terms.collect_until_daa - daa,
                    "acquisition_inclusion_daa": plan.acquisition.envelope.inclusion_daa,
                    "semantic_stages": stages,
                })
            }
            PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. } => json!({"missing":format!("{missing:?}")}),
        };
        json!({
            "mode": "offline_family_v4_fixture",
            "canonical_table_bytes": literal.len(),
            "quote": quote,
            "unbounded_door_breakdown": breakdown,
        })
    } else {
        serde_json::Value::Null
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
    let program = &entry.artifact.plan().program;
    let layout = &entry.class.layout;
    let dense_replay_diagnostic = replay
        .map(|(capture, max_roots)| {
            dense_replay(
                &entry.artifact,
                &original,
                &header,
                &record,
                layout,
                rules,
                params.palw_tir_demand_rules_at(daa),
                Path::new(capture),
                max_roots,
            )
        })
        .transpose()?;
    let (parameter_cover, all_root_cover) = match cover_mode {
        CoverMode::None => (serde_json::Value::Null, serde_json::Value::Null),
        CoverMode::Named(leaf) => {
            let binding = header.resolve_registered_layout_v1(&record, layout, original.execution_root, rules.maximum_leaves)?;
            let budget = ParameterCoverBudgetV1 {
                max_edges: kaspa_consensus_core::palw_g14_values_v1::PALW_G14_PARAMETER_COVER_BUDGET_V1.max_total_edges,
                max_coordinates: kaspa_consensus_core::palw_g14_values_v1::PALW_G14_PARAMETER_COVER_BUDGET_V1.max_total_coordinates,
            };
            let output = match derive_parameter_cover_v1(&binding, leaf, budget) {
                Err(why) => json!({"leaf":leaf,"refusal":why}),
                Ok(cover) if cover.leaves.is_empty() => json!({"leaf":leaf,"inventory_leaves":0,"multiproof_bytes":0}),
                Ok(cover) => {
                    let index = PalwTirInventoryIndexV1::new(program).ok_or("the original program has no usable inventory index")?;
                    let shape = parameter_shape(program, &index, &cover.leaves)?;
                    json!({
                        "leaf":leaf,"inventory_leaves":cover.leaves.len(),"siblings":shape.siblings,
                        "multiproof_bytes":shape.multiproof_bytes,
                        "explored_edges":cover.explored_edges,"explored_coordinates":cover.explored_coordinates,
                    })
                }
            };
            (output, serde_json::Value::Null)
        }
        CoverMode::All { budget, native_bytes } => {
            if entry.artifact.composite_ref().is_some() {
                return Err("all-root sizing currently requires the original single inventory root".into());
            }
            let limits = NativeSnapshotResourceLimitsV1 { memory_bytes: native_bytes, database_bytes: 0 };
            arm_native_logical_limits_v1(Some(limits));
            if native_declared_logical_limits_v1() != Some(limits) {
                return Err("could not install the requested diagnostic native memory limit".into());
            }
            if header.step_leaf_count > budget.max_original_leaves {
                return Err("original exceeds the requested diagnostic leaf limit".into());
            }
            let source = PalwTirPublicSourceV1::new_native_v1(
                &record,
                layout,
                &header,
                original.execution_root,
                original.trace_root,
                rules.maximum_leaves,
            )
            .map_err(|error| format!("all-root source: {error:?}"))?;
            let cover = PalwTirPublicSourceV1::parameter_cover_native_v1(&source, &record, layout, budget)
                .map_err(|error| format!("all-root cover: {error:?}"))?;
            let index = PalwTirInventoryIndexV1::new_native_v1(program).map_err(|error| format!("all-root index: {error:?}"))?;
            let mut worst: Option<(u64, usize, ParameterShape)> = None;
            for root in &cover.roots {
                let shape = parameter_shape(program, &index, &root.parameters)?;
                // The cover enumerates ascending leaves. A tie keeps the earlier leaf.
                if worst.as_ref().is_none_or(|(_, _, previous)| shape.multiproof_bytes > previous.multiproof_bytes) {
                    worst = Some((root.leaf, root.parameters.len(), shape));
                }
            }
            let output = json!({
                "scope":"all numerical roots, parameter values and canonical artifact openings only",
                "source_id":cover.source_id.to_string(),
                "artifact_root":entry.artifact_root.to_string(),
                "original_leaves":cover.original_leaves,
                "numerical_roots":cover.roots.len(),
                "unique_inventory_leaves":cover.union.len(),
                "explored_edges":cover.total_edges,
                "explored_coordinates":cover.total_coordinates,
                "root_memberships":cover.root_memberships,
                "limits":{
                    "max_original_leaves":budget.max_original_leaves,
                    "max_total_edges":budget.max_total_edges,
                    "max_total_coordinates":budget.max_total_coordinates,
                    "max_root_memberships":budget.max_root_memberships,
                    "native_analysis_bytes":native_bytes,
                },
                "worst":worst.map(|(leaf, count, shape)| json!({
                    "leaf":leaf,
                    "inventory_leaves":count,
                    "siblings":shape.siblings,
                    "parameter_value_bytes":shape.value_bytes,
                    "opening_and_framing_bytes":shape.multiproof_bytes - shape.value_bytes,
                    "multiproof_bytes":shape.multiproof_bytes,
                })),
            });
            (serde_json::Value::Null, output)
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "params_id":params.consensus_params_id().to_string(),
            "class_id":entry.class_id().to_string(),
            "claim_id":fp_claim_id_v3(&original).to_string(),
            "fixture_commitment":original_file.is_none(),
            "daa":daa,
            "fixture_eof":true,
            "custodian_upper":maximum_custodians,
            "maximum_door_daa":cost.clock.maximum_door_daa,
            "public_cost_quote":public,
            "offline_batch_v5_clock_quote":batch,
            "dense_replay_diagnostic":dense_replay_diagnostic,
            "exact_table_diagnostic":exact_table_diagnostic,
            "non_tokenizer_diagnostic":non_tokenizer,
            "disputed_leaf_parameter_cover":parameter_cover,
            "all_numerical_root_parameter_cover":all_root_cover,
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, palw_artifact_multiproof_v1};
    use misaka_palw_tir::{DType, ParamDecl, Schedule, TirProgramV1};

    #[test]
    fn named_leaf_and_all_root_limits_parse_without_implicit_budget() {
        let args = |args: &[&str]| args.iter().copied().map(OsString::from).collect::<Vec<_>>();
        assert!(matches!(cover_mode(&args(&["17"])).unwrap(), CoverMode::Named(17)));
        let parsed = cover_mode(&args(&["--all-roots", "16000", "500000", "600000", "1000000", "268435456"])).unwrap();
        assert!(matches!(
            parsed,
            CoverMode::All {
                budget: OriginalParameterCoverBudgetV1 {
                    max_original_leaves: 16_000,
                    max_total_edges: 500_000,
                    max_total_coordinates: 600_000,
                    max_root_memberships: 1_000_000,
                },
                native_bytes: 268_435_456
            }
        ));
        assert!(cover_mode(&args(&["--all-roots", "1", "2", "3", "4"])).is_err());
        assert!(cover_mode(&args(&["--all-roots", "1", "2", "3", "0", "5"])).is_err());
    }

    #[test]
    fn parameter_shape_equals_canonical_promote_odd_opening_codec() {
        // Inventory-only fixture. No execution or class admission is involved.
        let program = TirProgramV1 {
            version: 1,
            prim_set_id: [0; 64],
            token_bound: 1,
            history_bound: 1,
            params: vec![ParamDecl { name: "matrix".into(), dtype: DType::I8, shape: vec![3, 2], per_layer: false }],
            consts: vec![],
            states: vec![],
            blocks: vec![],
            schedule: Schedule { pre: 0, layers: vec![], post: 0 },
            logits: 0,
            logits_scheme_id: [0; 64],
        };
        let operands: Vec<_> = (0..3u32)
            .map(|row| PalwArtifactOperandV1 {
                tensor_name: "matrix".into(),
                layer: None,
                row_start: row * 2,
                bytes: vec![row as u8; 2],
            })
            .collect();
        let leaves: Vec<_> = operands.iter().map(artifact_leaf_v1).collect();
        let index = PalwTirInventoryIndexV1::new(&program).unwrap();
        for positions in [&[0, 2][..], &[2][..], &[0, 1, 2][..]] {
            let opened: Vec<_> = positions.iter().map(|&position| (position, operands[position as usize].clone())).collect();
            let native = palw_artifact_multiproof_v1(&leaves, &opened).unwrap();
            let shape = parameter_shape(&program, &index, positions).unwrap();
            assert_eq!(shape.value_bytes, 2 * positions.len() as u64);
            assert_eq!(shape.siblings, native.siblings.len() as u64);
            assert_eq!(shape.multiproof_bytes, borsh::object_length(&native).unwrap() as u64);
        }
        assert_eq!(parameter_shape(&program, &index, &[]).unwrap().multiproof_bytes, 0);
        assert!(parameter_shape(&program, &index, &[2, 0]).is_err());
    }
}
