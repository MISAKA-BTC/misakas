//! **Lane PL part E (ADR-0166): a seat's availability, from on-chain SUCCESS only — and only ever read by panel ASSIGNMENT.**
//!
//! A seat that files a credited receipt is observable; a seat that does not is indistinguishable from an honest outage or a
//! Sybil's silence (ADR-0064), so absence is never charged and never counted. What the chain does see is each seat's
//! assignments (a bound panel names it) and its credited receipts (the duty row's credit, written once at the carrying block's
//! DAA). Over a rolling window of [`PALW_AVAIL_WINDOW_EPOCHS_V1`] epochs of [`PALW_AVAIL_EPOCH_DAA_V1`] DAA those two counters
//! give a factor in permille that scales a seat's WEIGHT IN THE STAKE-WEIGHTED DRAW and nothing else — security weight, slashing,
//! the SW-10 floor, fork choice and pay stay stake-only (past `Params::palw_seat_availability`):
//!
//! * **0.5x — new or inactive**: fewer than [`PALW_AVAIL_NORMAL_MIN_CREDITS_V1`] credited receipts in the window (no row at all
//!   included). A seat is never below this, so a new operator is drawn, only less often until it has shown it answers.
//! * **1.2x — long, high availability**: at least [`PALW_AVAIL_HIGH_MIN_CREDITS_V1`] credited receipts in the window, at least
//!   [`PALW_AVAIL_HIGH_MIN_RATE_PERMILLE_V1`]‰ of the window's assignments credited, and a first credit at least two windows old.
//! * **1.0x — everything else.**
//!
//! Absence cannot lower a seat below 1.0x except by letting its credits leave the window (inactivity); it can only keep a
//! seat from 1.2x. All arithmetic is integer permille; the row is rooted state (journalled, reorg-safe), written only at the
//! panel bind (assignments) and at a credit (successes).

use borsh::{BorshDeserialize, BorshSerialize};

/// One epoch of the window, in DAA.
pub const PALW_AVAIL_EPOCH_DAA_V1: u64 = 2_000;
/// Epochs in the rolling window (6 x 2,000 = 12,000 DAA — twenty shipped receipt windows).
pub const PALW_AVAIL_WINDOW_EPOCHS_V1: u64 = 6;
/// Credited receipts in the window at which a seat stops being "new or inactive".
pub const PALW_AVAIL_NORMAL_MIN_CREDITS_V1: u32 = 3;
/// Credited receipts in the window that "long, high availability" asks.
pub const PALW_AVAIL_HIGH_MIN_CREDITS_V1: u32 = 30;
/// Share of the window's assignments that must have been credited, in permille.
pub const PALW_AVAIL_HIGH_MIN_RATE_PERMILLE_V1: u64 = 900;
/// The factors, in permille.
pub const PALW_AVAIL_FACTOR_NEW_PERMILLE_V1: u16 = 500;
pub const PALW_AVAIL_FACTOR_NORMAL_PERMILLE_V1: u16 = 1_000;
pub const PALW_AVAIL_FACTOR_HIGH_PERMILLE_V1: u16 = 1_200;

/// One epoch's counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSeatAvailEpochV1 {
    pub epoch: u64,
    pub assigned: u32,
    pub credited: u32,
}

/// A bond's availability row: its first credit ever and the counters of the epochs still inside the window.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSeatAvailabilityV1 {
    /// The DAA of the bond's first credited receipt, `None` until it has one.
    pub first_credit_daa: Option<u64>,
    /// Ascending by epoch, no epoch twice, every entry within the window of the last write.
    pub epochs: Vec<PalwSeatAvailEpochV1>,
}

fn epoch_of(daa: u64) -> u64 {
    daa / PALW_AVAIL_EPOCH_DAA_V1
}

impl PalwSeatAvailabilityV1 {
    fn bucket(&mut self, daa: u64) -> &mut PalwSeatAvailEpochV1 {
        let epoch = epoch_of(daa);
        // Drop what the window left behind at this write (a pure function of the row and `daa`).
        let oldest = epoch.saturating_sub(PALW_AVAIL_WINDOW_EPOCHS_V1 - 1);
        self.epochs.retain(|e| e.epoch >= oldest);
        match self.epochs.binary_search_by_key(&epoch, |e| e.epoch) {
            Ok(i) => &mut self.epochs[i],
            Err(i) => {
                self.epochs.insert(i, PalwSeatAvailEpochV1 { epoch, assigned: 0, credited: 0 });
                &mut self.epochs[i]
            }
        }
    }

    /// The seat was named on a panel bound at `daa`.
    pub fn note_assigned(&mut self, daa: u64) {
        let b = self.bucket(daa);
        b.assigned = b.assigned.saturating_add(1);
    }

    /// The seat's receipt was credited at `daa`.
    pub fn note_credited(&mut self, daa: u64) {
        if self.first_credit_daa.is_none() {
            self.first_credit_daa = Some(daa);
        }
        let b = self.bucket(daa);
        b.credited = b.credited.saturating_add(1);
    }

    /// (assigned, credited) over the window ending at `now_daa`.
    pub fn window_totals(&self, now_daa: u64) -> (u64, u64) {
        let cur = epoch_of(now_daa);
        let oldest = cur.saturating_sub(PALW_AVAIL_WINDOW_EPOCHS_V1 - 1);
        self.epochs
            .iter()
            .filter(|e| e.epoch >= oldest && e.epoch <= cur)
            .fold((0u64, 0u64), |(a, c), e| (a + u64::from(e.assigned), c + u64::from(e.credited)))
    }

    /// **The factor, in permille, at `now_daa`.**
    pub fn factor_permille(&self, now_daa: u64) -> u16 {
        let (assigned, credited) = self.window_totals(now_daa);
        if credited < u64::from(PALW_AVAIL_NORMAL_MIN_CREDITS_V1) {
            return PALW_AVAIL_FACTOR_NEW_PERMILLE_V1;
        }
        let window_daa = PALW_AVAIL_EPOCH_DAA_V1 * PALW_AVAIL_WINDOW_EPOCHS_V1;
        let seasoned = self.first_credit_daa.is_some_and(|first| first.saturating_add(2 * window_daa) <= now_daa);
        if seasoned
            && credited >= u64::from(PALW_AVAIL_HIGH_MIN_CREDITS_V1)
            && credited.saturating_mul(1_000) >= assigned.saturating_mul(PALW_AVAIL_HIGH_MIN_RATE_PERMILLE_V1)
        {
            return PALW_AVAIL_FACTOR_HIGH_PERMILLE_V1;
        }
        PALW_AVAIL_FACTOR_NORMAL_PERMILLE_V1
    }
}

/// The factor of an optional row: no row is a new seat.
pub fn palw_seat_availability_factor_v1(row: Option<&PalwSeatAvailabilityV1>, now_daa: u64) -> u16 {
    row.map_or(PALW_AVAIL_FACTOR_NEW_PERMILLE_V1, |row| row.factor_permille(now_daa))
}

/// **A stake weight scaled by a factor** — `max(1, ⌊w · f / 1000⌋)`, in u128 so it cannot overflow.
pub fn palw_seat_availability_scale_weight_v1(weight_msk: u64, factor_permille: u16) -> u64 {
    let scaled = (u128::from(weight_msk) * u128::from(factor_permille)) / 1_000;
    u64::try_from(scaled).unwrap_or(u64::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seat_with_no_successes_is_drawn_at_half_weight() {
        assert_eq!(palw_seat_availability_factor_v1(None, 10_000), 500);
        let mut row = PalwSeatAvailabilityV1::default();
        row.note_assigned(100);
        row.note_assigned(200);
        assert_eq!(row.factor_permille(300), 500, "assigned twice, credited never: still new, never lower");
        assert_eq!(palw_seat_availability_scale_weight_v1(130_000, 500), 65_000);
        assert_eq!(palw_seat_availability_scale_weight_v1(1, 500), 1, "a weight never vanishes from the race");
    }

    #[test]
    fn three_credits_make_a_seat_normal_and_thirty_seasoned_credits_make_it_high() {
        let mut row = PalwSeatAvailabilityV1::default();
        for d in [10u64, 20, 30] {
            row.note_assigned(d);
            row.note_credited(d);
        }
        assert_eq!(row.factor_permille(40), 1_000);
        // Thirty credits at 30 assignments, but not seasoned (first credit 10 DAA ago).
        let mut hot = PalwSeatAvailabilityV1::default();
        for i in 0..30u64 {
            hot.note_assigned(i);
            hot.note_credited(i);
        }
        assert_eq!(hot.factor_permille(40), 1_000, "high needs a first credit two windows old");
        let window = PALW_AVAIL_EPOCH_DAA_V1 * PALW_AVAIL_WINDOW_EPOCHS_V1;
        // Seasoned: first credit long ago, and a full window of successes now.
        let mut seasoned = PalwSeatAvailabilityV1 { first_credit_daa: Some(0), epochs: vec![] };
        let now = 3 * window;
        for i in 0..30u64 {
            seasoned.note_assigned(now - 100 + i);
            seasoned.note_credited(now - 100 + i);
        }
        assert_eq!(seasoned.factor_permille(now), 1_200);
        // One more unanswered assignment per ten drops the rate under 90 %: back to normal, never below.
        for i in 0..4u64 {
            seasoned.note_assigned(now - 50 + i);
        }
        assert_eq!(seasoned.factor_permille(now), 1_000);
    }

    #[test]
    fn credits_leave_the_window_and_the_factor_decays_to_half() {
        let mut row = PalwSeatAvailabilityV1::default();
        for d in [10u64, 20, 30, 40] {
            row.note_credited(d);
        }
        assert_eq!(row.factor_permille(100), 1_000);
        let window = PALW_AVAIL_EPOCH_DAA_V1 * PALW_AVAIL_WINDOW_EPOCHS_V1;
        assert_eq!(row.factor_permille(window + 2 * PALW_AVAIL_EPOCH_DAA_V1), 500, "inactive: its credits have left the window");
    }

    #[test]
    fn the_row_is_a_pure_function_of_the_writes_in_any_split_of_time() {
        // Determinism under replay: the same writes give the same row, and old epochs are pruned by the write, not by reading.
        let writes: Vec<(bool, u64)> = (0..200u64).map(|i| (i % 3 != 0, i * 311)).collect();
        let fold = |ws: &[(bool, u64)]| {
            let mut row = PalwSeatAvailabilityV1::default();
            for (credit, d) in ws {
                row.note_assigned(*d);
                if *credit {
                    row.note_credited(*d);
                }
            }
            row
        };
        assert_eq!(fold(&writes), fold(&writes));
        let row = fold(&writes);
        assert!(row.epochs.len() <= PALW_AVAIL_WINDOW_EPOCHS_V1 as usize);
        assert!(row.epochs.windows(2).all(|w| w[0].epoch < w[1].epoch));
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(borsh::from_slice::<PalwSeatAvailabilityV1>(&bytes).unwrap(), row);
    }
}
