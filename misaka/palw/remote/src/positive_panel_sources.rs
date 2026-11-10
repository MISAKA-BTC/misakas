//! A3's bounded G14 source cache for A0 v2/r1. Acquisition calls A2's existing
//! MinerSource port with an authenticated canonical duty; no budgets, service
//! ledgers, completion receipts, source trees or wire formats are created here.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{ErrorKind, Read},
    path::Path,
};

use crate::public_material::{TRACE_OPENING_PREFIX, TRACE_TILE_PREFIX, trace_tile_wire_overhead};
use kaspa_consensus_core::{
    Hash64,
    palw_artifact::verify_artifact_opening_v1,
    palw_attempt_rules_v1::palw_attempt_rendered_output_v1,
    palw_positive_contract_v2::{check_public_original_execution_v1, public_tokenizer_material_v1},
    palw_prompt_ids_v1::{
        PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, PalwPromptIdsOpeningV1, prompt_ids_form_id_v1, prompt_ids_tile_count_v1,
        prompt_token_ids_match_v1, verify_prompt_ids_opening_v1,
    },
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    palw_tir_artifact_v1::palw_tir_graph_ir_root_v1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::{
        PalwTirInventoryIndexV1, PublicOriginalStepRootV1, PublicOriginalTraceCountV1, PublicOriginalTraceIndexV1,
        PublicOriginalTraceTileV1, check_public_original_step_data_v1,
    },
    palw_tir_step_v1::PalwTirStepSpaceV2,
    palw_v2::{PalwJobContextV2, output_commitment_v2},
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{PUBKEY_BYTES, SIGNATURE_BYTES},
    messages::{AdmissionContext, ClaimForm, MinerResultCommit, MinerServiceContract, SourceKind, SourceRequest},
    ports::MinerSource,
    public_one_honest::{
        CheckedManifestV1, ManifestScopeRules, ManifestV1, MaterialScope, OriginalCommit, tir_cold_dependencies_material_v1,
        tir_cold_initial_state_material_v1, tir_inventory_material_v1, tir_output_material_codec_v1, tir_program_material_codec_v1,
    },
};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::{
    PalwTirContainerHeaderV1, PalwTirTensorEntryV1, param_instances_v1, tensor_bytes_v1, write_container_v1_streamed,
};
use misaka_palw_tir_exec::node::{
    TirArtifactV1,
    memory::TirMemoryReserverV1,
    positive_wide_material_v2::{CaptureExpectedV2, WidePanelCaptureV2, WidePanelWorkV2, capture_one_pass_v2},
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
};

use crate::miner::positive_v2::{CanonicalDataDutyV2, MinerErrorV2, PreparedDataV2, StoredUnitV2};
use crate::public_material::{MaterialProvider, public_original_manifest_path};

/// Obtain fixed trace metadata whose root is bound by the signed original.
pub fn fetch_public_trace_index_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
) -> Result<PublicOriginalTraceIndexV1, PanelSourceErrorR1> {
    let path = crate::public_material::public_original_trace_index_path(original.id());
    let bytes = provider
        .get(&path, PublicOriginalTraceIndexV1::WIRE_BYTES)
        .map_err(|error| PanelSourceErrorR1::ReplayProvider(error.0))?
        .ok_or(PanelSourceErrorR1::Incomplete)?;
    if bytes.len() != PublicOriginalTraceIndexV1::WIRE_BYTES {
        return Err(PanelSourceErrorR1::Binding);
    }
    let index: PublicOriginalTraceIndexV1 = borsh::from_slice(&bytes).map_err(|_| PanelSourceErrorR1::Binding)?;
    index.authenticate(original).map_err(|_| PanelSourceErrorR1::Binding)?;
    Ok(index)
}

/// Caller-reserved bounds for acquiring the complete published trace.
#[derive(Clone, Copy)]
pub struct PublicTraceAcquisitionV1 {
    pub max_tiles: u64,
    pub max_wire_bytes: u64,
}

pub(crate) fn check_public_replay_trace_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    replayed: &PublicOriginalTraceIndexV1,
    hashes: &[Hash64],
    total_bytes: u64,
    chunk_bytes: usize,
    limits: PublicTraceAcquisitionV1,
) -> Result<(), PanelSourceErrorR1> {
    let count = replayed.step_leaf_count;
    if count == 0 || count != hashes.len() as u64 {
        return Err(PanelSourceErrorR1::Binding);
    }
    // Reserve the longest possible opening for every tile before fetching.
    let overhead = trace_tile_wire_overhead(count) as u64;
    let max_wire = count.checked_mul(overhead).and_then(|bytes| total_bytes.checked_add(bytes));
    if count > limits.max_tiles || max_wire.is_none_or(|bytes| bytes > limits.max_wire_bytes) {
        return Err(PanelSourceErrorR1::Budget);
    }
    let published = fetch_public_trace_index_v1(provider, original)?;
    if published != *replayed {
        return Err(PanelSourceErrorR1::Binding);
    }
    let mut remaining = total_bytes.checked_sub(PublicOriginalTraceIndexV1::WIRE_BYTES as u64).ok_or(PanelSourceErrorR1::Binding)?;
    for (index, hash) in hashes.iter().enumerate() {
        let max_value_bytes = chunk_bytes.min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let (opening, tile) =
            fetch_public_trace_tile_v1(provider, original, &published, index as u64, limits.max_tiles, max_value_bytes)?;
        if opening.leaf_hash != *hash {
            return Err(PanelSourceErrorR1::Binding);
        }
        remaining -= tile.values_le.len() as u64;
    }
    if remaining != 0 {
        return Err(PanelSourceErrorR1::Binding);
    }
    Ok(())
}

/// Fetch one opening and tile, rejecting invalid vector lengths before
/// deserializing the payload and authenticating it against the original.
pub fn fetch_public_trace_tile_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    trace: &PublicOriginalTraceIndexV1,
    index: u64,
    max_step_leaf_count: u64,
    max_value_bytes: usize,
) -> Result<(PalwStepOpeningV1, PalwStepTileLeafV1), PanelSourceErrorR1> {
    trace.check(original, max_step_leaf_count).map_err(|_| PanelSourceErrorR1::Binding)?;
    if index >= trace.step_leaf_count {
        return Err(PanelSourceErrorR1::Binding);
    }
    // Borsh opening prefix, at most 64 siblings, then tile prefix.
    let depth = u64::BITS - (trace.step_leaf_count - 1).leading_zeros();
    let cap = trace_tile_wire_overhead(trace.step_leaf_count)
        .saturating_add(max_value_bytes)
        .min(crate::public_material::MATERIAL_UNIT_MAX_BYTES);
    let path = crate::public_material::public_original_trace_tile_path(original.id(), index);
    let bytes = provider
        .get(&path, cap)
        .map_err(|error| PanelSourceErrorR1::ReplayProvider(error.0))?
        .ok_or(PanelSourceErrorR1::Incomplete)?;
    let read_u32 = |offset| {
        bytes.get(offset..offset + 4).and_then(|raw| raw.try_into().ok()).map(u32::from_le_bytes).ok_or(PanelSourceErrorR1::Binding)
    };
    let siblings = read_u32(TRACE_OPENING_PREFIX - 4)?;
    if siblings > depth || bytes.len() > cap {
        return Err(PanelSourceErrorR1::Binding);
    }
    let header = TRACE_OPENING_PREFIX + siblings as usize * 64 + TRACE_TILE_PREFIX;
    let values = read_u32(header - 4)? as usize;
    if values > max_value_bytes || bytes.len().checked_sub(header) != Some(values) {
        return Err(PanelSourceErrorR1::Binding);
    }
    let (opening, tile) = borsh::from_slice(&bytes).map_err(|_| PanelSourceErrorR1::Binding)?;
    let root =
        PublicOriginalStepRootV1 { full_logits_trace_root: trace.full_logits_trace_root, step_leaf_count: trace.step_leaf_count };
    check_public_original_step_data_v1(original, &root, index, &opening, &tile, max_step_leaf_count, max_value_bytes)
        .map_err(|_| PanelSourceErrorR1::Binding)?;
    Ok((opening, tile))
}

/// Find the first authenticated mismatch in a caller-reserved range. Missing
/// material stops the scan. The returned tile is input to local adjudication.
#[allow(clippy::too_many_arguments)]
pub fn find_public_trace_difference_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    trace: &PublicOriginalTraceIndexV1,
    replay: &WidePanelCaptureV2<'_>,
    range: std::ops::Range<u64>,
    max_tiles: u64,
    max_step_leaf_count: u64,
    max_value_bytes: usize,
) -> Result<Option<PublicOriginalTraceTileV1>, PanelSourceErrorR1> {
    trace.check(original, max_step_leaf_count).map_err(|_| PanelSourceErrorR1::Binding)?;
    if range.start >= range.end || range.end > trace.step_leaf_count || range.end - range.start > max_tiles {
        return Err(PanelSourceErrorR1::Budget);
    }
    let hashes = replay.public_replay_leaf_hashes(original, trace.step_leaf_count).map_err(PanelSourceErrorR1::ReplayMismatch)?;
    if replay.run().step_merkle_root() == trace.step_merkle_root {
        return Ok(None);
    }
    for index in range {
        let (opening, tile) = fetch_public_trace_tile_v1(provider, original, trace, index, max_step_leaf_count, max_value_bytes)?;
        if opening.leaf_hash != hashes[index as usize] {
            return Ok(Some(PublicOriginalTraceTileV1 { original: original.id(), trace: trace.clone(), opening, tile }));
        }
    }
    Ok(None)
}

/// Fetch the program and prompt inputs for one checked original. Each payload
/// is bounded before acquisition and authenticated before use by the replay.
pub fn fetch_public_replay_inputs_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    form: PalwPromptIdsFormV1,
) -> Result<(TirProgramV1, Vec<u32>), PanelSourceErrorR1> {
    let program =
        open_public_tir_program_v1(manifest, &fetch_public_replay_material(provider, original, manifest, MaterialScope::Program)?)?;
    let prompt = fetch_public_replay_material(provider, original, manifest, MaterialScope::Prompt)?;
    let tokenizer = fetch_public_replay_material(provider, original, manifest, MaterialScope::Tokenizer)?;
    let ids = check_public_prompt_v1(original, manifest, form, &prompt, &tokenizer)?;
    Ok((program, ids))
}

/// Fetch the original's committed output for comparison with Panel replay.
pub fn fetch_public_replay_output_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
) -> Result<Vec<u32>, PanelSourceErrorR1> {
    if manifest.entry(MaterialScope::Output).total_bytes != u64::from(original.decode) * 4 {
        return Err(PanelSourceErrorR1::Binding);
    }
    let bytes = fetch_public_replay_material(provider, original, manifest, MaterialScope::Output)?;
    let ids: Vec<_> = bytes.chunks_exact(4).map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])).collect();
    open_public_output_tokens_v1(original, manifest, &ids)?;
    Ok(ids)
}

/// Caller-reserved acquisition limits for localizing a mismatched replay.
pub struct PublicTraceSearchV1 {
    pub range: std::ops::Range<u64>,
    pub max_tiles: u64,
    pub max_step_leaf_count: u64,
    pub max_value_bytes: usize,
}

/// Authenticate a signed original and replay it from public material. The
/// caller resolves the job, class, and limits from accepted state and owns
/// the temporary container and tape for this attempt.
#[allow(clippy::too_many_arguments)]
pub fn run_cold_public_wide_replay_v1(
    provider: &dyn MaterialProvider,
    signed: &misaka_palw_positive_contract::public_one_honest::SignedPublicOriginalV1,
    job: &PalwJobContextV2,
    class: &PalwTirClassV1,
    scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
    total_limit: u64,
    max_manifest_bytes: u32,
    artifact_limits: PanelSourceLimitsR1,
    container: &Path,
    run_limits: TirWideRunLimitsV2,
    work: &WidePanelWorkV2,
    tape: File,
    reserver: &TirMemoryReserverV1,
    acquisition: PublicTraceAcquisitionV1,
    search: Option<PublicTraceSearchV1>,
) -> Result<(), PanelSourceErrorR1> {
    let original = &signed.original;
    original.authenticate(&signed.key, &signed.signature)?;
    if borsh::object_length(class).map_err(|_| PanelSourceErrorR1::Binding)? as u64 > artifact_limits.max_class_bytes {
        return Err(PanelSourceErrorR1::Budget);
    }
    check_public_original_execution_v1(original, job, class, Hash64::from_bytes(original.artifact.0))?;
    let manifest = fetch_public_manifest_v1(provider, original, scope_limits, total_limit, max_manifest_bytes)?;
    let space = PalwTirStepSpaceV2::new(class).map_err(|error| PanelSourceErrorR1::ReplayExecution(error.to_string()))?;
    let artifact = fetch_public_wide_artifact_v1(provider, original, &manifest, class, container, artifact_limits)?;
    run_public_wide_replay_v1(
        provider,
        original,
        &manifest,
        job,
        class,
        &artifact,
        &space,
        run_limits,
        work,
        tape,
        reserver,
        acquisition,
        search,
    )
}

/// Sign a replay assertion for the exact drawn bond and supported relation plan
/// only after a complete cold replay. Native inclusion still enforces deadlines
/// and the independent local checks required for Final.
#[allow(clippy::too_many_arguments)]
pub fn replay_and_sign_drawn_public_wide_v1(
    provider: &dyn MaterialProvider,
    signed: &misaka_palw_positive_contract::public_one_honest::SignedPublicOriginalV1,
    draw: &kaspa_consensus_core::palw_positive_contract_v2::PreparedPublicDrawV1,
    bond: misaka_palw_positive_contract::public_one_honest::PanelBond,
    job: &PalwJobContextV2,
    class: &PalwTirClassV1,
    scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
    total_limit: u64,
    max_manifest_bytes: u32,
    artifact_limits: PanelSourceLimitsR1,
    container: &Path,
    run_limits: TirWideRunLimitsV2,
    work: &WidePanelWorkV2,
    tape: File,
    reserver: &TirMemoryReserverV1,
    acquisition: PublicTraceAcquisitionV1,
    search: Option<PublicTraceSearchV1>,
    key: &[u8; PUBKEY_BYTES],
    sign: impl FnOnce(&[u8], &[u8]) -> Result<[u8; SIGNATURE_BYTES], String>,
) -> Result<misaka_palw_positive_contract::public_one_honest::SignedFullReplayApprovalV1, PanelSourceErrorR1> {
    use misaka_palw_positive_contract::{
        identity::principal,
        public_one_honest::{FULL_REPLAY_SIGN_CONTEXT, FullReplayApproval, SignedFullReplayApprovalV1, stateless_wide_replay_plan_v1},
    };

    let context = draw.approval_context(bond)?;
    let original = signed.original.id();
    if context.original != original || context.full_scope != original.full_replay_scope(stateless_wide_replay_plan_v1()) {
        return Err(ContractError::Scope.into());
    }
    if principal(key) != context.member {
        return Err(ContractError::Identity.into());
    }
    if borsh::object_length(class).map_err(|_| PanelSourceErrorR1::Binding)? as u64 > artifact_limits.max_class_bytes {
        return Err(PanelSourceErrorR1::Budget);
    }
    let program = class.decode_program().map_err(|_| PanelSourceErrorR1::Binding)?;
    if !program.wide_logits_q24()
        || !program.states.is_empty()
        || program.blocks.iter().any(|block| block.nodes.iter().any(|node| !node.commit))
    {
        return Err(ContractError::UnsupportedProfile.into());
    }
    drop(program);
    run_cold_public_wide_replay_v1(
        provider,
        signed,
        job,
        class,
        scope_limits,
        total_limit,
        max_manifest_bytes,
        artifact_limits,
        container,
        run_limits,
        work,
        tape,
        reserver,
        acquisition,
        search,
    )?;
    let approval = FullReplayApproval { original, draw: context.draw, bond, member: context.member, full_scope: context.full_scope };
    let signature = sign(&approval.signing_digest().0, FULL_REPLAY_SIGN_CONTEXT).map_err(PanelSourceErrorR1::ReplaySigner)?;
    approval.authenticate(&context, key, &signature)?;
    Ok(SignedFullReplayApprovalV1 { approval, key: *key, signature })
}

/// Stream a registered artifact from public inventory rows into a caller-owned
/// temporary container. No model bytes are used for execution until the root
/// of the complete canonical inventory is authenticated.
pub fn fetch_public_wide_artifact_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    class: &PalwTirClassV1,
    container: &Path,
    limits: PanelSourceLimitsR1,
) -> Result<TirArtifactV1, PanelSourceErrorR1> {
    use crate::public_material::{MATERIAL_UNIT_MAX_BYTES, artifact_leaf_path};
    use kaspa_consensus_core::palw_artifact::{PalwArtifactMerkleFrontierV1, artifact_leaf_parts_v1};

    if borsh::object_length(class).map_err(|_| PanelSourceErrorR1::Binding)? as u64 > limits.max_class_bytes {
        return Err(PanelSourceErrorR1::Budget);
    }
    let root = Hash64::from_bytes(original.artifact.0);
    if manifest.original() != original.id()
        || class.class_id(&root).as_bytes() != original.registered_class.0
        || class.graph_ir_root().as_bytes() != original.program.0
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    let program = class.decode_program().map_err(|_| PanelSourceErrorR1::Binding)?;
    guard_inventory_metadata(program.params.iter().map(|param| param.per_layer), program.schedule.layers.len(), limits.max_units)?;
    let inventory = PalwTirInventoryIndexV1::new(&program).ok_or(PanelSourceErrorR1::Binding)?;
    let leaves = inventory.leaf_count();
    if leaves == 0 || u64::from(leaves) > limits.max_units {
        return Err(PanelSourceErrorR1::Budget);
    }
    let entry = manifest.entry(MaterialScope::Artifact);
    // Borsh string, optional layer, byte offset, and byte vector.
    let wire_len = |name: &str, layer: Option<u16>, len: u32| 13 + name.len() + 2 * usize::from(layer.is_some()) + len as usize;
    let mut raw_bytes = 0u64;
    let mut wire_bytes = 0u64;
    for index in 0..leaves {
        let (param, layer, _, len) = inventory.piece_of(index).ok_or(PanelSourceErrorR1::Binding)?;
        let name = &program.params[param as usize].name;
        let wire = wire_len(name, layer, len) as u64;
        if len > entry.chunk_bytes || wire > limits.max_unit_bytes.min(MATERIAL_UNIT_MAX_BYTES as u64) {
            return Err(PanelSourceErrorR1::Budget);
        }
        raw_bytes = raw_bytes.checked_add(u64::from(len)).ok_or(PanelSourceErrorR1::Budget)?;
        wire_bytes = wire_bytes.checked_add(wire).filter(|n| *n <= limits.max_total_bytes).ok_or(PanelSourceErrorR1::Budget)?;
    }
    let rules = ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes };
    if tir_inventory_material_v1(original.artifact, raw_bytes, rules)? != *entry {
        return Err(PanelSourceErrorR1::Binding);
    }
    let mut frontier = PalwArtifactMerkleFrontierV1::default();
    let mut next_index = 0;
    let mut failure = None;
    let result = write_bounded_artifact_container_v1(
        &program,
        class,
        container,
        raw_bytes,
        limits.max_container_bytes,
        &mut |param, layer, out| {
            let read = (|| {
                let name = &program.params[param as usize].name;
                let mut offset = 0;
                while offset < tensor_bytes_v1(&program, param) {
                    let (piece_param, piece_layer, start, len) = inventory.piece_of(next_index).ok_or(PanelSourceErrorR1::Binding)?;
                    if (param, layer, offset) != (piece_param, piece_layer, u64::from(start)) {
                        return Err(PanelSourceErrorR1::Binding);
                    }
                    let cap = wire_len(name, layer, len);
                    let path = artifact_leaf_path(&root, next_index);
                    let bytes = provider
                        .get(&path, cap)
                        .map_err(|error| PanelSourceErrorR1::ReplayProvider(error.0))?
                        .ok_or(PanelSourceErrorR1::Incomplete)?;
                    let prefix = borsh::to_vec(&(name, layer, start, len)).map_err(|_| PanelSourceErrorR1::Binding)?;
                    let values = bytes
                        .strip_prefix(prefix.as_slice())
                        .filter(|values| values.len() == len as usize)
                        .ok_or(PanelSourceErrorR1::Binding)?;
                    frontier.push(artifact_leaf_parts_v1(name, layer, start, values));
                    out.write_all(values).map_err(|error| PanelSourceErrorR1::Artifact(error.to_string()))?;
                    offset += u64::from(len);
                    next_index += 1;
                }
                Ok(())
            })();
            read.map_err(|error| {
                failure = Some(error);
                "public artifact inventory row could not be acquired".into()
            })
        },
    );
    if let Some(error) = failure {
        return Err(error);
    }
    result?;
    if next_index != leaves || frontier.root() != Some(root) {
        return Err(PanelSourceErrorR1::Binding);
    }
    TirArtifactV1::open(container).map_err(PanelSourceErrorR1::Artifact)
}

/// Acquire the public inputs and compare an independent wide TIR run with the
/// original. The caller supplies rooted job/class metadata and reserves
/// acquisition memory and work, including artifact identity verification.
#[allow(clippy::too_many_arguments)]
pub fn run_public_wide_replay_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    job: &PalwJobContextV2,
    class: &PalwTirClassV1,
    artifact: &TirArtifactV1,
    space: &PalwTirStepSpaceV2,
    run_limits: TirWideRunLimitsV2,
    work: &WidePanelWorkV2,
    tape: File,
    reserver: &TirMemoryReserverV1,
    acquisition: PublicTraceAcquisitionV1,
    search: Option<PublicTraceSearchV1>,
) -> Result<(), PanelSourceErrorR1> {
    let expected_root = Hash64::from_bytes(original.artifact.0);
    check_public_original_execution_v1(original, job, class, expected_root)?;
    let initial = manifest.entry(MaterialScope::InitialState);
    let rules = ManifestScopeRules { codec: initial.codec, max_bytes: initial.total_bytes, chunk_bytes: initial.chunk_bytes };
    let (expected, bytes) = tir_cold_initial_state_material_v1(original, rules)?;
    if expected != *initial || expected.subject != original.initial_state {
        return Err(PanelSourceErrorR1::Binding);
    }
    if fetch_public_replay_material(provider, original, manifest, MaterialScope::InitialState)?.as_slice() != bytes.as_slice() {
        return Err(PanelSourceErrorR1::Binding);
    }
    let dependencies = manifest.entry(MaterialScope::Dependencies);
    let rules =
        ManifestScopeRules { codec: dependencies.codec, max_bytes: dependencies.total_bytes, chunk_bytes: dependencies.chunk_bytes };
    let (expected, bytes) = tir_cold_dependencies_material_v1(original, rules)?;
    if expected.subject != original.dependencies {
        return Err(ContractError::UnsupportedProfile.into());
    }
    if expected != *dependencies
        || fetch_public_replay_material(provider, original, manifest, MaterialScope::Dependencies)?.as_slice() != bytes.as_slice()
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    // The wide capture currently accepts the native Flat input commitment.
    let (program, prompt) = fetch_public_replay_inputs_v1(provider, original, manifest, PalwPromptIdsFormV1::Flat)?;
    if &program != space.program() {
        return Err(PanelSourceErrorR1::Binding);
    }
    let entry = manifest.entry(MaterialScope::ExecutionRules);
    let rules = ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes };
    let (expected, bytes) = kaspa_consensus_core::palw_positive_contract_v2::public_wide_tir_rules_material_v1(job, &program, rules)?;
    if expected != *entry
        || fetch_public_replay_material(provider, original, manifest, MaterialScope::ExecutionRules)?.as_slice() != bytes.as_slice()
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    drop(program);
    let output = fetch_public_replay_output_v1(provider, original, manifest)?;
    if &artifact.class().map_err(PanelSourceErrorR1::Artifact)? != class {
        return Err(PanelSourceErrorR1::Binding);
    }
    let (root, _) = artifact.inventory_root().map_err(PanelSourceErrorR1::Artifact)?;
    if root != expected_root {
        return Err(PanelSourceErrorR1::Binding);
    }
    if artifact.composite_ref().is_some() {
        return Err(ContractError::UnsupportedProfile.into());
    }
    let parameter_bytes = artifact
        .header()
        .tensors
        .iter()
        .try_fold(0u64, |bytes, tensor| bytes.checked_add(tensor.bytes))
        .ok_or(PanelSourceErrorR1::Budget)?;
    let entry = manifest.entry(MaterialScope::Artifact);
    let rules = ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes };
    if tir_inventory_material_v1(original.artifact, parameter_bytes, rules)? != *entry {
        return Err(PanelSourceErrorR1::Binding);
    }
    let runner = TirClassRunnerWideQ24V2::new(space, artifact.plan(), artifact.params(), job.shape_profile_id)
        .map_err(PanelSourceErrorR1::ReplayExecution)?;
    let capture = capture_one_pass_v2(
        CaptureExpectedV2 { class, artifact: root, job, space, prompt: &prompt },
        &runner,
        run_limits,
        work,
        tape,
        reserver,
    )
    .map_err(PanelSourceErrorR1::ReplayExecution)?;
    let Err(mismatch) = capture.check_public_original_replay(original, &output) else {
        let trace = manifest.entry(MaterialScope::Trace);
        let rules = ManifestScopeRules { codec: trace.codec, max_bytes: trace.total_bytes, chunk_bytes: trace.chunk_bytes };
        if capture.public_trace_material(rules)? != *trace {
            return Err(PanelSourceErrorR1::Binding);
        }
        let replayed = capture.public_trace_index();
        let hashes =
            capture.public_replay_leaf_hashes(original, replayed.step_leaf_count).map_err(PanelSourceErrorR1::ReplayMismatch)?;
        let chunk_bytes = usize::try_from(trace.chunk_bytes).map_err(|_| PanelSourceErrorR1::Budget)?;
        return check_public_replay_trace_v1(provider, original, &replayed, hashes, trace.total_bytes, chunk_bytes, acquisition);
    };
    if let Some(search) = search
        && capture.run().execution_root().as_bytes() != original.trace_root.0
    {
        let trace = fetch_public_trace_index_v1(provider, original)?;
        let expected_leaf_count = capture.run().leaf_count();
        if trace.step_leaf_count != expected_leaf_count {
            let evidence = PublicOriginalTraceCountV1 { original: original.id(), trace };
            return Err(PanelSourceErrorR1::TraceLeafCountMismatch { evidence: Box::new(evidence), expected: expected_leaf_count });
        }
        let difference = find_public_trace_difference_v1(
            provider,
            original,
            &trace,
            &capture,
            search.range,
            search.max_tiles,
            search.max_step_leaf_count,
            search.max_value_bytes,
        )?;
        if let Some(difference) = difference {
            return Err(PanelSourceErrorR1::LocatedTraceDifference(Box::new(difference)));
        }
    }
    Err(PanelSourceErrorR1::ReplayMismatch(mismatch))
}

fn fetch_public_replay_material(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    scope: MaterialScope,
) -> Result<Vec<u8>, PanelSourceErrorR1> {
    if manifest.original() != original.id() {
        return Err(PanelSourceErrorR1::Binding);
    }
    let entry = manifest.entry(scope);
    let size = usize::try_from(entry.total_bytes).map_err(|_| PanelSourceErrorR1::Budget)?;
    let max = match scope {
        MaterialScope::Program => misaka_palw_tir::program::MAX_PROGRAM_BYTES,
        _ => crate::public_material::MATERIAL_UNIT_MAX_BYTES,
    };
    if size > max || entry.total_bytes > u64::from(entry.chunk_bytes) {
        return Err(PanelSourceErrorR1::Budget);
    }
    let path = crate::public_material::public_original_replay_path(original.id(), scope).ok_or(ContractError::UnsupportedProfile)?;
    let bytes = provider
        .get(&path, size)
        .map_err(|error| PanelSourceErrorR1::ReplayProvider(error.0))?
        .ok_or(PanelSourceErrorR1::Incomplete)?;
    if bytes.len() != size {
        return Err(PanelSourceErrorR1::Binding);
    }
    Ok(bytes)
}

/// Obtain the original manifest from a configured public material provider.
pub fn fetch_public_manifest_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
    total_limit: u64,
    max_manifest_bytes: u32,
) -> Result<CheckedManifestV1, PanelSourceErrorR1> {
    let size = ManifestV1::fetch_size(max_manifest_bytes)?;
    let bytes = provider
        .get(&public_original_manifest_path(original.id()), size as usize)
        .map_err(|error| PanelSourceErrorR1::ManifestProvider(error.0))?
        .ok_or(PanelSourceErrorR1::Incomplete)?;
    read_public_manifest_v1(original, scope_limits, total_limit, max_manifest_bytes, bytes.len() as u64, bytes.as_slice())
}

/// The Panel either acquired the manifest or prepared a first request under
/// its own draw slot. A prepared request still needs native inclusion.
pub enum PublicManifestAcquisitionV1 {
    Acquired(Box<CheckedManifestV1>),
    Request(Box<misaka_palw_positive_contract::public_one_honest::SignedManifestRequestV1>),
}

#[allow(clippy::too_many_arguments)]
pub fn acquire_or_request_public_manifest_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    admitted: &kaspa_consensus_core::palw_positive_contract_v2::PreparedPublicOriginalV1,
    draw: &kaspa_consensus_core::palw_positive_contract_v2::PreparedPublicDrawV1,
    bond: misaka_palw_positive_contract::public_one_honest::PanelBond,
    supply_obligation: misaka_palw_positive_contract::identity::Digest,
    scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
    total_limit: u64,
    max_manifest_bytes: u32,
    key: &[u8; PUBKEY_BYTES],
    sign: impl FnOnce(&[u8], &[u8]) -> Result<[u8; SIGNATURE_BYTES], String>,
) -> Result<PublicManifestAcquisitionV1, PanelSourceErrorR1> {
    use misaka_palw_positive_contract::{
        identity::principal,
        public_one_honest::{MANIFEST_REQUEST_SIGN_CONTEXT, SignedManifestRequestV1, stateless_wide_replay_plan_v1},
    };
    match fetch_public_manifest_v1(provider, original, scope_limits, total_limit, max_manifest_bytes) {
        Ok(manifest) => return Ok(PublicManifestAcquisitionV1::Acquired(Box::new(manifest))),
        Err(PanelSourceErrorR1::Incomplete) => {}
        Err(error) => return Err(error),
    }
    let request = admitted.drawn_manifest_request(draw, original, bond, stateless_wide_replay_plan_v1(), supply_obligation)?;
    if principal(key) != request.signer {
        return Err(ContractError::Identity.into());
    }
    let signature = sign(&request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT).map_err(PanelSourceErrorR1::ReplaySigner)?;
    request.authenticate(request.signer, key, &signature)?;
    Ok(PublicManifestAcquisitionV1::Request(Box::new(SignedManifestRequestV1 { request, key: *key, signature })))
}

/// Read a single length-framed manifest response and authenticate it before
/// interpreting any entries. The caller provides the frame's body length
/// and applies the transport deadline.
pub fn read_public_manifest_v1(
    original: &OriginalCommit,
    scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
    total_limit: u64,
    max_manifest_bytes: u32,
    frame_body_bytes: u64,
    mut body: impl Read,
) -> Result<CheckedManifestV1, PanelSourceErrorR1> {
    let size = ManifestV1::fetch_size(max_manifest_bytes)?;
    if frame_body_bytes != u64::from(size) {
        return Err(ContractError::NonCanonical.into());
    }
    let mut bytes = [0; ManifestV1::WIRE_BYTES as usize];
    body.read_exact(&mut bytes).map_err(|error| PanelSourceErrorR1::ManifestRead(error.kind()))?;
    Ok(original.authenticate_manifest(&bytes, max_manifest_bytes)?.decode(original, scope_limits, total_limit)?)
}

/// Authenticate the complete canonical program before decoding it for replay.
/// The caller bounds acquisition using the checked manifest's Program entry.
pub fn open_public_tir_program_v1(manifest: &CheckedManifestV1, bytes: &[u8]) -> Result<TirProgramV1, PanelSourceErrorR1> {
    let entry = manifest.entry(MaterialScope::Program);
    if entry.codec != tir_program_material_codec_v1()
        || entry.content_root != entry.subject
        || bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || entry.total_bytes > u64::from(entry.chunk_bytes)
        || entry.total_bytes != bytes.len() as u64
        || palw_tir_graph_ir_root_v1(bytes).as_bytes() != entry.subject.0
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    TirProgramV1::decode_canonical(bytes).map_err(|_| PanelSourceErrorR1::Binding)
}

/// Authenticate tokenizer bytes before handing them to the profile's parser.
pub fn check_public_tokenizer_v1(manifest: &CheckedManifestV1, bytes: &[u8]) -> Result<(), PanelSourceErrorR1> {
    let entry = manifest.entry(MaterialScope::Tokenizer);
    let rules = ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes };
    if public_tokenizer_material_v1(bytes, rules)? != *entry {
        return Err(PanelSourceErrorR1::Binding);
    }
    Ok(())
}

/// Replay plain text with the native byte-level tokenizer, without allowing
/// text to introduce special tokens. Run inside the Panel's reserved work.
pub fn check_public_prompt_v1(
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    form: PalwPromptIdsFormV1,
    prompt: &[u8],
    tokenizer: &[u8],
) -> Result<Vec<u32>, PanelSourceErrorR1> {
    if manifest.original() != original.id() {
        return Err(PanelSourceErrorR1::Binding);
    }
    let text = manifest.open_prompt_text(prompt)?;
    check_public_tokenizer_v1(manifest, tokenizer)?;
    let tokenizer = misaka_palw_base0::tokenizer::QwenTokenizer::from_json(tokenizer).map_err(|_| PanelSourceErrorR1::Binding)?;
    let ids = tokenizer.encode_without_specials(text).map_err(|_| PanelSourceErrorR1::Binding)?;
    check_public_input_tokens_v1(original, manifest, form, &ids)?;
    Ok(ids)
}

/// Authenticate the published output data under the original execution context.
pub fn open_public_output_tokens_v1(
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    ids: &[u32],
) -> Result<(), PanelSourceErrorR1> {
    let entry = manifest.entry(MaterialScope::Output);
    if manifest.original() != original.id()
        || entry.codec != tir_output_material_codec_v1()
        || entry.content_root != original.output
        || entry.total_bytes != u64::from(original.decode) * 4
        || entry.total_bytes > u64::from(entry.chunk_bytes)
        || ids.len() != original.decode as usize
        || output_commitment_v2(&Hash64::from_bytes(original.execution.0), ids, &palw_attempt_rendered_output_v1(ids)).as_bytes()
            != original.output.0
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    Ok(())
}

/// Authenticate the complete token sequence for a job using the native flat hash.
pub fn open_public_flat_input_tokens_v1(
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    ids: &[u32],
) -> Result<(), PanelSourceErrorR1> {
    check_public_input_tokens_v1(original, manifest, PalwPromptIdsFormV1::Flat, ids)
}

fn check_public_input_tokens_v1(
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    form: PalwPromptIdsFormV1,
    ids: &[u32],
) -> Result<(), PanelSourceErrorR1> {
    let entry = manifest.entry(MaterialScope::InputTokens);
    let canonical_layout = match form {
        PalwPromptIdsFormV1::Flat => entry.total_bytes <= u64::from(entry.chunk_bytes),
        PalwPromptIdsFormV1::MerkleV1 => entry.chunk_bytes == PALW_PROMPT_IDS_TILE_LEN * 4,
    };
    if manifest.original() != original.id()
        || entry.codec.0 != prompt_ids_form_id_v1(form).as_bytes()
        || entry.content_root != original.input_tokens
        || entry.total_bytes != u64::from(original.prefill) * 4
        || !canonical_layout
        || ids.len() != original.prefill as usize
        || !prompt_token_ids_match_v1(form, ids, &Hash64::from_bytes(entry.content_root.0))
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    Ok(())
}

/// Reuse the native token opening at the same index as the manifest range.
pub fn open_public_input_token_tile_v1<'a>(
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    index: u32,
    opening: &'a PalwPromptIdsOpeningV1,
) -> Result<&'a [u32], PanelSourceErrorR1> {
    let entry = manifest.entry(MaterialScope::InputTokens);
    let range = entry.chunk_range(u64::from(index))?;
    if manifest.original() != original.id()
        || entry.codec.0 != prompt_ids_form_id_v1(PalwPromptIdsFormV1::MerkleV1).as_bytes()
        || entry.content_root != original.input_tokens
        || entry.total_bytes != u64::from(original.prefill) * 4
        || entry.chunk_bytes != PALW_PROMPT_IDS_TILE_LEN * 4
        || opening.tile_index != index
        || opening.tile_ids.len() as u64 != (range.end - range.start) / 4
        || opening.opening.siblings.len() > 32
        || verify_prompt_ids_opening_v1(&Hash64::from_bytes(entry.content_root.0), original.prefill, opening).is_err()
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    Ok(&opening.tile_ids)
}

#[derive(Clone, Copy, Debug)]
pub struct PanelSourceLimitsR1 {
    pub max_units: u64,
    pub max_unit_bytes: u64,
    pub max_total_bytes: u64,
    pub max_class_bytes: u64,
    pub max_container_bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PanelSourceErrorR1 {
    Contract(ContractError),
    Miner(MinerErrorV2),
    Binding,
    Budget,
    Incomplete,
    ManifestRead(ErrorKind),
    ManifestProvider(String),
    ReplayProvider(String),
    ReplayExecution(String),
    ReplaySigner(String),
    ReplayMismatch(String),
    LocatedTraceDifference(Box<PublicOriginalTraceTileV1>),
    TraceLeafCountMismatch { evidence: Box<PublicOriginalTraceCountV1>, expected: u64 },
    Artifact(String),
}
impl From<ContractError> for PanelSourceErrorR1 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}
impl From<MinerErrorV2> for PanelSourceErrorR1 {
    fn from(error: MinerErrorV2) -> Self {
        Self::Miner(error)
    }
}

fn guard_inventory_metadata(
    per_layer_params: impl Iterator<Item = bool>,
    layers: usize,
    max_units: u64,
) -> Result<(), PanelSourceErrorR1> {
    let mut instances = 0u64;
    for per_layer in per_layer_params {
        instances = instances.checked_add(if per_layer { layers as u64 } else { 1 }).ok_or(PanelSourceErrorR1::Budget)?;
        if instances > max_units {
            return Err(PanelSourceErrorR1::Budget);
        }
    }
    Ok(())
}

/// Local authenticated bytes only. Native admission, obligations, fees and
/// completion remain with the passed A4 duty/A2 responder. Transfer by exporting
/// units and importing into another cache; every opening is checked again.
pub struct PanelSourceCacheR1 {
    result: MinerResultCommit,
    contract: MinerServiceContract,
    class: PalwTirClassV1,
    program: TirProgramV1,
    inventory: PalwTirInventoryIndexV1,
    prompt_form: PalwPromptIdsFormV1,
    limits: PanelSourceLimitsR1,
    retained_bytes: u64,
    entries: BTreeMap<(SourceKind, u64), StoredUnitV2>,
}
impl PanelSourceCacheR1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        admission: &AdmissionContext,
        result: MinerResultCommit,
        contract: MinerServiceContract,
        class: PalwTirClassV1,
        prompt_form: PalwPromptIdsFormV1,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        limits: PanelSourceLimitsR1,
        now: u64,
    ) -> Result<Self, PanelSourceErrorR1> {
        admission.check(ClaimForm::PositiveResultV2, &result)?;
        result.authenticate(key, signature)?;
        contract.validate(&result, now)?;
        if limits.max_units == 0
            || limits.max_unit_bytes == 0
            || limits.max_total_bytes == 0
            || limits.max_class_bytes == 0
            || limits.max_container_bytes == 0
            || borsh::object_length(&class).map_err(|_| PanelSourceErrorR1::Binding)? as u64 > limits.max_class_bytes
        {
            return Err(PanelSourceErrorR1::Budget);
        }
        // Caller-owned metadata can have tiny visible lengths but enormous
        // spare Vec/String capacities. Retain only freshly cloned contents.
        let normalized_class = class.clone();
        drop(class);
        let class = normalized_class;
        if class.class_id(&Hash64::from_bytes(result.artifact.0)).as_bytes() != result.registered_class.0
            || class.graph_ir_root().as_bytes() != result.program.0
        {
            return Err(PanelSourceErrorR1::Binding);
        }
        // The registered tokenizer is part of class_id; Result.tokenizer is
        // independently pinned by the actual expected job, which may use zero.
        let program = class.decode_program().map_err(|_| PanelSourceErrorR1::Binding)?;
        // The inventory index expands per-layer instance lists. Bound that
        // metadata before construction using the conservative all-layers count.
        // This is a local RAM refusal, not consensus admission: a param read by
        // only a subset of layers may be conservatively declined by this cache.
        guard_inventory_metadata(program.params.iter().map(|param| param.per_layer), program.schedule.layers.len(), limits.max_units)?;
        let inventory = PalwTirInventoryIndexV1::new(&program).ok_or(PanelSourceErrorR1::Binding)?;
        let input_units = match prompt_form {
            PalwPromptIdsFormV1::Flat => 1,
            PalwPromptIdsFormV1::MerkleV1 => prompt_ids_tile_count_v1(result.prefill as u64).ok_or(PanelSourceErrorR1::Binding)?,
        };
        if u64::from(inventory.leaf_count()).checked_add(input_units).filter(|count| *count <= limits.max_units).is_none()
            || (result.prefill as u64).checked_mul(4).filter(|bytes| *bytes <= limits.max_total_bytes).is_none()
        {
            return Err(PanelSourceErrorR1::Budget);
        }
        Ok(Self { result, contract, class, program, inventory, prompt_form, limits, retained_bytes: 0, entries: BTreeMap::new() })
    }

    pub fn units(&self, kind: SourceKind) -> u64 {
        match kind {
            SourceKind::ArtifactInventory => self.inventory.leaf_count() as u64,
            SourceKind::Input => match self.prompt_form {
                PalwPromptIdsFormV1::Flat => 1,
                PalwPromptIdsFormV1::MerkleV1 => prompt_ids_tile_count_v1(self.result.prefill as u64).expect("validated prefill"),
            },
        }
    }
    pub fn registered_class(&self) -> &PalwTirClassV1 {
        &self.class
    }
    pub fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }
    pub fn get(&self, kind: SourceKind, index: u64) -> Option<&StoredUnitV2> {
        self.entries.get(&(kind, index))
    }
    pub fn authenticated_entries(&self) -> impl Iterator<Item = (SourceKind, u64, &StoredUnitV2)> {
        self.entries.iter().map(|(&(kind, index), unit)| (kind, index, unit))
    }

    /// Assemble parameters from authenticated G14 inventory openings before
    /// replaying an original with the same registered class and artifact.
    /// The caller owns the reserved temporary container and tape.
    #[allow(clippy::too_many_arguments)]
    pub fn run_public_wide_replay(
        &self,
        provider: &dyn MaterialProvider,
        original: &OriginalCommit,
        manifest: &CheckedManifestV1,
        job: &PalwJobContextV2,
        container: &Path,
        run_limits: TirWideRunLimitsV2,
        work: &WidePanelWorkV2,
        tape: File,
        reserver: &TirMemoryReserverV1,
        acquisition: PublicTraceAcquisitionV1,
        search: Option<PublicTraceSearchV1>,
    ) -> Result<(), PanelSourceErrorR1> {
        if manifest.original() != original.id()
            || original.job.0 != job.job_id.as_bytes()
            || original.registered_class.0 != job.shape_profile_id.as_bytes()
            || original.registered_class != self.result.registered_class
            || original.artifact != self.result.artifact
            || original.program != self.result.program
        {
            return Err(PanelSourceErrorR1::Binding);
        }
        check_public_original_execution_v1(original, job, &self.class, Hash64::from_bytes(self.result.artifact.0))?;
        let space = PalwTirStepSpaceV2::new(&self.class).map_err(|error| PanelSourceErrorR1::ReplayExecution(error.to_string()))?;
        let artifact = self.assemble_artifact(container)?;
        run_public_wide_replay_v1(
            provider,
            original,
            manifest,
            job,
            &self.class,
            &artifact,
            &space,
            run_limits,
            work,
            tape,
            reserver,
            acquisition,
            search,
        )
    }

    fn source(&self, kind: SourceKind) -> misaka_palw_positive_contract::identity::Digest {
        match kind {
            SourceKind::ArtifactInventory => self.result.artifact,
            SourceKind::Input => self.result.input,
        }
    }

    /// Source requests are canonical half-open G14 unit ranges. Only an A4 duty
    /// can authorize them; this local plan is not an obligation or reservation.
    pub fn next_missing_request(&self, kind: SourceKind, max_units: u64) -> Result<Option<SourceRequest>, PanelSourceErrorR1> {
        if max_units == 0 || max_units > self.limits.max_units {
            return Err(PanelSourceErrorR1::Budget);
        }
        let count = self.units(kind);
        let mut first = 0;
        for (&(_, index), _) in self.entries.range((kind, 0)..(kind, count)) {
            if index != first {
                break;
            }
            first += 1;
        }
        if first == count {
            return Ok(None);
        }
        let cap = first.saturating_add(max_units).min(count);
        let end = self.entries.range((kind, first)..(kind, cap)).next().map_or(cap, |(&(_, index), _)| index);
        Ok(Some(SourceRequest {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: self.result.case_id(),
            contract: self.result.service_contract,
            source: self.source(kind),
            kind,
            first,
            end,
        }))
    }

    fn authenticate(&self, kind: SourceKind, index: u64, unit: &StoredUnitV2) -> Result<u64, PanelSourceErrorR1> {
        if index >= self.units(kind) {
            return Err(PanelSourceErrorR1::Binding);
        }
        // Bound every variable field before serialization or hashing. Stored
        // allocations are normalized by cloning only their visible contents.
        let bounded = match unit {
            StoredUnitV2::Artifact(opening) => {
                opening.operand.bytes.len() as u64 <= self.limits.max_unit_bytes
                    && opening.operand.tensor_name.len() as u64 <= self.limits.max_unit_bytes
                    && opening.path.len() <= 32
            }
            StoredUnitV2::Input(opening) => {
                opening.tile_ids.len() <= PALW_PROMPT_IDS_TILE_LEN as usize && opening.opening.siblings.len() <= 32
            }
            StoredUnitV2::FlatInput(ids) => (ids.len() as u64).checked_mul(4).is_some_and(|n| n <= self.limits.max_unit_bytes),
        };
        if !bounded {
            return Err(PanelSourceErrorR1::Budget);
        }
        let bytes = match unit {
            StoredUnitV2::Artifact(opening) => borsh::object_length(opening),
            StoredUnitV2::Input(opening) => borsh::object_length(opening),
            StoredUnitV2::FlatInput(ids) => borsh::object_length(ids),
        }
        .map_err(|_| PanelSourceErrorR1::Binding)? as u64;
        if bytes > self.limits.max_unit_bytes {
            return Err(PanelSourceErrorR1::Budget);
        }
        let valid = match (kind, unit) {
            (SourceKind::ArtifactInventory, StoredUnitV2::Artifact(opening)) => {
                let (param, layer, start, len) = self
                    .inventory
                    .piece_of(u32::try_from(index).map_err(|_| PanelSourceErrorR1::Binding)?)
                    .ok_or(PanelSourceErrorR1::Binding)?;
                opening.leaf_index as u64 == index
                    && opening.leaf_count == self.inventory.leaf_count()
                    && opening.operand.tensor_name == self.program.params[param as usize].name
                    && opening.operand.layer == layer
                    && opening.operand.row_start == start
                    && opening.operand.bytes.len() == len as usize
                    && verify_artifact_opening_v1(opening, Hash64::from_bytes(self.result.artifact.0)).is_ok()
            }
            (SourceKind::Input, StoredUnitV2::Input(opening)) if self.prompt_form == PalwPromptIdsFormV1::MerkleV1 => {
                opening.tile_index as u64 == index
                    && verify_prompt_ids_opening_v1(&Hash64::from_bytes(self.result.input.0), self.result.prefill, opening).is_ok()
                    && opening.tile_ids.iter().all(|id| *id < self.program.token_bound)
            }
            (SourceKind::Input, StoredUnitV2::FlatInput(ids)) if self.prompt_form == PalwPromptIdsFormV1::Flat => {
                index == 0
                    && ids.len() == self.result.prefill as usize
                    && ids.iter().all(|id| *id < self.program.token_bound)
                    && prompt_token_ids_match_v1(self.prompt_form, ids, &Hash64::from_bytes(self.result.input.0))
            }
            _ => false,
        };
        if !valid {
            return Err(PanelSourceErrorR1::Binding);
        }
        bytes.checked_add(256).ok_or(PanelSourceErrorR1::Budget)
    }

    pub fn import_authenticated(&mut self, kind: SourceKind, index: u64, unit: StoredUnitV2) -> Result<(), PanelSourceErrorR1> {
        let bytes = self.authenticate(kind, index, &unit)?;
        if self.entries.contains_key(&(kind, index)) {
            return Ok(());
        }
        let retained =
            self.retained_bytes.checked_add(bytes).filter(|n| *n <= self.limits.max_total_bytes).ok_or(PanelSourceErrorR1::Budget)?;
        if self.entries.len() as u64 >= self.limits.max_units {
            return Err(PanelSourceErrorR1::Budget);
        }
        // Clone copies visible elements into fresh allocations; incoming spare
        // capacity from a local takeover never becomes retained unmetered RAM.
        self.entries.insert((kind, index), unit.clone());
        self.retained_bytes = retained;
        Ok(())
    }

    /// Uses the actual A2 responder and its provider fallback. There is no local
    /// ledger or copied quota, and no Panel ACK can complete the native duty.
    /// A4 capability authorization is re-read after the responder's I/O.
    pub fn acquire<S, D>(&mut self, source: &mut S, duty: &D) -> Result<u64, PanelSourceErrorR1>
    where
        D: CanonicalDataDutyV2,
        S: MinerSource<AuthenticatedDuty = D, G14Response = PreparedDataV2, Error = MinerErrorV2>,
    {
        let request = *duty.request();
        let before = duty.authorize(&self.result)?;
        self.contract.validate(&self.result, before.now)?;
        request.validate(self.units(request.kind))?;
        if request.case != self.result.case_id()
            || request.contract != self.result.service_contract
            || request.source != self.source(request.kind)
            || before.source_units != self.units(request.kind)
            || before.now >= before.cutoff
            || before.cutoff > self.contract.deadline
            || request.end - request.first > self.limits.max_units
        {
            return Err(PanelSourceErrorR1::Binding);
        }
        // Reserve the conservative local response ceiling before asking A2
        // to read anything. This is a memory bound, never a new service quota.
        let cached = self.entries.range((request.kind, request.first)..(request.kind, request.end)).count() as u64;
        let fresh = (request.end - request.first).checked_sub(cached).ok_or(PanelSourceErrorR1::Binding)?;
        if before.max_unit_bytes == 0
            || before.max_unit_bytes > self.limits.max_unit_bytes
            || fresh
                .checked_mul(before.max_unit_bytes.checked_add(256).ok_or(PanelSourceErrorR1::Budget)?)
                .and_then(|bytes| self.retained_bytes.checked_add(bytes))
                .filter(|bytes| *bytes <= self.limits.max_total_bytes)
                .is_none()
            || (self.entries.len() as u64).checked_add(fresh).filter(|units| *units <= self.limits.max_units).is_none()
        {
            return Err(PanelSourceErrorR1::Budget);
        }
        let response = source.respond(duty)?;
        let after = duty.authorize(&self.result)?;
        self.contract.validate(&self.result, after.now)?;
        if *duty.request() != request
            || response.request != request
            || after.source_units != before.source_units
            || after.now < before.now
            || after.now >= after.cutoff
            || after.cutoff > self.contract.deadline
        {
            return Err(PanelSourceErrorR1::Binding);
        }
        if response.units.len() as u64 > request.end - request.first {
            return Err(PanelSourceErrorR1::Budget);
        }
        let mut previous = None;
        let mut additional = 0u64;
        let mut additional_units = 0u64;
        for (index, unit) in &response.units {
            if *index < request.first || *index >= request.end || previous.is_some_and(|old| old >= *index) {
                return Err(PanelSourceErrorR1::Binding);
            }
            previous = Some(*index);
            let bytes = self.authenticate(request.kind, *index, unit)?;
            if !self.entries.contains_key(&(request.kind, *index)) {
                additional = additional.checked_add(bytes).ok_or(PanelSourceErrorR1::Budget)?;
                additional_units += 1;
            }
        }
        if self.retained_bytes.checked_add(additional).filter(|n| *n <= self.limits.max_total_bytes).is_none()
            || (self.entries.len() as u64).checked_add(additional_units).filter(|n| *n <= self.limits.max_units).is_none()
        {
            return Err(PanelSourceErrorR1::Budget);
        }
        let count = response.units.len() as u64;
        for (index, unit) in response.units {
            self.import_authenticated(request.kind, index, unit)?;
        }
        Ok(count)
    }

    pub fn prompt(&self) -> Result<Vec<u32>, PanelSourceErrorR1> {
        // Missing input is common during acquisition. Check completeness before
        // reserving the entire prefill buffer, even for a large admitted prompt.
        let expected = self.units(SourceKind::Input);
        let present = self.entries.range((SourceKind::Input, 0)..(SourceKind::Input, expected)).count() as u64;
        if present != expected {
            return Err(PanelSourceErrorR1::Incomplete);
        }
        let mut ids = Vec::with_capacity(self.result.prefill as usize);
        for index in 0..self.units(SourceKind::Input) {
            match self.entries.get(&(SourceKind::Input, index)).ok_or(PanelSourceErrorR1::Incomplete)? {
                StoredUnitV2::Input(opening) => ids.extend_from_slice(&opening.tile_ids),
                StoredUnitV2::FlatInput(prompt) => ids.extend_from_slice(prompt),
                _ => return Err(PanelSourceErrorR1::Binding),
            }
        }
        if ids.len() != self.result.prefill as usize
            || !prompt_token_ids_match_v1(self.prompt_form, &ids, &Hash64::from_bytes(self.result.input.0))
        {
            return Err(PanelSourceErrorR1::Binding);
        }
        Ok(ids)
    }

    /// Rebuild the existing PALWTIR1 container from authenticated inventory
    /// operands. Use a temporary destination: write/open failure can leave a
    /// partial file. Publish atomically only after success. Returned artifact's
    /// actual root and class have been checked against registered admission.
    pub fn assemble_artifact(&self, temporary_path: &Path) -> Result<TirArtifactV1, PanelSourceErrorR1> {
        let mut data_bytes = 0u64;
        for index in 0..self.units(SourceKind::ArtifactInventory) {
            let unit = self.entries.get(&(SourceKind::ArtifactInventory, index)).ok_or(PanelSourceErrorR1::Incomplete)?;
            let StoredUnitV2::Artifact(opening) = unit else {
                return Err(PanelSourceErrorR1::Binding);
            };
            data_bytes = data_bytes.checked_add(opening.operand.bytes.len() as u64).ok_or(PanelSourceErrorR1::Budget)?;
        }
        write_bounded_artifact_container_v1(
            &self.program,
            &self.class,
            temporary_path,
            data_bytes,
            self.limits.max_container_bytes,
            &mut |param, layer, out| {
                let size = tensor_bytes_v1(&self.program, param);
                let mut at = 0u64;
                while at < size {
                    let index = self.inventory.leaf_of(param, layer, at).ok_or("missing inventory coordinate")?;
                    let Some(StoredUnitV2::Artifact(opening)) = self.entries.get(&(SourceKind::ArtifactInventory, index as u64))
                    else {
                        return Err("missing authenticated tensor piece".into());
                    };
                    if opening.operand.row_start as u64 != at {
                        return Err("non-contiguous tensor piece".into());
                    }
                    out.write_all(&opening.operand.bytes).map_err(|error| error.to_string())?;
                    at += opening.operand.bytes.len() as u64;
                }
                Ok(())
            },
        )?;
        let artifact = TirArtifactV1::open(temporary_path).map_err(PanelSourceErrorR1::Artifact)?;
        if artifact.class().map_err(PanelSourceErrorR1::Artifact)? != self.class
            || artifact.inventory_root().map_err(PanelSourceErrorR1::Artifact)?.0.as_bytes() != self.result.artifact.0
        {
            return Err(PanelSourceErrorR1::Binding);
        }
        Ok(artifact)
    }
}

fn write_bounded_artifact_container_v1(
    program: &TirProgramV1,
    class: &PalwTirClassV1,
    path: &Path,
    data_bytes: u64,
    max_container_bytes: u64,
    tensor: &mut dyn FnMut(u16, Option<u16>, &mut dyn std::io::Write) -> Result<(), String>,
) -> Result<(), PanelSourceErrorR1> {
    let instances = param_instances_v1(program);
    let tensors = instances
        .iter()
        .enumerate()
        .flat_map(|(param, layers)| {
            layers.iter().map(move |layer| PalwTirTensorEntryV1 {
                param: param as u16,
                layer: *layer,
                offset: 0,
                bytes: tensor_bytes_v1(program, param as u16),
            })
        })
        .collect::<Vec<_>>();
    let header = PalwTirContainerHeaderV1 {
        version: 1,
        program: class.program.clone(),
        layout: borsh::to_vec(&class.layout).map_err(|_| PanelSourceErrorR1::Binding)?,
        tokenizer_id: class.tokenizer_id.as_bytes(),
        tensors,
        meta: String::new(),
    };
    let header_bytes = borsh::object_length(&header).map_err(|_| PanelSourceErrorR1::Binding)? as u64;
    let _total_bound = data_bytes
        .checked_add(header_bytes)
        .and_then(|n| n.checked_add(14))
        .and_then(|n| (header.tensors.len() as u64).checked_mul(63).and_then(|padding| n.checked_add(padding)))
        .filter(|n| *n <= max_container_bytes)
        .ok_or(PanelSourceErrorR1::Budget)?;
    write_container_v1_streamed(path, program, header.layout, header.tokenizer_id, String::new(), tensor)
        .map_err(|error| PanelSourceErrorR1::Artifact(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_metadata_expansion_is_bounded_before_allocating_instances() {
        assert_eq!(guard_inventory_metadata([true, false].into_iter(), 3, 4), Ok(()));
        assert_eq!(guard_inventory_metadata([true, true].into_iter(), 3, 4), Err(PanelSourceErrorR1::Budget));
        assert_eq!(guard_inventory_metadata([true].into_iter(), usize::MAX, 16), Err(PanelSourceErrorR1::Budget));
        #[cfg(target_pointer_width = "64")]
        assert_eq!(guard_inventory_metadata([true, true].into_iter(), usize::MAX, u64::MAX), Err(PanelSourceErrorR1::Budget));
    }
}
