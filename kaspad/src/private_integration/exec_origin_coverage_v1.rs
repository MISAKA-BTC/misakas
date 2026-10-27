//! Private-only RFC-0006 historical-header coverage observation. Every minute this child writes, from its OWN store at its
//! own tip, how the eligible legacy claims' original headers stand against the EXEC origin carriage cap
//! (`palw_exec_origin_coverage_v1`). Run on an archive (non-pruning) node it is the coverage evidence the migration needs;
//! on a pruned node `unavailable` only says "not here". A file is an observation, never an activation or GC authority.
use super::exec_origin_coverage_bracket_v2::bracketed;
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use std::{
    fs::{File, OpenOptions},
    path::Path,
    time::Duration,
};

pub(super) const FILE: &str = "rfc06-exec-origin-coverage-v2.json";
/// The number of claim ids each list keeps (totals are always complete).
const LISTED: usize = 256;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    schema_version: u16,
    kind: &'static str,
    consensus_params_id: &'a str,
    generation: Option<&'a Generation>,
    census: Option<&'a kaspa_consensus_core::palw_work_slice_v2::PalwExecOriginCoverageV1>,
    error: Option<&'a str>,
    activation_approval: bool,
}
/// V2 binds this advisory census to this node's own import epoch and native tip.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Generation {
    sink: String,
    virtual_daa_score: u64,
    import_epoch: u64,
}

fn generation(api: &dyn kaspa_consensus_core::api::ConsensusApi) -> Result<Generation, String> {
    let (before, stable) = api.pruning_import_status().map_err(|e| e.to_string())?;
    if !stable || before.active.is_some() {
        return Err("coverage import incomplete".into());
    }
    let sink = api.get_sink().to_string();
    let virtual_daa_score = api.get_virtual_daa_score();
    let (after, stable) = api.pruning_import_status().map_err(|e| e.to_string())?;
    if !stable || after.active.is_some() || before.epoch != after.epoch {
        return Err("coverage import changed while reading generation".into());
    }
    Ok(Generation { sink, virtual_daa_score, import_epoch: before.epoch })
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
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let manager = ctx.consensus_manager.clone();
    let tick = ctx.tick_service.clone();
    let directory = ctx.data_dir.clone();
    let params = ctx.config.params.consensus_params_id().to_string();
    std::thread::Builder::new()
        .name("private-exec-origin-coverage-v2".into())
        .spawn(move || {
            runtime.block_on(async move {
                loop {
                    if matches!(tick.tick(Duration::from_secs(60)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                        return;
                    }
                    let api = manager.consensus().unguarded_session();
                    let result =
                        api.with_consensus_api_v1(|api| bracketed(|| generation(api), || api.palw_exec_origin_coverage_v1(LISTED)));
                    let (generation, census, error) = match &result {
                        Ok((generation, census)) => (Some(generation), Some(&**census), None),
                        Err(e) => (None, None, Some(e.as_str())),
                    };
                    let envelope = Envelope {
                        schema_version: 2,
                        kind: "rfc06-private-exec-origin-coverage-v2",
                        consensus_params_id: &params,
                        generation,
                        census,
                        error,
                        activation_approval: false,
                    };
                    if let Err(e) = publish(&directory, &envelope) {
                        kaspa_core::warn!("private EXEC origin coverage observation not written: {e}");
                    }
                }
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
