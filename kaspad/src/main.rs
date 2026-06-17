extern crate kaspa_consensus;
extern crate kaspa_core;
extern crate kaspa_hashes;

use std::sync::Arc;

use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_core::{info, signals::Signals};
use kaspa_utils::fd_budget;
use kaspad_lib::{
    args::parse_args,
    daemon::{DESIRED_DAEMON_SOFT_FD_LIMIT, MINIMUM_DAEMON_SOFT_FD_LIMIT, create_core},
};

#[cfg(feature = "heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

pub fn main() {
    #[cfg(feature = "heap")]
    let _profiler = dhat::Profiler::builder().file_name("kaspad-heap.json").build();

    init_allocator_with_default_settings();

    let args = parse_args();

    // audit H-01: refuse to launch a MAINNET node while the premine custody ceremony is pending.
    // The ceremony is COMPLETE as of the 2026-06-17 re-genesis (the 13B premine is locked to the
    // operator custody addresses, MAINNET_PREMINE_CEREMONY_PENDING = false), so this guard is now a
    // dormant safety net: if the flag is ever flipped back to true (placeholder payload), a mainnet
    // node refuses to start rather than run a chain with an unspendable premine. Test/devnet/simnet
    // are unaffected (Claude-managed test key); consensus harnesses never reach this binary entry.
    if args.network().network_type == kaspa_consensus_core::network::NetworkType::Mainnet
        && kaspa_consensus_core::config::premine::MAINNET_PREMINE_CEREMONY_PENDING
    {
        eprintln!(
            "FATAL (audit H-01): refusing to start a MAINNET node — the premine custody ceremony is \
             pending (the mainnet premine is an unspendable all-zero placeholder). Complete the offline \
             ML-DSA-87 key ceremony + re-genesis and set MAINNET_PREMINE_CEREMONY_PENDING=false first."
        );
        std::process::exit(1);
    }

    match fd_budget::try_set_fd_limit(DESIRED_DAEMON_SOFT_FD_LIMIT) {
        Ok(limit) => {
            if limit < MINIMUM_DAEMON_SOFT_FD_LIMIT {
                println!("Current OS file descriptor limit (soft FD limit) is set to {limit}");
                println!("The kaspad node requires a setting of at least {DESIRED_DAEMON_SOFT_FD_LIMIT} to operate properly.");
                println!("Please increase the limits using the following command:");
                println!("ulimit -n {DESIRED_DAEMON_SOFT_FD_LIMIT}");
            }
        }
        Err(err) => {
            println!("Unable to initialize the necessary OS file descriptor limit (soft FD limit) to: {}", err);
            println!("The kaspad node requires a setting of at least {DESIRED_DAEMON_SOFT_FD_LIMIT} to operate properly.");
        }
    }

    let fd_total_budget = fd_budget::limit() - args.rpc_max_clients as i32 - args.inbound_limit as i32 - args.outbound_target as i32;
    let (core, _) = create_core(args, fd_total_budget);

    // Bind the keyboard signal to the core
    Arc::new(Signals::new(&core)).init();

    core.run();
    info!("Kaspad has stopped...");
}
