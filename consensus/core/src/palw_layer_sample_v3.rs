//! **ADR-0133 S3 — post-execution random layer/position sampling.**
//!
//! After a producer commits the trace, a seat that is not the full-replay seat draws a set of
//! `(layer, position)` sites from the panel bind and re-runs only those sites from the committed
//! activations (one layer's weights, the committed inputs at that position). A lie that is not
//! on a drawn site is not caught by that seat; the full-replay seat and the court still are.
//!
//! Drawn after the bind, so the producer cannot grind the sample. Not a licence of its own: S3
//! seats attest the V2 segment mask of the sites they checked when S1 is live, or a full mask
//! when they checked enough sites to cover every segment the assignment named. Armed behind
//! `Params::palw_verification_s3`.

use kaspa_hashes::Hash64;

pub const PALW_LAYER_SAMPLE_V3_DOMAIN: &[u8] = b"misaka-palw/verification-s3/sample/v1";
pub const PALW_LAYER_SAMPLE_V3_ALL_DOMAINS: &[&[u8]] = &[PALW_LAYER_SAMPLE_V3_DOMAIN];
/// Sites a partial S3 seat redraws. Enough that two independent seats collide on a lie with
/// high probability once the full seat is also in the coverage; a knob, not a security proof.
pub const PALW_LAYER_SAMPLE_V3_SITES: u16 = 8;

/// One sampled site: a layer of the registered graph at one absolute position of the job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwLayerSiteV3 {
    pub layer: u16,
    pub position: u32,
}

fn sample_draw(anchor: Hash64, claim_id: Hash64, seat_index: u16, counter: u32) -> u64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_LAYER_SAMPLE_V3_DOMAIN).to_state();
    state.update(anchor.as_bytes().as_slice());
    state.update(claim_id.as_bytes().as_slice());
    state.update(&seat_index.to_le_bytes());
    state.update(&counter.to_le_bytes());
    let digest = state.finalize();
    u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("eight bytes"))
}

/// The sites seat `seat_index` of this bind must recompute. Distinct, stable, a function of the
/// bind — never of the seat's choosing. Empty when the class has no layer or the job has no
/// position: there is nothing to sample.
pub fn palw_layer_sample_v3(
    anchor: Hash64,
    claim_id: Hash64,
    seat_index: u16,
    layer_count: u16,
    positions: u32,
    n: u16,
) -> Vec<PalwLayerSiteV3> {
    if layer_count == 0 || positions == 0 || n == 0 {
        return Vec::new();
    }
    let want = (n as usize).min(layer_count as usize * positions as usize);
    let mut out = Vec::with_capacity(want);
    let mut seen = std::collections::HashSet::new();
    let mut counter = 0u32;
    while out.len() < want && counter < 4_096 {
        let draw = sample_draw(anchor, claim_id, seat_index, counter);
        counter += 1;
        let layer = (draw % layer_count as u64) as u16;
        let position = ((draw >> 16) % positions as u64) as u32;
        let site = PalwLayerSiteV3 { layer, position };
        if seen.insert(site) {
            out.push(site);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    #[test]
    fn the_sample_is_a_function_of_the_bind_and_not_of_the_seat_choosing() {
        let a = palw_layer_sample_v3(h(1), h(2), 0, 28, 128, PALW_LAYER_SAMPLE_V3_SITES);
        let b = palw_layer_sample_v3(h(1), h(2), 0, 28, 128, PALW_LAYER_SAMPLE_V3_SITES);
        assert_eq!(a, b);
        assert_eq!(a.len(), PALW_LAYER_SAMPLE_V3_SITES as usize);
        let other_seat = palw_layer_sample_v3(h(1), h(2), 1, 28, 128, PALW_LAYER_SAMPLE_V3_SITES);
        assert_ne!(a, other_seat, "two seats of one panel draw different sites");
        let other_claim = palw_layer_sample_v3(h(1), h(3), 0, 28, 128, PALW_LAYER_SAMPLE_V3_SITES);
        assert_ne!(a, other_claim);
        assert!(a.iter().all(|s| s.layer < 28 && s.position < 128));
        let mut uniq = a.clone();
        uniq.sort_by_key(|s| (s.layer, s.position));
        uniq.dedup();
        assert_eq!(uniq.len(), a.len(), "the draw does not repeat a site");
    }

    #[test]
    fn a_class_with_no_layer_or_position_draws_nothing() {
        assert!(palw_layer_sample_v3(h(1), h(2), 0, 0, 128, 8).is_empty());
        assert!(palw_layer_sample_v3(h(1), h(2), 0, 28, 0, 8).is_empty());
        assert!(palw_layer_sample_v3(h(1), h(2), 0, 28, 128, 0).is_empty());
    }
}
