use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    #[error("Configuration: --addpeer and --connect cannot be used together")]
    MixedConnectAndAddPeers,

    #[error("Configuration: --logdir and --nologfiles cannot be used together")]
    MixedLogDirAndNoLogFiles,

    #[error("Configuration: --ram-scale cannot be set below 0.1")]
    RamScaleTooLow,

    #[error("Configuration: --ram-scale cannot be set above 10.0")]
    RamScaleTooHigh,

    #[error("Configuration: --max-tracked-addresses cannot be set above {0}")]
    MaxTrackedAddressesTooHigh(usize),

    #[error("Configuration: --node-profile={0} is a sync-only profile and is incompatible with {1}")]
    NodeProfileIncompatible(String, &'static str),

    #[error("Configuration: --node-profile=recovery-sync requires at least one --connect peer")]
    RecoverySyncRequiresConnect,

    #[error("Configuration: --min-disk-free-percent ({0}) must be in the range 0..=99")]
    MinDiskFreePercentTooHigh(u8),

    /// kaspa-pq ADR-0040 (AUTH-03). Refusing at startup rather than warning is deliberate: without the
    /// authority key the service would start, draw tickets, and be unable to authorize any winner, so it
    /// would burn every interval it won while appearing to mine. A node that cannot mint should say so
    /// before it starts, not after it has spent its tickets.
    #[error(
        "Configuration: --palw-mine requires --palw-ticket-authority-key-file. Body clause 7 requires every \
         algo-4 block's authorization to be signed by the ticket authority its leaf named; this is a different \
         key from --palw-mine-address (payout)."
    )]
    PalwMineRequiresTicketAuthorityKey,

    /// A ticket nullifier is chosen once at leaf registration and cannot be re-derived from chain state,
    /// so mining without the store means the node cannot open its own leaves' commitments.
    #[error(
        "Configuration: --palw-mine requires --palw-ticket-secret-file. A registered leaf publishes only its \
         ticket_nullifier_commitment; the raw nullifier that opens it lives only in this file."
    )]
    PalwMineRequiresTicketSecretFile,

    #[error(
        "Configuration: --palw-mine requires --palw-mine-address. The algo-4 template cannot construct its coinbase payout without it."
    )]
    PalwMineRequiresAddress,

    #[error(
        "Configuration: --palw-mine requires at least one --palw-leaf=<batch_id>:<leaf_index>. The owned-ticket set is fixed at startup, so an empty set can never mint."
    )]
    PalwMineRequiresLeaf,

    #[error(
        "Configuration: --palw-mine requires --palw-enable-algo4. Otherwise every locally built algo-4 block is rejected by this node's own consensus rules."
    )]
    PalwMineRequiresAlgo4Acceptance,

    #[error(
        "Configuration: --palw-da-import-dir requires --palw-enable-algo4. The local spool is an explicit Object-v2 publication surface and must not run while algo-4 acceptance remains closed."
    )]
    PalwDaImportRequiresAlgo4Acceptance,

    #[error("Configuration: invalid PALW pruning snapshot checkpoints: {0}")]
    InvalidPalwPruningSnapshotCheckpoints(String),

    #[error("Configuration: invalid --palw-mine setup: {0}")]
    PalwMineInvalidConfiguration(String),

    /// C-01: the EVM storage knobs form a dependency chain. A half-configured chain used
    /// to be demoted to a no-op with a warning inside the virtual processor, which is how
    /// a node could be configured "to retire the per-block 206 state snapshot" and still
    /// write a full EVM state copy per block until the disk filled. Refuse at startup
    /// instead. Use `--evm-storage-profile=compact`, which cannot be half-configured.
    #[error(
        "Configuration: {0} requires {1}. The EVM storage knobs are a dependency chain and a partial chain is NOT applied \
         (it silently keeps writing the per-block 206 full-state snapshot). Prefer --evm-storage-profile=compact."
    )]
    EvmStorageKnobRequires(&'static str, &'static str),

    #[cfg(feature = "devnet-prealloc")]
    #[error("Cannot preallocate UTXOs on any network except devnet")]
    PreallocUtxosOnNonDevnet,

    #[cfg(feature = "devnet-prealloc")]
    #[error("--num-prealloc-utxos has to appear with --prealloc-address and vice versa")]
    MissingPreallocNumOrAddress,
}

pub type ConfigResult<T> = std::result::Result<T, ConfigError>;
