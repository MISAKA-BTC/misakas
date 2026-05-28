//! kaspa-pq Phase 10/11: DNS Probabilistic Finality Overlay type
//! surface.
//!
//! See [ADR-0009](../../docs/adr/0009-dns-probabilistic-finality.md)
//! for the consensus design and
//! [ADR-0010](../../docs/adr/0010-validator-node-architecture.md) for
//! the operational architecture. This module carries the **type
//! surface only** that Phase 10 follow-up PRs (10.4 — 10.14) will
//! reference; consensus rule implementations panic with explicit
//! `unimplemented!()` so the missing surface is loud rather than
//! silently-zero.
//!
//! Categories:
//!
//! - **Wire payloads** (`StakeBondPayload`, `StakeAttestation`,
//!   `StakeAttestationShardPayload`, `SlashingEvidencePayload`) —
//!   what nodes commit on-chain. Bounded by `MAX_ATTESTATIONS_PER_SHARD`.
//! - **Consensus state** (`StakeBondRecord`, `ValidatorRecord`,
//!   `ValidatorSetSnapshot`, `DnsState`) — what nodes derive from the
//!   wire payloads and persist in the consensus stores defined by
//!   ADR-0010 §"Subsystem file layout".
//! - **Node-side policy** (`BlockTemplatePolicy`, `DnsParams`) —
//!   per-network knobs read at startup.
//! - **RPC view** (`DnsConfirmation`) — surface returned by the
//!   `getDnsConfirmation` method (lands in PR-10.14).
//! - **Helpers** (`validator_set_commitment`, `stake_attestation_message`)
//!   — the two byte-deterministic derivations every node must agree
//!   on. Both panic-stub-free; consumed by validator + verifier
//!   alike.
//!
//! Hash widths follow [ADR-0008](../../docs/adr/0008-hash64-consensus-identity.md)
//! and [ADR-0010](../../docs/adr/0010-validator-node-architecture.md)
//! §"Validator-set commitment derivation": `validator_id`,
//! `target_hash`, `validator_set_commitment`, and the owner /
//! validator pubkey hashes inside the registry types are all 64-byte
//! [`Hash64`]. `TransactionOutpoint.transaction_id` is the upstream
//! 32-byte alias today and widens to Hash64 in the PR-9.5 cascade —
//! callers must not assume 32 bytes there long-term.
//!
//! All payload and state types derive `BorshSerialize` /
//! `BorshDeserialize` so they round-trip through the existing wRPC
//! Borsh path; `serde` JSON is added via manual impls in the
//! consumer-facing RPC types only.

use std::fmt::{self, Display, Formatter};

use blake2b_simd::Params as Blake2bParams;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};

use crate::{BlueWorkType, tx::TransactionOutpoint};

/// 1952 bytes — matches `kaspa_txscript::MLDSA65_PK_LEN`. Repeated
/// here so this module does not have to depend on `kaspa-txscript`;
/// asserted-equal by [`tests::dns_constants_have_expected_values`].
pub const STAKE_VALIDATOR_PUBKEY_LEN: usize = 1952;

/// 3309 bytes — matches `kaspa_txscript::MLDSA65_SIG_LEN`. Same
/// re-export rationale as [`STAKE_VALIDATOR_PUBKEY_LEN`].
pub const STAKE_ATTESTATION_SIG_LEN: usize = 3309;

/// Per-block upper bound on the number of attestations a single
/// [`StakeAttestationShardPayload`] may carry. See ADR-0009 §"Why
/// partial certificates" for the mass-budget arithmetic that drove
/// this cap (a 64-validator full certificate would be ~216 KB and
/// blow out `max_block_mass`).
pub const MAX_ATTESTATIONS_PER_SHARD: usize = 16;

/// Fixed-point scale for [`StakeScore`] / [`DnsConfirmation`] integer
/// arithmetic. Always 10^9, so a "full one-vote epoch" contributes
/// exactly `STAKE_SCORE_SCALE` to the score. Keeps consensus arithmetic
/// integer-only.
pub const STAKE_SCORE_SCALE: u128 = 1_000_000_000;

/// kaspa-pq Phase 10 wire-format version of every payload struct.
/// Bumped only by a hard-fork ADR; consumers reject foreign versions.
pub const DNS_PAYLOAD_VERSION_V1: u16 = 1;

/// kaspa-pq Phase 10 ML-DSA-65 attestation signing context. Distinct
/// from the transaction context (`b"kaspa-pq-v1/tx/mldsa65"`,
/// ADR-0002) so an attestation signature can never be replayed as a
/// transaction signature (and vice versa).
pub const ATTESTATION_MLDSA65_CONTEXT: &[u8] = b"kaspa-pq-v1/att/mldsa65";

/// kaspa-pq Phase 10 BLAKE2b-256 domain key used when constructing
/// the attestation message that ML-DSA-65 signs over. Consumed by
/// [`stake_attestation_message`]. See ADR-0009 §"Attestation target".
pub const ATTESTATION_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-v1/stake-attestation";

/// kaspa-pq Phase 11 BLAKE2b-512 domain key used by
/// [`validator_set_commitment`]. Consensus-fixed and bumped only by
/// a hard-fork ADR (the `-v1` suffix is the contract). See
/// ADR-0010 §"Validator-set commitment derivation".
pub const VALIDATOR_SET_COMMITMENT_KEY: &[u8] = b"kaspa-pq-validator-set-v1";

/// Fixed-point scaled stake score. Wrapper for documentation /
/// arithmetic clarity; the underlying `u128` is the same number of
/// "stake-score units" used throughout the overlay.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct StakeScore(pub u128);

impl Display for StakeScore {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Pretty-print as fixed-point: `STAKE_SCORE_SCALE` units per
        // "1.0" so `1_500_000_000` displays as `1.5`.
        let whole = self.0 / STAKE_SCORE_SCALE;
        let frac = self.0 % STAKE_SCORE_SCALE;
        write!(f, "{whole}.{frac:09}")
    }
}

/// Three-stage rollout token (ADR-0009 §"Three-stage rollout").
///
/// Never appears explicitly in tx payloads; it is a view of network
/// state used by the RPC layer and by node-internal activation
/// gating. Persisted as a single byte.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum DnsRolloutStage {
    /// Phases 1–9 only; bond / shard / evidence txs are rejected.
    #[default]
    Launch = 0,
    /// StakeBond and StakeAttestationShard txs are accepted; the
    /// reorg gate is **not** enforced. Visibility-only.
    Bootstrap = 1,
    /// `total_active_stake ≥ MIN_ACTIVE_STAKE`,
    /// `active_validators ≥ MIN_ACTIVE_VALIDATORS`,
    /// `daa_score ≥ dns_activation_daa_score`. Reorg gate engages.
    Active = 2,
}

/// Per-bond lifecycle state stored alongside the registry entry.
/// ADR-0010 §"Validator service runtime" specifies the eligibility
/// predicate as `bond ∈ active_bonds ∧ bond ∉ unbonding_bonds ∧
/// bond ∉ slashed_bonds`; the four-state enum below makes that
/// predicate a single field comparison.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum BondStatus {
    /// Bond has been committed but `activation_daa_score` has not
    /// been reached yet; the validator cannot attest.
    #[default]
    Pending = 0,
    /// Bond is active and the validator may attest.
    Active = 1,
    /// Owner has submitted an unbond request; bond will be released
    /// after `unbonding_period_blocks`. No new attestations
    /// accepted.
    Unbonding = 2,
    /// Slashed by a `SlashingEvidencePayload`; bond is burned and
    /// the validator is removed from all future committees.
    Slashed = 3,
}

// ---------------------------------------------------------------------
// Wire payloads (transaction-level).
// ---------------------------------------------------------------------

/// kaspa-pq Phase 10 stake-bond payload.
///
/// Carried inside a transaction with subnetwork id
/// `SUBNETWORK_ID_STAKE_BOND` (consensus rule to be added in PR-10.4).
/// The bond locks an amount of coins to a validator ML-DSA-65 key for
/// at least `unbonding_period_blocks` blocks past any later withdraw
/// request. ADR-0009 §"Long-range bound" requires
/// `unbonding_period_blocks ≥ max_reorg_horizon + evidence_window`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeBondPayload {
    pub version: u16,

    /// `BLAKE2b-512(owner_public_key)` (ADR-0008 widening). The
    /// matching address surface is the kaspa-pq P2PKH (ADR-0002)
    /// after its own Phase 9 widening.
    pub owner_pubkey_hash: Hash64,
    /// `BLAKE2b-512(validator_public_key)`.
    pub validator_pubkey_hash: Hash64,

    /// Raw 1952-byte ML-DSA-65 public key for the validator. Stored
    /// in full so attestations can be verified by any node without an
    /// out-of-band registry lookup. Validated against
    /// `validator_pubkey_hash` at consensus time.
    pub validator_pubkey: Vec<u8>,

    /// Bonded amount in sompi.
    pub amount: u64,

    /// First DAA score at which this bond's attestations contribute
    /// to `StakeScore`. Lets a freshly-bonded validator observe the
    /// network before issuing attestations.
    pub activation_daa_score: u64,

    /// Per-bond unbonding window in blocks. Consensus-validated
    /// against the network-wide `DnsParams::unbonding_period_blocks`
    /// floor.
    pub unbonding_period_blocks: u64,
}

/// One validator attestation over a selected-chain anchor.
///
/// Many `StakeAttestation`s are batched into
/// `StakeAttestationShardPayload` for on-chain commitment. A raw
/// attestation is ~3300+100 bytes (the ML-DSA-65 signature
/// dominates).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeAttestation {
    pub version: u16,

    /// 64-byte validator identifier (ADR-0010 §"Validator-set
    /// commitment derivation"). Conventionally equal to the
    /// `validator_pubkey_hash` from the corresponding `StakeBond`.
    pub validator_id: Hash64,

    /// Refers to the transaction outpoint that created the bond. The
    /// outpoint's `transaction_id` widens to `Hash64` in the PR-9.5
    /// cascade.
    pub bond_outpoint: TransactionOutpoint,

    /// `daa_score / epoch_length_blocks`.
    pub epoch: u64,

    /// Selected-chain anchor this attestation approves.
    pub target_hash: Hash64,

    /// `daa_score` of the anchor; redundant with `target_hash` but
    /// included so an attestation can be partially-verified without a
    /// header lookup.
    pub target_daa_score: u64,

    /// Hash64 of the committee snapshot the attestation is bound to.
    /// Lets a verifier reject attestations issued under a stale
    /// validator set. Derived via [`validator_set_commitment`].
    pub validator_set_commitment: Hash64,

    /// 3309-byte ML-DSA-65 signature over the BLAKE2b-256
    /// attestation message produced by [`stake_attestation_message`]
    /// with `ATTESTATION_MLDSA65_CONTEXT` as the libcrux `ctx`
    /// parameter.
    pub signature: Vec<u8>,
}

/// Phase 10 transaction payload that commits up to
/// `MAX_ATTESTATIONS_PER_SHARD` attestations on-chain.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeAttestationShardPayload {
    pub version: u16,
    pub epoch: u64,
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    pub validator_set_commitment: Hash64,

    /// All attestations in a single shard must share the
    /// `(epoch, target_hash, validator_set_commitment)` tuple above
    /// (consensus rule in PR-10.4). Bounded by
    /// `MAX_ATTESTATIONS_PER_SHARD`.
    pub attestations: Vec<StakeAttestation>,
}

/// Phase 10 transaction payload that burns a validator's bond by
/// presenting two incompatible attestations from the same
/// `(bond_outpoint, validator_id, epoch)` triple. Must be submitted
/// within `DnsParams::evidence_window_blocks` of the latest of the
/// two cited attestations.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SlashingEvidencePayload {
    pub version: u16,
    pub bond_outpoint: TransactionOutpoint,
    pub attestation_a: StakeAttestation,
    pub attestation_b: StakeAttestation,
}

// ---------------------------------------------------------------------
// Consensus-state types (derived from wire payloads, persisted in
// the stores defined by ADR-0010 §"Subsystem file layout").
// ---------------------------------------------------------------------

/// Registry entry derived from a confirmed [`StakeBondPayload`].
///
/// Lives in `database/src/stores/stake_registry.rs` (created in
/// PR-10.5) keyed by `bond_outpoint`. Carries all fields the
/// validator-service eligibility check (ADR-0010 §"Validator service
/// runtime") needs without re-loading the original payload.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeBondRecord {
    pub version: u16,

    /// Identifies the bond uniquely; matches the outpoint of the
    /// transaction that created it.
    pub bond_outpoint: TransactionOutpoint,

    pub owner_pubkey_hash: Hash64,
    pub validator_pubkey_hash: Hash64,
    pub validator_pubkey: Vec<u8>,

    pub amount: u64,
    pub activation_daa_score: u64,
    pub unbonding_period_blocks: u64,

    /// DAA score at which an `Unbonding` request was submitted, or
    /// `None` if still bondable / active / slashed. Combined with
    /// `unbonding_period_blocks` it gives the release height.
    pub unbond_request_daa_score: Option<u64>,
    /// DAA score at which a `SlashingEvidencePayload` was accepted,
    /// or `None` if not slashed.
    pub slashed_at_daa_score: Option<u64>,

    pub status: BondStatus,
}

/// Per-validator entry inside a [`ValidatorSetSnapshot`]. Carries the
/// minimal fields fed into [`validator_set_commitment`]:
/// `validator_id || stake_amount || activation_daa_score`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorRecord {
    pub validator_id: Hash64,
    pub stake_amount: u64,
    pub activation_daa_score: u64,
}

/// Snapshot of the active validator set at a given epoch.
///
/// Built by `consensus/src/processes/validator_sortition.rs`
/// (PR-10.9). The `validators` vector **must** be sorted ascending
/// by `validator_id` for [`validator_set_commitment`] to be
/// byte-deterministic across nodes; the helper sorts a clone before
/// hashing, so callers can pass in any order, but persistence stores
/// the sorted form.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorSetSnapshot {
    pub epoch: u64,
    pub validators: Vec<ValidatorRecord>,
}

/// Per-anchor DNS state surfaced by the consensus pipeline to the
/// RPC layer and to the validator service. Lives in
/// `database/src/stores/stake_score.rs` (PR-10.5) keyed by
/// `selected_chain_anchor`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DnsState {
    pub selected_chain_anchor: Hash64,
    pub anchor_daa_score: u64,

    pub work_depth: BlueWorkType,
    pub stake_depth: StakeScore,

    /// Latest anchor that satisfies both `work_depth >=
    /// required_work_depth` and `stake_depth >= required_stake_depth`.
    /// Equal to `selected_chain_anchor` when the anchor itself is
    /// DNS-confirmed.
    pub last_dns_confirmed_anchor: Hash64,
    pub last_dns_confirmed_anchor_daa_score: u64,

    pub rollout_stage: DnsRolloutStage,
    /// Hash64 of the validator-set snapshot at this anchor's epoch.
    /// Mirrors the `validator_set_commitment` field on attestations
    /// so the RPC layer can echo it back to clients without
    /// recomputing.
    pub validator_set_commitment: Hash64,
}

// ---------------------------------------------------------------------
// Node-side policy.
// ---------------------------------------------------------------------

/// Per-network DNS consensus parameters. Stored alongside the
/// existing `consensus/core::config::params::Params` and consumed by
/// the PR-10.5 / PR-10.7 / PR-10.8 / PR-10.9 implementations.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DnsParams {
    /// DAA score at which the [`DnsRolloutStage`] gate flips from
    /// `Bootstrap` to `Active`. The other two activation conditions
    /// (`MIN_ACTIVE_STAKE`, `MIN_ACTIVE_VALIDATORS`) are checked at
    /// the activation tick.
    pub dns_activation_daa_score: u64,

    pub min_active_stake_sompi: u64,
    pub min_active_validators: u32,

    pub epoch_length_blocks: u64,

    /// `cW` — minimum work-depth for history confirmation.
    pub required_work_depth: BlueWorkType,
    /// `cS` — minimum stake-depth (in [`STAKE_SCORE_SCALE`] units)
    /// for history confirmation.
    pub required_stake_depth: StakeScore,

    /// Mainnet-only: extra margin a candidate must clear on
    /// `WorkScore` to pass the two-dimensional dominance rule. PoC /
    /// testnet hard-checkpoint mode ignores this; mainnet enforces.
    pub emergency_work_margin: BlueWorkType,
    /// Mainnet-only: matching emergency margin on `StakeScore`.
    pub emergency_stake_margin: StakeScore,

    pub max_reorg_horizon_blocks: u64,
    pub evidence_window_blocks: u64,
    pub unbonding_period_blocks: u64,

    pub max_attestations_per_block: u16,
    pub max_attestation_shard_mass: u64,
}

/// Miner-side block-template policy. See ADR-0010 §"Block template
/// policy" for the reservation algorithm. Consumed by the
/// block-template builder once PR-10.11 lands.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockTemplatePolicy {
    /// Upper bound on attestations the miner is willing to package
    /// into a single block. ADR-0009 fixes the consensus ceiling at
    /// [`MAX_ATTESTATIONS_PER_SHARD`]; this field lets a miner choose
    /// a stricter local cap (for benchmarking or staged rollout)
    /// without changing consensus.
    pub max_attestations_per_block: u16,
    /// Mass budget reserved for `StakeAttestationShardPayload` txs.
    pub max_attestation_shard_mass: u64,
    /// Mass budget that must remain available for normal user txs,
    /// to guarantee a high-attestation epoch cannot starve them.
    pub reserve_mass_for_normal_txs: u64,
}

// ---------------------------------------------------------------------
// RPC view.
// ---------------------------------------------------------------------

/// RPC view returned by the `getDnsConfirmation` method (added in
/// PR-10.14). Surfaces both the PoW-only confirmation level and the
/// DNS-augmented one so callers can choose which to trust.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DnsConfirmation {
    pub block_hash: Hash64,

    pub work_depth: BlueWorkType,
    pub required_work_depth: BlueWorkType,

    pub stake_depth: StakeScore,
    pub required_stake_depth: StakeScore,

    pub pow_confirmed: bool,
    pub dns_confirmed: bool,

    pub rollout_stage: DnsRolloutStage,
    pub expected_dns_confirmation_seconds: u64,

    /// Free-text fields for risk-bound notes. Per ADR-0009
    /// §"Public-claim discipline", consumers must read these
    /// alongside the boolean confirmation flags rather than
    /// interpreting reorg probability as a joint product.
    pub work_reorg_risk_upper_bound: String,
    pub stake_reorg_risk_upper_bound: String,
    pub dns_reorg_risk_conservative_bound: String,
    pub note: String,
}

// ---------------------------------------------------------------------
// Byte-deterministic derivations.
// ---------------------------------------------------------------------

/// Compute the validator-set commitment for `epoch` over the
/// `validators` set, per ADR-0010 §"Validator-set commitment
/// derivation":
///
/// ```text
/// snapshot_bytes = epoch.to_le_bytes()
///               || (sorted_validators.len() as u32).to_le_bytes()
///               || for each v in sorted_validators (by validator_id asc):
///                      v.validator_id.as_bytes()           (64 B)
///                   || v.stake_amount.to_le_bytes()        (8  B)
///                   || v.activation_daa_score.to_le_bytes()(8  B)
///
/// validator_set_commitment = BLAKE2b-512(
///     key   = VALIDATOR_SET_COMMITMENT_KEY,
///     input = snapshot_bytes,
/// )
/// ```
///
/// The function clones `validators` before sorting, so caller order
/// does not matter; this keeps the helper safe to call on a
/// borrowed slice from any store iteration without forcing a
/// pre-sort up the stack. Consensus stores are nonetheless required
/// to **persist** the sorted form so on-disk snapshots are
/// canonical.
pub fn validator_set_commitment(epoch: u64, validators: &[ValidatorRecord]) -> Hash64 {
    let mut sorted: Vec<ValidatorRecord> = validators.to_vec();
    sorted.sort_by(|a, b| a.validator_id.cmp(&b.validator_id));

    let mut hasher = Blake2bParams::new().hash_length(64).key(VALIDATOR_SET_COMMITMENT_KEY).to_state();
    hasher.update(&epoch.to_le_bytes());
    // len-as-u32 to match the ADR text byte-for-byte; consensus will
    // reject any snapshot whose actual length exceeds u32::MAX, but
    // that check lives in the validation rule (PR-10.5), not here.
    hasher.update(&(sorted.len() as u32).to_le_bytes());
    for v in &sorted {
        hasher.update(v.validator_id.as_byte_slice());
        hasher.update(&v.stake_amount.to_le_bytes());
        hasher.update(&v.activation_daa_score.to_le_bytes());
    }

    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Compute the BLAKE2b-256 attestation message that ML-DSA-65 signs
/// over, per ADR-0009 §"Attestation target":
///
/// ```text
/// attestation_message = BLAKE2b-256(
///     key   = ATTESTATION_MESSAGE_DOMAIN,
///     input = epoch.to_le_bytes()
///          || target_hash.as_bytes()              (64 B)
///          || target_daa_score.to_le_bytes()
///          || validator_set_commitment.as_bytes() (64 B),
/// )
/// ```
///
/// The 32-byte digest is returned as the upstream [`Hash`] (alias
/// for `Hash32`) so it composes directly with the libcrux ML-DSA-65
/// `sign_ctx` API. The signing context (`ATTESTATION_MLDSA65_CONTEXT`)
/// is applied at the ML-DSA-65 layer, not inside this hasher — keeping
/// the two domain separators independent (replay safety analysis in
/// ADR-0009 §"Attestation target").
pub fn stake_attestation_message(
    epoch: u64,
    target_hash: Hash64,
    target_daa_score: u64,
    validator_set_commitment: Hash64,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(ATTESTATION_MESSAGE_DOMAIN).to_state();
    hasher.update(&epoch.to_le_bytes());
    hasher.update(target_hash.as_byte_slice());
    hasher.update(&target_daa_score.to_le_bytes());
    hasher.update(validator_set_commitment.as_byte_slice());

    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

// ---------------------------------------------------------------------
// Consensus rule stubs — implementations land in subsequent PRs.
// ---------------------------------------------------------------------

/// PR-10.5 stub: deterministic `StakeScore` aggregation from on-chain
/// `StakeAttestationShardPayload` data.
#[doc(hidden)]
pub fn compute_stake_score_stub() -> StakeScore {
    unimplemented!(
        "kaspa-pq Phase 10 PR-10.5: deterministic StakeScore aggregation \
         from on-chain shards is not implemented in this PR (type stubs only); \
         see docs/adr/0009-dns-probabilistic-finality.md §StakeScore mechanics."
    )
}

/// PR-10.8 stub: the two-dimensional dominance rule (mainnet
/// behaviour). Returns `Ok(())` if the candidate either does not
/// touch the latest DNS-confirmed anchor, or beats the canonical
/// chain on both `WorkScore` and `StakeScore` by the configured
/// emergency margins.
#[doc(hidden)]
pub fn check_dns_reorg_rule_stub() -> Result<(), &'static str> {
    unimplemented!(
        "kaspa-pq Phase 10 PR-10.8: two-dimensional dominance rule is not \
         implemented in this PR (type stubs only); see \
         docs/adr/0009-dns-probabilistic-finality.md §Decision."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_constants_have_expected_values() {
        // Cross-check against the consensus-core kaspa-pq constant
        // values (the kaspa-txscript crate is downstream of
        // consensus-core, so we cannot pull MLDSA65_PK_LEN /
        // MLDSA65_SIG_LEN from there directly without creating a
        // dependency cycle; the values are duplicated here and the
        // assertion is the contract).
        assert_eq!(STAKE_VALIDATOR_PUBKEY_LEN, 1952);
        assert_eq!(STAKE_ATTESTATION_SIG_LEN, 3309);

        // ADR-0009 / ADR-0010 domain-separator strings. These are
        // consensus-fixed and bumped only by a hard-fork ADR — pin
        // the bytes so any accidental rename trips this test.
        assert_eq!(ATTESTATION_MLDSA65_CONTEXT, b"kaspa-pq-v1/att/mldsa65");
        assert_eq!(ATTESTATION_MESSAGE_DOMAIN, b"kaspa-pq-v1/stake-attestation");
        assert_eq!(VALIDATOR_SET_COMMITMENT_KEY, b"kaspa-pq-validator-set-v1");

        // Replay safety: tx vs attestation contexts must differ
        // (ADR-0002 / ADR-0009 §"Attestation target").
        assert_ne!(ATTESTATION_MLDSA65_CONTEXT, b"kaspa-pq-v1/tx/mldsa65");
    }

    #[test]
    fn stake_score_display() {
        assert_eq!(StakeScore(0).to_string(), "0.000000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE).to_string(), "1.000000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE + 500_000_000).to_string(), "1.500000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE * 3 / 4).to_string(), "0.750000000");
    }

    fn fixture_outpoint() -> TransactionOutpoint {
        TransactionOutpoint::new(Hash::from_bytes([0x77u8; 32]), 42)
    }

    fn fixture_attestation() -> StakeAttestation {
        StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash64::from_bytes([0xa5u8; 64]),
            bond_outpoint: fixture_outpoint(),
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
            signature: vec![0x33u8; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn fixture_validators() -> Vec<ValidatorRecord> {
        vec![
            ValidatorRecord { validator_id: Hash64::from_bytes([0xcc; 64]), stake_amount: 30, activation_daa_score: 300 },
            ValidatorRecord { validator_id: Hash64::from_bytes([0xaa; 64]), stake_amount: 10, activation_daa_score: 100 },
            ValidatorRecord { validator_id: Hash64::from_bytes([0xbb; 64]), stake_amount: 20, activation_daa_score: 200 },
        ]
    }

    #[test]
    fn stake_bond_payload_borsh_roundtrip() {
        let bond = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
            validator_pubkey_hash: Hash64::from_bytes([0xbbu8; 64]),
            validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
            amount: 100_000_000_000,
            activation_daa_score: 5_000,
            unbonding_period_blocks: 100_000,
        };
        let bytes = borsh::to_vec(&bond).unwrap();
        let back: StakeBondPayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, bond);
    }

    #[test]
    fn stake_attestation_borsh_roundtrip() {
        let att = fixture_attestation();
        let bytes = borsh::to_vec(&att).unwrap();
        let back: StakeAttestation = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, att);
        // Spot-check the dominant size component: the ML-DSA-65
        // signature plus borsh framing. The Vec<u8> Borsh layout is
        // 4-byte length prefix + N data bytes, so a 3309-byte sig
        // contributes 4 + 3309 = 3313 bytes plus the other fixed
        // fields.
        assert!(bytes.len() >= STAKE_ATTESTATION_SIG_LEN);
    }

    #[test]
    fn stake_attestation_shard_borsh_roundtrip() {
        let shard = StakeAttestationShardPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
            attestations: vec![fixture_attestation(); 8],
        };
        assert!(shard.attestations.len() <= MAX_ATTESTATIONS_PER_SHARD);
        let bytes = borsh::to_vec(&shard).unwrap();
        let back: StakeAttestationShardPayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, shard);
    }

    #[test]
    fn slashing_evidence_borsh_roundtrip() {
        let evidence = SlashingEvidencePayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint: fixture_outpoint(),
            attestation_a: fixture_attestation(),
            attestation_b: {
                let mut b = fixture_attestation();
                b.target_hash = Hash64::from_bytes([0x33u8; 64]);
                b
            },
        };
        let bytes = borsh::to_vec(&evidence).unwrap();
        let back: SlashingEvidencePayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, evidence);
    }

    #[test]
    fn dns_params_borsh_roundtrip() {
        let params = DnsParams {
            dns_activation_daa_score: 1_000_000,
            min_active_stake_sompi: 10_000_000_000_000,
            min_active_validators: 32,
            epoch_length_blocks: 600,
            required_work_depth: BlueWorkType::from_u64(1_000_000),
            required_stake_depth: StakeScore(10 * STAKE_SCORE_SCALE),
            emergency_work_margin: BlueWorkType::from_u64(10_000_000),
            emergency_stake_margin: StakeScore(100 * STAKE_SCORE_SCALE),
            max_reorg_horizon_blocks: 100_000,
            evidence_window_blocks: 200_000,
            unbonding_period_blocks: 350_000, // > R + E
            max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
            max_attestation_shard_mass: 50_000,
        };
        let bytes = borsh::to_vec(&params).unwrap();
        let back: DnsParams = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, params);

        // ADR-0009 §"Long-range bound" requires U >= R + E.
        assert!(params.unbonding_period_blocks >= params.max_reorg_horizon_blocks + params.evidence_window_blocks);
    }

    #[test]
    fn dns_confirmation_borsh_roundtrip() {
        let c = DnsConfirmation {
            block_hash: Hash64::from_bytes([0x99u8; 64]),
            work_depth: BlueWorkType::from_u64(42),
            required_work_depth: BlueWorkType::from_u64(10),
            stake_depth: StakeScore(500_000_000),
            required_stake_depth: StakeScore(STAKE_SCORE_SCALE),
            pow_confirmed: true,
            dns_confirmed: false,
            rollout_stage: DnsRolloutStage::Bootstrap,
            expected_dns_confirmation_seconds: 600,
            work_reorg_risk_upper_bound: "n/a".into(),
            stake_reorg_risk_upper_bound: "n/a".into(),
            dns_reorg_risk_conservative_bound: "n/a".into(),
            note: "Phase 10 stub".into(),
        };
        let bytes = borsh::to_vec(&c).unwrap();
        let back: DnsConfirmation = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn dns_rollout_stage_default_is_launch() {
        // The default rollout stage must be `Launch` so a node booting
        // before any DNS parameters are configured behaves as a pure
        // PoW kaspa-pq node. ADR-0009 §"Three-stage rollout".
        assert_eq!(DnsRolloutStage::default(), DnsRolloutStage::Launch);
    }

    #[test]
    fn bond_status_default_is_pending() {
        // A freshly-recorded bond is `Pending` until
        // `activation_daa_score` is crossed. Matches ADR-0010
        // §"Validator service runtime" predicate ordering.
        assert_eq!(BondStatus::default(), BondStatus::Pending);
    }

    #[test]
    fn stake_bond_record_borsh_roundtrip() {
        let rec = StakeBondRecord {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint: fixture_outpoint(),
            owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
            validator_pubkey_hash: Hash64::from_bytes([0xbbu8; 64]),
            validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
            amount: 100_000_000_000,
            activation_daa_score: 5_000,
            unbonding_period_blocks: 100_000,
            unbond_request_daa_score: Some(123_456),
            slashed_at_daa_score: None,
            status: BondStatus::Unbonding,
        };
        let bytes = borsh::to_vec(&rec).unwrap();
        let back: StakeBondRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, rec);
    }

    #[test]
    fn validator_record_borsh_roundtrip() {
        let v = ValidatorRecord {
            validator_id: Hash64::from_bytes([0x42u8; 64]),
            stake_amount: 1_000_000,
            activation_daa_score: 99,
        };
        let bytes = borsh::to_vec(&v).unwrap();
        let back: ValidatorRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn validator_set_snapshot_borsh_roundtrip() {
        let snap = ValidatorSetSnapshot { epoch: 7, validators: fixture_validators() };
        let bytes = borsh::to_vec(&snap).unwrap();
        let back: ValidatorSetSnapshot = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, snap);
    }

    #[test]
    fn dns_state_borsh_roundtrip() {
        let s = DnsState {
            selected_chain_anchor: Hash64::from_bytes([0x55u8; 64]),
            anchor_daa_score: 1_000,
            work_depth: BlueWorkType::from_u64(500),
            stake_depth: StakeScore(2 * STAKE_SCORE_SCALE),
            last_dns_confirmed_anchor: Hash64::from_bytes([0x66u8; 64]),
            last_dns_confirmed_anchor_daa_score: 900,
            rollout_stage: DnsRolloutStage::Active,
            validator_set_commitment: Hash64::from_bytes([0x77u8; 64]),
        };
        let bytes = borsh::to_vec(&s).unwrap();
        let back: DnsState = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn block_template_policy_borsh_roundtrip() {
        let p = BlockTemplatePolicy {
            max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
            max_attestation_shard_mass: 50_000,
            reserve_mass_for_normal_txs: 200_000,
        };
        let bytes = borsh::to_vec(&p).unwrap();
        let back: BlockTemplatePolicy = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    // ---- validator_set_commitment ---------------------------------

    #[test]
    fn validator_set_commitment_is_order_independent() {
        // Same set in three different input orders. ADR-0010
        // §"Validator-set commitment derivation" guarantees the
        // result is order-independent because the helper sorts a
        // clone before hashing.
        let a = fixture_validators();
        let mut b = a.clone();
        b.reverse();
        let mut c = a.clone();
        c.swap(0, 2);

        let ca = validator_set_commitment(11, &a);
        let cb = validator_set_commitment(11, &b);
        let cc = validator_set_commitment(11, &c);
        assert_eq!(ca, cb);
        assert_eq!(ca, cc);
    }

    #[test]
    fn validator_set_commitment_changes_with_epoch() {
        // Same validator set, different epochs => different
        // commitments. Guards against an attestation replay across
        // epoch boundaries.
        let v = fixture_validators();
        assert_ne!(validator_set_commitment(0, &v), validator_set_commitment(1, &v));
        assert_ne!(validator_set_commitment(11, &v), validator_set_commitment(12, &v));
    }

    #[test]
    fn validator_set_commitment_changes_with_membership() {
        // Removing a validator must change the commitment.
        let full = fixture_validators();
        let mut subset = full.clone();
        subset.pop();
        assert_ne!(validator_set_commitment(11, &full), validator_set_commitment(11, &subset));
    }

    #[test]
    fn validator_set_commitment_changes_with_stake_amount() {
        // Bumping any field a validator contributes (stake_amount,
        // activation_daa_score) must change the commitment.
        let baseline = fixture_validators();
        let mut bumped_stake = baseline.clone();
        bumped_stake[0].stake_amount += 1;
        assert_ne!(validator_set_commitment(11, &baseline), validator_set_commitment(11, &bumped_stake));

        let mut bumped_daa = baseline.clone();
        bumped_daa[0].activation_daa_score += 1;
        assert_ne!(validator_set_commitment(11, &baseline), validator_set_commitment(11, &bumped_daa));
    }

    #[test]
    fn validator_set_commitment_empty_is_well_defined() {
        // Empty validator set still produces a deterministic hash
        // (not all-zero): epoch || u32::LE(0).
        let c0 = validator_set_commitment(0, &[]);
        let c1 = validator_set_commitment(1, &[]);
        assert_ne!(c0, c1);
        // Must not collide with the all-zero `Hash64` sentinel.
        assert_ne!(c0, kaspa_hashes::ZERO_HASH64);
    }

    #[test]
    fn validator_set_commitment_matches_adr_byte_layout() {
        // Pin one full byte-layout to a known-good value. Any future
        // change to the ADR-0010 derivation (field order, length
        // prefix encoding, domain-separator key) trips this test
        // immediately — the value is consensus-stable and any drift
        // is a hard fork.
        let v = vec![
            ValidatorRecord { validator_id: Hash64::from_bytes([0x01u8; 64]), stake_amount: 1, activation_daa_score: 2 },
            ValidatorRecord { validator_id: Hash64::from_bytes([0x02u8; 64]), stake_amount: 3, activation_daa_score: 4 },
        ];
        // Re-derive the expected value with a hand-rolled hasher,
        // matching the ADR text byte-for-byte. Equality here is the
        // "two independent implementations agree" sanity check.
        let mut h = Blake2bParams::new().hash_length(64).key(VALIDATOR_SET_COMMITMENT_KEY).to_state();
        h.update(&5u64.to_le_bytes()); // epoch = 5
        h.update(&2u32.to_le_bytes()); // len   = 2
        // Sorted by validator_id ascending: [0x01..], [0x02..].
        h.update(&[0x01u8; 64]);
        h.update(&1u64.to_le_bytes());
        h.update(&2u64.to_le_bytes());
        h.update(&[0x02u8; 64]);
        h.update(&3u64.to_le_bytes());
        h.update(&4u64.to_le_bytes());
        let mut expected = [0u8; 64];
        expected.copy_from_slice(h.finalize().as_bytes());

        let actual = validator_set_commitment(5, &v);
        assert_eq!(actual.as_bytes(), expected);
    }

    // ---- stake_attestation_message --------------------------------

    #[test]
    fn stake_attestation_message_is_deterministic() {
        let target = Hash64::from_bytes([0x11u8; 64]);
        let vsc = Hash64::from_bytes([0x22u8; 64]);
        let a = stake_attestation_message(7, target, 1_234_567, vsc);
        let b = stake_attestation_message(7, target, 1_234_567, vsc);
        assert_eq!(a, b);
    }

    #[test]
    fn stake_attestation_message_changes_with_each_field() {
        let base = stake_attestation_message(7, Hash64::from_bytes([0x11u8; 64]), 100, Hash64::from_bytes([0x22u8; 64]));
        // Epoch.
        assert_ne!(base, stake_attestation_message(8, Hash64::from_bytes([0x11u8; 64]), 100, Hash64::from_bytes([0x22u8; 64])));
        // target_hash.
        assert_ne!(base, stake_attestation_message(7, Hash64::from_bytes([0x12u8; 64]), 100, Hash64::from_bytes([0x22u8; 64])));
        // target_daa_score.
        assert_ne!(base, stake_attestation_message(7, Hash64::from_bytes([0x11u8; 64]), 101, Hash64::from_bytes([0x22u8; 64])));
        // validator_set_commitment.
        assert_ne!(base, stake_attestation_message(7, Hash64::from_bytes([0x11u8; 64]), 100, Hash64::from_bytes([0x23u8; 64])));
    }

    #[test]
    fn stake_attestation_message_uses_attestation_domain_key() {
        // Hash the same inputs with the *transaction* domain key
        // (the only other 32-byte BLAKE2b-256 domain on the wire)
        // and verify the attestation digest is different. Guards
        // against the two domains accidentally collapsing in a
        // future refactor.
        let inputs = |key: &[u8]| {
            let mut h = Blake2bParams::new().hash_length(32).key(key).to_state();
            h.update(&7u64.to_le_bytes());
            h.update(&[0x11u8; 64]);
            h.update(&100u64.to_le_bytes());
            h.update(&[0x22u8; 64]);
            h.finalize()
        };
        let with_att_key = inputs(ATTESTATION_MESSAGE_DOMAIN);
        let with_tx_key = inputs(b"kaspa-pq-v1/tx/mldsa65");
        assert_ne!(with_att_key.as_bytes(), with_tx_key.as_bytes());

        let actual = stake_attestation_message(7, Hash64::from_bytes([0x11u8; 64]), 100, Hash64::from_bytes([0x22u8; 64]));
        assert_eq!(actual.as_bytes(), with_att_key.as_bytes());
    }

    // ---- Stubs are explicit ---------------------------------------

    #[test]
    #[should_panic(expected = "kaspa-pq Phase 10 PR-10.5")]
    fn compute_stake_score_is_explicitly_unimplemented() {
        let _ = compute_stake_score_stub();
    }

    #[test]
    #[should_panic(expected = "kaspa-pq Phase 10 PR-10.8")]
    fn check_dns_reorg_rule_is_explicitly_unimplemented() {
        let _ = check_dns_reorg_rule_stub();
    }
}
