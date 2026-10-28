//! Private-only owned-process instrumentation. A file is an observation from
//! this child, never a public certificate or provider GC authority.
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::palw_material_recovery_observation_v4::NativeRecoveryObservationV4;
use std::{
    fs::{File, OpenOptions},
    path::Path,
    time::Duration,
};

pub(super) const FILE: &str = "rfc08-native-recovery-observation-v4.json";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    schema_version: u16,
    kind: &'static str,
    consensus_params_id: &'a str,
    genesis: &'a str,
    available: bool,
    observation: Option<&'a NativeRecoveryObservationV4>,
    error: Option<&'a str>,
    release_approval: bool,
    security_soundness_measured: bool,
}
fn publish(directory: &Path, value: &Envelope<'_>) -> Result<(), String> {
    let temporary = directory.join(format!("{FILE}.pending"));
    let destination = directory.join(FILE);
    let mut file = OpenOptions::new().create(true).truncate(true).write(true).open(&temporary).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, &destination).map_err(|e| e.to_string())?;
    File::open(directory).and_then(|parent| parent.sync_all()).map_err(|e| e.to_string())
}

pub(super) fn start(ctx: &PalwNativeCustodyContextV3) -> Result<(), String> {
    if ctx.config.params.palw_material_release_recovery_v2.is_none() {
        return Err("private recovery observations need the explicit recovery-v2 profile".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let manager = ctx.consensus_manager.clone();
    let tick = ctx.tick_service.clone();
    let directory = ctx.data_dir.clone();
    let params = ctx.config.params.consensus_params_id().to_string();
    let genesis = ctx.config.params.genesis.hash.to_string();
    let request = u64::try_from(directory.as_os_str().as_encoded_bytes().len())
        .ok()
        .and_then(|n| n.checked_add((FILE.len() * 2) as u64))
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(4096))
        .ok_or("private observation path request overflow")?;
    // Reserve status publication independently of a potentially refused state
    // read. Invalidate any pre-restart positive file before starting the worker.
    let workspace = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(request).map_err(|e| e.to_string())?;
    publish(
        &directory,
        &Envelope {
            schema_version: 4,
            kind: "rfc08-private-native-recovery-observation-v4",
            consensus_params_id: &params,
            genesis: &genesis,
            available: false,
            observation: None,
            error: Some("awaiting this process's coherent native observation"),
            release_approval: false,
            security_soundness_measured: false,
        },
    )?;
    std::thread::Builder::new()
        .name("palw-private-recovery-observation-v4".into())
        .spawn(move || {
            runtime.block_on(async move {
                let _workspace = workspace;
                loop {
                    if matches!(tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                        break;
                    }
                    // Metadata is streamed to File, not expanded into a JSON Value.
                    // Keep its native owner through serialization/fsync.
                    let instance = manager.consensus();
                    let api = instance.unguarded_session_blocking();
                    let observation = api.palw_material_recovery_observation_v4();
                    let envelope = Envelope {
                        schema_version: 4,
                        kind: "rfc08-private-native-recovery-observation-v4",
                        consensus_params_id: &params,
                        genesis: &genesis,
                        available: observation.is_ok(),
                        observation: observation.as_ref().ok().map(|row| &**row),
                        error: observation.as_ref().err().map(String::as_str),
                        release_approval: false,
                        security_soundness_measured: false,
                    };
                    if let Err(error) = publish(&directory, &envelope) {
                        kaspa_core::warn!("private native observation publication: {error}");
                    }
                }
            });
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
