//! Merge native Batch handoffs into the existing accepted lifecycle order.
//! The indices are internal counts of accepted ordinary objects, never wire data.

pub(crate) enum AcceptedCourtItemV1<'a, T, B> {
    Ordinary(&'a T),
    Batch(&'a B),
}

pub(crate) fn in_accepted_order_v1<'a, T, B>(
    ordinary: &'a [T],
    batch: &'a [(usize, B)],
) -> Option<impl Iterator<Item = AcceptedCourtItemV1<'a, T, B>>> {
    if batch.windows(2).any(|pair| pair[0].0 > pair[1].0) || batch.last().is_some_and(|last| last.0 > ordinary.len()) {
        return None;
    }
    let mut index = 0;
    let mut pending = batch.iter().peekable();
    Some(std::iter::from_fn(move || {
        if pending.peek().is_some_and(|(before, _)| *before == index) {
            return pending.next().map(|(_, handoff)| AcceptedCourtItemV1::Batch(handoff));
        }
        let object = ordinary.get(index)?;
        index += 1;
        Some(AcceptedCourtItemV1::Ordinary(object))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_handoffs_keep_same_position_order_and_follow_the_ordinary_notice() {
        let ordinary = ["setup", "notice", "close"];
        let batch = [(0, "earlier"), (2, "open"), (2, "begin"), (2, "part"), (2, "finish"), (3, "later")];
        let accepted: Vec<_> = in_accepted_order_v1(&ordinary, &batch)
            .unwrap()
            .map(|event| match event {
                AcceptedCourtItemV1::Ordinary(object) | AcceptedCourtItemV1::Batch(object) => *object,
            })
            .collect();
        assert_eq!(accepted, ["earlier", "setup", "notice", "open", "begin", "part", "finish", "close", "later"]);
        assert!(in_accepted_order_v1(&ordinary, &[(2, "late"), (1, "early")]).is_none());
        assert!(in_accepted_order_v1(&ordinary, &[(4, "invalid")]).is_none());
        assert_eq!(in_accepted_order_v1(&ordinary[..0], &[(0, "one")]).unwrap().count(), 1);
        assert_eq!(in_accepted_order_v1(&ordinary, &[] as &[(usize, &str)]).unwrap().count(), ordinary.len());
    }
}
