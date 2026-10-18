//! Own private process instrumentation only. Receiving this JSON from another
//! node/fixture supplies no authority. Every base has a separately coherent
//! snapshot; consumers must match its current sink/root/import generation.
use super::kernel_paid_producer_v4::{owned_dir, regular};
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::{
    Hash64, palw_native_resource_v1::reserve_native_requests_v1, palw_paid_claim_observation_v5::NativePaidClaimObservationV5,
};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    time::Duration,
};

pub(super) const DIRECTORY: &str = "rfc08-native-paid-observations-v5";
pub const MAX_OBSERVED_PAID_CLAIMS_V5: usize = 64;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    schema_version: u16,
    kind: &'static str,
    consensus_params_id: &'a str,
    genesis: &'a str,
    base_claim: Hash64,
    available: bool,
    observation: Option<&'a NativePaidClaimObservationV5>,
    error: Option<&'a str>,
    own_process_observation_only: bool,
    release_approval: bool,
    security_soundness_measured: bool,
}

fn publish(directory: &Path, base: Hash64, envelope: &Envelope<'_>) -> Result<(), String> {
    let destination = directory.join(format!("{base}.json"));
    let temporary = directory.join(format!("{base}.pending"));
    if destination.exists() {
        regular(&destination)?;
    }
    if temporary.exists() {
        regular(&temporary)?;
        std::fs::remove_file(&temporary).map_err(|why| why.to_string())?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|why| why.to_string())?;
    // Keep the native owner alive through streaming JSON and file fsync.
    serde_json::to_writer(&mut file, envelope).map_err(|why| why.to_string())?;
    file.write_all(b"\n").and_then(|_| file.sync_all()).map_err(|why| why.to_string())?;
    std::fs::rename(&temporary, &destination).map_err(|why| why.to_string())?;
    File::open(directory).and_then(|parent| parent.sync_all()).map_err(|why| why.to_string())
}

pub(super) fn start(ctx: &PalwNativeCustodyContextV3, bases: &[Hash64]) -> Result<(), String> {
    if ctx.config.params.palw_claim_challenge_v2.is_none() {
        return Err("private paid observations require explicit paid capability".into());
    }
    if bases.is_empty()
        || bases.len() > MAX_OBSERVED_PAID_CLAIMS_V5
        || bases.iter().enumerate().any(|(index, base)| *base == Hash64::default() || bases[..index].contains(base))
    {
        return Err("private paid observations need 1..64 distinct nonzero explicit base claims".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|why| why.to_string())?;
    let extent = (ctx.data_dir.as_os_str().len() as u64)
        .checked_add(1024)
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add((bases.len() * std::mem::size_of::<Hash64>()) as u64 + 16384))
        .ok_or("private paid observation path/list workspace overflow")?;
    let workspace = reserve_native_requests_v1(extent).map_err(|why| why.to_string())?;
    let mut claims = Vec::new();
    claims.try_reserve_exact(bases.len()).map_err(|why| why.to_string())?;
    claims.extend_from_slice(bases);
    let directory = ctx.data_dir.join(DIRECTORY);
    owned_dir(&directory)?;
    let params = ctx.config.params.consensus_params_id().to_string();
    let genesis = ctx.config.params.genesis.hash.to_string();
    for base in &claims {
        publish(
            &directory,
            *base,
            &Envelope {
                schema_version: 5,
                kind: "rfc08-private-native-paid-claim-observation-v5",
                consensus_params_id: &params,
                genesis: &genesis,
                base_claim: *base,
                available: false,
                observation: None,
                error: Some("awaiting this process's coherent native paid observation"),
                own_process_observation_only: true,
                release_approval: false,
                security_soundness_measured: false,
            },
        )?;
    }
    let manager = ctx.consensus_manager.clone();
    let tick = ctx.tick_service.clone();
    std::thread::Builder::new()
        .name("palw-private-paid-observations-v5".into())
        .spawn(move || {
            runtime.block_on(async move {
                let _workspace = workspace;
                loop {
                    if matches!(tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                        break;
                    }
                    for base in &claims {
                        let instance = manager.consensus();
                        let api = instance.unguarded_session_blocking();
                        let observed = api.palw_paid_claim_observation_v5(*base);
                        let error = observed.as_ref().err().map(|why| {
                            if why.len() <= 4096 { why.as_str() } else { "native paid observation error exceeds bounded status text" }
                        });
                        let envelope = Envelope {
                            schema_version: 5,
                            kind: "rfc08-private-native-paid-claim-observation-v5",
                            consensus_params_id: &params,
                            genesis: &genesis,
                            base_claim: *base,
                            available: observed.is_ok(),
                            observation: observed.as_ref().ok().map(|value| &**value),
                            error,
                            own_process_observation_only: true,
                            release_approval: false,
                            security_soundness_measured: false,
                        };
                        if let Err(error) = publish(&directory, *base, &envelope) {
                            kaspa_core::warn!("private paid observation publication: {error}");
                        }
                    }
                }
            });
        })
        .map_err(|why| why.to_string())?;
    Ok(())
}

#[cfg(test)]
mod source_controls {
    use super::*;
    #[test]
    fn owned_status_files_use_exact_base_and_never_advertise_release_or_security_approval() {
        let directory = tempfile::tempdir().unwrap();
        let directory = directory.path().canonicalize().unwrap();
        let base = Hash64::from_bytes([1; 64]);
        let envelope = Envelope {
            schema_version: 5,
            kind: "rfc08-private-native-paid-claim-observation-v5",
            consensus_params_id: "private-rules",
            genesis: "private-genesis",
            base_claim: base,
            available: false,
            observation: None,
            error: Some("own native import incomplete"),
            own_process_observation_only: true,
            release_approval: false,
            security_soundness_measured: false,
        };
        publish(&directory, base, &envelope).unwrap();
        let bytes = std::fs::read(directory.join(format!("{base}.json"))).unwrap();
        let read: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(read["available"], false);
        assert_eq!(read["observation"], serde_json::Value::Null);
        assert_eq!(read["ownProcessObservationOnly"], true);
        assert_eq!(read["releaseApproval"], false);
        assert_eq!(read["securitySoundnessMeasured"], false);
        assert!(!directory.join(format!("{base}.pending")).exists());
    }
}
