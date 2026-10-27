//! Collect genuine public native32 anchor openings and bounded full block
//! provenance from one independently configured node. Raw RPC flags remain
//! observations; this command never asserts A6/A8 or UTXO redemption.
use kaspa_consensus_core::{
    Hash64,
    header::Header,
    palw_state_proof_v1::{PalwCollectionOpeningV1, PalwFactProofV1, PalwStateOpeningV1},
};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use misaka_palw_sdk::{native_audit_io_v32::encode_native_fact_proof_v32, runtime_pack::commit::hex};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write, path::PathBuf, time::Duration};
fn run() -> Result<(), String> {
    let mut args = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if !flag.starts_with("--") || args.insert(flag.clone(), it.next().ok_or_else(|| format!("{flag} needs a value"))?).is_some() {
            return Err("distinct explicit flags required".into());
        }
    }
    let mut get = |key: &str| args.remove(key).ok_or_else(|| format!("{key} required"));
    let endpoint = get("--rpc")?;
    let block: Hash64 = get("--pinned-child")?.parse().map_err(|e| format!("child: {e}"))?;
    let parent_pin: Hash64 = get("--pinned-selected-parent")?.parse().map_err(|e| format!("parent: {e}"))?;
    let from: Hash64 = get("--pinned-provenance-start")?.parse().map_err(|e| format!("provenance start: {e}"))?;
    let job: Hash64 = get("--job")?.parse().map_err(|e| format!("job: {e}"))?;
    let network = get("--network")?;
    let genesis = get("--genesis")?;
    let operator = get("--operator-id")?;
    let database = get("--node-database-id")?;
    let out = PathBuf::from(get("--out")?);
    drop(get);
    // Owning ruleset: an isolated calibration34 snapshot (all three pins together),
    // or — with none of them — the shipped preset named by --network.
    let owning_params_sha = args.remove("--own-params-sha256");
    let owning_params_file = args.remove("--own-params-snapshot34").map(PathBuf::from);
    let owning_params_root = args
        .remove("--own-params-snapshot-root")
        .map(|v| v.parse::<Hash64>().map_err(|e| format!("own snapshot root: {e}")))
        .transpose()?;
    if [owning_params_sha.is_some(), owning_params_file.is_some(), owning_params_root.is_some()].iter().filter(|v| **v).count() % 3
        != 0
    {
        return Err("--own-params-snapshot34, --own-params-snapshot-root and --own-params-sha256 go together".into());
    }
    let redemption = match args.remove("--redemption").as_deref() {
        None | Some("false") => false,
        Some("true") => true,
        _ => return Err("--redemption requires literal true or false".into()),
    };
    // Optional spend35: the payee's own spending transaction (local file) plus the
    // chain block that accepted it; acceptance ids come from the node and are
    // authenticated later by palw-class against the pinned headers only.
    let spend_chain =
        args.remove("--spend-chain-block").map(|v| v.parse::<Hash64>().map_err(|e| format!("spend chain: {e}"))).transpose()?;
    let spend_tx = args.remove("--spend-tx-json").map(PathBuf::from);
    // delivery33's post-payment anchor: the own-chain child of the payment block.
    let post_pin =
        args.remove("--pinned-post-header").map(|v| v.parse::<Hash64>().map_err(|e| format!("post header: {e}"))).transpose()?;
    if post_pin.is_some() && !redemption {
        return Err("--pinned-post-header requires --redemption true".into());
    }
    if spend_chain.is_some() != spend_tx.is_some() || (spend_chain.is_some() && !redemption) {
        return Err("spend35 needs --spend-chain-block, --spend-tx-json and --redemption true together".into());
    }
    // Register/seat anchors need only the full readonly aux opening of the pinned child;
    // `--job` then names the class (register) or funded job (seat), never a Final.
    let lifecycle = match args.remove("--lifecycle-only").as_deref() {
        None | Some("false") => false,
        Some("true") => true,
        _ => return Err("--lifecycle-only requires literal true or false".into()),
    };
    if lifecycle && redemption {
        return Err("--lifecycle-only excludes --redemption".into());
    }
    let max_blocks = args.remove("--max-blocks").map(|s| s.parse::<usize>().map_err(|e| e.to_string())).transpose()?.unwrap_or(512);
    let timeout = args.remove("--timeout-seconds").map(|s| s.parse::<u64>().map_err(|e| e.to_string())).transpose()?.unwrap_or(30);
    if !args.is_empty()
        || out.exists()
        || max_blocks == 0
        || max_blocks > 65536
        || timeout == 0
        || timeout > 60
        || operator.is_empty()
        || database.is_empty()
    {
        return Err("bounded fetch profile/fresh output required".into());
    }
    let network_id: kaspa_consensus_core::network::NetworkId = network.parse().map_err(|e| format!("own network: {e}"))?;
    let base: kaspa_consensus_core::config::params::Params = network_id.into();
    let owning = match (&owning_params_file, owning_params_root, &owning_params_sha) {
        (Some(file), Some(root), Some(sha)) => misaka_palw_sdk::native_params_v34::load_own_native_parameters_v34(
            &base,
            file,
            root.as_bytes(),
            misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(sha)?,
        )?,
        _ => misaka_palw_sdk::native_params_v34::OwnedNativeParametersV34::shipped_preset(base),
    };
    if owning.parameters().genesis.hash.to_string() != genesis || owning.parameters().net.to_string() != network {
        return Err("predeclared owning snapshot derives another network/genesis".into());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().map_err(|e| e.to_string())?;
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&endpoint), None, None, None).map_err(|e| e.to_string())?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(timeout)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    runtime.block_on(client.connect(Some(options))).map_err(|e| e.to_string())?;
    // Every individual RPC is bounded by a wall-clock timeout. Response facts
    // are retained as evidence and must be independently replayed/verified.
    let (audit, payout, child_block, parent_block, status, info) = runtime.block_on(async {
        let audit = tokio::time::timeout(
            Duration::from_secs(timeout),
            client.get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                block_hash: block.to_string(),
                collection: if lifecycle {
                    "kernel-aux-v32".to_string()
                } else {
                    format!("{}:{job}", if redemption { "native-redemption-v33" } else { "native-audit-v27" })
                },
            }),
        )
        .await
        .map_err(|_| "audit RPC timeout".to_string())?
        .map_err(|e| e.to_string())?;
        let payout = tokio::time::timeout(
            Duration::from_secs(timeout),
            client.get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                block_hash: block.to_string(),
                collection: "pending-payouts-v32".into(),
            }),
        )
        .await
        .map_err(|_| "payout RPC timeout".to_string())?
        .map_err(|e| e.to_string())?;
        let child = tokio::time::timeout(Duration::from_secs(timeout), client.get_block(block, true))
            .await
            .map_err(|_| "child timeout".to_string())?
            .map_err(|e| e.to_string())?;
        let parent = tokio::time::timeout(Duration::from_secs(timeout), client.get_block(parent_pin, true))
            .await
            .map_err(|_| "parent timeout".to_string())?
            .map_err(|e| e.to_string())?;
        let status = tokio::time::timeout(Duration::from_secs(timeout), client.get_palw_node_status())
            .await
            .map_err(|_| "status timeout".to_string())?
            .map_err(|e| e.to_string())?;
        let info = tokio::time::timeout(Duration::from_secs(timeout), client.get_block_dag_info())
            .await
            .map_err(|_| "DAG timeout".to_string())?
            .map_err(|e| e.to_string())?;
        Ok::<_, String>((audit, payout, child, parent, status, info))
    })?;
    if !audit.available
        || !payout.available
        || audit.block_hash != block.to_string()
        || payout.block_hash != block.to_string()
        || audit.collection != "kernel-aux-v32"
        || payout.collection != "pending-payouts-v32"
        || status.genesis_hash != genesis
        || info.network.to_string() != network
        || child_block.verbose_data.as_ref().is_none_or(|v| v.selected_parent_hash != parent_pin)
        // The operator's OWN independently replayed node must hold the pinned child on its
        // selected chain; a side-DAG block's state opening is not this chain's Final.
        || child_block.verbose_data.as_ref().is_none_or(|v| !v.is_chain_block || v.is_header_only)
    {
        return Err("node returned another network/genesis/immutable anchor/collection or unavailable validated audit".into());
    }
    // Uniform mixing refusal: the node's own announced fingerprint/schedule/genesis must be exactly
    // this auditor's reconstruction from the same pinned snapshot (another network, snapshot or
    // policy value refuses here, before any opening is retained).
    owning.check_node_identity(&status.consensus_params_id, &status.consensus_schedule_id, &status.genesis_hash)?;
    let child = Header::try_from(&child_block.header).map_err(|e| e.to_string())?;
    let parent = Header::try_from(&parent_block.header).map_err(|e| e.to_string())?;
    if kaspa_consensus_core::hashing::header::hash(&child) != block
        || kaspa_consensus_core::hashing::header::hash(&parent) != parent_pin
        || audit
            .header
            .as_ref()
            .is_none_or(|h| Header::try_from(h).ok().is_none_or(|h| kaspa_consensus_core::hashing::header::hash(&h) != block))
        || payout
            .header
            .as_ref()
            .is_none_or(|h| Header::try_from(h).ok().is_none_or(|h| kaspa_consensus_core::hashing::header::hash(&h) != block))
    {
        return Err("native fetch headers differ from original independent pins".into());
    }
    let record = if lifecycle {
        None
    } else {
        let record = audit.native_audit_record.as_ref().ok_or("node did not export actual validated native audit record")?;
        kaspa_consensus_core::palw_native_audit_v27::NativeExecutionAuditRecordV27::from_bytes(record)?;
        Some(record)
    };
    let fact = |response: &kaspa_rpc_core::GetPalwStateProofResponse| PalwFactProofV1 {
        state: PalwStateOpeningV1 { preimage: response.state_preimage.clone() },
        collection: PalwCollectionOpeningV1 {
            label: response.collection.as_bytes().to_vec(),
            rows: response.rows.iter().map(|r| (r.key.clone(), r.value.clone())).collect(),
        },
    };
    let aux = encode_native_fact_proof_v32(&fact(&audit))?;
    let payouts = encode_native_fact_proof_v32(&fact(&payout))?;
    std::fs::create_dir(&out).map_err(|e| e.to_string())?;
    let mut evidence = Vec::new();
    let mut total = 0usize;
    fn keep(
        out: &std::path::Path,
        name: &str,
        raw: &[u8],
        total: &mut usize,
        evidence: &mut Vec<serde_json::Value>,
    ) -> Result<(), String> {
        *total = total.checked_add(raw.len()).filter(|v| *v <= 512 << 20).ok_or("native fetch aggregate retained bytes cap")?;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(out.join(name)).map_err(|e| e.to_string())?;
        file.write_all(raw).map_err(|e| e.to_string())?;
        evidence.push(serde_json::json!({"path":name,"bytes":raw.len(),"sha256":hex(&Sha256::digest(raw))}));
        Ok(())
    }
    keep(&out, "child-header.json", &serde_json::to_vec(&child).map_err(|e| e.to_string())?, &mut total, &mut evidence)?;
    keep(&out, "selected-parent-header.json", &serde_json::to_vec(&parent).map_err(|e| e.to_string())?, &mut total, &mut evidence)?;
    if let Some(record) = record {
        keep(&out, "native-record.borsh", record, &mut total, &mut evidence)?;
    }
    if redemption {
        let raw = audit.native_coinbase_proof.as_ref().ok_or("node omitted actual validated coinbase inclusion proof")?;
        misaka_palw_sdk::native_redemption_v33::decode_coinbase_proof_v33(raw)?;
        keep(&out, "native-coinbase-proof33.borsh", raw, &mut total, &mut evidence)?;
    }
    if let Some(post_hash) = post_pin {
        let (post_block, post_payout) = runtime.block_on(async {
            let post_block = tokio::time::timeout(Duration::from_secs(timeout), client.get_block(post_hash, false))
                .await
                .map_err(|_| "post header timeout".to_string())?
                .map_err(|e| e.to_string())?;
            let post_payout = tokio::time::timeout(
                Duration::from_secs(timeout),
                client.get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                    block_hash: post_hash.to_string(),
                    collection: "pending-payouts-v32".into(),
                }),
            )
            .await
            .map_err(|_| "post payout RPC timeout".to_string())?
            .map_err(|e| e.to_string())?;
            Ok::<_, String>((post_block, post_payout))
        })?;
        let verbose = post_block.verbose_data.as_ref().ok_or("post block omits selected parent")?;
        let post_header = Header::try_from(&post_block.header).map_err(|e| e.to_string())?;
        if verbose.selected_parent_hash != block
            || !verbose.is_chain_block
            || kaspa_consensus_core::hashing::header::hash(&post_header) != post_hash
            || !post_payout.available
            || post_payout.collection != "pending-payouts-v32"
            || post_payout.block_hash != post_hash.to_string()
        {
            return Err("post-payment anchor is not the own-chain child of the payment block".into());
        }
        keep(&out, "post-header.json", &serde_json::to_vec(&post_header).map_err(|e| e.to_string())?, &mut total, &mut evidence)?;
        keep(&out, "post-payout-proof32.borsh", &encode_native_fact_proof_v32(&fact(&post_payout))?, &mut total, &mut evidence)?;
    }
    if let (Some(chain_hash), Some(tx_path)) = (spend_chain, spend_tx.as_ref()) {
        let raw = misaka_palw_sdk::native_audit_io_v32::read_bounded_file_v32(tx_path, 8 << 20)?;
        let spend: kaspa_consensus_core::tx::Transaction = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let (chain_block, chain_parent, accepted) = runtime.block_on(async {
            let chain_block = tokio::time::timeout(Duration::from_secs(timeout), client.get_block(chain_hash, false))
                .await
                .map_err(|_| "spend chain timeout".to_string())?
                .map_err(|e| e.to_string())?;
            let verbose = chain_block.verbose_data.as_ref().ok_or("spend chain block omits selected parent")?;
            if !verbose.is_chain_block {
                return Err("spend35 accepting block is not on the own node's selected chain".to_string());
            }
            let parent_hash = verbose.selected_parent_hash;
            let chain_parent = tokio::time::timeout(Duration::from_secs(timeout), client.get_block(parent_hash, false))
                .await
                .map_err(|_| "spend selected-parent timeout".to_string())?
                .map_err(|e| e.to_string())?;
            let chain =
                tokio::time::timeout(Duration::from_secs(timeout), client.get_virtual_chain_from_block(parent_hash, true, None))
                    .await
                    .map_err(|_| "spend acceptance timeout".to_string())?
                    .map_err(|e| e.to_string())?;
            let accepted = chain
                .accepted_transaction_ids
                .into_iter()
                .find(|a| a.accepting_block_hash == chain_hash)
                .ok_or("node reports no acceptance for the requested spend chain block")?
                .accepted_transaction_ids;
            Ok::<_, String>((chain_block, chain_parent, accepted))
        })?;
        let chain_header = Header::try_from(&chain_block.header).map_err(|e| e.to_string())?;
        let parent_header = Header::try_from(&chain_parent.header).map_err(|e| e.to_string())?;
        let proof = kaspa_consensus_core::palw_native_spend_v35::prove_native_accepted_spend_v35(
            chain_hash,
            kaspa_consensus_core::hashing::header::hash(&parent_header),
            accepted,
            spend,
        )?;
        keep(
            &out,
            "spend-chain-header.json",
            &serde_json::to_vec(&chain_header).map_err(|e| e.to_string())?,
            &mut total,
            &mut evidence,
        )?;
        keep(
            &out,
            "spend-selected-parent-header.json",
            &serde_json::to_vec(&parent_header).map_err(|e| e.to_string())?,
            &mut total,
            &mut evidence,
        )?;
        keep(
            &out,
            "native-accepted-spend35.json",
            &serde_json::to_vec(&proof).map_err(|e| e.to_string())?,
            &mut total,
            &mut evidence,
        )?;
    }
    keep(&out, "aux-proof32.borsh", &aux, &mut total, &mut evidence)?;
    keep(&out, "payout-proof32.borsh", &payouts, &mut total, &mut evidence)?;
    let mut current = child_block;
    let mut reached = false;
    for index in 0..max_blocks {
        let current_header = Header::try_from(&current.header).map_err(|e| e.to_string())?;
        let hash = kaspa_consensus_core::hashing::header::hash(&current_header);
        let raw = serde_json::to_vec(&current).map_err(|e| e.to_string())?;
        if raw.len() > 16 << 20 {
            return Err("native fetch individual full block cap".into());
        }
        keep(&out, &format!("block-{index:05}.json"), &raw, &mut total, &mut evidence)?;
        if hash == from {
            reached = true;
            break;
        }
        let next = current.verbose_data.as_ref().ok_or("block omits selected-parent provenance")?.selected_parent_hash;
        if !current_header.parents_by_level.get(0).is_some_and(|parents| parents.contains(&next)) {
            return Err("node selected-parent provenance is outside authenticated parent set".into());
        }
        current = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(timeout), client.get_block(next, true))
                .await
                .map_err(|_| "provenance RPC timeout".to_string())?
                .map_err(|e| e.to_string())
        })?;
        let header = Header::try_from(&current.header).map_err(|e| e.to_string())?;
        if kaspa_consensus_core::hashing::header::hash(&header) != next {
            return Err("provenance block hash differs".into());
        }
    }
    let report = serde_json::json!({"schema":"misaka.palw.public-native-anchor-collection.v32","rpc":endpoint,
        "network":network,"genesis":genesis,"operator_id":operator,"node_database":database,"own_params_sha256":owning_params_sha,"own_params_snapshot_root":owning_params_root.map(|v| v.to_string()),
        "node_consensus_params_id":status.consensus_params_id,"node_consensus_schedule_id":status.consensus_schedule_id,
        "measurement_final_only":!owning.rfc02_qualified(),
        "qualified_native_profile":owning.rfc02_qualified(),"calibration_receipts_are_not_A6_or_A8_evidence":!owning.rfc02_qualified(),
        "pinned_child":block.to_string(),"pinned_selected_parent":parent_pin.to_string(),"original_job":job.to_string(),"coinbase_delivery_proof_requested":redemption,"lifecycle_only":lifecycle,
        "provenance_start":from.to_string(),"complete_requested_provenance_retained":reached,"artifacts":evidence,
        "total_retained_bytes":total,"raw_node_assertions_are_unverified":true,"canonical_blocks_verified":false,
        "pinned_child_on_own_node_selected_chain":true,"spend35_collected":spend_chain.is_some(),
        "native_final_verified":false,"utxo_redemption_verified":false,"A6_or_A8_credit":false});
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join("collection.json"))
        .map_err(|e| e.to_string())?
        .write_all(serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?.as_slice())
        .map_err(|e| e.to_string())?;
    println!("{report}");
    if !reached {
        return Err("native provenance reached caller's explicit bound before original pinned start".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("palw-native-audit-fetch: {error}");
        std::process::exit(1)
    }
}
