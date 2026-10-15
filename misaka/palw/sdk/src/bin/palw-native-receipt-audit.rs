//! Offline A6 checker. Frozen intent and file pointers are data; accepted native
//! claims, Final32 credits and shared queue counters come from audit32 proofs.
use kaspa_consensus_core::{Hash64, config::params::Params, network::NetworkId};
use misaka_palw_sdk::{native_audit_io_v32::*, native_receipt_windows_v1::*};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    child: Hash64,
    selected_parent: Hash64,
    collected: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transcript {
    schema: String,
    points: Vec<Point>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StreamHeader {
    schema: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MasterWitness {
    master: Hash64,
    jobs: Vec<Hash64>,
    outputs: String,
    generated: String,
}
fn relative(base: &Path, name: &str) -> Result<PathBuf, String> {
    let p = Path::new(name);
    if p.is_absolute() || p.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err("receipt audit relative evidence path required".into());
    }
    let root = base.canonicalize().map_err(|e| e.to_string())?;
    let p = root.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !p.starts_with(root) {
        return Err("receipt audit evidence escapes root".into());
    }
    Ok(p)
}
fn line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut raw = Vec::new();
    let n = reader.take(4097).read_until(b'\n', &mut raw).map_err(|e| e.to_string())?;
    if n > 4096 {
        return Err("receipt stream pointer line exceeds 4096-byte cap".into());
    }
    Ok((n != 0).then_some(raw))
}
fn run() -> Result<(), String> {
    let mut options = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || options.insert(k, it.next().ok_or("receipt audit flag value missing")?).is_some() {
            return Err("receipt audit distinct flags required".into());
        }
    }
    let mut get = |k: &str| options.remove(k).ok_or_else(|| format!("{k} required"));
    let network = get("--network")?;
    let root = PathBuf::from(get("--evidence-root")?);
    let contract_path = PathBuf::from(get("--contract")?);
    let points_path = PathBuf::from(get("--points")?);
    let out = PathBuf::from(get("--out")?);
    drop(get);
    let points_format = options.remove("--points-format").unwrap_or_else(|| "array-v1".into());
    let masters_file = options.remove("--master-witnesses");
    let base: Params = network.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let file = options.remove("--own-params-snapshot34");
    let pin = options.remove("--own-params-snapshot-root");
    let sha = options.remove("--own-params-snapshot-sha256");
    let own = match (file, pin, sha) {
        (None, None, None) => misaka_palw_sdk::native_params_v34::OwnedNativeParametersV34::shipped_preset(base),
        (Some(file), Some(pin), Some(sha)) => misaka_palw_sdk::native_params_v34::load_own_native_parameters_v34(
            &base,
            Path::new(&file),
            pin.parse::<Hash64>().map_err(|e| e.to_string())?.as_bytes(),
            misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&sha)?,
        )?,
        _ => return Err("receipt audit all owning snapshot pins required together".into()),
    };
    if !options.is_empty() || out.exists() || !own.rfc02_qualified() {
        return Err("receipt audit exact flags/fresh report/qualified owning Params required".into());
    }
    let contract: ReceiptWindowContractV1 =
        serde_json::from_slice(&read_bounded_file_v32(&contract_path, 4 << 20)?).map_err(|e| e.to_string())?;
    contract.check()?;
    let mut descriptors = Vec::new();
    if let Some(name) = masters_file {
        descriptors = serde_json::from_slice::<Vec<MasterWitness>>(&read_bounded_file_v32(&relative(&root, &name)?, 4 << 20)?)
            .map_err(|e| e.to_string())?;
        if descriptors.len() > 512 {
            return Err("receipt Master32 witness count cap".into());
        }
        let expected: BTreeMap<_, _> = contract.windows.iter().flat_map(|w| &w.masters).map(|m| (m.master, m)).collect();
        let mut seen = BTreeSet::new();
        for d in &descriptors {
            let original = expected.get(&d.master).ok_or("receipt witness has no frozen original Master32")?;
            if !seen.insert(d.master) || d.jobs != original.components.iter().map(|c| c.job).collect::<Vec<_>>() {
                return Err("receipt witness changes original Master32 component order".into());
            }
            relative(&root, &d.outputs)?;
            relative(&root, &d.generated)?;
        }
        if seen.len() != expected.len() {
            return Err("receipt witness omits an original Master32".into());
        }
    } else if contract.windows.iter().any(|w| !w.masters.is_empty()) {
        return Err("receipt complete Master32 public witnesses required".into());
    }
    let mut tracked: BTreeSet<_> = contract.jobs().into_iter().collect();
    let mut accumulator = stream::ReceiptWindowAccumulatorV1::new(contract)?;
    let mut cache = misaka_palw_sdk::native_master_witness_v1::NativeMasterWitnessCacheV1::default();
    let mut consume = |point: Point| -> Result<(), String> {
        let folder = relative(&root, &point.collected)?;
        let child = read_header_v32(&folder.join("child-header.json"))?;
        let parent = read_header_v32(&folder.join("selected-parent-header.json"))?;
        let aux_manifest = folder.join("aux-proof32.cas.json.gz");
        let payout_manifest = folder.join("payout-proof32.cas.json.gz");
        let aux = if aux_manifest.exists() {
            read_native_fact_proof_cas_v1(&root, &aux_manifest)?
        } else {
            read_native_fact_proof_v32(&folder.join("aux-proof32.borsh"))?
        };
        let payouts = if payout_manifest.exists() {
            read_native_fact_proof_cas_v1(&root, &payout_manifest)?
        } else {
            read_native_fact_proof_v32(&folder.join("payout-proof32.borsh"))?
        };
        let jobs: Vec<_> = tracked.iter().copied().collect();
        let checked = AnchoredReceiptPointV1::derive_with_master_loader(
            &child,
            point.child,
            &parent,
            point.selected_parent,
            own.parameters(),
            &aux,
            &payouts,
            &jobs,
            descriptors.len(),
            |snapshot, index| {
                let d = &descriptors[index];
                if !d.jobs.iter().all(|id| snapshot.jobs().get(&id.as_bytes()).is_some_and(|j| j.terminal == "Final")) {
                    return Ok(None);
                }
                // One complete Master's public values at a time. Unused cached
                // values are evicted before decoding another model, so adding
                // more original jobs does not retain their tensors in memory.
                let decoded = cache.load_transient(&relative(&root, &d.outputs)?)?;
                if decoded.values().len() != d.jobs.len() {
                    return Err("receipt public output witness omits an original stage".into());
                }
                let generated: Vec<u32> = serde_json::from_slice(&read_bounded_file_v32(&relative(&root, &d.generated)?, 1 << 20)?)
                    .map_err(|e| e.to_string())?;
                if generated.len() > 65536 {
                    return Err("receipt original generated token witness cap".into());
                }
                Ok(Some(MasterReceiptWitnessV1 {
                    master: d.master,
                    jobs: d.jobs.iter().map(|id| id.as_bytes()).collect(),
                    values: decoded.shared_values(),
                    generated,
                }))
            },
        )?;
        tracked.extend(checked.tracked());
        accumulator.push(checked)
    };
    match points_format.as_str() {
        "array-v1" => {
            let transcript: Transcript =
                serde_json::from_slice(&read_bounded_file_v32(&points_path, 16 << 20)?).map_err(|e| e.to_string())?;
            if transcript.schema != "misaka.rfc02.receipt-state-transcript.v1" || transcript.points.len() > 65536 {
                return Err("receipt audit legacy transcript extent/schema".into());
            }
            for point in transcript.points {
                consume(point)?;
            }
        }
        "stream-v2" => {
            let mut reader = BufReader::new(std::fs::File::open(&points_path).map_err(|e| e.to_string())?);
            let header: StreamHeader =
                serde_json::from_slice(&line(&mut reader)?.ok_or("receipt stream header absent")?).map_err(|e| e.to_string())?;
            if header.schema != "misaka.rfc02.receipt-state-stream.v2" {
                return Err("receipt stream explicit schema required".into());
            }
            while let Some(raw) = line(&mut reader)? {
                let point: Point = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
                consume(point)?;
            }
        }
        _ => return Err("receipt audit unknown explicit pointer format".into()),
    }
    drop(consume);
    let mut report = accumulator.finish()?;
    report["network"] = network.into();
    report["genesis"] = own.parameters().genesis.hash.to_string().into();
    report["own_snapshot_root"] = misaka_palw_sdk::runtime_pack::commit::hex(&own.snapshot_root()).into();
    report["local_qualified_validation_network"] = own.parameters().native_final_is_local_qualified_v34().into();
    let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    if bytes.len() > 16 << 20 {
        return Err("receipt audit report exceeds 16 MiB explicit profile".into());
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .map_err(|e| e.to_string())?
        .write_all(&bytes)
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("receipt audit refused: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_pointer_lines_refuse_oversized_input_before_growing_a_full_line() {
        let mut input = BufReader::new(&b"{\"schema\":\"misaka.rfc02.receipt-state-stream.v2\"}\n"[..]);
        let raw = line(&mut input).unwrap().unwrap();
        let header: StreamHeader = serde_json::from_slice(&raw).unwrap();
        assert_eq!(header.schema, "misaka.rfc02.receipt-state-stream.v2");
        assert!(line(&mut input).unwrap().is_none());
        let excessive = vec![b'x'; 1 << 20];
        let mut input = BufReader::new(excessive.as_slice());
        assert!(line(&mut input).is_err());
        // Only the fixed line prefix was consumed; the remainder never became
        // an owned line buffer even when the input contains no newline.
        let mut remainder = Vec::new();
        input.read_to_end(&mut remainder).unwrap();
        assert_eq!(remainder.len(), excessive.len() - 4097);
    }
}
