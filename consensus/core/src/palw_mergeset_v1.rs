//! ADR-0125 semantic amendment: a PALW semantic view over the unchanged raw GHOSTDAG mergeset.
//!
//! Algo-10 execution rounds remain raw reds. Classification does not grant a permit, change
//! colouring, order blocks, or contribute score/work/DAA. Each partition retains its input order.

use crate::BlockHash;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassifiedMergesetV1 {
    blues: Vec<BlockHash>,
    genuine_reds: Vec<BlockHash>,
    rounds: Vec<BlockHash>,
}

impl ClassifiedMergesetV1 {
    pub fn blues(&self) -> &[BlockHash] {
        &self.blues
    }

    /// Ordinary GHOSTDAG reds only; never execution rounds, including refused rounds.
    pub fn genuine_reds(&self) -> &[BlockHash] {
        &self.genuine_reds
    }

    /// Execution rounds, independent of their acceptance verdict.
    pub fn rounds(&self) -> &[BlockHash] {
        &self.rounds
    }
}

/// Classify using immutable headers (`pow_algo_id == POW_ALGO_ID_PALW_ROUND_V1`) or the
/// header-derived round set already resolved by the acceptance processor. Never use a verdict:
/// a refused round is still a round. The selected parent is included if present in `blues`.
pub fn classify_palw_mergeset_v1(
    blues: impl IntoIterator<Item = BlockHash>,
    raw_reds: impl IntoIterator<Item = BlockHash>,
    mut is_round: impl FnMut(BlockHash) -> bool,
) -> ClassifiedMergesetV1 {
    let result = try_classify_palw_mergeset_v1(blues, raw_reds, |hash| Ok::<_, std::convert::Infallible>(is_round(hash)));
    match result {
        Ok(view) => view,
        Err(never) => match never {},
    }
}

/// A missing header is an error, never evidence that a member is a genuine red.
pub fn try_classify_palw_mergeset_v1<E>(
    blues: impl IntoIterator<Item = BlockHash>,
    raw_reds: impl IntoIterator<Item = BlockHash>,
    mut is_round: impl FnMut(BlockHash) -> Result<bool, E>,
) -> Result<ClassifiedMergesetV1, E> {
    let mut view = ClassifiedMergesetV1::default();
    for member in blues {
        if is_round(member)? {
            view.rounds.push(member);
        } else {
            view.blues.push(member);
        }
    }
    for member in raw_reds {
        if is_round(member)? {
            view.rounds.push(member);
        } else {
            view.genuine_reds.push(member);
        }
    }
    Ok(view)
}

#[cfg(test)]
mod adr0125_semantic_tests {
    use super::*;

    #[test]
    fn mixed_rounds_and_genuine_reds_are_disjoint_and_keep_raw_order() {
        let h = BlockHash::from_u64_word;
        let raw_reds = vec![h(5), h(2), h(7), h(4)];
        let view = classify_palw_mergeset_v1([h(1), h(3)], raw_reds.iter().copied(), |member| [h(2), h(4)].contains(&member));
        assert_eq!(view.blues(), &[h(1), h(3)]);
        assert_eq!(view.rounds(), &[h(2), h(4)]);
        assert_eq!(view.genuine_reds(), &[h(5), h(7)]);
        assert_eq!(raw_reds, [h(5), h(2), h(7), h(4)], "raw GHOSTDAG representation is unchanged");
    }

    #[test]
    fn missing_header_cannot_silently_turn_a_round_into_a_red() {
        let member = BlockHash::from_u64_word(2);
        let result = try_classify_palw_mergeset_v1([], [member], |_| Err::<bool, _>("missing header"));
        assert_eq!(result, Err("missing header"));
    }
}
