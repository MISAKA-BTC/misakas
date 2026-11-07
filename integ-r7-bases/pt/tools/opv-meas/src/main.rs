//! `opv-meas` — RFC-0015 OPV measurement harness (test-only; no consensus change).
//!
//! ```text
//! opv-meas static      --container PATH [--label L] [--positions 4,64,512]
//! opv-meas build-world --container PATH --out DIR [--prompt 3] [--max-positions 8] [--claims honest,late,early,gather,elem,decode] [--panel]
//! opv-meas serve       --dir DIR [--listen 127.0.0.1:0]          # the reference HTTP provider of misaka-palw-remote
//! opv-meas verify      --world DIR --claim NAME --provider DIR|http://127.0.0.1:PORT --cache DIR [--rep N] [--rss-cap-gb G] [--serve] [--withhold-pos P]
//! opv-meas micro       [--k K --n N]                              # unit costs: the exact-row recompute a localization is, field mults, hashing
//! opv-meas derive      --in inputs.json                           # the window and collateral formulas over measured inputs (see derive.rs)
//! opv-meas weights     --container PATH [--reps 2] [--limit-params N]   # the per-parameter work of a fresh verifier, streaming (any host RAM)
//! ```
//! Each command prints one JSON object on stdout.
mod derive;
mod micro;
mod stat;
mod util;
mod verify;
mod weights;
mod wire;
mod world;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_default();
    let r = match cmd.as_str() {
        "static" => stat::run(&args),
        "build-world" => world::run(&args),
        "verify" => verify::run(&args),
        "micro" => micro::run(&args),
        "weights" => weights::run(&args),
        "derive" => derive::run(&args),
        "serve" => serve(&args),
        _ => Err("commands: static | build-world | serve | verify | micro | weights | derive (see the module doc)".into()),
    };
    match r {
        Ok(v) => println!("{}", serde_json::to_string(&v).unwrap()),
        Err(e) => {
            println!("{}", serde_json::json!({"cmd": cmd, "error": e}));
            std::process::exit(2);
        }
    }
}

fn serve(args: &[String]) -> Result<serde_json::Value, String> {
    use misaka_palw_remote::evidence::ManifestLimits;
    use misaka_palw_remote::transport::server::{ServerConfig, start};
    let dir = util::arg(args, "--dir").ok_or("--dir DIR")?;
    let listen = util::arg(args, "--listen").unwrap_or_else(|| "127.0.0.1:0".into());
    let cfg = ServerConfig {
        limits: ManifestLimits { max_chunks: 1 << 17, max_chunk_bytes: 1 << 20, max_total_bytes: 1 << 37 },
        max_store_bytes: 64 << 30,
    };
    let h = start(&listen, dir.into(), cfg).map_err(|e| e.to_string())?;
    // The orchestrator reads this line, then leaves the process running until it kills it by pid.
    println!("{}", serde_json::json!({"cmd": "serve", "url": h.url()}));
    use std::io::Write;
    let _ = std::io::stdout().flush();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
