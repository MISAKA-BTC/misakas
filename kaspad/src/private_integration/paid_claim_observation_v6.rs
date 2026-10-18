//! Separate aux138 observation sidecar. Same explicit base list as V5; no
//! interpretation of a V5 file as approved-bootstrap source provenance.
use super::{
    kernel_paid_producer_v4::owned_dir,
    paid_claim_observation_v5::{MAX_OBSERVED_PAID_CLAIMS_V5, publish},
};
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::{
    Hash64, palw_native_resource_v1::reserve_native_requests_v1, palw_paid_claim_observation_v6::NativePaidBootstrapObservationV6,
};
use std::time::Duration;
pub(super) const DIRECTORY: &str = "rfc08-native-paid-bootstrap-observations-v6";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    schema_version: u16,
    kind: &'static str,
    consensus_params_id: &'a str,
    genesis: &'a str,
    base_claim: Hash64,
    available: bool,
    observation: Option<&'a NativePaidBootstrapObservationV6>,
    error: Option<&'a str>,
    own_process_observation_only: bool,
    release_approval: bool,
    security_soundness_measured: bool,
}
pub(super) fn start(ctx: &PalwNativeCustodyContextV3, bases: &[Hash64]) -> Result<(), String> {
    if ctx.config.params.palw_paid_bootstrap_source_v6.is_none() {
        return Err("private V6 observations require explicit V6 source policy".into());
    }
    if bases.is_empty()
        || bases.len() > MAX_OBSERVED_PAID_CLAIMS_V5
        || bases.iter().enumerate().any(|(i, b)| *b == Hash64::default() || bases[..i].contains(b))
    {
        return Err("private V6 observations require 1..64 distinct nonzero exact claims".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|why| why.to_string())?;
    let extent = (ctx.data_dir.as_os_str().len() as u64)
        .checked_add(1024)
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add((bases.len() * std::mem::size_of::<Hash64>()) as u64 + 16384))
        .ok_or("private V6 observation path/list extent overflow")?;
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
                schema_version: 6,
                kind: "rfc08-private-native-paid-bootstrap-observation-v6",
                consensus_params_id: &params,
                genesis: &genesis,
                base_claim: *base,
                available: false,
                observation: None,
                error: Some("awaiting this process's coherent native V6 observation"),
                own_process_observation_only: true,
                release_approval: false,
                security_soundness_measured: false,
            },
        )?;
    }
    let manager = ctx.consensus_manager.clone();
    let tick = ctx.tick_service.clone();
    std::thread::Builder::new()
        .name("palw-private-paid-observations-v6".into())
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
                        let observed = api.palw_paid_bootstrap_observation_v6(*base);
                        let error = observed.as_ref().err().map(|why| {
                            if why.len() <= 4096 { why.as_str() } else { "V6 observation error exceeds bounded status text" }
                        });
                        let envelope = Envelope {
                            schema_version: 6,
                            kind: "rfc08-private-native-paid-bootstrap-observation-v6",
                            consensus_params_id: &params,
                            genesis: &genesis,
                            base_claim: *base,
                            available: observed.is_ok(),
                            observation: observed.as_ref().ok().map(|v| &**v),
                            error,
                            own_process_observation_only: true,
                            release_approval: false,
                            security_soundness_measured: false,
                        };
                        if let Err(why) = publish(&directory, *base, &envelope) {
                            kaspa_core::warn!("private V6 observation publication: {why}");
                        }
                    }
                }
            });
        })
        .map_err(|why| why.to_string())?;
    Ok(())
}
