//! Read-only first-fit for rigid, nonoverlapping causal service windows.
//! Only existing residual slots are visited: absent keys have the full budget.
//! No book clone, allocation, signature/domain change, or admission occurs here.
//! The caller must still perform native reserve once on the selected packets;
//! identity validation and independent holding capacity remain its responsibility.
use crate::palw_court_service_v1::{ServiceCost, ServiceReservations};

pub(crate) fn first_fit_v2(
    book: &ServiceReservations,
    costs: &[ServiceCost],
    first: u64,
    last: u64,
    span: u64,
) -> Result<Option<u64>, &'static str> {
    first_fit_width_v2(book, costs, first, last, span, span)
}

/// `spacing` separates successive releases; `width` is the half-open service
/// opportunity of each phase. Nonoverlap is required because this read-only
/// probe cannot spend the same residual capacity for two fresh phases.
/// A width of one makes the later native reservation's assigned DAA exact.
pub(crate) fn first_fit_width_v2(
    book: &ServiceReservations,
    costs: &[ServiceCost],
    first: u64,
    last: u64,
    spacing: u64,
    width: u64,
) -> Result<Option<u64>, &'static str> {
    if width == 0 || spacing < width || costs.is_empty() {
        return Err("empty or overlapping native calendar");
    }
    if costs.iter().any(|cost| !fits(book.budget, *cost)) {
        return Ok(None);
    }
    let mut start = first;
    if let Some(observed) = book.last_observed_daa {
        let Some(next) = observed.checked_add(1) else {
            return Ok(None);
        };
        start = start.max(next);
    }
    'search: while start <= last {
        for (ordinal, cost) in costs.iter().copied().enumerate() {
            let offset = u64::try_from(ordinal)
                .ok()
                .and_then(|i| i.checked_mul(spacing))
                .ok_or("calendar offset overflow")?;
            let from = start
                .checked_add(offset)
                .ok_or("calendar release overflow")?;
            let end = from
                .checked_add(width)
                .ok_or("calendar phase end overflow")?;
            let limit = last
                .checked_add(offset)
                .and_then(|n| n.checked_add(width))
                .ok_or("calendar final phase overflow")?;
            let Some(available) = first_usable_slot(book, cost, from, limit)? else {
                return Ok(None);
            };
            if available >= end {
                // Every earlier start leaves this entire phase before the
                // first usable slot. Jump exactly to the first start whose
                // half-open phase contains it; no feasible start is skipped.
                start = available
                    .checked_sub(offset)
                    .and_then(|n| n.checked_sub(width - 1))
                    .ok_or("calendar jump overflow")?;
                continue 'search;
            }
        }
        return Ok(Some(start));
    }
    Ok(None)
}

fn fits(residual: ServiceCost, cost: ServiceCost) -> bool {
    residual.bytes >= cost.bytes
        && residual.work >= cost.work
        && residual.io >= cost.io
        && residual.controls >= cost.controls
}

fn first_usable_slot(
    book: &ServiceReservations,
    cost: ServiceCost,
    from: u64,
    until: u64,
) -> Result<Option<u64>, &'static str> {
    let mut next = from;
    for (daa, residual) in book.remaining.range(from..until) {
        if *daa > next {
            return Ok(Some(next));
        }
        if fits(*residual, cost) {
            return Ok(Some(*daa));
        }
        next = daa
            .checked_add(1)
            .ok_or("calendar residual clock overflow")?;
    }
    Ok((next < until).then_some(next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_service_v1::ServicePacket;
    fn cost() -> ServiceCost {
        ServiceCost {
            bytes: 1,
            work: 1,
            io: 1,
            controls: 1,
        }
    }
    fn packet(id: u64, release: u64, before: u64) -> ServicePacket {
        let mut key = [0; 64];
        key[..8].copy_from_slice(&id.to_le_bytes());
        ServicePacket {
            id: key,
            release_daa: release,
            service_before: before,
            cost: cost(),
        }
    }
    #[test]
    fn family_calendar_sparse_firstfit_matches_literal_native_reserve() {
        for occupied in 0u16..256 {
            let mut book = ServiceReservations::new(cost()).unwrap();
            let old = (0..8)
                .filter(|i| occupied & (1 << i) != 0)
                .map(|i| packet(i, 2 + i, 3 + i))
                .collect::<Vec<_>>();
            book.reserve(&old, 10).unwrap();
            for count in 1..=3 {
                for span in 1..=3 {
                    let costs = vec![cost(); count];
                    let expected = (2..=6).find(|start| {
                        let fresh = (0..count)
                            .map(|i| {
                                packet(
                                    100 + i as u64,
                                    start + i as u64 * span,
                                    start + (i as u64 + 1) * span,
                                )
                            })
                            .collect::<Vec<_>>();
                        book.clone().reserve(&fresh, 15).is_ok()
                    });
                    let before = book.clone();
                    assert_eq!(first_fit_v2(&book, &costs, 2, 6, span).unwrap(), expected);
                    assert_eq!(book, before);
                }
            }
        }
    }
    #[test]
    fn family_calendar_sparse_width_spacing_matches_literal_native_reserve() {
        for occupied in 0u16..256 {
            let mut book = ServiceReservations::new(cost()).unwrap();
            let old = (0..8)
                .filter(|i| occupied & (1 << i) != 0)
                .map(|i| packet(i, 2 + i, 3 + i))
                .collect::<Vec<_>>();
            book.reserve(&old, 10).unwrap();
            for count in 1..=3 {
                for spacing in 1..=4 {
                    for width in 1..=spacing {
                        let costs = vec![cost(); count];
                        let expected = (2..=6).find(|start| {
                            let fresh = (0..count)
                                .map(|i| {
                                    let release = start + i as u64 * spacing;
                                    packet(100 + i as u64, release, release + width)
                                })
                                .collect::<Vec<_>>();
                            book.clone().reserve(&fresh, 20).is_ok()
                        });
                        let before = book.clone();
                        assert_eq!(
                            first_fit_width_v2(&book, &costs, 2, 6, spacing, width).unwrap(),
                            expected
                        );
                        assert_eq!(book, before);
                    }
                }
            }
        }
    }
    #[test]
    fn family_calendar_one_daa_width_preserves_exact_native_assignment_stride() {
        let mut book = ServiceReservations::new(cost()).unwrap();
        book.reserve(
            &[
                packet(1, 100, 101),
                packet(2, 104, 105),
                packet(3, 109, 110),
            ],
            110,
        )
        .unwrap();
        let first = first_fit_width_v2(&book, &[cost(), cost(), cost()], 100, 105, 4, 1)
            .unwrap()
            .unwrap();
        assert_eq!(first, 102);
        let phases = (0..3)
            .map(|i| packet(100 + i, first + i * 4, first + i * 4 + 1))
            .collect::<Vec<_>>();
        book.reserve(&phases, 111).unwrap();
        for phase in &phases {
            assert_eq!(book.assigned_daa(&phase.id), Some(phase.release_daa));
        }
        for pair in phases.windows(2) {
            assert_eq!(
                book.assigned_daa(&pair[1].id).unwrap() - book.assigned_daa(&pair[0].id).unwrap(),
                4
            );
        }
        assert!(first_fit_width_v2(&book, &[cost()], 112, 120, 1, 2).is_err());
        assert!(first_fit_width_v2(&book, &[cost()], 112, 120, 4, 0).is_err());
    }
    #[test]
    fn family_calendar_sparse_firstfit_crosses_full_book_without_trial_clones() {
        let mut book = ServiceReservations::new(cost()).unwrap();
        let old = (0..1024)
            .map(|i| packet(i, 100 + i, 101 + i))
            .collect::<Vec<_>>();
        book.reserve(&old, 1124).unwrap();
        assert_eq!(
            first_fit_v2(&book, &[cost(), cost(), cost()], 100, 1_000_000, 4).unwrap(),
            Some(1121)
        );
        assert_eq!(first_fit_v2(&book, &[cost()], 100, 1120, 4).unwrap(), None);
        book.last_observed_daa = Some(2000);
        assert_eq!(
            first_fit_v2(&book, &[cost()], 100, 3000, 4).unwrap(),
            Some(2001)
        );
    }
}
