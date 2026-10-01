//! `misaka palw gen-registration` and `misaka palw gen-claim` — RFC-0003's pipeline classes and tensor claims,
//! written to files (the carriage's CLI half; the IR twin of the first is `palw tir-registration`).
//!
//! * **`gen-registration`**: a signed `ClassRegisteredGenV1` for a `PALWTIR2` artifact's class at the connected
//!   chain's live pricing, weightless, the registrant bond named by `--bond` and signed with this key over
//!   `palw_gen_class_registration_message_v1` under the chain's own domain. Nothing is submitted: file it with
//!   `misaka palw submit-object --object <out> --yes`. The node's own gate is not asked first, so an object for a
//!   height the generative fence does not cover yet can be written.
//! * **`gen-claim`**: runs a tensor job on this machine over the held class — the same worker a seat replays
//!   with (`GenBackendV1::answer`) — and writes the claim: the signed commitment transaction
//!   (`<claim>.commitment-tx.borsh`, funded from this key at its measured relay fee; file it with `misaka palw
//!   fp-submit --tx <it> --yes`) and the claim's material (`<claim>.material`, the capture served as `FPG1`)
//!   into the node's retention directory, from which the executor's node serves the panel. A job is described
//!   in JSON (see [`JobRequest`]); the fields a class fixes (sampler, resolution, pooling) default from it.

use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_gen_job_v1::{
    PALW_GEN_JOB_VERSION_V1, PalwGenBodyV1, PalwGenEmbeddingBodyV1, PalwGenEmbeddingInputV1, PalwGenImageBodyV1, PalwGenJobV1,
    PalwJobEnvelopeV1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use misaka_palw_sdk::lineages::generative::GenLineageV1;
use std::path::Path;

fn parse_bond(bond: &str) -> Result<PalwBondKeyV2, CliError> {
    let (txid, index) =
        bond.split_once(':').ok_or_else(|| CliError::new(exit::CONFIG, format!("--bond {bond}: not <txid>:<index>")))?;
    Ok(PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        txid.parse().map_err(|_| CliError::new(exit::CONFIG, format!("--bond {bond}: not a transaction id")))?,
        index.parse().map_err(|_| CliError::new(exit::CONFIG, format!("--bond {bond}: not an index")))?,
    )))
}

fn hex_array<const N: usize>(what: &str, text: &str) -> Result<[u8; N], CliError> {
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| text.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()))
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(|| CliError::new(exit::CONFIG, format!("{what}: not hex")))?;
    bytes.try_into().map_err(|_| CliError::new(exit::CONFIG, format!("{what}: expected {N} bytes ({} hex digits)", N * 2)))
}

/// **`misaka palw gen-registration`** (see the module doc).
pub(crate) async fn gen_registration_object(
    ctx: &Ctx,
    key: &crate::keys::KeySource,
    artifact: &Path,
    bond: &str,
    out: &Path,
    model_id: Option<&str>,
) -> CliResult {
    let nv = connect(ctx).await?;
    let params = nv.params.clone();
    if !matches!(params.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
        return Err(CliError::new(exit::CONFIG, format!("{} has no PALW classes", params.net)));
    }
    let entry = GenLineageV1::open_entry(artifact).map_err(|e| CliError::new(exit::MODEL, e))?;
    if model_id.is_some_and(|m| m != entry.model_id) {
        return Err(CliError::new(
            exit::MODEL,
            format!("{} declares {}, not the --model-id given", artifact.display(), entry.model_id),
        ));
    }
    let registrant = parse_bond(bond)?;
    let terms_resp = nv
        .client
        .get_palw_registration_terms()
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwRegistrationTerms: {e}")))?;
    let (terms, _) = crate::operator::model_add::decode_terms(&terms_resp).map_err(|e| CliError::new(exit::GENERIC, e))?;
    let build = |signature: Vec<u8>| {
        kaspa_consensus_core::palw_gen_admission_v1::palw_gen_post_genesis_registration_v1(
            entry.row.class.as_ref().clone(),
            entry.artifact_root,
            0,
            terms.initial_target,
            terms.slash_value_per_pwu,
            0,
            registrant,
            signature,
        )
        .map_err(|e| CliError::new(exit::MODEL, format!("{}: {} ({e})", entry.model_id, e.code())))
    };
    let unsigned = build(Vec::new())?;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    let message = misaka_palw_sdk::gen_class::gen_registration_message_v1(domain, &unsigned)
        .ok_or_else(|| CliError::new(exit::GENERIC, "the builder returned another object than a generative registration"))?;
    let key = key.load_key()?;
    let signature = key.sign_with_context(
        message.as_byte_slice(),
        kaspa_consensus_core::palw_gen_class_v1::PALW_GEN_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
    );
    let object = build(signature.to_vec())?;
    let bytes = borsh::to_vec(&object).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?;
    std::fs::write(out, &bytes).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", out.display())))?;
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.gen-registration.v1",
                "out": out.display().to_string(),
                "bytes": bytes.len(),
                "class_id": entry.class_id().to_string(),
                "artifact_root": entry.artifact_root.to_string(),
                "model_id": entry.model_id,
                "registrant_bond": bond,
            })
        );
    } else {
        println!(
            "wrote {} ({} bytes): ClassRegisteredGenV1 for {} — signed by bond {bond}",
            out.display(),
            bytes.len(),
            entry.model_id
        );
        println!("class_id       {}", entry.class_id());
        println!("artifact_root  {}", entry.artifact_root);
        println!("file it with: misaka palw submit-object --object {} --yes", out.display());
    }
    Ok(())
}

/// **A tensor job, as the CLI reads it** (JSON). Everything a class fixes defaults from the class; everything
/// the user chooses is stated.
#[derive(serde::Deserialize, Debug)]
pub(crate) struct JobRequest {
    /// `"image"` or `"embedding"` (a text embedding).
    pub profile: String,
    /// The anchor block and its DAA (the job's freshness, read from the chain by whoever builds the request).
    pub anchor_block: String,
    pub anchor_daa: u64,
    /// 32 bytes, 64 hex digits each: uniqueness only, and R's key.
    pub job_nonce: String,
    pub seed: String,
    /// `"public"` (the ids ride the commitment) or `"panel"` (they ride the capture served to the panel).
    #[serde(default = "default_privacy")]
    pub privacy: String,
    #[serde(default)]
    pub prompt_token_ids: Vec<u32>,
    #[serde(default)]
    pub negative_token_ids: Vec<u32>,
    // An image job.
    pub steps: Option<u16>,
    pub guidance_q: Option<u16>,
    pub image_index: Option<u16>,
    // An embedding job.
    pub pooling: Option<u8>,
    pub dims: Option<u32>,
}

fn default_privacy() -> String {
    "public".to_string()
}

/// What `gen-claim` is asked for.
pub(crate) struct GenClaimArgs<'a> {
    pub artifact: &'a Path,
    pub request: &'a Path,
    /// The executor bond, `<txid>:<index>` (its key is the one `--key-file` names).
    pub bond: &'a str,
    /// The executor bond's operator id, 128 hex digits.
    pub operator_id: &'a str,
    /// Where to write `<claim>.commitment-tx.borsh`.
    pub out_dir: &'a Path,
    /// The node's retention directory: where `<claim>.material` is written (omit to write it beside the tx).
    pub retention_dir: Option<&'a Path>,
}

/// **`misaka palw gen-claim`** (see the module doc).
pub(crate) async fn gen_claim_files(ctx: &Ctx, key: &crate::keys::KeySource, a: GenClaimArgs<'_>) -> CliResult {
    use kaspa_consensus_core::mass::MassCalculator;
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, fp_claim_id_v3,
    };
    use misaka_palw_base0::gen_tensor_worker::{
        GenTensorCaptureV1, PalwGenTensorAnswerV1, PalwGenTensorRequestV1, gen_tensor_material_encode_v1,
    };

    let nv = connect(ctx).await?;
    let params = nv.params.clone();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        return Err(CliError::new(exit::CONFIG, format!("{} has no PALW classes", params.net)));
    };
    let form = params.palw_prompt_ids_form_v1();
    let entry = GenLineageV1::open_entry(a.artifact).map_err(|e| CliError::new(exit::MODEL, e))?;
    let backend = GenLineageV1::backend(&entry, &bundle.court, form);
    let row = &entry.row;

    // The job.
    let request: JobRequest = serde_json::from_slice(
        &std::fs::read(a.request).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", a.request.display())))?,
    )
    .map_err(|e| CliError::new(exit::CONFIG, format!("{}: {e}", a.request.display())))?;
    let bond = parse_bond(a.bond)?;
    let operator_id: Hash64 = a.operator_id.parse().map_err(|_| CliError::new(exit::CONFIG, "--operator-id: not a 128-hex Hash64"))?;
    let key = key.load_key()?;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    let privacy_mode = match request.privacy.as_str() {
        "public" => PALW_FP_PRIVACY_PUBLIC_DA,
        "panel" => PALW_FP_PRIVACY_PANEL_DA,
        other => return Err(CliError::new(exit::CONFIG, format!("privacy {other}: \"public\" or \"panel\""))),
    };
    let ids_hash = |ids: &[u32]| -> Result<Hash64, CliError> {
        if ids.is_empty() {
            return Ok(Hash64::default());
        }
        prompt_token_ids_commitment_v1(form, ids).map_err(|e| CliError::new(exit::CONFIG, format!("the ids' commitment: {e:?}")))
    };
    let class = row.class.as_ref();
    let body = match (request.profile.as_str(), &class.offers.profile) {
        ("image", kaspa_consensus_core::palw_gen_class_v1::PalwGenProfileOffersV1::Image(offers)) => {
            let shape = &class.output.shape;
            let (height, width) = match shape.as_slice() {
                [h, w, _] => (*h as u16, *w as u16),
                _ => return Err(CliError::new(exit::MODEL, "an image class's output is [height, width, 3]")),
            };
            PalwGenBodyV1::Image(PalwGenImageBodyV1 {
                prompt_token_ids_hash: ids_hash(&request.prompt_token_ids)?,
                prompt_tokens: request.prompt_token_ids.len() as u32,
                negative_token_ids_hash: ids_hash(&request.negative_token_ids)?,
                negative_tokens: request.negative_token_ids.len() as u32,
                guidance_q: request.guidance_q.or(offers.guidance.as_ref().map(|g| g.lo)).unwrap_or(0),
                image_index: request.image_index.unwrap_or(0),
                sampler_id: offers.sampler_id,
                steps: request
                    .steps
                    .or(class.offers.steps.last().map(|s| *s as u16))
                    .ok_or_else(|| CliError::new(exit::CONFIG, "the class offers no step count and the request names none"))?,
                width,
                height,
                output: class.output.kind,
            })
        }
        ("embedding", kaspa_consensus_core::palw_gen_class_v1::PalwGenProfileOffersV1::Embedding(offers)) => {
            PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 {
                input: PalwGenEmbeddingInputV1::Text {
                    token_ids_hash: ids_hash(&request.prompt_token_ids)?,
                    tokens: request.prompt_token_ids.len() as u32,
                },
                pooling: request.pooling.unwrap_or(offers.pooling),
                dims: request.dims.or(offers.dims.last().copied()).ok_or_else(|| CliError::new(exit::CONFIG, "no output width"))?,
                output: class.output.kind,
            })
        }
        (profile, _) => {
            return Err(CliError::new(
                exit::CONFIG,
                format!(
                    "the request's profile {profile:?} is not this class's (profile {}); \"image\" and \"embedding\" are built",
                    class.profile
                ),
            ));
        }
    };
    let job = PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: PalwJobEnvelopeV1 {
            network_domain: domain,
            class_id: row.class_id,
            executor_bond: bond.0,
            executor_pubkey: key.public_key().to_vec(),
            operator_id,
            anchor_block: request
                .anchor_block
                .parse()
                .map_err(|_| CliError::new(exit::CONFIG, "anchor_block: not a 128-hex Hash64"))?,
            anchor_daa: request.anchor_daa,
            job_nonce: hex_array::<32>("job_nonce", &request.job_nonce)?,
            privacy_mode,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
        },
        seed: hex_array::<32>("seed", &request.seed)?,
        body,
    };

    // The run: the worker a seat replays with.
    let frame = PalwGenTensorRequestV1 {
        job: job.clone(),
        prompt_ids: request.prompt_token_ids.clone(),
        negative_ids: request.negative_token_ids.clone(),
        images: Vec::new(),
    };
    let (answer, work) = backend.answer(&borsh::to_vec(&frame).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?);
    let work = match (answer, work) {
        (PalwGenTensorAnswerV1::Result { .. }, Some(work)) => work,
        (PalwGenTensorAnswerV1::Refused { why }, _) => {
            return Err(CliError::new(exit::MODEL, format!("the worker refuses the job: {why}")));
        }
        (PalwGenTensorAnswerV1::Result { .. }, None) => return Err(CliError::new(exit::GENERIC, "the worker answered with no run")),
    };
    let binding = &work.binding;

    // The commitment, the transaction.
    let ids_on_chain = if privacy_mode == PALW_FP_PRIVACY_PUBLIC_DA {
        let mut ids = request.prompt_token_ids.clone();
        ids.extend_from_slice(&request.negative_token_ids);
        ids
    } else {
        Vec::new()
    };
    let payload = kaspa_consensus_core::palw_gen_claim_v1::palw_gen_payload_v1(
        &job,
        binding.step_leaf_count,
        binding.step_root(),
        binding.output_root,
        ids_on_chain,
        Vec::new(),
    );
    let claim_id = fp_claim_id_v3(&payload.commitment);
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::lifecycle_candidates_v1(&nv, &addr).await?;
    let (outpoint, funding) = candidates
        .first()
        .cloned()
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("no mature, unbonded, unspent UTXO at {addr} to fund the claim")))?;
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let calc = MassCalculator::new(
        nv.params.mass_per_tx_byte,
        nv.params.mass_per_script_pub_key_byte,
        nv.params.mass_per_sig_op,
        nv.params.storage_mass_parameter,
    );
    let probe = key
        .build_gen_commitment_tx(payload.clone(), outpoint, &funding, floor)
        .map_err(|e| CliError::new(exit::GENERIC, format!("build the commitment: {e}")))?;
    let compute_mass = calc.calc_non_contextual_masses(&probe).compute_mass;
    let fee = kaspa_pq_validator_core::relay_fee_for_compute_mass(compute_mass).max(floor);
    if funding.amount <= fee {
        return Err(CliError::new(exit::GENERIC, format!("the funding holds {} sompi, under its {fee} sompi fee", funding.amount)));
    }
    let tx = key
        .build_gen_commitment_tx(payload, outpoint, &funding, fee)
        .map_err(|e| CliError::new(exit::GENERIC, format!("build the commitment: {e}")))?;
    // The tensor door's own stateless rules, asked of what was built, before a fee is spent on it.
    kaspa_consensus_core::palw_gen_claim_v1::validate_palw_fp_gen_commitment_tx_v1(
        &tx.payload,
        params.palw_panel_da_admissible(),
        form,
        kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP,
    )
    .map_err(|e| CliError::new(exit::GENERIC, format!("the commitment is not admissible: {e}")))?;

    // The files: the material (the capture, `FPG1`) and the transaction.
    let capture = GenTensorCaptureV1::of(&work).map_err(|e| CliError::new(exit::GENERIC, format!("the capture: {e}")))?;
    let material = gen_tensor_material_encode_v1(&capture);
    let material_dir = a.retention_dir.unwrap_or(a.out_dir);
    std::fs::create_dir_all(a.out_dir).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", a.out_dir.display())))?;
    std::fs::create_dir_all(material_dir).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", material_dir.display())))?;
    let material_path = material_dir.join(format!("{claim_id}.material"));
    std::fs::write(&material_path, &material).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", material_path.display())))?;
    let tx_path = a.out_dir.join(format!("{claim_id}.commitment-tx.borsh"));
    std::fs::write(&tx_path, borsh::to_vec(&tx).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?)
        .map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", tx_path.display())))?;
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.gen-claim.v1",
                "claim_id": claim_id.to_string(),
                "txid": tx.id().to_string(),
                "tx": tx_path.display().to_string(),
                "material": material_path.display().to_string(),
                "material_bytes": material.len(),
                "work_leaves": binding.step_leaf_count,
                "execution_root": binding.committed_execution_root.to_string(),
                "fee_sompi": fee,
            })
        );
    } else {
        println!("claim {claim_id}: {} step leaves, execution root {}", binding.step_leaf_count, binding.committed_execution_root);
        println!("  commitment {} ({}-byte payload, fee {fee} sompi) written: {}", tx.id(), tx.payload.len(), tx_path.display());
        println!("  material ({} bytes, the capture served as FPG1) written: {}", material.len(), material_path.display());
        println!("file it with: misaka palw fp-submit --tx {} --yes", tx_path.display());
    }
    Ok(())
}
