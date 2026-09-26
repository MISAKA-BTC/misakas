//! **ADR-0160 lane verify V2 — room v2** (`Params::palw_capacity_verify_room`, F-R; testnet-12 only,
//! dormant on every shipped preset). Pure arithmetic; the fold's readers
//! (`PalwFoldReadV1::panel_rate_v1`, `bond_class_share_v1`, `check_floor_room_v1`) dispatch here past
//! the fence and nowhere else.
//!
//! # What the class room counted, and what verification costs
//!
//! Below the fence a model class's capacity is `⌊ready × 2.4e12 × 700‰ × window / (5 × eccu)⌋` —
//! five FULL replays per claim at the ADR-0133 reference speed. Two of those terms are not what
//! Verification V2 costs:
//!
//! * **Redundancy.** A coverage licence needs every segment attested twice (the full-replay seat and
//!   the unique partial holder, `PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT = 2`), so the panel
//!   replays `2 × eccu`, not `5 × eccu` (×2.5).
//! * **Speed.** The reference (4 G MAC-eq/s) is the registry's safety constant; the seat's measured
//!   replay rate is [`PALW_CAPACITY_REPLAY_SPEED_PERMILLE_V1`] of it (a consensus constant: changing
//!   it is a new fence).
//!
//! Past the fence the room is [`palw_panel_capacity_v2`] with both, and every other class's owed
//! replay is priced the same way, so the rate rule's terms stay one unit.
//!
//! # The per-bond share (replaces T-2(a))
//!
//! T-2(a) gave every bond `⌈c/2⌉`: two bonds held the whole class, a split into many bonds bought
//! `⌈c/2⌉` each, and one bond could never use a room nobody else wanted. Past F-R a bond's room is
//! stake-proportional over the ACTIVE holders (the bonds holding unlicensed claims of the class, the
//! asking bond included):
//!
//! * its **share** [`palw_bond_room_share_v1`] `= ⌊room × C_b / ΣC⌋` — the whole room when no other
//!   bond holds one, the posted fraction under contention, none for a frozen bond (V-I5; lane liab's
//!   `palw_bond_is_frozen_v1` is the predicate at cap-int — [`palw_verify_bond_is_frozen_v1`] stands in
//!   for it here) — is GUARANTEED: the others' claims past their own shares never take it;
//! * the **rounding slack** `room − Σ shares` (less than one claim per active bond) is shared one
//!   unit a bond, first come ([`palw_bond_room_cap_v1`]): an attacker of stake fraction `s` holds at
//!   most `⌊s·room⌋ + 1`.
//!
//! **Deviation from ADR-0160 §7.4's `max(1, ⌈room × C_b / ΣC⌉)`, measured and deliberate.** A ceiling
//! with a floor of one gives every piece of a split one claim: 76 pieces of 13,000 MSK (988k) take 76
//! claims of a 58-claim room against an honest 1M bond, which then holds none (V-T4 / A4 fail). With
//! the floored share the pieces' shares sum to at most the whole bond's (`Σ⌊x_i⌋ ≤ ⌊Σx_i⌋`), a split
//! gains only the rounding slack it can win first (one unit per active piece, V-I4), and an honest
//! bond of stake fraction `s` always reaches `⌊s × room⌋` (A4). The price: a bond whose stake fraction
//! is below `1/room` holds only slack under contention.
//!
//! **Residual (stated):** the holders are the bonds holding claims now, so a bond that holds none is
//! not yet counted — an incumbent that filled an empty room refills a slot as it frees until the
//! newcomer's attempt wins one; from then on the newcomer's share is guaranteed.
//!
//! **The floor's racing headroom** ([`palw_floor_room_race_headroom_v1`]). On the floor the shares
//! divide the room less a headroom (at least 24 units, one in sixteen of a larger room, never more
//! than a quarter), and the slack is counted over the whole room, so small bonds — whose share beside
//! large holders is 0 — racing the same tip find at least that many single units instead of a
//! rounding slack smaller than the holder count. A racer the fold skips loses its withheld worker
//! carve (burned: it is unverified work), which the pre-check at its tip could not foresee; the
//! headroom is what keeps that rare. A model class's share has no headroom (its room is small, and
//! its class room is first come below F-R too).
//!
//! # The floor room (J-6)
//!
//! The floor is ungated below the fence, so a flood of floor claims fills the seats' capital and the
//! claims behind it void `BindTimeout` twenty DAA later — honest ones among them (the capacity map
//! §6.3, `K_floor_big`). Past the fence a floor claim is admitted only while the eligible seats' free
//! capital can bind every floor claim still waiting for a bind, and this one:
//! `pending + 1 ≤ ⌊Σ_seat ⌊work_room(seat) / eligibility⌋ / seat_count⌋` ([`palw_floor_room_v1`]).
//! Bound claims already hold their duty inside each seat's committed ledger, so only the claims not
//! yet bound are pending (ADR-0160 §7.4 wrote "unlicensed"; counting the bound ones there would
//! charge their duty twice). The refusal is a skip for the block's own attempt: the block stands and
//! still anchors.
//!
//! # The ×100 design (text; nothing below arms it)
//!
//! * **Carriage ×100: full-list window roots.** A seat's window is short (one DAA of its receipts); a
//!   batch that carries each window's leaf LIST instead of a path per receipt pays ≈ 0.5k transient
//!   mass per claim (five leaves' fields, no hashes) and recomputes each root once — ≈ 700 claims a
//!   500k block beside the roots, against ≈ 30 with paths. The fold's funnel is the same; only the
//!   proof form moves (a `ReceiptLicensedBatchV2`, new tag, its own fence).
//! * **2M sharded replay.** S shards of the canonical job, each attested by ≥ 3 seats (a quorum per
//!   shard, so the collusion bound P is not weakened), with the K/V state committed at every shard
//!   boundary (≤ 11.6 GiB at 2M with the i16 codec) so a shard seat replays from its boundary, not
//!   from token 0. Latency falls by S; WORK does not (≈ 1,399 reference seat-spans a claim, times the
//!   attesters).
//! * **Sampled coverage licence** → `Licensed { permille = covered }` for lane weight's staged weight:
//!   a claim licensed on the shards replayed so far carries that fraction of its weight, the rest at
//!   Final.
//! * **Deep audit before maturity**: every licensed 2M claim fully replayed by a non-panel operator
//!   before its escrow matures (the V3 sampler at a = 1.0 for C7), convicting through
//!   `ExecutorRefuted` / kind 3 — which is ADR-0153's attribution and the only way q_2M > 0.
//! * **GPU arithmetic.** Throughput `≈ N_gpu / (r_v × 1,398.9)` claims a span at reference speed:
//!   ×10 over today's one per ≈ 2,900 DAA needs ≈ 9.6 reference GPUs (`r_v = 2`), ×100 ≈ 96, ×1000 ≈
//!   965. C7 stays capped at 1 and no fence here arms 2M (ADR-0153; V-I6).

use crate::palw_model_registry_v1::PalwRegistryGlobalsV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};

/// **The seats' measured replay rate, in permille of the ADR-0133 reference** (4 G MAC-eq/s per seat,
/// `PalwRegistryGlobalsV1::reference_work_per_span`). A consensus constant: past F-R the class room
/// is priced at it, so changing it is a new fence.
///
/// **What it rests on (lane verify V4, 2026-09-26):**
///
/// * the A16 engine that replays the 8k row (Qwen2.5-1.5B, the `qwen25-1.5b-a16` artifact, a release
///   build at `nice 15` on the drill host) ran a 57-token forward in 6.237 s — at the 8k row's
///   1.357e9 MAC-eq a token (`vccu` 1,390,562,722,816 over its 1,025-token canonical job) that is
///   1.24e10 MAC-eq/s, **3,100‰** of the reference;
/// * live testnet-12's 8k licences (13, DAA 140–170) landed 2–8 DAA after their bind with the
///   carriage's one DAA included: at 120 s a DAA the median case bounds every signing seat's replay at
///   ≥ 1,450‰, the fastest at ≥ 2,900‰ (a latency bound, gossip and queueing inside it).
///
/// 2,000‰ is the engine's measured rate less a third for the seat hosts not yet measured. It is a
/// fence value in all but name: Stage 0 re-measures it on every seat host, and a host slower than it
/// is a reason to lower it by a new fence before F-R arms (ADR-0160 §9, gate G2's sibling).
pub const PALW_CAPACITY_REPLAY_SPEED_PERMILLE_V1: u32 = 2_000;

/// **Attestations a claim's replay costs under Verification V2**: each segment `Valid` twice (the
/// full seat and the unique partial holder), i.e. two whole replays across the panel.
pub const PALW_CAPACITY_ATTESTERS_PER_SEGMENT_V1: u32 = crate::palw_verification_v2::PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT as u32;

/// **How the panel's replay is priced under the room rule in force** — one seat's replay a span, and
/// the whole replays one claim costs the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwReplayPricingV1 {
    /// One ready seat's replay a span at the target utilization.
    pub per_seat_per_span: u128,
    /// Whole replays a claim costs the panel: the seat count below the fence, `k = 2` past it.
    pub replicas: u128,
}

/// **The pricing below F-R** (`room_v2 = false`: the reference speed, `seat_count` full replays — the
/// rule every block before the fence is judged by, byte for byte) **or past it** (the measured speed,
/// `k = 2`).
pub fn palw_replay_pricing_v1(room_v2: bool, g: &PalwRegistryGlobalsV1) -> PalwReplayPricingV1 {
    let utilization = g.utilization_permille.min(1_000) as u128;
    if !room_v2 {
        return PalwReplayPricingV1 {
            per_seat_per_span: g.reference_work_per_span.saturating_mul(utilization) / 1_000,
            replicas: g.seat_count as u128,
        };
    }
    PalwReplayPricingV1 {
        per_seat_per_span: palw_replay_ccu_per_span_measured_v1(g).saturating_mul(utilization) / 1_000,
        replicas: PALW_CAPACITY_ATTESTERS_PER_SEGMENT_V1 as u128,
    }
}

/// One seat's measured replay a span (before utilization): the reference at
/// [`PALW_CAPACITY_REPLAY_SPEED_PERMILLE_V1`].
pub fn palw_replay_ccu_per_span_measured_v1(g: &PalwRegistryGlobalsV1) -> u128 {
    g.reference_work_per_span.saturating_mul(PALW_CAPACITY_REPLAY_SPEED_PERMILLE_V1 as u128) / 1_000
}

/// **ADR-0160 V2: a class's panel capacity** — `⌊ready_eff × per_seat × window / (eccu × k)⌋`, the
/// claims of the class the ready seats' measured replay holds over its window with no other class
/// beside it ([`crate::palw_work_target_v1::palw_panel_capacity_by_rate_v1`] with no others' term).
/// `per_seat` is one seat's replay a span at the target utilization
/// ([`PalwReplayPricingV1::per_seat_per_span`]). V-I3: never above the measured replay × window ÷
/// (attesters × eccu).
pub fn palw_panel_capacity_v2(ready_eff: u128, per_seat_per_span: u128, window_spans: u64, eccu: u128, attesters_per_segment: u32) -> u64 {
    crate::palw_work_target_v1::palw_panel_capacity_by_rate_v1(
        ready_eff.saturating_mul(per_seat_per_span),
        0,
        window_spans,
        eccu.saturating_mul(attesters_per_segment as u128),
    )
}

/// **ADR-0160 V2: a bond's guaranteed share of a class's room** — `⌊room × own / (own + others)⌋`,
/// where `others` is the posted collateral of every OTHER active holder (a bond holding an unlicensed
/// claim of the class, not frozen). The whole room when nobody else holds one; the posted-stake
/// fraction under contention, floored; zero for a frozen bond (V-I5). Floored, so the pieces of a
/// split bond never share more than the whole (V-I4).
pub fn palw_bond_room_share_v1(room: u64, own_collateral: u64, others_collateral: u128, frozen: bool) -> u64 {
    if frozen {
        return 0;
    }
    let own = own_collateral as u128;
    let total = own.saturating_add(others_collateral);
    if total == 0 || others_collateral == 0 {
        return room;
    }
    ((room as u128).saturating_mul(own) / total).min(u64::MAX as u128) as u64
}

/// One active holder of a room: its posted collateral, the claims it holds against the room, and
/// whether it is frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRoomHolderV1 {
    pub collateral: u64,
    pub held: u64,
    pub frozen: bool,
}

/// **ADR-0160 V2: how many claims `holders[me]` may hold now** — its guaranteed share
/// ([`palw_bond_room_share_v1`] over every other holder's collateral) plus at most ONE unit of the
/// rounding slack no holder has taken yet (`room − Σ shares − Σ (held − share)⁺`): `share + 1` while
/// a unit is free or already its own, `share` otherwise. A bond may take one more claim iff
/// `held + 1 ≤ cap`. `holders` are the active holders, the asking bond included (with `held = 0` if it
/// holds none). A frozen holder's share is zero, so what it still holds is counted against the slack;
/// it may take nothing.
///
/// **One slack unit a bond** (V-T4, measured): with the slack first come, a 988k bond that filled
/// first beside an honest 1M bond and a 939k card took both free units — 21 claims of a 58-claim room
/// at a stake fraction of 0.3375 (s·room + 1.4), past §7.4's `≤ s·room + 1`. With one unit a bond an
/// attacker of stake fraction `s` holds at most `⌊s·room⌋ + 1`; the room left idle is at most the
/// slack no second bond wants (fewer units than active bonds).
///
/// **`headroom`: units kept out of the shares** (the floor's racing headroom,
/// [`palw_floor_room_race_headroom_v1`]; `0` for a model class's room, whose share is exactly the
/// above). The shares divide `room − headroom`; the slack is still counted over the whole room, so at
/// least `headroom` single units beyond the rounding slack stay open to the bonds past their share,
/// first come — the room small producers racing the same tip draw from. What a share guarantees is
/// then `⌊s·(room − headroom)⌋`.
pub fn palw_bond_room_cap_v1(room: u64, headroom: u64, me: usize, holders: &[PalwRoomHolderV1]) -> u64 {
    let Some(mine) = holders.get(me) else { return 0 };
    if mine.frozen {
        return 0;
    }
    // A frozen holder's claims still occupy the room until they leave; the live holders share the rest.
    let frozen_held: u64 = holders.iter().filter(|h| h.frozen).map(|h| h.held).fold(0u64, u64::saturating_add);
    let live_room = room.saturating_sub(frozen_held);
    let pool = live_room.saturating_sub(headroom);
    let live_total: u128 = holders.iter().filter(|h| !h.frozen).map(|h| h.collateral as u128).fold(0, u128::saturating_add);
    let share_of = |h: &PalwRoomHolderV1| palw_bond_room_share_v1(pool, h.collateral, live_total.saturating_sub(h.collateral as u128), false);
    let live = || holders.iter().filter(|h| !h.frozen);
    let shares: u64 = live().map(share_of).fold(0u64, u64::saturating_add);
    let excess: u64 = live().map(|h| h.held.saturating_sub(share_of(h))).fold(0u64, u64::saturating_add);
    let slack_free = live_room.saturating_sub(shares).saturating_sub(excess);
    let my_share = share_of(mine);
    // Its one unit: already taken (it holds past its share), or free to take.
    let my_unit = if mine.held > my_share { 1 } else { slack_free.min(1) };
    my_share.saturating_add(my_unit)
}

/// **Lane liab's freeze, as this lane reads it** (ADR-0160 AG-3: the first conviction freezes a
/// bond; a frozen bond holds no share). Lane liab's `palw_bond_is_frozen_v1` replaces this body at
/// `rcore/cap-int` (one line); on this lane's base there is no freeze map, so nothing is frozen and
/// the share's frozen clause is exercised by its tests through [`palw_bond_room_share_v1`].
pub fn palw_verify_bond_is_frozen_v1(_state: &PalwChainStateV2, _bond: &PalwBondKeyV2) -> bool {
    false
}

/// **ADR-0160 V2 (J-6): the own claims each seat keeps room for before its room counts as slots** —
/// measured need: the seats anchor the panels (past `palw_operator_anchor` they alone do), and a seat
/// whose work room the flood's duties filled fails the ceiling on its own attempt (V-T5: the card's
/// anchor block was disqualified at admission once the seats saturated). Eight genesis cards rotating
/// one anchor a DAA each hold ≈ 3 own claims at a time until their licences release the escrow; four
/// is that with one to spare (≈ 12,800 MSK a seat today, 2.7% of a genesis seat's work room).
pub const PALW_FLOOR_ROOM_ANCHOR_RESERVE_V1: u32 = 4;

/// **ADR-0160 V2 (J-6): the fewest units the floor's racing headroom holds** — V4 measured ≈ 21
/// attempts in one merge at a 32-producer flood; 24 covers one such merge.
pub const PALW_FLOOR_ROOM_RACE_HEADROOM_MIN_V1: u64 = 24;
/// **ADR-0160 V2 (J-6): the racing headroom's share of a larger floor** — one unit in sixteen, so a
/// room that grows with the seats' capital keeps its headroom in step with the producers racing it.
pub const PALW_FLOOR_ROOM_RACE_HEADROOM_DIVISOR_V1: u64 = 16;

/// **ADR-0160 V2 (J-6): the floor's racing headroom** — the units of a floor room of `capacity`
/// claims kept out of the stake shares ([`palw_bond_room_cap_v1`]'s `headroom`):
/// `min(capacity / 4, max(PALW_FLOOR_ROOM_RACE_HEADROOM_MIN_V1, capacity / 16))`.
///
/// **Why the floor needs one** (the lane-verify review's racing finding). A producer asks the floor
/// gate at its tip (`palw_floor_room_admits_v1`) and cannot see the attempts other producers mine on
/// the same tip; the chain merges those siblings and the fold admits them in order. Next to large
/// holders a small bond's guaranteed share is 0 and it holds only a slack unit, so when more small
/// producers race than the room has free units, the late ones are skipped — and a skipped merged
/// attempt's withheld worker carve is burned: measured, 4 of 12 racing 13k producers lost it. Below
/// F-R the floor has no share and all twelve were admitted.
///
/// **What it guarantees, and what it cannot.** With the headroom the rounding slack is never less
/// than it, so racers are skipped only when more of them land ahead than the free units their tip
/// showed — at least the headroom while nobody holds it. Measured (V-T5r, a 66-claim floor under a
/// seat load, headroom 16, a 10M flooder at its cap): 12 racers all admitted (8 before the headroom),
/// 24 racers 18 admitted, 40 racers 18. No rule can make it zero for any number of racers: the loser's
/// carve cannot be paid (a skipped attempt is unverified work, and paying it pays a producer that
/// mines attempts the gate will skip), and the winners cannot all be admitted past the room (the
/// overflow voids `BindTimeout` later, the harm J-6 exists to stop, and crowds the shares A4
/// guarantees). The price is the shares': a bond of stake fraction `s` is guaranteed
/// `⌊s·(capacity − headroom)⌋` rather than `⌊s·capacity⌋`.
pub fn palw_floor_room_race_headroom_v1(capacity: u64) -> u64 {
    PALW_FLOOR_ROOM_RACE_HEADROOM_MIN_V1.max(capacity / PALW_FLOOR_ROOM_RACE_HEADROOM_DIVISOR_V1).min(capacity / 4)
}

/// **ADR-0160 V2 (J-6): the floor's seat-capital room**, as the fold reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwFloorRoomV1 {
    /// Floor claims the eligible seats' free capital can bind now: `⌊Σ ⌊room_s / eligibility⌋ /
    /// seat_count⌋`.
    pub slots: u64,
    /// Floor claims accepted and not yet bound (each will reserve a duty on `seat_count` seats).
    pub pending: u64,
}

impl PalwFloorRoomV1 {
    /// Claims one more floor attempt may still take.
    pub fn room(&self) -> u64 {
        self.slots.saturating_sub(self.pending)
    }

    /// Whether one more floor claim fits.
    pub fn admits_one(&self) -> bool {
        self.pending < self.slots
    }

    /// The pipeline's whole size — what the floor share divides: the claims waiting for a bind and
    /// the ones still bindable.
    pub fn capacity(&self) -> u64 {
        self.slots.max(self.pending)
    }
}

/// **J-6's arithmetic**: every eligible seat's free work room (`rooms`), the seat-eligibility price
/// of one floor claim (`max(duty_bind, lock_2)`, what a seat must have room for to be drawn), the
/// panel's seat count, and the floor claims not yet bound. A zero price or seat count binds nothing.
pub fn palw_floor_room_v1(rooms: impl Iterator<Item = u128>, eligibility: u128, seat_count: u64, pending: u64) -> PalwFloorRoomV1 {
    if eligibility == 0 || seat_count == 0 {
        return PalwFloorRoomV1 { slots: 0, pending };
    }
    let seat_slots: u128 = rooms.map(|room| room / eligibility).fold(0u128, u128::saturating_add);
    PalwFloorRoomV1 { slots: (seat_slots / seat_count as u128).min(u64::MAX as u128) as u64, pending }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;

    /// testnet-12's 8k row (the capacity map §1): eccu 1,388,926,972,416, window 3 spans.
    const ECCU_8K: u128 = 1_388_926_972_416;
    const WINDOW_8K: u64 = 3;

    fn t12_globals() -> PalwRegistryGlobalsV1 {
        PalwRegistryGlobalsV1 { seat_count: 5, ..PALW_REGISTRY_GLOBALS_V1 }
    }

    /// Below the fence the pricing is today's (5 full replays at the reference): c_8k = 5 on eight
    /// ready seats, the capacity map's number.
    #[test]
    fn below_the_fence_the_pricing_is_todays_and_c8k_is_five() {
        let g = t12_globals();
        let today = palw_replay_pricing_v1(false, &g);
        assert_eq!(today.per_seat_per_span, 1_680_000_000_000);
        assert_eq!(today.replicas, 5);
        let c = crate::palw_work_target_v1::palw_panel_capacity_by_rate_v1(8 * today.per_seat_per_span, 0, WINDOW_8K, ECCU_8K * 5);
        assert_eq!(c, 5);
    }

    /// V-T3's arithmetic: past the fence, k = 2 and the measured speed; at 16 ready seats c_8k ≥ 50,
    /// and V-I3 — never above the measured replay × window ÷ (attesters × eccu).
    #[test]
    fn past_the_fence_c8k_at_sixteen_seats_is_at_least_fifty() {
        let g = t12_globals();
        let v2 = palw_replay_pricing_v1(true, &g);
        assert_eq!(v2.replicas, 2);
        let c16 = palw_panel_capacity_v2(16, v2.per_seat_per_span, WINDOW_8K, ECCU_8K, PALW_CAPACITY_ATTESTERS_PER_SEGMENT_V1);
        let c8 = palw_panel_capacity_v2(8, v2.per_seat_per_span, WINDOW_8K, ECCU_8K, PALW_CAPACITY_ATTESTERS_PER_SEGMENT_V1);
        println!("c_8k past F-R: {c8} at 8 ready seats, {c16} at 16 (speed {PALW_CAPACITY_REPLAY_SPEED_PERMILLE_V1}‰)");
        assert!(c16 >= 50, "the ×10 step: c_8k {c16}");
        // V-I3: the room never exceeds what the measured replay covers.
        let measured = palw_replay_ccu_per_span_measured_v1(&g) * 16 * WINDOW_8K as u128;
        assert!(c16 as u128 * ECCU_8K * 2 <= measured, "V-I3");
        // Monotone in ready seats, never zero-division.
        for ready in 0..40u128 {
            let a = palw_panel_capacity_v2(ready, v2.per_seat_per_span, WINDOW_8K, ECCU_8K, 2);
            let b = palw_panel_capacity_v2(ready + 1, v2.per_seat_per_span, WINDOW_8K, ECCU_8K, 2);
            assert!(a <= b);
        }
        assert_eq!(palw_panel_capacity_v2(16, v2.per_seat_per_span, WINDOW_8K, 0, 2), 0, "a class that costs nothing has no room");
    }

    /// V-I4 and V-I5 on the share: the whole room uncontended, stake-proportional (floored) under
    /// contention, a split never shares more than the whole, a frozen bond holds nothing.
    #[test]
    fn the_share_is_stake_proportional_split_neutral_and_frozen_bonds_hold_none() {
        const MSK: u64 = 100_000_000;
        let room = 58u64;
        assert_eq!(palw_bond_room_share_v1(room, 13_000 * MSK, 0, false), room, "uncontended: the whole room");
        assert_eq!(palw_bond_room_share_v1(room, 1_000_000 * MSK, 1_000_000 * MSK as u128, false), 29);
        assert_eq!(palw_bond_room_share_v1(room, 13_000 * MSK, 1_000_000 * MSK as u128, false), 0, "below 1/room: slack only");
        assert_eq!(palw_bond_room_share_v1(room, 13_000 * MSK, 0, true), 0, "V-I5");
        for (whole, n, honest) in [(988_000u64, 76u64, 1_000_000u64), (1_000_000, 10, 1_000_000), (390_000, 30, 13_000), (130_000, 2, 0)] {
            let whole_share = palw_bond_room_share_v1(room, whole * MSK, honest as u128 * MSK as u128, false);
            let piece = whole / n;
            let pieces: u64 = (0..n)
                .map(|_| palw_bond_room_share_v1(room, piece * MSK, (honest as u128 + (whole - piece) as u128) * MSK as u128, false))
                .sum();
            assert!(pieces <= whole_share, "V-I4: {whole} in {n}: {pieces} > {whole_share}");
        }
    }

    /// The cap: an honest bond of stake fraction s always reaches ⌊s·room⌋ whatever the others hold
    /// past their shares, the slack is shared one unit a bond, the room is never exceeded, a split
    /// gains at most the slack (≤ one unit per holder), and a whole attacker holds ≤ ⌊s·room⌋ + 1 —
    /// V-T4's arithmetic (76 × 13k, and one 988k bond, vs an honest 1M and a card).
    #[test]
    fn the_cap_guarantees_every_share_and_shares_only_the_slack() {
        let room = 58u64;
        let h = |c: u64, held: u64| PalwRoomHolderV1 { collateral: c, held, frozen: false };
        // Uncontended: the whole room.
        assert_eq!(palw_bond_room_cap_v1(room, 0, 0, &[h(13_000, 0)]), room);
        // The Sybil flood: 76 pieces each holding one claim (76 > room is not even reachable) — the
        // honest 1M bond's cap is its share whatever the pieces hold.
        let mut holders = vec![h(1_000_000, 0)];
        holders.extend((0..76).map(|_| h(13_000, 0)));
        let honest_share = palw_bond_room_share_v1(room, 1_000_000, 76 * 13_000, false);
        assert_eq!(honest_share, 29);
        // Fill greedily, pieces first: each piece takes a slot while its cap allows.
        let mut total = 0u64;
        for i in 1..holders.len() {
            while holders[i].held < palw_bond_room_cap_v1(room, 0, i, &holders) && total < room {
                holders[i].held += 1;
                total += 1;
            }
        }
        let pieces_hold = total;
        // The honest bond still reaches its share, and the room holds.
        while holders[0].held < palw_bond_room_cap_v1(room, 0, 0, &holders) {
            holders[0].held += 1;
            total += 1;
        }
        println!("V-T4 arithmetic: 76 × 13k pieces hold {pieces_hold}, the honest 1M bond {}, room {room}", holders[0].held);
        assert!(holders[0].held >= honest_share, "A4: the honest bond reaches ⌊s·room⌋");
        assert!(total <= room, "the room holds: {total}");
        // A whole 988k bond in the same place holds its share plus what slack it wins.
        let whole = vec![h(1_000_000, 0), h(988_000, 0)];
        let whole_cap = palw_bond_room_cap_v1(room, 0, 1, &whole);
        assert!(pieces_hold <= whole_cap + 76, "V-I4: the split gains at most one unit per piece");
        assert!(pieces_hold <= room - honest_share, "the pieces never take the honest share");
        // The whole attacker fills first beside the honest bond and a card (each holding one): at
        // most ⌊s·room⌋ + 1, however much slack the flooring left (two units here).
        let mut three = vec![h(1_000_000, 1), h(939_063, 1), h(988_000, 0)];
        while three[2].held < palw_bond_room_cap_v1(room, 0, 2, &three) {
            three[2].held += 1;
        }
        let s_room = room as f64 * 988_000.0 / (1_000_000.0 + 939_063.0 + 988_000.0);
        assert_eq!(three[2].held, s_room as u64 + 1, "the attacker: its share and one slack unit ({s_room:.2})");
        assert!(three[2].held as f64 <= s_room + 1.0, "A4/V-T4: ≤ s·room + 1");
        for i in 0..2 {
            while three[i].held < palw_bond_room_cap_v1(room, 0, i, &three) {
                three[i].held += 1;
            }
        }
        assert!(three.iter().map(|h| h.held).sum::<u64>() <= room);
        assert_eq!(three[0].held, palw_bond_room_share_v1(room, 1_000_000, 939_063 + 988_000, false) + 1, "the honest bond: its share and the other unit");
        // A frozen holder takes nothing, and what it holds is charged to the slack.
        let frozen = vec![h(1_000_000, 0), PalwRoomHolderV1 { collateral: 1_000_000, held: 10, frozen: true }];
        assert_eq!(palw_bond_room_cap_v1(room, 0, 1, &frozen), 0);
        assert_eq!(palw_bond_room_cap_v1(room, 0, 0, &frozen), room - 10, "the frozen bond's claims hold room until they leave");
    }

    /// **The floor's racing headroom** (the lane-verify review's racing finding, its probe's shape):
    /// eight cards holding one floor claim each, a 10M flooder filled to its cap and a 10M bond
    /// holding one, in a 66-claim floor. Twelve 13k bonds — share 0 beside them — racing one tip each
    /// ask for a unit; the fold admits them in order. With the headroom every one gets its unit
    /// (without it, the rounding slack runs out first — the review measured 4 of 12 skipped and their
    /// carves burned); the room is never exceeded; the 10M honest bond still reaches its share of the
    /// room less the headroom (A4); and the headroom's size is `min(c/4, max(24, c/16))`.
    #[test]
    fn the_floor_headroom_keeps_single_units_open_to_racers() {
        for (capacity, expected) in [(0u64, 0u64), (3, 0), (66, 16), (96, 24), (400, 25), (1_172, 73), (9_000, 562)] {
            assert_eq!(palw_floor_room_race_headroom_v1(capacity), expected, "capacity {capacity}");
        }
        let mut last = 0;
        for capacity in 0..6_000u64 {
            let headroom = palw_floor_room_race_headroom_v1(capacity);
            assert!(headroom <= capacity / 4 && headroom >= PALW_FLOOR_ROOM_RACE_HEADROOM_MIN_V1.min(capacity / 4), "{capacity}");
            assert!(headroom >= last, "monotone in the room: {capacity}");
            last = headroom;
        }
        let h = |c: u64, held: u64| PalwRoomHolderV1 { collateral: c, held, frozen: false };
        let race = |headroom: u64| -> (u64, Vec<PalwRoomHolderV1>) {
            let capacity = 66u64;
            let mut holders: Vec<PalwRoomHolderV1> = (0..8).map(|_| h(939_063, 1)).collect();
            holders.push(h(10_000_000, 0));
            holders.push(h(10_000_000, 1));
            while holders[8].held < palw_bond_room_cap_v1(capacity, headroom, 8, &holders) {
                holders[8].held += 1;
            }
            let mut admitted = 0u64;
            for _ in 0..12 {
                holders.push(h(13_000, 0));
                let me = holders.len() - 1;
                if palw_bond_room_cap_v1(capacity, headroom, me, &holders) >= 1 {
                    holders[me].held = 1;
                    admitted += 1;
                }
            }
            (admitted, holders)
        };
        let (bare, _) = race(0);
        let headroom = palw_floor_room_race_headroom_v1(66);
        let (admitted, mut holders) = race(headroom);
        println!("twelve 13k racers beside a 10M flooder in a 66-claim floor: {bare} admitted with no headroom, {admitted} with {headroom}");
        assert!(bare < 12, "the rounding slack alone runs out: {bare}");
        assert_eq!(admitted, 12, "with the headroom every racer finds its single unit");
        assert!(holders.iter().map(|h| h.held).sum::<u64>() <= 66, "the room holds");
        // A4 over the pool: the honest 10M bond (index 9) still reaches its share of 66 − headroom.
        let total: u128 = holders.iter().map(|h| h.collateral as u128).sum();
        let guaranteed = ((66 - headroom) as u128 * 10_000_000 / total) as u64;
        while holders[9].held < palw_bond_room_cap_v1(66, headroom, 9, &holders) {
            holders[9].held += 1;
        }
        assert!(holders[9].held >= guaranteed, "A4: {} ≥ ⌊s·(room − headroom)⌋ = {guaranteed}", holders[9].held);
        assert!(holders.iter().map(|h| h.held).sum::<u64>() <= 66, "the room still holds");
        // One unit a bond: no racer holds two.
        for (i, holder) in holders.iter().enumerate().skip(10) {
            assert_eq!(palw_bond_room_cap_v1(66, headroom, i, &holders), holder.held, "racer {i} is at its one unit");
        }
    }

    #[test]
    fn the_floor_room_counts_whole_seat_slots_and_the_pending_claims() {
        let price = 640;
        // Eight seats with 469,531 MSK of room each (500‰ of 939,063) at 640 a slot: 733 slots each,
        // 5,864 slots, 1,172 claims.
        let rooms = vec![469_531u128; 8];
        let r = palw_floor_room_v1(rooms.iter().copied(), price, 5, 1_000);
        assert_eq!(r.slots, 8 * (469_531 / 640) / 5);
        assert_eq!(r.room(), r.slots - 1_000);
        assert!(r.admits_one());
        // A seat's fragment below one price is no slot.
        assert_eq!(palw_floor_room_v1([639u128; 8].into_iter(), price, 5, 0).slots, 0);
        // Pending at the slots: nothing more.
        let full = palw_floor_room_v1(rooms.iter().copied(), price, 5, r.slots);
        assert!(!full.admits_one());
        assert_eq!(full.room(), 0);
        assert_eq!(full.capacity(), r.slots);
        assert_eq!(palw_floor_room_v1(rooms.into_iter(), 0, 5, 3), PalwFloorRoomV1 { slots: 0, pending: 3 });
    }
}
