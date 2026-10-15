//! Own-node paged fullaux collector. No measurements or assertions of native
//! execution are produced; the offline39 private verifier authenticates it.
use kaspa_consensus_core::{Hash64, config::params::Params, header::Header, network::NetworkId};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use misaka_palw_sdk::native_audit_io_v39::{AuxStreamHeaderV39, AuxStreamRowV39, CasObjectV39};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
struct Output {
    root: PathBuf,
    bytes: u64,
    maximum: u64,
}
impl Output {
    fn charge(&mut self, n: usize) -> Result<(), String> {
        self.bytes = self
            .bytes
            .checked_add(n as u64)
            .filter(|n| *n <= self.maximum)
            .ok_or("audit39 original evidence reservation exhausted")?;
        Ok(())
    }
    fn write(&mut self, path: &Path, raw: &[u8]) -> Result<(), String> {
        self.charge(raw.len())?;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
        f.write_all(raw).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())
    }
    fn object(&mut self, raw: &[u8]) -> Result<CasObjectV39, String> {
        let sha = misaka_palw_sdk::runtime_pack::commit::hex(&Sha256::digest(raw));
        let path = self.root.join(&sha);
        if !path.exists() {
            self.write(&path, raw)?;
        } else if path.metadata().map_err(|e| e.to_string())?.len() != raw.len() as u64 {
            return Err("audit39 existing CAS object length differs".into());
        }
        Ok(CasObjectV39 { sha256: sha, bytes: raw.len() as u64 })
    }
    fn line(&mut self, f: &mut std::fs::File, v: &impl serde::Serialize, cap: usize) -> Result<(), String> {
        let raw = serde_json::to_vec(v).map_err(|e| e.to_string())?;
        if raw.len() + 1 > cap {
            return Err("audit39 original stream line bound".into());
        }
        self.charge(raw.len() + 1)?;
        f.write_all(&raw).and_then(|_| f.write_all(b"\n")).map_err(|e| e.to_string())
    }
}
fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("audit39 flag value required")?).is_some() {
            return Err("audit39 distinct original flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let rpc = get("--rpc")?;
    let network = get("--network")?;
    let genesis = get("--genesis")?;
    let child = get("--pinned-child")?.parse::<Hash64>().map_err(|e| e.to_string())?;
    let parent = get("--pinned-selected-parent")?.parse::<Hash64>().map_err(|e| e.to_string())?;
    let root = PathBuf::from(get("--evidence-root")?).canonicalize().map_err(|e| e.to_string())?;
    let out = PathBuf::from(get("--out")?);
    let maximum = get("--maximum-evidence-bytes")?.parse::<u64>().map_err(|e| e.to_string())?;
    let timeout = get("--timeout-seconds")?.parse::<u64>().map_err(|e| e.to_string())?;
    drop(get);
    let shared_objects = a.remove("--objects-dir");
    let original_snapshot = a.remove("--own-params-snapshot34");
    let original_snapshot39 = a.remove("--own-params-snapshot39");
    let pin = a.remove("--own-params-snapshot-root");
    let sha = a.remove("--own-params-sha256");
    let base: Params = network.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let own = match (original_snapshot, original_snapshot39, pin, sha) {
        (None, None, None, None) => misaka_palw_sdk::native_params_v39::OwnedNativeParametersV39::shipped_preset(base),
        (Some(f), None, Some(p), Some(s)) => {
            let own = misaka_palw_sdk::native_params_v34::load_own_native_parameters_v34(
                &base,
                Path::new(&f),
                p.parse::<Hash64>().map_err(|e| e.to_string())?.as_bytes(),
                misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&s)?,
            )?;
            misaka_palw_sdk::native_params_v39::OwnedNativeParametersV39::shipped_preset(own.into_parameters())
        }
        (None, Some(f), Some(p), Some(s)) => misaka_palw_sdk::native_params_v39::load_own_native_parameters_v39(
            &base,
            Path::new(&f),
            p.parse::<Hash64>().map_err(|e| e.to_string())?.as_bytes(),
            misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&s)?,
        )?,
        _ => return Err("audit39 exactly one owning snapshot version and all root/SHA pins required".into()),
    };
    if !a.is_empty()
        || own.parameters().genesis.hash.to_string() != genesis
        || maximum == 0
        || maximum > 9u64 << 30
        || timeout == 0
        || timeout > 43200
        || out.exists()
    {
        return Err("audit39 original network/fresh output/evidence/time bound".into());
    }
    let parent_dir = out.parent().ok_or("audit39 output parent absent")?.canonicalize().map_err(|e| e.to_string())?;
    if !parent_dir.starts_with(&root) {
        return Err("audit39 output escapes evidence root".into());
    }
    let name = out.file_name().and_then(|n| n.to_str()).ok_or("audit39 output name")?;
    let final_out = parent_dir.join(name);
    let partial = parent_dir.join(format!(".{name}.partial"));
    std::fs::create_dir(&partial).map_err(|e| e.to_string())?;
    let (object_pool, objects) = if let Some(relative) = shared_objects {
        let rel = Path::new(&relative);
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err("audit39 shared CAS must be an existing normal relative directory".into());
        }
        let pool = root.join(rel).canonicalize().map_err(|e| e.to_string())?;
        if !pool.starts_with(&root) || !pool.is_dir() {
            return Err("audit39 shared CAS escapes original evidence root".into());
        }
        (pool, relative)
    } else {
        std::fs::create_dir(partial.join("objects")).map_err(|e| e.to_string())?;
        let logical = final_out
            .join("objects")
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("audit39 pool UTF8 path")?
            .to_string();
        (partial.join("objects"), logical)
    };
    let mut output = Output { root: object_pool, bytes: 0, maximum };
    let mut index = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(partial.join("aux-stream39.jsonl"))
        .map_err(|e| e.to_string())?;
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().map_err(|e| e.to_string())?;
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&rpc), None, None, None).map_err(|e| e.to_string())?;
    runtime
        .block_on(client.connect(Some(ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(Duration::from_secs(30)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        })))
        .map_err(|e| e.to_string())?;
    let (block, parent_block, status) = runtime.block_on(async {
        let status = client.get_palw_node_status().await.map_err(|e| e.to_string())?;
        own.check_node_identity(&status.consensus_params_id, &status.consensus_schedule_id, &status.genesis_hash)?;
        Ok::<_, String>((
            client.get_block(child, false).await.map_err(|e| e.to_string())?,
            client.get_block(parent, false).await.map_err(|e| e.to_string())?,
            status,
        ))
    })?;
    let header = Header::try_from(&block.header).map_err(|e| e.to_string())?;
    let ph = Header::try_from(&parent_block.header).map_err(|e| e.to_string())?;
    if kaspa_consensus_core::hashing::header::hash(&header) != child
        || kaspa_consensus_core::hashing::header::hash(&ph) != parent
        || !block.verbose_data.as_ref().is_some_and(|v| v.is_chain_block && !v.is_header_only && v.selected_parent_hash == parent)
        || !header.parents_by_level.get(0).is_some_and(|p| p.contains(&parent))
    {
        return Err("audit39 own validated child/parent selection pin differs".into());
    }
    output.write(&partial.join("child-header.json"), &serde_json::to_vec(&header).map_err(|e| e.to_string())?)?;
    output.write(&partial.join("selected-parent-header.json"), &serde_json::to_vec(&ph).map_err(|e| e.to_string())?)?;
    // Own39 anchor opens the exact envelope selected by our independently
    // reconstructed Params. An active Rule-E grammar never falls back to V1.
    let anchor = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(60),
            client.get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                block_hash: child.to_string(),
                collection: "kernel-anchor-v39".into(),
            }),
        )
        .await
        .map_err(|_| "audit39 anchor RPC timeout")?
        .map_err(|e| e.to_string())
    })?;
    if !anchor.available
        || anchor.block_hash != child.to_string()
        || anchor.collection != "kernel-anchor-v39"
        || anchor.state_preimage.len() > 4 << 20
        || anchor.rows.len() != 1
        || !anchor.rows[0].key.is_empty()
    {
        return Err("audit39 pinned own39 state-envelope opening unavailable".into());
    }
    let envelope = kaspa_consensus_core::palw_native_state_anchor_v39::NativeStateEnvelopeV39::from_bytes(&anchor.rows[0].value)?;
    kaspa_consensus_core::palw_native_state_anchor_v39::check_native_state_anchor_v39(
        &header,
        child,
        &ph,
        parent,
        own.parameters(),
        &kaspa_consensus_core::palw_state_proof_v1::PalwStateOpeningV1 { preimage: anchor.state_preimage.clone() },
        &envelope,
    )?;
    output.write(&partial.join("envelope39.bin"), &anchor.rows[0].value)?;
    let anchor_state = anchor.state_preimage;
    let (mut cursor, mut total, mut count, mut original_state) = ("-".to_string(), None, 0usize, None::<Vec<u8>>);
    let started = Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(timeout) {
            return Err("audit39 complete original snapshot timeout; partial remains NOT_RUN".into());
        }
        let page = runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(60),
                client.get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                    block_hash: child.to_string(),
                    collection: format!("kernel-aux-page-v39/{cursor}"),
                }),
            )
            .await
            .map_err(|_| "audit39 page RPC timeout")?
            .map_err(|e| e.to_string())
        })?;
        if !page.available || page.block_hash != child.to_string() || page.state_preimage.len() > 4 << 20 || page.rows.len() > 1024 {
            return Err("audit39 original bounded page unavailable".into());
        }
        if page.state_preimage != anchor_state {
            return Err("audit39 paged state differs from original pinned own39 envelope".into());
        }
        let fields: Vec<_> = page.collection.split('/').collect();
        if fields.len() != 4 || fields[0] != "kernel-aux-page-v39" || !matches!(fields[2], "0" | "1") {
            return Err("audit39 original response page grammar".into());
        }
        let rows = fields[1].parse::<usize>().map_err(|_| "audit39 original row count")?;
        if rows.to_string() != fields[1] || rows > kaspa_consensus_core::palw_native_audit_proof_v39::NATIVE_AUX_ROWS_MAX_V39 {
            return Err("audit39 original bounded rowcount".into());
        }
        if let Some(t) = total {
            if t != rows || original_state.as_deref() != Some(page.state_preimage.as_slice()) {
                return Err("audit39 pinned snapshot changed between pages".into());
            }
        } else {
            total = Some(rows);
            let state = output.object(&page.state_preimage)?;
            original_state = Some(page.state_preimage.clone());
            output.line(
                &mut index,
                &AuxStreamHeaderV39 {
                    schema: "misaka.palw.native-aux-stream.v39".into(),
                    state,
                    total_rows: rows,
                    objects: objects.clone(),
                },
                4096,
            )?;
        }
        let mut last = cursor.clone();
        let mut page_bytes = 0usize;
        for row in &page.rows {
            if row.key.len() < 5
                || row.key.len() > 4101
                || u32::from_le_bytes(row.key[1..5].try_into().unwrap()) as usize != row.key.len() - 5
                || row.value.len() > 32 << 20
            {
                return Err("audit39 original typed row extent".into());
            }
            page_bytes = page_bytes
                .checked_add(row.key.len() + row.value.len() + 4)
                .filter(|n| *n <= 32 << 20)
                .ok_or("audit39 original page byte cap")?;
            let next = format!("{:02x}{}", row.key[0], misaka_palw_sdk::runtime_pack::commit::hex(&row.key[5..]));
            if last != "-" && next <= last {
                return Err("audit39 original rows duplicated/reordered".into());
            }
            last = next;
            let value = output.object(&row.value)?;
            output.line(
                &mut index,
                &AuxStreamRowV39 { table: row.key[0], key: misaka_palw_sdk::runtime_pack::commit::hex(&row.key[5..]), value },
                16384,
            )?;
            count += 1;
            if count > rows {
                return Err("audit39 original complete row count exceeded".into());
            }
        }
        if fields[3] != last || fields[2] == "0" && page.rows.is_empty() {
            return Err("audit39 original page cursor/progress differs".into());
        }
        cursor = last;
        if fields[2] == "1" {
            if count != rows {
                return Err("audit39 original full transcript truncated".into());
            }
            break;
        }
    }
    index.sync_all().map_err(|e| e.to_string())?;
    drop(index);
    let report = serde_json::json!({"schema":"misaka.palw.native-aux-stream-collection.v39","network":network,"genesis":genesis,"child":child.to_string(),"selected_parent":parent.to_string(),"rows":count,"node_consensus_params_id":status.consensus_params_id,"local_qualified_validation_network":own.parameters().native_final_is_local_qualified_v34(),"raw_transport_only":true,"evidence_bytes":output.bytes});
    output.write(&partial.join("collection.json"), &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)?;
    std::fs::rename(partial, final_out).map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("audit39 collection refused: {e}");
        std::process::exit(1);
    }
}
