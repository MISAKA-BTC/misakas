//! Readonly owning39 Job/Final and complete Master Final audit, and archive
//! saving, from one own-node anchored full aux stream (palw-native-audit-stream39).
//!
//! Modes:
//!   --mode job      --id <job>      : CheckedNativeJobAuditV39 factory
//!   --mode master   --id <master>   : complete Master Final (every component)
//!   --mode archive                  : copy live source/class/consent bodies
//!
//! The report is a data-identity and monetary-conservation audit of the
//! anchored state. It never states PASS, never asserts LLM correctness
//! (PRINCIPLES §8), and grants no Final, mining receipt or consensus weight.
use kaspa_consensus_core::{Hash64, config::params::Params, network::NetworkId, palw_state_v2::PalwChainStateV2};
use misaka_palw_sdk::{native_archive_v39::NativeArchiveDirV39, runtime_pack::commit::hex};
use std::{collections::BTreeMap, io::Write, path::PathBuf};

fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("flag value required")?).is_some() {
            return Err("final-audit39 distinct exact flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let network = get("--network")?;
    let genesis = get("--genesis")?;
    let root = PathBuf::from(get("--evidence-root")?);
    let index = PathBuf::from(get("--aux-index")?);
    let child_header = PathBuf::from(get("--child-header")?);
    let parent_header = PathBuf::from(get("--parent-header")?);
    let envelope = PathBuf::from(get("--envelope")?);
    let child = get("--pinned-child")?.parse::<Hash64>().map_err(|e| e.to_string())?;
    let parent = get("--pinned-selected-parent")?.parse::<Hash64>().map_err(|e| e.to_string())?;
    let mode = get("--mode")?;
    let out = PathBuf::from(get("--out")?);
    drop(get);
    let id = a.remove("--id").map(|v| v.parse::<Hash64>().map_err(|e| e.to_string())).transpose()?;
    let archive_dir = a.remove("--archive-dir");
    let snapshot = a.remove("--own-params-snapshot39");
    let pin = a.remove("--own-params-snapshot-root");
    let sha = a.remove("--own-params-sha256");
    if !a.is_empty() || out.exists() {
        return Err("final-audit39 unknown option or existing output".into());
    }
    let base: Params = network.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let own = match (snapshot, pin, sha) {
        (None, None, None) => misaka_palw_sdk::native_params_v39::OwnedNativeParametersV39::shipped_preset(base),
        (Some(f), Some(p), Some(s)) => misaka_palw_sdk::native_params_v39::load_own_native_parameters_v39(
            &base,
            std::path::Path::new(&f),
            p.parse::<Hash64>().map_err(|e| e.to_string())?.as_bytes(),
            misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&s)?,
        )?,
        _ => return Err("final-audit39 the owning snapshot needs its root and SHA256 pins together".into()),
    };
    let params = own.parameters();
    if params.genesis.hash.to_string() != genesis {
        return Err("final-audit39 own genesis differs".into());
    }
    let stream = misaka_palw_sdk::native_audit_io_v39::read_native_aux_stream_anchored_v39(
        &root,
        &index,
        &child_header,
        &parent_header,
        &envelope,
        child,
        parent,
        params,
    )?;
    let archive = match &archive_dir {
        Some(d) if mode == "archive" => Some(NativeArchiveDirV39::create(std::path::Path::new(d))?),
        Some(d) => Some(NativeArchiveDirV39::open(std::path::Path::new(d))?),
        None => None,
    };
    let archive_ref =
        archive.as_ref().map(|a| a as &dyn kaspa_consensus_core::palw_native_execution_v39::NativeOriginalSourceArchiveV39);
    let anchor = serde_json::json!({
        "child": child.to_string(),
        "selected_parent": parent.to_string(),
        "inner_state": stream.anchor().inner().to_string(),
        "auxiliary_root": stream.auxiliary().root().to_string(),
        "daa": stream.anchor().daa(),
    });
    let report = match mode.as_str() {
        "job" => {
            let id = id.ok_or("final-audit39 --id <job> required")?.as_bytes();
            let audit =
                PalwChainStateV2::native_job_audit_from_stream_v39(stream.auxiliary(), stream.anchor(), params, id, archive_ref)?;
            let book = audit.book();
            serde_json::json!({
                "schema": "misaka.palw.native-job-audit.v39",
                "anchor": anchor,
                "job": hex(&id),
                "funding": hex(&book.funding()),
                "class": hex(&audit.original().class),
                "source": hex(&audit.original().source),
                "maximum": hex(&audit.original().maximum),
                "maximum_positions": audit.original().maximum_positions,
                "terminal": format!("{:?}", audit.terminal()),
                "claim": audit.claim().map(|c| hex(&c.claim)),
                "actual_positions": audit.claim().map(|c| c.positions),
                "settlement": audit.settlement().map(|s| hex(&s.root())),
                "single_credit": audit.credit().map(|c| serde_json::json!({
                    "root": hex(&c.root()), "queue": c.queue.to_string(), "payout": c.payout.to_string(),
                    "amount": c.amount, "established": c.established})),
                "master": (audit.master() != [0; 64]).then(|| hex(&audit.master())),
                "stage": audit.stage(),
                "authority": "data-identity-and-monetary-conservation-only",
            })
        }
        "master" => {
            let id = id.ok_or("final-audit39 --id <master> required")?.as_bytes();
            match PalwChainStateV2::native_master_final_audit_from_stream_v39(
                stream.auxiliary(),
                stream.anchor(),
                params,
                id,
                archive_ref,
            )? {
                None => serde_json::json!({
                    "schema": "misaka.palw.native-master-final-audit.v39",
                    "anchor": anchor,
                    "master": hex(&id),
                    "aggregate_final": serde_json::Value::Null,
                    "named_status": "MASTER_FINAL_NOT_RECORDED",
                }),
                Some(m) => serde_json::json!({
                    "schema": "misaka.palw.native-master-final-audit.v39",
                    "anchor": anchor,
                    "master": hex(&id),
                    "aggregate_final": hex(&m.record().root()),
                    "complete_source": hex(&m.record().complete_source),
                    "request": hex(&m.record().request),
                    "single_credit_set": hex(&m.record().single_credit_set),
                    "established": m.record().established,
                    "components": m.record().components.iter().map(|c| serde_json::json!({
                        "stage": c.original.stage, "job": hex(&c.original.job), "claim": hex(&c.claim),
                        "single_credit": hex(&c.single_credit), "queue": c.queue.to_string(), "amount": c.amount,
                        "finalized": c.finalized})).collect::<Vec<_>>(),
                    "authority": "escrow-Final-data-identity-only; no mining receipt, no Final weight",
                }),
            }
        }
        "archive" => {
            let archive = archive.as_ref().ok_or("final-audit39 --archive-dir required for archive mode")?;
            let saved = misaka_palw_sdk::native_archive_v39::save_native_archive_from_stream_v39(stream.auxiliary(), archive)?;
            serde_json::json!({"schema":"misaka.palw.native-archive-save.v39","anchor":anchor,"saved":saved})
        }
        _ => return Err("final-audit39 --mode is job, master or archive".into()),
    };
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&out).map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("final-audit39 refused: {e}");
        std::process::exit(1);
    }
}
