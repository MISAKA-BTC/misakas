//! **RFC-0008: once-only settlement across ALL consumers** (`palw_settlement_once_v1`; dormant — `None` on every preset and
//! refused by validation; kernel route aux table 140).
//!
//! Several consumers can reach the same economic obligation: a V2 claim's issuance-funded Final (`finalize_claim`, including its
//! EXEC root split and B/R/F budget draw), a kernel claim's GAP-5 escrow reward (the closing kernel tick, a kernel object arm, the
//! paid-claim Final projection's settlement batch and the EXEC kernel-fee route all apply the same `PayJobEscrow`/`FinalReward`
//! pair), a verification-gate deferral release, a SecurityCertified held-reward release, an EXEC leg's maturity, a vesting move,
//! and the monetary-material funding/fee/default legs. Each consumer has its OWN guard today (a phase, a removed escrow row, a
//! `paid` flag, a deleted deferral row) — none of them sees the others.
//!
//! This registry makes double settlement impossible by construction: every pay point first asks
//! `TransitionBuilder::settle_once_v1` with the exact [`PalwSettlementObligationV1`] — `(leg, subject, party, ordinal)`, never the
//! consumer — and the marker is checked and written in the SAME transition before any value moves. A second consumer of the same
//! obligation (or the same consumer twice) finds the marker and pays nothing: a strict object arm refuses (the object is dropped),
//! a closing tick skips the payment (the block stands). The marker row is the debit journal of that leg: it records the consumer,
//! the amount, the real debit source (issuance carve, poster escrow, held certification reward, sponsor prepaid, provider
//! collateral) and the DAA, and its key is recomputed from the row on every read (a row under a foreign key is refused).
//!
//! **No credit.** A marker is never consensus credit, weight or a Final: consensus credit stays exclusively with the RFC-0018
//! SecurityCertified / Gate C predicate (`palw_rfc0018_v1::palw_gate_c_weight_v1`); nothing here reads or writes weight.
//!
//! **No computation proof.** Hashes here are data identity only (PRINCIPLES §8).
use borsh::{BorshDeserialize, BorshSerialize};

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;

/// The kernel route aux table of the markers (allocated to this lane: aux 140).
pub const PALW_SETTLEMENT_ONCE_TABLE_V1: u8 = 140;
/// The marker row's version.
pub const PALW_SETTLEMENT_ONCE_ROW_VERSION_V1: u8 = 1;
const PALW_SETTLEMENT_OBLIGATION_DOMAIN_V1: &[u8] = b"misaka-palw/settlement-once/obligation/v1";

/// **The economic leg an obligation settles** — consumer-independent by design: two consumers paying the same leg of the same
/// subject compute the same obligation id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwSettlementLegV1 {
    /// A V2 claim's issuance-funded Final as a whole: its R/F budget draw, producer/seat/reserve/vesting split, buyback slice and
    /// EXEC root split. Subject: the V2 claim id.
    IssuanceFinal = 1,
    /// A kernel claim's GAP-5 escrow reward: the poster's `PayJobEscrow` debit and the producer `FinalReward` it routes (kernel
    /// tick, kernel object arm, paid-claim Final batch or EXEC kernel-fee route). Subject: the kernel claim id.
    EscrowFinalReward = 2,
    /// A model-verification-gate deferral released into the payout queue. Subject: the claim.
    GateDeferredRelease = 3,
    /// An RFC-0018 held issuance reward named at SecurityCertified. Subject: the claim.
    CertificationRelease = 4,
    /// One EXEC root reward leg matured into payment. Subject: the root claim. Party: the leg's bond (kernel id). Ordinal: its fund.
    ExecLegPayment = 5,
    /// A vesting row's matured move. Subject: the vesting row key.
    VestingRelease = 6,
    /// A monetary-material consent's sponsor prepaid funding debit. Subject: the consent id.
    MaterialFunding = 7,
    /// A material provider's prepaid fee payment. Subject: the provider fee income id.
    MaterialProviderFee = 8,
    /// A material provider's default collateral charge. Subject: the material root. Party: the provider (kernel id).
    MaterialProviderDefault = 9,
}

/// Which consumer settled a leg (audit only; never part of the obligation id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwSettlementConsumerV1 {
    NativeFinal = 1,
    KernelTick = 2,
    KernelObject = 3,
    ExecKernelFee = 4,
    VerificationGate = 5,
    SecurityCertification = 6,
    ExecLegMaturity = 7,
    VestingMaturity = 8,
    MonetaryMaterial = 9,
}

/// **The real debit a leg is paid from** (the journal's source side). A payment is never larger than its debit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwSettlementDebitV1 {
    /// The claim's issuance carve withheld at acceptance (`escrowed_reward`).
    Issuance { carve: u64 },
    /// A poster's job escrow, debited from the poster's bond (kernel id) in the same batch.
    Escrow { poster: [u8; 64], amount: u64 },
    /// A reward held earlier on chain (a gate deferral, a certification hold, a vesting row, an EXEC leg).
    Held { amount: u64 },
    /// A sponsor's prepaid funding (kernel id), debited by this leg itself.
    Prepaid { sponsor: [u8; 64], amount: u64 },
    /// A bond's posted collateral (kernel id), charged by this leg itself.
    Collateral { bond: [u8; 64], amount: u64 },
}

impl PalwSettlementDebitV1 {
    pub const fn amount(&self) -> u64 {
        match *self {
            Self::Issuance { carve } => carve,
            Self::Escrow { amount, .. } | Self::Held { amount } | Self::Prepaid { amount, .. } | Self::Collateral { amount, .. } => {
                amount
            }
        }
    }
}

/// **One exact obligation**: the leg, its subject, its party (`[0; 64]` when the leg has none) and an ordinal distinguishing
/// legitimately repeated legs of one subject (an EXEC leg's fund; `0` otherwise).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSettlementObligationV1 {
    pub leg: PalwSettlementLegV1,
    pub subject: [u8; 64],
    pub party: [u8; 64],
    pub ordinal: u64,
}

impl PalwSettlementObligationV1 {
    /// A leg of `subject` with no party and ordinal 0.
    pub const fn of(leg: PalwSettlementLegV1, subject: [u8; 64]) -> Self {
        Self { leg, subject, party: [0; 64], ordinal: 0 }
    }

    pub const fn with_party(leg: PalwSettlementLegV1, subject: [u8; 64], party: [u8; 64], ordinal: u64) -> Self {
        Self { leg, subject, party, ordinal }
    }

    /// The marker's key: a domain-keyed BLAKE2b-512 over the leg tag, subject, party and ordinal (data identity only).
    pub fn id_v1(&self) -> [u8; 64] {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_SETTLEMENT_OBLIGATION_DOMAIN_V1).to_state();
        state.update(&[self.leg as u8]);
        state.update(&self.subject);
        state.update(&self.party);
        state.update(&self.ordinal.to_le_bytes());
        let mut out = [0u8; 64];
        out.copy_from_slice(state.finalize().as_bytes());
        out
    }
}

/// **The marker row** (aux 140, keyed by [`PalwSettlementObligationV1::id_v1`]): the one debit-journal entry of a settled leg.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSettlementOnceRowV1 {
    pub version: u8,
    pub obligation: PalwSettlementObligationV1,
    pub consumer: PalwSettlementConsumerV1,
    /// What this leg paid (or debited, for a debit-only leg).
    pub amount: u64,
    pub debit: PalwSettlementDebitV1,
    pub settled_daa: u64,
}

impl PalwSettlementOnceRowV1 {
    /// Structural validity under `key`: the version, the key recomputed from the row, and no payment beyond its debit.
    pub fn validate_v1(&self, key: &[u8]) -> Result<(), &'static str> {
        if self.version != PALW_SETTLEMENT_ONCE_ROW_VERSION_V1 {
            return Err("unknown settlement-once row version");
        }
        if key != self.obligation.id_v1().as_slice() {
            return Err("a settlement-once row under a foreign obligation key");
        }
        if self.amount > self.debit.amount() {
            return Err("a settled leg pays more than its recorded debit");
        }
        Ok(())
    }
}

impl Params {
    /// Whether the once-only settlement registry is in force at `daa` (never on any shipped preset).
    pub fn palw_settlement_once_active_at(&self, daa: u64) -> bool {
        self.palw_settlement_once_v1.is_some_and(|at| at != ForkActivation::never() && at.is_active(daa))
    }

    /// **Refuse arming.** The markers live in kernel route aux 140, so the kernel route fence must be at or below this one; and the
    /// registry is not armable until every consumer's settlement and its reorg/restore controls have run (UNKNOWN until then).
    pub fn validate_palw_settlement_once_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_settlement_once_v1.filter(|at| *at != ForkActivation::never()) else { return Ok(()) };
        if !self.palw_probabilistic_constraints_v1.is_some_and(|k| k != ForkActivation::never() && k.daa_score() <= at.daa_score()) {
            return Err(PalwModeV2Error::Invalid("palw_settlement_once_v1 requires the kernel route fence at or below it"));
        }
        // Cross-consumer settlement, reorg and restore controls are evidence: arming needs a pinned measured-value record and no
        // open blocker (`palw_rfc0608_measured_v1`; records EMPTY: dormant). The fence has no policy, so the record pins its
        // activation height. testnet-12 DAA 14,000 release plan.
        self.palw_rfc0608_measured_v1(crate::palw_rfc0608_measured_v1::PalwRfc0608FenceV1::SettlementOnceV1, &at.daa_score())
    }

    /// Mirror the fence into the V2 bundle's state params (the fold holds only those). `None` where unarmed.
    pub fn sync_palw_settlement_once_v1(&mut self) {
        let from = self.palw_settlement_once_v1.filter(|at| *at != ForkActivation::never()).map(|at| at.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_settlement_once_from(from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obligation(leg: PalwSettlementLegV1, subject: u8) -> PalwSettlementObligationV1 {
        PalwSettlementObligationV1::of(leg, [subject; 64])
    }

    #[test]
    fn obligation_ids_are_consumer_independent_and_separate_every_coordinate() {
        let base = obligation(PalwSettlementLegV1::EscrowFinalReward, 7);
        // Same leg, same subject: one id, whatever consumer later writes the row.
        assert_eq!(base.id_v1(), obligation(PalwSettlementLegV1::EscrowFinalReward, 7).id_v1());
        // Each coordinate separates.
        assert_ne!(base.id_v1(), obligation(PalwSettlementLegV1::IssuanceFinal, 7).id_v1());
        assert_ne!(base.id_v1(), obligation(PalwSettlementLegV1::EscrowFinalReward, 8).id_v1());
        assert_ne!(
            base.id_v1(),
            PalwSettlementObligationV1::with_party(PalwSettlementLegV1::EscrowFinalReward, [7; 64], [1; 64], 0).id_v1()
        );
        assert_ne!(
            base.id_v1(),
            PalwSettlementObligationV1::with_party(PalwSettlementLegV1::EscrowFinalReward, [7; 64], [0; 64], 1).id_v1()
        );
    }

    #[test]
    fn a_row_validates_only_under_its_own_key_and_within_its_debit() {
        let o = obligation(PalwSettlementLegV1::EscrowFinalReward, 3);
        let row = PalwSettlementOnceRowV1 {
            version: PALW_SETTLEMENT_ONCE_ROW_VERSION_V1,
            obligation: o,
            consumer: PalwSettlementConsumerV1::KernelTick,
            amount: 10,
            debit: PalwSettlementDebitV1::Escrow { poster: [9; 64], amount: 10 },
            settled_daa: 100,
        };
        row.validate_v1(&o.id_v1()).unwrap();
        assert!(row.validate_v1(&obligation(PalwSettlementLegV1::EscrowFinalReward, 4).id_v1()).is_err(), "foreign key");
        let over = PalwSettlementOnceRowV1 { amount: 11, ..row };
        assert!(over.validate_v1(&o.id_v1()).is_err(), "no payment beyond its debit");
        let future = PalwSettlementOnceRowV1 { version: 2, ..row };
        assert!(future.validate_v1(&o.id_v1()).is_err());
        // The Borsh layout is the derive layout (reviewed by the native wire generator, not a handwritten codec).
        assert_eq!(borsh::from_slice::<PalwSettlementOnceRowV1>(&borsh::to_vec(&row).unwrap()).unwrap(), row);
    }

    #[test]
    fn every_shipped_preset_keeps_the_registry_dormant_and_arming_is_refused() {
        use crate::config::params::{
            DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params, palw_t12_shipped_params,
        };
        for params in
            [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_t12_shipped_params(), palw_rc_shipped_params()]
        {
            assert!(params.palw_settlement_once_v1.is_none());
            assert!(!params.palw_settlement_once_active_at(u64::MAX));
            params.validate_palw_settlement_once_v1().unwrap();
            assert!(params.palw_fences_v1().iter().any(|(name, at)| *name == "palw_settlement_once_v1" && at.is_none()));
            let mut armed = params.clone();
            armed.palw_settlement_once_v1 = Some(ForkActivation::new(9_000_000));
            assert!(armed.validate_palw_settlement_once_v1().is_err(), "no kernel route fence below it");
            armed.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(1));
            assert!(armed.validate_palw_settlement_once_v1().is_err(), "unexecuted controls: never armable here");
            let mut never = params.clone();
            never.palw_settlement_once_v1 = Some(ForkActivation::never());
            assert!(!never.palw_settlement_once_active_at(u64::MAX));
            never.validate_palw_settlement_once_v1().unwrap();
        }
    }
}
