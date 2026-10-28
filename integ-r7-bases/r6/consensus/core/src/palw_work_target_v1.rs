//! ADR-0137 — **a block buys one unit of work from any model, and a share is a result, not an
//! input.**
//!
//! The network holds one work target `W` (economic compute a block, MAC-eq — ADR-0131's CCU) the
//! way it holds `bits`. A model class `m` has no target of its own: a forward of it admits with
//! `p_m = min(1, CCU_m / W)`, so the expected compute a win is `W` for every model with a forward
//! lighter than `W`, and every Final is paid the same escrow for the same expected work. `W` is
//! walked once an epoch over MODEL blocks (the floor is the residual and is never priced here) and
//! never below `W₀ = escrow / rate_max` — the most the network will ever pay for a unit of compute.
//!
//! This module is the arithmetic and the types, pure and consensus-inert on its own:
//!
//! * [`palw_work_floor_v1`] — `W₀` from the block's escrow and the rate;
//! * [`palw_work_target_step_v1`] — the clamped epoch step, floored at `W₀`;
//! * [`palw_work_ticket_target_v1`] — a class's ticket target from its CCU and `W`;
//! * [`palw_panel_room_v1`] — the network-wide verification budget in one class's claims
//!   (ADR-0137 D5): one horizon, one pool, no per-class allocation;
//! * [`palw_panel_held_to_final_v1`] — which classes the panel room holds to Final (ADR-0152's C7);
//! * [`palw_final_work_shares_v1`] — the reader's share: finalized work over a window.
//!
//! **Shadow first.** The state carries `W` as a shadow value outside the state root
//! (`PalwChainStateV2::work_target_shadow`) so every node computes and prints it while the shipped
//! share → target rule keeps deciding the lottery; the fence (`Params::palw_work_target`, dormant
//! everywhere) is what makes the lottery read it.

use std::collections::BTreeMap;

use kaspa_hashes::Hash64;

use crate::palw_model_registry_v1::{PalwModelLifecycleRowV1, PalwModelWorkV1};

/// The clamp of one epoch step of `W`: never more than ×4 or ÷4 an epoch, the class DAA's own
/// bound (`class_daa_max_factor` on every shipped bundle).
pub const PALW_WORK_TARGET_MAX_FACTOR_V1: u32 = 4;

/// The rate the shadow prices `W₀` with where no payout fence is armed: 9 MSK per G MAC-eq, the
/// arming branch's calibration (ADR-0132 §7: the dense class at 69.9 % of the escrow).
pub const PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1: u64 = 900_000_000;

/// One G of compute, the rate's denominator (`rate_sompi_per_giga`).
pub const PALW_WORK_TARGET_GIGA_V1: u128 = 1_000_000_000;

/// How many closed epochs of finalized work the shadow keeps for the reader's share.
pub const PALW_FINAL_WORK_EPOCHS_KEPT_V1: u64 = 100;

/// **The work target as the state carries it** — `W`, its floor, and the closed epoch's census
/// that produced this step, kept so a reader (op 186) sees what the step saw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWorkTargetV2 {
    /// `W`: the economic compute one block buys at the class ticket, MAC-eq — the single-lottery
    /// target (ADR-0132 S), stepped by model blocks against the cadence. Under the shipped double
    /// draw it sits at the floor: the network draw against `bits` is the controller above it.
    pub work: u128,
    /// `W₀ = escrow / rate_max` at the last boundary: `W` never falls below it, and the fence's
    /// ticket reads it (`T_m = MAX · min(1, CCU_m / W₀)`).
    pub floor: u128,
    /// The network draws a class win costs at the boundary block's `bits`, Q32 — one where no
    /// `bits` were at hand.
    pub network_draws_q32: u128,
    /// `W · network_draws`: the compute a model block actually buys under the double draw.
    pub effective_work: u128,
    /// The epoch `work` governs (the epoch opened at the boundary that stepped it).
    pub epoch_index: u64,
    /// The closed epoch's model blocks (attempt lane, every class but the floor) the step read.
    pub closed_model_blocks: u64,
    /// The closed epoch's expected model blocks: the attempt lane's slice of the cadence
    /// (`epoch_length · split`), a DAA count and never a clock.
    pub closed_expected_blocks: u64,
}

/// The fold's input, built by the node from `Params` and the registry's works — the same on every
/// node because every term is derived from the chain and the ruleset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwWorkTargetFoldV1 {
    /// `rate_max`, sompi per G MAC-eq: the payout fence's rate where armed, the shadow constant otherwise.
    pub rate_sompi_per_giga: u64,
    /// The folded block's `bits` (0 where none were at hand): the network draw the reader sees.
    pub block_bits: u32,
    /// The epoch step's clamp.
    pub max_factor: u32,
    /// Every class's work (its `economic_ccu_per_claim` is the class's CCU).
    pub works: BTreeMap<Hash64, PalwModelWorkV1>,
}

/// `W₀ = escrow · 10⁹ / rate`, in CCU; at least one. A zero rate prices nothing and yields the
/// hardest floor (`u128::MAX`), so no class could ever be priced above it.
pub fn palw_work_floor_v1(escrow_sompi: u64, rate_sompi_per_giga: u64) -> u128 {
    if rate_sompi_per_giga == 0 {
        return u128::MAX;
    }
    ((escrow_sompi as u128).saturating_mul(PALW_WORK_TARGET_GIGA_V1) / rate_sompi_per_giga as u128).max(1)
}

/// **One epoch step of `W`**: `W · model_blocks / expected_blocks`, clamped to `[W / f, W · f]`,
/// never below `floor`. An epoch that expected nothing leaves `W` where it is (floored); an epoch
/// the models sat out eases `W` by the whole clamp toward the floor — silence is not evidence of
/// trying, but the floor is the price the network states, and `W` may not sit above it on nothing.
pub fn palw_work_target_step_v1(current: u128, floor: u128, model_blocks: u64, expected_blocks: u64, max_factor: u32) -> u128 {
    let f = max_factor.max(2) as u128;
    let current = current.max(1);
    if expected_blocks == 0 {
        return current.max(floor);
    }
    let low = current / f;
    let high = current.saturating_mul(f);
    if model_blocks == 0 {
        return low.max(floor).max(1);
    }
    let scaled = mul_div_u128(current, model_blocks as u128, expected_blocks as u128);
    scaled.clamp(low, high).max(floor).max(1)
}

/// **A class's ticket target from its CCU and `W`**: `MAX · min(1, CCU / W)`, never zero. A class
/// with no counted work is priced at the hardest target — it may not be handed the ticket space by
/// the arithmetic that prices the ones that declared work.
pub fn palw_work_ticket_target_v1(ccu: u128, work: u128) -> u128 {
    if ccu == 0 {
        return 1;
    }
    let work = work.max(1);
    if ccu >= work {
        return u128::MAX;
    }
    mul_div_u128(u128::MAX, ccu, work).max(1)
}

/// `CCU / W` in permille, saturating at `u32::MAX` — the reader's "how many of my forwards a block
/// costs" turned upside down: 1,000 ‰ and above is a class heavier than a block's work.
pub fn palw_work_ratio_permille_v1(ccu: u128, work: u128) -> u32 {
    let work = work.max(1);
    ccu.saturating_mul(1_000).checked_div(work).unwrap_or(u32::MAX as u128).min(u32::MAX as u128) as u32
}

/// The expected forwards a win, Q32: `W / CCU`, one where the class is heavier than `W`.
pub fn palw_expected_forwards_q32_v1(ccu: u128, work: u128) -> u128 {
    let one = crate::palw_economic_compute_v1::PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
    if ccu == 0 {
        return u128::MAX;
    }
    if ccu >= work {
        return one;
    }
    mul_div_u128(work, one, ccu).max(one)
}

/// **The verification budget, in one class's claims** (ADR-0137 D5): the panel's replay over a
/// common horizon, less what every class already holds in flight, divided by what one claim of
/// this class costs the panel. Zero where the panel is full or the class costs nothing to verify.
pub fn palw_panel_room_v1(panel_replay_per_span: u128, horizon_spans: u64, inflight_replay: u128, claim_replay: u128) -> u64 {
    if claim_replay == 0 {
        return 0;
    }
    let budget = panel_replay_per_span.saturating_mul(horizon_spans.max(1) as u128);
    let free = budget.saturating_sub(inflight_replay);
    (free / claim_replay).min(u64::MAX as u128) as u64
}

/// The fixed point of the panel's per-span replay demand (2026-09-24 audit #4): Q32, so a term
/// rounded up per class over-counts by at most 2^-32 of one replay unit a span.
pub const PALW_PANEL_DEMAND_SCALE_V1: u128 = 1 << 32;

/// **One class's share of the panel's per-span replay demand** (2026-09-24 audit #4), scaled by
/// [`PALW_PANEL_DEMAND_SCALE_V1`] and rounded UP: `claims × claim_replay / window_spans` — the
/// replay those claims still owe, spread over the window each of them is judged by. Fail-closed:
/// a product that does not fit is `u128::MAX`, a demand no budget holds.
pub fn palw_panel_demand_term_v1(claims: u128, claim_replay: u128, window_spans: u64) -> u128 {
    claims
        .checked_mul(claim_replay)
        .and_then(|replay| replay.checked_mul(PALW_PANEL_DEMAND_SCALE_V1))
        .map(|scaled| scaled.div_ceil(window_spans.max(1) as u128))
        .unwrap_or(u128::MAX)
}

/// **How many claims of one class the panel's budget holds beside every other class's demand**
/// (2026-09-24 audit #4; replaces [`palw_panel_room_v1`] past `palw_audit_2026_09_23`) — the
/// largest `n` whose term fits: `⌈n × claim_replay × 2^32 / window⌉ ≤ per_span × 2^32 − others`,
/// which is `n ≤ (per_span × 2^32 − others) × window / (claim_replay × 2^32)`, floored.
///
/// The class's room is this less the class's own claims owed (`n_max − owed`). Because the bound is
/// the feasibility of the class's WHOLE term, admitting `k` claims is exactly "the term with them
/// still fits", so a room read before a claim and after it agree (a room built from a rounded-up
/// term and a floored division did not: a room of 1 at step 3 could read 0 at step 4).
///
/// The common-horizon rule measured every class's backlog against the SHORTEST admitted window,
/// so admitting a class with a shorter window shrank every other class's budget after the fact,
/// and with no class admitting it fell back to one span, which made a hold self-reinforcing. Here
/// another class enters only through its own term, over its own window, and there is no fallback.
///
/// Fail-closed: any product that does not fit gives `0`.
pub fn palw_panel_capacity_by_rate_v1(panel_replay_per_span: u128, others_scaled: u128, window_spans: u64, claim_replay: u128) -> u64 {
    if claim_replay == 0 {
        return 0;
    }
    let (Some(supply), Some(per_claim)) =
        (panel_replay_per_span.checked_mul(PALW_PANEL_DEMAND_SCALE_V1), claim_replay.checked_mul(PALW_PANEL_DEMAND_SCALE_V1))
    else {
        return 0;
    };
    let free = supply.saturating_sub(others_scaled);
    let window = window_spans.max(1) as u128;
    // ⌊free × window / per_claim⌋ without the product: (free / d) × w + ((free % d) × w) / d.
    let whole = (free / per_claim).checked_mul(window);
    let part = (free % per_claim).checked_mul(window).map(|r| r / per_claim);
    match (whole, part) {
        (Some(whole), Some(part)) => whole.checked_add(part).map(|n| n.min(u64::MAX as u128) as u64).unwrap_or(0),
        _ => 0,
    }
}

/// **ADR-0152's C7 by the window rule** (§9 Q2): the verification window, in spans, from which the
/// panel room holds a class to Final ([`palw_panel_held_to_final_v1`]). A class's window is the spans
/// its panel is given to replay one claim, `⌈safety × verification CCU / reference⌉` plus the
/// receipt allowance ([`crate::palw_model_registry_v1::palw_verification_window_spans_v1`]). On
/// testnet-12's genesis this selects the 2M row alone (2,799 spans), not the short-window row (3).
pub const PALW_RCORE_C7_WINDOW_SPANS_V1: u32 = 1_000;

/// **Whether the panel room holds this class to Final**: ADR-0152's C7, by the window rule
/// ([`PALW_RCORE_C7_WINDOW_SPANS_V1`]). Read past `palw_audit_2026_09_23` only. A class it selects
/// (ADR-0152 T-2(b), the 2M lane):
///
/// * owes the panel every claim it has in flight until Final: no licence releases one, and a court
///   on a licensed claim adds nothing the class does not already owe (`palw_panel_owed_v1`);
/// * is refused past its static `max_inflight_claims` (`ClassInflightCapped`: c_2M = 1 until
///   ADR-0153), and op 186 shows it no more room than that cap leaves
///   ([`crate::palw_state_v2::palw_panel_room_read_v1`]).
///
/// Every other class is released at licence and judged by the rate room alone (T-2(a)). On
/// testnet-12 that is the short-window row (the "8k" row: window 3, `max_inflight_claims` 5).
///
/// **This is not ADR-0119's held regime.** `PalwChainStateV2::class_is_held_v1` asks whether a class
/// recorded its own step ladder at registration, which decides its court and its data availability.
/// On testnet-12 BOTH genesis model rows recorded one. f8c91f19 keyed this hold on that predicate,
/// so it held the short row to Final and to its cap of 5 as well, cutting its throughput about
/// fivefold (the 2026-09-24 re-review of f8c91f19). The two questions are independent: a class with
/// a held ladder and a short window is released at licence, and a class with a long window and no
/// ladder is held.
///
/// The window is a function of the class's registered work and the registry's globals, and the span
/// step re-derives it from the same inputs, so this answer does not move while the class's claims
/// are in flight. When `Params::palw_rcore_conservative_classes` lands with R-core+, C7 becomes the
/// union of that set and this rule.
pub fn palw_panel_held_to_final_v1(row: &PalwModelLifecycleRowV1) -> bool {
    row.profile.verification_window_spans >= PALW_RCORE_C7_WINDOW_SPANS_V1
}

/// **The reader's share**: each class's finalized work over the last `window_epochs` closed
/// epochs, in permille of every class's, class-id order; empty where nothing finalized.
pub fn palw_final_work_shares_v1(final_work: &BTreeMap<u64, BTreeMap<Hash64, u128>>, window_epochs: u64) -> Vec<(Hash64, u16)> {
    let Some(latest) = final_work.keys().next_back().copied() else { return Vec::new() };
    let oldest = latest.saturating_sub(window_epochs.max(1) - 1);
    let mut per_class: BTreeMap<Hash64, u128> = BTreeMap::new();
    for (_, classes) in final_work.range(oldest..=latest) {
        for (class, work) in classes {
            let slot = per_class.entry(*class).or_insert(0);
            *slot = slot.saturating_add(*work);
        }
    }
    let total: u128 = per_class.values().fold(0u128, |acc, w| acc.saturating_add(*w));
    if total == 0 {
        return Vec::new();
    }
    per_class.into_iter().map(|(class, work)| (class, (work.saturating_mul(1_000) / total).min(1_000) as u16)).collect()
}

/// `W · draws` with `draws` in Q32, saturating: the compute a model block buys under the double draw.
pub fn palw_effective_work_v1(work: u128, network_draws_q32: u128) -> u128 {
    let one = crate::palw_economic_compute_v1::PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
    mul_div_u128(work, network_draws_q32.max(one), one)
}

/// `a · b / d` without overflow in the middle (`a / d · b + (a mod d) · b / d`), saturating.
pub(crate) fn mul_div_u128(a: u128, b: u128, d: u128) -> u128 {
    let d = d.max(1);
    (a / d).saturating_mul(b).saturating_add((a % d).saturating_mul(b) / d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    const ESCROW: u64 = 275_628_448_680; // 2,756.28 MSK, testnet-11's worker carve (ADR-0132)

    #[test]
    fn adr0137_the_floor_is_the_escrow_at_the_rate_and_a_zero_rate_prices_nothing() {
        // 2,756.28 MSK / 9 MSK per G = 306.25 G MAC-eq a block.
        assert_eq!(palw_work_floor_v1(ESCROW, PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1), 306_253_831_866);
        assert_eq!(palw_work_floor_v1(0, 1), 1, "never zero");
        assert_eq!(palw_work_floor_v1(ESCROW, 0), u128::MAX, "no rate: the hardest floor");
    }

    #[test]
    fn adr0137_the_step_follows_model_blocks_clamped_and_never_below_the_floor() {
        let floor = 1_000;
        // Twice the expected model blocks: W doubles (under the clamp).
        assert_eq!(palw_work_target_step_v1(10_000, floor, 200, 100, 4), 20_000);
        // Twenty times: clamped to x4.
        assert_eq!(palw_work_target_step_v1(10_000, floor, 2_000, 100, 4), 40_000);
        // A tenth: clamped to /4.
        assert_eq!(palw_work_target_step_v1(10_000, floor, 10, 100, 4), 2_500);
        // The floor binds.
        assert_eq!(palw_work_target_step_v1(2_000, floor, 10, 100, 4), 1_000);
        // Silence eases by the whole clamp, floored.
        assert_eq!(palw_work_target_step_v1(10_000, floor, 0, 100, 4), 2_500);
        assert_eq!(palw_work_target_step_v1(3_000, floor, 0, 100, 4), 1_000);
        // Nothing expected: W stays (floored).
        assert_eq!(palw_work_target_step_v1(10_000, floor, 5, 0, 4), 10_000);
        assert_eq!(palw_work_target_step_v1(10, floor, 5, 0, 4), 1_000);
        // A factor below two is read as two.
        assert_eq!(palw_work_target_step_v1(10_000, floor, 2_000, 100, 1), 20_000);
    }

    /// **CCU > W: a class heavier than a block's work draws the whole ticket** — every forward
    /// wins, it is paid the escrow for more than `W` of compute, and the reader sees the ratio
    /// above 1,000 ‰ and one expected forward a win (ADR-0137 §9's stated limit).
    #[test]
    fn adr0137_a_class_heavier_than_the_work_target_draws_the_whole_ticket() {
        let w = palw_work_floor_v1(ESCROW, PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1);
        let heavy = w + 1;
        assert_eq!(palw_work_ticket_target_v1(heavy, w), u128::MAX);
        assert_eq!(palw_work_ticket_target_v1(w, w), u128::MAX, "exactly W is the whole ticket too");
        assert_eq!(palw_work_ratio_permille_v1(heavy, w), 1_000);
        assert_eq!(palw_work_ratio_permille_v1(2 * w, w), 2_000);
        let one = crate::palw_economic_compute_v1::PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
        assert_eq!(palw_expected_forwards_q32_v1(heavy, w), one, "one forward a win");
        // Below W the target is the exact ratio and the expected forwards its inverse.
        let quarter = w / 4;
        assert_eq!(palw_work_ticket_target_v1(quarter, w), mul_div_u128(u128::MAX, quarter, w));
        assert_eq!(palw_work_ratio_permille_v1(quarter, w), 249, "w / 4 truncates half a unit: 249.99 ‰ floors");
        assert_eq!(palw_work_ratio_permille_v1(100_000_000_000, 400_000_000_000), 250);
        assert_eq!(palw_expected_forwards_q32_v1(quarter, w) >> 32, 4);
        // No work: the hardest target, and never zero.
        assert_eq!(palw_work_ticket_target_v1(0, w), 1);
        assert_eq!(palw_work_ticket_target_v1(1, u128::MAX), 1);
    }

    /// **A thousand classes are priced like one**: the same CCU is the same ticket whatever the
    /// count, the expected compute a win is `W` for each, and `W`'s step reads blocks, not classes.
    #[test]
    fn adr0137_a_thousand_classes_share_one_ticket_price() {
        let w = 306_253_831_866u128;
        let ccu = 84_650_000_000u128; // Qwen2.5 @ 512 (ADR-0131)
        let one_class = palw_work_ticket_target_v1(ccu, w);
        let thousand: Vec<u128> = (0..1_000).map(|_| palw_work_ticket_target_v1(ccu, w)).collect();
        assert!(thousand.iter().all(|t| *t == one_class));
        // Expected compute a win = expected forwards x CCU = W, class count absent from the identity.
        let forwards_q32 = palw_expected_forwards_q32_v1(ccu, w);
        let compute_a_win = (forwards_q32 >> 32) * ccu + (((forwards_q32 & 0xFFFF_FFFF) * ccu) >> 32);
        assert!((compute_a_win as i128 - w as i128).unsigned_abs() <= ccu, "{compute_a_win} vs {w}");
        // The step is the same for one class producing 50 blocks and a thousand producing 50 between them.
        assert_eq!(palw_work_target_step_v1(w, w / 2, 50, 100, 4), palw_work_target_step_v1(w, w / 2, 50, 100, 4));
    }

    /// **Capacity overload leaves no room for any class**: one pool over one horizon, a heavy
    /// class's claims in flight take the light class's room, and a full panel admits nothing.
    #[test]
    fn adr0137_capacity_overload_leaves_no_room_for_any_class() {
        let per_span = 8u128 * 20_000_000_000 * 700 / 1_000; // eight seats, the globals' reference work at 70 %
        let heavy = 84_650_000_000u128 * 5; // Qwen2.5 replayed by five seats
        let light = 21_070_000_000u128 * 5;
        assert_eq!(palw_panel_room_v1(per_span, 4, 0, heavy), 1, "an empty panel over four spans holds one heavy claim");
        assert_eq!(palw_panel_room_v1(per_span, 4, 0, light), 4, "or four light ones");
        // Two light claims in flight: the heavy class's room is what is left, in ITS claims.
        assert_eq!(palw_panel_room_v1(per_span, 4, 2 * light, heavy), 0);
        assert_eq!(palw_panel_room_v1(per_span, 4, 2 * light, light), 2);
        // One heavy claim in flight fills the horizon for everyone.
        assert_eq!(palw_panel_room_v1(per_span, 4, heavy, light), 0);
        assert_eq!(palw_panel_room_v1(per_span, 4, heavy, heavy), 0);
        // Over the horizon: nothing, for anyone, and no underflow.
        assert_eq!(palw_panel_room_v1(per_span, 4, 100 * heavy, light), 0);
        // A class that costs nothing to verify has no room (it is not a class).
        assert_eq!(palw_panel_room_v1(per_span, 4, 0, 0), 0);
    }

    #[test]
    fn adr0137_the_readers_share_is_finalized_work_over_a_window_and_sums_to_the_whole() {
        let mut fw: BTreeMap<u64, BTreeMap<Hash64, u128>> = BTreeMap::new();
        fw.entry(10).or_default().insert(h(1), 300);
        fw.entry(10).or_default().insert(h(2), 100);
        fw.entry(11).or_default().insert(h(2), 200);
        fw.entry(12).or_default().insert(h(3), 400);
        // The last epoch alone: class 3 holds everything.
        assert_eq!(palw_final_work_shares_v1(&fw, 1), vec![(h(3), 1_000)]);
        // Three epochs: 300 / 300 / 400 of 1,000.
        assert_eq!(palw_final_work_shares_v1(&fw, 3), vec![(h(1), 300), (h(2), 300), (h(3), 400)]);
        // A window past the oldest epoch reads what there is.
        assert_eq!(palw_final_work_shares_v1(&fw, 100), vec![(h(1), 300), (h(2), 300), (h(3), 400)]);
        assert!(palw_final_work_shares_v1(&BTreeMap::new(), 10).is_empty());
    }

    #[test]
    fn adr0137_the_work_target_row_round_trips_borsh() {
        let row = PalwWorkTargetV2 {
            work: 306_253_831_866,
            floor: 306_253_831_866,
            network_draws_q32: 1 << 32,
            effective_work: 306_253_831_866,
            epoch_index: 7,
            closed_model_blocks: 3,
            closed_expected_blocks: 90,
        };
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(bytes.len(), 16 + 16 + 16 + 16 + 8 + 8 + 8);
        assert_eq!(borsh::from_slice::<PalwWorkTargetV2>(&bytes).unwrap(), row);
    }

    /// ADR-0152's C7 by the window rule: a window of 1,000 spans is held to Final and 999 is not,
    /// whatever the class's static cap.
    #[test]
    fn adr0152_c7_is_a_window_of_at_least_1000_spans() {
        use crate::palw_model_registry_v1::{PalwDerivedProfileV1, PalwModelLifecycleV1};
        let row = |window: u32, cap: u32| PalwModelLifecycleRowV1 {
            state: PalwModelLifecycleV1::Active,
            work: PalwModelWorkV1::default(),
            profile: PalwDerivedProfileV1 { verification_window_spans: window, max_inflight_claims: cap, ..Default::default() },
            since_span: 0,
            probes_passed: 0,
            probes_failed: 0,
            probes_passed_this_span: 0,
            probes_failed_this_span: 0,
            ready_seats: 0,
            inflight_claims: 0,
            utilization_permille: 0,
            admission_milli: 0,
            cap_utilization_permille: 0,
            priced_share_permille: 0,
        };
        assert_eq!(PALW_RCORE_C7_WINDOW_SPANS_V1, 1_000);
        // testnet-12's genesis model rows: the short-window row (3 spans, cap 5) and 2M (2,799, cap 1).
        assert!(!palw_panel_held_to_final_v1(&row(3, 5)), "the short-window row is released at licence");
        assert!(palw_panel_held_to_final_v1(&row(2_799, 1)), "the 2M row is held to Final");
        for cap in [1, 5, 64] {
            assert!(!palw_panel_held_to_final_v1(&row(999, cap)), "999 spans, cap {cap}");
            assert!(palw_panel_held_to_final_v1(&row(1_000, cap)), "1,000 spans, cap {cap}");
        }
        assert!(palw_panel_held_to_final_v1(&row(u32::MAX, 1)));
    }
}
