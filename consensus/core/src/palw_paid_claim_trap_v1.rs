//! **RFC-0007 §4.1 T6 lazy verifier: planted faults in paid claims and the receipt signers' liability** (`palw_paid_claim_trap_v1`;
//! dormant — `None` on every preset and under the RFC06–08 measured-value gate; kernel route aux 142). User decision 2026-10-15 §8
//! ("推奨で進めて良い"): the recommended (a)+(b) of the remaining-work note §4.1, with every paid claim carrying a commitment so a
//! planted fault is indistinguishable to its checkers until it is revealed.
//!
//! ```text
//! (a) commitments      past the fence EVERY non-staged paid V2 claim carries exactly one producer commitment, posted in the
//!                      paid-claim object family (`PaidClaimObjectV2::TrapCommitment`, appended; no new outer tag) strictly
//!                      before the claim's beacon seal window opens: H(base claim ‖ fault leaf ‖ tiles ‖ salt) — a planted fault
//!                      (a trap) or the null fault (tiles 0: an ordinary claim). Same object, same size, same poster, same
//!                      deposit, same lifetime: its presence says nothing. No seat is drawn (the paid lock) without it. Its own
//!                      rules, not the audit mesh's: no one-open-per-setter limit, no mesh lottery, and it lives to the claim's
//!                      own reveal-window end.
//! (b) trap validity    whether a revealed fault COUNTS as a trap is decided by the paid claim's beacon seed — fixed after the
//!                      commitment (it was posted before the seal window) — and the commitment's secret salt, at the measured
//!                      trap rate: the setter cannot choose which drawn checkers face a counted trap, and nobody but the setter
//!                      can tell whether a fault would count before the reveal.
//! (c) reveal           the EXISTING TrapRevealed (tag 103) opens the commitment to the fault after the receipt window, inside
//!                      the reveal window; its fault evidence is the court's own: the claim must stand convicted by the
//!                      kernel's deterministic FileProof/FileDemand check (PRINCIPLES §8 — no computation proof of any kind). A
//!                      null commitment needs no reveal (a null opening is refused: it changes nothing).
//! (d) signer liability every drawn checker that signed a Clean (complete) receipt on a paid claim the court convicts — a trap
//!                      or an ordinary claim alike — is charged the signer liability on its collateral at the conviction and
//!                      forfeits its held check fee at the fee's release; at a counted trap's reveal the conviction's accuser,
//!                      when it is a drawn seat other than the setter, is paid the existing trap bounty.
//! check fees           a paid claim's verifier check fee does not leave at its attestation: it is held (aux 142) until the
//!                      trap reveal window has passed and the claim is terminal, then released once through the once-only
//!                      registry under the combined liability check with that window as the verifier's liability horizon.
//! ```
//!
//! Nothing here grants consensus credit or weight (RFC-0018 Gate C stays the only credit path); a trap claim is a convicted claim,
//! so it has no Final payout and no credit, and its producer's escrow is settled by the court's existing rules. The commitment is a
//! salted hash (data identity, not a proof); no anonymous credential or other proof machinery is used (PRINCIPLES §8).
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
/// `0x03 ‖ enrollment` (65 bytes): a paid claim's one trap-or-null commitment ([`PalwPaidTrapCommitmentRowV1`]).
pub const PALW_PAID_CLAIM_TRAP_COMMITMENT_PREFIX_V1: u8 = 0x03;
/// The rows' version.
pub const PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1: u8 = 1;

/// **UNKNOWN — the trap rate** (basis points of 10,000): the chance that a committed fault counts as a trap, drawn from the paid
/// claim's beacon seed and the commitment's salt ([`palw_paid_trap_counts_v1`]). A probe value, not a proposal: the measured record
/// must pin `p` with `p × forfeiture > the check cost` and the bounty below what a counted trap costs its setter (note §4.1).
pub const PALW_PAID_CLAIM_TRAP_RATE_BP_V1: u64 = 100;
/// **UNKNOWN — check-fee forfeiture** (permille of each held check fee a Clean signer of a convicted claim loses). Probe value.
pub const PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1: u64 = 1_000;
/// **UNKNOWN — receipt-signer liability** (sompi charged once to each Clean signer of a convicted paid claim). Probe value.
pub const PALW_PAID_CLAIM_TRAP_SIGNER_LIABILITY_SOMPI_V1: u64 = 1;
/// **UNKNOWN — the trap reveal window** (DAA after the paid claim's receipt window, never past its signed liability horizon): a trap
/// is revealed inside it, a commitment lives to its end, and a verifier's check fee is held through it (the verifier's liability
/// horizon). Probe value.
pub const PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1: u64 = 240;
/// **UNKNOWN — the commitment deposit** (sompi reserved on the producer's bond from the commitment to the end of its lifetime,
/// IDENTICAL for a trap and a null commitment, returned whole either way). 0 unless the measured record requires one.
pub const PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1: u64 = 0;
/// Rows of each kind a block's tick releases or forgets (a structural bound on the sweep's work, not an economic value).
pub const PALW_PAID_CLAIM_TRAP_RELEASES_PER_BLOCK_V1: usize = 256;

const PALW_PAID_CLAIM_TRAP_COUNT_DOMAIN_V1: &[u8] = b"misaka-palw/paid-claim-trap/counts/v1";

/// **The policy the measured-value record pins** (every UNKNOWN constant above).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwPaidClaimTrapPolicyV1 {
    pub rate_bp: u64,
    pub check_fee_forfeit_permille: u64,
    pub signer_liability_sompi: u64,
    pub reveal_window_daa: u64,
    pub commit_deposit_sompi: u64,
}

/// The constants as one policy (UNMEASURED: probe values; the gate refuses to arm it without a pinned record).
pub const PALW_PAID_CLAIM_TRAP_POLICY_V1: PalwPaidClaimTrapPolicyV1 = PalwPaidClaimTrapPolicyV1 {
    rate_bp: PALW_PAID_CLAIM_TRAP_RATE_BP_V1,
    check_fee_forfeit_permille: PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1,
    signer_liability_sompi: PALW_PAID_CLAIM_TRAP_SIGNER_LIABILITY_SOMPI_V1,
    reveal_window_daa: PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1,
    commit_deposit_sompi: PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1,
};

/// **A paid claim's conviction, as the court recorded it** (aux 142, `0x01 ‖ base claim`): written once, at the conviction, past the
/// fence — the accuser it names is the one a counted trap's bounty may pay.
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

/// **A paid claim's one trap-or-null commitment** (aux 142, `0x03 ‖ enrollment`): the same shape for a trap and for the null fault;
/// deleted at a trap's reveal or once `forget_after_daa` (the claim's reveal-window end) has passed, its deposit returned either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidTrapCommitmentRowV1 {
    pub version: u8,
    pub enrollment: [u8; 64],
    pub base_claim: [u8; 64],
    pub producer: PalwBondKeyV2,
    pub commitment: [u8; 64],
    pub committed_daa: u64,
    pub deposit: u64,
    pub forget_after_daa: u64,
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

pub fn palw_paid_trap_commitment_key_v1(enrollment: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(PALW_PAID_CLAIM_TRAP_COMMITMENT_PREFIX_V1);
    key.extend_from_slice(enrollment);
    key
}

/// **The commitment a producer posts**: the audit mesh's salted trap hash over the paid base claim — `fault = Some((leaf, tiles))`
/// for a planted fault, `None` for the null fault (leaf 0, tiles 0). Both are one 64-byte digest: indistinguishable until opened.
pub fn palw_paid_trap_commitment_v1(base_claim: &[u8; 64], fault: Option<(u64, u64)>, salt: &[u8; 32]) -> [u8; 64] {
    let (leaf, tiles) = fault.unwrap_or((0, 0));
    crate::palw_mesh_v1::palw_trap_commitment_v1(&crate::Hash64::from_bytes(*base_claim), leaf, tiles, salt).as_bytes()
}

/// **Does a revealed fault count as a trap?** A uniform ticket over the paid claim's beacon seed (fixed after the commitment, which
/// precedes the seal window), its enrollment and the commitment's secret salt, against [`PALW_PAID_CLAIM_TRAP_RATE_BP_V1`]. Nobody
/// but the setter can evaluate it before the reveal (the salt is secret), and the setter fixed the salt before the seed existed.
pub fn palw_paid_trap_counts_v1(seed: &[u8; 64], enrollment: &[u8; 64], salt: &[u8; 32]) -> bool {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_PAID_CLAIM_TRAP_COUNT_DOMAIN_V1).to_state();
    state.update(seed);
    state.update(enrollment);
    state.update(salt);
    let mut digest = [0u8; 64];
    digest.copy_from_slice(state.finalize().as_bytes());
    crate::palw_panel_v2::palw_draw_ticket_u64_v1(&crate::Hash64::from_bytes(digest)) % 10_000 < PALW_PAID_CLAIM_TRAP_RATE_BP_V1
}

/// **The verifier's liability horizon on a paid claim**: the last DAA of the trap reveal window after the claim's signed receipt
/// window, never past the claim's signed liability horizon (no conviction lands after it, so nothing is held past it). A check fee is
/// held through it; a trap is revealed inside it; a commitment lives to it.
pub fn palw_paid_trap_reveal_end_v1(lifetime: &misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2) -> u64 {
    lifetime.checking_until.saturating_add(PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1).min(lifetime.liability_until)
}

/// What one Clean signer of a convicted claim loses of one held fee: `(forfeited, kept)`.
pub fn palw_paid_trap_forfeit_v1(fee: u64) -> (u64, u64) {
    let forfeited = u64::try_from(u128::from(fee) * u128::from(PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1.min(1_000)) / 1_000)
        .unwrap_or(fee)
        .min(fee);
    (forfeited, fee - forfeited)
}

/// **The paid V2 claim a commitment, a trap or a check fee is judged against** — the fields of its enrollment authority this
/// mechanism reads.
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

    /// The claim's trap reveal window end (the verifier's liability horizon; the commitment's lifetime).
    pub fn reveal_end(&self) -> u64 {
        palw_paid_trap_reveal_end_v1(&self.lifetime)
    }
}

/// **Everything a commitment is judged on**, gathered from the state by the fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPaidTrapCommitInputV1 {
    /// The object's signer.
    pub signer: PalwBondKeyV2,
    /// The paid claim's producer (its enrollment authority's).
    pub producer: PalwBondKeyV2,
    /// The claim's beacon source seal window start (the seed is not fixed before it).
    pub seal_start: u64,
    /// The claim already has a seat lock.
    pub locked: bool,
    /// The claim already has its commitment.
    pub committed: bool,
    /// The producer's free collateral for the deposit.
    pub free_collateral: u128,
    pub now: u64,
}

/// **The commitment rule**, pure: the producer's own, the claim's first, strictly before the seal window (so before any seat lock and
/// before the seed exists), with the deposit free. Nothing about the opening is (or can be) judged here.
pub fn palw_paid_trap_commit_admits_v1(i: &PalwPaidTrapCommitInputV1) -> Result<(), &'static str> {
    if i.signer != i.producer {
        return Err("paid trap commitment: only the paid claim's producer commits");
    }
    if i.committed {
        return Err("paid trap commitment: the claim already carries its one commitment");
    }
    if i.locked || i.now >= i.seal_start {
        return Err("paid trap commitment: posted at or after the claim's beacon seal window (or its seat lock)");
    }
    if i.free_collateral < u128::from(PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1) {
        return Err("paid trap commitment: the deposit is not free on the producer's bond");
    }
    Ok(())
}

/// **Everything a paid-trap reveal is judged on**, gathered from the state by the fold (and by the acceptance walk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPaidTrapRevealInputV1 {
    pub tiles: u64,
    pub fault_leaf: u64,
    /// `Some((producer, committed_daa))` of the claim's commitment when the reveal opens it; `None`: no commitment, or the reveal does
    /// not open it (another claim, tile, tile count or salt).
    pub trap: Option<(PalwBondKeyV2, u64)>,
    /// The reveal's own setter.
    pub setter: PalwBondKeyV2,
    /// The paid claim's producer (its enrollment authority's).
    pub producer: PalwBondKeyV2,
    pub interactive: bool,
    /// The paid claim's seat lock DAA (`None`: not locked).
    pub lock_daa: Option<u64>,
    /// [`palw_paid_trap_counts_v1`] over the lock's seed, the enrollment and the reveal's salt (`false` without a lock).
    pub counts: bool,
    /// The paid claim's signed receipt window end.
    pub checking_until: u64,
    /// [`palw_paid_trap_reveal_end_v1`] of the paid claim's signed lifetime.
    pub reveal_end: u64,
    /// The court convicted the paid claim (the kernel row's own verdict).
    pub convicted: bool,
    pub now: u64,
}

/// **The paid-trap reveal rule**, pure (every check names itself; the first that fails refuses): a fault opening (a null opening,
/// tiles 0, needs no reveal); it opens the claim's commitment; the setter is the paid claim's producer; a non-interactive claim; the
/// commitment landed STRICTLY before the seat lock; the fault counts as a trap; revealed after the receipt window and inside the
/// reveal window; and the court convicted the claim (the fault's evidence is the court's own deterministic check, never a proof
/// carried by the reveal).
pub fn palw_paid_trap_reveal_admits_v1(i: &PalwPaidTrapRevealInputV1) -> Result<(), &'static str> {
    if i.tiles == 0 || i.tiles > crate::palw_mesh_v1::PALW_TRAP_MAX_TILES_V1 || i.fault_leaf >= i.tiles {
        return Err("paid trap: not a fault opening (a null commitment needs no reveal) or the planted tile is out of range");
    }
    let Some((committer, committed_daa)) = i.trap else {
        return Err("paid trap: the reveal opens no commitment of this claim (claim, tile, tile count or salt differ)");
    };
    if committer != i.setter || i.setter != i.producer {
        return Err("paid trap: only the paid claim's producer, who committed, can have planted its fault");
    }
    if i.interactive {
        return Err("paid trap: a trap on an interactive (staged) paid claim is not supported");
    }
    let Some(lock_daa) = i.lock_daa else { return Err("paid trap: the paid claim has no seat lock") };
    if committed_daa >= lock_daa {
        return Err("paid trap: the commitment landed at or after the paid claim's seat lock");
    }
    if !i.counts {
        return Err("paid trap: the claim's beacon seed did not select this fault as a trap");
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
    /// (whose reveal object it reuses) and the once-only settlement registry (which settles every fee, forfeiture and liability
    /// it moves); then the measured-value gate over its UNKNOWN policy (records EMPTY: dormant).
    pub fn validate_palw_paid_claim_trap_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_paid_claim_trap_v1.filter(|at| *at != ForkActivation::never()) else { return Ok(()) };
        let at_or_below =
            |a: Option<ForkActivation>| a.is_some_and(|a| a != ForkActivation::never() && a.daa_score() <= at.daa_score());
        if !at_or_below(self.palw_claim_challenge_v2.as_ref().map(|f| f.activation)) {
            return Err(PalwModeV2Error::Invalid("palw_paid_claim_trap_v1 requires palw_claim_challenge_v2 at or below it"));
        }
        if !at_or_below(self.palw_audit_mesh_v1) {
            return Err(PalwModeV2Error::Invalid(
                "palw_paid_claim_trap_v1 requires palw_audit_mesh_v1 (its reveal object) at or below it",
            ));
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
            trap: Some((bond(1), 100)),
            setter: bond(1),
            producer: bond(1),
            interactive: false,
            lock_daa: Some(101),
            counts: true,
            checking_until: 200,
            reveal_end: 200 + PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1,
            convicted: true,
            now: 201,
        }
    }

    fn commit() -> PalwPaidTrapCommitInputV1 {
        PalwPaidTrapCommitInputV1 {
            signer: bond(1),
            producer: bond(1),
            seal_start: 100,
            locked: false,
            committed: false,
            free_collateral: u128::from(PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1),
            now: 99,
        }
    }

    #[test]
    fn a_commitment_is_the_producers_first_and_lands_before_the_seal_window_and_the_lock() {
        palw_paid_trap_commit_admits_v1(&commit()).unwrap();
        assert!(palw_paid_trap_commit_admits_v1(&PalwPaidTrapCommitInputV1 { now: 100, ..commit() }).is_err(), "at the seal window");
        assert!(palw_paid_trap_commit_admits_v1(&PalwPaidTrapCommitInputV1 { locked: true, ..commit() }).is_err(), "after the lock");
        assert!(palw_paid_trap_commit_admits_v1(&PalwPaidTrapCommitInputV1 { committed: true, ..commit() }).is_err(), "a second one");
        assert!(
            palw_paid_trap_commit_admits_v1(&PalwPaidTrapCommitInputV1 { signer: bond(2), ..commit() }).is_err(),
            "not the producer"
        );
    }

    #[test]
    fn trap_and_null_commitments_have_one_shape_and_only_a_fault_opening_is_a_reveal() {
        let base = [7; 64];
        let trap = palw_paid_trap_commitment_v1(&base, Some((3, 8)), &[1; 32]);
        let null = palw_paid_trap_commitment_v1(&base, None, &[1; 32]);
        assert_eq!(trap.len(), null.len());
        assert_ne!(trap, null);
        // The rows carrying them encode to the same length, field for field.
        let row = |commitment| PalwPaidTrapCommitmentRowV1 {
            version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
            enrollment: [2; 64],
            base_claim: base,
            producer: bond(1),
            commitment,
            committed_daa: 50,
            deposit: PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1,
            forget_after_daa: 440,
        };
        assert_eq!(borsh::to_vec(&row(trap)).unwrap().len(), borsh::to_vec(&row(null)).unwrap().len());
        // A null opening (tiles 0) is no reveal.
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { tiles: 0, fault_leaf: 0, ..input() }).is_err());
    }

    #[test]
    fn a_trap_committed_before_the_lock_is_admitted_and_at_or_after_it_refused() {
        palw_paid_trap_reveal_admits_v1(&input()).unwrap();
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: Some((bond(1), 101)), ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: Some((bond(1), 150)), ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { lock_daa: None, ..input() }).is_err());
    }

    #[test]
    fn a_reveal_that_does_not_open_its_commitment_or_does_not_count_is_refused() {
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: None, ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { trap: Some((bond(2), 100)), ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { producer: bond(3), ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { counts: false, ..input() }).is_err());
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { fault_leaf: 8, ..input() }).is_err());
    }

    #[test]
    fn a_reveal_needs_the_receipt_window_closed_its_own_window_open_and_the_courts_conviction() {
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { now: 200, ..input() }).is_err(), "inside receipts");
        let end = 200 + PALW_PAID_CLAIM_TRAP_REVEAL_WINDOW_DAA_V1;
        palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { now: end, ..input() }).unwrap();
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { now: end + 1, ..input() }).is_err(), "late");
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { convicted: false, ..input() }).is_err(), "no verdict");
        assert!(palw_paid_trap_reveal_admits_v1(&PalwPaidTrapRevealInputV1 { interactive: true, ..input() }).is_err());
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
    }

    #[test]
    fn the_forfeiture_splits_a_fee_exactly_and_counting_needs_the_secret_salt() {
        let (lost, kept) = palw_paid_trap_forfeit_v1(1_000);
        assert_eq!(lost + kept, 1_000);
        assert_eq!(lost, 1_000 * PALW_PAID_CLAIM_TRAP_CHECK_FEE_FORFEIT_PERMILLE_V1 / 1_000);
        assert_eq!(palw_paid_trap_forfeit_v1(0), (0, 0));
        // Deterministic in (seed, enrollment, salt); over many salts the share that counts is near the rate.
        assert_eq!(palw_paid_trap_counts_v1(&[1; 64], &[2; 64], &[3; 32]), palw_paid_trap_counts_v1(&[1; 64], &[2; 64], &[3; 32]));
        let salt_of = |i: u64| {
            let mut salt = [0u8; 32];
            salt[..8].copy_from_slice(&i.to_le_bytes());
            salt
        };
        let trials = 10_240u64;
        let hits = (0..trials).filter(|i| palw_paid_trap_counts_v1(&[1; 64], &[2; 64], &salt_of(*i))).count() as u64;
        let expected = trials * PALW_PAID_CLAIM_TRAP_RATE_BP_V1 / 10_000;
        assert!(hits.abs_diff(expected) <= expected / 2 + 10, "{hits} counted of {trials}, expected about {expected}");
    }

    #[test]
    fn the_rows_keys_have_their_audited_lengths() {
        assert_eq!(palw_paid_trap_conviction_key_v1(&[1; 64]).len(), 65);
        assert_eq!(palw_paid_trap_commitment_key_v1(&[1; 64]).len(), 65);
        assert_ne!(palw_paid_trap_commitment_key_v1(&[1; 64]), palw_paid_trap_conviction_key_v1(&[1; 64]));
        assert_eq!(palw_paid_trap_held_fee_key_v1(&[1; 64], &[2; 64], 3).len(), 137);
        assert!(
            palw_paid_trap_held_fee_key_v1(&[1; 64], &[2; 64], 3).starts_with(&palw_paid_trap_held_fee_prefix_v1(&[1; 64], &[2; 64]))
        );
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
