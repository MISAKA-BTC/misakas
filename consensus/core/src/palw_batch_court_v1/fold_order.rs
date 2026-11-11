//! Merge native court handoffs into the existing accepted lifecycle order.
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

pub enum AcceptedNativeCourtItemV1<T, B, R> {
    Ordinary(T),
    Batch(B),
    Tokenizer(R),
}

/// Merge native carriers around ordinary objects. Each position counts
/// ordinary objects before the carrier. At a shared position, the original
/// accepted transaction ordinal determines the order across both kinds.
/// The caller must derive positions and ordinals from the accepted stream.
pub fn in_accepted_native_order_v1<'a, T, B, R>(
    mut ordinary: impl ExactSizeIterator<Item = T>,
    mut batch: &'a [(usize, B)],
    mut tokenizer: &'a [(usize, R)],
    batch_ordinal: impl Fn(&B) -> usize,
    tokenizer_ordinal: impl Fn(&R) -> usize,
) -> Option<impl Iterator<Item = AcceptedNativeCourtItemV1<T, &'a B, &'a R>>> {
    let mut validate_batch = batch;
    let mut validate_tokenizer = tokenizer;
    let mut previous = None;
    while let Some((position, ordinal, _)) =
        next_native_v1::<T, _, _>(&mut validate_batch, &mut validate_tokenizer, &batch_ordinal, &tokenizer_ordinal)
    {
        if position > ordinary.len()
            || previous.is_some_and(|(prior_position, prior_ordinal)| position < prior_position || ordinal <= prior_ordinal)
        {
            return None;
        }
        previous = Some((position, ordinal));
    }

    let mut index = 0;
    let mut pending = next_native_v1(&mut batch, &mut tokenizer, &batch_ordinal, &tokenizer_ordinal);
    Some(std::iter::from_fn(move || {
        if pending.as_ref().is_some_and(|(position, _, _)| *position == index) {
            let (_, _, item) = pending.take()?;
            pending = next_native_v1(&mut batch, &mut tokenizer, &batch_ordinal, &tokenizer_ordinal);
            return Some(item);
        }
        let object = ordinary.next()?;
        index += 1;
        Some(AcceptedNativeCourtItemV1::Ordinary(object))
    }))
}

fn next_native_v1<'a, T, B, R>(
    batch: &mut &'a [(usize, B)],
    tokenizer: &mut &'a [(usize, R)],
    batch_ordinal: &impl Fn(&B) -> usize,
    tokenizer_ordinal: &impl Fn(&R) -> usize,
) -> Option<(usize, usize, AcceptedNativeCourtItemV1<T, &'a B, &'a R>)> {
    let batch_key = batch.first().map(|(position, handoff)| (*position, batch_ordinal(handoff)));
    let tokenizer_key = tokenizer.first().map(|(position, handoff)| (*position, tokenizer_ordinal(handoff)));
    if let Some((position, ordinal)) = batch_key.filter(|key| tokenizer_key.is_none_or(|other| *key <= other)) {
        let ((_, handoff), rest) = batch.split_first()?;
        *batch = rest;
        Some((position, ordinal, AcceptedNativeCourtItemV1::Batch(handoff)))
    } else {
        let (position, ordinal) = tokenizer_key?;
        let ((_, handoff), rest) = tokenizer.split_first()?;
        *tokenizer = rest;
        Some((position, ordinal, AcceptedNativeCourtItemV1::Tokenizer(handoff)))
    }
}

pub enum AcceptedPublicNativeCourtItemV2<T, B, R, M> {
    Ordinary(T),
    Batch(B),
    Tokenizer(R),
    Manifest(M),
}

/// Merge the three native lanes by accepted transaction ordinal at each
/// position. The caller derives each position as the number of ordinary
/// objects before the handoff, including any derived ordinary objects.
/// Validate the full order before yielding any item, using only slice cursors.
/// Each accessor must read the handoff's fixed accepted transaction ordinal.
pub fn in_accepted_public_native_order_v2<'a, T, B, R, M>(
    mut ordinary: impl ExactSizeIterator<Item = T>,
    mut batch: &'a [(usize, B)],
    mut tokenizer: &'a [(usize, R)],
    mut manifest: &'a [(usize, M)],
    batch_ordinal: impl Fn(&B) -> usize,
    tokenizer_ordinal: impl Fn(&R) -> usize,
    manifest_ordinal: impl Fn(&M) -> usize,
) -> Option<impl Iterator<Item = AcceptedPublicNativeCourtItemV2<T, &'a B, &'a R, &'a M>>> {
    let mut validate_batch = batch;
    let mut validate_tokenizer = tokenizer;
    let mut validate_manifest = manifest;
    let mut previous = None;
    while let Some((position, ordinal, _)) = next_public_native_v2::<T, _, _, _>(
        &mut validate_batch,
        &mut validate_tokenizer,
        &mut validate_manifest,
        &batch_ordinal,
        &tokenizer_ordinal,
        &manifest_ordinal,
    ) {
        if position > ordinary.len()
            || previous.is_some_and(|(prior_position, prior_ordinal)| position < prior_position || ordinal <= prior_ordinal)
        {
            return None;
        }
        previous = Some((position, ordinal));
    }

    let mut next = move || {
        next_public_native_v2(&mut batch, &mut tokenizer, &mut manifest, &batch_ordinal, &tokenizer_ordinal, &manifest_ordinal)
    };
    let mut index = 0;
    let mut pending = next();
    Some(std::iter::from_fn(move || {
        if pending.as_ref().is_some_and(|(position, _, _)| *position == index) {
            let (_, _, item) = pending.take()?;
            pending = next();
            return Some(item);
        }
        let object = ordinary.next()?;
        index += 1;
        Some(AcceptedPublicNativeCourtItemV2::Ordinary(object))
    }))
}

fn next_public_native_v2<'a, T, B, R, M>(
    batch: &mut &'a [(usize, B)],
    tokenizer: &mut &'a [(usize, R)],
    manifest: &mut &'a [(usize, M)],
    batch_ordinal: &impl Fn(&B) -> usize,
    tokenizer_ordinal: &impl Fn(&R) -> usize,
    manifest_ordinal: &impl Fn(&M) -> usize,
) -> Option<(usize, usize, AcceptedPublicNativeCourtItemV2<T, &'a B, &'a R, &'a M>)> {
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum Lane {
        Batch,
        Tokenizer,
        Manifest,
    }

    let (position, ordinal, lane) = [
        batch.first().map(|(position, handoff)| (*position, batch_ordinal(handoff), Lane::Batch)),
        tokenizer.first().map(|(position, handoff)| (*position, tokenizer_ordinal(handoff), Lane::Tokenizer)),
        manifest.first().map(|(position, handoff)| (*position, manifest_ordinal(handoff), Lane::Manifest)),
    ]
    .into_iter()
    .flatten()
    .min()?;
    let item = match lane {
        Lane::Batch => {
            let ((_, handoff), rest) = batch.split_first()?;
            *batch = rest;
            AcceptedPublicNativeCourtItemV2::Batch(handoff)
        }
        Lane::Tokenizer => {
            let ((_, handoff), rest) = tokenizer.split_first()?;
            *tokenizer = rest;
            AcceptedPublicNativeCourtItemV2::Tokenizer(handoff)
        }
        Lane::Manifest => {
            let ((_, handoff), rest) = manifest.split_first()?;
            *manifest = rest;
            AcceptedPublicNativeCourtItemV2::Manifest(handoff)
        }
    };
    Some((position, ordinal, item))
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
    fn native_handoffs_merge_by_accepted_ordinal_at_same_position() {
        let ordinary = ["setup", "close"];
        let batch = [(0, (1, "prior batch")), (1, (5, "batch")), (2, (12, "final batch"))];
        let tokenizer = [(1, (4, "open")), (1, (7, "part")), (2, (9, "finish"))];
        let ordinal = |handoff: &(usize, &str)| handoff.0;
        let accepted: Vec<_> = in_accepted_native_order_v1(ordinary.into_iter(), &batch, &tokenizer, ordinal, ordinal)
            .unwrap()
            .map(|item| match item {
                AcceptedNativeCourtItemV1::Ordinary(label) => label,
                AcceptedNativeCourtItemV1::Batch((_, label)) | AcceptedNativeCourtItemV1::Tokenizer((_, label)) => *label,
            })
            .collect();
        assert_eq!(accepted, ["prior batch", "setup", "open", "batch", "part", "close", "finish", "final batch"]);
    }

    #[test]
    fn native_handoffs_refuse_invalid_positions_and_ordinals() {
        let rejects = |batch: &[(usize, usize)], tokenizer: &[(usize, usize)]| {
            in_accepted_native_order_v1([(), ()].into_iter(), batch, tokenizer, |n| *n, |n| *n).is_none()
        };
        assert!(rejects(&[(0, 3)], &[(0, 3)]));
        assert!(rejects(&[(0, 4)], &[(1, 3)]));
        assert!(rejects(&[(1, 3), (0, 4)], &[]));
        assert!(rejects(&[], &[(1, 4), (1, 3)]));
        assert!(rejects(&[], &[(0, 3), (1, 3)]));
        assert!(rejects(&[(3, 5)], &[]));
        assert!(rejects(&[], &[(3, 5)]));
        assert_eq!(in_accepted_native_order_v1([()].into_iter(), &[], &[(1, 5)], |n: &usize| *n, |n| *n).unwrap().count(), 2);
    }

    #[test]
    fn public_native_handoffs_merge_all_three_lanes_at_one_accepted_position() {
        use AcceptedPublicNativeCourtItemV2::{Batch, Manifest, Ordinary, Tokenizer};
        let ordinary = ["draw", "lifecycle object"];
        let batch = [(0, (1, "earlier Batch control")), (1, (8, "Batch Open")), (2, (13, "trailing Batch response"))];
        let tokenizer = [(1, (5, "tokenizer Part")), (2, (11, "trailing tokenizer Finish"))];
        let manifest = [(1, (4, "paid MaterialRequest")), (1, (9, "second paid MaterialRequest")), (2, (12, "trailing paid request"))];
        let ordinal = |handoff: &(usize, &str)| handoff.0;
        let order: Vec<_> =
            in_accepted_public_native_order_v2(ordinary.into_iter(), &batch, &tokenizer, &manifest, ordinal, ordinal, ordinal)
                .unwrap()
                .map(|item| match item {
                    Ordinary(label) => ("ordinary", label),
                    Batch((_, label)) => ("batch", *label),
                    Tokenizer((_, label)) => ("tokenizer", *label),
                    Manifest((_, label)) => ("manifest", *label),
                })
                .collect();
        assert_eq!(
            order,
            [
                ("batch", "earlier Batch control"),
                ("ordinary", "draw"),
                ("manifest", "paid MaterialRequest"),
                ("tokenizer", "tokenizer Part"),
                ("batch", "Batch Open"),
                ("manifest", "second paid MaterialRequest"),
                ("ordinary", "lifecycle object"),
                ("tokenizer", "trailing tokenizer Finish"),
                ("manifest", "trailing paid request"),
                ("batch", "trailing Batch response"),
            ],
        );
    }

    #[test]
    fn public_native_handoffs_reject_ambiguous_ordinals_and_positions_before_iteration() {
        let merge = |batch: &[(usize, usize)], tokenizer: &[(usize, usize)], manifest: &[(usize, usize)]| {
            in_accepted_public_native_order_v2([(), ()].into_iter(), batch, tokenizer, manifest, |n| *n, |n| *n, |n| *n).is_none()
        };
        assert!(merge(&[(1, 7)], &[], &[(1, 7)])); // Different lanes, same ordinal and position.
        assert!(merge(&[], &[(0, 7)], &[(1, 7)])); // Different lanes and positions, same ordinal.
        assert!(merge(&[], &[], &[(1, 7), (1, 7)]));
        assert!(merge(&[], &[], &[(1, 7), (1, 6)]));
        assert!(merge(&[], &[], &[(1, 7), (0, 8)]));
        assert!(merge(&[(0, 8)], &[], &[(1, 7)])); // Positions contradict accepted ordinals.
        assert!(merge(&[], &[], &[(3, 7)]));

        let ordinal = |n: &usize| *n;
        let tail = in_accepted_public_native_order_v2([()].into_iter(), &[], &[], &[(1, 2)], ordinal, ordinal, ordinal).unwrap();
        assert_eq!(tail.count(), 2);
        let native_only =
            in_accepted_public_native_order_v2(std::iter::empty::<()>(), &[], &[], &[(0, 0)], ordinal, ordinal, ordinal).unwrap();
        assert_eq!(native_only.count(), 1);
        let ordinary_only = in_accepted_public_native_order_v2([()].into_iter(), &[], &[], &[], ordinal, ordinal, ordinal).unwrap();
        assert_eq!(ordinary_only.count(), 1);
    }

    #[test]
    fn indexing_preserves_batch_order_around_extracted_objects_and_skips() {
        let candidates = index_batch_carriers_v1([1, 2, 3, 4, 5, 6, 7], [2, 6], |&id| [1, 4, 5, 7].contains(&id).then_some(id));
        assert_eq!(candidates, Some(vec![(0, 1), (1, 4), (1, 5), (2, 7)]));
        assert!(index_batch_carriers_v1([1, 2], [2, 1], |_| None::<u8>).is_none());
    }
}
