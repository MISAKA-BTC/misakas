//! MISAKA Compute Token Program — what is left of it (`docs/misaka-compute-token-program-design-v0.1.md`).
//!
//! The token overlay is removed: the ledger fold, emission settlement, the token stores and the
//! token RPC reads are gone. No shipped network ever armed them — every preset carries
//! [`TokenParams::INERT`] — and [`crate::config::params::Params::validate_palw_v2`] refuses a
//! network that arms one, so nothing here can come back into force by configuration.
//!
//! Two things stay, because removing either would change consensus:
//!
//! * **The 0x30/0x31 payload shapes and their stateless validation.** Admission routes
//!   `SUBNETWORK_ID_TOKEN_TRANSFER` / `SUBNETWORK_ID_TOKEN_BURN` to
//!   [`validate_token_transfer_payload`] / [`validate_token_burn_payload`] on every network, so a
//!   chain may already carry such a transaction, and refusing a shape earlier builds admitted would
//!   split it. Past the validator overlay's retirement (ADR-0126) both ids are refused in header
//!   context. An admitted op is applied by nothing: its signature is length-checked, never
//!   verified, and it moves no balance.
//! * **[`TokenParams`].** [`crate::dns_finality::DnsParams`] embeds it and is hashed whole into
//!   `consensus_params_id`, so its fields and their inert values are part of every overlay
//!   network's fingerprint.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;

use crate::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN, validator_id_from_pubkey};

// ---------------------------------------------------------------------
// Constants.
// ---------------------------------------------------------------------

/// Wire-format version for every payload in this module.
pub const TOKEN_PAYLOAD_VERSION_V1: u16 = 1;

/// `asset_id` of the protocol asset **Token (TOK)** — design §4.1, the only asset id a payload
/// may name.
pub const TOK_ASSET_ID: u64 = 0;

/// Atomic units per 1 TOK (`10^8`, matching the sompi convention — design §4.2).
pub const TOK_ATOMIC_PER_UNIT: u64 = 100_000_000;

// ---------------------------------------------------------------------
// Payloads (design §4.3 — subnetworks 0x30/0x31).
// ---------------------------------------------------------------------

/// A `SUBNETWORK_ID_TOKEN_TRANSFER` (0x30) payload: the wire shape of a transfer of `amount` atomic
/// units of `asset_id` from the account of `from_pubkey`'s hash to `to`.
///
/// The carrier transaction is an ordinary base-coin transaction (mass-priced fee in base coin,
/// design §4.3). Admission checks this shape and nothing else; no ledger applies it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenTransferPayload {
    pub version: u16,
    /// Must be [`TOK_ASSET_ID`] (checked statelessly).
    pub asset_id: u64,
    /// The sender's ML-DSA-87 verifying key ([`STAKE_VALIDATOR_PUBKEY_LEN`] bytes). The sender id
    /// is [`validator_id_from_pubkey`]`(from_pubkey)`, which the self-transfer check compares
    /// against `to`.
    pub from_pubkey: Vec<u8>,
    /// The recipient owner id.
    pub to: Hash64,
    /// Atomic units. Zero is rejected statelessly.
    pub amount: u128,
    pub nonce: u64,
    /// Length checked statelessly ([`STAKE_ATTESTATION_SIG_LEN`]); never verified.
    pub signature: Vec<u8>,
}

/// A `SUBNETWORK_ID_TOKEN_BURN` (0x31) payload: the wire shape of a burn of `amount` atomic units
/// from the signer's own account. Admission checks this shape and nothing else.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenBurnPayload {
    pub version: u16,
    /// Must be [`TOK_ASSET_ID`].
    pub asset_id: u64,
    /// The burner's ML-DSA-87 verifying key ([`STAKE_VALIDATOR_PUBKEY_LEN`] bytes).
    pub owner_pubkey: Vec<u8>,
    /// Atomic units. Zero is rejected statelessly.
    pub amount: u128,
    pub nonce: u64,
    /// Length checked statelessly ([`STAKE_ATTESTATION_SIG_LEN`]); never verified.
    pub signature: Vec<u8>,
}

// ---------------------------------------------------------------------
// Stateless validation (design §4.3).
// ---------------------------------------------------------------------

/// Stateless validation failure for a token-op payload, carried by
/// `TxRuleError::InvalidTokenPayload`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenTxError {
    /// Payload bytes did not borsh-decode into the expected type (also fires
    /// on trailing bytes after an otherwise-valid prefix).
    Decode,
    /// The `version` field is not `TOKEN_PAYLOAD_VERSION_V1`.
    UnsupportedVersion(u16),
    /// The payload names an asset other than [`TOK_ASSET_ID`].
    UnknownAsset(u64),
    /// Sender/owner public key is not exactly `STAKE_VALIDATOR_PUBKEY_LEN`.
    InvalidPubKeyLen(usize),
    /// Signature is not exactly `STAKE_ATTESTATION_SIG_LEN`.
    InvalidSignatureLen(usize),
    /// `amount == 0`.
    ZeroAmount,
    /// `to == hash(from_pubkey)` — a transfer to self.
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

/// Decode a transfer payload. `None` on malformed bytes.
pub fn decode_token_transfer_payload(payload: &[u8]) -> Option<TokenTransferPayload> {
    borsh::from_slice(payload).ok()
}

/// Decode a burn payload — see [`decode_token_transfer_payload`].
pub fn decode_token_burn_payload(payload: &[u8]) -> Option<TokenBurnPayload> {
    borsh::from_slice(payload).ok()
}

/// Stateless validation of a [`TokenTransferPayload`]'s bytes — the whole of what consensus checks
/// about a transfer.
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
// Params (design §10). Kept for the fingerprint: `DnsParams` embeds them, and
// `Params::validate_palw_v2` refuses any fence that is not `u64::MAX`.
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
        Ok(())
    }

    /// The `D_settle` floor against the credit-finalization depth (design §5.3):
    /// settlement may only read epochs the (removed) VLT credit finalization
    /// accepted, so the delay must cover the challenge window plus the reorg
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

    /// A transfer whose recipient is the sender's own id is rejected at the door.
    #[test]
    fn self_transfer_is_rejected_statelessly() {
        let mut p = transfer_payload();
        p.to = validator_id_from_pubkey(&p.from_pubkey);
        assert_eq!(validate_token_transfer_payload(&borsh::to_vec(&p).unwrap()), Err(TokenTxError::SelfTransfer));
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
        assert!(p.is_coherent_with_vlt(3_000, 300, 300, 100, 1).is_err(), "token fence below the VLT shadow fence");
    }
}
