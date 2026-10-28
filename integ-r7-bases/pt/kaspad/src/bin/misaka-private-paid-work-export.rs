//! Offline actual Program work and signed paid-enrollment export for the
//! unpublished private integration profile. This binary starts no node and
//! grants no security, economics, source-independence or native Final approval.
use std::{collections::BTreeMap, path::Path};
const USAGE: &str = "misaka-private-paid-work-export --salt HEX --request FILE --out FRESH_DIRECTORY
Requires a separately authenticated live native kernel snapshot and a real posted Program job.
The request is bounded Borsh PrivatePaidProgramRequestV3; selected-chain ledger/aux roots are caller pins.
Build with --features private-integration-config. Output is real TIR work and a signed enrollment draft;
source enrollment, assigned verification, payment, native inclusion and Final require later live steps.";
fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    if args.len() != 6 {
        return Err(USAGE.into());
    }
    let mut fields = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        if !matches!(pair[0].as_str(), "--salt" | "--request" | "--out") || fields.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err("unknown or duplicate private export option".into());
        }
    }
    let salt = kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(fields.remove("--salt").ok_or("--salt required")?)
        .map_err(|e| e.to_string())?;
    let params = kaspad_lib::private_integration::private_complete_check_params_v1(&salt)?;
    let mut config = kaspa_consensus_core::config::Config::new(params);
    config.palw_drill_genesis_salt = Some(salt);
    let request = Path::new(fields.remove("--request").ok_or("--request required")?);
    let output = Path::new(fields.remove("--out").ok_or("--out required")?);
    let manifest = kaspad_lib::private_integration::export_private_paid_program_work_v3(&config, request, output)?;
    println!("{}", serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?);
    Ok(())
}
fn main() {
    kaspa_alloc::init_allocator_with_default_settings();
    if let Err(why) = run() {
        eprintln!("private paid work export: {why}");
        std::process::exit(1);
    }
}
