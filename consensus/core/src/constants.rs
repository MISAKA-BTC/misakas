/// BLOCK_VERSION represents the current block version
pub const BLOCK_VERSION: u16 = 1;

/// kaspa-pq Selected-Parent EVM Lane (ADR-0020): the block/header version at
/// which the single EVM commitment (`evm_commitment_root`) enters the
/// header-hash preimage and an `evm_payload` becomes mandatory.
///
/// MUST be strictly greater than every pre-EVM header version that already
/// exists on-chain — namely the genesis version `0`
/// (`consensus/core/src/config/genesis.rs`) and the live mined-block
/// [`BLOCK_VERSION`] = `1`. Picking `2` keeps the preimage of every existing
/// v0/v1 header byte-identical (the EVM fields are *not* hashed below the
/// gate), so all current genesis hashes and block identities are unchanged.
/// NEVER lower this value: doing so would pull the EVM fields into the
/// preimage of already-mined blocks and rewrite every block identity.
pub const EVM_HEADER_VERSION: u16 = 2;

/// TX_VERSION is the current latest supported transaction version.
pub const TX_VERSION: u16 = 0;

pub const LOCK_TIME_THRESHOLD: u64 = 500_000_000_000;

/// MAX_SCRIPT_PUBLIC_KEY_VERSION is the current latest supported public key script version.
pub const MAX_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;

/// SompiPerKaspa is the number of sompi in one kaspa (1 KAS).
pub const SOMPI_PER_KASPA: u64 = 100_000_000;

/// The parameter for scaling inverse KAS value to mass units (KIP-0009)
pub const STORAGE_MASS_PARAMETER: u64 = SOMPI_PER_KASPA * 10_000;

/// The parameter defining how much mass per byte to charge for when calculating
/// transient storage mass. Since normally the block mass limit is 500_000, this limits
/// block body byte size to 125_000 (KIP-0013).
pub const TRANSIENT_BYTE_TO_MASS_FACTOR: u64 = 4;

/// MaxSompi is the maximum transaction amount allowed in sompi.
///
/// kaspa-pq tokenomics (see docs): final supply is capped at 28B KAS =
/// 13B genesis premine (re-genesis 2026-06-17: 40 vaults × 0.1B + 1 main × 9B)
/// + 15B additional issuance over 20 years (5%/yr exponential decay; the mined
/// half is unchanged). This is the per-amount sanity cap used by tx validation
/// and reported by `GetCoinSupply` as the max supply.
pub const MAX_SOMPI: u64 = 28_000_000_000 * SOMPI_PER_KASPA;

// MAX_TX_IN_SEQUENCE_NUM is the maximum sequence number the sequence field
// of a transaction input can be.
pub const MAX_TX_IN_SEQUENCE_NUM: u64 = u64::MAX;

// SEQUENCE_LOCK_TIME_MASK is a mask that extracts the relative lock time
// when masked against the transaction input sequence number.
pub const SEQUENCE_LOCK_TIME_MASK: u64 = 0x00000000ffffffff;

// SEQUENCE_LOCK_TIME_DISABLED is a flag that if set on a transaction
// input's sequence number, the sequence number will not be interpreted
// as a relative lock time.
pub const SEQUENCE_LOCK_TIME_DISABLED: u64 = 1 << 63;

/// UNACCEPTED_DAA_SCORE is used to for UtxoEntries that were created by
/// transactions in the mempool, or otherwise not-yet-accepted transactions.
pub const UNACCEPTED_DAA_SCORE: u64 = u64::MAX;
