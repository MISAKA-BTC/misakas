//! Private, opt-in real-artifact role measurement. No consensus activation.
//! The Node subprocess receives program/commitments and bounded evidence parts,
//! never the model artifact path or an executor/weight store.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_positive_court_v2::*,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::PalwTirCourtRulesV1,
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir_exec::node::{TirArtifactV1, TirClassRunnerV1, positive_court::*};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    time::Instant,
};

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;
#[derive(BorshSerialize, BorshDeserialize)]
struct Header {
    result: MinerResultV2,
    witness: PanelWitnessV2,
    registered: PalwTirClassV1,
    registered_class_id: Hash64,
    miner_key: Vec<u8>,
    miner_signature: Vec<u8>,
    panel_key: Vec<u8>,
    panel_signature: Vec<u8>,
    policy: ExactPolicyV2,
}
fn create(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}
fn peak_rss_bytes() -> u64 {
    #[cfg(unix)]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == 0 {
            let peak = unsafe { usage.assume_init() }.ru_maxrss as u64;
            return if cfg!(target_os = "macos") { peak } else { peak * 1024 };
        }
    }
    0
}
fn read_u64(reader: &mut impl Read) -> std::io::Result<u64> {
    let mut b = [0u8; 8];
    reader.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}
fn node(header: &str, parts: &str, report: &str) -> AnyResult<()> {
    let started = Instant::now();
    // Registered program/class ID represent already authenticated chain metadata;
    // there are no model weights in this header. This is a local measurement
    // header, not a network decoder. Native ingress remains
    // closed; the production part decoder performs its structural preflight.
    if std::fs::metadata(header)?.len() > (16 << 20) {
        return Err("measurement header cap".into());
    }
    let h: Header = borsh::from_slice(&std::fs::read(header)?)?;
    let scope = AdmissionScopeV2 { network: h.result.network, branch: h.result.branch, ruleset: h.result.ruleset, required_form: gate::ClaimFormV2::MinerResultV2, registered_class_id: h.registered_class_id };
    let mut case = ExactCaseV2::admit(
        h.result,
        &SignatureV2(h.miner_signature),
        &h.miner_key,
        h.witness,
        &SignatureV2(h.panel_signature),
        &h.panel_key,
        &h.registered,
        h.policy,
        PalwPromptIdsFormV1::Flat,
        1,
        scope,
    )
    .map_err(|e| format!("admit: {e:?}"))?;
    let mut reader = BufReader::new(File::open(parts)?);
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != b"PCPARTV2" {
        return Err("measurement part framing".into());
    }
    let count = case.witness().binding.step_leaf_count;
    let mut total_bytes = 0u64;
    for index in 0..count {
        let cone_len = read_u64(&mut reader)?;
        let trace_len = read_u64(&mut reader)?;
        if cone_len.checked_add(trace_len).filter(|n| *n <= h.policy.max_part_bytes).is_none() {
            return Err("measurement frame cap".into());
        }
        let mut cone = vec![0u8; cone_len as usize];
        reader.read_exact(&mut cone)?;
        let mut trace = vec![0u8; trace_len as usize];
        reader.read_exact(&mut trace)?;
        case.submit_encoded(&cone, if trace.is_empty() { None } else { Some(&trace) }, 2)
            .map_err(|e| format!("Node leaf {index}: {e:?}"))?;
        total_bytes += cone_len + trace_len;
        if index % 128 == 0 {
            eprintln!("Node checked {index}/{count} leaves");
        }
    }
    if reader.read(&mut [0u8; 1])? != 0 {
        return Err("trailing measurement frames".into());
    }
    let status = case.finish(3).map_err(|e| format!("finish: {e:?}"))?;
    if status != ClaimStatusV2::Passed {
        return Err(format!("Node result: {status:?}").into());
    }
    let data = serde_json::json!({ "status": format!("{status:?}"), "checked_leaves": count, "evidence_bytes": total_bytes,
        "seconds": started.elapsed().as_secs_f64(), "peak_rss_bytes": peak_rss_bytes(),
        "model_artifact_opened": false, "node_model_preloaded": false,
        "public_activation": false, "consensus_credit": false });
    create(Path::new(report))?.write_all(serde_json::to_string_pretty(&data)?.as_bytes())?;
    Ok(())
}
fn main() -> AnyResult<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|x| x == "--node") {
        if args.len() != 4 {
            return Err("--node <local-header> <parts> <fresh-report>".into());
        }
        return node(&args[1], &args[2], &args[3]);
    }
    if args.len() != 3 {
        return Err("usage: positive-court-llm <trained-artifact> <calibration.json> <fresh-report.json>".into());
    }
    let report_path = Path::new(&args[2]);
    if report_path.exists() {
        return Err("report already exists; use a fresh measurement path".into());
    }
    let header_path = report_path.with_extension("header.borsh");
    let parts_path = report_path.with_extension("parts.bin");
    let node_report_path = report_path.with_extension("node.json");
    let open_start = Instant::now();
    let art = TirArtifactV1::open(Path::new(&args[0]))?;
    let class = art.class()?;
    let (artifact, _) = art.inventory_root()?;
    let class_id = class.class_id(&artifact);
    let open_seconds = open_start.elapsed().as_secs_f64();
    let calibration: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let prompt: Vec<u32> = calibration["sequences"][0]
        .as_array()
        .ok_or("calibration sequence missing")?
        .iter()
        .take(1)
        .map(|x| x.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or("invalid calibration token"))
        .collect::<Result<_, _>>()?;
    let mut ctx = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class_id, (prompt.len() as u32, 1))
        .ok_or("canonical context")?;
    let form = PalwPromptIdsFormV1::Flat;
    ctx.prompt_token_ids_hash = prompt_token_ids_commitment_v1(form, &prompt)?;
    let limits = RoleLimitsV2 { max_prefill: 16, max_decode: 16, max_leaves: 1 << 20, max_capture_bytes: 1 << 30 };
    let policy = ExactPolicyV2 {
        max_leaves: limits.max_leaves,
        max_part_bytes: 64 << 20,
        max_part_heap: 256 << 20,
        max_total_bytes: 4 << 30,
        max_elements_per_leaf: 1 << 25,
        max_terms_per_leaf: 1 << 31,
        max_total_elements: 1 << 56,
        max_total_terms: 1 << 60,
        max_commit_bytes: 16 << 20,
        deadline: 100,
    };
    let result = MinerResultV2 {
        version: 2,
        network: Hash64::from_u64_word(901),
        branch: Hash64::from_u64_word(902),
        ruleset: Hash64::from_u64_word(903),
        claim: Hash64::from_u64_word(904),
        job: ctx.clone(),
        artifact,
        output: Hash64::from_u64_word(0),
        sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
        service_policy: Hash64::from_u64_word(905),
        dependencies: Hash64::from_u64_word(906),
    };
    let miner_start = Instant::now();
    let (result, generated) = miner_result_v2(result, &class, art.plan(), art.params(), &prompt, form, limits)?;
    let miner_seconds = miner_start.elapsed().as_secs_f64();
    let space = PalwTirStepSpaceV1::new(&class)?;
    let runner = if art.plan().program.wide_logits_q24() {
        TirClassRunnerV1::new_wide_q24_v2(&space, art.plan(), art.params(), class_id)
    } else {
        TirClassRunnerV1::new(&space, art.plan(), art.params(), class_id)
    }?;
    let panel_start = Instant::now();
    let capture = PanelCaptureV2::replay(&runner, &class, artifact, &ctx, &prompt, form, limits)?;
    let panel_replay_seconds = panel_start.elapsed().as_secs_f64();
    if generated != capture.generated() {
        return Err("Miner/Panel replay differs".into());
    }
    let witness = capture.witness(&result, policy, 0)?;
    let miner_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
    let panel_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([82; 32]);
    let miner_signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&miner_key.signing_key, result.id().as_byte_slice(), RESULT_CONTEXT, [83; 32])?
            .as_ref()
            .to_vec();
    let panel_signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&panel_key.signing_key, witness.id().as_byte_slice(), WITNESS_CONTEXT, [84; 32])?
            .as_ref()
            .to_vec();
    let header = Header {
        result,
        witness,
        registered: class.clone(),
        registered_class_id: class_id,
        miner_key: miner_key.verification_key.as_ref().to_vec(),
        miner_signature,
        panel_key: panel_key.verification_key.as_ref().to_vec(),
        panel_signature,
        policy,
    };
    create(&header_path)?.write_all(&borsh::to_vec(&header)?)?;
    let mut writer = BufWriter::new(create(&parts_path)?);
    writer.write_all(b"PCPARTV2")?;
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: policy.max_leaves,
        prompt_form: form,
        limits: DemandLimits { max_elements: policy.max_elements_per_leaf, max_terms: policy.max_terms_per_leaf },
    };
    let build_start = Instant::now();
    let mut evidence_bytes = 0u64;
    let mut max_part_bytes = 0u64;
    capture.stream_parts(&runner, &art, &rules, policy.max_part_bytes, &mut |index, part| {
        let cone = borsh::to_vec(&part.cone).map_err(|e| e.to_string())?;
        let trace = part.trace.as_ref().map(borsh::to_vec).transpose().map_err(|e| e.to_string())?.unwrap_or_default();
        let bytes = (cone.len() + trace.len()) as u64;
        evidence_bytes =
            evidence_bytes.checked_add(bytes).filter(|n| *n <= policy.max_total_bytes).ok_or("measurement total evidence cap")?;
        max_part_bytes = max_part_bytes.max(bytes);
        writer
            .write_all(&(cone.len() as u64).to_le_bytes())
            .and_then(|_| writer.write_all(&(trace.len() as u64).to_le_bytes()))
            .and_then(|_| writer.write_all(&cone))
            .and_then(|_| writer.write_all(&trace))
            .map_err(|e| e.to_string())?;
        if index % 128 == 0 {
            eprintln!("Panel built {index}/{} leaves", header.witness.binding.step_leaf_count);
        }
        Ok(())
    })?;
    writer.flush()?;
    drop(writer);
    let panel_build_seconds = build_start.elapsed().as_secs_f64();
    let status = std::process::Command::new(std::env::current_exe()?)
        .arg("--node")
        .arg(&header_path)
        .arg(&parts_path)
        .arg(&node_report_path)
        .status()?;
    if !status.success() {
        return Err(format!("Node subprocess failed: {status}").into());
    }
    let node: serde_json::Value = serde_json::from_slice(&std::fs::read(&node_report_path)?)?;
    let report = serde_json::json!({ "artifact": args[0], "class_id": format!("{class_id}"), "artifact_root": format!("{artifact}"),
        "artifact_max_context": class.layout.max_context, "measured_prefill_tokens": prompt.len(), "measured_decode_tokens": 1,
        "generated": generated, "wide_q24": art.plan().program.wide_logits_q24(), "artifact_open_and_inventory_seconds": open_seconds,
        "miner_inference_seconds": miner_seconds, "miner_positive_evidence_built": false, "miner_intermediate_requests": 0,
        "panel_replay_seconds": panel_replay_seconds, "panel_evidence_build_seconds": panel_build_seconds,
        "evidence_bytes": evidence_bytes, "maximum_part_bytes": max_part_bytes, "miner_panel_process_peak_rss_bytes": peak_rss_bytes(),
        "node": node, "header_file": header_path, "parts_file": parts_path,
        "scope": "private fixed-length greedy EXACT_STREAM short-job measurement, not max-context qualification", "public_activation": false });
    create(report_path)?.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
