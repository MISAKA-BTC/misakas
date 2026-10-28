//! Long-window own-node collector: one state at a time, CAS-shared original
//! rows, bounded pointer lines. A partial run is never renamed to completed.
use kaspa_consensus_core::{Hash64, config::params::Params, header::Header, network::NetworkId};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use misaka_palw_sdk::native_receipt_cas_v1::*;
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn keep(path: &Path, raw: &[u8]) -> Result<String, String> {
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
    f.write_all(raw).map_err(|e| e.to_string())?;
    Ok(misaka_palw_sdk::runtime_pack::commit::hex(&Sha256::digest(raw)))
}
struct Store {
    physical: PathBuf,
    logical: String,
    bytes: u64,
    maximum: u64,
}
impl Store {
    fn charge(&mut self, n: u64) -> Result<(), String> {
        self.bytes =
            self.bytes.checked_add(n).filter(|v| *v <= self.maximum).ok_or("receipt complete evidence reservation exhausted")?;
        Ok(())
    }
    fn file(&mut self, path: &Path, raw: &[u8]) -> Result<(), String> {
        self.charge(raw.len() as u64)?;
        keep(path, raw)?;
        Ok(())
    }
    fn line(&mut self, file: &mut std::fs::File, raw: &[u8]) -> Result<(), String> {
        self.charge(raw.len() as u64 + 1)?;
        file.write_all(raw).and_then(|_| file.write_all(b"\n")).map_err(|e| e.to_string())
    }

    fn object(&mut self, raw: &[u8]) -> Result<NativeCasObjectV1, String> {
        let sha = misaka_palw_sdk::runtime_pack::commit::hex(&Sha256::digest(raw));
        let path = self.physical.join(&sha);
        if path.exists() {
            if path.metadata().map_err(|e| e.to_string())?.len() != raw.len() as u64 {
                return Err("receipt CAS existing object differs".into());
            }
        } else {
            self.bytes = self
                .bytes
                .checked_add(raw.len() as u64)
                .filter(|n| *n <= self.maximum)
                .ok_or("receipt original evidence byte reservation exhausted")?;
            keep(&path, raw)?;
        }
        Ok(NativeCasObjectV1 { sha256: sha, bytes: raw.len() as u64 })
    }
    fn proof(&mut self, r: &kaspa_rpc_core::GetPalwStateProofResponse, path: &Path) -> Result<(), String> {
        if !r.available
            || r.state_preimage.len() > 4 << 20
            || r.rows.len() > 65536
            || !matches!(r.collection.as_str(), "kernel-aux-v32" | "pending-payouts-v32")
        {
            return Err("receipt original complete state proof unavailable/oversized".into());
        }
        let total = r
            .rows
            .iter()
            .try_fold(r.state_preimage.len(), |n, r| {
                if r.key.len() > 4101 || r.value.len() > 32 << 20 {
                    None
                } else {
                    n.checked_add(r.key.len())?.checked_add(r.value.len())
                }
            })
            .filter(|n| *n <= 64 << 20)
            .ok_or("receipt original full proof extent exceeded")?;
        let _ = total;
        let state = self.object(&r.state_preimage)?;
        let mut rows = Vec::with_capacity(r.rows.len());
        for row in &r.rows {
            rows.push(NativeCasRowV1 { key: misaka_palw_sdk::runtime_pack::commit::hex(&row.key), value: self.object(&row.value)? });
        }
        let m = NativeCasFactProofV1 {
            schema: "misaka.palw.native-fact-proof-cas.v1".into(),
            objects: self.logical.clone(),
            state,
            label: r.collection.clone(),
            rows,
        };
        let f = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
        let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
        serde_json::to_writer(&mut gz, &m).map_err(|e| e.to_string())?;
        gz.finish().map_err(|e| e.to_string())?;
        self.bytes = self
            .bytes
            .checked_add(path.metadata().map_err(|e| e.to_string())?.len())
            .filter(|n| *n <= self.maximum)
            .ok_or("receipt proof metadata evidence reservation exhausted")?;
        Ok(())
    }
}
fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("flag value required")?).is_some() {
            return Err("receipt collect distinct flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let rpc = get("--rpc")?;
    let network = get("--network")?;
    let genesis = get("--genesis")?;
    let operator = get("--operator-id")?;
    let database = get("--node-database-id")?;
    let start_text = get("--start-child")?;
    let automatic_start = start_text == "own-current-sink";
    let mut start: Hash64 = if automatic_start { Hash64::default() } else { start_text.parse().map_err(|e| format!("start: {e}"))? };
    let out = PathBuf::from(get("--out")?);
    let root = PathBuf::from(get("--evidence-root")?).canonicalize().map_err(|e| e.to_string())?;
    let timeout = get("--timeout-seconds")?.parse::<u64>().map_err(|e| e.to_string())?;
    let maximum = get("--maximum-evidence-bytes")?.parse::<u64>().map_err(|e| e.to_string())?;
    drop(get);
    let mode = a.remove("--acquisition-mode").unwrap_or_else(|| "reverse-v1".into());
    let live = mode == "live-forward-v2";
    let (end, end_parent, stop_daa) = if live {
        if a.contains_key("--end-child") || a.contains_key("--end-selected-parent") {
            return Err("live receipt acquisition uses predeclared stop DAA, not a future guessed block".into());
        }
        let stop = a.remove("--stop-daa").ok_or("live receipt --stop-daa required")?.parse::<u64>().map_err(|e| e.to_string())?;
        if stop == 0 {
            return Err("live receipt original stop DAA must be positive".into());
        }
        (start, None, Some(stop))
    } else if mode == "reverse-v1" {
        let end = a.remove("--end-child").ok_or("--end-child required")?.parse::<Hash64>().map_err(|e| e.to_string())?;
        let parent =
            a.remove("--end-selected-parent").ok_or("--end-selected-parent required")?.parse::<Hash64>().map_err(|e| e.to_string())?;
        (end, Some(parent), None)
    } else {
        return Err("receipt acquisition mode must be reverse-v1 or live-forward-v2".into());
    };
    let first_window = a.remove("--first-window-start-daa").map(|v| v.parse::<u64>().map_err(|e| e.to_string())).transpose()?;
    let ready = a.remove("--ready-out").map(PathBuf::from);
    if automatic_start && (!live || first_window.is_none() || ready.is_none()) {
        return Err("automatic own-chain start requires live mode, original first window and fresh ready output".into());
    }
    if let Some(path) = &ready {
        if path.exists()
            || !path.parent().ok_or("receipt ready parent absent")?.canonicalize().map_err(|e| e.to_string())?.starts_with(&root)
        {
            return Err("receipt ready output must be fresh inside original evidence root".into());
        }
    }
    let file = a.remove("--own-params-snapshot34");
    let pin = a.remove("--own-params-snapshot-root");
    let sha = a.remove("--own-params-sha256");
    let base: Params = network.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let own = match (file, pin, sha) {
        (None, None, None) => misaka_palw_sdk::native_params_v34::OwnedNativeParametersV34::shipped_preset(base),
        (Some(f), Some(p), Some(s)) => misaka_palw_sdk::native_params_v34::load_own_native_parameters_v34(
            &base,
            Path::new(&f),
            p.parse::<Hash64>().map_err(|e| e.to_string())?.as_bytes(),
            misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&s)?,
        )?,
        _ => return Err("all owning pins required".into()),
    };
    if !a.is_empty()
        || timeout == 0
        || timeout > 604800
        || maximum == 0
        || maximum > 1u64 << 40
        || out.exists()
        || !own.rfc02_qualified()
        || own.parameters().genesis.hash.to_string() != genesis
        || operator.is_empty()
        || database.is_empty()
    {
        return Err("receipt collector explicit qualified original bounded profile required".into());
    }
    let parent = out.parent().ok_or("receipt collector fresh parent required")?.canonicalize().map_err(|e| e.to_string())?;
    if !parent.starts_with(&root) {
        return Err("receipt collector output escapes run root".into());
    }
    let name =
        out.file_name().ok_or("receipt collector output name required")?.to_str().ok_or("receipt collector UTF8 output name")?;
    let final_out = parent.join(name);
    let partial = parent.join(format!(".{name}.partial"));
    std::fs::create_dir(&partial).map_err(|e| e.to_string())?;
    std::fs::create_dir(partial.join("objects")).map_err(|e| e.to_string())?;
    let mut store = Store {
        physical: partial.join("objects"),
        logical: final_out
            .join("objects")
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("receipt pool UTF8 path")?
            .to_string(),
        bytes: 0,
        maximum,
    };
    let mut reverse = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(partial.join(if live { "points-stream-v2.jsonl" } else { "reverse-index.jsonl" }))
        .map_err(|e| e.to_string())?;
    let mut inventory = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(partial.join("evidence-index.jsonl"))
        .map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().map_err(|e| e.to_string())?;
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&rpc), None, None, None).map_err(|e| e.to_string())?;
    rt.block_on(client.connect(Some(ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(30)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    })))
    .map_err(|e| e.to_string())?;
    if automatic_start {
        start = rt
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(30), client.get_sink())
                    .await
                    .map_err(|_| "receipt initial sink timeout")?
                    .map_err(|e| e.to_string())
            })?
            .sink;
    }
    if live {
        store.line(&mut reverse, b"{\"schema\":\"misaka.rfc02.receipt-state-stream.v2\"}")?;
    }
    let mut pending = VecDeque::new();
    let mut observed_end = end;
    let mut observed_parent = end_parent.unwrap_or_default();
    let began = Instant::now();
    let mut next = if live { start } else { end };
    let mut count = 0u64;
    let mut params_id = String::new();
    let mut rolling = [0u8; 64];
    loop {
        if began.elapsed() > Duration::from_secs(timeout) || count >= 10_000_000 {
            return Err(
                "receipt complete original transcript collection time/state reservation exhausted; partial retained, no PASS".into()
            );
        }
        let (block, parent, aux, payout, status) = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(60), async {
                let status = client.get_palw_node_status().await.map_err(|e| e.to_string())?;
                own.check_node_identity(&status.consensus_params_id, &status.consensus_schedule_id, &status.genesis_hash)?;
                let block = client.get_block(next, false).await.map_err(|e| e.to_string())?;
                let v = block.verbose_data.as_ref().ok_or("receipt selected-chain metadata absent")?;
                if !v.is_chain_block || v.is_header_only {
                    return Err("receipt original pinned block not on own validated selected chain".to_string());
                }
                let parent = client.get_block(v.selected_parent_hash, false).await.map_err(|e| e.to_string())?;
                let aux = client
                    .get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                        block_hash: next.to_string(),
                        collection: "kernel-aux-v32".into(),
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                let payout = client
                    .get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                        block_hash: next.to_string(),
                        collection: "pending-payouts-v32".into(),
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>((block, parent, aux, payout, status))
            })
            .await
            .map_err(|_| "receipt point RPC timeout".to_string())?
        })?;
        let child = Header::try_from(&block.header).map_err(|e| e.to_string())?;
        let ph = Header::try_from(&parent.header).map_err(|e| e.to_string())?;
        let parent_hash = kaspa_consensus_core::hashing::header::hash(&ph);
        let verbose = block.verbose_data.as_ref().unwrap();
        if kaspa_consensus_core::hashing::header::hash(&child) != next
            || parent_hash != verbose.selected_parent_hash
            || parent_hash == next
            || !child.parents_by_level.get(0).is_some_and(|p| p.contains(&parent_hash))
            || ph.daa_score > child.daa_score
            || !live && count == 0 && Some(parent_hash) != end_parent
            || aux.block_hash != next.to_string()
            || payout.block_hash != next.to_string()
        {
            return Err("receipt original header/selected-parent/proof pin differs".into());
        }
        if count == 0 && first_window.is_some_and(|first| ph.daa_score >= first) {
            return Err("live receipt collector began after original first window; partial retained".into());
        }
        if params_id.is_empty() {
            params_id = status.consensus_params_id.clone();
        } else if params_id != status.consensus_params_id {
            return Err("receipt owning parameters changed during transcript collection".into());
        }
        let folder_name = format!("point-{count:010}");
        let folder = partial.join(&folder_name);
        std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
        store.file(&folder.join("child-header.json"), &serde_json::to_vec(&child).map_err(|e| e.to_string())?)?;
        store.file(&folder.join("selected-parent-header.json"), &serde_json::to_vec(&ph).map_err(|e| e.to_string())?)?;
        store.proof(&aux, &folder.join("aux-proof32.cas.json.gz"))?;
        store.proof(&payout, &folder.join("payout-proof32.cas.json.gz"))?;
        let collected = final_out
            .join(&folder_name)
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("receipt point path UTF8")?
            .to_string();
        let line = serde_json::to_vec(
            &serde_json::json!({"child":next.to_string(),"selected_parent":parent_hash.to_string(),"collected":collected}),
        )
        .map_err(|e| e.to_string())?;
        if line.len() + 1 > 4096 {
            return Err("receipt pointer line bound".into());
        }
        store.line(&mut reverse, &line)?;
        for name in ["child-header.json", "selected-parent-header.json", "aux-proof32.cas.json.gz", "payout-proof32.cas.json.gz"] {
            let path = folder.join(name);
            let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut h = Sha256::new();
            let mut buf = [0; 65536];
            loop {
                let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            let digest = misaka_palw_sdk::runtime_pack::commit::hex(&h.finalize());
            let record = serde_json::json!({"path":format!("{folder_name}/{name}"),"sha256":digest,"bytes":path.metadata().map_err(|e|e.to_string())?.len()});
            store.line(&mut inventory, &serde_json::to_vec(&record).map_err(|e| e.to_string())?)?;
        }
        rolling = misaka_palw_kernel::hash::object_id(
            if live {
                b"misaka-palw/audit/live-collected-transcript/v2".as_slice()
            } else {
                b"misaka-palw/audit/reverse-collected-transcript/v1".as_slice()
            },
            &(rolling, next, parent_hash, count),
        );
        if count == 0 {
            if let Some(path) = &ready {
                let doc=serde_json::to_vec(&serde_json::json!({"schema":"misaka.rfc02.receipt-live-ready.v2","start_child":start.to_string(),"first_parent_daa":ph.daa_score,"network":network,"genesis":genesis,"operator_id":operator,"node_database":database,"node_consensus_params_id":params_id})).map_err(|e|e.to_string())?;
                let temporary = path.with_extension("ready-partial");
                store.file(&temporary, &doc)?;
                std::fs::rename(temporary, path).map_err(|e| e.to_string())?;
                reverse.sync_data().map_err(|e| e.to_string())?;
            }
        }
        count += 1;
        observed_end = next;
        observed_parent = parent_hash;
        if !live {
            if next == start {
                break;
            }
            next = parent_hash;
            continue;
        }
        if stop_daa.is_some_and(|stop| ph.daa_score >= stop) {
            break;
        }
        if pending.is_empty() {
            let last = next;
            loop {
                if began.elapsed() > Duration::from_secs(timeout) {
                    return Err("live receipt original acquisition timeout; partial retained, no PASS".into());
                }
                let sink = rt
                    .block_on(async {
                        tokio::time::timeout(Duration::from_secs(30), client.get_sink())
                            .await
                            .map_err(|_| "live receipt sink timeout")?
                            .map_err(|e| e.to_string())
                    })?
                    .sink;
                if sink == last {
                    rt.block_on(async { tokio::time::sleep(Duration::from_secs(1)).await });
                    continue;
                }
                let mut cursor = sink;
                let mut descending = Vec::new();
                while cursor != last {
                    if descending.len() >= 512 {
                        return Err("live receipt catch-up exceeds original bounded512 block reservation or reorg removed prior anchor; partial retained".into());
                    }
                    let b = rt.block_on(async {
                        tokio::time::timeout(Duration::from_secs(30), client.get_block(cursor, false))
                            .await
                            .map_err(|_| "live receipt catch-up timeout")?
                            .map_err(|e| e.to_string())
                    })?;
                    let v = b.verbose_data.as_ref().ok_or("live receipt selected-chain metadata absent")?;
                    if !v.is_chain_block || v.is_header_only || v.selected_parent_hash == cursor {
                        return Err("live receipt catch-up left original validated selected chain".into());
                    }
                    let h = Header::try_from(&b.header).map_err(|e| e.to_string())?;
                    if kaspa_consensus_core::hashing::header::hash(&h) != cursor
                        || !h.parents_by_level.get(0).is_some_and(|p| p.contains(&v.selected_parent_hash))
                    {
                        return Err("live receipt original catch-up header/parent differs".into());
                    }
                    descending.push(cursor);
                    cursor = v.selected_parent_hash;
                }
                pending.extend(descending.into_iter().rev());
                break;
            }
        }
        let following = pending.pop_front().ok_or("live receipt next original selected child absent")?;
        let b = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(30), client.get_block(following, false))
                .await
                .map_err(|_| "live receipt following child timeout")?
                .map_err(|e| e.to_string())
        })?;
        if !b.verbose_data.as_ref().is_some_and(|v| v.is_chain_block && !v.is_header_only && v.selected_parent_hash == next) {
            return Err("live receipt reorg/discontinuity removed original prior anchor; partial retained, no PASS".into());
        }
        next = following;
    }
    reverse.sync_all().map_err(|e| e.to_string())?;
    inventory.sync_all().map_err(|e| e.to_string())?;
    drop(reverse);
    drop(inventory);
    let report = serde_json::json!({"schema":"misaka.rfc02.native-receipt-cas-collection.v1","network":network,"genesis":genesis,
        "operator_id":operator,"node_database":database,"rpc":rpc,"start_child":start.to_string(),"end_child":observed_end.to_string(),"end_selected_parent":observed_parent.to_string(),"acquisition_mode":mode,"stop_daa":stop_daa,
        "states":count,"collected_ordered_root":misaka_palw_sdk::runtime_pack::commit::hex(&rolling),"reserved_evidence_bytes":maximum,"CAS_owned_bytes":store.bytes,
        "qualified_native_profile":true,"local_qualified_validation_network":own.parameters().native_final_is_local_qualified_v34(),
        "node_consensus_params_id":params_id,"complete_original_transcript_retained":true,"raw_node_assertions_are_unverified":true});
    store.file(&partial.join("collection.json"), &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)?;
    // All pointer/object names target final_out; no partial directory is a usable
    // completed transcript. Interrupted files remain visibly partial for cleanup.
    std::fs::rename(partial, final_out).map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("receipt collect refused: {e}");
        std::process::exit(1);
    }
}
