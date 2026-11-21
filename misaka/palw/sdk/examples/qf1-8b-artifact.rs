//! Materialize the isolated fixed-scale QF1 conversion in the native PALWTIR1 format.
//! A replay uses the node reader and local QF1 instance, with a private memory attribution.
use kaspa_consensus_core::Hash64;
use kaspa_hashes::blake2b_512_keyed;
use misaka_palw_tir_artifact::{PALW_TIR_CONTAINER_MAGIC_V1, container_header_v1};
use misaka_palw_tir_exec::node::TirArtifactV1;
use misaka_palw_tir_lower::qf1::visit_fixed_scale_weights;
use serde_json::json;
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::{self, Seek, SeekFrom, Write},
    path::Path,
};

use misaka_palw_sdk::qf1_local::{MANIFEST, TOKENIZER, bind, layout};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn pack(inventory: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err("destination already exists".into());
    }
    let model = misaka_palw_sdk::qf1_local::program()?;
    let tokenizer: Hash64 = TOKENIZER.parse()?;
    let header =
        container_header_v1(&model.program, borsh::to_vec(&layout(&model.program))?, tokenizer.as_bytes(), MANIFEST.to_owned())?;
    let mut pending: BTreeMap<_, _> =
        header.tensors.iter().map(|e| ((model.program.params[usize::from(e.param)].name.clone(), e.layer), e)).collect();
    let hb = borsh::to_vec(&header)?;
    let parent = dest.parent().filter(|x| !x.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(PALW_TIR_CONTAINER_MAGIC_V1)?;
    tmp.write_all(&header.version.to_le_bytes())?;
    tmp.write_all(&u32::try_from(hb.len())?.to_le_bytes())?;
    tmp.write_all(&hb)?;
    visit_fixed_scale_weights(&mut io::stdin().lock(), &fs::read(inventory)?, |name, layer, bytes| {
        let e = pending.remove(&(name.to_owned(), layer)).ok_or("unknown or duplicate graph parameter")?;
        if e.bytes != bytes.len() as u64 {
            return Err("tensor length differs from registered shape".into());
        }
        tmp.seek(SeekFrom::Start(e.offset))?;
        tmp.write_all(&bytes)?;
        Ok(())
    })?;
    if !pending.is_empty() {
        return Err("missing graph parameter".into());
    }
    tmp.as_file().sync_all()?;
    // Validate the normal reader before making the verified complete file visible.
    let a = TirArtifactV1::open(tmp.path())?;
    bind(&a)?;
    drop(a);
    tmp.persist_noclobber(dest)?;
    Ok(())
}

fn replay(path: &Path, input: &Path, limit: u32, cap: u64) -> Result<()> {
    let a = TirArtifactV1::open(path)?;
    let local = bind(&a)?;
    let class = a.class()?;
    let ids: Vec<u32> = serde_json::from_slice(&fs::read(input)?)?;
    let id = local.identity();
    let answer =
        local.replay(&ids, limit, cap, blake2b_512_keyed(b"MISAKA/private/QF1/replay-job/v1", id.class.as_byte_slice()), [0; 32])?;
    let (_, leaves) = a.inventory_root()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "artifact_bytes":fs::metadata(path)?.len(),"parameter_instances":a.header().tensors.len(),
            "artifact_root":id.inventory.to_string(),"inventory_leaves":leaves,"class":id.class.to_string(),
            "graph_ir_root":class.graph_ir_root().to_string(),"layout_digest":class.layout_digest().to_string(),
            "tokenizer":id.tokenizer.to_string(),"local_descriptor":id.descriptor.to_string(),
            "input_positions":ids.len(),"processed_positions":answer.processed_positions,"output_ids":answer.tokens,
            "stop":format!("{:?}",answer.stop),"readout_z":Hash64::from_bytes(answer.z).to_string(),
        }))?
    );
    Ok(())
}

fn tiles(parent: &str, section: &str, job: Option<(&str, u32)>) -> Result<()> {
    use kaspa_consensus_core::palw_qf1::{CONTEXT, VOCAB, is_eos};
    use misaka_palw_sdk::qf1_local::residual_tiles::Candidate;
    use misaka_palw_tir_exec::{NoSink, Slice, TirExecutor};
    use sha2::{Digest, Sha256};
    let parent = TirArtifactV1::open(Path::new(parent))?;
    let candidate = Candidate::over(&parent)?;
    let section = Path::new(section);
    if job.is_none() {
        candidate.write(&parent, section)?;
    }
    let artifact = candidate.open(&parent, section)?;
    let id = candidate.bind(&artifact)?.identity();
    let reference = artifact.composite_ref().unwrap();
    let mut report = json!({"class":id.class,"parent":reference.parent_class,"inventory":id.inventory,"descriptor":id.descriptor,
        "adapter_root":reference.adapter_root,"section_bytes":fs::metadata(section)?.len(),
        "program_sha256":format!("{:x}",Sha256::digest(&artifact.class()?.program))});
    if job.is_none() {
        use kaspa_consensus_core::{
            config::params::{PALW_T12_MODEL_PANEL_ONE_FENCE_DAA_V1, palw_t12_shipped_params},
            palw_improve_composite_v1::{PalwTirCompositeAdmissionV1, palw_tir_composite_admits_v1},
            palw_mode_v2::PalwConsensusMode,
            palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1,
            palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        };
        let params = palw_t12_shipped_params();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { return Err("T12 has no court".into()) };
        let daa = PALW_T12_MODEL_PANEL_ONE_FENCE_DAA_V1;
        let rules = PalwTirCompositeAdmissionV1 {
            court: params.palw_kary_court_active_at(daa),
            carriable: palw_tir_carriable_close_bytes_v1(&bundle.court),
            work_cap: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        };
        let sizes = palw_tir_composite_admits_v1(
            &parent.class()?,
            &reference.parent_class,
            &reference.parent_root,
            &artifact.class()?,
            &id.class,
            &id.inventory,
            reference,
            &rules,
        );
        report["composite_court"] = match sizes {
            Ok(bounds) => {
                json!({"daa":daa,"carriable":rules.carriable,"bounds":bounds.len(),"max_close":bounds.iter().map(|b|b.close_bytes).max(),
                "max_root_claim":bounds.iter().map(|b|b.root_claim_bytes).max()})
            }
            Err(reason) => json!({"daa":daa,"carriable":rules.carriable,"refusal":reason.to_string()}),
        };
    }
    if let Some((input, limit)) = job {
        let ids: Vec<u32> = serde_json::from_slice(&fs::read(input)?)?;
        if ids.is_empty()
            || !(1..=1024).contains(&limit)
            || ids.len() + limit as usize - 1 > CONTEXT as usize
            || ids.iter().any(|&t| t >= VOCAB)
        {
            return Err("input exceeds QF1 bounds".into());
        }
        let mut engine = TirExecutor::new(artifact.plan(), artifact.params())?;
        engine.set_fused(true);
        let mut sink = NoSink;
        for &token in &ids[..ids.len() - 1] {
            engine.step_opt(token, &mut sink, false)?;
        }
        let mut token = *ids.last().unwrap();
        let mut answers = Vec::new();
        let mut hashes = Vec::new();
        for _ in 0..limit {
            engine.step_opt(token, &mut sink, true)?;
            let (_, Slice::I32(logits)) = engine.logits() else { return Err("missing i32 logits".into()) };
            if logits.len() != VOCAB as usize {
                return Err("vocabulary mismatch".into());
            }
            let max = logits.iter().max().ok_or("empty logits")?;
            token = logits.iter().position(|x| x == max).unwrap() as u32;
            hashes.push(format!("{:x}", Sha256::digest(logits.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())));
            answers.push(token);
            if is_eos(token) {
                break;
            }
        }
        artifact.check_weights_unmodified()?;
        report["output_ids"] = json!(answers);
        report["logits_sha256"] = json!(hashes);
        report["input_ids"] = json!(ids);
        report["stop"] = json!(if is_eos(token) { "Eos" } else { "Limit" });
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn full_tiles(parent: &str, section: &str, export: bool) -> Result<()> {
    use kaspa_consensus_core::{
        config::params::{PALW_T12_MODEL_PANEL_ONE_FENCE_DAA_V1, palw_t12_shipped_params},
        palw_mode_v2::PalwConsensusMode,
        palw_tir_admission_v1::{palw_tir_carriable_close_bytes_v1, palw_tir_carried_closes_admit_v1},
        palw_tir_attempt_v1::palw_tir_canonical_context_v1,
        palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        palw_tir_fence2_v1::palw_tir_close_twin_v1,
        palw_tir_step_v1::PalwTirStepSpaceV1,
    };
    use misaka_palw_sdk::qf1_local::residual_tiles::Candidate;
    let parent = TirArtifactV1::open(Path::new(parent))?;
    let candidate = Candidate::over(&parent)?;
    let source = candidate.open(&parent, Path::new(section))?;
    if export {
        return Ok(candidate.write_full(&source, io::BufWriter::new(io::stdout().lock()))?);
    }
    let (id, root) = candidate.full_identity(&source)?;
    let class = source.class()?;
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { return Err("T12 has no court".into()) };
    let daa = PALW_T12_MODEL_PANEL_ONE_FENCE_DAA_V1;
    let space = PalwTirStepSpaceV1::new(&class)?;
    let job = palw_tir_canonical_context_v1(&class, id, (1, class.layout.max_context)).ok_or("invalid QF1 context")?;
    let limit = palw_tir_carriable_close_bytes_v1(&bundle.court);
    let bounds = palw_tir_carried_closes_admit_v1(
        &space,
        &space.program,
        &job,
        params.palw_kary_court_active_at(daa),
        limit,
        PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        palw_tir_close_twin_v1(params.palw_tir_demand_rules_at(daa)),
    );
    let sizing = match bounds {
        Ok(bounds) => json!({"bounds":bounds.len(),"max_close":bounds.iter().map(|b|b.close_bytes).max(),
            "max_root_claim":bounds.iter().map(|b|b.root_claim_bytes).max()}),
        Err(reason) => json!({"refusal":reason.to_string()}),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"class":id,"inventory":root,"context":class.layout.max_context,
        "daa":daa,"carriable":limit,"full_court":sizing}))?
    );
    Ok(())
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [cmd, parent, section] if cmd == "full-tiles" || cmd == "export-tiles" => full_tiles(parent, section, cmd == "export-tiles"),
        [cmd, inventory, file] if cmd == "pack" => pack(Path::new(inventory), Path::new(file)),
        [cmd, parent, section] if cmd == "pack-tiles" => tiles(parent, section, None),
        [cmd, parent, section, input, limit] if cmd == "replay-tiles" => tiles(parent, section, Some((input, limit.parse()?))),
        [cmd, file, ids, limit, cap] if cmd == "replay" => replay(Path::new(file), Path::new(ids), limit.parse()?, cap.parse()?),
        _ => Err(concat!(
            "usage: qf1-8b-artifact pack <inventory.json> <new.palwtir> | ",
            "replay <model.palwtir> <input-ids.json> <decode-limit> <leaf-budget> | ",
            "pack-tiles <parent> <new-section> | replay-tiles <parent> <section> <input-ids.json> <decode-limit> | ",
            "full-tiles <parent> <section> | export-tiles <parent> <section> (full binary to stdout)"
        )
        .into()),
    }
}
