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

/// Stateless validation of a [`TokenTransferPayload`]'s bytes — everything
/// checkable without a chain. Nonce currency, balance sufficiency, and the
/// ML-DSA-87 signature itself are stateful and belong to the application seam.
pub fn validate_token_transfer_payload(payload: &[u8]) -> Result<(), TokenTxError> {
    let p: TokenTransferPayload = borsh::from_slice(payload).map_err(|_| TokenTxError::Decode)?;
    if p.version != TOKEN_PAYLOAD_VERSION_V1 {
        return Err(TokenTxError::UnsupportedVersion(p.version));
    }
    if p.asset_id != TOK_ASSET_ID {
        return Err(TokenTxError::UnknownAsset(p.asset_id));
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
    if p.asset_id != TOK_ASSET_ID {
        return Err(TokenTxError::UnknownAsset(p.asset_id));
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
    if params.emission_epoch_budget_r0_atomic == 0
        || params.emission_halving_epochs == 0
        || epoch < params.emission_activation_epoch
    {
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
    let mut settlement =
        TokenEmissionSettlement { budget, network_compute, paid_total: 0, rewards: Vec::new() };
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
                return Err("emission_activation_epoch must be set when R0 > 0 (a budget that never starts is a misconfiguration, not a policy)");
            }
            if self.emission_min_network_compute == 0 {
                return Err("emission_min_network_compute must be > 0 when R0 > 0 (design §5.1: no whole-budget mint on a near-empty network)");
            }
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
        vlt_activation_daa_score: u64,
        challenge_window_blocks: u64,
        max_reorg_horizon_blocks: u64,
        epoch_length_blocks: u64,
        credit_delay_epochs: u32,
    ) -> Result<(), &'static str> {
        self.is_coherent()?;
        if self.tkn_activation_daa_score < vlt_activation_daa_score {
            return Err("tkn_activation_daa_score must be >= vlt_activation_daa_score (design §10: no token program on an inert compute overlay)");
        }
        if self.emission_epoch_budget_r0_atomic > 0 {
            let floor = Self::min_settlement_delay_epochs(
                challenge_window_blocks,
                max_reorg_horizon_blocks,
                epoch_length_blocks,
                credit_delay_epochs,
            );
            if self.settlement_delay_epochs < floor {
                return Err("settlement_delay_epochs is below the credit-finalization depth (design §5.3: settlement must read only finalized epochs)");
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
        assert_eq!(
            validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()),
            Err(TokenTxError::UnsupportedVersion(2))
        );

        let mut p = transfer_payload();
        p.asset_id = 7;
        assert_eq!(validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::UnknownAsset(7)));

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
        };
        p.is_coherent_with_vlt(2_000, 300, 300, 100, 1).unwrap();
        p.settlement_delay_epochs = 6;
        assert!(p.is_coherent_with_vlt(2_000, 300, 300, 100, 1).is_err(), "below the burial depth");
        p.settlement_delay_epochs = 7;
        assert!(p.is_coherent_with_vlt(3_000, 300, 300, 100, 1).is_err(), "token fence below the VLT weight fence");
    }
}
