//! RFC12 request generator: local factory rules authenticate a live pinned snapshot.
use kaspa_consensus_core::{Hash64, config::Config, palw_state_v2::PalwStateCarriageV2};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use misaka_palw_sdk::claim_paid_v2::all_target_bootstrap::{NativePaidProducerInputsV3, NativePaidRequestV4};
use std::{
    io::{Read, Write},
    path::Path,
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Spec {
    base_job: Hash64,
    paid_class: Hash64,
    paid_job: Hash64,
    material_consent: Hash64,
    not_before_daa: u64,
    salt: Hash64,
    producer_card: u32,
}
fn bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    std::fs::File::open(path).map_err(|e| e.to_string())?.take(maximum + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > maximum {
        return Err("private request input exceeds its file bound".into());
    }
    Ok(data)
}

pub fn generate_private_paid_request_v4(
    config: &Config,
    rpc: &str,
    block: &str,
    spec: &Path,
    inputs: &Path,
    output: &Path,
) -> Result<serde_json::Value, String> {
    let pinned: Hash64 = block.parse().map_err(|e| format!("pinned block: {e}"))?;
    let spec: Spec = serde_json::from_slice(&bounded(spec, 1 << 20)?).map_err(|e| e.to_string())?;
    let input_bytes = bounded(inputs, 64 << 20)?;
    super::claim_paid_bootstrap::preflight_paid_inputs_v4(&input_bytes)?;
    let inputs: NativePaidProducerInputsV3 = borsh::from_slice(&input_bytes).map_err(|e| e.to_string())?;
    misaka_palw_sdk::claim_paid_v2::all_target_bootstrap::validate_input_v3(&inputs).map_err(|e| e.to_string())?;
    drop(input_bytes);
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("private request generator needs a drill salt")?;
    if spec.producer_card as usize >= kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt).len() {
        return Err("private request names an unknown producer card".into());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e| e.to_string())?;
    let verified = runtime.block_on(async {
        let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(rpc), None, None, None).map_err(|e| e.to_string())?;
        client
            .connect(Some(ConnectOptions {
                block_async_connect: true,
                connect_timeout: Some(Duration::from_secs(15)),
                strategy: ConnectStrategy::Fallback,
                ..Default::default()
            }))
            .await
            .map_err(|e| e.to_string())?;
        let status = client.get_palw_node_status().await.map_err(|e| e.to_string())?;
        if status.genesis_hash != config.params.genesis.hash.to_string()
            || status.consensus_params_id != config.params.consensus_params_id().to_string()
        {
            return Err("request RPC runs another private genesis or ruleset".to_string());
        }
        let result = misaka_palw_sdk::state_carriage_rpc::fetch_state_carriage_v1(&client, &config.params, pinned).await;
        let _ = client.disconnect().await;
        result
    })?;
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&verified.carriage).map_err(|e| e.to_string())?;
    let route = carriage.kernel_route.ok_or("the pinned native state has no kernel route")?;
    let request = NativePaidRequestV4 {
        version: 4,
        header: borsh::to_vec(&route.header).map_err(|e| e.to_string())?,
        rows: route.rows.iter().chain(route.aux.iter()).map(|((t, k), v)| (*t, k.clone(), v.clone())).collect(),
        ledger_root: route.ledger_root(),
        aux_root: route.aux_root(),
        base_job: spec.base_job.as_bytes(),
        paid_class: spec.paid_class.as_bytes(),
        paid_job: spec.paid_job.as_bytes(),
        material_consent: spec.material_consent.as_bytes(),
        not_before_daa: spec.not_before_daa,
        salt: spec.salt.as_bytes(),
        producer_card: spec.producer_card,
        inputs,
    };
    let bytes = borsh::to_vec(&request).map_err(|e| e.to_string())?;
    if bytes.len() > 64 << 20 {
        return Err("paid request exceeds the exporter's 64 MiB limit".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(output).map_err(|e| e.to_string())?;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::File::open(parent).and_then(|dir| dir.sync_all()).map_err(|e| e.to_string())?;
    }
    Ok(serde_json::json!({
        "schema":"misaka.private-paid-native-request.v4", "out":output.display().to_string(), "bytes":bytes.len(),
        "block":pinned.to_string(), "parent":verified.parent.to_string(), "stateRoot":verified.state_root.to_string(),
        "ledgerRoot":request.ledger_root.to_string(), "auxRoot":request.aux_root.to_string(),
        "verification":"caller-pinned-live-header", "releaseApproval":false,
    }))
}
