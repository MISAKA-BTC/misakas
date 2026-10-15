//! **RFC-0007 §4.1 T6 lazy verifier: planted faults in paid claims and the receipt signers' liability** (`palw_paid_claim_trap_v1`;
//! dormant — `None` on every preset and under the RFC06–08 measured-value gate; kernel route aux 142). User decision 2026-10-15 §8
//! ("推奨で進めて良い"): the recommended (a)+(b) of the remaining-work note §4.1.
//!
//! ```text
//! (a) planted faults   a producer drawn by the audit mesh's slot lottery commits a trap with the EXISTING TrapCommitted (tag
//!                      102): H(claim ‖ fault leaf ‖ tiles ‖ salt) under its own bond, salted, naming no claim — the same object
//!                      and the same admission as an audit-mesh trap. It must land strictly BEFORE the paid claim's seat lock
//!                      (the seats are then drawn by the paid-claim beacon over a fault that was already fixed). After the
//!                      receipt window, inside the reveal window, the EXISTING TrapRevealed (tag 103) opens it over the paid
//!                      base claim. Its fault evidence is the court's own: the claim must stand convicted by the kernel's
//!                      deterministic FileProof/FileDemand check (PRINCIPLES §8 — no computation proof of any kind).
//! (b) signer liability every drawn checker that signed a Clean (complete) receipt on a paid claim the court convicts — a trap
//!                      or an ordinary claim alike — is charged the signer liability on its collateral at the conviction and
//!                      forfeits its held check fee at the fee's release; the conviction's accuser, when it is a drawn seat
//!                      other than the setter, is paid the existing trap bounty at a trap's reveal.
//! check fees           a paid claim's verifier check fee does not leave at its attestation: it is held (aux 142) until the
//!                      trap reveal window has passed and the claim is terminal, then released once through the once-only
//!                      registry under the combined liability check with that window as the verifier's liability horizon.
//! ```
//!
//! Nothing here grants consensus credit or weight (RFC-0018 Gate C stays the only credit path); a trap claim is a convicted claim,
//! so it has no Final payout and no credit, and its producer's escrow is settled by the court's existing rules.
use borsh::{BorshDeserialize, BorshSerialize};

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::PalwBondKeyV2;

/// The kernel route aux table of the paid-claim trap rows (allocated 2026-10-16: aux 142).
pub const PALW_PAID_CLAIM_TRAP_TABLE_V1: u8 = 142;
/// `0x01 ‖ base claim` (65 bytes): a paid claim's conviction as the court recorded it ([`PalwPaidClaimConvictionRowV1`]).
pub const PALW_PAID_CLAIM_TRAP_CONVICTION_PREFIX_V1: u8 = 0x01;
/// `0x02 ‖ claim ‖ verifier ‖ le64(slot)` (137 bytes): a held verifier check fee ([`PalwHeldCheckFeeRowV1`]).
pub const PALW_PAID_CLAIM_TRAP_HELD_FEE_PREFIX_V1: u8 = 0x02;
/// The rows' version.
pub const PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1: u8 = 1;

/// **UNKNOWN — the paid-trap rate, conditional on the audit mesh's slot draw** (basis points of 10,000 per bond and trap slot).
/// A trap is committed through the mesh's own admission, so its setter was already drawn by the mesh lottery
/// (`palw_mesh_v1::PALW_TRAP_RATE_BP_V1`) at the commit; this second ticket, asked only at a paid reveal (the commit itself stays
/// identical to a mesh trap's), thins it. The effective paid-trap rate is `mesh rate × this / 10,000`; the probe value 10,000 leaves
/// the mesh rate alone. Not a proposal: the measured record must pin the effective `p` with `p × forfeiture > the check cost`
/// (note §4.1).
pub const PALW_PAID_CLAIM_TRAP_RATE_BP_V1: u64 = 10_000;
/// **UNKNOWN — check-fee forfeiture** (permille of each held check fee a Clean signer of a convicted claim loses). Probe value.
pub const PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1: u64 = 1_000;
/// **UNKNOWN — receipt-signer liability** (sompi charged once to each Clean signer of a convicted paid claim). Probe value.
pub const PALW_PAID_CLAIM_TRAP_SIGNER_LIABILITY_SOMPI_V1: u64 = 1;
/// **UNKNOWN — the trap reveal window** (DAA after the paid claim's receipt window, never past its signed liability horizon): a trap
/// is revealed inside it, and a verifier's check fee is held through it (the verifier's liability horizon). Probe value.
pub const PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1: u64 = 240;
/// Held check fees released by one block's tick (a structural bound on the sweep's work, not an economic value).
pub const PALW_PAID_CLAIM_TRAP_RELEASES_PER_BLOCK_V1: usize = 256;

const PALW_PAID_CLAIM_TRAP_SLOT_DOMAIN_V1: &[u8] = b"misaka-palw/paid-claim-trap/slot/v1";

/// **The policy the measured-value record pins** (every UNKNOWN constant above).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwPaidClaimTrapPolicyV1 {
    pub rate_bp: u64,
    pub check_fee_forfeit_permille: u64,
    pub signer_liability_sompi: u64,
    pub reveal_window_daa: u64,
}

/// The constants as one policy (UNMEASURED: probe values; the gate refuses to arm it without a pinned record).
pub const PALW_PAID_CLAIM_TRAP_POLICY_V1: PalwPaidClaimTrapPolicyV1 = PalwPaidClaimTrapPolicyV1 {
    rate_bp: PALW_PAID_CLAIM_TRAP_RATE_BP_V1,
    check_fee_forfeit_permille: PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1,
    signer_liability_sompi: PALW_PAID_CLAIM_TRAP_SIGNER_LIABILITY_SOMPI_V1,
    reveal_window_daa: PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1,
};

/// **A paid claim's conviction, as the court recorded it** (aux 142, `0x01 ‖ base claim`): written once, at the conviction, past the
/// fence — the accuser it names is the one a trap's bounty may pay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidClaimConvictionRowV1 {
    pub version: u8,
    pub base_claim: [u8; 64],
    pub enrollment: [u8; 64],
    pub accuser: [u8; 64],
    pub convicted_daa: u64,
    /// The row is forgotten once this DAA has passed (the end of the claim's trap reveal window, or the conviction's DAA if later).
    pub forget_after_daa: u64,
}

/// **A held verifier check fee** (aux 142, `0x02 ‖ claim ‖ verifier ‖ le64(slot)`): the poster's escrow was debited at the
/// attestation; the verifier is paid once `release_after_daa` has passed, never before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHeldCheckFeeRowV1 {
    pub version: u8,
    pub claim: [u8; 64],
    pub verifier: [u8; 64],
    pub slot: u64,
    pub poster: [u8; 64],
    pub fee: u64,
    pub release_after_daa: u64,
}

pub fn palw_paid_trap_conviction_key_v1(claim: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(PALW_PAID_CLAIM_TRAP_CONVICTION_PREFIX_V1);
    key.extend_from_slice(claim);
    key
}

/// The prefix of every held fee of `verifier` on `claim` (129 bytes).
pub fn palw_paid_trap_held_fee_prefix_v1(claim: &[u8; 64], verifier: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(137);
    key.push(PALW_PAID_CLAIM_TRAP_HELD_FEE_PREFIX_V1);
    key.extend_from_slice(claim);
    key.extend_from_slice(verifier);
    key
}

pub fn palw_paid_trap_held_fee_key_v1(claim: &[u8; 64], verifier: &[u8; 64], slot: u64) -> Vec<u8> {
    let mut key = palw_paid_trap_held_fee_prefix_v1(claim, verifier);
    key.extend_from_slice(&slot.to_le_bytes());
    key
}

/// **The paid-trap ticket**: may `setter`'s trap committed at `daa` be revealed over a paid claim? A uniform ticket per `(bond, slot)`
/// against [`PALW_PAID_CLAIM_TRAP_RATE_BP_V1`], on top of the mesh slot draw its commitment already passed; asked at the reveal only.
pub fn palw_paid_trap_slot_drawn_v1(setter: &PalwBondKeyV2, daa: u64) -> bool {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_PAID_CLAIM_TRAP_SLOT_DOMAIN_V1).to_state();
    state.update(&borsh::to_vec(setter).expect("a bond key serializes"));
    state.update(&(daa / crate::palw_mesh_v1::PALW_TRAP_SLOT_DAA_V1).to_le_bytes());
    let mut digest = [0u8; 64];
    digest.copy_from_slice(state.finalize().as_bytes());
    crate::palw_panel_v2::palw_draw_ticket_u64_v1(&crate::Hash64::from_bytes(digest)) % 10_000 < PALW_PAID_CLAIM_TRAP_RATE_BP_V1
}

/// **The verifier's liability horizon on a paid claim**: the last DAA of the trap reveal window after the claim's signed receipt
/// window, never past the claim's signed liability horizon (no conviction lands after it, so nothing is held past it). A check fee is
/// held through it; a trap is revealed inside it.
pub fn palw_paid_trap_reveal_end_v1(lifetime: &misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2) -> u64 {
    lifetime.checking_until.saturating_add(PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1).min(lifetime.liability_until)
}

/// What one Clean signer of a convicted claim loses of one held fee: `(forfeited, kept)`.
pub fn palw_paid_trap_forfeit_v1(fee: u64) -> (u64, u64) {
    let forfeited = u64::try_from(
        u128::from(fee) * u128::from(PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1.min(1_000)) / 1_000,
    )
    .unwrap_or(fee)
    .min(fee);
    (forfeited, fee - forfeited)
}

/// **The paid V2 claim a trap or a check fee is judged against** — the fields of its enrollment authority this mechanism reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPaidTrapClaimV1 {
    pub base_claim: [u8; 64],
    pub enrollment: [u8; 64],
    pub producer: PalwBondKeyV2,
    pub interactive: bool,
    pub lifetime: misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2,
}

impl PalwPaidTrapClaimV1 {
    pub fn of(a: &crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2) -> Self {
        Self {
            base_claim: a.enrollment.base_claim,
            enrollment: a.enrollment.id(),
            producer: a.producer,
            interactive: a.class.interactive_plan.is_some(),
            lifetime: a.enrollment.lifetime,
        }
    }

    /// The claim's trap reveal window end (the verifier's liability horizon).
    pub fn reveal_end(&self) -> u64 {
        palw_paid_trap_reveal_end_v1(&self.lifetime)
    }
}

/// **Everything a paid-trap reveal is judged on**, gathered from the state by the fold (and by the acceptance walk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPaidTrapRevealInputV1 {
    pub tiles: u64,
    pub fault_leaf: u64,
    /// `Some((setter, committed_daa, revealed))` of the commitment the reveal opens; `None`: no such commitment.
    pub trap: Option<(PalwBondKeyV2, u64, bool)>,
    /// The reveal's own setter.
    pub setter: PalwBondKeyV2,
    /// The paid claim's producer (its enrollment authority's).
    pub producer: PalwBondKeyV2,
    pub interactive: bool,
    /// The paid claim's seat lock DAA (`None`: not locked).
    pub lock_daa: Option<u64>,
    /// The setter drawn by the paid-trap lottery in its commitment's slot.
    pub setter_drawn: bool,
    /// The paid claim's signed receipt window end.
    pub checking_until: u64,
    /// [`palw_paid_trap_reveal_end_v1`] of the paid claim's signed lifetime.
    pub reveal_end: u64,
    /// The court convicted the paid claim (the kernel row's own verdict).
    pub convicted: bool,
    pub now: u64,
}

/// **The paid-trap reveal rule**, pure (every check names itself; the first that fails refuses):
/// a well-formed tile pair; an open commitment of this very setter; the setter is the paid claim's producer and was drawn; a
/// non-interactive claim; the commitment landed STRICTLY before the seat lock (a commitment at or after the lock could be shaped
/// to the drawn seats); revealed after the receipt window and inside the reveal window; and the court convicted the claim (the
/// fault's evidence is the court's own deterministic check, never a proof carried by the reveal).
pub fn palw_paid_trap_reveal_admits_v1(i: &PalwPaidTrapRevealInputV1) -> Result<(), &'static str> {
    if i.tiles == 0 || i.tiles > crate::palw_mesh_v1::PALW_TRAP_MAX_TILES_V1 || i.fault_leaf >= i.tiles {
        return Err("paid trap: the tile count or the planted tile is out of range");
    }
    let Some((setter, committed_daa, revealed)) = i.trap else {
        return Err("paid trap: the reveal opens no commitment (claim, tile, tile count or salt differ)");
    };
    if setter != i.setter {
        return Err("paid trap: the commitment belongs to another setter");
    }
    if revealed {
        return Err("paid trap: the commitment is already revealed");
    }
    if i.setter != i.producer {
        return Err("paid trap: only the paid claim's producer can have planted its fault");
    }
    if !i.setter_drawn {
        return Err("paid trap: the setter was not drawn by the paid-trap lottery in its commitment's slot");
    }
    if i.interactive {
        return Err("paid trap: a trap on an interactive (staged) paid claim is not supported");
    }
    let Some(lock_daa) = i.lock_daa else { return Err("paid trap: the paid claim has no seat lock") };
    if committed_daa >= lock_daa {
        return Err("paid trap: the trap was committed at or after the paid claim's seat lock");
    }
    if i.now <= i.checking_until {
        return Err("paid trap: revealed inside the receipt window");
    }
    if i.now > i.reveal_end {
        return Err("paid trap: revealed after the reveal window");
    }
    if !i.convicted {
        return Err("paid trap: the court has not convicted the claim (the trap's fault evidence is the court's conviction)");
    }
    Ok(())
}

impl Params {
    /// Whether the paid-claim trap is in force at `daa` (never on any shipped preset).
    pub fn palw_paid_claim_trap_active_at(&self, daa: u64) -> bool {
        self.palw_paid_claim_trap_v1.is_some_and(|at| at != ForkActivation::never() && at.is_active(daa))
    }

    /// **The fence's refusal.** Prerequisites at or below it: the paid claim challenge (whose checkers it judges), the audit mesh
    /// (whose trap objects it reuses) and the once-only settlement registry (which settles every fee, forfeiture and liability
    /// it moves); then the measured-value gate over its UNKNOWN policy (records EMPTY: dormant).
    pub fn validate_palw_paid_claim_trap_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_paid_claim_trap_v1.filter(|at| *at != ForkActivation::never()) else { return Ok(()) };
        let at_or_below = |a: Option<ForkActivation>| a.is_some_and(|a| a != ForkActivation::never() && a.daa_score() <= at.daa_score());
        if !at_or_below(self.palw_claim_challenge_v2.as_ref().map(|f| f.activation)) {
            return Err(PalwModeV2Error::Invalid("palw_paid_claim_trap_v1 requires palw_claim_challenge_v2 at or below it"));
        }
        if !at_or_below(self.palw_audit_mesh_v1) {
            return Err(PalwModeV2Error::Invalid("palw_paid_claim_trap_v1 requires palw_audit_mesh_v1 (its trap objects) at or below it"));
        }
        if !at_or_below(self.palw_settlement_once_v1) {
            return Err(PalwModeV2Error::Invalid("palw_paid_claim_trap_v1 requires palw_settlement_once_v1 at or below it"));
        }
        self.palw_rfc0608_measured_v1(
            crate::palw_rfc0608_measured_v1::PalwRfc0608FenceV1::PaidClaimTrapV1,
            &(at.daa_score(), PALW_PAID_CLAIM_TRAP_POLICY_V1),
        )
    }

    /// Mirror the fence into the V2 bundle's state params (the fold holds only those). `None` where unarmed.
    pub fn sync_palw_paid_claim_trap_v1(&mut self) {
        let from = self.palw_paid_claim_trap_v1.filter(|at| *at != ForkActivation::never()).map(|at| at.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_paid_claim_trap_from(from);
        }
    }
}

#[cfg(test)]
mod tests {
    //! SOURCE ONLY (not compiled or executed in this lane).
    use super::*;

    fn bond(i: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(i), 0))
    }

    fn input() -> PalwPaidTrapRevealInputV1 {
        PalwPaidTrapRevealInputV1 {
            tiles: 8,
            fault_leaf: 3,
            trap: Some((bond(1), 100, false)),
            setter: bond(1),
            producer: bond(1),
            interactive: false,
            lock_daa: Some(101),
            setter_drawn: true,
            checking_until: 200,
            reveal_end: 200 + PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1,
            convicted: true,
            now: 201,
        }
    }

    #[test]
    fn a_trap_committed_before_the_lock_is_admitted_and_at_or_after_it_refused() {
        palw_paid_trap_reveal_admits_v1(&input()).unwrap();
        // Committed at the lock's own DAA: the seats could already be known.
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: Some((bond(1), 101, false)), ..input() }).is_err());
        // After the lock.
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: Some((bond(1), 150, false)), ..input() }).is_err());
        // No lock at all.
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { lock_daa: None, ..input() }).is_err());
    }

    #[test]
    fn a_reveal_that_does_not_open_its_commitment_is_refused() {
        // Another claim, tile, tile count or salt: no commitment opens.
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: None, ..input() }).is_err());
        // Another setter's commitment, an already revealed one, a setter that is not the producer, an undrawn setter.
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: Some((bond(2), 100, false)), ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: Some((bond(1), 100, true)), ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { producer: bond(3), ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { setter_drawn: false, ..input() }).is_err());
        // Malformed tiles.
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { fault_leaf: 8, ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { tiles: 0, fault_leaf: 0, ..input() }).is_err());
    }

    #[test]
    fn a_reveal_needs_the_receipt_window_closed_its_own_window_open_and_the_courts_conviction() {
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { now: 200, ..input() }).is_err(), "inside receipts");
        let end = 200 + PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1;
        palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { now: end, ..input() }).unwrap();
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { now: end + 1, ..input() }).is_err(), "late");
        // The window never runs past the claim's signed liability horizon.
        let short = misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2 {
            committed_daa: 0,
            checking_until: 200,
            challenge_until: 210,
            dispute_until: 220,
            liability_until: 230,
        };
        assert_eq!(palw_paid_trap_reveal_end_v1(&short), 230);
        let long = misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2 { liability_until: u64::MAX, ..short };
        assert_eq!(palw_paid_trap_reveal_end_v1(&long), end);
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { convicted: false, ..input() }).is_err(), "no verdict");
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { interactive: true, ..input() }).is_err());
    }

    #[test]
    fn the_forfeiture_splits_a_fee_exactly_and_the_lottery_is_per_slot() {
        let (lost, kept) = palw_paid_trap_forfeit_v1(1_000);
        assert_eq!(lost + kept, 1_000);
        assert_eq!(lost, 1_000 * PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1 / 1_000);
        assert_eq!(palw_paid_trap_forfeit_v1(0), (0, 0));
        // One ticket per (bond, slot): two DAAs of one slot draw alike.
        let slot = crate::palw_mesh_v1::PALW_TRAP_SLOT_DAA_V1;
        assert_eq!(palw_paid_trap_slot_drawn_v1(&bond(1), slot * 7), palw_paid_trap_slot_drawn_v1(&bond(1), slot * 7 + slot - 1));
    }

    #[test]
    fn the_rows_keys_have_their_audited_lengths() {
        assert_eq!(palw_paid_trap_conviction_key_v1(&[1; 64]).len(), 65);
        assert_eq!(palw_paid_trap_held_fee_key_v1(&[1; 64], &[2; 64], 3).len(), 137);
        assert!(palw_paid_trap_held_fee_key_v1(&[1; 64], &[2; 64], 3).starts_with(&palw_paid_trap_held_fee_prefix_v1(&[1; 64], &[2; 64])));
    }

    #[test]
    fn every_shipped_preset_keeps_the_trap_dormant_and_arming_is_refused() {
        use crate::config::params::{
            DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params, palw_t12_shipped_params,
        };
        for params in
            [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_t12_shipped_params(), palw_rc_shipped_params()]
        {
            assert!(params.palw_paid_claim_trap_v1.is_none());
            assert!(!params.palw_paid_claim_trap_active_at(u64::MAX));
            params.validate_palw_paid_claim_trap_v1().unwrap();
            assert!(params.palw_fences_v1().iter().any(|(name, at)| *name == "palw_paid_claim_trap_v1" && at.is_none()));
            let mut armed = params.clone();
            armed.palw_paid_claim_trap_v1 = Some(ForkActivation::new(9_000_000));
            assert!(armed.validate_palw_paid_claim_trap_v1().is_err(), "no prerequisites below it");
            let mut never = params.clone();
            never.palw_paid_claim_trap_v1 = Some(ForkActivation::never());
            assert!(!never.palw_paid_claim_trap_active_at(u64::MAX));
            never.validate_palw_paid_claim_trap_v1().unwrap();
        }
    }
}
