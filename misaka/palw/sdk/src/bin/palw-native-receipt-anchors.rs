//! Bounded own-node clock observer for the receipt driver. Produces header
//! pointers only; independent audit32 replay grants all receipt/window credit.
use std::{collections::BTreeMap,path::{Path,PathBuf},time::{Duration,Instant},io::Write};
use kaspa_consensus_core::{Hash64,header::Header,config::params::Params,network::NetworkId};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{KaspaRpcClient,WrpcEncoding,client::{ConnectOptions,ConnectStrategy}};
fn run()->Result<(),String>{
    let mut a=BTreeMap::new();let mut it=std::env::args().skip(1);while let Some(k)=it.next(){if a.insert(k,it.next().ok_or("flag value required")?).is_some(){return Err("distinct flags required".into());}}
    let mut get=|k:&str|a.remove(k).ok_or_else(||format!("{k} required"));
    let rpc=get("--rpc")?;let network=get("--network")?;let contract=PathBuf::from(get("--contract")?);let out=PathBuf::from(get("--out")?);drop(get);
    let maximum=a.remove("--timeout-seconds").ok_or("bounded timeout required")?.parse::<u64>().map_err(|e|e.to_string())?;
    let file=a.remove("--own-params-snapshot34");let pin=a.remove("--own-params-snapshot-root");let sha=a.remove("--own-params-snapshot-sha256");
    let base:Params=network.parse::<NetworkId>().map_err(|e|e.to_string())?.into();
    let own=match(file,pin,sha){(None,None,None)=>misaka_palw_sdk::native_params_v34::OwnedNativeParametersV34::shipped_preset(base),
        (Some(f),Some(p),Some(s))=>misaka_palw_sdk::native_params_v34::load_own_native_parameters_v34(&base,Path::new(&f),p.parse::<Hash64>().map_err(|e|e.to_string())?.as_bytes(),misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&s)?)?,_=>return Err("all owning pins required".into())};
    if !a.is_empty()||maximum==0||maximum>43200||out.exists()||!own.rfc02_qualified(){return Err("fresh qualified bounded native clock observer required".into());}
    let contract:misaka_palw_sdk::native_receipt_windows_v1::ReceiptWindowContractV1=serde_json::from_slice(&misaka_palw_sdk::native_audit_io_v32::read_bounded_file_v32(&contract,4<<20)?).map_err(|e|e.to_string())?;contract.check()?;
    let start=contract.windows[0].start_daa;let end=contract.windows.iter().map(|w|w.end_daa+w.maximum_final_lag_daa).max().unwrap();
    let rt=tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().map_err(|e|e.to_string())?;
    let client=KaspaRpcClient::new(WrpcEncoding::Borsh,Some(&rpc),None,None,None).map_err(|e|e.to_string())?;
    rt.block_on(client.connect(Some(ConnectOptions{block_async_connect:true,connect_timeout:Some(Duration::from_secs(30)),strategy:ConnectStrategy::Fallback,..Default::default()}))).map_err(|e|e.to_string())?;
    let began=Instant::now();let mut first:Option<Hash64>=None;
    let pointers=rt.block_on(async{
        loop{
            if began.elapsed()>Duration::from_secs(maximum){return Err("original receipt end DAA not reached within timeout".to_string());}
            let result=tokio::time::timeout(Duration::from_secs(30),async{
                let status=client.get_palw_node_status().await.map_err(|e|e.to_string())?;
                own.check_node_identity(&status.consensus_params_id,&status.consensus_schedule_id,&status.genesis_hash)?;
                let info=client.get_block_dag_info().await.map_err(|e|e.to_string())?;
                if info.network.to_string()!=network{return Err("native receipt clock network changed".to_string());}
                let block=client.get_block(info.sink,false).await.map_err(|e|e.to_string())?;
                let verbose=block.verbose_data.as_ref().ok_or("clock observer full selected-chain metadata absent")?;
                let child=Header::try_from(&block.header).map_err(|e|e.to_string())?;
                let parent=client.get_block(verbose.selected_parent_hash,false).await.map_err(|e|e.to_string())?;
                let ph=Header::try_from(&parent.header).map_err(|e|e.to_string())?;
                let child_hash=kaspa_consensus_core::hashing::header::hash(&child);
                if !verbose.is_chain_block||child_hash!=info.sink||!child.parents_by_level.get(0).is_some_and(|p|p.contains(&verbose.selected_parent_hash))||kaspa_consensus_core::hashing::header::hash(&ph)!=verbose.selected_parent_hash{return Err("native receipt clock selected-chain pointers differ".to_string());}
                Ok::<_,String>((child_hash,verbose.selected_parent_hash,ph.daa_score))
            }).await.map_err(|_|"native receipt clock RPC timeout".to_string())??;
            if result.2<=start{first=Some(result.0);}else if first.is_none(){return Err("observer began after original first receipt window".into());}
            if result.2>=end{let first=first.ok_or("receipt start anchor absent")?;
                break Ok(serde_json::json!({"start_child":first.to_string(),"end_child":result.0.to_string(),"end_selected_parent":result.1.to_string(),"provenance_start":first.to_string()}));}
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })?;
    std::fs::OpenOptions::new().write(true).create_new(true).open(out).map_err(|e|e.to_string())?.write_all(&serde_json::to_vec_pretty(&pointers).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("native receipt anchors refused: {e}");std::process::exit(1);}}
