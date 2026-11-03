//! Merge native Batch handoffs into the existing accepted lifecycle order.
//! The indices are internal counts of accepted ordinary objects, never wire data.

pub enum AcceptedCourtItemV1<T, B> {
    Ordinary(T),
    Batch(B),
}

pub fn in_accepted_order_v1<T, B>(
    mut ordinary: impl ExactSizeIterator<Item = T>,
    batch: &[(usize, B)],
) -> Option<impl Iterator<Item = AcceptedCourtItemV1<T, &B>>> {
    if batch.windows(2).any(|pair| pair[0].0 > pair[1].0) || batch.last().is_some_and(|last| last.0 > ordinary.len()) {
        return None;
    }
    let mut index = 0;
    let mut pending = batch.iter().peekable();
    Some(std::iter::from_fn(move || {
        if pending.peek().is_some_and(|(before, _)| *before == index) {
            return pending.next().map(|(_, handoff)| AcceptedCourtItemV1::Batch(handoff));
        }
        let object = ordinary.next()?;
        index += 1;
        Some(AcceptedCourtItemV1::Ordinary(object))
    }))
}

/// Locate native handoffs relative to the ordinary objects extracted from
/// the same transaction stream. Skipped transaction IDs do not affect indices.
pub fn index_batch_carriers_v1<I: Eq, B>(
    transaction_ids: impl IntoIterator<Item = I>,
    ordinary_ids: impl IntoIterator<Item = I>,
    mut batch_for: impl FnMut(&I) -> Option<B>,
) -> Option<Vec<(usize, B)>> {
    let mut ordinary = ordinary_ids.into_iter().peekable();
    let mut index = 0;
    let mut indexed = Vec::new();
    for id in transaction_ids {
        if ordinary.peek() == Some(&id) {
            ordinary.next();
            index += 1;
        } else if let Some(batch) = batch_for(&id) {
            indexed.push((index, batch));
        }
    }
    ordinary.peek().is_none().then_some(indexed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_handoffs_keep_same_position_order_and_follow_the_ordinary_notice() {
        let ordinary = ["setup", "notice", "close"];
        let batch = [(0, "earlier"), (2, "open"), (2, "begin"), (2, "part"), (2, "finish"), (3, "later")];
        let accepted: Vec<_> = in_accepted_order_v1(ordinary.iter(), &batch)
            .unwrap()
            .map(|event| match event {
                AcceptedCourtItemV1::Ordinary(object) | AcceptedCourtItemV1::Batch(object) => *object,
            })
            .collect();
        assert_eq!(accepted, ["earlier", "setup", "notice", "open", "begin", "part", "finish", "close", "later"]);
        assert!(in_accepted_order_v1(ordinary.iter(), &[(2, "late"), (1, "early")]).is_none());
        assert!(in_accepted_order_v1(ordinary.iter(), &[(4, "invalid")]).is_none());
        assert_eq!(in_accepted_order_v1(ordinary[..0].iter(), &[(0, "one")]).unwrap().count(), 1);
        assert_eq!(in_accepted_order_v1(ordinary.iter(), &[] as &[(usize, &str)]).unwrap().count(), ordinary.len());
    }

    #[test]
    fn indexing_preserves_batch_order_around_extracted_objects_and_skips() {
        let candidates = index_batch_carriers_v1([1, 2, 3, 4, 5, 6, 7], [2, 6], |&id| [1, 4, 5, 7].contains(&id).then_some(id));
        assert_eq!(candidates, Some(vec![(0, 1), (1, 4), (1, 5), (2, 7)]));
        assert!(index_batch_carriers_v1([1, 2], [2, 1], |_| None::<u8>).is_none());
    }
}
