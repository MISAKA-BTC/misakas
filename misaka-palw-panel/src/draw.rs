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
    let used: BTreeSet<_> = used.iter().copied().collect();
    let mut groups: BTreeMap<Hash64, (u64, Vec<&SeatCandidateV1>)> = BTreeMap::new();
    for seat in &snapshot.candidates {
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
