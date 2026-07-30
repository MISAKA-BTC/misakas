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

/// ADR-0039 PALW Replica-GEMM lane: the header version at which the PALW fields
/// (component work `blue_hash_work`/`blue_compute_work`, the ticket reference, the
/// first-class `palw_ticket_nullifier`, `palw_chain_commit`, and the authorization
/// hash) enter the header-hash preimage, appended **after** `overlay_commitment_root`
/// (design §13.1/§13.2).
///
/// MUST be strictly greater than [`EVM_HEADER_VERSION`] = `2` (and thus every pre-PALW
/// header version), so the preimage of every existing v0/v1/v2 header stays
/// byte-identical — the PALW fields are *not* hashed below the gate. NEVER lower it.
/// Reserved and inert until the PALW activation fence; no header is minted at this
/// version until the PALW hot-path lands.
pub const PALW_HEADER_VERSION: u16 = 3;

/// PALW public/value-network anti-spam header version. Version 4 appends an authenticated accumulator
/// commitment and independent `palw_spam_nonce` after the v3 PALW fields. It is intentionally a new re-genesis/fork boundary:
/// the closed shared-testnet v3 format remains byte-identical, and no existing preset activates v4.
pub const PALW_ANTISPAM_HEADER_VERSION: u16 = 4;

/// ADR-MA (model-agnostic Compute Set registry) header version. Version 5 appends the three
/// generic registry commitments after the v4 anti-spam fields — `palw_compute_set_id`,
/// `palw_compute_policy_id`, `palw_allocation_plan_id` — so FUTURE model additions, revisions,
/// share changes and stops never need another header change (§13/§25: introduce the generic
/// references BEFORE PALW main-net operation, or pay a hard fork at the first model update).
/// Gated by `Params::palw_compute_registry_activation_daa_score`; no shipped preset activates
/// v5, and below the gate the three fields are hash-invisible and forced to zero
/// (`check_header_version`) exactly like every prior version boundary.
pub const PALW_COMPUTE_SET_HEADER_VERSION: u16 = 5;

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
/// kaspa-pq (misaka MSK) tokenomics (see docs): theoretical max supply is
/// ~26.013224875B MSK = 10B genesis premine (re-genesis: a single main UTXO per
/// network) + ~16.013224875B additional issuance over 30 years (1.4%/yr
/// exponential decay, q = 0.986). This is the per-amount sanity cap used by tx
/// validation and reported by `GetCoinSupply` as the max supply.
///
/// NOT a hard emission cap: each per-block reward is rounded UP to an integer
/// sompi, so the LIVE cumulative issuance runs a few tens of MSK above this
/// theoretical figure (≈ +44 MSK at 10 BPS). Emission stops via the terminal-0
/// entry of `SUBSIDY_BY_MONTH_TABLE` after year 30, not via this constant; a
/// strict cumulative cap would need explicit cutoff logic in `calc_block_subsidy`.
pub const MAX_SOMPI: u64 = 26_013_224_875 * SOMPI_PER_KASPA;

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
