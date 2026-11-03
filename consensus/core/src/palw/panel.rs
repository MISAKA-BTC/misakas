//! Deterministic PALW Panel selection.
//!
//! This module deliberately has no dependency on peer discovery, compute credit, or
//! activation, or persisted consensus state. The caller supplies the already
//! accepted future anchor and the candidate declarations; equal inputs produce
//! equal Panels on every node.

use blake2b_simd::Params as Blake2bParams;
use kaspa_hashes::Hash64;

use crate::BlockHash;

/// Keyed-BLAKE2b-512 domain for PALW Panel sortition.
pub const PANEL_SORTITION_KEY: &[u8] = b"misaka-palw-panel-sortition-v1";

/// Deterministically select the Panel for a PALW claim.
///
/// Selection happens after the Producer's claim is accepted. The sortition
/// depends only on the claim id, Producer id, future anchor, determinism class,
/// candidate declarations, and requested Panel size. The Producer is excluded,
/// candidates from other determinism classes are excluded, and candidate order
/// cannot affect the result.
pub fn select_panel_verifiers(
    claim_id: Hash64,
    producer_id: Hash64,
    future_anchor: BlockHash,
    runtime_class_id: Hash64,
    candidates: &[(Hash64, Hash64)],
    panel_size: usize,
) -> Vec<Hash64> {
    if panel_size == 0 {
        return Vec::new();
    }

    let mut ticketed: Vec<(Hash64, Hash64)> = candidates
        .iter()
        .filter(|(id, class)| *id != producer_id && *class == runtime_class_id)
        .map(|(candidate, _)| {
            let mut hasher = Blake2bParams::new().hash_length(64).key(PANEL_SORTITION_KEY).to_state();
            hasher.update(claim_id.as_byte_slice());
            hasher.update(producer_id.as_byte_slice());
            hasher.update(future_anchor.as_bytes().as_slice());
            hasher.update(candidate.as_byte_slice());
            let mut out = [0u8; 64];
            out.copy_from_slice(hasher.finalize().as_bytes());
            (Hash64::from_bytes(out), *candidate)
        })
        .collect();

    ticketed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    ticketed.truncate(panel_size);
    ticketed.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h64(value: u8) -> Hash64 {
        Hash64::from_bytes([value; 64])
    }

    fn candidates(ids: &[u8], class: Hash64) -> Vec<(Hash64, Hash64)> {
        ids.iter().map(|id| (h64(*id), class)).collect()
    }

    #[test]
    fn excludes_producer_and_is_independent_of_candidate_order() {
        let class = h64(200);
        let producer = h64(3);
        let forward = candidates(&(0u8..10).collect::<Vec<_>>(), class);
        let mut reversed = forward.clone();
        reversed.reverse();

        let selected = select_panel_verifiers(h64(100), producer, BlockHash::from_bytes([1; 64]), class, &forward, 3);
        assert_eq!(selected.len(), 3);
        assert!(!selected.contains(&producer));
        assert_eq!(selected, select_panel_verifiers(h64(100), producer, BlockHash::from_bytes([1; 64]), class, &reversed, 3));
    }

    #[test]
    fn future_anchor_changes_the_panel() {
        let class = h64(200);
        let pool = candidates(&(0u8..10).collect::<Vec<_>>(), class);
        let first = select_panel_verifiers(h64(100), h64(3), BlockHash::from_bytes([1; 64]), class, &pool, 3);
        let second = select_panel_verifiers(h64(100), h64(3), BlockHash::from_bytes([2; 64]), class, &pool, 3);
        assert_ne!(first, second);
    }

    #[test]
    fn filters_other_classes_and_handles_small_pools() {
        let class = h64(200);
        let other = h64(201);
        let mixed = vec![(h64(1), class), (h64(2), other), (h64(3), class)];
        let selected = select_panel_verifiers(h64(100), h64(1), BlockHash::from_bytes([1; 64]), class, &mixed, 3);
        assert_eq!(selected, vec![h64(3)]);
        assert!(select_panel_verifiers(h64(100), h64(1), BlockHash::from_bytes([1; 64]), class, &mixed, 0).is_empty());
    }
}
