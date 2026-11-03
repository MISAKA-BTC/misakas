//! Local diagnostic for one original T12 A16 embedding or query projection tile.
//! The Panel command opens the mapped artifact. The verifier has no artifact argument.
use kaspa_consensus_core::{
    Hash64,
    config::{
        class_manifest_const_v1::{QWEN25_A16_8K_MANIFEST_V1, artifact_digest_of, class_id_of_class, inventory_root_of_class},
        params::palw_t12_shipped_params,
    },
    hashing::header::{hash as header_hash, pre_pow_hash_64},
    header::Header,
    palw_artifact::{
        PalwArtifactMultiproofStreamV1, PalwArtifactMultiproofV1, PalwArtifactOpeningV1, PalwProvenOperandsV1, artifact_leaf_parts_v1,
        verify_artifact_opening_v1,
    },
    palw_attempt_rules_v1::{
        PalwAttemptRulesV1, palw_attempt_canonical_v1, palw_attempt_job_for_anchor_v1, palw_attempt_output_root_v1,
        palw_attempt_rules_of_params_v1,
    },
    palw_attempt_v2::{
        PALW_ATTEMPT_V2_TRACE_CHUNKS, PalwAttemptEnvelopeV2, attempt_id_v2, attempt_trace_manifest_root_v1, palw_attempt_job_v1,
        palw_job_anchor_v1, palw_network_domain_v2_for, palw_nonce_bucket_v1,
    },
    palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1},
    palw_base0_a16::A16QuantParams,
    palw_native_resource_v1::preflight_native_value_v1,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, palw_prompt_ids_form_of_class_v1},
    palw_shard_court_v1::{PalwLeafEvidenceV1, PalwOneMoveClaimV2, PalwShardCourtError, PalwShardCourtVerdictV1},
    palw_step::{
        PalwStepCoordinateV1, PalwStepOutLenV1, canonical_step_coordinates, canonical_step_leaf_index, kernel_semantics_id_v1,
        step_leaf_count_capped_v1,
    },
    palw_step_leg::{
        PalwStepEvidenceV1, PalwStepFaultV1, PalwStepLegError, PalwStepOpeningV1, PalwStepRangeOpeningV1, PalwStepRefutationV1,
        binding_commitment_root_v1, check_step_refutation_capped_v1, step_merkle_leaf_v1, step_merkle_node_v1,
        step_opening_root_capped_v1, step_range_opening_root_capped_v1, step_tile_leaf_hash_ctx_v1,
    },
    palw_step_refute::{
        KDESC_A16_EMBED, KDESC_A16_MATMUL_REQUANT, PalwStepRefuteError, canonical_input_leaves_v1,
        check_execution_step_refutation_opened_capped_v1, palw_refutation_prompt_carriage_v1,
    },
    palw_v2::PalwJobContextV2,
    pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
};
use kaspa_rpc_core::RpcHeader;
use misaka_palw_base0::{
    artifact::{Base0ArtifactV1, decode_artifact_file_mapped_v1},
    classes::{A16_GRAPH_V7_8K_MODEL_ID, CanonicalClassV1, a16_graph_v7_row_v1},
    fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
    inventory::a16_readiness_leaves_streamed_v1,
    mmap::ReadOnlyMap,
    produce::{
        Base0FpMaterialV2, Base0SeatFamilyV1, PALW_BASE0_FP_MATERIAL_MAGIC_V2, base0_fp_material_decode_v2,
        base0_fp_material_matches_claim_v2,
    },
    qwen25_a16_backend::Qwen25A16Backend,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

const MAX_OBSERVATION: usize = 128 << 10;
const MAX_CAPTURE: usize = 4 << 20;
const MAX_EVIDENCE: usize = 256 << 10;
const MAX_CAPTURE_HEAP: u64 = 32 << 20;
const MAX_EVIDENCE_HEAP: u64 = 2 << 20;
const MAX_ARTIFACT: u64 = 4 << 30;
const MAX_INVENTORY_LEAVES: u32 = 4_000_000;
const MAX_STEP_LEAVES: u64 = 1 << 28;
const EMBEDDING_LEAF: u64 = 0;
const QUERY_WEIGHTS: &str = "blk.{layer}.attn_q.weight";
const QUERY_QUANT: &str = "blk.{layer}.attn_q.weight.a16";

#[derive(Clone, Copy)]
enum Relation {
    Embedding,
    QueryProjection,
}

struct OperandKey<'a> {
    name: &'a str,
    layer: Option<u16>,
    offset: u32,
    width: u32,
}

struct Target<'a> {
    relation: Relation,
    leaf: u64,
    inputs: usize,
    output_lanes: u32,
    operands: Vec<OperandKey<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    consensus_params_id: String,
    consensus_schedule_id: String,
    genesis_hash: Hash64,
    block_hash: Hash64,
    claim_id: Hash64,
    material_bytes: usize,
    material_sha256: String,
    header: RpcHeader,
    registered_executor_bond: String,
    registered_executor_pubkey: String,
}

struct RootedClaim {
    observation: Observation,
    class: CanonicalClassV1,
    roots: PalwClaimRootsV1,
    expected: PalwOneMoveClaimV2,
    job: PalwJobContextV2,
    prompt: Vec<u32>,
    form: PalwPromptIdsFormV1,
    step_leaves: u64,
    daa: u64,
}

fn require(ok: bool, reason: &'static str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(reason.into()) }
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(path).map_err(|e| e.to_string())?;
    let stat = file.metadata().map_err(|e| e.to_string())?;
    require(stat.is_file() && stat.len() <= limit as u64, "input exceeds regular-file byte cap")?;
    let mut bytes = Vec::with_capacity(stat.len() as usize);
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    require(bytes.len() as u64 == stat.len(), "input length changed")?;
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn rooted_claim(input: &Path, expected_sha: &str) -> Result<RootedClaim, String> {
    let bytes = read_bounded(input, MAX_OBSERVATION)?;
    require(sha256(&bytes) == expected_sha, "observation differs from independently supplied SHA-256")?;
    let observation: Observation = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let params = palw_t12_shipped_params();
    require(
        observation.consensus_params_id == params.consensus_params_id().to_string()
            && observation.consensus_schedule_id == params.consensus_schedule_id().to_string()
            && observation.genesis_hash == params.genesis.hash
            && palw_attempt_rules_of_params_v1(&params) == PalwAttemptRulesV1::CoreV1,
        "observation is outside the compiled T12 CoreV1 scope",
    )?;
    let header = Header::try_from(&observation.header).map_err(|e| e.to_string())?;
    require(
        header.hash == observation.block_hash
            && header_hash(&header) == observation.block_hash
            && header.pow_algo_id == POW_ALGO_ID_PALW_COMMITTED_V2
            && header.palw_commitment.len() <= 16 << 10,
        "block identity or bounded PAV2 header differs",
    )?;
    let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).map_err(|e| format!("{e:?}"))?;
    let pre_pow = pre_pow_hash_64(&header);
    let domain = palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash));
    envelope
        .validate_stateless_v2_at_version(
            params.palw_attempt_lane_at(header.daa_score).attempt_version(),
            domain,
            pre_pow,
            header.timestamp,
            header.nonce,
        )
        .map_err(|e| format!("{e:?}"))?;
    envelope
        .validate_signature_v2(|key, message, sig, context| {
            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
        })
        .map_err(|e| format!("{e:?}"))?;
    let attempt = &envelope.attempt;
    let bond = format!("{}:{}", attempt.executor_bond.transaction_id, attempt.executor_bond.index);
    let pubkey = attempt.executor_pubkey.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write;
        write!(output, "{byte:02x}").expect("writing a String");
        output
    });
    require(
        attempt_id_v2(attempt) == observation.claim_id
            && observation.registered_executor_bond == bond
            && observation.registered_executor_pubkey == pubkey
            && attempt.trace_chunk_count == PALW_ATTEMPT_V2_TRACE_CHUNKS
            && attempt.trace_manifest_root == attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count),
        "signed claim, registered bond/key or trace manifest differs",
    )?;
    let class = a16_graph_v7_row_v1(8192, A16_GRAPH_V7_8K_MODEL_ID).ok_or("compiled T12 Qwen class is unavailable")?;
    require(
        attempt.class_id == class_id_of_class(QWEN25_A16_8K_MANIFEST_V1, 1)
            && attempt.class_id == class.class_id()
            && attempt.artifact_root == inventory_root_of_class(QWEN25_A16_8K_MANIFEST_V1, 1)
            && palw_attempt_canonical_v1(&class.profile, false) == Some(class.canonical_job),
        "signed class or registered inventory differs",
    )?;
    let anchor = palw_job_anchor_v1(domain, pre_pow, attempt.class_id, &attempt.executor_bond, palw_nonce_bucket_v1(header.nonce));
    require(anchor != Hash64::default() && params.palw_prefill_draw_active_at(header.daa_score), "expected a rooted prefill draw")?;
    let network_form = params.palw_prompt_ids_form_at(header.daa_score);
    let (job, prompt) = palw_attempt_job_for_anchor_v1(&class.profile, &anchor, class.canonical_job, network_form)
        .ok_or("the signed header's job cannot be derived")?;
    let job = palw_attempt_job_v1(job, true);
    require(prompt.len() == 1023 && job.exact_decode_tokens == 1 && job.max_context_tokens == 8192, "expected exact 1023/1 draw")?;
    let step_leaves = step_leaf_count_capped_v1(&class.profile, &job, MAX_STEP_LEAVES).map_err(|e| format!("{e:?}"))?;
    let roots = PalwClaimRootsV1 {
        execution_root: attempt.execution_root,
        trace_root: attempt.trace_root,
        anchor,
        attempt_draw: Some(true),
        output_root: Some(attempt.output_root),
        job_pin: None,
    };
    let expected = PalwOneMoveClaimV2 {
        execution_root: attempt.execution_root,
        class_id: attempt.class_id,
        artifact_root: attempt.artifact_root,
    };
    let form = palw_prompt_ids_form_of_class_v1(network_form, &class.profile);
    Ok(RootedClaim {
        observation,
        class,
        roots,
        expected,
        job,
        prompt: prompt.into_iter().map(|id| id as u32).collect(),
        form,
        step_leaves,
        daa: header.daa_score,
    })
}

fn target(claim: &RootedClaim, relation: Relation) -> Result<Target<'_>, String> {
    let profile = &claim.class.profile;
    let (leaf, coord, kernel) = match relation {
        Relation::Embedding => (
            EMBEDDING_LEAF,
            canonical_step_coordinates(profile, &claim.job, EMBEDDING_LEAF).ok_or("first leaf has no coordinates")?,
            KDESC_A16_EMBED,
        ),
        Relation::QueryProjection => {
            let coord = PalwStepCoordinateV1 {
                call_index: 0,
                position: 0,
                tile_index: 0,
                node_slot: u32::try_from(profile.pre_nodes.len()).map_err(|e| e.to_string())? + 2,
            };
            let leaf = canonical_step_leaf_index(profile, &claim.job, &coord).ok_or("first projection leaf is absent")?;
            (leaf, coord, KDESC_A16_MATMUL_REQUANT)
        }
    };
    let (node, layer) = profile.resolve_node_slot(coord.node_slot).ok_or("target leaf has no node")?;
    let PalwStepOutLenV1::Fixed { elements } = node.out_len else {
        return Err("target row width is not fixed".into());
    };
    require(
        coord.position == 0
            && coord.call_index == 0
            && coord.tile_index == 0
            && node.kernel_semantics_id == kernel_semantics_id_v1(kernel)
            && elements > 0
            && elements <= 4096,
        "target differs from the fixed A16 diagnostic relation",
    )?;
    let output_lanes = node.tile_len.min(elements);
    let (inputs, operands) = match relation {
        Relation::Embedding => {
            require(node.input_refs.is_empty(), "embedding must have no committed input row")?;
            let offset = claim.prompt[0].checked_mul(elements).ok_or("embedding byte offset overflow")?;
            (0, vec![OperandKey { name: &node.weight_name, layer, offset, width: elements }])
        }
        Relation::QueryProjection => {
            require(
                layer == Some(0)
                    && node.weight_name == QUERY_WEIGHTS
                    && elements == profile.hidden_dim
                    && output_lanes > 0
                    && output_lanes <= 128,
                "target differs from the first query tile",
            )?;
            let rows = canonical_input_leaves_v1(profile, &claim.job, &coord).ok_or("projection has no canonical input set")?;
            let [row] = rows.as_slice() else { return Err("projection must read one committed activation row".into()) };
            require(
                leaf < 1 << PALW_BASE0_SPARSE_RETAIN_LEVEL_V1
                    && !row.is_empty()
                    && row.len() <= 32
                    && row.iter().all(|(input, coord)| *input < leaf && coord.position == 0 && coord.call_index == 0),
                "projection inputs are outside the first bounded replay block",
            )?;
            let code_bytes = output_lanes.checked_mul(profile.hidden_dim).ok_or("query weight tile width overflow")?;
            require(code_bytes as usize <= 192 << 10, "query weight tile exceeds diagnostic cap")?;
            let quant_bytes = output_lanes * A16QuantParams::WIRE_BYTES as u32;
            (
                row.len(),
                vec![
                    OperandKey { name: QUERY_WEIGHTS, layer, offset: 0, width: code_bytes },
                    OperandKey { name: QUERY_QUANT, layer, offset: 0, width: quant_bytes },
                ],
            )
        }
    };
    Ok(Target { relation, leaf, inputs, output_lanes, operands })
}

fn operand_digests(target: &Target<'_>, artifact: &Base0ArtifactV1) -> Result<Vec<Hash64>, String> {
    let bytes: Vec<Vec<u8>> = match target.relation {
        Relation::Embedding => {
            let key = &target.operands[0];
            let end = key.offset.checked_add(key.width).ok_or("embedding row end overflow")?;
            let row = artifact.embed.get(key.offset as usize..end as usize).ok_or("embedding row outside mapped artifact")?;
            vec![row.iter().map(|byte| *byte as u8).collect()]
        }
        Relation::QueryProjection => {
            let layer = artifact.layers.first().ok_or("first artifact layer is missing")?;
            let codes = layer.wq.as_slice().get(..target.operands[0].width as usize).ok_or("first query weight tile is missing")?;
            let quant = artifact.a16_param(QUERY_QUANT, Some(0)).ok_or("first query quantization table is missing")?;
            let quant_width = target.operands[1].width as usize;
            // The inventory expands a shared 17-byte triple into a per-lane table.
            let quant = if quant.len() == A16QuantParams::WIRE_BYTES {
                quant.repeat(target.output_lanes as usize)
            } else {
                quant.get(..quant_width).ok_or("first query quantization tile is missing")?.to_vec()
            };
            vec![codes.iter().map(|byte| *byte as u8).collect(), quant]
        }
    };
    Ok(target.operands.iter().zip(bytes).map(|(key, row)| artifact_leaf_parts_v1(key.name, key.layer, key.offset, &row)).collect())
}

fn open_operands(claim: &RootedClaim, target: &Target<'_>, artifact: &Base0ArtifactV1) -> Result<Vec<PalwArtifactOpeningV1>, String> {
    let digests = operand_digests(target, artifact)?;
    let mut indices = vec![None; digests.len()];
    let mut duplicate = false;
    let mut visited = 0u64;
    let (root, count, _) = a16_readiness_leaves_streamed_v1(artifact, &claim.class.profile, &[], &mut |hash| {
        for (digest, index) in digests.iter().zip(&mut indices) {
            if hash == *digest {
                duplicate |= index.replace(visited).is_some();
            }
        }
        visited += 1;
    })
    .map_err(|e| format!("{e:?}"))?;
    require(
        root == claim.expected.artifact_root && count <= MAX_INVENTORY_LEAVES && visited == u64::from(count) && !duplicate,
        "mapped artifact inventory root or leaf cap differs",
    )?;
    let indices = indices
        .into_iter()
        .map(|index| u32::try_from(index.ok_or("required row is absent from registered inventory")?).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, String>>()?;
    let mut proofs = indices
        .iter()
        .map(|&index| PalwArtifactMultiproofStreamV1::new(count, &[index]).ok_or("invalid artifact inventory index"))
        .collect::<Result<Vec<_>, _>>()?;
    let (root, _, opened) = a16_readiness_leaves_streamed_v1(artifact, &claim.class.profile, &indices, &mut |hash| {
        proofs.iter_mut().for_each(|proof| proof.push(hash));
    })
    .map_err(|e| format!("{e:?}"))?;
    require(root == claim.expected.artifact_root, "mapped inventory changed during the opening pass")?;
    proofs
        .into_iter()
        .zip(opened)
        .map(|(proof, operand)| {
            one_opening(proof.finish(&[operand]).ok_or("incomplete artifact opening")?, claim.expected.artifact_root)
        })
        .collect()
}

fn one_opening(mut proof: PalwArtifactMultiproofV1, registered_root: Hash64) -> Result<PalwArtifactOpeningV1, String> {
    require(proof.opened.len() == 1, "expected one opened artifact row")?;
    let (leaf_index, operand) = proof.opened.pop().ok_or("missing artifact opening")?;
    let opening = PalwArtifactOpeningV1 { leaf_index, leaf_count: proof.leaf_count, operand, path: proof.siblings };
    verify_artifact_opening_v1(&opening, registered_root).map_err(|e| format!("{e:?}"))?;
    Ok(opening)
}

fn build(claim: &RootedClaim, target: &Target<'_>, capture: &Path, path: &Path) -> Result<PalwLeafEvidenceV1, String> {
    let bytes = read_bounded(capture, MAX_CAPTURE)?;
    require(
        bytes.len() == claim.observation.material_bytes && sha256(&bytes) == claim.observation.material_sha256,
        "folded capture differs from the independent observation",
    )?;
    let body = bytes.strip_prefix(&PALW_BASE0_FP_MATERIAL_MAGIC_V2).ok_or("wrong folded capture magic")?;
    let quote = preflight_native_value_v1::<Base0FpMaterialV2>(body, body.len() as u64).map_err(|e| format!("{e:?}"))?;
    require(quote.logical_requests_v1() <= MAX_CAPTURE_HEAP, "capture allocation quote exceeds cap")?;
    let folded = base0_fp_material_decode_v2(&bytes).map_err(|e| format!("{e:?}"))?;
    require(
        folded.binding.job_context == claim.job
            && folded.binding.shape_profile == claim.class.profile
            && folded.binding.step_leaf_count == claim.step_leaves
            && folded.prompt_token_ids == claim.prompt
            && folded.step_tree.retain_level() == PALW_BASE0_SPARSE_RETAIN_LEVEL_V1
            && folded.generated_token_ids.len() == 1
            && folded.generated_token_ids[0] < claim.class.profile.vocab_size
            && Some(palw_attempt_output_root_v1(&claim.job, &folded.generated_token_ids)) == claim.roots.output_root
            && base0_fp_material_matches_claim_v2(
                &folded,
                claim.roots.execution_root,
                claim.roots.trace_root,
                Base0SeatFamilyV1::IntegerKv,
            )
            .map_err(|e| format!("{e:?}"))?,
        "capture does not reproduce the original signed claim or first-block cap",
    )?;
    drop(folded);
    let stat = path.symlink_metadata().map_err(|e| e.to_string())?;
    require(stat.is_file() && stat.len() <= MAX_ARTIFACT, "artifact must be a bounded regular file")?;
    let map = Arc::new(ReadOnlyMap::open(path).map_err(|e| e.to_string())?);
    require(map.len() as u64 == stat.len(), "artifact length changed")?;
    let artifact = Arc::new(decode_artifact_file_mapped_v1(map).map_err(|e| format!("{e:?}"))?);
    require(
        artifact.shape == claim.class.artifact_shape && artifact.artifact_digest() == artifact_digest_of(QWEN25_A16_8K_MANIFEST_V1),
        "original T12 8K artifact digest or shape differs",
    )?;
    let artifact_openings = open_operands(claim, target, &artifact)?;
    let backend = Qwen25A16Backend::from_registered_profile_in_lane_v1(
        artifact,
        claim.job.network_id.clone(),
        claim.class.profile.clone(),
        claim.class.canonical_job,
        claim.class.source,
    )?
    .with_prompt_ids_form(claim.form)
    .with_attempt_rules(PalwAttemptRulesV1::CoreV1)
    .with_step_ladder_cap(MAX_STEP_LEAVES);
    // Use only the bounded folded replay. The generic evidence helper can fall back to dense replay.
    let refutation = backend.fp_leaf_refutation_v1(&bytes, &claim.prompt, claim.roots, claim.step_leaves, target.leaf)?;
    let (refutation, prompt_ids_opening) = palw_refutation_prompt_carriage_v1(claim.form, refutation).map_err(|e| format!("{e:?}"))?;
    Ok(PalwLeafEvidenceV1 { refutation, artifact_openings, prompt_ids_opening })
}

fn verdict(claim: &RootedClaim, target: &Target<'_>, evidence: &PalwLeafEvidenceV1) -> Result<PalwShardCourtVerdictV1, String> {
    let binding = &evidence.refutation.binding;
    require(
        evidence.leaf_index() == target.leaf
            && binding.job_context == claim.job
            && binding.shape_profile == claim.class.profile
            && binding.step_leaf_count == claim.step_leaves
            && binding.full_logits_trace_root == claim.roots.trace_root
            && evidence.artifact_openings.len() == target.operands.len(),
        "evidence changes the independently pinned leaf, class, original job or trace",
    )?;
    for (key, opening) in target.operands.iter().zip(&evidence.artifact_openings) {
        let operand = &opening.operand;
        require(
            operand.tensor_name == key.name
                && operand.layer == key.layer
                && operand.row_start == key.offset
                && operand.bytes.len() == key.width as usize,
            "evidence substitutes an artifact operand coordinate",
        )?;
    }
    let inputs = &evidence.refutation.inputs;
    require(
        match target.relation {
            Relation::Embedding => inputs.is_empty(),
            Relation::QueryProjection => inputs.len() == 1 && inputs[0].preimages.len() == target.inputs,
        },
        "evidence exceeds the bounded canonical input row",
    )?;
    evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true).map_err(|e| format!("{e:?}"))
}

fn read_evidence(path: &Path) -> Result<(Vec<u8>, PalwLeafEvidenceV1), String> {
    let bytes = read_bounded(path, MAX_EVIDENCE)?;
    let quote = preflight_native_value_v1::<PalwLeafEvidenceV1>(bytes.as_slice(), bytes.len() as u64).map_err(|e| format!("{e:?}"))?;
    require(quote.logical_requests_v1() <= MAX_EVIDENCE_HEAP, "evidence allocation quote exceeds cap")?;
    let evidence = borsh::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok((bytes, evidence))
}

fn verify(claim: &RootedClaim, target: &Target<'_>, path: &Path) -> Result<(), String> {
    let (bytes, mut evidence) = read_evidence(path)?;
    require(
        verdict(claim, target, &evidence)? == PalwShardCourtVerdictV1::FalseAccusation,
        "the named original tile did not recompute",
    )?;

    for index in 0..evidence.artifact_openings.len() {
        let mut opening = evidence.artifact_openings.remove(index);
        require(
            matches!(
                evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true),
                Err(PalwShardCourtError::Refutation(PalwStepRefuteError::Unadjudicable | PalwStepRefuteError::WeightUnavailable))
            ),
            "native checker did not defer the withheld operand",
        )?;
        opening.operand.bytes[0] ^= 1;
        evidence.artifact_openings.insert(index, opening);
        require(
            matches!(evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true), Err(PalwShardCourtError::ArtifactOpenings(_))),
            "native checker did not refuse the altered artifact operand",
        )?;
        evidence.artifact_openings[index].operand.bytes[0] ^= 1;
    }
    if matches!(target.relation, Relation::QueryProjection) {
        let input = &mut evidence.refutation.inputs[0].preimages[0].values_le;
        *input.first_mut().ok_or("verified projection input is empty")? ^= 1;
        require(
            matches!(
                evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true),
                Err(PalwShardCourtError::Refutation(PalwStepRefuteError::Leg(PalwStepLegError::CommittedRootMismatch)))
            ),
            "native checker did not refuse the altered committed input tile",
        )?;
    }
    let scope = match target.relation {
        Relation::Embedding => "one-original-t12-a16-embedding-tile",
        Relation::QueryProjection => "one-original-t12-a16-query-projection-tile",
    };
    let mut report = json!({
        "scope": scope,
        "claimId": claim.observation.claim_id,
        "blockHash": claim.observation.block_hash,
        "anchor": claim.roots.anchor,
        "daa": claim.daa,
        "leaf": target.leaf,
        "evidenceBytes": bytes.len(),
        "evidenceSha256": sha256(&bytes),
        "matched": true,
        "missingSourceDeferred": true,
        "wrongSourceRejected": true,
        "fullExecutionVerified": false,
        "chainAcceptanceInferred": false,
        "gateCInferred": false,
    });
    if matches!(target.relation, Relation::QueryProjection) {
        report["artifactOpenings"] = json!(target.operands.len());
        report["authenticatedInputTiles"] = json!(target.inputs);
        report["outputLanes"] = json!(target.output_lanes);
        report["tamperedInputRejected"] = json!(true);
    }
    println!("{report}");
    Ok(())
}

// The projection opens one contiguous activation row before its output. The
// one range sibling containing that output can be updated from its single
// path. All other subtrees, including the activation leaves, stay unchanged.
fn replace_later_range_sibling(
    leaf_count: u64,
    range: &mut PalwStepRangeOpeningV1,
    output: &PalwStepOpeningV1,
    new_leaf_hash: Hash64,
) -> Result<(), String> {
    let mut first = range.first_leaf_index;
    let mut end = first.checked_add(range.leaf_hashes.len() as u64).ok_or("range overflow")?;
    let mut position = output.leaf_index;
    require(first < end && end <= position && position < leaf_count, "expected one earlier nonempty activation range")?;
    let mut before = step_merkle_leaf_v1(position, &output.leaf_hash);
    let mut after = step_merkle_leaf_v1(position, &new_leaf_hash);
    let mut width = leaf_count;
    let mut range_siblings = range.siblings.iter_mut();
    let mut output_siblings = output.siblings.iter();
    let mut changed = false;
    while width > 1 {
        if !first.is_multiple_of(2) {
            range_siblings.next().ok_or("missing activation left sibling")?;
        }
        if !end.is_multiple_of(2) && end < width {
            let right = range_siblings.next().ok_or("missing activation right sibling")?;
            if end == position {
                require(!changed && *right == before, "activation and output paths disagree on their shared subtree")?;
                *right = after;
                changed = true;
            }
        }
        if position.is_multiple_of(2) && position + 1 == width {
            // Odd tail promotion does not consume a sibling or change either digest.
        } else {
            let sibling = output_siblings.next().ok_or("missing output sibling")?;
            (before, after) = if position.is_multiple_of(2) {
                (step_merkle_node_v1(&before, sibling), step_merkle_node_v1(&after, sibling))
            } else {
                (step_merkle_node_v1(sibling, &before), step_merkle_node_v1(sibling, &after))
            };
        }
        first /= 2;
        end = end.div_ceil(2);
        position /= 2;
        width = width.div_ceil(2);
    }
    require(changed && range_siblings.next().is_none() && output_siblings.next().is_none(), "invalid activation or output path extent")
}

fn verify_counterfactual_projection(claim: &RootedClaim, path: &Path) -> Result<(), String> {
    let target = target(claim, Relation::QueryProjection)?;
    let (bytes, mut evidence) = read_evidence(path)?;
    require(verdict(claim, &target, &evidence)? == PalwShardCourtVerdictV1::FalseAccusation, "original projection did not recompute")?;

    let refutation = &mut evidence.refutation;
    let binding = &mut refutation.binding;
    let count = binding.step_leaf_count;
    let profile_hash = binding.shape_profile.shape_profile_id();
    let required = canonical_input_leaves_v1(&binding.shape_profile, &binding.job_context, &refutation.output_preimage.coord)
        .ok_or("projection inputs cannot be derived")?;
    let [required] = required.as_slice() else { return Err("expected a single activation row".into()) };
    require(required.windows(2).all(|p| p[0].0 + 1 == p[1].0), "activation row is not contiguous")?;
    let [input] = refutation.inputs.as_mut_slice() else { return Err("missing activation row".into()) };
    let [siblings] = input.run_siblings.as_mut_slice() else { return Err("expected a single activation range opening".into()) };
    let mut range = PalwStepRangeOpeningV1 {
        first_leaf_index: required.first().ok_or("missing activation index")?.0,
        leaf_hashes: input
            .preimages
            .iter()
            .map(|leaf| step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile_hash, leaf))
            .collect(),
        siblings: siblings.clone(),
    };
    require(
        step_range_opening_root_capped_v1(count, &range, MAX_STEP_LEAVES).map_err(|e| format!("{e:?}"))? == binding.step_merkle_root,
        "activation row does not open against the original signed root",
    )?;
    let lane = refutation.output_preimage.values_le.get_mut(..4).ok_or("projection lane zero is absent")?;
    let old = i32::from_le_bytes(lane.try_into().map_err(|_| "invalid projection lane bytes")?);
    require((-32767..=32767).contains(&old), "original query lane is outside A16 code range")?;
    let changed = if old < 32767 { old + 1 } else { old - 1 };
    lane.copy_from_slice(&changed.to_le_bytes());
    let new_hash = step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile_hash, &refutation.output_preimage);
    replace_later_range_sibling(count, &mut range, &refutation.output_opening, new_hash)?;
    refutation.output_opening.leaf_hash = new_hash;
    let synthetic_step =
        step_opening_root_capped_v1(count, &refutation.output_opening, MAX_STEP_LEAVES).map_err(|e| format!("{e:?}"))?;
    require(
        synthetic_step != binding.step_merkle_root
            && step_range_opening_root_capped_v1(count, &range, MAX_STEP_LEAVES).map_err(|e| format!("{e:?}"))? == synthetic_step,
        "unchanged activation does not open against the synthetic step root",
    )?;
    *siblings = range.siblings;
    binding.step_merkle_root = synthetic_step;
    binding.committed_execution_root = binding_commitment_root_v1(binding);
    let synthetic = PalwOneMoveClaimV2 { execution_root: binding.committed_execution_root, ..claim.expected };
    let structural = PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: PalwStepEvidenceV1::StepTile {
            opening: refutation.output_opening.clone(),
            preimage: refutation.output_preimage.clone(),
        },
    };
    require(
        matches!(check_step_refutation_capped_v1(&structural, MAX_STEP_LEAVES), Err(PalwStepLegError::NoFaultFound)),
        "counterfactual failed the native structural check",
    )?;
    let weights = PalwProvenOperandsV1::from_openings_v1(&evidence.artifact_openings, claim.expected.artifact_root)
        .map_err(|e| format!("{e:?}"))?;
    let arithmetic =
        check_execution_step_refutation_opened_capped_v1(refutation, &weights, evidence.prompt_ids_opening.as_ref(), MAX_STEP_LEAVES)
            .map_err(|e| format!("{e:?}"))?;
    require(
        arithmetic.fault == (PalwStepFaultV1::ComputationMismatch { value_index: 0 }),
        "native arithmetic did not isolate lane zero",
    )?;
    require(
        matches!(
            evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true),
            Err(PalwShardCourtError::NotTheClaimsExecution { .. })
        ),
        "synthetic root was not rejected under the original signed claim",
    )?;
    require(
        matches!(evidence.verdict_at_v2(&synthetic, MAX_STEP_LEAVES, true), Ok(PalwShardCourtVerdictV1::ExecutorGuilty)),
        "bound native court did not detect the counterfactual arithmetic mismatch",
    )?;
    println!(
        "{}",
        json!({
            "scope": "local-unsigned-counterfactual-t12-a16-query-projection",
            "originalClaimId": claim.observation.claim_id,
            "originalEvidenceSha256": sha256(&bytes),
            "originalExecutionRoot": claim.expected.execution_root,
            "syntheticExecutionRoot": synthetic.execution_root,
            "leaf": target.leaf,
            "lane": 0,
            "originalValue": old,
            "counterfactualValue": changed,
            "structuralChecksPassed": true,
            "unchangedActivationAuthenticated": true,
            "nativeArithmeticMismatch": true,
            "originalClaimBindingRejected": true,
            "counterfactualSigned": false,
            "liveCarrierSubmitted": false,
        })
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [mode, input, sha, capture, artifact, output] if mode == "build" || mode == "build-projection" => {
            let claim = rooted_claim(Path::new(input), sha.to_str().ok_or("expected SHA-256 text")?)?;
            let relation = if mode == "build" { Relation::Embedding } else { Relation::QueryProjection };
            let target = target(&claim, relation)?;
            let evidence = build(&claim, &target, Path::new(capture), Path::new(artifact))?;
            require(verdict(&claim, &target, &evidence)? == PalwShardCourtVerdictV1::FalseAccusation, "built tile did not recompute")?;
            let size = borsh::object_length(&evidence).map_err(|e| e.to_string())?;
            require(size <= MAX_EVIDENCE, "encoded evidence exceeds cap")?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(output).map_err(|e| e.to_string())?;
            borsh::to_writer(&mut file, &evidence).map_err(|e| e.to_string())?;
            file.flush().and_then(|()| file.sync_all()).map_err(|e| e.to_string())
        }
        [mode, input, sha, evidence] if mode == "verify" || mode == "verify-projection" => {
            let claim = rooted_claim(Path::new(input), sha.to_str().ok_or("expected SHA-256 text")?)?;
            let relation = if mode == "verify" { Relation::Embedding } else { Relation::QueryProjection };
            verify(&claim, &target(&claim, relation)?, Path::new(evidence))
        }
        [mode, input, sha, evidence] if mode == "verify-counterfactual-projection" => {
            let claim = rooted_claim(Path::new(input), sha.to_str().ok_or("expected SHA-256 text")?)?;
            verify_counterfactual_projection(&claim, Path::new(evidence))
        }
        _ => Err("usage: check-t12-a16-source build|build-projection OBSERVATION OBS_SHA256 CAPTURE ARTIFACT NEW_EVIDENCE | verify|verify-projection|verify-counterfactual-projection OBSERVATION OBS_SHA256 EVIDENCE".into()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("check-t12-a16-source: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1};
    use kaspa_consensus_core::palw_step_leg::{step_merkle_range_siblings_v1, step_merkle_root_v1, step_opening_v1};

    #[test]
    fn changing_one_later_leaf_updates_only_its_range_sibling() {
        let original: Vec<_> = (0..9).map(Hash64::from_u64_word).collect();
        for (first, len, changed) in [(2, 3, 8), (0, 2, 3), (1, 5, 7)] {
            let output = step_opening_v1(&original, changed as u64).unwrap();
            let mut range = PalwStepRangeOpeningV1 {
                first_leaf_index: first as u64,
                leaf_hashes: original[first..first + len].to_vec(),
                siblings: step_merkle_range_siblings_v1(&original, first, len).unwrap(),
            };
            let new_leaf = Hash64::from_u64_word(20);
            let mut new = original.clone();
            new[changed] = new_leaf;
            replace_later_range_sibling(original.len() as u64, &mut range, &output, new_leaf).unwrap();
            assert_eq!(range.siblings, step_merkle_range_siblings_v1(&new, first, len).unwrap());
            assert_eq!(
                step_range_opening_root_capped_v1(original.len() as u64, &range, MAX_STEP_LEAVES).unwrap(),
                step_merkle_root_v1(&new).unwrap()
            );
        }
    }

    #[test]
    fn streamed_single_row_matches_the_native_opening_including_odd_tail() {
        let operands: Vec<_> = (0..5)
            .map(|i| PalwArtifactOperandV1 {
                tensor_name: "token_embd.weight".into(),
                layer: None,
                row_start: i,
                bytes: vec![i as u8],
            })
            .collect();
        let hashes: Vec<_> = operands.iter().map(artifact_leaf_v1).collect();
        let root = artifact_root_v1(&hashes).unwrap();
        for index in [0, 2, 4] {
            let mut stream = PalwArtifactMultiproofStreamV1::new(hashes.len() as u32, &[index]).unwrap();
            hashes.iter().for_each(|&hash| stream.push(hash));
            let proof = stream.finish(&[(index, operands[index as usize].clone())]).unwrap();
            assert!(one_opening(proof.clone(), Hash64::default()).is_err());
            assert_eq!(one_opening(proof, root).unwrap(), open_artifact_leaf_v1(&operands, index).unwrap());
        }
    }
}
