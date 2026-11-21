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

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [cmd, inventory, file] if cmd == "pack" => pack(Path::new(inventory), Path::new(file)),
        [cmd, file, ids, limit, cap] if cmd == "replay" => replay(Path::new(file), Path::new(ids), limit.parse()?, cap.parse()?),
        _ => Err(concat!(
            "usage: qf1-8b-artifact pack <inventory.json> <new.palwtir> | ",
            "replay <model.palwtir> <input-ids.json> <decode-limit> <leaf-budget>"
        )
        .into()),
    }
}
