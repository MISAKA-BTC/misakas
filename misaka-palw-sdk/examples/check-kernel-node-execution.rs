//! Generic real-artifact diagnostic: authenticate all decoder parameter instances against a
//! prepared v3 map, then compare the node's typed executor with the integer reference at every
//! commit point and logits for exactly the requested positions. No ModelSpec is consulted.
//! This is local execution evidence, not node registration, public conviction, fidelity or Final.
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::{ParamSource, Tensor};
use misaka_palw_tir_exec::node::{LazyContainerParams, TirArtifactV1, tir_conformance_prompt_v1, tir_executor_conformance_against_v1};
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 6 {
        return Err("usage: check-kernel-node-execution <artifact> <params.borsh> <positions> <hash-workspace MiB> <payload MiB> <reference-tensor MiB>".into());
    }
    let mib = |s: &str| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(s.parse::<u64>()?.checked_mul(1 << 20).filter(|n| *n > 0).ok_or("invalid MiB cap")?)
    };
    let positions: usize = a[2].parse()?;
    let (hash_limit, payload_limit, reference_limit) = (mib(&a[3])?, mib(&a[4])?, mib(&a[5])?);
    let path = std::path::Path::new(&a[0]);
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(path)?;
    if positions == 0 || positions > container.program.history_bound as usize {
        return Err("requested positions outside program history bound".into());
    }
    // Largest raw tensor plus its i128 decoding, including a small allocation allowance.
    // This is not total executor/verifier/process RAM; actual process RSS is measured separately.
    let mut reference_bound = 0;
    for d in &container.program.params {
        let elements = d.shape.iter().try_fold(1u64, |n, v| n.checked_mul(*v as u64)).ok_or("shape overflow")?;
        let bound =
            elements.checked_mul(16 + d.dtype.width() as u64).and_then(|n| n.checked_add(1 << 20)).ok_or("tensor bound overflow")?;
        reference_bound = reference_bound.max(bound);
    }
    if reference_bound > reference_limit {
        return Err(format!("reference tensor bound {reference_bound} > limit {reference_limit}").into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&a[1])?.take((64 << 20) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 << 20 {
        return Err("prepared map exceeds 64 MiB parse limit".into());
    }
    let expected: ParamCommitmentsV1 = borsh::from_slice(&bytes)?;
    let d = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
    let prepared =
        misaka_palw_sdk::kernel_params::prepare_kernel_params_file_v3(&d, path, hash_limit, payload_limit, &mut |j, l, _| {
            eprintln!("authenticate param {j} layer {l:?}")
        })?;
    if prepared.params != expected {
        return Err("actual artifact does not match the prepared instance commitments".into());
    }
    struct Reference<'a>(LazyContainerParams<'a>);
    impl ParamSource for Reference<'_> {
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            eprintln!("reference read {}: param {j} layer {l:?}", self.0.reads() + 1);
            self.0.param(j, l)
        }
    }
    let artifact = TirArtifactV1::open(path)?;
    if artifact.container().header.program != container.header.program {
        return Err("artifact program changed before execution".into());
    }
    let reference = Reference(LazyContainerParams::new(artifact.container()));
    let started = std::time::Instant::now();
    let (actual_positions, commits) = tir_executor_conformance_against_v1(&artifact, &reference, positions)?;
    let hex = misaka_palw_sdk::runtime_pack::commit::hex;
    println!(
        "{}",
        serde_json::json!({
            "schema": "misaka.palw.kernel-node-execution-reference.v1",
            "descriptor_digest": hex(&prepared.descriptor_digest), "program_root": hex(&prepared.program_root),
            "param_root": hex(&prepared.params.root()), "instances_authenticated": prepared.params.by_instance.len(),
            "positions_requested": positions, "positions_compared": actual_positions,
            "tokens": tir_conformance_prompt_v1(container.program.token_bound, positions),
            "commits_compared": commits, "all_compared_commits_and_logits_match": true,
            "reference_param_reads": reference.0.reads(), "reference_peak_decoded_tensor_bytes": reference.0.peak_tensor_bytes(),
            "reference_tensor_bound_bytes": reference_bound, "reference_tensor_limit_bytes": reference_limit,
            "hash_workspace_limit_bytes": hash_limit, "payload_limit_bytes": payload_limit,
            "execution_comparison_elapsed_ms": started.elapsed().as_millis(),
            "registration_public_conviction_fidelity_or_final_proven": false
        })
    );
    Ok(())
}
