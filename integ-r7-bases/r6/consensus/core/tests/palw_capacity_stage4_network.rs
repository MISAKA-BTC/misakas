//! **ADR-0160 stage 4 — lane N, the network level and the work-conserving fair share (F-N), through
//! testnet-12's own fold** (rcore/cap-s1; the user's staged plan, stage 4: `allowed_i = min(bond capacity,
//! rate, class room, fair share)`; property: 13k × 10 bonds never beats 130k × 1 bond).
//!
//! * the rule itself: the level caps the network work-conservingly, a registered bond below its share
//!   takes the freed units, the operator ring counts operator attempt blocks over 32 DAA;
//! * **the property on the rule** — random levels, competitors, lives and schedules: ten pieces never
//!   admit more than their whole under the same schedule;
//! * **the property through the fold (the stage-4 gate)** — every capacity fence armed (J-1, F-E, F-L,
//!   F-B, F-R, F-Q, F-S, F-N), a 1,000,000 MSK competitor, twelve attempt blocks a DAA with binds and
//!   licences between them: 10 × 13k never admits more than 130k × 1 by any DAA, at ρ 10 / 25 / 100 with
//!   the credit and at ρ 10 without it (where the level binds);
//! * a rewind and replay of a registered scenario refolds root for root; the ring reverts, IBDs and
//!   restarts.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_capacity_stage4_network -- --nocapture`

#[path = "capacity_stage1_common.rs"]
mod stage1;
use stage1::*;

use kaspa_consensus_core::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepV1};
use kaspa_consensus_core::palw_network_room_v1::{
    PALW_NETWORK_H_L_DAA_V1, PalwNetworkBondV1, palw_network_share_admits_v1, palw_network_units_v1,
};
use std::collections::BTreeSet;

/// testnet-12 with the capacity list at [`H`] but for `skip` (F-L's one step `(ρ, q)`).
fn armed(rho: u32, q: u16, skip: &[&str]) -> Params {
    let mut p = params_for(Class::Floor, false);
    for fence in PALW_T12_CAPACITY_FENCES_V1.iter().filter(|f| !skip.contains(&f.name)) {
        (fence.set)(&mut p, Some(ForkActivation::new(H)));
    }
    p.palw_capacity_aggregate_liability = Some(PalwCapacityLiabilityV1 {
        activation: ForkActivation::new(H),
        steps: vec![PalwCapacityStepV1 { from_daa: H, rho, q_credit_permille: q }],
    });
    p.sync_palw_capacity_liability();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the capacity list at ρ {rho}, q {q} without {skip:?}: {e:?}"));
    p
}

/// Lane N's own fixture: F-B and F-Q dormant and no credit, so a licence carries 3 claims a block and
/// `L_carry = H_L × 3 × max(1, ā_op)` = 63 while no operator attempts — the level binds early.
fn n_params(rho: u32) -> Params {
    armed(rho, 0, &["palw_capacity_batch_licence", "palw_capacity_audit_door"])
}

fn unlicensed(sim: &Sim) -> usize {
    sim.c.s.claims_iter().filter(|(_, c)| kaspa_consensus_core::palw_capacity_formulas_v1::palw_capacity_is_unlicensed_v1(c)).count()
}

fn held(sim: &Sim, n: u64) -> usize {
    sim.c
        .s
        .claims_iter()
        .filter(|(_, c)| c.bond == bond_key(n) && kaspa_consensus_core::palw_capacity_formulas_v1::palw_capacity_is_unlicensed_v1(c))
        .count()
}

/// `rounds` DAA of `per_daa` attempt blocks, round-robin over `bonds`; the claims admitted.
fn fill(sim: &mut Sim, bonds: &[u64], rounds: u64, per_daa: u64, salt: u64) -> Vec<Hash64> {
    let mut ids = Vec::new();
    let base = sim.c.daa;
    for d in 1..=rounds {
        for k in 0..per_daa {
            let n = bonds[((d * per_daa + k) % bonds.len() as u64) as usize];
            if let Some(id) = sim.block(base + d, vec![], Some((n, salt + (d << 16) + k))) {
                ids.push(id);
            }
        }
    }
    ids
}

/// **The level caps the network, work-conservingly**: a lone 1,000,000 MSK bond at ρ 10 takes every free
/// unit up to `L_net` (its own caps — N_bond, N_out 1,530 — are above it), and the next attempt is refused
/// `NetworkRoomExhausted` (or the floor's room, whichever binds first), which registers its demand for `H_L`.
#[test]
fn n_the_level_caps_the_network_and_a_lone_bond_takes_it_all() {
    let mut sim = Sim::new(n_params(10), Class::Floor, &[(90, 1_000_000)]);
    let ids = fill(&mut sim, &[90], 12, 12, 0x4E00);
    let u = unlicensed(&sim);
    println!("lane N: a lone 1M bond at ρ 10 holds {u} unlicensed claims ({} admitted); skips {:?}", ids.len(), sim.skips);
    assert!(u <= 63, "L_net ≤ L_carry = 21 × 3 = 63: {u}");
    let network = sim.skips.keys().any(|k| k.contains("finds no network unit"));
    let class = sim.skips.keys().any(|k| k.contains("floor room") || k.contains("share"));
    assert!(network || class, "refused for want of a unit: {:?}", sim.skips);
    let floor = sim.c.sp.base_class_id();
    let until = sim.c.s.network_demand_of_v1(&bond_key(90), &floor).expect("the refusal registered its demand for the floor");
    assert!(until > sim.c.daa && until <= sim.c.daa + PALW_NETWORK_H_L_DAA_V1, "registered through t + H_L: {until}");
}

/// **The honest share**: two 500,000 MSK bonds. A fills the level; B, refused, registers; once A's licences
/// free units, A (past its share with B's deficit standing) is refused and B takes the freed units.
#[test]
fn n_a_registered_bond_below_its_share_takes_the_freed_units() {
    let mut sim = Sim::new(n_params(10), Class::Floor, &[(90, 500_000), (91, 500_000)]);
    fill(&mut sim, &[90], 8, 12, 0x4E10);
    let level = unlicensed(&sim);
    assert_eq!(held(&sim, 90), level, "A holds the level");
    let before = sim.tape.len();
    assert!(sim.claim(91, 0x4E1F).is_none(), "B finds no free unit");
    let floor = sim.c.sp.base_class_id();
    assert!(sim.c.s.network_demand_of_v1(&bond_key(91), &floor).is_some(), "B registered its demand");
    // A licenses ten of its claims: ten units free.
    let seats = sim.seats();
    let a_claims: Vec<Hash64> = sim.c.s.claims_iter().filter(|(_, c)| c.bond == bond_key(90)).map(|(id, _)| *id).take(10).collect();
    for id in &a_claims {
        let bound = sim.bind(*id, &seats);
        sim.license(*id, &seats, bound);
    }
    assert_eq!(unlicensed(&sim), level - 10);
    assert!(sim.claim(90, 0x4E20).is_none(), "A holds more than its share while B is owed its share: refused");
    for k in 0..10u64 {
        assert!(sim.claim(91, 0x4E30 + k).is_some(), "B takes freed unit {k}");
    }
    assert_eq!(held(&sim, 91), 10);
    println!("lane N: level {level}; A licensed 10, was refused, B took the 10 freed units");
    // The registered scenario refolds root for root from before B's refusal.
    let off = sim.rewind_to(before);
    sim.replay(&off);
}

/// The operator ring counts an operator's attempt blocks and keeps 32 DAA (`ā_op`); its tape reverts,
/// IBDs and restarts.
#[test]
fn n_the_ring_counts_operator_attempt_blocks_over_32_daa() {
    let mut c = Chain::new(n_params(10));
    c.attribution = true;
    let mut t = Tape::new(c);
    for seed in 0..3u64 {
        t.attempt(None, 0x0A70 + seed);
    }
    let ring = t.c.s.operator_ring_v1().clone();
    assert_eq!(ring.values().sum::<u32>(), 3, "three operator attempt blocks: {ring:?}");
    t.at(t.c.daa + 40, vec![]);
    t.attempt(None, 0x0A7F);
    let ring = t.c.s.operator_ring_v1().clone();
    assert_eq!(ring.values().sum::<u32>(), 1, "entries older than 32 DAA left: {ring:?}");
    assert!(ring.keys().all(|daa| *daa + 32 > t.c.daa), "{ring:?}");
    t.revert_to_base_and_reapply();
    t.ibd_from(t.base.clone());
    for j in 0..=t.len() {
        t.restart_at(j);
    }
}

/// A deterministic generator (the property tests' own).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn key(i: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes([i; 64]), 0))
}

/// The rule's reading for asker `b` over `bonds` (key → (units, held, registered)), the shares dividing
/// `share_level` (a class room's `room − headroom`; the level itself for the network).
fn asks(level: u64, share_level: u64, b: &PalwBondKeyV2, bonds: &BTreeMap<PalwBondKeyV2, (u64, u64, bool)>) -> bool {
    let unlicensed: u64 = bonds.values().map(|(_, held, _)| *held).sum();
    let others: BTreeMap<PalwBondKeyV2, PalwNetworkBondV1> = bonds
        .iter()
        .map(|(k, (units, held, registered))| (*k, PalwNetworkBondV1 { units: *units, held: *held, registered: *registered }))
        .collect();
    let (units, held, registered) = bonds[b];
    palw_network_share_admits_v1(level, share_level, unlicensed, b, units, held, registered, &others).is_ok()
}

/// **A state's capacity for `ours`**: from `others` (key → (units, held, registered)), our bonds ask one
/// claim at a time in `order`'s order (a refused bond registers and stops; any admission lets the stopped
/// ask again) until none is admitted. What ours were admitted.
fn fill_rule(level: u64, share_level: u64, ours: &[u64], others: &[(u64, u64, bool)], order: &mut Lcg) -> u64 {
    let mut bonds: BTreeMap<PalwBondKeyV2, (u64, u64, bool)> = BTreeMap::new();
    for (j, (units, held, registered)) in others.iter().enumerate() {
        bonds.insert(key(200 + j as u8), (*units, *held, *registered));
    }
    let mine: Vec<PalwBondKeyV2> = (0..ours.len()).map(|i| key(i as u8 + 1)).collect();
    for (k, units) in mine.iter().zip(ours) {
        bonds.insert(*k, (*units, 0, false));
    }
    let (mut admitted, mut stopped) = (0u64, BTreeSet::new());
    while stopped.len() < mine.len() {
        let asking: Vec<&PalwBondKeyV2> = mine.iter().filter(|k| !stopped.contains(*k)).collect();
        let b = *asking[order.below(asking.len() as u64) as usize];
        if asks(level, share_level, &b, &bonds) {
            bonds.get_mut(&b).unwrap().1 += 1;
            admitted += 1;
            stopped.clear();
        } else {
            bonds.get_mut(&b).unwrap().2 = true;
            stopped.insert(b);
        }
    }
    admitted
}

/// **The property on the rule — 13k × 10 never beats 130k × 1 (and 100k × 10 never beats 1M × 1, 26k × 10
/// never beats 260k × 1), for every state and every order**: 10,000 random states (levels 1–200, up to
/// three other bonds of 1–120 units holding up to the level, each registered or not; a class room's
/// headroom kept out of the shares in half of them), our pieces asking in a random order, their whole
/// alone: what the pieces can take is never more.
#[test]
fn n_ten_small_bonds_never_beat_one_bond_of_their_total_on_the_rule() {
    let msk = 100_000_000u64;
    let u = |k: u64| palw_network_units_v1(k * 1_000 * msk);
    assert_eq!((u(13), u(130), u(100), u(1_000), u(26), u(260)), (1, 10, 7, 76, 2, 20));
    let mut rng = Lcg(0x5EED_0004);
    let (mut runs, mut ties) = (0u64, 0u64);
    for _ in 0..10_000 {
        let level = 1 + rng.below(200);
        let share_level = if rng.below(2) == 0 { level } else { level - rng.below(level + 1) };
        let others: Vec<(u64, u64, bool)> =
            (0..rng.below(4)).map(|_| (1 + rng.below(120), rng.below(level + 1), rng.below(10) < 6)).collect();
        for (piece, whole) in [(u(13), u(130)), (u(100), u(1_000)), (u(26), u(260))] {
            let mut order = Lcg(rng.next());
            let split = fill_rule(level, share_level, &[piece; 10], &others, &mut order);
            let one = fill_rule(level, share_level, &[whole], &others, &mut Lcg(0));
            assert!(
                split <= one,
                "10 × {piece} units took {split} > 1 × {whole}'s {one}: level {level} ({share_level} shared), others {others:?}"
            );
            runs += 1;
            ties += u64::from(split == one);
        }
    }
    println!("lane N rule, capacity: {runs} random states, the pieces never took more ({ties} ties)");
}

/// One schedule of the rule with releases: `ours` (units each) and `others` ask on `schedule` (`None` =
/// ours round-robin, `Some(j)` = other `j`), a claim licensing `life` steps after admission, a refusal
/// registering its bond for `h_l` steps. What ours were admitted.
fn run_rule(level: u64, ours: &[u64], others: &[u64], schedule: &[Option<usize>], life: u64, h_l: u64) -> u64 {
    let mut bonds: BTreeMap<PalwBondKeyV2, (u64, u64, bool)> = BTreeMap::new();
    for (i, units) in ours.iter().enumerate() {
        bonds.insert(key(i as u8 + 1), (*units, 0, false));
    }
    for (j, units) in others.iter().enumerate() {
        bonds.insert(key(200 + j as u8), (*units, 0, false));
    }
    let mut until: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
    let mut releases: Vec<(u64, PalwBondKeyV2)> = Vec::new();
    let (mut turn, mut admitted) = (0usize, 0u64);
    for (t, asker) in schedule.iter().enumerate() {
        let t = t as u64;
        releases.retain(|(at, k)| {
            if *at <= t {
                bonds.get_mut(k).unwrap().1 -= 1;
                false
            } else {
                true
            }
        });
        for (k, entry) in bonds.iter_mut() {
            entry.2 = until.get(k).is_some_and(|u| *u >= t);
        }
        let b = match asker {
            None => {
                turn += 1;
                key(((turn - 1) % ours.len()) as u8 + 1)
            }
            Some(j) => key(200 + *j as u8),
        };
        if asks(level, level, &b, &bonds) {
            bonds.get_mut(&b).unwrap().1 += 1;
            releases.push((t + life, b));
            admitted += u64::from(asker.is_none());
        } else {
            until.insert(b, t + h_l);
        }
    }
    admitted
}

/// **What the property does not cover, measured**: over 2,000 random schedules with releases (levels
/// 1–150, one to three competitors, lives 1–60, `H_L` 1–30, 400 asks), the pieces come out ahead of their
/// whole in about 1 % of runs — a registered bond's reserved units idle until it asks, which moves the
/// competitors' admissions in time (at small levels by up to a fifth of the whole's count) — and behind
/// it overall. Guarded so a rule change that made splitting pay shows up: in all, the pieces take no more
/// than their wholes, and they are ahead in at most 3 % of the runs.
#[test]
fn n_under_schedules_with_releases_a_split_rarely_comes_out_ahead_and_never_in_all() {
    let mut rng = Lcg(0x5EED_0005);
    let (mut runs, mut ahead, mut worst, mut all_split, mut all_whole) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for _ in 0..2_000 {
        let level = 1 + rng.below(150);
        let others: Vec<u64> = (0..1 + rng.below(3)).map(|_| 1 + rng.below(120)).collect();
        let (life, h_l, p_ours) = (1 + rng.below(60), 1 + rng.below(30), 20 + rng.below(61));
        let schedule: Vec<Option<usize>> =
            (0..400).map(|_| if rng.below(100) < p_ours { None } else { Some(rng.below(others.len() as u64) as usize) }).collect();
        for (piece, whole) in [(1u64, 10u64), (7, 76)] {
            let split = run_rule(level, &[piece; 10], &others, &schedule, life, h_l);
            let one = run_rule(level, &[whole], &others, &schedule, life, h_l);
            runs += 1;
            (all_split, all_whole) = (all_split + split, all_whole + one);
            if split > one {
                ahead += 1;
                worst = worst.max(split - one);
            }
        }
    }
    println!(
        "lane N rule, schedules with releases: {runs} runs, the pieces ahead in {ahead} (by at most {worst} claims); in all {all_split} ≤ {all_whole}"
    );
    assert!(all_split <= all_whole, "in all the pieces took {all_split} > their wholes' {all_whole}");
    assert!(ahead * 100 <= runs * 3, "the pieces ahead in {ahead} of {runs} runs");
}

/// The claims `bond` holds unlicensed, oldest first.
fn provisional_of(sim: &Sim, bond: u64) -> Vec<Hash64> {
    let mut claims: Vec<(u64, Hash64)> = sim
        .c
        .s
        .claims_iter()
        .filter(|(_, c)| c.bond == bond_key(bond) && matches!(c.phase, PalwClaimPhaseV2::Provisional))
        .map(|(id, c)| (c.accepted_blue_score, *id))
        .collect();
    claims.sort();
    claims.into_iter().map(|(_, id)| id).collect()
}

/// **One state's capacity for `ours`, through the fold**: a 1,000,000 MSK competitor (bond 80) fills
/// first — twelve attempt blocks a DAA until it is refused for want of a unit (and so registered) or 20
/// DAA — then `licensed` of its claims bind and license (units free up); then only `ours` ask, twelve
/// attempt blocks a DAA round-robin, for `days` DAA with nothing released. Our admitted claims by DAA, and
/// the skips' reasons.
fn stage4_capacity(p: &Params, ours: &[(u64, u64)], licensed: usize, days: u64) -> (Vec<usize>, BTreeMap<String, usize>) {
    let mut producers = ours.to_vec();
    producers.push((80, 1_000_000));
    let mut sim = Sim::new(p.clone(), Class::Floor, &producers);
    let class = sim.c.sp.base_class_id();
    let mut daa = sim.c.daa;
    for d in 1..=20u64 {
        daa += 1;
        for k in 0..12u64 {
            sim.block(daa, vec![], Some((80, 0x5000 + (d << 8) + k)));
        }
        if sim.c.s.network_demand_of_v1(&bond_key(80), &class).is_some() {
            break;
        }
    }
    // `licensed` of its claims bound in one block and licensed in the next — two DAA, well inside the
    // registration's `H_L`.
    let seats = sim.seats();
    let ids: Vec<Hash64> = provisional_of(&sim, 80).into_iter().take(licensed).collect();
    let binds = ids
        .iter()
        .enumerate()
        .map(|(i, id)| PalwConsensusObjectV2::PanelBound {
            claim: *id,
            anchor: h(0xAC_0000 + (sim.c.daa << 8) + i as u64),
            seats: seats_of(&seats),
        })
        .collect();
    sim.step(binds);
    let bound = sim.c.daa;
    let licences = ids
        .iter()
        .map(|id| PalwConsensusObjectV2::ReceiptLicensed {
            claim: *id,
            receipts: seats.iter().map(|(k, _)| valid(*id, *k, bound)).collect(),
        })
        .collect();
    sim.step(licences);
    assert!(
        ids.iter().all(|id| matches!(sim.c.claim(id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. })),
        "the competitor's licences fold"
    );
    println!(
        "  the competitor holds {} unlicensed, registered {:?}, before ours ask at DAA {}",
        held(&sim, 80),
        sim.c.s.network_demand_iter_v1().map(|((_, class), until)| (*class == sim.c.sp.base_class_id(), *until)).collect::<Vec<_>>(),
        sim.c.daa + 1
    );
    sim.skips.clear();
    let base = sim.c.daa;
    let (mut cumulative, mut total, mut turn) = (Vec::new(), 0usize, 0usize);
    for d in 1..=days {
        for k in 0..12u64 {
            turn += 1;
            let n = ours[(turn - 1) % ours.len()].0;
            if sim.block(base + d, vec![], Some((n, 0x5400 + (d << 8) + k))).is_some() {
                total += 1;
            }
        }
        cumulative.push(total);
    }
    (cumulative, sim.skips.clone())
}

/// **The stage-4 gate — `allowed_i = min(bond capacity, rate, class room, fair share)` never lets 13k × 10
/// beat 130k × 1**, through the fold, on the same state and the same attempt blocks, by every DAA: with
/// the credit at ρ 10, 25 and 100 (the whole capacity list armed; lane S, the bond's capacity, F-R's class
/// room and lane N's share all in play) and without it at ρ 10 (F-B and F-Q dormant, the level at 63).
#[test]
fn stage4_gate_ten_13k_bonds_never_beat_one_130k_bond_through_the_fold() {
    // Bonds 11–20 and 30 (1 and 61 are the fixture's accuser and challenger, 80 the competitor).
    let pieces: Vec<(u64, u64)> = (11..=20u64).map(|n| (n, 13_000)).collect();
    let whole = [(30u64, 130_000u64)];
    let cases: Vec<(&str, Params, usize)> = vec![
        ("ρ 10, credited", armed(10, 250, &[]), 30),
        ("ρ 25, credited", armed(25, 250, &[]), 30),
        ("ρ 100, credited", armed(100, 250, &[]), 30),
        ("ρ 10, no credit, the level at 63", n_params(10), 20),
    ];
    for (label, p, licensed) in cases {
        let (split, split_skips) = stage4_capacity(&p, &pieces, licensed, 20);
        let (one, one_skips) = stage4_capacity(&p, &whole, licensed, 20);
        println!(
            "stage 4 gate, {label}:\n  10 × 13k {split:?}\n   1 × 130k {one:?}\n  pieces refused {split_skips:?}\n  whole refused {one_skips:?}"
        );
        for (d, (s, w)) in split.iter().zip(&one).enumerate() {
            assert!(s <= w, "{label}: by DAA {} the pieces took {s} > the whole's {w}", d + 1);
        }
    }
}
