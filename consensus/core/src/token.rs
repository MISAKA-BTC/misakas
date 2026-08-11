//! MISAKA Compute Token Program — Phase A pure consensus surface
//! (`docs/misaka-compute-token-program-design-v0.1.md`).
//!
//! The protocol-native asset ledger (design §4) and the compute-backed emission
//! math (design §5). The flagship asset — **Token (TOK)**, `asset_id = 0` — has
//! **no mint authority**: nothing in this module (or anywhere else) lets a key
//! mint it. Its only issuance path is [`emission_rewards`] over an epoch's
//! *finalized* verified-compute credits [`crate::vlt::VltEpochCredits`], which is
//! the design's "PoW coinbase" analogy made literal: hash-work → block reward
//! becomes verified-LLM-work → TOK.
//!
//! # What this module is (and is not)
//!
//! Like [`crate::vlt`], this is the **pure, deterministic** consensus surface:
//! payload types, stateless validation, ledger-application semantics, and the
//! emission function. It performs no I/O, holds no state, and verifies no
//! ML-DSA-87 signatures (signature *length* is stateless and checked here;
//! signature *validity* is checked where the ledger is applied, exactly as the
//! credit walk does for certificates). The stores live in
//! `consensus/src/model/stores/token_ledger.rs`; the virtual-processor seam that
//! applies accepted payloads and settles emission is deliberately **not** part
//! of Phase A PR 1 (design §9.5 — it lands with the params wiring so this PR is
//! zero-behavior-change on every network).
//!
//! # The two consumers of one measure (design §6.2)
//!
//! `X_i(E)` already weights DNS-finality votes (decayed, non-transferable,
//! bond-capped — [`crate::vlt`]). Emission adds the second consumer: money
//! (undecayed, transferable, uncapped). The two never mix — a TOK balance MUST
//! NOT enter any voting-weight read, and this module gives it no way to.
//!
//! # Fork invariance for free (design §5.3)
//!
//! [`emission_rewards`] is meant to be fed from the finalized credit store
//! (`DbVltCreditStore`), whose rows are written only once an epoch is buried
//! past both the challenge window and the reorg horizon
//! ([`crate::vlt::vlt_epoch_finalized`]). Below that depth every branch shares
//! the same history, so the reward vector is the same on every branch by
//! construction — no coinbase-style maturity and no clawback path exist, and
//! none are needed. [`TokenParams::is_coherent_with_vlt`] pins the settlement
//! delay above that burial depth.

use blake2b_simd::Params as Blake2bParams;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};
use std::collections::BTreeMap;

use crate::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN, validator_id_from_pubkey};
use crate::vlt::VltEpochCredits;

// ---------------------------------------------------------------------
// Constants.
// ---------------------------------------------------------------------

/// Wire-format version for every payload in this module.
pub const TOKEN_PAYLOAD_VERSION_V1: u16 = 1;

/// `asset_id` of the protocol asset **Token (TOK)** — design §4.1. Reserved: no
/// `CreateMint` (Phase B) may ever claim it, and no mint authority exists for it.
pub const TOK_ASSET_ID: u64 = 0;

/// Atomic units per 1 TOK (`10^8`, matching the sompi convention — design §4.2).
pub const TOK_ATOMIC_PER_UNIT: u64 = 100_000_000;

/// Keyed-BLAKE2b-256 domain for [`token_transfer_message`]. A distinct key per
/// role so a digest from one role can never be replayed as another — same
/// discipline as the VLT domains.
pub const TOKEN_TRANSFER_MESSAGE_DOMAIN: &[u8] = b"misaka-tkn-v1/transfer";
/// Keyed-BLAKE2b-256 domain for [`token_burn_message`].
pub const TOKEN_BURN_MESSAGE_DOMAIN: &[u8] = b"misaka-tkn-v1/burn";

/// libcrux ML-DSA-87 `ctx` for a transfer signature. Distinct from every other
/// overlay context on purpose: a signature produced for any other role must not
/// verify as a transfer, whatever its message happens to be.
pub const TOKEN_TRANSFER_MLDSA87_CONTEXT: &[u8] = b"misaka-tkn-v1/transfer/mldsa87";
/// libcrux ML-DSA-87 `ctx` for a burn signature.
pub const TOKEN_BURN_MLDSA87_CONTEXT: &[u8] = b"misaka-tkn-v1/burn/mldsa87";

// Phase B (design §4.6, revised): permissionless mints. Domains/contexts follow the
// same one-key-per-role discipline.
/// Keyed-BLAKE2b-256 domain for [`token_create_mint_message`].
pub const TOKEN_CREATE_MINT_MESSAGE_DOMAIN: &[u8] = b"misaka-tkn-v1/create-mint";
/// Keyed-BLAKE2b-256 domain for [`token_mint_to_message`].
pub const TOKEN_MINT_TO_MESSAGE_DOMAIN: &[u8] = b"misaka-tkn-v1/mint-to";
/// libcrux ML-DSA-87 `ctx` for a create-mint signature.
pub const TOKEN_CREATE_MINT_MLDSA87_CONTEXT: &[u8] = b"misaka-tkn-v1/create-mint/mldsa87";
/// libcrux ML-DSA-87 `ctx` for a mint-to signature.
pub const TOKEN_MINT_TO_MLDSA87_CONTEXT: &[u8] = b"misaka-tkn-v1/mint-to/mldsa87";
/// Keyed-BLAKE2b-256 domain for [`asset_id_for_mint`].
pub const TOKEN_ASSET_ID_KEY: &[u8] = b"misaka-tkn-v1/asset-id";

/// The asset id a `CreateMint` by `creator` at `create_nonce` claims — Phase B, design §4.6
/// **revised**: derived from `(creator, nonce)` rather than the carrier transaction id, so
/// re-carrying the same signed payload on a fresh fee tx (which the nonce design deliberately
/// permits) re-claims the SAME asset and voids on the nonce, instead of double-creating.
///
/// `max(1)` keeps the result off [`TOK_ASSET_ID`]; the residual chance of two honest mints
/// colliding on one id is a birthday bound over 2^64, and a collision is merely the second
/// create voiding under first-wins — the same outcome any duplicate gets.
pub fn asset_id_for_mint(creator: Hash64, create_nonce: u64) -> u64 {
    let mut hasher = Blake2bParams::new().hash_length(32).key(TOKEN_ASSET_ID_KEY).to_state();
    hasher.update(creator.as_byte_slice());
    hasher.update(&create_nonce.to_le_bytes());
    let bytes = hasher.finalize();
    u64::from_le_bytes(bytes.as_bytes()[..8].try_into().expect("8 bytes")).max(1)
}

/// Sentinel for an uncapped mint (design: `supply_cap = u128::MAX`); a zero cap is rejected
/// statelessly — a mint that can never issue is a misconfiguration, not a policy.
pub const TOKEN_SUPPLY_CAP_UNCAPPED: u128 = u128::MAX;

// ---------------------------------------------------------------------
// Payloads (design §4.3 — subnetworks 0x30/0x31).
// ---------------------------------------------------------------------

/// A `SUBNETWORK_ID_TOKEN_TRANSFER` (0x30) payload: move `amount` atomic units
/// of `asset_id` from the account of `from_pubkey`'s hash to `to`.
///
/// The carrier transaction is an ordinary base-coin transaction (mass-priced
/// fee in base coin, design §4.3); this payload rides it the way every overlay
/// payload does. Replay protection is the ledger nonce, not the carrier tx:
/// the signed message binds `(network, asset, from, to, amount, nonce)`, and
/// application requires `nonce == stored + 1`, so the same signed payload can
/// never apply twice — while a wallet stays free to re-carry an unapplied
/// payload on a different fee tx without re-signing (design §4.4).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenTransferPayload {
    pub version: u16,
    /// Phase A: must be [`TOK_ASSET_ID`] (checked statelessly — there is no
    /// other asset until Phase B's `CreateMint`).
    pub asset_id: u64,
    /// The sender's ML-DSA-87 verifying key ([`STAKE_VALIDATOR_PUBKEY_LEN`]
    /// bytes). The debited owner id is
    /// [`validator_id_from_pubkey`]`(from_pubkey)` — the same unkeyed
    /// BLAKE2b-512 identity every overlay role uses, so one key is one identity
    /// across bonds, compute, and tokens.
    pub from_pubkey: Vec<u8>,
    /// The credited owner id. An id, not a key: crediting needs no signature,
    /// and the recipient's key is published if and when it first spends.
    pub to: Hash64,
    /// Atomic units. Zero is rejected statelessly.
    pub amount: u128,
    /// Must equal the sender's stored `(asset, owner)` nonce + 1 at application.
    pub nonce: u64,
    /// ML-DSA-87 over [`token_transfer_message`] under
    /// [`TOKEN_TRANSFER_MLDSA87_CONTEXT`]. Length checked statelessly;
    /// verified at application.
    pub signature: Vec<u8>,
}

/// A `SUBNETWORK_ID_TOKEN_BURN` (0x31) payload: destroy `amount` atomic units
/// from the signer's own account, decreasing circulating supply forever.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenBurnPayload {
    pub version: u16,
    /// Phase A: must be [`TOK_ASSET_ID`].
    pub asset_id: u64,
    /// The burner's ML-DSA-87 verifying key; the debited owner id is its
    /// [`validator_id_from_pubkey`] hash.
    pub owner_pubkey: Vec<u8>,
    /// Atomic units. Zero is rejected statelessly.
    pub amount: u128,
    /// Must equal the owner's stored nonce + 1 at application. Transfers and
    /// burns share one nonce sequence per `(asset, owner)`.
    pub nonce: u64,
    /// ML-DSA-87 over [`token_burn_message`] under [`TOKEN_BURN_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

/// Digest a sender signs for a transfer.
///
/// `network_id` is bound in for the same reason every overlay message binds it:
/// without it a testnet signature would be a mainnet signature. `from` is bound
/// so the digest names the debited account explicitly rather than leaving it
/// implicit in key possession.
pub fn token_transfer_message(network_id: &[u8], asset_id: u64, from: Hash64, to: Hash64, amount: u128, nonce: u64) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(TOKEN_TRANSFER_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(&asset_id.to_le_bytes());
    hasher.update(from.as_byte_slice());
    hasher.update(to.as_byte_slice());
    hasher.update(&amount.to_le_bytes());
    hasher.update(&nonce.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Digest an owner signs for a burn.
pub fn token_burn_message(network_id: &[u8], asset_id: u64, owner: Hash64, amount: u128, nonce: u64) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(TOKEN_BURN_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(&asset_id.to_le_bytes());
    hasher.update(owner.as_byte_slice());
    hasher.update(&amount.to_le_bytes());
    hasher.update(&nonce.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// A `SUBNETWORK_ID_TOKEN_CREATE_MINT` (0x32) payload — Phase B: claim
/// [`asset_id_for_mint`]`(creator, nonce)` and fix its immutable mint policy.
///
/// The nonce is the creator's **TOK-line** nonce (`(TOK_ASSET_ID, creator)`):
/// a create is assetless until it succeeds, and binding it to the one nonce
/// line every identity already has keeps replay protection uniform.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenCreateMintPayload {
    pub version: u16,
    /// The creator's ML-DSA-87 verifying key; creator id (and mint authority) is its
    /// [`validator_id_from_pubkey`] hash.
    pub creator_pubkey: Vec<u8>,
    /// Immutable issuance ceiling in atomic units ([`TOKEN_SUPPLY_CAP_UNCAPPED`] = none;
    /// zero rejected statelessly).
    pub supply_cap: u128,
    /// Display metadata only — no consensus meaning.
    pub decimals: u8,
    /// Must equal the creator's stored `(TOK, creator)` nonce + 1 at application.
    pub nonce: u64,
    /// ML-DSA-87 over [`token_create_mint_message`] under
    /// [`TOKEN_CREATE_MINT_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

/// A `SUBNETWORK_ID_TOKEN_MINT_TO` (0x33) payload — Phase B: issue `amount` of `asset_id`
/// to `to`, signed by the mint authority. [`TOK_ASSET_ID`] is rejected statelessly: TOK has
/// no mint authority, and no payload may claim otherwise (design §4.1).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenMintToPayload {
    pub version: u16,
    pub asset_id: u64,
    /// The mint authority's ML-DSA-87 verifying key.
    pub authority_pubkey: Vec<u8>,
    pub to: Hash64,
    /// Atomic units. Zero rejected statelessly.
    pub amount: u128,
    /// Must equal the authority's stored `(asset_id, authority)` nonce + 1 at application —
    /// the minted asset's own nonce line, like a transfer's.
    pub nonce: u64,
    /// ML-DSA-87 over [`token_mint_to_message`] under [`TOKEN_MINT_TO_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

/// Digest a creator signs to claim a mint.
pub fn token_create_mint_message(network_id: &[u8], creator: Hash64, supply_cap: u128, decimals: u8, nonce: u64) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(TOKEN_CREATE_MINT_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(creator.as_byte_slice());
    hasher.update(&supply_cap.to_le_bytes());
    hasher.update(&[decimals]);
    hasher.update(&nonce.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Digest a mint authority signs to issue.
pub fn token_mint_to_message(network_id: &[u8], asset_id: u64, authority: Hash64, to: Hash64, amount: u128, nonce: u64) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(TOKEN_MINT_TO_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(&asset_id.to_le_bytes());
    hasher.update(authority.as_byte_slice());
    hasher.update(to.as_byte_slice());
    hasher.update(&amount.to_le_bytes());
    hasher.update(&nonce.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// A mint's immutable policy row — Phase B, keyed by asset id in the token store family.
/// No freeze, no clawback, no authority rotation (design §11): what CreateMint fixed is
/// what the asset is.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct TokenMintMeta {
    pub creator: Hash64,
    /// The only key that may sign [`TokenMintToPayload`]s for this asset. Phase B fixes it
    /// to the creator; a distinct-authority variant is a later extension if ever needed.
    pub mint_authority: Hash64,
    pub supply_cap: u128,
    pub decimals: u8,
}

impl kaspa_utils::mem_size::MemSizeEstimator for TokenMintMeta {}

// ---------------------------------------------------------------------
// Stateless validation (design §4.3: two-stage, like every overlay band).
// ---------------------------------------------------------------------

/// Stateless validation failure for a token-op payload. The consensus
/// tx-validation layer will wrap this the way [`crate::dns_finality::DnsTxError`]
/// is wrapped (wiring PR); defined separately so this module does not grow the
/// DNS error surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenTxError {
    /// Payload bytes did not borsh-decode into the expected type (also fires
    /// on trailing bytes after an otherwise-valid prefix).
    Decode,
    /// The `version` field is not `TOKEN_PAYLOAD_VERSION_V1`.
    UnsupportedVersion(u16),
    /// Phase A knows only [`TOK_ASSET_ID`]; anything else is Phase B.
    UnknownAsset(u64),
    /// Sender/owner public key is not exactly `STAKE_VALIDATOR_PUBKEY_LEN`.
    InvalidPubKeyLen(usize),
    /// Signature is not exactly `STAKE_ATTESTATION_SIG_LEN`.
    InvalidSignatureLen(usize),
    /// `amount == 0` — a no-op that would still bump a nonce and bloat the
    /// ledger walk; rejected at the door.
    ZeroAmount,
    /// `to == hash(from_pubkey)` — a transfer to self moves nothing and exists
    /// only to bump a nonce. Rejecting it statelessly also spares the
    /// application seam from aliasing a debit and a credit of one account.
    SelfTransfer,
    /// Phase B: a `CreateMint` with `supply_cap == 0` — a mint that can never issue.
    ZeroSupplyCap,
    /// Phase B: a `MintTo` naming [`TOK_ASSET_ID`]. TOK has no mint authority, and no
    /// payload may claim otherwise (design §4.1) — rejected at the door, not just voided.
    TokMintForbidden,
}

impl std::fmt::Display for TokenTxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode => write!(f, "token payload failed to decode"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported token payload version {v}"),
            Self::UnknownAsset(id) => write!(f, "unknown asset id {id} (Phase A knows only TOK = {TOK_ASSET_ID})"),
            Self::InvalidPubKeyLen(l) => write!(f, "invalid pubkey length {l} (expected {STAKE_VALIDATOR_PUBKEY_LEN})"),
            Self::InvalidSignatureLen(l) => write!(f, "invalid signature length {l} (expected {STAKE_ATTESTATION_SIG_LEN})"),
            Self::ZeroAmount => write!(f, "token op amount must be non-zero"),
            Self::SelfTransfer => write!(f, "token transfer to self is rejected"),
            Self::ZeroSupplyCap => write!(f, "create-mint supply cap must be non-zero (u128::MAX = uncapped)"),
            Self::TokMintForbidden => write!(f, "TOK ({TOK_ASSET_ID}) has no mint authority; mint-to is forbidden"),
        }
    }
}

impl std::error::Error for TokenTxError {}

/// Decode a transfer payload (for the ledger fold, which lives in a crate
/// without a direct borsh dependency). `None` on malformed bytes — admission
/// already rejected those, so the fold treats it as a void op, not an error.
pub fn decode_token_transfer_payload(payload: &[u8]) -> Option<TokenTransferPayload> {
    borsh::from_slice(payload).ok()
}

/// Decode a burn payload — see [`decode_token_transfer_payload`].
pub fn decode_token_burn_payload(payload: &[u8]) -> Option<TokenBurnPayload> {
    borsh::from_slice(payload).ok()
}

/// Decode a create-mint payload (Phase B) — see [`decode_token_transfer_payload`].
pub fn decode_token_create_mint_payload(payload: &[u8]) -> Option<TokenCreateMintPayload> {
    borsh::from_slice(payload).ok()
}

/// Decode a mint-to payload (Phase B) — see [`decode_token_transfer_payload`].
pub fn decode_token_mint_to_payload(payload: &[u8]) -> Option<TokenMintToPayload> {
    borsh::from_slice(payload).ok()
}

/// Stateless validation of a [`TokenTransferPayload`]'s bytes — everything
/// checkable without a chain. Nonce currency, balance sufficiency, and the
/// ML-DSA-87 signature itself are stateful and belong to the application seam.
pub fn validate_token_transfer_payload(payload: &[u8]) -> Result<(), TokenTxError> {
    let p: TokenTransferPayload = borsh::from_slice(payload).map_err(|_| TokenTxError::Decode)?;
    if p.version != TOKEN_PAYLOAD_VERSION_V1 {
        return Err(TokenTxError::UnsupportedVersion(p.version));
    }
    if p.from_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(TokenTxError::InvalidPubKeyLen(p.from_pubkey.len()));
    }
    if p.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(TokenTxError::InvalidSignatureLen(p.signature.len()));
    }
    if p.amount == 0 {
        return Err(TokenTxError::ZeroAmount);
    }
    if validator_id_from_pubkey(&p.from_pubkey) == p.to {
        return Err(TokenTxError::SelfTransfer);
    }
    Ok(())
}

/// Stateless validation of a [`TokenBurnPayload`]'s bytes.
pub fn validate_token_burn_payload(payload: &[u8]) -> Result<(), TokenTxError> {
    let p: TokenBurnPayload = borsh::from_slice(payload).map_err(|_| TokenTxError::Decode)?;
    if p.version != TOKEN_PAYLOAD_VERSION_V1 {
        return Err(TokenTxError::UnsupportedVersion(p.version));
    }
    if p.owner_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(TokenTxError::InvalidPubKeyLen(p.owner_pubkey.len()));
    }
    if p.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(TokenTxError::InvalidSignatureLen(p.signature.len()));
    }
    if p.amount == 0 {
        return Err(TokenTxError::ZeroAmount);
    }
    Ok(())
}

/// Stateless validation of a [`TokenCreateMintPayload`]'s bytes (Phase B).
pub fn validate_token_create_mint_payload(payload: &[u8]) -> Result<(), TokenTxError> {
    let p: TokenCreateMintPayload = borsh::from_slice(payload).map_err(|_| TokenTxError::Decode)?;
    if p.version != TOKEN_PAYLOAD_VERSION_V1 {
        return Err(TokenTxError::UnsupportedVersion(p.version));
    }
    if p.creator_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(TokenTxError::InvalidPubKeyLen(p.creator_pubkey.len()));
    }
    if p.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(TokenTxError::InvalidSignatureLen(p.signature.len()));
    }
    if p.supply_cap == 0 {
        return Err(TokenTxError::ZeroSupplyCap);
    }
    Ok(())
}

/// Stateless validation of a [`TokenMintToPayload`]'s bytes (Phase B).
pub fn validate_token_mint_to_payload(payload: &[u8]) -> Result<(), TokenTxError> {
    let p: TokenMintToPayload = borsh::from_slice(payload).map_err(|_| TokenTxError::Decode)?;
    if p.version != TOKEN_PAYLOAD_VERSION_V1 {
        return Err(TokenTxError::UnsupportedVersion(p.version));
    }
    if p.asset_id == TOK_ASSET_ID {
        return Err(TokenTxError::TokMintForbidden);
    }
    if p.authority_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(TokenTxError::InvalidPubKeyLen(p.authority_pubkey.len()));
    }
    if p.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(TokenTxError::InvalidSignatureLen(p.signature.len()));
    }
    if p.amount == 0 {
        return Err(TokenTxError::ZeroAmount);
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Ledger semantics (design §4.2/§4.4) — pure application over explicit inputs.
// ---------------------------------------------------------------------

/// One `(asset_id, owner)` ledger row. An absent row reads as
/// `TokenAccount::default()` — there is no account-creation step (design §4.2);
/// the first credit materializes the row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct TokenAccount {
    /// Atomic units.
    pub balance: u128,
    /// Last applied nonce; the next payload must carry `nonce + 1`. Starts at 0
    /// for an absent row, so the first spend carries nonce 1.
    pub nonce: u64,
}

// The stores use an untracked (`Count`) cache policy, so this estimate is never
// consulted for eviction — an empty impl mirrors `VltEpochCredits`.
impl kaspa_utils::mem_size::MemSizeEstimator for TokenAccount {}

/// One asset's supply counters. The conservation invariant (design §4.2, MUST):
/// over the whole ledger, `Σ balance == minted − burned` — asserted by the
/// supply-conservation suite and, post-wiring, by store consistency checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct TokenSupply {
    /// Total atomic units ever created (for TOK: by emission settlement only).
    pub minted: u128,
    /// Total atomic units ever destroyed by burns.
    pub burned: u128,
}

impl kaspa_utils::mem_size::MemSizeEstimator for TokenSupply {}

impl TokenSupply {
    /// `minted − burned`. Well-formed stores never underflow (a burn debits an
    /// account whose credits are all inside `minted`); saturating keeps a
    /// corrupted read from panicking a diagnostic path.
    pub fn circulating(&self) -> u128 {
        self.minted.saturating_sub(self.burned)
    }
}

/// Stateful application failure for a token op. Wiring maps these to the
/// skip-class treatment (design §4.4): the carrier transaction stays valid, the
/// token effect is void — the same "invalid effects are ignored, not
/// consensus-fatal" stance the EVM lane takes for skippable txs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenOpError {
    /// Payload nonce is not `stored + 1`. Both replay (`got <= expected - 1`)
    /// and gap (`got > expected`) land here; only the exact successor applies.
    BadNonce { expected: u64, got: u64 },
    /// Debit exceeds the account balance.
    InsufficientBalance { balance: u128, amount: u128 },
    /// Credit would overflow the recipient's `u128` balance. Unreachable while
    /// total supply fits `u128`, kept as an explicit guard rather than a wrap.
    BalanceOverflow,
    /// The nonce counter itself would overflow (2^64 applied ops on one row).
    NonceOverflow,
    /// Phase B: transfer/burn/mint on an asset no accepted `CreateMint` has claimed.
    UnknownAsset { asset_id: u64 },
    /// Phase B: a `CreateMint` whose derived id an earlier create already claimed (first-wins).
    AssetExists { asset_id: u64 },
    /// Phase B: a `MintTo` signed by a key that is not the mint authority.
    NotMintAuthority,
    /// Phase B: a `MintTo` that would push `minted` past the immutable supply cap.
    SupplyCapExceeded { cap: u128, minted: u128, amount: u128 },
}

impl std::fmt::Display for TokenOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadNonce { expected, got } => write!(f, "bad token nonce: expected {expected}, got {got}"),
            Self::InsufficientBalance { balance, amount } => {
                write!(f, "insufficient token balance: have {balance}, debit {amount}")
            }
            Self::BalanceOverflow => write!(f, "token credit would overflow the recipient balance"),
            Self::NonceOverflow => write!(f, "token nonce counter exhausted"),
            Self::UnknownAsset { asset_id } => write!(f, "asset {asset_id} does not exist (no accepted create-mint)"),
            Self::AssetExists { asset_id } => write!(f, "asset {asset_id} already exists (create-mint is first-wins)"),
            Self::NotMintAuthority => write!(f, "signer is not this asset's mint authority"),
            Self::SupplyCapExceeded { cap, minted, amount } => {
                write!(f, "mint of {amount} would exceed the supply cap ({minted} of {cap} already minted)")
            }
        }
    }
}

impl std::error::Error for TokenOpError {}

/// Apply a transfer to the two accounts it touches (sender ≠ recipient — a
/// self-transfer never reaches here, [`TokenTxError::SelfTransfer`]).
///
/// Returns the updated `(from, to)` pair; the caller persists both rows in one
/// batch or neither. Pure: same inputs, same outputs, on every node and branch.
pub fn apply_token_transfer(
    from: TokenAccount,
    to: TokenAccount,
    amount: u128,
    nonce: u64,
) -> Result<(TokenAccount, TokenAccount), TokenOpError> {
    let expected = from.nonce.checked_add(1).ok_or(TokenOpError::NonceOverflow)?;
    if nonce != expected {
        return Err(TokenOpError::BadNonce { expected, got: nonce });
    }
    if amount > from.balance {
        return Err(TokenOpError::InsufficientBalance { balance: from.balance, amount });
    }
    let to_balance = to.balance.checked_add(amount).ok_or(TokenOpError::BalanceOverflow)?;
    Ok((TokenAccount { balance: from.balance - amount, nonce: expected }, TokenAccount { balance: to_balance, nonce: to.nonce }))
}

/// Apply a burn to the owner's account and the asset's supply counters.
pub fn apply_token_burn(
    owner: TokenAccount,
    supply: TokenSupply,
    amount: u128,
    nonce: u64,
) -> Result<(TokenAccount, TokenSupply), TokenOpError> {
    let expected = owner.nonce.checked_add(1).ok_or(TokenOpError::NonceOverflow)?;
    if nonce != expected {
        return Err(TokenOpError::BadNonce { expected, got: nonce });
    }
    if amount > owner.balance {
        return Err(TokenOpError::InsufficientBalance { balance: owner.balance, amount });
    }
    // `burned` cannot overflow before `minted` does: every burned unit was minted.
    Ok((
        TokenAccount { balance: owner.balance - amount, nonce: expected },
        TokenSupply { minted: supply.minted, burned: supply.burned.saturating_add(amount) },
    ))
}

/// Phase B: apply a `CreateMint` — bump the creator's TOK-line nonce and produce the
/// immutable policy row. `existing` is the ledger's current row for the derived id;
/// `Some` means an earlier create claimed it and this one is void (first-wins).
pub fn apply_token_create_mint(
    existing: Option<&TokenMintMeta>,
    creator_tok_account: TokenAccount,
    creator: Hash64,
    supply_cap: u128,
    decimals: u8,
    nonce: u64,
) -> Result<(TokenAccount, TokenMintMeta), TokenOpError> {
    let expected = creator_tok_account.nonce.checked_add(1).ok_or(TokenOpError::NonceOverflow)?;
    if nonce != expected {
        return Err(TokenOpError::BadNonce { expected, got: nonce });
    }
    if existing.is_some() {
        return Err(TokenOpError::AssetExists { asset_id: asset_id_for_mint(creator, nonce) });
    }
    Ok((
        TokenAccount { balance: creator_tok_account.balance, nonce: expected },
        TokenMintMeta { creator, mint_authority: creator, supply_cap, decimals },
    ))
}

/// Phase B: apply a `MintTo` **atomically** — authority, nonce, cap, credit, supply, in one
/// all-or-nothing step.
///
/// One function rather than a nonce step and a credit step, because a two-step form invites
/// the exact bug the devnet caught: a cap-breaching issuance whose nonce bump had already
/// been staged consumed a nonce it was not entitled to, and the next honest mint then failed
/// as a replay. A void op must consume nothing — the same invariant the Phase A overdraft
/// proves by leaving its nonce for the burn that follows it.
///
/// `to_account` is the recipient's staged row. When `to == authority` the caller passes the
/// authority's own row (they are the same row); the alias is resolved here, so the caller
/// never has to sequence two writes to one account.
#[allow(clippy::too_many_arguments)]
pub fn apply_token_mint_to(
    meta: &TokenMintMeta,
    authority: Hash64,
    authority_account: TokenAccount,
    to: Hash64,
    to_account: TokenAccount,
    supply: TokenSupply,
    amount: u128,
    nonce: u64,
) -> Result<(TokenAccount, TokenAccount, TokenSupply), TokenOpError> {
    if authority != meta.mint_authority {
        return Err(TokenOpError::NotMintAuthority);
    }
    let expected = authority_account.nonce.checked_add(1).ok_or(TokenOpError::NonceOverflow)?;
    if nonce != expected {
        return Err(TokenOpError::BadNonce { expected, got: nonce });
    }
    let minted = supply.minted.checked_add(amount).ok_or(TokenOpError::BalanceOverflow)?;
    if minted > meta.supply_cap {
        return Err(TokenOpError::SupplyCapExceeded { cap: meta.supply_cap, minted: supply.minted, amount });
    }
    let supply2 = TokenSupply { minted, burned: supply.burned };
    if to == authority {
        // Self-mint: one row carries both the nonce bump and the credit.
        let balance = authority_account.balance.checked_add(amount).ok_or(TokenOpError::BalanceOverflow)?;
        let merged = TokenAccount { balance, nonce: expected };
        return Ok((merged, merged, supply2));
    }
    let to_balance = to_account.balance.checked_add(amount).ok_or(TokenOpError::BalanceOverflow)?;
    Ok((
        TokenAccount { balance: authority_account.balance, nonce: expected },
        TokenAccount { balance: to_balance, nonce: to_account.nonce },
        supply2,
    ))
}

// ---------------------------------------------------------------------
// Emission (design §5).
// ---------------------------------------------------------------------

/// `⌊a·b/d⌋` with a 256-bit intermediate, so `budget · x_i` can never wrap
/// however large an epoch's µRTE credits grow. All-integer and branch-only —
/// two nodes computing a reward to different last bits would split the ledger.
///
/// Contract: `d > 0` (callers guard; `d == 0` returns 0 defensively) and
/// `b <= d` (every use is pro-rata with `x_i <= X`), which bounds the quotient
/// by `a` so it always fits `u128`.
fn mul_div_floor(a: u128, b: u128, d: u128) -> u128 {
    if d == 0 {
        return 0;
    }
    const LO: u128 = (1u128 << 64) - 1;
    // 256-bit product a·b as (hi, lo), schoolbook on 64-bit limbs.
    let (a_hi, a_lo) = (a >> 64, a & LO);
    let (b_hi, b_lo) = (b >> 64, b & LO);
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let (mid, mid_carry) = lh.overflowing_add(hl);
    let (lo, lo_carry) = ll.overflowing_add(mid << 64);
    let hi = hh + (mid >> 64) + ((mid_carry as u128) << 64) + (lo_carry as u128);

    // Restoring long division of the 256-bit (hi, lo) by d, one bit at a time.
    // The remainder before each shift is < d <= 2^128 − 1, so the shifted
    // 129-bit value is < 2d; a set carry bit therefore always means "subtract",
    // and the wrapping subtraction is exact.
    let mut rem: u128 = 0;
    let mut quo: u128 = 0;
    let mut quo_overflow = false;
    for i in (0..256).rev() {
        let bit = if i >= 128 { (hi >> (i - 128)) & 1 } else { (lo >> i) & 1 };
        let carry = rem >> 127;
        rem = (rem << 1) | bit;
        let subtract = carry == 1 || rem >= d;
        if subtract {
            rem = rem.wrapping_sub(d);
        }
        if i < 128 {
            quo = (quo << 1) | (subtract as u128);
        } else if subtract {
            // A quotient bit above 128 — only reachable when b > d, outside
            // the contract. Saturate rather than wrap.
            quo_overflow = true;
        }
    }
    if quo_overflow { u128::MAX } else { quo }
}

/// One executor's settled reward — the row emission settlement credits.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct TokenEmissionReward {
    /// The executor's overlay identity ([`validator_id_from_pubkey`]) — the
    /// TOK ledger owner credited (design §5.4).
    pub owner: Hash64,
    /// Atomic TOK.
    pub amount: u128,
}

/// The settled outcome of one epoch's emission — what the settlement store
/// records per epoch (idempotence marker + audit trail, design §9.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct TokenEmissionSettlement {
    /// `R(E)` — the epoch's scheduled budget (atomic TOK).
    pub budget: u128,
    /// `X(E)` — the epoch's total finalized µRTE credit the budget was divided by.
    pub network_compute: u128,
    /// `Σ reward_i` actually credited. `<= budget` always (floor rounding);
    /// the shortfall is *not* carried forward (design §5.1: supply never
    /// exceeds schedule).
    pub paid_total: u128,
    /// The rewards, sorted ascending by owner id (byte-deterministic encoding,
    /// mirroring [`VltEpochCredits`]).
    pub rewards: Vec<TokenEmissionReward>,
    /// Audit-emission v0.2: the portion of `paid_total` earned by counted verdicts
    /// (`Σ_i ⌊R·audit_i/W⌋` — exact by construction, see [`emission_rewards_v2`]).
    /// Zero on v0.1-style epochs (audit vec empty / pre-fence anchors).
    pub audit_paid: u128,
}

impl kaspa_utils::mem_size::MemSizeEstimator for TokenEmissionSettlement {}

/// Keyed-BLAKE2b-256 domain for [`TokenEmissionSettlement::digest`].
pub const TOKEN_SETTLEMENT_DIGEST_KEY: &[u8] = b"misaka-tkn-v1/settlement";

impl TokenEmissionSettlement {
    /// Deterministic digest of the whole settlement — keyed BLAKE2b-256 over the
    /// borsh encoding. Logged at settle time, so a verify harness can assert
    /// cross-node equality of the entire reward vector from the operator surface
    /// alone (the same role the frozen snapshots' `snapshot_root` plays for §5).
    pub fn digest(&self) -> Hash {
        let bytes = borsh::to_vec(self).expect("borsh serialization of a settlement is infallible");
        let mut hasher = Blake2bParams::new().hash_length(32).key(TOKEN_SETTLEMENT_DIGEST_KEY).to_state();
        hasher.update(&bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        Hash::from_bytes(out)
    }
}

/// `R(E)` — the halving-step emission schedule (design §5.2):
/// `R(E) = r0 >> ⌊(E − emission_activation_epoch) / H⌋`, and 0 before
/// activation or on an inert preset.
pub fn emission_epoch_budget(params: &TokenParams, epoch: u64) -> u128 {
    if params.emission_epoch_budget_r0_atomic == 0 || params.emission_halving_epochs == 0 || epoch < params.emission_activation_epoch {
        return 0;
    }
    let halvings = (epoch - params.emission_activation_epoch) / params.emission_halving_epochs;
    if halvings >= 128 {
        return 0;
    }
    params.emission_epoch_budget_r0_atomic >> halvings
}

/// The emission function (design §5.1):
/// `reward_i(E) = ⌊ budget · X_i(E) / X(E) ⌋`, over an epoch's **finalized**
/// credits, with the whole epoch skipped (no carry) when
/// `X(E) < min_network_compute` — the design's guard against minting the whole
/// budget to whoever shows up on a near-empty network.
///
/// The credit vector arrives sorted by validator id ([`VltEpochCredits`]
/// canonicalizes), so the reward vector is byte-deterministic. Zero-credit and
/// zero-reward (floor) entries are omitted.
pub fn emission_rewards(budget: u128, credits: &VltEpochCredits, min_network_compute: u128) -> TokenEmissionSettlement {
    let network_compute = credits.credits.iter().fold(0u128, |acc, (_, x)| acc.saturating_add(*x));
    let mut settlement = TokenEmissionSettlement { budget, network_compute, paid_total: 0, rewards: Vec::new(), audit_paid: 0 };
    if budget == 0 || network_compute == 0 || network_compute < min_network_compute {
        return settlement;
    }
    for (owner, x) in credits.credits.iter() {
        if *x == 0 {
            continue;
        }
        let amount = mul_div_floor(budget, *x, network_compute);
        if amount == 0 {
            continue;
        }
        settlement.paid_total = settlement.paid_total.saturating_add(amount);
        settlement.rewards.push(TokenEmissionReward { owner: *owner, amount });
    }
    debug_assert!(settlement.paid_total <= budget, "pro-rata floors can never exceed the budget");
    settlement
}

/// Audit-emission v0.2 (design §2.1): one budget, one measure — execution and counted-verdict
/// replay divide `R(E)` pro-rata over combined work `W = Σ exec + Σ audit`.
///
/// Each validator's reward is paid as **two floor terms**, `⌊R·exec_i/W⌋ + ⌊R·audit_i/W⌋`,
/// rather than one floor over the sum: the two differ by at most one atomic unit per validator,
/// and the two-term form is what makes `audit_paid` an exact ledger quantity instead of an
/// estimate (v0.2 §2.1 implementation note). The `min_network_compute` floor is judged against
/// **executor** work alone — verification accompanies execution, and letting replays help clear
/// the vacuous-network floor would count one physical job `1 + committee` times.
pub fn emission_rewards_v2(
    budget: u128,
    exec: &[(Hash64, u128)],
    audit: &[(Hash64, u128)],
    min_network_compute: u128,
) -> TokenEmissionSettlement {
    let exec_compute = exec.iter().fold(0u128, |acc, (_, x)| acc.saturating_add(*x));
    let audit_compute = audit.iter().fold(0u128, |acc, (_, x)| acc.saturating_add(*x));
    let total_work = exec_compute.saturating_add(audit_compute);
    let mut settlement =
        TokenEmissionSettlement { budget, network_compute: exec_compute, paid_total: 0, rewards: Vec::new(), audit_paid: 0 };
    if budget == 0 || exec_compute == 0 || exec_compute < min_network_compute {
        return settlement;
    }
    let mut per_owner: BTreeMap<Hash64, (u128, u128)> = BTreeMap::new();
    for (owner, x) in exec.iter().filter(|(_, x)| *x > 0) {
        per_owner.entry(*owner).or_default().0 = per_owner.get(owner).map(|v| v.0).unwrap_or(0).saturating_add(*x);
    }
    for (owner, x) in audit.iter().filter(|(_, x)| *x > 0) {
        per_owner.entry(*owner).or_default().1 = per_owner.get(owner).map(|v| v.1).unwrap_or(0).saturating_add(*x);
    }
    for (owner, (exec_i, audit_i)) in per_owner {
        let exec_pay = mul_div_floor(budget, exec_i, total_work);
        let audit_pay = mul_div_floor(budget, audit_i, total_work);
        let amount = exec_pay.saturating_add(audit_pay);
        if amount == 0 {
            continue;
        }
        settlement.paid_total = settlement.paid_total.saturating_add(amount);
        settlement.audit_paid = settlement.audit_paid.saturating_add(audit_pay);
        settlement.rewards.push(TokenEmissionReward { owner, amount });
    }
    debug_assert!(settlement.paid_total <= budget, "per-term floors can never exceed the budget");
    settlement
}

/// The `getTokenEmissionInfo` read DTO (design §9.3): one epoch's settlement
/// view plus the two live cursors — the ops gauges that let a harness or a
/// monitor tell "settling normally" from "stalled" without log access.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenEmissionInfo {
    /// The epoch this view describes.
    pub epoch: u64,
    /// Whether that epoch has a settlement row (false ⇒ the numeric fields are
    /// zero — not yet settled, or skipped as pre-program history).
    pub settled: bool,
    pub budget: u128,
    pub network_compute: u128,
    pub paid_total: u128,
    /// Audit-emission v0.2: the counted-verdict share of `paid_total`.
    pub audit_paid: u128,
    pub reward_count: u32,
    /// [`TokenEmissionSettlement::digest`] of the row (zero hash when unsettled).
    pub settlement_root: Hash,
    /// The settlement cursor: the next epoch settlement will consider.
    pub next_settlement_epoch: u64,
    /// The ledger fold cursor: the next selected-chain index the fold processes.
    pub fold_cursor: u64,
}

// ---------------------------------------------------------------------
// Params (design §10). Unwired in Phase A PR 1: the `DnsParams`/preset field
// lands with the processor seam so this PR changes no network's behavior.
// ---------------------------------------------------------------------

/// Per-network Token Program parameters. `INERT` (all fences `u64::MAX`, zero
/// budget) is the shipped default on every network — adopting this module is
/// not by itself a consensus change; moving a fence is the hard fork.
///
/// Borsh-derived because [`crate::dns_finality::DnsParams`] (which embeds this,
/// appended last like `vlt` before it) rides the ADR-0022 overlay snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenParams {
    /// Below this DAA score the ledger does not exist at all. In `[shadow,
    /// active)` the node computes and logs ledger/emission effects without
    /// persisting a row a consensus rule can read (design §10 shadow phase).
    pub tkn_shadow_activation_daa_score: u64,
    /// At/above this DAA score accepted token ops mutate the ledger and
    /// settled emission credits it — the hard-forking fence. Must sit at or
    /// above the VLT weight fence (`tkn_activation >= vlt_activation`,
    /// design §10): a token program on a network whose compute overlay is
    /// inert would define emission over credits that cannot exist.
    pub tkn_activation_daa_score: u64,
    /// `E_a` — the epoch the halving clock starts from (design §5.2).
    pub emission_activation_epoch: u64,
    /// `R0` in atomic TOK per epoch. **TBD by explicit decision** (design §12
    /// #2, confirmed 2026-08-10): every shipped preset carries 0 until the
    /// testnet shadow phase freezes real numbers. `0` = no emission (a
    /// ledger-only activation is legal staging — transfers/burns without
    /// issuance).
    pub emission_epoch_budget_r0_atomic: u128,
    /// `H` in epochs. TBD alongside `R0`.
    pub emission_halving_epochs: u64,
    /// `D_settle` — how many epochs behind the live edge settlement runs.
    /// [`Self::is_coherent_with_vlt`] pins it above the credit-finalization
    /// burial depth, which is what makes reading the finalized credit store
    /// sufficient (design §5.3).
    pub settlement_delay_epochs: u32,
    /// Epochs with `X(E)` below this settle to zero rewards, no carry
    /// (design §5.1). Same unit (µRTE) and same rationale as
    /// `VltParams::min_network_compute` — a separate knob because emission may
    /// want a higher floor than quorum legitimacy does.
    pub emission_min_network_compute: u128,
    /// Phase B (design §4.6): at/above this DAA score, accepted `CreateMint` /
    /// `MintTo` ops bind — permissionless mints exist. Independent of (and never
    /// below) the Phase A fence; `u64::MAX` on every shipped preset.
    pub tkn_phase_b_activation_daa_score: u64,
}

impl TokenParams {
    /// The shipped default everywhere: no ledger, no emission, forever, until
    /// a per-network hard fork says otherwise.
    pub const INERT: Self = Self {
        tkn_shadow_activation_daa_score: u64::MAX,
        tkn_activation_daa_score: u64::MAX,
        emission_activation_epoch: u64::MAX,
        emission_epoch_budget_r0_atomic: 0,
        emission_halving_epochs: 0,
        settlement_delay_epochs: 0,
        emission_min_network_compute: 0,
        tkn_phase_b_activation_daa_score: u64::MAX,
    };

    /// Whether the ledger machinery runs (shadow or live) at `daa_score`.
    pub fn shadow_active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.tkn_shadow_activation_daa_score
    }

    /// Whether accepted token ops and settled emission actually bind at
    /// `daa_score`.
    pub fn active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.tkn_activation_daa_score
    }

    /// Phase B: whether permissionless mints bind at `daa_score`.
    pub fn phase_b_active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.tkn_phase_b_activation_daa_score
    }

    /// Internal-consistency check for a preset — a startup/test assertion in
    /// the [`crate::vlt::VltParams::is_coherent`] mold, not a consensus rule.
    pub fn is_coherent(&self) -> Result<(), &'static str> {
        if self.tkn_shadow_activation_daa_score > self.tkn_activation_daa_score {
            return Err("tkn_shadow_activation_daa_score must be <= tkn_activation_daa_score");
        }
        if self.emission_epoch_budget_r0_atomic > 0 {
            if self.emission_halving_epochs == 0 {
                return Err("emission_halving_epochs must be >= 1 when R0 > 0 (H = 0 has no schedule)");
            }
            if self.settlement_delay_epochs == 0 {
                return Err("settlement_delay_epochs must be >= 1 when R0 > 0 (settling the live epoch mints on a fork)");
            }
            if self.emission_activation_epoch == u64::MAX {
                return Err(
                    "emission_activation_epoch must be set when R0 > 0 (a budget that never starts is a misconfiguration, not a policy)",
                );
            }
            if self.emission_min_network_compute == 0 {
                return Err(
                    "emission_min_network_compute must be > 0 when R0 > 0 (design §5.1: no whole-budget mint on a near-empty network)",
                );
            }
        }
        if self.tkn_phase_b_activation_daa_score < self.tkn_activation_daa_score {
            return Err("tkn_phase_b_activation_daa_score must be >= tkn_activation_daa_score (mints need a live ledger)");
        }
        Ok(())
    }

    /// The `D_settle` floor against the credit-finalization depth (design §5.3):
    /// settlement may only read epochs [`crate::vlt::vlt_epoch_finalized`]
    /// accepts, so the delay must cover the challenge window plus the reorg
    /// horizon (rounded up to whole epochs, plus one for the partial epoch in
    /// flight), and never sit below the vote-side `credit_delay_epochs`.
    pub fn min_settlement_delay_epochs(
        challenge_window_blocks: u64,
        max_reorg_horizon_blocks: u64,
        epoch_length_blocks: u64,
        credit_delay_epochs: u32,
    ) -> u32 {
        if epoch_length_blocks == 0 {
            return u32::MAX;
        }
        let burial_blocks = challenge_window_blocks.saturating_add(max_reorg_horizon_blocks);
        let burial_epochs = burial_blocks.div_ceil(epoch_length_blocks).saturating_add(1);
        let burial_epochs = u32::try_from(burial_epochs).unwrap_or(u32::MAX);
        burial_epochs.max(credit_delay_epochs)
    }

    /// Cross-check against the VLT preset this token program would settle over.
    /// Callers pass the same `(challenge_window, reorg_horizon, epoch_length)`
    /// the credit accumulator finalizes under.
    pub fn is_coherent_with_vlt(
        &self,
        vlt_shadow_activation_daa_score: u64,
        challenge_window_blocks: u64,
        max_reorg_horizon_blocks: u64,
        epoch_length_blocks: u64,
        credit_delay_epochs: u32,
    ) -> Result<(), &'static str> {
        self.is_coherent()?;
        // The SHADOW fence, deliberately: the credit accumulator runs (and finalizes
        // epochs) from the shadow fence, and settlement reads only those finalized
        // rows. Whether compute also WEIGHTS votes is irrelevant to money — pinning
        // emission to the weight fence would couple the ledger to the §6 activation
        // state machine, which is exactly the dependency the 2026-08-10 devnet run
        // showed stalls it (design §10, revised).
        if self.tkn_activation_daa_score < vlt_shadow_activation_daa_score {
            return Err(
                "tkn_activation_daa_score must be >= vlt_shadow_activation_daa_score (design §10: no token program on an inert compute overlay)",
            );
        }
        if self.emission_epoch_budget_r0_atomic > 0 {
            let floor = Self::min_settlement_delay_epochs(
                challenge_window_blocks,
                max_reorg_horizon_blocks,
                epoch_length_blocks,
                credit_delay_epochs,
            );
            if self.settlement_delay_epochs < floor {
                return Err(
                    "settlement_delay_epochs is below the credit-finalization depth (design §5.3: settlement must read only finalized epochs)",
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn transfer_payload() -> TokenTransferPayload {
        TokenTransferPayload {
            version: TOKEN_PAYLOAD_VERSION_V1,
            asset_id: TOK_ASSET_ID,
            from_pubkey: vec![0x11; STAKE_VALIDATOR_PUBKEY_LEN],
            to: id(0x22),
            amount: 1_000,
            nonce: 1,
            signature: vec![0x33; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn burn_payload() -> TokenBurnPayload {
        TokenBurnPayload {
            version: TOKEN_PAYLOAD_VERSION_V1,
            asset_id: TOK_ASSET_ID,
            owner_pubkey: vec![0x11; STAKE_VALIDATOR_PUBKEY_LEN],
            amount: 500,
            nonce: 1,
            signature: vec![0x33; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    // ---- messages -------------------------------------------------------

    /// The two role domains must never collide: a burn digest over the same
    /// fields is not a transfer digest.
    #[test]
    fn transfer_and_burn_messages_are_domain_separated() {
        let t = token_transfer_message(b"net", TOK_ASSET_ID, id(1), id(2), 5, 1);
        let b = token_burn_message(b"net", TOK_ASSET_ID, id(1), 5, 1);
        assert_ne!(t.as_bytes(), b.as_bytes());
    }

    /// Every signed field must move the digest — a digest indifferent to a
    /// field is a field an attacker may rewrite under a stolen signature.
    #[test]
    fn transfer_message_binds_every_field() {
        let base = token_transfer_message(b"net", 0, id(1), id(2), 5, 1);
        assert_ne!(base, token_transfer_message(b"other", 0, id(1), id(2), 5, 1), "network");
        assert_ne!(base, token_transfer_message(b"net", 1, id(1), id(2), 5, 1), "asset");
        assert_ne!(base, token_transfer_message(b"net", 0, id(9), id(2), 5, 1), "from");
        assert_ne!(base, token_transfer_message(b"net", 0, id(1), id(9), 5, 1), "to");
        assert_ne!(base, token_transfer_message(b"net", 0, id(1), id(2), 6, 1), "amount");
        assert_ne!(base, token_transfer_message(b"net", 0, id(1), id(2), 5, 2), "nonce");
    }

    // ---- stateless validation ------------------------------------------

    #[test]
    fn valid_payloads_pass_stateless_validation() {
        validate_token_transfer_payload(&borsh::to_vec(&transfer_payload()).unwrap()).unwrap();
        validate_token_burn_payload(&borsh::to_vec(&burn_payload()).unwrap()).unwrap();
    }

    #[test]
    fn stateless_validation_rejects_each_bad_shape() {
        let mut p = transfer_payload();
        p.version = 2;
        assert_eq!(validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::UnsupportedVersion(2)));

        // Phase B relaxed admission: any asset id passes statelessly (an absent mint
        // voids statefully) — asset 7 is now a valid SHAPE.
        let mut p = transfer_payload();
        p.asset_id = 7;
        validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()).unwrap();

        let mut p = transfer_payload();
        p.from_pubkey = vec![0; 32];
        assert_eq!(validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::InvalidPubKeyLen(32)));

        let mut p = transfer_payload();
        p.signature = vec![0; 64];
        assert_eq!(validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::InvalidSignatureLen(64)));

        let mut p = transfer_payload();
        p.amount = 0;
        assert_eq!(validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::ZeroAmount));

        assert_eq!(validate_token_transfer_payload(b"junk"), Err(TokenTxError::Decode));
    }

    /// A transfer whose recipient is the sender's own id is rejected at the
    /// door — it moves nothing, and admitting it would force the application
    /// seam to alias one account as both debit and credit.
    #[test]
    fn self_transfer_is_rejected_statelessly() {
        let mut p = transfer_payload();
        p.to = validator_id_from_pubkey(&p.from_pubkey);
        assert_eq!(validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::SelfTransfer));
    }

    // ---- ledger application --------------------------------------------

    #[test]
    fn transfer_moves_balance_and_advances_only_the_sender_nonce() {
        let from = TokenAccount { balance: 1_000, nonce: 4 };
        let to = TokenAccount { balance: 50, nonce: 9 };
        let (from2, to2) = apply_token_transfer(from, to, 300, 5).unwrap();
        assert_eq!(from2, TokenAccount { balance: 700, nonce: 5 });
        assert_eq!(to2, TokenAccount { balance: 350, nonce: 9 }, "a credit must not consume the recipient's nonce");
        // Conservation across the pair.
        assert_eq!(from.balance + to.balance, from2.balance + to2.balance);
    }

    /// Only the exact successor nonce applies: a replay (`<=`) and a gap (`>`)
    /// are both void, so one signed payload can apply at most once, in order.
    #[test]
    fn transfer_nonce_must_be_the_exact_successor() {
        let from = TokenAccount { balance: 1_000, nonce: 4 };
        let to = TokenAccount::default();
        assert_eq!(apply_token_transfer(from, to, 1, 4), Err(TokenOpError::BadNonce { expected: 5, got: 4 }));
        assert_eq!(apply_token_transfer(from, to, 1, 6), Err(TokenOpError::BadNonce { expected: 5, got: 6 }));
    }

    #[test]
    fn transfer_rejects_overdraft_and_credit_overflow() {
        let from = TokenAccount { balance: 10, nonce: 0 };
        assert_eq!(
            apply_token_transfer(from, TokenAccount::default(), 11, 1),
            Err(TokenOpError::InsufficientBalance { balance: 10, amount: 11 })
        );
        let rich_recipient = TokenAccount { balance: u128::MAX, nonce: 0 };
        assert_eq!(apply_token_transfer(from, rich_recipient, 1, 1), Err(TokenOpError::BalanceOverflow));
    }

    #[test]
    fn burn_debits_owner_and_raises_burned_supply() {
        let owner = TokenAccount { balance: 800, nonce: 2 };
        let supply = TokenSupply { minted: 1_000, burned: 100 };
        let (owner2, supply2) = apply_token_burn(owner, supply, 300, 3).unwrap();
        assert_eq!(owner2, TokenAccount { balance: 500, nonce: 3 });
        assert_eq!(supply2, TokenSupply { minted: 1_000, burned: 400 });
        assert_eq!(supply2.circulating(), 600);
    }

    /// The §4.2 conservation invariant over a scripted history: settle → spend
    /// → burn, checking `Σ balance == minted − burned` after every step.
    #[test]
    fn supply_conservation_holds_across_a_scripted_history() {
        let credits = VltEpochCredits::from_unordered([(id(1), 700u128), (id(2), 300u128)]);
        let settled = emission_rewards(1_000, &credits, 1);
        let mut a = TokenAccount { balance: settled.rewards[0].amount, nonce: 0 };
        let mut b = TokenAccount { balance: settled.rewards[1].amount, nonce: 0 };
        let mut supply = TokenSupply { minted: settled.paid_total, burned: 0 };
        let conserved = |a: &TokenAccount, b: &TokenAccount, s: &TokenSupply| {
            assert_eq!(a.balance + b.balance, s.minted - s.burned);
        };
        conserved(&a, &b, &supply);

        let (a2, b2) = apply_token_transfer(a, b, 250, 1).unwrap();
        (a, b) = (a2, b2);
        conserved(&a, &b, &supply);

        let (b3, s2) = apply_token_burn(b, supply, 400, 1).unwrap();
        (b, supply) = (b3, s2);
        conserved(&a, &b, &supply);
    }

    // ---- emission -------------------------------------------------------

    #[test]
    fn mul_div_floor_matches_naive_in_range_and_survives_wide_products() {
        assert_eq!(mul_div_floor(1_000, 700, 1_000), 700);
        assert_eq!(mul_div_floor(1_000, 1, 3), 333);
        assert_eq!(mul_div_floor(0, 5, 7), 0);
        assert_eq!(mul_div_floor(5, 0, 7), 0);
        assert_eq!(mul_div_floor(u128::MAX, u128::MAX, u128::MAX), u128::MAX);
    }

    /// The wide case: `2^127 · 6` needs 130 bits — a bare u128 multiply would
    /// wrap — and `⌊2^127 · 6 / 8⌋ = 3 · 2^125` exactly.
    #[test]
    fn mul_div_floor_wide_product_exact_value() {
        assert_eq!(mul_div_floor(1u128 << 127, 6, 8), 3u128 << 125);
        assert_eq!(mul_div_floor(u128::MAX, u128::MAX - 1, u128::MAX), u128::MAX - 1);
    }

    #[test]
    fn emission_budget_follows_the_halving_steps_and_the_fences() {
        let mut p = TokenParams::INERT;
        assert_eq!(emission_epoch_budget(&p, 0), 0, "inert preset never emits");
        p.emission_activation_epoch = 100;
        p.emission_epoch_budget_r0_atomic = 800;
        p.emission_halving_epochs = 10;
        assert_eq!(emission_epoch_budget(&p, 99), 0, "pre-activation");
        assert_eq!(emission_epoch_budget(&p, 100), 800);
        assert_eq!(emission_epoch_budget(&p, 109), 800);
        assert_eq!(emission_epoch_budget(&p, 110), 400, "first halving");
        assert_eq!(emission_epoch_budget(&p, 130), 100);
        assert_eq!(emission_epoch_budget(&p, 100 + 10 * 128), 0, "fully decayed");
    }

    #[test]
    fn emission_rewards_are_pro_rata_floored_and_never_exceed_budget() {
        let credits = VltEpochCredits::from_unordered([(id(3), 1u128), (id(1), 700u128), (id(2), 300u128)]);
        let s = emission_rewards(1_000, &credits, 1);
        assert_eq!(s.network_compute, 1_001);
        // Sorted by owner id, floor division: 700/1001 and 300/1001 shares.
        // id(3)'s 1-µRTE share floors to zero and a zero reward is omitted.
        assert_eq!(s.rewards.len(), 2);
        assert_eq!(s.rewards[0].owner, id(1));
        assert_eq!(s.rewards[0].amount, 1_000 * 700 / 1_001);
        assert_eq!(s.rewards[1].owner, id(2));
        assert_eq!(s.rewards[1].amount, 1_000 * 300 / 1_001);
        assert!(s.paid_total <= s.budget);
        assert_eq!(s.paid_total, s.rewards.iter().map(|r| r.amount).sum::<u128>());
    }

    /// A single whale on a near-dead network must not collect the whole budget:
    /// below the compute floor the epoch settles to nothing, with no carry.
    #[test]
    fn emission_skips_epochs_below_the_network_compute_floor() {
        let credits = VltEpochCredits::from_unordered([(id(1), 500u128)]);
        let s = emission_rewards(1_000, &credits, 1_000);
        assert_eq!(s.network_compute, 500);
        assert!(s.rewards.is_empty());
        assert_eq!(s.paid_total, 0);
        // And an empty epoch settles to an empty record, not a division by zero.
        let empty = emission_rewards(1_000, &VltEpochCredits::default(), 0);
        assert!(empty.rewards.is_empty());
    }

    /// Splitting one identity's compute across sybils must not change the
    /// total paid (linearity — design §8.7): the same X in one row or three
    /// rows pays the same total, up to per-row flooring, and the flooring loss
    /// is strictly less than one atomic unit per recipient.
    #[test]
    fn emission_is_sybil_neutral_up_to_flooring() {
        let thirds = VltEpochCredits::from_unordered([(id(1), 300u128), (id(2), 300u128), (id(3), 300u128)]);
        // A budget the thirds divide exactly: split == whole to the unit.
        let whole = emission_rewards(9_000, &VltEpochCredits::from_unordered([(id(1), 900u128)]), 1);
        let split = emission_rewards(9_000, &thirds, 1);
        assert_eq!(whole.paid_total, 9_000);
        assert_eq!(split.paid_total, 9_000);
        // A budget they do not: the split may only lose to flooring, and by
        // less than one unit per row (10_000/3 floors to 3_333 × 3 = 9_999).
        let whole = emission_rewards(10_000, &VltEpochCredits::from_unordered([(id(1), 900u128)]), 1);
        let split = emission_rewards(10_000, &thirds, 1);
        assert_eq!(whole.paid_total, 10_000);
        assert_eq!(split.paid_total, 9_999);
        assert!(whole.paid_total - split.paid_total < split.rewards.len() as u128);
    }

    // ---- Phase B: permissionless mints ----------------------------------

    /// The id is a pure function of (creator, nonce) — re-carrying a payload
    /// re-claims the same id — never TOK's, and creator-scoped.
    #[test]
    fn asset_id_is_deterministic_creator_scoped_and_never_tok() {
        assert_eq!(asset_id_for_mint(id(1), 4), asset_id_for_mint(id(1), 4));
        assert_ne!(asset_id_for_mint(id(1), 4), asset_id_for_mint(id(1), 5));
        assert_ne!(asset_id_for_mint(id(1), 4), asset_id_for_mint(id(2), 4));
        assert_ne!(asset_id_for_mint(id(1), 4), TOK_ASSET_ID);
    }

    #[test]
    fn create_mint_consumes_the_tok_nonce_line_and_is_first_wins() {
        let creator_tok = TokenAccount { balance: 77, nonce: 3 };
        let (creator2, meta) = apply_token_create_mint(None, creator_tok, id(1), 5_000_000, 8, 4).unwrap();
        assert_eq!(creator2, TokenAccount { balance: 77, nonce: 4 }, "balance untouched, TOK nonce consumed");
        assert_eq!(meta.mint_authority, id(1));
        // Wrong nonce: void without consuming anything.
        assert!(matches!(
            apply_token_create_mint(None, creator_tok, id(1), 1, 0, 9),
            Err(TokenOpError::BadNonce { expected: 4, got: 9 })
        ));
        // First-wins: an existing meta voids the claim.
        assert!(matches!(apply_token_create_mint(Some(&meta), creator_tok, id(1), 1, 0, 4), Err(TokenOpError::AssetExists { .. })));
    }

    #[test]
    fn mint_to_enforces_authority_cap_and_pays_exactly_to_the_cap() {
        let meta = TokenMintMeta { creator: id(1), mint_authority: id(1), supply_cap: 5_000_000, decimals: 8 };
        let auth = TokenAccount::default();
        // Authority check.
        assert!(matches!(
            apply_token_mint_to(&meta, id(2), auth, id(3), TokenAccount::default(), TokenSupply::default(), 1, 1),
            Err(TokenOpError::NotMintAuthority)
        ));
        // 3M to a third party.
        let (auth1, to1, supply1) =
            apply_token_mint_to(&meta, id(1), auth, id(3), TokenAccount::default(), TokenSupply::default(), 3_000_000, 1).unwrap();
        assert_eq!(auth1, TokenAccount { balance: 0, nonce: 1 });
        assert_eq!(to1.balance, 3_000_000);
        assert_eq!(supply1.minted, 3_000_000);
        // The cap is reachable exactly, and a self-mint merges the bump and the credit.
        let (auth2, to2, supply2) = apply_token_mint_to(&meta, id(1), auth1, id(1), auth1, supply1, 2_000_000, 2).unwrap();
        assert_eq!(auth2, to2, "a self-mint is one row");
        assert_eq!(auth2, TokenAccount { balance: 2_000_000, nonce: 2 });
        assert_eq!(supply2.minted, 5_000_000, "the cap itself is reachable");
        // Conservation for the minted asset.
        assert_eq!(to1.balance + auth2.balance, supply2.minted - supply2.burned);
    }

    /// The devnet caught this one: a cap-breaching mint whose nonce bump had already been
    /// staged consumed a nonce it never earned, and the next honest mint failed as a replay.
    /// A void op consumes NOTHING — the same invariant the Phase A overdraft proves by
    /// leaving its nonce for the burn behind it.
    #[test]
    fn a_cap_breaching_mint_consumes_no_nonce() {
        let meta = TokenMintMeta { creator: id(1), mint_authority: id(1), supply_cap: 5_000_000, decimals: 8 };
        let (auth1, _to1, supply1) = apply_token_mint_to(
            &meta,
            id(1),
            TokenAccount::default(),
            id(3),
            TokenAccount::default(),
            TokenSupply::default(),
            3_000_000,
            1,
        )
        .unwrap();
        // The breach: nonce 2 is the correct successor, but the cap refuses it.
        assert!(matches!(
            apply_token_mint_to(&meta, id(1), auth1, id(3), _to1, supply1, 2_000_001, 2),
            Err(TokenOpError::SupplyCapExceeded { .. })
        ));
        // Nonce 2 is therefore still unspent, and the honest mint that follows uses it.
        let (auth2, _to2, supply2) = apply_token_mint_to(&meta, id(1), auth1, id(3), _to1, supply1, 2_000_000, 2).unwrap();
        assert_eq!(auth2.nonce, 2);
        assert_eq!(supply2.minted, 5_000_000);
    }

    /// The absolute rule a payload can violate on its face: TOK cannot be minted.
    #[test]
    fn tok_mint_to_is_rejected_statelessly() {
        let p = TokenMintToPayload {
            version: TOKEN_PAYLOAD_VERSION_V1,
            asset_id: TOK_ASSET_ID,
            authority_pubkey: vec![0x11; STAKE_VALIDATOR_PUBKEY_LEN],
            to: id(2),
            amount: 1,
            nonce: 1,
            signature: vec![0x33; STAKE_ATTESTATION_SIG_LEN],
        };
        assert_eq!(validate_token_mint_to_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::TokMintForbidden));
        // And a zero-cap mint is a misconfiguration, not a policy.
        let c = TokenCreateMintPayload {
            version: TOKEN_PAYLOAD_VERSION_V1,
            creator_pubkey: vec![0x11; STAKE_VALIDATOR_PUBKEY_LEN],
            supply_cap: 0,
            decimals: 8,
            nonce: 1,
            signature: vec![0x33; STAKE_ATTESTATION_SIG_LEN],
        };
        assert_eq!(validate_token_create_mint_payload(&borsh::to_vec(&c).unwrap()), Err(TokenTxError::ZeroSupplyCap));
        // Phase B admission relax: a transfer naming a non-TOK asset now passes statelessly
        // (statefully it voids until its mint exists).
        let mut t = transfer_payload();
        t.asset_id = 7;
        validate_token_transfer_payload(&borsh::to_vec(&t).unwrap()).unwrap();
    }

    #[test]
    fn phase_b_fence_is_coherent_only_at_or_above_phase_a() {
        let mut p = TokenParams::INERT;
        p.is_coherent().unwrap();
        p.tkn_shadow_activation_daa_score = 1_000;
        p.tkn_activation_daa_score = 2_000;
        p.tkn_phase_b_activation_daa_score = 1_500;
        assert!(p.is_coherent().is_err(), "mints below the ledger fence");
        p.tkn_phase_b_activation_daa_score = 2_000;
        p.is_coherent().unwrap();
    }

    // ---- emission v0.2 (unified work) -----------------------------------

    /// One µRTE pays the same whether it executed or replayed: equal exec and
    /// audit work earn equal TOK, and `audit_paid` accounts the audit share
    /// exactly (two-term floors — design v0.2 §2.1).
    #[test]
    fn v2_pays_execution_and_audit_from_one_pie() {
        let s = emission_rewards_v2(900, &[(id(1), 300)], &[(id(2), 300), (id(1), 300)], 1);
        assert_eq!(s.network_compute, 300, "network_compute stays the EXEC measure");
        assert_eq!(s.rewards.len(), 2);
        assert_eq!(s.rewards[0].owner, id(1));
        assert_eq!(s.rewards[0].amount, 600, "300 exec + 300 audit of W=900 → 2/3 of 900");
        assert_eq!(s.rewards[1].amount, 300);
        assert_eq!(s.paid_total, 900);
        assert_eq!(s.audit_paid, 600, "a's 300 + b's 300 audit work");
    }

    /// The vacuous-network floor is judged on executor work alone: replays
    /// accompany execution and must not help clear it (×(1+c) double-count).
    #[test]
    fn v2_floor_ignores_audit_work() {
        let starved = emission_rewards_v2(1_000, &[(id(1), 40)], &[(id(2), 1_000_000)], 50);
        assert!(starved.rewards.is_empty(), "exec 40 < floor 50, however large the audit side");
        let ok = emission_rewards_v2(1_000, &[(id(1), 50)], &[(id(2), 1_000_000)], 50);
        assert!(!ok.rewards.is_empty());
        // The documented edge: an all-refuted epoch (exec 0) settles to nothing,
        // its refuters included.
        let refuted_only = emission_rewards_v2(1_000, &[], &[(id(2), 500)], 1);
        assert!(refuted_only.rewards.is_empty());
    }

    #[test]
    fn v2_accounting_is_exact_under_flooring() {
        let s = emission_rewards_v2(1_000, &[(id(1), 700)], &[(id(2), 300), (id(3), 1)], 1);
        // W = 1001; per-term floors: 1000·700/1001=699, 1000·300/1001=299, 1000·1/1001=0.
        assert_eq!(s.paid_total, 699 + 299);
        assert_eq!(s.audit_paid, 299);
        assert!(s.paid_total <= s.budget);
        assert_eq!(s.paid_total, s.rewards.iter().map(|r| r.amount).sum::<u128>());
    }

    /// v1 rows (empty audit) settle identically through v2 — the upgrade path
    /// for pre-fence epochs.
    #[test]
    fn v2_with_empty_audit_matches_v1() {
        let credits = VltEpochCredits::from_unordered([(id(1), 700u128), (id(2), 300u128)]);
        let v1 = emission_rewards(1_000, &credits, 1);
        let v2 = emission_rewards_v2(1_000, &credits.credits, &[], 1);
        assert_eq!(v1.rewards, v2.rewards);
        assert_eq!(v2.audit_paid, 0);
    }

    // ---- params ---------------------------------------------------------

    #[test]
    fn inert_params_are_coherent_and_inactive() {
        TokenParams::INERT.is_coherent().unwrap();
        assert!(!TokenParams::INERT.shadow_active_at(u64::MAX - 1));
        assert!(!TokenParams::INERT.active_at(u64::MAX - 1));
    }

    #[test]
    fn coherence_rejects_each_emission_misconfiguration() {
        let live = TokenParams {
            tkn_shadow_activation_daa_score: 1_000,
            tkn_activation_daa_score: 2_000,
            emission_activation_epoch: 10,
            emission_epoch_budget_r0_atomic: 500 * TOK_ATOMIC_PER_UNIT as u128,
            emission_halving_epochs: 315_360,
            settlement_delay_epochs: 8,
            emission_min_network_compute: 100_000_000_000,
            tkn_phase_b_activation_daa_score: u64::MAX,
        };
        live.is_coherent().unwrap();

        let mut p = live;
        p.tkn_shadow_activation_daa_score = 3_000;
        assert!(p.is_coherent().is_err(), "shadow fence above the live fence");
        let mut p = live;
        p.emission_halving_epochs = 0;
        assert!(p.is_coherent().is_err(), "R0 without a halving schedule");
        let mut p = live;
        p.settlement_delay_epochs = 0;
        assert!(p.is_coherent().is_err(), "settling the live epoch");
        let mut p = live;
        p.emission_activation_epoch = u64::MAX;
        assert!(p.is_coherent().is_err(), "a budget that never starts");
        let mut p = live;
        p.emission_min_network_compute = 0;
        assert!(p.is_coherent().is_err(), "no compute floor");
    }

    #[test]
    fn settlement_delay_floor_covers_the_finalization_depth() {
        // challenge 300 + reorg 300 over 100-block epochs → ceil(600/100)+1 = 7,
        // and never below the vote-side credit delay.
        assert_eq!(TokenParams::min_settlement_delay_epochs(300, 300, 100, 1), 7);
        assert_eq!(TokenParams::min_settlement_delay_epochs(300, 300, 100, 9), 9);
        assert_eq!(TokenParams::min_settlement_delay_epochs(0, 0, 100, 0), 1, "at least the in-flight epoch");

        let mut p = TokenParams {
            tkn_shadow_activation_daa_score: 1_000,
            tkn_activation_daa_score: 2_000,
            emission_activation_epoch: 10,
            emission_epoch_budget_r0_atomic: 1,
            emission_halving_epochs: 1,
            settlement_delay_epochs: 7,
            emission_min_network_compute: 1,
            tkn_phase_b_activation_daa_score: u64::MAX,
        };
        p.is_coherent_with_vlt(2_000, 300, 300, 100, 1).unwrap();
        p.settlement_delay_epochs = 6;
        assert!(p.is_coherent_with_vlt(2_000, 300, 300, 100, 1).is_err(), "below the burial depth");
        p.settlement_delay_epochs = 7;
        assert!(p.is_coherent_with_vlt(3_000, 300, 300, 100, 1).is_err(), "token fence below the VLT shadow fence");
    }
}
