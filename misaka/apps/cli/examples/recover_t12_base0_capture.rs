//! Offline recovery of the raw BASE0 capture for an accepted T12 PAV2 attempt.
//! The input's selected-chain observation comes from a trusted read-only node.
//! Re-execution checks the signed commitments. It cannot establish chain selection or Gate C.

use kaspa_consensus_core::{
    Hash64,
    config::params::{Params, palw_t12_shipped_params},
    hashing::header::{hash as header_hash, pre_pow_hash_64},
    header::Header,
    palw_attempt_rules_v1::palw_attempt_rules_of_params_v1,
    palw_attempt_v2::{PalwAttemptEnvelopeV2, attempt_id_v2, palw_attempt_job_v1, palw_network_domain_v2_for, palw_nonce_bucket_v1},
    palw_backend::{PalwExecutionBackendV1, PalwExecutionOutcomeV1},
    palw_mode_v2::PalwConsensusMode,
    pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
};
use kaspa_rpc_core::RpcHeader;
use misaka_palw_base0::{backend::Base0Backend, classes::resolve_class_v1};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const MAX_INPUT_BYTES: usize = 128 * 1024;
const MAX_PAV2_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptedAttempt {
    consensus_params_id: String,
    consensus_schedule_id: String,
    genesis_hash: Hash64,
    selected_sink: Hash64,
    is_selected_with_body: bool,
    accepted_block: Hash64,
    accepted_daa: u64,
    claim_id: Hash64,
    accepted_output_root: Hash64,
    registered_executor_bond: String,
    registered_executor_pubkey: String,
    stored_trace_retention_daa: u64,
    header: RpcHeader,
}

fn ensure(condition: bool, reason: &'static str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| reason.into())
}

fn decode_accepted_attempt(input: &AcceptedAttempt, params: &Params) -> Result<(Header, PalwAttemptEnvelopeV2), String> {
    ensure(
        input.consensus_params_id == params.consensus_params_id().to_string()
            && input.consensus_schedule_id == params.consensus_schedule_id().to_string()
            && input.genesis_hash == params.genesis.hash,
        "observation is not from the pinned testnet-12 ruleset",
    )?;
    ensure(
        input.is_selected_with_body && input.selected_sink != Hash64::default() && input.accepted_daa != 0,
        "missing selected-chain observation with body",
    )?;
    let header = Header::try_from(&input.header).map_err(|err| err.to_string())?;
    ensure(
        header.hash == input.accepted_block && header_hash(&header) == input.accepted_block && header.daa_score == input.accepted_daa,
        "accepted block, calculated header hash or acceptance DAA does not match",
    )?;
    ensure(
        header.pow_algo_id == POW_ALGO_ID_PALW_COMMITTED_V2 && header.palw_commitment.len() <= MAX_PAV2_BYTES,
        "expected a bounded native PAV2 attempt",
    )?;
    let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).map_err(|err| err.to_string())?;
    let attempt = &envelope.attempt;
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("testnet-12 BASE0 rules are unavailable".into());
    };
    ensure(attempt.class_id == bundle.base_class_id, "attempt is not the testnet-12 BASE0 class")?;
    ensure(attempt_id_v2(attempt) == input.claim_id, "signed attempt does not name the accepted claim")?;
    ensure(attempt.output_root == input.accepted_output_root, "signed and accepted output roots differ")?;
    ensure(
        input.registered_executor_bond == format!("{}:{}", attempt.executor_bond.transaction_id, attempt.executor_bond.index),
        "signed bond does not match the registered executor outpoint",
    )?;
    ensure(input.stored_trace_retention_daa >= attempt.trace_retention_daa, "stored retention is shorter than the signed deadline")?;
    ensure(
        input.registered_executor_pubkey.len() == 2 * kaspa_txscript::MLDSA87_PK_LEN
            && input.registered_executor_pubkey == faster_hex::hex_string(&attempt.executor_pubkey),
        "signed key does not match the registered executor public key",
    )?;
    let domain = palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash));
    envelope
        .validate_stateless_v2_at_version(
            params.palw_attempt_lane_at(header.daa_score).attempt_version(),
            domain,
            pre_pow_hash_64(&header),
            header.timestamp,
            header.nonce,
        )
        .map_err(|err| err.to_string())?;
    envelope
        .validate_signature_v2(|key, message, sig, context| {
            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
        })
        .map_err(|err| err.to_string())?;
    Ok((header, envelope))
}

fn replay(params: &Params, header: &Header, envelope: &PalwAttemptEnvelopeV2) -> Result<PalwExecutionOutcomeV1, String> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("testnet-12 BASE0 rules are unavailable".into());
    };
    let attempt = &envelope.attempt;
    let resolved = resolve_class_v1(&bundle.court, attempt.class_id, attempt.artifact_root, &[]).map_err(|err| err.to_string())?;
    let backend = Base0Backend::new(resolved)
        .with_step_ladder_cap(bundle.court.max_step_leaf_count())
        .with_prompt_ids_form(params.palw_prompt_ids_form_v1())
        .with_attempt_rules(palw_attempt_rules_of_params_v1(params));
    let anchor = backend
        .job_anchor_v1(
            attempt.network_domain,
            pre_pow_hash_64(header),
            attempt.class_id,
            &attempt.executor_bond,
            palw_nonce_bucket_v1(header.nonce),
        )
        .ok_or("cannot derive the canonical BASE0 attempt job anchor")?;
    let (job, prompt) = backend.job_for_anchor(anchor)?;
    let job = palw_attempt_job_v1(job, params.palw_prefill_draw_active_at(header.daa_score));
    backend.execute(&job, &prompt)
}

fn check_signed_roots(envelope: &PalwAttemptEnvelopeV2, actual: &PalwExecutionOutcomeV1) -> Result<(), String> {
    let attempt = &envelope.attempt;
    ensure(
        actual.trace_root == attempt.trace_root
            && actual.output_root == attempt.output_root
            && actual.execution_root == attempt.execution_root
            && actual.trace_manifest_root == attempt.trace_manifest_root
            && actual.trace_chunk_count == attempt.trace_chunk_count,
        "regenerated BASE0 material does not reproduce all signed commitments",
    )
}

fn private_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    file.write_all(data)?;
    file.sync_all()
}

fn run(input_path: &Path, stage: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(input_path)?;
    ensure(file.metadata()?.is_file(), "expected a regular public input file")?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    ensure(bytes.len() <= MAX_INPUT_BYTES, "public input exceeds 128 KiB")?;
    let input: AcceptedAttempt = serde_json::from_slice(&bytes)?;
    let params = palw_t12_shipped_params();
    let (header, envelope) = decode_accepted_attempt(&input, &params)?;
    let outcome = replay(&params, &header, &envelope)?;
    check_signed_roots(&envelope, &outcome)?;

    let material_name = format!("{}.material", input.claim_id);
    let report = json!({
        "format": "t12-base0-recovered-material-v1",
        "inputSha256": faster_hex::hex_string(&Sha256::digest(&bytes)),
        "consensusParamsId": input.consensus_params_id,
        "consensusScheduleId": input.consensus_schedule_id,
        "genesisHash": input.genesis_hash,
        "observedSelectedSink": input.selected_sink,
        "claimId": input.claim_id,
        "acceptedBlock": input.accepted_block,
        "acceptedDaa": input.accepted_daa,
        "classId": envelope.attempt.class_id,
        "artifactRoot": envelope.attempt.artifact_root,
        "signedTraceRetentionDaa": envelope.attempt.trace_retention_daa,
        "storedTraceRetentionDaa": input.stored_trace_retention_daa,
        "traceRoot": outcome.trace_root,
        "executionRoot": outcome.execution_root,
        "outputRoot": outcome.output_root,
        "traceManifestRoot": outcome.trace_manifest_root,
        "traceChunkCount": outcome.trace_chunk_count,
        "materialFile": material_name,
        "materialBytes": outcome.material.len(),
        "materialSha256": faster_hex::hex_string(&Sha256::digest(&outcome.material)),
    });
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    DirBuilder::new().mode(0o700).create(stage)?;
    private_write(&stage.join(&material_name), &outcome.material)?;
    private_write(&stage.join("recovery.json"), &report_bytes)?;
    File::open(stage)?.sync_all()?;
    let parent = stage.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [input_path, stage] = args.as_slice() else {
        return Err("usage: recover_t12_base0_capture <accepted-public-attempt.json> <new-private-stage>".into());
    };
    run(input_path, stage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PalwAttemptChainFactsV1, PalwAttemptExecutionV1, palw_attempt_from_execution_v1,
        },
        tx::TransactionOutpoint,
    };
    use kaspa_pq_validator_core::ValidatorKey;
    use misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1;

    #[test]
    fn shipped_base0_fixture_reproduces_signed_capture_and_refuses_changed_inputs() {
        let params = palw_t12_shipped_params();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("missing testnet-12 rules") };
        let artifact_root = palw_rc_base0_artifact_root_v1().unwrap();
        let backend = Base0Backend::new(resolve_class_v1(&bundle.court, bundle.base_class_id, artifact_root, &[]).unwrap())
            .with_step_ladder_cap(bundle.court.max_step_leaf_count())
            .with_prompt_ids_form(params.palw_prompt_ids_form_v1())
            .with_attempt_rules(palw_attempt_rules_of_params_v1(&params));
        let mut header = Header::new_finalized(
            2,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            1_793_600_000_000,
            params.genesis.bits,
            0,
            POW_ALGO_ID_PALW_COMMITTED_V2,
            19_188,
            Default::default(),
            40_000,
            params.genesis.hash,
        );
        let domain = palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash));
        let bond = TransactionOutpoint::new(Hash64::from_u64_word(17), 0);
        let anchor = backend
            .job_anchor_v1(domain, pre_pow_hash_64(&header), bundle.base_class_id, &bond, palw_nonce_bucket_v1(header.nonce))
            .unwrap();
        let (job, prompt) = backend.job_for_anchor(anchor).unwrap();
        let job = palw_attempt_job_v1(job, params.palw_prefill_draw_active_at(header.daa_score));
        let original = backend.execute(&job, &prompt).unwrap();
        let signer = ValidatorKey::from_seed([37; 32]);
        let mut attempt = palw_attempt_from_execution_v1(
            domain,
            pre_pow_hash_64(&header),
            header.timestamp,
            header.nonce,
            bond,
            signer.public_key().to_vec(),
            &PalwAttemptChainFactsV1 {
                class_id: bundle.base_class_id,
                artifact_root,
                pwu: 1,
                min_trace_retention_daa: 11_440,
                witness_chunks: 0,
                operator_id: Hash64::from_u64_word(19),
            },
            &PalwAttemptExecutionV1 {
                trace_root: original.trace_root,
                output_root: original.output_root,
                execution_root: original.execution_root,
                trace_manifest_root: original.trace_manifest_root,
                trace_chunk_count: original.trace_chunk_count,
            },
            header.daa_score,
        );
        attempt.version = params.palw_attempt_lane_at(header.daa_score).attempt_version();
        let claim_id = attempt_id_v2(&attempt);
        let signature = signer.sign_with_context(claim_id.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT).to_vec();
        let mut envelope = PalwAttemptEnvelopeV2 { attempt, signature };
        header.palw_commitment = envelope.encode_wire();
        header.finalize();
        let mut input = AcceptedAttempt {
            consensus_params_id: params.consensus_params_id().to_string(),
            consensus_schedule_id: params.consensus_schedule_id().to_string(),
            genesis_hash: params.genesis.hash,
            selected_sink: Hash64::from_u64_word(7),
            is_selected_with_body: true,
            accepted_block: header.hash,
            accepted_daa: header.daa_score,
            claim_id,
            accepted_output_root: original.output_root,
            registered_executor_bond: format!("{}:{}", bond.transaction_id, bond.index),
            registered_executor_pubkey: faster_hex::hex_string(signer.public_key()),
            stored_trace_retention_daa: 117_505_304,
            header: RpcHeader::from(&header),
        };
        let (verified_header, verified_envelope) = decode_accepted_attempt(&input, &params).unwrap();
        let mut regenerated = replay(&params, &verified_header, &verified_envelope).unwrap();
        check_signed_roots(&verified_envelope, &regenerated).unwrap();
        assert_eq!(regenerated.material, original.material);

        regenerated.trace_chunk_count += 1;
        assert!(check_signed_roots(&verified_envelope, &regenerated).is_err());
        input.accepted_block = Hash64::from_u64_word(3);
        assert!(decode_accepted_attempt(&input, &params).err().unwrap().contains("header hash"));
        input.accepted_block = header.hash;
        input.claim_id = Hash64::from_u64_word(3);
        assert!(decode_accepted_attempt(&input, &params).err().unwrap().contains("accepted claim"));
        input.claim_id = claim_id;
        envelope.signature[0] ^= 1;
        header.palw_commitment = envelope.encode_wire();
        header.finalize();
        input.header = RpcHeader::from(&header);
        input.accepted_block = header.hash;
        assert!(decode_accepted_attempt(&input, &params).err().unwrap().contains("signature"));
    }
}
