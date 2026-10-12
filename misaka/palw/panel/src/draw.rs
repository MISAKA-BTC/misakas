use crate::types::*;
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};

/// Wire v1: framed BLAKE2b-512 over this Borsh tuple (fixed hashes, LE u64 integers).
pub fn panel_seed_v3(network: Hash64, ruleset: Hash64, seal: &ClaimSealV1, snapshot: &PanelSnapshotV1, output: Hash64) -> Hash64 {
    digest("misaka-palw/panel-v3/seed", &(network, ruleset, seal.id, seal.anchor_slot, snapshot.root, seal.beacon_epoch, output))
}

// ADR-0152's integer exponential race, in Q64.64. No floating-point architecture dependence.
fn neg_log2(u: u64) -> u128 {
    let m = u as u128 + 1;
    let n = 127 - m.leading_zeros() as u128;
    let mut y = if n >= 63 { m >> (n - 63) } else { m << (63 - n) };
    let mut frac = 0;
    for i in 0..64 {
        y = (y * y) >> 63;
        if y >= 1u128 << 64 {
            y >>= 1;
            frac |= 1u128 << (63 - i);
        }
    }
    (64u128 << 64) - ((n << 64) | frac)
}

// Exact 192-bit product. Collateral is in sompi, without per-bond floor or a weight cap that
// would create first-choice gain by splitting. L <= 2^70 and W <= 2^64; u128 alone is insufficient.
fn product(l: u128, w: u64) -> [u64; 3] {
    let low = (l as u64 as u128) * w as u128;
    let high = (l >> 64) * w as u128 + (low >> 64);
    [(high >> 64) as u64, high as u64, low as u64]
}

fn ticket(seed: Hash64, retry: u16, role: u8, identity: Hash64, bond: Option<BondIdV1>) -> u128 {
    let h = digest("misaka-palw/panel-v3/seat-ticket", &(seed, retry, role, identity, bond));
    neg_log2(u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap()))
}

/// A complete, public ranking for one role/retry. Each operator is one race entry whose weight is
/// the EXACT aggregate frozen collateral. Keys are not people: different-key Sybil capture and
/// withholding/retry distributions remain activation gates. Same-key bonds cannot fill two seats.
pub fn seat_order_v1(
    snapshot: &PanelSnapshotV1,
    seed: Hash64,
    retry: u16,
    role: u8,
    used: &[Hash64],
) -> Result<Vec<BondIdV1>, PanelErrorV1> {
    seat_order_over(snapshot.candidates.iter(), seed, retry, role, used)
}

/// **Stratum `stratum`'s seed** of a stratified draw: `H("misaka-palw/panel-v3/stratum-seed" ‖ seed ‖ stratum)` — the strata are
/// independent races over one claim-sealed seed.
pub fn stratum_seed_v1(seed: Hash64, stratum: u16) -> Hash64 {
    digest("misaka-palw/panel-v3/stratum-seed", &(seed, stratum))
}

/// **The ranking of one role in stratum `stratum`** of a stratified draw: [`seat_order_v1`]'s race over the stratum's own seed
/// ([`stratum_seed_v1`]), the CLASS role restricted to the stratum's members (bit `stratum` of
/// `PanelSnapshotV1::strata_members`), the OUTSIDER role over the whole outsider population.
pub fn stratum_seat_order_v1(
    snapshot: &PanelSnapshotV1,
    stratum: u16,
    seed: Hash64,
    retry: u16,
    role: u8,
    used: &[Hash64],
) -> Result<Vec<BondIdV1>, PanelErrorV1> {
    if snapshot.strata_members.len() != snapshot.candidates.len() || stratum >= 64 {
        return Err(PanelErrorV1::InvalidSnapshot);
    }
    let members = snapshot
        .candidates
        .iter()
        .zip(&snapshot.strata_members)
        .filter(|(_, bits)| role != CLASS_ROLE_V1 || *bits & (1u64 << stratum) != 0)
        .map(|(seat, _)| seat);
    seat_order_over(members, stratum_seed_v1(seed, stratum), retry, role, used)
}

fn seat_order_over<'a>(
    candidates: impl Iterator<Item = &'a SeatCandidateV1>,
    seed: Hash64,
    retry: u16,
    role: u8,
    used: &[Hash64],
) -> Result<Vec<BondIdV1>, PanelErrorV1> {
    let used: BTreeSet<_> = used.iter().copied().collect();
    let mut groups: BTreeMap<Hash64, (u64, Vec<&SeatCandidateV1>)> = BTreeMap::new();
    for seat in candidates {
        if seat.roles & role == 0 || used.contains(&seat.operator) {
            continue;
        }
        let g = groups.entry(seat.operator).or_default();
        g.0 = g.0.checked_add(seat.collateral).ok_or(PanelErrorV1::InvalidSnapshot)?;
        g.1.push(seat);
    }
    let mut entries: Vec<_> = groups
        .into_iter()
        .map(|(op, (w, mut bonds))| {
            bonds.sort_by(|a, b| {
                product(ticket(seed, retry, role, op, Some(a.bond)), b.collateral)
                    .cmp(&product(ticket(seed, retry, role, op, Some(b.bond)), a.collateral))
                    .then(a.bond.cmp(&b.bond))
            });
            (op, w, ticket(seed, retry, role, op, None), bonds)
        })
        .collect();
    entries.sort_by(|a, b| product(a.2, b.1).cmp(&product(b.2, a.1)).then(a.0.cmp(&b.0)));
    Ok(entries.into_iter().flat_map(|e| e.3.into_iter().map(|s| s.bond)).collect())
}

/// The selected prefix of the complete operator race, in bounded working memory.
/// Input must be strictly ordered by `(operator, bond)` and include the ENTIRE frozen
/// role population. Headroom filters the selectable bond within an operator, but does
/// not reduce that operator's frozen aggregate race weight. This exactly matches the
/// full ranking followed by the bind-time headroom/operator filters.
/// This helper alone does not enlarge historical population eligibility or a policy cap.
pub fn seat_prefix_streaming_v1(
    candidates: impl IntoIterator<Item = SeatCandidateV1>,
    seed: Hash64,
    retry: u16,
    role: u8,
    used: &[Hash64],
    needed: usize,
    max_candidates: u32,
    mut has_headroom: impl FnMut(&SeatCandidateV1) -> bool,
) -> Result<Vec<BondIdV1>, PanelErrorV1> {
    if ![CLASS_ROLE_V1, OUTSIDER_ROLE_V1].contains(&role) || needed > MAX_STRATIFIED_SEATS_V1 as usize {
        return Err(PanelErrorV1::InvalidSnapshot);
    }
    let used: BTreeSet<_> = used.iter().copied().collect();
    // One group and at most `needed` winners are resident; neither all candidates nor
    // all operator groups are materialised by the selector.
    let mut winners: Vec<StreamWinnerV1> = Vec::with_capacity(needed + 1);
    let mut group: Option<(Hash64, u64, Option<(BondIdV1, u64, u128)>)> = None;
    let mut previous = None;
    let mut count = 0u32;
    for seat in candidates {
        count = count.checked_add(1).ok_or(PanelErrorV1::ResourceLimit)?;
        if count > max_candidates {
            return Err(PanelErrorV1::ResourceLimit);
        }
        let key = (seat.operator, seat.bond);
        if previous.is_some_and(|p| p >= key) {
            return Err(PanelErrorV1::InvalidSnapshot);
        }
        previous = Some(key);
        if group.as_ref().is_none_or(|g| g.0 != seat.operator) {
            if let Some(g) = group.take() {
                finish_stream_group_v1(g, &mut winners, needed, seed, retry, role);
            }
            group = Some((seat.operator, 0, None));
        }
        if seat.roles & role == 0 || used.contains(&seat.operator) {
            continue;
        }
        if seat.collateral == 0 {
            return Err(PanelErrorV1::InvalidSnapshot);
        }
        let g = group.as_mut().expect("a current operator group");
        g.1 = g.1.checked_add(seat.collateral).ok_or(PanelErrorV1::InvalidSnapshot)?;
        if has_headroom(&seat) {
            let score = ticket(seed, retry, role, seat.operator, Some(seat.bond));
            if g.2.is_none_or(|(bond, weight, old)| {
                product(score, weight).cmp(&product(old, seat.collateral)).then(seat.bond.cmp(&bond)).is_lt()
            }) {
                g.2 = Some((seat.bond, seat.collateral, score));
            }
        }
    }
    if let Some(g) = group {
        finish_stream_group_v1(g, &mut winners, needed, seed, retry, role);
    }
    Ok(winners.into_iter().map(|w| w.bond).collect())
}

struct StreamWinnerV1 {
    operator: Hash64,
    collateral: u64,
    score: u128,
    bond: BondIdV1,
}

fn finish_stream_group_v1(
    (operator, collateral, best): (Hash64, u64, Option<(BondIdV1, u64, u128)>),
    winners: &mut Vec<StreamWinnerV1>,
    needed: usize,
    seed: Hash64,
    retry: u16,
    role: u8,
) {
    let Some((bond, _, _)) = best else { return };
    let row = StreamWinnerV1 { operator, collateral, score: ticket(seed, retry, role, operator, None), bond };
    let at = winners
        .binary_search_by(|old| {
            product(old.score, row.collateral).cmp(&product(row.score, old.collateral)).then(old.operator.cmp(&row.operator))
        })
        .unwrap_or_else(|i| i);
    if at < needed {
        winners.insert(at, row);
        winners.truncate(needed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wide_product_preserves_extreme_collateral_order() {
        assert_eq!(product(1u128 << 70, u64::MAX), [63, u64::MAX - 63, 0]);
        for l in [0, 1, u64::MAX as u128, (1u128 << 64) + 1] {
            for w in [1, 5, 1_000_000] {
                let p = l * w as u128;
                assert_eq!(product(l, w), [0, (p >> 64) as u64, p as u64]);
            }
        }
        assert_eq!(neg_log2(0), 64u128 << 64);
        assert_eq!(neg_log2(u64::MAX), 0);
    }
}
