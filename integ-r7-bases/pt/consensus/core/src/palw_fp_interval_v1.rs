//! **The seat's interval draw** (ADR-0077 Decision 8).
//!
//! A free-prompt seat no longer fetches the whole capture and hashes it. It draws `k` checkpoint
//! intervals from the claim's beacon and its own seat index — unpredictable when the commitment
//! was fixed (the beacon does not exist yet, ADR-0044 F4/F5), different per seat — asks the
//! executor for each interval's opening, replays it with the class's own kernels and compares
//! every row exactly. This module is the draw alone: a pure function of chain facts, pinned by a
//! golden vector, so that "which intervals did seat 3 have to check" is a question anyone can
//! answer from the chain and nobody can answer before the beacon lands.
//!
//! The draw is NOT a consensus object: what a seat checked before signing is the seat's own duty
//! (the receipt object and its signature are unchanged). It is pinned anyway, because a seat that
//! could pick its own intervals could pick the ones an executor knows it will pick.

use kaspa_hashes::Hash64;

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// `k`: how many intervals a seat opens per claim. Four, as the sampled-leaf arm it replaces
/// drew; a skipped inference is wrong at every interval and any sample catches it, a single
/// corrupted row is the court's question and the court is cheap.
pub const PALW_FP_SEAT_INTERVAL_SAMPLES_V1: u32 = 4;

pub const PALW_FP_INTERVAL_DRAW_DOMAIN_V1: &[u8] = b"misaka-palw/fp-v3/seat-interval-draw/v1";

/// The intervals seat `seat_index` of the panel bound under `beacon_block` must open for
/// `claim_id`: `k` DISTINCT indices in `0..interval_count`, in draw order. When the claim has no
/// more than `k` intervals, every interval, in order — a short job is checked whole.
///
/// Draw `i` is `H(domain ‖ network_domain ‖ beacon_block ‖ claim_id ‖ seat_index ‖ i)` reduced
/// modulo `interval_count`; a repeat is skipped and the next draw taken, so the result never
/// depends on anything but those five inputs.
pub fn palw_fp_interval_draw_v1(
    network_domain: &Hash64,
    beacon_block: &Hash64,
    claim_id: &Hash64,
    seat_index: u8,
    k: u32,
    interval_count: u32,
) -> Vec<u32> {
    if interval_count == 0 || k == 0 {
        return Vec::new();
    }
    if interval_count <= k {
        return (0..interval_count).collect();
    }
    let mut out: Vec<u32> = Vec::with_capacity(k as usize);
    let mut draw: u64 = 0;
    while out.len() < k as usize {
        let mut state = keyed(PALW_FP_INTERVAL_DRAW_DOMAIN_V1);
        state.update(network_domain.as_byte_slice());
        state.update(beacon_block.as_byte_slice());
        state.update(claim_id.as_byte_slice());
        state.update(&[seat_index]);
        state.update(&draw.to_le_bytes());
        let digest = finish(state);
        let word = u64::from_le_bytes(digest.as_byte_slice()[..8].try_into().expect("8 bytes"));
        let index = (word % interval_count as u64) as u32;
        if !out.contains(&index) {
            out.push(index);
        }
        draw += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    #[test]
    fn a_short_job_is_checked_whole_and_a_long_one_is_sampled_distinctly() {
        assert_eq!(palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 3), vec![0, 1, 2]);
        assert_eq!(palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 4), vec![0, 1, 2, 3]);
        let long = palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 1_000);
        assert_eq!(long.len(), 4);
        let mut sorted = long.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "distinct");
        assert!(long.iter().all(|i| *i < 1_000));
        assert!(palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 0).is_empty());
    }

    /// Every input moves the draw: a different beacon, claim or seat is a different set.
    #[test]
    fn the_draw_is_beacon_claim_and_seat_bound() {
        let base = palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 100_000);
        assert_ne!(base, palw_fp_interval_draw_v1(&h(1), &h(9), &h(3), 0, 4, 100_000), "beacon");
        assert_ne!(base, palw_fp_interval_draw_v1(&h(1), &h(2), &h(9), 0, 4, 100_000), "claim");
        assert_ne!(base, palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 1, 4, 100_000), "seat");
        assert_ne!(base, palw_fp_interval_draw_v1(&h(9), &h(2), &h(3), 0, 4, 100_000), "network");
        assert_eq!(base, palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 100_000), "pure");
    }

    /// Frozen: the draw is a rule seats and executors both compute, so it cannot drift.
    #[test]
    fn golden_vector_is_frozen() {
        let draw = palw_fp_interval_draw_v1(&h(0x11), &h(0x22), &h(0x33), 2, 4, 1_000);
        assert_eq!(draw, vec![645, 20, 372, 227], "re-take deliberately, with a new domain suffix");
    }
}
