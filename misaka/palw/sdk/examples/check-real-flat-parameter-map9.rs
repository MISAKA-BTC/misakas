//! Independently authenticate every pinned integer-artifact parameter instance
//! against its v3 registered commitment, with raw-width streaming buffers.
//! This local source-continuity check grants no native registration or Final.
use misaka_palw_kernel::{merkle3_stream::*, trace::ParamCommitmentsV1};
use misaka_palw_tir_exec::node::TirArtifactV1;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Read, path::Path};
const WORKSPACE: u64 = 32 << 20;
fn hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut b = vec![0; 1 << 20];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn read(path: &Path, cap: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut b = vec![];
    std::fs::File::open(path)?.take(cap + 1).read_to_end(&mut b)?;
    if b.len() as u64 > cap {
        return Err("metadata cap".into());
    }
    Ok(b)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: check-real-flat-parameter-map9 <artifact> <param-commitments.borsh> <source policy.json>".into());
    }
    let started = std::time::Instant::now();
    let policy: serde_json::Value = serde_json::from_slice(&read(Path::new(&args[2]), 64 << 10)?)?;
    if policy["schema"] != "misaka.rfc02.independent-source-fidelity-protocol.v1" {
        return Err("source policy".into());
    }
    let pin = policy["artifact_sha256"].as_str().ok_or("artifact SHA")?;
    let path = Path::new(&args[0]);
    if hash(path)? != pin {
        return Err("artifact bytes before run".into());
    }
    let raw = read(Path::new(&args[1]), 64 << 20)?;
    let pcsha = Sha256::digest(&raw).iter().map(|b| format!("{b:02x}")).collect::<String>();
    if pcsha != "a45721593036993c53eb29ad1526906ca190fcdf7d7272fbf7aa3f0b62d19ef1" {
        return Err("pinned Qwen PC map".into());
    }
    let pc: ParamCommitmentsV1 = borsh::from_slice(&raw)?;
    let artifact = TirArtifactV1::open(path)?;
    let program = &artifact.plan().program;
    let psha = Sha256::digest(program.encode()).iter().map(|b| format!("{b:02x}")).collect::<String>();
    if psha != "16a0de98fe52cdf47364392551b15bb85a7aad76acd8b14853b1803074bca8f1" {
        return Err("pinned canonical Qwen program".into());
    }
    if artifact.plan().param_instances.iter().copied().collect::<BTreeSet<_>>() != pc.by_instance.keys().copied().collect() {
        return Err("exact parameter set".into());
    }
    let (mut expected_bytes, mut maximum_workspace) = (0u128, 0u64);
    for &(j, _) in &artifact.plan().param_instances {
        let d = &program.params[j as usize];
        let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
        let b = streamed_commitment_shape_v3(d.dtype, &shape)?;
        if b.workspace_bytes > WORKSPACE {
            return Err("workspace preflight before any parameter payload".into());
        }
        expected_bytes += b.tensor_bytes as u128;
        maximum_workspace = maximum_workspace.max(b.workspace_bytes);
    }
    let (mut actual_bytes, mut instances) = (0u128, 0usize);
    for &(j, layer) in &artifact.plan().param_instances {
        let d = &program.params[j as usize];
        let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
        let source = artifact.params().get(j, layer).ok_or("mapped parameter missing")?;
        let width = d.dtype.width();
        let (root, _) = tensor_commitment_streamed_v3(d.dtype, &shape, WORKSPACE, &mut |offset, out| {
            if offset as usize % width != 0 || out.len() % width != 0 {
                return Err("raw range alignment".into());
            }
            let start = usize::try_from(offset).map_err(|e| e.to_string())? / width;
            let count = out.len() / width;
            misaka_palw_tir_exec::with_slice!(source, values => {let part=values.get(start..start+count).ok_or("mapped parameter raw range")?;for(v,dst) in part.iter().zip(out.chunks_exact_mut(width)){dst.copy_from_slice(&v.to_le_bytes());}});
            actual_bytes += out.len() as u128;
            Ok(())
        })?;
        if pc.by_instance.get(&(j, layer)) != Some(&root) {
            return Err(format!("actual source root mismatch at {j}/{layer:?}").into());
        }
        instances += 1;
    }
    if actual_bytes != expected_bytes {
        return Err("parameter payload accounting".into());
    }
    if hash(path)? != pin {
        return Err("artifact bytes after run".into());
    }
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.actual-checkpoint-parameter-source-continuity.v1","artifact_sha256":pin,"canonical_program_sha256":psha,"parameter_map_sha256":pcsha,"independent_registered_parameter_root":misaka_palw_sdk::runtime_pack::commit::hex(&pc.root()),"parameter_instances":instances,"raw_parameter_bytes_read":actual_bytes,"declared_stream_workspace_limit_bytes":WORKSPACE,"maximum_derived_stream_workspace_bytes":maximum_workspace,"each_parameter_instance_compared_against_registered_root":true,"source_artifact_sha256_before_after_match":true,"source_dtype_semantics":"integer artifact storage, not original BF16 source bit equality","elapsed_ms":started.elapsed().as_millis(),"raw_source_float_fidelity_this_run":false,"native_registration_or_execution_final_or_a6_a8_credit":false,"full_rfc02_complete":false})
    );
    Ok(())
}
