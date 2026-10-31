//! Contemporaneous full-state raw transport. No measurement/qualification is
//! asserted. Every original state is later authenticated by the Core39 factory.
#![allow(warnings)]
use kaspa_consensus_core::{Hash64, config::params::Params, header::Header, network::NetworkId};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn executable_pin(path: &Path, pin: [u8; 32]) -> Result<(), String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let m = f.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() == 0 || m.len() > 1 << 30 {
        return Err("live39 original collector executable extent".into());
    }
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    let mut bytes = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > m.len() {
            return Err("live39 collector executable grew during pinning".into());
        }
        hash.update(&buf[..n]);
    }
    if bytes != m.len()
        || <[u8; 32]>::from(hash.finalize()) != pin
        || f.metadata().map_err(|e| e.to_string())?.modified().ok() != m.modified().ok()
    {
        return Err("live39 original collector executable SHA256/extent changed".into());
    }
    Ok(())
}
fn executable_unchanged(path: &Path, original: &std::fs::Metadata) -> Result<(), String> {
    let current = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if current.len() != original.len() || current.modified().ok() != original.modified().ok() {
        return Err("live39 pinned executable metadata changed".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if current.dev() != original.dev() || current.ino() != original.ino() {
            return Err("live39 pinned executable inode changed".into());
        }
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("flag value required")?).is_some() {
            return Err("live39 distinct exact flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let rpc = get("--rpc")?;
    let network = get("--network")?;
    let genesis = get("--genesis")?;
    let start_text = get("--start-child")?;
    let automatic_start = start_text == "own-current-sink";
    let mut start = if automatic_start { Hash64::default() } else { start_text.parse::<Hash64>().map_err(|e| e.to_string())? };
    let stop = get("--stop-daa")?.parse::<u64>().map_err(|e| e.to_string())?;
    let timeout = get("--timeout-seconds")?.parse::<u64>().map_err(|e| e.to_string())?;
    let point_timeout = get("--point-timeout-seconds")?.parse::<u64>().map_err(|e| e.to_string())?;
    let maximum = get("--maximum-evidence-bytes")?.parse::<u64>().map_err(|e| e.to_string())?;
    let root = PathBuf::from(get("--evidence-root")?).canonicalize().map_err(|e| e.to_string())?;
    let out = PathBuf::from(get("--out")?);
    let objects = PathBuf::from(get("--objects-dir")?).canonicalize().map_err(|e| e.to_string())?;
    let exe = PathBuf::from(get("--collector-executable")?).canonicalize().map_err(|e| e.to_string())?;
    let exe_sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--collector-sha256")?)?;
    let snapshot = PathBuf::from(get("--own-params-snapshot39")?).canonicalize().map_err(|e| e.to_string())?;
    let pin = get("--own-params-snapshot-root")?;
    let sha = get("--own-params-sha256")?;
    #[allow(clippy::drop_non_drop)]
    drop(get);
    let first_window = a.remove("--first-window-start-daa").map(|v| v.parse::<u64>().map_err(|e| e.to_string())).transpose()?;
    let ready = a.remove("--ready-out").map(PathBuf::from);
    let operator = a.remove("--operator-id").ok_or("live39 original operator-id required")?;
    let database = a.remove("--node-database-id").ok_or("live39 original node-database-id required")?;
    if automatic_start && (first_window.is_none() || ready.is_none()) {
        return Err("live39 automatic start requires original first window and fresh readiness output".into());
    }
    if operator.is_empty() || database.is_empty() {
        return Err("live39 original operator/database identity absent".into());
    }
    #[allow(clippy::collapsible_if)]
    if let Some(path) = &ready {
        if path.exists()
            || !path.parent().ok_or("live39 ready parent absent")?.canonicalize().map_err(|e| e.to_string())?.starts_with(&root)
        {
            return Err("live39 ready output must be fresh inside evidence root".into());
        }
    }
    if !a.is_empty()
        || stop == 0
        || timeout == 0
        || timeout > 604800
        || point_timeout == 0
        || point_timeout > 3600
        || maximum == 0
        || maximum > 1u64 << 40
        || out.exists()
        || !objects.is_dir()
        || !objects.starts_with(&root)
        || std::fs::read_dir(&objects).map_err(|e| e.to_string())?.next().is_some()
    {
        return Err("live39 original fresh output/CAS/time/byte bound required".into());
    }
    let parent = out.parent().ok_or("live39 output parent")?.canonicalize().map_err(|e| e.to_string())?;
    if !parent.starts_with(&root) {
        return Err("live39 output escapes original evidence root".into());
    }
    let name = out.file_name().and_then(|n| n.to_str()).ok_or("live39 output name")?;
    let final_out = parent.join(name);
    let partial = parent.join(format!(".{name}.partial"));
    if objects.starts_with(&final_out) || objects.starts_with(&partial) {
        return Err("live39 shared CAS must survive final directory rename".into());
    }
    let base: Params = network.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let own = misaka_palw_sdk::native_params_v39::load_own_native_parameters_v39(
        &base,
        &snapshot,
        pin.parse::<Hash64>().map_err(|e| e.to_string())?.as_bytes(),
        misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&sha)?,
    )?;
    if own.parameters().genesis.hash.to_string() != genesis {
        return Err("live39 original owning genesis differs".into());
    }
    executable_pin(&exe, exe_sha)?;
    let executable_metadata = std::fs::metadata(&exe).map_err(|e| e.to_string())?;
    std::fs::create_dir(&partial).map_err(|e| e.to_string())?;
    let mut points = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(partial.join("points-stream-v39.jsonl"))
        .map_err(|e| e.to_string())?;
    let header = b"{\"schema\":\"misaka.rfc02.receipt-state-stream.v39\"}\n";
    points.write_all(header).map_err(|e| e.to_string())?;
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
                    .map_err(|_| "live39 initial sink timeout")?
                    .map_err(|e| e.to_string())
            })?
            .sink;
    }
    let began = Instant::now();
    let mut next = start;
    let mut pending = VecDeque::new();
    let mut count = 0u64;
    let mut used = header.len() as u64;
    let mut rolling = [0; 64];
    loop {
        if began.elapsed() > Duration::from_secs(timeout) || count >= 10_000_000 {
            return Err("live39 original full transcript time/state reservation exhausted; partial retained".into());
        }
        let (status, block) = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(30), async {
                Ok::<_, String>((
                    client.get_palw_node_status().await.map_err(|e| e.to_string())?,
                    client.get_block(next, false).await.map_err(|e| e.to_string())?,
                ))
            })
            .await
            .map_err(|_| "live39 own-node metadata timeout")?
        })?;
        own.check_node_identity(&status.consensus_params_id, &status.consensus_schedule_id, &status.genesis_hash)?;
        let h = Header::try_from(&block.header).map_err(|e| e.to_string())?;
        let v = block.verbose_data.as_ref().ok_or("live39 selected-chain metadata absent")?;
        let parent_hash = v.selected_parent_hash;
        if kaspa_consensus_core::hashing::header::hash(&h) != next
            || !v.is_chain_block
            || v.is_header_only
            || parent_hash == next
            || !h.parents_by_level.get(0).is_some_and(|p| p.contains(&parent_hash))
        {
            return Err("live39 original child/parent left validated selected chain".into());
        }
        let p = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(30), client.get_block(parent_hash, false))
                .await
                .map_err(|_| "live39 parent timeout")?
                .map_err(|e| e.to_string())
        })?;
        let ph = Header::try_from(&p.header).map_err(|e| e.to_string())?;
        if kaspa_consensus_core::hashing::header::hash(&ph) != parent_hash || ph.daa_score > h.daa_score {
            return Err("live39 original parent header differs".into());
        }
        if count == 0 && first_window.is_some_and(|first| ph.daa_score >= first) {
            return Err("live39 collection began after original first window; partial retained".into());
        }
        let folder = format!("point-{count:010}");
        let point = partial.join(&folder);
        let remaining = maximum
            .checked_sub(used)
            .and_then(|n| n.checked_sub(1 << 20))
            .ok_or("live39 original evidence reservation exhausted")?
            .min(9u64 << 30);
        executable_unchanged(&exe, &executable_metadata)?;
        let mut child = Command::new(&exe)
            .args([
                "--rpc",
                &rpc,
                "--network",
                &network,
                "--genesis",
                &genesis,
                "--pinned-child",
                &next.to_string(),
                "--pinned-selected-parent",
                &parent_hash.to_string(),
                "--evidence-root",
                root.to_str().ok_or("live39 root UTF8")?,
                "--out",
                point.to_str().ok_or("live39 point UTF8")?,
                "--objects-dir",
                objects.strip_prefix(&root).map_err(|e| e.to_string())?.to_str().ok_or("live39 CAS UTF8")?,
                "--maximum-evidence-bytes",
                &remaining.to_string(),
                "--timeout-seconds",
                &point_timeout.to_string(),
                "--own-params-snapshot39",
                snapshot.to_str().ok_or("live39 snapshot UTF8")?,
                "--own-params-snapshot-root",
                &pin,
                "--own-params-sha256",
                &sha,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("live39 pinned raw collector failed with {status}; partial retained"));
                }
                break;
            }
            if started.elapsed() > Duration::from_secs(point_timeout) || began.elapsed() > Duration::from_secs(timeout) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("live39 original point collection timeout; partial retained".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        executable_unchanged(&exe, &executable_metadata)?;
        let report = misaka_palw_sdk::native_audit_io_v32::read_bounded_file_v32(&point.join("collection.json"), 16384)?;
        let report_json: serde_json::Value = serde_json::from_slice(&report).map_err(|e| e.to_string())?;
        let owned = report_json
            .get("evidence_bytes")
            .and_then(|v| v.as_u64())
            .filter(|n| *n <= remaining)
            .ok_or("live39 original collector owned evidence accounting absent")?;
        if report_json.get("child").and_then(|v| v.as_str()) != Some(next.to_string().as_str())
            || report_json.get("selected_parent").and_then(|v| v.as_str()) != Some(parent_hash.to_string().as_str())
            || report_json.get("raw_transport_only").and_then(|v| v.as_bool()) != Some(true)
        {
            return Err("live39 original raw collector anchor/classification differs".into());
        }
        let path =
            final_out.join(&folder).strip_prefix(&root).map_err(|e| e.to_string())?.to_str().ok_or("live39 pointer UTF8")?.to_string();
        let line = serde_json::to_vec(
            &serde_json::json!({"child":next.to_string(),"selected_parent":parent_hash.to_string(),"collected":path}),
        )
        .map_err(|e| e.to_string())?;
        if line.len() + 1 > 4096 {
            return Err("live39 original pointer line bound".into());
        }
        used = used
            .checked_add(owned)
            .and_then(|n| n.checked_add(report.len() as u64 + line.len() as u64 + 1))
            .filter(|n| *n <= maximum)
            .ok_or("live39 original cumulative evidence reservation exhausted")?;
        points.write_all(&line).and_then(|_| points.write_all(b"\n")).and_then(|_| points.sync_data()).map_err(|e| e.to_string())?;
        rolling = misaka_palw_kernel::hash::object_id(
            b"misaka-palw/audit/live-collected-transcript/v39",
            &(rolling, next, parent_hash, count),
        );
        #[allow(clippy::collapsible_if)]
        if count == 0 {
            if let Some(path) = &ready {
                let raw=serde_json::to_vec(&serde_json::json!({"schema":"misaka.rfc02.receipt-live-ready.v39","start_child":start.to_string(),"first_parent_daa":ph.daa_score,"network":network,"genesis":genesis,"operator_id":operator,"node_database":database,"node_consensus_params_id":status.consensus_params_id})).map_err(|e|e.to_string())?;
                used = used.checked_add(raw.len() as u64).filter(|n| *n <= maximum).ok_or("live39 readiness reservation exhausted")?;
                let temporary = path.with_extension("ready-partial");
                let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| e.to_string())?;
                f.write_all(&raw).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
                std::fs::rename(temporary, path).map_err(|e| e.to_string())?;
            }
        }
        count += 1;
        if ph.daa_score >= stop {
            break;
        }
        while pending.is_empty() {
            if began.elapsed() > Duration::from_secs(timeout) {
                return Err("live39 acquisition deadline; partial retained".into());
            }
            let sink = rt
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(30), client.get_sink())
                        .await
                        .map_err(|_| "live39 sink timeout")?
                        .map_err(|e| e.to_string())
                })?
                .sink;
            if sink == next {
                rt.block_on(async { tokio::time::sleep(Duration::from_secs(1)).await });
                continue;
            }
            let mut at = sink;
            let mut descending = Vec::new();
            while at != next {
                if descending.len() >= 512 {
                    return Err("live39 bounded512 catch-up exhausted or original anchor removed by reorg; partial retained".into());
                }
                let block = rt.block_on(async {
                    tokio::time::timeout(Duration::from_secs(30), client.get_block(at, false))
                        .await
                        .map_err(|_| "live39 chain timeout")?
                        .map_err(|e| e.to_string())
                })?;
                let v = block.verbose_data.as_ref().ok_or("live39 chain metadata absent")?;
                let h = Header::try_from(&block.header).map_err(|e| e.to_string())?;
                if kaspa_consensus_core::hashing::header::hash(&h) != at
                    || !v.is_chain_block
                    || v.is_header_only
                    || v.selected_parent_hash == at
                    || !h.parents_by_level.get(0).is_some_and(|p| p.contains(&v.selected_parent_hash))
                {
                    return Err("live39 catch-up is not original validated selected chain".into());
                }
                descending.push(at);
                at = v.selected_parent_hash;
            }
            pending.extend(descending.into_iter().rev());
        }
        let following = pending.pop_front().ok_or("live39 next original child absent")?;
        let b = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(30), client.get_block(following, false))
                .await
                .map_err(|_| "live39 child timeout")?
                .map_err(|e| e.to_string())
        })?;
        if !b.verbose_data.as_ref().is_some_and(|v| v.is_chain_block && !v.is_header_only && v.selected_parent_hash == next) {
            return Err("live39 chain reorg/discontinuity; partial retained".into());
        }
        next = following;
    }
    executable_pin(&exe, exe_sha)?;
    let report=serde_json::to_vec_pretty(&serde_json::json!({"schema":"misaka.rfc02.native-live-receipt-collection.v39","network":network,"genesis":genesis,"operator_id":operator,"node_database":database,"node_consensus_params_id":own.parameters().consensus_params_id().to_string(),"start_child":start.to_string(),"end_child":next.to_string(),"stop_daa":stop,"states":count,"raw_transport_only":true,"collected_ordered_root":misaka_palw_sdk::runtime_pack::commit::hex(&rolling),"reserved_evidence_bytes":maximum,"owned_bytes_before_report":used,"collector_sha256":misaka_palw_sdk::runtime_pack::commit::hex(&exe_sha)})).map_err(|e|e.to_string())?;
    if used.checked_add(report.len() as u64).is_none_or(|n| n > maximum) {
        return Err("live39 final metadata exceeds original reservation".into());
    }
    let mut f =
        std::fs::OpenOptions::new().write(true).create_new(true).open(partial.join("collection.json")).map_err(|e| e.to_string())?;
    f.write_all(&report).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
    drop(points);
    std::fs::rename(partial, final_out).map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("live39 collection refused: {e}");
        std::process::exit(1);
    }
}
