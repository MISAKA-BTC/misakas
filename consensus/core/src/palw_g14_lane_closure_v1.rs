//! **G14 lane closure (2026-10-16): the kernel route / OPV, the EXEC slice and the native funded (v11 / v29 / v39) lanes** — the rules
//! that close the gaps the G14 completeness audit found in these three dormant lanes
//! (`docs/design/palw/g14-completeness-proof-2026-10-16.md` §3.4–§3.6, §4.2; IDs G-K1, G-X1, G-N1, G-N3, G-14 and the collateral
//! depletion class). The user's claim: when the producer and every Panel seat collude, one public bonded verifier outside the Panel
//! still reaches an objective conviction from public authenticated material alone, and that conviction voids the claim, slashes the
//! producer and every bond that signed Valid, and refuses block production, reward and credit.
//!
//! ```text
//! G-K1  kernel route   a seat's counted PASSING receipt locks a signer liability on the seat's FREE collateral (aux 143, 0x01) —
//!                      counted in V2's committed ledger, so the bond cannot exit while it is held — from the receipt to the claim's
//!                      liability horizon; a conviction (pre- or post-Final, any batch) slashes every lock of the claim (once: the
//!                      row is the marker); the horizon or a timeout releases it. A receipt the seat cannot back is not counted.
//! G-X1  EXEC           the REAL root's own licensed piece (its prefix claim) proven false charges every V2 Valid signer of the REAL
//!                      claim S4 on its lock, through the same funnel as every other covering-signer charge (once per seat and claim).
//! G-N1  native         an unrevealed owner / seal that has passed its horizon frees its prosecution slot.
//! G-N3  native         the finder's share of a collected slash is at most ADR-0032's reporter share (490‰): ≥ 51 % always burns.
//! G-14  native         a conviction is never refused for short collateral: the claim is voided, the requester refunded from its own
//!                      escrow, and whatever remains of the producer's reserve is slashed (collected, never the promised amount).
//! ```
//!
//! **Fences.** Every rule here is in force exactly where its lane's own fence is in force — `palw_probabilistic_constraints_v1`
//! (kernel route), `palw_exec_payload_v2` (EXEC), `palw_funded_execution_v11` / `palw_native_final_v27` / `palw_native_execution_v39`
//! (native). Each is `None` on every preset and refused by its own validation, so no live network (testnet-12 included) runs any of
//! this and no params identity moves (no new `Params` field). The rules hold with those fences at DAA 0 (G14 from genesis, user
//! decision 2026-10-16): none reads pre-fence history. New rows live in a new kernel-route aux table (143) that no fold writes below
//! its lane's fence, so its existence is its own gate.
//!
//! **Values.** Nothing here is a measured value. The signer lock is the existing seat-lock relation
//! ([`crate::palw_economic_safety_v1::palw_seat_lock_required_v2`], ADR-0151 D1/D4: the colluding quorum's locks out-value the claim's
//! gain by the margin) over the route's own claim reward and interim quorum; the finder cap is ADR-0032's reporter share
//! ([`crate::palw_kernel_route_v1::PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1`]). The lanes' arming refusals keep every interim value
//! unarmed.
//!
//! No cryptographic computation proof of any kind is used (PRINCIPLES §8): a conviction is the lane's existing deterministic court;
//! the rows are data identity and bookkeeping.
use borsh::{BorshDeserialize, BorshSerialize};

use crate::palw_state_v2::PalwBondKeyV2;

/// The kernel route aux table of the lane-closure rows (allocated 2026-10-16: aux 143).
pub const PALW_G14_LANE_CLOSURE_TABLE_V1: u8 = 143;
/// `0x01 ‖ seat kernel bond ‖ claim` (129 bytes): a kernel route signer's liability lock ([`PalwKernelSignerLockRowV1`]). Keyed by
/// the seat first, so a bond's held locks are one range.
pub const PALW_KERNEL_SIGNER_LOCK_PREFIX_V1: u8 = 0x01;
/// `0x02 ‖ claim` (65 bytes): G-K4 — the first block that served material of a segmented (K2-TIR-v4) claim
/// ([`PalwServedMaterialRowV1`]). Served segment parts are kept only in block bodies, so pruning may not pass that block while the
/// claim can still be convicted.
pub const PALW_SERVED_MATERIAL_PREFIX_V1: u8 = 0x02;
/// The rows' version.
pub const PALW_G14_LANE_CLOSURE_ROW_VERSION_V1: u8 = 1;

/// **G-N3: the most a native finder is paid of a collected slash, permille** — ADR-0032's PALW reporter share (49 %), the route's one
/// reporter rate. A finder that is the producer's own Sybil recovers at most this; the rest (≥ 51 %) is burned.
pub const PALW_NATIVE_FINDER_SHARE_CAP_PERMILLE_V1: u16 = crate::palw_kernel_route_v1::PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1;

/// **A kernel route signer's liability lock** (aux 143, `0x01 ‖ seat ‖ claim`): written when the seat's passing receipt is counted,
/// held against the seat's collateral (V2's committed ledger reads it), slashed at the claim's conviction, released at its horizon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelSignerLockRowV1 {
    pub version: u8,
    pub claim: [u8; 64],
    pub seat: PalwBondKeyV2,
    pub amount: u64,
    pub locked_daa: u64,
}

/// **G-K4: where a segmented claim's served material lives** (aux 143, `0x02 ‖ claim`): the blue score and DAA of the first block that
/// carried a response to one of its demands. The material is in that block's body and later ones, never in the ledger; the pruning
/// point may not pass `first_blue_score` while the claim is still convictable (until its liability horizon), so a verifier that joins
/// late can still read every served part a conviction rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwServedMaterialRowV1 {
    pub version: u8,
    pub claim: [u8; 64],
    pub first_blue_score: u64,
    pub first_daa: u64,
}

/// The key of `claim`'s served-material row (65 bytes).
pub fn palw_served_material_key_v1(claim: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(PALW_SERVED_MATERIAL_PREFIX_V1);
    key.extend_from_slice(claim);
    key
}

/// The key of `seat`'s lock on `claim` (129 bytes).
pub fn palw_kernel_signer_lock_key_v1(seat: &[u8; 64], claim: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(129);
    key.push(PALW_KERNEL_SIGNER_LOCK_PREFIX_V1);
    key.extend_from_slice(seat);
    key.extend_from_slice(claim);
    key
}

/// The prefix of every lock `seat` holds (65 bytes).
pub fn palw_kernel_signer_lock_seat_prefix_v1(seat: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(PALW_KERNEL_SIGNER_LOCK_PREFIX_V1);
    key.extend_from_slice(seat);
    key
}

/// **G-K1: what one passing receipt locks** — the seat-lock relation (ADR-0151 D1/D4) over the claim's gain (the route's claim reward:
/// a Panel-licensed kernel claim earns nothing else) and the assignment's quorum: the quorum's locks together out-value the gain.
pub fn palw_kernel_signer_lock_amount_v1(claim_reward: u64, quorum: u8) -> u64 {
    let lock = crate::palw_economic_safety_v1::palw_seat_lock_required_v2(u128::from(claim_reward), u64::from(quorum.max(1)));
    u64::try_from(lock).unwrap_or(u64::MAX)
}

/// The fate of a signer lock at a closing tick, from the kernel ledger's claim row (`None`: the row is gone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwKernelSignerLockFateV1 {
    /// The claim can still be convicted: keep the lock.
    Hold,
    /// The claim is convicted: charge the lock (a conviction batch normally charged it already; this is the sweep's safety net).
    Charge,
    /// The claim can no longer be convicted (timed out, gone, or past its liability horizon unconvicted): release it.
    Release,
}

/// **When a lock ends** — exactly when the claim's own reservation would: a conviction charges it; a timeout or a missing row releases
/// it; a Final or defaulted claim holds it through `liability_until` (the window a post-Final proof may still convict in).
pub fn palw_kernel_signer_lock_fate_v1(
    row: Option<&misaka_palw_kernel::ledger::ClaimRowV1>,
    now_daa: u64,
) -> PalwKernelSignerLockFateV1 {
    match row {
        None => PalwKernelSignerLockFateV1::Release,
        Some(row) => palw_kernel_signer_lock_fate_of_v1(row.convicted, &row.life.state, row.liability_until, now_daa),
    }
}

/// [`palw_kernel_signer_lock_fate_v1`] on the row's three facts.
pub fn palw_kernel_signer_lock_fate_of_v1(
    convicted: bool,
    state: &misaka_palw_kernel::lifecycle::ClaimStateV1,
    liability_until: Option<u64>,
    now_daa: u64,
) -> PalwKernelSignerLockFateV1 {
    use misaka_palw_kernel::lifecycle::ClaimStateV1 as S;
    if convicted {
        return PalwKernelSignerLockFateV1::Charge;
    }
    match state {
        S::TimedOut { .. } => PalwKernelSignerLockFateV1::Release,
        S::Final { .. } if liability_until.is_some_and(|until| now_daa > until) => PalwKernelSignerLockFateV1::Release,
        S::Unavailable { .. } if liability_until.is_none_or(|until| now_daa > until) => PalwKernelSignerLockFateV1::Release,
        _ => PalwKernelSignerLockFateV1::Hold,
    }
}

/// **G-N3: is a native policy's finder share within the cap?** (`≤ 490‰`).
pub fn palw_native_finder_share_within_cap_v1(finder_permille: u16) -> bool {
    finder_permille <= PALW_NATIVE_FINDER_SHARE_CAP_PERMILLE_V1
}

/// **G-14: what a native conviction collects** — the producer's reservation, capped at the collateral the bond still has. A bond
/// another lane drained can no longer pay the whole reservation; the conviction still voids the claim and refunds the requester (from
/// the requester's own escrow, never from the slash) and takes what is left.
pub fn palw_native_collectible_slash_v1(reserved: u64, collateral: u64) -> u64 {
    reserved.min(collateral)
}

/// **G-14 with G-N3: the finder's credit of a (possibly partial) collection** — the funded finder award scaled by what was actually
/// collected of the reserve: exactly the award on a full collection, never more than was collected, so the finder is only ever paid
/// out of a real slash and the burned remainder keeps the award's share.
pub fn palw_native_scaled_finder_award_v1(finder_award: u64, reserve: u64, collected: u64) -> u64 {
    if reserve == 0 {
        return 0;
    }
    let scaled = u128::from(finder_award) * u128::from(collected.min(reserve)) / u128::from(reserve);
    u64::try_from(scaled).unwrap_or(u64::MAX).min(collected)
}

/// **G-14: the producer's collateral a native conviction can still collect** (`0` for a bond record that is gone).
pub fn palw_native_producer_collectible_v1(bonds_collateral: Option<u64>, reserve: u64) -> u64 {
    palw_native_collectible_slash_v1(reserve, bonds_collateral.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g14_lane_closure_values_come_from_documented_relations() {
        assert_eq!(PALW_NATIVE_FINDER_SHARE_CAP_PERMILLE_V1, 490, "ADR-0032's reporter share");
        assert!(palw_native_finder_share_within_cap_v1(490));
        assert!(!palw_native_finder_share_within_cap_v1(491));
        assert!(!palw_native_finder_share_within_cap_v1(1000), "a Sybil finder never recovers a whole self-conviction");
        let reward = 5 * crate::constants::SOMPI_PER_KASPA;
        let lock = palw_kernel_signer_lock_amount_v1(reward, 3);
        assert!(u128::from(lock) * 3 > u128::from(reward), "the colluding quorum out-values the gain");
        assert_eq!(u128::from(lock), crate::palw_economic_safety_v1::palw_seat_lock_required_v2(u128::from(reward), 3));
        assert_eq!(palw_native_collectible_slash_v1(100, 40), 40);
        assert_eq!(palw_native_collectible_slash_v1(100, 400), 100);
        assert_eq!(palw_native_scaled_finder_award_v1(49, 100, 100), 49, "a full collection pays the whole award");
        assert_eq!(palw_native_scaled_finder_award_v1(49, 100, 50), 24, "a half collection, half the award (floored)");
        assert_eq!(palw_native_scaled_finder_award_v1(49, 100, 0), 0);
        assert_eq!(palw_native_producer_collectible_v1(None, 100), 0, "a bond that is gone collects nothing");
        assert_eq!(palw_kernel_signer_lock_key_v1(&[1; 64], &[2; 64]).len(), 129);
        assert!(palw_kernel_signer_lock_key_v1(&[1; 64], &[2; 64]).starts_with(&palw_kernel_signer_lock_seat_prefix_v1(&[1; 64])));
    }

    /// **G-X1's exit hold**: a REAL root's V2 `Valid` lock is re-dated at the root's `Final`, which waits for its prefix claim's
    /// kernel `Final`; it must outlive that prefix claim's liability horizon (`Final + liability_daa`) or a post-Final conviction of the
    /// prefix would find no lock to charge. The interim route policy keeps that relation; a re-pricing must keep it too.
    #[test]
    fn g14_lane_closure_exec_v2_signer_locks_outlive_the_prefix_liability_horizon() {
        let policy = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(crate::Hash64::default(), crate::Hash64::default());
        assert!(
            policy.liability_daa <= crate::palw_state_v2::PALW_FINAL_LOCK_LIFE_DAA_V1,
            "the Final-dated lock outlives the horizon"
        );
    }

    #[test]
    fn g14_lane_closure_a_signer_lock_lives_to_the_claims_liability_horizon() {
        use misaka_palw_kernel::lifecycle::ClaimStateV1 as S;
        let fate = palw_kernel_signer_lock_fate_of_v1;
        assert_eq!(palw_kernel_signer_lock_fate_v1(None, 5), PalwKernelSignerLockFateV1::Release, "a row that is gone");
        let live = S::ProbabilisticPass { passed_daa: 20, window_end_daa: 70 };
        assert_eq!(fate(false, &live, None, 50), PalwKernelSignerLockFateV1::Hold);
        let fin = S::Final { final_daa: 100 };
        assert_eq!(fate(false, &fin, Some(300), 300), PalwKernelSignerLockFateV1::Hold, "inside the horizon");
        assert_eq!(fate(false, &fin, Some(300), 301), PalwKernelSignerLockFateV1::Release);
        assert_eq!(fate(true, &fin, Some(300), 301), PalwKernelSignerLockFateV1::Charge);
        assert_eq!(fate(false, &S::TimedOut { daa: 120 }, None, 121), PalwKernelSignerLockFateV1::Release);
        let defaulted = S::Unavailable { daa: 80, producer_defaulted: true };
        assert_eq!(fate(false, &defaulted, Some(280), 200), PalwKernelSignerLockFateV1::Hold, "a default never erases a fraud");
        assert_eq!(fate(false, &defaulted, Some(280), 281), PalwKernelSignerLockFateV1::Release);
    }
}
