//! **ADR-0160 lane N — the network level and the work-conserving fair share** (testnet-12 only, behind the
//! dormant `Params::palw_capacity_network_room`, F-N; rcore/cap-s1 stage 4; ADR-0160 v3 §7).
//!
//! **What it adds.** F-R prices each class's room; lane S caps each bond's outstanding claims and rate.
//! Neither sees the NETWORK: how many claims the whole chain can bind, carry and license at once, and who
//! among the bonds that want more is owed its part of it. Lane N is that level and that share:
//!
//! ```text
//! L_net    = min(L_seat, L_carry, L_anchor)
//!   L_seat   = the seats' room for unlicensed claims: the floor's J-6 pipeline plus its bound unlicensed
//!              claims, plus every model class's share room (F-R's rooms, on U's basis)
//!   L_carry  = H_L × LPB × max(1, ā_op)                 licences the carriage lands over H_L DAA
//!   L_anchor = B_bind × max(1, ā_op) × anchor_delay      binds the operator anchors make over an anchor delay
//!   ā_op     = operator-attempt chain blocks a DAA over the last 32 DAA (a rooted ring)
//! units_a  = ⌊C_a / C_min⌋, C_min = 13,000 MSK
//! Reg(t)   = { a : network_demand[(a, ·)] ≥ t }   rooted (bond, class) → until_daa, t + H_L at a refusal
//! Act(t)   = Reg(t) ∪ { a : a holds an unlicensed claim }
//! share_a  = ⌊L_net × units_a / Σ_{x ∈ Act ∪ {b}} units_x⌋
//! d_a      = max(0, share_a − held_a)                     what a registered bond is owed
//! owed¬b   = Σ_{a ∈ Reg, a ≠ b} d_a
//! free     = L_net − U                                    (U: unlicensed claims network-wide)
//! admits(b) ⟺ free ≥ 1 ∧ ( free − owed¬b ≥ 1  ∨  b ∈ Reg ∧ d_b ≥ 1 ∧ b is the most owed of Reg )
//! ```
//!
//! * **Work-conserving**: with nothing owed, any bond takes every free unit; units are withheld only while
//!   a registered bond is owed them.
//! * **The honest share**: a registered bond below its share is owed the difference; nobody else takes a
//!   freed unit while it stands (unless free units exceed everything owed), and the most-owed registered
//!   bond takes a unit whenever one is free — so registered deficits are served largest first, and no unit
//!   idles between two owed bonds.
//! * **Split-neutral, for every state and every order of asks** (the user's stage-4 property, "13k × 10
//!   bonds never beats 130k × 1"): a bond's own share never admits it past what the others are owed —
//!   only the most-owed path does, and only a REGISTERED bond's deficit — and the shares divide over every
//!   bond that holds a claim or is registered, so pieces that hold or are refused see each other. Pieces
//!   hold `Σ⌊L·u_i/Σ⌋ ≤ ⌊L·Σu_i/Σ⌋` and face at least the others' deficits their whole faces
//!   (`palw_capacity_stage4_network::n_ten_small_bonds_never_beat_one_bond_of_their_total_on_the_rule`:
//!   every random state and order).
//!
//! **Deviations from ADR-0160 v3 §7.2**, each forced by that property (measured on the rule, 30,000
//! random states with adversarial orders): the ADR's `held_b < max(share_b, 1)` path over `Reg ∪ {b}`
//! let ten 13k pieces take up to 76 claims more than their 130k whole — a piece that is neither
//! registered nor holding is invisible to the others' shares, so the first piece takes the units nobody
//! registered is owed and every later piece then takes its own share out of the registered bonds'
//! reservations (and the `max(·, 1)` gave each piece a unit its whole does not get). Here the shares count
//! holders, a bond's own share is not a licence to take reserved units, and the one-unit slack is gone; a
//! bond whose share rounds to 0 is guaranteed nothing while others are owed units.
//! Further: `L_anchor` floors `ā_op` at one block a DAA like `L_carry` (else the fence's empty ring, or
//! an operator pause, refuses every admission); `L_seat` is on `U`'s basis (above).
//!
//! **What "never beats" does not cover**: the property is about capacity — what a state lets the pieces
//! take. Under a schedule with releases a registered bond's reserved units idle until it asks, which moves
//! the others' admissions in time; over random schedules the pieces came out ahead in about 1 % of runs by
//! at most a few claims (the ADR's rule: more often and by more). Only a rule with no reservation (first
//! come) is neutral under every schedule, and it protects nobody's share.
//!
//! **Past F-N, F-R's per-bond share of a class room is this rule too** (`PalwFoldReadV1::class_share_n_v1`):
//! F-R's `room_cap_v1` gives every bond one slack unit and draws the floor's racing headroom one unit a
//! bond, so ten pieces take ten units past their shares where their whole takes one. The class room (the
//! floor's J-6 pipeline, a model class's `min(c_v2, ρ·⌈c_ship/2⌉)`) is divided by
//! [`palw_network_share_admits_v1`] over the bonds holding or registered for THAT class; the floor's racing
//! headroom stays out of the shares and open first come. Registration is keyed by (bond, class): lane N's
//! refusal and, past F-N, the class room's (`FloorRoomExhausted`, `BondClassShareExceeded`) set it.
//!
//! **Queue bound, never safety**: nothing in the liability, the weight cap or the audit door reads it.

use crate::palw_state_v2::PalwBondKeyV2;
use std::collections::BTreeMap;

/// `C_min` — one issuance unit of lane N (13,000 MSK, the producer floor's scale).
pub const PALW_NETWORK_UNIT_SOMPI_V1: u64 = 13_000 * 100_000_000;
/// `H_L` — the registration horizon and the carriage horizon, in DAA.
pub const PALW_NETWORK_H_L_DAA_V1: u64 = 21;
/// The operator-attempt ring's span, in DAA.
pub const PALW_NETWORK_RING_DAA_V1: u64 = 32;
/// `B_bind` — binds one operator anchor makes (V4's bind-cost run sets it; 100 until measured).
pub const PALW_NETWORK_B_BIND_V1: u64 = 100;
/// `LPB` — licences a block carries: 3 without F-B, 64 with F-B's path form on eight genesis seats.
pub const PALW_NETWORK_LPB_SINGLE_V1: u64 = 3;
pub const PALW_NETWORK_LPB_BATCH_V1: u64 = 64;

/// **`ā_op` in milli-blocks a DAA**: the ring's operator-attempt chain blocks over the last
/// [`PALW_NETWORK_RING_DAA_V1`] DAA before `now_daa` (inclusive of `now_daa`), ×1,000 / 32.
pub fn palw_network_a_op_milli_v1(ring: &BTreeMap<u64, u32>, now_daa: u64) -> u64 {
    let from = now_daa.saturating_sub(PALW_NETWORK_RING_DAA_V1 - 1);
    let total: u64 = ring.range(from..=now_daa).map(|(_, n)| u64::from(*n)).sum();
    total.saturating_mul(1_000) / PALW_NETWORK_RING_DAA_V1
}

/// **`L_net`** from its three terms: `L_seat` (the seats' room, claims), and `ā_op` (milli-blocks a
/// DAA), the licence carriage per block and the anchor delay.
pub fn palw_network_level_v1(l_seat: u64, a_op_milli: u64, lpb: u64, anchor_delay: u64) -> u64 {
    let b = a_op_milli.max(1_000);
    let l_carry = PALW_NETWORK_H_L_DAA_V1.saturating_mul(lpb).saturating_mul(b) / 1_000;
    let l_anchor = PALW_NETWORK_B_BIND_V1.saturating_mul(b).saturating_mul(anchor_delay.max(1)) / 1_000;
    l_seat.min(l_carry).min(l_anchor)
}

/// `units = ⌊C / C_min⌋`.
pub fn palw_network_units_v1(collateral_sompi: u64) -> u64 {
    collateral_sompi / PALW_NETWORK_UNIT_SOMPI_V1
}

/// **Does `claim` count in `U`** — waiting for its licence: `Provisional` or `PanelBound`, through a
/// `DefaultDisputed` to the phase it resumes (a DA accusation on a licensed claim does not make it
/// unlicensed again). The consensus copy of the shadow's `palw_capacity_is_unlicensed_v1` (S-I1: no
/// consensus rule calls the capacity formulas).
pub fn palw_network_is_unlicensed_v1(claim: &crate::palw_state_v2::PalwClaimStateV2) -> bool {
    use crate::palw_state_v2::PalwClaimPhaseV2 as P;
    fn waiting(phase: &P) -> bool {
        match phase {
            P::Provisional | P::PanelBound { .. } => true,
            P::DefaultDisputed { resumed, .. } => waiting(resumed),
            P::ReceiptLicensed { .. } | P::Final { .. } | P::Voided { .. } => false,
        }
    }
    waiting(&claim.phase)
}

/// One bond other than the asker that the share reads: its units, what it holds against the level, and
/// whether its demand is registered (a registered bond is owed its deficit; a holder only divides).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwNetworkBondV1 {
    pub units: u64,
    pub held: u64,
    pub registered: bool,
}

/// Why lane N refuses an admission (`NetworkRoomExhausted`, non-fatal for the block's own attempt).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwNetworkRefusalV1 {
    pub level: u64,
    pub unlicensed: u64,
    pub held: u64,
    pub share: u64,
    /// `owed¬b`: what the registered bonds other than the asker are owed.
    pub deficit_others: u64,
}

/// **The admission rule** over one network reading: `level` (`L_net`), `unlicensed` (`U`), the asking
/// bond `b` (`units_b`, `held_b`, whether it is registered) and `others` — every OTHER bond that holds
/// against the level or is registered. `Ok` or the refusal's numbers.
pub fn palw_network_admits_v1(
    level: u64,
    unlicensed: u64,
    b: &PalwBondKeyV2,
    units_b: u64,
    held_b: u64,
    b_registered: bool,
    others: &BTreeMap<PalwBondKeyV2, PalwNetworkBondV1>,
) -> Result<(), PalwNetworkRefusalV1> {
    palw_network_share_admits_v1(level, level, unlicensed, b, units_b, held_b, b_registered, others)
}

/// **The same rule with the shares dividing `share_level ≤ level`** — a class room past F-N, whose
/// racing headroom (`level − share_level`) stays out of the shares and open first come. `free` is
/// `level − unlicensed`; the shares, the deficits and the refusal's `level` read `share_level`.
#[allow(clippy::too_many_arguments)]
pub fn palw_network_share_admits_v1(
    level: u64,
    share_level: u64,
    unlicensed: u64,
    b: &PalwBondKeyV2,
    units_b: u64,
    held_b: u64,
    b_registered: bool,
    others: &BTreeMap<PalwBondKeyV2, PalwNetworkBondV1>,
) -> Result<(), PalwNetworkRefusalV1> {
    let free = level.saturating_sub(unlicensed);
    let shared = share_level.min(level);
    let others = || others.iter().filter(|(key, bond)| *key != b && (bond.registered || bond.held > 0));
    let units_total: u64 = others().map(|(_, bond)| bond.units).fold(units_b, u64::saturating_add);
    let share = |units: u64| -> u64 {
        if units_total == 0 {
            0
        } else {
            ((u128::from(shared) * u128::from(units)) / u128::from(units_total)).min(u128::from(u64::MAX)) as u64
        }
    };
    let owed_to = |bond: &PalwNetworkBondV1| share(bond.units).saturating_sub(bond.held);
    let owed: u64 = others().filter(|(_, bond)| bond.registered).map(|(_, bond)| owed_to(bond)).fold(0u64, u64::saturating_add);
    let share_b = share(units_b);
    let refusal = PalwNetworkRefusalV1 { level: shared, unlicensed, held: held_b, share: share_b, deficit_others: owed };
    if free < 1 {
        return Err(refusal);
    }
    if free.saturating_sub(owed) >= 1 {
        return Ok(());
    }
    // The most-owed registered bond takes a free unit (ties to the greater key), so no unit idles between
    // two bonds each owed more than it.
    let owed_b = share_b.saturating_sub(held_b);
    let most_owed = b_registered
        && owed_b >= 1
        && others().filter(|(_, bond)| bond.registered).all(|(key, bond)| (owed_to(bond), key) < (owed_b, b));
    if most_owed { Ok(()) } else { Err(refusal) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([n; 64]), 0))
    }

    fn registered(units: u64, held: u64) -> PalwNetworkBondV1 {
        PalwNetworkBondV1 { units, held, registered: true }
    }

    fn holder(units: u64, held: u64) -> PalwNetworkBondV1 {
        PalwNetworkBondV1 { units, held, registered: false }
    }

    #[test]
    fn the_level_is_the_least_of_seat_carriage_and_anchors() {
        // ā_op 2.6 (the live rate): carriage 21 × 64 × 2.6 = 3,494; anchors 100 × 2.6 × 20 = 5,200.
        assert_eq!(palw_network_level_v1(10_000, 2_600, PALW_NETWORK_LPB_BATCH_V1, 20), 3_494);
        assert_eq!(palw_network_level_v1(900, 2_600, PALW_NETWORK_LPB_BATCH_V1, 20), 900, "the seats bind");
        assert_eq!(palw_network_level_v1(10_000, 0, PALW_NETWORK_LPB_SINGLE_V1, 20), 63, "no operator block: ā_op counts 1");
        let mut ring = BTreeMap::new();
        ring.insert(100, 3u32);
        ring.insert(90, 2u32);
        ring.insert(60, 99u32);
        assert_eq!(palw_network_a_op_milli_v1(&ring, 100), 5_000 / 32, "the last 32 DAA only");
    }

    /// Work-conserving: with nothing owed a bond takes every free unit; a registered bond below its share
    /// is owed the difference, and a bond past its share is refused while it stands.
    #[test]
    fn the_share_is_work_conserving_and_honours_registered_demand() {
        let none = BTreeMap::new();
        assert!(palw_network_admits_v1(100, 99, &bond(1), 1, 99, false, &none).is_ok(), "alone: the whole level");
        assert!(palw_network_admits_v1(100, 100, &bond(1), 1, 100, false, &none).is_err(), "free = 0 refuses");
        // Bond 2 registered with 10 units and 0 held; bond 1 (10 units) holds 90 of 100.
        let mut others = BTreeMap::new();
        others.insert(bond(2), registered(10, 0));
        let refused = palw_network_admits_v1(100, 90, &bond(1), 10, 90, false, &others).expect_err("its deficit stands");
        assert_eq!((refused.share, refused.deficit_others), (50, 50));
        let mut from_2 = BTreeMap::new();
        from_2.insert(bond(1), holder(10, 90));
        assert!(palw_network_admits_v1(100, 90, &bond(2), 10, 0, true, &from_2).is_ok(), "the owed bond takes a free unit");
        // A holder only divides: bond 1 holding 90 unregistered owes bond 3 (asking, 0 units) nothing.
        assert!(palw_network_admits_v1(100, 90, &bond(3), 0, 0, false, &from_2).is_ok());
    }

    /// Two registered bonds each owed more than the free units: the most owed takes one (no idle unit).
    #[test]
    fn the_most_owed_registered_bond_takes_a_unit_the_others_reserve() {
        // Level 20, 18 held by bond 3 (unregistered, 0 units): free 2; bonds 1 and 2 are owed 10 each.
        let mut from_1 = BTreeMap::new();
        from_1.insert(bond(2), registered(10, 0));
        from_1.insert(bond(3), holder(0, 18));
        assert!(palw_network_admits_v1(20, 18, &bond(1), 10, 0, true, &from_1).is_err(), "tie on 10: the greater key (2) first");
        let mut from_2 = BTreeMap::new();
        from_2.insert(bond(1), registered(10, 0));
        from_2.insert(bond(3), holder(0, 18));
        assert!(palw_network_admits_v1(20, 18, &bond(2), 10, 0, true, &from_2).is_ok(), "bond 2 is the most owed");
        assert!(palw_network_admits_v1(20, 18, &bond(2), 10, 9, true, &from_2).is_err(), "owed 1 < 10: bond 1 is");
        assert!(
            palw_network_admits_v1(20, 18, &bond(4), 10, 0, false, &from_2).is_err(),
            "an unregistered asker takes no reserved unit"
        );
    }

    /// A class room's headroom stays out of the shares and is first come.
    #[test]
    fn a_class_rooms_headroom_is_first_come() {
        let mut others = BTreeMap::new();
        others.insert(bond(2), registered(10, 0));
        // Room 40, headroom 24: shares divide 16. Bond 2 registered, the asker with no units: owed 16.
        let refused = palw_network_share_admits_v1(40, 16, 24, &bond(1), 0, 0, false, &others).expect_err("16 free, 16 owed");
        assert_eq!((refused.level, refused.share, refused.deficit_others), (16, 0, 16));
        assert!(palw_network_share_admits_v1(40, 16, 23, &bond(1), 0, 0, false, &others).is_ok(), "17 free, 16 owed: one first come");
    }

    /// **Split-neutral, the unit arithmetic**: ten 13k bonds hold ten units, one 130k bond ten.
    #[test]
    fn ten_small_bonds_hold_no_more_units_than_one_of_their_total() {
        let msk = 100_000_000u64;
        assert_eq!(10 * palw_network_units_v1(13_000 * msk), palw_network_units_v1(130_000 * msk));
        assert!(10 * palw_network_units_v1(19_000 * msk) <= palw_network_units_v1(190_000 * msk));
    }
}
