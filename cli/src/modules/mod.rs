use crate::imports::*;

pub mod account;
pub mod address;
pub mod broadcast;
pub mod close;
pub mod connect;
#[path = "create-unsigned-tx.rs"]
pub mod create_unsigned_tx;
pub mod details;
pub mod disconnect;
pub mod estimate;
pub mod exit;
// kaspa-pq PQ-only (ADR-0019 §14): `export` (extended-public-key / mnemonic xpub
// export) is a classical secp256k1-only command — xpub derivation does not exist
// in a PQ-only build.
#[cfg(feature = "legacy-secp256k1")]
pub mod export;
pub mod guide;
pub mod halt;
pub mod help;
pub mod history;
// pub mod import;
pub mod list;
// kaspa-pq PQ-only (ADR-0019 §14): `message` (secp256k1 Schnorr message
// sign/verify over XOnlyPublicKey) is classical-only and has no PQ-only analogue
// on this command surface.
#[cfg(feature = "legacy-secp256k1")]
pub mod message;
pub mod miner;
pub mod monitor;
pub mod mute;
pub mod network;
pub mod node;
pub mod open;
pub mod ping;
// kaspa-pq PQ-only (ADR-0019 §14): PSKB/PSKT (partially-signed P2SH bundles) is an
// entirely classical secp256k1 feature; the whole `kaspa-wallet-pskt` crate is
// absent in a PQ-only build.
#[cfg(feature = "legacy-secp256k1")]
pub mod pskb;
pub mod reload;
pub mod rpc;
pub mod select;
pub mod send;
pub mod server;
pub mod settings;
pub mod sign;
pub mod start;
pub mod stop;
pub mod sweep;
// pub mod test;
pub mod theme;
pub mod track;
pub mod transfer;
pub mod wallet;

// this module is registered manually within
// applications that support metrics
pub mod metrics;

// TODO
// broadcast
// create-unsigned-tx
// sign

pub fn register_handlers(cli: &Arc<KaspaCli>) -> Result<()> {
    register_handlers!(
        cli,
        cli.handlers(),
        [
            account, address, close, connect, details, disconnect, estimate, exit, guide, help, history, rpc, list, miner, monitor,
            mute, network, node, open, ping, reload, select, send, server, settings, sweep, track, transfer,
            wallet,
            // halt,
            // theme,  start, stop
        ]
    );

    // kaspa-pq PQ-only (ADR-0019 §14): `export` (xpub export), `message` (secp256k1
    // message sign/verify) and `pskb` (P2SH partially-signed bundles) are classical
    // secp256k1-only commands and are only registered in a `legacy-secp256k1` build.
    #[cfg(feature = "legacy-secp256k1")]
    register_handlers!(cli, cli.handlers(), [export, message, pskb]);

    Ok(())
}
