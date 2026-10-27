//! Prepare bounded honest fixed-count jobs for independent native drills.
//! Whole-model source fidelity, eligibility and on-chain Final are separate.
use misaka_palw_kernel::{
    flat_court5::flat_profile_commitment_v5, merkle3_stream::tensor_commitment_streamed_v3, trace::ParamCommitmentsV1,
};
use misaka_palw_sdk::real_checkpoint_fixture11::RealCheckpointExecutionFixtureV11;
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, node::TirArtifactV1};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Protocol {
    schema: String,
    artifact_sha256: String,
    program_sha256: String,
    prompt: Vec<u32>,
    generated_count: u32,
    max_positions: u32,
    max_artifact_bytes: u64,
    max_position_tensor_bytes: u64,
    max_fixture_bytes: usize,
}
fn hash(p: &Path) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mut h = Sha256::new();
    let mut f = std::fs::File::open(p)?;
    let mut b = vec![0; 1 << 20];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(h.finalize().into())
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
struct Sink {
    map: BTreeMap<(u8, Option<u16>), usize>,
    nodes: Vec<Vec<Option<misaka_palw_kernel::hash::Digest>>>,
    tensor_bytes: u64,
    cap: u64,
    error: Option<String>,
}
impl StepSink for Sink {
    fn every_node(&self) -> bool {
        true
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        let Some(total) = (v.data.len() as u64).checked_mul(16).and_then(|n| self.tensor_bytes.checked_add(n)) else {
            self.error = Some("position payload overflow".into());
            return;
        };
        self.tensor_bytes = total;
        if total > self.cap {
            self.error = Some("position payload cap".into());
            return;
        }
        let Some(&occ) = self.map.get(&(v.block, v.layer)) else {
            self.error = Some("unknown occurrence".into());
            return;
        };
        match flat_profile_commitment_v5(&v.to_tensor()) {
            Ok(root) => {
                if self.nodes[occ][v.node as usize].replace(root).is_some() {
                    self.error = Some("duplicate executed node".into());
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let total_started = std::time::Instant::now();
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.len() != 3 {
        return Err("usage: prepare-real-checkpoint-execution11 <artifact> <fixed protocol.json> <fresh fixture.borsh>".into());
    }
    let mut json = vec![];
    std::fs::File::open(&a[1])?.take((64 << 10) + 1).read_to_end(&mut json)?;
    if json.len() > 64 << 10 {
        return Err("protocol byte bound".into());
    }
    let protocol_hash: [u8; 32] = Sha256::digest(&json).into();
    let p: Protocol = serde_json::from_slice(&json)?;
    let path = Path::new(&a[0]);
    let positions =
        (p.prompt.len() as u32).checked_add(p.generated_count).and_then(|v| v.checked_sub(1)).ok_or("position overflow")?;
    if p.schema != "misaka.rfc02.actual-checkpoint-fixed-job-protocol.v1"
        || p.prompt.is_empty()
        || p.prompt.len() > 4096
        || !(1..=128).contains(&p.generated_count)
        || positions > p.max_positions
        || p.max_positions > 4096
        || p.max_artifact_bytes == 0
        || p.max_artifact_bytes > 32 << 30
        || p.max_position_tensor_bytes == 0
        || p.max_position_tensor_bytes > 256 << 20
        || p.max_fixture_bytes == 0
        || p.max_fixture_bytes > 64 << 20
        || std::fs::metadata(path)?.len() > p.max_artifact_bytes
        || Path::new(&a[2]).exists()
    {
        return Err("fixed job preallocation bounds".into());
    }
    let artifact_hash = hash(path)?;
    if hex(&artifact_hash) != p.artifact_sha256 {
        return Err("artifact byte identity".into());
    }
    let artifact = TirArtifactV1::open(path)?;
    let program = &artifact.plan().program;
    if hex(&Sha256::digest(program.encode())) != p.program_sha256 || p.prompt.iter().any(|t| *t >= program.token_bound) {
        return Err("canonical program or token identity".into());
    }
    let scope = misaka_palw_kernel::flat_scope9::flat_resource_scope_v9(program, positions, None).map_err(|e| format!("{e:?}"))?;
    if scope.reference_tensor_bytes_per_position > p.max_position_tensor_bytes as u128 {
        return Err("derived per-position payload cap".into());
    }
    let occurrences = program.occurrences();
    let node_count: usize = occurrences.iter().map(|(b, _)| program.blocks[*b as usize].nodes.len()).sum();
    let retained =
        node_count.checked_mul(positions as usize).and_then(|v| v.checked_mul(64)).ok_or("commitment directory overflow")?;
    if retained > p.max_fixture_bytes {
        return Err("commitment directory preallocation cap".into());
    }
    let mut pc = ParamCommitmentsV1::default();
    for &(j, layer) in &artifact.plan().param_instances {
        let d = &program.params[j as usize];
        let source = artifact.params().get(j, layer).ok_or("missing mapped parameter")?;
        let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
        let width = d.dtype.width();
        let (root, _) = tensor_commitment_streamed_v3(d.dtype, &shape, 32 << 20, &mut |offset, out| {
            if !(offset as usize).is_multiple_of(width) || !out.len().is_multiple_of(width) {
                return Err("parameter alignment".into());
            }
            let start = offset as usize / width;
            let count = out.len() / width;
            misaka_palw_tir_exec::with_slice!(source,v=>{for(x,b)in v.get(start..start+count).ok_or("parameter range")?.iter().zip(out.chunks_exact_mut(width)){b.copy_from_slice(&x.to_le_bytes());}});
            Ok(())
        })?;
        pc.by_instance.insert((j, layer), root);
    }
    let descriptor = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
    let inventory = kaspa_consensus_core::palw_tir_artifact_v1::PalwTirModelInventoryV2::new(&descriptor, program)?;
    let inventory_quote = misaka_palw_sdk::flat_streamed_locator_v9::mapped_model_inventory_quote_v9(program)?;
    if inventory_quote.raw_bytes > p.max_artifact_bytes as u128
        || inventory_quote.leaves > 16 << 20
        || inventory_quote.workspace_bytes > 64 << 20
    {
        return Err("descriptor-scoped model inventory preallocation cap".into());
    }
    let (model_root, model_leaves) = misaka_palw_sdk::tir_stream::palw_tir_model_inventory_root_streamed_v2(
        &inventory,
        &misaka_palw_sdk::tir_stream::ContainerRanges::open(artifact.container())?,
    )?;
    let map: BTreeMap<_, _> = occurrences.iter().copied().enumerate().map(|(i, k)| (k, i)).collect();
    if map.len() != occurrences.len() {
        return Err("ambiguous occurrence coordinates".into());
    }
    let mut ex = TirExecutor::new(artifact.plan(), artifact.params())?;
    let mut commitments = vec![];
    let started = std::time::Instant::now();
    let mut run = |ex: &mut TirExecutor<'_>, token| -> Result<(), Box<dyn std::error::Error>> {
        let mut sink = Sink {
            map: map.clone(),
            nodes: occurrences.iter().map(|(b, _)| vec![None; program.blocks[*b as usize].nodes.len()]).collect(),
            tensor_bytes: 0,
            cap: p.max_position_tensor_bytes,
            error: None,
        };
        ex.step(token, &mut sink)?;
        if let Some(e) = sink.error {
            return Err(e.into());
        }
        commitments.push(
            sink.nodes
                .into_iter()
                .map(|nodes| nodes.into_iter().map(|r| r.ok_or("missing executed node")).collect::<Result<Vec<_>, _>>())
                .collect::<Result<Vec<_>, _>>()?,
        );
        Ok(())
    };
    for token in &p.prompt {
        run(&mut ex, *token)?;
    }
    let mut generated = vec![];
    for i in 0..p.generated_count {
        let (_, logits) = ex.logits();
        let mut best = 0;
        for j in 1..logits.len() {
            if logits.get(j) > logits.get(best) {
                best = j;
            }
        }
        generated.push(best as u32);
        if i + 1 < p.generated_count {
            run(&mut ex, best as u32)?;
        }
    }
    if commitments.len() != positions as usize || hash(path)? != artifact_hash || hash(Path::new(&a[1]))? != protocol_hash {
        return Err("position completeness or changed artifact/protocol".into());
    }
    let fixture = RealCheckpointExecutionFixtureV11 {
        version: 11,
        artifact_sha256: artifact_hash,
        protocol_sha256: protocol_hash,
        program_bytes: program.encode(),
        params: pc,
        model_inventory_root: model_root,
        prompt: p.prompt,
        generated,
        commitments,
    };
    let bytes = borsh::to_vec(&fixture)?;
    if bytes.len() > p.max_fixture_bytes {
        return Err("fixture wire byte cap".into());
    }
    use std::io::Write;
    std::fs::OpenOptions::new().write(true).create_new(true).open(&a[2])?.write_all(&bytes)?;
    let mut rss: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut rss) } != 0 {
        return Err("peak RSS unavailable".into());
    }
    let peak_rss = rss.ru_maxrss as u64 * if cfg!(target_os = "macos") { 1 } else { 1024 };
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.actual-checkpoint-fixed-job-observation.v1", "protocol_sha256":hex(&protocol_hash),
        "artifact_sha256":hex(&artifact_hash),"program_sha256":p.program_sha256,"model_inventory_root":model_root.to_string(),
        "parameter_root":hex(&fixture.params.root()),"parameter_instances":fixture.params.by_instance.len(),"prompt":fixture.prompt,
        "generated":fixture.generated,"positions":positions,"whole_graph_node_commitments":node_count*positions as usize,
        "fixture_bytes":bytes.len(),"fixture_sha256":hex(&Sha256::digest(&bytes)),"execution_ms":started.elapsed().as_millis(),
        "total_ms":total_started.elapsed().as_millis(),"whole_process_peak_rss_bytes":peak_rss,
        "model_inventory_version":2,"model_inventory_leaves":model_leaves,
        "model_inventory_raw_bytes":inventory_quote.raw_bytes,"model_inventory_workspace_bound":inventory_quote.workspace_bytes,
        "aggregate_retained_tensor_values":false,"per_position_tensor_payload_cap":p.max_position_tensor_bytes,
        "fixed_count_including_after_eos":true,"native_final_or_source_fidelity_or_a6_a8_proven":false})
    );
    Ok(())
}
