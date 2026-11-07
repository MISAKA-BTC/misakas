//! Research diagnostic: compare prepared v3 commitments to decoded hashing of EVERY real tensor.
//! This deliberately needs the largest tensor's i128 expansion, unlike kernel-params. The explicit
//! memory cap is checked over all instances first. This grants no registration, activation or Final.
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir_artifact::PalwTirContainerV1;
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 3 {
        return Err("usage: check-kernel-params-reference <artifact> <params.borsh> <positive reference-workspace MiB>".into());
    }
    let limit: u64 = a[2].parse::<u64>()?.checked_mul(1 << 20).filter(|n| *n > 0).ok_or("invalid memory cap")?;
    let c = PalwTirContainerV1::open(std::path::Path::new(&a[0]))?;
    let mut bytes = Vec::new();
    std::fs::File::open(&a[1])?.take((64 << 20) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 << 20 {
        return Err("prepared commitment file exceeds 64 MiB parse cap".into());
    }
    let prepared: ParamCommitmentsV1 = borsh::from_slice(&bytes)?;
    let expected = misaka_palw_tir_artifact::param_instances_v1(&c.program)
        .into_iter()
        .enumerate()
        .flat_map(|(j, layers)| layers.into_iter().map(move |l| (j as u16, l)))
        .collect::<Vec<_>>();
    if prepared.by_instance.keys().copied().collect::<Vec<_>>() != expected {
        return Err("not the decoder's exact param-instance inventory".into());
    }
    let mut peak = 0;
    let mut payload = 0;
    for entry in &c.header.tensors {
        let d = &c.program.params[entry.param as usize];
        let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
        let l = misaka_palw_kernel::merkle3::LayoutV3::try_of(&shape).ok_or("shape overflow")?;
        // Raw bytes + the decoded tensor + conservatively three arrays of all leaf hashes.
        let bound = entry
            .bytes
            .checked_add(l.len.checked_mul(16).ok_or("decoded size overflow")?)
            .and_then(|v| v.checked_add((l.leaves(0) + l.leaves(1)) * 64 * 3))
            .and_then(|v| v.checked_add(1 << 20))
            .ok_or("reference bound overflow")?;
        if bound > limit {
            return Err(format!("param {}: reference workspace {bound} > limit {limit}", entry.param).into());
        }
        peak = peak.max(bound);
        payload += entry.bytes;
    }
    let started = std::time::Instant::now();
    let mut checked = ParamCommitmentsV1::default();
    for entry in &c.header.tensors {
        eprintln!("decoded reference param {} layer {:?}: {} bytes", entry.param, entry.layer, entry.bytes);
        let t = c.read_tensor(entry.param, entry.layer)?;
        let root = misaka_palw_kernel::merkle3::tensor_commitment_v3(&t);
        if prepared.by_instance.get(&(entry.param, entry.layer)) != Some(&root) {
            return Err(format!("mismatch: param {} layer {:?}", entry.param, entry.layer).into());
        }
        checked.by_instance.insert((entry.param, entry.layer), root);
    }
    if prepared.root() != checked.root() {
        return Err("param map root mismatch".into());
    }
    let hex = misaka_palw_sdk::runtime_pack::commit::hex;
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.palw.kernel-params-decoded-reference.v3","instances":checked.by_instance.len(),"payload_bytes":payload,"param_root":hex(&checked.root()),"program_root":hex(&misaka_palw_kernel::public::program_root_v1(&c.header.program)),"reference_workspace_bound":peak,"reference_workspace_limit":limit,"elapsed_ms":started.elapsed().as_millis(),"all_instance_roots_match":true,"registration_or_activation_granted":false})
    );
    Ok(())
}
