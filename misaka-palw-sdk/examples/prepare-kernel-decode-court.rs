//! Actual-weight native trace into a small terminal-court reproduction bundle. No ModelSpec.
//! Run a fixed full prompt and one Greedy output. It proves a substituted delivered token
//! against actual trace roots; it does not prove computation integrity, public acquisition,
//! source-model fidelity, chain registration or Final. Only the final position is captured.
use misaka_palw_sdk::kernel_execution::{DecodeCourtBundleV3, NativeTraceLimitsV3, decode_fault_from_logits_v3, native_trace_v3};
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 7 {
        return Err("usage: prepare-kernel-decode-court <artifact> <params.borsh> <positions> <hash MiB> <trace MiB> <payload MiB> <bundle.borsh>".into());
    }
    let mib = |s: &str| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(s.parse::<u64>()?.checked_mul(1 << 20).filter(|n| *n > 0).ok_or("invalid MiB limit")?)
    };
    let count: u32 = a[2].parse()?;
    let (hash_limit, trace_limit, payload_limit) = (mib(&a[3])?, mib(&a[4])?, mib(&a[5])?);
    let input = std::path::Path::new(&a[0]);
    let out = std::path::Path::new(&a[6]);
    if [input, std::path::Path::new(&a[1])]
        .iter()
        .any(|source| *source == out || source.canonicalize().ok().is_some_and(|p| out.canonicalize().ok() == Some(p)))
    {
        return Err("bundle output must not replace an input file".into());
    }
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(input)?;
    if count == 0 || count > container.program.history_bound {
        return Err("requested positions outside program bound".into());
    }
    let d = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
    let root = misaka_palw_kernel::public::program_root_v1(&container.header.program);
    let plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &container.program, root, count).map_err(|(_, e)| e)?;
    let mut bytes = Vec::new();
    std::fs::File::open(&a[1])?.take((64 << 20) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 << 20 {
        return Err("prepared map exceeds parse cap".into());
    }
    let params: misaka_palw_kernel::trace::ParamCommitmentsV1 = borsh::from_slice(&bytes)?;
    let prepared =
        misaka_palw_sdk::kernel_params::prepare_kernel_params_file_v3(&d, input, hash_limit, payload_limit, &mut |j, l, _| {
            eprintln!("authenticate param {j}, layer {l:?}")
        })?;
    if prepared.params != params || prepared.program_root != root {
        return Err("not the prepared program/parameter map".into());
    }
    let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(input)?;
    if artifact.header().program != container.header.program {
        return Err("program changed before execution".into());
    }
    eprintln!("compute actual inventory root");
    let (inventory_root, inventory_leaves) = artifact.inventory_root()?;
    let tokens = misaka_palw_tir_exec::node::tir_conformance_prompt_v1(container.program.token_bound, count as usize);
    let started = std::time::Instant::now();
    let mut last = None;
    let trace = native_trace_v3(
        &artifact,
        &d,
        &plan,
        &tokens,
        NativeTraceLimitsV3 { tensor_hash_workspace_bytes: hash_limit, trace_workspace_bytes: trace_limit },
        &|p| p + 1 == count,
        &mut |p| {
            eprintln!(
                "native position {}: all {} node commitments complete",
                p.position,
                p.commitments.iter().map(Vec::len).sum::<usize>()
            );
            if p.position + 1 == count {
                last = Some(p);
            }
            Ok(())
        },
    )?;
    let elapsed = started.elapsed().as_millis();
    let last = last.ok_or("no final position")?;
    let post = last.commitments.len() - 1;
    let logits = &last.values.as_ref().ok_or("no final capture")?[post][container.program.logits as usize];
    let best = misaka_palw_kernel::job::DecodeRuleV1::Greedy.select(logits).ok_or("no Greedy token")?;
    if logits.len() < 2 {
        return Err("delivery-lie diagnostic needs at least two possible outputs".into());
    }
    let wrong = (best + 1) % logits.len() as u32;
    let honest = [best];
    let delivered = [wrong];
    let opening = misaka_palw_kernel::seg::position_node_opening_v1(
        count - 1,
        &last.commitments,
        misaka_palw_kernel::seg::position_path_of_roots_v1(&trace.position_roots, count - 1).ok_or("no position path")?.1,
        post as u16,
        container.program.logits,
    )
    .ok_or("no logits opening")?;
    let mut context = misaka_palw_kernel::element::SegClaimContextV1 {
        program: &container.program,
        params: &params,
        segment_roots: &trace.segment_roots,
        positions: count,
        prompt_len: count,
        prompt_root: misaka_palw_kernel::seg::prompt_root_of_ids_v1(&tokens),
        inline_prompt: Some(&tokens),
        generated: &honest,
        decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
        encoder: None,
    };
    if decode_fault_from_logits_v3(&context, &opening, logits, 0)?.is_some() {
        return Err("honest output convicted".into());
    }
    context.generated = &delivered;
    let fault = decode_fault_from_logits_v3(&context, &opening, logits, 0)?.ok_or("substituted output not convicted")?;
    let fault_bytes = fault.to_bytes().len();
    context.generated = &honest;
    let mut honest_candidate = fault.clone();
    if let misaka_palw_kernel::element::SegFaultV1::Decode(candidate) = &mut honest_candidate {
        candidate.rival = wrong;
    }
    if misaka_palw_kernel::element::verify_seg_fault_v1(&context, &honest_candidate)
        != Err(misaka_palw_kernel::verify::DismissalV1::NoFault)
    {
        return Err("authenticated weaker-rival proof did not dismiss honest delivery".into());
    }
    let withheld = misaka_palw_kernel::seg_scope::seg_withheld_mask_v1(&container.program)[post][container.program.logits as usize];
    let bundle = DecodeCourtBundleV3 {
        version: 1,
        inventory_root: inventory_root.as_bytes(),
        program_bytes: container.header.program.clone(),
        plan,
        params: params.clone(),
        position_roots: trace.position_roots.clone(),
        tokens,
        honest_generated: honest.to_vec(),
        wrong_generated: delivered.to_vec(),
        fault,
    };
    let wire = borsh::to_vec(&bundle)?;
    misaka_palw_sdk::runtime_pack::beacon_run::write_atomic(out, &wire)?;
    let hex = misaka_palw_sdk::runtime_pack::commit::hex;
    println!(
        "{}",
        serde_json::json!({
            "schema":"misaka.palw.native-decode-court.v3","positions":count,
            "program_root":hex(&root),"descriptor_digest":hex(&d.digest()),"plan_root":hex(&trace.plan_root),
            "param_root":hex(&params.root()),"inventory_root":hex(inventory_root.as_byte_slice()),"inventory_leaves":inventory_leaves,
            "claim_root":hex(&trace.claim_root),"segment_roots":trace.segment_roots.iter().map(|v|hex(v)).collect::<Vec<_>>(),
            "native_trace_elapsed_ms":elapsed,"trace_workspace_bound":trace.trace_workspace_bound,"max_tensor_hash_workspace":trace.max_tensor_hash_workspace,
            "trace_workspace_limit":trace_limit,"hash_workspace_limit":hash_limit,"payload_limit":payload_limit,
            "honest_token":best,"wrong_token":wrong,"honest_dismissed":true,"substituted_delivery_convicted":true,
            "proof_bytes":fault_bytes,"bundle_bytes":wire.len(),"logits_normally_withheld":withheld,
            "private_values_in_bundle":false,"registration_public_acquisition_computation_integrity_fidelity_final_proven":false
        })
    );
    Ok(())
}
