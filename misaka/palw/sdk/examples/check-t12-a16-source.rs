//! Local diagnostic for one original, parameter-dependent T12 A16 embedding tile.
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
        PalwArtifactMultiproofStreamV1, PalwArtifactMultiproofV1, PalwArtifactOpeningV1, artifact_leaf_parts_v1,
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
    palw_native_resource_v1::preflight_native_value_v1,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, palw_prompt_ids_form_of_class_v1},
    palw_shard_court_v1::{PalwLeafEvidenceV1, PalwOneMoveClaimV2, PalwShardCourtError, PalwShardCourtVerdictV1},
    palw_step::{PalwStepOutLenV1, canonical_step_coordinates, kernel_semantics_id_v1, step_leaf_count_capped_v1},
    palw_step_refute::{KDESC_A16_EMBED, PalwStepRefuteError, palw_refutation_prompt_carriage_v1},
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
const LEAF: u64 = 0;

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

fn operand_key(claim: &RootedClaim) -> Result<(&str, Option<u16>, u32, u32), String> {
    let coord = canonical_step_coordinates(&claim.class.profile, &claim.job, LEAF).ok_or("first leaf has no coordinates")?;
    let (node, layer) = claim.class.profile.resolve_node_slot(coord.node_slot).ok_or("first leaf has no node")?;
    require(
        coord.position == 0
            && coord.call_index == 0
            && coord.tile_index == 0
            && node.input_refs.is_empty()
            && node.kernel_semantics_id == kernel_semantics_id_v1(KDESC_A16_EMBED),
        "the original first leaf is not an input-only A16 embedding tile",
    )?;
    let PalwStepOutLenV1::Fixed { elements } = node.out_len else {
        return Err("embedding row width is not fixed".into());
    };
    require(elements > 0 && elements <= 4096, "embedding row width exceeds diagnostic cap")?;
    let offset = claim.prompt[0].checked_mul(elements).ok_or("embedding byte offset overflow")?;
    Ok((&node.weight_name, layer, offset, elements))
}

fn open_operand(claim: &RootedClaim, artifact: &Base0ArtifactV1) -> Result<PalwArtifactOpeningV1, String> {
    let (name, layer, offset, width) = operand_key(claim)?;
    let end = offset.checked_add(width).ok_or("embedding row end overflow")?;
    let row = artifact.embed.get(offset as usize..end as usize).ok_or("embedding row outside mapped artifact")?;
    let row: Vec<u8> = row.iter().map(|byte| *byte as u8).collect();
    let digest = artifact_leaf_parts_v1(name, layer, offset, &row);
    let mut index = None;
    let mut duplicate = false;
    let mut visited = 0u64;
    let (root, count, _) = a16_readiness_leaves_streamed_v1(artifact, &claim.class.profile, &[], &mut |hash| {
        if hash == digest {
            duplicate |= index.replace(visited).is_some();
        }
        visited += 1;
    })
    .map_err(|e| format!("{e:?}"))?;
    require(
        root == claim.expected.artifact_root && count <= MAX_INVENTORY_LEAVES && visited == u64::from(count) && !duplicate,
        "mapped artifact inventory root or leaf cap differs",
    )?;
    let index = u32::try_from(index.ok_or("embedding row is absent from the registered inventory")?).map_err(|e| e.to_string())?;
    let mut proof = PalwArtifactMultiproofStreamV1::new(count, &[index]).ok_or("invalid embedding inventory index")?;
    let (root, _, opened) = a16_readiness_leaves_streamed_v1(artifact, &claim.class.profile, &[index], &mut |hash| proof.push(hash))
        .map_err(|e| format!("{e:?}"))?;
    require(root == claim.expected.artifact_root, "mapped inventory changed during the opening pass")?;
    let proof = proof.finish(&opened).ok_or("incomplete embedding opening")?;
    one_opening(proof, claim.expected.artifact_root)
}

fn one_opening(mut proof: PalwArtifactMultiproofV1, registered_root: Hash64) -> Result<PalwArtifactOpeningV1, String> {
    require(proof.opened.len() == 1, "expected one opened embedding row")?;
    let (leaf_index, operand) = proof.opened.pop().ok_or("missing embedding opening")?;
    let opening = PalwArtifactOpeningV1 { leaf_index, leaf_count: proof.leaf_count, operand, path: proof.siblings };
    verify_artifact_opening_v1(&opening, registered_root).map_err(|e| format!("{e:?}"))?;
    Ok(opening)
}

fn build(claim: &RootedClaim, capture: &Path, path: &Path) -> Result<PalwLeafEvidenceV1, String> {
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
    let opening = open_operand(claim, &artifact)?;
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
    let refutation = backend.fp_leaf_refutation_v1(&bytes, &claim.prompt, claim.roots, claim.step_leaves, LEAF)?;
    let (refutation, prompt_ids_opening) = palw_refutation_prompt_carriage_v1(claim.form, refutation).map_err(|e| format!("{e:?}"))?;
    Ok(PalwLeafEvidenceV1 { refutation, artifact_openings: vec![opening], prompt_ids_opening })
}

fn verdict(claim: &RootedClaim, evidence: &PalwLeafEvidenceV1) -> Result<PalwShardCourtVerdictV1, String> {
    let binding = &evidence.refutation.binding;
    require(
        evidence.leaf_index() == LEAF
            && binding.job_context == claim.job
            && binding.shape_profile == claim.class.profile
            && binding.step_leaf_count == claim.step_leaves
            && binding.full_logits_trace_root == claim.roots.trace_root
            && evidence.artifact_openings.len() == 1,
        "evidence changes the independently pinned leaf, class, original job or trace",
    )?;
    let (name, layer, offset, width) = operand_key(claim)?;
    let operand = &evidence.artifact_openings[0].operand;
    require(
        operand.tensor_name == name && operand.layer == layer && operand.row_start == offset && operand.bytes.len() == width as usize,
        "evidence substitutes the embedding operand coordinate",
    )?;
    evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true).map_err(|e| format!("{e:?}"))
}

fn verify(claim: &RootedClaim, path: &Path) -> Result<(), String> {
    let bytes = read_bounded(path, MAX_EVIDENCE)?;
    let quote = preflight_native_value_v1::<PalwLeafEvidenceV1>(bytes.as_slice(), bytes.len() as u64).map_err(|e| format!("{e:?}"))?;
    require(quote.logical_requests_v1() <= MAX_EVIDENCE_HEAP, "evidence allocation quote exceeds cap")?;
    let mut evidence: PalwLeafEvidenceV1 = borsh::from_slice(&bytes).map_err(|e| e.to_string())?;
    require(verdict(claim, &evidence)? == PalwShardCourtVerdictV1::FalseAccusation, "the named original tile did not recompute")?;

    let mut opening = evidence.artifact_openings.pop().ok_or("missing checked opening")?;
    require(
        matches!(
            evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true),
            Err(PalwShardCourtError::Refutation(PalwStepRefuteError::Unadjudicable))
        ),
        "native checker did not defer the withheld operand",
    )?;
    opening.operand.bytes[0] ^= 1;
    evidence.artifact_openings.push(opening);
    require(
        matches!(evidence.verdict_at_v2(&claim.expected, MAX_STEP_LEAVES, true), Err(PalwShardCourtError::ArtifactOpenings(_))),
        "native checker did not refuse the altered artifact operand",
    )?;
    println!(
        "{}",
        json!({
            "scope": "one-original-t12-a16-embedding-tile",
            "claimId": claim.observation.claim_id,
            "blockHash": claim.observation.block_hash,
            "anchor": claim.roots.anchor,
            "daa": claim.daa,
            "leaf": LEAF,
            "evidenceBytes": bytes.len(),
            "evidenceSha256": sha256(&bytes),
            "matched": true,
            "missingSourceDeferred": true,
            "wrongSourceRejected": true,
            "fullExecutionVerified": false,
            "chainAcceptanceInferred": false,
            "gateCInferred": false,
        })
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [mode, input, sha, capture, artifact, output] if mode == "build" => {
            let claim = rooted_claim(Path::new(input), sha.to_str().ok_or("expected SHA-256 text")?)?;
            let evidence = build(&claim, Path::new(capture), Path::new(artifact))?;
            require(verdict(&claim, &evidence)? == PalwShardCourtVerdictV1::FalseAccusation, "built tile did not recompute")?;
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
        [mode, input, sha, evidence] if mode == "verify" => {
            let claim = rooted_claim(Path::new(input), sha.to_str().ok_or("expected SHA-256 text")?)?;
            verify(&claim, Path::new(evidence))
        }
        _ => Err("usage: check-t12-a16-source build OBSERVATION OBS_SHA256 CAPTURE ARTIFACT NEW_EVIDENCE | verify OBSERVATION OBS_SHA256 EVIDENCE".into()),
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
