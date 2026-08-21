use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    #[error(
        "Configuration: {0} activates the EVM lane at DAA 0 and this kaspad was built without the `evm` feature — it could sync but never build a block. The feature is on by default, so this is a `--no-default-features` build; rebuild with: cargo build --release -p kaspad"
    )]
    EvmLaneRequiresEvmBuild(String),

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

    #[error("Configuration: {0} requires {1}")]
    MissingDependentArg(&'static str, &'static str),

    #[error("Configuration: --min-disk-free-percent ({0}) must be in the range 0..=99")]
    MinDiskFreePercentTooHigh(u8),

    #[cfg(feature = "devnet-prealloc")]
    #[error("Cannot preallocate UTXOs on any network except devnet")]
    PreallocUtxosOnNonDevnet,

    #[cfg(feature = "devnet-prealloc")]
    #[error("--num-prealloc-utxos has to appear with --prealloc-address and vice versa")]
    MissingPreallocNumOrAddress,
}

pub type ConfigResult<T> = std::result::Result<T, ConfigError>;
