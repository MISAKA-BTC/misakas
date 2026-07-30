//! misaka-palw-bridge daemon entry point. See `lib.rs` for what this is (and is not).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use misaka_palw_bridge::http::{HttpConfig, serve};
use misaka_palw_bridge::state::BridgeState;

struct Args {
    listen: SocketAddr,
    data_dir: PathBuf,
    auth_token: Option<String>,
    assignment_deadline_ms: i64,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        listen: "127.0.0.1:26621".parse().expect("static addr"),
        data_dir: PathBuf::from("palw-bridge-data"),
        auth_token: None,
        assignment_deadline_ms: 120_000,
    };
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let take = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            argv.get(*i).cloned().ok_or_else(|| format!("{} needs a value", argv[*i - 1]))
        };
        match argv[i].as_str() {
            "--listen" => args.listen = take(&mut i)?.parse().map_err(|e| format!("--listen: {e}"))?,
            "--data-dir" => args.data_dir = PathBuf::from(take(&mut i)?),
            "--auth-token" => args.auth_token = Some(take(&mut i)?),
            "--assignment-deadline-ms" => {
                args.assignment_deadline_ms =
                    take(&mut i)?.parse().map_err(|e| format!("--assignment-deadline-ms: {e}"))?
            }
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    Ok(args)
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("misaka-palw-bridge: {e}");
            eprintln!(
                "usage: misaka-palw-bridge [--listen 127.0.0.1:26621] [--data-dir <dir>] \
                 [--auth-token T] [--assignment-deadline-ms N]"
            );
            std::process::exit(2);
        }
    };

    let state = match BridgeState::open(&args.data_dir, args.assignment_deadline_ms) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("misaka-palw-bridge: journal: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "[palw-bridge] journal seq {} head {} (data-dir {})",
        state.seq(),
        &state.head_root_hex()[..16],
        args.data_dir.display()
    );

    let state = Arc::new(Mutex::new(state));
    if let Err(e) = serve(state, HttpConfig { listen: args.listen, auth_token: args.auth_token }).await {
        eprintln!("misaka-palw-bridge: {e}");
        std::process::exit(1);
    }
}
