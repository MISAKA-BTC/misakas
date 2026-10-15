//! Offline A6 checker. Frozen intent and file pointers are data; accepted native
//! claims, Final32 credits and shared queue counters come from audit32 proofs.
use std::{collections::{BTreeMap,BTreeSet},path::{Path,PathBuf},io::Write};
use kaspa_consensus_core::{Hash64,config::params::Params,network::NetworkId};
use misaka_palw_sdk::{native_audit_io_v32::*,native_receipt_windows_v1::*};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {child:Hash64,selected_parent:Hash64,collected:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transcript {schema:String,points:Vec<Point>}
fn relative(base:&Path,name:&str)->Result<PathBuf,String>{let p=Path::new(name);if p.is_absolute()||p.components().any(|c|!matches!(c,std::path::Component::Normal(_))){return Err("receipt audit relative evidence path required".into());}
    let root=base.canonicalize().map_err(|e|e.to_string())?;let p=root.join(p).canonicalize().map_err(|e|e.to_string())?;
    if !p.starts_with(root){return Err("receipt audit evidence escapes root".into());}Ok(p)}
fn run()->Result<(),String>{
    let mut options=BTreeMap::new();let mut it=std::env::args().skip(1);
    while let Some(k)=it.next(){if !k.starts_with("--")||options.insert(k,it.next().ok_or("receipt audit flag value missing")?).is_some(){return Err("receipt audit distinct flags required".into());}}
    let mut get=|k:&str|options.remove(k).ok_or_else(||format!("{k} required"));
    let network=get("--network")?;let root=PathBuf::from(get("--evidence-root")?);let contract_path=PathBuf::from(get("--contract")?);
    let points_path=PathBuf::from(get("--points")?);let out=PathBuf::from(get("--out")?);drop(get);
    let base:Params=network.parse::<NetworkId>().map_err(|e|e.to_string())?.into();
    let file=options.remove("--own-params-snapshot34");let pin=options.remove("--own-params-snapshot-root");let sha=options.remove("--own-params-snapshot-sha256");
    let own=match(file,pin,sha){(None,None,None)=>misaka_palw_sdk::native_params_v34::OwnedNativeParametersV34::shipped_preset(base),
        (Some(file),Some(pin),Some(sha))=>misaka_palw_sdk::native_params_v34::load_own_native_parameters_v34(&base,Path::new(&file),pin.parse::<Hash64>().map_err(|e|e.to_string())?.as_bytes(),misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&sha)?)?,
        _=>return Err("receipt audit all owning snapshot pins required together".into())};
    if !options.is_empty()||out.exists()||!own.rfc02_qualified(){return Err("receipt audit exact flags/fresh report/qualified owning Params required".into());}
    let contract:ReceiptWindowContractV1=serde_json::from_slice(&read_bounded_file_v32(&contract_path,4<<20)?).map_err(|e|e.to_string())?;contract.check()?;
    let transcript:Transcript=serde_json::from_slice(&read_bounded_file_v32(&points_path,16<<20)?).map_err(|e|e.to_string())?;
    if transcript.schema!="misaka.rfc02.receipt-state-transcript.v1"||transcript.points.len()<3||transcript.points.len()>65536{return Err("receipt audit transcript extent/schema".into());}
    let mut tracked:BTreeSet<_>=contract.jobs().into_iter().collect();let mut points=Vec::new();
    for point in transcript.points {
        let folder=relative(&root,&point.collected)?;
        let child=read_header_v32(&folder.join("child-header.json"))?;let parent=read_header_v32(&folder.join("selected-parent-header.json"))?;
        let aux=read_native_fact_proof_v32(&folder.join("aux-proof32.borsh"))?;let payouts=read_native_fact_proof_v32(&folder.join("payout-proof32.borsh"))?;
        let jobs:Vec<_>=tracked.iter().copied().collect();let checked=AnchoredReceiptPointV1::derive(&child,point.child,&parent,point.selected_parent,own.parameters(),&aux,&payouts,&jobs)?;
        tracked.extend(checked.tracked());points.push(checked);
    }
    let mut report=audit_receipt_windows_v1(&contract,&points)?;
    report["network"]=network.into();report["genesis"]=own.parameters().genesis.hash.to_string().into();
    report["own_snapshot_root"]=misaka_palw_sdk::runtime_pack::commit::hex(&own.snapshot_root()).into();
    report["local_qualified_validation_network"]=own.parameters().native_final_is_local_qualified_v34().into();
    std::fs::OpenOptions::new().write(true).create_new(true).open(out).map_err(|e|e.to_string())?.write_all(&serde_json::to_vec_pretty(&report).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("receipt audit refused: {e}");std::process::exit(1);}}
