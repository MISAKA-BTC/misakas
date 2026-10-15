//! Original, versioned TIR resource derivation. No old ClaimBundle is extended or
//! reinterpreted and no carrier/admission/fence is enabled by this module.
//! The witness sized here is original public material with Merkle identity
//! authentication and deterministic local adjudication, NOT a cryptographic
//! computation-validity proof. Host-supplied operational Needs are never inputs.
//!
//! `Complete` means the components below have a checked native derivation. It is
//! not network inclusion, global-state clone amplification, a custody signature,
//! payment, artifact availability, or a G14 completion certificate.
use crate::palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1};
use crate::palw_staged_preclaim_v1::{PalwPreclaimSourceGeometryV1, PalwStagedClaimGeometryV1};
use crate::palw_tir_public_owned_v1::{
    PalwTirPublicOwnedPlanV1, PalwTirPublicPreparationPlanV1, PalwTirPublicWorkAllocationV1, PalwTirPublicWorkRulesV1,
    derive_tir_public_preparation_plan_v1, derive_tir_public_work_allocation_v1,
};
use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicSourceV1};
use crate::{Hash64, palw_mode_v2::PalwCourtParamsV2};
use borsh::{BorshDeserialize, BorshSerialize};

const VERSION: u16 = 1;
const CARRIER: u64 = 90_000;
fn narrow(n: u128) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "native TIR allocation exceeds u64 (not covered)".into())
}
fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "native TIR allocation sum overflow (not covered)".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    narrow(u128::from(a) * u128::from(b))
}
fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Result<Hash64, String> {
    let bytes = borsh::to_vec(value).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(domain).hash(&bytes).as_bytes().try_into().expect("64")))
}
/// Exact disk hashes for all levels, including ragged odd levels, without
/// allocating N hashes or enumerating the original leaf preimages.
pub fn tir_public_tree_disk_bytes_v1(mut n: u64) -> Result<u64, String> {
    let mut bytes = 0;
    while n != 0 {
        bytes = add(bytes, mul(n, 64)?)?;
        if n == 1 {
            break;
        }
        n = n.div_ceil(2);
    }
    Ok(bytes)
}

/// Resolved by the accepting native ruleset, never deserialized from a carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirPublicAllocationRulesV1 {
    pub maximum_leaves: u64,
    pub maximum_custodians: u64,
    pub court: PalwCourtParamsV2,
    /// Actual native dissection availability, not inferred just from a program
    /// containing a history reduction or an operational builder preference.
    pub dissection: bool,
    pub prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    pub config: PalwStagedPublicCourtConfigV1,
}
impl PalwTirPublicAllocationRulesV1 {
    fn commitment_v1(self) -> Result<Hash64, String> {
        let form = match self.prompt_form {
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat => 0u8,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 => 1u8,
        };
        hash(
            b"misaka-palw/tir-allocation-rules/v1",
            &(VERSION, self.maximum_leaves, self.maximum_custodians, self.court, self.dissection, form, self.config),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicExecutorPlanV1 {
    pub artifact_bytes: u64,
    pub inventory_leaves: u64,
    pub inventory_ram_bytes: u64,
    pub step_tree_bytes: u64,
    pub row_tree_bytes: u64,
    pub tile_forest_bytes: u64,
    pub original_capture_bytes: u64,
    pub prepared_disk_bytes: u64,
    pub own_disk_bytes: u64,
    pub maximum_file_operations: u64,
    pub working_ram_bytes: u64,
    pub envelope: PalwRenderingResourcePlanV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicTerminalPlanV1 {
    pub maximum_witness_bytes: u64,
    pub maximum_root_claim_bytes: u64,
    pub maximum_dissection_rounds: u64,
    pub sizing_work: u64,
    pub envelope: PalwRenderingResourcePlanV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTirPublicAllocationMissingV1 {
    NativeSemanticStatement,
    NativeFloatRoute,
    CanonicalWitnessSizing(String),
    NativeWitnessCeiling { required: u64, native: u64 },
    NativeRootClaimCeiling { required: u64, native: u64 },
    PreparedPublicationBound { required: u64, allocated: u64 },
    PreparedSliceBound { required: u64, allocated: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTirPublicAllocationQuoteV1 {
    Complete(Box<PalwTirPublicAllocationPlanV1>),
    /// Not admitted, not a zero-cost component, and not an excluded profile.
    Incomplete {
        source_id: Hash64,
        missing: Vec<PalwTirPublicAllocationMissingV1>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicAllocationPlanV1 {
    pub version: u16,
    pub source_id: Hash64,
    pub original_claim: Hash64,
    pub rules_id: Hash64,
    /// Original producer can sign this before inclusion. It commits resource
    /// geometry/config/rules, not an unknowable accepting block timestamp.
    pub resource_plan_id: Hash64,
    /// Native accepted instance: commits ALL absolute immutable clocks too.
    pub allocation_id: Hash64,
    pub semantic: Option<crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1>,
    pub semantic_resource_plan_id: Option<Hash64>,
    pub maximum_semantic_history_records: u64,
    pub acquisition: PalwTirPublicOwnedPlanV1,
    pub flat_trace: Option<PalwRenderingResourcePlanV1>,
    pub preparation_work: PalwTirPublicWorkAllocationV1,
    pub owner_work: PalwTirPublicWorkAllocationV1,
    pub preparation: PalwTirPublicPreparationPlanV1,
    pub executor: PalwTirPublicExecutorPlanV1,
    pub terminal: PalwTirPublicTerminalPlanV1,
    pub controls: PalwRenderingResourcePlanV1,
    pub preparation_until_daa: u64,
    pub terms: PalwRenderingClaimTermsV1,
}

fn component(
    storage: u64,
    ram: u64,
    work: u64,
    moves: u64,
    vocab: u32,
    config: PalwStagedPublicCourtConfigV1,
) -> Result<PalwRenderingResourcePlanV1, String> {
    let carrier = mul(moves, CARRIER)?;
    let bytes = add(storage, carrier)?;
    let row = add(8192, mul(config.prices.reorg_replay_moves.checked_add(1).ok_or("native restart count overflow")?, 1024)?)?;
    let parser = add(work, mul(bytes, 32)?)?;
    Ok(PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab,
        maximum_stream_bytes: bytes,
        maximum_stream_leaves: moves,
        maximum_eog_bytes: 0,
        maximum_moves: moves,
        maximum_parser_steps: parser,
        maximum_blake_blocks: parser.div_ceil(128),
        inclusion_daa: mul(moves, config.prices.inclusion_margin_daa)?,
        rent_sompi: add(
            add(mul(bytes, config.prices.rent_per_stream_byte)?, mul(row, config.prices.rent_per_row_byte)?)?,
            mul(parser, config.prices.rent_per_parser_step)?,
        )?,
        deposit_sompi: mul(moves, config.prices.deposit_per_move)?,
        row_bytes: row,
        resident_ram_bytes: add(ram, mul(row, 8)?)?,
    })
}

fn executor_plan(
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    space: &crate::palw_tir_step_v1::PalwTirStepSpaceV1,
    source: &PalwTirPublicSourceV1,
    rules: PalwTirPublicAllocationRulesV1,
) -> Result<PalwTirPublicExecutorPlanV1, String> {
    let p = &space.program;
    let ctx = &source.header_v1().job_context;
    let shape = space.job_shape(ctx).map_err(|e| e.to_string())?;
    let positions = u64::from(shape.positions);
    let d = u64::from(source.decode_count_v1());
    let prefill = u64::from(ctx.declared_prefill_tokens);
    let vocab = u64::from(source.vocab_v1());
    let tiles = vocab.div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    let n = source.leaf_count_v1();
    let step_tree_bytes = tir_public_tree_disk_bytes_v1(n)?;
    let row_tree_bytes = tir_public_tree_disk_bytes_v1(d)?;
    let tile_forest_bytes = mul(d, tir_public_tree_disk_bytes_v1(tiles)?)?;
    let mut artifact_bytes = 0;
    let mut largest_chunk = 0;
    let mut most_chunk_leaves = 0;
    let mut artifact_instances = 0;
    let instances = crate::palw_tir_artifact_v1::palw_tir_param_instances_v1(p);
    // Check arithmetic before invoking the original inventory index helpers.
    for (param, instances) in p.params.iter().zip(&instances) {
        let bytes = param
            .shape
            .iter()
            .try_fold(param.dtype.width() as u128, |n, x| n.checked_mul(u128::from(*x)))
            .ok_or("native parameter byte product overflow")?;
        if bytes > u128::from(u32::MAX) {
            return Err("original inventory cannot address this native tensor (not covered)".into());
        }
        artifact_bytes = add(artifact_bytes, mul(narrow(bytes)?, instances.len() as u64)?)?;
        artifact_instances = add(artifact_instances, instances.len() as u64)?;
        let row = if param.shape.len() < 2 {
            narrow(bytes)?
        } else {
            param.shape[1..].iter().try_fold(param.dtype.width() as u64, |n, x| mul(n, u64::from(*x)))?
        };
        if row != 0 && bytes != 0 {
            const CHUNK: u64 = 32 << 20;
            let tensor = narrow(bytes)?;
            let chunk = if row <= CHUNK { mul(CHUNK / row, row)?.min(tensor) } else { CHUNK.min(tensor) };
            let piece = crate::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1 as u64;
            let leaves = if row <= CHUNK { mul(chunk / row, row.div_ceil(piece))? } else { chunk.div_ceil(piece) };
            largest_chunk = largest_chunk.max(chunk);
            most_chunk_leaves = most_chunk_leaves.max(leaves);
        }
    }
    let inventory = crate::palw_tir_court_v1::PalwTirInventoryIndexV1::new(p).ok_or("native inventory index is not representable")?;
    let inventory_leaves = u64::from(inventory.leaf_count());
    let artifact_tree = tir_public_tree_disk_bytes_v1(inventory_leaves)?;
    // The actual artifact provider currently builds a RESIDENT inventory tree,
    // unlike the disk-backed own step tree. Tiny rows can mean one hash per
    // byte; tensor bytes alone do not cover hash/coordinate-vector memory.
    let inventory_ram_bytes = add(
        add(mul(inventory_leaves, 4 * 64)?, add(16 << 20, mul(u64::from(record.program_bytes), 64)?)?)?,
        add(mul(largest_chunk, 2)?, mul(most_chunk_leaves, 2 * (16 + 64))?)?,
    )?;
    let max_leaf = source.unit_bound_v1(&PalwTirPublicRequestV1::StepLeaf { index: 0 })?.maximum_response_bytes;
    // Full original capture, offsets, original-leaf spool, every public index,
    // and a disjoint durable prepared generation. No sender-proposed length.
    let raw_format_leaf = add(mul(u64::from(crate::palw_step::PALW_STEP_MAX_TILE_LEN), 4)?, 26)?;
    let capture_payload = add(add(mul(prefill, 4)?, mul(d, add(mul(vocab, 4)?, 8)?)?)?, mul(n, raw_format_leaf)?)?;
    let original_capture_bytes = add(capture_payload, add(mul(u64::from(record.program_bytes), 64)?, 65_536)?)?;
    let original_indexes = add(add(add(step_tree_bytes, mul(n, 8)?)?, tile_forest_bytes)?, mul(row_tree_bytes, 2)?)?;
    let spool = mul(n, raw_format_leaf)?;
    let prepared_disk_bytes = add(add(add(original_capture_bytes, original_indexes)?, spool)?, 4096)?;
    // The independent own executor stores just its own disk step/row roots and
    // IDs. It does not retain all N hashes or all historical checkpoints in RAM.
    let own_disk_bytes = add(add(step_tree_bytes, row_tree_bytes)?, add(mul(d, 4)?, artifact_tree)?)?;
    let mut values = 0;
    for block in &p.blocks {
        for node in &block.nodes {
            let count = node
                .out
                .shape
                .iter()
                .try_fold(1u128, |n, dim| {
                    let x = match dim {
                        misaka_palw_tir::types::Dim::Fixed(x) => u64::from(*x),
                        misaka_palw_tir::types::Dim::H => positions.min(u64::from(p.history_bound)),
                    };
                    n.checked_mul(u128::from(x))
                })
                .ok_or("native node element product overflow")?;
            values = add(values, mul(narrow(count)?, 16)?)?;
        }
        for carry in &block.carry_in {
            values = add(values, mul(carry.elements_at(1), 32)?)?;
        }
    }
    for instance in space.fixed_instances() {
        values = add(values, mul(u64::from(instance.elements), 32)?)?;
    }
    for instance in space.hist_instances() {
        let misaka_palw_tir::program::StateKind::Hist { window } = p.states[instance.state as usize].kind else {
            return Err("native Hist instance changes its registered kind".into());
        };
        let rows = add(u64::from(window).min(positions), u64::from(space.layout.h_tile))?;
        values = add(values, mul(mul(u64::from(instance.elements), rows)?, 16)?)?;
    }
    // All own parameter data is an own-copy premise, not a new global artifact
    // availability obligation. Charge even a resident provider's whole closure.
    let metadata = mul(mul(u64::from(record.program_bytes), 64)?, space.occurrences().len().max(1) as u64)?;
    // i128 node values plus conversion/output Vecs, live Fixed/Hist copies,
    // resident parameter copies, decoded-program/maps and allocator spare
    // capacity. These local structure factors do NOT bound global ledger maps.
    let execution = add(add(mul(add(values, artifact_bytes)?, 32)?, metadata)?, 16 << 20)?;
    let scratch = add(mul(max_leaf, 16)?, add(mul(d, 32)?, 2 * 1024 * 64 + 262_144)?)?;
    let working_ram_bytes = add(add(execution, scratch)?, inventory_ram_bytes)?;
    let actors = add(rules.maximum_custodians, 1)?;
    let passes = add(rules.config.prices.reorg_replay_moves, 1)?;
    // Original cold-preparation files coexist with one sealed Ready generation
    // and one deterministic staging/retry generation; none borrows another's
    // paid disk ticket. Each literal actor and the outside owner is separate.
    let artifact_storage = add(add(artifact_bytes, artifact_tree)?, mul(artifact_instances, 4096)?)?;
    let disk = mul(add(add(mul(prepared_disk_bytes, 3)?, own_disk_bytes)?, artifact_storage)?, actors)?;
    let slices = add(prepared_disk_bytes.div_ceil(16_384), 8)?;
    let maximum_file_operations = mul(mul(add(mul(slices, 4)?, 64)?, actors)?, passes)?;
    let copies = add(mul(mul(add(disk, original_capture_bytes)?, passes)?, 16)?, mul(maximum_file_operations, 4096)?)?;
    let envelope = component(disk, mul(working_ram_bytes, actors)?, copies, mul(actors, 8)?, source.vocab_v1(), rules.config)?;
    Ok(PalwTirPublicExecutorPlanV1 {
        artifact_bytes,
        inventory_leaves,
        inventory_ram_bytes,
        step_tree_bytes,
        row_tree_bytes,
        tile_forest_bytes,
        original_capture_bytes,
        prepared_disk_bytes,
        own_disk_bytes,
        maximum_file_operations,
        working_ram_bytes,
        envelope,
    })
}

fn work_rules(
    space: &crate::palw_tir_step_v1::PalwTirStepSpaceV1,
    source: &PalwTirPublicSourceV1,
    execution: &PalwTirPublicExecutorPlanV1,
    rules: PalwTirPublicAllocationRulesV1,
) -> Result<PalwTirPublicWorkRulesV1, String> {
    let limits = crate::palw_court_v2::palw_tir_court_limits_v1(&rules.court);
    let positions = u64::from(space.job_shape(&source.header_v1().job_context).map_err(|e| e.to_string())?.positions);
    let mut cells = 0;
    let mut inputs = 1;
    for (block, _) in space.occurrences() {
        for node in &space.program.blocks[*block as usize].nodes {
            cells = add(cells, node.out.elements_at(positions.min(u64::from(space.program.history_bound))))?;
            inputs = inputs.max(node.inputs.len() as u64);
        }
    }
    // Every registered cell may be a requested target, including each k-ary
    // reduction child. Cached hits do not evade the separate input-call bound.
    let targets = mul(mul(cells.max(1), positions)?, u64::from(rules.court.dissection_arity()))?;
    let reads =
        add(targets, mul(add(limits.max_elements, limits.max_terms)?, add(inputs, 1)?)?)?.max(execution.maximum_file_operations);
    let copy = add(
        add(mul(rules.court.max_close_bytes(), 32)?, mul(execution.artifact_bytes, 8)?)?,
        add(
            mul(source.leaf_count_v1(), source.unit_bound_v1(&PalwTirPublicRequestV1::StepLeaf { index: 0 })?.maximum_response_bytes)?,
            mul(u64::from(source.decode_count_v1()), 32)?,
        )?,
    )?;
    // This must constrain the ACCEPTED actor Work.io lane as well as rent in
    // the executor component. A separate paid disk/RAM quote does not authorize
    // a legitimate replay slice that the rooted IO counter would otherwise deny.
    let maximum_replay_io_bytes = add(
        mul(mul(execution.working_ram_bytes, 8)?, positions)?,
        mul(add(execution.prepared_disk_bytes, execution.original_capture_bytes)?, 16)?,
    )?;
    Ok(PalwTirPublicWorkRulesV1 {
        max_step_leaf_count: rules.maximum_leaves,
        prompt_form: rules.prompt_form,
        max_eval_elements: limits.max_elements,
        max_eval_terms: limits.max_terms,
        max_target_requests: targets,
        max_source_reads: reads,
        maximum_probe_copy_bytes: copy,
        maximum_replay_io_bytes,
    })
}

fn terminal_plan(
    space: &crate::palw_tir_step_v1::PalwTirStepSpaceV1,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    source: &PalwTirPublicSourceV1,
    rules: PalwTirPublicAllocationRulesV1,
) -> Result<Result<PalwTirPublicTerminalPlanV1, PalwTirPublicAllocationMissingV1>, String> {
    use crate::palw_tir_close_size_v1 as size;
    let inventory = crate::palw_tir_court_v1::PalwTirInventoryIndexV1::new(&space.program)
        .ok_or("native terminal inventory is not representable")?;
    let sizing = size::PalwTirCloseSizingV1 {
        form: size::PalwTirParamFormV1::Multiproof,
        court: rules.dissection && !record.dissected.is_empty(),
        cap: size::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        stop_above: None,
    };
    let (bounds, sizing_work) = match crate::palw_tir_close_range_v1::palw_tir_worst_closes_range_work_v1(
        space,
        &inventory,
        &source.header_v1().job_context,
        &sizing,
    ) {
        Ok(bounds) => bounds,
        Err(error) => return Ok(Err(PalwTirPublicAllocationMissingV1::CanonicalWitnessSizing(error))),
    };
    let maximum_witness_bytes = bounds.iter().map(|b| b.close_bytes).max().unwrap_or(0);
    let maximum_root_claim_bytes = bounds.iter().map(|b| b.root_claim_bytes).max().unwrap_or(0);
    if maximum_witness_bytes == 0 {
        return Ok(Err(PalwTirPublicAllocationMissingV1::CanonicalWitnessSizing("no native commit-point bound".into())));
    }
    if maximum_witness_bytes > rules.court.max_close_bytes() {
        return Ok(Err(PalwTirPublicAllocationMissingV1::NativeWitnessCeiling {
            required: maximum_witness_bytes,
            native: rules.court.max_close_bytes(),
        }));
    }
    // Same signed move + carrier-frame limit used by native TIR admission, NOT
    // this module's arbitrary multipart/accounting carrier reservation.
    let root_ceiling = crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1;
    if maximum_root_claim_bytes > root_ceiling {
        return Ok(Err(PalwTirPublicAllocationMissingV1::NativeRootClaimCeiling {
            required: maximum_root_claim_bytes,
            native: root_ceiling,
        }));
    }
    let positions = u64::from(space.job_shape(&source.header_v1().job_context).map_err(|e| e.to_string())?.positions);
    let rounds = if !rules.dissection || record.dissected.is_empty() {
        0
    } else {
        u64::from(rules.court.history_dissection_rounds(positions, space.layout.h_tile).ok_or("native dissection duration overflow")?)
    };
    let attempts = add(rules.config.prices.reorg_replay_moves, 1)?;
    let turns = mul(add(mul(rounds, 2)?, 3)?, attempts)?;
    let bytes = mul(add(mul(maximum_witness_bytes, attempts)?, mul(root_ceiling.max(CARRIER), turns)?)?, 8)?;
    let limits = crate::palw_court_v2::palw_tir_court_limits_v1(&rules.court);
    let work = add(mul(add(limits.max_elements, limits.max_terms)?, mul(turns, 32)?)?, sizing_work)?;
    let envelope = component(
        bytes,
        add(mul(maximum_witness_bytes.max(maximum_root_claim_bytes), 32)?, 262_144)?,
        work,
        add(turns, mul(attempts, maximum_witness_bytes.div_ceil(16_384))?)?,
        source.vocab_v1(),
        rules.config,
    )?;
    Ok(Ok(PalwTirPublicTerminalPlanV1 {
        maximum_witness_bytes,
        maximum_root_claim_bytes,
        maximum_dissection_rounds: rounds,
        sizing_work,
        envelope,
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn derive_tir_public_allocation_v1(
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    rules: PalwTirPublicAllocationRulesV1,
) -> Result<PalwTirPublicAllocationQuoteV1, String> {
    let public_header = crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(header.clone());
    crate::palw_staged_preclaim_v1::check_preclaim_identity_v1(
        geometry,
        original,
        &public_header,
        rules.maximum_custodians,
        rules.config,
    )?;
    let source =
        PalwTirPublicSourceV1::new_v1(record, layout, header, original.execution_root, original.trace_root, rules.maximum_leaves)?;
    let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, rules.maximum_leaves)?;
    let binding = header.resolve_registered_layout_v1(record, layout, original.execution_root, rules.maximum_leaves)?;
    let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, rules.maximum_leaves).map_err(|e| e.to_string())?;
    if header.job_context.exact_decode_tokens != original.decode_tokens_executed
        || header.job_context.tokenizer_id != original.job.tokenizer_id
        || original.work_leaves != source.leaf_count_v1()
        || inventory.is_some_and(|certificate| certificate.completed_daa > geometry.accepted_daa)
    {
        return Err("native TIR full allocation changes original count, tokenizer, leaves or preclaim inventory clock".into());
    }
    let incomplete =
        |missing| Ok(PalwTirPublicAllocationQuoteV1::Incomplete { source_id: source.source_id_v1(), missing: vec![missing] });
    if view.lane == crate::palw_step::PalwStepLaneV1::Float32 {
        return incomplete(PalwTirPublicAllocationMissingV1::NativeFloatRoute);
    }
    if semantics.is_none() && (original.job.is_constraint() || original.job.is_prefix_state() || inventory.is_some()) {
        return incomplete(PalwTirPublicAllocationMissingV1::NativeSemanticStatement);
    }
    let mut semantic = if let Some(semantics) = semantics {
        if !semantics.matches_context(&header.job_context)
            || !semantics.well_formed_for(&geometry.class_id, &header.artifact_root)
            || semantics.bound_from_daa > geometry.accepted_daa
        {
            return Err("full allocation changes native immutable class semantics".into());
        }
        let mut families = Vec::new();
        if original.job.is_prefix_state() {
            let prefix = crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(
                record,
                layout,
                header,
                &original.job,
                rules.maximum_leaves,
            )?;
            families.push((
                crate::palw_staged_bundle_v1::PalwStagedFamilyV1::PublicPrefix,
                crate::palw_tir_prefix_owned_v1::tir_prefix_resource_plan_v1(
                    prefix.template.total_v1(),
                    prefix.leaf_bytes,
                    prefix.depth,
                    u32::try_from(rules.maximum_custodians).map_err(|_| "native prefix custody is not representable")?,
                    rules.config.prices,
                )?,
            ));
        }
        if original.job.is_constraint() {
            families.push((
                crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint,
                PalwRenderingResourcePlanV1::derive(4095, view.vocab_size, rules.config.prices)?,
            ));
        }
        // NEW allocation only: this shared finishing kernel already quotes the
        // original Flat view bridge. The old V1 Flat-TIR refusal is unchanged.
        crate::palw_staged_preclaim_v1::finish_preclaim_quote_v1(
            geometry,
            original,
            semantics,
            inventory,
            families,
            PalwPreclaimSourceGeometryV1 {
                vocab: view.vocab_size,
                logits_scheme_id: view.logits_scheme_id,
                lane: view.lane,
                profile_bytes: u64::from(record.program_bytes),
                step_leaf_count: source.leaf_count_v1(),
            },
            rules.maximum_custodians,
            rules.config,
        )?
    } else {
        None
    };
    let acquisition = PalwTirPublicOwnedPlanV1::derive_v1(&source, rules.maximum_custodians, rules.config)?;
    // D=0 has no trace rows/IDs to acquire, not an unknown source silently
    // omitted. Original output identity over the empty IDs is a local check.
    let flat_trace = if view.is_tiled_v1() || source.decode_count_v1() == 0 {
        None
    } else {
        Some(crate::palw_flat_constraint_stream_v1::flat_constraint_source_resource_plan_v1(
            source.decode_count_v1(),
            source.vocab_v1(),
            u64::from(record.program_bytes),
            u32::try_from(rules.maximum_custodians).map_err(|_| "native Flat custody is not representable")?,
            source.leaf_count_v1(),
            rules.config.prices,
        )?)
    };
    let executor = executor_plan(record, &verified.space, &source, rules)?;
    let native_work = work_rules(&verified.space, &source, &executor, rules)?;
    let owner_work = derive_tir_public_work_allocation_v1(record, layout, header, &source, &acquisition, native_work)?;
    // Two separate rooted spent pools MUST have two prepaid components. This
    // conservative quote can later be tightened by a separately versioned rule;
    // it never lets public responders consume the owner's replay/probe credit.
    let preparation_work = owner_work.clone();
    let preparation = derive_tir_public_preparation_plan_v1(&source, &preparation_work, &acquisition)?;
    if executor.prepared_disk_bytes > preparation.maximum_prepared_bytes {
        return incomplete(PalwTirPublicAllocationMissingV1::PreparedPublicationBound {
            required: executor.prepared_disk_bytes,
            allocated: preparation.maximum_prepared_bytes,
        });
    }
    let native_slices = mul(add(executor.prepared_disk_bytes.div_ceil(16_384), 8)?, add(rules.config.prices.reorg_replay_moves, 1)?)?;
    if native_slices > preparation.maximum_slices_per_actor {
        return incomplete(PalwTirPublicAllocationMissingV1::PreparedSliceBound {
            required: native_slices,
            allocated: preparation.maximum_slices_per_actor,
        });
    }
    let terminal = match terminal_plan(&verified.space, record, &source, rules)? {
        Ok(plan) => plan,
        Err(missing) => return incomplete(missing),
    };
    // Original pledge, each distinct signed Valid grant, allocation admission,
    // original public context, immutable leases and final metadata/ref controls.
    let controls = component(
        mul(add(rules.maximum_custodians, 4)?, 16_384)?,
        mul(add(rules.maximum_custodians, 1)?, 262_144)?,
        mul(add(rules.maximum_custodians, 4)?, 131_072)?,
        add(rules.maximum_custodians, 4)?,
        source.vocab_v1(),
        rules.config,
    )?;
    let mut sum = acquisition
        .envelope
        .checked_sum_v1(preparation_work.envelope)?
        .checked_sum_v1(owner_work.envelope)?
        .checked_sum_v1(preparation.envelope)?
        .checked_sum_v1(executor.envelope)?
        .checked_sum_v1(terminal.envelope)?
        .checked_sum_v1(controls)?;
    if let Some(flat) = flat_trace {
        sum = sum.checked_sum_v1(flat)?;
    }
    let semantic_resource_plan_id = semantic.as_ref().map(|quote| quote.resource_plan_id);
    let maximum_semantic_history_records = semantic.as_ref().map_or(0, |quote| quote.maximum_records);
    let preparation_until_daa = add(geometry.accepted_daa, preparation.preparation_daa)?;
    let open = add(preparation_until_daa, rules.config.acquisition_daa)?;
    // Cold work and Ready are already before owner opening. They remain in the
    // monetary SUM, but do NOT allocate a second cold-preparation deadline after
    // every request. Per-owner WorkDebit/response inclusion stays after open.
    let pre_inclusion = add(preparation_work.envelope.inclusion_daa, preparation.envelope.inclusion_daa)?;
    let post_inclusion = sum.inclusion_daa.checked_sub(pre_inclusion).ok_or("native preparation inclusion is not inside SUM")?;
    let generic_end = add(open, post_inclusion.max(1))?;
    let stream = if let Some(quote) = &mut semantic {
        let shift = generic_end.checked_sub(geometry.accepted_daa).ok_or("native semantic offset underflow")?;
        rebase_new_semantic_plan(&mut quote.plan, shift)?;
        sum = sum.checked_sum_v1(quote.plan.terms.plan)?;
        quote.plan.terms.stream_until_daa
    } else {
        generic_end
    };
    let collect = add(stream, rules.config.collection_daa)?;
    let material = add(collect, rules.config.reorg_retention_daa)?;
    let terms = PalwRenderingClaimTermsV1 {
        accepted_daa: geometry.accepted_daa,
        open_until_daa: open,
        stream_until_daa: stream,
        collect_until_daa: collect,
        material_until_daa: material,
        bond_exit_after_daa: material,
        plan: sum,
    };
    terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).ok_or("native full allocation capital overflow")?;
    let mut plan = PalwTirPublicAllocationPlanV1 {
        version: VERSION,
        source_id: source.source_id_v1(),
        original_claim: crate::palw_freeprompt_v3::fp_claim_id_v3(original),
        rules_id: rules.commitment_v1()?,
        resource_plan_id: Hash64::default(),
        allocation_id: Hash64::default(),
        semantic: semantic.map(|quote| quote.plan),
        semantic_resource_plan_id,
        maximum_semantic_history_records,
        acquisition,
        flat_trace,
        preparation_work,
        owner_work,
        preparation,
        executor,
        terminal,
        controls,
        preparation_until_daa,
        terms,
    };
    plan.resource_plan_id = plan.resource_id_v1()?;
    plan.allocation_id = plan.accepted_id_v1()?;
    plan.check_v1()?;
    Ok(PalwTirPublicAllocationQuoteV1::Complete(Box::new(plan)))
}

/// Called ONLY while deriving a NEW versioned initial allocation. Existing
/// native claim/owner clocks are never mutated or extended at runtime.
fn rebase_new_semantic_plan(plan: &mut crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1, shift: u64) -> Result<(), String> {
    fn rebase(terms: &mut PalwRenderingClaimTermsV1, shift: u64) -> Result<(), String> {
        terms.open_until_daa = add(terms.open_until_daa, shift)?;
        terms.stream_until_daa = add(terms.stream_until_daa, shift)?;
        terms.collect_until_daa = add(terms.collect_until_daa, shift)?;
        terms.material_until_daa = add(terms.material_until_daa, shift)?;
        terms.bond_exit_after_daa = add(terms.bond_exit_after_daa, shift)?;
        Ok(())
    }
    rebase(&mut plan.terms, shift)?;
    for stage in &mut plan.stages {
        rebase(&mut stage.terms, shift)?;
    }
    plan.check_v1()
}

impl PalwTirPublicAllocationPlanV1 {
    fn summed_v1(&self) -> Result<PalwRenderingResourcePlanV1, String> {
        let mut total = self
            .acquisition
            .envelope
            .checked_sum_v1(self.preparation_work.envelope)?
            .checked_sum_v1(self.owner_work.envelope)?
            .checked_sum_v1(self.preparation.envelope)?
            .checked_sum_v1(self.executor.envelope)?
            .checked_sum_v1(self.terminal.envelope)?
            .checked_sum_v1(self.controls)?;
        if let Some(flat) = self.flat_trace {
            total = total.checked_sum_v1(flat)?;
        }
        if let Some(semantic) = &self.semantic {
            total = total.checked_sum_v1(semantic.terms.plan)?;
        }
        Ok(total)
    }
    fn resource_id_v1(&self) -> Result<Hash64, String> {
        // Semantic resource id already binds the true original EOF fact and
        // immutable registered statement while excluding inventory timestamps.
        hash(
            b"misaka-palw/tir-full-resource/v1",
            &(
                self.version,
                self.source_id,
                self.original_claim,
                self.rules_id,
                self.semantic_resource_plan_id,
                self.maximum_semantic_history_records,
                &self.acquisition,
                self.flat_trace,
                &self.preparation_work,
                &self.owner_work,
                &self.preparation,
                &self.executor,
                &self.terminal,
                self.controls,
            ),
        )
    }
    fn accepted_id_v1(&self) -> Result<Hash64, String> {
        let mut copy = self.clone();
        copy.allocation_id = Hash64::default();
        hash(b"misaka-palw/tir-full-accepted/v1", &copy)
    }
    /// Structural checksum only. No deserialized plan proves native payment,
    /// registry provenance, qualified custody or the native accepting clock.
    pub fn check_v1(&self) -> Result<(), String> {
        if self.version != VERSION
            || self.terms.plan != self.summed_v1()?
            || self.resource_plan_id != self.resource_id_v1()?
            || self.allocation_id != self.accepted_id_v1()?
            || self.preparation_until_daa != add(self.terms.accepted_daa, self.preparation.preparation_daa)?
            || self.terms.open_until_daa != add(self.preparation_until_daa, self.acquisition.config.acquisition_daa)?
            || self.terms.open_until_daa >= self.terms.stream_until_daa
            || self.terms.stream_until_daa >= self.terms.collect_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.material_until_daa != self.terms.bond_exit_after_daa
            || self.semantic.is_some() != self.semantic_resource_plan_id.is_some()
        {
            return Err("native TIR full allocation changes SUM, components, commitment or original clock".into());
        }
        if let Some(semantic) = &self.semantic {
            semantic.check_v1()?;
            if semantic.terms.accepted_daa != self.terms.accepted_daa
                || semantic.terms.stream_until_daa != self.terms.stream_until_daa
                || semantic.terms.material_until_daa != self.terms.material_until_daa
            {
                return Err("new semantic component changes original full allocation identity or fixed schedule".into());
            }
        }
        Ok(())
    }
}

/// Re-derive from native immutable inputs, not from the carried plan's caps.
/// Equality is a quote check only; admission still owes actual payment/grants.
#[allow(clippy::too_many_arguments)]
pub fn check_tir_public_allocation_native_v1(
    carried: &PalwTirPublicAllocationPlanV1,
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    rules: PalwTirPublicAllocationRulesV1,
) -> Result<(), String> {
    carried.check_v1()?;
    match derive_tir_public_allocation_v1(geometry, original, header, record, layout, semantics, inventory, rules)? {
        PalwTirPublicAllocationQuoteV1::Complete(expected) if expected.as_ref() == carried => Ok(()),
        PalwTirPublicAllocationQuoteV1::Complete(_) => Err("carried TIR plan substitutes native derivation".into()),
        PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. } => Err(format!("native TIR allocation is incomplete: {missing:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        geometry: PalwStagedClaimGeometryV1,
        original: crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        header: crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        record: crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: crate::palw_tir_class_v1::PalwTirLayoutV1,
        semantics: crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        rules: PalwTirPublicAllocationRulesV1,
    }
    fn fixture(flat: bool, constrained: bool) -> Fixture {
        use crate::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3};
        use crate::palw_freeprompt_v3::*;
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut binding = x.binding.clone();
        if flat {
            let mut program = misaka_palw_tir::TirProgramV1::decode_canonical(&binding.class.program).unwrap();
            program.logits_scheme_id = crate::palw_step_refute::flat_logits_scheme_id_v1().as_byte_slice().try_into().unwrap();
            binding.class.program = program.encode();
        }
        let class_id = binding.class.class_id(&binding.artifact_root);
        let (_, _, mut job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
        job.version = PALW_FP_V3_VERSION;
        job.decode = None;
        job.tail = None;
        job.class_id = class_id;
        job.tokenizer_id = binding.class.tokenizer_id;
        job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&x.prompt);
        job.prompt_tokens = x.prompt.len() as u32;
        job.decode_token_limit = x.generated.len() as u32;
        job.max_context_tokens = 64;
        if constrained {
            use crate::palw_decode_constraint_v1::*;
            let grammar = PalwDecodeConstraintV1 {
                version: 1,
                compiler_id: Hash64::from_u64_word(91),
                start_frame: 0,
                frames: vec![PalwConstraintFrameV1 {
                    start: 0,
                    nodes: vec![PalwConstraintNodeV1 {
                        accepting: true,
                        edges: vec![PalwConstraintEdgeV1 { lo: 0, hi: 255, action: PalwConstraintActionV1::Goto(0) }],
                    }],
                }],
            };
            PalwDecodeConstraintV1::from_bytes(&grammar.to_bytes()).unwrap();
            job.version = crate::palw_fp_constraint_job_v1::PALW_FP_CONSTRAINT_VERSION;
            job.tail = Some(PalwFpJobTailV1::Constraint(crate::palw_fp_constraint_job_v1::PalwFpConstraintTailV1 {
                constraint: grammar.to_bytes(),
                table_root: Hash64::from_u64_word(92),
            }));
        }
        let class = PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: class_id,
            cu_ruleset_id: Hash64::default(),
        };
        let facts = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: x.hashes.len() as u64,
        };
        binding.job_context = crate::palw_fp_execution_v3::palw_fp_job_context_v3(&job, &class, &facts, b"testnet-12").unwrap();
        let context = binding.job_context.context_hash();
        let hashes: Vec<_> =
            x.preimages.iter().map(|leaf| crate::palw_step_leg::step_tile_leaf_hash_v1(&context, &class_id, leaf)).collect();
        binding.step_merkle_root = crate::palw_step_leg::step_merkle_root_v1(&hashes).unwrap();
        binding.full_logits_trace_root = if flat {
            crate::palw_step_refute::base0_logits_trace_root_v1(&binding.job_context, &x.rows, &x.generated)
        } else {
            crate::palw_step_refute::tiled_logits_trace_root_v1(&binding.job_context, &x.rows, &x.generated).unwrap()
        };
        binding.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &context,
            &binding.full_logits_trace_root,
            &class_id,
            binding.step_leaf_count,
            &binding.step_merkle_root,
        );
        let original = PalwFreePromptCommitmentV3 {
            job,
            trace_root: binding.full_logits_trace_root,
            output_root: crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &x.generated),
            schedule_root: Hash64::default(),
            execution_root: binding.committed_execution_root,
            decode_tokens_executed: x.generated.len() as u32,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            work_leaves: binding.step_leaf_count,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 100,
        };
        let geometry = PalwStagedClaimGeometryV1 {
            class_id,
            producer: crate::palw_state_v2::PalwBondKeyV2(original.job.executor_bond),
            execution_root: original.execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&original),
            accepted_daa: 100,
        };
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&binding.class, &binding.artifact_root).unwrap();
        let mut semantics = crate::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(
            class_id,
            binding.artifact_root,
            &binding.job_context,
            100,
        );
        if constrained {
            semantics.token_table_root = Some(Hash64::from_u64_word(92));
        }
        assert!(semantics.well_formed_for(&class_id, &binding.artifact_root));
        Fixture {
            geometry,
            original,
            header: crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&binding),
            record,
            layout: binding.class.layout,
            semantics,
            rules: PalwTirPublicAllocationRulesV1 {
                maximum_leaves: 1 << 40,
                maximum_custodians: 1,
                court: PalwCourtParamsV2::new(1 << 40, 45, 2).unwrap(),
                dissection: true,
                prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                config: PalwStagedPublicCourtConfigV1::network_v1(),
            },
        }
    }
    fn quote(f: &Fixture, semantics: bool) -> PalwTirPublicAllocationQuoteV1 {
        derive_tir_public_allocation_v1(
            &f.geometry,
            &f.original,
            &f.header,
            &f.record,
            &f.layout,
            semantics.then_some(&f.semantics),
            None,
            f.rules,
        )
        .unwrap()
    }
    fn complete(f: &Fixture, semantics: bool) -> Box<PalwTirPublicAllocationPlanV1> {
        match quote(f, semantics) {
            PalwTirPublicAllocationQuoteV1::Complete(plan) => plan,
            missing => panic!("native fixture lacks a complete resource derivation: {missing:?}"),
        }
    }
    #[test]
    fn generic_native_tir_gets_real_sum_without_a_fourth_semantic_family() {
        let f = fixture(false, false);
        let plan = complete(&f, false);
        assert!(plan.semantic.is_none());
        assert!(plan.flat_trace.is_none());
        plan.check_v1().unwrap();
        assert!(plan.terms.plan.covers_v1(plan.owner_work.envelope));
        assert!(plan.terms.plan.covers_v1(plan.preparation_work.envelope));
        assert!(plan.terms.plan.covers_v1(plan.executor.envelope));
        assert!(plan.terms.plan.covers_v1(plan.terminal.envelope));
        assert!(plan.terms.plan.maximum_stream_bytes > plan.acquisition.envelope.maximum_stream_bytes);
        assert!(plan.preparation_until_daa > f.geometry.accepted_daa);
        assert!(plan.terms.open_until_daa > plan.preparation_until_daa);
        assert!(plan.executor.envelope.resident_ram_bytes >= plan.executor.inventory_ram_bytes);
        assert!(plan.executor.maximum_file_operations > 0);
        assert!(plan.owner_work.rules.maximum_replay_io_bytes > 0);
        assert!(plan.owner_work.per_actor.io_bytes >= plan.owner_work.rules.maximum_replay_io_bytes);
        check_tir_public_allocation_native_v1(&plan, &f.geometry, &f.original, &f.header, &f.record, &f.layout, None, None, f.rules)
            .unwrap();
    }
    #[test]
    fn flat_constraint_new_quote_keeps_original_domain_and_old_v1_refusal() {
        let f = fixture(true, true);
        let old = crate::palw_staged_preclaim_v1::staged_preclaim_tir_quote_v1(
            &f.geometry,
            &f.original,
            &crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(f.header.clone()),
            &f.record,
            Some(&f.layout),
            &f.semantics,
            None,
            f.rules.maximum_custodians,
            f.rules.maximum_leaves,
            f.rules.config,
        );
        assert!(old.unwrap_err().contains("flat-scheme TIR"));
        let plan = complete(&f, true);
        assert!(plan.flat_trace.is_some());
        let semantic = plan.semantic.as_ref().unwrap();
        assert_eq!(semantic.stages.len(), 1);
        assert_eq!(semantic.stages[0].family, crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint);
        assert_eq!(semantic.terms.accepted_daa, f.geometry.accepted_daa);
        assert_eq!(semantic.terms.material_until_daa, plan.terms.material_until_daa);
        assert_eq!(
            plan.source_id,
            PalwTirPublicSourceV1::new_v1(
                &f.record,
                &f.layout,
                &f.header,
                f.original.execution_root,
                f.original.trace_root,
                f.rules.maximum_leaves
            )
            .unwrap()
            .source_id_v1()
        );
        assert_eq!(f.header.full_logits_trace_root, f.original.trace_root);
        assert_eq!(f.header.job_context.shape_profile_id, f.record.facts.class_id);
    }
    #[test]
    fn resource_commitment_is_clock_free_accepted_plan_is_not() {
        let mut f = fixture(false, false);
        let before = complete(&f, false);
        f.geometry.accepted_daa += 17;
        let after = complete(&f, false);
        assert_eq!(before.resource_plan_id, after.resource_plan_id);
        assert_ne!(before.allocation_id, after.allocation_id);
        assert_eq!(before.preparation_until_daa + 17, after.preparation_until_daa);
        assert_eq!(before.terms.material_until_daa + 17, after.terms.material_until_daa);
        let mut forged = *after;
        forged.terms.plan.resident_ram_bytes -= 1;
        assert!(forged.check_v1().is_err());
    }
    #[test]
    fn native_rederive_rejects_self_consistent_underfunding_and_foreign_rules() {
        let f = fixture(false, false);
        let mut plan = *complete(&f, false);
        plan.controls.maximum_stream_bytes -= 1;
        plan.terms.plan = plan.summed_v1().unwrap();
        plan.resource_plan_id = plan.resource_id_v1().unwrap();
        plan.allocation_id = plan.accepted_id_v1().unwrap();
        plan.check_v1().unwrap(); // A checksum is not a native admission marker.
        assert!(
            check_tir_public_allocation_native_v1(
                &plan,
                &f.geometry,
                &f.original,
                &f.header,
                &f.record,
                &f.layout,
                None,
                None,
                f.rules
            )
            .is_err()
        );
        let mut changed = f.rules;
        changed.maximum_custodians += 1;
        assert!(
            check_tir_public_allocation_native_v1(
                &complete(&f, false),
                &f.geometry,
                &f.original,
                &f.header,
                &f.record,
                &f.layout,
                None,
                None,
                changed
            )
            .is_err()
        );
        changed.config.prices.rent_per_parser_step = u64::MAX;
        assert!(
            derive_tir_public_allocation_v1(&f.geometry, &f.original, &f.header, &f.record, &f.layout, None, None, changed).is_err()
        );
    }
    #[test]
    fn missing_statement_is_incomplete_not_zero_and_tree_geometry_is_exact() {
        let f = fixture(true, true);
        assert!(matches!(quote(&f, false), PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. }
            if missing == vec![PalwTirPublicAllocationMissingV1::NativeSemanticStatement]));
        assert_eq!(tir_public_tree_disk_bytes_v1(0).unwrap(), 0);
        assert_eq!(tir_public_tree_disk_bytes_v1(1).unwrap(), 64);
        assert_eq!(tir_public_tree_disk_bytes_v1(3).unwrap(), (3 + 2 + 1) * 64);
        assert_eq!(tir_public_tree_disk_bytes_v1(4).unwrap(), (4 + 2 + 1) * 64);
        assert!(tir_public_tree_disk_bytes_v1(u64::MAX).is_err());
    }
}
